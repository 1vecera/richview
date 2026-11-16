use crate::markdown::{Context, markdown_html, safe_media};
use crate::{Diagnostic, Output, RenderOptions, RichBlock, escape, json_script, text};
use regex_lite::Regex;
use richview_chart::numbers;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::{borrow::Cow, cell::OnceCell, cmp::Ordering, sync::OnceLock};

pub(crate) fn resolve_data(
    block: &mut RichBlock,
    _frontmatter: &Value,
    options: &RenderOptions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Inline sources already belong to the block. Cloning and reinserting them made dense
    // reports pay for several full row-tree copies before reaching the chart engine. Their key
    // order is read again from the fence text, because the parsed rows keep none (TYP-25).
    if let Some(inline) = block.value.pointer("/data/inline") {
        if let Some((row, message)) =
            flat_rows_problem(inline.as_array().map_or(&[][..], Vec::as_slice))
        {
            // At the row as written (TYP-56).
            diagnostics.push(
                Diagnostic::new(
                    "error",
                    "data.inline",
                    message,
                    format!("{}.data.inline[{row}]", block.tag),
                )
                .at(block.line, &block.tag),
            );
            return;
        }
        if matches!(block.tag.as_str(), "chart" | "table") {
            let order = crate::yaml::key_order(&block.raw, &["data", "inline"]);
            if let Some(data) = block.value.get_mut("data").and_then(Value::as_object_mut)
                && !order.is_empty()
            {
                data.insert("columns".into(), json!(order));
            }
        }
        return;
    }
    let Some(data) = block.value.get("data") else {
        return;
    };
    // The source key names the format; the content never does (TYP-92).
    let source = ["csv", "json"]
        .iter()
        .find_map(|key| data.get(*key).and_then(Value::as_str).map(|r| (*key, r)));
    let mut csv_numeric_text = false;
    let mut columns = Vec::new();
    let rows = if let Some((content, (kind, reference))) =
        source.and_then(|source| options.data.get(source.1).map(|content| (content, source)))
    {
        parsed_data(
            content,
            kind,
            reference,
            block,
            diagnostics,
            &mut csv_numeric_text,
            &mut columns,
        )
    } else {
        let tag = &block.tag;
        let sources = "write the rows under `data.inline`, or save them as a CSV or JSON file in the report folder and name it in `data.csv` or `data.json`";
        let (message, path, fix) = match (source, data.get("url").and_then(Value::as_str)) {
            (Some((kind, reference)), _) => (
                format!("Data file '{reference}' was not resolved inside this report folder."),
                format!("{tag}.data.{kind}"),
                format!("Add '{reference}' to the report folder, or {sources}."),
            ),
            (None, Some(url)) => (
                format!("{tag}.data.url '{url}' is not loaded by this host, so the block has no rows."),
                format!("{tag}.data.url"),
                "Save the rows from that address as a CSV or JSON file in the report folder and name it in `data.csv` or `data.json`, or write them under `data.inline`.".to_owned(),
            ),
            (None, None) => (
                format!("{tag}.data names no rows this host can read."),
                format!("{tag}.data"),
                format!("{}{}.", sources[..1].to_uppercase(), &sources[1..]),
            ),
        };
        diagnostics.push(
            Diagnostic::new("error", "data.unresolved", message, path)
                .fix(fix)
                .at(block.line, tag),
        );
        vec![]
    };
    let keeps_order = matches!(block.tag.as_str(), "chart" | "table");
    if let Some(data) = block.value.get_mut("data").and_then(Value::as_object_mut) {
        data.insert("inline".into(), Value::Array(rows));
        // The source column order, from a CSV header or the JSON rows' keys, which chart payloads,
        // Table views, CSV downloads and derived table columns keep (TYP-25).
        if keeps_order && !columns.is_empty() {
            data.insert("columns".into(), json!(columns));
        }
    }
    // Persist actual CSV typing for header-click sorting after the resolved bytes are gone.
    // JSON text cells keep their authored type, even when a reference uses the CSV label.
    if csv_numeric_text && block.tag == "table" {
        block.value["_csvNumericText"] = json!(true);
    }
}

/// The keys of a JSON file's rows in first-seen order: the first row's keys as written, then each
/// later row's new keys (TYP-25); fences use `yaml::key_order`. `path` leads from a top-level mapping to the rows; a top-level
/// sequence is the rows themselves. Values are skipped without being built. Text that does not
/// read as rows of mappings gives no order, and callers fall back to their own.
fn key_order<'de, D: serde::Deserializer<'de>>(source: D, path: &[&str]) -> Vec<String> {
    use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
    use std::collections::HashSet;
    use std::fmt;

    struct Order {
        keys: Vec<String>,
        seen: HashSet<String>,
    }
    /// Follows `path` through mappings, then reads a sequence of rows.
    struct Rows<'a, 'p>(&'a mut Order, &'p [&'p str]);
    /// One row: records its keys and skips its values.
    struct Row<'a>(&'a mut Order);

    impl<'de> DeserializeSeed<'de> for Rows<'_, '_> {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_any(self)
        }
    }
    impl<'de> Visitor<'de> for Rows<'_, '_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("rows of mappings")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            while seq.next_element_seed(Row(&mut *self.0))?.is_some() {}
            Ok(())
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            let Some((step, rest)) = self.1.split_first() else {
                return Err(serde::de::Error::custom("rows expected"));
            };
            while let Some(key) = map.next_key::<String>()? {
                if key == *step {
                    map.next_value_seed(Rows(&mut *self.0, rest))?;
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(())
        }
    }
    impl<'de> DeserializeSeed<'de> for Row<'_> {
        type Value = ();
        fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_map(self)
        }
    }
    impl<'de> Visitor<'de> for Row<'_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a row mapping")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while let Some(key) = map.next_key::<String>()? {
                map.next_value::<IgnoredAny>()?;
                if self.0.seen.insert(key.clone()) {
                    self.0.keys.push(key);
                }
            }
            Ok(())
        }
    }

    let mut order = Order {
        keys: Vec::new(),
        seen: HashSet::new(),
    };
    match Rows(&mut order, path).deserialize(source) {
        Ok(()) => order.keys,
        Err(_) => Vec::new(),
    }
}

/// Whether a row set holds only flat rows, as the index of the first row that is not and the
/// message to show. A row is a mapping of text, finite numbers, `true`/`false` and null; a nested
/// mapping or list is not data a table or chart can read (TYP-38).
pub(crate) fn flat_rows_problem(rows: &[Value]) -> Option<(usize, String)> {
    for (index, row) in rows.iter().enumerate() {
        let number = index + 1;
        let Value::Object(row) = row else {
            return Some((
                index,
                format!(
                    "Row {number} is not a mapping of column names to values. Write each row as key: value pairs."
                ),
            ));
        };
        for (key, value) in row {
            let kind = match value {
                Value::Array(_) => "a list",
                Value::Object(_) => "a mapping",
                _ => continue,
            };
            return Some((
                index,
                format!(
                    "Row {number}, key '{key}' holds {kind}. Rows hold only text, numbers, true, false or null; flatten it into separate columns."
                ),
            ));
        }
    }
    None
}

/// The data file a block reads, named in its `data.csv` or `data.json`.
struct DataFile<'a> {
    kind: &'a str,
    reference: &'a str,
    block: &'a RichBlock,
}

impl DataFile<'_> {
    /// A finding about the file, at its line and column in the file (TYP-56); without them, at
    /// the field that names the file.
    fn error(
        &self,
        code: &str,
        message: String,
        at: Option<(usize, usize)>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Value> {
        let path = format!("{}.data.{}", self.block.tag, self.kind);
        let mut d =
            Diagnostic::new("error", code, message, path).at(self.block.line, &self.block.tag);
        if let Some((line, column)) = at {
            d = d.in_file(self.reference);
            d.line = Some(line);
            d.column = Some(column);
        }
        diagnostics.push(d);
        vec![]
    }
}

/// The 1-based line and column of a byte offset, counting columns in characters as YAML
/// positions do.
fn text_position(content: &str, offset: usize) -> (usize, usize) {
    let before = &content[..offset];
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    (
        before.matches('\n').count() + 1,
        before[start..].chars().count() + 1,
    )
}

/// The 1-based line a CSV record starts on, counted from its byte offset. The reader's own line
/// count is one short in a file with CRLF line endings, where it starts a record before it has
/// read the line feed that ended the last one.
fn csv_line(content: &str, position: &csv::Position) -> usize {
    let bytes = content.as_bytes();
    let mut start = (position.byte() as usize).min(bytes.len());
    while bytes.get(start).is_some_and(|b| matches!(b, b'\r' | b'\n')) {
        start += 1;
    }
    bytes[..start].iter().filter(|b| **b == b'\n').count() + 1
}

