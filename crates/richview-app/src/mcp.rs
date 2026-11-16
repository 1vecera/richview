//! MCP's standard newline-framed JSON-RPC stdio transport; stdout contains protocol only.
use crate::{
    asset_upload, cli,
    ops::{self, Failure},
};
use serde_json::{Value, json};
use std::path::PathBuf;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// Protocol versions the server speaks, newest first (FEA-SF-10).
const PROTOCOLS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "Call richview_learn before drafting. Read data and save or edit the report's .rmd with your host's file tools, in its own report folder. Run richview_validate and richview_check after edits, including the final edit. Inspect structuredContent.status and diagnostics: status clean means no errors or warnings; ok true means no errors, and isError describes whether the call could execute. Repair warnings and errors, check again, then richview_render and inspect the output against the brief. Publish only when asked. The user chooses RichView-hosted Cowork at https://app.richview.ai or their own server, and configures RICHVIEW_SERVER and its RICHVIEW_TOKEN in the MCP host before publishing.";

/// The seven tools of D-MCP-SURFACE plus the Unsplash pair (FEA-RP-62), each twin to a command.
pub fn tool_definitions() -> Vec<Value> {
    let input_path =
        json!({"type":"string","description":"The report's .rmd, saved in its own report folder"});
    let styles = json!({"type":"array","items":{"type":"string"},"description":"Styles (a builtin name or a file in the report folder) replacing the source's `styles` for this call"});
    let tool = |name: &str,
                description: &str,
                properties: Value,
                required: Vec<&str>,
                read_only: bool,
                idempotent: bool,
                open_world: bool| {
        json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":read_only,"destructiveHint":false,"idempotentHint":idempotent,"openWorldHint":open_world}})
    };
    let mut definitions = vec![
        tool(
            "richview_learn",
            "Returns RichView's authoring guide, or one section of it. Call this first. Same text as `richview learn`.",
            json!({"section":{"type":"string","enum":richview_core::document::learn_sections().iter().map(|s| s.kind.clone()).collect::<Vec<_>>()}}),
            vec![],
            true,
            true,
            false,
        ),
        tool(
            "richview_validate",
            "Checks a report's structure and files without rendering it. With publish: true it adds the publish rules. Same result as `richview validate`.",
            json!({"inputPath":input_path,"publish":{"type":"boolean","description":"Also apply the publish rules: title, description, version and a valid slug"}}),
            vec!["inputPath"],
            true,
            true,
            false,
        ),
        tool(
            "richview_check",
            "Renders the saved report and checks provenance and chart geometry: clipping, collisions and dropped labels. Read structuredContent.status and diagnostics, repair warnings/errors, and check again after the final edit. A report finding returns isError: false; status: clean means no warnings or errors. Same result as `richview check`.",
            json!({"inputPath":input_path,"styles":styles,"chartWidth":{"type":"number","minimum":240,"maximum":4096,"description":"Measure charts at this width in pixels instead of the widths readers get"}}),
            vec!["inputPath"],
            true,
            true,
            false,
        ),
        tool(
            "richview_render",
            "Writes the self-contained HTML report, or a PDF printed by a local Chromium-family browser, into the report folder's Rendered/. Run richview_check after the final source edit for geometry and provenance findings before generating. Its result has ok, status, output (the file written) and the render diagnostics. Same result as `richview generate`.",
            json!({"inputPath":input_path,"styles":styles,"output":{"type":"string","enum":["report","pdf"],"description":"report (default) or pdf"},"outputName":{"type":"string","description":"File name inside Rendered/; a .pdf name implies pdf"},"page":{"type":"string","enum":["a4","letter"]}}),
            vec!["inputPath"],
            false,
            false,
            false,
        ),
        tool(
            "richview_publish",
            "Publishes a version of the report to the named RichView server. The token comes from the host environment (RICHVIEW_TOKEN), never from arguments. Same result as `richview publish`.",
            json!({"inputPath":input_path,"server":{"type":"string","description":"Server URL; defaults to RICHVIEW_SERVER and must equal it when that is set, so the token only goes to the configured server"},"slug":{"type":"string"},"new":{"type":"boolean","description":"Create a report address the server has not stored before"}}),
            vec!["inputPath"],
            false,
            true,
            true,
        ),
    ];
    definitions.extend(asset_upload::tool_definitions());
    definitions.extend(crate::unsplash::tool_definitions());
    definitions
}

pub fn list_resources() -> Vec<Value> {
    vec![
        json!({"uri":"richview://spec/rmd","name":"Rich Markdown format reference","mimeType":"text/markdown"}),
        json!({"uri":"richview://examples/hello-world","name":"Example: hello-world report.rmd","description":"Reading this resource also returns its data/visits.csv companion; save both in the same report folder.","mimeType":"text/markdown"}),
    ]
}
fn read_resource(uri: &str) -> Result<Value, String> {
    const HELLO_DATA_URI: &str = "richview://examples/hello-world/data/visits.csv";
    let text = match uri {
        "richview://spec/rmd" => RMD_SPEC,
        "richview://examples/hello-world" => ops::HELLO_REPORT,
        HELLO_DATA_URI => {
            return Ok(json!({"contents":[{
                "uri":uri,"mimeType":"text/csv","text":ops::HELLO_DATA
            }]}));
        }
        _ => return Err(format!("Unknown resource: {uri}")),
    };
    let mut contents = vec![json!({"uri":uri,"mimeType":"text/markdown","text":text})];
    if uri == "richview://examples/hello-world" {
        contents.push(json!({"uri":HELLO_DATA_URI,"mimeType":"text/csv","text":ops::HELLO_DATA}));
    }
    Ok(json!({"contents":contents}))
}
const RMD_SPEC: &str = include_str!("../../../docs/RMD-SPEC.md");

