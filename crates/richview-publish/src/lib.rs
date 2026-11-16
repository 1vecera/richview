//! Publication contracts and deterministic native assembly; report geometry stays in RichView.
mod build;
mod cover;
mod presentation;
pub use build::{build, build_with_connectors};
pub use richview_publish_server::model;
pub use richview_publish_server::model::*;
