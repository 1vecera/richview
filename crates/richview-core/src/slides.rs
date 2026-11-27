//! Rich Slides (FEA-RP-71, TYP-106): a deck is a format of its own, a `.rslide` file whose slides
//! hold named slots on a stage of 24 columns by 24 rows. This module reads the source, checks it,
//! and composes the slides' HTML around the blocks the report renderer already knows.
//!
//! A source is a deck when its first line is `format: rslide/1`, so every host that is given the
//! text alone (the CLI, the publish server, the editor, the browser worker) reads it the same way
//! through [`crate::document::parse`]. The text before the first `---` line is the setup, which
//! becomes the document's frontmatter; each later `---` line opens a slide. A slide is a mapping of
//! keys at the left edge: furniture (`title`, `subtitle`, `source`, `footer`), the setup of its
//! grid (`cols`, `rows`), and either one piece (`chart`, `metric`, `table` or `text`) or the
//! pieces of the slots of one of eight layouts. The author chooses what a slide holds and where;
//! the stylesheet (`assets/slides.css`) sets everything else from the style.
//!
//! Charts, metrics and tables are written in their own vocabulary. They are read by the report
//! parser itself, from a fence the reader writes for each of them with the deck's line numbers
//! kept, so a block's checks and diagnostics are the report's own.

use crate::{Diagnostic, Document, MAX_SOURCE_BYTES, RenderOptions, RichBlock, escape};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

/// The stylesheet of a deck, added after the report's own.
pub(crate) const CSS: &str = include_str!("../assets/slides.css");

/// The marker a deck's first line carries, and the format's major version.
pub(crate) const FORMAT_NAME: &str = "rslide";
pub(crate) const FORMAT_MAJOR: u32 = 1;

/// The width of the stage a landscape slide is composed on, in CSS pixels. The stylesheet scales
/// the stage to the window, so a chart's reference layout is made for this width.
const STAGE_WIDTH: f64 = 1280.0;
const STAGE_HEIGHT: f64 = STAGE_WIDTH * 9.0 / 16.0;
/// The stage's margins and the gutter between slots, as shares of its width; the stylesheet
/// writes the same shares (`5su`, `3.2su`, `2.6su` and `3su`).
const PAD_SIDE: f64 = 0.05;
const PAD_TOP: f64 = 0.032;
const PAD_BOTTOM: f64 = 0.026;
const GAP: f64 = 0.03;
/// One of the stage's 24 rows, in CSS pixels.
const ROW: f64 = (STAGE_HEIGHT - STAGE_WIDTH * (PAD_TOP + PAD_BOTTOM)) / 24.0;
/// What a chart's own title, legend and axis lettering take of its slot, in CSS pixels.
const CHART_CHROME: f64 = 52.0;

/// The head's rows (1 to 4) and the foot's (23 and 24); content takes the rest.
const HEAD_ROWS: usize = 4;
const FOOT_ROWS: usize = 2;
/// The words a text slot reads up to, in a slot under half the stage's width and in a wider one.
const WORDS_NARROW: usize = 40;
const WORDS_WIDE: usize = 80;
/// The most rows a table shows, and the longest title, before a slide holds more than it shows well.
const MOST_ROWS: usize = 10;
const MOST_TITLE: usize = 90;

const SETUP_KEYS: [&str; 7] = [
    "format",
    "title",
    "description",
    "version",
    "slug",
    "lang",
    "styles",
];
const FURNITURE: [&str; 4] = ["title", "subtitle", "source", "footer"];
const SLOTS: [&str; 9] = [
    "left",
    "right",
    "top",
    "bottom",
    "middle",
    "top-left",
    "top-right",
    "bottom-left",
    "bottom-right",
];
/// The keys that stand for the one piece of a slide with no slots.
const PIECES: [&str; 4] = ["chart", "metric", "table", "text"];
/// The pieces that are blocks, written in the block's own vocabulary.
const BLOCKS: [&str; 3] = ["chart", "metric", "table"];
/// Keys later answers may add; a deck that writes one is told it is not read yet.
const LATER: [&str; 2] = ["notes", "reveal"];

/// The slots of a slide, as one of nine layouts. There are no layout names to learn: the slots a
/// slide uses are its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Layout {
    /// No piece: a cover or a section.
    Bare,
    /// One piece alone.
    One,
    LeftRight,
    TopBottom,
    LeftMiddleRight,
    TopMiddleBottom,
    /// `top` across `bottom-left` and `bottom-right`.
    TopPair,
    /// `bottom` under `top-left` and `top-right`.
    BottomPair,
    /// `left` beside `top-right` and `bottom-right`.
    LeftPair,
    /// `right` beside `top-left` and `bottom-left`.
    RightPair,
}

impl Layout {
    const MANY: [Layout; 8] = [
        Layout::LeftRight,
        Layout::TopBottom,
        Layout::LeftMiddleRight,
        Layout::TopMiddleBottom,
        Layout::TopPair,
        Layout::BottomPair,
        Layout::LeftPair,
        Layout::RightPair,
    ];

    /// The slots in reading order: the row each starts in, then its column.
    fn slots(self) -> &'static [&'static str] {
        match self {
            Layout::Bare => &[],
            Layout::One => &["main"],
            Layout::LeftRight => &["left", "right"],
            Layout::TopBottom => &["top", "bottom"],
            Layout::LeftMiddleRight => &["left", "middle", "right"],
            Layout::TopMiddleBottom => &["top", "middle", "bottom"],
            Layout::TopPair => &["top", "bottom-left", "bottom-right"],
            Layout::BottomPair => &["top-left", "top-right", "bottom"],
            Layout::LeftPair => &["left", "top-right", "bottom-right"],
            Layout::RightPair => &["top-left", "right", "bottom-left"],
        }
    }

    fn name(self) -> &'static str {
        match self {
            Layout::Bare => "bare",
            Layout::One => "one",
            Layout::LeftRight => "left-right",
            Layout::TopBottom => "top-bottom",
            Layout::LeftMiddleRight => "left-middle-right",
            Layout::TopMiddleBottom => "top-middle-bottom",
            Layout::TopPair => "top-pair",
            Layout::BottomPair => "bottom-pair",
            Layout::LeftPair => "left-pair",
            Layout::RightPair => "right-pair",
        }
    }

    /// The layout as the guide and the spec describe it.
    fn describe(self) -> &'static str {
        match self {
            Layout::Bare => "Furniture only",
            Layout::One => "One piece alone",
            Layout::LeftRight => "Two side by side",
            Layout::TopBottom => "Two stacked",
            Layout::LeftMiddleRight => "Three side by side",
            Layout::TopMiddleBottom => "Three stacked",
            Layout::TopPair => "One across two below it",
            Layout::BottomPair => "One across two above it",
            Layout::LeftPair => "One beside two stacked",
            Layout::RightPair => "Two stacked beside one",
        }
    }

    /// How the layout reads in a finding.
    fn words(self) -> String {
        match self {
            Layout::Bare => "no slots".into(),
            Layout::One => "one piece alone".into(),
            Layout::LeftRight => "`left` and `right`".into(),
            Layout::TopBottom => "`top` and `bottom`".into(),
            Layout::LeftMiddleRight => "`left`, `middle` and `right`".into(),
            Layout::TopMiddleBottom => "`top`, `middle` and `bottom`".into(),
            Layout::TopPair => "`top` with `bottom-left` and `bottom-right`".into(),
            Layout::BottomPair => "`bottom` with `top-left` and `top-right`".into(),
            Layout::LeftPair => "`left` with `top-right` and `bottom-right`".into(),
            Layout::RightPair => "`right` with `top-left` and `bottom-left`".into(),
        }
    }

    /// The layout whose slots are exactly `found`.
    fn of(found: &[&str]) -> Option<Layout> {
        Layout::MANY.into_iter().find(|layout| {
            let slots = layout.slots();
            slots.len() == found.len() && slots.iter().all(|slot| found.contains(slot))
        })
    }

    /// How many tracks the layout has across the stage, and down it: what `cols` and `rows` cut.
    fn tracks(self) -> (usize, usize) {
        match self {
            Layout::Bare | Layout::One => (1, 1),
            Layout::LeftRight => (2, 1),
            Layout::TopBottom => (1, 2),
            Layout::LeftMiddleRight => (3, 1),
            Layout::TopMiddleBottom => (1, 3),
            Layout::TopPair | Layout::BottomPair | Layout::LeftPair | Layout::RightPair => (2, 2),
        }
    }

    /// The tracks a slot covers: the first and one past the last column, then row.
    fn area(self, slot: &str) -> (usize, usize, usize, usize) {
        match (self, slot) {
            (Layout::LeftRight, "right") => (1, 2, 0, 1),
            (Layout::TopBottom, "bottom") => (0, 1, 1, 2),
            (Layout::LeftMiddleRight, "middle") => (1, 2, 0, 1),
            (Layout::LeftMiddleRight, "right") => (2, 3, 0, 1),
            (Layout::TopMiddleBottom, "middle") => (0, 1, 1, 2),
            (Layout::TopMiddleBottom, "bottom") => (0, 1, 2, 3),
            (Layout::TopPair, "top") => (0, 2, 0, 1),
            (Layout::TopPair, "bottom-left") => (0, 1, 1, 2),
            (Layout::TopPair, "bottom-right") => (1, 2, 1, 2),
            (Layout::BottomPair, "top-right") => (1, 2, 0, 1),
            (Layout::BottomPair, "bottom") => (0, 2, 1, 2),
            (Layout::LeftPair, "left") => (0, 1, 0, 2),
            (Layout::LeftPair, "top-right") => (1, 2, 0, 1),
            (Layout::LeftPair, "bottom-right") => (1, 2, 1, 2),
            (Layout::RightPair, "bottom-left") => (0, 1, 1, 2),
            (Layout::RightPair, "right") => (1, 2, 0, 2),
            _ => (0, 1, 0, 1),
        }
    }
}

