//! Parse, validate, render and check Rich Markdown using resources supplied by the host.
//!
//! Parse the source, validate the report-folder inventory, discover its resources and supply their
//! resolved bytes through [`RenderOptions::data`]. Rendering and checks perform no I/O and return
//! the original [`Diagnostic`] records. A parsed [`Document`] can cross a worker boundary through
//! Serde before rendering; its generated block ownership is retained.
//!
//! Document types are accessed through this module, while positional YAML loading and retired
//! schema migration records remain internal.
//!
//! ```compile_fail
//! use richview_core::RenderOptions;
//! ```
//!
//! ```compile_fail
//! richview_core::yaml::load("title: Report");
//! ```
//!
//! ```compile_fail
//! let _ = richview_core::registry::RETIRED;
//! ```

pub use crate::folder::{
    ReportFolder, ReportResource, apple_double_sidecar, attach_asset_findings, is_source_file,
    locate_report_input, operation_result, report_relative_path, report_resources,
    validate_report_document, validate_report_folder,
};
pub use crate::knowledge::{LearnSection, learn_bundle, learn_section, learn_sections};
pub use crate::parser::{parse, validate, validate_publish};
pub use crate::render::{check, check_document, render, render_document};
pub use crate::slides::{SlideSpan, is_deck};

use crate::{PAGE_MARGINS_MM, registry};
use pulldown_cmark::Options;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The CommonMark extensions a report's Markdown is read with (TYP-34). The renderer and every
/// scan of report Markdown, in core and in its hosts, use these and no others, so a scan never
/// finds structure the renderer does not render: wikilinks, old-style footnotes, smart
/// punctuation or metadata blocks.
pub const MARKDOWN_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_DEFINITION_LIST)
    .union(Options::ENABLE_MATH)
    .union(Options::ENABLE_HEADING_ATTRIBUTES);
/// The major version of the `.rmd` format this crate reads and writes (`format: rmd/1`).
pub const FORMAT_MAJOR: u32 = 1;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The `.rmd` source: `source.limit` (TYP-43).
pub const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
/// One resolved dataset: `data.limit` (TYP-43).
pub const MAX_DATASET_BYTES: usize = 16 * 1024 * 1024;
/// The `data` keys that name a file in the report folder (TYP-35). The key decides how its
/// content is read (TYP-92).
pub(crate) const DATA_FILE_KEYS: [&str; 3] = ["csv", "json", "parquet"];
/// Maximum accumulated rich-block HTML and final report HTML, including embedded assets.
pub const MAX_REPORT_BYTES: usize = 64 * 1024 * 1024;

/// The one diagnostic record every surface returns unchanged (TYP-53, ARC-08).
///
/// `path` is a field path in one grammar: dot-separated keys with `[n]` indices, rooted at the
/// block's tag (`chart.encoding.y`), at `frontmatter`, at `styles[n]` for a style file or at
/// `rows[n].field` for a resolved data row. `line` and `column` are 1-based source positions
/// (TYP-56); `file` names a report-relative file when the source is not the `.rmd`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diagnostic {
    pub level: String,
    pub code: String,
    pub message: String,
    /// How to fix it, in one sentence.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fix: Option<String>,
    /// The allowed values or grammar, when a value broke its type, enum or range.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected: Option<String>,
    /// Only on host failures (I/O, network, server): whether the same request can succeed later.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retry: Option<bool>,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tag: Option<String>,
    /// Code-specific data, such as a chart check's geometry or dropped-label provenance.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub details: Option<Value>,
}

impl Diagnostic {
    pub fn new(
        level: &str,
        code: &str,
        message: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            level: level.into(),
            code: code.into(),
            // No finding repeats a credential written into an address (ARC-14).
            message: redacted(message.into()),
            fix: registry::fix(code).map(str::to_owned),
            expected: None,
            retry: None,
            path: path.into(),
            file: None,
            line: None,
            column: None,
            tag: None,
            details: None,
        }
    }
    pub(crate) fn at(mut self, line: usize, tag: &str) -> Self {
        self.line = Some(line);
        self.tag = Some(tag.into());
        self
    }
    /// How to fix it, in one sentence.
    pub fn fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(redacted(fix.into()));
        self
    }
    /// The allowed values or grammar.
    pub fn expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }
    /// A host failure, and whether the same request can succeed later.
    pub fn retry(mut self, retry: bool) -> Self {
        self.retry = Some(retry);
        self
    }
    /// Code-specific data, such as a stored hash.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
    /// The report-relative file the position is in, when it is not the `.rmd`.
    pub fn in_file(mut self, file: impl Into<String>) -> Self {
        self.file = Some(file.into());
        self
    }
}

