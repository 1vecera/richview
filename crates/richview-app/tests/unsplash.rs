//! `richview image` and the MCP image tools against a local mock of the Unsplash API.
//! No test reaches api.unsplash.com.
use axum::{
    Router,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{io::AsyncWriteExt, process::Command};

const KEY: &str = "test-access-key-9f3a";

#[derive(Clone, Debug)]
struct Seen {
    path_and_query: String,
    authorization: Option<String>,
    accept_version: Option<String>,
}

#[derive(Clone, Default)]
struct Mock {
    seen: Arc<Mutex<Vec<Seen>>>,
    redirect_to: Arc<Mutex<String>>,
}

impl Mock {
    fn record(&self, uri: &Uri, headers: &HeaderMap) {
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        self.seen.lock().unwrap().push(Seen {
            path_and_query: uri.path_and_query().unwrap().to_string(),
            authorization: header("authorization"),
            accept_version: header("accept-version"),
        });
    }
    fn requests(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
    fn count(&self, needle: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.path_and_query.contains(needle))
            .count()
    }
}

fn photo(id: &str) -> Value {
    json!({
        "id": id,
        "width": 6000,
        "height": 4000,
        "color": "#0c2640",
        "description": null,
        "alt_description": "calm sea under a pale sky",
        "urls": {
            "raw": format!("https://images.unsplash.com/photo-{id}?ixid=M3wx&ixlib=rb-4.1.0"),
            "regular": format!("https://images.unsplash.com/photo-{id}?w=1080"),
            "thumb": format!("https://images.unsplash.com/photo-{id}?w=200")
        },
        "links": {
            "html": format!("https://unsplash.com/photos/{id}"),
            "download_location": format!("https://api.unsplash.com/photos/{id}/download?ixid=M3wx")
        },
        "user": {
            "name": "Marek <Rucinski>",
            "username": "marulla",
            "links": {"html": "https://unsplash.com/@marulla"}
        }
    })
}

fn with_limit(status: StatusCode, remaining: &str, body: Value) -> Response {
    (
        status,
        [("x-ratelimit-remaining", remaining.to_owned())],
        axum::Json(body),
    )
        .into_response()
}

fn long_photo() -> Value {
    let mut long = photo("ghi789");
    long["description"] = json!(format!("line one\n\n{}", "word ".repeat(100)));
    long
}

async fn search(State(mock): State<Mock>, uri: Uri, headers: HeaderMap) -> Response {
    mock.record(&uri, &headers);
    if uri.query().is_some_and(|q| q.contains("query=long")) {
        return with_limit(
            StatusCode::OK,
            "48",
            json!({"total": 1, "total_pages": 1, "results": [long_photo()]}),
        );
    }
    with_limit(
        StatusCode::OK,
        "48",
        json!({"total": 2, "total_pages": 1, "results": [photo("abc123"), photo("def456")]}),
    )
}

async fn show(
    State(mock): State<Mock>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    mock.record(&uri, &headers);
    match id.as_str() {
        "missing" => with_limit(StatusCode::NOT_FOUND, "47", json!({"errors": ["nope"]})),
        "limited" => with_limit(StatusCode::TOO_MANY_REQUESTS, "0", json!({})),
        "forbidden" => with_limit(StatusCode::FORBIDDEN, "0", json!({})),
        "badcolor" => with_limit(StatusCode::OK, "47", {
            let mut bad = photo(&id);
            bad["color"] = json!("red;background:url(//evil.test/x)");
            bad
        }),
        "redirect" => (
            StatusCode::FOUND,
            [("location", mock.redirect_to.lock().unwrap().clone())],
        )
            .into_response(),
        "inject" => {
            let mut hostile = photo(&id);
            hostile["description"] = json!("a\n```\n# injected");
            hostile["user"]["name"] = json!("Eve\n````\n# author");
            with_limit(StatusCode::OK, "47", hostile)
        }
        "evilraw" => with_limit(StatusCode::OK, "47", {
            let mut bad = photo(&id);
            bad["urls"]["raw"] = json!("https://images.unsplash.com.evil.test/p?x=1");
            bad
        }),
        "httpraw" => with_limit(StatusCode::OK, "47", {
            let mut bad = photo(&id);
            bad["urls"]["raw"] = json!("http://images.unsplash.com/p?x=1");
            bad
        }),
        "evilprofile" => with_limit(StatusCode::OK, "47", {
            let mut bad = photo(&id);
            bad["user"]["links"]["html"] = json!("https://evil.test/@marulla");
            bad
        }),
        "evilpage" => with_limit(StatusCode::OK, "47", {
            let mut bad = photo(&id);
            bad["links"]["html"] = json!("https://unsplash.com.evil.test/photos/x");
            bad
        }),
        "badtracking" => with_limit(StatusCode::OK, "47", {
            let mut bad = photo(&id);
            bad["links"]["download_location"] =
                json!("https://api.unsplash.com/photos/other/download?ixid=1");
            bad
        }),
        _ => with_limit(StatusCode::OK, "47", photo(&id)),
    }
}

async fn download(
    State(mock): State<Mock>,
    Path(id): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    mock.record(&uri, &headers);
    with_limit(
        StatusCode::OK,
        "46",
        json!({"url": format!("https://images.unsplash.com/photo-{id}")}),
    )
}

async fn serve() -> (Mock, String) {
    let mock = Mock::default();
    let app = Router::new()
        .route("/search/photos", get(search))
        .route("/photos/{id}", get(show))
        .route("/photos/{id}/download", get(download))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (mock, base)
}

fn richview(base: &str, key: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_richview"));
    command
        .env_remove("UNSPLASH_ACCESS_KEY")
        .env_remove("RICHVIEW_UNSPLASH_APP")
        .env("RICHVIEW_UNSPLASH_API", base)
        .kill_on_drop(true);
    if let Some(key) = key {
        command.env("UNSPLASH_ACCESS_KEY", key);
    }
    command
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

async fn run(mut command: Command) -> Run {
    let output = command.output().await.unwrap();
    Run {
        code: output.status.code().unwrap(),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

#[tokio::test]
async fn search_lists_attributed_candidates_and_never_registers_a_download() {
    let (mock, base) = serve().await;
    let mut command = richview(&base, Some(KEY));
    command.args([
        "image",
        "search",
        "calm sea",
        "--orientation",
        "landscape",
        "--per-page",
        "2",
        "--format",
        "json",
    ]);
    let out = run(command).await;
    assert_eq!(out.code, 0, "{}", out.stderr);
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["count"], 2);
    assert_eq!(value["rate_limit_remaining"], 48);
    let first = &value["results"][0];
    assert_eq!(first["id"], "abc123");
    assert_eq!(first["description"], "calm sea under a pale sky");
    assert_eq!(first["author"], "Marek <Rucinski>");
    assert_eq!(
        first["author_url"],
        "https://unsplash.com/@marulla?utm_source=richview&utm_medium=referral"
    );
    assert_eq!(
        first["unsplash_url"],
        "https://unsplash.com/photos/abc123?utm_source=richview&utm_medium=referral"
    );
    assert_eq!(
        (first["width"].as_u64(), first["height"].as_u64()),
        (Some(6000), Some(4000))
    );
    assert_eq!(first["color"], "#0c2640");
    assert!(first["thumb"].as_str().unwrap().ends_with("w=200"));
    assert!(first["regular"].as_str().unwrap().ends_with("w=1080"));

    let requests = mock.requests();
    assert_eq!(requests.len(), 1, "one search request and nothing else");
    let request = &requests[0];
    assert!(request.path_and_query.starts_with("/search/photos?"));
    for part in ["query=calm+sea", "per_page=2", "orientation=landscape"] {
        assert!(request.path_and_query.contains(part), "{part}");
    }
    assert_eq!(
        request.authorization.as_deref(),
        Some(format!("Client-ID {KEY}").as_str())
    );
    assert_eq!(request.accept_version.as_deref(), Some("v1"));
    assert_eq!(mock.count("download"), 0);
    assert!(!out.stdout.contains(KEY) && !out.stderr.contains(KEY));
}

#[tokio::test]
async fn search_text_lists_the_photographer_and_the_next_step() {
    let (_, base) = serve().await;
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "search", "sea"]);
    let out = run(command).await;
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .contains("abc123  6000x4000  calm sea under a pale sky")
    );
    assert!(
        out.stdout
            .contains("by Marek <Rucinski> (https://unsplash.com/@marulla?utm_source=richview")
    );
    assert!(out.stdout.contains("richview image use <id>"));
}

