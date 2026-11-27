//! `richview login`, `richview logout` and the sign-in a computer keeps: the token is never shown,
//! is kept privately and goes only to the server it was kept for (ARC-23, FEA-SF-12, INT-79).
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::{Arc, Mutex},
};

/// The one token the fake server knows a person for.
const TOKEN: &str = "rvt_kept-Sign1n-token";
/// A token the fake server knows a person for, and says no organization of.
const UNBOUND: &str = "rvt_unbound-Sign1n-token";
/// The refresh token the fake server renews a sign-in for, and what it renews it to.
const REFRESH: &str = "rvr_still-good-refresh";
const RENEWED: &str = "rvt_renewed-access-token";
const RENEWED_REFRESH: &str = "rvr_renewed-refresh-token";
/// A token this fake server refuses, though the sign-in that holds it looks good for hours: the
/// computer's clock or the server's was wrong, or the server ended it early.
const REFUSED: &str = "rvt_refused-by-the-server";
const SOURCE: &str = "---\ntitle: T\ndescription: D\nversion: 1.0.0\nslug: t\n---\n\n# T\n";

/// One request a fake server received.
#[derive(Clone, Debug)]
struct Seen {
    method: String,
    path: String,
    bearer: Option<String>,
    /// The JSON the request carried, `null` when it carried none.
    body: Value,
}

/// A stand-in RichView server on loopback that records what it is sent. It answers the session
/// question for `TOKEN` and models a server that still requires explicit permission to create.
struct Fake {
    base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Fake {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let record = seen.clone();
        std::thread::spawn(move || {
            let mut published = std::collections::BTreeSet::new();
            for stream in listener.incoming().flatten() {
                answer(stream, &record, &mut published);
            }
        });
        Self { base, seen }
    }
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
    fn bearers(&self, method: &str, path: &str) -> Vec<Option<String>> {
        self.seen()
            .into_iter()
            .filter(|s| s.method == method && s.path == path)
            .map(|s| s.bearer)
            .collect()
    }
}

fn answer(
    mut stream: std::net::TcpStream,
    record: &Mutex<Vec<Seen>>,
    published: &mut std::collections::BTreeSet<String>,
) {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
        }
    };
    let head = String::from_utf8_lossy(&bytes[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut first = lines.next().unwrap_or_default().split(' ');
    let (method, path) = (
        first.next().unwrap_or_default().to_owned(),
        first.next().unwrap_or_default().to_owned(),
    );
    let header = |name: &str| {
        head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    };
    let length: usize = header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    while bytes.len() < head_end + length {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
        }
    }
    let bearer = header("authorization").and_then(|v| v.strip_prefix("Bearer ").map(str::to_owned));
    record.lock().unwrap().push(Seen {
        method: method.clone(),
        path: path.clone(),
        bearer: bearer.clone(),
        body: serde_json::from_slice(&bytes[head_end..]).unwrap_or_default(),
    });
    let (status, body) = match (method.as_str(), path.as_str()) {
        ("GET", "/api/session") if bearer.as_deref() == Some(TOKEN) => (
            "200 OK",
            json!({"user": {"id": "u1", "name": "Ada", "email": "ada@example.test", "role": "author"}, "organization": {"id": "aaaa0001", "name": "Ada Analytics"}, "csrf": null}),
        ),
        // A server that does not say which organization a token is for.
        ("GET", "/api/session") if bearer.as_deref() == Some(UNBOUND) => (
            "200 OK",
            json!({"user": {"id": "u1", "name": "Ada", "email": "ada@example.test", "role": "author"}, "csrf": null}),
        ),
        ("GET", "/api/session") => ("200 OK", json!({"user": null, "csrf": null})),
        ("POST", "/api/cli/oauth/token") => {
            let sent: Value = serde_json::from_slice(&bytes[head_end..]).unwrap_or_default();
            if sent["refreshToken"] == REFRESH {
                (
                    "200 OK",
                    json!({"status": "allowed", "token": RENEWED, "refreshToken": RENEWED_REFRESH, "expiresIn": 3600, "account": {"name": "Ada", "email": "ada@example.test"}, "organization": null}),
                )
            } else {
                (
                    "401 Unauthorized",
                    json!({"status": "ended", "error": "This computer's sign-in has ended. Sign in again."}),
                )
            }
        }
        (_, _) if bearer.is_none() || bearer.as_deref() == Some(REFUSED) => {
            ("401 Unauthorized", json!({"error": "Unauthorized"}))
        }
        ("POST", "/api/reports") => {
            let sent: Value = serde_json::from_slice(&bytes[head_end..]).unwrap_or_default();
            let slug = sent["slug"].as_str().unwrap();
            if !published.contains(slug) && sent["new"] != true {
                (
                    "409 Conflict",
                    json!({"diagnostics":[{"code":"publish.new-slug","level":"error","message":"Expected a stored address","path":"slug"}]}),
                )
            } else {
                published.insert(slug.to_owned());
                (
                    "200 OK",
                    json!({"created": true, "slug": sent["slug"], "version": "1.0.0", "visibility": "private", "permalink": "/r/t/v/1.0.0", "diagnostics": []}),
                )
            }
        }
        ("DELETE", "/api/reports/t/latest") => ("200 OK", json!({"diagnostics": []})),
        _ => ("404 Not Found", json!({"error": "Not found"})),
    };
    let body = body.to_string();
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
}