/// The byte offset where each wanted element of a top-level JSON array starts, for text that
/// has already parsed as an array.
fn json_row_starts(content: &str, wanted: &BTreeSet<usize>) -> BTreeMap<usize, usize> {
    let mut starts = BTreeMap::new();
    let (mut depth, mut row) = (0usize, 0usize);
    let (mut in_string, mut escaped, mut next) = (false, false, false);
    for (offset, byte) in content.bytes().enumerate() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        if next && !byte.is_ascii_whitespace() {
            next = false;
            if wanted.contains(&row) {
                starts.insert(row, offset);
                if starts.len() == wanted.len() {
                    break;
                }
            }
            row += 1;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                next = depth == 1;
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 1 => next = true,
            _ => {}
        }
    }
    starts
}

/// The 1-based line and column where each wanted row of a data file starts: a CSV record or an
/// element of the JSON array.
fn row_positions(
    content: &str,
    kind: &str,
    wanted: &BTreeSet<usize>,
) -> BTreeMap<usize, (usize, usize)> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    if kind == "json" {
        return json_row_starts(content, wanted)
            .into_iter()
            .map(|(row, offset)| (row, text_position(content, offset)))
            .collect();
    }
    let mut reader = csv::ReaderBuilder::new().from_reader(content.as_bytes());
    let mut record = csv::StringRecord::new();
    let mut positions = BTreeMap::new();
    let last = wanted.last().copied().unwrap_or(0);
    for row in 0..=last {
        if !reader.read_record(&mut record).unwrap_or(false) {
            break;
        }
        if wanted.contains(&row)
            && let Some(position) = record.position()
        {
            positions.insert(row, (csv_line(content, position), 1));
        }
    }
    positions
}

/// Point each finding about one resolved row (`rows[n].field`) at that row in the data file the
/// block read, when it read one (TYP-56). Rows written in the fence keep their YAML position.
pub(crate) fn place_in_data_file(
    diagnostics: &mut [Diagnostic],
    block: &RichBlock,
    options: &RenderOptions,
) {
    let row_of = |d: &Diagnostic| -> Option<usize> {
        if d.file.is_some() {
            return None;
        }
        let rest = d.path.strip_prefix("rows[")?;
        rest[..rest.find(']')?].parse().ok()
    };
    let wanted: BTreeSet<usize> = diagnostics.iter().filter_map(row_of).collect();
    if wanted.is_empty() {
        return;
    }
    let Some(data) = block.value.get("data") else {
        return;
    };
    let Some((kind, reference, content)) = ["csv", "json"].into_iter().find_map(|kind| {
        let reference = data.get(kind)?.as_str()?;
        Some((kind, reference, options.data.get(reference)?))
    }) else {
        return;
    };
    let positions = row_positions(content, kind, &wanted);
    for d in diagnostics {
        if let Some(&(line, column)) = row_of(d).and_then(|row| positions.get(&row)) {
            d.file = Some(reference.to_owned());
            d.line = Some(line);
            d.column = Some(column);
        }
    }
}

fn parsed_data(
    content: &str,
    kind: &str,
    reference: &str,
    block: &RichBlock,
    diagnostics: &mut Vec<Diagnostic>,
    csv_numeric_text: &mut bool,
    columns: &mut Vec<String>,
) -> Vec<Value> {
    let file = DataFile {
        kind,
        reference,
        block,
    };
    if content.len() > crate::MAX_DATASET_BYTES {
        let message = format!(
            "Data file '{reference}' is {} MiB, over the 16 MiB limit for one dataset. Aggregate or split it.",
            content.len().div_ceil(1024 * 1024)
        );
        return file.error("data.limit", message, None, diagnostics);
    }
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    if kind == "json" {
        parse_json(content, &file, diagnostics, columns)
    } else {
        parse_csv(content, &file, diagnostics, csv_numeric_text, columns)
    }
}

/// A JSON data file: an array of flat row objects (TYP-38).
fn parse_json(
    content: &str,
    file: &DataFile,
    diagnostics: &mut Vec<Diagnostic>,
    columns: &mut Vec<String>,
) -> Vec<Value> {
    let reference = file.reference;
    let fail = |message: String, offset: usize, diagnostics: &mut Vec<Diagnostic>| {
        let at = text_position(content, offset);
        file.error("data.json", message, Some(at), diagnostics)
    };
    // The top-level value starts at the first character that is not white space.
    let start = content.len() - content.trim_start().len();
    let rows = match serde_json::from_str::<Value>(content) {
        Ok(Value::Array(rows)) => rows,
        Ok(_) => {
            return fail(
                format!(
                    "JSON data file '{reference}' must be an array of row objects, such as [{{\"x\": 1}}]. Unwrap any outer object."
                ),
                start,
                diagnostics,
            );
        }
        Err(err) => {
            // serde_json counts the column in bytes; the record counts characters.
            let line_start = content
                .split_inclusive('\n')
                .take(err.line().saturating_sub(1))
                .map(str::len)
                .sum::<usize>();
            let mut offset = (line_start + err.column().saturating_sub(1)).min(content.len());
            while !content.is_char_boundary(offset) {
                offset -= 1;
            }
            return fail(
                format!(
                    "JSON data file '{reference}' is not valid JSON ({err}). Write an array of row objects."
                ),
                offset,
                diagnostics,
            );
        }
    };
    if let Some((row, problem)) = flat_rows_problem(&rows) {
        let offset = json_row_starts(content, &BTreeSet::from([row]))
            .get(&row)
            .copied()
            .unwrap_or(start);
        return fail(
            format!("JSON data file '{reference}': {problem}"),
            offset,
            diagnostics,
        );
    }
    *columns = key_order(&mut serde_json::Deserializer::from_str(content), &[]);
    rows
}

/// A CSV data file: UTF-8, RFC 4180 quoting, comma delimiter, trimmed cells (TYP-37).
fn parse_csv(
    content: &str,
    file: &DataFile,
    diagnostics: &mut Vec<Diagnostic>,
    csv_numeric_text: &mut bool,
    columns: &mut Vec<String>,
) -> Vec<Value> {
    let reference = file.reference;
    // A CSV position is a record's first line; its column is the line's start.
    let at = |position: Option<&csv::Position>| position.map(|p| (csv_line(content, p), 1));
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());
    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(err) => {
            let message = format!(
                "CSV data file '{reference}' has an unreadable header row ({err}). Write comma-separated values with a header row."
            );
            return file.error("data.csv", message, at(err.position()), diagnostics);
        }
    };
    let mut seen = std::collections::HashSet::new();
    if headers.is_empty() || headers.iter().any(|h| h.is_empty() || !seen.insert(h)) {
        let message = format!(
            "CSV data file '{reference}' needs a header row whose column names are non-empty and unique. Rename the empty or repeated columns."
        );
        let header = at(headers.position()).or(Some((1, 1)));
        return file.error("data.csv.header", message, header, diagnostics);
    }
    columns.extend(headers.iter().map(str::to_owned));
    let mut rows = Vec::new();
    for record in reader.records() {
        match record {
            Ok(record) => {
                let row = headers
                    .iter()
                    .zip(record.iter())
                    .map(|(key, value)| (key.to_owned(), csv_value(value, csv_numeric_text)))
                    .collect::<serde_json::Map<_, _>>();
                rows.push(Value::Object(row));
            }
            Err(err) => {
                let line = err.position().map_or(0, |p| csv_line(content, p));
                let message = match err.kind() {
                    csv::ErrorKind::UnequalLengths {
                        expected_len, len, ..
                    } => format!(
                        "Line {line} of CSV data file '{reference}' has {len} fields but the header has {expected_len}. Only commas separate fields; convert semicolon or tab files and quote cells that contain a comma."
                    ),
                    _ => format!(
                        "Line {line} of CSV data file '{reference}' cannot be read ({err}). Fix the quoting."
                    ),
                };
                // The file is misread from here on; one finding is enough.
                return file.error("data.csv", message, at(err.position()), diagnostics);
            }
        }
    }
    rows
}

fn csv_identifier(input: &str) -> bool {
    input.starts_with('0') && input.len() > 1 && !input.starts_with("0.")
}

