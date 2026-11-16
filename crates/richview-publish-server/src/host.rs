//! The protocol between the pure server and its host adapter.
//!
//! The adapter runs `handle` with the state it loaded and whatever it has fetched so far. A `Need` asks it to
//! fetch something and call again; a `Done` carries the response, the state to commit and the storage writes.
//! A call is a pure function of its inputs, so repeating it after answering a `Need` is always safe.
use crate::{ledger::State, respond};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::rc::Rc;

/// An HTTP failure with the status and the message a reader may see.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpError {
    pub status: u16,
    pub message: String,
}

/// Why a handler stopped early: it failed, or it needs the host to fetch something first.
#[derive(Clone, Debug, PartialEq)]
pub enum Stop {
    Http(HttpError),
    Need(Vec<Need>),
}
pub type R<T> = Result<T, Stop>;

/// A failure carrying `status` and `message`.
pub fn error(status: u16, message: &str) -> Stop {
    Stop::Http(HttpError {
        status,
        message: message.into(),
    })
}

/// A request already stripped of transport detail; header names are lowercase.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub method: String,
    pub host: String,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
impl Request {
    pub fn is_read(&self) -> bool {
        matches!(self.method.as_str(), "GET" | "HEAD")
    }
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
    /// The body as JSON, refused above `limit` bytes.
    pub fn json<T: DeserializeOwned>(&self, limit: usize) -> R<T> {
        if self.body.len() > limit {
            return Err(error(413, "Request exceeds the limit"));
        }
        serde_json::from_slice(&self.body).map_err(|_| error(400, "Invalid JSON"))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The deployment's settings; secrets such as the Git token stay in the adapter.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Config {
    pub publication_host: String,
    /// `owner/name` of the private content repository.
    pub repository: String,
    pub branch: String,
    pub content_root: String,
    /// The principal key of the one publishing service, such as `service:publisher`.
    pub publisher_identity: String,
    pub bootstrap_super_editor: String,
    #[serde(default)]
    pub bootstrap_github_login: Option<String>,
    #[serde(default)]
    pub bootstrap_commit: Option<String>,
}

/// An identity the adapter verified; the rules never see an unverified header.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Principal {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub service: bool,
}

