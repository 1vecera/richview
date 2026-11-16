use crate::{Diagnostic, Document, MAX_SOURCE_BYTES, RichBlock};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(crate) const BLOCK_PLACEHOLDER_PREFIX: &str = "<!--richview:block:";

const TAG_COUNT: usize = crate::registry::TAGS.len();
const TAG_NAMES: [&str; TAG_COUNT] = {
    let mut names = [""; TAG_COUNT];
    let mut i = 0;
    while i < TAG_COUNT {
        names[i] = crate::registry::TAGS[i].name;
        i += 1;
    }
    names
};

/// The fence tags that are rich blocks: the registry's tags (TYP-09).
pub const RICH_TAGS: &[&str] = &TAG_NAMES;

/// A JSON pointer from the chart engine's option checks in the path grammar (TYP-53):
/// `/data/inline/3/lower` is `data.inline[3].lower`.
fn pointer_path(pointer: &str) -> String {
    let mut path = String::new();
    for part in pointer.split('/').filter(|part| !part.is_empty()) {
        let part = part.replace("~1", "/").replace("~0", "~");
        if part.bytes().all(|b| b.is_ascii_digit()) {
            path.push_str(&format!("[{part}]"));
        } else {
            if !path.is_empty() {
                path.push('.');
            }
            path.push_str(&part);
        }
    }
    path
}

/// Parse drafts without requiring publication metadata. Invalid rich blocks remain visible as
/// source, with precise diagnostics; only resource limits are hard errors.
pub fn parse(source: &str) -> Result<Document, Vec<Diagnostic>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(vec![Diagnostic::new(
            "error",
            "source.limit",
            "The .rmd source is over the 16 MiB limit. Move large data into CSV or JSON files and split the report.",
            "source",
        )]);
    }
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    let mut doc = Document {
        frontmatter: json!({}),
        body: String::new(),
        blocks: vec![],
        diagnostics: vec![],
        block_placeholder_prefix: unique_placeholder_prefix(source),
        failed: vec![],
        body_line: 1,
        frontmatter_lines: Default::default(),
        frontmatter_columns: Default::default(),
    };
    let mut content = source;
    let mut body_line = 1;
    let mut lines = source.split_inclusive('\n');
    if lines
        .next()
        .is_some_and(|line| line.trim_end_matches(['\r', '\n']) == "---")
    {
        let opener_len = source.find('\n').map_or(source.len(), |p| p + 1);
        let mut offset = opener_len;
        let mut close = None;
        for line in source[opener_len..].split_inclusive('\n') {
            if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
                close = Some((offset, offset + line.len()));
                break;
            }
            offset += line.len();
        }
        if let Some((end, after)) = close {
            // The YAML starts on the line after the opening `---`.
            match crate::yaml::load(&source[opener_len..end]) {
                Ok(loaded) => match loaded.value {
                    Value::Object(fm) => {
                        // Positions are relative to the YAML, which starts on line 2.
                        doc.frontmatter_lines = loaded
                            .positions
                            .iter()
                            .map(|(key, pos)| (key.clone(), pos.line + 1))
                            .collect();
                        doc.frontmatter_columns = loaded
                            .positions
                            .iter()
                            .map(|(key, pos)| (key.clone(), pos.column))
                            .collect();
                        doc.frontmatter = Value::Object(fm);
                        let first = doc.diagnostics.len();
                        // Unknown keys warn and stay in the metadata (TYP-84); the field
                        // checks are `validate_frontmatter`'s until 6c-2.
                        let unknown = crate::walker::check(
                            crate::walker::Root::Frontmatter,
                            &mut doc.frontmatter,
                        );
                        doc.diagnostics.extend(unknown);
                        validate_frontmatter(&mut doc);
                        place(
                            &mut doc.diagnostics[first..],
                            "frontmatter",
                            &loaded.positions,
                            1,
                        );
                    }
                    Value::Null => {}
                    _ => doc.diagnostics.push(
                        Diagnostic::new(
                            "error",
                            "frontmatter.mapping",
                            "Frontmatter must be a mapping of fields such as `title: ...`",
                            "frontmatter",
                        )
                        .at(2, "frontmatter"),
                    ),
                },
                Err(error) => {
                    doc.diagnostics
                        .push(yaml_diagnostic(&error, "frontmatter", "frontmatter", 1))
                }
            }
            content = &source[after..];
            body_line = source[..after].bytes().filter(|c| *c == b'\n').count() + 1;
            doc.body_line = body_line;
        } else {
            doc.diagnostics.push(
                Diagnostic::new(
                    "warning",
                    "frontmatter.unclosed",
                    "Frontmatter has no closing ---; rendering it as Markdown",
                    "frontmatter",
                )
                .at(1, "frontmatter"),
            );
        }
    }

    // CommonMark decides which fences are actually code blocks (respecting indentation,
    // longer fences and nested code). A regex-only parser mistakenly consumes examples.
    // A retired tag is scanned too, so it gets its own error instead of passing as code.
    let scan_tags: Vec<&str> = RICH_TAGS
        .iter()
        .copied()
        .chain(crate::registry::RETIRED_TAGS.iter().map(|(old, _)| *old))
        .collect();
    let (found, near_misses) = crate::fences::scan(content, &scan_tags);
    let line_of =
        |offset: usize| body_line + content[..offset].bytes().filter(|b| *b == b'\n').count();
    for miss in near_misses {
        doc.diagnostics.push(
            Diagnostic::new(
                "warning",
                "block.unknown-tag",
                format!(
                    "`{}` is not a rich tag; did you mean `{}`?",
                    miss.info, miss.tag
                ),
                miss.info.clone(),
            )
            .fix(format!("Write ```{}", miss.tag))
            .at(line_of(miss.offset), &miss.info),
        );
    }
    let prefix = doc
        .block_placeholder_prefix
        .clone()
        .unwrap_or_else(|| BLOCK_PLACEHOLDER_PREFIX.to_owned());
    let failed_prefix = failed_prefix(&prefix);
    let mut cursor = 0;
    let mut line_cursor = 0;
    let mut line = body_line;
    for crate::fences::Fence {
        start,
        end,
        tag,
        raw,
        nested,
    } in found
    {
        doc.body.push_str(&content[cursor..start]);
        // Fences are ordered source offsets. Count each intervening source byte once,
        // including previous fence contents, while retaining absolute diagnostic lines.
        line += content[line_cursor..start]
            .bytes()
            .filter(|b| *b == b'\n')
            .count();
        line_cursor = start;
        // The body keeps the fence's line count, so a body line is a source line.
        let lines = content[start..end].bytes().filter(|b| *b == b'\n').count();
        // `shown` is false when the fence may hold credentials, so its notice never repeats them.
        let fail = |doc: &mut Document, message: String, shown: bool| {
            let index = doc.failed.len();
            doc.failed.push(crate::FailedBlock {
                tag: tag.clone(),
                source: if shown {
                    content[start..end].trim_end().to_owned()
                } else {
                    String::new()
                },
                line,
                message,
            });
            doc.body.push_str(&format!("\n{failed_prefix}{index}-->\n"));
            doc.body.push_str(&"\n".repeat(lines.saturating_sub(2)));
        };
        if nested {
            // Rich blocks sit only at the top level of the body (FEA-RP-53).
            let message = "a rich block cannot sit inside a list, quote or note; move it out to the top level".to_owned();
            doc.diagnostics.push(
                Diagnostic::new("error", "block.nested", &message, tag.as_str())
                    .fix("Move the block out of the list, quote or note")
                    .at(line, &tag),
            );
            fail(&mut doc, message, true);
            cursor = end;
            continue;
        }
        if let Some(replacement) = crate::registry::retired_tag(&tag) {
            // A retired tag never turns silently into a YAML code listing (TYP-09).
            let message = format!("The `{tag}` block is now `{replacement}`");
            doc.diagnostics.push(
                Diagnostic::new("error", "block.renamed", &message, tag.clone())
                    .at(line, &tag)
                    .fix(format!(
                        "Change the fence tag from `{tag}` to `{replacement}`."
                    )),
            );
            fail(&mut doc, message, true);
            cursor = end;
            continue;
        }
        match crate::yaml::load(&raw) {
            Ok(loaded) => {
                // Read before the walk drops retired request headers.
                let credentials = crate::document::holds_credentials(&loaded.value);
                let mut value = loaded.value;
                if tag == "chart" {
                    flow_rows(&mut value);
                }
                // The registry's fields first: unknown keys are named and dropped, and each
                // known value is checked against its type (TYP-10, TYP-11).
                let mut issues = crate::walker::check(crate::walker::Root::Block(&tag), &mut value);
                for issue in &mut issues {
                    issue.line = Some(line);
                    issue.tag = Some(tag.clone());
                }
                let walked: HashSet<String> = issues
                    .iter()
                    .filter(|d| d.level == "error")
                    .map(|d| d.path.clone())
                    .collect();
                // The bespoke rules add what the registry cannot say; a path the walk already
                // refused is not reported twice, nor is a plain schema error about a mapping
                // the walk refused a field of. Their own codes, such as image.credit-missing,
                // still say what the mapping lacks.
                let below = |path: &str| {
                    walked.iter().any(|w| {
                        w.strip_prefix(path)
                            .is_some_and(|rest| rest.starts_with(['.', '[']))
                    })
                };
                // A retired data key's finding already says which source to write, so a block it
                // leaves with no source is not reported again for lacking one.
                let data_path = format!("{tag}.data");
                let retired_data = issues.iter().any(|d| {
                    d.code == "data.local"
                        && d.path
                            .strip_prefix(&data_path)
                            .is_some_and(|rest| rest.starts_with('.'))
                });
                let sourceless = !value
                    .get("data")
                    .and_then(Value::as_object)
                    .is_some_and(|data| DATA_SOURCES.iter().any(|key| data.contains_key(*key)));
                issues.extend(validate_block(&tag, &value, line).into_iter().filter(|d| {
                    !walked.contains(&d.path)
                        && !(d.code == "block.schema" && below(&d.path))
                        && !(d.code == "data.local"
                            && d.path == data_path
                            && retired_data
                            && sourceless)
                }));
                // The YAML starts on the line after the fence.
                place(&mut issues, &tag, &loaded.positions, line);
                let first_error = issues
                    .iter()
                    .find(|d| d.level == "error")
                    .map(|d| d.message.clone());
                doc.diagnostics.extend(issues);
                if let Some(message) = first_error {
                    fail(&mut doc, message, !credentials);
                } else {
                    crate::compat::apply_defaults(&tag, &mut value);
                    let index = doc.blocks.len();
                    doc.blocks.push(RichBlock {
                        tag: tag.clone(),
                        value,
                        raw,
                        line,
                    });
                    doc.body.push_str(&format!("\n{prefix}{index}-->\n"));
                    doc.body.push_str(&"\n".repeat(lines.saturating_sub(2)));
                }
            }
            Err(error) => {
                let diagnostic = yaml_diagnostic(&error, &tag, "block", line);
                let message = diagnostic.message.clone();
                doc.diagnostics.push(diagnostic);
                fail(&mut doc, message, true);
            }
        }
        cursor = end;
    }
    doc.body.push_str(&content[cursor..]);
    Ok(doc)
}

