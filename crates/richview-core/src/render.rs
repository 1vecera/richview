use crate::{
    CheckReport, Diagnostic, Output, RenderOptions, RenderedReport, VERSION, escape, json_script,
    parse,
};
use crate::{blocks, figure, markdown, reading, shell, style};
use serde_json::{Value, json};
use std::fmt::Write as _;

/// Generate a self-contained report. Recoverable errors remain visible and are returned as
/// diagnostics; resource-limit errors use `Err`. No filesystem or remote fetch is performed.
pub fn render(source: &str, options: &RenderOptions) -> Result<RenderedReport, Vec<Diagnostic>> {
    render_internal(source, options, false).map(|(report, _, _)| report)
}

/// Render an already-parsed document without reparsing or copying its AST. Native callers can
/// resolve file references between [`parse`] and this operation. The AST is trusted caller input;
/// source validation and resource limits are established by [`parse`].
pub fn render_document(
    document: crate::Document,
    options: &RenderOptions,
) -> Result<RenderedReport, Vec<Diagnostic>> {
    render_document_internal(document, options, false).map(|(report, _, _)| report)
}

/// Render once and return geometry-aware checks with source provenance. Structural validation
/// alone cannot establish readability; the chart engine reports its emitted geometry here.
pub fn check(source: &str, options: &RenderOptions) -> Result<CheckReport, Vec<Diagnostic>> {
    check_document(parse(source)?, options)
}

/// Check an already-parsed, trusted document using the same geometry-aware render pipeline.
pub fn check_document(
    mut document: crate::Document,
    options: &RenderOptions,
) -> Result<CheckReport, Vec<Diagnostic>> {
    report_guidance(&mut document);
    render_document_internal(document, options, false).map(checked)
}

/// Findings that help authors complete a report, outside the reader's rendered content.
fn report_guidance(document: &mut crate::Document) {
    use pulldown_cmark::{Event, Parser, Tag};
    let prefix = document
        .block_placeholder_prefix
        .as_deref()
        .unwrap_or(crate::parser::BLOCK_PLACEHOLDER_PREFIX);
    let noted = markdown::noted_blocks(&document.body, prefix);
    if let Some(image) = document
        .frontmatter
        .get("title_image")
        .filter(|v| !v.is_null())
    {
        let caption = image
            .get("caption")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty());
        let credit = image
            .get("credit")
            .and_then(Value::as_object)
            .is_some_and(|v| !v.is_empty());
        if !caption && !credit {
            let mut d = Diagnostic::new(
                "info",
                "image.context",
                "The header image has neither a caption nor a credit",
                "frontmatter.title_image",
            )
            .fix("Add a caption or credit describing the header image");
            d.line = document.frontmatter_lines.get("title_image").copied();
            d.tag = Some("frontmatter".into());
            document.diagnostics.push(d);
        }
    }
    for (index, block) in document.blocks.iter().enumerate() {
        let has_text = |key: &str| {
            block
                .value
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty())
        };
        let mut issue = |level, code, message: &str, path: String, fix: &str| {
            document.diagnostics.push(
                Diagnostic::new(level, code, message, path)
                    .at(block.line, &block.tag)
                    .fix(fix),
            );
        };
        if matches!(block.tag.as_str(), "chart" | "table") && !noted.contains(&index) {
            issue(
                "warning",
                "block.source",
                "This block has no source note",
                block.tag.clone(),
                "Add a source note directly after the block, such as *Source: data/sales.csv.*; name synthetic data as such",
            );
        }
        if block.tag == "table" {
            if !has_text("title") {
                issue(
                    "info",
                    "table.title",
                    "The table has no title or accessible name",
                    "table.title".into(),
                    "Add title: with the table's subject",
                );
            }
            if let Some(rows) = block
                .value
                .pointer("/data/inline")
                .and_then(Value::as_array)
            {
                for (row, value) in rows.iter().enumerate() {
                    for (column, value) in value.as_object().into_iter().flatten() {
                        if let Some(text) = value.as_str()
                            && Parser::new_ext(text, crate::markdown::FLAGS).any(|e| {
                                matches!(
                                    e,
                                    Event::Start(Tag::Emphasis | Tag::Strong | Tag::Link { .. })
                                )
                            })
                        {
                            issue(
                                "info",
                                "table.markdown",
                                "This cell contains Markdown syntax and is shown as written",
                                format!("table.data.inline[{row}].{column}"),
                                "Use a Markdown table in the prose when cells need emphasis or links",
                            );
                        }
                    }
                }
            }
        }
        if matches!(block.tag.as_str(), "audio" | "video") {
            if block.tag == "video" && !has_text("captions") {
                issue(
                    "info",
                    "media.captions",
                    "The video has no captions",
                    "video.captions".into(),
                    "Add captions: pointing to a WebVTT file in the report folder",
                );
            }
            if !has_text("transcript") {
                issue(
                    "info",
                    "media.transcript",
                    "The media has no transcript",
                    format!("{}.transcript", block.tag),
                    "Add transcript: pointing to a text or Markdown file in the report folder",
                );
            }
        }
    }
}

fn checked(
    (report, charts, _): (RenderedReport, Vec<Value>, Option<crate::Document>),
) -> CheckReport {
    CheckReport {
        status: report.metadata["_richview"]["status"]
            .as_str()
            .unwrap_or("issues")
            .into(),
        diagnostics: report.diagnostics,
        charts,
    }
}

fn render_internal(
    source: &str,
    options: &RenderOptions,
    retain_document: bool,
) -> Result<(RenderedReport, Vec<Value>, Option<crate::Document>), Vec<Diagnostic>> {
    render_document_internal(parse(source)?, options, retain_document)
}

fn render_document_internal(
    doc: crate::Document,
    options: &RenderOptions,
    retain_document: bool,
) -> Result<(RenderedReport, Vec<Value>, Option<crate::Document>), Vec<Diagnostic>> {
    crate::folder::with_inventory(options.report_folder.as_ref(), || {
        render_document_in_scope(doc, options, retain_document)
    })
}

