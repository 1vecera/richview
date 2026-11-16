//! Integrity-pinned references to remote data and media objects.
//!
//! A reference names one stored object by its absolute URL and pins its size, SHA-256, media
//! type and visibility, so a host can fetch the object later and prove it received the authored
//! bytes. A presigned URL is never a reference: it expires and grants access to anyone who holds
//! it. Private objects are read with the credentials of a store profile instead.
use crate::Diagnostic;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Largest declared size: the largest integer a browser runtime represents exactly.
pub(crate) const MAX_ASSET_BYTES: u64 = (1 << 53) - 1;
const MAX_URL_CHARS: usize = 2048;
const KEYS: [&str; 8] = [
    "url",
    "bytes",
    "sha256",
    "type",
    "visibility",
    "duration",
    "width",
    "height",
];

/// What a referenced object is used for; each use has its own media type allowlist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetUse {
    Data,
    Image,
    Video,
    Audio,
}

impl AssetUse {
    pub const ALL: [Self; 4] = [Self::Data, Self::Image, Self::Video, Self::Audio];

    pub fn name(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
        }
    }

    /// Media types a reference may declare for this use. `text/html` and other active
    /// document types are never allowed.
    pub fn types(self) -> &'static [&'static str] {
        match self {
            Self::Data => &["text/csv", "application/json"],
            Self::Image => &[
                "image/png",
                "image/jpeg",
                "image/webp",
                "image/avif",
                "image/gif",
                "image/svg+xml",
            ],
            Self::Video => &["video/mp4", "video/webm"],
            Self::Audio => &["audio/mpeg", "audio/mp4", "audio/ogg", "audio/webm"],
        }
    }

    /// The use an allowed media type belongs to.
    pub fn of_type(media_type: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|usage| usage.types().contains(&media_type))
    }
}

/// The allowed media type for a file extension (case-insensitive): the default an uploader
/// declares when the author does not name one.
pub fn media_type_for_extension(extension: &str) -> Option<&'static str> {
    Some(match extension.to_ascii_lowercase().as_str() {
        "csv" => "text/csv",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "weba" => "audio/webm",
        _ => return None,
    })
}

/// `public` objects are readable by anyone with the URL; `private` ones need credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetRef {
    pub url: String,
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the complete object.
    pub sha256: String,
    #[serde(rename = "type")]
    pub media_type: String,
    pub visibility: Visibility,
    /// Seconds; video and audio only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    /// Pixels; image and video only. Reserves layout before the media loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

