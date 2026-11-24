//! Hosted mode must refuse unsafe configuration before opening its data directory.
use std::{path::PathBuf, process::Command};

#[test]
fn hosted_startup_requires_https_origins_limits_mail_and_no_service_token() {
    let root = std::env::current_dir()
        .unwrap()
        .join("tmp")
        .join(format!("hosted-startup-{}", std::process::id()));
    let data: PathBuf = root.join("data");
    let outbox = root.join("outbox");
    let run = |extra: &[&str]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_richview"));
        cmd.args(["server", "--hosted", "--data-dir"])
            .arg(&data)
            .args([
                "--public-url",
                "https://app.test",
                "--report-origin",
                "https://reports.test",
                "--mail-outbox",
            ])
            .arg(&outbox)
            .env_remove("RICHVIEW_TOKEN")
            .env_remove("RICHVIEW_ADMIN_EMAIL")
            .env_remove("RICHVIEW_QUOTA_BYTES")
            .env_remove("RICHVIEW_DEFAULT_VISIBILITY")
            .env_remove("RICHVIEW_ORG_QUOTA_BYTES")
            .env_remove("RICHVIEW_ORG_REPORT_LIMIT")
            .env_remove("RICHVIEW_MAIL_DAILY_LIMIT")
            .env_remove("RICHVIEW_SIGNUP_ACCOUNT_LIMIT")
            .env_remove("RICHVIEW_OPERATOR_PORT")
            .env_remove("RICHVIEW_OPERATOR_TOKEN")
            .env_remove("RICHVIEW_SMTP_HOST");
        cmd.args(extra).output().unwrap()
    };
    let missing = run(&[]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("limits"));
    assert!(!data.exists());
    let public_bind = run(&["--quota-bytes", "104857600", "--bind", "0.0.0.0"]);
    assert!(!public_bind.status.success());
    assert!(String::from_utf8_lossy(&public_bind.stderr).contains("loopback"));
    assert!(!data.exists());
    let service = run(&["--quota-bytes", "104857600", "--token", "local-test-token"]);
    assert!(!service.status.success());
    assert!(String::from_utf8_lossy(&service.stderr).contains("service token"));
    assert!(!String::from_utf8_lossy(&service.stderr).contains("local-test-token"));
    assert!(!data.exists());
    let public = run(&[
        "--quota-bytes",
        "104857600",
        "--default-visibility",
        "public",
    ]);
    assert!(!public.status.success());
    assert!(String::from_utf8_lossy(&public.stderr).contains("private"));
    assert!(!data.exists());
    let two_transports = run(&["--quota-bytes", "104857600", "--smtp-host", "localhost"]);
    assert!(!two_transports.status.success());
    assert!(String::from_utf8_lossy(&two_transports.stderr).contains("one mail transport"));
    assert!(!data.exists());
    let no_budget = run(&["--quota-bytes", "104857600", "--mail-daily-limit", "0"]);
    assert!(!no_budget.status.success());
    assert!(String::from_utf8_lossy(&no_budget.stderr).contains("daily limit"));
    assert!(!data.exists());
    let no_operator = run(&["--quota-bytes", "104857600"]);
    assert!(!no_operator.status.success());
    assert!(String::from_utf8_lossy(&no_operator.stderr).contains("operator-port"));
    let collision = run(&["--quota-bytes", "104857600", "--operator-port", "8080"]);
    assert!(!collision.status.success());
    assert!(String::from_utf8_lossy(&collision.stderr).contains("differ"));
    let invalid_token = run(&["--quota-bytes", "104857600", "--operator-port", "9090"]);
    assert!(!invalid_token.status.success());
    assert!(String::from_utf8_lossy(&invalid_token.stderr).contains("RICHVIEW_OPERATOR_TOKEN"));
    let excessive_accounts = run(&[
        "--quota-bytes",
        "104857600",
        "--signup-account-limit",
        "1025",
    ]);
    assert!(!excessive_accounts.status.success());
    assert!(String::from_utf8_lossy(&excessive_accounts.stderr).contains("account limit"));
    assert!(!data.exists());
    let no_mail = Command::new(env!("CARGO_BIN_EXE_richview"))
        .args(["server", "--hosted", "--data-dir"])
        .arg(&data)
        .args([
            "--public-url",
            "https://app.test",
            "--report-origin",
            "https://reports.test",
            "--quota-bytes",
            "104857600",
        ])
        .env_remove("RICHVIEW_TOKEN")
        .env_remove("RICHVIEW_ADMIN_EMAIL")
        .env_remove("RICHVIEW_MAIL_OUTBOX")
        .env_remove("RICHVIEW_MAIL_DAILY_LIMIT")
        .env_remove("RICHVIEW_SMTP_HOST")
        .env_remove("RICHVIEW_DEFAULT_VISIBILITY")
        .env_remove("RICHVIEW_SIGNUP_ACCOUNT_LIMIT")
        .env_remove("RICHVIEW_ORG_QUOTA_BYTES")
        .env_remove("RICHVIEW_ORG_REPORT_LIMIT")
        .output()
        .unwrap();
    assert!(!no_mail.status.success());
    assert!(String::from_utf8_lossy(&no_mail.stderr).contains("mail transport"));
    assert!(!data.exists());
    let no_smtp_password = Command::new(env!("CARGO_BIN_EXE_richview"))
        .args(["server", "--hosted", "--data-dir"])
        .arg(&data)
        .args([
            "--public-url",
            "https://app.test",
            "--report-origin",
            "https://reports.test",
            "--quota-bytes",
            "104857600",
            "--smtp-host",
            "localhost",
            "--smtp-user",
            "local-user",
            "--mail-from",
            "RichView <sender@example.test>",
        ])
        .env_remove("RICHVIEW_TOKEN")
        .env_remove("RICHVIEW_ADMIN_EMAIL")
        .env_remove("RICHVIEW_MAIL_OUTBOX")
        .env_remove("RICHVIEW_SMTP_PASSWORD")
        .env_remove("RICHVIEW_MAIL_DAILY_LIMIT")
        .env_remove("RICHVIEW_DEFAULT_VISIBILITY")
        .env_remove("RICHVIEW_ORG_QUOTA_BYTES")
        .env_remove("RICHVIEW_ORG_REPORT_LIMIT")
        .output()
        .unwrap();
    assert!(!no_smtp_password.status.success());
    assert!(String::from_utf8_lossy(&no_smtp_password.stderr).contains("RICHVIEW_SMTP_PASSWORD"));
    assert!(!data.exists());
    let insecure = Command::new(env!("CARGO_BIN_EXE_richview"))
        .args(["server", "--hosted", "--data-dir"])
        .arg(&data)
        .args([
            "--public-url",
            "http://app.test",
            "--report-origin",
            "https://reports.test",
            "--quota-bytes",
            "104857600",
            "--mail-outbox",
        ])
        .arg(&outbox)
        .env_remove("RICHVIEW_TOKEN")
        .env_remove("RICHVIEW_ADMIN_EMAIL")
        .env_remove("RICHVIEW_DEFAULT_VISIBILITY")
        .env_remove("RICHVIEW_ORG_QUOTA_BYTES")
        .env_remove("RICHVIEW_ORG_REPORT_LIMIT")
        .env_remove("RICHVIEW_MAIL_DAILY_LIMIT")
        .env_remove("RICHVIEW_SMTP_HOST")
        .output()
        .unwrap();
    assert!(!insecure.status.success());
    assert!(String::from_utf8_lossy(&insecure.stderr).contains("HTTPS"));
    assert!(!data.exists());
    // Plain HTTP at a loopback address passes the origin check, says so at the start, and then
    // stops at the next missing setting, the operator port. Hosted mode binds loopback anyway.
    let local = |bind: &str| {
        Command::new(env!("CARGO_BIN_EXE_richview"))
            .args(["server", "--hosted", "--data-dir"])
            .arg(&data)
            .args([
                "--bind",
                bind,
                "--public-url",
                "http://localhost:8080",
                "--report-origin",
                "http://127.0.0.1:8081",
                "--quota-bytes",
                "104857600",
                "--mail-outbox",
            ])
            .arg(&outbox)
            .env_remove("RICHVIEW_TOKEN")
            .env_remove("RICHVIEW_ADMIN_EMAIL")
            .env_remove("RICHVIEW_DEFAULT_VISIBILITY")
            .env_remove("RICHVIEW_ORG_QUOTA_BYTES")
            .env_remove("RICHVIEW_ORG_REPORT_LIMIT")
            .env_remove("RICHVIEW_MAIL_DAILY_LIMIT")
            .env_remove("RICHVIEW_SMTP_HOST")
            .env_remove("RICHVIEW_OPERATOR_PORT")
            .output()
            .unwrap()
    };
    let loopback = local("127.0.0.1");
    let stderr = String::from_utf8_lossy(&loopback.stderr);
    assert!(!loopback.status.success());
    assert!(
        stderr.contains(
            "hosted mode on plain HTTP at http://localhost:8080: for local development only"
        ) && stderr.contains("operator-port")
            && !stderr.contains("requires HTTPS"),
        "{stderr}"
    );
    assert!(!data.exists());
    let everywhere = local("0.0.0.0");
    assert!(!everywhere.status.success());
    assert!(String::from_utf8_lossy(&everywhere.stderr).contains("loopback"));
    assert!(!data.exists());
    let _ = std::fs::remove_dir_all(root);
}