/// A report folder and a private configuration folder that no other test or person shares.
struct Desk {
    _root: tempfile::TempDir,
    config: PathBuf,
    report: PathBuf,
}

impl Desk {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let report = root.path().join("report/report.rmd");
        std::fs::create_dir_all(report.parent().unwrap()).unwrap();
        std::fs::write(&report, SOURCE).unwrap();
        Self {
            config: root.path().join("config"),
            report,
            _root: root,
        }
    }
    /// `richview` with this desk's configuration and no server or token in its environment.
    fn richview(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_richview"));
        command
            .env("XDG_CONFIG_HOME", &self.config)
            // A test never opens a browser on the machine it runs on.
            .env("RICHVIEW_NO_BROWSER", "1")
            .env_remove("RICHVIEW_SERVER")
            .env_remove("RICHVIEW_TOKEN")
            .stdin(Stdio::null());
        command
    }
    fn credentials(&self) -> PathBuf {
        self.config.join("richview/credentials.json")
    }
    /// Signs in to `server` by piping `token`, as a script would.
    fn login(&self, server: &str, token: &str) -> Output {
        self.login_with(server, token, &[])
    }
    /// The same with more arguments to `richview login`.
    fn login_with(&self, server: &str, token: &str, more: &[&str]) -> Output {
        let mut child = self
            .richview()
            .args(["login", "--server", server, "--with-token"])
            .args(more)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{token}\n").as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn publishing_the_same_file_to_a_changed_address_requires_intent_to_create_again() {
    let (desk, server) = (Desk::new(), Fake::start());
    let publish = |slug: &str, extra: &[&str]| {
        desk.richview()
            .arg("publish")
            .arg(&desk.report)
            .args(["--server", &server.base, "--token", TOKEN, "--slug", slug])
            .args(extra)
            .output()
            .unwrap()
    };
    let first = publish("quarterly", &[]);
    assert!(first.status.success(), "{}", text(&first));
    let typo = publish("quaterly", &[]);
    assert_eq!(typo.status.code(), Some(1), "{}", text(&typo));
    assert!(text(&typo).contains("publish.new-slug"));
    let mcp_typo = mcp_publish(
        desk.richview()
            .env("RICHVIEW_SERVER", &server.base)
            .env("RICHVIEW_TOKEN", TOKEN),
        json!({"inputPath": desk.report, "slug": "quaterly"}),
    );
    assert_eq!(mcp_typo["structuredContent"]["ok"], false, "{mcp_typo}");
    assert_eq!(
        mcp_typo["structuredContent"]["diagnostics"][0]["code"],
        "publish.new-slug"
    );
    assert!(publish("quarterly", &[]).status.success());
    assert!(publish("annual", &["--new"]).status.success());
    let blocked_config = desk.config.with_file_name("read-only-config");
    std::fs::write(&blocked_config, "no configuration directory").unwrap();
    let scripted = desk
        .richview()
        .env("XDG_CONFIG_HOME", blocked_config)
        .arg("publish")
        .arg(&desk.report)
        .args(["--server", &server.base, "--token", TOKEN, "--new"])
        .output()
        .unwrap();
    assert!(scripted.status.success(), "{}", text(&scripted));
}

