//! Integrity-pinned uploads to S3-compatible object stores: AWS S3, Cloudflare R2, the Google
//! Cloud Storage XML API with an HMAC key, and local servers such as Garage.
//!
//! `put` hashes a file, stores it under a content-addressed key with a signed media type and
//! `x-amz-meta-sha256`, checks the stored object and returns the reference that
//! [`richview_core::AssetRef`] validates. Every request is presigned locally with rusty-s3 and sent
//! with reqwest. Multipart uploads keep an on-disk journal and resume from the store's part list.
//! Credentials come from the environment or the AWS shared credentials file; they are never
//! written to profiles, journals or output.
mod store;
mod transfer;
mod upload;

use crate::cli::OutputFormat;
use richview_core::{AssetRef, Visibility};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use store::{journal_dir, load_store};
use transfer::Policy;
use upload::{Uploader, abort, verify};

/// Cache policy of public objects: content keys never change meaning.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

#[derive(Debug)]
struct AssetError {
    code: &'static str,
    /// Process exit code: 2 invalid input or profile, 3 transfer failed, 4 verification mismatch.
    exit: i32,
    message: String,
}

impl AssetError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "asset.invalid",
            exit: 2,
            message: message.into(),
        }
    }
    fn transfer(message: impl Into<String>) -> Self {
        Self {
            code: "asset.transfer",
            exit: 3,
            message: message.into(),
        }
    }
    fn mismatch(message: impl Into<String>) -> Self {
        Self {
            code: "asset.mismatch",
            exit: 4,
            message: message.into(),
        }
    }
    fn to_json(&self) -> Value {
        json!({"ok": false, "code": self.code, "error": self.message})
    }
}

fn io_error(context: &str, path: &Path, error: std::io::Error) -> AssetError {
    AssetError::invalid(format!("{context} {}: {error}", path.display()))
}

/// Receives machine-readable events: `start`, `exists`, `progress`, `retry`, `restart`, `done`.
pub type Events<'a> = &'a (dyn Fn(Value) + Send + Sync);

#[derive(clap::Subcommand)]
pub enum AssetCommand {
    /// Upload a data or media file and print its integrity-pinned reference
    Put {
        file: PathBuf,
        /// Store profile name
        #[arg(long = "to", value_name = "STORE")]
        store: String,
        /// Write under the public prefix with immutable caching (private by default)
        #[arg(long)]
        public: bool,
        /// Media type when the extension is ambiguous, for example audio/webm
        #[arg(long = "type", value_name = "MEDIA_TYPE")]
        media_type: Option<String>,
        #[command(flatten)]
        options: AssetOptions,
    },
    /// Check stored objects against reference files (YAML or JSON)
    Verify {
        #[arg(required = true)]
        references: Vec<PathBuf>,
        /// Store profile that signs requests for private references
        #[arg(long = "to", value_name = "STORE")]
        store: Option<String>,
        /// Stream each object and compare its SHA-256
        #[arg(long)]
        hash: bool,
        #[command(flatten)]
        options: AssetOptions,
    },
    /// Abort journaled multipart uploads (all, or those whose SHA-256 starts with a prefix)
    Abort {
        sha256: Vec<String>,
        /// Only uploads to this store profile
        #[arg(long = "to", value_name = "STORE")]
        store: Option<String>,
        #[command(flatten)]
        options: AssetOptions,
    },
}

#[derive(clap::Args)]
pub struct AssetOptions {
    /// Store profiles file [default: $RICHVIEW_STORES, then $XDG_CONFIG_HOME/richview/stores.yaml]
    #[arg(long, value_name = "FILE")]
    stores: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "pretty")]
    format: OutputFormat,
}

