//! The editor service over real stdio: the framing, the debounced diagnostics, the providers and
//! the rule that editor diagnostics equal `richview validate` (ARC-44, FEA-SF-24).
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Client {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Client {
    fn start() -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_richview-editor"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Client {
            child,
            input,
            output,
            next: 1,
        }
    }

    fn send(&mut self, message: Value) {
        let body = message.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.input.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).unwrap() > 0,
                "service closed"
            );
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        self.output.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        loop {
            let message = self.read();
            if message["id"] == id {
                return message;
            }
        }
    }

    fn notification(&mut self, method: &str) -> Value {
        loop {
            let message = self.read();
            if message["method"] == method {
                return message["params"].clone();
            }
        }
    }

    fn open(&mut self, uri: &str, text: &str) {
        self.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"richview","version":1,"text":text}}}));
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn folder(name: &str, source: &str) -> (std::path::PathBuf, String) {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tmp/lsp-tests")
        .join(format!("{}-{name}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("report.rmd");
    std::fs::write(&path, source).unwrap();
    let uri = format!("file://{}", path.canonicalize().unwrap().display());
    (path, uri)
}

const BROKEN: &str = "---\ntitle: Broken\n---\n\n```chart\ntype: nope\n```\n";

fn assert_diagnostics_equal_validate(published: &Value, validate: &Value) {
    let editor = published["diagnostics"].as_array().unwrap();
    let cli = validate["diagnostics"].as_array().unwrap();
    assert!(!editor.is_empty());
    assert_eq!(editor.len(), cli.len(), "{published}");
    for (actual, expected) in editor.iter().zip(cli) {
        assert_eq!(actual["code"], expected["code"]);
        assert_eq!(actual["message"], expected["message"]);
        assert_eq!(actual["source"], "RichView");
        let severity = match expected["level"].as_str().unwrap() {
            "error" => 1,
            "warning" => 2,
            _ => 3,
        };
        assert_eq!(actual["severity"], severity);
        for key in [
            "path", "tag", "fix", "expected", "retry", "details", "file", "column",
        ] {
            assert!(actual["data"].as_object().unwrap().contains_key(key));
            assert_eq!(actual["data"][key], expected[key], "{key}: {actual}");
        }
    }
}

#[test]
fn diagnostics_equal_validate_and_follow_unsaved_edits() {
    let (path, uri) = folder("diag", BROKEN);
    let mut client = Client::start();
    let init = client.request("initialize", json!({"capabilities":{}}));
    assert_eq!(
        init["result"]["capabilities"]["textDocumentSync"]["change"],
        2
    );
    client.open(&uri, BROKEN);
    let published = client.notification("textDocument/publishDiagnostics");
    let input = richview_app::ops::Input {
        path: path.clone(),
        ..Default::default()
    };
    let validate = richview_app::ops::validate(&input, false).unwrap();
    assert_diagnostics_equal_validate(&published, &validate);

    // An unsaved incremental edit removes the chart and its findings; the file on disk is unchanged.
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":uri,"version":2},
        "contentChanges":[{"range":{"start":{"line":4,"character":0},"end":{"line":7,"character":0}},"text":"Fine.\n"}]}}));
    let published = client.notification("textDocument/publishDiagnostics");
    assert_eq!(published["version"], 2);
    let errors = published["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["severity"] == 1)
        .count();
    assert_eq!(errors, 0, "{published}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), BROKEN);

    let tabs = "---\ntitle: Tabs\ntabs:\n  export: all\n  panels:\n    - id: one\n      label: One\n      from: one\n    - id: one\n      label: Two\n      from: two\n---\n\n## One {#one}\n\nFirst.\n\n## Two {#two}\n\nSecond.\n";
    let (path, uri) = folder("diag-tabs", tabs);
    client.open(&uri, tabs);
    let published = client.notification("textDocument/publishDiagnostics");
    assert_eq!(published["uri"], uri);
    let validate = richview_app::ops::validate(
        &richview_app::ops::Input {
            path,
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_diagnostics_equal_validate(&published, &validate);
    let duplicate = published["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "frontmatter.tabs")
        .unwrap();
    assert_eq!(duplicate["data"]["path"], "frontmatter.tabs.panels[1].id");
    assert_eq!(
        duplicate["range"],
        json!({"start":{"line":8,"character":6},"end":{"line":8,"character":8}}),
        "the duplicate tab highlights the second id field"
    );
}

#[test]
fn a_markdown_finding_is_highlighted_at_its_column() {
    // Core gives the column in characters; the range counts UTF-16 units, so the emoji is two.
    let source = "---\ntitle: Links\n---\n\n😀 é [gone](gone.md) here.\n";
    let (_, uri) = folder("markdown-column", source);
    let mut client = Client::start();
    client.request("initialize", json!({"capabilities":{}}));
    client.open(&uri, source);
    let published = client.notification("textDocument/publishDiagnostics");
    let missing = published["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "report.asset.missing")
        .unwrap_or_else(|| panic!("{published}"));
    assert_eq!(missing["data"]["column"], 5);
    assert_eq!(
        missing["range"],
        json!({"start":{"line":4,"character":5},"end":{"line":4,"character":6}}),
        "the range starts at the link's `[`"
    );
}

#[test]
fn providers_answer_from_unsaved_source() {
    let source = "---\ntitle: Providers\n---\n\n# Heading\n\n```chart\ntype: bar\n```\n";
    let (_, uri) = folder("providers", source);
    let mut client = Client::start();
    client.request("initialize", json!({"capabilities":{}}));
    client.open(&uri, source);
    let doc = json!({"uri":uri});
    let symbols = client.request("textDocument/documentSymbol", json!({"textDocument":doc}));
    assert_eq!(symbols["result"][0]["name"], "Heading");
    let folding = client.request("textDocument/foldingRange", json!({"textDocument":doc}));
    assert!(!folding["result"].as_array().unwrap().is_empty());
    let hover = client.request(
        "textDocument/hover",
        json!({"textDocument":doc,"position":{"line":7,"character":1}}),
    );
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("type")
    );
    let completion = client.request(
        "textDocument/completion",
        json!({"textDocument":doc,"position":{"line":7,"character":6}}),
    );
    assert!(
        completion["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "bar")
    );
    let tabs = "---\ntitle: Tabs\ntabs:\n  export: all\n  panels:\n    - id: one\n      label: One\n      from: one\n    - id: two\n      label: Two\n      from: two\n---\n\n## One {#one}\n\nFirst.\n\n## Two {#two}\n\nSecond.\n";
    let (_, tabs_uri) = folder("providers-tabs", tabs);
    client.open(&tabs_uri, tabs);
    let tabs_doc = json!({"uri": tabs_uri});
    for (line, character, key) in [
        (2, 2, "tabs"),
        (3, 4, "export"),
        (5, 7, "id"),
        (7, 8, "from"),
        (9, 8, "label"),
        (10, 8, "from"),
    ] {
        let hovered = client.request(
            "textDocument/hover",
            json!({"textDocument": tabs_doc, "position": {"line": line, "character": character}}),
        );
        let value = hovered["result"]["contents"]["value"]
            .as_str()
            .unwrap_or_else(|| panic!("hover missing for {key} on line {line}: {hovered}"));
        assert!(value.contains(&format!("`{key}`")), "{hovered}");
        assert_eq!(hovered["result"]["range"]["start"]["line"], line);
        if key == "export" {
            assert!(value.contains("all") && value.contains("active"), "{value}");
        }
    }
    let snippets = client.request("richview/snippets", json!({}));
    assert!(!snippets["result"].as_array().unwrap().is_empty());
    let unknown = client.request("richview/nope", json!({}));
    assert_eq!(unknown["error"]["code"], -32601);
}

#[test]
fn export_refuses_unsaved_document_uris_before_rendering() {
    let mut client = Client::start();
    client.request("initialize", json!({"capabilities": {}}));
    for uri in [
        "untitled:Untitled-1",
        "vscode-remote://ssh-remote/work/report.rmd",
    ] {
        client.open(uri, "# Unsaved report\n");
        let refused = client.request("richview/export", json!({"uri": uri}));
        assert_eq!(
            refused["error"],
            json!({"code": -32602, "message": "Save the report in its own folder first."}),
            "{refused}"
        );
    }
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn preview_renders_the_open_text_and_unsaved_documents_get_a_save_message() {
    let source = "---\ntitle: Preview\n---\n\n# Preview heading\n";
    let (_, uri) = folder("preview", source);
    let mut client = Client::start();
    client.request("initialize", json!({"capabilities":{}}));
    client.open(&uri, source);
    let preview = client.request("richview/preview", json!({"uri":uri}));
    assert!(
        preview["result"]["html"]
            .as_str()
            .unwrap()
            .contains("Preview heading")
    );
    client.open("untitled:Untitled-1", "# x");
    let refused = client.request("richview/preview", json!({"uri":"untitled:Untitled-1"}));
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("report.root.missing")
    );
}

#[test]
fn every_preview_and_every_failed_request_carries_ok_status_and_output() {
    // TYP-54: a preview result says ok, status and output (null: a preview writes nothing). A
    // request that fails carries that object as its error's data, diagnostics typed, and names
    // each error in its message; a failed preview over a good one keeps it, with the object.
    let source = "---\ntitle: Envelope\n---\n\n# Envelope heading\n";
    let (path, uri) = folder("envelope", source);
    let mut client = Client::start();
    client.request("initialize", json!({"capabilities":{}}));
    client.open(&uri, source);
    // A trusted runtime address, so the debug service needs no embedded runtime.
    let runtime = "https://file+.vscode-resource.vscode-cdn.net/extension/web/richview_runtime";
    let good = client.request(
        "richview/preview",
        json!({"uri":uri,"runtimeUrl":runtime,"runtimeJs":"fixture"}),
    );
    let result = &good["result"];
    assert_eq!(result["ok"], true, "{good}");
    assert_eq!(result["output"], Value::Null, "{good}");
    let clean = result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .all(|d| d["level"] == "info");
    assert_eq!(result["status"], if clean { "clean" } else { "issues" });
    assert!(
        result["html"]
            .as_str()
            .unwrap()
            .contains("Envelope heading")
    );

    client.open("untitled:Untitled-1", "# x");
    let refused = client.request("richview/preview", json!({"uri":"untitled:Untitled-1"}));
    let error = &refused["error"];
    assert_eq!(error["code"], -32000, "{refused}");
    assert_eq!(
        (
            &error["data"]["ok"],
            &error["data"]["status"],
            &error["data"]["output"]
        ),
        (&json!(false), &json!("issues"), &Value::Null),
        "{refused}"
    );
    assert_eq!(
        error["data"]["diagnostics"][0]["code"],
        "report.root.missing"
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .starts_with("report.root.missing: Save one .rmd"),
        "{refused}"
    );

    // The report folder disappears: the render fails, and the last good render stays.
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    let kept = client.request(
        "richview/preview",
        json!({"uri":uri,"runtimeUrl":runtime,"runtimeJs":"fixture"}),
    );
    let result = &kept["result"];
    assert_eq!(result["failed"], true, "{kept}");
    assert_eq!(
        (&result["ok"], &result["status"], &result["output"]),
        (&json!(false), &json!("issues"), &Value::Null),
        "{kept}"
    );
    assert!(
        !result["diagnostics"].as_array().unwrap().is_empty(),
        "{kept}"
    );
    assert!(
        result["html"]
            .as_str()
            .unwrap()
            .contains("Envelope heading")
    );
}

#[test]
fn an_oversized_frame_is_refused_and_the_service_stops() {
    let mut client = Client::start();
    write!(client.input, "Content-Length: 99999999\r\n\r\n").unwrap();
    client.input.flush().unwrap();
    let error = client.read();
    assert_eq!(error["error"]["code"], -32600);
    assert!(client.child.wait().unwrap().success());
}

#[test]
fn the_trusted_editor_runtime_is_external_only_for_preview() {
    let source = "---\nruntime_url: https://untrusted.invalid/evil\n---\n\n# Cached preview";
    let (_, uri) = folder("runtime", source);
    let mut client = Client::start();
    client.request("initialize", json!({"capabilities":{}}));
    client.open(&uri, source);
    let trusted = "https://file+.vscode-resource.vscode-cdn.net/extension/web/richview_runtime";
    let missing = client.request("richview/preview", json!({"uri":uri,"runtimeUrl":trusted}));
    assert!(
        !missing["result"]["html"]
            .as_str()
            .unwrap()
            .contains("<script src=")
    );
    let result = client.request(
        "richview/preview",
        json!({"uri":uri,"runtimeUrl":trusted,"runtimeJs":"fixture"}),
    );
    let html = result["result"]["html"].as_str().unwrap();
    assert!(html.contains(&format!(
        "<script src=\"{trusted}.js\" data-rv-wasm=\"{trusted}_bg.wasm\""
    )));
    assert!(html.contains("integrity=\"sha256-"));
    assert!(!html.contains("untrusted.invalid"));
    // The shared external initializer can decode parent-supplied bytes. This URL-only
    // response supplies neither those bytes nor an inline runtime module.
    assert!(!html.contains("data-rv-wasm-bytes=\""));
    assert!(!html.contains("<script type=\"module\">"));
    let result = client.request(
        "richview/preview",
        json!({"uri":uri,"runtimeUrl":"javascript:alert(1)","runtimeJs":"fixture"}),
    );
    assert!(
        !result["result"]["html"]
            .as_str()
            .unwrap()
            .contains("javascript:alert(1)")
    );
}

#[test]
#[ignore = "release: needs the real embedded report runtime staged by richview-build (ARC-07)"]
fn a_failed_preview_keeps_the_last_good_render_under_a_banner() {
    let source = "---\ntitle: Keep\n---\n\n# Kept heading\n";
    let (path, uri) = folder("keep", source);
    let mut client = Client::start();
    client.request("initialize", json!({"capabilities":{}}));
    client.open(&uri, source);
    let good = client.request("richview/preview", json!({"uri":uri}));
    assert!(
        !good["result"]["html"]
            .as_str()
            .unwrap()
            .contains("richview-banner")
    );
    // The report folder disappears: the render fails outright.
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    let kept = client.request("richview/preview", json!({"uri":uri}));
    let html = kept["result"]["html"].as_str().unwrap();
    assert_eq!(kept["result"]["failed"], true, "{kept}");
    assert!(html.contains("Kept heading") && html.contains("richview-banner"));

    // A new editing session must not resurrect the closed document's cached HTML.
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri}}}));
    client.open(&uri, "# A new editing session\n");
    let reopened = client.request("richview/preview", json!({"uri":uri}));
    assert_eq!(
        reopened["error"]["code"],
        -32000,
        "closed preview was reused: kept heading present = {}",
        reopened["result"]["html"]
            .as_str()
            .is_some_and(|html| html.contains("Kept heading"))
    );
    assert!(reopened.get("result").is_none());
}