impl AssetRef {
    /// Reads an authored reference mapping for `usage`, adding every finding to `diagnostics`.
    /// Returns the reference only when it has no errors; unknown keys are warnings.
    pub fn parse(
        value: &Value,
        usage: AssetUse,
        path: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<Self> {
        let Some(map) = value.as_object() else {
            diagnostics.push(Diagnostic::new(
                "error",
                "asset.mapping",
                format!(
                    "A remote {} reference is a mapping with url, bytes, sha256, type and visibility.",
                    usage.name()
                ),
                path,
            ));
            return None;
        };
        let mut errors = Vec::new();
        let mut fail = |code: &str, key: &str, message: String| {
            errors.push(Diagnostic::new(
                "error",
                code,
                message,
                format!("{path}.{key}"),
            ))
        };
        for key in map.keys().filter(|key| !KEYS.contains(&key.as_str())) {
            diagnostics.push(Diagnostic::new(
                "warning",
                "asset.unknown-key",
                format!(
                    "Unknown asset reference key '{key}'; references take {}.",
                    KEYS.join(", ")
                ),
                format!("{path}.{key}"),
            ));
        }
        let text = |key| map.get(key).and_then(Value::as_str);
        let url = match text("url") {
            Some(url) => match check_url(url) {
                Ok(()) => Some(url),
                Err((code, message)) => {
                    fail(code, "url", message);
                    None
                }
            },
            None => {
                fail(
                    "asset.url",
                    "url",
                    "Add the object's absolute https URL.".into(),
                );
                None
            }
        };
        let bytes = map
            .get("bytes")
            .and_then(Value::as_u64)
            .filter(|bytes| (1..=MAX_ASSET_BYTES).contains(bytes));
        if bytes.is_none() {
            fail(
                "asset.bytes",
                "bytes",
                "bytes is the object size: a positive whole number of bytes.".into(),
            );
        }
        let sha256 = text("sha256").filter(|hash| is_sha256(hash));
        if sha256.is_none() {
            fail(
                "asset.sha256",
                "sha256",
                "sha256 is the SHA-256 of the complete object as 64 lowercase hex characters."
                    .into(),
            );
        }
        let media_type = text("type").filter(|kind| usage.types().contains(kind));
        if media_type.is_none() {
            let declared = text("type").unwrap_or_default();
            let hint = match AssetUse::of_type(declared) {
                Some(other) => format!(" '{declared}' is for {} assets.", other.name()),
                None if declared.is_empty() => String::new(),
                None => format!(" '{declared}' is not allowed."),
            };
            fail(
                "asset.type",
                "type",
                format!(
                    "type for {} assets is one of {}.{hint}",
                    usage.name(),
                    usage.types().join(", ")
                ),
            );
        }
        let visibility = match text("visibility") {
            Some("public") => Some(Visibility::Public),
            Some("private") => Some(Visibility::Private),
            _ => {
                fail(
                    "asset.visibility",
                    "visibility",
                    "visibility is public (readable by anyone with the URL) or private (requires credentials).".into(),
                );
                None
            }
        };
        let media = matches!(usage, AssetUse::Video | AssetUse::Audio);
        let duration = map.get("duration").and_then(|value| {
            let seconds = value.as_f64().filter(|s| s.is_finite() && *s > 0.0);
            if !media {
                fail(
                    "asset.duration",
                    "duration",
                    "duration applies to video and audio only.".into(),
                );
            } else if seconds.is_none() {
                fail(
                    "asset.duration",
                    "duration",
                    "duration is a positive number of seconds.".into(),
                );
            }
            seconds
        });
        let mut dimension = |key: &str| {
            map.get(key).and_then(|value| {
                let pixels = value
                    .as_u64()
                    .filter(|n| *n > 0)
                    .and_then(|n| u32::try_from(n).ok());
                if !matches!(usage, AssetUse::Image | AssetUse::Video) {
                    fail(
                        "asset.dimensions",
                        key,
                        format!("{key} applies to images and video only."),
                    );
                } else if pixels.is_none() {
                    fail(
                        "asset.dimensions",
                        key,
                        format!("{key} is a positive whole number of pixels."),
                    );
                }
                pixels
            })
        };
        let (width, height) = (dimension("width"), dimension("height"));
        if !errors.is_empty() {
            diagnostics.extend(errors);
            return None;
        }
        Some(Self {
            url: url?.to_owned(),
            bytes: bytes?,
            sha256: sha256?.to_owned(),
            media_type: media_type?.to_owned(),
            visibility: visibility?,
            duration,
            width,
            height,
        })
    }

    /// Checks a reference a host built (for example an upload result) with the same rules as
    /// authored source.
    pub fn validate(&self, usage: AssetUse) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let value = serde_json::to_value(self).unwrap_or(Value::Null);
        Self::parse(&value, usage, "asset", &mut diagnostics);
        diagnostics
    }
}

