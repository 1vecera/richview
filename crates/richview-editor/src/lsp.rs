//! The editor service: the Language Server Protocol over stdio, plus the `richview/*` methods
//! (ARC-44, TYP-98). It keeps the open documents, validates them 250 ms after the last change and
//! answers completion, formatting, outline, folding and hover from unsaved, unfinished source.
//! Validation, export and publishing call the same operations as the CLI (ARC-08).
use crate::{authoring, snippets};
use richview_app::ops::{self, Failure, Input};
use richview_core::document::Diagnostic;
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    io::{self, BufRead, Write},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

/// Largest request the service reads (TYP-43).
pub const MAX_REQUEST_BYTES: usize = 24 * 1024 * 1024;
const LINT_DELAY: Duration = Duration::from_millis(250);

enum Incoming {
    Message(Value),
    /// A frame the reader could not accept; the service answers and stops.
    Invalid(&'static str),
    Eof,
}

/// Reads one `Content-Length` framed message.
fn read_frame(input: &mut impl BufRead) -> io::Result<Option<Result<Value, &'static str>>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let Some(length) = length else {
        return Ok(Some(Err("Missing Content-Length header")));
    };
    if length > MAX_REQUEST_BYTES {
        return Ok(Some(Err("Editor request exceeds 24 MiB")));
    }
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    Ok(Some(
        serde_json::from_slice(&body).map_err(|_| "Invalid JSON request"),
    ))
}

fn write_message(output: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

struct Document {
    text: String,
    version: i64,
}

struct Server<W: Write> {
    out: W,
    documents: HashMap<String, Document>,
    /// When each document is due to be validated.
    due: HashMap<String, Instant>,
    /// The last preview that rendered, per document: a render that fails keeps it under the
    /// findings banner (FEA-SF-23).
    last_good: HashMap<String, String>,
    shutdown: bool,
}

/// Runs the service on standard input and output until `exit` or end of input.
pub fn run() -> io::Result<()> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut input = io::BufReader::new(io::stdin().lock());
        loop {
            let message = match read_frame(&mut input) {
                Ok(Some(Ok(value))) => Incoming::Message(value),
                Ok(Some(Err(reason))) => {
                    let _ = sender.send(Incoming::Invalid(reason));
                    return;
                }
                Ok(None) | Err(_) => {
                    let _ = sender.send(Incoming::Eof);
                    return;
                }
            };
            if sender.send(message).is_err() {
                return;
            }
        }
    });
    let mut server = Server {
        out: io::BufWriter::new(io::stdout().lock()),
        documents: HashMap::new(),
        due: HashMap::new(),
        last_good: HashMap::new(),
        shutdown: false,
    };
    server.serve(receiver)
}

