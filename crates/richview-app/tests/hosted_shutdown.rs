//! Hosted process shutdown on a loopback-only server with disposable state and outbox.
#![cfg(all(unix, feature = "embedded-runtime"))]

use std::{net::TcpListener, process::Stdio, time::Duration};

#[tokio::test]
async fn sigterm_stops_all_hosted_listeners_and_exits() {
    let root = std::env::current_dir().unwrap().join("tmp");
    std::fs::create_dir_all(&root).unwrap();
    let dir = tempfile::tempdir_in(root).unwrap();
    let ports: Vec<_> = (0..3)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let [app, report, operator] = [0, 1, 2].map(|index| ports[index].local_addr().unwrap().port());
    drop(ports);
    let token = concat!("operator-fixture-", "0123456789abcdef");
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_richview"))
        .args(["server", "--hosted", "--bind", "127.0.0.1"])
        .args([
            "--port",
            &app.to_string(),
            "--report-port",
            &report.to_string(),
        ])
        .args(["--operator-port", &operator.to_string()])
        .args([
            "--public-url",
            "https://app.test",
            "--report-origin",
            "https://reports.test",
        ])
        .args(["--quota-bytes", "104857600", "--data-dir"])
        .arg(dir.path().join("data"))
        .arg("--mail-outbox")
        .arg(dir.path().join("outbox"))
        .env_clear()
        .env("RICHVIEW_OPERATOR_TOKEN", token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let url = format!("http://127.0.0.1:{operator}/operator/summary");
    let summary = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(response) = client.get(&url).bearer_auth(token).send().await
                && response.status().is_success()
            {
                break response.json::<serde_json::Value>().await.unwrap();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(summary["mailsWaiting"], 0);
    assert_eq!(summary["mailsSentTotal"], 0);
    let pid = child.id().unwrap() as libc::pid_t;
    // SAFETY: this PID belongs to the disposable server started by this test.
    assert_eq!(unsafe { libc::kill(pid, libc::SIGTERM) }, 0);
    let status = tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .unwrap()
        .unwrap();
    assert!(
        status.success(),
        "server did not shut down cleanly: {status}"
    );
    assert!(client.get(&url).bearer_auth(token).send().await.is_err());
}