#[test]
fn a_deck_is_published_by_the_command_as_a_report_is() {
    // FEA-RP-71: a `.rslide` goes through `richview publish` with the same checks, folder and request
    // a report has; what the server stores is the deck's own text.
    let (desk, server) = (Desk::new(), Fake::start());
    let folder = desk.report.parent().unwrap().parent().unwrap().join("deck");
    std::fs::create_dir_all(&folder).unwrap();
    let deck = "format: rslide/1\ntitle: Deck\ndescription: A deck\nversion: 1.0.0\n---\ntitle: Cover\n---\nmetric:\n  value: 3\n  label: Things\n";
    std::fs::write(folder.join("deck.rslide"), deck).unwrap();
    let published = desk
        .richview()
        .arg("publish")
        .arg(folder.join("deck.rslide"))
        .args(["--server", &server.base, "--token", TOKEN, "--new"])
        .output()
        .unwrap();
    assert!(published.status.success(), "{}", text(&published));
    let sent = server
        .seen()
        .into_iter()
        .find(|s| s.method == "POST" && s.path == "/api/reports")
        .expect("the publication was sent");
    assert_eq!(sent.body["source"], deck);
    assert_eq!(sent.body["slug"], "deck");
    assert_eq!(sent.body["reportFolder"]["source"], "deck.rslide");
    // The folder also publishes by its name, as a report's folder does.
    let by_folder = desk
        .richview()
        .arg("publish")
        .arg(&folder)
        .args(["--server", &server.base, "--token", TOKEN])
        .output()
        .unwrap();
    assert!(by_folder.status.success(), "{}", text(&by_folder));
    // A deck with a mistake is refused before anything is sent.
    std::fs::write(
        folder.join("deck.rslide"),
        "format: rslide/1\ntitle: Deck\ndescription: A\nversion: 1.0.0\n---\ntitle: One\nleft: a\nbottom: b\n",
    )
    .unwrap();
    let sends = server.seen().len();
    let refused = desk
        .richview()
        .arg("publish")
        .arg(folder.join("deck.rslide"))
        .args(["--server", &server.base, "--token", TOKEN])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(text(&refused).contains("deck.layout"), "{}", text(&refused));
    assert!(
        !server.seen()[sends..]
            .iter()
            .any(|s| s.method == "POST" && s.path == "/api/reports"),
        "a refused deck is not sent"
    );
}

/// Nothing a command printed, and no address it asked for, carries a token.
fn assert_no_token_shown(output: &Output, servers: &[&Fake], tokens: &[&str]) {
    let printed = text(output);
    for token in tokens {
        assert!(!printed.contains(token), "a token was printed: {printed}");
        for server in servers {
            for seen in server.seen() {
                assert!(!seen.path.contains(token), "a token in a URL: {seen:?}");
            }
        }
    }
}

