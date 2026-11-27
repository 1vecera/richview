//! The command line's sign-in through the browser on its own computer, end to end: the real
//! `richview` binary on a pseudo-terminal against a hosted app served on loopback, with the test
//! acting as the person's browser. The command line listens on loopback, the server asks one
//! click, and the browser comes back with a one-time code that is useless without the verifier
//! (ARC-23, FEA-SF-12, INT-79).
#![cfg(unix)]
use richview_app::{
    access::{Access, cli_challenge},
    server::{AppState, build_app},
    storage::FsStorage,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpStream,
    os::fd::{FromRawFd, OwnedFd},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const SESSION: &str = "session-ada";
const CSRF: &str = "csrf-ada";
/// Another person on the same server, with a session of their own: whoever else on a computer
/// can answer a sign-in that Ada's terminal started.
const BOB_SESSION: &str = "session-bob";
const BOB_CSRF: &str = "csrf-bob";

/// A hosted app on loopback with one person, Ada, in one organization, and her browser session.
struct Hosted {
    _dir: tempfile::TempDir,
    data: PathBuf,
    base: String,
    /// The origin the app is served at, as its pages and forms are checked against.
    origin: String,
}

impl Hosted {
    fn start() -> Self {
        Self::start_at("https://app.test")
    }

    /// The same app, served at `origin` (a host name the test's browser acts as being at).
    fn start_at(origin: &str) -> Self {
        Self::start_as(origin, true)
    }

    /// The same app, told no address of its own: its pages are still checked against `origin`.
    fn start_unconfigured() -> Self {
        Self::start_as("https://app.test", false)
    }

    fn start_as(origin: &str, configured: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir_all(data.join(".access")).unwrap();
        let hash = |text: &str| format!("{:x}", Sha256::digest(text.as_bytes()));
        let state = json!({"schema":2,"tenancy":"hosted",
            "organizations":[
                {"id":"aaaa0001","name":"Ada Analytics","created":"now","suspended":false,"limits":{}},
                {"id":"bbbb0002","name":"Bob Brothers","created":"now","suspended":false,"limits":{}}],
            "memberships":[
                {"person":"ada","org":"aaaa0001","role":"owner","created":"now"},
                {"person":"bob","org":"bbbb0002","role":"owner","created":"now"}],
            "users":[
                {"id":"ada","name":"Ada","email":"ada@example.test","created":"now","verified":"now","tokens":[]},
                {"id":"bob","name":"Bob","email":"bob@example.test","created":"now","verified":"now","tokens":[]}],
            "sessions":[
                {"sha256":hash(SESSION),"csrf":CSRF,"user":"ada","expires":4102444800u64,"scope":""},
                {"sha256":hash(BOB_SESSION),"csrf":BOB_CSRF,"user":"bob","expires":4102444800u64,"scope":""}],
            "acls":{}});
        std::fs::write(
            data.join(".access/state.json"),
            serde_json::to_vec(&state).unwrap(),
        )
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = build_app(AppState {
            storage: FsStorage::new(&data).unwrap().with_hosted(true),
            access: Access::open_hosted(&data).unwrap(),
            preview: None,
            public_origin: configured.then(|| origin.into()),
            report_origin: Some("https://reports.test".into()),
        });
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
        Self {
            _dir: dir,
            data,
            base,
            origin: origin.to_owned(),
        }
    }

    fn address(&self) -> &str {
        self.base.trim_start_matches("http://")
    }

    /// One request to the app, as its pages are served at `app.test`.
    fn send(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, String, String) {
        let host = self.origin.split_once("://").map_or("app.test", |(_, h)| h);
        send_to(self.address(), host, method, path, headers, body)
    }

    /// Ada's browser opens a page.
    fn page(&self, path: &str) -> (u16, String, String) {
        self.page_as(SESSION, path)
    }

    /// The browser of whoever holds `session` opens a page.
    fn page_as(&self, session: &str, path: &str) -> (u16, String, String) {
        let cookie = format!("rv_session={session}");
        self.send("GET", path, &[("cookie", &cookie)], "")
    }

    /// Ada's browser answers the sign-in page's form.
    fn answer(&self, fields: &[(&str, &str)]) -> (u16, String, String) {
        self.answer_as(SESSION, fields)
    }

    /// The browser of whoever holds `session` answers the sign-in page's form.
    fn answer_as(&self, session: &str, fields: &[(&str, &str)]) -> (u16, String, String) {
        self.post_form("/ui/cli/authorize", session, fields)
    }

    fn post_form(
        &self,
        path: &str,
        session: &str,
        fields: &[(&str, &str)],
    ) -> (u16, String, String) {
        let cookie = format!("rv_session={session}");
        let body = fields
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        self.send(
            "POST",
            path,
            &[
                ("cookie", &cookie),
                ("origin", &self.origin),
                ("content-type", "application/x-www-form-urlencoded"),
            ],
            &body,
        )
    }

    fn json(&self, path: &str, body: Value) -> (u16, Value) {
        let (status, _, text) = self.send(
            "POST",
            path,
            &[
                ("content-type", "application/json"),
                ("x-real-ip", "198.51.100.20"),
            ],
            &body.to_string(),
        );
        (status, serde_json::from_str(&text).unwrap_or(Value::Null))
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

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// One request to `address`, with the `Host` a browser would send there. Returns the status, the
/// response's head and its body.
fn send_to(
    address: &str,
    host: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String, String) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nhost: {host}\r\nconnection: close\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(body);
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = Vec::new();
    let _ = stream.read_to_end(&mut answer);
    let answer = String::from_utf8_lossy(&answer).into_owned();
    let (head, body) = answer.split_once("\r\n\r\n").unwrap_or((&answer, ""));
    let status = head
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    (status, head.to_owned(), body.to_owned())
}

/// The value of a response header, whatever the case of its name.
fn header(head: &str, name: &str) -> Option<String> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_owned())
    })
}

