//! Native host for the shared Rust RichView engine. Browser policy remains in richview-site.
pub mod access;
pub mod asset_upload;
pub mod banner;
mod child_env;
pub mod cli;
pub mod connectors;
mod credentials;
mod email;
pub mod export_pdf;
mod hosted_links;
pub mod mail;
pub mod mcp;
pub mod ops;
mod publish_destination;
pub mod reference;
pub mod registration;
pub mod remote;
pub mod resolver;
pub mod runtime;
pub mod server;
mod sign_in;
pub mod social;
pub mod storage;
#[cfg(test)]
mod test_points;
pub mod time;
pub mod unsplash;
#[cfg(windows)]
mod win_process;

pub use resolver::{render_file, resolve_document_options, resolve_options};
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;
