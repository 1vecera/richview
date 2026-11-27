//! The command line's half of signing in through the browser on this computer (RFC 8252, with
//! PKCE as RFC 7636 has it). The command line listens on a loopback port the system picks, sends
//! its person to the server's sign-in page with a challenge only it can answer, and waits. The
//! person signs in the way they always do and clicks Allow; the browser comes back to this port
//! with a one-time code; the command line trades the code and its verifier for the sign-in and
//! tells the browser it worked.
//!
//! The listener is the only thing here that anyone but the person can reach, so it is narrow. It
//! binds `127.0.0.1` alone, on a port nobody chose in advance, and answers two paths. It takes a
//! code only with this run's `state`, so a page that merely guesses the port can do nothing, and
//! only for a `Host` of this port, so a name that points at it cannot. It takes one answer, and
//! it is gone with the first or after ten minutes. The code it receives is useless without the
//! verifier, which has not left this process before the trade. Its answers are never kept by the
//! browser and carry no code: the browser is sent on to a page whose address has none. Nor do they
//! say whose sign-in it is: that is shown at the terminal, which asks to confirm it before
//! anything is kept, because anything on this computer that has seen the address this run opened
//! can answer it with a sign-in of its own.
use crate::{
    access::{CliAuthRequest, cli_challenge},
    ops::Failure,
    sign_in::{self, Granted},
};
use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use base64::Engine as _;
use serde::Deserialize;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, oneshot};

/// How long a person has to sign in.
const LASTS: Duration = Duration::from_secs(600);
/// How long the browser has, after the answer, to ask for the page that says it worked.
const LINGER: Duration = Duration::from_secs(3);
/// The longest a code is taken to be; the server's are 43 characters.
const MAX_CODE_BYTES: usize = 512;

/// What the browser is told when the sign-in is over.
enum Outcome {
    /// The server handed over a sign-in; the terminal decides whether to keep it.
    Signed,
    Cancelled,
    Failed(String),
}

struct Shared {
    url: reqwest::Url,
    state: String,
    verifier: String,
    redirect_uri: String,
    /// The `Host` of a request that is for this listener: `127.0.0.1:<port>`.
    host: String,
    claimed: AtomicBool,
    outcome: Mutex<Option<Outcome>>,
    result: Mutex<Option<oneshot::Sender<Result<Granted, Failure>>>>,
    shown: Notify,
}

fn random(bytes: usize) -> Result<String, Failure> {
    let mut buffer = vec![0u8; bytes];
    getrandom::fill(&mut buffer).map_err(|e| Failure::host(e.to_string()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer))
}

/// Whether two texts are the same, in the time of their length.
fn same(a: &str, b: &str) -> bool {
    let mut diff = a.len() ^ b.len();
    for (at, byte) in a.bytes().enumerate() {
        diff |= usize::from(byte ^ b.as_bytes().get(at).copied().unwrap_or(0));
    }
    diff == 0
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            other => other.to_string(),
        })
        .collect()
}

