//! Store profiles name where assets go; credentials come from the environment or the AWS shared
//! credentials file and never from a profile.
use super::{AssetError, io_error};
use reqwest::Url;
use richview_core::Visibility;
use rusty_s3::{Bucket, Credentials, UrlStyle};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const DEFAULT_MAX_BYTES: u64 = 5 << 30;

/// One entry of `stores.yaml`. Profiles name places, never secrets.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct StoreProfile {
    #[serde(default)]
    kind: Option<String>,
    /// S3 API origin; AWS S3 in `region` when omitted.
    #[serde(default)]
    endpoint: Option<String>,
    region: String,
    bucket: String,
    /// Bucket for `--public` uploads; the main bucket's public prefix when omitted.
    #[serde(default)]
    public_bucket: Option<String>,
    /// `path` or `virtual-host`; path style with an endpoint, virtual-host style for AWS.
    #[serde(default)]
    url_style: Option<String>,
    #[serde(default = "private_prefix")]
    private_prefix: String,
    #[serde(default = "public_prefix")]
    public_prefix: String,
    /// Origin that serves public objects: CDN, custom domain or website endpoint.
    #[serde(default)]
    public_base_url: Option<String>,
    /// `env` (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, AWS_SESSION_TOKEN) or `profile:<name>`
    /// from the AWS shared credentials file.
    #[serde(default = "env_credentials")]
    credentials: String,
    #[serde(default)]
    max_bytes: Option<u64>,
}

fn private_prefix() -> String {
    "private/".into()
}
fn public_prefix() -> String {
    "public/".into()
}
fn env_credentials() -> String {
    "env".into()
}

#[derive(Clone)]
pub(super) struct Store {
    pub(super) name: String,
    pub(super) private: Bucket,
    pub(super) public: Bucket,
    pub(super) private_prefix: String,
    pub(super) public_prefix: String,
    pub(super) public_base_url: Option<Url>,
    pub(super) max_bytes: u64,
    pub(super) credentials: Credentials,
}

impl Store {
    pub(super) fn new(
        name: &str,
        profile: StoreProfile,
        credentials: Credentials,
    ) -> Result<Self, AssetError> {
        let invalid = |message: String| AssetError::invalid(format!("Store '{name}': {message}"));
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        {
            return Err(invalid(
                "names use letters, digits, '.', '_' and '-'".into(),
            ));
        }
        if profile.kind.as_deref().is_some_and(|kind| kind != "s3") {
            return Err(invalid(format!(
                "kind '{}' is not supported; this version uploads to S3-compatible stores (kind: s3)",
                profile.kind.unwrap_or_default()
            )));
        }
        let endpoint = match &profile.endpoint {
            Some(endpoint) => origin(endpoint).map_err(|e| invalid(format!("endpoint {e}")))?,
            None => origin(&format!("https://s3.{}.amazonaws.com", profile.region))
                .map_err(|_| invalid(format!("region '{}' is not valid", profile.region)))?,
        };
        let style = match profile.url_style.as_deref() {
            Some("path") => UrlStyle::Path,
            Some("virtual-host") => UrlStyle::VirtualHost,
            None if profile.endpoint.is_some() => UrlStyle::Path,
            None => UrlStyle::VirtualHost,
            Some(other) => {
                return Err(invalid(format!(
                    "url_style '{other}' is not path or virtual-host"
                )));
            }
        };
        let bucket = |bucket: &str| {
            if bucket.is_empty()
                || !bucket
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-.".contains(&b))
            {
                return Err(invalid(format!(
                    "bucket '{bucket}' is not a valid bucket name"
                )));
            }
            Bucket::new(
                endpoint.clone(),
                style,
                bucket.to_owned(),
                profile.region.clone(),
            )
            .map_err(|e| invalid(format!("bucket '{bucket}': {e}")))
        };
        let private = bucket(&profile.bucket)?;
        let public = bucket(profile.public_bucket.as_deref().unwrap_or(&profile.bucket))?;
        let prefix = |prefix: &str| {
            let valid = !prefix.starts_with('/')
                && !prefix.split('/').any(|segment| segment == "..")
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-/".contains(&b));
            if !valid {
                return Err(invalid(format!(
                    "prefix '{prefix}' may use letters, digits, '.', '_', '-' and '/'"
                )));
            }
            Ok(if prefix.is_empty() || prefix.ends_with('/') {
                prefix.to_owned()
            } else {
                format!("{prefix}/")
            })
        };
        let public_base_url = profile
            .public_base_url
            .as_deref()
            .map(|base| origin(base).map_err(|e| invalid(format!("public_base_url {e}"))))
            .transpose()?;
        Ok(Self {
            name: name.to_owned(),
            private,
            public,
            private_prefix: prefix(&profile.private_prefix)?,
            public_prefix: prefix(&profile.public_prefix)?,
            public_base_url,
            max_bytes: profile.max_bytes.unwrap_or(DEFAULT_MAX_BYTES),
            credentials,
        })
    }

