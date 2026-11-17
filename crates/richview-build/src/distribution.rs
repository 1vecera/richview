//! Local native install archives; publication and selecting `prod` remain separate actions (INT-71).
use super::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    process::Command,
    time::UNIX_EPOCH,
};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Daniel enables this only when he authorizes launch (INT-43).
pub fn release_enabled(root: &Path) -> Result<bool> {
    let value: Value =
        serde_json::from_slice(&regular(&root.join("assets/install/release.json"))?)?;
    value["enabled"]
        .as_bool()
        .ok_or_else(|| "install release must say whether it is enabled".into())
}

/// Whether the published release carries the Windows preview's archive and installer (ARC-67).
/// Daniel switches it on with the release that has them; until then the install guide does not
/// show a command that would find nothing.
pub fn windows_preview(root: &Path) -> Result<bool> {
    let value: Value =
        serde_json::from_slice(&regular(&root.join("assets/install/release.json"))?)?;
    match &value["windows"] {
        Value::Null => Ok(false),
        Value::Bool(on) => Ok(*on),
        _ => Err("install release must say `windows` as true or false".into()),
    }
}

/// Text shown only while the release carries the Windows preview: its install section, and
/// sentences that name it.
const WINDOWS_PREVIEW: &str = "windows-preview";
/// Text shown only while it does not, such as "Windows is not supported yet".
const WINDOWS_UNSUPPORTED: &str = "windows-unsupported";

/// A page's Windows text for this release: the preview's spans when the release carries the
/// Windows archive, the "not supported" spans when it does not. The install guide and the
/// documentation both pass through it, so the two always say the same.
pub(crate) fn windows_text(page: &str, preview: bool) -> Result<String> {
    let page = marked(page, WINDOWS_PREVIEW, preview)?;
    marked(&page, WINDOWS_UNSUPPORTED, !preview)
}

/// Every `<!-- name:start -->…<!-- name:end -->` span, kept without its markers or dropped
/// with the indentation before it and the line break after it.
fn marked(page: &str, name: &str, keep: bool) -> Result<String> {
    let (open, close) = (
        format!("<!-- {name}:start -->"),
        format!("<!-- {name}:end -->"),
    );
    let mut text = page.to_owned();
    while let Some(start) = text.find(&open) {
        let end = start
            + text[start..]
                .find(&close)
                .ok_or_else(|| format!("a {name} section is never closed"))?;
        let (before, after) = (&text[..start], &text[end + close.len()..]);
        text = if keep {
            format!("{before}{}{after}", &text[start + open.len()..end])
        } else {
            format!(
                "{}{}",
                before.trim_end_matches([' ', '\t']),
                after.trim_start_matches('\n')
            )
        };
    }
    if text.contains(&close) {
        return Err(format!("a {name} section is closed before it opens").into());
    }
    Ok(text)
}

/// Stages the install guide; release assets are distributed through GitHub.
pub fn stage_site(root: &Path, public: &Path) -> Result<()> {
    let template = windows_text(
        &fs::read_to_string(root.join("assets/install/index.html"))?,
        windows_preview(root)?,
    )?;
    fs::write(
        public.join("install.html"),
        crate::chrome::apply(&crate::chrome::apply_styles(&template)),
    )?;
    Ok(())
}

pub fn platform() -> Result<String> {
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "linux" if cfg!(target_env = "gnu") => "unknown-linux-gnu",
        other => return Err(format!("no shell install archive for {other}").into()),
    };
    let arch = std::env::consts::ARCH;
    let platform = format!("{arch}-{os}");
    if !matches!(
        platform.as_str(),
        "aarch64-apple-darwin" | "x86_64-unknown-linux-gnu"
    ) {
        return Err(format!("no shell install archive for {platform}").into());
    }
    Ok(platform)
}

/// The preview target (ARC-67). Its binaries are cross-built, so its archive is packed on another host.
pub const WINDOWS: &str = "x86_64-pc-windows-msvc";

/// The suffix of `platform`'s executables, and the extension of the installer its archive carries.
fn layout(platform: &str) -> (&'static str, &'static str) {
    if platform == WINDOWS {
        (".exe", "ps1")
    } else {
        ("", "sh")
    }
}