fn content(text: String, is_error: bool) -> Value {
    json!({"content":[{"type":"text","text":text}],"isError":is_error})
}
fn structured(value: Value, summary: String, is_error: bool) -> Value {
    json!({"content":[{"type":"text","text":summary}],"structuredContent":value,"isError":is_error})
}

pub async fn dispatch_tool(name: &str, args: &Value) -> Value {
    dispatch_tool_reporting(name, args, &|_| {}).await
}
/// Like [`dispatch_tool`]; `events` receives the asset tools' upload events as they happen.
async fn dispatch_tool_reporting(
    name: &str,
    args: &Value,
    events: asset_upload::Events<'_>,
) -> Value {
    if let Some((value, is_error)) = asset_upload::call_tool(name, args, events).await {
        let summary = serde_json::to_string(&value).unwrap_or_default();
        return structured(value, summary, is_error);
    }
    if let Some((value, is_error)) = crate::unsplash::call_tool(name, args).await {
        let summary = serde_json::to_string(&value).unwrap_or_default();
        return structured(value, summary, is_error);
    }
    match dispatch(name, args).await {
        Ok((_, text)) if name == "richview_learn" => content(text, false),
        Ok((value, summary)) => structured(value, summary, false),
        // A report with errors is a result; only a call that could not run is an error (TYP-69).
        Err(Failure::Diagnostics(diagnostics))
            if !diagnostics
                .iter()
                .any(|d| d.code.starts_with("report.root.")) =>
        {
            let summary = format!("{} diagnostics, the report has errors.", diagnostics.len());
            structured(Failure::Diagnostics(diagnostics).to_json(), summary, false)
        }
        Err(failure) => {
            let value = failure.to_json();
            structured(value, failure_summary(&failure), true)
        }
    }
}

fn failure_summary(failure: &Failure) -> String {
    match failure {
        Failure::Diagnostics(d) => cli::diagnostics_text(d, None),
        other => other.message(),
    }
}

fn arg_str<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>, Failure> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(Failure::usage(format!("{key} must be a string."))),
    }
}

fn input_of(args: &Value) -> Result<ops::Input, Failure> {
    let path = arg_str(args, "inputPath")?
        .ok_or_else(|| Failure::usage("inputPath is required: the report's saved .rmd."))?;
    let styles = match args.get("styles") {
        None | Some(Value::Null) => vec![],
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|_| Failure::usage("styles must be an array of strings."))?,
    };
    Ok(ops::Input {
        path: PathBuf::from(path),
        styles,
        connectors: None,
    })
}

/// One summary line for a report result.
fn summary_of(what: &str, result: &Value) -> String {
    let count = |key| result[key].as_array().map_or(0, Vec::len);
    format!(
        "{what}: {}, {} diagnostics (status: {}).",
        if ops::is_ok(result) {
            "ok"
        } else {
            "has errors"
        },
        count("diagnostics"),
        result["status"].as_str().unwrap_or("issues")
    )
}