/// A YAML failure as a diagnostic at its own line and column. `offset` is the source line
/// before the YAML's first line. Text that is not YAML at all keeps its family's code
/// (`frontmatter.yaml`, `block.yaml`); YAML outside the profile keeps `yaml.*` (TYP-02).
pub(crate) fn yaml_diagnostic(
    error: &crate::yaml::YamlError,
    root: &str,
    family: &str,
    offset: usize,
) -> Diagnostic {
    let code = if error.code == "yaml.syntax" {
        format!("{family}.yaml")
    } else {
        error.code.to_owned()
    };
    let message = if error.code == "yaml.syntax" {
        format!("Invalid YAML in `{root}`: {}", error.message)
    } else {
        error.message.clone()
    };
    let mut d = Diagnostic::new("error", &code, message, root).at(offset + error.pos.line, root);
    d.column = Some(error.pos.column);
    d
}

/// Point each diagnostic at its field: the longest prefix of its path (after `root.`) that the
/// YAML recorded, at that key's line and column. `offset` is the source line before the YAML's
/// first line. Diagnostics whose path names no recorded field keep their position (TYP-56).
pub(crate) fn place(
    diagnostics: &mut [Diagnostic],
    root: &str,
    positions: &std::collections::BTreeMap<String, crate::yaml::Pos>,
    offset: usize,
) {
    for d in diagnostics {
        let row_field = d
            .path
            .strip_prefix("rows[")
            .map(|rest| format!("data.inline[{rest}"));
        let Some(mut field) = row_field.as_deref().or_else(|| {
            d.path
                .strip_prefix(root)
                .and_then(|rest| rest.strip_prefix('.'))
        }) else {
            continue;
        };
        let pos = loop {
            if let Some(pos) = positions.get(field) {
                break Some(*pos);
            }
            match field.rfind(['.', '[']) {
                Some(cut) => field = &field[..cut],
                None => break None,
            }
        };
        if let Some(pos) = pos {
            d.line = Some(offset + pos.line);
            d.column = Some(pos.column);
            if d.tag.is_none() {
                d.tag = Some(root.to_owned());
            }
        }
    }
}

/// Point each `frontmatter.<field>` diagnostic at its field in the frontmatter (TYP-56).
pub(crate) fn place_frontmatter(document: &Document, diagnostics: &mut [Diagnostic]) {
    let positions = document
        .frontmatter_lines
        .iter()
        .map(|(field, line)| {
            (
                field.clone(),
                crate::yaml::Pos {
                    line: *line,
                    column: document
                        .frontmatter_columns
                        .get(field)
                        .copied()
                        .unwrap_or(1),
                },
            )
        })
        .collect();
    place(diagnostics, "frontmatter", &positions, 0);
}

/// Place late findings from resolved data and geometry at the unchanged authored YAML.
pub(crate) fn place_block(diagnostics: &mut [Diagnostic], block: &RichBlock) {
    let prefix = format!("{}.", block.tag);
    if !diagnostics.iter().any(|d| {
        d.file.is_none()
            && d.column.is_none()
            && (d.path.starts_with(&prefix) || d.path.starts_with("rows["))
    }) {
        return;
    }
    if let Ok(loaded) = crate::yaml::load(&block.raw) {
        // A diagnostic already placed in an external data file must keep that file's position.
        for d in diagnostics
            .iter_mut()
            .filter(|d| d.file.is_none() && d.column.is_none())
        {
            place(
                std::slice::from_mut(d),
                &block.tag,
                &loaded.positions,
                block.line,
            );
        }
    }
}

