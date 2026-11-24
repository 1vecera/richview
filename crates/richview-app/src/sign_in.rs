//! Signing this computer in to a RichView server, in one of two ways. In the browser: the command
//! line shows a short code, its person confirms it on the server's page, and the command line
//! collects a token. Or by hand: the person pastes a CLI token, and the server says whose it is.
//! Either way the token is kept for that server alone (ARC-23, FEA-SF-12), is never printed and
//! is never part of a message or an address.
use crate::{credentials, ops::Failure, publish_destination::HOSTED};
use std::io::{BufRead, IsTerminal, Read, Write};

/// A token is one short line of visible characters; anything else was not copied from a server.
const MAX_TOKEN_BYTES: u64 = 4096;

/// A sign-in that was just kept.
pub(crate) struct SignedIn {
    pub token: String,
    pub account: Option<String>,
}

/// Where a person gets a token for `server`, in words.
fn token_source(server: &str) -> String {
    if server.trim_end_matches('/') == HOSTED {
        format!("Create a CLI token at {HOSTED}/tokens")
    } else {
        "Create a CLI token on that server".to_owned()
    }
}

/// The token as typed or piped: one line, trimmed; an empty line is no token.
fn token_line(line: &str) -> Result<Option<String>, Failure> {
    let token = line.trim();
    if token.is_empty() {
        return Ok(None);
    }
    if !token.chars().all(|c| c.is_ascii_graphic()) {
        return Err(Failure::usage(
            "That is not a CLI token: a token is one line without spaces.",
        ));
    }
    Ok(Some(token.to_owned()))
}

/// Reads one line from the terminal without showing it.
#[cfg(unix)]
fn hidden_line() -> Result<String, Failure> {
    struct Restore(libc::termios);
    impl Drop for Restore {
        fn drop(&mut self) {
            // SAFETY: restores the settings read from this same descriptor below.
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &self.0) };
        }
    }
    let mut settings = std::mem::MaybeUninit::<libc::termios>::uninit();
    // SAFETY: `tcgetattr` fills the struct it is given and reports failure by its result.
    if unsafe { libc::tcgetattr(libc::STDIN_FILENO, settings.as_mut_ptr()) } != 0 {
        return Err(Failure::host(
            "Cannot hide what is typed on this terminal; pipe the token to richview login --with-token instead.",
        ));
    }
    // SAFETY: `tcgetattr` succeeded, so the struct is initialized.
    let shown = unsafe { settings.assume_init() };
    let mut hidden = shown;
    hidden.c_lflag &= !libc::ECHO;
    hidden.c_lflag |= libc::ECHONL;
    // SAFETY: sets settings derived from the ones just read; `Restore` puts them back.
    if unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &hidden) } != 0 {
        return Err(Failure::host(
            "Cannot hide what is typed on this terminal; pipe the token to richview login --with-token instead.",
        ));
    }
    let _restore = Restore(shown);
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .take(MAX_TOKEN_BYTES)
        .read_line(&mut line)
        .map_err(|e| Failure::host(e.to_string()))?;
    Ok(line)
}

#[cfg(not(unix))]
fn hidden_line() -> Result<String, Failure> {
    Err(Failure::usage(
        "This system cannot hide what is typed; pipe the token to richview login --with-token instead.",
    ))
}

/// Asks for a token on the terminal without showing it. `None` when the person only pressed Enter.
pub(crate) fn ask_token(server: &str) -> Result<Option<String>, Failure> {
    eprintln!(
        "{} and paste it here; it is not shown. Enter alone cancels.",
        token_source(server)
    );
    eprint!("CLI token: ");
    std::io::stderr()
        .flush()
        .map_err(|e| Failure::host(e.to_string()))?;
    token_line(&hidden_line()?)
}

/// Reads a token from standard input: asked for without echo on a terminal, else the first line
/// of what was piped.
pub(crate) fn read_token(server: &str) -> Result<String, Failure> {
    let token = if std::io::stdin().is_terminal() {
        ask_token(server)?
    } else {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .take(MAX_TOKEN_BYTES)
            .read_line(&mut line)
            .map_err(|e| Failure::host(e.to_string()))?;
        token_line(&line)?
    };
    token.ok_or_else(|| Failure::usage("No token given; nothing was kept."))
}