/// The layouts as the spec lists them: the layout's slots in reading order, and how many parts
/// `cols` and `rows` write for it, when it cuts that axis.
pub(crate) fn layout_rows() -> Vec<Vec<String>> {
    let cut = |tracks: usize| {
        if tracks > 1 {
            format!("{tracks} parts")
        } else {
            String::new()
        }
    };
    let mut rows = vec![vec![
        Layout::One.describe().to_owned(),
        "`chart`, `metric`, `table` or `text` as the slide's own key".to_owned(),
        String::new(),
        String::new(),
    ]];
    rows.extend(Layout::MANY.iter().map(|layout| {
        let (across, down) = layout.tracks();
        vec![
            layout.describe().to_owned(),
            layout
                .slots()
                .iter()
                .map(|slot| format!("`{slot}`"))
                .collect::<Vec<_>>()
                .join(", "),
            cut(across),
            cut(down),
        ]
    }));
    rows
}

/// A piece of text with the source line it starts on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Text {
    pub text: String,
    pub line: usize,
}

/// What a slot holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum Content {
    Text(Text),
    /// The index of a block of the document.
    Block(usize),
    /// The index of a fence of the document that failed; its notice shows in the slot.
    Failed(usize),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Piece {
    pub slot: String,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Slide {
    /// The line of the `---` that opens the slide.
    pub line: usize,
    /// The last line of the slide: the one before the next `---`, or the source's last line.
    #[serde(default)]
    pub end: usize,
    pub title: Option<Text>,
    pub subtitle: Option<Text>,
    pub source: Option<Text>,
    pub footer: Option<Text>,
    pub layout: Layout,
    /// The parts of 24 each column track takes; empty for equal tracks.
    pub cols: Vec<u8>,
    pub rows: Vec<u8>,
    /// The pieces in the layout's reading order.
    pub pieces: Vec<Piece>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Deck {
    pub slides: Vec<Slide>,
}

/// Where one slide of a deck stands in its source: what an editor needs to keep its preview on the
/// slide under the caret, and the caret on the slide a reader opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlideSpan {
    /// The slide's number, from 1: the `data-rv-slide` of its section in the rendition and the
    /// `N` of `#slide=N`.
    pub number: usize,
    /// The line of the `---` that opens the slide, from 1.
    pub first_line: usize,
    /// The slide's last line: the one before the next `---`, or the source's last line.
    pub last_line: usize,
    /// The slide's title as the head shows it, without its Markdown marks.
    pub title: Option<String>,
    /// The layout the slide's slots make: `bare`, `one`, `left-right` and so on.
    pub layout: String,
}

/// Whether `source` is a deck: its first line, after blank lines and comments, is `format:
/// rslide/…`.
pub fn is_deck(source: &str) -> bool {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    source
        .lines()
        .map(str::trim_end)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| line.strip_prefix("format:"))
        .is_some_and(|value| {
            value
                .trim()
                .strip_prefix(FORMAT_NAME)
                .is_some_and(|rest| rest.starts_with('/'))
        })
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn syntax(line: usize, message: impl Into<String>, path: &str) -> Diagnostic {
    Diagnostic::new("error", "deck.syntax", message, path).at(line, "slides")
}

fn piece_error(line: usize, message: impl Into<String>, path: &str) -> Diagnostic {
    Diagnostic::new("error", "deck.piece", message, path).at(line, "slides")
}

/// One `key: value` entry at the left edge of the setup or of a slide.
#[derive(Clone)]
struct Entry {
    key: String,
    /// The source line of the key.
    line: usize,
    /// The text after the colon on the key's line, trimmed and taken as written.
    inline: String,
    /// The lines under the key, with their source lines, blank ones included.
    body: Vec<(usize, String)>,
}

/// The entries of `lines`, which hold no `---` line. A line that is not one is a finding.
fn entries(lines: &[(usize, &str)], path: &str, found: &mut Vec<Diagnostic>) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for &(number, line) in lines {
        if line.trim().is_empty() {
            if let Some(entry) = out.last_mut() {
                entry.body.push((number, String::new()));
            }
            continue;
        }
        let indent = indent_of(line);
        if line[..indent].contains('\t')
            || line.starts_with('\t')
            || line[indent..].starts_with('\t')
        {
            found.push(
                syntax(number, "Indent with spaces, not tabs", path)
                    .fix("Replace the tab with spaces."),
            );
            continue;
        }
        if indent > 0 {
            match out.last_mut() {
                Some(entry) => entry.body.push((number, line.to_owned())),
                None => found.push(syntax(
                    number,
                    "This line is indented, but no key stands above it",
                    path,
                )),
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        let split = line.split_once(':').filter(|(key, rest)| {
            !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && (rest.is_empty() || rest.starts_with(' '))
        });
        match split {
            Some((key, rest)) => out.push(Entry {
                key: key.to_owned(),
                line: number,
                inline: rest.trim().to_owned(),
                body: Vec::new(),
            }),
            None => found.push(
                syntax(
                    number,
                    format!("Expected `key: value`, found `{}`", line.trim()),
                    path,
                )
                .fix("Write each setting as `key: value` at the left edge; indent what belongs to a key under it."),
            ),
        }
    }
    out
}

/// The lines without the blank ones at either end.
fn trim_blank(mut lines: Vec<(usize, String)>) -> Vec<(usize, String)> {
    while lines.last().is_some_and(|(_, line)| line.trim().is_empty()) {
        lines.pop();
    }
    let lead = lines
        .iter()
        .take_while(|(_, line)| line.trim().is_empty())
        .count();
    lines.drain(..lead);
    lines
}

/// The lines with their common indent removed, and that indent.
fn dedent(lines: &[(usize, String)]) -> (usize, Vec<(usize, String)>) {
    let base = lines
        .iter()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(_, line)| indent_of(line))
        .min()
        .unwrap_or(0);
    let out = lines
        .iter()
        .map(|(number, line)| {
            let text = if line.trim().is_empty() {
                String::new()
            } else {
                line[base..].to_owned()
            };
            (*number, text)
        })
        .collect();
    (base, out)
}

/// The rich tag a line opens as a mapping (`chart:`), when it opens one.
fn mapping_key(line: &str) -> Option<&str> {
    let key = line.trim_end().strip_suffix(':')?;
    (!key.is_empty()
        && key.chars().all(|c| c.is_ascii_lowercase() || c == '-')
        && crate::RICH_TAGS.contains(&key))
    .then_some(key)
}

/// A block's YAML, as the slot or slide wrote it, waiting for the report parser.
struct BlockDraft {
    slide: usize,
    slot: String,
    tag: String,
    /// The line of the key that names the block.
    line: usize,
    /// How far the YAML was indented in the deck, to place a finding's column.
    indent: usize,
    yaml: Vec<(usize, String)>,
}

enum Draft {
    Text(Text),
    /// An index into the block drafts.
    Block(usize),
}

struct SlideDraft {
    line: usize,
    end: usize,
    title: Option<Text>,
    subtitle: Option<Text>,
    source: Option<Text>,
    footer: Option<Text>,
    pieces: Vec<(String, Draft)>,
    cols: Option<(usize, String)>,
    rows: Option<(usize, String)>,
}

/// Text from a key's own line and the lines under it, read as Markdown.
fn text_of(entry: &Entry, path: &str, found: &mut Vec<Diagnostic>) -> Option<Text> {
    let mut lines: Vec<(usize, String)> = Vec::new();
    if !entry.inline.is_empty() {
        lines.push((entry.line, entry.inline.clone()));
    }
    let (_, body) = dedent(&trim_blank(entry.body.clone()));
    lines.extend(body);
    if lines.is_empty() {
        return None;
    }
    if lines.iter().any(|(_, line)| {
        let line = line.trim_start();
        line.starts_with("```") || line.starts_with("~~~")
    }) {
        found.push(
            piece_error(
                entry.line,
                "A fenced block cannot sit in text",
                path,
            )
            .fix("Write a chart, metric or table as a mapping under a slot: `chart:` with its fields indented below it."),
        );
        return None;
    }
    Some(Text {
        text: lines
            .iter()
            .map(|(_, line)| line.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        line: lines[0].0,
    })
}

/// A furniture key's text on one line: the lines under the key join to it with spaces.
fn line_of(entry: &Entry) -> Option<Text> {
    let mut words: Vec<&str> = Vec::new();
    if !entry.inline.is_empty() {
        words.push(&entry.inline);
    }
    words.extend(
        entry
            .body
            .iter()
            .map(|(_, line)| line.trim())
            .filter(|line| !line.is_empty()),
    );
    (!words.is_empty()).then(|| Text {
        text: words.join(" "),
        line: entry.line,
    })
}

/// The piece `entry` names, a chart, metric or table written as its own YAML under the key.
fn block_draft(
    entry: &Entry,
    (slide, slot): (usize, &str),
    shift: usize,
    path: &str,
    found: &mut Vec<Diagnostic>,
    blocks: &mut Vec<BlockDraft>,
) -> Option<Draft> {
    if !entry.inline.is_empty() {
        found.push(
            piece_error(
                entry.line,
                format!("`{}` takes its fields on the lines below it", entry.key),
                path,
            )
            .fix(format!(
                "Write `{}:` alone on its line and indent the fields under it.",
                entry.key
            )),
        );
        return None;
    }
    let (base, yaml) = dedent(&trim_blank(entry.body.clone()));
    if yaml.is_empty() {
        found.push(
            piece_error(entry.line, format!("`{}` has no fields", entry.key), path).fix(format!(
                "Indent the fields of the {} under `{}:`.",
                entry.key, entry.key
            )),
        );
        return None;
    }
    blocks.push(BlockDraft {
        slide,
        slot: slot.to_owned(),
        tag: entry.key.clone(),
        line: entry.line,
        indent: shift + base,
        yaml,
    });
    Some(Draft::Block(blocks.len() - 1))
}

/// What a slot holds: text, or a mapping of one chart, metric or table.
fn slot_content(
    entry: &Entry,
    slide: usize,
    path: &str,
    found: &mut Vec<Diagnostic>,
    blocks: &mut Vec<BlockDraft>,
) -> Option<Draft> {
    let (base, plain) = dedent(&trim_blank(entry.body.clone()));
    let first = plain
        .iter()
        .find(|(_, line)| !line.trim().is_empty())
        .and_then(|(_, line)| mapping_key(line));
    if entry.inline.is_empty()
        && let Some(key) = first
    {
        if !BLOCKS.contains(&key) {
            found.push(
                piece_error(
                    entry.line,
                    format!("A slot holds text or one chart, metric or table, not `{key}`"),
                    path,
                )
                .fix("Write the slot's text under its key, or a `chart:`, `metric:` or `table:` mapping."),
            );
            return None;
        }
        let lines: Vec<(usize, &str)> = plain
            .iter()
            .map(|(number, line)| (*number, line.as_str()))
            .collect();
        let inner = entries(&lines, path, found);
        if inner.len() != 1 {
            found.push(
                piece_error(
                    entry.line,
                    format!("A slot holds one piece, and this one holds {}", inner.len()),
                    path,
                )
                .fix("Keep one of them here and give the rest a slot of their own: left and right, or top and bottom."),
            );
            return None;
        }
        let slot = path.rsplit('.').next().unwrap_or(path);
        return block_draft(&inner[0], (slide, slot), base, path, found, blocks);
    }
    text_of(entry, path, found).map(Draft::Text)
}

/// The parts of 24 a `cols` or `rows` value names: whole numbers joined by `-` that add up to 24.
fn parts_of(text: &str) -> Option<Vec<u8>> {
    let parts: Option<Vec<u8>> = text
        .split('-')
        .map(|part| part.trim().parse::<u8>().ok().filter(|n| *n >= 1))
        .collect();
    parts.filter(|parts| parts.len() >= 2 && parts.iter().map(|n| u32::from(*n)).sum::<u32>() == 24)
}

/// Read one slide's lines into a draft, with its findings.
fn draft_slide(
    number: usize,
    line: usize,
    lines: &[(usize, &str)],
    found: &mut Vec<Diagnostic>,
    blocks: &mut Vec<BlockDraft>,
) -> SlideDraft {
    let path = format!("slides[{number}]");
    let mut draft = SlideDraft {
        line,
        end: lines.last().map_or(line, |(last, _)| *last),
        title: None,
        subtitle: None,
        source: None,
        footer: None,
        pieces: Vec::new(),
        cols: None,
        rows: None,
    };
    let mut seen: HashSet<String> = HashSet::new();
    for entry in entries(lines, &path, found) {
        let key = entry.key.as_str();
        let at = format!("{path}.{key}");
        if !seen.insert(entry.key.clone()) {
            found.push(
                syntax(
                    entry.line,
                    format!("`{key}` is written twice on this slide"),
                    &at,
                )
                .fix("Keep one of them."),
            );
            continue;
        }
        match key {
            "title" => draft.title = line_of(&entry),
            "subtitle" => draft.subtitle = line_of(&entry),
            "source" => draft.source = line_of(&entry),
            "footer" => draft.footer = line_of(&entry),
            "cols" => draft.cols = Some((entry.line, entry.inline.clone())),
            "rows" => draft.rows = Some((entry.line, entry.inline.clone())),
            "text" => {
                if let Some(text) = text_of(&entry, &at, found) {
                    draft.pieces.push(("main".into(), Draft::Text(text)));
                }
            }
            "chart" | "metric" | "table" => {
                if let Some(piece) =
                    block_draft(&entry, (number, "main"), 0, &at, found, blocks)
                {
                    draft.pieces.push(("main".into(), piece));
                }
            }
            _ if SLOTS.contains(&key) => {
                if let Some(piece) = slot_content(&entry, number, &at, found, blocks) {
                    draft.pieces.push((key.to_owned(), piece));
                } else if draft.pieces.iter().all(|(slot, _)| slot != key) {
                    // An empty slot is still the slot the author named, so the layout holds.
                    if entry.inline.is_empty() && entry.body.iter().all(|(_, l)| l.trim().is_empty()) {
                        found.push(
                            piece_error(entry.line, format!("The slot `{key}` is empty"), &at)
                                .fix("Put text or a chart, metric or table in it, or remove the key."),
                        );
                    }
                }
            }
            _ if LATER.contains(&key) => found.push(
                Diagnostic::new(
                    "warning",
                    "deck.key",
                    format!("`{key}` is not read yet: speaker notes and reveals are not part of this format's first build"),
                    &at,
                )
                .at(entry.line, "slides"),
            ),
            _ => {
                let known: Vec<&str> = FURNITURE
                    .iter()
                    .chain(["cols", "rows"].iter())
                    .chain(PIECES.iter())
                    .chain(SLOTS.iter())
                    .copied()
                    .collect();
                found.push(
                    Diagnostic::new(
                        "warning",
                        "deck.key",
                        format!("`{key}` is not a key of a slide, so it is ignored"),
                        &at,
                    )
                    .expected(known.join(", "))
                    .at(entry.line, "slides"),
                );
            }
        }
    }
    draft
}

/// The layout that shares most slots with `found`, and has the fewest others: what a finding
/// offers when the slots a slide uses are none of the nine.
fn nearest(found: &[&str]) -> Layout {
    Layout::MANY
        .into_iter()
        .max_by_key(|layout| {
            let slots = layout.slots();
            let shared = slots.iter().filter(|slot| found.contains(slot)).count();
            (shared, std::cmp::Reverse(slots.len()))
        })
        .unwrap_or(Layout::LeftRight)
}

/// A deck as the reader checks it, ready for the block parser: the slides without their blocks'
/// contents, the setup, and every finding of the file's own.
struct Read {
    setup: Option<Value>,
    setup_positions: BTreeMap<String, crate::yaml::Pos>,
    slides: Vec<SlideDraft>,
    blocks: Vec<BlockDraft>,
    found: Vec<Diagnostic>,
}

fn read(source: &str) -> Read {
    let mut read = Read {
        setup: None,
        setup_positions: BTreeMap::new(),
        slides: Vec::new(),
        blocks: Vec::new(),
        found: Vec::new(),
    };
    let numbered: Vec<(usize, &str)> = source
        .split('\n')
        .enumerate()
        .map(|(index, line)| (index + 1, line.strip_suffix('\r').unwrap_or(line)))
        .collect();
    // Part 0 is the setup; each `---` line opens the next part, a slide.
    let mut parts: Vec<(usize, Vec<(usize, &str)>)> = vec![(0, Vec::new())];
    for &(number, line) in &numbered {
        if line.trim_end() == "---" {
            parts.push((number, Vec::new()));
        } else if let Some((_, lines)) = parts.last_mut() {
            lines.push((number, line));
        }
    }
    let setup = parts.remove(0).1;
    let text = setup
        .iter()
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n");
    match crate::yaml::load(&text) {
        Ok(loaded) => match loaded.value {
            Value::Object(map) => {
                read.setup = Some(Value::Object(map));
                read.setup_positions = loaded.positions;
            }
            Value::Null => read.setup = Some(json!({})),
            _ => read.found.push(
                Diagnostic::new(
                    "error",
                    "frontmatter.mapping",
                    "The setup must be a mapping of fields such as `title: ...`",
                    "frontmatter",
                )
                .at(1, "frontmatter"),
            ),
        },
        Err(error) => read.found.push(crate::parser::yaml_diagnostic(
            &error,
            "frontmatter",
            "frontmatter",
            0,
        )),
    }
    for (index, (line, lines)) in parts.iter().enumerate() {
        let draft = draft_slide(index + 1, *line, lines, &mut read.found, &mut read.blocks);
        read.slides.push(draft);
    }
    read
}

/// The fence that holds one block's YAML for the report parser, with the deck line of each of
/// its lines.
fn synthetic(blocks: &[BlockDraft]) -> (String, Vec<(usize, usize)>, Vec<usize>) {
    let mut text = String::new();
    // For each generated line: the deck's line and how far it was indented.
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut fences = Vec::with_capacity(blocks.len());
    for block in blocks {
        let longest = block
            .yaml
            .iter()
            .map(|(_, line)| line.split(|c| c != '`').map(str::len).max().unwrap_or(0))
            .max()
            .unwrap_or(0);
        let fence = "`".repeat((longest + 1).max(3));
        fences.push(lines.len() + 1);
        let _ = writeln!(text, "{fence}{}", block.tag);
        lines.push((block.line, 0));
        for (number, line) in &block.yaml {
            text.push_str(line);
            text.push('\n');
            lines.push((*number, block.indent));
        }
        let last = block.yaml.last().map_or(block.line, |(number, _)| *number);
        let _ = writeln!(text, "{fence}\n");
        lines.push((last, 0));
        lines.push((last, 0));
    }
    (text, lines, fences)
}

/// Read a deck, and give back the document the renderer reads: the setup as frontmatter, the
/// blocks the slides hold, and every finding with its line in the deck.
pub(crate) fn parse(source: &str) -> Result<Document, Vec<Diagnostic>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic::new(
            "error",
            "source.limit",
            "The .rslide source is over the 16 MiB limit. Move large data into CSV or JSON files and split the deck.",
            "source",
        )]);
    }
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let Read {
        setup,
        setup_positions,
        slides,
        blocks: drafts,
        found,
    } = read(source);
    let mut diagnostics = found;
    let mut frontmatter = setup.unwrap_or_else(|| json!({}));
    let positions = setup_positions;

    // The setup: the keys a deck reads, checked as a report's frontmatter is.
    let lines: BTreeMap<String, usize> = positions
        .iter()
        .map(|(key, pos)| (key.clone(), pos.line))
        .collect();
    let columns: BTreeMap<String, usize> = positions
        .iter()
        .map(|(key, pos)| (key.clone(), pos.column))
        .collect();
    if let Some(map) = frontmatter.as_object_mut() {
        let unknown: Vec<String> = map
            .keys()
            .filter(|key| !SETUP_KEYS.contains(&key.as_str()))
            .cloned()
            .collect();
        for key in unknown {
            map.remove(&key);
            let mut finding = Diagnostic::new(
                "warning",
                "deck.key",
                format!("`{key}` is not a key of a deck's setup, so it is ignored"),
                format!("frontmatter.{key}"),
            )
            .expected(SETUP_KEYS.join(", "));
            finding.line = lines.get(&key).copied();
            finding.tag = Some("frontmatter".into());
            diagnostics.push(finding);
        }
        // The marker: this build reads `rslide/1`.
        if let Some(format) = map.remove("format") {
            let text = format.as_str().unwrap_or_default();
            let major = text
                .strip_prefix(FORMAT_NAME)
                .and_then(|rest| rest.strip_prefix('/'))
                .and_then(|rest| rest.split('.').next())
                .and_then(|rest| rest.parse::<u32>().ok());
            if major.is_none_or(|major| major > FORMAT_MAJOR) {
                let mut finding = Diagnostic::new(
                    "error",
                    "frontmatter.format",
                    format!("format '{text}' is not one this renderer reads ({FORMAT_NAME}/{FORMAT_MAJOR})"),
                    "frontmatter.format",
                )
                .expected(format!("{FORMAT_NAME}/{FORMAT_MAJOR}"))
                .fix(format!("Write `format: {FORMAT_NAME}/{FORMAT_MAJOR}` as the first line, or update RichView."));
                finding.line = lines.get("format").copied();
                finding.tag = Some("frontmatter".into());
                diagnostics.push(finding);
            }
        }
    }

    // The blocks: one fence each, read by the report parser, then placed back in the deck.
    let (fenced, map, fences) = synthetic(&drafts);
    let (mut blocks, mut failed, block_findings) = if drafts.is_empty() {
        (Vec::new(), Vec::new(), Vec::new())
    } else {
        let parsed = crate::parser::parse(&fenced)?;
        (parsed.blocks, parsed.failed, parsed.diagnostics)
    };
    let place = |generated: usize| map.get(generated.wrapping_sub(1)).copied();
    // The path of a finding of a block reads from the slide and the slot.
    let ranges: Vec<(usize, usize)> = fences
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = fences.get(index + 1).map_or(map.len() + 1, |next| *next);
            (*start, end)
        })
        .collect();
    for mut finding in block_findings {
        let owner = finding.line.and_then(|line| {
            ranges
                .iter()
                .position(|(start, end)| (*start..*end).contains(&line))
        });
        if let Some(index) = owner {
            let draft = &drafts[index];
            let slot = if draft.slot == "main" {
                String::new()
            } else {
                format!(".{}", draft.slot)
            };
            finding.path = format!("slides[{}]{slot}.{}", draft.slide, finding.path);
        }
        if let Some((line, shift)) = finding.line.and_then(place) {
            finding.line = Some(line);
            finding.column = finding.column.map(|column| column + shift);
        }
        diagnostics.push(finding);
    }
    // Which fence became which block, or failed: by the fence's own line.
    let mut content: Vec<Content> = Vec::with_capacity(drafts.len());
    for fence in &fences {
        let index = blocks
            .iter()
            .position(|block| block.line == *fence)
            .map(Content::Block)
            .or_else(|| {
                failed
                    .iter()
                    .position(|fail| fail.line == *fence)
                    .map(Content::Failed)
            });
        // A fence the report parser left out of both is not rendered; keep the slot empty.
        content.push(index.unwrap_or(Content::Failed(usize::MAX)));
    }
    for block in &mut blocks {
        if let Some((line, _)) = place(block.line) {
            block.line = line;
        }
    }
    for fail in &mut failed {
        if let Some((line, _)) = place(fail.line) {
            fail.line = line;
        }
    }

    // The slides.
    let mut deck = Deck::default();
    for (index, draft) in slides.into_iter().enumerate() {
        let number = index + 1;
        let path = format!("slides[{number}]");
        let mut pieces: Vec<(String, Content)> = Vec::new();
        for (slot, piece) in draft.pieces {
            let content = match piece {
                Draft::Text(text) => Content::Text(text),
                Draft::Block(at) => match content.get(at).cloned() {
                    Some(Content::Failed(usize::MAX)) | None => continue,
                    Some(content) => content,
                },
            };
            pieces.push((slot, content));
        }
        // The layout is the slots the slide uses.
        let names: Vec<&str> = pieces.iter().map(|(slot, _)| slot.as_str()).collect();
        let layout = if pieces.is_empty() {
            Layout::Bare
        } else if names.contains(&"main") {
            if names.len() > 1 {
                diagnostics.push(
                    Diagnostic::new(
                        "error",
                        "deck.layout",
                        "A slide holds either one piece (`chart`, `metric`, `table` or `text`) or the pieces of named slots, not both",
                        path.clone(),
                    )
                    .fix("Keep one piece alone, or move each piece into a slot such as `left` and `right`.")
                    .at(draft.line, "slides"),
                );
                pieces.clear();
                Layout::Bare
            } else {
                Layout::One
            }
        } else if let Some(layout) = Layout::of(&names) {
            layout
        } else {
            let near = nearest(&names);
            let written = names
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ");
            diagnostics.push(
                Diagnostic::new(
                    "error",
                    "deck.layout",
                    format!(
                        "The slots {written} are not a layout; the nearest is {}",
                        near.words()
                    ),
                    path.clone(),
                )
                .fix("Use the slots of one layout: left and right; top and bottom; left, middle and right; top, middle and bottom; top over bottom-left and bottom-right; bottom under top-left and top-right; left beside top-right and bottom-right; or right beside top-left and bottom-left.")
                .at(draft.line, "slides"),
            );
            pieces.clear();
            Layout::Bare
        };
        pieces.sort_by_key(|(slot, _)| {
            layout
                .slots()
                .iter()
                .position(|name| name == slot)
                .unwrap_or(0)
        });
        // `cols` and `rows` cut the layout's tracks.
        let mut span = |given: Option<(usize, String)>, key: &str, count: usize| -> Vec<u8> {
            let Some((line, text)) = given else {
                return Vec::new();
            };
            let at = format!("{path}.{key}");
            let parts = parts_of(&text);
            let cut = if count < 2 {
                Some(format!(
                    "This layout has nothing to cut {}; remove `{key}`",
                    if key == "cols" { "across" } else { "down" }
                ))
            } else {
                match &parts {
                    None => Some(format!(
                        "`{key}` is {count} parts of 24 joined by `-`, such as `{}`, and `{text}` is not",
                        if count == 2 { "9-15" } else { "8-8-8" }
                    )),
                    Some(parts) if parts.len() != count => Some(format!(
                        "`{key}` has {} parts and this layout cuts {count}",
                        parts.len()
                    )),
                    Some(_) => None,
                }
            };
            match cut {
                Some(message) => {
                    diagnostics.push(
                        Diagnostic::new("error", "deck.span", message, at)
                            .expected("whole numbers joined by `-` that add up to 24")
                            .at(line, "slides"),
                    );
                    Vec::new()
                }
                None => parts.unwrap_or_default(),
            }
        };
        let (across, down) = layout.tracks();
        let cols = span(draft.cols, "cols", if across > 1 { across } else { 0 });
        let rows = span(draft.rows, "rows", if down > 1 { down } else { 0 });
        let nothing = draft.title.is_none()
            && draft.subtitle.is_none()
            && draft.source.is_none()
            && draft.footer.is_none()
            && pieces.is_empty();
        if nothing {
            diagnostics.push(
                Diagnostic::new(
                    "error",
                    "deck.empty",
                    format!("Slide {number} has nothing in it"),
                    path.clone(),
                )
                .at(draft.line, "slides"),
            );
        }
        if draft.title.is_none() && draft.subtitle.is_some() {
            diagnostics.push(
                Diagnostic::new(
                    "warning",
                    "deck.key",
                    format!("Slide {number} has a `subtitle` and no `title`"),
                    format!("{path}.subtitle"),
                )
                .fix("Add a `title`, or make the subtitle the title.")
                .at(draft.line, "slides"),
            );
        }
        deck.slides.push(Slide {
            line: draft.line,
            end: draft.end,
            title: draft.title,
            subtitle: draft.subtitle,
            source: draft.source,
            footer: draft.footer,
            layout,
            cols,
            rows,
            pieces: pieces
                .into_iter()
                .map(|(slot, content)| Piece { slot, content })
                .collect(),
        });
    }
    if deck.slides.is_empty() {
        diagnostics.push(
            Diagnostic::new(
                "error",
                "deck.empty",
                "The deck has no slide: each slide opens with a line of `---`",
                "slides",
            )
            .at(1, "slides"),
        );
    }
    // The deck's title is the first slide's when the setup has none, so it names the page and a
    // published link.
    if let Some(map) = frontmatter.as_object_mut()
        && !map.get("title").is_some_and(Value::is_string)
        && let Some(title) = deck.slides.iter().find_map(|slide| slide.title.as_ref())
    {
        map.insert("title".into(), json!(plain(&title.text)));
    }
    // The setup is checked as the frontmatter of a report is: types, then the version, slug and
    // language rules.
    let mut doc = Document {
        frontmatter,
        body: String::new(),
        blocks,
        diagnostics,
        block_placeholder_prefix: crate::parser::unique_placeholder_prefix(source),
        failed,
        body_line: 0,
        frontmatter_lines: BTreeMap::new(),
        frontmatter_columns: BTreeMap::new(),
        deck: Some(deck),
    };
    let first = doc.diagnostics.len();
    let unknown = crate::walker::check(crate::walker::Root::Frontmatter, &mut doc.frontmatter);
    doc.diagnostics.extend(unknown);
    crate::parser::validate_frontmatter(&mut doc);
    doc.frontmatter_lines = lines;
    doc.frontmatter_columns = columns;
    let mut late = doc.diagnostics.split_off(first);
    crate::parser::place_frontmatter(&doc, &mut late);
    doc.diagnostics.extend(late);
    Ok(doc)
}