/// A call to the Git host, relative to the configured repository (`""`, `/pulls/3`, `/git/refs`).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ForgeCall {
    pub method: String,
    pub path: String,
    pub body: Option<String>,
}
impl ForgeCall {
    /// The key under which the host returns this call's answer, so a repeated call is not repeated on the wire.
    pub fn key(&self) -> String {
        let body = self.body.as_deref().unwrap_or("");
        format!(
            "{} {} {}",
            self.method,
            self.path,
            hex(&Sha256::digest(body.as_bytes()))
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ForgeReply {
    pub status: u16,
    pub body: Value,
}

/// Something the host must fetch before the call can finish.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "need", rename_all = "snake_case")]
pub enum Need {
    /// The bytes at a storage key (`releases/<id>.json`, `objects/<sha256>`).
    Blob {
        key: String,
    },
    /// The size and SHA-256 recorded for a stored object, without its bytes.
    Head {
        key: String,
    },
    Forge(ForgeCall),
    /// The newest audit entries.
    Audit {
        limit: usize,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BlobHead {
    pub size: usize,
    pub sha256: String,
}

/// What the host has fetched so far.
#[derive(Clone, Debug, Default)]
pub struct Io {
    /// `None` records a key known to be absent.
    pub blobs: BTreeMap<String, Option<Rc<Vec<u8>>>>,
    pub heads: BTreeMap<String, Option<BlobHead>>,
    pub forge: BTreeMap<String, ForgeReply>,
    pub audit: Option<Vec<AuditEntry>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct AuditEntry {
    pub sequence: u64,
    pub at: String,
    pub actor: String,
    pub action: String,
    pub details: Value,
}

/// A storage write the host performs after the state commits.
#[derive(Clone, Debug, PartialEq)]
pub enum Write {
    /// Store `bytes` at `key`; with `if_absent` an existing key is left alone.
    Blob {
        key: String,
        bytes: Vec<u8>,
        if_absent: bool,
        sha256: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct Done {
    pub response: Response,
    /// The state to commit, or `None` when the call changed nothing.
    pub state: Option<State>,
    pub audit: Vec<AuditEntry>,
    pub writes: Vec<Write>,
}
#[derive(Clone, Debug)]
pub enum Outcome {
    Done(Box<Done>),
    Need(Vec<Need>),
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// Everything a handler may read, and the one place it records changes.
pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub principal: Option<&'a Principal>,
    /// The state as the call began; checks read this.
    pub state: &'a State,
    pub io: &'a Io,
    /// RFC 3339 time and a fresh random token, both from the host so the rules stay deterministic.
    pub now: &'a str,
    pub nonce: &'a str,
    /// The state to commit; handlers change this through `change`.
    pub next: State,
    pub audit: Vec<AuditEntry>,
    pub writes: Vec<Write>,
}
impl<'a> Ctx<'a> {
    pub fn new(
        cfg: &'a Config,
        principal: Option<&'a Principal>,
        state: &'a State,
        io: &'a Io,
        now: &'a str,
        nonce: &'a str,
    ) -> Self {
        Ctx {
            cfg,
            principal,
            state,
            io,
            now,
            nonce,
            next: state.clone(),
            audit: vec![],
            writes: vec![],
        }
    }
    pub fn actor_key(&self) -> String {
        self.principal.map_or("system".into(), |p| p.key.clone())
    }
    /// Records one audited change: bumps the audit sequence, and the access revision for actions that change who may read.
    pub fn change(&mut self, action: &str, details: Value) {
        self.next.audit_sequence += 1;
        if [
            "release.activated",
            "release.rollback",
            "item.hidden",
            "item.restored",
            "item.published",
            "role.updated",
        ]
        .contains(&action)
        {
            self.next.access_revision += 1;
        }
        self.audit.push(AuditEntry {
            sequence: self.next.audit_sequence,
            at: self.now.into(),
            actor: self.actor_key(),
            action: action.into(),
            details,
        });
    }
    pub fn blob(&self, key: &str) -> R<Rc<Vec<u8>>> {
        match self.io.blobs.get(key) {
            Some(Some(bytes)) => Ok(bytes.clone()),
            Some(None) => Err(error(404, "Not found")),
            None => Err(Stop::Need(vec![Need::Blob { key: key.into() }])),
        }
    }
    pub fn blob_text(&self, key: &str) -> R<String> {
        String::from_utf8(self.blob(key)?.to_vec())
            .map_err(|_| error(503, "Release object unavailable"))
    }
    pub fn head(&self, key: &str) -> R<Option<BlobHead>> {
        match self.io.heads.get(key) {
            Some(head) => Ok(head.clone()),
            None => Err(Stop::Need(vec![Need::Head { key: key.into() }])),
        }
    }
    /// Calls the Git host; GitHub's 404 stays 404, 409 and 422 become 409, anything else 502.
    pub fn forge(&self, method: &str, path: &str, body: Option<Value>) -> R<Value> {
        let call = ForgeCall {
            method: method.into(),
            path: path.into(),
            body: body.map(|v| v.to_string()),
        };
        match self.io.forge.get(&call.key()) {
            None => Err(Stop::Need(vec![Need::Forge(call)])),
            Some(reply) if (200..300).contains(&reply.status) => Ok(reply.body.clone()),
            Some(reply) => Err(error(
                match reply.status {
                    404 => 404,
                    409 | 422 => 409,
                    _ => 502,
                },
                &format!(
                    "Git host {method} {} returned {}",
                    path.split('?').next().unwrap_or(""),
                    reply.status
                ),
            )),
        }
    }
    /// Asks for every missing GET answer at once, so a loop over many calls costs one round trip, not one per call.
    pub fn prefetch(&self, paths: &[String]) -> R<()> {
        let missing: Vec<Need> = paths
            .iter()
            .map(|path| ForgeCall {
                method: "GET".into(),
                path: path.clone(),
                body: None,
            })
            .filter(|call| !self.io.forge.contains_key(&call.key()))
            .map(Need::Forge)
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Stop::Need(missing))
        }
    }
    pub fn put_blob(
        &mut self,
        key: String,
        bytes: Vec<u8>,
        if_absent: bool,
        sha256: Option<String>,
    ) {
        self.writes.push(Write::Blob {
            key,
            bytes,
            if_absent,
            sha256,
        });
    }
}

/// Turns a handler's result into an outcome: a response, a request for more data, or a failure page.
pub fn finish(ctx: Ctx, result: R<Response>) -> Outcome {
    match result {
        Ok(response) => {
            let changed = !ctx.audit.is_empty() || ctx.next != *ctx.state;
            Outcome::Done(Box::new(Done {
                response,
                state: changed.then(|| {
                    let mut next = ctx.next;
                    next.revision += 1;
                    next
                }),
                audit: ctx.audit,
                writes: ctx.writes,
            }))
        }
        Err(Stop::Need(needs)) => Outcome::Need(needs),
        Err(Stop::Http(e)) => Outcome::Done(Box::new(Done {
            response: respond::failure(e.status, &e.message),
            state: None,
            audit: vec![],
            writes: vec![],
        })),
    }
}
