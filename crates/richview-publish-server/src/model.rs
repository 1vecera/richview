use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Site {
    pub title: String,
    pub description: String,
    pub base_url: String,
    pub layout: Layout,
    pub mode: Mode,
    #[serde(default)]
    pub theme: Theme,
    pub sections: Vec<Section>,
    pub navigation: Vec<Navigation>,
    pub lead: Vec<String>,
    #[serde(default = "default_accent")]
    pub accent: String,
}
/// Where a story page's link-preview tags go. The stored article carries only this slot; the server
/// fills it per request (FEA-RP-51), so a story that is not public today advertises nothing.
pub const SOCIAL_SLOT: &str = "<!--PUBLISH:SOCIAL-->";
/// What the slot becomes when the story has no public preview: the generic page type.
pub const OG_WEBSITE: &str = "<meta property=\"og:type\" content=\"website\">";
/// Asset name of a story's 1200x630 link-preview PNG, served at `/_content/<id>/og.png`.
pub const SHARE_IMAGE: &str = "og.png";
pub const DEFAULT_ACCENT: &str = "#1a6dac";
fn default_accent() -> String {
    DEFAULT_ACCENT.into()
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    Broadsheet,
    Dispatch,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    External,
    Internal,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Paper,
    Ink,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub description: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Navigation {
    pub label: String,
    pub path: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Draft,
    Published,
    Hidden,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Retired by FEA-BR-30: no builder produces it, but a release stored before the retirement may list one.
    /// The server never serves it (410); the builder refuses a source that names it.
    Dashboard,
    Report,
    Page,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Correction {
    pub date: String,
    pub note: String,
}
/// One headline number a story stands behind, shown in the front-page figures strip.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Figure {
    /// The number as printed, such as `84%` or `−0.2 pp`; at most 16 characters.
    pub value: String,
    /// What the number measures; at most 80 characters.
    pub label: String,
    /// When the number was true, such as `Q3 2026`; at most 32 characters.
    pub as_of: String,
}
/// Who made a cover photo, in the same shape as an image `credit` in Rich Markdown.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Credit {
    /// The photographer's name; required.
    pub author: String,
    /// Their profile, an http(s) address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_url: Option<String>,
    /// Where the photo comes from, such as `Unsplash`; a remote photo defaults to Unsplash.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The source's address, an http(s) address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub id: String,
    pub slug: String,
    pub summary: String,
    pub byline: String,
    pub section: String,
    pub tags: Vec<String>,
    pub published: String,
    pub updated: String,
    pub state: State,
    /// `public`, `members`, or an explicit entitlement such as `research`.
    pub audience: String,
    pub kind: Kind,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub corrections: Vec<Correction>,
    #[serde(default)]
    pub cover: Option<String>,
    #[serde(default)]
    pub cover_alt: Option<String>,
    /// Required for a remote cover; optional for a local one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_credit: Option<Credit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figure: Option<Figure>,
    /// Retired with the dashboard kind (FEA-BR-30); read from stored releases, refused in sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    #[serde(flatten)]
    pub meta: Metadata,
    pub title: String,
    /// Relative to the site directory, never a private host path.
    pub source: String,
    pub route: String,
    pub article: String,
    pub content: String,
    /// Pre-rendered list row.
    pub card: String,
    /// Pre-rendered front-page lead treatment.
    pub lead: String,
    /// Pre-rendered figures-strip entry; empty when the item defines no `figure`.
    #[serde(default)]
    pub stat: String,
    pub assets: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    pub sha256: String,
    pub bytes: usize,
    pub content_type: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub schema: u32,
    pub id: String,
    pub commit: String,
    pub toolchain: String,
    pub site: Site,
    pub items: Vec<Item>,
    pub templates: BTreeMap<String, String>,
    pub runtime: BTreeMap<String, String>,
    pub objects: BTreeMap<String, Object>,
}
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
pub fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