async fn dispatch(name: &str, args: &Value) -> Result<(Value, String), Failure> {
    if name == "richview_learn" {
        let text = ops::learn(arg_str(args, "section")?)?;
        return Ok((json!({"ok":true,"text":text}), text));
    }
    let input = input_of(args)?;
    let result = match name {
        "richview_validate" => ops::validate(
            &input,
            args.get("publish")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )?,
        "richview_check" => {
            let widths: Vec<f64> = match args.get("chartWidth") {
                None | Some(Value::Null) => vec![],
                Some(v) => vec![
                    v.as_f64()
                        .ok_or_else(|| Failure::usage("chartWidth must be a number."))?,
                ],
            };
            ops::check(&input, &widths)?
        }
        "richview_render" => {
            let output = arg_str(args, "outputName")?.map(PathBuf::from);
            let pdf = match arg_str(args, "output")? {
                None => output.as_deref().is_some_and(cli::names_pdf),
                Some("report") => false,
                Some("pdf") => true,
                Some(_) => return Err(Failure::usage("output must be report or pdf.")),
            };
            let page = match arg_str(args, "page")? {
                None | Some("a4") => richview_core::document::Page::A4,
                Some("letter") => richview_core::document::Page::Letter,
                Some(_) => return Err(Failure::usage("page must be a4 or letter.")),
            };
            ops::generate(&ops::GenerateRequest {
                input,
                output,
                pdf,
                page,
                browser: None,
                settle_timeout: crate::export_pdf::DEFAULT_SETTLE_TIMEOUT,
            })
            .await?
        }
        "richview_publish" => {
            let configured = std::env::var("RICHVIEW_SERVER")
                .ok()
                .filter(|s| !s.is_empty());
            let server = match (arg_str(args, "server")?, configured) {
                (Some(named), Some(configured))
                    if named.trim_end_matches('/') != configured.trim_end_matches('/') =>
                {
                    return Err(Failure::usage(
                        "The token is sent only to the configured server: omit server or use RICHVIEW_SERVER.",
                    ));
                }
                (Some(_), None) => {
                    return Err(Failure::usage(
                        "No server is configured: set RICHVIEW_SERVER in the MCP host's environment; the token is sent only there.",
                    ));
                }
                (Some(named), Some(_)) => named.to_owned(),
                (None, Some(configured)) => configured,
                (None, None) => {
                    return Err(Failure::usage(
                        "No server: set RICHVIEW_SERVER in the MCP host's environment.",
                    ));
                }
            };
            let server = server.as_str();
            let token = std::env::var("RICHVIEW_TOKEN")
                .ok()
                .filter(|t| !t.is_empty())
                .ok_or_else(|| {
                    Failure::usage("No token: set RICHVIEW_TOKEN in the MCP host's environment.")
                })?;
            ops::publish(
                &input,
                server,
                &token,
                arg_str(args, "slug")?,
                args["new"].as_bool().unwrap_or(false),
            )
            .await?
        }
        _ => return Err(Failure::usage(format!("Unknown tool: {name}"))),
    };
    let summary = summary_of(name.trim_start_matches("richview_"), &result);
    Ok((result, summary))
}

pub async fn serve_stdio() -> Result<(), String> {
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    loop {
        let mut bytes = Vec::new();
        loop {
            let buffer = stdin.fill_buf().await.map_err(|e| e.to_string())?;
            if buffer.is_empty() {
                break;
            }
            let count = buffer
                .iter()
                .position(|b| *b == b'\n')
                .map(|p| p + 1)
                .unwrap_or(buffer.len());
            if bytes.len() + count > crate::MAX_INPUT_BYTES {
                return Err("MCP request exceeds 32 MiB".into());
            }
            let finished = buffer[count - 1] == b'\n';
            bytes.extend_from_slice(&buffer[..count]);
            stdin.consume(count);
            if finished {
                break;
            }
        }
        if bytes.is_empty() {
            break;
        }
        let request: Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => {
                write_response(&mut stdout,&json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}})).await?;
                continue;
            }
        };
        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if id.is_none() {
            continue;
        }
        if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            write_response(&mut stdout,&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32600,"message":"Invalid JSON-RPC request"}})).await?;
            continue;
        }
        let params = request.get("params").cloned().unwrap_or(json!({}));
        let result = match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str);
                let version = asked
                    .filter(|v| PROTOCOLS.contains(v))
                    .unwrap_or(PROTOCOLS[0]);
                Ok(
                    json!({"protocolVersion":version,"capabilities":{"tools":{},"resources":{}},"serverInfo":{"name":"richview","version":crate::VERSION},"instructions":INSTRUCTIONS}),
                )
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools":tool_definitions()})),
            "resources/list" => Ok(json!({"resources":list_resources()})),
            "resources/templates/list" => Ok(json!({"resourceTemplates":[]})),
            "resources/read" => read_resource(
                params
                    .get("uri")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            "tools/call" => Ok(call_with_progress(&params, &mut stdout).await?),
            _ => Err(format!("Method not found: {method}")),
        };
        let response = match result {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
            Err(error) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":if method=="resources/read"{-32602}else{-32601},"message":error}})
            }
        };
        write_response(&mut stdout, &response).await?;
    }
    Ok(())
}
/// Runs a tool call and, when the request carries `_meta.progressToken`, writes upload progress
/// as `notifications/progress` before the reply.
async fn call_with_progress(
    params: &Value,
    stdout: &mut tokio::io::Stdout,
) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let token = params.pointer("/_meta/progressToken").cloned();
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let events = move |event: Value| {
        let _ = sender.send(event);
    };
    let call = dispatch_tool_reporting(name, &args, &events);
    tokio::pin!(call);
    let notify = async |event: Value, stdout: &mut tokio::io::Stdout| {
        let (Some(token), Some((progress, total, message))) =
            (&token, asset_upload::progress_of(&event))
        else {
            return Ok(());
        };
        write_response(
            stdout,
            &json!({"jsonrpc":"2.0","method":"notifications/progress","params":{"progressToken":token,"progress":progress,"total":total,"message":message}}),
        )
        .await
    };
    let result = loop {
        tokio::select! {
            result = &mut call => break result,
            Some(event) = receiver.recv() => notify(event, stdout).await?,
        }
    };
    while let Ok(event) = receiver.try_recv() {
        notify(event, stdout).await?;
    }
    Ok(result)
}
async fn write_response(stdout: &mut tokio::io::Stdout, response: &Value) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(response).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    stdout.write_all(&bytes).await.map_err(|e| e.to_string())?;
    stdout.flush().await.map_err(|e| e.to_string())
}