#[tokio::test]
async fn use_registers_exactly_one_download_and_emits_a_credited_image_block() {
    let (mock, base) = serve().await;
    let mut command = richview(&base, Some(KEY));
    command
        .env("RICHVIEW_UNSPLASH_APP", "demo-app")
        .args(["image", "use", "abc123", "--format", "json"]);
    let out = run(command).await;
    assert_eq!(out.code, 0, "{}", out.stderr);
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["download_tracked"], true);
    assert_eq!(value["rate_limit_remaining"], 46);
    assert_eq!(
        value["src"],
        "https://images.unsplash.com/photo-abc123?ixid=M3wx&ixlib=rb-4.1.0&w=1600&q=80&fm=jpg&fit=max"
    );
    assert_eq!(
        (value["width"].as_u64(), value["height"].as_u64()),
        (Some(1600), Some(1067))
    );
    assert_eq!(value["alt"], "calm sea under a pale sky");
    assert_eq!(value["color"], "#0c2640");
    assert_eq!(
        value["credit"]["author_url"],
        "https://unsplash.com/@marulla?utm_source=demo-app&utm_medium=referral"
    );
    assert_eq!(
        value["credit"]["source_url"],
        "https://unsplash.com/?utm_source=demo-app&utm_medium=referral"
    );

    let requests = mock.requests();
    assert_eq!(requests.len(), 2, "photo lookup and one download request");
    assert_eq!(requests[0].path_and_query, "/photos/abc123");
    assert_eq!(
        requests[1].path_and_query,
        "/photos/abc123/download?ixid=M3wx"
    );
    assert_eq!(mock.count("download"), 1);
    for request in &requests {
        assert_eq!(
            request.authorization.as_deref(),
            Some(format!("Client-ID {KEY}").as_str())
        );
        assert_eq!(request.accept_version.as_deref(), Some("v1"));
    }

    // The YAML is exactly what the report core accepts, with no warnings.
    let yaml = value["yaml"].as_str().unwrap();
    let body = yaml.strip_prefix("```image\n").unwrap();
    assert!(body.ends_with("```\n"));
    let source = format!("---\ntitle: Sea\nversion: 0.0.1\n---\n\n{yaml}");
    let document = richview_core::document::parse(&source).unwrap();
    assert_eq!(document.blocks.len(), 1);
    assert!(
        document.diagnostics.is_empty(),
        "{:?}",
        document.diagnostics
    );
    let credit = &document.blocks[0].value["credit"];
    assert_eq!(credit["author"], "Marek <Rucinski>");
    assert_eq!(credit["source"], "Unsplash");
    assert_eq!(document.blocks[0].value["color"], "#0c2640");
    assert!(!out.stdout.contains(KEY) && !out.stderr.contains(KEY));
}