impl<W: Write> Server<W> {
    fn serve(&mut self, receiver: mpsc::Receiver<Incoming>) -> io::Result<()> {
        let mut queue: VecDeque<Incoming> = VecDeque::new();
        loop {
            while let Ok(message) = receiver.try_recv() {
                queue.push_back(message);
            }
            let next = match queue.pop_front() {
                Some(message) => message,
                None => {
                    let wait = self
                        .due
                        .values()
                        .min()
                        .map_or(Duration::from_secs(3600), |at| {
                            at.saturating_duration_since(Instant::now())
                        });
                    match receiver.recv_timeout(wait) {
                        Ok(message) => message,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            self.lint_due()?;
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
                    }
                }
            };
            match next {
                Incoming::Eof => return Ok(()),
                Incoming::Invalid(reason) => {
                    write_message(
                        &mut self.out,
                        &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":reason}}),
                    )?;
                    return Ok(());
                }
                Incoming::Message(message) => {
                    // A preview that a later preview of the same document supersedes is not
                    // rendered: renders coalesce to the latest edit (ARC-44).
                    if message["method"] == "richview/preview" {
                        let uri = message["params"]["uri"].clone();
                        let newer = |m: &Incoming| matches!(m, Incoming::Message(m) if m["method"] == "richview/preview" && m["params"]["uri"] == uri);
                        while let Ok(m) = receiver.try_recv() {
                            queue.push_back(m);
                        }
                        if queue.iter().any(newer) {
                            self.respond(&message["id"], Ok(json!({"cancelled": true})))?;
                            continue;
                        }
                    }
                    if self.handle(&message)? {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn respond(&mut self, id: &Value, result: Result<Value, RpcError>) -> io::Result<()> {
        if id.is_null() {
            return Ok(());
        }
        let message = match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(RpcError {
                code,
                message,
                data,
            }) => {
                let mut error = json!({"code": code, "message": message});
                if let Some(data) = data {
                    error["data"] = data;
                }
                json!({"jsonrpc":"2.0","id":id,"error":error})
            }
        };
        write_message(&mut self.out, &message)
    }

    fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        write_message(
            &mut self.out,
            &json!({"jsonrpc":"2.0","method":method,"params":params}),
        )
    }

    /// Handles one message; true means the service should stop.
    fn handle(&mut self, message: &Value) -> io::Result<bool> {
        let method = message["method"].as_str().unwrap_or_default();
        let params = &message["params"];
        let id = &message["id"];
        match method {
            "initialize" => self.respond(id, Ok(capabilities()))?,
            "initialized" | "$/cancelRequest" | "workspace/didChangeConfiguration" => {}
            "shutdown" => {
                self.shutdown = true;
                self.respond(id, Ok(Value::Null))?
            }
            "exit" => return Ok(true),
            "textDocument/didOpen" => {
                let item = &params["textDocument"];
                if let Some(uri) = item["uri"].as_str() {
                    self.documents.insert(
                        uri.to_owned(),
                        Document {
                            text: item["text"].as_str().unwrap_or_default().to_owned(),
                            version: item["version"].as_i64().unwrap_or(0),
                        },
                    );
                    self.due.insert(uri.to_owned(), Instant::now());
                }
            }
            "textDocument/didChange" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str()
                    && let Some(document) = self.documents.get_mut(uri)
                {
                    for change in params["contentChanges"].as_array().into_iter().flatten() {
                        apply_change(&mut document.text, change);
                    }
                    document.version = params["textDocument"]["version"]
                        .as_i64()
                        .unwrap_or(document.version);
                    self.due.insert(uri.to_owned(), Instant::now() + LINT_DELAY);
                }
            }
            "textDocument/didSave" | "workspace/didChangeWatchedFiles" => {
                // A saved or externally changed file may be another open document's dependency.
                let now = Instant::now();
                for uri in self.documents.keys().cloned().collect::<Vec<_>>() {
                    self.due.insert(uri, now);
                }
            }
            "textDocument/didClose" => {
                if let Some(uri) = params["textDocument"]["uri"].as_str() {
                    self.documents.remove(uri);
                    self.due.remove(uri);
                    self.last_good.remove(uri);
                    self.notify(
                        "textDocument/publishDiagnostics",
                        json!({"uri":uri,"diagnostics":[]}),
                    )?;
                }
            }
            _ if message.get("id").is_some() => {
                let result = self.request(method, params);
                self.respond(id, result)?
            }
            _ => {}
        }
        // A request after `shutdown` is invalid, but the client decides when to `exit`.
        let _ = self.shutdown;
        Ok(false)
    }

    fn text_of(&self, params: &Value) -> Result<(&str, String), (i64, String)> {
        let uri = params["textDocument"]["uri"]
            .as_str()
            .or_else(|| params["uri"].as_str())
            .ok_or((-32602, "Missing document uri".to_owned()))?;
        let document = self
            .documents
            .get(uri)
            .ok_or((-32602, format!("{uri} is not open")))?;
        Ok((&document.text, uri.to_owned()))
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "textDocument/completion" => {
                let (text, uri) = self.text_of(params)?;
                Ok(completion(text, &uri, params))
            }
            "textDocument/formatting" => {
                let (text, _) = self.text_of(params)?;
                let formatted = authoring::format(text);
                if formatted == text {
                    return Ok(json!([]));
                }
                Ok(json!([{"range":whole(text),"newText":formatted}]))
            }
            "textDocument/documentSymbol" => {
                let (text, _) = self.text_of(params)?;
                Ok(document_symbols(&authoring::outline(text)))
            }
            "textDocument/foldingRange" => {
                let (text, _) = self.text_of(params)?;
                let outline = authoring::outline(text);
                Ok(Value::Array(
                    outline["folds"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|f| json!({"startLine":f["start"],"endLine":f["end"]}))
                        .collect(),
                ))
            }
            "textDocument/hover" => {
                let (text, _) = self.text_of(params)?;
                Ok(hover(
                    text,
                    params["position"]["line"].as_u64().unwrap_or(0) as usize,
                    params["position"]["character"].as_u64().unwrap_or(0) as usize,
                ))
            }
            "richview/snippets" => Ok(snippets()),
            "richview/preview" => {
                let (text, uri) = self.text_of(params)?;
                let text = text.to_owned();
                match preview(&text, &uri, params) {
                    Ok(mut result) => {
                        if let Some(html) = result["html"].as_str() {
                            self.last_good.insert(uri.clone(), html.to_owned());
                        }
                        let diagnostics: Vec<Diagnostic> =
                            serde_json::from_value(result["diagnostics"].clone())
                                .unwrap_or_default();
                        let html = result["html"].as_str().unwrap_or_default();
                        result["html"] = json!(richview_app::banner::inject(html, &diagnostics));
                        Ok(result)
                    }
                    Err(failure) => match self.last_good.get(&uri) {
                        // The last good render stays, with the reason on top; nothing is written.
                        Some(html) => {
                            let diagnostics = match failure {
                                Failure::Diagnostics(diagnostics) => diagnostics,
                                other => richview_app::banner::diagnostics_of(&other.message()),
                            };
                            let html = richview_app::banner::inject(html, &diagnostics);
                            Ok(richview_core::document::operation_result(
                                diagnostics,
                                json!({
                                    "html": html,
                                    "title": "RichView Preview ✕",
                                    "severity": "error",
                                    "message": "RichView: the last good render is shown",
                                    "failed": true,
                                }),
                            ))
                        }
                        None => Err(failure.into()),
                    },
                }
            }
            "richview/export" => self.operation(params, |input, params| async move {
                let pdf = params["output"] == "pdf";
                ops::generate(&ops::GenerateRequest {
                    input,
                    output: None,
                    pdf,
                    page: if params["page"] == "letter" {
                        richview_core::document::Page::Letter
                    } else {
                        richview_core::document::Page::A4
                    },
                    browser: None,
                    settle_timeout: Duration::from_secs(30),
                })
                .await
            }),
            "richview/publish" => self.operation(params, |input, params| async move {
                let server = params["server"]
                    .as_str()
                    .ok_or_else(|| Failure::usage("Missing server URL."))?
                    .to_owned();
                let token = params["token"]
                    .as_str()
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| Failure::usage("Missing publish token."))?
                    .to_owned();
                ops::publish(
                    &input,
                    &server,
                    &token,
                    params["slug"].as_str(),
                    params["new"].as_bool().unwrap_or(false),
                )
                .await
            }),
            other => Err((-32601, format!("Method not found: {other}")).into()),
        }
    }

