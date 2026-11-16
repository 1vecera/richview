//! The JSON form of the host protocol, for adapters that cannot share Rust types (the Cloudflare Worker).
//!
//! One call in, one answer out; byte bodies travel as base64. The adapter loops: call `handle_json`, answer
//! each need, call again, and on `done` commit the state and writes.
use crate::{host::*, ledger::State};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::rc::Rc;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in text.bytes().filter(|c| *c != b'=') {
        acc = (acc << 6) | ALPHABET.iter().position(|a| *a == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[derive(Deserialize)]
struct WireRequest {
    method: String,
    host: String,
    path: String,
    #[serde(default)]
    query: BTreeMap<String, String>,
    #[serde(default)]
    headers: BTreeMap<String, String>,
    #[serde(default)]
    body: String,
}
#[derive(Deserialize, Default)]
struct WireIo {
    #[serde(default)]
    blobs: BTreeMap<String, Option<String>>,
    #[serde(default)]
    heads: BTreeMap<String, Option<BlobHead>>,
    #[serde(default)]
    forge: BTreeMap<String, ForgeReply>,
    #[serde(default)]
    audit: Option<Vec<AuditEntry>>,
}
#[derive(Deserialize)]
struct Input {
    config: Config,
    principal: Option<Principal>,
    state: State,
    #[serde(default)]
    io: WireIo,
    now: String,
    nonce: String,
    request: WireRequest,
}

/// Runs one call; the answer is `{"kind":"done"|"need"|"invalid", ...}`.
pub fn handle_json(input: &str) -> String {
    let Ok(input) = serde_json::from_str::<Input>(input) else {
        return json!({"kind": "invalid"}).to_string();
    };
    let Some(body) = decode(&input.request.body) else {
        return json!({"kind": "invalid"}).to_string();
    };
    let mut io = Io {
        heads: input.io.heads,
        forge: input.io.forge,
        audit: input.io.audit,
        ..Io::default()
    };
    for (key, value) in input.io.blobs {
        io.blobs
            .insert(key, value.and_then(|v| decode(&v)).map(Rc::new));
    }
    let request = Request {
        method: input.request.method,
        host: input.request.host,
        path: input.request.path,
        query: input.request.query,
        headers: input
            .request
            .headers
            .into_iter()
            .map(|(k, v)| (k.to_lowercase(), v))
            .collect(),
        body,
    };
    let outcome = crate::handle(
        &input.config,
        input.principal.as_ref(),
        &input.state,
        &io,
        &input.now,
        &input.nonce,
        &request,
    );
    match outcome {
        Outcome::Need(needs) => {
            let needs: Vec<Value> = needs
                .iter()
                .map(|need| match need {
                    Need::Forge(call) => json!({"need": "forge", "key": call.key(), "method": call.method, "path": call.path, "body": call.body}),
                    other => serde_json::to_value(other).unwrap_or(Value::Null),
                })
                .collect();
            json!({"kind": "need", "needs": needs}).to_string()
        }
        Outcome::Done(done) => json!({
            "kind": "done",
            "response": {
                "status": done.response.status,
                "headers": done.response.headers,
                "body": encode(&done.response.body),
            },
            "state": done.state,
            "audit": done.audit,
            "writes": done.writes.iter().map(|Write::Blob { key, bytes, if_absent, sha256 }| json!({
                "key": key, "bytes": encode(bytes), "if_absent": if_absent, "sha256": sha256,
            })).collect::<Vec<_>>(),
        })
        .to_string(),
    }
}

/// Converts PR #70's stored ledger: `{"state":…,"activated":{id:record},"runtimes":{id:map}}` in,
/// `{"kind":"migrated","state":…}` or `{"kind":"invalid","error":…}` out.
pub fn migrate_json(input: &str) -> String {
    #[derive(Deserialize)]
    struct Old {
        state: Value,
        #[serde(default)]
        activated: BTreeMap<String, Value>,
        #[serde(default)]
        runtimes: BTreeMap<String, Value>,
    }
    let result = serde_json::from_str::<Old>(input)
        .map_err(|e| e.to_string())
        .and_then(|old| crate::migrate::from_pr70(&old.state, &old.activated, &old.runtimes));
    match result {
        Ok(state) => json!({"kind": "migrated", "state": state}).to_string(),
        Err(error) => json!({"kind": "invalid", "error": error}).to_string(),
    }
}
