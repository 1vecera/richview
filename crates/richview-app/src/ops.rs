//! The operation layer behind every native surface (ARC-08, TYP-94).
//!
//! CLI, MCP and the editor decode their input, call one function here and encode the result.
//! Each operation returns the T-CLI-JSON object (TYP-68) and fails with a typed [`Failure`], so
//! diagnostics reach every surface unchanged and are never serialized into a message.
use crate::resolver;
use richview_core::document::{Diagnostic, RenderOptions};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// Why an operation could not produce a result.
#[derive(Debug, Clone)]
pub enum Failure {
    /// The operation was called wrongly (exit 2).
    Usage(String),
    /// The source or the report folder has errors that stop the operation.
    Diagnostics(Vec<Diagnostic>),
    /// The host could not do its part: one sentence, and whether retrying can succeed.
    Host {
        message: String,
        retry: Option<bool>,
    },
}

impl Failure {
    pub fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }
    pub fn host(message: impl Into<String>) -> Self {
        Self::Host {
            message: message.into(),
            retry: None,
        }
    }
    /// Process exit code: 2 for a wrong call, 1 otherwise (TYP-54).
    pub fn exit_code(&self) -> i32 {
        if matches!(self, Self::Usage(_)) { 2 } else { 1 }
    }
    /// The diagnostics this failure carries, if it has any.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        match self {
            Self::Diagnostics(diagnostics) => diagnostics,
            _ => &[],
        }
    }
    /// One sentence for a failure without diagnostics.
    pub fn message(&self) -> String {
        match self {
            Self::Usage(message) | Self::Host { message, .. } => message.clone(),
            Self::Diagnostics(diagnostics) => diagnostics
                .iter()
                .find(|d| d.level == "error")
                .or(diagnostics.first())
                .map(|d| d.message.clone())
                .unwrap_or_else(|| "The report has errors.".into()),
        }
    }
    /// The T-CLI-JSON object of a failed call: diagnostics once, `error` only without any.
    pub fn to_json(&self) -> Value {
        match self {
            Self::Diagnostics(diagnostics) => {
                json!({"ok":false,"status":"issues","output":null,"diagnostics":diagnostics})
            }
            Self::Host { message, retry } => {
                let mut value = json!({"ok":false,"status":"issues","output":null,"error":message,"diagnostics":[]});
                if let Some(retry) = retry {
                    value["retry"] = json!(retry);
                }
                value
            }
            Self::Usage(message) => {
                json!({"ok":false,"status":"issues","output":null,"error":message,"diagnostics":[]})
            }
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}
impl std::error::Error for Failure {}

/// Marks a message as a usage error where host services still return plain strings.
const USAGE_MARK: &str = "\u{1}usage:";

/// Host services that still return strings hand diagnostics over as JSON; this is the one
/// place that turns them back into the typed record.
impl From<String> for Failure {
    fn from(message: String) -> Self {
        if let Some(rest) = message.strip_prefix(USAGE_MARK) {
            return Self::Usage(rest.into());
        }
        match serde_json::from_str::<Vec<Diagnostic>>(&message) {
            Ok(diagnostics) if !diagnostics.is_empty() => Self::Diagnostics(diagnostics),
            _ => Self::host(message),
        }
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
impl From<Vec<Diagnostic>> for Failure {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        Self::Diagnostics(diagnostics)
    }
}

/// The report `init` writes, also served as the MCP example resource (FEA-SF-02, FEA-SF-11).
pub const HELLO_REPORT: &str = include_str!("../../../examples/hello-world/report.rmd");
/// The data file the template's chart reads, written beside it.
pub const HELLO_DATA: &str = include_str!("../../../examples/hello-world/data/visits.csv");

pub type Outcome = Result<Value, Failure>;

/// True when any diagnostic is an error (TYP-54).
pub fn has_error(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.level == "error")
}

/// `clean` without errors or warnings, otherwise `issues` (TYP-54).
pub fn status_of(diagnostics: &[Diagnostic]) -> &'static str {
    if diagnostics
        .iter()
        .any(|d| d.level == "error" || d.level == "warning")
    {
        "issues"
    } else {
        "clean"
    }
}