fn csv_value(input: &str, numeric_text: &mut bool) -> Value {
    if input.is_empty() {
        return Value::Null;
    }
    if matches!(input, "true" | "false") {
        return json!(input == "true");
    }
    if csv_identifier(input) {
        return json!(input);
    }
    // JavaScript consumers parse JSON numbers as binary64, including JSON integers.
    // Keep integer representation when its decimal value survives that boundary.
    const EXACT_INTEGER: u64 = 1 << 53;
    if let Ok(integer) = input.parse::<i64>() {
        return if integer.unsigned_abs() <= EXACT_INTEGER
            || richview_chart::data::parse_roundtrip_number(input).is_some()
        {
            json!(integer)
        } else {
            *numeric_text = true;
            json!(input)
        };
    }
    if let Ok(integer) = input.parse::<u64>() {
        return if integer <= EXACT_INTEGER
            || richview_chart::data::parse_roundtrip_number(input).is_some()
        {
            json!(integer)
        } else {
            *numeric_text = true;
            json!(input)
        };
    }
    if let Some(value) = richview_chart::data::parse_roundtrip_number(input) {
        json!(value)
    } else {
        if !*numeric_text {
            *numeric_text = richview_chart::data::parse_finite_number(input).is_ok();
        }
        json!(input)
    }
}

pub(crate) fn render_block(
    block: &RichBlock,
    options: &RenderOptions,
    lang: &str,
    failed_prefix: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> String {
    match block.tag.as_str() {
        "metric" => render_big(&block.value),
        "table" => table_html(
            &block.value,
            if options.output == Output::Pdf {
                TableMode::Print
            } else {
                TableMode::Report
            },
            diagnostics,
            block.line,
        ),
        "callout" => {
            let c = &block.value;
            let kind = c.get("kind").and_then(Value::as_str).unwrap_or("neutral");
            if kind != "neutral"
                && !c
                    .get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|t| !t.trim().is_empty())
            {
                diagnostics.push(
                    Diagnostic::new(
                        "warning",
                        "callout.title",
                        "Name the finding in a title; colour alone is not read",
                        "callout.title",
                    )
                    .fix("Add `title:` with the finding in a few words")
                    .at(block.line, "callout"),
                );
            }
            // A rich fence inside the body cannot render here; it shows a notice (FEA-RP-53).
            let source = c.get("body").and_then(Value::as_str).unwrap_or("");
            let (fences, _) = crate::fences::scan(source, crate::RICH_TAGS);
            let mut text = String::with_capacity(source.len());
            let mut notices = vec![];
            let mut cursor = 0;
            for fence in &fences {
                text.push_str(&source[cursor..fence.start]);
                text.push_str(&format!("\n{failed_prefix}{}-->\n", notices.len()));
                let message =
                    "a rich block cannot sit inside a callout; move it out to the top level";
                diagnostics.push(
                    Diagnostic::new("error", "block.nested", message, "callout.body")
                        .fix("Move the block out of the callout body")
                        .at(block.line, "callout"),
                );
                notices.push(crate::render::notice(
                    &format!("{} not rendered: {message}", tag_title(&fence.tag)),
                    Some(block.line),
                    Some(source[fence.start..fence.end].trim_end()),
                ));
                cursor = fence.end;
            }
            text.push_str(&source[cursor..]);
            let block_prefix = failed_prefix.replacen("failed:", "block:", 1);
            let body = markdown_html(
                &text,
                options,
                &Context {
                    heading_ids: &[],
                    first_line: Some(block.line),
                    columns: false,
                    path: "callout.body",
                    scope: &format!("c{}-", block.line),
                    placeholder: &block_prefix,
                    lang,
                },
                diagnostics,
            );
            let body = if notices.is_empty() {
                body
            } else {
                crate::render::expand_blocks(&body, failed_prefix, &notices, &[]).unwrap_or(body)
            };
            // The title runs into the first paragraph; a body that opens with another block
            // keeps the title as its own line.
            let body = match c.get("title").and_then(Value::as_str).map(escape) {
                Some(title) => match body.strip_prefix("<p>") {
                    Some(rest) => format!("<p><b class=\"rv-callout__title\">{title}</b> {rest}"),
                    None => format!("<p class=\"rv-callout__title\">{title}</p>{body}"),
                },
                None => body,
            };
            format!(
                "<aside class=\"rv-callout rv-callout--{}\"><div class=\"rv-callout__body\">{body}</div></aside>",
                escape(c.get("kind").and_then(Value::as_str).unwrap_or("neutral")),
            )
        }
        "image" | "audio" | "video" => render_media(
            &block.tag,
            &block.value,
            options,
            lang,
            diagnostics,
            block.line,
        ),
        "embed" => render_embed(&block.value, options, diagnostics, block.line),
        "scrolly" => crate::scrolly::render(
            &block.value,
            options,
            lang,
            &failed_prefix.replacen("failed:", "block:", 1),
            diagnostics,
            block.line,
            None,
        ),
        _ => String::new(),
    }
}

/// How one number prints (TYP-23): a `format` name, a `unit`, significant `digits` and the
/// report language's separators.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ValueFormat<'a> {
    pub format: Option<&'a str>,
    pub unit: Option<&'a str>,
    pub digits: Option<u8>,
    pub lang: Option<&'a str>,
}

impl<'a> ValueFormat<'a> {
    /// The `format`, `unit` and `digits` fields of a column or metric mapping.
    pub fn of(value: &'a Value, lang: Option<&'a str>) -> Self {
        let lang = lang.or_else(|| value.get("_lang").and_then(Value::as_str));
        ValueFormat {
            format: value.get("format").and_then(Value::as_str),
            unit: value.get("unit").and_then(Value::as_str),
            digits: value
                .get("digits")
                .and_then(Value::as_u64)
                .map(|d| d.clamp(1, numbers::MAX_DIGITS as u64) as u8),
            lang,
        }
    }
}

/// `value` as readers see it: numbers and numeric strings take the format, the unit and the
/// three-digit rule; any other text is shown as written, without format or unit (TYP-27).
pub(crate) fn format_value(value: &Value, spec: &ValueFormat) -> String {
    if value.is_null() {
        return String::new();
    }
    let Some(n) = richview_chart::rmd_render::number(value) else {
        return text(value);
    };
    numbers::value(
        n,
        &numbers::Format {
            kind: spec
                .format
                .and_then(numbers::Kind::parse)
                .unwrap_or_default(),
            unit: spec.unit.map(str::to_owned),
            digits: spec.digits.unwrap_or(numbers::DEFAULT_DIGITS),
            locale: spec.lang.map(numbers::Locale::from_tag).unwrap_or_default(),
        },
    )
}

/// A signed percentage at `digits` significant digits (three by default), never more: the
/// percentage is rounded to them first (`+10%` for 12.3 at one digit, `+120%` for 123.4 at two),
/// then printed with at least one decimal when that adds no digit beyond them: `+4.0%`, `−2.0%`,
/// `0.0%`. The minus is U+2212 and the decimal mark follows `locale`.
fn signed_percent(fraction: f64, digits: u8, locale: numbers::Locale) -> String {
    let pct = fraction * 100.0;
    let magnitude = numbers::round_sig(pct.abs(), digits);
    let most = f64::from(digits.max(numbers::DEFAULT_DIGITS)) + 3.0;
    let decimals = if magnitude == 0.0 {
        1
    } else {
        (f64::from(digits) - 1.0 - magnitude.log10().floor()).clamp(0.0, most) as usize
    };
    let mut text = format!("{magnitude:.decimals$}");
    // Drop trailing zeros, keeping the one decimal the digits allow.
    let mut shown = decimals;
    if let Some((whole, frac)) = text.split_once('.').filter(|_| decimals > 0) {
        let frac = frac.trim_end_matches('0');
        shown = frac.len().max(1);
        text = format!("{whole}.{}", if frac.is_empty() { "0" } else { frac });
    }
    let sign = if pct > 0.0 && text != "0.0" {
        "+"
    } else if pct < 0.0 && text != "0.0" {
        "\u{2212}"
    } else {
        ""
    };
    let number = numbers::grouped(text.parse().unwrap_or(magnitude), shown, locale, false);
    format!("{sign}{number}%")
}

/// A metric as a stat line (FEA-RP-23): value, label, then the delta with its sign and words.
/// The trend colour defaults to the delta's sign (TYP-88); the sign carries the meaning too.
fn render_big(big: &Value) -> String {
    // Numbers and numeric strings take the format, unit and digits; any other text reads as
    // written, without format or unit (TYP-27).
    let spec = ValueFormat::of(big, None);
    let value = format_value(&big["value"], &spec);
    let label = big
        .get("label")
        .and_then(Value::as_str)
        .map(|v| format!("<div class=\"rv-metric__label\">{}</div>", escape(v)))
        .unwrap_or_default();
    let delta = big
        .get("delta")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .map(|n| {
            let sign = if n > 0.0 {
                "up"
            } else if n < 0.0 {
                "down"
            } else {
                "flat"
            };
            let trend = big
                .get("trend")
                .and_then(Value::as_str)
                .filter(|t| matches!(*t, "up" | "down" | "flat"))
                .unwrap_or(sign);
            format!(
                "<span class=\"rv-metric__delta rv-metric__delta--{trend}\">{}{}</span>",
                signed_percent(
                    n,
                    spec.digits.unwrap_or(numbers::DEFAULT_DIGITS),
                    spec.lang.map(numbers::Locale::from_tag).unwrap_or_default()
                ),
                big.get("delta_label")
                    .and_then(Value::as_str)
                    .map(|s| format!(" {}", escape(s)))
                    .unwrap_or_default()
            )
        })
        .unwrap_or_default();
    format!(
        "<div class=\"rv-metric\"><div class=\"rv-metric__value\">{}</div>{label}{delta}</div>",
        escape(&value)
    )
}

