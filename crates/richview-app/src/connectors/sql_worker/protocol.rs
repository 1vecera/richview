//! One bounded request and reply; author input cannot select executables or engine settings.
use super::super::spec::{SchemaField, SqlError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    path::PathBuf,
};

pub(super) const VERSION: u8 = 2;
pub(super) const MAX_BYTES: usize = crate::MAX_INPUT_BYTES;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub version: u8,
    pub source: PathBuf,
    pub sql: String,
    pub connector: String,
    pub fingerprint: String,
    pub params: BTreeMap<String, Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Reply {
    Complete {
        rows: Vec<Value>,
        schema: Vec<SchemaField>,
    },
    Failed {
        error: Failure,
    },
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Failure {
    Query(SqlError),
    Metadata,
    Parameters,
    Values,
    Rows,
    Timeout,
    Memory,
    Sandbox,
    KernelTooOld,
    Terminated,
    Input,
    Output,
    /// The host's own refusals, before any worker starts; a worker cannot send them.
    #[serde(skip)]
    PendingWal,
    #[cfg(windows)]
    #[serde(skip)]
    CopyTooLarge,
    #[cfg(windows)]
    #[serde(skip)]
    CopyNoSpace,
}

pub(super) fn read(mut source: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    (&mut source)
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::other(
            "SQL worker message exceeds its byte limit",
        ));
    }
    Ok(bytes)
}

pub(super) fn encode(value: &impl Serialize) -> io::Result<Vec<u8>> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
                return Err(io::Error::other(
                    "SQL worker message exceeds its byte limit",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut out = Bounded(Vec::new());
    serde_json::to_writer(&mut out, value).map_err(io::Error::other)?;
    Ok(out.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_pipe_input_and_serialized_results_are_rejected() {
        assert!(read(io::repeat(0).take(MAX_BYTES as u64 + 1)).is_err());
        let text = "x".repeat(MAX_BYTES);
        assert!(encode(&text).is_err()); // JSON quotes also count toward the limit.
        assert_eq!(read(&b"{}"[..]).unwrap(), b"{}");
    }

    #[test]
    fn old_linux_kernel_failure_reaches_the_user() {
        let reply: Reply = serde_json::from_value(serde_json::json!({
            "status": "failed", "error": "kernel_too_old"
        }))
        .unwrap();
        let Reply::Failed { error } = reply else {
            panic!("expected a failure")
        };
        let message = super::super::connector_error(error, "local").to_string();
        assert!(
            message.contains("SQL data sources need Linux kernel 6.2 or newer"),
            "{message}"
        );
    }

    #[test]
    fn protocol_cannot_add_settings_or_choose_an_executable() {
        let request = serde_json::json!({"version":VERSION,"source":"/declared/source.duckdb","sql":"SELECT 1","connector":"local","fingerprint":"fingerprint","params":{}});
        assert!(serde_json::from_value::<Request>(request.clone()).is_ok());
        for (name, value) in [
            (
                "settings",
                serde_json::json!({"enable_external_access":true}),
            ),
            ("worker", Value::from("/bin/sh")),
        ] {
            let mut changed = request.clone();
            changed[name] = value;
            assert!(serde_json::from_value::<Request>(changed).is_err());
        }
        assert!(
            serde_json::from_value::<Reply>(
                serde_json::json!({"status":"failed","error":"raw-driver-text"})
            )
            .is_err()
        );
    }
}
