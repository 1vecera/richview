//! Signing this computer in to a RichView server, in one of three ways. In the browser on this
//! computer: the command line opens the server's sign-in page, its person signs in the way they
//! always do and clicks Allow, and the browser comes back to the command line (`sign_in_loopback`).
//! With a short code, where there is no browser here: the command line shows a code, its person
//! confirms it on any device, and the command line collects a token. Or by hand: the person
//! pastes a CLI token, and the server says whose it is. Whichever way, the token is kept for that
//! server alone (ARC-23, FEA-SF-12), is never printed and is never part of a message or an address.
use crate::{credentials, ops::Failure, publish_destination::HOSTED};
use std::io::{BufRead, IsTerminal, Read, Write};

/// A token is one short line of visible characters; anything else was not copied from a server.
const MAX_TOKEN_BYTES: u64 = 4096;

/// A sign-in that was just kept.
pub(crate) struct SignedIn {
    pub token: String,
    pub account: Option<String>,
}

/// What a server's sign-in handed over: a token, and from a server that renews sign-ins, the
/// refresh token that renews it and the seconds it lasts. Nothing is kept until the person at the
/// terminal has seen whose it is and said so (`keep_after_confirming`).
pub(crate) struct Granted {
    token: String,
    refresh: Option<String>,
    lasts: Option<u64>,
    account: Option<String>,
    /// The server's words for who this is, each made plain: what the person is asked about, and
    /// what an expected account is compared with.
    email: Option<String>,
    organization: Option<String>,
    name: Option<String>,
    /// The address, the organization and its id as the server wrote them, whole. What is shown is
    /// cut short and made plain; what an expectation is compared with is not.
    email_whole: Option<String>,
    organization_whole: Option<String>,
    organization_id: Option<String>,
}

/// The longest a token's life may be claimed to be; a server that says more is believed to say a day.
const MAX_LASTS_SECONDS: u64 = 24 * 3600;

/// The sign-in a server's answer holds: `None` when it holds none a person could use.
fn grant_of(answer: &serde_json::Value) -> Option<Granted> {
    let one_line = |value: &serde_json::Value| {
        value
            .as_str()
            .and_then(|text| token_line(text).ok().flatten())
    };
    let token = one_line(&answer["token"])?;
    let refresh = one_line(&answer["refreshToken"]);
    let lasts = answer["expiresIn"]
        .as_u64()
        .map(|seconds| seconds.clamp(60, MAX_LASTS_SECONDS));
    // A token that ends comes with the way to renew it, and the other way round.
    if refresh.is_some() != lasts.is_some() {
        return None;
    }
    Some(Granted {
        token,
        refresh,
        lasts,
        email: answer["account"]["email"].as_str().map(plain),
        organization: answer["organization"].as_str().map(plain),
        name: answer["account"]["name"].as_str().map(plain),
        email_whole: answer["account"]["email"].as_str().map(str::to_owned),
        organization_whole: answer["organization"].as_str().map(str::to_owned),
        organization_id: answer["organizationId"].as_str().map(str::to_owned),
        account: account_label(
            answer["account"]["name"].as_str(),
            answer["account"]["email"].as_str(),
            answer["organization"].as_str(),
        ),
    })
}