/// The keys of a block's one data source (TYP-35).
const DATA_SOURCES: [&str; 7] = ["inline", "csv", "json", "url", "sql", "metricflow", "cache"];

/// The marker of a rich fence that failed, beside the marker of one that became a block.
pub(crate) fn failed_prefix(block_prefix: &str) -> String {
    block_prefix.replacen("block:", "failed:", 1)
}

fn unique_placeholder_prefix(source: &str) -> Option<String> {
    if !source.contains(BLOCK_PLACEHOLDER_PREFIX) {
        return None;
    }
    const GENERATED_PREFIX: &str = "<!--richview:generated:";
    let mut used = HashSet::new();
    let mut remaining = source;
    while let Some(start) = remaining.find(GENERATED_PREFIX) {
        remaining = &remaining[start + GENERATED_PREFIX.len()..];
        let digits = remaining.bytes().take_while(u8::is_ascii_digit).count();
        if remaining[digits..].starts_with(":block:")
            && let Ok(index) = remaining[..digits].parse::<usize>()
        {
            used.insert(index);
        }
        remaining = &remaining[digits..];
    }
    let index = (0..=used.len())
        .find(|index| !used.contains(index))
        .unwrap();
    Some(format!("{GENERATED_PREFIX}{index}:block:"))
}

/// The frontmatter checks the registry cannot say: version grammar, date shape, language,
/// format marker, tabs and the header image. Types, unknown keys and retired names are the
/// walker's.
pub(crate) fn validate_frontmatter(doc: &mut Document) {
    let fm = doc
        .frontmatter
        .as_object_mut()
        .expect("frontmatter mapping");
    if let Some(version) = fm.get("version").and_then(Value::as_str)
        && let Some(problem) = crate::version_problem(version)
    {
        doc.diagnostics.push(
            Diagnostic::new(
                "warning",
                "frontmatter.version",
                format!("{problem} The version is ignored until it is fixed."),
                "frontmatter.version",
            )
            .expected(crate::VERSION_GRAMMAR),
        );
        fm.remove("version");
    }
    if let Some(slug) = fm.get("slug").and_then(Value::as_str)
        && let Some(problem) = crate::slug_problem(slug)
    {
        doc.diagnostics.push(
            Diagnostic::new(
                "warning",
                "frontmatter.slug",
                format!("{problem} Publishing fails until it is fixed."),
                "frontmatter.slug",
            )
            .expected(crate::SLUG_GRAMMAR),
        );
    }
    if let Some(date) = fm.get("date").and_then(Value::as_str)
        && !crate::compat::date_shape(date)
    {
        doc.diagnostics.push(
            Diagnostic::new(
                "warning",
                "frontmatter.date",
                format!(
                    "frontmatter.date '{date}' is not YYYY-MM-DD or RFC 3339; it renders as written"
                ),
                "frontmatter.date",
            )
            .expected("YYYY-MM-DD or an RFC 3339 timestamp"),
        );
    }
    if let Some(lang) = fm.get("lang").and_then(Value::as_str) {
        if !crate::compat::language_tag(lang) {
            doc.diagnostics.push(
                Diagnostic::new(
                    "warning",
                    "frontmatter.lang",
                    format!("frontmatter.lang '{lang}' is not a BCP 47 language tag; using en"),
                    "frontmatter.lang",
                )
                .expected("a BCP 47 tag such as en, cs or de-DE"),
            );
            fm.remove("lang");
        } else if !richview_chart::numbers::Locale::supported(lang) {
            doc.diagnostics.push(
                Diagnostic::new(
                    "warning",
                    "frontmatter.lang",
                    format!(
                        "frontmatter.lang '{lang}' sets the page language, but number formatting stays English"
                    ),
                    "frontmatter.lang",
                )
                .expected("en, cs, de or ja for formatting"),
            );
        }
    }
    if let Some(format) = fm.get("format").and_then(Value::as_str) {
        match crate::compat::format_major(format) {
            Some(major) if major <= crate::FORMAT_MAJOR => {}
            Some(major) => doc.diagnostics.push(
                Diagnostic::new(
                    "error",
                    "frontmatter.format",
                    format!(
                        "frontmatter.format '{format}' is newer than this renderer reads (rmd/{})",
                        crate::FORMAT_MAJOR
                    ),
                    "frontmatter.format",
                )
                .fix(format!(
                    "Update RichView, or write the report for rmd/{} (version {major} was refused).",
                    crate::FORMAT_MAJOR
                )),
            ),
            None => doc.diagnostics.push(
                Diagnostic::new(
                    "error",
                    "frontmatter.format",
                    format!("frontmatter.format '{format}' is not a format marker"),
                    "frontmatter.format",
                )
                .expected("rmd/1"),
            ),
        }
    }
    validate_tabs(fm, &mut doc.diagnostics);
    if let Some(image) = title_image(&doc.frontmatter) {
        doc.diagnostics
            .extend(image_diagnostics(&image, "frontmatter.title_image", None));
    }
}

fn validate_tabs(fm: &mut serde_json::Map<String, Value>, diagnostics: &mut Vec<Diagnostic>) {
    let Some(tabs) = fm.get("tabs") else {
        return;
    };
    let mut tabs_issue = |path: &str, msg: &str| {
        diagnostics.push(Diagnostic::new("warning", "frontmatter.tabs", msg, path));
    };
    let Some(spec) = tabs.as_object() else {
        tabs_issue(
            "frontmatter.tabs",
            "frontmatter.tabs must be a mapping with a panels array",
        );
        fm.remove("tabs");
        return;
    };
    let Some(panels) = spec.get("panels").and_then(Value::as_array) else {
        tabs_issue(
            "frontmatter.tabs.panels",
            "frontmatter.tabs.panels must be a non-empty array",
        );
        fm.remove("tabs");
        return;
    };
    if panels.is_empty() {
        tabs_issue(
            "frontmatter.tabs.panels",
            "frontmatter.tabs.panels must be a non-empty array",
        );
        fm.remove("tabs");
        return;
    }
    if let Some(export) = spec.get("export")
        && !matches!(export.as_str(), Some("all" | "active"))
    {
        tabs_issue(
            "frontmatter.tabs.export",
            "frontmatter.tabs.export must be \"all\" or \"active\"; rendering the body without tabs",
        );
        fm.remove("tabs");
        return;
    }
    if spec.get("default").is_some_and(|v| !v.is_string()) {
        tabs_issue(
            "frontmatter.tabs.default",
            "frontmatter.tabs.default must be a panel id string; rendering the body without tabs",
        );
        fm.remove("tabs");
        return;
    }
    let mut seen_ids: HashSet<String> = HashSet::new();
    for (index, panel) in panels.iter().enumerate() {
        let Some(panel) = panel.as_object() else {
            tabs_issue(
                &format!("frontmatter.tabs.panels[{index}]"),
                &format!("frontmatter.tabs.panels[{index}] must be a mapping"),
            );
            fm.remove("tabs");
            return;
        };
        for key in ["id", "label", "from"] {
            match panel.get(key).and_then(Value::as_str) {
                Some(value) if !value.trim().is_empty() => {}
                _ => {
                    tabs_issue(
                        &format!("frontmatter.tabs.panels[{index}].{key}"),
                        &format!(
                            "frontmatter.tabs.panels[{index}].{key} must be a non-empty string"
                        ),
                    );
                    fm.remove("tabs");
                    return;
                }
            }
        }
        // Ids name `#tab=<id>` links and element ids; anchors match `{#id}` heading attributes.
        for key in ["id", "from"] {
            if let Some(problem) = crate::slug_problem(panel[key].as_str().unwrap_or_default()) {
                tabs_issue(
                    &format!("frontmatter.tabs.panels[{index}].{key}"),
                    &format!("frontmatter.tabs.panels[{index}].{key}: {problem}"),
                );
                fm.remove("tabs");
                return;
            }
        }
        let id = panel["id"].as_str().unwrap_or_default().to_owned();
        if !seen_ids.insert(id) {
            tabs_issue(
                &format!("frontmatter.tabs.panels[{index}].id"),
                &format!("frontmatter.tabs.panels[{index}].id must be unique"),
            );
            fm.remove("tabs");
            return;
        }
    }
    if let Some(default) = spec.get("default").and_then(Value::as_str)
        && !seen_ids.contains(default)
    {
        tabs_issue(
            "frontmatter.tabs.default",
            &format!("frontmatter.tabs.default \"{default}\" does not match any panel id"),
        );
    }
}