#[test]
fn a_kept_sign_in_publishes_without_a_token_and_goes_to_its_own_server_only() {
    let (desk, home, other) = (Desk::new(), Fake::start(), Fake::start());
    let signed_in = desk.login(&home.base, TOKEN);
    assert_eq!(signed_in.status.code(), Some(0), "{}", text(&signed_in));
    assert_eq!(
        String::from_utf8_lossy(&signed_in.stdout).trim(),
        format!(
            "Signed in to {} as Ada (ada@example.test), for Ada Analytics. This computer stays signed in; `richview logout` ends it.",
            home.base
        )
    );
    assert_no_token_shown(&signed_in, &[&home, &other], &[TOKEN]);

    // The token is kept where only its owner reads it, and the server is now the destination.
    let kept: Value = serde_json::from_slice(&std::fs::read(desk.credentials()).unwrap()).unwrap();
    assert_eq!(kept["servers"][&home.base]["token"], TOKEN);
    #[cfg(unix)]
    {
        use std::{os::unix::fs::PermissionsExt, path::Path};
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&desk.credentials()), 0o600);
        assert_eq!(mode(desk.credentials().parent().unwrap()), 0o700);
    }
    assert_eq!(
        std::fs::read_to_string(desk.config.join("richview/publish-server.json")).unwrap(),
        format!("\"{}\"", home.base)
    );

    // Publishing names no server and no token, and the kept token goes to the kept server.
    let published = desk
        .richview()
        .arg("publish")
        .arg(&desk.report)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert_eq!(published.status.code(), Some(0), "{}", text(&published));
    let result: Value = serde_json::from_slice(&published.stdout).unwrap();
    assert_eq!(
        (&result["ok"], &result["slug"]),
        (&json!(true), &json!("t"))
    );
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [Some(TOKEN.to_owned())]
    );
    assert_no_token_shown(&published, &[&home, &other], &[TOKEN]);

    // An owner action finds the server and the token the same way.
    let unpinned = desk
        .richview()
        .args(["report", "unpin", "t"])
        .output()
        .unwrap();
    assert_eq!(unpinned.status.code(), Some(0), "{}", text(&unpinned));
    assert_eq!(
        home.bearers("DELETE", "/api/reports/t/latest"),
        [Some(TOKEN.to_owned())]
    );

    // Another server gets nothing: not the token, not even a request.
    for format in ["json", "pretty"] {
        let elsewhere = desk
            .richview()
            .arg("publish")
            .arg(&desk.report)
            .args(["--server", &other.base, "--new", "--format", format])
            .output()
            .unwrap();
        assert_eq!(elsewhere.status.code(), Some(2), "{}", text(&elsewhere));
        assert!(
            text(&elsewhere).contains(&format!("not signed in to {}", other.base)),
            "{}",
            text(&elsewhere)
        );
        assert!(
            text(&elsewhere).contains("richview login"),
            "{}",
            text(&elsewhere)
        );
        assert_no_token_shown(&elsewhere, &[&home, &other], &[TOKEN]);
    }
    assert!(other.seen().is_empty(), "{:?}", other.seen());

    // Signing out forgets the token here, and says that the server, which offers no way to
    // revoke it from the command line, still knows it.
    let signed_out = desk.richview().arg("logout").output().unwrap();
    assert_eq!(signed_out.status.code(), Some(0), "{}", text(&signed_out));
    assert!(
        text(&signed_out).contains(&format!("Signed out of {} on this computer.", home.base))
            && text(&signed_out)
                .contains("the token still works until it is revoked on the server"),
        "{}",
        text(&signed_out)
    );
    assert!(!desk.credentials().exists());
    let again = desk.richview().arg("logout").output().unwrap();
    assert!(
        text(&again).contains("keeps no sign-in"),
        "{}",
        text(&again)
    );
    let refused = desk
        .richview()
        .arg("publish")
        .arg(&desk.report)
        .args(["--new", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2), "{}", text(&refused));
    assert_eq!(home.bearers("POST", "/api/reports").len(), 1);
}