#[tokio::test]
async fn use_leaves_out_a_colour_that_is_not_rrggbb() {
    let (_, base) = serve().await;
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "use", "badcolor", "--format", "json"]);
    let out = run(command).await;
    assert_eq!(out.code, 0, "{}", out.stderr);
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert!(value["color"].is_null());
    let yaml = value["yaml"].as_str().unwrap();
    assert!(
        !yaml.contains("\ncolor:") && !yaml.contains("evil.test"),
        "{yaml}"
    );
    let document = assert_one_image_block(yaml);
    assert!(
        document.diagnostics.is_empty(),
        "{:?}",
        document.diagnostics
    );
}

#[tokio::test]
async fn use_as_title_image_scales_dimensions_and_honours_alt_and_width() {
    let (_, base) = serve().await;
    let mut command = richview(&base, Some(KEY));
    command.args([
        "image",
        "use",
        "abc123",
        "--as",
        "title-image",
        "--width",
        "800",
        "--alt",
        "Waves: gentle",
    ]);
    let out = run(command).await;
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stderr.contains("Download registered"));
    let frontmatter: Value = richview_core::yaml::from_str(&out.stdout).unwrap();
    let image = &frontmatter["title_image"];
    assert_eq!(image["alt"], "Waves: gentle");
    assert!(
        image["src"]
            .as_str()
            .unwrap()
            .ends_with("&w=800&q=80&fm=jpg&fit=max")
    );
    assert_eq!(
        (image["width"].as_u64(), image["height"].as_u64()),
        (Some(800), Some(533))
    );
    assert_eq!(image["credit"]["source"], "Unsplash");
    assert_eq!(image["color"], "#0c2640");
    let report = richview_core::document::render(
        &format!("---\ntitle: Sea\n{}---\n\nBody.\n", out.stdout),
        &richview_core::document::RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "sea".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(report.html.contains("<p class=\"rv-credit\">Photo by <a href=\"https://unsplash.com/@marulla?utm_source=richview&amp;utm_medium=referral\""));
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|d| d.code.starts_with("image.credit"))
    );
}