    /// Runs an async operation on the saved file of the document named in `params`.
    fn operation<F, Fut>(&self, params: &Value, run: F) -> Result<Value, RpcError>
    where
        F: FnOnce(Input, Value) -> Fut,
        Fut: std::future::Future<Output = ops::Outcome>,
    {
        let uri = params["uri"]
            .as_str()
            .ok_or((-32602, "Missing document uri".to_owned()))?;
        let path = path_of(uri).ok_or((
            -32602,
            "Save the report in its own folder first.".to_owned(),
        ))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| (-32603, e.to_string()))?;
        let input = Input {
            path,
            ..Default::default()
        };
        Ok(runtime.block_on(run(input, params.clone()))?)
    }

    fn lint_due(&mut self) -> io::Result<()> {
        let now = Instant::now();
        let due: Vec<String> = self
            .due
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(uri, _)| uri.clone())
            .collect();
        for uri in due {
            self.due.remove(&uri);
            let Some(document) = self.documents.get(&uri) else {
                continue;
            };
            let diagnostics = lint(&document.text, &uri);
            let version = document.version;
            self.notify(
                "textDocument/publishDiagnostics",
                json!({"uri":uri,"version":version,"diagnostics":diagnostics}),
            )?;
        }
        Ok(())
    }
}

fn empty_document() -> richview_core::document::Document {
    richview_core::document::parse("").expect("empty source parses")
}