/// The shared result object: `ok`, `status`, `output` and `diagnostics`, plus the operation's
/// own fields (TYP-68).
pub fn envelope(diagnostics: Vec<Diagnostic>, output: Option<&Path>, extra: Value) -> Value {
    let mut value = richview_core::document::operation_result(diagnostics, extra);
    value["output"] = json!(output);
    value
}

/// Whether a result says the operation found errors (`ok: false`).
pub fn is_ok(result: &Value) -> bool {
    result["ok"].as_bool().unwrap_or(false)
}

/// One command's outcomes over the reports of a folder, in the order they ran (FEA-SF-01).
#[derive(Default)]
pub struct FolderRun(Vec<(PathBuf, Outcome)>);

impl FolderRun {
    /// Keeps a report's outcome. A wrong call is wrong for every report, so it ends the run.
    pub fn keep(&mut self, report: &Path, outcome: Outcome) -> Result<(), Failure> {
        if let Err(Failure::Usage(message)) = outcome {
            return Err(Failure::Usage(message));
        }
        self.0.push((report.to_path_buf(), outcome));
        Ok(())
    }
    pub fn outcomes(&self) -> &[(PathBuf, Outcome)] {
        &self.0
    }
    /// The one object a folder run returns (TYP-68): `reports` holds each report's own result, or
    /// its failure as the same object, with the report it read in `input`. `ok` and `status` speak
    /// for all of them; `diagnostics` stays empty, because a finding belongs to its report.
    pub fn result(&self) -> Value {
        let reports: Vec<Value> = self
            .0
            .iter()
            .map(|(report, outcome)| {
                let mut value = match outcome {
                    Ok(value) => value.clone(),
                    Err(failure) => failure.to_json(),
                };
                value["input"] = json!(report.display().to_string());
                value
            })
            .collect();
        let status = if reports.iter().all(|report| report["status"] == "clean") {
            "clean"
        } else {
            "issues"
        };
        json!({"ok": reports.iter().all(is_ok), "status": status, "output": null, "diagnostics": [], "reports": reports})
    }
}

fn render_meta<'a>(
    report: &'a richview_core::document::RenderedReport,
    key: &str,
) -> Option<&'a Value> {
    report.metadata.pointer(&format!("/_richview/{key}"))
}

/// What every report operation needs to find its input.
#[derive(Debug, Clone, Default)]
pub struct Input {
    /// The saved `.rmd` in its report folder.
    pub path: PathBuf,
    /// Builtin names or files in the folder that replace the source's `styles` for this call.
    pub styles: Vec<String>,
    /// Trusted path to a connector registry (ARC-59).
    pub connectors: Option<PathBuf>,
}

fn resolve(
    input: &Input,
    include_runtime: bool,
) -> Result<(richview_core::document::Document, RenderOptions), Failure> {
    let source = crate::cli::read_source(&input.path)?;
    let root = input
        .path
        .canonicalize()
        .map_err(|e| Failure::host(format!("{}: {e}", input.path.display())))?
        .parent()
        .ok_or_else(|| Failure::host("The report has no parent directory."))?
        .to_path_buf();
    // A connector the report names but the host does not define is a finding on the report, not a
    // bare host error: every surface shows it with a code.
    let registry = crate::connectors::load_registry(&root, input.connectors.as_deref()).map_err(
        |message| {
            Failure::Diagnostics(vec![Diagnostic::new(
                "error",
                "resource.resolve",
                message,
                "connectors",
            )])
        },
    )?;
    Ok(resolver::resolve_document_options_full(
        &source,
        &input.path,
        &input.styles,
        false,
        include_runtime,
        registry.as_ref(),
    )?)
}

/// `validate`: structure and report folder, without rendering (FEA-SF-03). With `publish` it adds
/// the publish rules (TYP-94).
pub fn validate(input: &Input, publish: bool) -> Outcome {
    let source = crate::cli::read_source(&input.path)?;
    validate_source(&source, input, publish)
}