/// The text of a title without Markdown's marks, for a page title.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '*' | '`' | '\\'))
        .collect::<String>()
        .trim()
        .to_owned()
}

/// What a slide's stylesheet classes are made of.
impl Slide {
    fn has_head(&self) -> bool {
        self.title.is_some() || self.subtitle.is_some()
    }

    fn has_foot(&self) -> bool {
        self.source.is_some() || self.footer.is_some()
    }

    /// The rows of the content region on the stage's 24: it starts under the head and ends over
    /// the foot, and takes their rows when the slide has none.
    fn content_rows(&self) -> usize {
        let first = if self.has_head() { HEAD_ROWS + 1 } else { 1 };
        let last = if self.has_foot() { 24 - FOOT_ROWS } else { 24 };
        last + 1 - first
    }

    /// The parts of 24 each track takes: the author's cut, or equal shares.
    fn parts(&self, cut: &[u8], tracks: usize) -> Vec<u32> {
        if cut.len() == tracks {
            cut.iter().map(|part| u32::from(*part)).collect()
        } else {
            vec![24 / tracks as u32; tracks]
        }
    }

    fn column_parts(&self) -> Vec<u32> {
        self.parts(&self.cols, self.layout.tracks().0)
    }

    fn row_parts(&self) -> Vec<u32> {
        self.parts(&self.rows, self.layout.tracks().1)
    }