/// Runs an asset command and returns the process exit code.
pub async fn run(command: AssetCommand) -> i32 {
    let options = match &command {
        AssetCommand::Put { options, .. }
        | AssetCommand::Verify { options, .. }
        | AssetCommand::Abort { options, .. } => options,
    };
    let format = options.format;
    let events = move |event: Value| match format {
        OutputFormat::Json => eprintln!("{event}"),
        OutputFormat::Pretty => {
            if let Some(line) = describe(&event) {
                eprintln!("{line}")
            }
        }
    };
    let result = execute(&command, &events).await;
    match (result, format) {
        (Ok((value, exit)), OutputFormat::Json) => {
            println!("{value}");
            exit
        }
        (Ok((value, exit)), OutputFormat::Pretty) => {
            print!("{}", pretty(&command, &value));
            exit
        }
        (Err(error), OutputFormat::Json) => {
            println!("{}", error.to_json());
            error.exit
        }
        (Err(error), OutputFormat::Pretty) => {
            eprintln!("richview: {}", error.message);
            error.exit
        }
    }
}

async fn execute(command: &AssetCommand, events: Events<'_>) -> Result<(Value, i32), AssetError> {
    match command {
        AssetCommand::Put {
            file,
            store,
            public,
            media_type,
            options,
        } => {
            let store = load_store(options.stores.as_deref(), store)?;
            let uploader = Uploader::new(&store, journal_dir()?, Policy::default(), events)?;
            let visibility = if *public {
                Visibility::Public
            } else {
                Visibility::Private
            };
            Ok((
                uploader
                    .put(file, visibility, media_type.as_deref())
                    .await?,
                0,
            ))
        }
        AssetCommand::Verify {
            references,
            store,
            hash,
            options,
        } => {
            let store = store
                .as_deref()
                .map(|name| load_store(options.stores.as_deref(), name))
                .transpose()?;
            let mut values = Vec::new();
            for path in references {
                let text =
                    std::fs::read_to_string(path).map_err(|e| io_error("Cannot read", path, e))?;
                let value: Value = richview_core::yaml::from_str(&text).map_err(|e| {
                    AssetError::invalid(format!("{} is not YAML or JSON: {e}", path.display()))
                })?;
                // Accepts a reference or the JSON that `put` prints.
                let value = value.get("asset").cloned().unwrap_or(value);
                values.push((path.display().to_string(), value));
            }
            verify(&values, store.as_ref(), *hash, Policy::default(), events).await
        }
        AssetCommand::Abort {
            sha256,
            store,
            options,
        } => Ok((
            abort(
                &journal_dir()?,
                store.as_deref(),
                sha256,
                |name| load_store(options.stores.as_deref(), name),
                events,
            )
            .await?,
            0,
        )),
    }
}

fn describe(event: &Value) -> Option<String> {
    let text = |key: &str| event[key].as_str().unwrap_or_default().to_owned();
    let number = |key: &str| event[key].as_u64().unwrap_or_default();
    Some(match event["event"].as_str()? {
        "start" => format!(
            "{} ({} bytes, {} part{}) goes to store {} as {}",
            text("file"),
            number("bytes"),
            number("parts"),
            if number("parts") == 1 { "" } else { "s" },
            text("store"),
            text("key")
        ),
        "exists" => format!("Already stored at {}", text("key")),
        "progress" => format!(
            "{} of {} bytes, {}/{} parts",
            number("sent"),
            number("total"),
            number("parts_done"),
            number("parts")
        ),
        "retry" => format!(
            "Retrying {} in {} ms (attempt {}): {}",
            event
                .get("part")
                .map(|p| format!("part {p}"))
                .unwrap_or_else(|| text("request")),
            number("wait_ms"),
            number("attempt"),
            text("reason")
        ),
        "restart" => format!("Starting the upload again: {}", text("reason")),
        _ => return None,
    })
}