/// Packages an existing clean full build from the exact native bytes its manifest hashes.
///
/// With `cross`, packages the binaries in `native` that were cross-built for that target. The
/// manifest hashes only the host's binaries, so these are tied to its commit by the short commit
/// the CLI embeds for `--version`, and both must be Windows executables.
pub fn existing(
    root: &Path,
    manifest: &Path,
    native: &Path,
    output: &Path,
    cross: Option<&str>,
) -> Result<()> {
    let record: Value = serde_json::from_slice(&fs::read(manifest)?)?;
    if record["schema"] != 1 || record["mode"] != "full" {
        return Err("install archive needs a full schema-1 build manifest".into());
    }
    let build = commit(&record["inputs"])?;
    let platform = match cross {
        None => platform()?,
        Some(WINDOWS) => WINDOWS.to_owned(),
        Some(other) => return Err(format!("no cross-built install archive for {other}").into()),
    };
    let mut files = BTreeMap::new();
    for name in ["richview", "richview-sql-worker"] {
        let file = format!("{name}{}", layout(&platform).0);
        let bytes = regular(&native.join(&file))?;
        if cross.is_some() {
            // Every Windows executable starts with these two bytes.
            let short = &build.as_bytes()[..10];
            if !bytes.starts_with(b"MZ")
                || (name == "richview" && !bytes.windows(short.len()).any(|at| at == short))
            {
                return Err(format!(
                    "install archive input is not a {platform} build of {build}: {file}"
                )
                .into());
            }
        } else {
            let expected = &record["outputs"][format!("target:release/{name}")];
            if expected["bytes"].as_u64() != Some(bytes.len() as u64)
                || expected["sha256"].as_str() != Some(hash(&bytes).as_str())
            {
                return Err(
                    format!("install archive input differs from build manifest: {name}").into(),
                );
            }
        }
        files.insert(format!("bin/{file}"), bytes);
    }
    let parent = output
        .parent()
        .ok_or("missing install output parent")?
        .canonicalize()?;
    let destination = parent.join(output.file_name().ok_or("missing install output name")?);
    if destination.starts_with(native.canonicalize()?)
        || destination.starts_with(
            manifest
                .canonicalize()?
                .parent()
                .ok_or("missing manifest parent")?,
        )
        || destination.exists()
    {
        return Err(
            "archive-only output must be a new directory outside the immutable inputs".into(),
        );
    }
    fs::create_dir(&destination)?;
    let result = write_archive(root, files, &destination, &record["inputs"], &platform);
    if result.is_err() {
        fs::remove_dir_all(&destination)?;
    }
    result
}

fn commit(inputs: &Value) -> Result<&str> {
    let commit = inputs["commit"]
        .as_str()
        .ok_or("missing install build commit")?;
    if commit.len() != 40
        || !commit
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || inputs["dirty"] != false
        || inputs["source_diff_sha256"] != hash(b"")
    {
        return Err("install archive requires a clean, committed build".into());
    }
    Ok(commit)
}

fn regular(path: &Path) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(format!(
            "install archive input is not a regular file: {}",
            path.display()
        )
        .into());
    }
    Ok(fs::read(path)?)
}

fn notices(source: &Path, destination: &str, files: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
    for item in fs::read_dir(source)? {
        let item = item?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| "non-UTF-8 notice name")?;
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err("unsafe install notice name".into());
        }
        files.insert(format!("{destination}/{name}"), regular(&item.path())?);
    }
    Ok(())
}

/// Stages the native host's CLI/worker, notices, installer and SHA-256 lists without publishing.
pub fn stage(root: &Path, native: &Path, output: &Path, inputs: &Value) -> Result<()> {
    let mut files = BTreeMap::new();
    for name in ["richview", "richview-sql-worker"] {
        files.insert(format!("bin/{name}"), regular(&native.join(name))?);
    }
    write_archive(root, files, output, inputs, &platform()?)
}

