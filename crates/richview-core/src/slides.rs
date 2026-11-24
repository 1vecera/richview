//! Slides (FEA-RP-65): a second rendition of a Rich Markdown source. `slides: true`
//! cuts the rendered body into screens at its top-level headings and thematic breaks, and places
//! what each screen holds in four regions in a fixed order: the title, a row of numbers (the
//! `metric` blocks), the figures (charts, tables, images) and the text. The author chooses what a
//! slide holds and never where it sits. The stylesheet (`assets/slides.css`) sizes a landscape
//! slide as a 16:9 stage whose type and gaps are shares of its width, and stacks the same regions
//! on a phone; the counts that no screen shows well are reported as `slides.density`.

use crate::{Diagnostic, RichBlock, escape};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use serde_json::{Value, json};
use std::fmt::Write as _;

/// The stylesheet of a slides rendition, added after the report's own.
pub(crate) const CSS: &str = include_str!("../assets/slides.css");

/// The width of the stage a landscape slide is composed on, in CSS pixels. The stylesheet scales
/// the stage to the window, so a chart's reference layout is made for this width.
const STAGE_WIDTH: f64 = 1280.0;
/// The stage's side padding and the gap between its columns, as shares of its width; the
/// stylesheet writes the same shares (`5su` and `3su`).
const PADDING: f64 = 0.05;
const GAP: f64 = 0.03;

/// The least height a chart's box has on the stage, as the stylesheet sets it (`13su`).
const MIN_ROOM: f64 = 166.0;

/// The most numbers one row holds, and the most a slide holds in two rows.
const ROW_NUMBERS: usize = 4;
const MOST_NUMBERS: usize = 8;
/// The most figures a slide shows side by side.
const MOST_FIGURES: usize = 2;
/// The most words of text beside a figure or a number, and on a slide of text alone.
const WORDS_BESIDE: usize = 50;
const WORDS_ALONE: usize = 80;
/// The most rows of a table a slide shows.
const MOST_ROWS: usize = 10;

/// Whether the source asks for the slides rendition.
pub(crate) fn enabled(frontmatter: &Value) -> bool {
    frontmatter.get("slides").and_then(Value::as_bool) == Some(true)
}

/// The rendered body without its column markers (FEA-RP-68). A slide places what it holds in its
/// own regions, so slides lay out no columns: the markers go before the body is cut, and the
/// columns' content reads in the slide's numbers, figures and text like any other.
pub(crate) fn without_columns(html: &str, block_prefix: &str) -> String {
    let prefix = crate::columns::prefix(block_prefix);
    if !html.contains(&prefix) {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len());
    let mut remaining = html;
    while let Some(start) = remaining.find(&prefix) {
        out.push_str(&remaining[..start]);
        let rest = &remaining[start..];
        let Some(end) = rest.find("-->").map(|end| end + 3) else {
            out.push_str(rest);
            return out;
        };
        if crate::columns::mark_of(&rest[..end], &prefix).is_none() {
            out.push_str(&rest[..end]);
        }
        remaining = &rest[end..];
    }
    out.push_str(remaining);
    out
}

/// Elements that never have a closing tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];

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

/// The element name of the tag that starts `tag`, opening or closing.
fn tag_name(tag: &str) -> &str {
    let name = tag.trim_start_matches('<').trim_start_matches('/');
    let end = name
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == ':'))
        .unwrap_or(name.len());
    &name[..end]
}

/// Where the node that opens at `start` ends: a comment at its `-->`, an element after its
/// closing tag. The body is sanitized HTML, so every element that opens also closes.
fn node_end(html: &str, start: usize) -> usize {
    let mut depth = 0usize;
    let mut at = start;
    loop {
        let rest = &html[at..];
        if rest.starts_with("<!--") {
            at += rest.find("-->").map_or(rest.len(), |end| end + 3);
        } else {
            let end = tag_end(rest);
            let tag = &rest[..end];
            if tag.starts_with("</") {
                depth = depth.saturating_sub(1);
            } else if !(tag.ends_with("/>") || VOID.contains(&tag_name(tag))) {
                depth += 1;
            }
            at += end;
        }
        if depth == 0 {
            return at;
        }
        match html[at..].find('<') {
            Some(next) => at += next,
            None => return html.len(),
        }
    }
}

/// The top-level nodes of the rendered body, in order: each element with all it holds, each
/// comment, and any text between them.
fn top_level(html: &str) -> Vec<&str> {
    let mut nodes = Vec::new();
    let mut at = 0;
    while at < html.len() {
        let Some(open) = html[at..].find('<').map(|next| at + next) else {
            if !html[at..].trim().is_empty() {
                nodes.push(&html[at..]);
            }
            break;
        };
        if !html[at..open].trim().is_empty() {
            nodes.push(&html[at..open]);
        }
        let end = node_end(html, open);
        nodes.push(&html[open..end]);
        at = end;
    }
    nodes
}

/// What a top-level node of the body is to a slide.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// A top-level heading: it starts a slide and is its title.
    Heading,
    /// A thematic break: it starts a slide without a title.
    Break,
    /// A `metric` block.
    Number,
    /// A chart, table, image, video, embed or story block, or a Markdown table.
    Figure,
    /// The source note of the block before it.
    Source,
    /// Prose, lists, quotes, callouts, audio and notices.
    Text,
}

/// The index of the block whose placeholder `node` is.
fn placeholder(node: &str, prefix: &str) -> Option<usize> {
    node.trim()
        .strip_prefix(prefix)?
        .strip_suffix("-->")?
        .parse()
        .ok()
}