#[derive(Clone)]
struct Column {
    key: String,
    label: String,
    format: Option<String>,
    unit: Option<String>,
    digits: Option<u8>,
    lang: Option<String>,
    /// `left`, `center` or `right` when the author set it.
    align: Option<String>,
    /// Every cell is a number, so the column is right-aligned unless `align` says otherwise.
    numeric: bool,
}

/// How a table is written: into the report, into a browser re-render, or for paper.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TableMode {
    /// Plain headers: the runtime turns them into sort buttons once it runs (FEA-RP-22).
    Report,
    /// Sort buttons and the table payload, for a browser that re-renders a sorted table.
    Browser,
    /// Every row in one table, plain headers, no payload.
    Print,
}

/// Render a resolved table and its source state. Native `sort:` is deterministic; the Rust
/// browser controller uses this same function after a header click, without a second engine.
pub fn render_table(table: &Value) -> String {
    table_html(table, TableMode::Browser, &mut Vec::new(), 0)
}

fn table_columns(table: &Value, rows: &[&Value], csv: bool) -> Vec<Column> {
    let numeric = |key: &str| {
        let mut any = false;
        for row in rows {
            let value = &row[key];
            // A missing cell is neither a number nor a reason to call the column text (TYP-91).
            if richview_chart::rows::is_missing(value) {
                continue;
            }
            if table_number(value, csv).is_none() {
                return false;
            }
            any = true;
        }
        any
    };
    let keys: Vec<(String, Option<&Value>)> =
        if let Some(columns) = table.get("columns").and_then(Value::as_array) {
            columns
                .iter()
                .map(|c| (c["key"].as_str().unwrap_or("").to_owned(), Some(c)))
                .collect()
        } else {
            // Source order: the CSV header or the rows' first-seen keys (TYP-25); the first row's
            // keys when the source recorded none.
            let recorded: Option<Vec<String>> = table
                .pointer("/data/columns")
                .and_then(Value::as_array)
                .map(|keys| {
                    keys.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                });
            recorded
                .or_else(|| {
                    rows.first()
                        .and_then(|row| row.as_object())
                        .map(|r| r.keys().cloned().collect())
                })
                .unwrap_or_default()
                .into_iter()
                .map(|key| (key, None))
                .collect()
        };
    keys.into_iter()
        .map(|(key, spec)| {
            let text = |name: &str| spec.and_then(|c| c.get(name)).and_then(Value::as_str);
            Column {
                label: text("label").unwrap_or(&key).to_owned(),
                format: text("format").map(str::to_owned),
                unit: text("unit").map(str::to_owned),
                digits: spec
                    .map(|c| ValueFormat::of(c, None))
                    .and_then(|f| f.digits),
                lang: table
                    .get("_lang")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                align: text("align")
                    .filter(|s| matches!(*s, "left" | "right" | "center"))
                    .map(str::to_owned),
                numeric: numeric(&key),
                key,
            }
        })
        .collect()
}

/// The characters a column needs, so a table split into parts keeps its columns aligned.
fn column_weights(columns: &[Column], rows: &[&Value]) -> Vec<usize> {
    columns
        .iter()
        .map(|c| {
            let widest = rows
                .iter()
                .map(|row| format_value(&row[&c.key], &c.spec()))
                .map(|shown| shown.chars().count())
                .max()
                .unwrap_or(0);
            widest.max(c.label.chars().count()).clamp(4, 28)
        })
        .collect()
}

/// The digits after the decimal point of a number as written, counting an exponent (`1e-7` has 7).
fn decimals_of(text: &str) -> usize {
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map_or((text, 0i32), |(m, e)| (m, e.parse().unwrap_or(0)));
    let digits = mantissa
        .split_once('.')
        .map_or(0, |(_, d)| d.bytes().take_while(u8::is_ascii_digit).count());
    (digits as i32 - exponent).max(0) as usize
}

/// The `total` row (FEA-RP-21): the sum of each listed column, or with `true` of every numeric
/// column except `percent` and `year` ones, since rates and years do not add up. The first column
/// reads "Total" unless it is summed.
fn total_row(
    table: &Value,
    columns: &[Column],
    rows: &[&Value],
    csv: bool,
    diagnostics: &mut Vec<Diagnostic>,
    line: usize,
) -> Option<String> {
    let warn = |diagnostics: &mut Vec<Diagnostic>, message: String| {
        diagnostics.push(
            Diagnostic::new("warning", "table.total", message, "table.total")
                .fix("List the columns to sum: total: [accounts, revenue]")
                .at(line, "table"),
        );
    };
    let summed: Vec<bool> = match table.get("total") {
        Some(Value::Bool(true)) => {
            let unsummed = |c: &Column| matches!(c.format.as_deref(), Some("percent" | "year"));
            let skipped: Vec<&Column> = columns
                .iter()
                .filter(|c| c.numeric && unsummed(c))
                .collect();
            if !skipped.is_empty() {
                let kinds: Vec<&str> = ["percent", "year"]
                    .into_iter()
                    .filter(|kind| skipped.iter().any(|c| c.format.as_deref() == Some(kind)))
                    .collect();
                let keys: Vec<&str> = skipped.iter().map(|c| c.key.as_str()).collect();
                warn(
                    diagnostics,
                    format!(
                        "total: true skips {} columns ({})",
                        kinds.join(" and "),
                        keys.join(", ")
                    ),
                );
            }
            columns.iter().map(|c| c.numeric && !unsummed(c)).collect()
        }
        Some(Value::Array(keys)) => {
            let wanted: Vec<&str> = keys.iter().filter_map(Value::as_str).collect();
            let mut unsummable = vec![];
            let flags = columns
                .iter()
                .map(|c| {
                    let listed = wanted.contains(&c.key.as_str());
                    if listed && !c.numeric {
                        unsummable.push(c.key.as_str());
                    }
                    listed && c.numeric
                })
                .collect();
            if !unsummable.is_empty() {
                warn(
                    diagnostics,
                    format!(
                        "total skips columns that are not all numbers ({})",
                        unsummable.join(", ")
                    ),
                );
            }
            flags
        }
        _ => return None,
    };
    let cells = columns
        .iter()
        .zip(&summed)
        .enumerate()
        .map(|(i, (c, summed))| {
            let value = if *summed {
                let sum: f64 = rows
                    .iter()
                    .filter_map(|row| table_number(&row[&c.key], csv))
                    .sum();
                // The sum shows no more decimals than the cells it adds (FEA-CH-37).
                let places = rows
                    .iter()
                    .map(|row| match &row[&c.key] {
                        Value::Number(n) => decimals_of(&n.to_string()),
                        Value::String(t) => decimals_of(t.trim()),
                        _ => 0,
                    })
                    .max()
                    .unwrap_or(0)
                    .min(9);
                let scale = 10f64.powi(places as i32);
                let sum = (sum * scale).round() / scale;
                // A whole sum prints as a whole number, not `1230.0`.
                let sum = if sum.fract() == 0.0 && sum.abs() < 9e15 {
                    json!(sum as i64)
                } else {
                    json!(sum)
                };
                format_value(&sum, &c.spec())
            } else if i == 0 {
                "Total".into()
            } else {
                String::new()
            };
            format!("<td class=\"{}\">{}</td>", column_class(c), escape(&value))
        })
        .collect::<String>();
    Some(format!("<tr>{cells}</tr>"))
}