    /// A slot's box on the stage as the stylesheet sets it, in CSS pixels at the stage's
    /// reference width: its tracks share the content region, less the gutters between them.
    fn slot_box(&self, slot: &str) -> (f64, f64) {
        let (c0, c1, r0, r1) = self.layout.area(slot);
        let gap = STAGE_WIDTH * GAP;
        let across = |parts: &[u32], from: usize, to: usize, total: f64| {
            let spare = total - gap * (parts.len() as f64 - 1.0);
            let share: u32 = parts[from..to].iter().sum();
            spare * f64::from(share) / 24.0 + gap * (to - from - 1) as f64
        };
        (
            across(
                &self.column_parts(),
                c0,
                c1,
                STAGE_WIDTH * (1.0 - 2.0 * PAD_SIDE),
            ),
            across(&self.row_parts(), r0, r1, ROW * self.content_rows() as f64),
        )
    }

    /// How many of the stage's 24 columns a slot covers.
    fn slot_columns(&self, slot: &str) -> u32 {
        let (c0, c1, _, _) = self.layout.area(slot);
        self.column_parts()[c0..c1].iter().sum()
    }

    fn kind(&self, index: usize, blocks: &[RichBlock]) -> &'static str {
        let tag = |piece: &Piece| match piece.content {
            Content::Block(at) => blocks.get(at).map(|block| block.tag.as_str()),
            _ => None,
        };
        match self.layout {
            Layout::Bare if index == 0 => "cover",
            Layout::Bare => "section",
            Layout::One if self.pieces.first().and_then(tag) == Some("metric") => "hero",
            _ if self
                .pieces
                .iter()
                .all(|piece| matches!(piece.content, Content::Text(_))) =>
            {
                "text"
            }
            _ if self.pieces.iter().all(|piece| tag(piece) == Some("metric")) => "numbers",
            _ => "figures",
        }
    }
}

