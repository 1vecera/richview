//! Columns (FEA-RP-68): two to four columns side by side, written as fenced divs.
//!
//! ```text
//! ::: columns {widths: 2-1}
//! ::: column
//! Prose and rich fences of the first column.
//! :::
//! ::: column
//! The second column.
//! :::
//! :::
//! ```
//!
//! The parser finds the marker lines after the rich fences became placeholders, so a marker
//! inside a fence is never one. A marker counts only at the top level of the body, outside lists,
//! quotes and code. A well-formed group's marker lines become generated placeholders on the same
//! lines; any problem leaves every line of the group as it was, so the content renders in one
//! column exactly as without the markers, and warns. Other fenced divs (`::: note`) are left as
//! text without a warning, as before.
//!
//! The opener's placeholder carries the checked options and the width under which the columns
//! stack, so the renderer and a worker document need nothing else.

use crate::{Diagnostic, Document};
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use serde_json::Value;

/// The fewest and most columns of one group.
const MIN_COLUMNS: usize = 2;
const MAX_COLUMNS: usize = 4;
/// The narrowest a column with a chart, table, frame, video or code may be before the columns stack,
/// in rem: a phone's column (358 px), the narrowest width a chart is designed for.
const WIDE_FLOOR_REM: f64 = 358.0 / 16.0;
/// The narrowest a column of prose, metrics, callouts or images may be, in rem.
const TEXT_FLOOR_REM: f64 = 11.0;
/// The rich tags that need a chart's width; code scrolls sideways below it.
const WIDE_TAGS: &[&str] = &["chart", "table", "embed", "video", "code"];
/// The rich tags a column cannot hold: they need the page's width.
const PAGE_TAGS: &[&str] = &["scrolly"];

/// The placeholder prefix of the column markers, beside the rich blocks' prefix.
pub(crate) fn prefix(block_prefix: &str) -> String {
    block_prefix.replacen("block:", "columns:", 1)
}

/// One marker line.
#[derive(Debug, Clone, PartialEq)]
enum Marker {
    /// `::: columns`, with the text of its options.
    Columns(String),
    /// `::: column`.
    Column,
    /// Another fenced div, such as `::: note`: text, as before.
    Other,
    /// `:::`.
    Close,
}

fn marker(line: &str) -> Option<Marker> {
    let line = line.trim_end_matches(['\r', '\n']);
    let indent = line.len() - line.trim_start_matches(' ').len();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let colons = rest.len() - rest.trim_start_matches(':').len();
    if colons < 3 {
        return None;
    }
    let rest = rest[colons..].trim();
    if rest.is_empty() {
        return Some(Marker::Close);
    }
    if rest == "column" {
        return Some(Marker::Column);
    }
    if let Some(options) = rest.strip_prefix("columns")
        && (options.is_empty() || options.starts_with([' ', '\t', '{']))
    {
        return Some(Marker::Columns(options.trim().to_owned()));
    }
    // `:::text:::` and similar are prose, not a div.
    if rest.contains(":::") {
        return None;
    }
    Some(Marker::Other)
}

/// The checked options of one group.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Options {
    /// One weight per column.
    pub weights: Vec<u8>,
    pub gap: &'static str,
    pub align: &'static str,
    pub wide: bool,
    /// The width under which the columns stack, in hundredths of a rem, before the gaps.
    pub stack_at: u32,
}

impl Options {
    /// The opener's placeholder text after the prefix.
    fn encode(&self) -> String {
        let weights: Vec<String> = self.weights.iter().map(u8::to_string).collect();
        format!(
            "o:{}:{}:{}:{}:{}",
            weights.join("-"),
            self.gap,
            self.align,
            if self.wide { "wide" } else { "measure" },
            self.stack_at
        )
    }

    fn decode(text: &str) -> Option<Self> {
        let mut parts = text.strip_prefix("o:")?.split(':');
        let weights: Vec<u8> = parts
            .next()?
            .split('-')
            .map(|w| match w {
                "1" => Some(1),
                "2" => Some(2),
                "3" => Some(3),
                _ => None,
            })
            .collect::<Option<_>>()?;
        if !(MIN_COLUMNS..=MAX_COLUMNS).contains(&weights.len()) {
            return None;
        }
        let gap = parts.next()?;
        let gap = ["small", "medium", "large"]
            .into_iter()
            .find(|g| *g == gap)?;
        let align = parts.next()?;
        let align = ["top", "center", "bottom"]
            .into_iter()
            .find(|a| *a == align)?;
        let wide = match parts.next()? {
            "wide" => true,
            "measure" => false,
            _ => return None,
        };
        let stack = parts.next()?;
        if stack.is_empty() || stack.len() > 6 || !stack.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let stack_at = stack.parse().ok()?;
        parts.next().is_none().then_some(Self {
            weights,
            gap,
            align,
            wide,
            stack_at,
        })
    }

