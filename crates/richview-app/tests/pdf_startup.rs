//! The real CLI/CDP process boundary: bounded startup relaunch, descendant cleanup and telemetry.
#![cfg(unix)]

use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

#[test]
#[cfg(target_os = "macos")]
#[ignore = "release: actual Chromium and the macOS system STIX Two Math face are needed for this source-identity control"]
fn a_system_math_face_cannot_hide_a_missing_embedded_face() {
    use base64::Engine;
    use richview_app::export_pdf::{DEFAULT_SETTLE_TIMEOUT, PdfRequest, print_pdf};
    use richview_core::document::{Output as RenderOutput, Page};
    use sha2::Digest;

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let scratch = workspace.join("tmp");
    fs::create_dir_all(&scratch).unwrap();
    let folder = tempfile::tempdir_in(scratch).unwrap();
    let source = "---\ntitle: Math font source\nstyles: [scientific]\n---\n\n$$\\bar{x}=\\frac{1}{n}\\sum_{i=1}^{n}x_i$$\n";
    let file = folder.path().join("report.rmd");
    fs::write(&file, source).unwrap();
    let (document, mut options) =
        richview_app::resolver::resolve_document_options_policy(source, &file, &[], false, false)
            .unwrap();
    options.output = RenderOutput::Pdf;
    let report = richview_core::document::render_document(document, &options).unwrap();

    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(workspace.join("crates/richview-core/assets/font-manifest.json"))
            .unwrap(),
    )
    .unwrap();
    let face = manifest["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|face| face["catalog"] == "fonts-math.css")
        .expect("the math face is shipped in the common manifest");
    let css = fs::read_to_string(workspace.join("assets/fonts-math.css")).unwrap();
    let payload = regex::Regex::new(r"data:font/woff2;base64,([A-Za-z0-9+/=]+)")
        .unwrap()
        .captures(&css)
        .unwrap()
        .get(1)
        .unwrap()
        .as_str();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .unwrap();
    assert_eq!(
        format!("{:x}", sha2::Sha256::digest(bytes)),
        face["sha256"].as_str().unwrap()
    );
    assert!(
        report.html.contains(payload),
        "the pinned math payload is embedded"
    );
    let font_rule = regex::Regex::new(r"@font-face\s*\{[^}]+\}")
        .unwrap()
        .find_iter(&report.html)
        .find(|rule| rule.as_str().contains(payload))
        .unwrap()
        .as_str();
    let missing_face = report.html.replacen(font_rule, "", 1);
    assert!(!missing_face.contains(payload));
    let browser = std::env::var_os("RICHVIEW_BROWSER").map(PathBuf::from);
    let print = |html| {
        print_pdf(&PdfRequest {
            html,
            page: Page::A4,
            slides: false,
            browser: browser.clone(),
            settle_timeout: DEFAULT_SETTLE_TIMEOUT,
        })
        .unwrap()
    };
    let bundled = print(&report.html);
    assert!(
        !bundled
            .diagnostics
            .iter()
            .any(|d| d.code == "export.font-fallback" || d.code == "export.font-check"),
        "{:?}",
        bundled.diagnostics
    );
    let fallback = print(&missing_face);
    assert!(
        !fallback
            .diagnostics
            .iter()
            .any(|d| d.code == "export.font-check")
    );
    let family = face["family"].as_str().unwrap();
    assert!(
        fallback
            .diagnostics
            .iter()
            .any(|d| d.code == "export.font-fallback" && d.message.contains(family)),
        "same-name system fallback must be reported: {:?}",
        fallback.diagnostics
    );
}

struct Fixture {
    root: tempfile::TempDir,
    journal: PathBuf,
}