/// One semantic table (FEA-RP-19): header cells with `scope="col"`, values formatted per column,
/// rows beyond `max_rows` in a native disclosure directly below, and the total row after all
/// rows. Print sets every row in one table.
fn table_html(
    table: &Value,
    mode: TableMode,
    diagnostics: &mut Vec<Diagnostic>,
    line: usize,
) -> String {
    let print = mode == TableMode::Print;
    let csv = table["_csvNumericText"].as_bool().unwrap_or(false);
    let mut rows: Vec<_> = table
        .pointer("/data/inline")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    let columns = table_columns(table, &rows, csv);
    let sorted_by = table.pointer("/sort/key").and_then(Value::as_str);
    if let Some(key) = sorted_by {
        // Missing cells sort last in both directions (TYP-91).
        let (mut present, missing): (Vec<_>, Vec<_>) = rows
            .iter()
            .map(|&row| (row, TableSortKey::new(&row[key], csv)))
            .partition(|(_, key)| !matches!(key, TableSortKey::Missing));
        present.sort_by(|(_, left), (_, right)| left.compare(right));
        if table.pointer("/sort/dir").and_then(Value::as_str) == Some("desc") {
            present.reverse();
        }
        rows.clear();
        rows.extend(present.into_iter().chain(missing).map(|(row, _)| row));
    }
    let limit = if print {
        rows.len()
    } else {
        table.get("max_rows").and_then(Value::as_u64).unwrap_or(20) as usize
    };
    let folded = rows.len() > limit;
    let head = |hidden: bool| {
        let cells = columns
            .iter()
            .map(|c| {
                let active = sorted_by == Some(c.key.as_str());
                let direction = if !active {
                    "none"
                } else if table.pointer("/sort/dir").and_then(Value::as_str) == Some("desc") {
                    "descending"
                } else {
                    "ascending"
                };
                let class = column_class(c);
                let label = escape(&c.label);
                if mode == TableMode::Browser {
                    format!("<th scope=\"col\" class=\"{class}\" aria-sort=\"{direction}\"><button type=\"button\" data-rv-sort=\"{}\" aria-label=\"Sort by {label}\">{label}</button></th>", escape(&c.key))
                } else if print || hidden {
                    format!("<th scope=\"col\" class=\"{class}\">{label}</th>")
                } else {
                    let aria = if active {
                        format!(" aria-sort=\"{direction}\"")
                    } else {
                        String::new()
                    };
                    format!("<th scope=\"col\" class=\"{class}\" data-rv-key=\"{}\"{aria}>{label}</th>", escape(&c.key))
                }
            })
            .collect::<String>();
        let class = if hidden {
            " class=\"rv-visually-hidden\""
        } else {
            ""
        };
        format!("<thead{class}><tr>{cells}</tr></thead>")
    };
    let visible = table_rows(&rows[..rows.len().min(limit)], &columns, print, csv);
    let totals = total_row(table, &columns, &rows, csv, diagnostics, line);
    let caption = table
        .get("title")
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .map(|t| format!("<caption class=\"rv-table__title\">{}</caption>", escape(t)))
        .unwrap_or_default();
    // A typed table's cells carry their sort values, so the runtime reads no type from text.
    let typed = if print { "" } else { " data-rv-typed" };
    let payload = if mode == TableMode::Browser {
        format!(
            "<script type=\"application/json\" data-rv-table>{}</script>",
            json_script(table)
        )
    } else {
        String::new()
    };
    if !folded {
        let foot = totals
            .map(|row| format!("<tfoot>{row}</tfoot>"))
            .unwrap_or_default();
        return format!(
            "<div class=\"rv-table{}\"><table{typed}>{caption}{}<tbody>{visible}</tbody>{foot}</table>{payload}</div>",
            wide(table, "rv-table--full"),
            head(false)
        );
    }
    // Parts of one table keep the same column widths, so the folded rows continue the columns.
    let weights = column_weights(&columns, &rows);
    let sum: usize = weights.iter().sum();
    let cols = weights
        .iter()
        .map(|w| {
            format!(
                "<col style=\"width:{:.2}%\">",
                *w as f64 * 100.0 / sum as f64
            )
        })
        .collect::<String>();
    let min = format!(" style=\"min-width:{}ch\"", sum + 2 * columns.len());
    let rest = table_rows(&rows[limit..], &columns, false, csv);
    let total = totals
        .map(|row| {
            format!("<table class=\"rv-table__part rv-table__total\"{min}><colgroup>{cols}</colgroup><tbody>{row}</tbody></table>")
        })
        .unwrap_or_default();
    format!(
        "<div class=\"rv-table{}\"><table class=\"rv-table__part\"{typed}{min}>{caption}<colgroup>{cols}</colgroup>{}<tbody>{visible}</tbody></table><details class=\"rv-table__overflow\"><summary>Showing {limit} of {} rows</summary><table class=\"rv-table__part\" data-rv-typed{min}><colgroup>{cols}</colgroup>{}<tbody>{rest}</tbody></table></details>{total}{payload}</div>",
        wide(table, "rv-table--full"),
        head(false),
        rows.len(),
        head(true)
    )
}

/// ` <class>` when the block asked to take the report's full width (TYP-89).
fn wide(value: &Value, class: &str) -> String {
    if value.get("wide").and_then(Value::as_bool) == Some(true) {
        format!(" {class}")
    } else {
        String::new()
    }
}

/// Resolved CSV keeps precise numeric cells as text. Preserve their numeric operations,
/// while authored inline strings and leading-zero CSV identifiers remain textual.
fn table_number(value: &Value, csv: bool) -> Option<f64> {
    value.as_f64().or_else(|| {
        let text = value.as_str().filter(|text| csv && !csv_identifier(text))?;
        richview_chart::data::parse_finite_number(text).ok()
    })
}

// Classify each cell once. Exact decimal keys are needed only when different source
// values share a rounded binary64 value; cache that parsing within this one sort.
enum TableSortKey<'a> {
    /// Null or a missing marker: after every value in both directions (TYP-91).
    Missing,
    Number {
        number: f64,
        source: &'a Value,
        exact: OnceCell<Option<richview_chart::data::DecimalKey>>,
    },
    Text(Cow<'a, str>),
}

impl<'a> TableSortKey<'a> {
    fn new(value: &'a Value, csv: bool) -> Self {
        if richview_chart::rows::is_missing(value) {
            Self::Missing
        } else if let Some(number) = table_number(value, csv) {
            Self::Number {
                number,
                source: value,
                exact: OnceCell::new(),
            }
        } else {
            Self::Text(match value {
                Value::String(value) => Cow::Borrowed(value),
                _ => Cow::Owned(text(value)),
            })
        }
    }

    fn compare(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Missing, Self::Missing) => Ordering::Equal,
            (Self::Missing, _) => Ordering::Greater,
            (_, Self::Missing) => Ordering::Less,
            (
                Self::Number {
                    number: left,
                    source: a,
                    exact: a_key,
                },
                Self::Number {
                    number: right,
                    source: b,
                    exact: b_key,
                },
            ) => left.total_cmp(right).then_with(|| {
                if a == b {
                    return Ordering::Equal;
                }
                match (
                    a_key.get_or_init(|| richview_chart::data::DecimalKey::parse(&text(a))),
                    b_key.get_or_init(|| richview_chart::data::DecimalKey::parse(&text(b))),
                ) {
                    (Some(a), Some(b)) => a.cmp(b),
                    _ => Ordering::Equal,
                }
            }),
            (Self::Number { .. }, Self::Text(_)) => Ordering::Less,
            (Self::Text(_), Self::Number { .. }) => Ordering::Greater,
            (Self::Text(a), Self::Text(b)) => a.cmp(b),
        }
    }
}
impl Column {
    fn spec(&self) -> ValueFormat<'_> {
        ValueFormat {
            format: self.format.as_deref(),
            unit: self.unit.as_deref(),
            digits: self.digits,
            lang: self.lang.as_deref(),
        }
    }
}

fn column_class(c: &Column) -> &'static str {
    match c.align.as_deref() {
        Some("right") => "num",
        Some("center") => "center",
        Some(_) => "",
        None if c.numeric
            || matches!(
                c.format.as_deref(),
                Some("percent" | "number" | "number_compact" | "year")
            ) =>
        {
            "num"
        }
        None => "",
    }
}
fn table_rows(rows: &[&Value], columns: &[Column], print: bool, csv: bool) -> String {
    rows.iter()
        .map(|row| {
            format!(
                "<tr>{}</tr>",
                columns
                    .iter()
                    .map(|c| {
                        let value = &row[&c.key];
                        // Numbers take the format and the three-digit rule; text, identifiers
                        // such as `001` and inline strings are shown as written (TYP-23).
                        let shown = if table_number(value, csv).is_some() {
                            format_value(value, &c.spec())
                        } else {
                            text(value)
                        };
                        table_cell(
                            &shown,
                            c,
                            print,
                            sort_value(value, csv, &c.spec()).as_deref(),
                        )
                    })
                    .collect::<String>()
            )
        })
        .collect()
}

/// The value a numeric cell sorts by in the report runtime, as decimal text: the number the cell
/// shows (`45%`, `$1,200`, `1.2 million`, or `1.23` under the three-digit rule), at the precision
/// it displays and never finer, so the page carries no digit the reader cannot see (FEA-CH-37,
/// FEA-RP-22). Cells that show the same value tie, and the runtime keeps the author's order for
/// them. Text and empty cells have none.
fn sort_value(value: &Value, csv: bool, spec: &ValueFormat) -> Option<String> {
    let TableSortKey::Number { number, .. } = TableSortKey::new(value, csv) else {
        return None;
    };
    let shown = numbers::shown(
        number,
        &numbers::Format {
            kind: spec
                .format
                .and_then(numbers::Kind::parse)
                .unwrap_or_default(),
            unit: None,
            digits: spec.digits.unwrap_or(numbers::DEFAULT_DIGITS),
            locale: numbers::Locale::default(),
        },
    );
    Some(format!("{shown}"))
}