/// The page that says how it went: one card, in the person's light or dark, with nothing to click.
/// It never says whose sign-in it is, nor which server's: any program on this computer can ask
/// for it, and the terminal that started the sign-in is where that is shown.
fn card(outcome: &Outcome) -> String {
    let (good, heading, message) = match outcome {
        Outcome::Signed => (
            true,
            "Back to your terminal".to_owned(),
            "RichView on this computer has the sign-in. Go back to your terminal: it shows who is signed in and asks you to confirm. You can close this tab.".to_owned(),
        ),
        Outcome::Cancelled => (
            false,
            "Sign-in cancelled".to_owned(),
            "Nothing was signed in. You can close this tab.".to_owned(),
        ),
        Outcome::Failed(why) => (
            false,
            "This sign-in could not be finished".to_owned(),
            format!(
                "{} You can close this tab and run the command again.",
                escape(why)
            ),
        ),
    };
    let (mark, glyph) = if good { ("", "✓") } else { (" no", "!") };
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{heading}</title><style>\
:root{{color-scheme:light dark;--ink:#1c1b19;--paper:#f6f4ef;--ok:#1d6f48;--no:#9a3a2b}}\
@media(prefers-color-scheme:dark){{:root{{--ink:#f0eee8;--paper:#161513;--ok:#4cb482;--no:#e0705d}}}}\
body{{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--paper);color:var(--ink);font:16px/1.55 system-ui,-apple-system,\"Segoe UI\",sans-serif}}\
main{{box-sizing:border-box;width:min(26rem,100%);padding:2rem 1.5rem;text-align:center}}\
.mark{{display:grid;place-items:center;width:3.25rem;height:3.25rem;margin:0 auto 1.25rem;border-radius:50%;border:2px solid var(--ok);color:var(--ok);font-size:1.6rem;font-weight:600}}\
.mark.no{{border-color:var(--no);color:var(--no)}}\
h1{{margin:0 0 .5rem;font-size:1.45rem;line-height:1.25}}p{{margin:0;opacity:.78}}\
</style></head><body><main><div class=\"mark{mark}\" aria-hidden=\"true\">{glyph}</div><h1>{heading}</h1><p>{message}</p></main></body></html>"
    )
}

/// A page for the browser, which nothing may keep, show elsewhere or run.
fn html(status: StatusCode, body: String) -> Response {
    let mut response = (status, body).into_response();
    let headers = response.headers_mut();
    for (name, value) in [
        (header::CONTENT_TYPE, "text/html; charset=utf-8"),
        (header::CACHE_CONTROL, "no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
        ),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

#[derive(Deserialize, Default)]
struct Callback {
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    error: String,
}

/// Whether a request is for this listener's own address: the `Host` of its port, so that a name
/// someone else controls that points here is not answered.
fn for_us(shared: &Shared, headers: &HeaderMap) -> bool {
    headers
        .get(header::HOST)
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| host == shared.host)
}

/// `GET /callback`: the browser comes back. Only the answer to this run's own request counts.
async fn callback(
    State(shared): State<Arc<Shared>>,
    headers: HeaderMap,
    Query(query): Query<Callback>,
) -> Response {
    if !for_us(&shared, &headers) {
        return html(StatusCode::BAD_REQUEST, "Wrong address.".to_owned());
    }
    if !same(&query.state, &shared.state) {
        // Nobody who has not seen the address this run opened can make this say anything more,
        // and the run goes on waiting for the real answer.
        return html(
            StatusCode::BAD_REQUEST,
            card(&Outcome::Failed(
                "This address does not belong to the sign-in this terminal started.".into(),
            )),
        );
    }
    if shared.claimed.swap(true, Ordering::SeqCst) {
        return html(
            StatusCode::GONE,
            card(&Outcome::Failed(
                "This sign-in was already finished.".into(),
            )),
        );
    }
    let (outcome, result) = if !query.error.is_empty() {
        if query.error == "access_denied" {
            (
                Outcome::Cancelled,
                Err(Failure::usage(
                    "The sign-in was cancelled in the browser; nothing was kept.",
                )),
            )
        } else {
            let failure = Failure::usage(
                "The server could not finish the sign-in; nothing was kept. Run the command again.",
            );
            (Outcome::Failed(failure.message().to_owned()), Err(failure))
        }
    } else if query.code.is_empty()
        || query.code.len() > MAX_CODE_BYTES
        || !query.code.chars().all(|c| c.is_ascii_graphic())
    {
        let failure = Failure::usage(
            "The browser came back without a sign-in code; nothing was kept. Run the command again.",
        );
        (Outcome::Failed(failure.message().to_owned()), Err(failure))
    } else {
        let granted = match sign_in::trade(
            &shared.url,
            &query.code,
            &shared.verifier,
            &shared.redirect_uri,
        )
        .await
        {
            Ok(granted) => granted,
            Err(failure) => {
                // Anything that has seen the address this run opened (a process list shows it)
                // can bring a code that is not ours back to this port. That ends nothing: the
                // answer is refused, and the run waits for the person's own.
                shared.claimed.store(false, Ordering::SeqCst);
                eprintln!(
                    "richview: a sign-in the browser brought back was refused ({}); still waiting. Press Ctrl-C to stop and start again.",
                    failure.message().trim_end_matches('.')
                );
                return html(
                    StatusCode::BAD_REQUEST,
                    card(&Outcome::Failed(failure.message().to_owned())),
                );
            }
        };
        // Not kept here: the terminal shows whose it is and keeps it once its person says so.
        (Outcome::Signed, Ok(granted))
    };
    if let Ok(mut told) = shared.outcome.lock() {
        *told = Some(outcome);
    }
    if let Ok(mut waiting) = shared.result.lock()
        && let Some(sender) = waiting.take()
    {
        let _ = sender.send(result);
    }
    // The address the browser stays on has no code in it, so history and screenshots have none.
    let mut response = StatusCode::SEE_OTHER.into_response();
    response
        .headers_mut()
        .insert(header::LOCATION, HeaderValue::from_static("/done"));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// `GET /done`: what the browser is told once the answer is in.
async fn done(State(shared): State<Arc<Shared>>, headers: HeaderMap) -> Response {
    if !for_us(&shared, &headers) {
        return html(StatusCode::BAD_REQUEST, "Wrong address.".to_owned());
    }
    let page = shared
        .outcome
        .lock()
        .ok()
        .and_then(|told| told.as_ref().map(card));
    let Some(page) = page else {
        return StatusCode::NOT_FOUND.into_response();
    };
    shared.shown.notify_one();
    html(StatusCode::OK, page)
}

/// Signs this computer in through its own browser and returns what the server handed over, which
/// the caller keeps only after its person has seen whose it is. `None` when this computer cannot
/// listen, so the caller falls back to the short code. `open` opens the page in the person's
/// browser; the address is printed either way.
pub(crate) async fn sign_in(url: &reqwest::Url, open: bool) -> Result<Option<Granted>, Failure> {
    sign_in_within(url, open, LASTS).await
}

/// `sign_in`, giving up after `lasts`.
async fn sign_in_within(
    url: &reqwest::Url,
    open: bool,
    lasts: Duration,
) -> Result<Option<Granted>, Failure> {
    let Ok(listener) = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await else {
        return Ok(None);
    };
    let Ok(port) = listener.local_addr().map(|address| address.port()) else {
        return Ok(None);
    };
    let verifier = random(32)?;
    let state = random(16)?;
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let Some(request) = CliAuthRequest::new(
        &redirect_uri,
        &state,
        &cli_challenge(&verifier),
        &sign_in::computer_name(),
    ) else {
        return Ok(None);
    };
    let page = url
        .join(&request.path())
        .map_err(|e| Failure::host(e.to_string()))?;
    let (sender, receiver) = oneshot::channel();
    let shared = Arc::new(Shared {
        url: url.clone(),
        state,
        verifier,
        redirect_uri,
        host: format!("127.0.0.1:{port}"),
        claimed: AtomicBool::new(false),
        outcome: Mutex::new(None),
        result: Mutex::new(Some(sender)),
        shown: Notify::new(),
    });
    let app = Router::new()
        .route("/callback", get(callback))
        .route("/done", get(done))
        .fallback(|| async { StatusCode::NOT_FOUND })
        .with_state(shared.clone());
    let (stop, stopped) = oneshot::channel::<()>();
    let listening = tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await;
    });
    let shown_server = url.origin().ascii_serialization();
    if open {
        eprintln!("Opening {shown_server} in your browser to sign in.");
        eprintln!("If it does not open, use this address:");
    } else {
        eprintln!("Open this address in your browser to sign in to {shown_server}:");
    }
    eprintln!("{page}");
    // What the page shows too, so that a request somebody else's program made cannot pass for this.
    eprintln!(
        "Press Allow only if the page shows the code {}.",
        request.check_code()
    );
    if open {
        sign_in::open_browser(page.as_str());
    }
    let answer = tokio::time::timeout(lasts, receiver).await;
    if matches!(answer, Ok(Ok(_))) {
        // The browser is being sent to the page that says it worked; let it ask.
        let _ = tokio::time::timeout(LINGER, shared.shown.notified()).await;
    }
    let _ = stop.send(());
    // A browser that keeps its connection open must not keep the command waiting.
    let _ = tokio::time::timeout(Duration::from_secs(2), listening).await;
    match answer {
        Ok(Ok(result)) => result.map(Some),
        Ok(Err(_)) => Err(Failure::host(
            "The sign-in ended unexpectedly; nothing was kept. Run the command again.",
        )),
        Err(_) => Err(Failure::usage(
            "Nobody finished signing in within 10 minutes; nothing was kept. Run the command again.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn nobody_answering_ends_the_wait_and_frees_the_port() {
        let url = reqwest::Url::parse("http://127.0.0.1:9").unwrap();
        let started = std::time::Instant::now();
        let ended = sign_in_within(&url, false, Duration::from_millis(300)).await;
        let Err(failure) = ended else {
            panic!("nobody signed in, and it still signed in");
        };
        assert!(
            failure
                .message()
                .contains("Nobody finished signing in within 10 minutes"),
            "{}",
            failure.message()
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn two_texts_are_the_same_only_when_every_byte_is() {
        assert!(same("state-0123", "state-0123"));
        for other in ["", "state-012", "state-01234", "State-0123", "state-0124"] {
            assert!(!same("state-0123", other), "{other}");
        }
    }

    #[test]
    fn what_the_page_says_is_text_and_never_markup_and_never_whose_sign_in_it_is() {
        // Anything on this computer can ask the page for its words: it names no account, no
        // organization and no server.
        let page = card(&Outcome::Signed);
        assert!(page.contains("Back to your terminal"), "{page}");
        for word in [
            "signed in to",
            "Signed in to",
            "Ada",
            "Acme",
            "example.test",
        ] {
            assert!(!page.contains(word), "{word}: {page}");
        }
        let failed = card(&Outcome::Failed("<script>1</script>".into()));
        assert!(failed.contains("&lt;script&gt;1&lt;/script&gt;") && !failed.contains("<script>"));
        assert!(card(&Outcome::Cancelled).contains("Sign-in cancelled"));
    }

    #[test]
    fn a_verifier_and_a_state_are_as_long_and_as_plain_as_the_server_asks() {
        let verifier = random(32).unwrap();
        let state = random(16).unwrap();
        assert_eq!((verifier.len(), state.len()), (43, 22));
        assert!(
            CliAuthRequest::new(
                "http://127.0.0.1:49152/callback",
                &state,
                &cli_challenge(&verifier),
                "ada-laptop"
            )
            .is_some()
        );
        assert_ne!(random(32).unwrap(), verifier);
    }
}