fn render_document_in_scope(
    mut doc: crate::Document,
    options: &RenderOptions,
    retain_document: bool,
) -> Result<(RenderedReport, Vec<Value>, Option<crate::Document>), Vec<Diagnostic>> {
    for (path, width) in std::iter::once(("options.width".to_owned(), options.width)).chain(
        options
            .chart_widths
            .iter()
            .enumerate()
            .map(|(index, width)| (format!("options.chart_widths[{index}]"), *width)),
    ) {
        if !width.is_finite() || !(240.0..=4096.0).contains(&width) {
            return Err(vec![
                Diagnostic::new(
                    "error",
                    "options.width",
                    "Chart width must be finite and between 240 and 4096",
                    path,
                )
                .fix("Set every chart width to a finite number from 240 through 4096"),
            ]);
        }
    }
    // The list of styles is bounded before any of it is copied, parsed or reported on.
    if let Some(problem) = style::style_list_problem(&doc.frontmatter, |name| {
        options.data.get(name).map(String::len)
    }) {
        return Err(vec![problem]);
    }
    crate::folder::render_blockers(&mut doc, options.report_folder.as_ref())?;
    // Findings a host attached before the render, such as the folder check's.
    let attached = doc.diagnostics.len();
    let pdf = options.output == Output::Pdf;
    let lang = shell::language(&doc.frontmatter);
    // A PDF's charts are laid out at the width of the printed text column.
    let width = if pdf {
        options.page.text_area().0
    } else {
        options.width
    };
    let mut refs = Vec::new();
    // The report-relative file each loaded style came from, by its index in `refs`.
    let mut files = Vec::new();
    if let Some(styles) = doc.frontmatter.get("styles").and_then(Value::as_array) {
        for (index, reference) in styles
            .iter()
            .enumerate()
            .filter_map(|(index, style)| Some((index, style.as_str()?)))
        {
            // A builtin, retired or misspelt name; the cascade reports the last two.
            if style::is_style_name(reference) {
                refs.push(reference.into());
                continue;
            }
            if let Some(source) = options.data.get(reference) {
                files.push((refs.len(), reference.to_owned()));
                refs.push(source.clone());
            } else if doc.diagnostics[..attached]
                .iter()
                .any(|d| d.path == format!("frontmatter.styles[{index}]"))
            {
                // The host said why it could not read the file, such as a file that is not text.
            } else {
                // A style file that cannot be read is skipped; the rest of the cascade stays
                // (FEA-RP-30).
                let (code, message) =
                    crate::folder::asset_problem(options.report_folder.as_ref(), reference)
                        .unwrap_or((
                            "style.unresolved",
                            format!("Styling file '{reference}' was not resolved by the host."),
                        ));
                let mut finding = Diagnostic::new(
                    "error",
                    code,
                    message,
                    format!("frontmatter.styles[{index}]"),
                )
                .fix(crate::folder::STYLE_FILE_FIX);
                crate::parser::place_frontmatter(&doc, std::slice::from_mut(&mut finding));
                doc.diagnostics.push(finding);
            }
        }
    }
    refs.extend(options.styles.iter().cloned());
    // The report's own background comes last, so its word wins (STY-74).
    let report_background = style::report_background_layer(&doc.frontmatter).map(|layer| {
        refs.push(layer);
        refs.len() - 1
    });
    let authored = refs.len();
    let refs = style::cascade_refs(&refs);
    let default_below = refs.len() - authored;
    let first = doc.diagnostics.len();
    let mut resolved = style::resolve_styles(&refs, &mut doc.diagnostics);
    style::embed_brand_fonts(&mut resolved, &options.data, &mut doc.diagnostics);
    let company = crate::resolve_company_brand(&resolved, &options.data, &mut doc.diagnostics);
    let background_picture = crate::company_brand::read_background_picture(
        &resolved,
        &options.data,
        &mut doc.diagnostics,
    );
    style::shift_style_paths(&mut doc.diagnostics[first..], default_below);
    if let Some(index) = report_background {
        let mut late = doc.diagnostics.split_off(first);
        style::place_report_background(&mut late, index);
        crate::parser::place_frontmatter(&doc, &mut late);
        doc.diagnostics.append(&mut late);
    }
    for d in &mut doc.diagnostics[first..] {
        if let Some((_, file)) = files.iter().find(|(index, _)| {
            let root = format!("styles[{index}]");
            d.path == root || d.path.starts_with(&format!("{root}."))
        }) {
            d.file = Some(file.clone());
        }
    }
    doc.diagnostics
        .extend(style::checks::style_findings(&resolved));
    let block_text: Vec<(String, usize)> = doc
        .blocks
        .iter()
        .map(|block| {
            let mut strings = String::new();
            collect_strings(&block.value, &mut strings);
            (strings, block.line)
        })
        .collect();
    let front = |key: &str| {
        doc.frontmatter
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let (front_title, front_description) = (front("title"), front("description"));
    let mut scans = vec![
        style::checks::GlyphScan {
            text: &front_title,
            first_line: 1,
            role: "display",
            markdown: false,
        },
        style::checks::GlyphScan {
            text: &front_description,
            first_line: 1,
            role: "text",
            markdown: false,
        },
        style::checks::GlyphScan {
            text: &doc.body,
            first_line: doc.body_line.max(1),
            role: "text",
            markdown: true,
        },
    ];
    scans.extend(
        block_text
            .iter()
            .map(|(text, line)| style::checks::GlyphScan {
                text,
                first_line: *line,
                role: "text",
                markdown: false,
            }),
    );
    let deck_texts = doc
        .deck
        .as_ref()
        .map(|deck| deck.texts())
        .unwrap_or_default();
    scans.extend(
        deck_texts
            .iter()
            .map(|(text, line, role)| style::checks::GlyphScan {
                text,
                first_line: *line,
                role,
                markdown: true,
            }),
    );
    let glyphs = style::checks::glyphs(&resolved, &scans);
    doc.diagnostics.extend(glyphs);
    let print = style::print_style(&refs, &resolved);
    let chart_theme = figure::chart_theme(&resolved);
    let (paper, print_dark) = style::print_cascade(&refs, &resolved);
    if print_dark {
        doc.diagnostics.push(Diagnostic::new(
            "warning",
            "style.print-dark",
            "The style is still dark after the print swap, so the page prints with Air's surfaces and inks",
            "styles",
        ));
    }
    // The page also prints, so the printed cascade meets the same contrast floors: a pale
    // `link` an author sets over Sleek reads on the dark screen and fails on white paper.
    if let Some(paper) = &paper {
        let screen: Vec<String> = style::checks::contrast(&resolved)
            .into_iter()
            .map(|d| d.path)
            .collect();
        for mut finding in style::checks::contrast(paper) {
            if !screen.contains(&finding.path) {
                finding.message = format!("On paper: {}", finding.message);
                doc.diagnostics.push(finding);
            }
        }
    }
    // Paper takes the light style's colours but draws with the faces the file embeds.
    let print_theme = paper
        .as_ref()
        .map(|paper| richview_chart::layout::ChartTheme {
            data_font: chart_theme.data_font.clone(),
            figures_font: chart_theme.figures_font.clone(),
            ui_font: chart_theme.ui_font.clone(),
            ..figure::chart_theme(paper)
        });
    // On a slide's stage bars take their band's share, on screen and on paper, and the plot
    // stands on the stage itself rather than on a panel of its own (FEA-RP-65). A builtin
    // style's stage also lets the first series lead: the others step back to quiet tones. The
    // sketchbook's charts lead in its red pencil, the ink its frame is drawn in, ahead of its
    // greys. A chart keeps the style's data face, which the engine measures, so the runtime
    // never clips a label to an estimated box.
    let stage = resolved
        .document
        .pointer("/components/slides/stage")
        .and_then(Value::as_str)
        .filter(|stage| *stage != "plain");
    let wide = |theme: &richview_chart::layout::ChartTheme| richview_chart::layout::ChartTheme {
        wide_bars: true,
        few_gridlines: stage.is_some(),
        stage_values: stage.is_some(),
        surface: theme.page.clone(),
        palette: match stage {
            Some("sketchbook") => crate::slides::quiet_palette(
                &[std::slice::from_ref(&theme.accent), &theme.palette[..]].concat(),
                &theme.page,
            ),
            Some(_) => crate::slides::quiet_palette(&theme.palette, &theme.page),
            None => theme.palette.clone(),
        },
        ..theme.clone()
    };
    let on_slides = doc.deck.is_some();
    let slide_theme = on_slides.then(|| wide(&chart_theme));
    let slide_print = print_theme.as_ref().filter(|_| on_slides).map(wide);
    let figure_context = figure::Context {
        theme: &chart_theme,
        print: print_theme.as_ref(),
        lang: &lang,
        colors: &[],
        widths: match options.chart_widths.as_slice() {
            [] => {
                let inset = style::frame_inset_px(&resolved.document, pdf);
                [PHONE_COLUMN - inset, figure_width(&resolved) - inset]
            }
            [only] => [*only, *only],
            [first, second, ..] => [*first, *second],
        },
        pdf: pdf.then(|| [width, options.page.text_area().1]),
        stacked: None,
    };
    let failed_prefix = crate::parser::failed_prefix(
        doc.block_placeholder_prefix
            .as_deref()
            .unwrap_or(crate::parser::BLOCK_PLACEHOLDER_PREFIX),
    );
    // A chart in a column is laid out at its column's width (FEA-RP-68), so the first paint
    // matches the runtime's first layout; a phone stacks the columns and keeps the phone width.
    let block_prefix = doc
        .block_placeholder_prefix
        .clone()
        .unwrap_or_else(|| crate::parser::BLOCK_PLACEHOLDER_PREFIX.to_owned());
    // Slides lay out no columns (FEA-RP-65): a chart there takes its box on the stage.
    let placements = if on_slides {
        Vec::new()
    } else {
        crate::columns::placements(&doc.body, &block_prefix, doc.blocks.len())
    };
    let line_px = body_line_px(&resolved);
    let page_height = options.page.text_area().1;
    let column_context = |placement: &crate::columns::Placement| {
        let inset = style::frame_inset_px(&resolved.document, pdf);
        let options = &placement.options;
        let container = if options.wide {
            desktop_column(&resolved.document)
        } else {
            figure_width(&resolved)
        };
        let desktop = options
            .column_px(placement.column, container, line_px)
            .round()
            - inset;
        // Between a phone and the width where the columns stand side by side they stack, and a
        // chart takes the whole container: the box it reserves there comes from a layout at the
        // widest stacked container, so a tablet does not stretch the side-by-side box (the
        // window is the container plus the report's 24 px gutters).
        let stack = options.stack_px(line_px);
        let stacked = (container >= stack).then(|| (stack.floor() - inset, stack + 48.0));
        // Paper sets type at its own scale, so its gaps are the printed line's.
        let paper = pdf.then(|| {
            [
                options
                    .column_px(placement.column, width, line_px * style::PRINT_TYPE_SCALE)
                    .floor(),
                page_height,
            ]
        });
        (desktop, paper, stacked)
    };
    let mut column_css = String::new();
    let heading_ids: Vec<String> = doc
        .frontmatter
        .pointer("/tabs/panels")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|panel| panel.get("from").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    let placeholder = doc
        .block_placeholder_prefix
        .clone()
        .unwrap_or_else(|| crate::parser::BLOCK_PLACEHOLDER_PREFIX.to_owned());
    // Slides (FEA-RP-65) show everything a slide holds, so a slides source folds no section.
    let slides = doc.deck.is_some();
    let collapsed: Vec<(usize, &str)> = if slides {
        Vec::new()
    } else {
        doc.frontmatter
            .get("collapsed_sections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, value)| value.as_str().map(|anchor| (index, anchor)))
            .collect()
    };
    let heading_marker = placeholder.replacen("block:", "heading:", 1);
    let mut encoded_sections: std::collections::HashSet<String> = collapsed
        .iter()
        .map(|(_, anchor)| markdown::heading_marker_id(anchor))
        .collect();
    let body_line = doc.body_line;
    let body_context = markdown::Context {
        heading_ids: &heading_ids,
        section_markers: !collapsed.is_empty(),
        first_line: (body_line > 0).then_some(body_line),
        columns: true,
        path: "markdown",
        scope: "",
        placeholder: &placeholder,
        lang: &lang,
    };
    // Slides (FEA-RP-71): a chart is laid out for the box its slot has on the stage, which the
    // slide's layout and cuts decide.
    let slide_charts = match &doc.deck {
        Some(deck) => deck.chart_boxes(&doc.blocks),
        None => Vec::new(),
    };
    let frame_inset = style::frame_inset_px(&resolved.document, pdf);
    let mut bodies = Vec::with_capacity(doc.blocks.len());
    let mut block_bytes = 0usize;
    let mut charts = vec![];
    let mut chart_diagnostics = vec![];
    for (i, block) in doc.blocks.iter_mut().enumerate() {
        let diagnostic_start = doc.diagnostics.len();
        if media_output_bytes(block, options) > crate::MAX_REPORT_BYTES - block_bytes {
            return Err(output_limit(Some(block)));
        }
        // A dataset that cannot be read fails only its block (FEA-RP-30).
        let mut data_failure = None;
        if matches!(block.tag.as_str(), "chart" | "table") {
            blocks::resolve_data(block, &doc.frontmatter, options, &mut doc.diagnostics);
            data_failure = failed_data(
                &mut doc.diagnostics,
                (attached, diagnostic_start),
                &block.tag,
                &block.value,
            );
        }
        if data_failure.is_none() && matches!(block.tag.as_str(), "chart" | "table") {
            let issues = crate::walker::field_issues(&block.tag, &block.value, block.line);
            // A missing encoding cannot be laid out: preserve the existing diagnostic and
            // leave this block as a notice instead of passing invalid fields to the engine.
            if block.tag == "chart" {
                data_failure = issues
                    .iter()
                    .find(|d| d.level == "error" && d.path.starts_with("chart.encoding."))
                    .map(|d| d.message.clone());
            }
            doc.diagnostics.extend(issues);
        }
        let mut body = if let Some(message) = data_failure {
            notice(
                &format!("{} not rendered: {message}", blocks::tag_title(&block.tag)),
                Some(block.line),
                None,
            )
        } else if block.tag == "chart" {
            let (rows, order) = chart_rows(&block.value);
            let id = format!("rv-chart-{i}");
            let in_column = placements.get(i).cloned().flatten().map(|placement| {
                let (desktop, paper, stacked) = column_context(&placement);
                if let Some((_, window)) = stacked.filter(|_| !pdf) {
                    // Up to where the columns stand side by side, and at least to the phone
                    // width under which they always stack (columns.css).
                    let below = (window - 0.02).max(640.0);
                    column_css.push_str(&format!(
                        "@media screen and (min-width:601px) and (max-width:{below:.2}px){{#{id} .rv-chart__frame{{aspect-ratio:var(--rv-ratio-stacked,var(--rv-ratio))}}}}\n"
                    ));
                }
                figure::Context {
                    widths: [figure_context.widths[0], desktop],
                    pdf: paper.or(figure_context.pdf),
                    stacked: if pdf { None } else { stacked.map(|(width, _)| width) },
                    ..figure_context
                }
            });
            // On a slide the chart's reference layout is its box on the stage; paper narrows a
            // chart taller than the room its slide leaves (FEA-RP-65).
            let on_slide =
                slide_charts
                    .iter()
                    .find(|(index, _, _)| *index == i)
                    .map(|&(_, width, room)| figure::Context {
                        theme: slide_theme.as_ref().unwrap_or(figure_context.theme),
                        print: slide_print.as_ref().or(figure_context.print),
                        widths: [figure_context.widths[0], width - frame_inset],
                        pdf: pdf.then(|| [width - frame_inset, room / figure::PRINT_HEIGHT_SHARE]),
                        ..figure_context
                    });
            // On a stage the number is the message: a chart prints the values it adds by itself,
            // as in a report, unless its author turns them off (FEA-RP-65).
            let figure = figure::chart(
                &block.value,
                &rows,
                &order,
                &id,
                block.line,
                on_slide
                    .as_ref()
                    .or(in_column.as_ref())
                    .unwrap_or(&figure_context),
            );
            doc.diagnostics.extend(figure.diagnostics);
            charts.extend(figure.payload);
            chart_diagnostics.push(figure.check);
            figure.html
        } else if block.tag == "scrolly" && block.value.get("chart").is_some_and(Value::is_object) {
            // A story whose figure is a chart: the chart renders as any chart block does, and
            // each step names the control choices that move it (FEA-RP-48).
            let mut chart_block = crate::RichBlock {
                tag: "chart".into(),
                value: block.value["chart"].clone(),
                raw: String::new(),
                line: block.line,
            };
            let first = doc.diagnostics.len();
            blocks::resolve_data(
                &mut chart_block,
                &doc.frontmatter,
                options,
                &mut doc.diagnostics,
            );
            let mut failure = failed_data(
                &mut doc.diagnostics,
                (attached, first),
                "scrolly.chart",
                &chart_block.value,
            );
            if failure.is_none() {
                let mut issues =
                    crate::walker::field_issues("chart", &chart_block.value, block.line);
                failure = issues
                    .iter()
                    .find(|d| d.level == "error" && d.path.starts_with("chart.encoding."))
                    .map(|d| d.message.clone());
                for issue in &mut issues {
                    issue.path = format!("scrolly.{}", issue.path);
                    issue.tag = Some("scrolly".into());
                }
                doc.diagnostics.extend(issues);
            }
            if let Some(message) = failure {
                notice(
                    &format!("Story not rendered: {message}"),
                    Some(block.line),
                    None,
                )
            } else {
                let chart = &chart_block.value;
                let (rows, order) = chart_rows(chart);
                let id = format!("rv-chart-{i}");
                let figure = figure::chart(chart, &rows, &order, &id, block.line, &figure_context);
                doc.diagnostics.extend(figure.diagnostics);
                blocks::place_in_data_file(&mut doc.diagnostics[first..], &chart_block, options);
                charts.extend(figure.payload);
                chart_diagnostics.push(figure.check);
                let controls = richview_chart::reader_controls::Controls::parse(chart, &rows);
                let steps = block
                    .value
                    .get("steps")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let (mut choices, mut stills) = (vec![], vec![]);
                for (n, step) in steps.iter().enumerate() {
                    let mut picks: Vec<(usize, String)> = vec![];
                    for (control, wanted) in step
                        .get("state")
                        .and_then(Value::as_object)
                        .into_iter()
                        .flatten()
                    {
                        let wanted = match wanted {
                            Value::String(t) => t.trim().to_owned(),
                            other => other.to_string(),
                        };
                        let found = controls
                            .list
                            .iter()
                            .position(|c| &c.id == control)
                            .and_then(|k| {
                                controls.list[k]
                                    .options
                                    .iter()
                                    .position(|o| o.value == wanted)
                                    .map(|o| (k, o))
                            });
                        match found {
                            Some((k, o)) => picks.push((k, format!("{k}:{o}"))),
                            None => doc.diagnostics.push(
                                Diagnostic::new(
                                    "warning",
                                    "scrolly.state",
                                    format!("The step sets control \"{control}\" to \"{wanted}\", which the chart does not offer"),
                                    format!("scrolly.steps[{n}].state.{control}"),
                                )
                                .fix("Use a control id and option value from the chart's `controls`")
                                .at(block.line, "scrolly"),
                            ),
                        }
                    }
                    choices.push(
                        picks
                            .iter()
                            .map(|(_, pair)| pair.as_str())
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                    // On paper each step with a state of its own prints its chart in that state.
                    let still = if pdf && !picks.is_empty() {
                        let mut moved = chart.clone();
                        for (k, pair) in &picks {
                            let option: usize = pair
                                .split_once(':')
                                .and_then(|(_, o)| o.parse().ok())
                                .unwrap_or(0);
                            if let Some(value) = controls.list[*k].options.get(option) {
                                moved["controls"][*k]["initial"] = json!(value.value);
                            }
                        }
                        let still = figure::chart(
                            &moved,
                            &rows,
                            &order,
                            &format!("{id}-s{n}"),
                            block.line,
                            &figure_context,
                        );
                        still.html
                    } else {
                        String::new()
                    };
                    stills.push(still);
                }
                crate::scrolly::render(
                    &block.value,
                    options,
                    &lang,
                    &failed_prefix.replacen("failed:", "block:", 1),
                    &mut doc.diagnostics,
                    block.line,
                    Some(crate::scrolly::ChartStage {
                        html: figure.html,
                        choices,
                        stills,
                    }),
                )
            }
        } else {
            if matches!(block.tag.as_str(), "table" | "metric") {
                // A host-internal key, like `_csvNumericText`: the number formatter reads it.
                block.value["_lang"] = json!(lang);
                if slides && block.tag == "table" {
                    // A slide shows the table, not the report's filter box (FEA-RP-70).
                    block.value["_slides"] = json!(true);
                }
            }
            blocks::render_block(block, options, &lang, &failed_prefix, &mut doc.diagnostics)
        };
        blocks::place_in_data_file(&mut doc.diagnostics[diagnostic_start..], block, options);
        crate::parser::place_block(&mut doc.diagnostics[diagnostic_start..], block);
        if let Some(provenance) = provenance_note(block, pdf) {
            body.push_str(&provenance);
        }
        let emitted = body.len();
        if emitted > crate::MAX_REPORT_BYTES - block_bytes
            || has_output_limit(&doc.diagnostics[diagnostic_start..])
        {
            return Err(output_limit(Some(block)));
        }
        block_bytes += emitted;
        bodies.push(body);
    }
    let diagnostic_start = doc.diagnostics.len();
    let markdown_body =
        markdown::markdown_html(&doc.body, options, &body_context, &mut doc.diagnostics);
    if has_output_limit(&doc.diagnostics[diagnostic_start..]) {
        return Err(output_limit(None));
    }
    // The contents list the body's own sections, before its blocks fill in (FEA-RP-63).
    let sections = reading::sections(&markdown_body);
    if !collapsed.is_empty() {
        let first = doc.diagnostics.len();
        let mut headings = std::collections::HashMap::new();
        let mut after_previous = 0;
        for (at, _) in markdown_body.match_indices(&heading_marker) {
            let meta = at + heading_marker.len();
            let Some((marker, _)) = markdown_body[meta..].split_once("-->") else {
                break;
            };
            let Some((level, id)) = marker.split_once(':') else {
                continue;
            };
            let start_tag = format!("<h{level} id=\"");
            let contains_link = markdown_body[after_previous..at]
                .rfind(&start_tag)
                .is_some_and(|offset| markdown_body[after_previous + offset..at].contains("<a "));
            headings.insert(id, contains_link);
            after_previous = meta + marker.len() + 3;
        }
        for (index, anchor) in &collapsed {
            let encoded = markdown::heading_marker_id(anchor);
            let finding = match headings.get(encoded.as_str()) {
                None => Some((
                    "section.anchor-missing",
                    format!(
                        "Collapsed section anchor \"{anchor}\" does not name a top-level heading"
                    ),
                )),
                Some(true) => {
                    encoded_sections.remove(&encoded);
                    Some((
                        "section.heading-interactive",
                        format!("Collapsed section heading \"{anchor}\" contains a link"),
                    ))
                }
                Some(false) => None,
            };
            if let Some((code, message)) = finding {
                doc.diagnostics.push(Diagnostic::new(
                    "warning",
                    code,
                    message,
                    format!("frontmatter.collapsed_sections[{index}]"),
                ));
            }
        }
        let mut findings = doc.diagnostics.split_off(first);
        crate::parser::place_frontmatter(&doc, &mut findings);
        doc.diagnostics.extend(findings);
    }
    // On paper a chart and its source note are one unit, so no page break separates them. Browser
    // print needs the box too: Chrome gives up the second avoided break in a heading, chart and
    // note chain. On screen the box generates none (`display: contents`).
    let keep: Vec<bool> = doc.blocks.iter().map(|b| b.tag == "chart").collect();
    // A fence that failed shows its notice in place (FEA-RP-29), with its source unless the parser
    // withheld it.
    let failed: Vec<String> = doc
        .failed
        .iter()
        .map(|f| {
            notice(
                &format!("{} not rendered: {}", blocks::tag_title(&f.tag), f.message),
                Some(f.line),
                (!f.source.is_empty()).then_some(f.source.as_str()),
            )
        })
        .collect();
    let markdown_body = expand_blocks(&markdown_body, &failed_prefix, &failed, &[])
        .map_err(|_| output_limit(None))?;
    let colophon = shell::colophon(&resolved.document);
    let footer = format!("<footer class=\"rv-report__footer\">{colophon}</footer>");
    // How many slides the rendition has, its title slide included; 0 for a report.
    let mut slide_count = 0;
    let body = if let Some(deck) = &doc.deck {
        slide_count = deck.len();
        // The colophon closes the last slide.
        let compose = crate::slides::Compose {
            options,
            lang: &lang,
            prefix: &placeholder,
            colophon: if colophon.is_empty() { "" } else { &footer },
        };
        let mut found = Vec::new();
        let composed = deck.html(&compose, &doc.blocks, &bodies, &mut found);
        doc.diagnostics.extend(found);
        expand_blocks(&composed, &failed_prefix, &failed, &[])
            .and_then(|composed| expand_blocks(&composed, &placeholder, &bodies, &[]))
    } else {
        expand_blocks(&markdown_body, &placeholder, &bodies, &keep)
    }
    .map_err(|_| output_limit(None))?;
    drop(bodies);
    let mut body = crate::columns::expand(&body, &block_prefix);
    let has_columns = body.contains("<div class=\"rv-columns");
    let runtime = runtime_script(options);
    let tabs_output = if pdf {
        TabsOutput::Paper
    } else {
        TabsOutput::Screen
    };
    let mut tabbed = false;
    if doc.frontmatter.get("tabs").is_some() && !slides {
        let authored = markdown::authored_heading_ids(&doc.body);
        let first = doc.diagnostics.len();
        (body, tabbed) = apply_tabs(
            &doc.frontmatter,
            body,
            &authored,
            tabs_output,
            &heading_marker,
            &encoded_sections,
            &mut doc.diagnostics,
        );
        let mut tabs = doc.diagnostics.split_off(first);
        crate::parser::place_frontmatter(&doc, &mut tabs);
        doc.diagnostics.extend(tabs);
    }
    if !tabbed && !collapsed.is_empty() {
        body = fold_sections(&body, &heading_marker, &encoded_sections, pdf);
    }
    let title = doc
        .frontmatter
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(if doc.deck.is_some() {
            "Untitled deck"
        } else {
            "Untitled report"
        });
    let mut header = String::new();
    if let Some(error) = unreadable_frontmatter(&doc.diagnostics) {
        // The title block is lost with the frontmatter that held it (FEA-RP-58).
        header.push_str(&notice(
            &format!("Frontmatter not read: {}", error.message),
            error.line,
            None,
        ));
    } else {
        header.push_str(&company.header_html(resolved.scheme == "dark"));
        if let Some(image) = shell::header_image(&doc.frontmatter, &mut doc.diagnostics) {
            // An Unsplash photo without a valid credit was reported as `image.credit-missing` or
            // `image.credit-invalid`; the header omits it and the frontmatter keeps the source.
            let uncredited = image
                .get("src")
                .and_then(Value::as_str)
                .is_some_and(crate::unsplash_image_url)
                && crate::parser::image_diagnostics(&image, "", None)
                    .iter()
                    .any(|d| d.level == "error");
            if image.is_object() && !uncredited {
                let first = doc.diagnostics.len();
                let html =
                    blocks::render_media("image", &image, options, &lang, &mut doc.diagnostics, 1);
                // A header image that cannot load shows its alt text instead (FEA-RP-30).
                if doc.diagnostics[first..]
                    .iter()
                    .any(|d| d.level == "error" && d.code.starts_with("report.asset."))
                {
                    let alt = image.get("alt").and_then(Value::as_str).unwrap_or("");
                    if !alt.is_empty() {
                        header
                            .push_str(&format!("<p class=\"rv-report__meta\">{}</p>", escape(alt)));
                    }
                    let mut found = doc.diagnostics.split_off(first);
                    for d in &mut found {
                        d.path = "frontmatter.title_image".into();
                        (d.line, d.column, d.tag) = (None, None, None);
                    }
                    crate::parser::place_frontmatter(&doc, &mut found);
                    doc.diagnostics.extend(found);
                } else {
                    header.push_str(&html);
                }
            }
        }
        header.push_str(&format!(
            "<h1 class=\"rv-report__title\">{}</h1>",
            escape(title)
        ));
        if let Some(value) = doc.frontmatter.get("description").and_then(Value::as_str) {
            header.push_str(&format!(
                "<p class=\"rv-report__lede\">{}</p>",
                escape(value)
            ));
        }
        header.push_str(&shell::byline(&doc.frontmatter, &lang));
    }
    // A PDF page is light whatever colour scheme the style asks for.
    let mut theme_attr = if pdf {
        " data-rv-output=\"pdf\"".to_owned()
    } else {
        String::new()
    };
    if slides {
        theme_attr.push_str(" data-rv-slides");
        // The style's stage (FEA-RP-65): slides.css sets each builtin's world under it.
        if let Some(stage) = resolved
            .document
            .pointer("/components/slides/stage")
            .and_then(Value::as_str)
            .filter(|stage| *stage != "plain")
        {
            theme_attr.push_str(&format!(" data-rv-stage=\"{}\"", escape(stage)));
        }
        // A picture behind the header (STY-74) fills the title slide's stage: slides.css sets
        // the picture and the title's panel under this mark. Paper has no background.
        let behind_header = style::background::Spec::read(&resolved.document)
            .is_some_and(|spec| spec.picture.is_some() && spec.place == "header");
        if behind_header && background_picture.is_some() && !pdf {
            theme_attr.push_str(" data-rv-title-picture");
        }
    }
    let mut math_font_link = "";
    if body.contains("<math ") {
        let math_font = style::math_font_css();
        if math_font.is_empty() {
            math_font_link = "<link rel=\"stylesheet\" href=\"/fonts-math.css\">";
        }
        resolved.font_css.push_str(&math_font);
        resolved
            .font_css
            .push_str(".rv-report__body math{font-family: \"STIX Two Math\"}\n");
    }
    // Reading aids are for the screen; paper has no scroll to follow (FEA-RP-63, FEA-RP-64).
    let contents = if !pdf && !slides && reading::wants_contents(&doc.frontmatter, sections.len()) {
        reading::contents(&sections)
    } else {
        String::new()
    };
    let progress = if !pdf && reading::wants_progress(&doc.frontmatter) {
        reading::PROGRESS
    } else {
        ""
    };
    let mut css = format!(
        "{}\n{}\n{}\n{}{}{}{}{PRINT_CSS}{}",
        resolved.font_css,
        style::base_css(),
        resolved.css,
        EXTRA_CSS,
        if doc.blocks.iter().any(|b| b.tag == "scrolly") {
            SCROLLY_CSS
        } else {
            ""
        },
        if tabbed { TABS_CSS } else { "" },
        figure::FIGURE_CSS,
        print.css
    );
    if body.contains("class=\"hl-") {
        css.push_str(&style::code::css(
            &resolved,
            &chart_theme.palette,
            paper.as_ref().zip(print_theme.as_ref()),
        ));
    }
    // Only a report that holds columns carries their rules (FEA-RP-68), so every other report
    // keeps its bytes.
    if has_columns {
        css.push_str(COLUMNS_CSS);
        css.push_str(&column_css);
    }
    // A table's heat and its filter box bring their rules only to a report that uses them
    // (FEA-RP-69, FEA-RP-70).
    if body.contains(" style=\"--rv-heat:") {
        css.push_str(&crate::heat::css(&resolved, paper.as_ref()));
    }
    // The marker is a bare attribute, which escaped author text cannot write.
    if body.contains(" data-rv-filter>") {
        css.push_str(TABLE_FILTER_CSS);
    }
    // Short labels in table cells (FEA-RP-19); escaped author text cannot write the cell.
    if body.contains("<td class=\"label\"") {
        css.push_str(TABLE_LABEL_CSS);
    }
    css.push_str(&company.header_css());
    // Paper drops the background (STY-74); its CSS is screen only, and a PDF leaves it out.
    if let Some(picture) = background_picture.as_ref().filter(|_| !pdf) {
        css.push_str(&style::background::picture_css(
            &resolved.document,
            &picture.data_uri(),
            picture.width,
            picture.height,
        ));
    }
    // A slide is its own page, so slides bring their page box (FEA-RP-65).
    if pdf && !slides {
        css.push_str(&page_css(&print, options.page, title));
    }
    if !contents.is_empty() || !progress.is_empty() {
        css.push_str(reading::READING_CSS);
    }
    if slides {
        css.push_str(crate::slides::CSS);
        css.push_str(&style::slides_css(&resolved));
    }
    let font_notices = used_font_notices(&resolved.font_css);
    if options.runtime_url.is_some() {
        doc.diagnostics.push(Diagnostic::new("info","assets.runtime-external","Preview uses a cached external Rust runtime; embed the runtime when exporting an offline report","options.runtime_url"));
        if runtime.is_empty() {
            let (message, path) = if options.runtime_wasm.as_deref().is_some_and(|bytes| {
                options.runtime_wasm_codec != "raw" || !safe_wasm_base64(bytes)
            }) {
                (
                    "Wasm bytes supplied with a cached runtime URL must be raw base64",
                    "options.runtime_wasm",
                )
            } else if options.runtime_js.as_deref().is_none_or(str::is_empty) {
                (
                    "Cached runtime needs the matching JavaScript bindings to verify integrity",
                    "options.runtime_js",
                )
            } else {
                (
                    "Cached runtime URL must be a root-relative path or an absolute HTTP(S) base without credentials, query or fragment",
                    "options.runtime_url",
                )
            };
            doc.diagnostics.push(Diagnostic::new(
                "warning",
                "security.runtime-url",
                message,
                path,
            ));
        }
    }
    let escaped_title = escape(title);
    // Fixed HTML overhead is small; the exact assembled limit below remains authoritative.
    // Bound the variable parts before allocating another complete report-sized string.
    let variable_bytes = [
        css.as_str(),
        body.as_str(),
        header.as_str(),
        contents.as_str(),
        runtime.as_str(),
        escaped_title.as_str(),
        theme_attr.as_str(),
        font_notices.as_str(),
    ]
    .iter()
    .fold(0usize, |bytes, part| bytes.saturating_add(part.len()));
    if variable_bytes > crate::MAX_REPORT_BYTES {
        return Err(output_limit(None));
    }
    let embed_hosts = embed_hosts(&doc.blocks);
    let hotlinks = header.contains("https://images.unsplash.com")
        || body.contains("https://images.unsplash.com");
    let head = shell::head(&shell::Head {
        title,
        description: doc.frontmatter.get("description").and_then(Value::as_str),
        scheme: color_scheme(&resolved),
        csp: shell::csp(&runtime, &embed_hosts, hotlinks),
    });
    // A style that switches motion off renders everything settled (FEA-RP-35, TYP-49).
    let motion_attr = if resolved.document["motion"]["enabled"] == json!(false) {
        " data-motion=\"off\""
    } else {
        ""
    };
    // Reserve the large parts before formatting; appending the closing tags must not double
    // a buffer that has just grown to hold the embedded runtime.
    let mut html = String::with_capacity(
        variable_bytes
            .saturating_add(head.len())
            .saturating_add(colophon.len())
            .saturating_add(1024)
            .min(crate::MAX_REPORT_BYTES),
    );
    // A deck has no header: its first slide is its cover. Its page keeps the title as a heading
    // the stage hides, and the colophon closed the last slide.
    let (header, footer) = if slides {
        (
            format!("<h1 class=\"rv-slides__title\">{escaped_title}</h1>"),
            String::new(),
        )
    } else {
        (
            format!("<header class=\"rv-report__header\">{header}</header>"),
            footer,
        )
    };
    write!(
        html,
        "<!doctype html>\n<html lang=\"{}\"{theme_attr}><head>{head}{math_font_link}<style>{css}</style>{font_notices}</head><body><article class=\"rv-report{rail}\"{motion_attr}>{header}{contents}<main class=\"rv-report__body\">{body}</main>{footer}</article>{progress}{runtime}</body></html>\n",
        escape(&lang),
        rail = if slides {
            " rv-report--slides"
        } else if contents.is_empty() {
            ""
        } else {
            " rv-report--toc"
        },
    )
    .expect("writing to a String cannot fail");
    if html.len() > crate::MAX_REPORT_BYTES {
        return Err(output_limit(None));
    }
    let mut metadata = if retain_document {
        doc.frontmatter.clone()
    } else {
        std::mem::take(&mut doc.frontmatter)
    };
    drop_refound_files(&mut doc.diagnostics, attached);
    let status = if doc
        .diagnostics
        .iter()
        .any(|d| d.level == "error" || d.level == "warning")
    {
        "issues"
    } else {
        "clean"
    };
    metadata["_richview"] = json!({"generator":format!("richview/{VERSION}"),"format":crate::shell::FORMAT,"source":options.source,"block_count":doc.blocks.len(),"chart_count":chart_diagnostics.len(),"status":status,"offline":!doc.diagnostics.iter().any(|d|d.code.starts_with("assets.")),"interactive":!runtime.is_empty(),"chart_diagnostics":chart_diagnostics});
    if slides {
        metadata["_richview"]["slides"] = json!(slide_count);
    }
    let diagnostics = if retain_document {
        doc.diagnostics.clone()
    } else {
        std::mem::take(&mut doc.diagnostics)
    };
    let retained = if retain_document { Some(doc) } else { None };
    Ok((
        RenderedReport {
            company,
            html,
            diagnostics,
            metadata,
            charts,
        },
        chart_diagnostics,
        retained,
    ))
}

/// Why a block's data failed, once `resolve_data` added its findings from `at.1` on. A file the
/// folder check or the host already reported among the first `at.0` findings (missing, escaping,
/// or not UTF-8 text) is one finding, not two (FEA-RP-30): the render's own `data.unresolved`
/// goes, and the message is the host's when it gave a data finding. `field` is the path of the
/// value that holds `data`.
fn failed_data(
    diagnostics: &mut Vec<Diagnostic>,
    at: (usize, usize),
    field: &str,
    value: &Value,
) -> Option<String> {
    let (attached, start) = at;
    let added = diagnostics.split_off(start);
    let file = crate::document::DATA_FILE_KEYS
        .into_iter()
        .find_map(|kind| Some((kind, value["data"][kind].as_str()?)));
    let reported = file.and_then(|(kind, reference)| {
        let (path, quoted) = (format!("{field}.data.{kind}"), format!("'{reference}'"));
        diagnostics[..attached].iter().find(|d| {
            d.level == "error"
                && d.path == path
                && (d.file.as_deref() == Some(reference) || d.message.contains(&quoted))
        })
    });
    let message = reported
        .filter(|d| d.code.starts_with("data."))
        .or_else(|| {
            added
                .iter()
                .find(|d| d.level == "error" && d.code.starts_with("data."))
        })
        .map(|d| d.message.clone());
    let answered = reported.is_some();
    diagnostics.extend(
        added
            .into_iter()
            .filter(|d| !(answered && d.code == "data.unresolved")),
    );
    message
}

/// A file the folder check named is one finding (FEA-RP-30, TYP-56): drops each render finding
/// about a file that one of the first `attached` findings already reported at the same field and
/// line. Each earlier finding answers for one later finding.
fn drop_refound_files(diagnostics: &mut Vec<Diagnostic>, attached: usize) {
    let file = |d: &Diagnostic| crate::folder::ASSET_FINDINGS.contains(&d.code.as_str());
    let mut answered = vec![false; attached];
    let mut keep = Vec::with_capacity(diagnostics.len());
    for (index, d) in diagnostics.iter().enumerate() {
        let earlier = (index >= attached && file(d))
            .then(|| {
                (0..attached).find(|&i| {
                    let e = &diagnostics[i];
                    !answered[i]
                        && file(e)
                        && e.path == d.path
                        && (e.line == d.line || e.line.is_none() || d.line.is_none())
                })
            })
            .flatten();
        if let Some(i) = earlier {
            answered[i] = true;
        }
        keep.push(earlier.is_none());
    }
    let mut keep = keep.into_iter();
    diagnostics.retain(|_| keep.next().unwrap_or(true));
}

/// The licence notices of the faces the report embeds and no others (FEA-RP-38). JSON preserves
/// each complete text after decoding; script-safe escaping prevents a notice delimiter from
/// becoming executable HTML.
fn used_font_notices(font_css: &str) -> String {
    // A face that lends a family its script digits or signs names its lender after the rule:
    // `/* script digits from RichView Serif */`.
    let lenders = font_css
        .split("/* script ")
        .skip(1)
        .filter_map(|tail| tail.split(" */").next()?.split_once(" from "))
        .map(|(_, lender)| lender);
    let used: std::collections::BTreeSet<String> = font_css
        .split("font-family: \"")
        .skip(1)
        .filter_map(|tail| tail.split('"').next())
        .chain(lenders)
        .map(|family| match family.to_lowercase().as_str() {
            // Scientific and Editorial embed upstream faces under RichView's own names.
            "richview serif" => "source serif 4".to_owned(),
            "richview sans" => "source sans 3".to_owned(),
            "richview code" => "source code pro".to_owned(),
            "richview mono" => "ibm plex mono".to_owned(),
            other => other.to_owned(),
        })
        .collect();
    let fonts = [
        (
            "Big Shoulders",
            include_str!("../assets/font-licenses/big-shoulders.txt"),
        ),
        ("Geist", include_str!("../assets/font-licenses/geist.txt")),
        (
            "Geist Mono",
            include_str!("../assets/font-licenses/geist-mono.txt"),
        ),
        (
            "IBM Plex Mono",
            include_str!("../assets/font-licenses/ibm-plex-mono.txt"),
        ),
        (
            "Newsreader",
            include_str!("../assets/font-licenses/newsreader.txt"),
        ),
        (
            "Patrick Hand",
            include_str!("../assets/font-licenses/patrick-hand.txt"),
        ),
        (
            "Source Code Pro",
            include_str!("../assets/font-licenses/source-code-pro.txt"),
        ),
        (
            "Source Sans 3",
            include_str!("../assets/font-licenses/source-sans-3.txt"),
        ),
        (
            "Source Serif 4",
            include_str!("../assets/font-licenses/source-serif-4.txt"),
        ),
        (
            "Space Grotesk",
            include_str!("../assets/font-licenses/space-grotesk.txt"),
        ),
        (
            "Space Mono",
            include_str!("../assets/font-licenses/space-mono.txt"),
        ),
        (
            "STIX Two Math",
            include_str!("../assets/font-licenses/stix-two-math.txt"),
        ),
    ];
    let mut fonts: Vec<Value> = fonts
        .into_iter()
        .filter(|(font, _)| used.contains(&font.to_lowercase()))
        .map(|(font, license)| json!({"font":font,"license":license}))
        .collect();
    // A brand font is the author's own file: the notice names it and whose licence applies.
    let mut brand: Vec<&str> = font_css
        .lines()
        .filter(|line| line.starts_with("/* brand font */"))
        .filter_map(|line| line.split("font-family: \"").nth(1)?.split('"').next())
        .collect();
    brand.dedup();
    fonts.extend(brand.into_iter().map(|font| {
        json!({"font":font,"license":"Supplied with this report by its author, whose licence for the file applies."})
    }));
    if fonts.is_empty() {
        return String::new();
    }
    format!(
        "<script id=\"richview-font-notices\" type=\"application/json\">{}</script>",
        json_script(&json!({"fonts":fonts}))
    )
}

/// `dark` or `light`: the scheme the cascade's page colours declare (STY-57).
fn color_scheme(style: &style::ResolvedStyle) -> &'static str {
    if style.scheme == "dark" {
        "dark"
    } else {
        "light"
    }
}

/// The hosts of the report's allowed embeds, for the policy's `frame-src`.
fn embed_hosts(blocks: &[crate::RichBlock]) -> Vec<String> {
    let mut hosts: Vec<String> = blocks
        .iter()
        .filter(|b| b.tag == "embed")
        .filter_map(|b| blocks::embed_host(&b.value))
        .collect();
    hosts.sort();
    hosts.dedup();
    hosts
}

/// Render only the reader fields of host-supplied connector provenance, never the query inputs.
fn provenance_note(block: &crate::RichBlock, pdf: bool) -> Option<String> {
    if !matches!(block.tag.as_str(), "chart" | "table") {
        return None;
    }
    let provenance = block.value.pointer("/data/_provenance")?;
    let connector = provenance.get("connector")?.as_str()?;
    let source = provenance.get("source")?.as_str()?;
    let captured = provenance.get("fetched_at")?.as_str()?;
    let fingerprint = provenance.get("fingerprint")?.as_str()?;
    let rows = provenance.get("row_count")?.as_u64()?;
    let stale = provenance
        .get("stale")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let open = if pdf { " open" } else { "" };
    let (summary, stale_note) = if stale {
        (
            "Data source (stale snapshot)",
            "<p>This snapshot is stale.</p>",
        )
    } else {
        ("Data source", "")
    };
    Some(format!(
        "<details class=\"rv-provenance\"{open}><summary>{summary}</summary><dl><dt>Connector</dt><dd>{}</dd><dt>Source</dt><dd>{}</dd><dt>Query fingerprint</dt><dd><code>{}</code></dd><dt>Captured</dt><dd><time datetime=\"{}\">{}</time></dd><dt>Rows</dt><dd>{rows}</dd></dl>{stale_note}</details>",
        escape(connector),
        escape(source),
        escape(&fingerprint.chars().take(12).collect::<String>()),
        escape(captured),
        escape(captured),
    ))
}

/// The frontmatter error that left the title block unread, if any (FEA-RP-58).
fn unreadable_frontmatter(diagnostics: &[Diagnostic]) -> Option<&Diagnostic> {
    diagnostics.iter().find(|d| {
        d.level == "error"
            && d.path == "frontmatter"
            && (d.code == "frontmatter.yaml"
                || d.code == "frontmatter.mapping"
                || d.code.starts_with("yaml."))
    })
}

/// A block that failed, in the report's ink and rule rather than an alarm colour (FEA-RP-29).
/// The inline rows and column order of a chart's data.
fn chart_rows(chart: &Value) -> (Vec<Value>, Vec<String>) {
    let data = chart.get("data");
    let rows = data
        .and_then(|d| d.get("inline"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let order = data
        .and_then(|d| d.get("columns"))
        .and_then(Value::as_array)
        .map(|c| {
            c.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    (rows, order)
}

/// The line is `<message> (line N)`; `source` follows in a code block when it is the problem.
pub(crate) fn notice(message: &str, line: Option<usize>, source: Option<&str>) -> String {
    let at = line.map(|n| format!(" (line {n})")).unwrap_or_default();
    let source = source
        .map(|s| {
            format!(
                "<pre class=\"rv-notice__source\"><code>{}</code></pre>",
                markdown::code_lines(s.trim_end())
            )
        })
        .unwrap_or_default();
    // A notice never repeats a credential written into an address (ARC-14).
    format!(
        "<div class=\"rv-notice\" role=\"note\"><p class=\"rv-notice__line\">{}{at}</p>{source}</div>",
        escape(&crate::document::redact_userinfo(message))
    )
}

/// Scan only the sanitized Markdown once. Inserted block HTML is never rescanned, so comments
/// inside a callout cannot become recursive rich blocks or duplicate chart runtime IDs. A block
/// marked in `keep` is wrapped together with the source note that directly follows it.
pub(crate) fn expand_blocks(
    markdown: &str,
    prefix: &str,
    bodies: &[String],
    keep: &[bool],
) -> Result<String, ()> {
    if prefix.is_empty() {
        return (markdown.len() <= crate::MAX_REPORT_BYTES)
            .then(|| markdown.to_owned())
            .ok_or(());
    }
    let capacity = bodies.iter().fold(markdown.len(), |bytes, body| {
        bytes.saturating_add(body.len())
    });
    let mut output = String::with_capacity(capacity.min(crate::MAX_REPORT_BYTES));
    let mut remaining = markdown;
    while let Some(start) = remaining.find(prefix) {
        append_report(&mut output, &remaining[..start])?;
        remaining = &remaining[start + prefix.len()..];
        let digits = remaining.bytes().take_while(u8::is_ascii_digit).count();
        let index = remaining[..digits]
            .parse::<usize>()
            .ok()
            .filter(|_| remaining[digits..].starts_with("-->"));
        if let Some((index, replacement)) =
            index.and_then(|index| Some((index, bodies.get(index)?)))
        {
            remaining = &remaining[digits + 3..];
            let note = keep
                .get(index)
                .copied()
                .unwrap_or(false)
                .then(|| source_note_end(remaining))
                .flatten();
            match note {
                Some(end) => {
                    append_report(&mut output, "<div class=\"rv-keep\">")?;
                    append_report(&mut output, replacement)?;
                    append_report(&mut output, &remaining[..end])?;
                    append_report(&mut output, "</div>")?;
                    remaining = &remaining[end..];
                }
                None => append_report(&mut output, replacement)?,
            }
        } else {
            append_report(&mut output, prefix)?;
            append_report(&mut output, &remaining[..digits])?;
            remaining = &remaining[digits..];
        }
    }
    append_report(&mut output, remaining)?;
    Ok(output)
}

/// The end of a source note paragraph at the start of `html`, after optional whitespace.
fn source_note_end(html: &str) -> Option<usize> {
    const NOTE: &str = "<p class=\"rv-source\">";
    let start = html.len() - html.trim_start().len();
    let rest = html[start..].strip_prefix(NOTE)?;
    Some(start + NOTE.len() + rest.find("</p>")? + "</p>".len())
}

fn append_report(output: &mut String, part: &str) -> Result<(), ()> {
    if part.len() > crate::MAX_REPORT_BYTES - output.len() {
        return Err(());
    }
    output.push_str(part);
    Ok(())
}

fn media_output_bytes(block: &crate::RichBlock, options: &RenderOptions) -> usize {
    let tag = block.tag.as_str();
    if !matches!(tag, "image" | "audio" | "video") {
        return 0;
    }
    let reference = block.value["src"].as_str().unwrap_or("");
    let src = options
        .data
        .get(reference)
        .map(String::as_str)
        .unwrap_or(reference);
    if !markdown::safe_media(src, tag) {
        return 0;
    }
    // Audio/video retain both a source and a download link; video may also embed a poster.
    let mut bytes = src.len().saturating_mul(if tag == "image" { 1 } else { 2 });
    if tag == "video"
        && let Some(reference) = block.value["poster"].as_str()
    {
        let poster = options
            .data
            .get(reference)
            .map(String::as_str)
            .unwrap_or(reference);
        if markdown::safe_media(poster, "image") {
            bytes = bytes.saturating_add(poster.len());
        }
    }
    bytes
}

fn has_output_limit(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "render.output-limit")
}

pub(crate) fn output_limit(block: Option<&crate::RichBlock>) -> Vec<Diagnostic> {
    let diagnostic = Diagnostic::new(
        "error",
        "render.output-limit",
        "Generated report exceeds the 64 MiB HTML limit; reduce repeated embedded media/data or split the report",
        "output",
    );
    vec![if let Some(block) = block {
        diagnostic.at(block.line, &block.tag)
    } else {
        diagnostic
    }]
}

/// Provide the public JavaScript render interface through the generated browser binding.
/// Translation and product behavior remain Rust; native hot paths do not serialize an AST.
pub fn render_compat(source: &str, legacy_options: &Value) -> Result<Value, Vec<Diagnostic>> {
    let mut options = RenderOptions {
        report_folder: legacy_options
            .get("report_folder")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|e| {
                vec![Diagnostic::new(
                    "error",
                    "report.root.invalid",
                    e.to_string(),
                    "report_folder",
                )]
            })?,
        width: legacy_options
            .get("chartWidth")
            .and_then(Value::as_f64)
            .unwrap_or(720.0),
        source: legacy_options
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_owned),
        output: legacy_options
            .get("output")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|error| {
                vec![Diagnostic::new(
                    "error",
                    "render.output.invalid",
                    error.to_string(),
                    "options.output",
                )]
            })?
            .unwrap_or_default(),
        page: legacy_options
            .get("page")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|error| {
                vec![Diagnostic::new(
                    "error",
                    "render.page.invalid",
                    error.to_string(),
                    "options.page",
                )]
            })?
            .unwrap_or_default(),
        ..RenderOptions::default()
    };
    if let Some(styles) = legacy_options.get("styles") {
        options.styles = style::legacy_style_refs(styles)?;
    }
    if let Some(data) = legacy_options.get("data").and_then(Value::as_object) {
        for (key, value) in data {
            if let Some(value) = value.as_str() {
                options.data.insert(key.clone(), value.into());
            }
        }
    }
    let (report, charts, parsed) = render_internal(source, &options, true)?;
    let mut parsed = parsed.expect("retained compatibility AST");
    parsed
        .diagnostics
        .retain(|diagnostic| !diagnostic.code.starts_with("chart."));
    let status = report.metadata["_richview"]["status"].clone();
    Ok(
        json!({"html":report.html,"parsed":parsed,"hasCharts":!charts.is_empty(),"diagnostics":{"status":status,"charts":charts,"parse":report.diagnostics.into_iter().filter(|diagnostic|!diagnostic.code.starts_with("chart.")).collect::<Vec<_>>()}}),
    )
}

/// The report column at a 390 px phone viewport (two 16 px gutters).
const PHONE_COLUMN: f64 = 358.0;

/// A pixel length from a style token: `px`, `rem` at 16 px, or `ch` in the text face at the base
/// size.
fn token_px(style: &style::ResolvedStyle, name: &str) -> Option<f64> {
    let value = style.custom_properties.get(name)?.trim().to_owned();
    let number = |suffix: &str| {
        value
            .strip_suffix(suffix)
            .and_then(|n| n.trim().parse::<f64>().ok())
    };
    let px = if let Some(n) = number("px") {
        n
    } else if let Some(n) = number("rem") {
        n * 16.0
    } else {
        let n = number("ch")?;
        let size = token_px(style, "--rv-size-base").unwrap_or(16.0);
        let face = style
            .custom_properties
            .get("--rv-font-text")
            .map_or("serif", String::as_str);
        n * richview_chart::text::Font::new(face).ch() * size
    };
    (px.is_finite() && px > 0.0).then_some(px)
}

/// The width a figure occupies on a desktop: the report's measure, which every body block
/// shares (STY-30), within the theme's column.
fn figure_width(style: &style::ResolvedStyle) -> f64 {
    let column = desktop_column(&style.document);
    // Whole pixels, as the runtime reads the frame's width.
    token_px(style, "--rv-measure").map_or(column, |m| m.clamp(PHONE_COLUMN + 1.0, column).round())
}

/// One line of body text in px: the base size times the line height, as `--rv-line` is.
fn body_line_px(style: &style::ResolvedStyle) -> f64 {
    let size = token_px(style, "--rv-size-base").unwrap_or(16.0);
    let line = style
        .custom_properties
        .get("--rv-line-base")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|line| line.is_finite() && *line > 0.0)
        .unwrap_or(1.5);
    size * line
}