/// A command run on a real pseudo-terminal that the test reads while it runs, as a person would.
struct Terminal {
    child: std::process::Child,
    shown: Arc<Mutex<String>>,
    reader: Option<std::thread::JoinHandle<()>>,
    /// What the person types.
    keyboard: std::fs::File,
}

impl Terminal {
    fn start(mut command: Command) -> Self {
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
        let child = command
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(slave)
            .spawn()
            .unwrap();
        // The command keeps copies of the terminal's far end until it is dropped; without them
        // the read below ends when the child exits.
        drop(command);
        let keyboard = master.try_clone().unwrap();
        let shown = Arc::new(Mutex::new(String::new()));
        let record = shown.clone();
        let reader = std::thread::spawn(move || {
            let mut chunk = [0u8; 1024];
            loop {
                match master.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => record
                        .lock()
                        .unwrap()
                        .push_str(&String::from_utf8_lossy(&chunk[..n])),
                }
            }
        });
        Self {
            child,
            shown,
            reader: Some(reader),
            keyboard,
        }
    }

    /// The person types a line and presses Enter.
    fn type_line(&self, text: &str) {
        (&self.keyboard)
            .write_all(format!("{text}\n").as_bytes())
            .unwrap();
    }

    /// The terminal asks whether to keep the sign-in: returns what it showed before asking, and
    /// the person answers.
    fn answer_keep(&self, answer: &str) -> String {
        let shown = self.wait_for("Keep this sign-in?");
        self.type_line(answer);
        shown
    }

    /// Everything shown so far, as a person reads it: a pseudo-terminal ends each line with
    /// a carriage return and a line feed.
    fn shown(&self) -> String {
        self.shown.lock().unwrap().replace('\r', "")
    }

    /// Waits until the terminal has shown `text` and returns everything shown so far.
    fn wait_for(&self, text: &str) -> String {
        let until = Instant::now() + Duration::from_secs(30);
        loop {
            let shown = self.shown();
            if shown.contains(text) {
                return shown;
            }
            assert!(
                Instant::now() < until,
                "the terminal never showed {text:?}: {shown}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Ends a command that is still waiting, as its person would with Ctrl-C.
    fn stop(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }

    /// Waits for the command to end; returns everything it showed and its exit code.
    fn finish(mut self) -> (String, Option<i32>) {
        let until = Instant::now() + Duration::from_secs(30);
        let code = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status.code();
            }
            if Instant::now() >= until {
                let _ = self.child.kill();
                panic!("the command did not end: {}", self.shown());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        (self.shown().replace('\r', ""), code)
    }
}

/// `richview` with its own configuration folder, at a desktop whose browser opener does nothing
/// (the test is the browser), over no SSH connection, and with no server or token in its
/// environment.
fn richview(config: &Path) -> Command {
    let opener = config.join("openers");
    std::fs::create_dir_all(&opener).unwrap();
    for name in ["xdg-open", "open"] {
        let script = opener.join(name);
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::join_paths(std::iter::once(opener).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_richview"));
    command
        .env("XDG_CONFIG_HOME", config)
        .env("PATH", path)
        .env_remove("RICHVIEW_NO_BROWSER")
        .env("DISPLAY", ":99")
        .env_remove("SSH_CONNECTION")
        .env_remove("SSH_TTY")
        .env_remove("RICHVIEW_SERVER")
        .env_remove("RICHVIEW_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// What a `richview login` under way has shown: the address of its listener and the page it sent
/// the person to.
struct Waiting {
    /// The path and query of that page.
    path: String,
    /// `127.0.0.1:<port>`, where the command line listens.
    listener: String,
    state: String,
}

fn waiting(terminal: &Terminal, server: &str) -> Waiting {
    let shown = terminal.wait_for("/cli/authorize?");
    let page = shown
        .split_whitespace()
        .find(|word| word.contains("/cli/authorize?"))
        .unwrap()
        .to_owned();
    // Printed whole: the terminal does not wrap or cut an address.
    let path = page.strip_prefix(server).unwrap().to_owned();
    let query: std::collections::BTreeMap<String, String> = path
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap();
            (key.to_owned(), value.to_owned())
        })
        .collect();
    let redirect = &query["redirect_uri"];
    let listener = redirect
        .strip_prefix("http%3A%2F%2F")
        .unwrap()
        .strip_suffix("%2Fcallback")
        .unwrap()
        .replace("%3A", ":");
    Waiting {
        path,
        listener,
        state: query["state"].clone(),
    }
}

/// The values of a page's hidden inputs, the way the browser sends them back: each name once, as
/// the page's two forms repeat them.
fn fields(page: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut rest = page;
    while let Some(at) = rest.find("<input type=\"hidden\" name=\"") {
        rest = &rest[at + "<input type=\"hidden\" name=\"".len()..];
        let name = &rest[..rest.find('"').unwrap()];
        let value_at = rest.find("value=\"").unwrap() + "value=\"".len();
        let value = &rest[value_at..value_at + rest[value_at..].find('"').unwrap()];
        if out.iter().all(|(seen, _)| seen != name) {
            out.push((
                name.to_owned(),
                value
                    .replace("&amp;", "&")
                    .replace("&quot;", "\"")
                    .replace("&lt;", "<")
                    .replace("&gt;", ">"),
            ));
        }
    }
    out
}

fn kept(config: &Path, server: &str) -> Option<Value> {
    let bytes = std::fs::read(config.join("richview/credentials.json")).ok()?;
    let file: Value = serde_json::from_slice(&bytes).ok()?;
    let entry = file["servers"][server].clone();
    (!entry.is_null()).then_some(entry)
}

/// Ada's browser at the consent page for what the command line printed, answering `decision`.
/// Returns the moving-on page's response and the address it sends the browser to.
fn consent(hosted: &Hosted, waiting: &Waiting, decision: &str) -> (String, String) {
    let (status, _, page) = hosted.page(&waiting.path);
    assert_eq!(status, 200, "{page}");
    let mut form = fields(&page);
    form.retain(|(name, _)| name != "decision");
    form.push(("decision".into(), decision.into()));
    let sent: Vec<(&str, &str)> = form.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
    let (status, head, answer) = hosted.answer(&sent);
    assert_eq!(status, 200, "{answer}");
    let target = header(&head, "refresh")
        .and_then(|refresh| refresh.split_once("url=").map(|(_, url)| url.to_owned()))
        .unwrap_or_else(|| panic!("no address to move on to: {head}"));
    (answer, target)
}

/// The browser follows the address it was sent to, as it does after a form: to the command line's
/// own port, which answers with the page that says it worked.
fn follow(target: &str) -> (u16, String, String) {
    let address = target
        .strip_prefix("http://")
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    let path = &target[("http://".len() + address.len())..];
    let (status, head, body) = send_to(address, address, "GET", path, &[], "");
    if status == 303 {
        let next = header(&head, "location").unwrap();
        assert_eq!(next, "/done", "{head}");
        return send_to(address, address, "GET", &next, &[], "");
    }
    (status, head, body)
}

#[test]
fn the_command_line_signs_in_through_the_browser_on_its_own_computer_and_signs_out_again() {
    let hosted = Hosted::start();
    let desk = tempfile::tempdir().unwrap();
    let mut command = richview(desk.path());
    command.args(["login", "--server", &hosted.base]);
    let terminal = Terminal::start(command);
    let waiting = waiting(&terminal, &hosted.base);
    // It says what it does and where to go, and nothing about a code to compare.
    let shown = terminal.shown();
    assert!(
        shown.contains(&format!(
            "Opening {} in your browser to sign in.\nIf it does not open, use this address:",
            hosted.base
        )) && !shown.contains("Confirm this code"),
        "{shown}"
    );
    // Both the terminal and the page show the code that tells this request from a link somebody
    // else's program sent.
    let check = loop {
        let shown = terminal.wait_for("Press Allow only if the page shows the code ");
        if let Some(code) = shown
            .split("the page shows the code ")
            .nth(1)
            .and_then(|rest| rest.get(..10))
            .filter(|code| code.ends_with('.'))
        {
            break code[..9].to_owned();
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(check.as_bytes()[4] == b'-', "{check}");
    // The address carries a public challenge, this run's state and its loopback port, no secret.
    assert!(
        waiting
            .path
            .starts_with("/cli/authorize?redirect_uri=http%3A%2F%2F127.0.0.1%3A")
            && waiting
                .path
                .contains("&code_challenge_method=S256&computer="),
        "{}",
        waiting.path
    );

    // Someone not signed in is sent to sign in, and comes back to the same request.
    let (status, head, _) = hosted.send("GET", &waiting.path, &[], "");
    assert_eq!(status, 303, "{head}");
    let back = header(&head, "location").unwrap();
    assert!(
        back.starts_with("/?next=/cli/authorize%3Fredirect_uri%3D"),
        "{back}"
    );
    let (_, _, sign_in) = hosted.send("GET", &back, &[], "");
    assert!(
        sign_in.contains(&format!(
            "name=\"next\" value=\"{}\"",
            waiting.path.replace('&', "&amp;")
        )),
        "sign-in does not return to the request"
    );

    // Ada's browser shows who is asking, who she is and where it will publish, and asks once.
    let (status, _, page) = hosted.page(&waiting.path);
    assert_eq!(status, 200, "{page}");
    assert!(
        page.contains("Let this command line publish as you?")
            && page.contains("You are signed in as <strong>Ada (ada@example.test)</strong>")
            && page.contains("Ada Analytics")
            && page.contains(&format!("<p class=\"cli-code\"><code>{check}</code></p>"))
            && page.contains("name=\"org\" value=\"aaaa0001\"")
            && page.contains(">Allow<")
            && page.contains(">Cancel<")
            && page.contains(&format!(
                "name=\"redirect_uri\" value=\"http://{}/callback\"",
                waiting.listener
            )),
        "{page}"
    );
    // A form without the session's CSRF token decides nothing.
    let form = fields(&page);
    let mut forged: Vec<(&str, &str)> = form
        .iter()
        .filter(|(name, _)| name != "csrf" && name != "decision")
        .map(|(n, v)| (n.as_str(), v.as_str()))
        .collect();
    forged.push(("csrf", "wrong"));
    forged.push(("decision", "allow"));
    assert_eq!(hosted.answer(&forged).0, 403);

    // She clicks Allow: the browser is sent to the command line's own port with a one-time code
    // and the command line's own state, and nowhere else.
    let (moving, target) = consent(&hosted, &waiting, "allow");
    assert!(
        moving.contains("Taking you back to your terminal."),
        "{moving}"
    );
    assert!(
        target.starts_with(&format!("http://{}/callback?code=", waiting.listener))
            && target.ends_with(&format!("&state={}", waiting.state)),
        "{target}"
    );
    let (status, _, done) = follow(&target);
    assert_eq!(status, 200, "{done}");
    // The page the browser is left on says nothing about whose sign-in it is: anything on this
    // computer can ask the listener for it.
    assert!(
        done.contains("Back to your terminal")
            && !done.contains("Ada")
            && !done.contains("ada@example.test")
            && !done.contains(&hosted.base)
            && !done.contains("code="),
        "{done}"
    );

    // The terminal shows whose it is and the organization it is for, keeps nothing until its
    // person says so, and then says who is signed in in one line and shows no token.
    let asked = terminal.answer_keep("y");
    assert!(
        asked.contains("address       ada@example.test")
            && asked.contains("organization  Ada Analytics")
            && asked.contains("name          \u{201c}Ada\u{201d}"),
        "{asked}"
    );
    // The address and the organization come first, and the name, which the account's owner
    // chose, last.
    assert!(
        asked.find("address ").unwrap() < asked.find("organization ").unwrap()
            && asked.find("organization ").unwrap() < asked.find("name  ").unwrap(),
        "{asked}"
    );
    let (printed, exit) = terminal.finish();
    assert_eq!(exit, Some(0), "{printed}");
    assert!(
        printed.contains(&format!(
            "Signed in to {} as Ada (ada@example.test), for Ada Analytics. This computer stays signed in; `richview logout` ends it.",
            hosted.base
        )),
        "{printed}"
    );
    let entry = kept(desk.path(), &hosted.base).expect("no sign-in was kept");
    let (token, refresh) = (
        entry["token"].as_str().unwrap().to_owned(),
        entry["refresh"].as_str().unwrap().to_owned(),
    );
    assert!(token.starts_with("rvt_") && refresh.starts_with("rvr_"));
    assert!(
        !printed.contains(&token) && !printed.contains(&refresh),
        "a secret was printed: {printed}"
    );
    let ends = entry["expires"].as_u64().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(
        (now + 3500..=now + 3700).contains(&ends),
        "the token should last about an hour: {ends}"
    );
    assert_eq!(hosted.whose(&token)["email"], "ada@example.test");
    // The file is private, and the server holds digests only.
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&desk.path().join("richview/credentials.json")), 0o600);
        assert_eq!(mode(&desk.path().join("richview")), 0o700);
    }
    let written = hosted.written();
    assert!(
        !written.contains(&token) && !written.contains(&refresh),
        "the server stored a secret"
    );
    assert!(
        written.contains("cli.sign-in.allow") && written.contains("token.create"),
        "the sign-in left no audit lines"
    );
    // The command line's port is closed again, and the code it was sent works once, here, never again.
    assert!(TcpStream::connect(&waiting.listener).is_err());

    // Signing out revokes the sign-in on the server.
    let signed_out = richview(desk.path()).arg("logout").output().unwrap();
    let said = String::from_utf8_lossy(&signed_out.stdout).into_owned();
    assert_eq!(signed_out.status.code(), Some(0), "{said}");
    assert!(said.contains("This computer's token is revoked."), "{said}");
    assert!(kept(desk.path(), &hosted.base).is_none());
    assert!(hosted.whose(&token).is_null(), "the token still works");
    assert_eq!(
        hosted
            .json(
                "/api/cli/oauth/token",
                json!({"grantType": "refresh_token", "refreshToken": refresh})
            )
            .0,
        401,
        "the refresh token still works"
    );
}

#[test]
fn saying_no_in_the_browser_keeps_nothing_and_says_so_in_the_terminal() {
    let hosted = Hosted::start();
    let desk = tempfile::tempdir().unwrap();
    let mut command = richview(desk.path());
    command.args(["login", "--server", &hosted.base]);
    let terminal = Terminal::start(command);
    let waiting = waiting(&terminal, &hosted.base);
    let (moving, target) = consent(&hosted, &waiting, "cancel");
    assert!(
        moving.contains("Taking you back to your terminal."),
        "{moving}"
    );
    assert_eq!(
        target,
        format!(
            "http://{}/callback?error=access_denied&state={}",
            waiting.listener, waiting.state
        )
    );
    let (status, _, done) = follow(&target);
    assert_eq!(status, 200, "{done}");
    assert!(done.contains("Sign-in cancelled"), "{done}");
    let (printed, exit) = terminal.finish();
    assert_eq!(exit, Some(2), "{printed}");
    assert!(
        printed.contains("The sign-in was cancelled in the browser; nothing was kept."),
        "{printed}"
    );
    assert!(kept(desk.path(), &hosted.base).is_none());
    assert!(!hosted.written().contains("rvt_"), "a token was issued");
}

#[test]
fn allow_answers_for_the_organization_the_page_named_and_for_no_other() {
    let hosted = Hosted::start();
    let desk = tempfile::tempdir().unwrap();
    let mut command = richview(desk.path());
    command.args(["login", "--server", &hosted.base]);
    let terminal = Terminal::start(command);
    let waiting = waiting(&terminal, &hosted.base);
    let (status, _, page) = hosted.page(&waiting.path);
    assert_eq!(status, 200, "{page}");
    // The page named one organization. Its form arriving with another, as it would from a tab
    // that was open before the session moved to a second organization, signs nothing in.
    let mut form = fields(&page);
    form.retain(|(name, _)| name != "decision" && name != "org");
    form.push(("org".into(), "bbbb0002".into()));
    form.push(("decision".into(), "allow".into()));
    let sent: Vec<(&str, &str)> = form.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
    let (_, head, answer) = hosted.answer(&sent);
    assert!(
        answer.contains("Your organization changed while this page was open"),
        "{answer}"
    );
    assert!(header(&head, "refresh").is_none(), "{head}");
    assert!(!hosted.written().contains("rvt_"), "a token was issued");
    assert!(kept(desk.path(), &hosted.base).is_none());
    // The command line is still waiting for its person, and a Cancel ends it.
    let (_, target) = consent(&hosted, &waiting, "cancel");
    let (status, _, _) = follow(&target);
    assert_eq!(status, 200);
    let (printed, exit) = terminal.finish();
    assert_eq!(exit, Some(2), "{printed}");
}

#[test]
fn only_the_browser_that_was_sent_can_answer_the_listener_and_a_wrong_answer_is_just_refused() {
    let hosted = Hosted::start();
    let desk = tempfile::tempdir().unwrap();
    let mut command = richview(desk.path());
    command.args(["login", "--server", &hosted.base]);
    let terminal = Terminal::start(command);
    let waiting = waiting(&terminal, &hosted.base);
    let at = |host: &str, method: &str, path: &str| {
        send_to(&waiting.listener, host, method, path, &[], "")
    };
    // Another state is refused, and refused alike for any code, so the run learns nothing.
    for state in ["wrong-state-wrong-state", "", &waiting.state[1..]] {
        let (status, _, body) = at(
            &waiting.listener,
            "GET",
            &format!("/callback?code=x&state={state}"),
        );
        assert_eq!(status, 400, "{state}: {body}");
        assert!(body.contains("does not belong to the sign-in"), "{body}");
    }
    let (status, _, _) = at(&waiting.listener, "GET", "/callback?code=x");
    assert_eq!(status, 400);
    // The right state at a name that only points here is refused too (DNS rebinding).
    let port = waiting.listener.rsplit(':').next().unwrap();
    for host in [format!("evil.test:{port}"), format!("localhost:{port}")] {
        // Nor does it say what it has to anyone at such a name.
        let (status, _, body) = at(&host, "GET", "/done");
        assert_eq!(status, 400, "{host}: {body}");
        let (status, _, body) = at(
            &host,
            "GET",
            &format!("/callback?code=x&state={}", waiting.state),
        );
        assert_eq!(status, 400, "{host}: {body}");
        assert!(body.contains("Wrong address."), "{body}");
    }
    // Nothing else is served: no other path, no other method, and no page before an answer.
    for (method, path) in [
        ("GET", "/"),
        ("GET", "/favicon.ico"),
        ("GET", "/done"),
        ("GET", "/cancel"),
        ("GET", "/callback/"),
    ] {
        assert_eq!(
            at(&waiting.listener, method, path).0,
            404,
            "{method} {path}"
        );
    }
    assert!(matches!(
        at(&waiting.listener, "POST", "/callback").0,
        404 | 405
    ));
    // None of that ended the wait: it is still waiting for the real answer.
    assert!(!terminal.shown().contains("Signed in"));

    // The right state with a code the server never made, which is what anything that has seen
    // the address in a process list can bring: the server refuses it, the browser is told so at
    // once, and the run goes on waiting for the person's own answer.
    let (status, head, forged) = at(
        &waiting.listener,
        "GET",
        &format!(
            "/callback?code=not-a-code-the-server-made&state={}",
            waiting.state
        ),
    );
    assert_eq!(status, 400, "{head}");
    assert!(
        forged.contains("This sign-in could not be finished")
            && forged.contains("refused or has expired"),
        "{forged}"
    );
    for (name, wanted) in [
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
    ] {
        assert_eq!(header(&head, name).as_deref(), Some(wanted), "{head}");
    }
    assert!(
        header(&head, "content-security-policy")
            .unwrap()
            .contains("default-src 'none'"),
        "{head}"
    );
    // Not an answer: nothing to show yet, and the terminal says it is still waiting.
    assert_eq!(at(&waiting.listener, "GET", "/done").0, 404);
    let shown = terminal.wait_for("still waiting");
    assert!(
        shown.contains("a sign-in the browser brought back was refused")
            && !shown.contains("Signed in"),
        "{shown}"
    );

    // The person's own Allow still signs in.
    let (_, target) = consent(&hosted, &waiting, "allow");
    let (status, _, done) = follow(&target);
    assert_eq!(status, 200, "{done}");
    assert!(done.contains("Back to your terminal"), "{done}");
    terminal.answer_keep("y");
    let (printed, exit) = terminal.finish();
    assert_eq!(exit, Some(0), "{printed}");
    assert!(
        printed.contains(&format!("Signed in to {}", hosted.base)),
        "{printed}"
    );
    assert!(kept(desk.path(), &hosted.base).is_some());
}

#[test]
fn a_code_trades_once_with_its_verifier_and_nothing_else_signs_in() {
    let hosted = Hosted::start();
    let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    let redirect = "http://127.0.0.1:49152/callback";
    let state = "state-of-this-login-0123";
    let request = |verifier: &str| {
        let challenge = cli_challenge(verifier);
        format!(
            "/cli/authorize?redirect_uri={}&state={state}&code_challenge={challenge}&code_challenge_method=S256&computer=ada-laptop",
            encode(redirect)
        )
    };
    // A code is what a click on Allow makes.
    let code_for = |verifier: &str| {
        let (status, _, page) = hosted.page(&request(verifier));
        assert_eq!(status, 200, "{page}");
        let mut form = fields(&page);
        form.retain(|(name, _)| name != "decision");
        form.push(("decision".into(), "allow".into()));
        let sent: Vec<(&str, &str)> = form.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
        let (status, head, answer) = hosted.answer(&sent);
        assert_eq!(status, 200, "{answer}");
        let target = header(&head, "refresh").unwrap();
        target
            .split_once("code=")
            .unwrap()
            .1
            .split('&')
            .next()
            .unwrap()
            .to_owned()
    };
    let trade = |code: &str, verifier: &str, redirect: &str| {
        hosted.json(
            "/api/cli/oauth/token",
            json!({"grantType": "authorization_code", "code": code, "codeVerifier": verifier, "redirectUri": redirect}),
        )
    };

    // The right verifier and redirect trade the code for a token that renews.
    let code = code_for(verifier);
    let (status, grant) = trade(&code, verifier, redirect);
    assert_eq!(status, 200, "{grant}");
    assert_eq!(grant["status"], "allowed");
    assert_eq!(grant["expiresIn"], 3600);
    assert_eq!(grant["account"]["email"], "ada@example.test");
    assert_eq!(grant["organization"], "Ada Analytics");
    let (token, refresh) = (
        grant["token"].as_str().unwrap().to_owned(),
        grant["refreshToken"].as_str().unwrap().to_owned(),
    );
    assert_eq!(hosted.whose(&token)["id"], "ada");
    // Once: the same code gives nothing more.
    assert_eq!(trade(&code, verifier, redirect).0, 400);

    // A wrong verifier, a wrong redirect or a made-up code give nothing, and a code that was
    // tried wrong is spent.
    for (verifier_sent, redirect_sent) in [
        ("another-verifier-another-verifier-another-0", redirect),
        (verifier, "http://127.0.0.1:49153/callback"),
        (verifier, "https://evil.test/callback"),
    ] {
        let code = code_for(verifier);
        let (status, refused) = trade(&code, verifier_sent, redirect_sent);
        assert_eq!(status, 400, "{refused}");
        assert_eq!(refused["status"], "refused");
        assert!(refused.get("token").is_none() && refused.get("refreshToken").is_none());
        assert_eq!(
            trade(&code, verifier, redirect).0,
            400,
            "the code was not spent"
        );
    }
    assert_eq!(trade("no-such-code", verifier, redirect).0, 400);
    assert_eq!(
        hosted
            .json("/api/cli/oauth/token", json!({"grantType": "password"}))
            .0,
        400
    );

    // The refresh token trades itself for the next pair, once.
    let (status, next) = hosted.json(
        "/api/cli/oauth/token",
        json!({"grantType": "refresh_token", "refreshToken": refresh}),
    );
    assert_eq!(status, 200, "{next}");
    let (next_token, next_refresh) = (
        next["token"].as_str().unwrap().to_owned(),
        next["refreshToken"].as_str().unwrap().to_owned(),
    );
    assert!(next_token != token && next_refresh != refresh);
    assert_eq!(hosted.whose(&next_token)["id"], "ada");
    // Asked for again, the traded refresh token ends the sign-in: a lost answer is no different
    // to the server from a copy of the file, so the computer that lost it signs in again.
    let (status, ended) = hosted.json(
        "/api/cli/oauth/token",
        json!({"grantType": "refresh_token", "refreshToken": refresh}),
    );
    assert_eq!(status, 401, "{ended}");
    assert_eq!(ended["status"], "ended");
    assert_ne!(
        hosted.whose(&next_token)["id"],
        "ada",
        "the new token still works"
    );
    let (status, gone) = hosted.json(
        "/api/cli/oauth/token",
        json!({"grantType": "refresh_token", "refreshToken": next_refresh}),
    );
    assert_eq!(status, 401, "{gone}");
    // The server holds digests only.
    let written = hosted.written();
    for secret in [&token, &refresh, &next_token, &next_refresh, &code] {
        assert!(
            !written.contains(secret.as_str()),
            "the server stored a secret"
        );
    }

    // A sign-out by refresh token ends a sign-in whose access token is long over.
    let (_, fresh) = trade(&code_for(verifier), verifier, redirect);
    let refresh = fresh["refreshToken"].as_str().unwrap().to_owned();
    let (status, revoked) = hosted.json("/api/cli/oauth/revoke", json!({"token": refresh}));
    assert_eq!((status, &revoked["revoked"]), (200, &json!(true)));
    let (_, again) = hosted.json("/api/cli/oauth/revoke", json!({"token": refresh}));
    assert_eq!(again["revoked"], false);
    assert!(hosted.whose(fresh["token"].as_str().unwrap()).is_null());
}

#[test]
fn an_address_that_is_not_a_command_lines_own_is_refused_before_anything_is_sent() {
    let hosted = Hosted::start();
    let challenge = cli_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    let state = "state-of-this-login-0123";
    let at = |redirect: &str, method: &str, state: &str| {
        format!(
            "/cli/authorize?redirect_uri={}&state={state}&code_challenge={challenge}&code_challenge_method={method}",
            encode(redirect)
        )
    };
    for (redirect, method) in [
        ("https://evil.test/callback", "S256"),
        ("http://evil.test:49152/callback", "S256"),
        ("http://127.0.0.1:80/callback", "S256"),
        ("http://127.0.0.1:49152/callback?x=1", "S256"),
        ("http://127.0.0.1:49152/callback", "plain"),
        ("http://127.0.0.1:49152/callback", ""),
    ] {
        // Signed in or not, it is refused as what it is, and nothing is sent to the address.
        for (status, _, page) in [
            hosted.send("GET", &at(redirect, method, state), &[], ""),
            hosted.page(&at(redirect, method, state)),
        ] {
            assert_eq!(status, 400, "{redirect} {method}: {page}");
            assert!(page.contains("This sign-in link is not valid"), "{page}");
            assert!(!page.contains("evil.test"), "{page}");
        }
    }
    assert_eq!(
        hosted
            .page(&at("http://127.0.0.1:49152/callback", "S256", "short"))
            .0,
        400
    );
    // The answering form is held to the same rules: no code is made for an address of anyone's.
    let (status, _, page) = hosted.answer(&[
        ("csrf", CSRF),
        ("redirect_uri", "https://evil.test/callback"),
        ("state", state),
        ("code_challenge", &challenge),
        ("computer", "x"),
        ("decision", "allow"),
    ]);
    assert_eq!(status, 200);
    assert!(
        page.contains("This sign-in cannot be finished") && !page.contains("evil.test"),
        "{page}"
    );
    assert!(!hosted.written().contains("cli.sign-in.allow"));
    // The trade is bounded like the other routes anyone may call: a body that is no request is
    // refused, an oversized one is cut off, and its answers are not for a cache to keep.
    let json_headers = [
        ("content-type", "application/json"),
        ("x-real-ip", "203.0.113.11"),
    ];
    assert_eq!(
        hosted
            .send("POST", "/api/cli/oauth/token", &json_headers, "{}")
            .0,
        400
    );
    let (status, _, _) = hosted.send(
        "POST",
        "/api/cli/oauth/token",
        &json_headers,
        &format!("{{\"grantType\":\"{}\"}}", "x".repeat(70 * 1024)),
    );
    assert_eq!(status, 413);
    let (_, head, _) = hosted.send(
        "POST",
        "/api/cli/oauth/token",
        &json_headers,
        "{\"grantType\":\"refresh_token\",\"refreshToken\":\"rvr_nope\"}",
    );
    assert!(head.to_lowercase().contains("no-store"), "{head}");
    // Letting these through opened no other write to a caller without a token.
    for (method, path) in [
        ("POST", "/api/cli/oauth/other"),
        ("PUT", "/api/cli/oauth/token"),
        ("GET", "/api/cli/oauth/token"),
    ] {
        let (status, _, _) = hosted.send(method, path, &json_headers, "{}");
        assert!(
            matches!(status, 401 | 404 | 405),
            "{method} {path}: {status}"
        );
    }
    // What a server says about itself is for any command line to read.
    let (status, _, body) = hosted.send("GET", "/api/cli/oauth/metadata", &[], "");
    let metadata: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, 200);
    assert_eq!(metadata["flows"], json!(["loopback", "device"]));
    assert_eq!(metadata["token"], "/api/cli/oauth/token");
}

#[test]
fn over_ssh_or_on_request_the_short_code_is_used_and_it_renews_too() {
    let hosted = Hosted::start();
    // `--no-browser` and RICHVIEW_NO_BROWSER say the browser is not on this computer or not for
    // the command to open, so the address of a loopback sign-in would lead nowhere.
    struct Way {
        label: &'static str,
        args: &'static [&'static str],
        env: &'static [(&'static str, &'static str)],
    }
    let ways = [
        Way {
            label: "--device",
            args: &["--device"],
            env: &[],
        },
        Way {
            label: "ssh",
            args: &[],
            env: &[("SSH_CONNECTION", "192.0.2.1 50000 192.0.2.2 22")],
        },
        Way {
            label: "--no-browser",
            args: &["--no-browser"],
            env: &[],
        },
        Way {
            label: "RICHVIEW_NO_BROWSER",
            args: &[],
            env: &[("RICHVIEW_NO_BROWSER", "1")],
        },
    ];
    for Way { label, args, env } in ways {
        let desk = tempfile::tempdir().unwrap();
        let mut command = richview(desk.path());
        command.args(["login", "--server", &hosted.base]).args(args);
        command.envs(env.iter().copied());
        let terminal = Terminal::start(command);
        let shown = terminal.wait_for("/cli?code=");
        assert!(
            shown.contains("Confirm this code in your browser to sign in: ")
                && !shown.contains("/cli/authorize"),
            "{label}: {shown}"
        );
        let code = shown
            .split("/cli?code=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned();
        let cookie = format!("rv_session={SESSION}");
        let (status, _, done) = hosted.send(
            "POST",
            "/ui/cli",
            &[
                ("cookie", &cookie),
                ("origin", "https://app.test"),
                ("content-type", "application/x-www-form-urlencoded"),
            ],
            &format!("csrf={CSRF}&code={code}&org=aaaa0001&decision=allow"),
        );
        assert_eq!(status, 200, "{done}");
        terminal.answer_keep("y");
        let (printed, exit) = terminal.finish();
        assert_eq!(exit, Some(0), "{label}: {printed}");
        assert!(printed.contains("Signed in to "), "{label}: {printed}");
        // This server renews sign-ins, so this one renews.
        let entry = kept(desk.path(), &hosted.base).expect("no sign-in was kept");
        assert!(
            entry["token"].as_str().unwrap().starts_with("rvt_")
                && entry["refresh"].as_str().unwrap().starts_with("rvr_")
                && entry["expires"].as_u64().is_some(),
            "{label}: {entry}"
        );
        assert_eq!(hosted.whose(entry["token"].as_str().unwrap())["id"], "ada");
    }
}

/// The address the browser is sent on to after a click, from the moving-on page's headers.
fn moved_to(head: &str) -> String {
    header(head, "refresh")
        .and_then(|refresh| refresh.split_once("url=").map(|(_, url)| url.to_owned()))
        .unwrap_or_else(|| panic!("no address to move on to: {head}"))
}

#[test]
fn a_sign_in_another_person_answered_is_shown_by_name_and_kept_only_if_the_terminal_says_so() {
    let hosted = Hosted::start();
    let desk = tempfile::tempdir().unwrap();
    let mut command = richview(desk.path());
    command.args(["login", "--server", &hosted.base]);
    let terminal = Terminal::start(command);
    let waiting = waiting(&terminal, &hosted.base);
    // Bob, who read the address this run opened in a process list, opens it in his own session
    // and clicks Allow. His browser delivers the code to Ada's listener, and the state, the
    // challenge and the check code all match.
    let (status, _, page) = hosted.page_as(BOB_SESSION, &waiting.path);
    assert_eq!(status, 200, "{page}");
    let mut form = fields(&page);
    form.retain(|(name, _)| name != "decision");
    form.push(("decision".into(), "allow".into()));
    let sent: Vec<(&str, &str)> = form.iter().map(|(n, v)| (n.as_str(), v.as_str())).collect();
    let (status, head, answer) = hosted.answer_as(BOB_SESSION, &sent);
    assert_eq!(status, 200, "{answer}");
    let (status, _, done) = follow(&moved_to(&head));
    assert_eq!(status, 200, "{done}");
    assert!(
        !done.contains("Bob") && !done.contains("bob@example.test"),
        "{done}"
    );
    // Ada's terminal says whose sign-in it was handed. She does not know Bob: no.
    let asked = terminal.answer_keep("n");
    assert!(
        asked.contains("address       bob@example.test")
            && asked.contains("organization  Bob Brothers"),
        "{asked}"
    );
    let (printed, exit) = terminal.finish();
    assert_eq!(exit, Some(2), "{printed}");
    assert!(printed.contains("nothing was kept"), "{printed}");
    assert!(
        !printed.contains("Signed in to"),
        "it went on as if it had been signed in: {printed}"
    );
    assert!(kept(desk.path(), &hosted.base).is_none());
    // What the server made is ended too, not left waiting for whoever has it.
    let written = hosted.written();
    assert!(written.contains("token.revoke"), "{written}");
}

#[test]
fn nothing_but_a_yes_keeps_a_sign_in() {
    // An empty line, a stray word and the end of the input are all a no.
    for answer in ["", "maybe", "no", "yess"] {
        let hosted = Hosted::start();
        let desk = tempfile::tempdir().unwrap();
        let mut command = richview(desk.path());
        command.args(["login", "--server", &hosted.base]);
        let terminal = Terminal::start(command);
        let waiting = waiting(&terminal, &hosted.base);
        let (_, target) = consent(&hosted, &waiting, "allow");
        follow(&target);
        terminal.answer_keep(answer);
        let (printed, exit) = terminal.finish();
        assert_eq!(exit, Some(2), "{answer:?}: {printed}");
        assert!(kept(desk.path(), &hosted.base).is_none(), "{answer:?}");
    }
}

#[test]
fn an_app_told_no_address_and_reached_at_this_computers_own_offers_the_short_code_only() {
    // Its browser goes where the command line was pointed, and the consent page refuses a loopback
    // sign-in there, so the metadata does not offer one.
    let hosted = Hosted::start_unconfigured();
    let flows = |host: &str| {
        let (status, _, body) = send_to(
            hosted.address(),
            host,
            "GET",
            "/api/cli/oauth/metadata",
            &[],
            "",
        );
        assert_eq!(status, 200);
        serde_json::from_str::<Value>(&body).unwrap()["flows"].clone()
    };
    assert_eq!(flows("localhost:4821"), json!(["device"]));
    assert_eq!(flows("127.1:4821"), json!(["device"]));
    assert_eq!(flows("app.localhost"), json!(["device"]));
    assert_eq!(flows("app.test"), json!(["loopback", "device"]));
    // A server with an address of its own is asked about that address: the same request to it
    // still offers the loopback sign-in, and the consent page's refusal sends a command line that
    // reached it at this computer's own address to `--device`.
    let configured = Hosted::start();
    let (_, _, body) = send_to(
        configured.address(),
        "localhost:4821",
        "GET",
        "/api/cli/oauth/metadata",
        &[],
        "",
    );
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["flows"],
        json!(["loopback", "device"])
    );
}

#[test]
fn an_app_reached_at_127_0_0_1_signs_the_command_line_in_with_the_short_code_only() {
    // A cookie is not scoped by port: the session cookie of an app at 127.0.0.1 goes to every
    // port of that address, a command line's loopback port among them.
    let hosted = Hosted::start_at("http://127.0.0.1:4820");
    let (status, _, body) = hosted.send("GET", "/api/cli/oauth/metadata", &[], "");
    let metadata: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, 200);
    assert_eq!(metadata["flows"], json!(["device"]));
    // The authorize page refuses, signed in or not, and so does its form; nothing is issued.
    let challenge = cli_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    let state = "state-of-this-login-0123";
    let path = format!(
        "/cli/authorize?redirect_uri={}&state={state}&code_challenge={challenge}&code_challenge_method=S256&computer=ada-laptop",
        encode("http://127.0.0.1:49152/callback")
    );
    for (status, _, page) in [hosted.send("GET", &path, &[], ""), hosted.page(&path)] {
        assert_eq!(status, 400, "{page}");
        assert!(page.contains("richview login --device"), "{page}");
    }
    let (status, _, page) = hosted.answer(&[
        ("csrf", CSRF),
        ("redirect_uri", "http://127.0.0.1:49152/callback"),
        ("state", state),
        ("code_challenge", &challenge),
        ("computer", "ada-laptop"),
        ("org", "aaaa0001"),
        ("decision", "allow"),
    ]);
    assert_eq!(status, 400, "{page}");
    assert!(!hosted.written().contains("cli.sign-in.allow"));
    // The command line, told so by the metadata, shows the short code and not a loopback address.
    let desk = tempfile::tempdir().unwrap();
    let mut command = richview(desk.path());
    command.args(["login", "--server", &hosted.base]);
    let terminal = Terminal::start(command);
    let shown = terminal.wait_for("/cli?code=");
    assert!(
        shown.contains("Confirm this code in your browser to sign in: ")
            && !shown.contains("/cli/authorize"),
        "{shown}"
    );
    terminal.stop();
}

