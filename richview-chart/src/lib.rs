//! RichView Chart: the chart engine behind Rich Markdown chart blocks. It performs no I/O.
//!
//! [`block::ChartBlock`] validates a chart block, and [`layout::layout`] turns it, its resolved
//! rows, a width and the style's [`layout::ChartTheme`] into one [`geometry::Geometry`]: scales,
//! marks, labels, annotations and their boxes (ARC-18). Every check measures that model,
//! [`vector`] draws it as SVG for reports and PDF, and the browser runtime draws the same model.

pub mod axis;
pub mod binning;
pub mod block;
pub mod box_summary;
pub mod color;
pub mod curve;
pub mod data;
pub mod derived;
pub mod det;
pub mod diag;
pub mod geometry;
pub mod hit;
pub mod interaction;
pub mod labels;
pub mod layout;
pub mod limits;
pub mod motion;
pub mod numbers;
mod optical_metrics;
pub mod payload;
pub mod pencil;
pub mod reader_controls;
pub mod rmd_render;
pub mod rows;
pub mod scale;
mod source_number;
pub mod svg;
pub mod text;
pub mod time;
pub mod vector;
pub mod world;

pub use diag::{Diagnostic, Level};