/// The theme's desktop column: its report width less the two 24 px gutters of the report grid.
fn desktop_column(document: &Value) -> f64 {
    let width = document
        .pointer("/layout/report-max-width")
        .and_then(Value::as_str)
        .and_then(|w| {
            let w = w.trim();
            w.strip_suffix("px")
                .and_then(|n| n.trim().parse::<f64>().ok())
                .or_else(|| {
                    w.strip_suffix("rem")
                        .and_then(|n| n.trim().parse::<f64>().ok())
                        .map(|n| n * 16.0)
                })
        })
        .filter(|w| w.is_finite() && *w > 0.0)
        .unwrap_or(980.0);
    (width - 48.0).clamp(PHONE_COLUMN + 1.0, 4096.0)
}

/// The PDF page box: paper size and margins, the theme background to the paper's edge, and from
/// the second page on the running title and page numbers in the bottom margin. Colours and faces
/// are literal because custom properties do not reach `@page`.
fn page_css(print: &style::PrintStyle, page: crate::Page, title: &str) -> String {
    let margin_box = format!(
        "font:8pt/1.3 {};color:{};font-variant-numeric:tabular-nums",
        print.font_ui, print.ink_soft
    );
    let [top, side, bottom] = crate::PAGE_MARGINS_MM;
    format!(
        "@page{{size:{} portrait;margin:{top}mm {side}mm {bottom}mm;background:{};@bottom-left{{content:{};{margin_box}}}@bottom-right{{content:counter(page) \" / \" counter(pages);{margin_box}}}}}@page:first{{@bottom-left{{content:none}}@bottom-right{{content:none}}}}\n",
        page.css_size(),
        print.bg,
        css_string(&running_title(title)),
    )
}