/// Problems with `highlight` as `(path, message)` pairs. Only the shape is checked here:
/// the column must exist when inline rows are present (CSV/JSON columns are known only to the
/// host), and values that match no row simply highlight nothing.
fn highlight_issues(
    highlight: &Value,
    chart_type: Option<&str>,
    inline: Option<&Vec<Value>>,
) -> Vec<(String, String)> {
    let mut issues = vec![];
    let mut add = |key: &str, message: String| {
        let path = if key.is_empty() {
            "highlight".to_owned()
        } else {
            format!("highlight.{key}")
        };
        issues.push((path, message));
    };
    let Some(map) = highlight.as_object() else {
        add(
            "",
            "expected a mapping such as {by: field, include: [value], priority: [value]}".into(),
        );
        return issues;
    };
    if chart_type.is_some() && chart_type != Some("scatter") {
        add("", "highlights require type: scatter".into());
    }
    for key in map
        .keys()
        .filter(|key| !["by", "include", "priority"].contains(&key.as_str()))
    {
        add(key, "unknown key; expected by, include or priority".into());
    }
    match map.get("by") {
        None => add(
            "by",
            "required: name the column whose values `include` matches".into(),
        ),
        Some(Value::String(field)) if !field.is_empty() => {
            if let Some(rows) = inline.filter(|rows| !rows.is_empty()) {
                let mut columns: Vec<&str> = rows
                    .iter()
                    .filter_map(Value::as_object)
                    .flat_map(|row| row.keys().map(String::as_str))
                    .collect();
                columns.sort_unstable();
                columns.dedup();
                if !columns.contains(&field.as_str()) {
                    let shown = columns
                        .iter()
                        .take(8)
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ");
                    add(
                        "by",
                        format!(
                            "no column '{field}' in the inline data; columns: {shown}{}",
                            if columns.len() > 8 { ", ..." } else { "" }
                        ),
                    );
                }
            }
        }
        Some(_) => add("by", "expected a non-empty field-name string".into()),
    }
    for key in ["include", "priority"] {
        match map.get(key) {
            None if key == "include" => add(
                key,
                "required: list the values to label, for example [\"Alpha\", \"Beta\"]".into(),
            ),
            None => {}
            Some(Value::Array(values)) => {
                if values.iter().any(|value| {
                    !matches!(value, Value::String(_) | Value::Number(_) | Value::Bool(_))
                }) {
                    add(key, "expected an array of strings or numbers".into());
                } else if key == "include" && values.is_empty() {
                    add(key, "expected at least one value to highlight".into());
                }
            }
            Some(_) => add(key, "expected an array of strings or numbers".into()),
        }
    }
    issues
}

/// The encoding a flow written as `nodes` and `edges` reads its Table rows by.
fn flow_encoding(rows: &[Value]) -> Value {
    let has = |key: &str| rows.iter().any(|r| r.get(key).is_some());
    let mut encoding = serde_json::Map::new();
    encoding.insert("label".into(), "step".into());
    if has("from") {
        encoding.insert("source".into(), "from".into());
        encoding.insert("target".into(), "to".into());
    }
    if has("label") {
        encoding.insert("y".into(), "label".into());
    }
    Value::Object(encoding)
}

/// A flow chart written as `nodes` and `edges` brings its own data: one Table row per card and
/// per edge, read through a fixed encoding (FEA-CH-76). A block that also gives `data` or
/// `encoding` is left as written, and validation names the clash.
pub(crate) fn flow_rows(value: &mut Value) {
    let Some(map) = value.as_object() else {
        return;
    };
    if map.get("type").and_then(Value::as_str) != Some("flow")
        || !map.contains_key("nodes")
        || map.contains_key("data")
        || map.contains_key("encoding")
    {
        return;
    }
    let Some(rows) = richview_chart::layout::flow_table_rows(value) else {
        return;
    };
    let encoding = flow_encoding(&rows);
    if let Some(map) = value.as_object_mut() {
        map.insert("data".into(), serde_json::json!({ "inline": rows }));
        map.insert("encoding".into(), encoding);
    }
}