/// Whether this run can ask its person a question: a terminal on both ends of the conversation.
pub(crate) fn can_ask() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Asks `server` whose token this is and keeps it for that server. A token the server does not
/// know is not kept.
pub(crate) async fn keep_token(server: &str, token: &str) -> Result<SignedIn, Failure> {
    let url = crate::remote::server_url(server).map_err(Failure::usage)?;
    let refused = || {
        Failure::usage(format!(
            "{server} did not accept this token; nothing was kept. {} and try again.",
            token_source(server)
        ))
    };
    let session =
        match crate::remote::send(&url, token, reqwest::Method::GET, "/api/session", None).await {
            Ok(session) => session,
            Err(error) if error.starts_with("The server did not accept this token.") => {
                return Err(refused());
            }
            Err(error) => {
                return Err(Failure::host(format!(
                    "Cannot ask {server} about this token: {error}"
                )));
            }
        };
    let user = &session["user"];
    if !user.is_object() {
        return Err(refused());
    }
    let account = account_label(user["name"].as_str(), user["email"].as_str(), None);
    credentials::keep(server, token, account.as_deref())?;
    Ok(SignedIn {
        token: token.to_owned(),
        account,
    })
}

/// What this computer calls itself, so its person recognizes the sign-in on the confirm page and
/// its token on the CLI tokens page.
fn computer_name() -> String {
    #[cfg(unix)]
    {
        let mut name = [0u8; 256];
        // SAFETY: `gethostname` writes at most the length it is given into this buffer.
        if unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len()) } == 0 {
            let end = name.iter().position(|b| *b == 0).unwrap_or(name.len());
            return String::from_utf8_lossy(&name[..end])
                .trim_end_matches(".local")
                .to_owned();
        }
    }
    std::env::var("COMPUTERNAME").unwrap_or_default()
}

/// Whether `code` is a short sign-in code: `XXXX-XXXX` in capital letters. Only such a code is
/// printed, put in an address or handed to a browser.
fn short_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 9
        && bytes.iter().enumerate().all(|(at, b)| {
            if at == 4 {
                *b == b'-'
            } else {
                b.is_ascii_uppercase()
            }
        })
}