#[cfg(feature = "embedded-runtime")]
#[tokio::test]
async fn hosted_server_starts_with_a_separate_loopback_operator_listener() {
    use axum::http::StatusCode;
    use std::{net::TcpListener, process::Stdio, time::Duration};
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let ports: Vec<_> = (0..3)
        .map(|_| TcpListener::bind("127.0.0.1:0").unwrap())
        .collect();
    let [app, report, operator] = [
        ports[0].local_addr().unwrap().port(),
        ports[1].local_addr().unwrap().port(),
        ports[2].local_addr().unwrap().port(),
    ];
    drop(ports);
    let dir = tempfile::tempdir().unwrap();
    let token = concat!("test-operator-token-", "0123456789abcdef");
    let child = Command::new(env!("CARGO_BIN_EXE_richview"))
        .args([
            "server",
            "--hosted",
            "--bind",
            "127.0.0.1",
            "--port",
            &app.to_string(),
            "--report-port",
            &report.to_string(),
            "--operator-port",
            &operator.to_string(),
            "--public-url",
            "https://app.test",
            "--report-origin",
            "https://reports.test",
            "--quota-bytes",
            "104857600",
            "--data-dir",
        ])
        .arg(dir.path().join("data"))
        .arg("--mail-outbox")
        .arg(dir.path().join("outbox"))
        .env("RICHVIEW_OPERATOR_TOKEN", token)
        .env_remove("RICHVIEW_TOKEN")
        .env_remove("RICHVIEW_ADMIN_EMAIL")
        .env_remove("RICHVIEW_SMTP_HOST")
        .env_remove("RICHVIEW_DEFAULT_VISIBILITY")
        .env_remove("RICHVIEW_MAIL_DAILY_LIMIT")
        .env_remove("RICHVIEW_SIGNUP_ACCOUNT_LIMIT")
        .env_remove("RICHVIEW_ORG_QUOTA_BYTES")
        .env_remove("RICHVIEW_ORG_REPORT_LIMIT")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _child = ChildGuard(child);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let app_url = format!("http://127.0.0.1:{app}");
    let report_url = format!("http://127.0.0.1:{report}");
    let operator_url = format!("http://127.0.0.1:{operator}");
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if client.get(format!("{app_url}/health")).send().await.is_ok()
                && client
                    .get(format!("{operator_url}/operator/organizations"))
                    .send()
                    .await
                    .is_ok()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        client
            .get(format!("{report_url}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{operator_url}/operator/organizations"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        client
            .get(format!("{operator_url}/operator/organizations"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{app_url}/operator/organizations"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{report_url}/operator/organizations"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        client
            .get(format!("{app_url}/sign-up"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
}