    /// The gap in lines of body text.
    pub fn gap_lines(&self) -> f64 {
        match self.gap {
            "small" => 0.5,
            "large" => 1.5,
            _ => 1.0,
        }
    }

    /// The width under which the columns stack, in px, for a body line of `line` px.
    pub fn stack_px(&self, line: f64) -> f64 {
        f64::from(self.stack_at) / 100.0 * 16.0
            + (self.weights.len() - 1) as f64 * self.gap_lines() * line
    }

    /// The width of column `index` in a container `container` px wide, for a body line of
    /// `line` px: its share when the columns stand side by side, the container's when they stack.
    pub fn column_px(&self, index: usize, container: f64, line: f64) -> f64 {
        if container < self.stack_px(line) {
            return container;
        }
        let total: f64 = self.weights.iter().map(|w| f64::from(*w)).sum();
        let gaps = (self.weights.len() - 1) as f64 * self.gap_lines() * line;
        (container - gaps) * f64::from(self.weights[index]) / total
    }
}

/// What a placeholder in the body stands for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Mark {
    Open(Options),
    Start,
    End,
    Close,
}

/// The mark `html` is, when it is exactly one column placeholder of `prefix`.
pub(crate) fn mark_of(html: &str, prefix: &str) -> Option<Mark> {
    let rest = html.trim().strip_prefix(prefix)?.strip_suffix("-->")?;
    match rest {
        "s" => Some(Mark::Start),
        "e" => Some(Mark::End),
        "c" => Some(Mark::Close),
        _ => Options::decode(rest).map(Mark::Open),
    }
}

/// One group of columns while the body is read.
#[derive(Default)]
struct Group {
    open: usize,
    options: String,
    close: Option<usize>,
    /// Each column's marker lines and whether it holds anything.
    columns: Vec<Column>,
    /// The first problem: its code, body line and what it is.
    error: Option<(&'static str, usize, String)>,
    /// The rich blocks in each column, by tag.
    tags: Vec<Vec<String>>,
}

struct Column {
    open: usize,
    close: Option<usize>,
    filled: bool,
}

enum Frame {
    Columns(usize),
    Column(usize),
    Other,
}

impl Group {
    fn fail(&mut self, code: &'static str, line: usize, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some((code, line, message.into()));
        }
    }
}