/// What composing a deck's HTML needs from the render.
pub(crate) struct Compose<'a> {
    pub options: &'a RenderOptions,
    pub lang: &'a str,
    /// The rich-block placeholder prefix; a block is `{prefix}{index}-->`.
    pub prefix: &'a str,
    /// The closing credit, set on the last slide; empty when the style has none.
    pub colophon: &'a str,
}

impl Deck {
    pub(crate) fn len(&self) -> usize {
        self.slides.len()
    }

    /// Each slide's place in the source, in order.
    pub(crate) fn spans(&self) -> Vec<SlideSpan> {
        self.slides
            .iter()
            .enumerate()
            .map(|(index, slide)| SlideSpan {
                number: index + 1,
                first_line: slide.line,
                last_line: slide.end.max(slide.line),
                title: slide.title.as_ref().map(|title| plain(&title.text)),
                layout: slide.layout.name().to_owned(),
            })
            .collect()
    }

    /// Every text of the deck with the line it starts on and the role its face plays, for the
    /// checks that read text: titles are set in the display face, everything else in the text's.
    pub(crate) fn texts(&self) -> Vec<(&str, usize, &'static str)> {
        let mut out = Vec::new();
        for slide in &self.slides {
            let furniture = [
                (&slide.title, "display"),
                (&slide.subtitle, "text"),
                (&slide.source, "text"),
                (&slide.footer, "text"),
            ];
            for (text, role) in furniture {
                if let Some(text) = text {
                    out.push((text.text.as_str(), text.line, role));
                }
            }
            for piece in &slide.pieces {
                if let Content::Text(text) = &piece.content {
                    out.push((text.text.as_str(), text.line, "text"));
                }
            }
        }
        out
    }