/// `validate` on source text that may differ from the saved file, as the editor sends it. The
/// result is what `validate` returns for the same text (ARC-08).
pub fn validate_source(source: &str, input: &Input, publish: bool) -> Outcome {
    let (blocks, mut diagnostics) =
        match resolver::resolve_for_validation(source, &input.path, &input.styles) {
            Ok((document, _)) => (document.blocks.len(), document.diagnostics),
            Err(error) => match Failure::from(error) {
                Failure::Diagnostics(mut diagnostics) => {
                    // An unsaved or unresolvable report still gets the source's own findings.
                    if let Err(parse) = richview_core::document::parse(source) {
                        diagnostics.extend(parse);
                    } else if let Ok(document) = richview_core::document::parse(source) {
                        diagnostics.extend(document.diagnostics);
                    }
                    diagnostics.dedup();
                    let blocks =
                        richview_core::document::parse(source).map_or(0, |d| d.blocks.len());
                    (blocks, diagnostics)
                }
                other => return Err(other),
            },
        };
    if publish {
        diagnostics.extend(richview_core::document::validate_publish(source));
        diagnostics.extend(slug_derived(source, None));
    }
    Ok(envelope(diagnostics, None, json!({"blocks": blocks})))
}

/// `validate` on text and a folder inventory alone, as the HTTP surface receives them: nothing is
/// read from a disk and nothing is stored (ARC-08).
pub fn validate_text(
    source: &str,
    folder: Option<&richview_core::document::ReportFolder>,
    publish: bool,
) -> Outcome {
    let (blocks, mut diagnostics) = match richview_core::document::parse(source) {
        Ok(mut document) => {
            match richview_core::document::attach_asset_findings(&mut document, folder) {
                Ok(()) => (document.blocks.len(), document.diagnostics),
                Err(found) => {
                    let mut diagnostics = document.diagnostics;
                    diagnostics.extend(found);
                    (0, diagnostics)
                }
            }
        }
        Err(found) => (0, found),
    };
    if publish {
        diagnostics.extend(richview_core::document::validate_publish(source));
        diagnostics.extend(slug_derived(source, None));
    }
    Ok(envelope(diagnostics, None, json!({"blocks": blocks})))
}

/// `check` on text and already-resolved options, as the HTTP surface receives them.
pub fn check_text(
    source: &str,
    mut options: RenderOptions,
    folder: Option<&richview_core::document::ReportFolder>,
    widths: &[f64],
) -> Outcome {
    for width in widths {
        if !width.is_finite() || !(240.0..=4096.0).contains(width) {
            return Err(Failure::usage(
                "Chart width must be between 240 and 4096 pixels.",
            ));
        }
    }
    let mut document = richview_core::document::parse(source).map_err(Failure::Diagnostics)?;
    richview_core::document::attach_asset_findings(&mut document, folder)
        .map_err(Failure::Diagnostics)?;
    options.chart_widths = widths.to_vec();
    let report = richview_core::document::check_document(document, &options)?;
    Ok(envelope(
        report.diagnostics,
        None,
        json!({"charts": report.charts}),
    ))
}

/// The `publish.slug-derived` warning: the address comes from the title, so a new title would
/// publish to a new address (FEA-SF-13).
fn slug_derived(source: &str, explicit: Option<&str>) -> Option<Diagnostic> {
    if explicit.is_some() {
        return None;
    }
    let document = richview_core::document::parse(source).ok()?;
    if document.frontmatter["slug"].is_string() {
        return None;
    }
    let slug = richview_core::create_publish_request(source, None)
        .ok()?
        .slug;
    Some(
        Diagnostic::new(
            "warning",
            "publish.slug-derived",
            format!("The report address `{slug}` was derived from the title, so a new title would publish to a new address."),
            "frontmatter.slug",
        )
        .fix(format!("Add `slug: {slug}` to the frontmatter")),
    )
}

/// `check`: render and return every diagnostic and the per-chart geometry results (FEA-SF-04).
/// `widths` replaces the widths figures occupy for readers; empty keeps them.
pub fn check(input: &Input, widths: &[f64]) -> Outcome {
    for width in widths {
        if !width.is_finite() || !(240.0..=4096.0).contains(width) {
            return Err(Failure::usage(
                "Chart width must be between 240 and 4096 pixels.",
            ));
        }
    }
    let (document, mut options) = resolve(input, false)?;
    options.chart_widths = widths.to_vec();
    let report = richview_core::document::check_document(document, &options)?;
    Ok(envelope(
        report.diagnostics,
        None,
        json!({"charts": report.charts}),
    ))
}

/// How `generate` writes its output.
#[derive(Debug, Clone, Default)]
pub struct GenerateRequest {
    pub input: Input,
    /// A file name inside `Rendered/`.
    pub output: Option<PathBuf>,
    pub pdf: bool,
    pub page: richview_core::document::Page,
    pub browser: Option<PathBuf>,
    pub settle_timeout: std::time::Duration,
}