/// A JSON-RPC error. A failed operation carries its result object in `data`, with `ok`, `status`,
/// `output` and the diagnostics as every surface returns them (TYP-53, TYP-54); `message` says
/// the same in words for a client that shows only that.
struct RpcError {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl From<(i64, String)> for RpcError {
    fn from((code, message): (i64, String)) -> Self {
        Self {
            code,
            message,
            data: None,
        }
    }
}

impl From<Failure> for RpcError {
    fn from(failure: Failure) -> Self {
        // Each error finding as `code: message`, else the failure's one sentence.
        let errors: Vec<String> = failure
            .diagnostics()
            .iter()
            .filter(|d| d.level == "error")
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        Self {
            code: -32000,
            message: if errors.is_empty() {
                failure.message()
            } else {
                errors.join("\n")
            },
            data: Some(failure.to_json()),
        }
    }
}

fn capabilities() -> Value {
    json!({
        "capabilities": {
            "textDocumentSync": {"openClose": true, "change": 2, "save": {"includeText": false}},
            "completionProvider": {"triggerCharacters": [":", " ", "{", ",", "[", "-"]},
            "documentFormattingProvider": true,
            "documentSymbolProvider": true,
            "foldingRangeProvider": true,
            "hoverProvider": true
        },
        "serverInfo": {"name": "richview-editor", "version": richview_app::VERSION}
    })
}

/// The file path of a `file:` URI, percent-decoded.
pub fn path_of(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(drive_path(
        String::from_utf8(decoded).ok()?,
        cfg!(windows),
    )))
}

/// On Windows a file URI's path starts with a slash before the drive, as VS Code writes
/// `file:///c%3A/Users/…`; the drive path starts at the letter.
fn drive_path(path: String, windows: bool) -> String {
    let bytes = path.as_bytes();
    if windows
        && bytes.len() >= 3
        && bytes[0] == b'/'
        && bytes[1].is_ascii_alphabetic()
        && bytes[2] == b':'
    {
        path[1..].to_owned()
    } else {
        path
    }
}

/// Byte offset of an LSP position (line, UTF-16 code units).
pub fn offset(text: &str, line: usize, character: usize) -> usize {
    let mut start = 0;
    for (n, l) in text.split_inclusive('\n').enumerate() {
        if n == line {
            let mut units = 0;
            for (index, c) in l.char_indices() {
                if units >= character || c == '\n' || (c == '\r' && l[index..].starts_with("\r\n"))
                {
                    return start + index;
                }
                units += c.len_utf16();
            }
            return start + l.trim_end_matches(['\r', '\n']).len();
        }
        start += l.len();
    }
    text.len()
}