impl Fixture {
    fn new(timeouts: usize, failure: &str) -> Self {
        let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
        fs::create_dir_all(&scratch).unwrap();
        let root = tempfile::tempdir_in(scratch).unwrap();
        fs::create_dir_all(root.path().join("report/Rendered")).unwrap();
        fs::write(root.path().join("report/report.rmd"), "# Browser startup\n").unwrap();
        fs::write(
            root.path().join("report/Rendered/report.pdf"),
            b"previous PDF",
        )
        .unwrap();
        fs::write(
            root.path().join("scenario.json"),
            json!({ "timeouts": timeouts, "failure": failure }).to_string(),
        )
        .unwrap();
        let browser = root.path().join("browser.cjs");
        // Host reason (ARC-03): this fixture speaks real NUL-framed CDP and spawns an actual
        // pipe-holding descendant. Retry policy and cleanup belong exclusively to RichView.
        fs::write(&browser, BROWSER).unwrap();
        fs::set_permissions(browser, fs::Permissions::from_mode(0o700)).unwrap();
        let journal = std::env::var_os("RICHVIEW_BROWSER_EVENT_LOG")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.path().join("events.jsonl"));
        Self { root, journal }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_richview"));
        command
            .args(["generate"])
            .arg(self.root.path().join("report/report.rmd"))
            .args(["--to", "pdf", "--format", "json", "--browser"])
            .arg(self.root.path().join("browser.cjs"))
            .current_dir(self.root.path())
            .env("RICHVIEW_BROWSER_EVENT_LOG", &self.journal);
        command
    }

    fn run(&self) -> (u32, Output) {
        let child = self
            .command()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        (pid, child.wait_with_output().unwrap())
    }

    fn records(&self) -> Vec<Value> {
        fs::read_to_string(self.root.path().join("launches.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn assert_stopped(&self) {
        for record in self.records() {
            for key in ["pid", "descendant"] {
                let Some(pid) = record[key].as_i64() else {
                    continue;
                };
                // SAFETY: signal zero only checks a PID recorded by our own fixture.
                assert_eq!(unsafe { libc::kill(pid as libc::pid_t, 0) }, -1, "{record}");
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ESRCH),
                    "{record}"
                );
            }
            let profile = PathBuf::from(record["profile"].as_str().unwrap());
            assert!(
                !profile.parent().unwrap().exists(),
                "profile was left behind"
            );
        }
        assert_eq!(
            fs::read(self.root.path().join("report/Rendered/report.pdf")).unwrap(),
            b"previous PDF"
        );
    }

    fn assert_event(&self, pid: u32, output: &Output, expected: bool) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let markers: Vec<_> = stderr
            .lines()
            .filter_map(|line| line.strip_prefix("RICHVIEW_BROWSER_RELAUNCH "))
            .collect();
        let journal = fs::read_to_string(&self.journal).unwrap_or_default();
        let lines: Vec<_> = journal
            .lines()
            .filter(|line| serde_json::from_str::<Value>(line).unwrap()["pid"] == pid)
            .collect();
        assert_eq!(markers.len(), usize::from(expected), "{stderr}");
        assert_eq!(lines.len(), usize::from(expected), "{journal}");
        if expected {
            let event = json!({"version":1,"event":"browser-startup-relaunch","surface":"pdf","engine":"chromium","attempt":2,"maxAttempts":2,"timeoutMs":15000,"pid":pid});
            assert_eq!(serde_json::from_str::<Value>(lines[0]).unwrap(), event);
            assert_eq!(markers[0], lines[0]);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // The baseline regression deliberately leaves a descendant. Clean only fixture PIDs
        // whose current command still contains this unique private path, even after a panic.
        let token = self.root.path().display().to_string();
        for record in self.records() {
            for key in ["pid", "descendant"] {
                let Some(pid) = record[key].as_i64() else {
                    continue;
                };
                let identity = Command::new("ps")
                    .args(["-p", &pid.to_string(), "-o", "command="])
                    .output();
                if identity
                    .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&token))
                {
                    // SAFETY: the exact PID and unique fixture command were checked above.
                    unsafe { libc::kill(pid as libc::pid_t, libc::SIGKILL) };
                }
            }
        }
    }
}