/// One body cell. A value that is a whole web address links with its host as the text, and the
/// address stays in `href` and `title`; on paper the whole address is the text (`address`), so
/// the reference survives printing. ISO dates and identifiers never break. The authoring
/// re-render's payload keeps the source value; the sort value is the one shown (`sort_value`).
fn table_cell(value: &str, column: &Column, print: bool, sort: Option<&str>) -> String {
    let class = column_class(column);
    let sort = sort
        .filter(|_| !print)
        .map(|v| format!(" data-rv-sort-value=\"{}\"", escape(v)))
        .unwrap_or_default();
    if let Some(host) = web_address_host(value) {
        let address = escape(value);
        let kind = if print { "address" } else { "link" };
        let class = if class.is_empty() {
            kind.into()
        } else {
            format!("{class} {kind}")
        };
        let text = if print {
            address.clone()
        } else {
            escape(&host)
        };
        return format!(
            "<td class=\"{class}\"{sort}><a href=\"{address}\" title=\"{address}\">{text}</a></td>"
        );
    }
    let token = if !class.is_empty() {
        ""
    } else if iso_date(value) {
        "date"
    } else if identifier(value) {
        "id"
    } else {
        ""
    };
    format!(
        "<td class=\"{}\"{sort}>{}</td>",
        if token.is_empty() { class } else { token },
        escape(value)
    )
}

/// The host, without `www.`, of a value that is exactly one http(s) address.
fn web_address_host(value: &str) -> Option<String> {
    if !(value.starts_with("https://") || value.starts_with("http://"))
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?;
    Some(host.strip_prefix("www.").unwrap_or(host).to_owned())
}

fn iso_date(value: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d{4}-\d{2}(?:-\d{2}(?:[T ]\d{2}:\d{2}(?::\d{2}(?:\.\d+)?)?(?:Z|[+-]\d{2}:?\d{2})?)?)?$")
            .expect("static grammar")
    })
    .is_match(value)
}

/// A short single token with a digit or separator, such as `C001`, `D-ITUNES` or `v2.1`.
fn identifier(value: &str) -> bool {
    (2..=24).contains(&value.len())
        && value.parse::<f64>().is_err()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:#".contains(&b))
        && value
            .bytes()
            .any(|b| b.is_ascii_digit() || b"-_.".contains(&b))
}