/// Find the column markers of `doc.body`, check each group and write the well-formed ones as
/// placeholders. A heading a tab panel starts at cannot sit in a column.
pub(crate) fn mark(doc: &mut Document) {
    if !doc.body.contains(":::") {
        return;
    }
    let block_prefix = doc
        .block_placeholder_prefix
        .clone()
        .unwrap_or_else(|| crate::parser::BLOCK_PLACEHOLDER_PREFIX.to_owned());
    let tab_anchors: Vec<String> = doc
        .frontmatter
        .pointer("/tabs/panels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|panel| panel.get("from").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let body = doc.body.clone();
    let lines: Vec<&str> = body.split_inclusive('\n').collect();
    let mut starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in &lines {
        starts.push(offset);
        offset += line.len();
    }
    let line_at = |offset: usize| starts.partition_point(|&s| s <= offset).saturating_sub(1);
    // Which lines are markers at the top level, and the section headings and tab starts, by
    // line. The scan reads a copy in which each candidate line is a thematic break: a break, like
    // the placeholder comment that replaces a marker, interrupts a paragraph, so a closing `:::`
    // under a quote or a list ends it instead of continuing it lazily, and it stays inside code
    // and HTML blocks. (In the body itself a `:::` line after a paragraph is a definition.)
    let candidates: Vec<bool> = lines.iter().map(|line| marker(line).is_some()).collect();
    let mut scan = String::with_capacity(body.len());
    for (line, candidate) in lines.iter().zip(&candidates) {
        if *candidate {
            let indent = line.len() - line.trim_start_matches(' ').len();
            scan.push_str(&line[..indent]);
            scan.push_str("***");
            scan.push_str(
                &" ".repeat(
                    line.trim_end_matches(['\r', '\n'])
                        .len()
                        .saturating_sub(indent + 3),
                ),
            );
            scan.push_str(&line[line.trim_end_matches(['\r', '\n']).len()..]);
        } else {
            scan.push_str(line);
        }
    }
    let mut top = vec![false; lines.len()];
    let mut headings: Vec<(usize, String)> = vec![];
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(&scan, crate::markdown::FLAGS).into_offset_iter() {
        match event {
            Event::Rule if depth == 0 => {
                let line = line_at(range.start);
                top[line] = candidates[line];
            }
            Event::Start(tag) => {
                if depth == 0
                    && let Tag::Heading { level, id, .. } = &tag
                {
                    let line = line_at(range.start);
                    let anchor = id.as_ref().map(|id| id.to_string());
                    if matches!(level, HeadingLevel::H1 | HeadingLevel::H2) {
                        headings.push((line, "a level 1 or 2 heading".to_owned()));
                    } else if let Some(anchor) =
                        anchor.filter(|anchor| tab_anchors.contains(anchor))
                    {
                        headings.push((line, format!("the start of the tab `{anchor}`")));
                    }
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let block_at = |line: &str| -> Option<String> {
        let rest = line.trim().strip_prefix(block_prefix.as_str())?;
        let index: usize = rest.strip_suffix("-->")?.parse().ok()?;
        doc.blocks.get(index).map(|block| block.tag.clone())
    };
    let source_line = |index: usize| doc.body_line + index;

    let mut groups: Vec<Group> = vec![];
    let mut stack: Vec<Frame> = vec![];
    let mut diagnostics = vec![];
    // The innermost group a line is in.
    let innermost = |stack: &[Frame]| {
        stack.iter().rev().find_map(|frame| match frame {
            Frame::Columns(g) | Frame::Column(g) => Some(*g),
            Frame::Other => None,
        })
    };
    for (i, line) in lines.iter().enumerate() {
        let found = top[i].then(|| marker(line)).flatten();
        match found {
            Some(Marker::Columns(options)) => {
                if let Some(outer) = innermost(&stack) {
                    let message = "columns cannot sit inside other columns";
                    groups[outer].fail("columns.nested", i, message);
                }
                let nested = innermost(&stack).is_some();
                groups.push(Group {
                    open: i,
                    options,
                    ..Group::default()
                });
                let g = groups.len() - 1;
                if nested {
                    groups[g].fail(
                        "columns.nested",
                        i,
                        "columns cannot sit inside other columns",
                    );
                }
                stack.push(Frame::Columns(g));
            }
            Some(Marker::Column) => match stack.last() {
                Some(Frame::Columns(g)) => {
                    let g = *g;
                    groups[g].columns.push(Column {
                        open: i,
                        close: None,
                        filled: false,
                    });
                    groups[g].tags.push(vec![]);
                    stack.push(Frame::Column(g));
                }
                _ => {
                    match innermost(&stack) {
                        Some(g) => groups[g].fail(
                            "columns.stray",
                            i,
                            "a `::: column` sits inside a column instead of directly in `::: columns`",
                        ),
                        None => diagnostics.push(
                            Diagnostic::new(
                                "warning",
                                "columns.stray",
                                "This `::: column` has no `::: columns` around it, so it stays text",
                                "columns",
                            )
                            .at(source_line(i), "columns"),
                        ),
                    }
                    stack.push(Frame::Other);
                }
            },
            Some(Marker::Other) => {
                if let Some(Frame::Columns(g)) = stack.last() {
                    let g = *g;
                    groups[g].fail(
                        "columns.stray",
                        i,
                        "a fenced div sits between columns instead of inside one",
                    );
                } else if let Some(Frame::Column(g)) = stack.last() {
                    let g = *g;
                    if let Some(column) = groups[g].columns.last_mut() {
                        column.filled = true;
                    }
                }
                stack.push(Frame::Other);
            }
            Some(Marker::Close) => match stack.pop() {
                Some(Frame::Column(g)) => {
                    if let Some(column) = groups[g].columns.last_mut() {
                        column.close = Some(i);
                    }
                }
                Some(Frame::Columns(g)) => groups[g].close = Some(i),
                Some(Frame::Other) | None => {}
            },
            None => {
                if line.trim().is_empty() {
                    continue;
                }
                match stack.last() {
                    Some(Frame::Columns(g)) => {
                        let g = *g;
                        groups[g].fail(
                            "columns.stray",
                            i,
                            "text sits between the columns instead of inside a `::: column`",
                        );
                    }
                    Some(Frame::Column(g)) => {
                        let g = *g;
                        if let Some(column) = groups[g].columns.last_mut() {
                            column.filled = true;
                        }
                        if let Some(tag) = block_at(line)
                            && let Some(tags) = groups[g].tags.last_mut()
                        {
                            tags.push(tag);
                        }
                    }
                    _ => {}
                }
                if let Some(g) = innermost(&stack)
                    && let Some(tag) =
                        block_at(line).filter(|tag| PAGE_TAGS.contains(&tag.as_str()))
                {
                    groups[g].fail(
                        "columns.block",
                        i,
                        format!(
                            "a `{tag}` block needs the page's width and cannot sit in a column"
                        ),
                    );
                }
            }
        }
    }
    // Whatever is still open was never closed.
    for frame in stack {
        if let Frame::Columns(g) | Frame::Column(g) = frame {
            let open = groups[g].open;
            groups[g].fail(
                "columns.unclosed",
                open,
                "these columns have no closing `:::`",
            );
        }
    }
    let column_prefix = prefix(&block_prefix);
    let mut rewritten: Vec<Option<String>> = vec![None; lines.len()];
    for group in &mut groups {
        let Some(close) = group.close else {
            continue;
        };
        if let Some((line, what)) = headings
            .iter()
            .find(|(line, _)| (group.open..close).contains(line))
        {
            group.fail("columns.heading", *line, format!("a column holds {what}"));
        }
        let count = group.columns.len();
        if group.error.is_none() && !(MIN_COLUMNS..=MAX_COLUMNS).contains(&count) {
            group.fail(
                "columns.count",
                group.open,
                format!("columns hold two to four columns, and these have {count}"),
            );
        }
        if group.error.is_none()
            && let Some(column) = group.columns.iter().find(|c| !c.filled)
        {
            group.fail("columns.empty", column.open, "this column holds nothing");
        }
        if group.error.is_none() {
            let (options, mut found) = options(&group.options, count, source_line(group.open));
            let refused = found.iter().any(|d| d.level == "error");
            diagnostics.append(&mut found);
            match options {
                Some(mut options) if !refused => {
                    options.stack_at = stack_at(&options.weights, &group.tags);
                    rewritten[group.open] = Some(format!("{column_prefix}{}-->", options.encode()));
                    for column in &group.columns {
                        rewritten[column.open] = Some(format!("{column_prefix}s-->"));
                        if let Some(close) = column.close {
                            rewritten[close] = Some(format!("{column_prefix}e-->"));
                        }
                    }
                    rewritten[close] = Some(format!("{column_prefix}c-->"));
                }
                _ => {}
            }
        }
    }
    for group in &groups {
        if let Some((code, line, message)) = &group.error {
            diagnostics.push(
                Diagnostic::new(
                    "warning",
                    code,
                    format!(
                        "These columns stay text and their content renders in one column: {message}"
                    ),
                    "columns",
                )
                .at(source_line(*line), "columns"),
            );
        }
    }
    diagnostics.sort_by_key(|d| d.line);
    doc.diagnostics.extend(diagnostics);
    if rewritten.iter().all(Option::is_none) {
        return;
    }
    let mut out = String::with_capacity(body.len() + 64);
    for (line, replacement) in lines.iter().zip(rewritten) {
        match replacement {
            // A marker line becomes its placeholder, alone between blank lines' worth of
            // separation, on the same line so every later line keeps its source line.
            Some(placeholder) => {
                out.push_str(&placeholder);
                out.push('\n');
            }
            None => out.push_str(line),
        }
    }
    doc.body = out;
}

/// The width under which the columns stack, in hundredths of a rem before the gaps: where the
/// narrowest column, by its share, would fall below its floor.
fn stack_at(weights: &[u8], tags: &[Vec<String>]) -> u32 {
    let total: f64 = weights.iter().map(|w| f64::from(*w)).sum();
    let need = weights
        .iter()
        .zip(tags)
        .map(|(weight, tags)| {
            let floor = if tags.iter().any(|t| WIDE_TAGS.contains(&t.as_str())) {
                WIDE_FLOOR_REM
            } else {
                TEXT_FLOOR_REM
            };
            floor * total / f64::from(*weight)
        })
        .fold(0.0, f64::max);
    (need * 100.0).ceil() as u32
}

/// The opener's options, checked by the registry's walker, and their findings. `None` when the
/// walk refused a value: the group then stays text, as a refused block stays source.
fn options(text: &str, count: usize, line: usize) -> (Option<Options>, Vec<Diagnostic>) {
    let mut found = vec![];
    let mut value = if text.is_empty() {
        Value::Object(Default::default())
    } else {
        match crate::yaml::load(text) {
            Ok(loaded) if loaded.value.is_object() => loaded.value,
            _ => {
                found.push(
                    Diagnostic::new(
                        "warning",
                        "columns.options",
                        "The options after `::: columns` are not a mapping such as `{widths: 2-1}`, so the columns take their defaults",
                        "columns",
                    )
                    .at(line, "columns"),
                );
                Value::Object(Default::default())
            }
        }
    };
    let mut issues = crate::walker::check(crate::walker::Root::Block("columns"), &mut value);
    for issue in &mut issues {
        issue.line = Some(line);
        issue.tag = Some("columns".into());
    }
    let refused = issues.iter().any(|d| d.level == "error");
    found.extend(issues);
    if refused {
        return (None, found);
    }
    let word = |key: &str, default: &'static str, words: &[&'static str]| -> &'static str {
        value
            .get(key)
            .and_then(Value::as_str)
            .and_then(|v| words.iter().copied().find(|w| *w == v))
            .unwrap_or(default)
    };
    let gap = word("gap", "medium", &["small", "medium", "large"]);
    let align = word("align", "top", &["top", "center", "bottom"]);
    let wide = value.get("wide").and_then(Value::as_bool).unwrap_or(true);
    let widths = word("widths", "equal", crate::registry::COLUMN_WIDTHS);
    let mut weights: Vec<u8> = if widths == "equal" {
        vec![1; count]
    } else {
        widths.split('-').map(|w| w.parse().unwrap_or(1)).collect()
    };
    if weights.len() != count {
        found.push(
            Diagnostic::new(
                "warning",
                "columns.widths",
                format!(
                    "`widths: {widths}` names {} columns and there are {count}, so the columns are equal",
                    weights.len()
                ),
                "columns.widths",
            )
            .at(line, "columns"),
        );
        weights = vec![1; count];
    }
    (
        Some(Options {
            weights,
            gap,
            align,
            wide,
            stack_at: 0,
        }),
        found,
    )
}

/// Where a rich block sits: its group's options and its column.
#[derive(Debug, Clone)]
pub(crate) struct Placement {
    pub options: Options,
    pub column: usize,
}

/// The placement of each rich block of `body`, by block index.
pub(crate) fn placements(body: &str, block_prefix: &str, blocks: usize) -> Vec<Option<Placement>> {
    let mut out = vec![None; blocks];
    if blocks == 0 {
        return out;
    }
    let column_prefix = prefix(block_prefix);
    let mut current: Option<(Options, usize)> = None;
    for line in body.lines() {
        match mark_of(line, &column_prefix) {
            Some(Mark::Open(options)) => current = Some((options, 0)),
            Some(Mark::End) => {
                if let Some((_, column)) = current.as_mut() {
                    *column += 1;
                }
            }
            Some(Mark::Close) => current = None,
            Some(Mark::Start) => {}
            None => {
                let Some((options, column)) = current.as_ref() else {
                    continue;
                };
                let index = line
                    .trim()
                    .strip_prefix(block_prefix)
                    .and_then(|rest| rest.strip_suffix("-->"))
                    .and_then(|digits| digits.parse::<usize>().ok());
                if let Some(slot) = index.and_then(|index| out.get_mut(index)) {
                    *slot = Some(Placement {
                        options: options.clone(),
                        column: *column,
                    });
                }
            }
        }
    }
    out
}

/// Replace the column placeholders of rendered `html` with the columns' boxes. Reading order is
/// source order: the boxes only place the columns, nothing reorders them.
pub(crate) fn expand(html: &str, block_prefix: &str) -> String {
    let column_prefix = prefix(block_prefix);
    if !html.contains(&column_prefix) {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len() + 256);
    let mut remaining = html;
    let mut weights: Vec<u8> = vec![];
    let mut column = 0usize;
    while let Some(start) = remaining.find(&column_prefix) {
        out.push_str(&remaining[..start]);
        let rest = &remaining[start..];
        let Some(end) = rest.find("-->").map(|e| e + 3) else {
            out.push_str(rest);
            return out;
        };
        match mark_of(&rest[..end], &column_prefix) {
            Some(Mark::Open(options)) => {
                weights = options.weights.clone();
                column = 0;
                let mut class = String::from("rv-columns");
                class.push_str(if options.wide {
                    " rv-columns--wide"
                } else {
                    " rv-columns--measure"
                });
                if options.gap != "medium" {
                    class.push_str(&format!(" rv-columns--gap-{}", options.gap));
                }
                if options.align != "top" {
                    class.push_str(&format!(" rv-columns--{}", options.align));
                }
                out.push_str(&format!(
                    "<div class=\"{class}\" style=\"--rv-columns-at:{:.2}rem;--rv-columns-n:{}\">",
                    f64::from(options.stack_at) / 100.0,
                    options.weights.len()
                ));
            }
            Some(Mark::Start) => {
                let weight = weights.get(column).copied().unwrap_or(1);
                if weight > 1 {
                    out.push_str(&format!("<div class=\"rv-column rv-column--{weight}\">"));
                } else {
                    out.push_str("<div class=\"rv-column\">");
                }
            }
            Some(Mark::End) => {
                column += 1;
                out.push_str("</div>");
            }
            Some(Mark::Close) => out.push_str("</div>"),
            None => out.push_str(&rest[..end]),
        }
        remaining = &rest[end..];
    }
    out.push_str(remaining);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_read_as_pandoc_divs() {
        assert_eq!(marker(":::"), Some(Marker::Close));
        assert_eq!(marker("::::  \n"), Some(Marker::Close));
        assert_eq!(marker("::: column"), Some(Marker::Column));
        assert_eq!(marker("::: columns"), Some(Marker::Columns(String::new())));
        assert_eq!(
            marker(":::: columns {widths: 2-1}"),
            Some(Marker::Columns("{widths: 2-1}".into()))
        );
        assert_eq!(
            marker("::: columns{gap: large}"),
            Some(Marker::Columns("{gap: large}".into()))
        );
        assert_eq!(marker("::: callout"), Some(Marker::Other));
        assert_eq!(marker("::: columnsx"), Some(Marker::Other));
        assert_eq!(marker(":: column"), None);
        assert_eq!(marker("    ::: column"), None);
        assert_eq!(marker(":::text:::"), None);
    }

    #[test]
    fn options_round_trip_through_the_placeholder() {
        let options = Options {
            weights: vec![2, 1],
            gap: "large",
            align: "center",
            wide: false,
            stack_at: 6713,
        };
        assert_eq!(Options::decode(&options.encode()), Some(options));
        for bad in [
            "o:2-1:large:center:wide",
            "o:4-1:medium:top:wide:10",
            "o:1:medium:top:wide:10",
            "o:1-1:huge:top:wide:10",
            "o:1-1:medium:top:wide:10:x",
            "o:1-1:medium:top:wide:1e3",
            "o:1-1-1-1-1:medium:top:wide:10",
        ] {
            assert_eq!(Options::decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_column_is_its_share_until_the_narrowest_falls_under_its_floor() {
        let wide = |weights: &[u8], tags: &[&[&str]]| {
            let tags: Vec<Vec<String>> = tags
                .iter()
                .map(|t| t.iter().map(|s| s.to_string()).collect())
                .collect();
            stack_at(weights, &tags)
        };
        // Two charts: two phone columns.
        assert_eq!(wide(&[1, 1], &[&["chart"], &["chart"]]), 4475);
        // A chart beside its note, 2-1: the chart's floor one and a half times over.
        assert_eq!(wide(&[2, 1], &[&["chart"], &[]]), 3357);
        let options = Options {
            weights: vec![1, 1],
            gap: "medium",
            align: "top",
            wide: true,
            stack_at: 4475,
        };
        // 832 px of report column with a 28.8 px line: side by side, 401.6 px each.
        assert!((options.column_px(0, 832.0, 28.8) - 401.6).abs() < 1e-9);
        // 720 px on a tablet: stacked, the whole width.
        assert_eq!(options.column_px(1, 720.0, 28.8), 720.0);
    }
}
