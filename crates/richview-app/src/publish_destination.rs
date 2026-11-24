//! The user's explicit choice of where CLI publications go; credentials stay elsewhere.
use crate::{cli::OutputFormat, ops::Failure};
use std::{
    io::{BufRead, IsTerminal, Write},
    path::{Path, PathBuf},
};

pub(crate) const HOSTED: &str = "https://app.richview.ai";
const ACCESS: &str = "Create an account at https://app.richview.ai/sign-up (if sign-ups are paused, that page says so; meanwhile publish to a server you run), create a CLI token on the CLI tokens page and run `richview login --server https://app.richview.ai` once in a terminal. A script sets RICHVIEW_SERVER=https://app.richview.ai and RICHVIEW_TOKEN to that token instead.";

/// The user's own RichView configuration folder: the publish choice and the kept sign-ins.
pub(crate) fn config_dir() -> Result<PathBuf, Failure> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|p| !p.is_empty())
                .map(|p| PathBuf::from(p).join(".config"))
        })
        .or_else(|| {
            std::env::var_os("APPDATA")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from)
        })
        .ok_or_else(|| {
            Failure::usage(
                "Cannot find your configuration directory; pass --server or set RICHVIEW_SERVER.",
            )
        })?;
    Ok(base.join("richview"))
}

/// Holds the first publish of a source file to a server until its caller records success by
/// setting the file length to one. Subsequent publishes need no lock during the network call.
/// Store only this bit outside the report, keyed by its canonical path and server origin.
pub(crate) fn first_publication(
    source: &Path,
    server: &str,
) -> Result<Option<std::fs::File>, Failure> {
    use sha2::{Digest, Sha256};
    let origin = crate::remote::server_url(server)?
        .origin()
        .ascii_serialization();
    let source = source
        .canonicalize()
        .map_err(|e| Failure::host(e.to_string()))?;
    let key = Sha256::new()
        .chain_update(origin.as_bytes())
        .chain_update([0])
        .chain_update(source.as_os_str().as_encoded_bytes())
        .finalize();
    let dir = config_dir()?.join("published");
    std::fs::create_dir_all(&dir)
        .map_err(|e| Failure::host(format!("Cannot open publish history: {e}")))?;
    let mut options = std::fs::OpenOptions::new();
    options.create(true).read(true).write(true).truncate(false);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let file = options
        .open(dir.join(format!("{key:x}")))
        .map_err(|e| Failure::host(format!("Cannot open publish history: {e}")))?;
    // Keep overlapping publishes from both deciding this is the first one. An unrelated
    // report or destination has a different record and never waits for this request.
    file.try_lock().map_err(|error| Failure::host(format!(
        "Cannot lock this report's publish history: {error}. Retry after the other publish finishes."
    )))?;
    let length = file
        .metadata()
        .map_err(|e| Failure::host(e.to_string()))?
        .len();
    Ok((length == 0).then_some(file))
}

fn choice_file() -> Result<PathBuf, Failure> {
    Ok(config_dir()?.join("publish-server.json"))
}

/// The destination the user chose before, if any; never asks.
pub(crate) fn saved() -> Result<Option<String>, Failure> {
    load(&choice_file()?)
}

/// Remembers `server` as the destination, as choosing it at the question does.
pub(crate) fn remember(server: &str) -> Result<(), Failure> {
    crate::remote::server_url(server).map_err(Failure::usage)?;
    save_private(&choice_file()?, &server)
}

fn load(path: &Path) -> Result<Option<String>, Failure> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(Failure::host(format!(
                "Cannot read {}: {error}",
                path.display()
            )));
        }
    };
    let server: String = serde_json::from_slice(&bytes)
        .map_err(|e| Failure::host(format!("Invalid saved publish destination: {e}")))?;
    crate::remote::server_url(&server).map_err(Failure::usage)?;
    Ok(Some(server))
}