    pub(super) fn bucket(&self, visibility: Visibility) -> &Bucket {
        match visibility {
            Visibility::Public => &self.public,
            Visibility::Private => &self.private,
        }
    }

    pub(super) fn bucket_named(&self, name: &str) -> Option<&Bucket> {
        [&self.private, &self.public]
            .into_iter()
            .find(|bucket| bucket.name() == name)
    }

    pub(super) fn object_url(&self, visibility: Visibility, key: &str) -> Result<Url, AssetError> {
        let url = match visibility {
            Visibility::Private => self.private.object_url(key),
            Visibility::Public => self
                .public_base_url
                .as_ref()
                .ok_or_else(|| {
                    AssetError::invalid(format!(
                        "Store '{}' has no public_base_url, so it cannot serve public objects",
                        self.name
                    ))
                })?
                .join(key),
        };
        url.map_err(|e| AssetError::invalid(format!("Object key '{key}': {e}")))
    }

    /// The object key of a private reference in this store.
    pub(super) fn private_key(&self, url: &str) -> Option<String> {
        let path = url.strip_prefix(self.private.base_url().as_str())?;
        percent_decode(path).filter(|key| !key.is_empty() && !key.contains(['?', '#']))
    }

    /// Strings that must never reach output.
    pub(super) fn secrets(&self) -> Vec<String> {
        [
            Some(self.credentials.key()),
            Some(self.credentials.secret()),
            self.credentials.token(),
        ]
        .into_iter()
        .flatten()
        .filter(|s| s.len() >= 4)
        .map(str::to_owned)
        .collect()
    }
}

/// An http(s) base URL whose path ends in `/`, so keys join below it; http only on loopback
/// hosts.
fn origin(raw: &str) -> Result<Url, String> {
    let mut url = Url::parse(raw).map_err(|_| "is not an absolute URL".to_owned())?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err("must not contain user information".into());
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err("must not have a query or fragment".into());
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    match url.scheme() {
        "https" => Ok(url),
        "http" if richview_core::is_loopback(&url) => Ok(url),
        _ => Err("must use https (http only on loopback hosts)".into()),
    }
}

fn percent_decode(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `--stores`, `RICHVIEW_STORES`, then `$XDG_CONFIG_HOME/richview/stores.yaml`.
fn stores_file(explicit: Option<&Path>) -> Result<PathBuf, AssetError> {
    if let Some(path) = explicit
        .map(Path::to_owned)
        .or_else(|| std::env::var_os("RICHVIEW_STORES").map(PathBuf::from))
    {
        return Ok(path);
    }
    xdg_dir("XDG_CONFIG_HOME", ".config", "APPDATA")
        .map(|dir| dir.join("richview/stores.yaml"))
        .ok_or_else(|| AssetError::invalid("Set RICHVIEW_STORES to the store profiles file"))
}

pub(super) fn journal_dir() -> Result<PathBuf, AssetError> {
    xdg_dir("XDG_STATE_HOME", ".local/state", "LOCALAPPDATA")
        .map(|dir| dir.join("richview/uploads"))
        .ok_or_else(|| AssetError::invalid("Set XDG_STATE_HOME or HOME for the upload journal"))
}

/// `variable` when it is absolute, then `fallback` below `HOME`, then the folder the Windows
/// variable `windows` names, where `HOME` is usually unset.
fn xdg_dir(variable: &str, fallback: &str, windows: &str) -> Option<PathBuf> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|dir| dir.is_absolute())
    };
    absolute(variable)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .or_else(|| absolute(windows))
}