pub(crate) fn validate_block(tag: &str, value: &Value, line: usize) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    let Some(map) = value.as_object() else {
        return vec![
            Diagnostic::new(
                "error",
                "block.type",
                format!("{tag} block must be a YAML mapping"),
                tag,
            )
            .at(line, tag),
        ];
    };
    let mut issue = |key: &str, msg: &str| {
        // One finding per field: a later rule about a field an earlier rule refused says the
        // same thing again (a missing boxplot `y` is not also "requires a numeric field").
        let path = format!("{tag}.{key}");
        if diagnostics.iter().any(|d: &Diagnostic| d.path == path) {
            return;
        }
        diagnostics.push(
            Diagnostic::new("error", "block.schema", format!("{path}: {msg}"), path).at(line, tag),
        )
    };
    let string_keys: &[&str] = match tag {
        "chart" => &["title", "subtitle"],
        "metric" => &["label", "unit", "format", "delta_label"],
        "callout" => &["title"],
        "image" => &["alt", "caption"],
        "audio" | "video" => &["alt", "title", "caption", "poster"],
        "embed" => &["caption"],
        _ => &[],
    };
    for key in string_keys {
        if map.get(*key).is_some_and(|value| !value.is_string()) {
            issue(key, "expected a string");
        }
    }
    match tag {
        "chart" => {
            if !map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| crate::authoring::CHART_TYPES.contains(&kind))
            {
                let types = crate::authoring::CHART_TYPES;
                let (last, rest) = types.split_last().expect("chart types");
                issue("type", &format!("expected {} or {last}", rest.join(", ")));
            }
            let flow = map.get("type").and_then(Value::as_str) == Some("flow");
            for key in ["nodes", "edges", "groups", "direction"] {
                if map.contains_key(key) && !flow {
                    issue(key, &format!("only a flow chart takes {key}"));
                }
            }
            if flow && map.contains_key("nodes") {
                let rows = map
                    .get("data")
                    .and_then(|d| d.get("inline"))
                    .and_then(Value::as_array)
                    .map_or(&[][..], Vec::as_slice);
                if map.get("encoding") != Some(&flow_encoding(rows)) {
                    issue(
                        "nodes",
                        "a flow is written as nodes and edges or drawn from data through encoding; keep one",
                    );
                }
            } else if flow && map.contains_key("edges") {
                issue("edges", "edges join nodes; write the cards under nodes");
            }
            if !map.get("data").is_some_and(Value::is_object) {
                issue("data", "expected an inline or referenced data mapping");
            }
            if !map.get("encoding").is_some_and(Value::is_object) {
                issue("encoding", "expected data-channel mapping");
            }
            if let (Some(encoding), Some(kind)) = (
                map.get("encoding").and_then(Value::as_object),
                map.get("type")
                    .and_then(Value::as_str)
                    .and_then(crate::registry::chart_type),
            ) {
                // The channel matrix of the type (TYP-14): required channels, and no others.
                for channel in kind.required {
                    if !encoding.contains_key(*channel) {
                        issue(
                            &format!("encoding.{channel}"),
                            &format!(
                                "{} {} chart needs this channel",
                                article(kind.name),
                                kind.name
                            ),
                        );
                    }
                }
                for channel in encoding.keys() {
                    if !kind.encodings.contains(&channel.as_str()) {
                        // A channel few types read names them, so the author can pick the type
                        // the data asks for (`lower` belongs to an interval, not a line).
                        let readers: Vec<&str> = crate::registry::CHART_TYPES
                            .iter()
                            .filter(|t| t.encodings.contains(&channel.as_str()))
                            .map(|t| t.name)
                            .collect();
                        let read_by = match readers.as_slice() {
                            [] => String::new(),
                            [one] => format!("; {channel} is read by {one} charts"),
                            [init @ .., last] if readers.len() <= 5 => {
                                format!(
                                    "; {channel} is read by {} and {last} charts",
                                    init.join(", ")
                                )
                            }
                            _ => String::new(),
                        };
                        issue(
                            &format!("encoding.{channel}"),
                            &format!(
                                "{} {} chart takes {}{read_by}",
                                article(kind.name),
                                kind.name,
                                kind.encodings.join(", ")
                            ),
                        );
                    }
                }
            }
            if let Some(encoding) = map.get("encoding").and_then(Value::as_object) {
                for channel in [
                    "x", "y", "lower", "upper", "color", "size", "label", "facet",
                ] {
                    if encoding
                        .get(channel)
                        .is_some_and(|value| !value.is_string())
                    {
                        issue(
                            &format!("encoding.{channel}"),
                            "expected a field-name string",
                        );
                    }
                }
            }
            if let Some(highlight) = map.get("highlight") {
                for (path, message) in highlight_issues(
                    highlight,
                    map.get("type").and_then(Value::as_str),
                    map.get("data")
                        .and_then(|data| data.get("inline"))
                        .and_then(Value::as_array),
                ) {
                    issue(&path, &message);
                }
            }
            if map.get("stack").and_then(Value::as_bool) == Some(true) {
                let kind = map.get("type").and_then(Value::as_str);
                if !matches!(kind, Some("bar" | "area")) {
                    issue("stack", "stacking needs type: bar or area");
                } else if !map["encoding"].get("color").is_some_and(Value::is_string) {
                    issue("stack", "stacking needs encoding.color to name the series");
                }
            }
            if map.get("type").and_then(Value::as_str) == Some("histogram") {
                if let Err(error) = richview_chart::rmd_render::histogram_options(value) {
                    issue(&pointer_path(&error.path), &error.message);
                }
            } else if map.contains_key("binning") {
                issue("binning", "binning options require type: histogram");
            }
            if map.get("type").and_then(Value::as_str) == Some("boxplot")
                && let Err(error) = richview_chart::rmd_render::boxplot_options(value)
            {
                issue(&pointer_path(&error.path), &error.message);
            }
            // Bounds on a type that reads none are the channel matrix's finding, which names
            // the types that read them.
            if map.get("type").and_then(Value::as_str) == Some("interval")
                && let Err(error) = richview_chart::rmd_render::interval_options(value)
            {
                issue(&pointer_path(&error.path), &error.message);
            }
            if let Some(axes) = map.get("axes") {
                if !axes.is_object() {
                    issue("axes", "expected axis mapping");
                }
                for axis in ["x", "y", "color"] {
                    if let Some(axis_spec) = axes.get(axis) {
                        if !axis_spec.is_object() {
                            issue(&format!("axes.{axis}"), "expected axis options mapping");
                        }
                        if let Some(locale) = axis_spec.get("locale").and_then(Value::as_str)
                            && !["en-US", "en-GB", "cs-CZ", "de-DE", "ja-JP"].contains(&locale)
                        {
                            issue(
                                &format!("axes.{axis}.locale"),
                                "supported locales: en-US, en-GB, cs-CZ, de-DE, ja-JP",
                            );
                        }
                        for key in ["label", "format", "unit", "locale"] {
                            if axis_spec.get(key).is_some_and(|v| !v.is_string()) {
                                issue(&format!("axes.{axis}.{key}"), "expected a string");
                            }
                        }
                    }
                }
                if let Some(y) = axes.get("y") {
                    for key in ["min", "max"] {
                        if y.get(key)
                            .is_some_and(|v| !v.as_f64().is_some_and(f64::is_finite))
                        {
                            issue(&format!("axes.y.{key}"), "expected a number");
                        }
                    }
                    if let (Some(lo), Some(hi)) = (
                        y.get("min").and_then(Value::as_f64),
                        y.get("max").and_then(Value::as_f64),
                    ) && lo >= hi
                    {
                        issue("axes.y.min", "expected a value below axes.y.max");
                    }
                }
            }
            if let Some(style) = map.get("style") {
                if let Some(points) = style.get("points") {
                    let kind = map.get("type").and_then(Value::as_str);
                    // The registry checks the mode vocabulary; this rule checks eligibility.
                    if !matches!(kind, Some("line" | "area" | "scatter")) {
                        issue(
                            "style.points",
                            "point modes require type: line, area or scatter",
                        );
                    } else if kind == Some("scatter") && points.as_str() != Some("all") {
                        issue(
                            "style.points",
                            "scatter accepts all; omit points for the automatic policy",
                        );
                    }
                }
                if style.get("curve").is_some_and(|v| {
                    !matches!(
                        v.as_str(),
                        Some("linear" | "monotone" | "step" | "step_before" | "step_after")
                    )
                }) {
                    issue(
                        "style.curve",
                        "expected linear, monotone, step, step_before or step_after",
                    );
                }
                if style.get("curvature").is_some_and(|v| {
                    !v.as_f64()
                        .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n))
                }) {
                    issue("style.curvature", "expected a number between 0 and 1");
                }
                if !style.is_object() {
                    issue("style", "expected chart style mapping");
                }
                if style.get("palette").is_some_and(|v| {
                    !matches!(
                        v.as_str(),
                        Some("sleek" | "air" | "scientific" | "brutalist" | "hand-drawn")
                    )
                }) {
                    issue(
                        "style.palette",
                        "expected sleek, air, scientific, brutalist or hand-drawn",
                    );
                }
                if style.get("orientation").is_some_and(|v| {
                    !matches!(v.as_str(), Some("auto" | "vertical" | "horizontal"))
                }) {
                    issue("style.orientation", "expected auto, vertical or horizontal");
                }
                if style.get("ratio").is_some_and(|v| {
                    !v.is_string() && !v.as_f64().is_some_and(|n| n > 0.0 && n.is_finite())
                }) {
                    issue("style.ratio", "expected a positive number or W:H string");
                }
            }
            if let Some(annotations) = map.get("annotations") {
                if !annotations.as_array().is_some_and(|a| {
                    a.iter().all(|a| {
                        matches!(
                            a.get("kind").and_then(Value::as_str),
                            Some("point" | "range" | "line" | "band" | "text")
                        )
                    })
                }) {
                    issue(
                        "annotations",
                        "expected point, range, line, band or text annotations",
                    );
                }
                if let Some(annotations) = annotations.as_array() {
                    for (index, annotation) in annotations.iter().enumerate() {
                        let kind = annotation.get("kind").and_then(Value::as_str).unwrap_or("");
                        let required: &[&str] = if matches!(kind, "range" | "band") {
                            &["from", "to"]
                        } else {
                            &["at"]
                        };
                        for coordinate in required {
                            if !annotation
                                .get(*coordinate)
                                .and_then(Value::as_object)
                                .is_some_and(|m| m.values().all(|v| v.is_string() || v.is_number()))
                            {
                                issue(
                                    &format!("annotations[{index}].{coordinate}"),
                                    "expected a string/number coordinate mapping",
                                );
                            }
                        }
                        if annotation.get("text").is_some_and(|v| !v.is_string())
                            || matches!(kind, "point" | "text")
                                && !annotation.get("text").is_some_and(Value::is_string)
                        {
                            issue(
                                &format!("annotations[{index}].text"),
                                "expected annotation text string",
                            );
                        }
                        if annotation.get("rotate").is_some() && kind != "text" {
                            issue(
                                &format!("annotations[{index}].rotate"),
                                "rotate applies to text annotations only",
                            );
                        }
                        if annotation.get("rotate").is_some_and(|v| !v.is_number()) {
                            issue(
                                &format!("annotations[{index}].rotate"),
                                "expected rotation number",
                            );
                        }
                    }
                }
            }
        }
        "table" => {
            if !map.get("data").is_some_and(Value::is_object) {
                issue("data", "expected data mapping");
            }
            if map
                .get("max_rows")
                .is_some_and(|v| v.as_u64().is_none_or(|n| n == 0))
            {
                issue("max_rows", "expected a positive integer");
            }
            if map.get("columns").is_some_and(|v| {
                !v.as_array().is_some_and(|cols| {
                    cols.iter()
                        .all(|c| c.get("key").is_some_and(Value::is_string))
                })
            }) {
                issue("columns", "every column needs a string key");
            }
            if let Some(columns) = map.get("columns").and_then(Value::as_array) {
                for (index, column) in columns.iter().enumerate() {
                    for key in ["label", "format"] {
                        if column.get(key).is_some_and(|v| !v.is_string()) {
                            issue(&format!("columns[{index}].{key}"), "expected string");
                        }
                    }
                    if column
                        .get("align")
                        .is_some_and(|v| !matches!(v.as_str(), Some("left" | "center" | "right")))
                    {
                        issue(
                            &format!("columns[{index}].align"),
                            "expected left, center or right",
                        );
                    }
                }
            }
            if let Some(sort) = map.get("sort") {
                if !sort.get("key").is_some_and(Value::is_string) {
                    issue("sort.key", "expected column key string");
                }
                if sort
                    .get("dir")
                    .is_some_and(|v| !matches!(v.as_str(), Some("asc" | "desc")))
                {
                    issue("sort.dir", "expected asc or desc");
                }
            }
            if map.get("total").is_some_and(|v| {
                !v.is_boolean()
                    && !v
                        .as_array()
                        .is_some_and(|keys| keys.iter().all(Value::is_string))
            }) {
                issue("total", "expected true or a list of column keys");
            }
        }
        "metric" => {
            if !map
                .get("value")
                .is_some_and(|v| v.is_number() || v.is_string())
            {
                issue("value", "expected number or string");
            }
            if map
                .get("trend")
                .is_some_and(|v| !matches!(v.as_str(), Some("up" | "down" | "flat")))
            {
                issue("trend", "expected up, down or flat");
            }
            if map.get("delta").is_some_and(|v| !v.is_number()) {
                issue("delta", "expected a number");
            }
        }
        "callout" => {
            if !matches!(
                map.get("kind").and_then(Value::as_str),
                Some("up" | "down" | "neutral" | "hold")
            ) {
                issue("kind", "expected up, down, neutral or hold");
            }
            if !map.get("body").is_some_and(Value::is_string) {
                issue("body", "expected Markdown string");
            }
        }
        "image" | "audio" | "video" => {
            if !map.get("src").is_some_and(Value::is_string) {
                issue("src", "expected source string");
            }
            for dim in ["width", "height"] {
                if map
                    .get(dim)
                    .is_some_and(|v| v.as_u64().is_none_or(|n| n == 0))
                {
                    issue(dim, "expected positive integer");
                }
            }
            if tag == "image"
                && map
                    .get("fit")
                    .is_some_and(|v| !matches!(v.as_str(), Some("contain" | "cover" | "fill")))
            {
                issue("fit", "expected contain, cover or fill");
            }
            if tag == "image"
                && let Some(credit) = map.get("credit")
            {
                match credit.as_object() {
                    None => issue("credit", "expected a mapping with author"),
                    Some(credit) => {
                        // On an Unsplash photo the missing credit gets its own error below.
                        if !crate::blocks::has_credit(value)
                            && !map
                                .get("src")
                                .and_then(Value::as_str)
                                .is_some_and(crate::unsplash_image_url)
                        {
                            issue("credit.author", "expected a non-empty string");
                        }
                        for key in ["author_url", "source", "source_url"] {
                            if credit.get(key).is_some_and(|v| !v.is_string()) {
                                issue(&format!("credit.{key}"), "expected a string");
                            }
                        }
                    }
                }
            }
        }
        "scrolly" => {
            for (key, message) in crate::scrolly::validate(map) {
                issue(&key, &message);
            }
        }
        "embed" => {
            if !map
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|v| url::Url::parse(v).is_ok())
            {
                issue("url", "expected absolute URL");
            }
            if map
                .get("allow")
                .is_some_and(|v| !v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
            {
                issue("allow", "expected host-name array");
            }
            if map
                .get("height")
                .is_some_and(|v| v.as_u64().is_none_or(|n| n == 0))
            {
                issue("height", "expected positive integer");
            }
        }
        _ => {}
    }
    if let Some(data) = map.get("data") {
        // A row that is not a mapping is data.inline at that row, below.
        if data.get("inline").is_some_and(|v| !v.is_array()) {
            issue("data.inline", "expected array of rows");
        }
        for key in ["id", "csv", "json", "url"] {
            if data.get(key).is_some_and(|v| !v.is_string()) {
                issue(&format!("data.{key}"), "expected string");
            }
        }
        for key in ["sql", "metricflow", "cache"] {
            if data.get(key).is_some_and(|v| !v.is_object()) {
                issue(&format!("data.{key}"), "expected mapping");
            }
        }
        if let Some(sql) = data.get("sql").and_then(Value::as_object) {
            if !sql.get("connector").is_some_and(Value::is_string) {
                issue("data.sql.connector", "expected named connector");
            }
            let has_query = sql.get("query").is_some_and(Value::is_string);
            let has_inline = sql.get("inline").is_some_and(Value::is_string);
            if has_query == has_inline {
                issue(
                    "data.sql",
                    "set exactly one of query (report-relative file) or inline (SQL text)",
                );
            }
        }
        if let Some(mf) = data.get("metricflow").and_then(Value::as_object) {
            if !mf.get("connector").is_some_and(Value::is_string) {
                issue("data.metricflow.connector", "expected named connector");
            }
            if !mf.get("metrics").is_some_and(|v| {
                v.as_array()
                    .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
            }) {
                issue(
                    "data.metricflow.metrics",
                    "expected non-empty array of metric names",
                );
            }
        }
        if let Some(c) = data.get("cache").and_then(Value::as_object)
            && !c.get("key").is_some_and(Value::is_string)
        {
            issue("data.cache.key", "expected string");
        }
    }
    if let Some(data) = map.get("data") {
        if let Some(rows) = data.get("inline").and_then(Value::as_array)
            && let Some((row, message)) = crate::blocks::flat_rows_problem(rows)
        {
            // At the row as written (TYP-56).
            diagnostics.push(
                Diagnostic::new(
                    "error",
                    "data.inline",
                    message,
                    format!("{tag}.data.inline[{row}]"),
                )
                .at(line, tag),
            );
        }
        if DATA_SOURCES
            .iter()
            .filter(|key| data.get(**key).is_some())
            .count()
            != 1
        {
            diagnostics.push(Diagnostic::new("error","data.local","Choose exactly one block-local data source: inline observations, a csv/json file, an https url, a sql/metricflow connector query, or a cache reference.",format!("{tag}.data")).at(line,tag));
        }

        if let Some(url) = data.get("url").and_then(Value::as_str) {
            if !url.starts_with("https://") {
                diagnostics.push(
                    Diagnostic::new(
                        "error",
                        "block.schema",
                        format!("{tag}.data.url: expected an absolute https address"),
                        format!("{tag}.data.url"),
                    )
                    .at(line, tag),
                );
            } else if crate::document::has_userinfo(url) {
                // As an embed never takes one (ARC-14): readers would receive the credential.
                diagnostics.push(
                    Diagnostic::new(
                        "error",
                        "block.schema",
                        format!("{tag}.data.url carries a user name or password, which a data address never takes"),
                        format!("{tag}.data.url"),
                    )
                    .fix("Remove the user name and password: use an address that needs none, or save the data in the report folder and use data.csv or data.json")
                    .at(line, tag),
                );
            }
        }
        for key in ["csv", "json"] {
            if let Some(path) = data.get(key).and_then(Value::as_str)
                && !crate::report_relative_path(path)
            {
                diagnostics.push(Diagnostic::new("error", "report.asset.escape", format!("Data path '{path}' escapes the report folder. Copy the file into this report's data subfolder and use a relative path."), format!("{tag}.data.{key}")).at(line,tag));
            }
        }
    }
    if tag == "image" {
        diagnostics.extend(image_diagnostics(value, "image", Some(line)));
    }
    diagnostics
}