/// Keeps a sign-in that renews, as signing in through the browser does: `ends` seconds from the
/// epoch.
#[cfg(unix)]
fn keep_renewable(desk: &Desk, server: &str, token: &str, refresh: &str, ends: u64) {
    use std::os::unix::fs::PermissionsExt;
    let file = desk.credentials();
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::set_permissions(
        file.parent().unwrap(),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::write(
        &file,
        json!({"servers": {server: {"token": token, "refresh": refresh, "expires": ends, "kept": "2026-11-25T10:00:00Z"}}})
            .to_string(),
    )
    .unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(unix)]
#[test]
fn a_sign_in_that_is_over_is_renewed_once_before_it_is_used_and_one_that_ended_asks_again() {
    let (desk, home) = (Desk::new(), Fake::start());
    keep_renewable(&desk, &home.base, "rvt_stale-access", REFRESH, 1);
    // Two commands at the same moment find the same sign-in over; one renews it, the other waits
    // for that and uses the result.
    let second = desk
        .report
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("second/report.rmd");
    std::fs::create_dir_all(second.parent().unwrap()).unwrap();
    std::fs::write(&second, SOURCE).unwrap();
    let publish = |report: &PathBuf| {
        desk.richview()
            .arg("publish")
            .arg(report)
            .args(["--server", &home.base, "--new"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let (a, b) = (publish(&desk.report), publish(&second));
    for output in [a.wait_with_output().unwrap(), b.wait_with_output().unwrap()] {
        assert_eq!(output.status.code(), Some(0), "{}", text(&output));
        assert_no_token_shown(
            &output,
            &[&home],
            &[REFRESH, RENEWED, RENEWED_REFRESH, "rvt_stale-access"],
        );
    }
    assert_eq!(home.bearers("POST", "/api/cli/oauth/token").len(), 1);
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [Some(RENEWED.to_owned()), Some(RENEWED.to_owned())]
    );
    // What is kept is the renewed pair, ending in about an hour, in a file only its owner reads.
    let kept: Value = serde_json::from_slice(&std::fs::read(desk.credentials()).unwrap()).unwrap();
    let entry = &kept["servers"][&home.base];
    assert_eq!(
        (&entry["token"], &entry["refresh"]),
        (&json!(RENEWED), &json!(RENEWED_REFRESH))
    );
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!((now + 3500..=now + 3700).contains(&entry["expires"].as_u64().unwrap()));
    // A later command finds it good and asks nothing.
    let third = desk
        .richview()
        .arg("publish")
        .arg(&desk.report)
        .args(["--server", &home.base, "--new"])
        .output()
        .unwrap();
    assert_eq!(third.status.code(), Some(0), "{}", text(&third));
    assert_eq!(home.bearers("POST", "/api/cli/oauth/token").len(), 1);

    // A sign-in the server says is over is forgotten, and a command with no terminal says so and
    // names both ways to sign in again.
    let ended = Desk::new();
    keep_renewable(&ended, &home.base, "rvt_stale-access", "rvr_signed-out", 1);
    let output = ended
        .richview()
        .arg("publish")
        .arg(&ended.report)
        .args(["--server", &home.base, "--new"])
        .output()
        .unwrap();
    let said = text(&output);
    assert_eq!(output.status.code(), Some(2), "{said}");
    assert!(
        said.contains(&format!(
            "this computer's sign-in to {} has ended",
            home.base
        )) && said.contains("richview login --server")
            && !said.contains("rvr_signed-out"),
        "{said}"
    );
    assert!(!ended.credentials().exists(), "the ended sign-in was kept");
}

#[cfg(unix)]
#[test]
fn a_token_the_server_refuses_is_renewed_once_and_the_request_goes_again() {
    let (desk, home) = (Desk::new(), Fake::start());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // The kept sign-in says it has most of an hour left; the server says otherwise.
    keep_renewable(&desk, &home.base, REFUSED, REFRESH, now + 3000);
    let output = desk
        .richview()
        .arg("publish")
        .arg(&desk.report)
        .args(["--server", &home.base, "--new"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    assert_no_token_shown(
        &output,
        &[&home],
        &[REFRESH, RENEWED, RENEWED_REFRESH, REFUSED],
    );
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [Some(REFUSED.to_owned()), Some(RENEWED.to_owned())]
    );
    assert_eq!(home.bearers("POST", "/api/cli/oauth/token").len(), 1);
    let kept: Value = serde_json::from_slice(&std::fs::read(desk.credentials()).unwrap()).unwrap();
    assert_eq!(kept["servers"][&home.base]["token"], RENEWED);

    // A token the person gave is not theirs to swap for the kept sign-in: it is refused as it is.
    let (other, away) = (Desk::new(), Fake::start());
    keep_renewable(
        &other,
        &away.base,
        "rvt_another-computers-own",
        REFRESH,
        now + 3000,
    );
    let given = other
        .richview()
        .arg("publish")
        .arg(&other.report)
        .args(["--server", &away.base, "--new", "--token", REFUSED])
        .output()
        .unwrap();
    assert_ne!(given.status.code(), Some(0), "{}", text(&given));
    assert!(away.bearers("POST", "/api/cli/oauth/token").is_empty());
    assert_eq!(
        away.bearers("POST", "/api/reports"),
        [Some(REFUSED.to_owned())]
    );
}

#[test]
fn a_token_the_server_does_not_know_is_not_kept() {
    let (desk, home) = (Desk::new(), Fake::start());
    let wrong = "rvt_not-a-token-of-this-server";
    let refused = desk.login(&home.base, wrong);
    assert_eq!(refused.status.code(), Some(2), "{}", text(&refused));
    assert!(
        text(&refused).contains("did not accept this token; nothing was kept"),
        "{}",
        text(&refused)
    );
    assert_no_token_shown(&refused, &[&home], &[wrong]);
    assert!(!desk.credentials().exists());
    assert!(!desk.config.join("richview/publish-server.json").exists());
    // The one request was the question whose token this is; nothing was published with it.
    let paths: Vec<_> = home.seen().into_iter().map(|s| s.path).collect();
    assert_eq!(paths, ["/api/session"]);

    // A line that is not a token is refused before any request, without being repeated.
    let pasted = desk.login(&home.base, "two words");
    assert_eq!(pasted.status.code(), Some(2), "{}", text(&pasted));
    assert!(!text(&pasted).contains("two words"), "{}", text(&pasted));
    let empty = desk.login(&home.base, "");
    assert_eq!(empty.status.code(), Some(2), "{}", text(&empty));
    assert_eq!(home.seen().len(), 1);
}

#[test]
fn a_token_for_another_account_than_the_one_expected_is_not_kept() {
    let (desk, home) = (Desk::new(), Fake::start());
    let refused = desk.login_with(
        &home.base,
        TOKEN,
        &["--expect-account", "grace@example.test"],
    );
    assert_eq!(refused.status.code(), Some(2), "{}", text(&refused));
    assert!(
        text(&refused)
            .contains("This token is for ada@example.test, and not for grace@example.test"),
        "{}",
        text(&refused)
    );
    assert_no_token_shown(&refused, &[&home], &[TOKEN]);
    assert!(!desk.credentials().exists());
    // An address that only begins like the expected one is another address.
    let longer = desk.login_with(
        &home.base,
        TOKEN,
        &["--expect-account", "ada@example.test.evil"],
    );
    assert_eq!(longer.status.code(), Some(2), "{}", text(&longer));
    assert!(!desk.credentials().exists());
    // A token is bound to an organization, and the server says which: another one, an id given as
    // a name or a name given as an id, and a server that says none, are all refused.
    for flags in [
        ["--expect-organization", "Grace Industries"],
        ["--expect-organization", "aaaa0001"],
        ["--expect-organization-id", "Ada Analytics"],
        ["--expect-organization-id", "bbbb0002"],
    ] {
        let org = desk.login_with(&home.base, TOKEN, &flags);
        assert_eq!(org.status.code(), Some(2), "{flags:?}: {}", text(&org));
        assert!(
            text(&org).contains("This token is for Ada Analytics, and not for the organization"),
            "{flags:?}: {}",
            text(&org)
        );
        assert!(!desk.credentials().exists(), "{flags:?}");
    }
    let unbound = desk.login_with(
        &home.base,
        UNBOUND,
        &["--expect-organization", "Ada Analytics"],
    );
    assert_eq!(unbound.status.code(), Some(2), "{}", text(&unbound));
    assert!(
        text(&unbound).contains("an organization the server did not name"),
        "{}",
        text(&unbound)
    );
    assert!(!desk.credentials().exists());
    // The right organization, by its name or by its id, is kept.
    for flags in [
        ["--expect-organization", "Ada Analytics"],
        ["--expect-organization-id", "aaaa0001"],
    ] {
        let kept = desk.login_with(&home.base, TOKEN, &flags);
        assert_eq!(kept.status.code(), Some(0), "{flags:?}: {}", text(&kept));
        assert!(desk.credentials().exists(), "{flags:?}");
        let _ = std::fs::remove_file(desk.credentials());
    }
    // The right account, in any case, is kept.
    let kept = desk.login_with(&home.base, TOKEN, &["--expect-account", "Ada@Example.Test"]);
    assert_eq!(kept.status.code(), Some(0), "{}", text(&kept));
    assert!(desk.credentials().exists());
}

#[test]
fn a_token_given_by_flag_or_environment_wins_over_the_kept_one() {
    let (desk, home) = (Desk::new(), Fake::start());
    assert_eq!(desk.login(&home.base, TOKEN).status.code(), Some(0));
    let publish = |command: &mut Command, extra: &[&str]| {
        let output = command
            .arg("publish")
            .arg(&desk.report)
            .args(["--new", "--format", "json"])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0), "{}", text(&output));
    };
    publish(
        desk.richview()
            .env("RICHVIEW_SERVER", &home.base)
            .env("RICHVIEW_TOKEN", "from-the-environment"),
        &[],
    );
    publish(&mut desk.richview(), &["--token", "from-the-flag"]);
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [
            Some("from-the-environment".to_owned()),
            Some("from-the-flag".to_owned())
        ]
    );
    // RICHVIEW_TOKEN without RICHVIEW_SERVER belongs to no server (FEA-SF-12): it is sent nowhere,
    // and the sign-in this computer keeps for the server is used instead, saying so in one line.
    let stray = |desk: &Desk| {
        desk.richview()
            .env("RICHVIEW_TOKEN", "from-the-environment")
            .arg("publish")
            .arg(&desk.report)
            .args(["--server", &home.base, "--new", "--format", "json"])
            .output()
            .unwrap()
    };
    let kept = stray(&desk);
    assert_eq!(kept.status.code(), Some(0), "{}", text(&kept));
    assert_eq!(
        String::from_utf8_lossy(&kept.stderr).trim(),
        format!(
            "richview: RICHVIEW_TOKEN was not used because RICHVIEW_SERVER is not set; this computer's sign-in to {} was.",
            home.base
        )
    );
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [
            Some("from-the-environment".to_owned()),
            Some("from-the-flag".to_owned()),
            Some(TOKEN.to_owned())
        ]
    );
    // Without a kept sign-in for that server, the stray token is refused and still sent nowhere.
    let unbound = stray(&Desk::new());
    assert_eq!(unbound.status.code(), Some(2), "{}", text(&unbound));
    assert!(
        text(&unbound).contains("only sent to RICHVIEW_SERVER"),
        "{}",
        text(&unbound)
    );
    assert_eq!(home.bearers("POST", "/api/reports").len(), 3);
}