/// `generate`: the self-contained HTML report, or a PDF printed by a local browser
/// (FEA-SF-05, FEA-SF-06). A report with block errors is still written; the result says `ok: false`.
pub async fn generate(request: &GenerateRequest) -> Outcome {
    let (document, mut options) = resolve(&request.input, true)?;
    if request.pdf {
        options.output = richview_core::document::Output::Pdf;
        options.page = request.page;
    }
    let report = richview_core::document::render_document(document, &options)?;
    let blocks = render_meta(&report, "block_count")
        .cloned()
        .unwrap_or(json!(0));
    let charts = render_meta(&report, "chart_diagnostics")
        .cloned()
        .unwrap_or(json!([]));
    let interactive = render_meta(&report, "interactive").cloned();
    if request.pdf {
        // A PDF leaves the author's control once sent: it is refused while any error remains,
        // listing the errors and writing nothing (FEA-RP-44).
        let errors: Vec<Diagnostic> = report
            .diagnostics
            .iter()
            .filter(|d| d.level == "error")
            .cloned()
            .collect();
        if !errors.is_empty() {
            return Err(Failure::Diagnostics(errors));
        }
        let (output, export) = crate::export_pdf::export(
            &request.input.path,
            request.output.as_deref(),
            &report,
            &options,
            request.browser.clone(),
            request
                .settle_timeout
                .max(std::time::Duration::from_secs(1)),
        )
        .await?;
        let mut diagnostics = report.diagnostics.clone();
        diagnostics.extend(export.diagnostics.iter().cloned());
        return Ok(envelope(
            diagnostics,
            Some(&output),
            // Slides print on their own 16:9 page whatever `--page` asked for (FEA-RP-65).
            json!({"blocks":blocks,"charts":charts,"interactive":interactive,"page":if render_meta(&report, "slides").is_some() { json!("16:9") } else { json!(options.page) },"pages":export.pages(),"bytes":export.pdf.len(),"browser":export.browser,"settled":export.settled}),
        ));
    }
    let output = match resolver::export_report(
        &request.input.path,
        request.output.as_deref(),
        &report,
        &options,
    ) {
        Ok(output) => output,
        // Writing failed after rendering: the author gets every finding, not only the last one.
        Err(error) => return Err(with_findings(report.diagnostics, error.into())),
    };
    Ok(envelope(
        report.diagnostics,
        Some(&output),
        json!({"blocks":blocks,"charts":charts,"interactive":interactive}),
    ))
}

/// `failure` with the findings of the render before it, each finding once.
fn with_findings(mut found: Vec<Diagnostic>, failure: Failure) -> Failure {
    let Failure::Diagnostics(later) = failure else {
        return failure;
    };
    for diagnostic in later {
        if !found.contains(&diagnostic) {
            found.push(diagnostic);
        }
    }
    Failure::Diagnostics(found)
}

/// The `learn` text: the bundle, or one section (FEA-SF-47). Both surfaces print these bytes.
pub fn learn(section: Option<&str>) -> Result<String, Failure> {
    let Some(section) = section else {
        return Ok(richview_core::document::learn_bundle());
    };
    richview_core::document::learn_section(section)
        .and_then(|s| {
            serde_json::to_value(s)
                .ok()?
                .get("body")?
                .as_str()
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            let known: Vec<String> = richview_core::document::learn_sections()
                .into_iter()
                .map(|s| s.kind)
                .collect();
            Failure::usage(format!(
                "Unknown section '{section}'. Known: {}.",
                known.join(", ")
            ))
        })
}

/// `init`: write the hello-world report and its data into `dir`, never overwriting (FEA-SF-02).
/// Returns the path of the `.rmd`.
pub fn init(dir: &Path, name: &str) -> Result<PathBuf, Failure> {
    crate::storage::validate_slug(name)?;
    let dir = crate::cli::absolute(dir)?;
    let file = dir.join(format!("{name}.rmd"));
    let data = dir.join("data/visits.csv");
    for path in [&file, &data] {
        if path.exists() {
            return Err(Failure::host(format!(
                "{} already exists; nothing was written.",
                path.display()
            )));
        }
    }
    let write = |path: &Path, text: &str| -> Result<(), Failure> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Failure::host(e.to_string()))?;
        }
        let mut target = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| Failure::host(format!("{}: {e}", path.display())))?;
        std::io::Write::write_all(&mut target, text.as_bytes())
            .map_err(|e| Failure::host(e.to_string()))
    };
    write(&data, HELLO_DATA)?;
    write(&file, HELLO_REPORT)?;
    Ok(file)
}