/// The image mapping of the report header, `title_image`, a bare source string standing for
/// `{src}`.
pub(crate) fn title_image(frontmatter: &Value) -> Option<Value> {
    let image = frontmatter.get("title_image")?;
    match image {
        Value::String(src) => Some(json!({"src": src})),
        Value::Object(_) => Some(image.clone()),
        _ => None,
    }
}

/// Whether `url` is an `https://unsplash.com/...` page: exact lowercase host, no userinfo, port,
/// whitespace, control characters or backslashes.
fn unsplash_page_url(url: &str) -> bool {
    url.strip_prefix("https://unsplash.com")
        .is_some_and(|rest| rest.starts_with(['/', '?']))
        && !url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\\')
        && url::Url::parse(url).is_ok_and(|url| {
            url.host_str() == Some("unsplash.com")
                && url.port().is_none()
                && url.username().is_empty()
                && url.password().is_none()
        })
}

/// What is wrong with the credit of an Unsplash photo, as (credit key, message) pairs. The
/// photographer link is required and, like the optional source link, must be on
/// `https://unsplash.com/`; the optional source must name Unsplash. Credits of other images keep
/// the lenient rules of the renderer, which only links plain http(s) addresses.
fn unsplash_credit_problems(image: &Value) -> Vec<(&'static str, &'static str)> {
    let credit = image.get("credit");
    let get = |key: &str| credit.and_then(|c| c.get(key));
    let mut problems = vec![];
    match get("author_url") {
        None | Some(Value::Null) => problems.push((
            "author_url",
            "Unsplash credit needs author_url, the photographer's https://unsplash.com/@name profile",
        )),
        Some(url) if !url.as_str().is_some_and(unsplash_page_url) => problems.push((
            "author_url",
            "Unsplash credit author_url must be an https://unsplash.com/... URL (lowercase host, no userinfo or port)",
        )),
        Some(_) => {}
    }
    if get("source_url")
        .is_some_and(|url| !url.is_null() && !url.as_str().is_some_and(unsplash_page_url))
    {
        problems.push((
            "source_url",
            "Unsplash credit source_url must be an https://unsplash.com/... URL (lowercase host, no userinfo or port)",
        ));
    }
    if get("source").is_some_and(|source| {
        !source.is_null()
            && !source
                .as_str()
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("Unsplash"))
    }) {
        problems.push((
            "source",
            "Unsplash credit source must be Unsplash, so leave it out or set it to Unsplash",
        ));
    }
    problems
}