/// The report title cut at a word boundary to fit one line of the page foot.
fn running_title(title: &str) -> String {
    const LIMIT: usize = 70;
    if title.chars().count() <= LIMIT {
        return title.to_owned();
    }
    let cut: String = title.chars().take(LIMIT - 1).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", cut.trim_end_matches([' ', ',', '.', ';', ':']))
}

/// A CSS string. `<` is escaped too, so report text cannot close the style element.
fn css_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '<' => out.push_str("\\3C "),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// One body for URL-only and parent-supplied bytes: the site's parent CSP pins this exact hash.
const EXTERNAL_RUNTIME_INITIALIZER: &str = "(()=>{const s=document.currentScript.previousElementSibling;const b=s.getAttribute('data-rv-wasm-bytes');richview_runtime({module_or_path:b===null?s.dataset.rvWasm:(b=>{const u=new Uint8Array(b.length);for(let i=0;i<b.length;i++)u[i]=b.charCodeAt(i);return u})(atob(b))});})();";

/// Keep trusted base64 from escaping its JavaScript string or HTML attribute.
fn safe_wasm_base64(value: &str) -> bool {
    value
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'='))
}

fn runtime_script(options: &RenderOptions) -> String {
    if let Some(base) = options.runtime_url.as_deref() {
        // A root-relative path keeps one rendered report valid on every host that serves it.
        let same_origin = base.starts_with('/')
            && !base.starts_with("//")
            && !base.contains(['\\', '?', '#', '"', '\'', '<', '>', ' ']);
        let allowed = (same_origin
            || url::Url::parse(base).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
            }))
            && !base.chars().any(char::is_control);
        if allowed {
            let Some(js) = options.runtime_js.as_deref().filter(|js| !js.is_empty()) else {
                return String::new();
            };
            if options.runtime_wasm.as_deref().is_some_and(|bytes| {
                options.runtime_wasm_codec != "raw" || !safe_wasm_base64(bytes)
            }) {
                return String::new();
            }
            let base = base.strip_suffix(".js").unwrap_or(base);
            let wasm = format!("{base}_bg.wasm");
            let mut runtime = String::with_capacity(
                options
                    .runtime_wasm
                    .as_ref()
                    .map_or(0, String::len)
                    .saturating_add(base.len().saturating_mul(2))
                    .saturating_add(EXTERNAL_RUNTIME_INITIALIZER.len())
                    .saturating_add(128),
            );
            write!(
                runtime,
                "<script src=\"{}\" data-rv-wasm=\"{}\" integrity=\"{}\" crossorigin=\"anonymous\"",
                escape(&format!("{base}.js")),
                escape(&wasm),
                shell::script_integrity(js)
            )
            .expect("writing to a String cannot fail");
            if let Some(bytes) = &options.runtime_wasm {
                runtime.push_str(" data-rv-wasm-bytes=\"");
                runtime.push_str(bytes);
                runtime.push('"');
            }
            runtime.push_str("></script><script>");
            runtime.push_str(EXTERNAL_RUNTIME_INITIALIZER);
            runtime.push_str("</script>");
            return runtime;
        }
        return String::new();
    }
    match (&options.runtime_js, &options.runtime_wasm) {
        (Some(js), Some(wasm)) if safe_wasm_base64(wasm) => {
            let gzip = match options.runtime_wasm_codec.as_str() {
                "raw" => false,
                "gzip" => true,
                _ => return String::new(),
            };
            // The report runtime's bindings name their global `richview_runtime`, so a page
            // that also loads the site runtime keeps both (ARC-06).
            let initializer = if js.contains("export default __wbg_init") {
                "__wbg_init"
            } else {
                "richview_runtime"
            };
            let js = js
                .replace("</script", "<\\/script")
                .replace("</SCRIPT", "<\\/SCRIPT");
            // Write the base64 bytes once. Formatting them through several intermediate
            // Strings retained several report-sized allocations during every offline render.
            let mut runtime =
                String::with_capacity(js.len().saturating_add(wasm.len()).saturating_add(1024));
            runtime.push_str("<script type=\"module\">\n");
            runtime.push_str(&js);
            runtime.push_str("\nawait ");
            runtime.push_str(initializer);
            runtime.push_str("({module_or_path:");
            if gzip {
                runtime.push_str("await new Response(new Blob([");
            }
            // A plain loop keeps the existing fast base64-to-byte conversion.
            runtime.push_str("(b=>{const u=new Uint8Array(b.length);for(let i=0;i<b.length;i++)u[i]=b.charCodeAt(i);return u})(atob(\"");
            runtime.push_str(wasm);
            runtime.push_str("\"))");
            if gzip {
                runtime.push_str(
                    "]).stream().pipeThrough(new DecompressionStream('gzip'))).arrayBuffer()",
                );
            }
            runtime.push_str("});\n</script>");
            runtime
        }
        _ => String::new(),
    }
}