/// `publish`: send the report's source and files to a server with a bearer token (FEA-SF-12).
/// The client runs the validation first; the server runs it again.
pub async fn publish(
    input: &Input,
    server: &str,
    token: &str,
    slug: Option<&str>,
    new: bool,
) -> Outcome {
    publish_as(input, server, token, slug, new, None).await
}

/// `publish` that also says who can open the report: `private`, `org` or `public`. `None` keeps
/// a report's setting and leaves a new one private.
pub async fn publish_as(
    input: &Input,
    server: &str,
    token: &str,
    slug: Option<&str>,
    new: bool,
    visibility: Option<&str>,
) -> Outcome {
    let source = crate::cli::read_source(&input.path)?;
    let (_, options) = resolver::resolve_document_options_policy(
        &source,
        &input.path,
        &input.styles,
        false,
        false,
    )?;
    // Existing scripts can explicitly allow creation even on machines without writable config.
    let first = match crate::publish_destination::first_publication(&input.path, server) {
        Ok(first) => first,
        Err(_) if new => None,
        Err(error) => return Err(error),
    };
    let mut result = crate::cli::publish_as(
        &source,
        &options,
        server,
        token,
        slug,
        new || first.is_some(),
        visibility,
    )
    .await?;
    if let Some(history) = first {
        history.set_len(1).map_err(|error| {
            Failure::host(format!(
                "Published at {}, but could not save the local publish history: {error}",
                result["url"].as_str().unwrap_or(server)
            ))
        })?;
    }
    let mut diagnostics: Vec<Diagnostic> = slug_derived(&source, slug).into_iter().collect();
    // The server's render may add findings of its own; the envelope fields are computed here,
    // from both lists, whatever an older or newer server sent.
    if let Some(sent) = result.as_object_mut() {
        if let Some(server) = sent.remove("diagnostics") {
            diagnostics.extend(server_findings(server));
        }
        for key in ["ok", "status", "output"] {
            sent.remove(key);
        }
    }
    Ok(envelope(diagnostics, None, result))
}

/// The server's findings, one at a time, so one this client cannot read whole never drops the
/// rest: a finding that keeps its code and message keeps them, with its level and path when they
/// are text (`warning` and `publish` when not).
fn server_findings(sent: Value) -> Vec<Diagnostic> {
    let Value::Array(entries) = sent else {
        return vec![];
    };
    entries
        .into_iter()
        .filter_map(|entry| {
            serde_json::from_value::<Diagnostic>(entry.clone())
                .ok()
                .or_else(|| {
                    let text = |key: &str| entry.get(key).and_then(Value::as_str);
                    Some(Diagnostic::new(
                        text("level").unwrap_or("warning"),
                        text("code")?,
                        text("message")?,
                        text("path").unwrap_or("publish"),
                    ))
                })
        })
        .collect()
}

/// An owner action on a stored report (FEA-SF-53).
#[derive(Debug, Clone, Copy)]
pub enum OwnerAction<'a> {
    /// Delete a version for good; its address answers 410 from then on.
    Delete { slug: &'a str, version: &'a str },
    /// Make a version the one `/r/{slug}` serves.
    Pin { slug: &'a str, version: &'a str },
    /// Return `/r/{slug}` to the highest release version.
    Unpin { slug: &'a str },
    /// Re-render a stored version without storing it, and say whether the HTML changed.
    Regenerate { slug: &'a str, version: &'a str },
}