#[test]
fn the_short_code_page_signs_in_for_the_organization_it_showed_and_for_no_other() {
    let hosted = Hosted::start();
    let (status, started) = hosted.json(
        "/api/cli/sign-in",
        json!({"computer": "ada-laptop", "refresh": true}),
    );
    assert_eq!(status, 200, "{started}");
    let (code, user_code) = (
        started["code"].as_str().unwrap().to_owned(),
        started["userCode"].as_str().unwrap().to_owned(),
    );
    let (status, _, page) = hosted.page(&format!("/cli?code={user_code}"));
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("name=\"org\" value=\"aaaa0001\""), "{page}");
    let ask = |org: &str| {
        hosted.post_form(
            "/ui/cli",
            SESSION,
            &[
                ("csrf", CSRF),
                ("code", &user_code),
                ("org", org),
                ("decision", "allow"),
            ],
        )
    };
    // The page showed one organization; a form arriving with another, or none, signs nothing in.
    for org in ["bbbb0002", ""] {
        let (_, _, answer) = ask(org);
        assert!(
            answer.contains("Your organization changed while this page was open"),
            "{org:?}: {answer}"
        );
        let (_, collected) = hosted.json("/api/cli/sign-in/token", json!({"code": code}));
        assert_eq!(collected["status"], "waiting", "{org:?}: {collected}");
    }
    // The one it showed is signed in, and it is the one the token is for.
    let (status, _, answer) = ask("aaaa0001");
    assert_eq!(status, 200, "{answer}");
    let (_, collected) = hosted.json("/api/cli/sign-in/token", json!({"code": code}));
    assert_eq!(collected["status"], "allowed", "{collected}");
    assert_eq!(collected["organization"], "Ada Analytics");
}

#[test]
fn signing_out_tells_the_browser_to_clear_what_the_app_kept_in_it() {
    // An editor's unsaved drafts live in the browser's storage for the app; they belong to the
    // person who is leaving.
    let hosted = Hosted::start();
    let (status, head, _) = hosted.post_form("/ui/sign-out", SESSION, &[("csrf", CSRF)]);
    assert!(matches!(status, 200 | 303), "{status}: {head}");
    assert_eq!(
        header(&head, "clear-site-data").as_deref(),
        Some("\"storage\""),
        "{head}"
    );
}