fn has_class(tag: &str, class: &str) -> bool {
    tag.split_once("class=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .is_some_and(|(classes, _)| classes.split_whitespace().any(|c| c == class))
}

fn classify(node: &str, prefix: &str, blocks: &[RichBlock]) -> (Kind, Option<usize>) {
    if let Some(index) = placeholder(node, prefix) {
        let kind = match blocks.get(index).map(|block| block.tag.as_str()) {
            Some("metric") => Kind::Number,
            Some("chart" | "table" | "image" | "video" | "embed" | "scrolly") => Kind::Figure,
            _ => Kind::Text,
        };
        return (kind, Some(index));
    }
    if !node.starts_with('<') || node.starts_with("<!--") {
        return (Kind::Text, None);
    }
    let tag = &node[..tag_end(node)];
    let kind = match tag_name(tag) {
        // The footnotes' own heading sits inside their section, so it is never top-level.
        "h2" => Kind::Heading,
        "hr" => Kind::Break,
        "p" if has_class(tag, "rv-source") => Kind::Source,
        "table" => Kind::Figure,
        "div" if has_class(tag, "rv-table-scroll") => Kind::Figure,
        _ => Kind::Text,
    };
    (kind, None)
}

/// A number or a figure of a slide, with the source note that follows it.
struct Part<'a> {
    html: &'a str,
    block: Option<usize>,
    source: Option<&'a str>,
    /// A table: Markdown's (no block) or a `table` fence.
    table: bool,
}

#[derive(Default)]
struct Slide<'a> {
    heading: Option<&'a str>,
    numbers: Vec<Part<'a>>,
    figures: Vec<Part<'a>>,
    text: Vec<&'a str>,
}

impl Slide<'_> {
    fn is_empty(&self) -> bool {
        self.heading.is_none()
            && self.numbers.is_empty()
            && self.figures.is_empty()
            && self.text.is_empty()
    }

    /// What the slide is made of, which names its layout.
    fn kind(&self) -> &'static str {
        if !self.figures.is_empty() {
            "figures"
        } else if !self.numbers.is_empty() {
            "numbers"
        } else if !self.text.is_empty() {
            "text"
        } else {
            "section"
        }
    }

    /// One figure with text beside it: the figure takes two thirds of the width. A table alone
    /// keeps the whole width, its text under it, so it can be set large.
    fn split(&self) -> bool {
        self.figures.len() == 1 && !self.text.is_empty() && !self.lone_table()
    }

    /// The slide's only figure is a table.
    fn lone_table(&self) -> bool {
        self.figures.len() == 1 && self.figures[0].table
    }

    /// The slide is about one chart, with no numbers: the chart's title is its claim, and a stage
    /// sets it as the slide's headline, the slide's own title above it as its topic.
    fn claim(&self, blocks: &[RichBlock]) -> bool {
        self.numbers.is_empty()
            && self.figures.len() == 1
            && self.figures[0]
                .block
                .and_then(|index| blocks.get(index))
                .is_some_and(|block| block.tag == "chart")
    }

    /// A row of two to four numbers over one chart, with no text beside it: a stage sets the
    /// numbers as a column on the left, the first leading, and gives the chart the slide's full
    /// height beside them rather than the strip a row leaves.
    fn rail(&self, blocks: &[RichBlock]) -> bool {
        (2..=ROW_NUMBERS).contains(&self.numbers.len())
            && self.figures.len() == 1
            && self.text.is_empty()
            && self.figures[0]
                .block
                .and_then(|index| blocks.get(index))
                .is_some_and(|block| block.tag == "chart")
    }
}

/// The body of a slides rendition, cut into slides.
pub(crate) struct Deck<'a> {
    slides: Vec<Slide<'a>>,
}

impl<'a> Deck<'a> {
    /// Cut the rendered Markdown body, whose blocks are still placeholders that start with
    /// `prefix`, into slides.
    pub(crate) fn cut(body: &'a str, prefix: &str, blocks: &[RichBlock]) -> Self {
        let mut slides: Vec<Slide> = Vec::new();
        let mut slide = Slide::default();
        // Whether the node before this one was a number or a figure, which a source note joins.
        let mut noted: Option<Kind> = None;
        for node in top_level(body) {
            let (kind, block) = classify(node, prefix, blocks);
            let before = noted.take();
            match kind {
                Kind::Heading => {
                    if !slide.is_empty() {
                        slides.push(std::mem::take(&mut slide));
                    }
                    slide.heading = Some(node);
                }
                Kind::Break => {
                    if !slide.is_empty() {
                        slides.push(std::mem::take(&mut slide));
                    }
                }
                Kind::Number | Kind::Figure => {
                    let part = Part {
                        html: node,
                        block,
                        source: None,
                        table: kind == Kind::Figure
                            && block.is_none_or(|index| {
                                blocks.get(index).is_some_and(|block| block.tag == "table")
                            }),
                    };
                    if kind == Kind::Number {
                        slide.numbers.push(part);
                    } else {
                        slide.figures.push(part);
                    }
                    noted = Some(kind);
                }
                Kind::Source => {
                    let part = match before {
                        Some(Kind::Number) => slide.numbers.last_mut(),
                        Some(Kind::Figure) => slide.figures.last_mut(),
                        _ => None,
                    };
                    match part {
                        Some(part) => part.source = Some(node),
                        None => slide.text.push(node),
                    }
                }
                Kind::Text => slide.text.push(node),
            }
        }
        if !slide.is_empty() {
            slides.push(slide);
        }
        Deck { slides }
    }

    /// How many slides the rendition has, the title slide included.
    pub(crate) fn len(&self) -> usize {
        self.slides.len() + 1
    }