#[tokio::test]
async fn failures_have_clear_codes_and_never_leak_the_key() {
    let (mock, base) = serve().await;
    for (id, code, text) in [
        ("missing", "unsplash.not-found", "no such photo"),
        ("limited", "unsplash.rate-limited", "50 requests per hour"),
        ("forbidden", "unsplash.rate-limited", "rate limit"),
    ] {
        let mut command = richview(&base, Some(KEY));
        command.args(["image", "use", id, "--format", "json"]);
        let out = run(command).await;
        assert_eq!(out.code, 3, "{id}");
        let value: Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(
            (value["ok"].clone(), value["code"].clone()),
            (json!(false), json!(code))
        );
        assert!(value["error"].as_str().unwrap().contains(text), "{value}");
        assert!(!out.stdout.contains(KEY) && !out.stderr.contains(KEY));
    }
    assert_eq!(
        mock.count("download"),
        0,
        "a failed lookup registers no download"
    );

    let mut command = richview(&base, None);
    command.args(["image", "search", "sea", "--format", "json"]);
    let out = run(command).await;
    assert_eq!(out.code, 2);
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["code"], "unsplash.key-missing");
    assert!(
        value["error"]
            .as_str()
            .unwrap()
            .contains("UNSPLASH_ACCESS_KEY")
    );

    let mut command = richview(&base, Some(KEY));
    command.args(["image", "search", "sea", "--per-page", "11"]);
    assert_ne!(run(command).await.code, 0);

    let mut command = richview(&base, Some(KEY));
    command.args(["image", "use", "../etc", "--format", "json"]);
    let out = run(command).await;
    assert_eq!(out.code, 2);
    assert_eq!(mock.requests().len(), 3, "invalid input sends no request");

    // The key is not accepted as an argument.
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "search", "sea", "--access-key", KEY]);
    let out = run(command).await;
    assert_ne!(out.code, 0);
}

#[tokio::test]
async fn unauthorized_key_reports_401_without_echoing_it() {
    let app = Router::new().fallback(|| async { StatusCode::UNAUTHORIZED });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "search", "sea", "--format", "json"]);
    let out = run(command).await;
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["code"], "unsplash.unauthorized");
    assert!(!out.stdout.contains(KEY) && !out.stderr.contains(KEY));
}

#[tokio::test]
async fn mcp_tools_mirror_the_cli() {
    let (mock, base) = serve().await;
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"richview_image_search","arguments":{"query":"calm sea","orientation":"portrait","per_page":3}}}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"richview_image_use","arguments":{"photo_id":"abc123","width":1200,"as":"block"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"richview_image_use","arguments":{"photo_id":"missing"}}}),
    ];
    let mut child = richview(&base, Some(KEY))
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for request in &requests {
        stdin
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().await.unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(KEY));
    let replies: Vec<Value> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let names: Vec<_> = replies[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"richview_image_search") && names.contains(&"richview_image_use"));

    let search = &replies[1]["result"];
    assert_eq!(search["isError"], false);
    assert_eq!(search["structuredContent"]["count"], 2);
    let used = &replies[2]["result"];
    assert_eq!(used["isError"], false);
    assert_eq!(used["structuredContent"]["download_tracked"], true);
    assert_eq!(used["structuredContent"]["color"], "#0c2640");
    assert_eq!(
        (
            used["structuredContent"]["width"].as_u64(),
            used["structuredContent"]["height"].as_u64()
        ),
        (Some(1200), Some(800))
    );
    assert!(
        used["structuredContent"]["yaml"]
            .as_str()
            .unwrap()
            .starts_with("```image\n")
    );
    let failed = &replies[3]["result"];
    assert_eq!(failed["isError"], true);
    assert_eq!(failed["structuredContent"]["code"], "unsplash.not-found");

    assert_eq!(mock.count("/search/photos"), 1);
    assert_eq!(mock.count("download"), 1);
}

