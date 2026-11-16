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

/// One authenticated request. A success returns the JSON body; a failure returns the server's own
/// findings as JSON-encoded diagnostics when it sent any (TYP-54), else one sentence.
pub async fn send(
    server: &reqwest::Url,
    token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, String> {
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
    if publish && status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        let request_mib = request_bytes as f64 / (1024.0 * 1024.0);
        return Err(format!(
            "Request failed: {status} — Your report's publish request is {request_mib:.1} MiB. Hosted RichView limits each publish request to 32 MiB. Use smaller data or link large data."
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if bytes.len() + chunk.len() > MAX_INPUT_BYTES {
            return Err("Server response exceeded size limit".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    if status.is_success() {
        return serde_json::from_slice(&bytes).map_err(|e| e.to_string());
    }
    let text = String::from_utf8_lossy(&bytes).replace(token, "[redacted]");
    if let Some(diagnostics) = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<Diagnostic>>(v["diagnostics"].clone()).ok())
        .filter(|d| !d.is_empty())
    {
        return Err(serde_json::to_string(&diagnostics).unwrap_or_default());
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(
            "The server did not accept this token. Check RICHVIEW_TOKEN or --token, or create a new token on the server's CLI tokens page."
                .into(),
        );
    }
    // A server's own error object says what went wrong in its `error` field; print the words.
    let said = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_owned))
        .unwrap_or(text);
    Err(format!(
        "Request failed: {status} — {}",
        said.chars().take(200).collect::<String>()
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
        use axum::{Router, http::StatusCode};
        let app = Router::new().fallback(|| async {
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