/// `text`, unless an address in it carries a user name or password (ARC-14).
fn redacted(text: String) -> String {
    if text.contains("://") && text.contains('@') {
        redact_userinfo(&text)
    } else {
        text
    }
}

/// `text` with the user name and password of each address in it replaced by `***`
/// (`https://***@host/`), so no finding or notice repeats a credential (ARC-14).
pub(crate) fn redact_userinfo(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let authority = &tail[..authority_end(tail)];
        match authority.rfind('@') {
            Some(i) => {
                out.push_str("***");
                out.push_str(&authority[i..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[authority.len()..];
    }
    out.push_str(rest);
    out
}

/// Whether `url` carries a user name or password before its host (`https://user:pass@host`).
pub(crate) fn has_userinfo(url: &str) -> bool {
    url.split_once("://")
        .is_some_and(|(_, tail)| tail[..authority_end(tail)].contains('@'))
}

/// Where the authority that starts `tail`, the text after `://`, ends.
fn authority_end(tail: &str) -> usize {
    tail.find(|c: char| {
        matches!(
            c,
            '/' | '?' | '#' | '\'' | '"' | '`' | '<' | '>' | '(' | ')' | '[' | ']'
        ) || c.is_whitespace()
    })
    .unwrap_or(tail.len())
}

/// Whether a block holds credentials: retired request headers in a `data` mapping, or an address
/// with a user name or password anywhere in it. Its notice then never shows the fence (ARC-14).
pub(crate) fn holds_credentials(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, v)| (key == "data" && v.get("headers").is_some()) || holds_credentials(v)),
        Value::Array(items) => items.iter().any(holds_credentials),
        Value::String(text) => text.contains('@') && redact_userinfo(text) != *text,
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichBlock {
    pub tag: String,
    pub value: Value,
    pub raw: String,
    /// One-based source line, including frontmatter.
    pub line: usize,
}

/// A rich fence that did not become a block: the report shows a notice in its place, with the
/// fence's source (FEA-RP-29, FEA-RP-53).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedBlock {
    pub tag: String,
    /// The fence as written, for the notice's code block.
    pub source: String,
    /// One-based source line of the fence.
    pub line: usize,
    /// Why it failed: the first error that stopped it.
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub frontmatter: Value,
    /// CommonMark with generated rich-block placeholders, never executed as code.
    pub body: String,
    pub blocks: Vec<RichBlock>,
    pub diagnostics: Vec<Diagnostic>,
    /// Collision-free generated marker namespace, present only when source contains the
    /// ordinary reserved marker. Legacy caller-authored ASTs default to the ordinary prefix.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) block_placeholder_prefix: Option<String>,
    /// The source line of the first body line; 0 when the body did not come from a source file.
    /// Worker documents retain it so later Markdown diagnostics name real source lines.
    #[serde(default)]
    pub(crate) body_line: usize,
    /// The source line of each frontmatter field, for diagnostics the renderer raises.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) frontmatter_lines: BTreeMap<String, usize>,
    /// Field columns accompany lines on worker documents; older documents default to column 1.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) frontmatter_columns: BTreeMap<String, usize>,
    /// Rich fences that failed, in source order; the body holds a marker for each.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub failed: Vec<FailedBlock>,
    /// The slides of a `.rslide` deck, whose slots hold the blocks above and the text it keeps;
    /// the body of a deck is empty (FEA-RP-71).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub(crate) deck: Option<crate::slides::Deck>,
}

impl Document {
    /// The Markdown a deck's slides hold, each text with the source line it starts on: slide
    /// texts and furniture, where a report has its body. Empty for a report.
    pub fn deck_markdown(&self) -> Vec<(&str, usize)> {
        self.deck
            .as_ref()
            .map(|deck| {
                deck.texts()
                    .into_iter()
                    .map(|(text, line, _)| (text, line))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Each slide of a deck and where it stands in the source, in order; empty for a report.
    pub fn deck_slides(&self) -> Vec<SlideSpan> {
        self.deck
            .as_ref()
            .map(|deck| deck.spans())
            .unwrap_or_default()
    }

    /// The number of the slide the source line belongs to, from 1, or `None` for a line of a
    /// deck's setup, a line past the last slide and any line of a report.
    pub fn slide_at_line(&self, line: usize) -> Option<usize> {
        self.deck_slides()
            .into_iter()
            .find(|span| (span.first_line..=span.last_line).contains(&line))
            .map(|span| span.number)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderOptions {
    /// Builtin names or resolved YAML sources; ordered, later leaves override earlier leaves.
    pub styles: Vec<String>,
    /// Explicit host inventory of the dedicated physical or virtual report folder.
    pub report_folder: Option<ReportFolder>,
    /// Resolved CSV/JSON bytes or safe media data URIs, keyed by source reference or named ID.
    pub data: BTreeMap<String, String>,
    pub width: f64,
    /// Widths `check` measures figures at instead of the ones readers get (phone column and
    /// the style's measure); empty keeps those.
    pub chart_widths: Vec<f64>,
    /// Exact trusted binding bytes as UTF-8. Linked runtimes use these to calculate integrity;
    /// embedded runtimes also put them in the report.
    pub runtime_js: Option<String>,
    /// Trusted host base URL for cached preview bindings/Wasm, without extension: an absolute
    /// HTTP(S) URL or a root-relative path on the serving host. Exports use embedded runtime
    /// fields instead; report source cannot choose this URL. Requires matching `runtime_js`.
    /// Supplying raw `runtime_wasm` bytes keeps an opaque preview from fetching Wasm.
    pub runtime_url: Option<String>,
    /// WebAssembly bytes encoded as base64 by a trusted caller. With `runtime_url`, only raw
    /// bytes are supported; without it, the embedded binding also supports the gzip codec.
    pub runtime_wasm: Option<String>,
    /// Packaging codec for trusted embedded bytes: raw or gzip. Decompression uses the
    /// browser's host stream API before the Rust module starts.
    pub runtime_wasm_codec: String,
    pub source: Option<String>,
    /// The interactive report, or the paged document a browser prints to PDF.
    pub output: Output,
    /// Paper size of the `pdf` output.
    pub page: Page,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            styles: vec![],
            report_folder: None,
            data: BTreeMap::new(),
            width: 720.0,
            chart_widths: vec![],
            runtime_js: None,
            runtime_url: None,
            runtime_wasm: None,
            runtime_wasm_codec: "raw".into(),
            source: None,
            output: Output::Report,
            page: Page::A4,
        }
    }
}

/// What a render produces. `pdf` is the same report prepared for paper: light colours, a page
/// box with a running title and page numbers, settled charts and print renditions of media.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    #[default]
    Report,
    Pdf,
}

/// Paper sizes of the `pdf` output. Margins are 20 mm at the top, 18 mm at the sides and
/// 22 mm at the bottom, where the running title and page numbers sit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Page {
    #[default]
    A4,
    Letter,
}

impl Page {
    /// The CSS `@page` size keyword.
    pub fn css_size(self) -> &'static str {
        match self {
            Page::A4 => "A4",
            Page::Letter => "letter",
        }
    }
    /// Width and height of the printed text area in CSS pixels (96 per inch), so a host can lay
    /// the report out at the size it prints at.
    pub fn text_area(self) -> (f64, f64) {
        let (width, height) = match self {
            Page::A4 => (210.0, 297.0),
            Page::Letter => (215.9, 279.4),
        };
        let px = |mm: f64| mm * 96.0 / 25.4;
        (
            px(width - 2.0 * PAGE_MARGINS_MM[1]),
            px(height - PAGE_MARGINS_MM[0] - PAGE_MARGINS_MM[2]),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedReport {
    /// The company identity actually used to render this report, for native publishing adapters.
    #[serde(skip)]
    pub company: crate::CompanyBrand,
    pub html: String,
    pub diagnostics: Vec<Diagnostic>,
    /// Frontmatter retained verbatim, with generated `_richview` information.
    pub metadata: Value,
    /// Chart runtime payloads; geometry diagnostics are returned by [`check`].
    pub charts: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckReport {
    pub status: String,
    pub diagnostics: Vec<Diagnostic>,
    pub charts: Vec<Value>,
}