/// Checks that apply to an image block and to the header image alike. A photo on the one
/// hotlink origin must carry a credit pinned to unsplash.com (an error: the block stays source
/// and does not render), and a `color` that is not `#rrggbb` is dropped with a warning.
pub(crate) fn image_diagnostics(image: &Value, path: &str, line: Option<usize>) -> Vec<Diagnostic> {
    let at = |diagnostic: Diagnostic| match line {
        Some(line) => diagnostic.at(line, "image"),
        None => diagnostic,
    };
    let mut diagnostics = vec![];
    if image
        .get("src")
        .and_then(Value::as_str)
        .is_some_and(crate::unsplash_image_url)
    {
        if !crate::blocks::has_credit(image) {
            diagnostics.push(at(Diagnostic::new(
                "error",
                "image.credit-missing",
                "Unsplash image has no credit, so it is not shown; add credit: {author, author_url, source, source_url} so the report reads \"Photo by <name> on Unsplash\" (richview image use writes it)",
                format!("{path}.credit"),
            )));
        } else {
            for (key, message) in unsplash_credit_problems(image) {
                diagnostics.push(at(Diagnostic::new(
                    "error",
                    "image.credit-invalid",
                    format!("{message}, so the Unsplash image is not shown"),
                    format!("{path}.credit.{key}"),
                )));
            }
        }
    }
    if image
        .get("color")
        .is_some_and(|color| !color.as_str().is_some_and(crate::image_color))
    {
        diagnostics.push(at(Diagnostic::new(
            "warning",
            "image.color-invalid",
            "image color must be a #rrggbb hex colour, so it is dropped",
            format!("{path}.color"),
        )));
    }
    diagnostics
}

pub fn validate(source: &str) -> Vec<Diagnostic> {
    match parse(source) {
        Ok(document) => document.diagnostics,
        Err(diagnostics) => diagnostics,
    }
}