#[test]
fn without_a_terminal_nobody_is_asked_and_both_ways_are_named() {
    let (desk, home) = (Desk::new(), Fake::start());
    // `login` has nobody to ask and no piped token.
    for format in ["pretty", "json"] {
        let login = desk
            .richview()
            .args(["login", "--server", &home.base, "--format", format])
            .output()
            .unwrap();
        assert_eq!(login.status.code(), Some(2), "{}", text(&login));
        assert!(text(&login).contains("--with-token"), "{}", text(&login));
    }
    // `publish` and an owner action never wait for an answer: exit 2, naming both ways.
    let publish = desk
        .richview()
        .arg("publish")
        .arg(&desk.report)
        .args(["--server", &home.base, "--new"])
        .output()
        .unwrap();
    let pin = desk
        .richview()
        .args(["report", "pin", "t", "1.0.0", "--server", &home.base])
        .output()
        .unwrap();
    for output in [&publish, &pin] {
        assert_eq!(output.status.code(), Some(2), "{}", text(output));
        let said = text(output);
        assert!(
            said.contains("richview login --server") && said.contains("RICHVIEW_TOKEN"),
            "{said}"
        );
    }
    // No server named and none chosen: `logout` and an owner action say so.
    let logout = desk.richview().arg("logout").output().unwrap();
    assert_eq!(logout.status.code(), Some(2), "{}", text(&logout));
    // All the server saw was `login` asking how it signs a command line in and whether it offers a
    // sign-in through the browser: no token and no publication.
    assert!(
        home.seen().iter().all(|seen| seen.bearer.is_none()
            && matches!(
                seen.path.as_str(),
                "/api/cli/oauth/metadata" | "/api/cli/sign-in"
            )),
        "{:?}",
        home.seen()
    );
}

