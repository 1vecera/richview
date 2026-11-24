//! The command line's sign-in through the browser, end to end: the real `richview` binary against
//! a hosted app served on loopback, with the test acting as the person's browser. The short code
//! is the only thing in an address; the command line's secret and the token travel in bodies
//! only, are used once and are kept nowhere on the server (ARC-23, FEA-SF-12, INT-79).
use richview_app::{
    access::{Access, Visibility},
    server::{AppState, build_app},
    storage::FsStorage,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// The browser session of the one person the hosted fixture knows.
const SESSION: &str = "session-ada";
const CSRF: &str = "csrf-ada";

/// A hosted app on loopback with one person, Ada, in one organization, and her browser session.
struct Hosted {
    _dir: tempfile::TempDir,
    data: PathBuf,
    base: String,
}

impl Hosted {
    fn start() -> Self {
        Self::start_for("Ada Analytics")
    }

    /// The same app, with Ada's organization called `organization`.
    fn start_for(organization: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join(".access")).unwrap();
        let hash = |text: &str| format!("{:x}", Sha256::digest(text.as_bytes()));
        let state = json!({"schema":2,"tenancy":"hosted",
            "organizations":[{"id":"aaaa0001","name":organization,"created":"now","suspended":false,"limits":{}}],
            "memberships":[{"person":"ada","org":"aaaa0001","role":"owner","created":"now"}],
            "users":[{"id":"ada","name":"Ada","email":"ada@example.test","created":"now","verified":"now","tokens":[]}],
            "sessions":[{"sha256":hash(SESSION),"csrf":CSRF,"user":"ada","expires":4102444800u64,"scope":""}],
            "acls":{}});
        std::fs::write(
            data.join(".access/state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let base = serve(build_app(AppState {
            storage: FsStorage::new(&data).unwrap().with_hosted(true),
            access: Access::open_hosted(&data).unwrap(),
            preview: None,
            public_origin: Some("https://app.test".into()),
            report_origin: Some("https://reports.test".into()),
        }));
        Self {
            _dir: dir,
            data,
            base,
        }
    }

    /// A self-hosted app on loopback whose operator set `service` as the server's own token.
    fn self_hosted(service: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let base = serve(build_app(AppState {
            storage: FsStorage::new(&data).unwrap(),
            access: Access::open(&data, Visibility::Private, Some(service.into())).unwrap(),
            preview: None,
            public_origin: None,
            report_origin: None,
        }));
        Self {
            _dir: dir,
            data,
            base,
        }
    }

    /// One request, as a browser or a command line would send it. Returns the status, the
    /// response's head and its body.
    fn send(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, String, String) {
        let mut stream = TcpStream::connect(self.base.trim_start_matches("http://")).unwrap();
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nhost: app.test\r\nconnection: close\r\ncontent-length: {}\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str("\r\n");
        request.push_str(body);
        stream.write_all(request.as_bytes()).unwrap();
        let mut answer = Vec::new();
        stream.read_to_end(&mut answer).unwrap();
        let answer = String::from_utf8_lossy(&answer).into_owned();
        let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
        let status = head
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        (status, head.to_owned(), body.to_owned())
    }

    fn json(&self, method: &str, path: &str, address: &str, body: Value) -> (u16, Value) {
        let (status, _, text) = self.send(
            method,
            path,
            &[("content-type", "application/json"), ("x-real-ip", address)],
            &body.to_string(),
        );
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    /// Ada's browser opens a page.
    fn page(&self, path: &str) -> (u16, String, String) {
        let cookie = format!("rv_session={SESSION}");
        self.send("GET", path, &[("cookie", &cookie)], "")
    }

    /// Ada's browser answers a sign-in on the confirm page.
    fn decide(&self, code: &str, decision: &str, csrf: &str) -> (u16, String, String) {
        let cookie = format!("rv_session={SESSION}");
        self.send(
            "POST",
            "/ui/cli",
            &[
                ("cookie", &cookie),
                ("origin", "https://app.test"),
                ("content-type", "application/x-www-form-urlencoded"),
            ],
            &format!("csrf={csrf}&code={code}&decision={decision}"),
        )
    }

    /// Who the server says a token belongs to.
    fn whose(&self, token: &str) -> Value {
        let bearer = format!("Bearer {token}");
        let (_, _, body) = self.send("GET", "/api/session", &[("authorization", &bearer)], "");
        serde_json::from_str::<Value>(&body).unwrap()["user"].clone()
    }

    /// Everything the server has written to its data folder, as one text.
    fn written(&self) -> String {
        fn walk(dir: &Path, out: &mut String) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else {
                    out.push_str(&String::from_utf8_lossy(&std::fs::read(&path).unwrap()));
                    out.push('\n');
                }
            }
        }
        let mut out = String::new();
        walk(&self.data, &mut out);
        out
    }
}