fn apply_change(text: &mut String, change: &Value) {
    let new = change["text"].as_str().unwrap_or_default();
    match change.get("range").filter(|r| r.is_object()) {
        None => *text = new.to_owned(),
        Some(range) => {
            let at = |p: &Value| {
                offset(
                    text,
                    p["line"].as_u64().unwrap_or(0) as usize,
                    p["character"].as_u64().unwrap_or(0) as usize,
                )
            };
            let (start, end) = (at(&range["start"]), at(&range["end"]));
            if start <= end && end <= text.len() {
                text.replace_range(start..end, new);
            }
        }
    }
}

/// The range covering the whole text.
fn whole(text: &str) -> Value {
    let line = text.matches('\n').count();
    let last = text.rsplit('\n').next().unwrap_or("");
    json!({"start":{"line":0,"character":0},"end":{"line":line,"character":last.encode_utf16().count()}})
}

fn severity(level: &str) -> u8 {
    match level {
        "error" => 1,
        "warning" => 2,
        _ => 3,
    }
}

/// The diagnostics of `richview validate` for this text, as LSP diagnostics with ranges
/// (FEA-SF-24).
pub fn lint(text: &str, uri: &str) -> Value {
    let (issues, parsed) = match path_of(uri) {
        Some(path) => {
            let input = Input {
                path,
                ..Default::default()
            };
            let issues = match ops::validate_source(text, &input, false) {
                Ok(result) => {
                    serde_json::from_value::<Vec<Diagnostic>>(result["diagnostics"].clone())
                        .unwrap_or_default()
                }
                Err(failure) => match failure {
                    Failure::Diagnostics(d) => d,
                    other => vec![Diagnostic::new(
                        "error",
                        "editor.service",
                        other.message(),
                        "source",
                    )],
                },
            };
            (issues, richview_core::document::parse(text).ok())
        }
        None => match richview_core::document::parse(text) {
            Ok(document) => (document.diagnostics.clone(), Some(document)),
            Err(diagnostics) => (diagnostics, None),
        },
    };
    let parsed = parsed.unwrap_or_else(empty_document);
    let located = authoring::diagnostics(text, issues, &parsed, &Default::default());
    Value::Array(
        located
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| {
                let line = d["line"].as_u64().unwrap_or(1).saturating_sub(1);
                json!({
                    "range": d.get("range").filter(|r| r.is_object()).cloned()
                        .unwrap_or_else(|| json!({"start":{"line":line,"character":0},"end":{"line":line,"character":0}})),
                    "severity": severity(d["level"].as_str().unwrap_or("info")),
                    "code": d["code"],
                    "source": "RichView",
                    "message": d["message"],
                    // LSP has no slot for these; every field of a diagnostic travels unchanged (TYP-53).
                    "data": {
                        "path": d["path"], "tag": d["tag"], "fix": d["fix"],
                        "expected": d["expected"], "retry": d["retry"], "details": d["details"],
                        "file": d["file"], "column": d["column"],
                    },
                })
            })
            .collect(),
    )
}

fn completion(text: &str, uri: &str, params: &Value) -> Value {
    let line = params["position"]["line"].as_u64().unwrap_or(0) as usize;
    let character = params["position"]["character"].as_u64().unwrap_or(0) as usize;
    let parsed = richview_core::document::parse(text).unwrap_or_else(|_| empty_document());
    let mut options = richview_core::document::RenderOptions::default();
    if let Some(path) = path_of(uri)
        && let Ok((_, resolved)) =
            richview_app::resolver::resolve_document_options_policy(text, &path, &[], false, false)
    {
        options = resolved;
    }
    let items = authoring::complete(text, line, character, &parsed, &options);
    Value::Array(
        items
            .as_array()
            .into_iter()
            .flatten()
            .map(|item| {
                json!({
                    "label": item["label"],
                    "kind": if item["kind"] == "key" { 10 } else { 12 },
                    "textEdit": {"range": item["range"], "newText": item["insert_text"]},
                })
            })
            .collect(),
    )
}