    /// The box each chart block has on the stage, as `(block index, width, height)` in CSS
    /// pixels. The width is the stylesheet's; the height is what the slide's title, numbers and
    /// the chart's own title, legend and source note leave, estimated from the stylesheet's sizes.
    /// A screen's chart takes its real room from the runtime; paper has no runtime to ask.
    pub(crate) fn chart_boxes(&self, blocks: &[RichBlock]) -> Vec<(usize, f64, f64)> {
        let unit = STAGE_WIDTH / 100.0;
        let inner = STAGE_WIDTH * (1.0 - 2.0 * PADDING);
        let gap = STAGE_WIDTH * GAP;
        let mut boxes = Vec::new();
        for slide in &self.slides {
            let rail = slide.rail(blocks);
            let width = if slide.split() || rail {
                (inner - gap) * 2.0 / 3.0
            } else {
                let columns = slide.figures.len().clamp(1, MOST_FIGURES) as f64;
                (inner - gap * (columns - 1.0)) / columns
            };
            // In stage units: the padding above and below, the title's line and the numbers' band
            // with the gap under each, three lines of text under two figures, and what a chart
            // sets around its plot.
            let mut taken = 8.0 + 6.0;
            if slide.heading.is_some() {
                taken += 5.5;
            }
            // A claim takes two lines of the headline's size, and the title above it one of text.
            if slide.claim(blocks) {
                taken += 2.5;
            }
            // A row whose first number leads stands a few units taller; a rail stands beside
            // the chart and takes none of its height.
            taken += match slide.numbers.len() {
                _ if rail => 0.0,
                0 => 0.0,
                2..=ROW_NUMBERS => 14.0,
                _ => 11.0,
            };
            if !slide.split() && !slide.text.is_empty() {
                taken += 8.0;
            }
            let rows = slide.figures.len().div_ceil(MOST_FIGURES).max(1) as f64;
            let room = ((STAGE_WIDTH * 9.0 / 16.0 - taken * unit) / rows).max(MIN_ROOM);
            for part in &slide.figures {
                if let Some(index) = part.block
                    && blocks.get(index).is_some_and(|block| block.tag == "chart")
                {
                    boxes.push((index, width.floor(), room.floor()));
                }
            }
        }
        boxes
    }

    /// The slides as HTML, their blocks still placeholders. `bodies` holds each block's rendered
    /// HTML, which sizes a number to its column; `colophon` closes the last slide.
    pub(crate) fn html(&self, blocks: &[RichBlock], bodies: &[String], colophon: &str) -> String {
        let total = self.len();
        let mut out = String::new();
        let (mut figures, mut tables) = (0_usize, 0_usize);
        for (index, slide) in self.slides.iter().enumerate() {
            let number = index + 2;
            let kind = slide.kind();
            let hero = kind == "numbers" && slide.numbers.len() == 1;
            let _ = write!(
                out,
                "<section class=\"rv-slide rv-slide--{kind}{}{}{}{}\" data-rv-slide=\"{number}\" aria-roledescription=\"slide\" aria-label=\"Slide {number} of {total}\"><div class=\"rv-slide__stage\"><div class=\"rv-slide__body\">",
                if hero { " rv-slide--hero" } else { "" },
                if slide.heading.is_none() {
                    " rv-slide--untitled"
                } else {
                    ""
                },
                if slide.claim(blocks) {
                    " rv-slide--claim"
                } else {
                    ""
                },
                if slide.rail(blocks) {
                    " rv-slide--rail"
                } else {
                    ""
                },
            );
            out.push_str(slide.heading.unwrap_or_default());
            if !slide.numbers.is_empty() {
                let count = slide.numbers.len();
                let columns = if count > ROW_NUMBERS {
                    count.div_ceil(2)
                } else {
                    count
                };
                // In one row of several numbers the first leads, as a review's headline figure
                // does, and the others support it; a stage sets it larger.
                let _ = write!(
                    out,
                    "<div class=\"rv-slide__numbers{}\" style=\"--rv-cols:{columns}\">",
                    if (2..=ROW_NUMBERS).contains(&count) {
                        " rv-slide__numbers--lead"
                    } else {
                        ""
                    }
                );
                for part in &slide.numbers {
                    let width = part
                        .block
                        .and_then(|index| bodies.get(index))
                        .map_or(6.0, |body| value_width(body));
                    let _ = write!(
                        out,
                        "<div class=\"rv-slide__number\" style=\"--rv-chars:{width:.1}\">{}{}</div>",
                        part.html,
                        part.source.unwrap_or_default()
                    );
                }
                out.push_str("</div>");
            }
            if !slide.figures.is_empty() || !slide.text.is_empty() {
                let _ = write!(
                    out,
                    "<div class=\"rv-slide__main{}\">",
                    if slide.split() {
                        " rv-slide__main--split"
                    } else if slide.lone_table() {
                        " rv-slide__main--table"
                    } else {
                        ""
                    }
                );
                if !slide.figures.is_empty() {
                    let _ = write!(
                        out,
                        "<div class=\"rv-slide__figures\" style=\"--rv-cols:{}\">",
                        slide.figures.len().clamp(1, MOST_FIGURES)
                    );
                    for part in &slide.figures {
                        let tag = part
                            .block
                            .and_then(|index| blocks.get(index))
                            .map_or("table", |block| block.tag.as_str());
                        // Each figure carries its number in the deck, tables counted apart, for
                        // a stage that labels them as a paper does ("Figure 2", "Table 1").
                        let seen = if tag == "table" {
                            &mut tables
                        } else {
                            &mut figures
                        };
                        *seen += 1;
                        // A Markdown table is here whole; a table block is still its placeholder,
                        // and marks its own total row.
                        let staged = (tag == "table" && part.block.is_none())
                            .then(|| stage_table(part.html));
                        // A table is set as large as its rows and its widest row allow; a chip
                        // takes about three characters more than its change.
                        let size = if tag == "table" {
                            let html = part
                                .block
                                .and_then(|index| bodies.get(index))
                                .map_or(part.html, String::as_str);
                            let (rows, chars) = table_extent(html);
                            let chips = staged.as_ref().map_or(0, |(_, changes)| changes * 3);
                            format!(";--rv-rows:{rows};--rv-row-chars:{}", chars + chips)
                        } else {
                            String::new()
                        };
                        let staged = staged.map(|(html, _)| html);
                        let _ = write!(
                            out,
                            "<div class=\"rv-slide__figure rv-slide__figure--{}\" style=\"--rv-n:{seen}{size}\">{}{}</div>",
                            escape(tag),
                            staged.as_deref().unwrap_or(part.html),
                            part.source.unwrap_or_default()
                        );
                    }
                    out.push_str("</div>");
                }
                if !slide.text.is_empty() {
                    out.push_str("<div class=\"rv-slide__text\">");
                    for node in &slide.text {
                        out.push_str(node);
                    }
                    out.push_str("</div>");
                }
                out.push_str("</div>");
            }
            if index + 1 == self.slides.len() {
                out.push_str(colophon);
            }
            out.push_str("</div></div></section>");
        }
        out
    }