/// The tag as a sentence start: `Chart`, `Table`, `Image`.
pub(crate) fn tag_title(tag: &str) -> String {
    let mut chars = tag.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The text of a resolved report file: a `data:` URI decoded, else the text as supplied.
fn resolved_text(options: &RenderOptions, reference: &str) -> Option<String> {
    use base64::Engine as _;
    let content = options.data.get(reference)?;
    match content.strip_prefix("data:") {
        Some(uri) => {
            let (_, body) = uri.split_once(";base64,")?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(body)
                .ok()?;
            String::from_utf8(bytes).ok()
        }
        None => Some(content.clone()),
    }
}

/// A captions file as a `data:` URI the `<track>` can load without the network.
fn captions_uri(options: &RenderOptions, reference: &str) -> Option<String> {
    use base64::Engine as _;
    let content = options.data.get(reference)?;
    if content.starts_with("data:text/vtt") {
        return (!content.contains(['<', '>', '"', '\'', ' '])).then(|| content.clone());
    }
    let text = resolved_text(options, reference)?;
    Some(format!(
        "data:text/vtt;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(text)
    ))
}

/// Transcript under the player in a native disclosure, open on paper (FEA-RP-55).
fn transcript(
    value: &Value,
    options: &RenderOptions,
    lang: &str,
    diagnostics: &mut Vec<Diagnostic>,
    line: usize,
    tag: &str,
) -> String {
    let Some(reference) = value.get("transcript").and_then(Value::as_str) else {
        return String::new();
    };
    let Some(text) = resolved_text(options, reference) else {
        diagnostics.push(
            Diagnostic::new(
                "error",
                "report.asset.missing",
                format!("Transcript '{reference}' could not be read, so it is not shown"),
                format!("{tag}.transcript"),
            )
            .at(line, tag),
        );
        return String::new();
    };
    let body = markdown_html(
        &text,
        options,
        &Context {
            heading_ids: &[],
            first_line: None,
            columns: false,
            path: "transcript",
            scope: &format!("t{line}-"),
            placeholder: "",
            lang,
        },
        diagnostics,
    );
    let open = if options.output == Output::Pdf {
        " open"
    } else {
        ""
    };
    format!(
        "<details class=\"rv-transcript\"{open}><summary>Transcript</summary><div class=\"rv-transcript__text\">{body}</div></details>"
    )
}

/// The notice that stands in for a media block that failed (FEA-RP-29).
#[allow(clippy::too_many_arguments)]
fn media_failed(
    diagnostics: &mut Vec<Diagnostic>,
    tag: &str,
    line: usize,
    code: &str,
    message: String,
    path: String,
    fix: &str,
    level: &str,
) -> String {
    diagnostics.push(
        Diagnostic::new(level, code, message.clone(), path)
            .fix(fix)
            .at(line, tag),
    );
    crate::render::notice(
        &format!("{} not rendered: {message}", tag_title(tag)),
        Some(line),
        None,
    )
}

pub(crate) fn render_media(
    tag: &str,
    value: &Value,
    options: &RenderOptions,
    lang: &str,
    diagnostics: &mut Vec<Diagnostic>,
    line: usize,
) -> String {
    let reference = value.get("src").and_then(Value::as_str).unwrap_or("");
    let src = options
        .data
        .get(reference)
        .map(String::as_str)
        .unwrap_or(reference);
    let hotlink = tag == "image" && crate::unsplash_image_url(src);
    if crate::markdown::remote_media(src) && !hotlink {
        return media_failed(
            diagnostics,
            tag,
            line,
            "report.asset.remote",
            format!(
                "'{reference}' is a remote address; the report loads only files from its folder"
            ),
            format!("{tag}.src"),
            "Download the file into the report folder and reference it by relative path",
            "error",
        );
    }
    if !safe_media(src, tag) {
        return media_failed(
            diagnostics,
            tag,
            line,
            "security.media",
            format!("the {tag} source is not a report file or a base64 data URI"),
            format!("{tag}.src"),
            "Use a file in the report folder",
            "warning",
        );
    }
    // A file the folder lacks, or one outside it, fails only this element (FEA-RP-30).
    if !hotlink
        && let Some((code, message)) =
            crate::folder::asset_problem(options.report_folder.as_ref(), reference)
    {
        return media_failed(
            diagnostics,
            tag,
            line,
            code,
            message,
            format!("{tag}.src"),
            "Add the file inside the report folder and reference it by relative path",
            "error",
        );
    }
    if hotlink {
        diagnostics.push(
            Diagnostic::new(
                "info",
                "assets.network-dependent",
                format!("{tag} source '{reference}' loads from the network"),
                format!("{tag}.src"),
            )
            .at(line, tag),
        );
    } else if !src.starts_with("data:") {
        diagnostics.push(Diagnostic::new("info","assets.external-reference",format!("{tag} source '{reference}' is not embedded; the report requires this external asset"),format!("{tag}.src")).at(line,tag));
    }
    let caption_text = value.get("caption").and_then(Value::as_str);
    let caption = caption_text
        .map(|v| format!("<figcaption>{}</figcaption>", escape(v)))
        .unwrap_or_default();
    let credit = if tag == "image" {
        render_credit(value, diagnostics, line)
    } else {
        String::new()
    };
    // An image's `alt` is its description (`""` marks it decorative); audio and video are named
    // by `title`.
    let label = if tag == "image" {
        value.get("alt")
    } else {
        value.get("title")
    }
    .and_then(Value::as_str)
    .unwrap_or("");
    let dimensions = ["width", "height"]
        .iter()
        .filter_map(|key| {
            value
                .get(*key)
                .and_then(Value::as_u64)
                .map(|n| format!(" {key}=\"{n}\""))
        })
        .collect::<String>();
    let poster = if tag == "video" {
        value.get("poster").and_then(Value::as_str).and_then(|reference| {
            let poster = options.data.get(reference).map(String::as_str).unwrap_or(reference);
            if crate::markdown::remote_media(poster) {
                diagnostics.push(Diagnostic::new("error","report.asset.remote",format!("The video poster '{reference}' is a remote address; the poster is left out"),"video.poster").fix("Download the file into the report folder").at(line,tag));
                None
            } else if !safe_media(poster,"image") {
                diagnostics.push(Diagnostic::new("warning","security.media","Blocked unsafe video poster source","video.poster").at(line,tag));
                None
            } else if let Some((code, message)) =
                crate::folder::asset_problem(options.report_folder.as_ref(), reference)
            {
                diagnostics.push(Diagnostic::new("error",code,format!("{message} The poster is left out."),"video.poster").at(line,tag));
                None
            } else {
                if !poster.starts_with("data:") {
                    diagnostics.push(Diagnostic::new("info","assets.external-reference",format!("Video poster '{reference}' is not embedded; the report requires this external asset"),"video.poster").at(line,tag));
                }
                Some(escape(poster))
            }
        })
    } else {
        None
    };
    let print = options.output == Output::Pdf;
    let transcript = transcript(value, options, lang, diagnostics, line, tag);
    // Paper cannot play: audio and video print as a poster frame or a text card naming the file.
    // The PDF output is paper, so it carries only the stand-in; a screen report carries it beside
    // the player and the print stylesheet swaps them (FEA-RP-34, FEA-RP-45).
    let standin = if tag == "image" {
        String::new()
    } else {
        let kind = if tag == "video" { "Video" } else { "Audio" };
        match poster.as_deref().filter(|_| tag == "video") {
            Some(poster) => format!(
                "<figure class=\"rv-video{}\"><img src=\"{poster}\" alt=\"{}\"{dimensions} loading=\"eager\">{caption}{}{transcript}</figure>",
                if print { "" } else { " rv-print-only" },
                escape(label),
                media_line(kind, "", reference)
            ),
            None => {
                if tag == "video" && print {
                    diagnostics.push(Diagnostic::new("warning","export.video-poster","Video has no poster, so the PDF prints a text card in its place; add a poster image showing a frame of the video","video.poster").at(line,tag));
                }
                let title = caption_text.unwrap_or(label);
                format!(
                    "{}{}",
                    media_card(tag, kind, title, reference, !print),
                    if print { transcript.as_str() } else { "" }
                )
            }
        }
    };
    if print && tag != "image" {
        return standin;
    }
    // The renderer alone marks a credited hotlink; sanitized Markdown HTML cannot carry data-*
    // attributes, so hosts that load photos (the PDF export) trust only this marker.
    let photo = if hotlink && has_credit(value) {
        " data-rv-photo=\"credited\""
    } else {
        ""
    };
    let poster = poster
        .map(|poster| format!(" poster=\"{poster}\""))
        .unwrap_or_default();
    // Captions are one embedded WebVTT track in the report language.
    let track = if tag == "video" {
        value
            .get("captions")
            .and_then(Value::as_str)
            .and_then(|reference| match captions_uri(options, reference) {
                Some(uri) => Some(format!(
                    "<track kind=\"captions\" src=\"{}\" srclang=\"{}\" label=\"Captions\" default>",
                    escape(&uri),
                    escape(lang)
                )),
                None => {
                    diagnostics.push(Diagnostic::new("error","report.asset.missing",format!("Captions '{reference}' could not be read, so the video has none"),"video.captions").at(line,tag));
                    None
                }
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    let (content, class) = match tag {
        "image" => (
            format!(
                "<img src=\"{}\"{photo} alt=\"{}\"{dimensions} style=\"{}object-fit:{}\" loading=\"{}\" decoding=\"async\">",
                escape(src),
                escape(label),
                image_box(value),
                value
                    .get("fit")
                    .and_then(Value::as_str)
                    .filter(|s| matches!(*s, "contain" | "cover" | "fill"))
                    .unwrap_or("contain"),
                if print { "eager" } else { "lazy" }
            ),
            wide(value, "rv-image--full"),
        ),
        // The file is embedded once, as the element's own source, with no fallback link.
        "audio" => (
            format!(
                "<audio controls preload=\"metadata\" aria-label=\"{}\" src=\"{}\"></audio>",
                escape(label),
                escape(src)
            ),
            String::new(),
        ),
        "video" => (
            format!(
                "<video controls preload=\"metadata\" aria-label=\"{}\"{dimensions}{poster} src=\"{}\">{track}</video>",
                escape(label),
                escape(src)
            ),
            wide(value, "rv-video--full"),
        ),
        _ => (String::new(), String::new()),
    };
    format!(
        "<figure class=\"rv-{tag}{class}\">{content}{caption}{credit}{transcript}</figure>{standin}"
    )
}

/// A photo's dominant colour: exactly `#rrggbb`. The renderer paints it behind the image, so
/// anything else (names, `rgb()`, alpha, CSS) is dropped rather than passed to a style attribute.
pub fn image_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Whether an image mapping names its photographer, the minimum a credit line needs.
pub(crate) fn has_credit(value: &Value) -> bool {
    value
        .pointer("/credit/author")
        .and_then(Value::as_str)
        .is_some_and(|author| !author.trim().is_empty())
}

/// The box a photo will fill, painted before and without the file: its aspect ratio from
/// `width`/`height` and its dominant colour, with a text colour that stays readable on it.
fn image_box(value: &Value) -> String {
    let size = |key: &str| value.get(key).and_then(Value::as_u64).filter(|n| *n > 0);
    let mut style = String::new();
    if let (Some(width), Some(height)) = (size("width"), size("height")) {
        style.push_str(&format!("aspect-ratio:{width}/{height};"));
    }
    if let Some(color) = value
        .get("color")
        .and_then(Value::as_str)
        .filter(|color| image_color(color))
    {
        let channel =
            |at: usize| u8::from_str_radix(&color[at..at + 2], 16).unwrap_or(0) as f64 / 255.0;
        let linear = |c: f64| {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let luminance =
            0.2126 * linear(channel(1)) + 0.7152 * linear(channel(3)) + 0.0722 * linear(channel(5));
        let ink = if luminance > 0.18 {
            "oklch(0 0 0)"
        } else {
            "oklch(1 0 0)"
        };
        style.push_str(&format!("background-color:{color};color:{ink};"));
    }
    style
}

/// The photo credit line of an image: "Photo by <author> on <source>", with each name linked
/// when its address is a plain http(s) URL. An Unsplash image without a credit never gets here:
/// parsing reports `image.credit-missing` as an error and the block stays source.
fn render_credit(value: &Value, diagnostics: &mut Vec<Diagnostic>, line: usize) -> String {
    let warn = |code: &str, message: String, path: &str| {
        Diagnostic::new("warning", code, message, path).at(line, "image")
    };
    let credit = value.get("credit");
    let field = |name: &str| {
        credit
            .and_then(|c| c.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    };
    if field("author").is_none() {
        if credit.is_some() {
            diagnostics.push(warn(
                "image.credit-incomplete",
                "image credit needs a non-empty author, so no credit is shown; write credit: {author: ...}".into(),
                "image.credit.author",
            ));
        }
        return String::new();
    }
    let mut named = |name: &str, key: &str| -> String {
        let text = field(name).unwrap_or_default();
        match field(key) {
            Some(url) if web_address_host(url).is_some() => format!(
                "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                escape(url),
                escape(text)
            ),
            Some(_) => {
                diagnostics.push(warn(
                    "image.credit-url",
                    format!(
                        "image credit {key} is not an http(s) address, so '{text}' is not linked"
                    ),
                    &format!("image.credit.{key}"),
                ));
                escape(text)
            }
            None => escape(text),
        }
    };
    let mut html = format!("Photo by {}", named("author", "author_url"));
    if field("source").is_some() {
        html.push_str(&format!(" on {}", named("source", "source_url")));
    }
    format!("<p class=\"rv-credit\">{html}</p>")
}

/// A printed stand-in for media that cannot play on paper: one line under a rule.
fn media_card(tag: &str, kind: &str, title: &str, reference: &str, print_only: bool) -> String {
    format!(
        "<figure class=\"rv-{tag} rv-media-card{}\">{}</figure>",
        if print_only { " rv-print-only" } else { "" },
        media_line(kind, title, reference)
    )
}

/// "Video · caption · address": a web address links, a report file is named, embedded data is
/// left out.
fn media_line(kind: &str, title: &str, reference: &str) -> String {
    let mut line = escape(kind);
    if !title.is_empty() {
        line.push_str(&format!(" · {}", escape(title)));
    }
    if reference.starts_with("https://") || reference.starts_with("http://") {
        line.push_str(&format!(" · <a href=\"{0}\">{0}</a>", escape(reference)));
    } else if !reference.is_empty() && !reference.starts_with("data:") {
        line.push_str(&format!(" · {}", escape(reference)));
    }
    format!("<p class=\"rv-media__line\">{line}</p>")
}

/// The default embed hosts; one list serves rendering and schema defaults (TYP-33).
const EMBED_DEFAULT: &[&str] = crate::registry::EMBED_HOSTS;

/// Why an embed is refused: its URL must be https, carry no credentials and sit on an allowed
/// host. `None` when it may be framed.
fn embed_refusal(value: &Value, url: &url::Url) -> Option<String> {
    let host = url.host_str().unwrap_or("");
    if url.scheme() != "https" {
        return Some(format!("the URL must be https, not {}", url.scheme()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Some("the URL carries a user name or password, which an embed never takes".into());
    }
    let allow = value
        .get("allow")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_else(|| EMBED_DEFAULT.to_vec());
    let listed = allow
        .iter()
        .any(|rule| !rule.is_empty() && (host == *rule || host.ends_with(&format!(".{rule}"))));
    (!listed).then(|| format!("host '{host}' is not allowed"))
}

fn embed_allowed(value: &Value, url: &url::Url) -> bool {
    embed_refusal(value, url).is_none()
}

/// The host of an embed the policy allows, for the report's frame policy.
pub(crate) fn embed_host(value: &Value) -> Option<String> {
    let url = url::Url::parse(value.get("url")?.as_str()?).ok()?;
    embed_allowed(value, &url)
        .then(|| url.host_str().map(str::to_owned))
        .flatten()
}

fn render_embed(
    value: &Value,
    options: &RenderOptions,
    diagnostics: &mut Vec<Diagnostic>,
    line: usize,
) -> String {
    let source = value.get("url").and_then(Value::as_str).unwrap_or("");
    let parsed = url::Url::parse(source).ok();
    let host = parsed
        .as_ref()
        .and_then(|url| url.host_str())
        .unwrap_or("")
        .to_owned();
    if !parsed.as_ref().is_some_and(|url| embed_allowed(value, url)) {
        let message = parsed
            .as_ref()
            .and_then(|url| embed_refusal(value, url))
            .unwrap_or_else(|| "the address is not a valid https URL".to_owned());
        return media_failed(
            diagnostics,
            "embed",
            line,
            "security.embed",
            message,
            "embed.url".into(),
            "Use an https URL on an allowed host, or list the host in `allow`",
            "error",
        );
    }
    // A PDF links to the embedded page instead of loading it.
    if options.output == Output::Pdf {
        let caption = value.get("caption").and_then(Value::as_str).unwrap_or("");
        return media_card("embed", "Embedded page", caption, source, false);
    }
    let height = value.get("height").and_then(Value::as_u64).unwrap_or(420);
    diagnostics.push(
        Diagnostic::new(
            "info",
            "assets.network-dependent",
            format!("Embedded content from '{host}' requires network access"),
            "embed.url",
        )
        .at(line, "embed"),
    );
    let caption = value
        .get("caption")
        .and_then(Value::as_str)
        .unwrap_or("Embedded content");
    // Known third-party players need their origin to work. Custom author allowlists do not
    // gain that permission: same-origin scripted frames could remove their own sandbox.
    let sandbox = if EMBED_DEFAULT
        .iter()
        .any(|rule| host == *rule || host.ends_with(&format!(".{rule}")))
    {
        "allow-scripts allow-same-origin allow-presentation"
    } else {
        "allow-scripts allow-presentation"
    };
    // The frame loads only when the reader asks (FEA-RP-56): until then a card names the host and
    // links to the page. The button appears once the runtime runs, so none is dead.
    // Paper cannot load a page: print swaps the card for a link card (FEA-RP-45).
    let standin = media_card(
        "embed",
        "Embedded page",
        value.get("caption").and_then(Value::as_str).unwrap_or(""),
        source,
        true,
    );
    let (source, host, caption) = (escape(source), escape(&host), escape(caption));
    format!(
        "<figure class=\"rv-embed{}\" data-rv-embed=\"{source}\" data-rv-height=\"{height}\" data-rv-sandbox=\"{sandbox}\" data-rv-title=\"{caption}\"><div class=\"rv-embed__card\"><p class=\"rv-embed__caption\">{caption}</p><p class=\"rv-embed__host\">{host}</p><p class=\"rv-embed__actions\"><button type=\"button\" class=\"rv-embed__load\" hidden>Load from {host}</button> <a class=\"rv-embed__link\" href=\"{source}\" rel=\"noopener noreferrer\">Open {host}</a></p></div></figure>{standin}",
        wide(value, "rv-embed--full")
    )
}

#[cfg(test)]
mod tests {
    use super::{TableMode, ValueFormat, format_value, key_order, signed_percent, table_html};
    use richview_chart::numbers;
    use serde_json::json;

    #[test]
    fn metrics_follow_the_unit_rule_and_the_three_digit_rule() {
        let f = |v: serde_json::Value, format, unit| {
            format_value(
                &v,
                &ValueFormat {
                    format,
                    unit,
                    ..Default::default()
                },
            )
        };
        // A percent prints its own `%` once; a `%` unit adds nothing.
        assert_eq!(f(json!(0.682), Some("percent"), Some("%")), "68.2%");
        // Numeric strings format like numbers; other text is shown as written.
        assert_eq!(
            f(json!("1234.5"), Some("number"), Some("users")),
            "1,235\u{a0}users"
        );
        assert_eq!(f(json!("n/a"), Some("percent"), Some("%")), "n/a");
        assert_eq!(
            f(json!(4234567), Some("number_compact"), Some("$")),
            "$4.23 million"
        );
        assert_eq!(
            format_value(
                &json!(1234.5),
                &ValueFormat {
                    digits: Some(5),
                    ..Default::default()
                }
            ),
            "1,234.5"
        );
    }

    #[test]
    fn total_takes_a_list_of_columns_and_skips_text_with_a_warning() {
        let table = json!({"total": ["units", "name"], "data": {"inline": [
            {"name": "a", "units": 1200}, {"name": "b", "units": 30}]}});
        let mut diagnostics = vec![];
        let html = table_html(&table, TableMode::Report, &mut diagnostics, 3);
        assert!(
            html.contains(
                "<tfoot><tr><td class=\"\">Total</td><td class=\"num\">1,230</td></tr></tfoot>"
            ),
            "{html}"
        );
        let warning = diagnostics
            .iter()
            .find(|d| d.code == "table.total")
            .unwrap();
        assert!(warning.message.contains("name") && warning.line == Some(3));
    }

    #[test]
    fn deltas_have_three_significant_digits_and_one_decimal_at_least() {
        for (fraction, shown) in [
            (0.04, "+4.0%"),
            (-0.02, "\u{2212}2.0%"),
            (0.0, "0.0%"),
            (0.12345, "+12.3%"),
            (1.234, "+123%"),
            (-0.0404, "\u{2212}4.04%"),
        ] {
            assert_eq!(
                signed_percent(fraction, numbers::DEFAULT_DIGITS, numbers::Locale::EnUs),
                shown
            );
        }
    }

    #[test]
    fn deltas_follow_the_metrics_digits_and_decimal_mark() {
        let cs = numbers::Locale::CsCz;
        for (fraction, digits, locale, shown) in [
            (0.12345, 1, numbers::Locale::EnUs, "+10%"),
            (1.234, 2, numbers::Locale::EnUs, "+120%"),
            (0.04, 1, numbers::Locale::EnUs, "+4%"),
            (0.04, 2, numbers::Locale::EnUs, "+4.0%"),
            (0.0996, 2, numbers::Locale::EnUs, "+10%"),
            (0.1234, 1, cs, "+10%"),
            (0.12345, 5, numbers::Locale::EnUs, "+12.345%"),
            (0.1234, 4, cs, "+12,34%"),
            (-0.0404, 3, numbers::Locale::DeDe, "\u{2212}4,04%"),
            (0.04, 3, cs, "+4,0%"),
            (12.0, 3, cs, "+1200%"),
        ] {
            assert_eq!(
                signed_percent(fraction, digits, locale),
                shown,
                "{fraction} at {digits}"
            );
        }
    }

    #[test]
    fn key_order_reads_json_rows_in_first_seen_order() {
        let json = r#"[{"b": 1, "a": {"nested": [1, 2]}}, {"c": null}]"#;
        assert_eq!(
            key_order(&mut serde_json::Deserializer::from_str(json), &["inline"]),
            ["b", "a", "c"]
        );
        let wrapped = r#"{"note": "x", "inline": [{"y": 1, "x": 2}]}"#;
        assert_eq!(
            key_order(
                &mut serde_json::Deserializer::from_str(wrapped),
                &["inline"]
            ),
            ["y", "x"]
        );
        for odd in ["[1, 2]", r#"{"rows": [{"a": 1}]}"#, "not json"] {
            assert!(
                key_order(&mut serde_json::Deserializer::from_str(odd), &["inline"]).is_empty(),
                "{odd}"
            );
        }
    }
}