/// `report delete|pin|unpin|regenerate`: one request to the server with the owner's token.
pub async fn report(server: &str, token: &str, action: OwnerAction<'_>) -> Outcome {
    use reqwest::Method;
    let server = crate::remote::server_url(server)?;
    let (method, path, body, what) = match action {
        OwnerAction::Delete { slug, version } => (
            Method::DELETE,
            format!("/api/reports/{slug}/v/{version}"),
            None,
            "deleted",
        ),
        OwnerAction::Pin { slug, version } => (
            Method::PUT,
            format!("/api/reports/{slug}/latest"),
            Some(json!({"version": version})),
            "pinned",
        ),
        OwnerAction::Unpin { slug } => (
            Method::DELETE,
            format!("/api/reports/{slug}/latest"),
            None,
            "unpinned",
        ),
        OwnerAction::Regenerate { slug, version } => (
            Method::POST,
            format!("/api/reports/{slug}/v/{version}/regenerate"),
            None,
            "regenerated",
        ),
    };
    let reply = crate::remote::send(&server, token, method, &path, body.as_ref()).await?;
    let diagnostics: Vec<Diagnostic> =
        serde_json::from_value(reply["diagnostics"].clone()).unwrap_or_default();
    let mut extra = json!({"action": what});
    if let OwnerAction::Pin { version, .. } = action {
        extra["pinned"] = json!(version);
    }
    if let Some(changed) = reply["changed"].as_bool() {
        extra["changed"] = json!(changed);
    }
    Ok(envelope(diagnostics, None, extra))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_survive_the_string_boundary_and_stay_out_of_error() {
        let diagnostics = vec![Diagnostic::new(
            "error",
            "report.asset.missing",
            "No file.",
            "image.src",
        )];
        let failure = Failure::from(serde_json::to_string(&diagnostics).unwrap());
        assert_eq!(failure.diagnostics(), diagnostics.as_slice());
        let json = failure.to_json();
        assert_eq!(json["ok"], false);
        assert!(json.get("error").is_none());
        let host = Failure::from("Disk full.".to_string());
        assert_eq!(host.to_json()["error"], "Disk full.");
        assert_eq!(host.to_json()["diagnostics"], json!([]));
        assert_eq!(
            Failure::from("\u{1}usage:Bad flag".to_string()).exit_code(),
            2
        );
        assert_eq!(host.exit_code(), 1);
    }

    #[test]
    fn status_is_clean_only_without_errors_and_warnings() {
        let info = Diagnostic::new("info", "chart.binning", "Binned.", "chart");
        assert_eq!(
            envelope(vec![info.clone()], None, json!({}))["status"],
            "clean"
        );
        let warning = Diagnostic::new("warning", "chart.clipping", "Clipped.", "chart");
        let result = envelope(vec![warning], None, json!({"blocks": 2}));
        assert_eq!(
            (result["ok"].as_bool(), result["status"].as_str()),
            (Some(true), Some("issues"))
        );
        assert_eq!(result["blocks"], 2);
    }

    #[test]
    fn learn_names_the_sections_when_one_is_unknown() {
        assert!(
            learn(Some("nope"))
                .unwrap_err()
                .message()
                .contains("anti-slop")
        );
        assert_eq!(
            learn(None).unwrap(),
            richview_core::document::learn_bundle()
        );
    }

    #[test]
    fn one_unreadable_server_finding_never_drops_the_others() {
        let findings = server_findings(json!([
            {"level": "warning", "code": "chart.clipping", "message": "Clipped.", "path": "chart"},
            {"level": "error", "code": "publish.quota", "message": "Over quota.", "path": "publish", "line": "seven"},
            {"code": "block.source", "message": "Add a source."},
            {"level": "error", "message": "No code."},
        ]));
        let kept: Vec<_> = findings
            .iter()
            .map(|d| (d.level.as_str(), d.code.as_str(), d.path.as_str()))
            .collect();
        assert_eq!(
            kept,
            [
                ("warning", "chart.clipping", "chart"),
                ("error", "publish.quota", "publish"),
                ("warning", "block.source", "publish"),
            ]
        );
        assert!(server_findings(json!({"not": "a list"})).is_empty());
    }

    #[test]
    fn a_title_derived_slug_gets_a_warning_and_an_authored_one_does_not() {
        let derived = "---\ntitle: Visits grew\nversion: 0.0.1\n---\n\nText\n";
        let warning = slug_derived(derived, None).unwrap();
        assert_eq!(warning.code, "publish.slug-derived");
        assert!(warning.message.contains("visits-grew"));
        assert!(slug_derived(derived, Some("chosen")).is_none());
        let authored = "---\ntitle: Visits grew\nslug: visits\nversion: 0.0.1\n---\n\nText\n";
        assert!(slug_derived(authored, None).is_none());
    }
}