fn pretty(command: &AssetCommand, value: &Value) -> String {
    match command {
        AssetCommand::Put { .. } => serde_json::from_value::<AssetRef>(value["asset"].clone())
            .ok()
            .and_then(|asset| serde_json::to_value(&asset).ok())
            .map(|asset| {
                // The reference's own field order, as an author writes it.
                let order = [
                    "url",
                    "bytes",
                    "sha256",
                    "type",
                    "visibility",
                    "duration",
                    "width",
                    "height",
                ];
                richview_core::yaml::to_string(&richview_core::yaml::Node::Map(
                    order
                        .iter()
                        .filter_map(|key| asset.get(*key).map(|v| ((*key).to_owned(), v.into())))
                        .collect(),
                ))
            })
            .unwrap_or_default(),
        AssetCommand::Verify { .. } => value["checked"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|check| {
                let problems: Vec<_> = check["problems"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .chain(
                        check["diagnostics"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|d| d["message"].as_str()),
                    )
                    .collect();
                if check["ok"] == true {
                    format!("ok {}\n", check["url"].as_str().unwrap_or_default())
                } else {
                    format!(
                        "FAILED {}: {}\n",
                        check["reference"].as_str().unwrap_or_default(),
                        problems.join("; ")
                    )
                }
            })
            .collect(),
        AssetCommand::Abort { .. } => {
            let lines: String = value["aborted"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| {
                    format!(
                        "{} {} in {}\n",
                        a["status"].as_str().unwrap_or_default(),
                        a["key"].as_str().unwrap_or_default(),
                        a["store"].as_str().unwrap_or_default()
                    )
                })
                .collect();
            if lines.is_empty() {
                "No journaled uploads.\n".into()
            } else {
                lines
            }
        }
    }
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "richview_asset_put",
            "description": "Upload a local data or media file to a configured S3-compatible store and return its integrity-pinned reference {url, bytes, sha256, type, visibility}. Private unless public is true. Calling again resumes an interrupted upload or reports the stored object. Progress notifications follow the request's progressToken.",
            "inputSchema": {"type": "object", "properties": {
                "path": {"type": "string", "description": "Local file to upload"},
                "store": {"type": "string", "description": "Store profile name from stores.yaml"},
                "public": {"type": "boolean", "description": "Serve under the public prefix with immutable caching"},
                "type": {"type": "string", "description": "Media type when the extension is ambiguous"}
            }, "required": ["path", "store"]},
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true}
        }),
        json!({
            "name": "richview_asset_verify",
            "description": "Check a stored object against its reference: HEAD status, size, type and range support; with hash, stream the object and compare its SHA-256. Private references need store.",
            "inputSchema": {"type": "object", "properties": {
                "asset": {"type": "object", "description": "Reference {url, bytes, sha256, type, visibility}"},
                "store": {"type": "string", "description": "Store profile that signs requests for private references"},
                "hash": {"type": "boolean"}
            }, "required": ["asset"]},
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": true}
        }),
    ]
}

/// Runs an asset MCP tool; `None` for other tools. Returns the CLI's JSON and whether it failed.
pub async fn call_tool(name: &str, args: &Value, events: Events<'_>) -> Option<(Value, bool)> {
    let text = |key: &str| args.get(key).and_then(Value::as_str);
    let result = match name {
        "richview_asset_put" => {
            async {
                let (Some(path), Some(store)) = (text("path"), text("store")) else {
                    return Err(AssetError::invalid("path and store are required"));
                };
                let store = load_store(None, store)?;
                let uploader = Uploader::new(&store, journal_dir()?, Policy::default(), events)?;
                let visibility = if args.get("public").and_then(Value::as_bool) == Some(true) {
                    Visibility::Public
                } else {
                    Visibility::Private
                };
                Ok((
                    uploader
                        .put(Path::new(path), visibility, text("type"))
                        .await?,
                    0,
                ))
            }
            .await
        }
        "richview_asset_verify" => {
            async {
                let asset = args
                    .get("asset")
                    .cloned()
                    .ok_or_else(|| AssetError::invalid("asset is required"))?;
                let store = text("store").map(|s| load_store(None, s)).transpose()?;
                let hash = args.get("hash").and_then(Value::as_bool) == Some(true);
                verify(
                    &[("asset".into(), asset)],
                    store.as_ref(),
                    hash,
                    Policy::default(),
                    events,
                )
                .await
            }
            .await
        }
        _ => return None,
    };
    Some(match result {
        Ok((value, exit)) => (value, exit != 0),
        Err(error) => (error.to_json(), true),
    })
}

/// Byte progress of an upload event, for MCP progress notifications.
pub fn progress_of(event: &Value) -> Option<(u64, u64, String)> {
    (event["event"] == "progress").then(|| {
        (
            event["sent"].as_u64().unwrap_or_default(),
            event["total"].as_u64().unwrap_or_default(),
            format!(
                "{} of {} parts",
                event["parts_done"].as_u64().unwrap_or_default(),
                event["parts"].as_u64().unwrap_or_default()
            ),
        )
    })
}