#[test]
#[ignore = "release: exercises the real 15 s browser startup deadline and embedded CLI (ARC-07)"]
fn startup_timeout_relaunches_once_only_after_its_descendant_is_gone() {
    let fixture = Fixture::new(1, "post-start");
    let started = Instant::now();
    let (pid, output) = fixture.run();
    assert!(!output.status.success());
    assert!(started.elapsed() >= Duration::from_secs(15));
    let records = fixture.records();
    assert_eq!(
        records.len(),
        2,
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(
        records[1]["priorAlive"], false,
        "relaunch preceded descendant cleanup"
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("fixture post-start failure"));
    fixture.assert_event(pid, &output, true);
    fixture.assert_stopped();
}

#[test]
#[ignore = "release: exercises two real 15 s startup deadlines and embedded CLI (ARC-07)"]
fn a_second_startup_timeout_fails_without_a_third_launch() {
    let fixture = Fixture::new(2, "post-start");
    let started = Instant::now();
    let (pid, output) = fixture.run();
    assert!(!output.status.success());
    assert!(started.elapsed() >= Duration::from_secs(30));
    let records = fixture.records();
    assert_eq!(
        records.len(),
        2,
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(records[1]["priorAlive"], false);
    assert!(String::from_utf8_lossy(&output.stdout).contains("did not start: timed out"));
    fixture.assert_event(pid, &output, true);
    fixture.assert_stopped();
}

#[test]
#[ignore = "release: real embedded CLI is staged by richview-build (ARC-07)"]
fn protocol_exit_and_post_start_failures_never_relaunch() {
    for failure in ["protocol", "exit", "post-start"] {
        let fixture = Fixture::new(0, failure);
        let (pid, output) = fixture.run();
        assert!(!output.status.success());
        assert_eq!(
            fixture.records().len(),
            1,
            "{failure}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        fixture.assert_event(pid, &output, false);
        fixture.assert_stopped();
    }
}

#[test]
#[ignore = "release: real 15 s startup deadline and embedded CLI prove failed telemetry refuses relaunch (ARC-07)"]
fn an_unwritable_relaunch_journal_fails_before_the_second_launch() {
    let fixture = Fixture::new(1, "post-start");
    // An existing directory is a deterministic failed append, including when running as root.
    let output = fixture
        .command()
        .env("RICHVIEW_BROWSER_EVENT_LOG", fixture.root.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(fixture.records().len(), 1);
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("Cannot record browser startup relaunch")
    );
    fixture.assert_stopped();
}

const BROWSER: &str = r##"#!/usr/bin/env node
const fs = require('node:fs');
const { spawn } = require('node:child_process');
const root = __dirname;
const config = JSON.parse(fs.readFileSync(root + '/scenario.json'));
const file = root + '/launches.jsonl';
const previous = fs.existsSync(file) ? fs.readFileSync(file, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
const attempt = previous.length + 1;
const prior = previous.at(-1)?.descendant;
let priorAlive = false;
if (prior) { try { process.kill(prior, 0); priorAlive = true; } catch {} }
const profile = process.argv.find(arg => arg.startsWith('--user-data-dir=')).slice('--user-data-dir='.length);
const record = { attempt, pid: process.pid, descendant: null, priorAlive, profile };
if (attempt <= config.timeouts) {
  const child = spawn(process.execPath, ['-e', "require('node:fs').fstatSync(3); require('node:fs').fstatSync(4); setInterval(() => {}, 1000)", root], { stdio: ['ignore', 'ignore', 'inherit', 3, 4] });
  record.descendant = child.pid;
  fs.appendFileSync(file, JSON.stringify(record) + '\n');
  // The original Chrome leader had exited; the descendant still owns both real CDP pipes.
  process.exit(0);
}
fs.appendFileSync(file, JSON.stringify(record) + '\n');
if (config.failure === 'exit') process.exit(7);
let pending = Buffer.alloc(0);
fs.createReadStream(null, { fd: 3 }).on('data', chunk => {
  pending = Buffer.concat([pending, chunk]);
  let end;
  while ((end = pending.indexOf(0)) >= 0) {
    const request = JSON.parse(pending.subarray(0, end)); pending = pending.subarray(end + 1);
    const result = request.method === 'Browser.getVersion' && config.failure !== 'protocol'
      ? { result: { product: 'Chromium/156.0.0.0', protocolVersion: '1.3', revision: 'fixture' } }
      : request.method === 'Browser.close' ? { result: {} }
      : { error: { message: request.method === 'Browser.getVersion' ? 'timed out' : 'fixture post-start failure' } };
    fs.writeSync(4, JSON.stringify({ id: request.id, ...result }) + '\0');
    if (request.method === 'Browser.close') process.exit(0);
  }
});
"##;
