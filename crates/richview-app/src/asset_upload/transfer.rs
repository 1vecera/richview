//! Presigned requests with retries, backoff with jitter, and checks of stored objects.
use super::{AssetError, Events, IMMUTABLE};
use bytes::Bytes;
use reqwest::{Method, Url, header};
use richview_core::{AssetRef, AssetUse, Visibility};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 4 << 20;

#[derive(Debug, Clone, Copy)]
pub(super) struct Policy {
    /// Attempts per request, including the first.
    pub(super) attempts: u32,
    pub(super) base_delay: Duration,
    pub(super) max_delay: Duration,
    /// Timeout of a request without a body; bodies add `bytes / min_rate` seconds.
    pub(super) timeout: Duration,
    pub(super) min_rate: u64,
    /// Parts in flight.
    pub(super) parallel: usize,
    /// How long a retried request holds its body back, so an early refusal is read cleanly.
    pub(super) body_hold: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            attempts: 5,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(20),
            timeout: Duration::from_secs(60),
            min_rate: 64 * 1024,
            parallel: 4,
            body_hold: Duration::from_secs(1),
        }
    }
}

impl Policy {
    pub(super) fn timeout_for(&self, bytes: u64) -> Duration {
        self.timeout + Duration::from_secs(bytes / self.min_rate.max(1))
    }

    /// Exponential backoff with full jitter.
    fn backoff(&self, attempt: u32) -> Duration {
        let ceiling = self
            .base_delay
            .saturating_mul(1 << attempt.saturating_sub(1).min(16))
            .min(self.max_delay);
        Duration::from_millis(fastrand::u64(0..=ceiling.as_millis() as u64))
    }
}

/// A client that never follows redirects and resolves RFC 6761 `localhost` names to loopback
/// itself. Loopback-only work bypasses proxies.
pub(super) fn http_client(urls: &[&Url]) -> Result<reqwest::Client, AssetError> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("richview/", env!("CARGO_PKG_VERSION")));
    if urls.iter().all(|url| richview_core::is_loopback(url)) {
        builder = builder.no_proxy();
    }
    for url in urls {
        if let Some(host) = url.host_str()
            && (host == "localhost" || host.ends_with(".localhost"))
        {
            builder = builder.resolve_to_addrs(
                host,
                &[
                    (std::net::Ipv4Addr::LOCALHOST, 0).into(),
                    (std::net::Ipv6Addr::LOCALHOST, 0).into(),
                ],
            );
        }
    }
    builder
        .build()
        .map_err(|e| AssetError::transfer(format!("HTTP client: {e}")))
}

pub(super) enum Failure {
    /// Network errors, timeouts, 5xx, 429 and S3's RequestTimeout or BadDigest.
    Retry {
        reason: String,
        after: Option<Duration>,
    },
    /// Any other HTTP error, with the S3 error code when the body had one.
    Status {
        status: u16,
        code: String,
        message: String,
    },
    Fatal(AssetError),
}

impl Failure {
    pub(super) fn into_error(self, what: &str) -> AssetError {
        match self {
            Failure::Retry { reason, .. } => AssetError::transfer(format!("{what}: {reason}")),
            Failure::Status {
                status,
                code,
                message,
            } => AssetError::transfer(format!(
                "{what}: {status} {code}{}",
                match (message.is_empty(), status) {
                    (false, _) => format!(": {message}"),
                    // HEAD replies carry no error document.
                    (true, 403) => ": the store refused the credentials; check the access key, its permissions on the bucket and the system clock".into(),
                    (true, _) => String::new(),
                }
            )),
            Failure::Fatal(error) => error,
        }
    }
}

fn transport(error: reqwest::Error) -> Failure {
    let error = error.without_url();
    if error.is_builder() {
        return Failure::Fatal(AssetError::transfer(format!("request: {error}")));
    }
    let mut cause: &dyn std::error::Error = &error;
    while let Some(source) = cause.source() {
        cause = source;
    }
    Failure::Retry {
        reason: if error.is_timeout() {
            "timeout".into()
        } else {
            format!("network error ({cause})")
        },
        after: None,
    }
}