/// Publication needs a title, a description and a version (TYP-07); local drafts never do.
pub fn validate_publish(source: &str) -> Vec<Diagnostic> {
    match parse(source) {
        Err(d) => d,
        Ok(doc) => {
            let mut d = doc.diagnostics.clone();
            let version_diagnosed = d
                .iter()
                .any(|x| x.path == "frontmatter.version" && x.code.starts_with("frontmatter."));
            for key in ["title", "description", "version"] {
                if !doc
                    .frontmatter
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
                {
                    let message = if key == "version" && version_diagnosed {
                        "Publishing needs a valid frontmatter.version; fix the version problem reported above.".to_owned()
                    } else {
                        format!("Publishing requires frontmatter.{key}")
                    };
                    d.push(Diagnostic::new(
                        "error",
                        "publish.metadata",
                        message,
                        format!("frontmatter.{key}"),
                    ));
                }
            }
            if let Err(diagnostics) = crate::compat::resolve_slug(&doc, None) {
                // A missing title is already reported above; the slug has nothing to add then.
                let no_slug_source =
                    doc.frontmatter.get("slug").is_none() && doc.frontmatter.get("title").is_none();
                if !(diagnostics[0].path == "frontmatter.slug" && no_slug_source) {
                    d.extend(diagnostics);
                }
            }
            d
        }
    }
}

/// `a` or `an` before a chart type's name: "an interval chart".
fn article(name: &str) -> &'static str {
    if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

#[cfg(test)]
mod tests {
    use super::{pointer_path, validate_block};
    use crate::{authoring_keys, authoring_values, parse_schema};
    use serde_json::{Value, json};

    fn scatter(highlight: Value) -> Value {
        json!({
            "type": "scatter",
            "title": "Labelled",
            "data": {"inline": [{"x": 1, "y": 2, "name": "Alpha", "group": "A"}, {"x": 2, "y": 3, "name": "Beta", "group": "B"}]},
            "encoding": {"x": "x", "y": "y", "label": "name"},
            "highlight": highlight,
        })
    }

    fn messages(chart: &Value) -> Vec<String> {
        validate_block("chart", chart, 7)
            .into_iter()
            .map(|d| format!("{}: {}", d.path, d.message))
            .collect()
    }

    #[test]
    fn chart_engine_pointers_read_in_the_path_grammar() {
        assert_eq!(pointer_path("/style/orientation"), "style.orientation");
        assert_eq!(pointer_path("/data/inline/3/lower"), "data.inline[3].lower");
        assert_eq!(pointer_path("/panes/0/series/12"), "panes[0].series[12]");
        assert_eq!(pointer_path("/data/inline/1/a~1b"), "data.inline[1].a/b");
        // The interval check names the offending axis.
        assert!(messages(&json!({"type": "interval", "title": "T", "encoding": {"x": "k", "y": "v", "lower": "lo", "upper": "hi"}, "axes": {"y": {"min": 5, "max": 1}}}))
            .iter()
            .any(|m| m.starts_with("chart.axes.y: ")));
    }

    #[test]
    fn a_well_formed_highlight_is_accepted_and_survives_schema_parsing() {
        let chart = scatter(json!({"by": "group", "include": ["a", 2, true], "priority": ["A"]}));
        assert_eq!(messages(&chart), Vec::<String>::new());
        let parsed = parse_schema("chart", chart.clone()).unwrap();
        assert_eq!(parsed["highlight"], chart["highlight"]);
        assert!(authoring_keys("chart", &[]).unwrap().contains(&"highlight"));
        assert_eq!(
            authoring_keys("chart", &["highlight"]).unwrap(),
            ["by", "include", "priority"]
        );
    }

    #[test]
    fn malformed_highlights_name_the_key_and_the_fix() {
        let cases = [
            (json!("Alpha"), "highlight", "expected a mapping"),
            (json!({"include": ["Alpha"]}), "highlight.by", "required"),
            (
                json!({"by": 3, "include": ["Alpha"]}),
                "highlight.by",
                "field-name string",
            ),
            (
                json!({"by": "nmae", "include": ["Alpha"]}),
                "highlight.by",
                "no column 'nmae' in the inline data; columns: group, name, x, y",
            ),
            (json!({"by": "name"}), "highlight.include", "required"),
            (
                json!({"by": "name", "include": []}),
                "highlight.include",
                "at least one",
            ),
            (
                json!({"by": "name", "include": "Alpha"}),
                "highlight.include",
                "array",
            ),
            (
                json!({"by": "name", "include": ["Alpha"], "priority": [["Alpha"]]}),
                "highlight.priority",
                "array of strings or numbers",
            ),
            (
                json!({"by": "name", "include": [null]}),
                "highlight.include",
                "array of strings or numbers",
            ),
            (
                json!({"by": "name", "include": ["Alpha"], "order": []}),
                "highlight.order",
                "unknown key",
            ),
        ];
        for (highlight, path, needle) in cases {
            let found = messages(&scatter(highlight.clone()));
            assert!(
                found.iter().any(|m| m.contains(path) && m.contains(needle)),
                "{highlight}: expected {path} / {needle}, got {found:?}"
            );
        }
        let mut line = scatter(json!({"by": "name", "include": ["Alpha"]}));
        line["type"] = json!("line");
        assert!(
            messages(&line)
                .iter()
                .any(|m| m.contains("require type: scatter"))
        );
    }

    #[test]
    fn highlight_columns_are_not_checked_without_inline_rows() {
        let mut chart = scatter(json!({"by": "name", "include": ["Alpha"]}));
        chart["data"] = json!({"csv": "points.csv"});
        assert_eq!(messages(&chart), Vec::<String>::new());
    }

    #[test]
    fn the_scatter_highlights_example_validates_and_renders_its_named_points() {
        let source = include_str!("../../../examples/scatter-highlights/report.rmd");
        assert!(
            !crate::validate(source).iter().any(|d| d.level == "error"),
            "{:?}",
            crate::validate(source)
        );
        let options = crate::RenderOptions {
            report_folder: Some(crate::ReportFolder {
                root: "scatter-highlights".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        };
        let report = crate::check(source, &options).unwrap();
        assert!(
            !report.diagnostics.iter().any(|d| d.level == "error"),
            "{:?}",
            report.diagnostics
        );
        let rendered = crate::render(source, &options).unwrap();
        let payload: richview_chart::payload::Payload =
            serde_json::from_value(rendered.charts[1].clone()).unwrap();
        let block = richview_chart::block::ChartBlock::from_value(&payload.block).unwrap();
        let geometry = richview_chart::layout::layout(
            &block,
            &payload.objects(),
            &payload.theme,
            &richview_chart::layout::Options {
                width: 832.0,
                ..Default::default()
            },
        );
        let labels: Vec<&str> = geometry
            .items()
            .filter_map(|item| match item {
                richview_chart::geometry::Item::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        // Highlighted points keep the spelling of the data, not of `include` ("MILL YARD").
        for name in [
            "Harbour Quay",
            "Harbour Row",
            "Mill Yard",
            "Ridge Cross",
            "Stone Gap",
        ] {
            assert!(labels.contains(&name), "{name} is labelled: {labels:?}");
        }
    }

    #[test]
    fn axes_columns_and_metrics_offer_one_format_vocabulary_with_year() {
        let formats = authoring_values("chart", &["axes", "x", "format"]);
        assert!(formats.contains(&"year"), "{formats:?}");
        for axis in ["y", "y2", "color"] {
            assert_eq!(
                authoring_values("chart", &["axes", axis, "format"]),
                formats,
                "{axis}"
            );
        }
        assert_eq!(authoring_values("table", &["columns", "format"]), formats);
        assert_eq!(authoring_values("metric", &["format"]), formats);
    }
}