fn assert_one_image_block(yaml: &str) -> richview_core::document::Document {
    let source = format!("---\ntitle: Sea\nversion: 0.0.1\n---\n\n{yaml}");
    let document = richview_core::document::parse(&source).unwrap();
    assert_eq!(document.blocks.len(), 1, "{yaml}");
    assert!(
        document.diagnostics.is_empty(),
        "{:?}",
        document.diagnostics
    );
    document
}

#[tokio::test]
async fn hostile_text_cannot_close_the_fence_or_inject_markdown() {
    let (_, base) = serve().await;
    for args in [
        vec!["image", "use", "inject", "--format", "json"],
        vec![
            "image",
            "use",
            "abc123",
            "--alt",
            "a\n```\n# injected",
            "--format",
            "json",
        ],
    ] {
        let mut command = richview(&base, Some(KEY));
        command.args(&args);
        let out = run(command).await;
        assert_eq!(out.code, 0, "{}", out.stderr);
        let value: Value = serde_json::from_str(&out.stdout).unwrap();
        let yaml = value["yaml"].as_str().unwrap();
        assert!(
            yaml.starts_with("````") && yaml.contains("`image\n"),
            "{yaml}"
        );
        assert!(!value["alt"].as_str().unwrap().contains('\n'));
        assert!(!yaml.lines().any(|line| line.starts_with('#')), "{yaml}");
        let document = assert_one_image_block(yaml);
        assert_eq!(document.blocks[0].value["alt"], value["alt"]);
        // Nothing but the image block's placeholder remains in the body: no heading, no text.
        assert!(
            !document
                .body
                .lines()
                .any(|l| l.trim_start().starts_with('#')),
            "{}",
            document.body
        );
        assert!(!document.body.contains("injected"), "{}", document.body);
    }
}

#[tokio::test]
async fn response_links_off_unsplash_hosts_are_refused() {
    let (mock, base) = serve().await;
    for (id, field) in [
        ("evilraw", "urls.raw"),
        ("httpraw", "urls.raw"),
        ("evilprofile", "user.links.html"),
        ("evilpage", "links.html"),
    ] {
        let mut command = richview(&base, Some(KEY));
        command.args(["image", "use", id, "--format", "json"]);
        let out = run(command).await;
        assert_eq!(out.code, 3, "{id}");
        let value: Value = serde_json::from_str(&out.stdout).unwrap();
        assert_eq!(value["code"], "unsplash.untrusted-host", "{id}");
        assert!(value["error"].as_str().unwrap().contains(field), "{value}");
    }
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "use", "badtracking", "--format", "json"]);
    let out = run(command).await;
    assert_eq!(out.code, 3);
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["code"], "unsplash.response");
    assert_eq!(
        mock.count("download"),
        0,
        "no refused photo registers a download"
    );
}

#[tokio::test]
async fn a_redirect_is_not_followed_and_the_key_goes_nowhere_else() {
    let (mock, base) = serve().await;
    let (other, other_base) = serve().await;
    // Point the redirect at the second listener, as a hostile or compromised API could.
    *mock.redirect_to.lock().unwrap() = format!("{other_base}/photos/abc123");
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "use", "redirect", "--format", "json"]);
    let out = run(command).await;
    assert_eq!(out.code, 3);
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["code"], "unsplash.redirect");
    assert!(
        other.requests().is_empty(),
        "the second listener got a request"
    );
    assert!(!out.stdout.contains(KEY) && !out.stderr.contains(KEY));
    assert!(!out.stdout.contains(&other_base));
}

