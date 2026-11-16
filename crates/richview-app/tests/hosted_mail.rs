//! Hosted mail never needs a provider for its queue and fallback tests.
use richview_app::mail::{Mail, MailQueue, Outbox, Transport};
use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Retry {
    attempts: AtomicUsize,
}
impl Transport for Retry {
    fn send<'a>(
        &'a self,
        _: &'a Mail,
    ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
        Box::pin(async move {
            if self.attempts.fetch_add(1, Ordering::SeqCst) < 2 {
                Err(())
            } else {
                Ok(())
            }
        })
    }
}
fn message() -> Mail {
    Mail {
        to: "person@example.test".into(),
        subject: "RichView".into(),
        body: "https://app.test/link/secret".into(),
        html: None,
    }
}

#[tokio::test]
async fn budget_survives_restart_and_worker_retries() {
    let dir = tempfile::tempdir().unwrap();
    let sender = Arc::new(Retry {
        attempts: AtomicUsize::new(0),
    });
    let queue = MailQueue::start(sender.clone(), dir.path(), 1).unwrap();
    queue.enqueue(message()).await.unwrap();
    assert!(queue.exhausted());
    assert!(queue.enqueue(message()).await.is_err());
    let reopened = MailQueue::start(sender.clone(), dir.path(), 1).unwrap();
    assert!(reopened.exhausted());
    assert!(reopened.enqueue(message()).await.is_err());
    tokio::time::timeout(std::time::Duration::from_secs(6), async {
        while sender.attempts.load(Ordering::SeqCst) < 3 {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(sender.attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn outbox_contains_the_message_only_in_private_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("outbox");
    let outbox = Outbox::open(&path).unwrap();
    outbox.send(&message()).await.unwrap();
    let files = std::fs::read_dir(&path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    assert!(
        std::fs::read_to_string(&files[0])
            .unwrap()
            .contains("https://app.test/link/secret")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let mut invalid = message();
    invalid.to.push_str("\nBcc: other@example.test");
    assert!(outbox.send(&invalid).await.is_err());
}

#[tokio::test]
async fn smtp_refuses_to_send_to_a_relay_without_starttls() {
    use richview_app::mail::Smtp;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let relay = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b"220 localhost ESMTP\r\n").await.unwrap();
        let mut socket = BufReader::new(socket);
        let mut line = String::new();
        socket.read_line(&mut line).await.unwrap();
        assert!(line.starts_with("EHLO "));
        socket
            .get_mut()
            .write_all(b"250 localhost\r\n")
            .await
            .unwrap();
        line.clear();
        let _ = socket.read_line(&mut line).await;
        assert!(!line.starts_with("MAIL FROM"));
        assert!(!line.contains("secret"));
    });
    let smtp = Smtp::new(
        "localhost",
        port,
        "user".into(),
        "password".into(),
        "RichView <sender@example.test>".into(),
        Some(richview_app::mail::SmtpTls::StartTls),
    )
    .unwrap();
    assert!(smtp.send(&message()).await.is_err());
    tokio::time::timeout(std::time::Duration::from_secs(5), relay)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn delivery_mode_is_available_for_the_check_mail_page() {
    let dir = tempfile::tempdir().unwrap();
    let access = richview_app::access::Access::open_hosted(dir.path()).unwrap();
    assert_eq!(access.manual_outbox(), None);
    let outbox = Outbox::open(&dir.path().join("outbox")).unwrap();
    access.configure_mail(Arc::new(outbox), 10, true).unwrap();
    assert_eq!(access.manual_outbox(), Some(true));
    assert!(access.mail_queue().is_some());
}

#[tokio::test]
async fn a_rate_limited_manual_request_does_not_hold_up_another_address() {
    use richview_app::mail::{ManualKind, ManualRequest};
    struct ManualRefusal {
        refused: AtomicUsize,
        delivered: AtomicUsize,
    }
    impl Transport for ManualRefusal {
        fn send<'a>(
            &'a self,
            _: &'a Mail,
        ) -> Pin<Box<dyn Future<Output = Result<(), ()>> + Send + 'a>> {
            Box::pin(async { Ok(()) })
        }
        fn write_manual(&self, request: &ManualRequest) -> Result<(), String> {
            if request.to == "blocked@example.test" {
                self.refused.fetch_add(1, Ordering::SeqCst);
                Err("Manual request limit reached".into())
            } else {
                self.delivered.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let sender = Arc::new(ManualRefusal {
        refused: AtomicUsize::new(0),
        delivered: AtomicUsize::new(0),
    });
    let queue = MailQueue::start(sender.clone(), dir.path(), 10).unwrap();
    queue
        .enqueue_manual("blocked@example.test".into(), ManualKind::Signup, true)
        .await
        .unwrap();
    queue
        .enqueue_manual("other@example.test".into(), ManualKind::Signup, true)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while sender.delivered.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(sender.refused.load(Ordering::SeqCst), 1);
}