fn is_sha256(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// URL rules shared by every reference. Messages name the host, never the full URL, so a
/// rejected capability URL is not copied into logs.
fn check_url(raw: &str) -> Result<(), (&'static str, String)> {
    if raw.chars().count() > MAX_URL_CHARS {
        return Err((
            "asset.url",
            format!("The URL is longer than {MAX_URL_CHARS} characters."),
        ));
    }
    let url = url::Url::parse(raw).map_err(|_| {
        (
            "asset.url",
            "url is the object's absolute https URL, not a local path.".to_owned(),
        )
    })?;
    if let Some(parameter) = signing_parameter(&url) {
        return Err((
            "asset.presigned",
            format!(
                "The URL carries the signing parameter '{parameter}'. A presigned URL expires and grants access to anyone who holds it: reference the object's stable URL; private objects are read with a store profile's credentials."
            ),
        ));
    }
    let host = url.host_str().unwrap_or_default();
    match url.scheme() {
        "https" => {}
        "http" if is_loopback(&url) => {}
        "http" => {
            return Err((
                "asset.url",
                format!(
                    "Use https for '{host}'; http is allowed only for loopback development stores."
                ),
            ));
        }
        scheme => {
            return Err((
                "asset.url",
                format!("Unsupported URL scheme '{scheme}'; use https."),
            ));
        }
    }
    if host.is_empty() {
        return Err(("asset.url", "The URL has no host.".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err((
            "asset.url",
            format!(
                "Remove the user information from the URL for '{host}'; credentials never belong in a reference."
            ),
        ));
    }
    if url.fragment().is_some() {
        return Err((
            "asset.url",
            format!("Remove the #fragment from the URL for '{host}'."),
        ));
    }
    Ok(())
}

/// Query parameters that make a URL a temporary capability: AWS and Google V4 and V2
/// signatures, CloudFront signed URLs and Azure shared access signatures.
fn signing_parameter(url: &url::Url) -> Option<String> {
    let names: Vec<String> = url
        .query_pairs()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    let has = |name: &str| names.iter().any(|n| n == name);
    names
        .iter()
        .find(|name| {
            name.starts_with("x-amz-")
                || name.starts_with("x-goog-")
                || [
                    "signature",
                    "key-pair-id",
                    "awsaccesskeyid",
                    "googleaccessid",
                ]
                .contains(&name.as_str())
        })
        .cloned()
        .or_else(|| (has("sig") && (has("se") || has("sp"))).then(|| "sig".to_owned()))
}

/// Loopback hosts, including RFC 6761 `localhost` names.
pub fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(domain)) => domain == "localhost" || domain.ends_with(".localhost"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const VIDEO_SHA: &str = "0c6eb955d7b052f79ab511fb0ab43105ed10c31dcf3ff8169b23133583d4aaf4";
    const CSV_SHA: &str = "6f0bf32234c39405d2ff40b50432458bd2276446346d348511da602026837769";

    fn video(url: &str) -> Value {
        json!({"url": url, "bytes": 20037160, "sha256": VIDEO_SHA, "type": "video/mp4", "visibility": "public"})
    }

    fn findings(value: &Value, usage: AssetUse) -> (Option<AssetRef>, Vec<Diagnostic>) {
        let mut diagnostics = Vec::new();
        let asset = AssetRef::parse(value, usage, "video.src", &mut diagnostics);
        (asset, diagnostics)
    }

    fn codes(value: &Value, usage: AssetUse) -> Vec<String> {
        let (asset, diagnostics) = findings(value, usage);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.level == "error")
            .map(|d| d.code.clone())
            .collect();
        assert_eq!(asset.is_none(), !errors.is_empty(), "{diagnostics:?}");
        errors
    }

    #[test]
    fn accepts_the_specified_video_and_dataset_references() {
        let source = json!({
            "url": "https://assets.example.com/r/demo/media/0c6eb955d7b052f7/clip.mp4",
            "bytes": 20037160, "sha256": VIDEO_SHA, "type": "video/mp4", "visibility": "public",
            "duration": 40.0, "width": 1280, "height": 720
        });
        let (asset, diagnostics) = findings(&source, AssetUse::Video);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let asset = asset.unwrap();
        assert_eq!(
            (asset.duration, asset.width, asset.height),
            (Some(40.0), Some(1280), Some(720))
        );
        assert_eq!(serde_json::to_value(&asset).unwrap(), source);
        let dataset = json!({
            "url": "https://assets.example.com/r/demo/data/6f0bf32234c39405/dataset.csv",
            "bytes": 1313527, "sha256": CSV_SHA, "type": "text/csv", "visibility": "private"
        });
        assert!(codes(&dataset, AssetUse::Data).is_empty());
    }

    #[test]
    fn allows_http_only_on_loopback_hosts() {
        for url in [
            "http://127.0.0.1:18740/richview-assets/private/video/0c6eb955d7b052f7/clip.mp4",
            "http://127.1.2.3/clip.mp4",
            "http://[::1]:3900/clip.mp4",
            "http://localhost:3900/clip.mp4",
            "http://richview-public.web.garage.localhost:3902/public/video/clip.mp4",
            "https://assets.example.com/clip.mp4?v=2",
        ] {
            assert!(codes(&video(url), AssetUse::Video).is_empty(), "{url}");
        }
        for url in [
            "http://assets.example.com/clip.mp4",
            "http://192.168.1.10:9000/clip.mp4",
            "http://localhost.example.com/clip.mp4",
            "ftp://assets.example.com/clip.mp4",
            "data:video/mp4;base64,AAAA",
            "media/clip.mp4",
            "/clip.mp4",
        ] {
            assert_eq!(codes(&video(url), AssetUse::Video), ["asset.url"], "{url}");
        }
    }

    #[test]
    fn rejects_presigned_urls_without_copying_the_capability() {
        let secret = "AKIAIOSFODNN7EXAMPLE";
        for (provider, url) in [
            (
                "aws",
                format!(
                    "https://b.s3.eu-central-1.amazonaws.com/clip.mp4?X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential={secret}%2F20261010%2Feu-central-1%2Fs3%2Faws4_request&X-Amz-Date=20261010T000000Z&X-Amz-Expires=900&X-Amz-SignedHeaders=host&X-Amz-Signature=abcdef"
                ),
            ),
            (
                "aws-v2",
                format!(
                    "https://b.s3.amazonaws.com/clip.mp4?AWSAccessKeyId={secret}&Expires=1760000000&Signature=abc%3D"
                ),
            ),
            (
                "google",
                format!(
                    "https://storage.googleapis.com/b/clip.mp4?X-Goog-Algorithm=GOOG4-RSA-SHA256&X-Goog-Credential={secret}%2F20261010%2Fauto%2Fstorage%2Fgoog4_request&X-Goog-Signature=abcdef"
                ),
            ),
            (
                "azure",
                format!(
                    "https://account.blob.core.windows.net/assets/clip.mp4?sv=2025-05-05&se=2026-10-17T00%3A00%3A00Z&sr=b&sp=r&sig={secret}"
                ),
            ),
            (
                "azure-permissions-only",
                format!("https://account.blob.core.windows.net/assets/clip.mp4?sp=r&sig={secret}"),
            ),
            (
                "cloudfront",
                format!(
                    "https://d111111abcdef8.cloudfront.net/clip.mp4?Expires=1760000000&Signature={secret}~&Key-Pair-Id=K2JCJMDEHXQW5F"
                ),
            ),
            (
                "cloudfront-key-pair-only",
                "https://d111111abcdef8.cloudfront.net/clip.mp4?Key-Pair-Id=K2JCJMDEHXQW5F"
                    .to_owned(),
            ),
        ] {
            let (_, diagnostics) = findings(&video(&url), AssetUse::Video);
            assert_eq!(
                diagnostics
                    .iter()
                    .map(|d| d.code.as_str())
                    .collect::<Vec<_>>(),
                ["asset.presigned"],
                "{provider}"
            );
            assert!(!diagnostics[0].message.contains(secret), "{provider}");
            assert_eq!(diagnostics[0].path, "video.src.url");
        }
        // A signature alone is not an Azure SAS; ordinary query strings stay allowed.
        let unsigned = "https://account.blob.core.windows.net/assets/clip.mp4?sig=1";
        assert!(codes(&video(unsigned), AssetUse::Video).is_empty());
    }

    #[test]
    fn rejects_user_information_and_fragments() {
        let (_, diagnostics) = findings(
            &video("https://uploader:s3cr3t@assets.example.com/clip.mp4"),
            AssetUse::Video,
        );
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "asset.url");
        assert!(!diagnostics[0].message.contains("s3cr3t"));
        assert_eq!(
            codes(
                &video("https://assets.example.com/clip.mp4#t=10"),
                AssetUse::Video
            ),
            ["asset.url"]
        );
        let long = format!("https://assets.example.com/{}", "a".repeat(2100));
        assert_eq!(codes(&video(&long), AssetUse::Video), ["asset.url"]);
    }

    #[test]
    fn rejects_malformed_hashes_and_sizes() {
        let base = video("https://assets.example.com/clip.mp4");
        for hash in [
            json!(VIDEO_SHA.to_uppercase()),
            json!(&VIDEO_SHA[..63]),
            json!(format!("{}0", VIDEO_SHA)),
            json!(VIDEO_SHA.replace('0', "g")),
            json!(format!("sha256-{VIDEO_SHA}")),
            json!(null),
        ] {
            let mut value = base.clone();
            value["sha256"] = hash.clone();
            assert_eq!(codes(&value, AssetUse::Video), ["asset.sha256"], "{hash}");
        }
        for bytes in [
            json!(0),
            json!(-1),
            json!(1.5),
            json!("20037160"),
            json!(MAX_ASSET_BYTES + 1),
        ] {
            let mut value = base.clone();
            value["bytes"] = bytes.clone();
            assert_eq!(codes(&value, AssetUse::Video), ["asset.bytes"], "{bytes}");
        }
    }

    #[test]
    fn allows_only_listed_media_types_for_each_use() {
        let base = video("https://assets.example.com/a");
        for (usage, kind) in [
            (AssetUse::Data, "text/html"),
            (AssetUse::Data, "text/csv; charset=utf-8"),
            (AssetUse::Data, "TEXT/CSV"),
            (AssetUse::Image, "image/tiff"),
            (AssetUse::Image, "video/mp4"),
            (AssetUse::Video, "video/quicktime"),
            (AssetUse::Video, "image/svg+xml"),
            (AssetUse::Audio, "audio/wav"),
            (AssetUse::Audio, "application/octet-stream"),
        ] {
            let mut value = base.clone();
            value["type"] = json!(kind);
            assert_eq!(codes(&value, usage), ["asset.type"], "{kind}");
        }
        let mut wrong_use = base.clone();
        wrong_use["type"] = json!("image/png");
        let (_, diagnostics) = findings(&wrong_use, AssetUse::Video);
        assert!(
            diagnostics[0]
                .message
                .contains("'image/png' is for image assets")
        );
        for usage in AssetUse::ALL {
            for kind in usage.types() {
                assert_eq!(AssetUse::of_type(kind), Some(usage));
            }
        }
    }

    #[test]
    fn checks_visibility_and_use_specific_keys() {
        let base = video("https://assets.example.com/clip.mp4");
        for visibility in ["Public", "unlisted", ""] {
            let mut value = base.clone();
            value["visibility"] = json!(visibility);
            assert_eq!(codes(&value, AssetUse::Video), ["asset.visibility"]);
        }
        let mut image = base.clone();
        image["type"] = json!("image/png");
        image["duration"] = json!(4);
        assert_eq!(codes(&image, AssetUse::Image), ["asset.duration"]);
        let mut audio = base.clone();
        audio["type"] = json!("audio/mpeg");
        audio["width"] = json!(640);
        assert_eq!(codes(&audio, AssetUse::Audio), ["asset.dimensions"]);
        let mut bad = base.clone();
        bad["duration"] = json!(-1);
        bad["height"] = json!(0);
        assert_eq!(
            codes(&bad, AssetUse::Video),
            ["asset.duration", "asset.dimensions"]
        );
        assert_eq!(
            codes(&json!("clip.mp4"), AssetUse::Video),
            ["asset.mapping"]
        );
        assert_eq!(
            codes(&json!({}), AssetUse::Video),
            [
                "asset.url",
                "asset.bytes",
                "asset.sha256",
                "asset.type",
                "asset.visibility"
            ]
        );
    }

    #[test]
    fn unknown_keys_warn_without_invalidating() {
        let mut value = video("https://assets.example.com/clip.mp4");
        value["etag"] = json!("\"abc\"");
        let (asset, diagnostics) = findings(&value, AssetUse::Video);
        assert!(asset.is_some());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            (diagnostics[0].level.as_str(), diagnostics[0].code.as_str()),
            ("warning", "asset.unknown-key")
        );
    }

    #[test]
    fn host_built_references_validate_with_the_authored_rules() {
        let asset = AssetRef {
            url: "http://127.0.0.1:18740/richview-assets/private/data/6f0bf32234c39405/dataset.csv"
                .into(),
            bytes: 1313527,
            sha256: CSV_SHA.into(),
            media_type: "text/csv".into(),
            visibility: Visibility::Private,
            duration: None,
            width: None,
            height: None,
        };
        assert!(asset.validate(AssetUse::Data).is_empty());
        assert_eq!(
            serde_json::to_value(&asset).unwrap()["type"],
            json!("text/csv")
        );
        let presigned = AssetRef {
            url: format!("{}?X-Amz-Signature=abc", asset.url),
            ..asset.clone()
        };
        assert_eq!(
            presigned.validate(AssetUse::Data)[0].code,
            "asset.presigned"
        );
        assert_eq!(asset.validate(AssetUse::Image)[0].code, "asset.type");
    }

    #[test]
    fn extension_defaults_are_allowed_types() {
        for extension in [
            "csv", "json", "png", "jpg", "JPEG", "webp", "avif", "gif", "svg", "mp4", "m4v",
            "webm", "mp3", "m4a", "ogg", "oga", "opus", "weba",
        ] {
            let kind = media_type_for_extension(extension).unwrap();
            assert!(AssetUse::of_type(kind).is_some(), "{extension}");
        }
        for extension in ["html", "htm", "js", "mov", "wav", "tiff", "exe", ""] {
            assert_eq!(media_type_for_extension(extension), None, "{extension}");
        }
    }
}
