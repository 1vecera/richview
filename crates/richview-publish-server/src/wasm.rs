//! The Wasm entry for the Cloudflare Worker adapter: one function, JSON in and out (ARC-03).
use wasm_bindgen::prelude::wasm_bindgen;

/// Runs one call of the Publish server; see `wire` for the shape.
#[wasm_bindgen]
pub fn publish_handle(input: &str) -> String {
    crate::wire::handle_json(input)
}

/// Converts PR #70's stored ledger into this server's; see `wire::migrate_json`.
#[wasm_bindgen]
pub fn publish_migrate(input: &str) -> String {
    crate::wire::migrate_json(input)
}