#[tokio::test]
async fn search_descriptions_are_one_line_and_at_most_200_characters() {
    let (_, base) = serve().await;
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "search", "long", "--format", "json"]);
    let out = run(command).await;
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    let long = value["results"][0]["description"].as_str().unwrap();
    assert!(long.chars().count() <= 200, "{}", long.chars().count());
    assert!(long.starts_with("line one word word"), "{long}");
    assert!(!long.contains('\n'));
}

#[tokio::test]
async fn plain_http_is_refused_for_a_remote_api() {
    let mut command = richview("http://api.example.test", Some(KEY));
    command.args(["image", "search", "sea", "--format", "json"]);
    let out = run(command).await;
    assert_eq!(out.code, 2);
    let value: Value = serde_json::from_str(&out.stdout).unwrap();
    assert_eq!(value["code"], "unsplash.config");
    assert!(value["error"].as_str().unwrap().contains("https"));
}

async fn chosen_photo_fixture() -> (Mock, String, tempfile::TempDir, Value) {
    let (mock, base) = serve().await;
    let mut command = richview(&base, Some(KEY));
    command.args(["image", "use", "abc123", "--format", "json"]);
    let used: Value = serde_json::from_str(&run(command).await.stdout).unwrap();
    assert_eq!(mock.count("download"), 1);

    let scratch = std::env::current_dir().unwrap().join("tmp");
    std::fs::create_dir_all(&scratch).unwrap();
    let folder = tempfile::tempdir_in(scratch).unwrap();
    let source = folder.path().join("report.rmd");
    let yaml = used["yaml"].as_str().unwrap();
    std::fs::write(
        &source,
        format!("---\ntitle: Sea\nversion: 0.0.1\n---\n\n{yaml}"),
    )
    .unwrap();
    (mock, base, folder, used)
}

#[tokio::test]
async fn validating_and_checking_a_chosen_photo_never_touch_the_network() {
    let (mock, base, folder, used) = chosen_photo_fixture().await;
    let source = folder.path().join("report.rmd");
    for verb in ["validate", "check"] {
        let mut command = richview(&base, Some(KEY));
        command
            .args([verb, source.to_str().unwrap(), "--format", "json"])
            .current_dir(folder.path());
        let out = run(command).await;
        assert_eq!(out.code, 0, "{verb}: {}{}", out.stdout, out.stderr);
    }
    // The photo lookup and the single download ping happened before; validation added none.
    assert_eq!(mock.requests().len(), 2);
    assert_eq!(mock.count("download"), 1);

    // Without its credit the photo is an error and the source stays visible.
    let uncredited = used["yaml"]
        .as_str()
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with("credit:") && !line.starts_with("  "))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &source,
        format!("---\ntitle: Sea\nversion: 0.0.1\n---\n\n{uncredited}\n```\n"),
    )
    .unwrap();
    let mut command = richview(&base, Some(KEY));
    command
        .args(["validate", source.to_str().unwrap(), "--format", "json"])
        .current_dir(folder.path());
    let out = run(command).await;
    assert!(
        out.stdout.contains("image.credit-missing"),
        "{}",
        out.stdout
    );
    assert_eq!(mock.requests().len(), 2);
}

#[tokio::test]
#[ignore = "release: generate needs the real embedded report runtime staged by richview-build (ARC-07)"]
async fn rendering_a_chosen_photo_never_touches_the_network() {
    let (mock, base, folder, used) = chosen_photo_fixture().await;
    let source = folder.path().join("report.rmd");
    let mut command = richview(&base, Some(KEY));
    command
        .args(["generate", source.to_str().unwrap(), "--format", "json"])
        .current_dir(folder.path());
    let out = run(command).await;
    assert_eq!(out.code, 0, "generate: {}{}", out.stdout, out.stderr);
    let html = std::fs::read_to_string(folder.path().join("Rendered/report.html")).unwrap();
    assert!(html.contains(used["src"].as_str().unwrap().replace('&', "&amp;").as_str()));
    assert!(html.contains("aspect-ratio:1600/1067;background-color:#0c2640;"));
    assert!(html.contains("class=\"rv-credit\""));
    assert!(!html.contains("data:image/"));
    assert_eq!(mock.requests().len(), 2);
    assert_eq!(mock.count("download"), 1);
}