/// Writes one of the user's own configuration files so that only they can read it: the folder
/// is theirs alone, and the file replaces the old one in a single step.
pub(crate) fn save_private<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), Failure> {
    let dir = path
        .parent()
        .ok_or_else(|| Failure::host("Invalid configuration path"))?;
    std::fs::create_dir_all(dir)
        .map_err(|e| Failure::host(format!("Cannot create {}: {e}", dir.display())))?;
    if !std::fs::symlink_metadata(dir)
        .map_err(|e| Failure::host(e.to_string()))?
        .file_type()
        .is_dir()
    {
        return Err(Failure::host(
            "The RichView configuration directory must not be a symlink.",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| Failure::host(format!("Cannot protect {}: {e}", dir.display())))?;
    }
    let mut file =
        tempfile::NamedTempFile::new_in(dir).map_err(|e| Failure::host(e.to_string()))?;
    serde_json::to_writer(&mut file, value).map_err(|e| Failure::host(e.to_string()))?;
    file.persist(path)
        .map_err(|e| Failure::host(format!("Cannot save {}: {}", path.display(), e.error)))?;
    Ok(())
}

fn ask(question: &str) -> Result<String, Failure> {
    eprint!("{question}");
    std::io::stderr()
        .flush()
        .map_err(|e| Failure::host(e.to_string()))?;
    let mut answer = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut answer)
        .map_err(|e| Failure::host(e.to_string()))?;
    Ok(answer.trim().to_owned())
}

/// Resolves a destination without ever prompting or uploading for an unconfigured agent.
pub(crate) fn resolve(
    explicit: Option<String>,
    choose: bool,
    format: OutputFormat,
) -> Result<String, Failure> {
    if let Some(server) = explicit.filter(|s| !s.is_empty()) {
        if choose {
            return Err(Failure::usage(
                "To choose a new destination, remove --server and unset RICHVIEW_SERVER, then retry --choose-server.",
            ));
        }
        return Ok(server);
    }
    let path = choice_file()?;
    if !choose && let Some(server) = load(&path)? {
        return Ok(server);
    }
    if matches!(format, OutputFormat::Json) || !std::io::stdin().is_terminal() {
        return Err(Failure::usage(format!(
            "No publish destination selected. Pass --server or set RICHVIEW_SERVER, or run richview publish in a terminal to choose. {ACCESS}"
        )));
    }
    eprintln!(
        "Publish to RichView Cowork at {HOSTED}, operated by RichView? Reports and linked files will be uploaded. You can run your own RichView server instead.\nHosted reports are private to their owner by default; the operator can administer them.\n  1. RichView Cowork, hosted by RichView\n  2. My own server\n  3. Cancel"
    );
    let server = match ask("Choose [3]: ")?.as_str() {
        "1" => HOSTED.to_owned(),
        "2" => ask("Self-hosted server URL: ")?,
        _ => {
            return Err(Failure::usage(
                "Publishing cancelled; nothing was uploaded.",
            ));
        }
    };
    crate::remote::server_url(&server).map_err(Failure::usage)?;
    save_private(&path, &server)?;
    Ok(server)
}

/// What to do when a command has no token for `server` and cannot ask for one.
pub(crate) fn missing_token(server: &str) -> String {
    if server.trim_end_matches('/') == HOSTED {
        format!("This computer is not signed in to {HOSTED}. {ACCESS}")
    } else {
        format!(
            "This computer is not signed in to {server}. Run `richview login --server {server}` once in a terminal; a script sets RICHVIEW_TOKEN for RICHVIEW_SERVER or passes this server's token with --token."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_is_private_and_contains_only_the_destination() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("richview/publish-server.json");
        assert_eq!(load(&path).unwrap(), None);
        save_private(&path, &HOSTED).unwrap();
        assert_eq!(load(&path).unwrap().as_deref(), Some(HOSTED));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("\"{HOSTED}\"")
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
                0
            );
            assert_eq!(
                std::fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
        std::fs::write(&path, "\"http://example.com\"").unwrap();
        assert!(load(&path).is_err());
    }
}