/// Loads a named profile and resolves its credentials.
pub(super) fn load_store(file: Option<&Path>, name: &str) -> Result<Store, AssetError> {
    let path = stores_file(file)?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| io_error("Cannot read store profiles", &path, e))?;
    // Values are not echoed: a misplaced secret must not reach the terminal.
    let file: Value = richview_core::yaml::from_str(&text).map_err(|e| {
        AssetError::invalid(format!(
            "{} is not valid YAML (line {})",
            path.display(),
            e.pos.line
        ))
    })?;
    let entry = file
        .get("stores")
        .and_then(|stores| stores.get(name))
        .cloned()
        .ok_or_else(|| {
            let known = file
                .get("stores")
                .and_then(Value::as_object)
                .map(|stores| stores.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            AssetError::invalid(format!(
                "No store '{name}' in {}{}",
                path.display(),
                if known.is_empty() {
                    String::new()
                } else {
                    format!("; known stores: {known}")
                }
            ))
        })?;
    let profile = parse_profile(&entry).map_err(|reason| {
        AssetError::invalid(format!(
            "Store '{name}' in {}: {reason}. Profiles hold no secrets; credentials come from \"env\" or \"profile:<name>\".",
            path.display()
        ))
    })?;
    let credentials = resolve_credentials(name, &profile.credentials)?;
    Store::new(name, profile, credentials)
}

/// Parses one profile. A type error quotes the value it rejects, so the error is taken from a
/// copy whose strings are redacted.
fn parse_profile(entry: &Value) -> Result<StoreProfile, String> {
    StoreProfile::deserialize(entry).map_err(|_| {
        StoreProfile::deserialize(&without_strings(entry))
            .err()
            .map_or_else(|| "invalid profile".to_owned(), |e| e.to_string())
    })
}

fn without_strings(value: &Value) -> Value {
    match value {
        Value::String(_) => Value::String("[redacted]".into()),
        Value::Array(items) => Value::Array(items.iter().map(without_strings).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), without_strings(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn resolve_credentials(store: &str, source: &str) -> Result<Credentials, AssetError> {
    if source == "env" {
        return Credentials::from_env().ok_or_else(|| {
            AssetError::invalid(format!(
                "Store '{store}' reads credentials from the environment: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY (and AWS_SESSION_TOKEN for temporary credentials)"
            ))
        });
    }
    let Some(profile) = source.strip_prefix("profile:") else {
        return Err(AssetError::invalid(format!(
            "Store '{store}': credentials is \"env\" or \"profile:<name>\""
        )));
    };
    let path = std::env::var_os("AWS_SHARED_CREDENTIALS_FILE")
        .map(PathBuf::from)
        .or_else(|| {
            // The AWS tools keep it below `USERPROFILE` on Windows, where `HOME` is usually unset.
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".aws/credentials"))
        })
        .ok_or_else(|| AssetError::invalid("Set AWS_SHARED_CREDENTIALS_FILE or HOME"))?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| io_error("Cannot read AWS credentials", &path, e))?;
    shared_credentials(&text, profile).ok_or_else(|| {
        AssetError::invalid(format!(
            "{} has no static keys for profile '{profile}'. For SSO or role profiles, export temporary keys with `aws configure export-credentials --profile {profile} --format env` and use credentials: env.",
            path.display()
        ))
    })
}