/// Opens the confirm page in the person's browser, where there is one to open: not over SSH and,
/// on Linux, only inside a desktop session. A failure is silent; the address is printed anyway.
fn open_browser(address: &str) {
    if std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some() {
        return;
    }
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(unix)
        && (std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some())
    {
        "xdg-open"
    } else {
        return;
    };
    let _ = std::process::Command::new(program)
        .arg(address)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Text a server sent, kept short and free of control characters, so printing it cannot move the
/// terminal's cursor, change its title or hide a link.
fn plain(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// The server's own words for a refusal, made plain.
fn said(answer: &serde_json::Value, otherwise: &str) -> String {
    answer["error"]
        .as_str()
        .map_or_else(|| otherwise.to_owned(), plain)
}

/// Who a sign-in belongs to, in the server's words made plain: "Name (address)", then ", for
/// Organization" when the server names one. It is printed and kept as the file's label.
fn account_label(
    name: Option<&str>,
    email: Option<&str>,
    organization: Option<&str>,
) -> Option<String> {
    let who = match (name.map(plain), email.map(plain)) {
        (Some(name), Some(email)) => Some(format!("{name} ({email})")),
        (Some(one), None) | (None, Some(one)) => Some(one),
        (None, None) => None,
    };
    match (who, organization.map(plain)) {
        (Some(who), Some(organization)) => Some(format!("{who}, for {organization}")),
        (who, _) => who,
    }
}

/// Signs in through the browser. The command line starts a sign-in, shows its short code and the
/// page to confirm it on, and asks every few seconds whether its person has decided; a yes hands
/// over a token, which is kept. `None` when the server has no browser sign-in, so the caller
/// asks for a token instead. `open` also opens the page in the person's browser.
pub(crate) async fn in_browser(server: &str, open: bool) -> Result<Option<SignedIn>, Failure> {
    use reqwest::{Method, StatusCode};
    let url = crate::remote::server_url(server).map_err(Failure::usage)?;
    let (status, started) = crate::remote::call(
        &url,
        None,
        Method::POST,
        "/api/cli/sign-in",
        Some(&serde_json::json!({"computer": computer_name()})),
    )
    .await
    .map_err(|error| Failure::host(format!("Cannot reach {server}: {error}")))?;
    if status == StatusCode::TOO_MANY_REQUESTS {
        return Err(Failure::host(said(
            &started,
            "Too many sign-ins were started. Wait a few minutes and try again.",
        )));
    }
    if status != StatusCode::OK {
        // An older server, or one that only knows tokens.
        return Ok(None);
    }
    let unexpected = || {
        Failure::host(format!(
            "{server} answered the sign-in in a way this version does not understand."
        ))
    };
    let code = started["code"]
        .as_str()
        .filter(|code| {
            !code.is_empty() && code.len() <= 512 && code.chars().all(|c| c.is_ascii_graphic())
        })
        .ok_or_else(unexpected)?
        .to_owned();
    let user_code = started["userCode"]
        .as_str()
        .filter(|code| short_code(code))
        .ok_or_else(unexpected)?;
    let lasts = started["expiresIn"].as_u64().unwrap_or(600).clamp(30, 900);
    let mut every = started["interval"].as_u64().unwrap_or(3).clamp(1, 30);
    let mut page = url.join("/cli").map_err(|e| Failure::host(e.to_string()))?;
    page.set_query(Some(&format!("code={user_code}")));
    eprintln!("Confirm this code in your browser to sign in: {user_code}");
    eprintln!("{page}");
    eprintln!(
        "Waiting for you there; the code lasts {} minutes.",
        lasts / 60
    );
    if open {
        open_browser(page.as_str());
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(lasts);
    let collect = serde_json::json!({"code": code});
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(every)).await;
        if std::time::Instant::now() >= until {
            return Err(Failure::usage(
                "Nobody confirmed the sign-in in time; nothing was kept. Run the command again.",
            ));
        }
        let Ok((status, answer)) = crate::remote::call(
            &url,
            None,
            Method::POST,
            "/api/cli/sign-in/token",
            Some(&collect),
        )
        .await
        else {
            // A dropped connection is asked again; the time limit ends it.
            continue;
        };
        match (status, answer["status"].as_str()) {
            (StatusCode::OK, Some("waiting")) => {}
            (StatusCode::OK, Some("allowed")) => {
                let token = answer["token"]
                    .as_str()
                    .map(token_line)
                    .transpose()?
                    .flatten()
                    .ok_or_else(unexpected)?;
                let account = account_label(
                    answer["account"]["name"].as_str(),
                    answer["account"]["email"].as_str(),
                    answer["organization"].as_str(),
                );
                credentials::keep(server, &token, account.as_deref())?;
                return Ok(Some(SignedIn { token, account }));
            }
            (StatusCode::TOO_MANY_REQUESTS, _) => every = (every * 2).min(30),
            (StatusCode::FORBIDDEN, _) => {
                return Err(Failure::usage(
                    "The sign-in was cancelled in the browser; nothing was kept.",
                ));
            }
            (StatusCode::GONE, _) => {
                return Err(Failure::usage(
                    "The sign-in expired or was already used; nothing was kept. Run the command again.",
                ));
            }
            _ => {
                return Err(Failure::host(said(
                    &answer,
                    "The server could not complete the sign-in. Run the command again.",
                )));
            }
        }
    }
}

/// Whether a run may open a browser: only for a person at a terminal who has not said no.
pub(crate) fn may_open_browser() -> bool {
    can_ask() && std::env::var_os("RICHVIEW_NO_BROWSER").is_none_or(|v| v.is_empty())
}

/// What a server said when `richview logout` asked it to end a kept token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Revoked {
    /// The server revoked it.
    Yes,
    /// The server no longer knew it.
    Unknown,
    /// The server has no way for a command line to revoke its token: a self-hosted server, whose
    /// people's tokens an administrator revokes and whose own token only its settings change, or
    /// a server older than the browser sign-in.
    NotOffered,
    /// The server could not be asked.
    NotAsked,
}