fn write_archive(
    root: &Path,
    mut files: BTreeMap<String, Vec<u8>>,
    output: &Path,
    inputs: &Value,
    platform: &str,
) -> Result<()> {
    let commit = commit(inputs)?;
    let installer = format!("install.{}", layout(platform).1);
    let uninstaller = format!("un{installer}");
    files.insert("notices/LICENSE".into(), regular(&root.join("LICENSE"))?);
    files.insert(
        "notices/THIRD-PARTY-NOTICES.txt".into(),
        regular(&root.join("tmp/THIRD-PARTY-NOTICES.txt"))?,
    );
    notices(
        &root.join("crates/richview-core/assets/font-licenses"),
        "notices/report",
        &mut files,
    )?;
    notices(
        &root.join("assets/showcase/fonts/licenses"),
        "notices/showcase",
        &mut files,
    )?;
    files.insert(
        uninstaller.clone(),
        regular(&root.join("scripts").join(&installer))?,
    );
    files.insert(
        "BUILD".into(),
        format!("commit={commit}\nplatform={platform}\n").into_bytes(),
    );
    files.insert(
        "build-inputs.json".into(),
        serde_json::to_vec_pretty(inputs)?,
    );
    let sums = files
        .iter()
        .map(|(name, bytes)| format!("{}  {name}\n", hash(bytes)))
        .collect::<String>();
    files.insert("SHA256SUMS".into(), sums.into_bytes());
    fs::create_dir_all(output)?;
    // One private staging directory; only this invocation's temporary files are ever removed.
    let work = output.join(format!(".install-{}", std::process::id()));
    fs::create_dir(&work)?;
    let result = (|| -> Result<()> {
        let payload = work.join("payload");
        fs::create_dir(&payload)?;
        let mut entries = BTreeSet::from([".".to_owned()]);
        for (name, bytes) in &files {
            let path = payload.join(name);
            fs::create_dir_all(path.parent().ok_or("missing archive parent")?)?;
            fs::write(&path, bytes)?;
            for entry in Path::new(name)
                .ancestors()
                .filter(|p| !p.as_os_str().is_empty())
            {
                entries.insert(format!("./{}", entry.display()));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(
                    &path,
                    fs::Permissions::from_mode(
                        if name.starts_with("bin/") || name == "uninstall.sh" {
                            0o755
                        } else {
                            0o644
                        },
                    ),
                )?;
            }
        }
        // Set metadata after the complete tree exists: creating children changes directory times.
        // Explicit member order avoids the filesystem's directory iteration order in either tar.
        for entry in &entries {
            let path = payload.join(entry);
            #[cfg(unix)]
            if path.is_dir() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
            }
            fs::File::open(path)?.set_modified(UNIX_EPOCH)?;
        }
        let tar = work.join("payload.tar");
        let archive = work.join(format!("richview-{platform}.tar.gz"));
        let status = Command::new("tar")
            .env("COPYFILE_DISABLE", "1")
            .env_remove("TAR_OPTIONS")
            .args(["-cf"])
            .arg(&tar)
            .args(["--format=ustar", "--no-recursion", "-C"])
            .arg(&payload)
            .args(&entries)
            .status()?;
        if !status.success() {
            return Err("install archive tar failed".into());
        }
        let mut gzip = flate2::GzBuilder::new()
            .mtime(0)
            .write(fs::File::create(&archive)?, flate2::Compression::default());
        std::io::copy(&mut fs::File::open(tar)?, &mut gzip)?;
        gzip.finish()?;
        let name = archive.file_name().ok_or("missing archive name")?;
        let destination = output.join(commit);
        fs::create_dir_all(&destination)?;
        let checksum = format!(
            "{}  {}\n",
            hash(&fs::read(&archive)?),
            name.to_string_lossy()
        );
        fs::write(work.join("checksum"), checksum)?;
        fs::rename(&archive, destination.join(name))?;
        fs::rename(
            work.join("checksum"),
            destination.join(format!("{}.sha256", name.to_string_lossy())),
        )?;
        fs::write(output.join(&installer), &files[&uninstaller])?;
        Ok(())
    })();
    fs::remove_dir_all(&work)?;
    result
}

