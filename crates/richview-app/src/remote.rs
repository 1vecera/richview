//! Calls to a RichView server with the owner's token: publishing and the owner actions. HTTPS,
//! or HTTP on loopback only; no redirects; a bounded response; the token never appears in an error.
use crate::MAX_INPUT_BYTES;
use richview_core::document::Diagnostic;
use serde_json::Value;

/// The server's base URL, credential-free and safe to send a bearer token to.
pub fn server_url(server: &str) -> Result<reqwest::Url, String> {
    let server = reqwest::Url::parse(server).map_err(|e| e.to_string())?;
    if !["https", "http"].contains(&server.scheme())
        || !server.username().is_empty()
        || server.password().is_some()
        || server.query().is_some()
        || server.fragment().is_some()
    {
        return Err("Server URL must be credential-free HTTP(S) without query or fragment".into());
    }
    if server.scheme() == "http"
        && !matches!(
            server.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
    {
        return Err(
            "Publishing with a bearer token requires HTTPS (HTTP permitted on loopback only)"
                .into(),
        );
    }
    Ok(server)
}

/// What a refusal of the token starts with, so that a caller can tell it from other failures.
pub(crate) const NOT_ACCEPTED: &str = "The server did not accept this token.";

/// One authenticated request. A success returns the JSON body; a failure returns the server's own
/// findings as JSON-encoded diagnostics when it sent any (TYP-54), else one sentence.
///
/// A token the server does not accept that is this computer's own renewable sign-in is renewed
/// once and the request goes again: the sign-in's end is a time the server may count otherwise.
pub async fn send(
    server: &reqwest::Url,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let first = send_once(server, token, method.clone(), path, body).await;
    let Err(refused) = &first else {
        return first;
    };
    if !refused.starts_with(NOT_ACCEPTED) {
        return first;
    }
    match crate::credentials::renew_refused(server.as_str(), token).await {
        Ok(Some(next)) => send_once(server, &next, method, path, body).await,
        _ => first,
    }
}

/// How many times a request the server answered "busy, ask again in a few seconds" goes again.
const BUSY_RETRIES: usize = 3;

/// The server's own request for a pause: it names the seconds in `Retry-After` when its workers
/// are busy, and only then, so a limit that does not lift in seconds is never waited out.
fn busy_pause(response: &reqwest::Response) -> Option<std::time::Duration> {
    if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return None;
    }
    let seconds = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(std::time::Duration::from_secs(seconds.clamp(1, 10)))
}

async fn send_once(
    server: &reqwest::Url,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
    let mut retries = 0;
    loop {
        match send_attempt(server, token, method.clone(), path, body).await {
            Err(Attempt::Busy(pause)) if retries < BUSY_RETRIES => {
                retries += 1;
                tokio::time::sleep(pause).await;
            }
            Err(Attempt::Busy(_)) => {
                return Err(
                    "Request failed: 429 Too Many Requests — RichView is busy; try again in a few seconds."
                        .into(),
                );
            }
            Err(Attempt::Failed(message)) => return Err(message),
            Ok(value) => return Ok(value),
        }
    }
}

/// Why one attempt at a request did not return its answer.
enum Attempt {
    /// The server asked for a pause and another try.
    Busy(std::time::Duration),
    Failed(String),
}

impl From<String> for Attempt {
    fn from(message: String) -> Self {
        Self::Failed(message)
    }
}