/// How a tabbed report reaches its readers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TabsOutput {
    /// The HTML report: every panel, stacked under its label until the runtime builds the row.
    Screen,
    /// A PDF shows every panel, or only the default one under `export: active`.
    Paper,
}

/// Split the rendered body into panels at the heading anchors (`<hN id="...">`) that
/// `frontmatter.tabs.panels[*].from` names. A panel runs to the next panel's anchor or the end of
/// the body; content before the first anchor stays above the panels, so it is never hidden.
/// Returns the body and whether it was split. A missing or out-of-order anchor leaves the body
/// whole and adds a diagnostic. Under `export: active` a PDF keeps only the default panel and an
/// interactive report prints only the selected one, each with a notice of what is left out.
fn apply_tabs(
    frontmatter: &Value,
    body: String,
    authored: &[String],
    output: TabsOutput,
    heading_marker: &str,
    collapsed: &std::collections::HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> (String, bool) {
    let Some(spec) = frontmatter.get("tabs").and_then(Value::as_object) else {
        return (body, false);
    };
    let Some(panels) = spec.get("panels").and_then(Value::as_array) else {
        return (body, false);
    };
    let export_active = spec.get("export").and_then(Value::as_str) == Some("active");
    let mut entries: Vec<(&str, &str, usize)> = Vec::with_capacity(panels.len());
    for (index, panel) in panels.iter().enumerate() {
        let (Some(id), Some(label), Some(from)) = (
            panel.get("id").and_then(Value::as_str),
            panel.get("label").and_then(Value::as_str),
            panel.get("from").and_then(Value::as_str),
        ) else {
            return (body, false);
        };
        // The anchor's level is whatever the body gave it: a `#` title shifts every heading.
        let Some(offset) = authored
            .iter()
            .any(|id| id == from)
            .then(|| {
                (1..=6)
                    .filter_map(|level| body.find(&format!("<h{level} id=\"{from}\"")))
                    .min()
            })
            .flatten()
        else {
            diagnostics.push(Diagnostic::new(
                "warning",
                "tabs.anchor-missing",
                format!(
                    "Tab panel \"{id}\" starts at anchor \"{from}\", but the body has no `## Heading {{#{from}}}`; rendering the body without tabs"
                ),
                format!("frontmatter.tabs.panels[{index}].from"),
            ).fix(format!("Add a heading such as `## Heading {{#{from}}}`, or set this panel's `from` to an existing explicit heading id.")));
            return (body, false);
        };
        entries.push((id, label, offset));
    }
    if entries.is_empty() {
        return (body, false);
    }
    if let Some(index) = entries.windows(2).position(|pair| pair[0].2 >= pair[1].2) {
        diagnostics.push(Diagnostic::new(
            "warning",
            "tabs.order",
            "frontmatter.tabs.panels must list distinct from-anchors in document order; rendering the body without tabs",
            format!("frontmatter.tabs.panels[{}].from", index + 1),
        ));
        return (body, false);
    }
    // The report's Notes section would sit in the last panel; each panel lists its own (FEA-BR-40).
    let (body, notes) = detach_notes(body, entries.last().map_or(0, |e| e.2));
    let mut copies: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let active = spec
        .get("default")
        .and_then(Value::as_str)
        .filter(|id| entries.iter().any(|entry| entry.0 == *id))
        .unwrap_or(entries[0].0);
    let mut html = String::with_capacity(body.len() + 256 * entries.len());
    // Content above the first panel is always shown, so the notes it cites follow it.
    let prefix = panel_notes(&body[..entries[0].2], &notes, &mut copies);
    html.push_str(&fold_sections(
        &prefix,
        heading_marker,
        collapsed,
        output == TabsOutput::Paper,
    ));
    if export_active && entries.len() > 1 {
        match output {
            // Paper names what it leaves out, in full (FEA-BR-09).
            TabsOutput::Paper => {
                let label = |i: usize| escape(entries[i].1);
                let active_index = entries.iter().position(|e| e.0 == active).unwrap_or(0);
                let left: Vec<String> = (0..entries.len())
                    .filter(|i| *i != active_index)
                    .map(label)
                    .collect();
                let list = match left.as_slice() {
                    [one] => one.clone(),
                    [init @ .., last] => format!("{} and {last}", init.join(", ")),
                    [] => String::new(),
                };
                html.push_str(&format!(
                    "<p class=\"rv-tabs__export-note\" role=\"note\">Showing {}, 1 of {} views. {list} {} in the live report.</p>",
                    label(active_index),
                    entries.len(),
                    if left.len() == 1 { "is" } else { "are" }
                ));
            }
            // The browser fills the note when the reader prints (the runtime knows the view).
            TabsOutput::Screen => {
                html.push_str("<p class=\"rv-tabs__export-note\" role=\"note\"></p>")
            }
        }
    }
    for (index, (id, label, start)) in entries.iter().enumerate() {
        let is_active = *id == active;
        if export_active && output == TabsOutput::Paper && !is_active {
            continue;
        }
        let end = entries.get(index + 1).map_or(body.len(), |next| next.2);
        let (id, label) = (escape(id), escape(label));
        let default = if is_active { " data-rv-default" } else { "" };
        let export = if export_active {
            " data-rv-export=\"active\""
        } else {
            ""
        };
        html.push_str(&format!(
            "<section class=\"rv-tabpanel\" id=\"panel-{id}\" data-rv-tab=\"{id}\" data-rv-tab-label=\"{label}\"{default}{export}>"
        ));
        let panel = panel_notes(&body[*start..end], &notes, &mut copies);
        html.push_str(&fold_sections(
            &panel,
            heading_marker,
            collapsed,
            output == TabsOutput::Paper,
        ));
        html.push_str("</section>");
    }
    (html, true)
}

/// Wrap marked top-level Markdown sections after tabs have split the body. Markers are produced
/// by the Markdown renderer, so headings inside rich blocks do not change section boundaries.
/// The Notes that end a body or a panel are never folded: every fold closes before them, so a
/// reference above still finds its note in view. On paper nothing folds: the markers go and the
/// body is the unfolded report's, so a PDF places a section's blocks as its own.
fn fold_sections(
    body: &str,
    marker: &str,
    collapsed: &std::collections::HashSet<String>,
    paper: bool,
) -> String {
    const NOTES: &str = "<section class=\"rv-notes\"";
    let mut html = String::with_capacity(body.len());
    let mut cursor = 0;
    let mut open_levels = Vec::new();
    while let Some(relative) = body[cursor..].find(marker) {
        let at = cursor + relative;
        let metadata = at + marker.len();
        let Some(end) = body[metadata..].find("-->").map(|n| metadata + n) else {
            break;
        };
        let Some((level, id)) = body[metadata..end].split_once(':') else {
            break;
        };
        let Ok(level) = level.parse::<u8>() else {
            break;
        };
        let heading = format!("<h{level} id=\"");
        let Some(start) = body[cursor..at].rfind(&heading).map(|n| cursor + n) else {
            break;
        };
        html.push_str(&body[cursor..start]);
        while open_levels
            .last()
            .is_some_and(|previous| *previous >= level)
        {
            html.push_str("</details>");
            open_levels.pop();
        }
        if collapsed.contains(id) && !paper {
            html.push_str("<details class=\"rv-section\"><summary>");
            html.push_str(&body[start..at]);
            html.push_str("</summary>");
            open_levels.push(level);
        } else {
            html.push_str(&body[start..at]);
        }
        cursor = end + 3;
    }
    // The closing Notes are the last `rv-notes` section and end the body; a callout's or an
    // expand's own notes sit inside their block, so they never match.
    let rest = &body[cursor..];
    let notes = rest
        .rfind(NOTES)
        .filter(|n| rest[*n..].trim_end().ends_with("</ol></section>"))
        .unwrap_or(rest.len());
    html.push_str(&rest[..notes]);
    while open_levels.pop().is_some() {
        html.push_str("</details>");
    }
    html.push_str(&rest[notes..]);
    html
}

/// One footnote of the report: its number and the HTML inside its `<li>`.
struct Note {
    number: usize,
    inner: String,
}

/// Take the report's closing Notes section out of `body`, when it sits after `after` and ends the
/// body, and return its notes in order. The body is returned whole when there is none.
fn detach_notes(body: String, after: usize) -> (String, Vec<Note>) {
    const OPEN: &str = "<section class=\"rv-notes\"";
    const LIST: &str = "<ol>";
    const CLOSE: &str = "</ol></section>";
    let Some(start) = body.rfind(OPEN).filter(|start| *start >= after) else {
        return (body, Vec::new());
    };
    let section = &body[start..];
    let Some(items) = section.find(LIST).map(|i| &section[i + LIST.len()..]) else {
        return (body, Vec::new());
    };
    let Some(items) = items.trim_end().strip_suffix(CLOSE) else {
        return (body, Vec::new());
    };
    let notes = items
        .split("<li id=\"rv-fn-")
        .skip(1)
        .filter_map(|chunk| {
            let (number, rest) = chunk.split_once("\">")?;
            Some(Note {
                number: number.parse().ok()?,
                inner: rest.strip_suffix("</li>")?.to_owned(),
            })
        })
        .collect();
    (body[..start].to_owned(), notes)
}

/// The note numbers that `html` references.
fn cited_notes(html: &str) -> Vec<usize> {
    html.split("href=\"#rv-fn-")
        .skip(1)
        .filter_map(|rest| rest.split_once('"')?.0.parse().ok())
        .collect()
}

/// The panel's HTML followed by its own Notes section, which lists the notes the panel cites
/// (and the notes those cite), numbered as across the report. A note cited from several panels is
/// listed in each, under an id of its own so ids stay unique, and each panel's references and
/// back links point at its own copy. `copies` counts how often each note has been listed.
fn panel_notes(
    panel: &str,
    notes: &[Note],
    copies: &mut std::collections::HashMap<usize, usize>,
) -> String {
    let mut wanted: Vec<usize> = cited_notes(panel);
    let mut at = 0;
    while at < wanted.len() {
        if let Some(note) = notes.iter().find(|n| n.number == wanted[at]) {
            for cited in cited_notes(&note.inner) {
                if !wanted.contains(&cited) {
                    wanted.push(cited);
                }
            }
        }
        at += 1;
    }
    let mut listed: Vec<&Note> = notes
        .iter()
        .filter(|n| wanted.contains(&n.number))
        .collect();
    if listed.is_empty() {
        return panel.to_owned();
    }
    listed.sort_by_key(|n| n.number);
    let mut panel = panel.to_owned();
    let mut items = String::new();
    for note in listed {
        let copy = copies.entry(note.number).or_insert(0);
        *copy += 1;
        let (id, target) = if *copy == 1 {
            (
                format!("rv-fn-{}", note.number),
                format!("#rv-fn-{}", note.number),
            )
        } else {
            let id = format!("rv-fn-{}-{}", note.number, copy);
            let target = format!("#{id}");
            panel = panel.replace(
                &format!("href=\"#rv-fn-{}\"", note.number),
                &format!("href=\"{target}\""),
            );
            (id, target)
        };
        let mut inner = note.inner.clone();
        if *copy > 1 {
            inner = inner.replace(
                &format!("href=\"#rv-fn-{}\"", note.number),
                &format!("href=\"{target}\""),
            );
            // The first listing owns the ids of the references inside a note.
            while let Some(i) = inner.find(" id=\"rv-fnref-") {
                let Some(len) = inner[i + 5..].find('"') else {
                    break;
                };
                inner.replace_range(i..i + 5 + len + 1, "");
            }
        }
        // The way back leads to the first reference in this panel.
        let first = panel.split("<a id=\"").skip(1).find_map(|rest| {
            let (reference, tail) = rest.split_once('"')?;
            (reference.starts_with("rv-fnref-") && tail.starts_with(&format!(" href=\"{target}\"")))
                .then(|| reference.to_owned())
        });
        if let Some(first) = first {
            inner = inner.replace(
                &format!("href=\"#rv-fnref-{}\"", note.number),
                &format!("href=\"#{first}\""),
            );
        }
        items.push_str(&format!(
            "<li id=\"{id}\" value=\"{}\">{inner}</li>",
            note.number
        ));
    }
    format!(
        "{panel}<section class=\"rv-notes\" aria-label=\"Notes\"><h2>Notes</h2><ol>{items}</ol></section>\n"
    )
}

/// Tab rules, emitted only by reports whose body was split into panels. Until the runtime builds
/// the row, every panel is visible under its label; print gives every panel its label too, unless
/// the report exports only the active view.
const TABS_CSS: &str = r#"
.rv-tabs{display:flex;flex-wrap:nowrap;gap:.25rem;overflow-x:auto;scroll-snap-type:x proximity;border-bottom:1px solid var(--rv-rule);margin-block:1.25rem .75rem;padding-block-end:.25rem}
.rv-tabs__tab{scroll-snap-align:start;background:none;border:0;color:var(--rv-ink-soft);cursor:pointer;font:inherit;padding:.55rem .9rem;border-bottom:2px solid transparent;margin-block-end:-1px;white-space:nowrap}
.rv-tabs__tab:hover{color:var(--rv-ink)}
.rv-tabs__tab[aria-selected="true"]{color:var(--rv-ink);border-bottom-color:var(--rv-accent)}
.rv-tabs__tab:focus-visible{outline:2px solid var(--rv-accent);outline-offset:3px}
.rv-tabpanel[hidden]{display:none}
.rv-tabpanel::before{content:attr(data-rv-tab-label);display:block;font:600 .8rem/1 var(--rv-font-mono);color:var(--rv-ink-soft);text-transform:uppercase;letter-spacing:.08em;margin-block:1.5rem .5rem}
html[data-rv-runtime] .rv-tabpanel::before{display:none}
.rv-tabs__export-note{display:none;font-size:.85em;color:var(--rv-ink-soft);margin-block:.5rem 1rem;padding:.5rem .75rem;border-inline-start:2px solid var(--rv-rule)}
@media print{.rv-tabs{display:none}.rv-tabpanel[hidden]{display:block!important}.rv-tabpanel[data-rv-export="active"][hidden]{display:none!important}.rv-tabpanel::before{display:block!important}.rv-tabs__export-note:not(:empty){display:block}.rv-tabpanel{break-inside:auto}}
"#;

/// Paper rules every report carries, so a browser's own print gets them too.
const PRINT_CSS: &str = include_str!("../assets/print.css");

/// The scrollytelling element's layout, emitted only by reports that hold one (FEA-RP-48).
const SCROLLY_CSS: &str = include_str!("../assets/scrolly.css");

/// The columns' layout, emitted only by reports that hold them (FEA-RP-68).
const COLUMNS_CSS: &str = include_str!("../assets/columns.css");
const TABLE_FILTER_CSS: &str = include_str!("../assets/table-filter.css");
/// A short label of several words in a table cell stands on one line where the runtime found
/// that the table then fits its box, or scrolls either way, and is no taller than with its
/// labels wrapped (`data-rv-labels`, FEA-RP-19). Until
/// the runtime has measured, a page that runs scripts keeps labels whole on a wide screen, where
/// tables mostly fit, and lets them wrap on a phone, so a table that fits wrapped never scrolls
/// sideways. Without scripts and on paper labels wrap at their words.
const TABLE_LABEL_CSS: &str = "@media screen{.rv-table[data-rv-labels=line] td.label{white-space:nowrap}}\n@media screen and (scripting:enabled) and (min-width:641px){.rv-table:not([data-rv-labels]) td.label{white-space:nowrap}}\n";

/// Rules every report carries beyond its style. On screen a number cell never breaks inside its
/// value, as a date or an id never does: a table too wide for its column scrolls in its own box
/// (a Markdown table's wrapper, a table block, a chart's Table). On paper a number may wrap so
/// that the table fits the page, which does not scroll.
const EXTRA_CSS: &str = r#"
.rv-print-only{display:none}
.rv-report__body :is(th,td),.rv-chart-dialog :is(th,td){vertical-align:baseline}.rv-report__body td:is(.date,.id){white-space:nowrap}@media screen{.rv-report__body td.num,.rv-chart-dialog td.num{white-space:nowrap}}
.rv-table .center{text-align:center}.rv-table details{margin-block:1rem}.rv-table summary{cursor:pointer;color:var(--rv-link);font-family:var(--rv-font-mono);font-size:.8rem}.rv-table tfoot{font-weight:600;border-top:1px solid var(--rv-rule)}
.rv-table th button{background:none;border:0;color:inherit;font:inherit;text-align:inherit;padding:0;cursor:pointer}.rv-table th button:focus-visible{outline:2px solid var(--rv-accent);outline-offset:3px}.rv-table th[aria-sort="ascending"] button::after{content:" ↑"}.rv-table th[aria-sort="descending"] button::after{content:" ↓"}
.rv-chart--svg svg{display:block;width:100%;height:auto}.rv-report__body .rv-notes{clear:both;margin-block:var(--rv-block-gap) 0;padding-block:.6rem 0;border-top:1px solid var(--rv-rule-soft,var(--rv-rule))}.rv-notes h2{font-size:1em;margin:0 0 .4rem}.rv-notes ol{margin:0;padding-inline-start:0;list-style-position:inside}.rv-notes li{margin-block:.25rem}
.rv-image img,.rv-video video{max-width:100%;height:auto}.rv-audio audio{max-width:100%;width:100%}.rv-embed iframe{width:100%;border:0}
.rv-notes{font-size:.86em;color:var(--rv-ink-soft);max-width:var(--rv-measure-w)}.rv-notes p{display:inline}.footnote-reference{font-family:var(--rv-font-mono);font-size:.7em}.rv-math{padding-block:.7em}.rv-report__body dl{max-width:var(--rv-measure-w)}.rv-report__body dt{font-weight:600}.rv-report__body dd{margin-inline-start:1.5em}
.rv-report__body blockquote{max-width:var(--rv-measure-w);padding-inline-start:2.5ch;box-sizing:border-box}
.rv-visually-hidden{position:absolute;width:1px;height:1px;margin:-1px;padding:0;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap;border:0}
.rv-table__title{caption-side:top;text-align:start;font-weight:600;padding-block-end:.5em}.rv-table__part{width:100%}.rv-table__part td,.rv-table__part th{overflow-wrap:break-word}.rv-table__overflow{margin-block:0}.rv-table__total{border-top:1px solid var(--rv-rule)}.rv-table__total td{font-weight:600}
.rv-notice{border-block:1px solid var(--rv-rule);padding-block:.6em;margin-block:calc(var(--rv-line) * 1.25);color:var(--rv-ink)}.rv-notice__line{margin:0;font-family:var(--rv-font-ui);font-size:var(--rv-size-small)}.rv-notice__source{margin:.6em 0 0;overflow-x:auto;font-size:var(--rv-size-small)}
.rv-embed__card{border-block:1px solid var(--rv-rule);padding-block:1em}.rv-embed__caption{margin:0 0 .25em}.rv-embed__host{margin:0;color:var(--rv-ink-soft);font-size:var(--rv-size-small)}.rv-embed__actions{margin:.75em 0 0;display:flex;flex-wrap:wrap;gap:.5em 1em;align-items:baseline}.rv-embed__load{font:inherit;color:var(--rv-link);background:none;border:1px solid currentColor;padding:.3em .8em;cursor:pointer}.rv-embed__load:focus-visible{outline:2px solid var(--rv-accent);outline-offset:3px}
.rv-transcript{margin-block:.75em}.rv-transcript summary{cursor:pointer;color:var(--rv-link);font-size:var(--rv-size-small)}
.rv-report__colophon{margin:0;font-size:var(--rv-size-small);color:var(--rv-ink-soft)}
"#;

/// Every string value of a block's YAML, one per line: the text a reader sees from a chart,
/// metric or callout, for the glyph check.
fn collect_strings(value: &Value, out: &mut String) {
    match value {
        Value::String(text) => {
            out.push_str(text);
            out.push('\n');
        }
        Value::Array(items) => items.iter().for_each(|item| collect_strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| collect_strings(item, out)),
        _ => {}
    }
}