#[cfg(all(
    test,
    any(target_os = "macos", all(target_os = "linux", target_env = "gnu")),
    any(target_arch = "aarch64", target_arch = "x86_64")
))]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> tempfile::TempDir {
        let folder = tempfile::tempdir().unwrap();
        for path in [
            "native",
            "crates/richview-core/assets/font-licenses",
            "assets/showcase/fonts/licenses",
            "scripts",
            "tmp",
        ] {
            fs::create_dir_all(folder.path().join(path)).unwrap();
        }
        for (path, bytes) in [
            ("LICENSE", "MIT notice"),
            ("tmp/THIRD-PARTY-NOTICES.txt", "Third-party notice"),
            ("native/richview", "native CLI"),
            ("native/richview-sql-worker", "native worker"),
            (
                "crates/richview-core/assets/font-licenses/report.txt",
                "report font notice",
            ),
            (
                "assets/showcase/fonts/licenses/showcase.txt",
                "showcase font notice",
            ),
            (
                "scripts/install.sh",
                "#!/bin/sh\n# https://github.com/1vecera/richview/releases/latest/download\nexit 0\n",
            ),
            ("scripts/install.ps1", "# PowerShell installer\n"),
        ] {
            fs::write(folder.path().join(path), bytes).unwrap();
        }
        folder
    }

    fn inputs() -> Value {
        json!({"commit":"1".repeat(40),"dirty":false,"source_diff_sha256":hash(b"")})
    }

    #[test]
    fn install_page_does_not_stage_hosted_downloads() {
        let folder = fixture();
        let root = folder.path();
        fs::create_dir_all(root.join("assets/install")).unwrap();
        fs::write(
            root.join("assets/install/release.json"),
            json!({"enabled": false}).to_string(),
        )
        .unwrap();
        assert!(!release_enabled(root).unwrap());
        fs::write(
            root.join("assets/install/release.json"),
            json!({"enabled": true}).to_string(),
        )
        .unwrap();
        assert!(release_enabled(root).unwrap());
        fs::write(
            root.join("assets/install/index.html"),
            "<!-- rv-site-header -->GitHub release<!-- rv-site-footer -->",
        )
        .unwrap();
        fs::create_dir(root.join("web")).unwrap();
        stage_site(root, &root.join("web")).unwrap();
        assert!(
            fs::read_to_string(root.join("web/install.html"))
                .unwrap()
                .contains("GitHub release")
        );
        for name in ["install.sh", "install-release.json", "downloads"] {
            assert!(!root.join("web").join(name).exists());
        }
    }

    #[test]
    fn install_page_names_windows_only_when_the_release_carries_its_archive() {
        let folder = fixture();
        let root = folder.path();
        fs::create_dir_all(root.join("assets/install")).unwrap();
        fs::create_dir(root.join("web")).unwrap();
        fs::write(
            root.join("assets/install/index.html"),
            "<!-- rv-site-header -->curl\n  <!-- windows-preview:start -->\n  <section>install.ps1</section>\n  <!-- windows-preview:end -->\n  after<!-- rv-site-footer -->",
        )
        .unwrap();
        let page = |release: Value| {
            fs::write(
                root.join("assets/install/release.json"),
                release.to_string(),
            )
            .unwrap();
            stage_site(root, &root.join("web"))
                .map(|()| fs::read_to_string(root.join("web/install.html")).unwrap())
        };
        // A release file that does not mention Windows is one without the preview.
        for off in [
            json!({"enabled": true}),
            json!({"enabled": true, "windows": false}),
        ] {
            let page = page(off).unwrap();
            assert!(!page.contains("install.ps1") && !page.contains("windows-preview"));
            assert!(page.contains("curl\n  after"), "{page}");
        }
        let on = page(json!({"enabled": true, "windows": true})).unwrap();
        assert!(on.contains("<section>install.ps1</section>") && !on.contains("windows-preview"));
        assert!(page(json!({"enabled": true, "windows": "yes"})).is_err());
    }

    #[test]
    fn repeated_packaging_ignores_source_and_staging_times() {
        let first = fixture();
        let second = fixture();
        for (folder, seconds) in [(&first, 1_000_000_000), (&second, 1_700_000_000)] {
            let root = folder.path();
            for name in ["LICENSE", "native/richview", "scripts/install.sh", "."] {
                fs::File::open(root.join(name))
                    .unwrap()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds))
                    .unwrap();
            }
        }
        let package = |folder: &tempfile::TempDir| {
            let root = folder.path();
            stage(root, &root.join("native"), &root.join("out"), &inputs()).unwrap();
            let archive = root
                .join("out")
                .join("1".repeat(40))
                .join(format!("richview-{}.tar.gz", platform().unwrap()));
            (
                fs::read(&archive).unwrap(),
                fs::read(archive.with_extension("gz.sha256")).unwrap(),
            )
        };
        let before = package(&first);
        // Cross a whole tar/gzip timestamp tick with real staging, without a test-only clock seam.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let after = package(&second);
        assert!(
            before == after,
            "unchanged payloads must have identical archive and sidecar bytes"
        );
        assert_eq!(
            &before.0[4..8],
            &[0; 4],
            "gzip must not record the packaging time"
        );
        fs::write(second.path().join("LICENSE"), "changed MIT notice").unwrap();
        let changed = package(&second);
        assert_ne!(
            hash(&after.0),
            hash(&changed.0),
            "changed payload changes the archive"
        );
        assert_ne!(after.1, changed.1, "changed payload changes the sidecar");
    }

    #[test]
    fn native_archive_carries_both_executables_notices_and_verified_checksums() {
        let folder = fixture();
        let root = folder.path();
        stage(root, &root.join("native"), &root.join("out"), &inputs()).unwrap();
        let name = format!("richview-{}.tar.gz", platform().unwrap());
        let archive = root.join("out").join("1".repeat(40)).join(&name);
        assert_eq!(
            fs::read_to_string(archive.with_file_name(format!("{name}.sha256"))).unwrap(),
            format!("{}  {name}\n", hash(&fs::read(&archive).unwrap()))
        );
        fs::create_dir(root.join("read")).unwrap();
        assert!(
            Command::new("tar")
                .arg("-xzf")
                .arg(archive)
                .arg("-C")
                .arg(root.join("read"))
                .status()
                .unwrap()
                .success()
        );
        for (path, expected) in [
            ("bin/richview", "native CLI"),
            ("bin/richview-sql-worker", "native worker"),
            ("notices/LICENSE", "MIT notice"),
            ("notices/THIRD-PARTY-NOTICES.txt", "Third-party notice"),
            ("notices/report/report.txt", "report font notice"),
            ("notices/showcase/showcase.txt", "showcase font notice"),
        ] {
            assert_eq!(
                fs::read_to_string(root.join("read").join(path)).unwrap(),
                expected
            );
            assert_eq!(
                fs::metadata(root.join("read").join(path))
                    .unwrap()
                    .modified()
                    .unwrap(),
                UNIX_EPOCH,
                "archive modification time for {path}"
            );
        }
        assert_eq!(
            fs::read(root.join("out/install.sh")).unwrap(),
            fs::read(root.join("read/uninstall.sh")).unwrap()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for (path, mode) in [
                ("bin/richview", 0o755),
                ("bin/richview-sql-worker", 0o755),
                ("uninstall.sh", 0o755),
                ("notices/LICENSE", 0o644),
                ("notices/THIRD-PARTY-NOTICES.txt", 0o644),
            ] {
                assert_eq!(
                    fs::metadata(root.join("read").join(path))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    mode,
                    "archive permission for {path}"
                );
            }
        }
        for line in fs::read_to_string(root.join("read/SHA256SUMS"))
            .unwrap()
            .lines()
        {
            let (expected, file) = line.split_once("  ").unwrap();
            assert_eq!(
                hash(&fs::read(root.join("read").join(file)).unwrap()),
                expected
            );
        }
    }

    #[test]
    fn existing_build_refuses_changed_missing_linked_or_dirty_inputs_before_replacing_an_archive() {
        let folder = fixture();
        let root = folder.path();
        let manifest = root.join("native/manifest.json");
        let record = json!({"schema":1,"mode":"full","inputs":inputs(),"outputs":{
            "target:release/richview":{"bytes":10,"sha256":hash(b"native CLI")},
            "target:release/richview-sql-worker":{"bytes":13,"sha256":hash(b"native worker")}
        }});
        fs::write(&manifest, record.to_string()).unwrap();
        let refused = root.join("native/install");
        assert!(existing(root, &manifest, &root.join("native"), &refused, None).is_err());
        assert!(!refused.exists());
        existing(
            root,
            &manifest,
            &root.join("native"),
            &root.join("out"),
            None,
        )
        .unwrap();
        assert!(
            existing(
                root,
                &manifest,
                &root.join("native"),
                &root.join("out"),
                None
            )
            .is_err()
        );
        let archive = root
            .join("out")
            .join("1".repeat(40))
            .join(format!("richview-{}.tar.gz", platform().unwrap()));
        let before = fs::read(&archive).unwrap();
        fs::write(root.join("native/richview-sql-worker"), b"changedworker").unwrap();
        assert!(
            existing(
                root,
                &manifest,
                &root.join("native"),
                &root.join("changed"),
                None
            )
            .unwrap_err()
            .to_string()
            .contains("differs from build manifest")
        );
        assert!(!root.join("changed").exists());
        assert_eq!(fs::read(&archive).unwrap(), before);
        fs::remove_file(root.join("native/richview-sql-worker")).unwrap();
        assert!(
            existing(
                root,
                &manifest,
                &root.join("native"),
                &root.join("missing"),
                None
            )
            .is_err()
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                root.join("native/richview"),
                root.join("native/richview-sql-worker"),
            )
            .unwrap();
            assert!(
                existing(
                    root,
                    &manifest,
                    &root.join("native"),
                    &root.join("linked"),
                    None
                )
                .unwrap_err()
                .to_string()
                .contains("not a regular file")
            );
            fs::remove_file(root.join("native/richview-sql-worker")).unwrap();
        }
        fs::write(root.join("native/richview-sql-worker"), b"native worker").unwrap();
        let mut dirty = record.clone();
        dirty["inputs"]["dirty"] = json!(true);
        fs::write(&manifest, dirty.to_string()).unwrap();
        assert!(
            existing(
                root,
                &manifest,
                &root.join("native"),
                &root.join("dirty"),
                None
            )
            .unwrap_err()
            .to_string()
            .contains("clean, committed build")
        );
        assert_eq!(fs::read(&archive).unwrap(), before);
    }

    #[test]
    fn cross_built_windows_archive_carries_exe_names_and_the_powershell_installer() {
        let folder = fixture();
        let root = folder.path();
        let manifest = root.join("native/manifest.json");
        let record = json!({"schema":1,"mode":"full","inputs":inputs(),"outputs":{}});
        fs::write(&manifest, record.to_string()).unwrap();
        let cross = root.join("windows");
        fs::create_dir(&cross).unwrap();
        fs::write(cross.join("richview.exe"), b"MZ CLI of commit 1111111111").unwrap();
        fs::write(cross.join("richview-sql-worker.exe"), b"MZ worker").unwrap();
        existing(root, &manifest, &cross, &root.join("out"), Some(WINDOWS)).unwrap();
        let name = "richview-x86_64-pc-windows-msvc.tar.gz";
        let archive = root.join("out").join("1".repeat(40)).join(name);
        assert_eq!(
            fs::read_to_string(archive.with_file_name(format!("{name}.sha256"))).unwrap(),
            format!("{}  {name}\n", hash(&fs::read(&archive).unwrap()))
        );
        fs::create_dir(root.join("read")).unwrap();
        assert!(
            Command::new("tar")
                .arg("-xzf")
                .arg(archive)
                .arg("-C")
                .arg(root.join("read"))
                .status()
                .unwrap()
                .success()
        );
        let build = format!(
            "commit={}\nplatform=x86_64-pc-windows-msvc\n",
            "1".repeat(40)
        );
        for (path, expected) in [
            ("bin/richview.exe", "MZ CLI of commit 1111111111"),
            ("bin/richview-sql-worker.exe", "MZ worker"),
            ("uninstall.ps1", "# PowerShell installer\n"),
            ("notices/LICENSE", "MIT notice"),
            ("BUILD", build.as_str()),
        ] {
            assert_eq!(
                fs::read_to_string(root.join("read").join(path)).unwrap(),
                expected
            );
        }
        assert_eq!(
            fs::read_to_string(root.join("out/install.ps1")).unwrap(),
            "# PowerShell installer\n"
        );
        for absent in ["read/uninstall.sh", "read/bin/richview", "out/install.sh"] {
            assert!(!root.join(absent).exists(), "{absent}");
        }
        let sums = fs::read_to_string(root.join("read/SHA256SUMS")).unwrap();
        assert!(sums.contains("  bin/richview.exe\n") && sums.contains("  uninstall.ps1\n"));
        // Binaries of another commit, the host's own binaries and another target are refused.
        for (cli, worker, target, message) in [
            (
                &b"MZ CLI of commit 2222222222"[..],
                &b"MZ worker"[..],
                WINDOWS,
                "is not a x86_64-pc-windows-msvc build of 1111",
            ),
            (
                b"\x7fELF CLI of commit 1111111111",
                b"MZ worker",
                WINDOWS,
                "is not a x86_64-pc-windows-msvc build of 1111",
            ),
            (
                b"MZ CLI of commit 1111111111",
                b"\x7fELF worker",
                WINDOWS,
                "richview-sql-worker.exe",
            ),
            (
                b"MZ CLI of commit 1111111111",
                b"MZ worker",
                "aarch64-pc-windows-msvc",
                "no cross-built install archive for aarch64-pc-windows-msvc",
            ),
        ] {
            fs::write(cross.join("richview.exe"), cli).unwrap();
            fs::write(cross.join("richview-sql-worker.exe"), worker).unwrap();
            let error = existing(root, &manifest, &cross, &root.join("refused"), Some(target))
                .unwrap_err()
                .to_string();
            assert!(error.contains(message), "{error}");
            assert!(!root.join("refused").exists());
        }
    }
}