/// One MCP tool call against a fresh `richview mcp`; returns the call's result.
fn mcp_publish(command: &mut Command, arguments: Value) -> Value {
    let mut child = command
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let call = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": "richview_publish", "arguments": arguments}});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{call}\n").as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|reply| reply["id"] == 1)
        .map(|reply| reply["result"].clone())
        .unwrap_or_else(|| panic!("no reply: {}", text(&output)))
}

#[test]
fn the_mcp_publish_tool_uses_the_kept_sign_in_and_cannot_send_it_elsewhere() {
    let (desk, home, other) = (Desk::new(), Fake::start(), Fake::start());
    let input = json!(desk.report.to_str().unwrap());

    // Nobody signed in and the host names nothing: the tool says what the user does once.
    let unconfigured = mcp_publish(&mut desk.richview(), json!({"inputPath": input}));
    assert_eq!(unconfigured["isError"], true, "{unconfigured}");
    assert!(
        unconfigured["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("richview login"),
        "{unconfigured}"
    );

    assert_eq!(desk.login(&home.base, TOKEN).status.code(), Some(0));
    let published = mcp_publish(&mut desk.richview(), json!({"inputPath": input}));
    assert_eq!(published["isError"], false, "{published}");
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [Some(TOKEN.to_owned())]
    );
    assert!(!published.to_string().contains(TOKEN), "{published}");

    // A tool argument cannot move the publication, and with it the token, to another server.
    let moved = mcp_publish(
        &mut desk.richview(),
        json!({"inputPath": input, "new": true, "server": other.base}),
    );
    assert_eq!(moved["isError"], true, "{moved}");
    // A server the host names has no kept token unless the user signed in there.
    let named = mcp_publish(
        desk.richview().env("RICHVIEW_SERVER", &other.base),
        json!({"inputPath": input}),
    );
    assert_eq!(named["isError"], true, "{named}");
    assert!(
        named["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(&format!("No token for {}", other.base)),
        "{named}"
    );
    assert!(other.seen().is_empty(), "{:?}", other.seen());

    // The host's own server and token still win; a token without a server belongs to none, so
    // the kept sign-in is used and the stray token is sent nowhere.
    let from_host = mcp_publish(
        desk.richview()
            .env("RICHVIEW_SERVER", &home.base)
            .env("RICHVIEW_TOKEN", "from-the-host"),
        json!({"inputPath": input}),
    );
    assert_eq!(from_host["isError"], false, "{from_host}");
    let stray = mcp_publish(
        desk.richview().env("RICHVIEW_TOKEN", "stray-token"),
        json!({"inputPath": input}),
    );
    assert_eq!(stray["isError"], false, "{stray}");
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [
            Some(TOKEN.to_owned()),
            Some("from-the-host".to_owned()),
            Some(TOKEN.to_owned())
        ]
    );
}