    /// The box each chart has on its slide, as `(block index, width, height)` in CSS pixels at the
    /// stage's reference width. A screen's chart takes its real room from the runtime; paper has
    /// no runtime to ask.
    pub(crate) fn chart_boxes(&self, blocks: &[RichBlock]) -> Vec<(usize, f64, f64)> {
        let mut boxes = Vec::new();
        for slide in &self.slides {
            for piece in &slide.pieces {
                if let Content::Block(at) = piece.content
                    && blocks.get(at).is_some_and(|block| block.tag == "chart")
                {
                    let (width, height) = slide.slot_box(&piece.slot);
                    boxes.push((
                        at,
                        width.floor(),
                        (height - CHART_CHROME).max(120.0).floor(),
                    ));
                }
            }
        }
        boxes
    }

    /// The deck as HTML, its blocks still placeholders. `bodies` holds each block's rendered HTML,
    /// which sizes a number and a table to its slot. Findings of the text and of what a slide
    /// holds beyond what its slot shows go to `found`.
    pub(crate) fn html(
        &self,
        compose: &Compose,
        blocks: &[RichBlock],
        bodies: &[String],
        found: &mut Vec<Diagnostic>,
    ) -> String {
        let total = self.slides.len();
        let mut out = String::new();
        // Figures and tables are counted apart, in the order written; a slide holds its own style
        // scope, so Paper's captions read their number from the slot, not from a counter.
        let (mut figures, mut tables) = (0, 0);
        for (index, slide) in self.slides.iter().enumerate() {
            let number = index + 1;
            let path = format!("slides[{number}]");
            let kind = slide.kind(index, blocks);
            let _ = write!(
                out,
                "<section class=\"rv-slide rv-slide--{kind} rv-slide--{}{}{}{}\" data-rv-slide=\"{number}\" aria-roledescription=\"slide\" aria-label=\"Slide {number} of {total}\"><div class=\"rv-slide__stage\"><div class=\"rv-slide__body\">",
                slide.layout.name(),
                if slide.has_head() {
                    ""
                } else {
                    " rv-slide--nohead"
                },
                if slide.has_foot() {
                    ""
                } else {
                    " rv-slide--nofoot"
                },
                if number == total {
                    " rv-slide--last"
                } else {
                    ""
                },
            );
            let mut scope = 0usize;
            let mut inline = |text: &Text, at: &str, found: &mut Vec<Diagnostic>| -> String {
                scope += 1;
                let html = markdown_of(
                    compose,
                    text,
                    &format!("{path}.{at}"),
                    &format!("s{number}t{scope}"),
                    found,
                );
                // One paragraph is the line; anything the Markdown made of it is shown as written.
                let one = html.trim();
                match one
                    .strip_prefix("<p")
                    .and_then(|rest| rest.split_once('>'))
                    .and_then(|(_, rest)| rest.strip_suffix("</p>"))
                {
                    Some(inner) if !inner.contains("<p") => inner.to_owned(),
                    _ => escape(&text.text),
                }
            };
            if slide.has_head() {
                out.push_str("<header class=\"rv-slide__head\">");
                if let Some(title) = &slide.title {
                    let html = inline(title, "title", found);
                    let _ = write!(out, "<h2 class=\"rv-slide__title\">{html}</h2>");
                    let chars = title.text.chars().count();
                    if chars > MOST_TITLE {
                        density(
                            found,
                            (number, "title"),
                            title.line,
                            format!(
                                "has a title of {chars} characters; a title reads up to {MOST_TITLE}"
                            ),
                            (chars, MOST_TITLE),
                        );
                    }
                }
                if let Some(subtitle) = &slide.subtitle {
                    let html = inline(subtitle, "subtitle", found);
                    let _ = write!(out, "<p class=\"rv-slide__subtitle\">{html}</p>");
                }
                out.push_str("</header>");
            }
            if !slide.pieces.is_empty() {
                let style = |parts: &[u32]| {
                    parts
                        .iter()
                        .map(|part| format!("{part}fr"))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                let _ = write!(
                    out,
                    "<div class=\"rv-slide__content\" style=\"--rv-x:{};--rv-y:{}\">",
                    style(&slide.column_parts()),
                    style(&slide.row_parts())
                );
                for (k, piece) in slide.pieces.iter().enumerate() {
                    let (c0, c1, r0, r1) = slide.layout.area(&piece.slot);
                    let place = format!(
                        "--rv-gc:{} / {};--rv-gr:{} / {}",
                        c0 + 1,
                        c1 + 1,
                        r0 + 1,
                        r1 + 1
                    );
                    let (class, extra, inner) = match &piece.content {
                        Content::Text(text) => {
                            let html = markdown_of(
                                compose,
                                text,
                                &format!("{path}.{}", piece.slot),
                                &format!("s{number}p{k}"),
                                found,
                            );
                            let words = strip_tags(&html, " ")
                                .split_whitespace()
                                .filter(|word| word.chars().any(char::is_alphanumeric))
                                .count();
                            let limit = if slide.slot_columns(&piece.slot) < 12 {
                                WORDS_NARROW
                            } else {
                                WORDS_WIDE
                            };
                            if words > limit {
                                density(
                                    found,
                                    (number, &piece.slot),
                                    text.line,
                                    format!(
                                        "holds {words} words in its `{}` slot; a slot this wide reads up to {limit}",
                                        piece.slot
                                    ),
                                    (words, limit),
                                );
                            }
                            ("text rv-slide__text", String::new(), html)
                        }
                        Content::Block(at) => {
                            let tag = blocks.get(*at).map_or("", |block| block.tag.as_str());
                            let body = bodies.get(*at).map_or("", String::as_str);
                            let placeholder = format!("{}{at}-->", compose.prefix);
                            match tag {
                                "metric" => (
                                    "metric rv-slide__number",
                                    format!(";--rv-chars:{:.1}", value_width(body)),
                                    placeholder,
                                ),
                                "table" => {
                                    let (rows, chars) = table_extent(body);
                                    let shown = rows.saturating_sub(1);
                                    if shown > MOST_ROWS {
                                        density(
                                            found,
                                            (number, &piece.slot),
                                            blocks.get(*at).map_or(slide.line, |block| block.line),
                                            format!(
                                                "holds a table of {shown} rows in its `{}` slot; a slide shows up to {MOST_ROWS}",
                                                piece.slot
                                            ),
                                            (shown, MOST_ROWS),
                                        );
                                    }
                                    tables += 1;
                                    (
                                        "table rv-slide__figure rv-slide__figure--table",
                                        format!(
                                            ";--rv-n:{tables};--rv-rows:{rows};--rv-row-chars:{chars}"
                                        ),
                                        placeholder,
                                    )
                                }
                                _ => {
                                    figures += 1;
                                    (
                                        "chart rv-slide__figure rv-slide__figure--chart",
                                        format!(";--rv-n:{figures}"),
                                        placeholder,
                                    )
                                }
                            }
                        }
                        Content::Failed(at) => (
                            "notice",
                            String::new(),
                            format!("{}{at}-->", crate::parser::failed_prefix(compose.prefix)),
                        ),
                    };
                    let _ = write!(
                        out,
                        "<div class=\"rv-slot rv-slot--{} rv-slot--{class}\" data-rv-slot=\"{}\" style=\"{place}{extra}\">{inner}</div>",
                        piece.slot, piece.slot
                    );
                }
                out.push_str("</div>");
            }
            if slide.has_foot() {
                out.push_str("<footer class=\"rv-slide__foot\">");
                if let Some(source) = &slide.source {
                    let html = inline(source, "source", found);
                    let _ = write!(out, "<p class=\"rv-slide__source\">{html}</p>");
                }
                if let Some(footer) = &slide.footer {
                    let html = inline(footer, "footer", found);
                    let _ = write!(out, "<p class=\"rv-slide__note\">{html}</p>");
                }
                out.push_str("</footer>");
            }
            if number == total && !compose.colophon.is_empty() {
                let _ = write!(
                    out,
                    "<div class=\"rv-slide__colophon\">{}</div>",
                    compose.colophon
                );
            }
            out.push_str("</div></div></section>");
        }
        out
    }
}

/// A text as Markdown, with the findings of the report's own Markdown reader at the deck's lines.
fn markdown_of(
    compose: &Compose,
    text: &Text,
    path: &str,
    scope: &str,
    found: &mut Vec<Diagnostic>,
) -> String {
    let context = crate::markdown::Context {
        heading_ids: &[],
        section_markers: false,
        first_line: Some(text.line),
        columns: false,
        path,
        scope,
        placeholder: compose.prefix,
        lang: compose.lang,
    };
    crate::markdown::markdown_html(&text.text, compose.options, &context, found)
}

/// A `slides.density` finding: what a slide holds beyond what its slot shows well.
fn density(
    found: &mut Vec<Diagnostic>,
    (number, slot): (usize, &str),
    line: usize,
    what: String,
    (count, limit): (usize, usize),
) {
    let mut finding = Diagnostic::new(
        "warning",
        "slides.density",
        format!("Slide {number} {what}"),
        format!("slides[{number}].{slot}"),
    );
    finding.line = Some(line);
    finding.tag = Some("slides".into());
    finding.details = Some(json!({"slide": number, "slot": slot, "count": count, "limit": limit}));
    found.push(finding);
}

/// The index just past the tag that starts `html`: its first `>` outside a quoted attribute.
fn tag_end(html: &str) -> usize {
    let mut quote = None;
    for (at, byte) in html.bytes().enumerate() {
        match (quote, byte) {
            (Some(open), _) if byte == open => quote = None,
            (None, b'"' | b'\'') => quote = Some(byte),
            (None, b'>') => return at + 1,
            _ => {}
        }
    }
    html.len()
}

/// The text of `html` without its tags, with `between` in place of each tag: a space where tags
/// stand between words, as a line break or a list item does, and nothing inside a title, whose
/// tags are emphasis.
fn strip_tags(html: &str, between: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(open) = html[at..].find('<') {
        out.push_str(&html[at..at + open]);
        out.push_str(between);
        let rest = &html[at + open..];
        at += open
            + if rest.starts_with("<!--") {
                rest.find("-->").map_or(rest.len(), |end| end + 3)
            } else {
                tag_end(rest)
            };
    }
    out.push_str(&html[at..]);
    out
}

/// A table's rows, its head included, and the characters of its widest row with two for each
/// cell's padding, which the stylesheet sets the table's size from.
fn table_extent(html: &str) -> (usize, usize) {
    let mut rows = 0;
    // Each column is as wide as its widest cell, whichever row holds it.
    let mut columns: Vec<usize> = Vec::new();
    for row in html.split("<tr").skip(1) {
        let row = row.split("</tr>").next().unwrap_or(row);
        rows += 1;
        // A cell runs from its `<td` or `<th` to the next one, tags inside it included.
        let starts: Vec<usize> = row
            .match_indices("<t")
            .map(|(at, _)| at)
            .filter(|&at| {
                let name = &row.as_bytes()[at + 2..];
                matches!(name.first(), Some(b'd' | b'h'))
                    && matches!(name.get(1), Some(b'>' | b' '))
            })
            .collect();
        for (index, &start) in starts.iter().enumerate() {
            let end = starts.get(index + 1).copied().unwrap_or(row.len());
            let width = unescape(&strip_tags(&row[start..end], ""))
                .trim()
                .chars()
                .count();
            if columns.len() <= index {
                columns.push(0);
            }
            columns[index] = columns[index].max(width);
        }
    }
    // A cell's padding, .6em a side on a slide, is two characters of .6em; one more covers a
    // bold head and wide glyphs.
    let widest: usize = columns.iter().map(|width| width + 3).sum();
    (rows.max(1), widest.max(1))
}

/// A chart's palette on a stage: the first colour leads and the others step back to quiet tones.
/// On a light page a quiet tone keeps its own hue; on a dark page tinted with a hue of its own
/// it takes the page's hue, a slate that recedes into the night rather than a muddy version of
/// its colour. Each quiet tone is the one nearest the page that still keeps the palette rule the
/// engine checks (STY-22): the first four at 3:1 on the page, and every pair among them and each
/// pair of neighbours apart by ΔE 15, and by 8 under colour-vision simulation. A colour that no
/// quiet tone keeps the rule for, or that the engine cannot read, stays as it is.
pub(crate) fn quiet_palette(palette: &[String], page: &str) -> Vec<String> {
    use richview_chart::color::{self, Oklch, Vision};
    let (Some(page), Some(Some(lead))) = (
        color::parse(page),
        palette.first().map(|css| color::parse(css)),
    ) else {
        return palette.to_vec();
    };
    // A hair over each threshold, so the tone still passes once written to three decimals.
    let apart = |a: Oklch, b: Oklch| {
        color::delta_e(a, b) >= color::MIN_DELTA_E + 0.005
            && Vision::ALL.into_iter().all(|vision| {
                color::delta_e(vision.simulate(a), vision.simulate(b))
                    >= color::MIN_DELTA_E_CVD + 0.005
            })
    };
    // From the page outwards: the first tone that passes is the quietest.
    let (from, step) = if page.l < 0.5 {
        (page.l + 0.05, 0.0125)
    } else {
        (page.l - 0.05, -0.0125)
    };
    let night = page.l < 0.5 && page.c >= 0.008;
    let mut shown = vec![lead];
    let mut out = vec![palette[0].clone()];
    for (index, css) in palette.iter().enumerate().skip(1) {
        let Some(own) = color::parse(css) else {
            out.push(css.clone());
            shown.push(lead);
            continue;
        };
        let (chroma, hue) = if night {
            ((page.c * 3.0).max(0.03), page.h)
        } else {
            (own.c * 0.22, own.h)
        };
        // Every tone keeps 3:1 and stays apart from the lead, since a report's entity colours
        // can bring a later slot among a chart's first four.
        let keeps = |quiet: Oklch| {
            color::contrast(quiet, page) >= color::MIN_CONTRAST + 0.05
                && shown
                    .iter()
                    .enumerate()
                    .filter(|&(before, _)| index < 4 || before == 0 || before + 1 == index)
                    .all(|(_, other)| apart(quiet, *other))
        };
        let quiet = (0..64)
            .map(|k| Oklch::new((from + step * f64::from(k)).clamp(0.0, 1.0), chroma, hue))
            .find(|quiet| keeps(*quiet));
        match quiet {
            Some(quiet) => {
                out.push(quiet.css());
                shown.push(quiet);
            }
            None => {
                out.push(css.clone());
                shown.push(own);
            }
        }
    }
    out
}

/// Text as the reader sees it, from text the renderer escaped.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// How wide a metric's value sets, in characters of its face: the longer line when a value of
/// several words takes two. Narrow punctuation and spaces count half. The stylesheet divides the
/// column's width by it, so a long value never leaves its column.
fn value_width(metric: &str) -> f64 {
    const OPEN: &str = "rv-metric__value\">";
    let value = metric
        .split_once(OPEN)
        .and_then(|(_, rest)| rest.split_once("</div>"))
        .map_or("", |(value, _)| value);
    let value = unescape(value);
    let width = |text: &str| -> f64 {
        text.chars()
            .map(|c| match c {
                ' ' | '\u{a0}' | '.' | ',' | ':' | ';' | '\'' | '\u{2019}' | '|' | '!' => 0.5,
                _ => 1.0,
            })
            .sum()
    };
    let whole = width(&value);
    // A short value, or one word, stays on one line.
    let breaks: Vec<usize> = value
        .char_indices()
        .filter(|(_, c)| *c == ' ')
        .map(|(at, _)| at)
        .collect();
    if whole <= 8.0 || breaks.is_empty() {
        return whole.max(2.0);
    }
    breaks
        .into_iter()
        .map(|at| width(&value[..at]).max(width(&value[at + 1..])))
        .fold(whole, f64::min)
        .max(2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECK: &str = "format: rslide/1\ntitle: Review\nstyles: [sleek]\n---\ntitle: Cover\nsubtitle: Small\n---\ntitle: Two\nleft:\n  - one\n  - two\nright:\n  metric:\n    value: 5\n    label: Things\ncols: 9-15\nsource: rows\n---\nmetric:\n  value: 1\n  label: One\n";

    fn codes(doc: &Document) -> Vec<&str> {
        doc.diagnostics
            .iter()
            .map(|finding| finding.code.as_str())
            .collect()
    }

    #[test]
    fn a_deck_names_itself_by_its_first_line() {
        assert!(is_deck(DECK));
        assert!(is_deck("\u{feff}# a comment\n\nformat: rslide/1\n"));
        assert!(!is_deck("---\nformat: rmd/1\n---\n"));
        assert!(!is_deck("---\ntitle: A\n---\n"));
        assert!(!is_deck("format: slides\n"));
    }

    #[test]
    fn the_nine_layouts_are_the_slots_a_slide_uses() {
        assert_eq!(Layout::of(&["right", "left"]), Some(Layout::LeftRight));
        assert_eq!(
            Layout::of(&["bottom-right", "top", "bottom-left"]),
            Some(Layout::TopPair)
        );
        assert_eq!(Layout::of(&["left", "bottom"]), None);
        assert_eq!(Layout::of(&["left", "middle"]), None);
        // Every layout covers its slots with the tracks it has, each slot once.
        for layout in Layout::MANY {
            let (across, down) = layout.tracks();
            let mut covered = vec![0; across * down];
            for slot in layout.slots() {
                let (c0, c1, r0, r1) = layout.area(slot);
                for row in r0..r1 {
                    for column in c0..c1 {
                        covered[row * across + column] += 1;
                    }
                }
            }
            assert!(
                covered.iter().all(|n| *n == 1),
                "{layout:?} covers {covered:?}"
            );
        }
    }

    #[test]
    fn spans_are_parts_of_24() {
        assert_eq!(parts_of("9-15"), Some(vec![9, 15]));
        assert_eq!(parts_of("8-8-8"), Some(vec![8, 8, 8]));
        assert_eq!(parts_of("9-14"), None);
        assert_eq!(parts_of("24"), None);
        assert_eq!(parts_of("0-24"), None);
        assert_eq!(parts_of("a-b"), None);
    }

    #[test]
    fn a_deck_reads_into_slides_slots_and_blocks() {
        let doc = parse(DECK).unwrap();
        let deck = doc.deck.as_ref().unwrap();
        assert_eq!(codes(&doc), Vec::<&str>::new());
        assert_eq!(deck.slides.len(), 3);
        assert_eq!(
            deck.slides
                .iter()
                .map(|slide| slide.layout)
                .collect::<Vec<_>>(),
            vec![Layout::Bare, Layout::LeftRight, Layout::One]
        );
        assert_eq!(deck.slides[1].cols, vec![9, 15]);
        assert_eq!(doc.blocks.len(), 2);
        // The setup is the frontmatter; the marker is not.
        assert_eq!(doc.frontmatter["title"], "Review");
        assert_eq!(doc.frontmatter["styles"][0], "sleek");
        assert!(doc.frontmatter.get("format").is_none());
        // A block's line is its key's line in the deck.
        assert_eq!(doc.blocks[0].line, 13);
        assert_eq!(doc.blocks[1].line, 19);
        // Text keeps its Markdown and the line it starts on.
        assert!(matches!(
            &deck.slides[1].pieces[0].content,
            Content::Text(text) if text.text == "- one\n- two" && text.line == 10
        ));
    }

    #[test]
    fn the_title_of_the_first_slide_names_a_deck_without_a_setup_title() {
        let doc = parse("format: rslide/1\n---\ntitle: *First* slide\n").unwrap();
        assert_eq!(doc.frontmatter["title"], "First slide");
    }

    #[test]
    fn slots_that_are_not_a_layout_are_an_error_that_names_the_nearest() {
        let doc = parse("format: rslide/1\n---\ntitle: T\nleft: a\nbottom: b\n").unwrap();
        let finding = doc
            .diagnostics
            .iter()
            .find(|finding| finding.code == "deck.layout")
            .expect("a layout finding");
        assert_eq!(finding.level, "error");
        assert_eq!(finding.path, "slides[1]");
        assert_eq!(finding.line, Some(2));
        assert!(finding.message.contains("`left`") && finding.message.contains("nearest"));
    }

    #[test]
    fn a_span_that_does_not_fit_the_layout_is_named() {
        let doc = parse("format: rslide/1\n---\nleft: a\nright: b\ncols: 9-14\n---\ntop: a\nbottom: b\ncols: 12-12\n").unwrap();
        let spans: Vec<(&str, usize)> = doc
            .diagnostics
            .iter()
            .filter(|finding| finding.code == "deck.span")
            .map(|finding| (finding.path.as_str(), finding.line.unwrap_or(0)))
            .collect();
        assert_eq!(spans, vec![("slides[1].cols", 5), ("slides[2].cols", 9)]);
    }

    #[test]
    fn a_block_that_fails_keeps_its_slot_and_its_findings_at_the_decks_lines() {
        let doc = parse(
            "format: rslide/1\n---\ntitle: T\nleft:\n  chart:\n    type: nonsense\nright: ok\n",
        )
        .unwrap();
        assert_eq!(doc.blocks.len(), 0);
        assert_eq!(doc.failed.len(), 1);
        assert_eq!(doc.failed[0].line, 5);
        let deck = doc.deck.as_ref().unwrap();
        assert_eq!(deck.slides[0].layout, Layout::LeftRight);
        assert!(matches!(
            deck.slides[0].pieces[0].content,
            Content::Failed(0)
        ));
        let finding = doc
            .diagnostics
            .iter()
            .find(|finding| finding.path.starts_with("slides[1].left.chart"))
            .expect("a chart finding with the slot in its path");
        assert!(finding.line.is_some_and(|line| (5..=6).contains(&line)));
    }

    #[test]
    fn mistakes_in_the_file_are_findings_not_failures() {
        let doc = parse("format: rslide/1\nbogus: 1\n---\ntitle: T\nfoo: bar\nnotes: later\ntitle: again\n  tabbed:\nthis is not a key\n").unwrap();
        let found: Vec<(&str, &str)> = doc
            .diagnostics
            .iter()
            .map(|finding| (finding.code.as_str(), finding.level.as_str()))
            .collect();
        assert!(found.contains(&("deck.key", "warning")));
        assert!(found.contains(&("deck.syntax", "error")));
        assert_eq!(
            doc.diagnostics
                .iter()
                .filter(|finding| finding.code == "deck.key")
                .count(),
            3
        );
    }

    #[test]
    fn a_deck_with_no_slide_or_a_newer_format_is_an_error() {
        let none = parse("format: rslide/1\ntitle: T\n").unwrap();
        assert!(
            none.diagnostics
                .iter()
                .any(|finding| finding.code == "deck.empty")
        );
        let newer = parse("format: rslide/9\n---\ntitle: T\n").unwrap();
        assert!(
            newer
                .diagnostics
                .iter()
                .any(|finding| finding.code == "frontmatter.format")
        );
    }

    #[test]
    fn a_slots_box_is_its_tracks_of_the_content_region() {
        let slide = |layout, cols: Vec<u8>, rows: Vec<u8>, head: bool, foot: bool| Slide {
            line: 1,
            end: 1,
            title: head.then(|| Text {
                text: "T".into(),
                line: 2,
            }),
            subtitle: None,
            source: foot.then(|| Text {
                text: "S".into(),
                line: 3,
            }),
            footer: None,
            layout,
            cols,
            rows,
            pieces: vec![],
        };
        let two = slide(Layout::LeftRight, vec![9, 15], vec![], true, true);
        let (left, height) = two.slot_box("left");
        let (right, _) = two.slot_box("right");
        let gap = STAGE_WIDTH * GAP;
        assert!((left + right + gap - STAGE_WIDTH * 0.9).abs() < 0.01);
        assert!((right / left - 15.0 / 9.0).abs() < 0.2);
        // A head and a foot leave 18 rows; none leaves 24.
        assert!((height - 18.0 * ROW).abs() < 0.01);
        assert_eq!(
            slide(Layout::One, vec![], vec![], false, false).content_rows(),
            24
        );
        assert_eq!(
            slide(Layout::One, vec![], vec![], true, false).content_rows(),
            20
        );
        // A slot across both columns is as wide as both and the gutter between them.
        let top = slide(Layout::TopPair, vec![], vec![], true, true);
        assert!((top.slot_box("top").0 - STAGE_WIDTH * 0.9).abs() < 0.01);
        assert_eq!(top.slot_columns("bottom-left"), 12);
    }

    #[test]
    fn an_editor_finds_the_slide_under_a_line() {
        let doc = parse(DECK).unwrap();
        let spans = doc.deck.as_ref().unwrap().spans();
        let places: Vec<(usize, usize, usize)> = spans
            .iter()
            .map(|span| (span.number, span.first_line, span.last_line))
            .collect();
        assert_eq!(places, vec![(1, 4, 6), (2, 7, 17), (3, 18, 22)]);
        assert_eq!(spans[0].title.as_deref(), Some("Cover"));
        assert_eq!(spans[2].title, None);
        assert_eq!(
            spans
                .iter()
                .map(|span| span.layout.as_str())
                .collect::<Vec<_>>(),
            vec!["bare", "left-right", "one"]
        );
    }

    #[test]
    fn a_value_is_sized_by_its_longest_line() {
        let metric = |value: &str| format!("<div class=\"rv-metric__value\">{value}</div>");
        assert_eq!(value_width(&metric("$630K")), 5.0);
        assert!(value_width(&metric("Enterprise Plus")) < 15.0);
    }

    #[test]
    fn a_table_counts_its_rows_and_the_widest_cell_of_each_column() {
        // The table block's own markup: a head row and two body rows. Each column is as wide as its
        // widest cell, whichever row holds it, plus three characters of padding.
        let table = "<table><thead><tr><th>Decade</th><th>Mean</th></tr></thead><tbody><tr><td>1950s</td><td>\u{2212}0.05</td></tr><tr><td>2020\u{2013}25</td><td>1.06</td></tr></tbody></table>";
        let (rows, chars) = table_extent(table);
        assert_eq!(rows, 3, "the head row counts");
        assert_eq!(
            chars,
            (7 + 3) + (5 + 3),
            "7 characters and 5, with their padding"
        );
    }
}