fn symbol(item: &Value) -> Value {
    let kind = match item["detail"].as_str().unwrap_or_default() {
        "chart" => 19,
        "table" => 18,
        "big" | "metric" => 16,
        "image" | "audio" | "video" => 1,
        _ if item["kind"] == "heading" => 15,
        _ => 19,
    };
    json!({
        "name": item["name"],
        "detail": item["detail"],
        "kind": kind,
        "range": item["range"],
        "selectionRange": item["selectionRange"],
        "children": item["children"].as_array().into_iter().flatten().map(symbol).collect::<Vec<_>>(),
    })
}

fn document_symbols(outline: &Value) -> Value {
    Value::Array(
        outline["symbols"]
            .as_array()
            .into_iter()
            .flatten()
            .map(symbol)
            .collect(),
    )
}

/// The key or value under the cursor: its one-line description and allowed values from the
/// authoring vocabulary (FEA-SF-25). Nothing else is shown.
fn hover(text: &str, line: usize, character: usize) -> Value {
    let lines: Vec<&str> = text.lines().collect();
    let Some(current) = lines.get(line) else {
        return Value::Null;
    };
    let mut at = (offset(text, line, character) - offset(text, line, 0)).min(current.len());
    while !current.is_char_boundary(at) {
        at -= 1;
    }
    let is_word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '-');
    let start = current[..at]
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_word(c))
        .map_or(0, |(i, c)| i + c.len_utf8());
    let end = current[at..]
        .find(|c: char| !is_word(c))
        .map_or(current.len(), |i| at + i);
    let word = &current[start..end];
    if word.is_empty() {
        return Value::Null;
    }
    // The block the line belongs to: the nearest open fence above, or the frontmatter.
    let mut tag = None;
    let mut in_front = false;
    for (n, l) in lines.iter().enumerate().take(line + 1) {
        if n == 0 && l.trim_end() == "---" {
            in_front = true;
            tag = Some("frontmatter".to_owned());
        } else if in_front && l.trim_end() == "---" {
            in_front = false;
            tag = None;
        } else if let Some(info) = l.strip_prefix("```") {
            tag = if tag.is_some() && !in_front && info.trim().is_empty() {
                None
            } else {
                Some(info.trim().to_lowercase())
            };
        }
    }
    let Some(tag) = tag.filter(|t| !t.is_empty()) else {
        return Value::Null;
    };
    // Parent keys by indentation.
    let mut path: Vec<String> = vec![];
    let mut indent = current.len() - current.trim_start().len();
    for l in lines[..line].iter().rev() {
        // The dash precedes a list item's first key; later fields at that key's indentation
        // are siblings, rather than children of it.
        let i = l.len() - l.trim_start().len() + usize::from(l.trim_start().starts_with("- ")) * 2;
        if i < indent
            && let Some((key, _)) = l.trim_start().trim_start_matches("- ").split_once(':')
        {
            path.insert(0, key.trim().to_owned());
            indent = i;
        }
    }
    let path_refs: Vec<&str> = path.iter().map(String::as_str).collect();
    let is_key = current[end..].trim_start().starts_with(':');
    let description = if is_key {
        richview_core::authoring_keys(&tag, &path_refs)
            .filter(|keys| keys.contains(&word))
            .map(|_| {
                let values =
                    richview_core::authoring_values(&tag, &[&path_refs[..], &[word]].concat());
                let allowed = if values.is_empty() {
                    String::new()
                } else {
                    format!("\n\nAllowed values: {}", values.join(", "))
                };
                format!("`{word}`: a field of `{tag}`.{allowed}")
            })
    } else {
        let key = path_refs.split_last().map(|(k, rest)| (*k, rest.to_vec()));
        let here = current.trim_start().trim_start_matches("- ");
        let owner = here.split_once(':').map(|(k, _)| k.trim().to_owned());
        let (key, parents) = match (owner, key) {
            (Some(owner), _) => (owner, path_refs.clone()),
            (None, Some((k, rest))) => (k.to_owned(), rest),
            _ => return Value::Null,
        };
        let values =
            richview_core::authoring_values(&tag, &[&parents[..], &[key.as_str()]].concat());
        values.contains(&word).then(|| {
            format!(
                "`{word}`: a value of `{key}`.\n\nAllowed values: {}",
                values.join(", ")
            )
        })
    };
    match description {
        Some(markdown) => json!({
            "contents": {"kind": "markdown", "value": markdown},
            "range": {"start":{"line":line,"character":current[..start].encode_utf16().count()},"end":{"line":line,"character":current[..end].encode_utf16().count()}}
        }),
        None => Value::Null,
    }
}

