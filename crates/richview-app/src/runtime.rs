//! The report runtime the binary carries (ARC-07).
//!
//! `richview` embeds the runtime at build time, so a report generated from any working directory
//! is self-contained. No flag or environment variable selects a runtime from disk; only the build
//! tools load staged files while bootstrapping the native binaries.
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/runtime_bytes.rs"));
include!(concat!(env!("OUT_DIR"), "/docs_bytes.rs"));
include!(concat!(env!("OUT_DIR"), "/editor_bytes.rs"));

/// Whether this binary contains the generated docs runtime used by hosted app pages.
pub fn docs_available() -> bool {
    DOCS_JS.is_some() && DOCS_WASM.is_some() && DOCS_BOOTSTRAP.is_some()
}

/// Whether this binary contains what the online editor loads: the site runtime, its render
/// worker, the font catalogs and the report runtime its preview frame runs.
pub fn editor_available() -> bool {
    SITE_JS.is_some()
        && SITE_WASM.is_some()
        && SITE_WORKER.is_some()
        && SITE_FONTS.is_some()
        && SITE_MATH_FONTS.is_some()
        && embedded().is_some()
}

/// The report runtime's Wasm module as the editor's preview frame is given it: the raw bytes,
/// decompressed once.
pub fn raw_wasm() -> Option<&'static [u8]> {
    static WASM: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    WASM.get_or_init(|| wasm_bytes().ok()).as_deref()
}

/// The source of a script-src entry that lets an editor page's sandboxed preview frame start the
/// report runtime. A frame made from `srcdoc` inherits its parent's policy, and the preview's one
/// inline module is the report runtime with its Wasm in base64: the same module, whatever the
/// report, so one hash names it. Computed as `richview-build` pins it for the site's pages.
pub fn preview_script_hash() -> Option<&'static str> {
    use base64::Engine as _;
    static HASH: OnceLock<Option<String>> = OnceLock::new();
    HASH.get_or_init(|| {
        let runtime = embedded()?;
        let wasm = raw_wasm()?;
        let options = richview_core::document::RenderOptions {
            runtime_js: Some(runtime.js.clone()),
            runtime_wasm: Some(base64::engine::general_purpose::STANDARD.encode(wasm)),
            runtime_wasm_codec: "raw".into(),
            report_folder: Some(richview_core::document::ReportFolder {
                root: "preview-policy".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        };
        let report = richview_core::document::render("# Preview policy", &options).ok()?;
        let (_, rest) = report.html.split_once("<script type=\"module\">")?;
        let (module, _) = rest.split_once("</script>")?;
        Some(format!(
            "'sha256-{}'",
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(module.as_bytes()))
        ))
    })
    .as_deref()
}

/// The runtime as render options carry it: script text and gzip-compressed Wasm in base64.
pub struct Runtime {
    pub js: String,
    pub wasm_gzip_base64: String,
    /// SHA-256 of the script and the compressed module, in hex.
    pub sha256: String,
}

/// The embedded runtime, or `None` in a build that does not carry one.
pub fn embedded() -> Option<&'static Runtime> {
    static RUNTIME: OnceLock<Option<Runtime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            let (js, wasm) = (RUNTIME_JS?, RUNTIME_WASM_GZ?);
            let mut hash = Sha256::new();
            hash.update(js);
            hash.update(wasm);
            Some(Runtime {
                js: String::from_utf8_lossy(js).into_owned(),
                wasm_gzip_base64: base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    wasm,
                ),
                sha256: format!("{:x}", hash.finalize()),
            })
        })
        .as_ref()
}

/// Requires the runtime this native host carries before producing a report.
pub fn require() -> Result<&'static Runtime, String> {
    embedded().ok_or_else(|| {
        serde_json::to_string(&[richview_core::document::Diagnostic::new(
            "error",
            "report.output.runtime",
            "The embedded report runtime is missing. Run cargo run -p richview-build to stage it and rebuild the native binaries.",
            "output",
        )]).unwrap()
    })
}

/// The runtime's Wasm module, decompressed, as a browser loads it.
pub fn wasm_bytes() -> Result<Vec<u8>, String> {
    use std::io::Read;
    let runtime = require()?;
    let compressed = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        &runtime.wasm_gzip_base64,
    )
    .map_err(|e| e.to_string())?;
    let mut wasm = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_slice())
        .read_to_end(&mut wasm)
        .map_err(|e| e.to_string())?;
    Ok(wasm)
}

/// What `richview --version` prints: the version, the build commit and the runtime hash.
pub fn version_text() -> String {
    format!(
        "{} (commit {}, runtime {})",
        crate::VERSION,
        env!("RICHVIEW_BUILD_COMMIT"),
        embedded().map_or("none".to_owned(), |r| r.sha256[..12].to_owned())
    )
}