/// Serves `app` on a loopback port of its own; returns its address.
fn serve(app: axum::Router) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
    });
    base
}

/// `richview` with its own configuration folder, no server or token in its environment and no
/// browser to open.
fn richview(config: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_richview"));
    command
        .env("XDG_CONFIG_HOME", config)
        .env("RICHVIEW_NO_BROWSER", "1")
        .env_remove("RICHVIEW_SERVER")
        .env_remove("RICHVIEW_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// A `richview login` under way: what it has said so far, and the short code it showed.
struct Login {
    child: std::process::Child,
    said: String,
    errors: BufReader<std::process::ChildStderr>,
    code: String,
}

impl Login {
    /// Starts `richview login` and reads what it says until it has shown the page to confirm on.
    fn start(config: &Path, server: &str) -> Self {
        let mut child = richview(config)
            .args(["login", "--server", server])
            .spawn()
            .unwrap();
        let mut errors = BufReader::new(child.stderr.take().unwrap());
        let mut said = String::new();
        let code = loop {
            let mut line = String::new();
            assert_ne!(
                errors.read_line(&mut line).unwrap(),
                0,
                "login ended early: {said}"
            );
            said.push_str(&line);
            if let Some((_, code)) = line.trim().split_once("/cli?code=") {
                break code.to_owned();
            }
        };
        Self {
            child,
            said,
            errors,
            code,
        }
    }

    /// Waits for the command to end; returns its exit code and everything it printed.
    fn finish(mut self) -> (Option<i32>, String) {
        self.errors.read_to_string(&mut self.said).unwrap();
        let mut out = String::new();
        self.child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut out)
            .unwrap();
        (
            self.child.wait().unwrap().code(),
            format!("{out}{}", self.said),
        )
    }
}

fn kept_token(config: &Path, server: &str) -> Option<String> {
    let bytes = std::fs::read(config.join("richview/credentials.json")).ok()?;
    serde_json::from_slice::<Value>(&bytes).ok()?["servers"][server]["token"]
        .as_str()
        .map(str::to_owned)
}

#[test]
fn the_command_line_signs_in_through_the_browser_and_signs_out_again() {
    let hosted = Hosted::start();
    let desk = tempfile::tempdir().unwrap();
    let login = Login::start(desk.path(), &hosted.base);
    let code = login.code.clone();
    // What the command line shows is the short code and the page to confirm it on, nothing else.
    assert!(
        login.said.contains(&format!(
            "Confirm this code in your browser to sign in: {code}"
        )) && login
            .said
            .contains(&format!("{}/cli?code={code}", hosted.base)),
        "{}",
        login.said
    );

    // Someone not signed in is sent to sign in and comes back to the same code.
    let (status, head, _) = hosted.send("GET", &format!("/cli?code={code}"), &[], "");
    assert_eq!(status, 303, "{head}");
    assert!(
        head.to_lowercase().contains(&format!(
            "location: /?next=/cli%3fcode%3d{}",
            code.to_lowercase()
        )),
        "{head}"
    );
    let (_, _, sign_in) = hosted.send("GET", &format!("/?next=/cli%3Fcode%3D{code}"), &[], "");
    assert!(
        sign_in.contains(&format!("name=\"next\" value=\"/cli?code={code}\"")),
        "sign-in does not return to the code"
    );
    // Only that page is a place to return to: anything else after it is dropped.
    let (_, _, hostile) = hosted.send(
        "GET",
        "/?next=/cli%3Fcode%3DBCDF-GHJK%22%3E%3Cscript",
        &[],
        "",
    );
    assert!(
        !hostile.contains("name=\"next\""),
        "an unsafe return was kept"
    );

    // Ada's browser shows the code, the computer's own name and the organization, and asks.
    let (status, _, page) = hosted.page(&format!("/cli?code={code}"));
    assert_eq!(status, 200, "{page}");
    assert!(
        page.contains(&format!("<code>{code}</code>"))
            && page.contains("Let this command line publish as you?")
            && page.contains("Ada Analytics")
            && page.contains(">Allow<")
            && page.contains(">Cancel<"),
        "{page}"
    );
    // A form without the session's CSRF token decides nothing.
    let (status, _, _) = hosted.decide(&code, "allow", "wrong");
    assert_eq!(status, 403);
    let (status, _, done) = hosted.decide(&code, "allow", CSRF);
    assert_eq!(status, 200, "{done}");
    assert!(done.contains("The command line is signed in"), "{done}");

    // The command line collects its token by itself and says who it is, never the token.
    let (exit, printed) = login.finish();
    assert_eq!(exit, Some(0), "{printed}");
    assert!(
        printed.contains(&format!(
            "Signed in to {} as Ada (ada@example.test), for Ada Analytics.",
            hosted.base
        )),
        "{printed}"
    );
    let token = kept_token(desk.path(), &hosted.base).expect("no token was kept");
    assert!(token.starts_with("rvt_"), "not a CLI token");
    assert!(
        !printed.contains(&token),
        "the token was printed: {printed}"
    );
    assert_eq!(hosted.whose(&token)["email"], "ada@example.test");

    // The server holds the token only as a digest, and its audit trail names the token, not it.
    let written = hosted.written();
    assert!(!written.contains(&token), "the server stored the token");
    assert!(
        written.contains("cli.sign-in.allow") && written.contains("token.create"),
        "the sign-in left no audit lines"
    );

    // Signing out revokes that token on the server.
    let signed_out = richview(desk.path()).arg("logout").output().unwrap();
    let said = String::from_utf8_lossy(&signed_out.stdout).into_owned();
    assert_eq!(signed_out.status.code(), Some(0), "{said}");
    assert!(said.contains("This computer's token is revoked."), "{said}");
    assert!(kept_token(desk.path(), &hosted.base).is_none());
    assert!(hosted.whose(&token).is_null(), "the token still works");
}

#[test]
fn what_the_server_says_about_the_account_reaches_the_terminal_without_control_characters() {
    // An organization name that would clear the screen, retitle the window and hide a link.
    let hosted = Hosted::start_for(
        "Ada\u{1b}[2J\u{1b}]0;owned\u{7}\u{1b}]8;;https://evil.test\u{7} Analytics",
    );
    let desk = tempfile::tempdir().unwrap();
    let login = Login::start(desk.path(), &hosted.base);
    let code = login.code.clone();
    let (status, _, done) = hosted.decide(&code, "allow", CSRF);
    assert_eq!(status, 200, "{done}");
    let (exit, printed) = login.finish();
    assert_eq!(exit, Some(0), "{printed}");
    assert!(
        printed.contains(
            "as Ada (ada@example.test), for Ada[2J]0;owned]8;;https://evil.test Analytics."
        ),
        "{printed:?}"
    );
    assert!(
        !printed.contains(['\u{1b}', '\u{7}']),
        "a control character reached the terminal: {printed:?}"
    );
    // The label kept beside the token is made plain too.
    let kept = std::fs::read_to_string(desk.path().join("richview/credentials.json")).unwrap();
    assert!(
        !kept.contains("\\u001b") && !kept.contains("\\u0007"),
        "{kept}"
    );
}

#[test]
fn signing_out_of_a_self_hosted_server_says_its_token_still_works() {
    let service = "rv-service-token-of-this-server";
    let server = Hosted::self_hosted(service);
    let desk = tempfile::tempdir().unwrap();
    let mut login = richview(desk.path())
        .args(["login", "--server", &server.base, "--with-token"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    login
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{service}\n").as_bytes())
        .unwrap();
    let signed_in = login.wait_with_output().unwrap();
    assert_eq!(
        signed_in.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&signed_in.stderr)
    );

    // A self-hosted server has no command-line revocation, and the command line says what that
    // leaves in true words.
    let bearer = format!("Bearer {service}");
    let (status, _, _) = server.send(
        "DELETE",
        "/api/cli/token",
        &[("authorization", &bearer)],
        "",
    );
    assert_eq!(status, 404);
    let signed_out = richview(desk.path()).arg("logout").output().unwrap();
    let said = String::from_utf8_lossy(&signed_out.stdout).into_owned();
    assert_eq!(signed_out.status.code(), Some(0), "{said}");
    assert!(
        said.contains(&format!("Signed out of {} on this computer.", server.base))
            && said
                .contains("does not revoke tokens from the command line, so the token still works")
            && said.contains("if it is the server's own token, changed in its settings")
            && !said.contains("no longer knew")
            && !said.contains("CLI tokens page")
            && !said.contains(service),
        "{said}"
    );
    assert!(kept_token(desk.path(), &server.base).is_none());
    assert_eq!(
        server.whose(service)["id"],
        "service",
        "the server's token stopped working"
    );
}

#[test]
fn the_confirm_page_says_what_is_asked_in_each_state() {
    let hosted = Hosted::start();
    let start = |computer: &str| {
        hosted
            .json(
                "POST",
                "/api/cli/sign-in",
                "198.51.100.20",
                json!({"computer": computer}),
            )
            .1["userCode"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let ask = hosted.page("/cli").2;
    assert!(
        ask.contains("<h1>Sign in the command line</h1>")
            && ask.contains("Enter the code your terminal shows.")
            && ask.contains("name=\"code\""),
        "{ask}"
    );
    let unknown = hosted.page("/cli?code=BBBB-BBBB").2;
    assert!(
        unknown.contains("No sign-in is waiting under that code. A code lasts 10 minutes and works once. Run the command again for a new one."),
        "{unknown}"
    );
    // A computer's name is the command line's own claim: shown as text, never as markup.
    let code = start("<b>ada</b>-laptop");
    let confirm = hosted.page(&format!("/cli?code={code}")).2;
    assert!(
        confirm.contains("<h1>Let this command line publish as you?</h1><p class=\"lede\">Check that your terminal shows this code.</p>")
            && confirm.contains("It names its computer as <strong>&lt;b&gt;ada&lt;/b&gt;-laptop</strong>.")
            && confirm.contains("It will publish to <strong>Ada Analytics</strong> with your role there, until you sign it out or revoke it on <a href=\"/tokens\">CLI tokens</a>.")
            && confirm.contains("Allow only if you started this yourself. If someone sent you this link, cancel."),
        "{confirm}"
    );
    // The code is read the way a person types it.
    let typed = hosted
        .page(&format!(
            "/cli?code={}",
            code.to_lowercase().replace('-', "")
        ))
        .2;
    assert!(typed.contains(&format!("<code>{code}</code>")), "{typed}");
    let allowed = hosted.decide(&code, "allow", CSRF).2;
    assert!(
        allowed.contains("Return to your terminal; it carries on by itself. It can now publish as you to <strong>Ada Analytics</strong>.")
            && allowed.contains("To end it, run <code>richview logout</code> there, or revoke “Command line on &lt;b&gt;ada&lt;/b&gt;-laptop” on <a href=\"/tokens\">CLI tokens</a>."),
        "{allowed}"
    );
    let cancelled = hosted.decide(&start(""), "cancel", CSRF).2;
    assert!(
        cancelled.contains("<h1>Cancelled</h1><p class=\"lede\">Nothing was signed in.</p>"),
        "{cancelled}"
    );
    // For a design capture: the same pages as files, when a folder is named for them.
    if let Some(folder) = std::env::var_os("RICHVIEW_CLI_PAGES") {
        let folder = PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        // The capture carries a plain computer name; the hostile one stays in the assertions.
        let plain = start("ada-laptop");
        let waiting = hosted.page(&format!("/cli?code={plain}")).2;
        let allowed = hosted.decide(&plain, "allow", CSRF).2;
        for (name, page) in [
            ("ask", &ask),
            ("unknown", &unknown),
            ("confirm", &waiting),
            ("allowed", &allowed),
            ("cancelled", &cancelled),
        ] {
            std::fs::write(folder.join(format!("cli-{name}.html")), page).unwrap();
        }
    }
}

#[test]
fn a_cancelled_sign_in_keeps_nothing() {
    let hosted = Hosted::start();
    let desk = tempfile::tempdir().unwrap();
    let login = Login::start(desk.path(), &hosted.base);
    let (status, _, page) = hosted.decide(&login.code, "cancel", CSRF);
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("Nothing was signed in."), "{page}");
    let (exit, printed) = login.finish();
    assert_eq!(exit, Some(2), "{printed}");
    assert!(
        printed.contains("The sign-in was cancelled in the browser; nothing was kept."),
        "{printed}"
    );
    assert!(kept_token(desk.path(), &hosted.base).is_none());
    let (_, _, tokens) = hosted.page("/tokens");
    assert!(tokens.contains("No tokens yet."), "a token was issued");
}

#[test]
fn the_secret_collects_once_and_never_reaches_an_address_or_the_servers_files() {
    let hosted = Hosted::start();
    let address = "198.51.100.7";
    let (status, started) = hosted.json(
        "POST",
        "/api/cli/sign-in",
        address,
        json!({"computer": "ada-laptop"}),
    );
    assert_eq!(status, 200, "{started}");
    let (secret, code) = (
        started["code"].as_str().unwrap().to_owned(),
        started["userCode"].as_str().unwrap().to_owned(),
    );
    assert_eq!(
        (&started["expiresIn"], &started["interval"]),
        (&json!(600), &json!(3))
    );
    let collect = |with: &str| {
        hosted.json(
            "POST",
            "/api/cli/sign-in/token",
            address,
            json!({"code": with}),
        )
    };

    // Before anyone decides, the secret only waits; the short code collects nothing at all.
    assert_eq!(collect(&secret), (200, json!({"status": "waiting"})));
    assert_eq!(collect(&code).0, 410);
    // The page shows the computer's own name; the short code is all its address carries.
    let (_, _, page) = hosted.page(&format!("/cli?code={code}"));
    assert!(page.contains("ada-laptop"), "{page}");
    assert!(!page.contains(&secret), "the secret is on the page");
    assert_eq!(hosted.decide(&code, "allow", CSRF).0, 200);

    let (status, allowed) = collect(&secret);
    assert_eq!(status, 200, "{allowed}");
    assert_eq!(allowed["status"], "allowed");
    assert_eq!(allowed["organization"], "Ada Analytics");
    let token = allowed["token"].as_str().unwrap().to_owned();
    assert_eq!(hosted.whose(&token)["id"], "ada");
    // It is an ordinary CLI token on her list, named after the computer.
    let (_, _, tokens) = hosted.page("/tokens");
    assert!(tokens.contains("Command line on ada-laptop"), "{tokens}");

    // Single use: the secret collects nothing more and the code confirms nothing more.
    let (status, again) = collect(&secret);
    assert_eq!(
        (status, &again["status"]),
        (410, &json!("expired")),
        "{again}"
    );
    assert!(again.get("token").is_none(), "{again}");
    let (_, _, spent) = hosted.decide(&code, "allow", CSRF);
    assert!(
        spent.contains("No sign-in is waiting under that code."),
        "{spent}"
    );

    // Nothing the server wrote holds the secret or the token.
    let written = hosted.written();
    assert!(!written.contains(&secret), "the server stored the secret");
    assert!(!written.contains(&token), "the server stored the token");
    // Answers that carry either are not for a cache to keep.
    let (_, head, _) = hosted.send(
        "POST",
        "/api/cli/sign-in",
        &[("content-type", "application/json"), ("x-real-ip", address)],
        "{}",
    );
    assert!(
        head.to_lowercase().contains("cache-control: no-store"),
        "{head}"
    );

    // A token revokes itself once; nobody else's request can do it.
    let bearer = format!("Bearer {token}");
    let (status, _, _) = hosted.send("DELETE", "/api/cli/token", &[], "");
    assert_eq!(status, 401);
    let (status, _, _) = hosted.send(
        "DELETE",
        "/api/cli/token",
        &[("authorization", &bearer)],
        "",
    );
    assert_eq!(status, 200);
    assert!(hosted.whose(&token).is_null());
    let (status, _, _) = hosted.send(
        "DELETE",
        "/api/cli/token",
        &[("authorization", &bearer)],
        "",
    );
    assert_eq!(status, 401);
}

#[test]
fn the_routes_anyone_may_call_are_bounded_and_open_nothing_else() {
    let hosted = Hosted::start();
    // One address starts ten sign-ins in ten minutes; the eleventh waits, another address does not.
    for _ in 0..10 {
        let (status, _) = hosted.json("POST", "/api/cli/sign-in", "203.0.113.5", json!({}));
        assert_eq!(status, 200);
    }
    let (status, refused) = hosted.json("POST", "/api/cli/sign-in", "203.0.113.5", json!({}));
    assert_eq!(status, 429, "{refused}");
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("Too many sign-ins"),
        "{refused}"
    );
    let (status, _) = hosted.json("POST", "/api/cli/sign-in", "203.0.113.6", json!({}));
    assert_eq!(status, 200);
    // So does collecting: 120 attempts a minute from one address.
    let mut last = 0;
    for _ in 0..121 {
        last = hosted
            .json(
                "POST",
                "/api/cli/sign-in/token",
                "203.0.113.9",
                json!({"code": "guess"}),
            )
            .0;
    }
    assert_eq!(last, 429);
    // A body that is no sign-in is refused, and an oversized one is cut off.
    let (status, _) = hosted.json("POST", "/api/cli/sign-in/token", "203.0.113.10", json!({}));
    assert_eq!(status, 400);
    let (status, _, _) = hosted.send(
        "POST",
        "/api/cli/sign-in",
        &[
            ("content-type", "application/json"),
            ("x-real-ip", "203.0.113.11"),
        ],
        &format!("{{\"computer\":\"{}\"}}", "x".repeat(70 * 1024)),
    );
    assert_eq!(status, 413);
    // Letting these two through opened no other write to a caller without a token.
    for (method, path) in [
        ("POST", "/api/reports"),
        ("POST", "/api/cli/sign-in/other"),
        ("PUT", "/api/cli/sign-in"),
        ("DELETE", "/api/cli/token"),
    ] {
        let (status, _) = hosted.json(method, path, "203.0.113.12", json!({}));
        assert!(
            matches!(status, 401 | 404 | 405),
            "{method} {path}: {status}"
        );
    }
    // The decision is a signed-in person's: nobody else's form reaches it.
    let (status, head, _) = hosted.send(
        "POST",
        "/ui/cli",
        &[("content-type", "application/x-www-form-urlencoded")],
        "csrf=x&code=BCDF-GHJK&decision=allow",
    );
    assert_eq!(status, 303, "{head}");
    // A person who keeps typing wrong codes is told to wait.
    let mut page = String::new();
    for _ in 0..11 {
        page = hosted.page("/cli?code=BBBB-BBBB").2;
    }
    assert!(page.contains("Too many codes did not match."), "{page}");
}