    /// The `slides.density` findings: what a slide holds beyond what one screen shows well. The
    /// lines are those of [`cut_lines`], one for each slide in order.
    pub(crate) fn density(
        &self,
        prefix: &str,
        blocks: &[RichBlock],
        bodies: &[String],
        lines: &[usize],
    ) -> Vec<Diagnostic> {
        let mut found = Vec::new();
        // The closing Notes can add one slide the source has no line for. Any other difference
        // means the two cuts disagree, and a wrong line would mislead more than none.
        let lines = if lines.len() == self.slides.len() || lines.len() + 1 == self.slides.len() {
            lines
        } else {
            &[]
        };
        for (index, slide) in self.slides.iter().enumerate() {
            let number = index + 2;
            let name = match slide
                .heading
                .map(|heading| unescape(&strip_tags(heading, "")))
            {
                Some(title) if !title.trim().is_empty() => {
                    format!("Slide {number} (\"{}\")", title.trim())
                }
                _ => format!("Slide {number}"),
            };
            let body_of = |part: &Part| part.block.and_then(|index| bodies.get(index));
            let mut report = |what: String, count: usize, limit: usize| {
                let mut finding = Diagnostic::new(
                    "warning",
                    "slides.density",
                    format!("{name} holds {what}"),
                    "markdown",
                );
                // The first block's line stands in for a slide whose start was not found.
                let line = lines.get(index).copied().or_else(|| {
                    slide
                        .numbers
                        .iter()
                        .chain(&slide.figures)
                        .filter_map(|part| blocks.get(part.block?))
                        .map(|block| block.line)
                        .min()
                });
                finding.line = line;
                finding.tag = Some("markdown".into());
                finding.details = Some(json!({"slide": number, "count": count, "limit": limit}));
                found.push(finding);
            };
            let numbers = slide.numbers.len();
            if !slide.figures.is_empty() && numbers > ROW_NUMBERS {
                report(
                    format!("{numbers} numbers beside a figure; one row reads up to {ROW_NUMBERS}"),
                    numbers,
                    ROW_NUMBERS,
                );
            } else if numbers > MOST_NUMBERS {
                report(
                    format!("{numbers} numbers; two rows read up to {MOST_NUMBERS}"),
                    numbers,
                    MOST_NUMBERS,
                );
            }
            let figures = slide.figures.len();
            if figures > MOST_FIGURES {
                report(
                    format!("{figures} figures; a slide shows up to {MOST_FIGURES} side by side"),
                    figures,
                    MOST_FIGURES,
                );
            }
            let words: usize = slide
                .text
                .iter()
                .map(|node| words_of(node, prefix, bodies))
                .sum();
            let alone = slide.figures.is_empty() && slide.numbers.is_empty();
            let limit = if alone { WORDS_ALONE } else { WORDS_BESIDE };
            if words > limit {
                report(
                    if alone {
                        format!("{words} words of text; a slide reads up to {limit}")
                    } else {
                        format!(
                            "{words} words of text beside its figures and numbers; the text there reads up to {limit}"
                        )
                    },
                    words,
                    limit,
                );
            }
            for part in &slide.figures {
                let html = body_of(part).map_or(part.html, String::as_str);
                let is_table = part.block.is_none()
                    || part
                        .block
                        .and_then(|index| blocks.get(index))
                        .is_some_and(|block| block.tag == "table");
                // The head row is not one of the table's rows.
                let rows = html.matches("<tr").count().saturating_sub(1);
                if is_table && rows > MOST_ROWS {
                    report(
                        format!("a table of {rows} rows; a slide shows up to {MOST_ROWS}"),
                        rows,
                        MOST_ROWS,
                    );
                }
            }
        }
        found
    }
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

/// The words of a text node. A callout or another block in the text is still a placeholder,
/// whose rendered HTML holds its words.
fn words_of(node: &str, prefix: &str, bodies: &[String]) -> usize {
    let block = placeholder(node, prefix).and_then(|index| bodies.get(index));
    let html = block.map_or(node, String::as_str);
    strip_tags(html, " ")
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
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

/// One cell of a table: its opening tag and what it holds, as byte ranges of the table's HTML.
struct Cell {
    open: std::ops::Range<usize>,
    inner: std::ops::Range<usize>,
}

/// A Markdown table's rows of cells, in order, the head's row first.
fn table_rows(html: &str) -> Vec<Vec<Cell>> {
    let mut rows = Vec::new();
    let mut at = 0;
    while let Some(found) = html[at..].find("<tr") {
        let start = at + found;
        let end = html[start..]
            .find("</tr>")
            .map_or(html.len(), |end| start + end);
        let mut cells = Vec::new();
        let mut cursor = start + 3;
        while let Some(found) = html[cursor..end].find("<t") {
            let open = cursor + found;
            let name = &html.as_bytes()[open + 2..];
            if !(matches!(name.first(), Some(b'd' | b'h'))
                && matches!(name.get(1), Some(b'>' | b' ')))
            {
                cursor = open + 2;
                continue;
            }
            let opened = open + tag_end(&html[open..end]);
            let close = if name[0] == b'd' { "</td>" } else { "</th>" };
            let closed = html[opened..end].find(close).map_or(end, |c| opened + c);
            cells.push(Cell {
                open: open..opened,
                inner: opened..closed,
            });
            cursor = closed;
        }
        rows.push(cells);
        at = end.max(start + 3);
    }
    rows
}

/// A number as a table prints it: `$280K`, `1,250`, `+90.9%`, `−3.1 pp`, `1 200 Kč`. Currency
/// marks and the units K, M, B, % and pp are read; anything else is not a number.
fn amount(text: &str) -> Option<f64> {
    let mut text = text
        .trim()
        .replace(['\u{a0}', '\u{202f}', '\u{2009}', ' '], "");
    for mark in ["Kč", "CZK", "EUR", "USD", "GBP", "$", "€", "£", "¥"] {
        text = text.replace(mark, "");
    }
    let (sign, rest) = match text.chars().next()? {
        '+' => (1.0, &text[1..]),
        '-' => (-1.0, &text[1..]),
        '−' => (-1.0, &text['−'.len_utf8()..]),
        _ => (1.0, text.as_str()),
    };
    let (digits, scale) = [
        ("pp", 1.0),
        ("%", 1.0),
        ("K", 1e3),
        ("k", 1e3),
        ("M", 1e6),
        ("B", 1e9),
    ]
    .iter()
    .find_map(|(unit, scale)| rest.strip_suffix(unit).map(|digits| (digits, *scale)))
    .unwrap_or((rest, 1.0));
    if digits.is_empty()
        || !digits
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return None;
    }
    // The separator that comes last is the decimal point, unless a comma only groups thousands.
    let normal = match (digits.rfind('.'), digits.rfind(',')) {
        (Some(dot), Some(comma)) if comma > dot => digits.replace('.', "").replace(',', "."),
        (_, Some(_)) if digits.split(',').skip(1).all(|group| group.len() == 3) => {
            digits.replace(',', "")
        }
        (None, Some(_)) => digits.replace(',', "."),
        _ => digits.replace(',', ""),
    };
    normal.parse::<f64>().ok().map(|value| sign * value * scale)
}

/// A table as a stage sets it (FEA-RP-65). A column whose every body cell is a signed change
/// (`+50%`, `−3.1 pp`) carries each as a chip that names its direction; the number column just
/// before a closing run of such columns, the latest one the changes compare, is marked to lead;
/// and a last row whose every number is the sum of the rows above it is marked as their total.
/// The text and the order of every cell stay as the author wrote them. Returns the table and how
/// many columns of changes it carries as chips.
fn stage_table(html: &str) -> (String, usize) {
    let rows = table_rows(html);
    let text = |cell: &Cell| {
        unescape(&strip_tags(&html[cell.inner.clone()], ""))
            .trim()
            .to_owned()
    };
    // The head is the row of `th` cells; the body is every other row.
    let body: Vec<&Vec<Cell>> = rows
        .iter()
        .filter(|row| {
            row.iter()
                .any(|cell| html[cell.open.clone()].starts_with("<td"))
        })
        .collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let column = |index: usize| body.iter().filter_map(move |row| row.get(index)).map(text);
    let signed = |cell: &str| {
        cell.starts_with(['+', '-', '−'])
            && cell[cell.chars().next().map_or(0, char::len_utf8)..]
                .starts_with(|c: char| c.is_ascii_digit() || c == ' ' || c == '$')
            && amount(cell).is_some()
    };
    let numeric: Vec<bool> = (0..width)
        .map(|index| {
            index > 0
                && !body.is_empty()
                && column(index).count() == body.len()
                && column(index).all(|cell| amount(&cell).is_some())
        })
        .collect();
    let change: Vec<bool> = (0..width)
        .map(|index| {
            numeric[index] && column(index).all(|cell| signed(&cell) || amount(&cell) == Some(0.0))
        })
        .collect();
    // The closing run of change columns, and the number column just before it.
    let run = (0..width).rev().take_while(|&index| change[index]).count();
    let latest = (run > 0 && width > run + 1)
        .then(|| width - run - 1)
        .filter(|&index| index > 0 && numeric[index] && !change[index]);
    // A total: the last of at least three rows, each of its number columns the sum of those above.
    let total = (body.len() >= 3).then(|| body.len() - 1).filter(|&last| {
        let sums: Vec<bool> = (1..width)
            .filter(|&index| numeric[index] && !change[index])
            .map(|index| {
                let values: Vec<f64> = column(index).filter_map(|cell| amount(&cell)).collect();
                let (above, total) = values.split_at(last);
                let sum: f64 = above.iter().sum();
                (sum - total[0]).abs() <= total[0].abs() * 0.006
            })
            .collect();
        !sums.is_empty() && sums.iter().all(|&sums| sums)
    });
    // Edits, by the byte where each applies, made from the end so earlier ranges stay true.
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            let head = html[cell.open.clone()].starts_with("<th");
            let mut classes = Vec::new();
            if latest == Some(index) {
                classes.push("rv-slide__latest");
            }
            if change[index] && !head {
                classes.push("rv-slide__change");
                let shown = text(cell);
                let way = match amount(&shown) {
                    Some(value) if value > 0.0 => " rv-slide__chip--up",
                    Some(value) if value < 0.0 => " rv-slide__chip--down",
                    _ => "",
                };
                edits.push((
                    cell.inner.clone(),
                    format!(
                        "<span class=\"rv-slide__chip{way}\">{}</span>",
                        &html[cell.inner.clone()]
                    ),
                ));
            }
            if !classes.is_empty() {
                edits.push((
                    cell.open.clone(),
                    with_class(&html[cell.open.clone()], &classes.join(" ")),
                ));
            }
        }
    }
    if let Some(last) = total
        && let Some(cell) = body[last].first()
        && let Some(start) = html[..cell.open.start].rfind("<tr")
    {
        let open = start..start + tag_end(&html[start..]);
        edits.push((open.clone(), with_class(&html[open], "rv-slide__total")));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut out = html.to_owned();
    for (range, with) in edits {
        out.replace_range(range, &with);
    }
    (out, change.iter().filter(|&&change| change).count())
}