pub(super) async fn read_limited(mut response: reqwest::Response) -> Result<String, Failure> {
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(Failure::Fatal(AssetError::transfer(
                "The store sent an oversized response",
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

pub(super) fn xml_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let start = xml.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = xml[start..].find(&format!("</{tag}>"))? + start;
    Some(&xml[start..end])
}

pub(super) fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// HEAD response fields that a reference pins.
#[derive(Debug, Default, Serialize)]
pub(super) struct Head {
    status: u16,
    bytes: Option<u64>,
    #[serde(rename = "type")]
    media_type: Option<String>,
    sha256: Option<String>,
    accept_ranges: Option<String>,
    cache_control: Option<String>,
}

impl Head {
    fn from(response: &reqwest::Response) -> Self {
        let text = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        Self {
            status: response.status().as_u16(),
            // The header, not the body length hint: a HEAD response has no body.
            bytes: text("content-length").and_then(|n| n.parse().ok()),
            media_type: text("content-type"),
            sha256: text("x-amz-meta-sha256"),
            accept_ranges: text("accept-ranges"),
            cache_control: text("cache-control"),
        }
    }

    /// Differences from the reference. `stored` also requires the upload's own metadata.
    pub(super) fn problems(&self, asset: &AssetRef, stored: bool) -> Vec<String> {
        let mut problems = Vec::new();
        match self.bytes {
            Some(bytes) if bytes == asset.bytes => {}
            Some(bytes) => problems.push(format!(
                "Content-Length {bytes} differs from bytes {}",
                asset.bytes
            )),
            None => problems.push("Content-Length is missing".into()),
        }
        let essence = self.media_type.as_deref().map(|t| {
            t.split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        });
        match (&self.media_type, essence) {
            (_, Some(essence)) if essence == asset.media_type => {}
            (Some(stored), _) => problems.push(format!(
                "Content-Type {stored} differs from type {}",
                asset.media_type
            )),
            (None, _) => problems.push("Content-Type is missing".into()),
        }
        match &self.sha256 {
            Some(stored_hash) if stored_hash != &asset.sha256 => problems.push(format!(
                "stored sha256 metadata {stored_hash} differs from sha256 {}",
                asset.sha256
            )),
            None if stored => problems.push("the object has no sha256 metadata".into()),
            _ => {}
        }
        let media = matches!(
            AssetUse::of_type(&asset.media_type),
            Some(AssetUse::Video | AssetUse::Audio)
        );
        if media && self.accept_ranges.as_deref() != Some("bytes") {
            problems.push(
                "Accept-Ranges is not bytes, so players cannot seek or load metadata early".into(),
            );
        }
        if stored
            && asset.visibility == Visibility::Public
            && self.cache_control.as_deref() != Some(IMMUTABLE)
        {
            problems.push(format!("Cache-Control is not '{IMMUTABLE}'"));
        }
        problems
    }
}

/// A request body; retries hold it back briefly (see [`Transfer::request`]).
pub(super) struct Payload {
    pub(super) bytes: Bytes,
    pub(super) retry: bool,
}

/// Sends presigned requests with retries; shared by uploads, verification and aborts.
pub(super) struct Transfer<'a> {
    pub(super) client: reqwest::Client,
    pub(super) policy: Policy,
    pub(super) events: Events<'a>,
    pub(super) secrets: Vec<String>,
}

impl Transfer<'_> {
    pub(super) fn redact(&self, text: &str) -> String {
        self.secrets.iter().fold(text.to_owned(), |text, secret| {
            text.replace(secret, "[redacted]")
        })
    }

    pub(super) async fn request(
        &self,
        method: Method,
        url: Url,
        headers: &[(&str, &str)],
        body: Option<Payload>,
        timeout: Duration,
    ) -> Result<reqwest::Response, Failure> {
        let mut request = self.client.request(method, url).timeout(timeout);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        match body {
            Some(Payload {
                bytes,
                retry: false,
            }) => request = request.body(bytes),
            // A store that refuses a request before reading its body closes the connection, so
            // the refusal can arrive as a reset. A retry holds the body back briefly so the
            // refusal is read cleanly, as with `Expect: 100-continue`.
            Some(Payload { bytes, retry: true }) => {
                let (length, hold) = (bytes.len(), self.policy.body_hold);
                let held = futures_util::stream::once(async move {
                    tokio::time::sleep(hold).await;
                    Ok::<_, std::io::Error>(bytes)
                });
                request = request
                    .header(header::CONTENT_LENGTH, length)
                    .body(reqwest::Body::wrap_stream(held));
            }
            None => {}
        }
        let response = request.send().await.map_err(transport)?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let after = response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok())
            .map(Duration::from_secs);
        let date = response
            .headers()
            .get(header::DATE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let body = read_limited(response).await.unwrap_or_default();
        let code = xml_text(&body, "Code").unwrap_or_default().to_owned();
        let message = self.redact(&unescape(xml_text(&body, "Message").unwrap_or_default()));
        let label = if code.is_empty() {
            status.canonical_reason().unwrap_or_default().to_owned()
        } else {
            code.clone()
        };
        if status.is_server_error()
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || matches!(code.as_str(), "RequestTimeout" | "BadDigest" | "SlowDown")
        {
            return Err(Failure::Retry {
                reason: format!("{} {label}", status.as_u16()),
                after,
            });
        }
        if code == "RequestTimeTooSkewed" {
            let skew = date
                .and_then(|d| chrono::DateTime::parse_from_rfc2822(&d).ok())
                .map(|d| (chrono::Utc::now() - d.with_timezone(&chrono::Utc)).num_seconds())
                .map_or(String::new(), |s| format!(" by {s} s"));
            return Err(Failure::Fatal(AssetError::transfer(format!(
                "The local clock differs from the store's{skew}; correct the system time and rerun"
            ))));
        }
        Err(Failure::Status {
            status: status.as_u16(),
            code: label,
            message,
        })
    }

    /// Repeats `attempt` (called with the attempt number, from 1) on retryable failures with
    /// backoff, reporting each retry.
    pub(super) async fn retrying<T, F, Fut>(
        &self,
        label: Value,
        mut attempt: F,
    ) -> Result<T, Failure>
    where
        F: FnMut(u32) -> Fut,
        Fut: Future<Output = Result<T, Failure>>,
    {
        let mut n = 1;
        loop {
            match attempt(n).await {
                Err(Failure::Retry { reason, after }) if n < self.policy.attempts => {
                    let wait = self
                        .policy
                        .backoff(n)
                        .max(after.unwrap_or_default())
                        .min(self.policy.max_delay);
                    let mut event = json!({"event": "retry", "attempt": n + 1, "reason": reason, "wait_ms": wait.as_millis() as u64});
                    if let (Some(event), Some(label)) = (event.as_object_mut(), label.as_object()) {
                        event.extend(label.clone());
                    }
                    (self.events)(event);
                    tokio::time::sleep(wait).await;
                    n += 1;
                }
                Err(Failure::Retry { reason, .. }) => {
                    return Err(Failure::Retry {
                        reason: format!("{reason} after {n} attempts"),
                        after: None,
                    });
                }
                result => return result,
            }
        }
    }

    pub(super) async fn head(&self, url: impl Fn() -> Url) -> Result<Option<Head>, Failure> {
        let url = &url;
        self.retrying(json!({"request": "head"}), move |_| async move {
            match self
                .request(Method::HEAD, url(), &[], None, self.policy.timeout)
                .await
            {
                Ok(response) => Ok(Some(Head::from(&response))),
                Err(Failure::Status { status: 404, .. }) => Ok(None),
                Err(failure) => Err(failure),
            }
        })
        .await
    }

    /// Streams an object and returns its SHA-256 and length, reading at most `limit + 1` bytes.
    pub(super) async fn hash(
        &self,
        url: impl Fn() -> Url,
        limit: u64,
    ) -> Result<(String, u64), Failure> {
        let url = &url;
        self.retrying(json!({"request": "get"}), move |_| async move {
            let mut response = self
                .request(
                    Method::GET,
                    url(),
                    &[],
                    None,
                    self.policy.timeout_for(limit),
                )
                .await?;
            let mut hasher = Sha256::new();
            let mut total = 0;
            while let Some(chunk) = response.chunk().await.map_err(transport)? {
                total += chunk.len() as u64;
                if total > limit {
                    break;
                }
                hasher.update(&chunk);
            }
            Ok((hex(&hasher.finalize()), total))
        })
        .await
    }
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_checks_name_every_difference_and_backoff_stays_under_its_ceiling() {
        let asset: AssetRef = serde_json::from_value(json!({
            "url": "https://assets.example.com/public/video/0c6eb955d7b052f7/clip.mp4",
            "bytes": 20037160, "sha256": "0c6eb955d7b052f79ab511fb0ab43105ed10c31dcf3ff8169b23133583d4aaf4",
            "type": "video/mp4", "visibility": "public"
        }))
        .unwrap();
        let good = Head {
            status: 200,
            bytes: Some(20037160),
            media_type: Some("video/mp4; codecs=avc1".into()),
            sha256: Some(asset.sha256.clone()),
            accept_ranges: Some("bytes".into()),
            cache_control: Some(IMMUTABLE.into()),
        };
        assert!(good.problems(&asset, true).is_empty());
        let bad = Head {
            bytes: Some(1),
            media_type: Some("text/html".into()),
            sha256: Some("0".repeat(64)),
            accept_ranges: None,
            cache_control: Some("no-cache".into()),
            ..good
        };
        assert_eq!(bad.problems(&asset, true).len(), 5);
        assert_eq!(bad.problems(&asset, false).len(), 4);
        let policy = Policy::default();
        for attempt in 1..=12 {
            let ceiling = (policy.base_delay * 2u32.pow(attempt - 1)).min(policy.max_delay);
            assert!(policy.backoff(attempt) <= ceiling);
        }
    }
}