/// Shows the person whose sign-in the server handed over, and asks whether to keep it. Only a yes
/// is a yes: anything else, an empty line or the end of the input is a no. The sign-in came back
/// from a page that anything on this computer, or anyone the person was sent a link by, could
/// have answered with an account of their own, and everything this computer publishes from now on
/// goes to the account and organization shown here. The address and the organization come first,
/// each on a line of its own, and the name, which the account's owner chose and could have made
/// to read like either, comes last and in quotation marks.
pub(crate) fn ask_to_keep(
    granted: &Granted,
    input: &mut impl std::io::BufRead,
    out: &mut impl Write,
) -> bool {
    let quoted = granted
        .name
        .as_ref()
        .map(|name| format!("\u{201c}{name}\u{201d}"));
    // The whole address, which is what a sign-in is compared by: two addresses that read the same
    // for as long as a line of the prompt shows are not the same address.
    let address = granted
        .email_whole
        .as_deref()
        .map(|address| shown_whole(address, MOST_SHOWN_OF_AN_ADDRESS))
        .or_else(|| granted.email.clone());
    let _ = writeln!(out, "The server signed this computer in with:");
    let _ = writeln!(out, "  address       {}", or_unsaid(address.as_deref()));
    let _ = writeln!(
        out,
        "  organization  {}",
        or_unsaid(granted.organization.as_deref())
    );
    if let Some(id) = granted.organization_id.as_deref() {
        // What a rename does not change, and what `--expect-organization-id` is compared with.
        let _ = writeln!(
            out,
            "  organization id  {}",
            shown_whole(id, MOST_SHOWN_OF_AN_ID)
        );
    }
    let _ = writeln!(out, "  name          {}", or_unsaid(quoted.as_deref()));
    let _ = write!(
        out,
        "Keep this sign-in? Answer y only if that is you and the organization you meant [y/N]: "
    );
    let _ = out.flush();
    let mut line = String::new();
    if input.read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

/// The most characters of an address the prompt shows: an address is at most 254, so a longer text
/// is not one and is shown cut, and said to be.
const MOST_SHOWN_OF_AN_ADDRESS: usize = 254;
const MOST_SHOWN_OF_AN_ID: usize = 64;

/// `text` made plain but not cut to what [`plain`] keeps: all of it up to `most` characters, and
/// marked when it is longer.
fn shown_whole(text: &str, most: usize) -> String {
    let kept: Vec<char> = text
        .chars()
        .filter(|c| !c.is_control() && !is_format_character(*c))
        .collect();
    if kept.len() <= most {
        return kept.into_iter().collect();
    }
    format!(
        "{} … (cut: {} characters in all)",
        kept[..most].iter().collect::<String>(),
        kept.len()
    )
}

/// What the server said about a sign-in, or that it said nothing.
fn or_unsaid(value: Option<&str>) -> String {
    value.map_or_else(|| "(the server did not say)".to_owned(), str::to_owned)
}

/// What a person says a sign-in must be for, when nobody is at the terminal to say whether it is.
#[derive(Clone, Debug, Default)]
pub(crate) struct Expected {
    /// The address of the account (`--expect-account`).
    pub account: Option<String>,
    /// The organization's name, as the server writes it (`--expect-organization`).
    pub organization_name: Option<String>,
    /// The organization's id, which a rename does not change (`--expect-organization-id`). It is
    /// compared with the id and the name with the name, so an organization that is named like
    /// another one's id is never taken for it.
    pub organization_id: Option<String>,
}

/// Whether the address a server named is the one that was expected: both as one bare canonical
/// address, whole. A longer address that begins like the expected one is another address, and one
/// that is no address at all is never the expected one.
fn same_address(named: &str, expected: &str) -> bool {
    match (
        crate::email::normalize(named),
        crate::email::normalize(expected),
    ) {
        (Some(named), Some(expected)) => named == expected,
        _ => false,
    }
}

/// Why a sign-in just handed over is not to be kept, or `None` when whose it is has been
/// confirmed: by the person at this terminal, who is shown it and says yes, or by `expected`, the
/// address the person said it must be for, which the server's answer must match whole, and the
/// organization too when one was named. With neither (a script, a pipe, `--format json` and no
/// expected address) nobody can say whose it is, and a short code shown in a log can be confirmed
/// by anyone who reads it. An organization alone is no confirmation: it says where, not whose.
fn refusal_for(granted: &Granted, person_at_terminal: bool, expected: &Expected) -> Option<String> {
    if let Some(account) = expected.account.as_deref() {
        let got = granted.email_whole.as_deref();
        if !got.is_some_and(|named| same_address(named, account)) {
            return Some(format!(
                "The server signed this computer in as {}, and not as {}, so nothing was kept.",
                granted
                    .email
                    .as_deref()
                    .unwrap_or("an account it did not name"),
                plain(account.trim())
            ));
        }
    }
    for (said, got, expected) in [
        (
            "organization",
            granted.organization_whole.as_deref(),
            expected.organization_name.as_deref(),
        ),
        (
            "organization id",
            granted.organization_id.as_deref(),
            expected.organization_id.as_deref(),
        ),
    ] {
        if let Some(wanted) = expected
            && (wanted.is_empty() || got != Some(wanted))
        {
            return Some(format!(
                "The server signed this computer in for {}, and not for the {said} {}, so nothing was kept.",
                granted
                    .organization
                    .as_deref()
                    .unwrap_or("an organization it did not name"),
                plain(wanted)
            ));
        }
    }
    if expected.account.is_some() {
        return None;
    }
    if person_at_terminal && can_ask() {
        let confirmed = ask_to_keep(
            granted,
            &mut std::io::stdin().lock(),
            &mut std::io::stderr(),
        );
        return (!confirmed)
            .then(|| "That is not the sign-in you meant, so nothing was kept.".to_owned());
    }
    Some("Nobody at this terminal can confirm whose sign-in this is, so nothing was kept. Run `richview login` in a terminal, or add --expect-account <address> to name the account it must be.".to_owned())
}

/// Keeps a sign-in the server just handed over once `refusal_for` has nothing against it. A
/// refusal ends the sign-in on the server too, so that nothing waits unused, and says whether that
/// worked.
async fn keep_after_confirming(
    server: &str,
    granted: Granted,
    person_at_terminal: bool,
    expected: &Expected,
) -> Result<SignedIn, Failure> {
    let Some(refusal) = refusal_for(&granted, person_at_terminal, expected) else {
        return keep_granted(server, granted);
    };
    let kept = credentials::Kept::new(
        &granted.token,
        granted.refresh.as_deref(),
        granted.lasts,
        granted.account.as_deref(),
    );
    Err(Failure::usage(format!(
        "{refusal} {}",
        revocation_said(server, revoke(server, &kept).await)
    )))
}

/// What a refused sign-in's end on the server comes to, in words that claim only what the server
/// answered: that it ended it, that it did not know it, or that nothing could be established and
/// the person ends it on the server's CLI tokens page.
fn revocation_said(server: &str, revoked: Revoked) -> String {
    let page = server.trim_end_matches('/');
    match revoked {
        Revoked::Yes => "The sign-in was ended on the server.".to_owned(),
        Revoked::Unknown => {
            "The server did not know this sign-in, so there is nothing left to end.".to_owned()
        }
        Revoked::NotOffered => format!(
            "This server has no way to end a sign-in from the command line, so it may still be active: end it on {page}'s CLI tokens page."
        ),
        Revoked::NotAsked => format!(
            "The server could not be asked to end it, so it may still be active: end it on {page}'s CLI tokens page."
        ),
    }
}

/// Keeps what a server handed over, for that server.
pub(crate) fn keep_granted(server: &str, granted: Granted) -> Result<SignedIn, Failure> {
    credentials::keep(
        server,
        &credentials::Kept::new(
            &granted.token,
            granted.refresh.as_deref(),
            granted.lasts,
            granted.account.as_deref(),
        ),
    )?;
    Ok(SignedIn {
        token: granted.token,
        account: granted.account,
    })
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
/// know is not kept, nor is one that is another account's than `expected` names, whole: a token
/// someone pasted is checked the way a sign-in is, the organization too. A token is bound to the
/// organization it was made for, and the server says which; one that says nothing of it, or
/// another, does not meet an expected organization and is not kept.
pub(crate) async fn keep_token(
    server: &str,
    token: &str,
    expected: &Expected,
) -> Result<SignedIn, Failure> {
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
    if let Some(account) = expected.account.as_deref()
        && !user["email"]
            .as_str()
            .is_some_and(|named| same_address(named, account))
    {
        return Err(Failure::usage(format!(
            "This token is for {}, and not for {}, so nothing was kept.",
            user["email"]
                .as_str()
                .map_or_else(|| "an account the server did not name".to_owned(), plain),
            plain(account.trim())
        )));
    }
    for (said, got, wanted) in [
        (
            "organization",
            session["organization"]["name"].as_str(),
            expected.organization_name.as_deref(),
        ),
        (
            "organization id",
            session["organization"]["id"].as_str(),
            expected.organization_id.as_deref(),
        ),
    ] {
        if let Some(wanted) = wanted
            && (wanted.is_empty() || got != Some(wanted))
        {
            return Err(Failure::usage(format!(
                "This token is for {}, and not for the {said} {}, so nothing was kept.",
                session["organization"]["name"].as_str().map_or_else(
                    || "an organization the server did not name".to_owned(),
                    plain
                ),
                plain(wanted)
            )));
        }
    }
    let account = account_label(
        user["name"].as_str(),
        user["email"].as_str(),
        session["organization"]["name"].as_str(),
    );
    credentials::keep_token(server, token, account.as_deref())?;
    Ok(SignedIn {
        token: token.to_owned(),
        account,
    })
}

/// What this computer calls itself, so its person recognizes the sign-in on the confirm page and
/// its token on the CLI tokens page.
pub(crate) fn computer_name() -> String {
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

/// Whether this computer has a browser the person is looking at: not over SSH and, on Linux,
/// only inside a desktop session. A sign-in through this computer's browser needs one.
fn browser_here() -> bool {
    if std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some() {
        return false;
    }
    cfg!(any(target_os = "macos", windows))
        || (cfg!(unix)
            && (std::env::var_os("DISPLAY").is_some()
                || std::env::var_os("WAYLAND_DISPLAY").is_some()))
}

/// Opens `address` in the person's browser, where there is one to open. A failure is silent; the
/// address is printed anyway.
pub(crate) fn open_browser(address: &str) {
    if !browser_here() {
        return;
    }
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        // `rundll32` takes the address as one argument; `cmd /c start` would read its `&`.
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler");
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    let _ = command
        .arg(address)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Text a server sent, kept short and free of control characters, so printing it cannot move the
/// terminal's cursor, change its title or hide a link.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() && !is_format_character(*c))
        .take(200)
        .collect()
}

/// Characters that print nothing and reorder or join what is around them (the invisible ones of
/// Unicode's format category): a name can hide behind them or read backwards.
fn is_format_character(c: char) -> bool {
    matches!(c, '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{FEFF}')
        || ('\u{200B}'..='\u{200F}').contains(&c)
        || ('\u{202A}'..='\u{202E}').contains(&c)
        || ('\u{2060}'..='\u{206F}').contains(&c)
        || ('\u{FFF9}'..='\u{FFFB}').contains(&c)
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

/// How a command asks to be signed in.
pub(crate) struct Way {
    /// The account the sign-in must be for (`--expect-account`), which stands in for asking when
    /// nobody is at the terminal, and the organization (`--expect-organization` by name,
    /// `--expect-organization-id` by id).
    pub expect: Expected,
    /// The person asked for a short code to confirm on any device (`--device`).
    pub device: bool,
    /// Open the page in the person's browser; else only print its address.
    pub open: bool,
    /// A person is at this terminal, so a browser on this computer is theirs to use.
    pub terminal: bool,
}

/// What a server says about how it signs a command line in.
struct Methods {
    /// Through a redirect to this computer's loopback, with PKCE.
    loopback: bool,
    /// The server renews a sign-in with a refresh token, so asks for one.
    refresh: bool,
}

/// Asks `server` how it signs a command line in. `None` for a server that does not say: an older
/// one, or one that only knows tokens.
async fn methods(server: &str, url: &reqwest::Url) -> Result<Option<Methods>, Failure> {
    let (status, answer) = crate::remote::call(
        url,
        None,
        reqwest::Method::GET,
        "/api/cli/oauth/metadata",
        None,
    )
    .await
    .map_err(|error| Failure::host(format!("Cannot reach {server}: {error}")))?;
    if status != reqwest::StatusCode::OK {
        return Ok(None);
    }
    Ok(Some(Methods {
        loopback: answer["flows"]
            .as_array()
            .is_some_and(|flows| flows.iter().any(|flow| flow == "loopback")),
        refresh: answer["refresh"].as_bool() == Some(true),
    }))
}

/// Signs in through the browser: on this computer's own when a person is at its terminal and the
/// server offers it, and with a short code to confirm anywhere else. The sign-in is kept. `None`
/// when the server has no browser sign-in, so the caller asks for a token instead.
pub(crate) async fn in_browser(server: &str, way: Way) -> Result<Option<SignedIn>, Failure> {
    let url = crate::remote::server_url(server).map_err(Failure::usage)?;
    let offered = methods(server, &url).await?;
    // `--no-browser` means the browser is not on this computer or not for this command to open, so
    // the address a loopback sign-in prints would lead nowhere: the short code works anywhere.
    let granted = if !way.device
        && way.terminal
        && way.open
        && browser_here()
        && offered.as_ref().is_some_and(|offered| offered.loopback)
        && let Some(granted) = crate::sign_in_loopback::sign_in(&url, way.open).await?
    {
        Some(granted)
    } else {
        with_code(
            server,
            way.open,
            offered.is_some_and(|offered| offered.refresh),
        )
        .await?
    };
    match granted {
        Some(granted) => keep_after_confirming(server, granted, way.terminal, &way.expect)
            .await
            .map(Some),
        None => Ok(None),
    }
}

/// Trades the one-time code the browser brought back, and the verifier only this command line
/// knows, for the sign-in.
pub(crate) async fn trade(
    url: &reqwest::Url,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<Granted, Failure> {
    let (status, answer) = crate::remote::call(
        url,
        None,
        reqwest::Method::POST,
        "/api/cli/oauth/token",
        Some(&serde_json::json!({
            "grantType": "authorization_code",
            "code": code,
            "codeVerifier": verifier,
            "redirectUri": redirect_uri,
        })),
    )
    .await
    .map_err(|error| {
        Failure::host(format!(
            "Cannot reach the server to finish the sign-in: {error}"
        ))
    })?;
    match status {
        reqwest::StatusCode::OK => grant_of(&answer).ok_or_else(|| {
            Failure::host(
                "The server answered the sign-in in a way this version does not understand.",
            )
        }),
        reqwest::StatusCode::TOO_MANY_REQUESTS => Err(Failure::host(said(
            &answer,
            "Too many sign-ins were started. Wait a few minutes and try again.",
        ))),
        _ => Err(Failure::usage(said(
            &answer,
            "The server refused this sign-in; nothing was kept. Run the command again.",
        ))),
    }
}

/// Why a sign-in could not be renewed.
pub(crate) enum Renewal {
    /// The server says this sign-in is over: it was signed out, ran out or was used twice.
    Ended,
    /// The server could not be asked, or did not answer well; the sign-in may still be good.
    Unreachable(String),
}

/// A renewed sign-in, as it is to be kept.
pub(crate) struct Renewed {
    pub token: String,
    pub account: Option<String>,
    pub kept: credentials::Kept,
}

/// Trades a refresh token for the next token and the next refresh token.
pub(crate) async fn renew(server: &str, refresh: &str) -> Result<Renewed, Renewal> {
    let url = crate::remote::server_url(server).map_err(Renewal::Unreachable)?;
    let (status, answer) = crate::remote::call(
        &url,
        None,
        reqwest::Method::POST,
        "/api/cli/oauth/token",
        Some(&serde_json::json!({"grantType": "refresh_token", "refreshToken": refresh})),
    )
    .await
    .map_err(Renewal::Unreachable)?;
    match status {
        reqwest::StatusCode::OK => {
            let granted = grant_of(&answer).filter(|granted| granted.refresh.is_some());
            let granted = granted.ok_or_else(|| {
                Renewal::Unreachable(
                    "the server answered in a way this version does not understand".into(),
                )
            })?;
            Ok(Renewed {
                kept: credentials::Kept::new(
                    &granted.token,
                    granted.refresh.as_deref(),
                    granted.lasts,
                    granted.account.as_deref(),
                ),
                token: granted.token,
                account: granted.account,
            })
        }
        // Only the server's own word ends a sign-in: a proxy that says no, a deploy that has not
        // finished or a server of another version leaves it for the next try.
        reqwest::StatusCode::UNAUTHORIZED if answer["status"] == "ended" => Err(Renewal::Ended),
        other => Err(Renewal::Unreachable(format!("the server answered {other}"))),
    }
}

/// Signs in with a short code. The command line starts a sign-in, shows its short code and the
/// page to confirm it on, and asks every few seconds whether its person has decided; a yes hands
/// over a token, which the caller keeps once its person confirms whose it is. `None` when the server has no such sign-in, so the caller asks for
/// a token instead. `open` also opens the page in the person's browser; `refresh` asks for a
/// token that renews itself.
async fn with_code(server: &str, open: bool, refresh: bool) -> Result<Option<Granted>, Failure> {
    use reqwest::{Method, StatusCode};
    let url = crate::remote::server_url(server).map_err(Failure::usage)?;
    let (status, started) = crate::remote::call(
        &url,
        None,
        Method::POST,
        "/api/cli/sign-in",
        Some(&serde_json::json!({"computer": computer_name(), "refresh": refresh})),
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
                return Ok(Some(grant_of(&answer).ok_or_else(unexpected)?));
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

/// Ends a kept sign-in on its server, for `richview logout`. A sign-in that renews itself is
/// ended by its refresh token, which still says who it is when the token has run out.
pub(crate) async fn revoke(server: &str, kept: &credentials::Kept) -> Revoked {
    let Ok(url) = crate::remote::server_url(server) else {
        return Revoked::NotAsked;
    };
    if let Some(refresh) = &kept.refresh {
        let answer = crate::remote::call(
            &url,
            None,
            reqwest::Method::POST,
            "/api/cli/oauth/revoke",
            Some(&serde_json::json!({"token": refresh})),
        )
        .await;
        // Only an answer that says so is taken for one: an answer without the boolean, or with
        // another thing in its place, is not known to be either, and ends nothing for certain.
        return match answer {
            Ok((reqwest::StatusCode::OK, body)) => revoked_by(&body, Revoked::Unknown),
            _ => Revoked::NotAsked,
        };
    }
    let answer = crate::remote::call(
        &url,
        Some(&kept.token),
        reqwest::Method::DELETE,
        "/api/cli/token",
        None,
    )
    .await;
    match answer {
        Ok((reqwest::StatusCode::OK, body)) => revoked_by(&body, Revoked::NotAsked),
        Ok((reqwest::StatusCode::UNAUTHORIZED, _)) => Revoked::Unknown,
        Ok((reqwest::StatusCode::NOT_FOUND, _)) => Revoked::NotOffered,
        _ => Revoked::NotAsked,
    }
}

/// What a `200` answer to a revocation says: `revoked: true` is a yes, `revoked: false` that the
/// server did not know the sign-in, and anything else, a boolean that is missing or is not one,
/// is `unproven`.
fn revoked_by(body: &serde_json::Value, unproven: Revoked) -> Revoked {
    match body["revoked"].as_bool() {
        Some(true) => Revoked::Yes,
        Some(false) => Revoked::Unknown,
        None => unproven,
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
    let way = Way {
        expect: Expected::default(),
        device: false,
        open: may_open_browser(),
        terminal: true,
    };
    if let Some(signed_in) = in_browser(server, way).await? {
        eprintln!("{}", signed_in_line(server, signed_in.account.as_deref()));
        return Ok(Some(signed_in.token));
    }
    let Some(token) = ask_token(server)? else {
        return Ok(None);
    };
    let signed_in = keep_token(server, &token, &Expected::default()).await?;
    eprintln!("{}", signed_in_line(server, signed_in.account.as_deref()));
    Ok(Some(signed_in.token))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_explicit_boolean_says_whether_a_sign_in_was_ended() {
        use serde_json::json;
        assert_eq!(
            revoked_by(&json!({"revoked": true}), Revoked::NotAsked),
            Revoked::Yes
        );
        assert_eq!(
            revoked_by(&json!({"revoked": false}), Revoked::NotAsked),
            Revoked::Unknown
        );
        for body in [
            json!({}),
            json!({"ok": true}),
            json!({"revoked": "true"}),
            json!({"revoked": 1}),
            json!({"revoked": null}),
            json!("revoked"),
            json!(null),
        ] {
            assert_eq!(
                revoked_by(&body, Revoked::NotAsked),
                Revoked::NotAsked,
                "{body}"
            );
        }
    }

    #[test]
    fn a_refused_sign_in_claims_only_what_the_server_answered_about_ending_it() {
        let said = |revoked| revocation_said("https://app.test/", revoked);
        assert_eq!(said(Revoked::Yes), "The sign-in was ended on the server.");
        assert!(said(Revoked::Unknown).contains("did not know this sign-in"));
        for unproven in [Revoked::NotOffered, Revoked::NotAsked] {
            let words = said(unproven);
            // Nothing is claimed to have ended, and the person is told where to end it.
            assert!(
                words.contains("may still be active")
                    && words.contains("https://app.test's CLI tokens page")
                    && !words.contains("no longer lists"),
                "{words}"
            );
        }
    }

    fn granted_for(email: &str, organization: &str, id: &str) -> Granted {
        Granted {
            token: "rvt_x".into(),
            refresh: None,
            lasts: None,
            account: None,
            email: Some(plain(email)),
            organization: Some(plain(organization)),
            name: None,
            email_whole: Some(email.into()),
            organization_whole: Some(organization.into()),
            organization_id: Some(id.into()),
        }
    }

    fn expecting(account: Option<&str>, organization: Option<&str>) -> Expected {
        Expected {
            account: account.map(str::to_owned),
            organization_name: organization.map(str::to_owned),
            organization_id: None,
        }
    }

    fn expecting_id(account: Option<&str>, id: Option<&str>) -> Expected {
        Expected {
            account: account.map(str::to_owned),
            organization_name: None,
            organization_id: id.map(str::to_owned),
        }
    }

    #[test]
    fn an_expected_address_is_compared_whole_and_never_by_what_is_shown_of_it() {
        // Two addresses that read the same for 200 characters and are not the same.
        let tail = format!("{}.{}.{}", "d".repeat(61), "e".repeat(61), "f".repeat(61));
        let long = format!("{}@{tail}.test", "u".repeat(60));
        let other = format!("{}@{tail}.tesu", "u".repeat(60));
        assert!(long.len() > 200 && long.len() <= 254, "{}", long.len());
        let granted = granted_for(&long, "Org", "o1");
        assert_eq!(granted.email, granted_for(&other, "Org", "o1").email);
        // The prompt shows the whole of each, so the person can tell them apart.
        let (mut first, mut second) = (Vec::new(), Vec::new());
        assert!(!ask_to_keep(&granted, &mut "n\n".as_bytes(), &mut first));
        assert!(!ask_to_keep(
            &granted_for(&other, "Org", "o1"),
            &mut "n\n".as_bytes(),
            &mut second
        ));
        let (first, second) = (
            String::from_utf8(first).unwrap(),
            String::from_utf8(second).unwrap(),
        );
        assert!(
            first.contains(&format!("address       {long}\n")),
            "{first}"
        );
        assert!(
            second.contains(&format!("address       {other}\n")),
            "{second}"
        );
        assert!(first.contains("organization id  o1\n"), "{first}");
        // A valid address longer than what is shown is the expected one, in any case.
        let upper = long.to_ascii_uppercase();
        assert!(refusal_for(&granted, false, &expecting(Some(&upper), None)).is_none());
        // The other address, which is shown the same, is not.
        let refusal = refusal_for(&granted, false, &expecting(Some(&other), None)).unwrap();
        assert!(refusal.contains("nothing was kept"), "{refusal}");
        // An expectation that is no address at all is never met, and nor is no address at all.
        assert!(refusal_for(&granted, false, &expecting(Some("ada"), None)).is_some());
        let mut unnamed = granted_for(&long, "Org", "o1");
        unnamed.email_whole = None;
        assert!(refusal_for(&unnamed, false, &expecting(Some(&long), None)).is_some());
    }

    #[test]
    fn an_expected_organization_is_named_by_its_whole_name_or_by_its_id_never_by_either() {
        let granted = granted_for("ada@example.test", "Ada Analytics", "aaaa0001");
        let account = Some("ada@example.test");
        assert!(refusal_for(&granted, false, &expecting(account, Some("Ada Analytics"))).is_none());
        assert!(refusal_for(&granted, false, &expecting_id(account, Some("aaaa0001"))).is_none());
        for named in ["ada analytics", "Ada", "bbbb0002", "aaaa0001", ""] {
            let refusal = refusal_for(&granted, false, &expecting(account, Some(named))).unwrap();
            assert!(
                refusal.contains("for Ada Analytics, and not for the organization"),
                "{refusal}"
            );
        }
        for named in ["Ada Analytics", "AAAA0001", "aaaa", ""] {
            let refusal =
                refusal_for(&granted, false, &expecting_id(account, Some(named))).unwrap();
            assert!(
                refusal.contains("and not for the organization id"),
                "{refusal}"
            );
        }
        // An organization that is named like another one's id is not that one: the id flag looks
        // at ids alone, and the name flag at names alone.
        let named_like_an_id = granted_for("ada@example.test", "bbbb0002", "aaaa0001");
        assert!(
            refusal_for(
                &named_like_an_id,
                false,
                &expecting_id(account, Some("bbbb0002"))
            )
            .is_some()
        );
        assert!(
            refusal_for(
                &named_like_an_id,
                false,
                &expecting(account, Some("aaaa0001"))
            )
            .is_some()
        );
        // Both may be given, and both must hold.
        let both = Expected {
            account: account.map(str::to_owned),
            organization_name: Some("Ada Analytics".into()),
            organization_id: Some("bbbb0002".into()),
        };
        assert!(refusal_for(&granted, false, &both).is_some());
        // The right organization of another account is still another account.
        assert!(
            refusal_for(
                &granted,
                false,
                &expecting(Some("grace@example.test"), Some("aaaa0001"))
            )
            .is_some()
        );
        // Where, without whose, confirms nothing: with nobody to ask, nothing is kept.
        let alone = refusal_for(&granted, false, &expecting_id(None, Some("aaaa0001"))).unwrap();
        assert!(alone.contains("Nobody at this terminal"), "{alone}");
    }

    #[test]
    fn the_prompt_puts_the_address_and_the_organization_before_a_name_and_hides_no_character() {
        // A name that reads like the line a person is looking for, with a character that would
        // reorder what follows it.
        let granted = Granted {
            token: "rvt_x".into(),
            refresh: None,
            lasts: None,
            account: None,
            email: Some("mallory@evil.test".into()),
            organization: Some("Mallory Org".into()),
            name: Some(plain(
                "Ada Lovelace (ada@example.com), for Analytical Engines\u{202e}\u{200b}",
            )),
            email_whole: None,
            organization_whole: None,
            organization_id: None,
        };
        let mut shown = Vec::new();
        assert!(!ask_to_keep(&granted, &mut "n\n".as_bytes(), &mut shown));
        let shown = String::from_utf8(shown).unwrap();
        assert!(
            shown.find("address       mallory@evil.test").unwrap() < shown.find("name  ").unwrap()
                && shown.find("organization  Mallory Org").unwrap() < shown.find("name  ").unwrap(),
            "{shown}"
        );
        assert!(
            shown.contains(
                "name          \u{201c}Ada Lovelace (ada@example.com), for Analytical Engines\u{201d}"
            ),
            "{shown}"
        );
        assert!(!shown.contains(['\u{202e}', '\u{200b}']), "{shown:?}");
        // Only a yes keeps it.
        assert!(ask_to_keep(
            &granted,
            &mut "Yes\n".as_bytes(),
            &mut Vec::new()
        ));
        assert!(!ask_to_keep(&granted, &mut "".as_bytes(), &mut Vec::new()));
    }

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