/// Static keys of one profile in the AWS shared credentials file (INI).
fn shared_credentials(text: &str, profile: &str) -> Option<Credentials> {
    let mut section = None;
    let (mut key, mut secret, mut token) = (None, None, None);
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = Some(name.trim().to_owned());
        } else if section.as_deref() == Some(profile)
            && let Some((name, value)) = line.split_once('=')
        {
            let value = Some(value.trim().to_owned()).filter(|v| !v.is_empty());
            match name.trim().to_ascii_lowercase().as_str() {
                "aws_access_key_id" => key = value,
                "aws_secret_access_key" => secret = value,
                "aws_session_token" => token = value,
                _ => {}
            }
        }
    }
    Some(match token {
        Some(token) => Credentials::new_with_token(key?, secret?, token),
        None => Credentials::new(key?, secret?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KEY: &str = "GKa1b2c3d4e5f6a7b8c9d0e1f2";
    const SECRET: &str = "5f1c0e9d8b7a6f5e4d3c2b1a0f9e8d7c";

    fn profile(value: Value) -> StoreProfile {
        serde_json::from_value(value).unwrap()
    }

    fn store(endpoint: &str) -> Store {
        let profile =
            profile(json!({"endpoint": endpoint, "region": "garage", "bucket": "assets"}));
        Store::new("local", profile, Credentials::new(KEY, SECRET)).unwrap()
    }

    #[test]
    fn profiles_name_safe_places_and_never_hold_secrets() {
        let aws = Store::new(
            "aws",
            profile(json!({"region": "eu-central-1", "bucket": "reports"})),
            Credentials::new(KEY, SECRET),
        )
        .unwrap();
        assert_eq!(
            aws.private.object_url("private/a").unwrap().as_str(),
            "https://reports.s3.eu-central-1.amazonaws.com/private/a"
        );
        let nested = store("http://127.0.0.1:3900/s3");
        assert_eq!(
            nested.private.base_url().as_str(),
            "http://127.0.0.1:3900/s3/assets/"
        );
        for (field, value) in [
            ("endpoint", "http://storage.example.com"),
            ("endpoint", "https://key:secret@storage.example.com"),
            ("bucket", "Reports"),
            ("private_prefix", "../escape/"),
            ("url_style", "dns"),
            ("kind", "azure"),
            ("public_base_url", "https://cdn.example.com/?token=1"),
        ] {
            let mut fields =
                json!({"endpoint": "https://s3.example.com", "region": "auto", "bucket": "b"});
            fields[field] = json!(value);
            let error = Store::new("s", profile(fields), Credentials::new(KEY, SECRET));
            assert_eq!(error.err().map(|e| e.exit), Some(2), "{field}");
        }
        // Errors name a misplaced secret's key or the expected type, never its value.
        for (field, expected) in [
            ("secret_access_key", "unknown field `secret_access_key`"),
            ("max_bytes", "expected u64"),
        ] {
            let mut fields = json!({"region": "r", "bucket": "b"});
            fields[field] = json!(SECRET);
            let error = parse_profile(&fields).unwrap_err();
            assert!(
                error.contains(expected) && !error.contains(SECRET),
                "{error}"
            );
        }
        let shared = "[default]\naws_access_key_id = AKIDEFAULT\naws_secret_access_key = default\n\n[richview-uploader]\naws_access_key_id = AKIDUPLOADER\naws_secret_access_key = uploader-secret\naws_session_token = token\n";
        let uploader = shared_credentials(shared, "richview-uploader").unwrap();
        assert_eq!(
            (uploader.key(), uploader.secret(), uploader.token()),
            ("AKIDUPLOADER", "uploader-secret", Some("token"))
        );
        assert!(shared_credentials(shared, "missing").is_none());
        assert!(shared_credentials("[p]\naws_access_key_id = A\n", "p").is_none());
    }
}