/// The preview of the open text: its HTML and the findings, with the title VS Code shows.
fn preview(text: &str, uri: &str, params: &Value) -> Result<Value, Failure> {
    let path = path_of(uri).ok_or_else(|| {
        Failure::Diagnostics(vec![Diagnostic::new(
            "error",
            "report.root.missing",
            "Save one .rmd in its own dedicated folder before previewing.",
            "report_folder",
        )])
    })?;
    // Only the trusted editor host supplies this URL; report metadata cannot select code.
    let runtime_url = params["runtimeUrl"].as_str().map(String::from);
    let (document, mut options) = richview_app::resolver::resolve_document_options_policy(
        text,
        &path,
        &[],
        false,
        runtime_url.is_none(),
    )
    .map_err(Failure::from)?;
    options.runtime_url = runtime_url;
    if options.runtime_url.is_some() {
        // A linked preview is only valid when the host supplies the matching bindings.
        options.runtime_js = params["runtimeJs"].as_str().map(String::from);
    }
    let report = richview_core::document::render_document(document, &options)
        .map_err(Failure::Diagnostics)?;
    let count = |level: &str| {
        report
            .diagnostics
            .iter()
            .filter(|d| d.level == level)
            .count()
    };
    let (errors, warnings) = (count("error"), count("warning"));
    let charts = report.metadata["_richview"]["chart_count"]
        .as_u64()
        .unwrap_or(report.charts.len() as u64);
    // A preview writes nothing, so its `output` is null (TYP-54).
    Ok(richview_core::document::operation_result(
        report.diagnostics,
        json!({
            "html": report.html,
            "charts": report.metadata["_richview"]["chart_diagnostics"],
            "title": if errors > 0 { "RichView Preview ✕" } else if warnings > 0 { "RichView Preview ▲" } else { "RichView Preview ✓" },
            "severity": if errors > 0 { "error" } else if warnings > 0 { "warning" } else { "information" },
            "message": format!("RichView: {charts} chart(s), {errors} error(s), {warnings} warning(s)"),
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_count_utf16_units_and_incremental_edits_apply_in_order() {
        let mut text = "a😀b\nsecond\n".to_owned();
        assert_eq!(offset(&text, 0, 3), "a😀".len());
        assert_eq!(offset(&text, 1, 2), "a😀b\nse".len());
        apply_change(
            &mut text,
            &json!({"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":3}},"text":"X"}),
        );
        assert_eq!(text, "aXb\nsecond\n");
        apply_change(&mut text, &json!({"text":"fresh"}));
        assert_eq!(text, "fresh");
        assert_eq!(whole("a\nbc")["end"], json!({"line":1,"character":2}));
    }

    #[test]
    fn uris_decode_to_paths() {
        assert_eq!(
            path_of("file:///tmp/my%20report/report.rmd"),
            Some(PathBuf::from("/tmp/my report/report.rmd"))
        );
        assert_eq!(path_of("untitled:Untitled-1"), None);
        assert_eq!(
            drive_path("/c:/Users/a b/report.rmd".into(), true),
            "c:/Users/a b/report.rmd"
        );
        assert_eq!(
            drive_path("/c:/Users/report.rmd".into(), false),
            "/c:/Users/report.rmd"
        );
        assert_eq!(
            drive_path("/tmp/report.rmd".into(), true),
            "/tmp/report.rmd"
        );
    }

    #[test]
    fn frames_read_and_write_with_content_length() {
        let mut bytes = Vec::new();
        write_message(&mut bytes, &json!({"jsonrpc":"2.0","method":"exit"})).unwrap();
        let frame = read_frame(&mut io::BufReader::new(bytes.as_slice()))
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(frame["method"], "exit");
        let big = format!("Content-Length: {}\r\n\r\n", MAX_REQUEST_BYTES + 1);
        assert!(
            read_frame(&mut io::BufReader::new(big.as_bytes()))
                .unwrap()
                .unwrap()
                .is_err()
        );
    }

    #[test]
    fn watched_dependencies_update_validation_without_changing_the_document() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tmp")
            .join(format!("editor-watched-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let source = "```chart\ntype: bar\ntitle: Watched data\ndata: {csv: data.csv}\nencoding: {x: month, y: value}\n```\n";
        std::fs::write(root.join("report.rmd"), source).unwrap();
        std::fs::write(root.join("data.csv"), "month,value\nJan,100\n").unwrap();
        let uri = format!("file://{}", root.join("report.rmd").display());
        let dependency = format!("file://{}", root.join("data.csv").display());
        let mut server = Server {
            out: Vec::new(),
            documents: HashMap::new(),
            due: HashMap::new(),
            last_good: HashMap::new(),
            shutdown: false,
        };
        server.handle(&json!({"method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"text":source,"version":7}}})).unwrap();
        let diagnostics = |server: &mut Server<Vec<u8>>| {
            server.lint_due().unwrap();
            let bytes = std::mem::take(&mut server.out);
            let message = read_frame(&mut io::BufReader::new(bytes.as_slice()))
                .unwrap()
                .expect("A document or dependency event publishes current validation")
                .unwrap();
            assert_eq!(message["method"], "textDocument/publishDiagnostics");
            assert_eq!(message["params"]["uri"], uri);
            assert_eq!(message["params"]["version"], 7);
            message["params"]["diagnostics"].as_array().unwrap().clone()
        };
        assert!(diagnostics(&mut server).iter().all(|d| d["severity"] != 1));
        std::fs::remove_file(root.join("data.csv")).unwrap();
        server.handle(&json!({"method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":dependency,"type":3}]}})).unwrap();
        assert!(
            diagnostics(&mut server)
                .iter()
                .any(|d| { d["code"] == "report.asset.missing" && d["severity"] == 1 })
        );
        std::fs::write(root.join("data.csv"), "month,value\nJan,200\n").unwrap();
        server.handle(&json!({"method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":dependency,"type":1}]}})).unwrap();
        assert!(diagnostics(&mut server).iter().all(|d| d["severity"] != 1));
        assert_eq!(server.documents[&uri].text, source);
        assert_eq!(
            std::fs::read_to_string(root.join("report.rmd")).unwrap(),
            source
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hover_describes_a_key_with_its_allowed_values_and_nothing_else() {
        let source = "```chart\ntype: bar\ntitle: Hello\n```\n";
        let key = hover(source, 1, 1);
        let text = key["contents"]["value"].as_str().unwrap();
        assert!(text.contains("`type`") && text.contains("bar"), "{text}");
        assert!(hover(source, 2, 9).is_null(), "prose values get no hover");
        assert!(hover("plain prose", 0, 2).is_null());
    }

    #[test]
    fn lint_reports_what_validate_reports_with_ranges() {
        let diagnostics = lint("```chart\ntype: nope\n```\n", "untitled:x");
        let diagnostics = diagnostics.as_array().unwrap();
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .all(|d| d["source"] == "RichView" && d["range"].is_object())
        );
    }

    #[test]
    fn hover_survives_multibyte_text_around_the_word() {
        let text = "---\ntitle: „Tržby“ — report\n---\n";
        for character in 0..=30 {
            let _ = hover(text, 1, character);
        }
    }
}
