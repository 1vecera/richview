//! The report runtime the binary carries (ARC-07).
//!
//! `richview` embeds the runtime at build time, so a report generated from any working directory
//! is self-contained. No flag or environment variable selects a runtime from disk; only the build
//! tools load staged files while bootstrapping the native binaries.
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

include!(concat!(env!("OUT_DIR"), "/runtime_bytes.rs"));
include!(concat!(env!("OUT_DIR"), "/docs_bytes.rs"));

/// Whether this binary contains the generated docs runtime used by hosted app pages.
pub fn docs_available() -> bool {
    DOCS_JS.is_some() && DOCS_WASM.is_some() && DOCS_BOOTSTRAP.is_some()
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