async fn send_attempt(
    server: &reqwest::Url,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, Attempt> {
    let endpoint = server.join(path).map_err(|e| e.to_string())?;
    let publish = method == reqwest::Method::POST && path == "/api/reports";
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?;
    let request_bytes = body.as_ref().map_or(0, Vec::len);
    // The server would refuse this with a bare 413 after reading it all; say the ceiling first.
    if publish && request_bytes > crate::input_limits::REQUEST_LIMIT {
        return Err(
            crate::input_limits::encoded(crate::input_limits::request_too_large(request_bytes))
                .into(),
        );
    }
    let mut request = client.request(method, endpoint).bearer_auth(token);
    if let Some(body) = body {
        request = request
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let status = response.status();
    if let Some(pause) = busy_pause(&response) {
        return Err(Attempt::Busy(pause));
    }
    if publish && status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        let request_mib = request_bytes as f64 / (1024.0 * 1024.0);
        return Err(format!(
            "Request failed: {status} — Your report's publish request is {request_mib:.1} MiB. Hosted RichView limits each publish request to 32 MiB. Use smaller data or link large data."
        )
        .into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_INPUT_BYTES {
            return Err(String::from("Server response exceeded size limit").into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if status.is_success() {
        return Ok(serde_json::from_slice(&bytes).map_err(|e| e.to_string())?);
    }
    let text = String::from_utf8_lossy(&bytes).replace(token, "[redacted]");
    if let Some(diagnostics) = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<Diagnostic>>(v["diagnostics"].clone()).ok())
        .filter(|d| !d.is_empty())
    {
        return Err(serde_json::to_string(&diagnostics)
            .unwrap_or_default()
            .into());
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(format!(
            "{NOT_ACCEPTED} Run richview login again, or check RICHVIEW_TOKEN or --token; a new token comes from the server's CLI tokens page."
        )
        .into());
    }
    // A server's own error object says what went wrong in its `error` field; print the words.
    let said = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_owned))
        .unwrap_or(text);
    Err(format!(
        "Request failed: {status} — {}",
        said.chars().take(200).collect::<String>()
    )
    .into())
}

/// One request that may carry no token, answered with the status and the JSON body whatever the
/// status is: for the sign-in itself, where a refusal is an answer to read and not a failure.
/// No redirects and a small bounded response; an error never names the address or a token.
pub(crate) async fn call(
    server: &reqwest::Url,
    token: Option<&str>,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<(reqwest::StatusCode, Value), String> {
    const MAX_ANSWER_BYTES: usize = 64 * 1024;
    let endpoint = server.join(path).map_err(|e| e.to_string())?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let mut request = client.request(method, endpoint);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    if let Some(body) = body {
        request = request.json(body);
    }
    let mut response = request
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    let status = response.status();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| e.without_url().to_string())?
    {
        if bytes.len() + chunk.len() > MAX_ANSWER_BYTES {
            return Err("Server response exceeded size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_urls_refuse_credentials_queries_fragments_and_non_loopback_http() {
        for url in [
            "https://reports.example.test",
            "http://localhost:3000",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
        ] {
            assert!(server_url(url).is_ok(), "{url}");
        }
        for url in [
            "relative/path",
            "ftp://reports.example.test",
            "http://reports.example.test",
            "https://user:password@reports.example.test",
            "https://reports.example.test?token=x",
            "https://reports.example.test?",
            "https://reports.example.test#report",
            "https://reports.example.test#",
        ] {
            assert!(
                server_url(url).is_err(),
                "unsafe destination accepted: {url}"
            );
        }
    }

    #[tokio::test]
    async fn publish_explains_a_proxy_413_without_relabeling_other_requests() {
        use axum::{Router, body::Body, http::StatusCode};
        // The stand-in proxy reads the whole request before it answers, as nginx's lingering close
        // does. Answering first, hyper reads only what has arrived and closes the connection; a
        // client the scheduler has not run meanwhile then writes the rest of its body into the
        // closed connection and fails with "error sending request" (broken pipe), never seeing the
        // 413. That is how this test flaked under load.
        let app = Router::new().fallback(|body: Body| async move {
            let _ = axum::body::to_bytes(body, usize::MAX).await;
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                "Proxy says the request is too large",
            )
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = server_url(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let body = serde_json::json!({"source": "x".repeat(1_500_000)});
        let publish = send(
            &server,
            "test-token",
            reqwest::Method::POST,
            "/api/reports",
            Some(&body),
        )
        .await
        .unwrap_err();
        assert!(
            publish.contains("32 MiB") && publish.contains("link large data"),
            "{publish}"
        );
        assert!(publish.contains("is 1.4 MiB"), "{publish}");
        let other = send(
            &server,
            "test-token",
            reqwest::Method::POST,
            "/api/validate",
            None,
        )
        .await
        .unwrap_err();
        assert!(
            other.contains("Proxy says the request is too large"),
            "{other}"
        );
        assert!(!other.contains("32 MiB"), "{other}");
        worker.abort();
    }

    #[tokio::test]
    async fn a_busy_server_is_asked_again_when_it_names_the_seconds_and_only_then() {
        use axum::{Router, http::StatusCode, response::IntoResponse, routing::post};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let busy_calls = Arc::new(AtomicUsize::new(0));
        let limited_calls = Arc::new(AtomicUsize::new(0));
        let app = Router::new()
            .route(
                "/api/busy",
                post({
                    let calls = busy_calls.clone();
                    move || async move {
                        if calls.fetch_add(1, Ordering::SeqCst) == 0 {
                            (
                                StatusCode::TOO_MANY_REQUESTS,
                                [("retry-after", "1")],
                                "busy",
                            )
                                .into_response()
                        } else {
                            axum::Json(serde_json::json!({"ok": true})).into_response()
                        }
                    }
                }),
            )
            .route(
                "/api/limited",
                post({
                    let calls = limited_calls.clone();
                    move || async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        (StatusCode::TOO_MANY_REQUESTS, "An hour's limit").into_response()
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = server_url(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let answered = send(&server, "tok-9z", reqwest::Method::POST, "/api/busy", None)
            .await
            .unwrap();
        assert_eq!(answered["ok"], true);
        assert_eq!(busy_calls.load(Ordering::SeqCst), 2);
        // A limit that does not lift in seconds names no pause, and is reported at once.
        let limited = send(
            &server,
            "tok-9z",
            reqwest::Method::POST,
            "/api/limited",
            None,
        )
        .await
        .unwrap_err();
        assert!(limited.contains("An hour's limit"), "{limited}");
        assert_eq!(limited_calls.load(Ordering::SeqCst), 1);
        worker.abort();
    }

    #[tokio::test]
    async fn publish_refuses_a_request_over_the_limit_before_sending_it() {
        // Nothing listens here: a request that were sent would fail to connect instead.
        let server = server_url("http://127.0.0.1:1").unwrap();
        let limit = crate::input_limits::REQUEST_LIMIT;
        let big = serde_json::json!({"source": "x".repeat(limit)});
        let refused = send(
            &server,
            "t",
            reqwest::Method::POST,
            "/api/reports",
            Some(&big),
        )
        .await
        .unwrap_err();
        let found: Vec<Diagnostic> = serde_json::from_str(&refused).expect(&refused);
        assert_eq!(found[0].code, "report.input-limit");
        assert!(
            found[0].message.contains("up to 32 MiB (33,554,432 bytes)")
                && found[0]
                    .message
                    .contains("about 25,000,000 bytes of linked files"),
            "{}",
            found[0].message
        );
        // A small request goes out, and a request to another route is not judged here.
        let small = serde_json::json!({"source": "x"});
        let sent = send(
            &server,
            "t",
            reqwest::Method::POST,
            "/api/reports",
            Some(&small),
        )
        .await
        .unwrap_err();
        assert!(!sent.contains("report.input-limit"), "{sent}");
        let other = send(
            &server,
            "t",
            reqwest::Method::POST,
            "/api/check",
            Some(&big),
        )
        .await
        .unwrap_err();
        assert!(!other.contains("report.input-limit"), "{other}");
    }

    #[tokio::test]
    async fn a_refused_token_and_a_server_error_are_said_in_words() {
        use axum::{Json, Router, http::StatusCode, routing::post};
        let app = Router::new()
            .route(
                "/api/reports",
                post(|| async {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(serde_json::json!({"diagnostics":[],"error":"Unauthorized","ok":false,"output":null,"status":"issues"})),
                    )
                }),
            )
            .route(
                "/api/validate",
                post(|| async {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(serde_json::json!({"diagnostics":[],"error":"Storage is busy","ok":false})),
                    )
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = server_url(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let worker = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let post = |path: &'static str| {
            let server = server.clone();
            async move {
                send(&server, "wrong-token", reqwest::Method::POST, path, None)
                    .await
                    .unwrap_err()
            }
        };
        let refused = post("/api/reports").await;
        assert!(
            refused.starts_with("The server did not accept this token."),
            "{refused}"
        );
        assert!(!refused.contains('{'), "{refused}");
        assert_eq!(
            post("/api/validate").await,
            "Request failed: 503 Service Unavailable — Storage is busy"
        );
        worker.abort();
    }

    #[tokio::test]
    async fn authenticated_requests_refuse_every_redirect_without_following_or_leaking_the_token() {
        use axum::{Json, Router, http::StatusCode, routing::post};
        use std::sync::{
            Arc,
            atomic::{AtomicU16, AtomicUsize, Ordering},
        };

        let followed = Arc::new(AtomicUsize::new(0));
        let received = followed.clone();
        let target = Router::new().fallback(move || {
            let received = received.clone();
            async move {
                received.fetch_add(1, Ordering::SeqCst);
                Json(serde_json::json!({"ok": true}))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let destination = format!("http://{}/must-not-receive", listener.local_addr().unwrap());
        let target = tokio::spawn(async move { axum::serve(listener, target).await.unwrap() });

        let status = Arc::new(AtomicU16::new(301));
        let reply_status = status.clone();
        let redirect = Router::new().route(
            "/api/reports",
            post(move |headers: axum::http::HeaderMap| {
                let reply_status = reply_status.clone();
                let destination = destination.clone();
                async move {
                    assert_eq!(headers["authorization"], "Bearer publish-fixture-token");
                    (
                        StatusCode::from_u16(reply_status.load(Ordering::SeqCst)).unwrap(),
                        [(axum::http::header::LOCATION, destination)],
                        "Reflected publish-fixture-token",
                    )
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server = server_url(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let redirect = tokio::spawn(async move { axum::serve(listener, redirect).await.unwrap() });
        for code in [301, 302, 303, 307, 308] {
            status.store(code, Ordering::SeqCst);
            let result = send(
                &server,
                "publish-fixture-token",
                reqwest::Method::POST,
                "/api/reports",
                Some(&serde_json::json!({"source": "# Local report"})),
            )
            .await;
            assert!(result.is_err(), "redirect {code} succeeded: {result:?}");
            assert_eq!(followed.load(Ordering::SeqCst), 0, "followed {code}");
            let error = result.unwrap_err();
            assert!(
                error.starts_with(&format!("Request failed: {code} ")),
                "{error}"
            );
            assert!(error.contains("Reflected [redacted]"), "{error}");
            assert!(!error.contains("publish-fixture-token"));
        }
        redirect.abort();
        target.abort();
    }
}