/// An opening tag with `class` added to its classes.
fn with_class(tag: &str, class: &str) -> String {
    match tag.find(" class=\"") {
        Some(at) => {
            let at = at + " class=\"".len();
            format!("{}{class} {}", &tag[..at], &tag[at..])
        }
        None => {
            let name = tag.find([' ', '>']).unwrap_or(tag.len());
            format!("{} class=\"{class}\"{}", &tag[..name], &tag[name..])
        }
    }
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

/// The source line where each slide of the body starts, in order: its top-level heading or the
/// thematic break before it, else its first line. The headings that start slides are the ones the
/// renderer writes as `h2`: the body's `#` headings when it has any, else its `##` headings.
pub(crate) fn cut_lines(body: &str, first_line: usize) -> Vec<usize> {
    let top = if Parser::new_ext(body, crate::markdown::FLAGS).any(|event| {
        matches!(
            event,
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            })
        )
    }) {
        HeadingLevel::H1
    } else {
        HeadingLevel::H2
    };
    let line_of = |offset: usize| first_line + body[..offset].matches('\n').count();
    let mut lines = Vec::new();
    // How many container blocks (quotes, lists, notes, tables) enclose the current event.
    let mut depth = 0usize;
    // Whether the slide being read holds anything yet, and where it started.
    let mut open: Option<usize> = None;
    for (event, range) in Parser::new_ext(body, crate::markdown::FLAGS).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) if depth == 0 && level == top => {
                if let Some(start) = open.take() {
                    lines.push(start);
                }
                open = Some(line_of(range.start));
            }
            Event::Rule if depth == 0 => {
                if let Some(start) = open.take() {
                    lines.push(start);
                }
            }
            // A footnote's definition renders in the closing Notes, not where it is written.
            Event::Start(Tag::FootnoteDefinition(_)) => depth += 1,
            Event::Start(
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Table(_) | Tag::DefinitionList,
            ) => {
                if depth == 0 && open.is_none() {
                    open = Some(line_of(range.start));
                }
                depth += 1;
            }
            Event::End(
                TagEnd::BlockQuote(_)
                | TagEnd::List(_)
                | TagEnd::FootnoteDefinition
                | TagEnd::Table
                | TagEnd::DefinitionList,
            ) => depth = depth.saturating_sub(1),
            Event::Start(_) | Event::Html(_) | Event::Text(_) if depth == 0 && open.is_none() => {
                open = Some(line_of(range.start));
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        lines.push(start);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &str = "<!--richview:block:";

    fn block(tag: &str) -> RichBlock {
        RichBlock {
            tag: tag.into(),
            value: Value::Null,
            raw: String::new(),
            line: 1,
        }
    }

    #[test]
    fn top_level_nodes_keep_what_they_hold_and_respect_quoted_brackets() {
        let html = "<h2 id=\"a\">A <em>b</em></h2>\n<p title=\"x > y\">One<br>two</p><!--richview:block:0-->\n<hr><ul><li><p>in</p></li></ul>";
        assert_eq!(
            top_level(html),
            vec![
                "<h2 id=\"a\">A <em>b</em></h2>",
                "<p title=\"x > y\">One<br>two</p>",
                "<!--richview:block:0-->",
                "<hr>",
                "<ul><li><p>in</p></li></ul>",
            ]
        );
    }

    #[test]
    fn a_heading_or_a_break_starts_a_slide_and_content_takes_its_region() {
        let blocks = [block("metric"), block("chart"), block("callout")];
        let body = "<p>Lede.</p><h2 id=\"a\">A</h2><!--richview:block:0--><p>Text.</p><!--richview:block:1--><p class=\"rv-source\">Source: x.</p><hr><!--richview:block:2--><h2 id=\"b\">B</h2>";
        let deck = Deck::cut(body, PREFIX, &blocks);
        let kinds: Vec<_> = deck.slides.iter().map(Slide::kind).collect();
        assert_eq!(kinds, ["text", "figures", "text", "section"]);
        assert_eq!(deck.len(), 5);
        let second = &deck.slides[1];
        assert_eq!(second.numbers.len(), 1);
        assert_eq!(
            second.figures[0].source,
            Some("<p class=\"rv-source\">Source: x.</p>")
        );
        assert!(second.split());
        // One figure beside text is two thirds of the stage's inner width, less the gap.
        // Its room is the stage's 720 less the padding, the title, the numbers and its own chrome.
        assert_eq!(deck.chart_boxes(&blocks), vec![(1, 742.0, 329.0)]);
    }

    #[test]
    fn a_table_alone_keeps_the_slides_width_and_its_text_goes_under_it() {
        let blocks = [block("chart")];
        let body = "<h2 id=\"a\">A</h2><div class=\"rv-table-scroll\"><table><tr><td>1</td></tr></table></div><p>Text.</p><h2 id=\"b\">B</h2><!--richview:block:0--><p>Text.</p>";
        let deck = Deck::cut(body, PREFIX, &blocks);
        assert!(deck.slides[0].lone_table() && !deck.slides[0].split());
        assert!(!deck.slides[1].lone_table() && deck.slides[1].split());
        let html = deck.html(&blocks, &[String::new()], "");
        assert_eq!(html.matches("rv-slide__main--table").count(), 1, "{html}");
        assert_eq!(html.matches("rv-slide__main--split").count(), 1, "{html}");
        // The slide about one chart states a claim; the table's slide does not.
        assert_eq!(html.matches("rv-slide--claim").count(), 1, "{html}");
        assert!(
            html.contains("rv-slide--figures rv-slide--claim\" data-rv-slide=\"3\""),
            "{html}"
        );
    }

    #[test]
    fn a_heading_inside_a_quote_or_the_notes_starts_no_slide() {
        let body = "<blockquote><h2 id=\"q\">Quoted</h2></blockquote><section class=\"rv-notes\"><h2 id=\"rv-notes-title\">Notes</h2><ol></ol></section>";
        let deck = Deck::cut(body, PREFIX, &[]);
        assert_eq!(deck.slides.len(), 1);
        assert_eq!(deck.slides[0].kind(), "text");
    }

    #[test]
    fn the_slides_keep_region_order_and_fit_each_number_by_its_characters() {
        let blocks = [block("metric"), block("metric"), block("chart")];
        let bodies = [
            "<div class=\"rv-metric\"><div class=\"rv-metric__value\">$630 thousand</div></div>"
                .to_owned(),
            "<div class=\"rv-metric\"><div class=\"rv-metric__value\">61.5%</div></div>".to_owned(),
            String::new(),
        ];
        let body = "<h2 id=\"a\">A</h2><p>After.</p><!--richview:block:2--><!--richview:block:0--><!--richview:block:1-->";
        let html = Deck::cut(body, PREFIX, &blocks).html(&blocks, &bodies, "<footer></footer>");
        let order = [
            "<h2 id=\"a\">",
            "rv-slide__numbers",
            "rv-slide__figures",
            "rv-slide__text",
        ]
        .map(|part| html.find(part).unwrap());
        assert!(order.is_sorted(), "{html}");
        // "$630" over "thousand": eight characters; "61.5%" with its narrow point: four and a half.
        assert!(html.contains("--rv-chars:8.0"), "{html}");
        assert!(html.contains("--rv-chars:4.5"), "{html}");
        assert!(html.contains("--rv-cols:2"), "{html}");
        // In a row of two the first number leads; a slide with numbers states no claim.
        assert!(
            html.contains("rv-slide__numbers rv-slide__numbers--lead"),
            "{html}"
        );
        assert!(!html.contains("rv-slide--claim"), "{html}");
        // Text beside the chart keeps the numbers a row.
        assert!(!html.contains("rv-slide--rail"), "{html}");
        assert!(html.contains("data-rv-slide=\"2\""), "{html}");
        assert!(
            html.ends_with("<footer></footer></div></div></section>"),
            "{html}"
        );
    }

    #[test]
    fn numbers_over_one_chart_stand_in_a_rail_and_give_the_chart_the_slides_height() {
        let blocks = [block("metric"), block("metric"), block("chart")];
        let body = "<h2 id=\"a\">A</h2><!--richview:block:0--><!--richview:block:1--><!--richview:block:2--><p class=\"rv-source\">Source: x.</p>";
        let deck = Deck::cut(body, PREFIX, &blocks);
        assert!(deck.slides[0].rail(&blocks) && !deck.slides[0].split());
        let html = deck.html(&blocks, &[String::new(), String::new(), String::new()], "");
        assert!(
            html.contains("rv-slide--figures rv-slide--rail\" data-rv-slide=\"2\""),
            "{html}"
        );
        // Two thirds of the inner width, as beside text; the room is the stage's 720 less the
        // padding, the title and the chart's own chrome, the numbers standing beside it.
        assert_eq!(deck.chart_boxes(&blocks), vec![(2, 742.0, 470.0)]);
    }

    #[test]
    fn density_names_the_slide_its_line_and_what_exceeds() {
        let blocks: Vec<RichBlock> = std::iter::repeat_with(|| block("chart")).take(3).collect();
        let words = "word ".repeat(51);
        let body = format!(
            "<h2 id=\"a\">Too <em>much</em></h2><p>{words}</p><!--richview:block:0--><!--richview:block:1--><!--richview:block:2-->"
        );
        let bodies = vec![String::new(); 3];
        let found = Deck::cut(&body, PREFIX, &blocks).density(PREFIX, &blocks, &bodies, &[12]);
        let messages: Vec<_> = found.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "Slide 2 (\"Too much\") holds 3 figures; a slide shows up to 2 side by side",
                "Slide 2 (\"Too much\") holds 51 words of text beside its figures and numbers; the text there reads up to 50",
            ]
        );
        assert!(
            found
                .iter()
                .all(|d| d.line == Some(12) && d.code == "slides.density")
        );
    }

    #[test]
    fn a_table_counts_its_rows_and_the_widest_cell_of_each_column() {
        let html = "<table><thead><tr><th>Segment</th><th class=\"num\">July</th><th>Since July</th></tr></thead><tbody><tr><td>Mid-market</td><td class=\"num\">$110K</td><td>+9%</td></tr><tr><td>A &amp; B</td><td>1</td><td>a <time>long</time> one</td></tr></tbody></table>";
        // Mid-market, $110K and "a long one" are 25 characters, from three different rows, and
        // three cells pad 9 more.
        assert_eq!(table_extent(html), (3, 34));
        assert_eq!(table_extent("<p>no table</p>"), (1, 1));
    }

    #[test]
    fn a_staged_table_chips_its_changes_leads_with_the_latest_column_and_marks_its_total() {
        let html = "<table><thead><tr><th scope=\"col\">Segment</th><th scope=\"col\" class=\"num\">August</th><th scope=\"col\" class=\"num\">September</th><th scope=\"col\" class=\"num\">Change</th></tr></thead><tbody>\n<tr><td>Enterprise</td><td class=\"num\">$340K</td><td class=\"num\">$420K</td><td class=\"num\">+23.5%</td></tr>\n<tr><td>Mid-market</td><td class=\"num\">$160K</td><td class=\"num\">$210K</td><td class=\"num\">−4%</td></tr>\n<tr><td>Total</td><td class=\"num\">$500K</td><td class=\"num\">$630K</td><td class=\"num\">−0.5 pp</td></tr>\n</tbody></table>";
        let (staged, changes) = stage_table(html);
        assert_eq!(changes, 1);
        assert!(staged.contains("<td class=\"rv-slide__change num\"><span class=\"rv-slide__chip rv-slide__chip--up\">+23.5%</span></td>"), "{staged}");
        assert!(
            staged.contains("<span class=\"rv-slide__chip rv-slide__chip--down\">−0.5 pp</span>"),
            "{staged}"
        );
        assert_eq!(staged.matches("rv-slide__latest").count(), 4, "{staged}");
        assert!(
            staged.contains("<th scope=\"col\" class=\"rv-slide__latest num\">September</th>"),
            "{staged}"
        );
        assert!(
            staged.contains("<tr class=\"rv-slide__total\"><td>Total</td>"),
            "{staged}"
        );
        // What the reader sees is unchanged.
        assert_eq!(strip_tags(&staged, ""), strip_tags(html, ""));
        // A last row that is not the sum of those above, and a column of plain counts, are left as they are.
        let plain = "<table><tbody><tr><td>a</td><td>1</td></tr><tr><td>b</td><td>2</td></tr><tr><td>c</td><td>4</td></tr></tbody></table>";
        assert_eq!(stage_table(plain), (plain.to_owned(), 0));
    }

    #[test]
    fn amounts_read_as_tables_print_them() {
        assert_eq!(amount("$280K"), Some(280_000.0));
        assert_eq!(amount("+90.9%"), Some(90.9));
        assert_eq!(amount("−3.1 pp"), Some(-3.1));
        assert_eq!(amount("1,250"), Some(1250.0));
        assert_eq!(amount("1 200 Kč"), Some(1200.0));
        assert_eq!(amount("3,5"), Some(3.5));
        assert_eq!(amount("Total"), None);
        assert_eq!(amount("Q3"), None);
    }

    #[test]
    fn a_stage_palette_keeps_the_first_colour_and_quiets_the_rest_within_the_palette_rule() {
        use richview_chart::color::{self, Vision};
        // Each builtin's palette on its own page; Air shares Sleek's palette on white, and the
        // sketchbook leads with Hand-drawn's red pencil ahead of its greys.
        for (name, page, lead) in [
            ("sleek", "oklch(0.16 0.015 260)", None),
            ("sleek", "oklch(1 0 0)", None),
            ("brutalist", "oklch(0.985 0.004 70)", None),
            ("scientific", "oklch(0.99 0.004 70)", None),
            ("hand-drawn", "oklch(0.982 0.007 85)", None),
            (
                "hand-drawn",
                "oklch(0.982 0.007 85)",
                Some("oklch(0.52 0.174 31)"),
            ),
        ] {
            let palette: Vec<String> = lead
                .into_iter()
                .chain(richview_chart::rmd_render::palette(name).iter().copied())
                .map(str::to_owned)
                .collect();
            let quiet = quiet_palette(&palette, page);
            assert_eq!(quiet.len(), palette.len());
            assert_eq!(quiet[0], palette[0], "{name}: the first colour leads");
            let page = color::parse(page).unwrap();
            let shown: Vec<_> = quiet.iter().map(|c| color::parse(c).unwrap()).collect();
            let own: Vec<_> = palette.iter().map(|c| color::parse(c).unwrap()).collect();
            for (k, c) in shown.iter().enumerate().skip(1) {
                // On the night stage a quiet tone is a slate of the page's hue; elsewhere a tone
                // of its own.
                if page.l < 0.5 {
                    assert!(
                        (c.h - page.h).abs() < 1.0 && c.c <= 0.046,
                        "{name} slot {}: a slate of the night, {c:?}",
                        k + 1
                    );
                } else {
                    assert!(
                        c.c <= own[k].c * 0.22 + 0.001,
                        "{name} slot {}: quiet, {c:?}",
                        k + 1
                    );
                }
            }
            // The engine's rule (STY-22), so a stage never turns its patterns on or warns.
            for (k, c) in shown.iter().take(4).enumerate() {
                assert!(
                    color::contrast(*c, page) >= color::MIN_CONTRAST,
                    "{name} slot {}: 3:1",
                    k + 1
                );
            }
            for i in 0..shown.len() {
                for j in i + 1..shown.len() {
                    if j < 4 || j == i + 1 {
                        let (a, b) = (shown[i], shown[j]);
                        assert!(
                            color::delta_e(a, b) >= color::MIN_DELTA_E,
                            "{name}: slots {} and {} apart",
                            i + 1,
                            j + 1
                        );
                        for vision in Vision::ALL {
                            assert!(
                                color::delta_e(vision.simulate(a), vision.simulate(b))
                                    >= color::MIN_DELTA_E_CVD,
                                "{name}: slots {} and {} apart under {}",
                                i + 1,
                                j + 1,
                                vision.name()
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cut_lines_follow_headings_and_breaks_at_the_top_level() {
        let body = "Lede.\n\n# One\n\ntext\n\n> # quoted\n\n---\n\nalone\n\n# Two\n";
        assert_eq!(cut_lines(body, 10), vec![10, 12, 20, 22]);
        // Without `#`, the `##` headings start the slides.
        assert_eq!(cut_lines("## A\n\n### sub\n\n## B\n", 1), vec![1, 5]);
    }
}