/// Runs a command on a real pseudo-terminal: waits until it has printed `prompt`, types `typed`
/// and returns everything the terminal showed, with the exit code.
#[cfg(unix)]
fn on_a_terminal(mut command: Command, prompt: &str, typed: &str) -> (String, Option<i32>) {
    use std::os::fd::{FromRawFd, OwnedFd};
    let (mut master, mut slave) = (0, 0);
    // SAFETY: `openpty` fills the two descriptors it is given and reports failure by its result.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert_eq!(opened, 0, "no pseudo-terminal");
    // SAFETY: both descriptors were just opened and are owned by nobody else.
    let (mut master, slave) = unsafe {
        (
            std::fs::File::from_raw_fd(master),
            OwnedFd::from_raw_fd(slave),
        )
    };
    let mut child = command
        .stdin(slave.try_clone().unwrap())
        .stdout(slave.try_clone().unwrap())
        .stderr(slave)
        .spawn()
        .unwrap();
    // The command keeps copies of the terminal's far end until it is dropped; without them the
    // read below ends when the child exits.
    drop(command);
    let mut shown = String::new();
    let mut chunk = [0u8; 1024];
    let mut typed = Some(typed);
    loop {
        match master.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => shown.push_str(&String::from_utf8_lossy(&chunk[..n])),
        }
        if typed.is_some() && shown.contains(prompt) {
            // The prompt is printed before echo is switched off; give that a moment.
            std::thread::sleep(std::time::Duration::from_millis(300));
            master
                .write_all(format!("{}\n", typed.take().unwrap()).as_bytes())
                .unwrap();
        }
    }
    (shown, child.wait().unwrap().code())
}

#[cfg(unix)]
#[test]
fn a_token_typed_at_a_terminal_is_never_shown() {
    let (desk, home) = (Desk::new(), Fake::start());

    // `publish` on a terminal asks its person to sign in, keeps the token and publishes.
    let mut publish = desk.richview();
    publish
        .arg("publish")
        .arg(&desk.report)
        .args(["--server", &home.base, "--new"]);
    let (shown, code) = on_a_terminal(publish, "CLI token: ", TOKEN);
    assert_eq!(code, Some(0), "{shown}");
    assert!(
        shown.contains(&format!("This computer is not signed in to {}.", home.base))
            && shown.contains("it is not shown")
            && shown.contains("Signed in to")
            && shown.contains("Published"),
        "{shown}"
    );
    assert!(!shown.contains(TOKEN), "the typed token was shown: {shown}");
    assert_eq!(
        home.bearers("POST", "/api/reports"),
        [Some(TOKEN.to_owned())]
    );
    let kept: Value = serde_json::from_slice(&std::fs::read(desk.credentials()).unwrap()).unwrap();
    assert_eq!(kept["servers"][&home.base]["token"], TOKEN);

    // Enter alone cancels: nothing is kept and nothing is sent.
    let other = Desk::new();
    let mut cancelled = other.richview();
    cancelled
        .arg("publish")
        .arg(&other.report)
        .args(["--server", &home.base, "--new"]);
    let (shown, code) = on_a_terminal(cancelled, "CLI token: ", "");
    assert_eq!(code, Some(2), "{shown}");
    assert!(shown.contains("Cancelled; nothing was sent"), "{shown}");
    assert!(!other.credentials().exists());
    assert_eq!(home.bearers("POST", "/api/reports").len(), 1);

    // `richview login` asks the same way.
    let third = Desk::new();
    let mut login = third.richview();
    login.args(["login", "--server", &home.base]);
    let (shown, code) = on_a_terminal(login, "CLI token: ", TOKEN);
    assert_eq!(code, Some(0), "{shown}");
    assert!(
        shown.contains("Signed in to") && !shown.contains(TOKEN),
        "{shown}"
    );
    assert!(third.credentials().exists());
}