/// Ends a kept token on its server, for `richview logout`.
pub(crate) async fn revoke(server: &str, token: &str) -> Revoked {
    let Ok(url) = crate::remote::server_url(server) else {
        return Revoked::NotAsked;
    };
    let answer = crate::remote::call(
        &url,
        Some(token),
        reqwest::Method::DELETE,
        "/api/cli/token",
        None,
    )
    .await;
    match answer.map(|(status, _)| status) {
        Ok(reqwest::StatusCode::OK) => Revoked::Yes,
        Ok(reqwest::StatusCode::UNAUTHORIZED) => Revoked::Unknown,
        Ok(reqwest::StatusCode::NOT_FOUND) => Revoked::NotOffered,
        _ => Revoked::NotAsked,
    }
}

/// One line saying who is signed in where.
pub(crate) fn signed_in_line(server: &str, account: Option<&str>) -> String {
    let who = account.map_or(String::new(), |account| format!(" as {account}"));
    format!("Signed in to {server}{who}. This computer stays signed in; `richview logout` ends it.")
}

/// A command on a terminal found no token for `server`: sign its person in, through the browser
/// where the server offers that and with a pasted token otherwise, then carry on. `None` when
/// the person cancelled.
pub(crate) async fn offer(server: &str) -> Result<Option<String>, Failure> {
    eprintln!("This computer is not signed in to {server}.");
    if let Some(signed_in) = in_browser(server, may_open_browser()).await? {
        eprintln!("{}", signed_in_line(server, signed_in.account.as_deref()));
        return Ok(Some(signed_in.token));
    }
    let Some(token) = ask_token(server)? else {
        return Ok(None);
    };
    let signed_in = keep_token(server, &token).await?;
    eprintln!("{}", signed_in_line(server, signed_in.account.as_deref()));
    Ok(Some(signed_in.token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_one_line_of_visible_characters() {
        assert_eq!(token_line("rvt_abc\n").unwrap().as_deref(), Some("rvt_abc"));
        assert_eq!(
            token_line("  rvt_abc \r\n").unwrap().as_deref(),
            Some("rvt_abc")
        );
        assert!(token_line("\n").unwrap().is_none());
        assert!(token_line("").unwrap().is_none());
        for wrong in ["rvt abc", "rvt\tabc", "rvt_\u{1b}[0m", "žeton"] {
            let refused = token_line(wrong).err().unwrap();
            // The refusal never repeats what was typed.
            assert!(!refused.message().contains("rvt"), "{}", refused.message());
        }
    }

    #[test]
    fn only_a_short_code_is_shown_or_put_in_an_address() {
        assert!(short_code("BCDF-GHJK"));
        for wrong in [
            "",
            "BCDFGHJK",
            "bcdf-ghjk",
            "BCDF-GHJ",
            "BCDF-GHJKL",
            "BCDF GHJK",
            "BCD\u{1b}-GHJK",
            "BCDF-GH&K",
            "ČCDF-GHJK",
        ] {
            assert!(!short_code(wrong), "{wrong:?}");
        }
        assert_eq!(
            said(&serde_json::json!({"error": "No\u{1b}[31m room"}), "x"),
            "No[31m room"
        );
        assert_eq!(said(&serde_json::json!({}), "Otherwise."), "Otherwise.");
        assert_eq!(
            account_label(
                Some("Ada\u{1b}]0;owned\u{7}"),
                Some("ada@example.test\r"),
                Some("Org\u{1b}[2J\n")
            )
            .as_deref(),
            Some("Ada]0;owned (ada@example.test), for Org[2J")
        );
        assert_eq!(
            account_label(None, Some("ada@example.test"), None).as_deref(),
            Some("ada@example.test")
        );
    }

    #[test]
    fn the_sign_in_line_names_the_server_and_the_account_and_no_token() {
        let line = signed_in_line("https://app.richview.ai", Some("Ada (ada@example.test)"));
        assert!(
            line.starts_with("Signed in to https://app.richview.ai as Ada (ada@example.test).")
        );
        assert!(
            signed_in_line("http://127.0.0.1:1", None)
                .starts_with("Signed in to http://127.0.0.1:1. ")
        );
    }
}
