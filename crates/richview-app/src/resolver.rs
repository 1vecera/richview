use base64::{Engine, engine::general_purpose::STANDARD};
use richview_core::document::{Document, RenderOptions, RenderedReport};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// Aggregate retained native data, encoded media and style contents per report.
pub const MAX_RESOLVED_RESOURCE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct ResourceBudget {
    retained: usize,
    /// Validation reads no warehouse: a connector-backed `data` block is passed over, not run.
    skip_connectors: bool,
    /// The connector names `report.connectors.yaml` declares, read only when validating.
    declared: std::collections::BTreeSet<String>,
    /// Findings on files the report names but the host did not read.
    findings: Vec<richview_core::document::Diagnostic>,
    /// For each Parquet file the report names, the texts of the blocks that name it, whose
    /// columns are the ones read; `None` when a block reads every column.
    parquet_names: BTreeMap<String, Option<Vec<String>>>,
    /// Parquet files that gave no rows, so each is read once however many blocks name it.
    parquet_refused: BTreeMap<String, crate::connectors::parquet_source::ParquetRefusal>,
}
impl ResourceBudget {
    /// Keeps an asset finding for the report; any other error stays fatal.
    fn tolerate(&mut self, error: String) -> Result<(), String> {
        // A file the folder lacks or holds outside it, and a style file that is not text, fail
        // only what reads them (FEA-RP-30).
        if !is_asset_finding(&error) && !error.contains("\"style.unresolved\"") {
            return Err(error);
        }
        if let Ok(found) = serde_json::from_str::<Vec<richview_core::document::Diagnostic>>(&error)
        {
            for d in found {
                if !self
                    .findings
                    .iter()
                    .any(|f| f.code == d.code && f.path == d.path && f.message == d.message)
                {
                    self.findings.push(d);
                }
            }
        }
        Ok(())
    }
    fn remaining(&self) -> usize {
        MAX_RESOLVED_RESOURCE_BYTES - self.retained
    }
    fn error(&self, reference: &str) -> String {
        format!(
            "Resolving '{reference}' exceeds the 64 MiB combined data/media/style budget ({} bytes already retained). Reduce referenced file sizes or use fewer resources; media includes base64 expansion.",
            self.retained
        )
    }
    fn charge(&mut self, reference: &str, bytes: usize) -> Result<(), String> {
        if bytes > self.remaining() {
            return Err(self.error(reference));
        }
        self.retained += bytes;
        Ok(())
    }
}

/// Explicit native file resolution. Report references cannot escape the source directory,
/// including through symlinks. Caller-selected local styles use the same report boundary.
pub fn resolve_options(
    source: &str,
    source_path: &Path,
    styles: &[String],
) -> Result<RenderOptions, String> {
    resolve_document_options(source, source_path, styles).map(|(_, options)| options)
}

/// Parse once, resolve native references, and retain the document for the renderer.
pub fn resolve_document_options(
    source: &str,
    source_path: &Path,
    styles: &[String],
) -> Result<(Document, RenderOptions), String> {
    resolve_document_options_policy(source, source_path, styles, false, true)
}

pub fn resolve_options_policy(
    source: &str,
    source_path: &Path,
    styles: &[String],
    allow_network: bool,
) -> Result<RenderOptions, String> {
    resolve_document_options_policy(source, source_path, styles, allow_network, true)
        .map(|(_, options)| options)
}

/// Validation can skip runtime assets while using the same safe data/style resolver.
pub fn resolve_document_options_policy(
    source: &str,
    source_path: &Path,
    styles: &[String],
    allow_network: bool,
    include_runtime: bool,
) -> Result<(Document, RenderOptions), String> {
    resolve_document_options_full(
        source,
        source_path,
        styles,
        allow_network,
        include_runtime,
        None,
    )
}

/// Variant that lets a trusted caller attach a connector [`crate::connectors::Registry`].
/// When a registry is attached, chart-local `data.sql`/`data.metricflow`/`data.cache`
/// blocks are resolved into inline rows before the existing file-based data resolver
/// runs. Without a registry, those blocks still produce the native-host error from
/// [`resolve_data`] below.
pub fn resolve_document_options_full(
    source: &str,
    source_path: &Path,
    styles: &[String],
    allow_network: bool,
    include_runtime: bool,
    registry: Option<&crate::connectors::Registry>,
) -> Result<(Document, RenderOptions), String> {
    resolve_inner(
        source,
        source_path,
        styles,
        allow_network,
        include_runtime,
        registry,
        false,
    )
}

/// Resolution for `validate`: everything is read and checked except connector-backed blocks,
/// which need a warehouse and are passed over, so every other finding is still reported.
pub fn resolve_for_validation(
    source: &str,
    source_path: &Path,
    styles: &[String],
) -> Result<(Document, RenderOptions), String> {
    resolve_inner(source, source_path, styles, false, false, None, true)
}

#[allow(clippy::too_many_arguments)]
fn resolve_inner(
    source: &str,
    source_path: &Path,
    styles: &[String],
    allow_network: bool,
    include_runtime: bool,
    registry: Option<&crate::connectors::Registry>,
    skip_connectors: bool,
) -> Result<(Document, RenderOptions), String> {
    if source.len() > crate::MAX_INPUT_BYTES {
        return Err("Report exceeds 32 MiB source limit".into());
    }
    let folder = report_folder(source_path)?;
    let source_path =
        fs::canonicalize(source_path).map_err(|e| format!("{}: {e}", source_path.display()))?;
    let root = source_path
        .parent()
        .ok_or("Source file has no parent directory")?;
    let mut doc = richview_core::document::parse(source)
        .map_err(|e| serde_json::to_string(&e).unwrap_or_default())?;
    // A missing file is a finding on the report, not a reason to render nothing: the rest of the
    // report still shows, and the findings say what to add.
    richview_core::document::attach_asset_findings(&mut doc, Some(&folder))
        .map_err(|found| serde_json::to_string(&found).unwrap())?;
    let mut options = RenderOptions {
        source: Some(source_path.display().to_string()),
        report_folder: Some(folder),
        ..Default::default()
    };
    if let Some(registry) = registry {
        let mut extra = Vec::new();
        crate::connectors::resolve::resolve_connector_data(
            &mut doc,
            root,
            registry,
            &mut options,
            &mut extra,
        )?;
        let failed = extra.iter().any(|finding| finding.level == "error");
        doc.diagnostics.extend(extra);
        // The registry already named the cause. Unresolved connector data must not replace it
        // with the fallback for callers that supplied no registry.
        if failed {
            return Err(serde_json::to_string(&doc.diagnostics).unwrap());
        }
    }
    let mut budget = ResourceBudget {
        skip_connectors,
        declared: if skip_connectors {
            declared_connectors(root)
        } else {
            Default::default()
        },
        ..Default::default()
    };
    if let Some(refs) = doc.frontmatter.get("styles").and_then(Value::as_array) {
        for (index, reference) in refs.iter().enumerate() {
            let reference = reference
                .as_str()
                .ok_or("Style reference must be a string")?;
            if !richview_core::builtin_style_names().contains(&reference)
                && !richview_core::is_retired_style_name(reference)
            {
                // A style file the folder lacks or cannot read is skipped with its finding, and
                // the report renders without it (FEA-RP-30).
                let field = format!("frontmatter.styles[{index}]");
                match resolve_style(reference, root, &field, &mut budget) {
                    Ok(content) => {
                        options.data.insert(reference.into(), content);
                    }
                    Err(error) => budget.tolerate(error)?,
                }
            }
        }
    }
    for reference in styles {
        options.styles.push(resolve_style(
            reference,
            root,
            "options.styles",
            &mut budget,
        )?);
    }
    // Brand fonts name files in the report folder (STY-16); a file that is absent is left for
    // the render to report as `style.font-missing`.
    // Match the renderer's cascade: report styles first, then styles explicitly passed by the caller.
    let mut cascade_sources: Vec<String> = doc
        .frontmatter
        .get("styles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|reference| {
            options
                .data
                .get(reference)
                .cloned()
                .unwrap_or_else(|| reference.to_owned())
        })
        .collect();
    cascade_sources.extend(options.styles.iter().cloned());
    // The frontmatter's background is the cascade's last layer (STY-74); its picture is bounded
    // below like a style file's.
    cascade_sources.extend(richview_core::report_background_layer(&doc.frontmatter));
    let cascade = richview_core::resolve_styles(&cascade_sources, &mut Vec::new());
    for file in richview_core::brand_font_files(&cascade) {
        if contained_path(root, &file).is_ok_and(|path| path.is_file()) {
            resolve_media(&file, root, allow_network, &mut options.data, &mut budget)?;
        }
    }
    for key in ["title_image", "title-image", "image"] {
        if let Some(reference) = doc.frontmatter.get(key).and_then(|image| {
            image
                .as_str()
                .or_else(|| image.get("src").and_then(Value::as_str))
                .filter(|reference| !richview_core::unsplash_image_url(reference))
        }) {
            resolve_media(
                reference,
                root,
                allow_network,
                &mut options.data,
                &mut budget,
            )?;
        }
    }
    for block in &doc.blocks {
        let named = block.tag == "chart" || block.value.get("columns").is_some();
        note_parquet_names(&block.value, &block.value, named, &mut budget.parquet_names);
        // A scrolly's steps name fields of its chart's data too.
        if block.tag == "scrolly"
            && let Some(chart) = block.value.get("chart")
        {
            note_parquet_names(chart, &block.value, true, &mut budget.parquet_names);
        }
    }
    for block in &doc.blocks {
        if let Some(data) = block.value.get("data") {
            resolve_data(
                data,
                &block.tag,
                block.line,
                root,
                allow_network,
                &mut options.data,
                &mut budget,
            )?;
        }
        if let Some(reference) = block
            .value
            .get("src")
            .and_then(Value::as_str)
            .filter(|_| ["image", "audio", "video"].contains(&block.tag.as_str()))
            .filter(|reference| {
                !(block.tag == "image" && richview_core::unsplash_image_url(reference))
            })
        {
            resolve_media(
                reference,
                root,
                allow_network,
                &mut options.data,
                &mut budget,
            )?;
        }
        if block.tag == "video"
            && let Some(reference) = block.value.get("poster").and_then(Value::as_str)
        {
            resolve_media(
                reference,
                root,
                allow_network,
                &mut options.data,
                &mut budget,
            )?;
        }
        // A scrolly over a chart reads the chart's own data file.
        if block.tag == "scrolly"
            && let Some(data) = block.value.get("chart").and_then(|chart| chart.get("data"))
        {
            resolve_data(
                data,
                "scrolly.chart",
                block.line,
                root,
                allow_network,
                &mut options.data,
                &mut budget,
            )?;
        }
        if block.tag == "scrolly"
            && let Some(reference) = block
                .value
                .get("image")
                .and_then(|image| image.get("src"))
                .and_then(Value::as_str)
                .filter(|reference| !richview_core::unsplash_image_url(reference))
        {
            resolve_media(
                reference,
                root,
                allow_network,
                &mut options.data,
                &mut budget,
            )?;
        }
        if ["audio", "video"].contains(&block.tag.as_str()) {
            for key in ["captions", "transcript"] {
                if let Some(reference) = block.value.get(key).and_then(Value::as_str) {
                    resolve_media(
                        reference,
                        root,
                        allow_network,
                        &mut options.data,
                        &mut budget,
                    )?;
                }
            }
        }
        if block.tag == "code"
            && let Some(reference) = block.value.get("src").and_then(Value::as_str)
        {
            resolve_media(
                reference,
                root,
                allow_network,
                &mut options.data,
                &mut budget,
            )?;
        }
        if ["callout", "expand"].contains(&block.tag.as_str())
            && let Some(body) = block.value.get("body").and_then(Value::as_str)
        {
            resolve_markdown_media(body, root, allow_network, &mut options.data, &mut budget)?;
        }
    }
    if let Some(prolog) = doc.frontmatter.get("prolog").and_then(Value::as_str) {
        resolve_markdown_media(prolog, root, allow_network, &mut options.data, &mut budget)?;
    }
    resolve_markdown_media(
        &doc.body,
        root,
        allow_network,
        &mut options.data,
        &mut budget,
    )?;
    // Resolve logos after other media: if a file is also a report image, its ordinary full-sized
    // copy must not be replaced by the bounded prefix used to diagnose an oversized logo.
    for key in ["logo", "logo-dark"] {
        if let Some(file) = cascade.document["company"][key].as_str() {
            resolve_company_logo(
                file,
                root,
                richview_core::COMPANY_LOGO_MAX_BYTES,
                &mut options.data,
                &mut budget,
            )?;
        }
    }
    // The background picture (STY-74) is bounded the same way, by its own limit.
    if let Some(file) = cascade
        .document
        .pointer("/settings/background/picture")
        .and_then(Value::as_str)
    {
        resolve_company_logo(
            file,
            root,
            richview_core::BACKGROUND_PICTURE_MAX_BYTES,
            &mut options.data,
            &mut budget,
        )?;
    }
    for finding in std::mem::take(&mut budget.findings) {
        if !doc
            .diagnostics
            .iter()
            // One diagnostic per cause: the inventory check may have named this file already,
            // and calls a remote address remote where this host calls it outside the folder.
            .any(|d| {
                ASSET_FINDINGS.contains(&d.code.as_str())
                    && ASSET_FINDINGS.contains(&finding.code.as_str())
                    && (d.message == finding.message
                        || d.message.contains(&format!("'{}'", finding.path)))
            })
        {
            doc.diagnostics.push(finding);
        }
    }
    if include_runtime {
        load_runtime(&mut options)?;
    }
    Ok((doc, options))
}

fn resolve_markdown_media(
    markdown: &str,
    root: &Path,
    allow_network: bool,
    data: &mut BTreeMap<String, String>,
    budget: &mut ResourceBudget,
) -> Result<(), String> {
    for event in
        pulldown_cmark::Parser::new_ext(markdown, richview_core::document::MARKDOWN_OPTIONS)
    {
        if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Image { dest_url, .. }) = event {
            resolve_media(&dest_url, root, allow_network, data, budget)?;
        }
    }
    Ok(())
}

fn resolve_media(
    reference: &str,
    root: &Path,
    _allow_network: bool,
    data: &mut BTreeMap<String, String>,
    budget: &mut ResourceBudget,
) -> Result<(), String> {
    if data.contains_key(reference) {
        return Ok(());
    }
    if reference.starts_with("data:") {
        return Ok(());
    }
    let path = match contained_path(root, reference) {
        Ok(path) => path,
        // Reported once as `report.asset.missing`; the renderer shows its placeholder.
        Err(e) => return budget.tolerate(e),
    };
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    let prefix = format!("data:{mime};base64,");
    let overhead = prefix.len() + reference.len();
    if overhead > budget.remaining() {
        return Err(budget.error(reference));
    }
    let raw_limit = (budget.remaining() - overhead) / 4 * 3;
    let bytes = bounded_read_limit(&path, raw_limit, &budget.error(reference))?;
    insert_media(reference, &prefix, bytes, data, budget)?;
    Ok(())
}

/// Read a logo within the report boundary, retaining one extra byte to diagnose an oversized file.
fn resolve_company_logo(
    reference: &str,
    root: &Path,
    max_bytes: usize,
    data: &mut BTreeMap<String, String>,
    budget: &mut ResourceBudget,
) -> Result<(), String> {
    if data.contains_key(reference) || !richview_core::document::report_relative_path(reference) {
        return Ok(());
    }
    let Ok(path) = contained_path(root, reference) else {
        // Core reports the missing or invalid logo, without preventing the rest of the report.
        return Ok(());
    };
    if !path.is_file() {
        return Ok(());
    }
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    let prefix = format!("data:{mime};base64,");
    let mut bytes = Vec::new();
    let Ok(file) = fs::File::open(&path) else {
        return Ok(());
    };
    if file
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Ok(());
    }
    insert_media(reference, &prefix, bytes, data, budget)
}

fn insert_media(
    reference: &str,
    prefix: &str,
    bytes: Vec<u8>,
    data: &mut BTreeMap<String, String>,
    budget: &mut ResourceBudget,
) -> Result<(), String> {
    let length = prefix.len() + bytes.len().div_ceil(3) * 4;
    budget.charge(reference, length + reference.len())?;
    let mut encoded = String::with_capacity(length);
    encoded.push_str(prefix);
    STANDARD.encode_string(&bytes, &mut encoded);
    data.insert(reference.into(), encoded);
    Ok(())
}

/// The text of the style `reference` names, for the style at `field`. A file that is not UTF-8
/// is a `style.unresolved` finding at its first byte that is not (TYP-56).
fn resolve_style(
    reference: &str,
    root: &Path,
    field: &str,
    budget: &mut ResourceBudget,
) -> Result<String, String> {
    if reference.contains('\n') || richview_core::is_style_name(reference) {
        budget.charge(reference, reference.len())?;
        return Ok(reference.into());
    }
    let path = contained_path(root, reference)?;
    let content = String::from_utf8(bounded_read_limit(
        &path,
        budget.remaining(),
        &budget.error(reference),
    )?)
    .map_err(|error| {
        let valid = error.utf8_error().valid_up_to();
        let before = String::from_utf8_lossy(&error.as_bytes()[..valid]);
        let mut finding = richview_core::document::Diagnostic::new(
            "error",
            "style.unresolved",
            format!("Styling file '{reference}' is not UTF-8 text."),
            field,
        )
        .fix("Save the file as UTF-8.")
        .in_file(reference);
        finding.line = Some(before.matches('\n').count() + 1);
        finding.column = Some(before.rsplit('\n').next().unwrap_or("").chars().count() + 1);
        finding.tag = field.split('.').next().map(str::to_owned);
        serde_json::to_string(&[finding]).unwrap()
    })?;
    budget.charge(reference, content.len())?;
    Ok(content)
}

/// Record, for the Parquet file `holder` names in its `data`, which columns its block uses:
/// every text in `block`, of which the reader takes those that are column names. `named` says
/// the block names each field it reads, as a chart does (TYP-36) and a table that lists its
/// `columns`; any other block reads every column.
fn note_parquet_names(
    holder: &Value,
    block: &Value,
    named: bool,
    names: &mut BTreeMap<String, Option<Vec<String>>>,
) {
    let Some(reference) = holder.pointer("/data/parquet").and_then(Value::as_str) else {
        return;
    };
    fn texts(value: &Value, into: &mut Vec<String>) {
        match value {
            Value::String(text) => into.push(text.clone()),
            Value::Array(items) => items.iter().for_each(|item| texts(item, into)),
            Value::Object(map) => {
                for (key, item) in map {
                    into.push(key.clone());
                    texts(item, into);
                }
            }
            _ => {}
        }
    }
    let named = named.then(|| {
        let mut found = Vec::new();
        texts(block, &mut found);
        found
    });
    match (names.get_mut(reference), named) {
        (None, named) => {
            names.insert(reference.to_owned(), named);
        }
        (Some(Some(all)), Some(more)) => all.extend(more),
        (Some(slot), None) => *slot = None,
        (Some(None), Some(_)) => {}
    }
}

/// Read the Parquet file a block names in `data.parquet`, once per report, in the restricted
/// worker, which returns only the columns the report's blocks name. A file that gives no rows
/// is a finding at each block that names it (FEA-RP-30).
fn resolve_parquet(
    reference: &str,
    field: &str,
    line: usize,
    root: &Path,
    data: &mut BTreeMap<String, String>,
    budget: &mut ResourceBudget,
) -> Result<(), String> {
    use crate::connectors::parquet_source;
    use richview_core::document::Diagnostic;
    if data.contains_key(reference) {
        return Ok(());
    }
    let finding = |level: &str, code: &str, message: String, fix: &str| {
        let mut finding = Diagnostic::new(level, code, message, format!("{field}.data.parquet"))
            .fix(fix)
            .in_file(reference);
        finding.line = Some(line);
        finding.tag = field.split('.').next().map(str::to_owned);
        finding
    };
    if !budget.parquet_refused.contains_key(reference) {
        let path = match contained_path(root, reference) {
            Ok(path) => path,
            Err(e) => return budget.tolerate(e),
        };
        let names = budget.parquet_names.get(reference).cloned().flatten();
        match parquet_source::read_columns(&path, reference, None, names.as_deref()) {
            Ok(table) => {
                if table.text.len() + reference.len() > budget.remaining() {
                    return Err(budget.error(reference));
                }
                budget.charge(reference, table.text.len() + reference.len())?;
                for (column, count) in table.non_finite {
                    budget.findings.push(finding(
                        "warning",
                        "data.parquet.non-finite",
                        format!(
                            "Column '{column}' of Parquet file '{reference}' holds {count} NaN or infinite value{}, read as missing.",
                            if count == 1 { "" } else { "s" }
                        ),
                        "Write those values as nulls when you export the file, or check the calculation that produced them.",
                    ));
                }
                data.insert(reference.into(), table.text);
                return Ok(());
            }
            Err(refusal) => {
                budget.parquet_refused.insert(reference.to_owned(), refusal);
            }
        }
    }
    let refusal = &budget.parquet_refused[reference];
    let found = finding("error", refusal.code, refusal.message.clone(), &refusal.fix);
    budget.findings.push(found);
    Ok(())
}

/// Reads the CSV, JSON or Parquet file `value` names into `data`. `field` is the path of the
/// block field that holds `value`, such as `chart` or `scrolly.chart`, and `line` is its block's.
fn resolve_data(
    value: &Value,
    field: &str,
    line: usize,
    root: &Path,
    _allow_network: bool,
    data: &mut BTreeMap<String, String>,
    budget: &mut ResourceBudget,
) -> Result<(), String> {
    // `data.url` names no report-folder file, and this host does not load it; the core reports
    // it once, at the field (`data.unresolved`).
    for kind in ["csv", "json"] {
        let Some(reference) = value.get(kind).and_then(Value::as_str) else {
            continue;
        };
        if data.contains_key(reference) {
            continue;
        }
        if reference.len() > budget.remaining() {
            return Err(budget.error(reference));
        }
        let content_limit = budget.remaining() - reference.len();
        let path = match contained_path(root, reference) {
            Ok(path) => path,
            Err(e) => {
                budget.tolerate(e)?;
                continue;
            }
        };
        let content = match String::from_utf8(bounded_read_limit(
            &path,
            content_limit,
            &budget.error(reference),
        )?) {
            Ok(content) => content,
            // A file that is not UTF-8 text fails only the blocks that read it (FEA-RP-30); each
            // reads it again and gets its own finding.
            Err(error) => {
                let valid = error.utf8_error().valid_up_to();
                let finding = not_utf8(field, kind, reference, &error.as_bytes()[..valid]);
                budget.findings.push(finding);
                continue;
            }
        };
        budget.charge(reference, content.len() + reference.len())?;
        data.insert(reference.into(), content);
    }
    if let Some(reference) = value.get("parquet").and_then(Value::as_str) {
        resolve_parquet(reference, field, line, root, data, budget)?;
    }
    for key in ["sql", "metricflow", "cache"] {
        // Validation passes the block over but still checks that the connector it names exists.
        if budget.skip_connectors
            && let Some(name) = value
                .get(key)
                .and_then(|block| block.get("connector"))
                .and_then(Value::as_str)
            && !budget.declared.contains(name)
        {
            budget.findings.push(
                richview_core::document::Diagnostic::new(
                    "error",
                    "resource.resolve",
                    format!("The chart names the connector '{name}', which report.connectors.yaml does not declare."),
                    format!("{field}.data.{key}.connector"),
                )
                .fix(format!(
                    "Declare '{name}' under connectors: in report.connectors.yaml beside the report, or read the data from a csv or json file in the report folder."
                )),
            );
        }
        if value.get(key).is_some() && !budget.skip_connectors {
            return Err(root_error_fix(
                "resource.resolve",
                &format!("{field}.data.{key}"),
                format!("{CONNECTOR_UNSET} (data.{key})."),
                "Declare the connector in report.connectors.yaml beside the report (examples/sql-report shows the form), and for a MetricFlow runtime also pass --connectors <connectors.toml>; or save the result as a csv or json in the report folder and name that file in data.",
            ));
        }
    }
    Ok(())
}

/// The finding for a data file that is not UTF-8 text, at its first byte that is not (TYP-56);
/// `before` is the text in front of that byte.
fn not_utf8(
    field: &str,
    kind: &str,
    reference: &str,
    before: &[u8],
) -> richview_core::document::Diagnostic {
    let before = String::from_utf8_lossy(before);
    let mut finding = richview_core::document::Diagnostic::new(
        "error",
        &format!("data.{kind}"),
        format!("Data file '{reference}' is not UTF-8 text."),
        format!("{field}.data.{kind}"),
    )
    .fix(if kind == "csv" {
        "Save the file as UTF-8; in Excel, choose CSV UTF-8 (Comma delimited) under Save As."
    } else {
        "Save the file as UTF-8."
    })
    .in_file(reference);
    finding.line = Some(before.matches('\n').count() + 1);
    finding.column = Some(before.rsplit('\n').next().unwrap_or("").chars().count() + 1);
    finding.tag = field.split('.').next().map(str::to_owned);
    finding
}

use richview_core::ASSET_FINDINGS;

fn is_asset_finding(error: &str) -> bool {
    ASSET_FINDINGS.iter().any(|code| error.contains(code))
}

/// The connector names `report.connectors.yaml` declares beside the report; none when it is absent
/// or unreadable.
fn declared_connectors(root: &Path) -> std::collections::BTreeSet<String> {
    fs::read_to_string(root.join("report.connectors.yaml"))
        .ok()
        .and_then(|text| richview_core::yaml::from_str(&text).ok())
        .and_then(|value: Value| value.get("connectors")?.as_object().cloned())
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

/// The finding for a connector-backed `data` block when the report declares no connectors.
pub const CONNECTOR_UNSET: &str =
    "This report reads a connector, but the report has no report.connectors.yaml";

fn root_error_fix(code: &str, path: &str, message: String, fix: &str) -> String {
    serde_json::to_string(&vec![
        richview_core::document::Diagnostic::new("error", code, message, path).fix(fix),
    ])
    .unwrap()
}

fn root_error(code: &str, path: &str, message: String) -> String {
    serde_json::to_string(&vec![richview_core::document::Diagnostic::new(
        "error", code, message, path,
    )])
    .unwrap()
}

/// A finding on the report folder itself, at `report_folder`, with the report-relative `file` it
/// concerns. Messages name a folder or file by name, never by its place on this machine (TYP-53).
fn folder_error(code: &str, message: String, file: Option<&str>) -> String {
    let diagnostic =
        richview_core::document::Diagnostic::new("error", code, message, "report_folder");
    serde_json::to_string(&vec![match file {
        Some(file) => diagnostic.in_file(file),
        None => diagnostic,
    }])
    .unwrap()
}

/// A finding on the output, at `output`, with its path inside the report folder as `file`.
fn output_error(code: &str, message: String, file: Option<&str>) -> String {
    let diagnostic = richview_core::document::Diagnostic::new("error", code, message, "output");
    serde_json::to_string(&vec![match file {
        Some(file) => diagnostic.in_file(file),
        None => diagnostic,
    }])
    .unwrap()
}

/// The last part of `path`: how a finding names a folder or file outside the report (TYP-53).
fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(
        || ".".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// `path` relative to `root` with forward slashes, for a finding's `file`.
fn inside(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

/// Inspect the physical inventory instead of inferring a report root from the process cwd.
pub fn report_folder(source: &Path) -> Result<richview_core::document::ReportFolder, String> {
    inspect_report_folder(source, true)
}

/// What a report command is pointed at: one report, or a folder that stands for every report
/// folder below it (FEA-SF-01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Report(PathBuf),
    Folder {
        root: PathBuf,
        /// Every `.rmd` below `root`, in path order.
        reports: Vec<PathBuf>,
    },
}

impl Target {
    /// Reads `path` as a target. A `.rmd` is its report, and so is a folder holding one
    /// (`report.rmd` first). A folder with no `.rmd` of its own stands for the reports below it.
    /// Anything else that is not a folder is left for the command to answer with its own finding.
    pub fn of(path: &Path) -> Result<Self, String> {
        if !path.is_dir() {
            return Ok(Self::Report(path.to_path_buf()));
        }
        let named = path.join("report.rmd");
        if named.is_file() {
            return Ok(Self::Report(named));
        }
        let text = path.display();
        let (mut sources, _) = folder_entries(path)?;
        match sources.len() {
            1 => Ok(Self::Report(sources.remove(0))),
            0 => {
                let reports = reports_below(path)?;
                if reports.is_empty() {
                    return Err(format!(
                        "{text} is a folder with no .rmd file in it or below it: pass the report's .rmd, e.g. {text}/report.rmd"
                    ));
                }
                Ok(Self::Folder {
                    root: path.to_path_buf(),
                    reports,
                })
            }
            _ => Err(format!(
                "{text} holds several .rmd files: pass the one report's .rmd file"
            )),
        }
    }

    /// The report of a command that takes exactly one; a folder of reports names what it holds.
    pub fn one(self, command: &str) -> Result<PathBuf, String> {
        const NAMED: usize = 5;
        match self {
            Self::Report(report) => Ok(report),
            Self::Folder { root, reports } => {
                let mut names: Vec<String> = reports
                    .iter()
                    .take(NAMED)
                    .map(|report| report.display().to_string())
                    .collect();
                if reports.len() > NAMED {
                    names.push(format!("{} more", reports.len() - NAMED));
                }
                Err(format!(
                    "{} holds {} reports and {command} takes one: pass one of {}",
                    root.display(),
                    reports.len(),
                    names.join(", ")
                ))
            }
        }
    }
}

/// The `.rmd` files, in name order, and the folders directly inside `folder`. Hidden folders and
/// symlinked folders are left out, so a walk stays inside the folder it was given, and so is a
/// macOS sidecar (`._report.rmd`), which is no report.
fn folder_entries(folder: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let (mut sources, mut folders) = (vec![], vec![]);
    for entry in fs::read_dir(folder).map_err(|e| format!("{}: {e}", folder.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            if !entry.file_name().to_string_lossy().starts_with('.') {
                folders.push(path);
            }
        } else if path.is_file() && path.extension().is_some_and(|e| e == "rmd") && !sidecar(&entry)
        {
            sources.push(path);
        }
    }
    sources.sort();
    Ok((sources, folders))
}

/// Whether a folder entry is an AppleDouble sidecar, which no walk of a report folder counts.
fn sidecar(entry: &fs::DirEntry) -> bool {
    richview_core::document::apple_double_sidecar(&entry.file_name().to_string_lossy())
}

/// Every `.rmd` below `root`, in path order. A folder that holds a `.rmd` is a report folder: its
/// subfolders are that report's data, media and output, so the walk does not enter them.
fn reports_below(root: &Path) -> Result<Vec<PathBuf>, String> {
    const MAX_FOLDERS: usize = 10_000;
    let mut reports = vec![];
    let mut pending = folder_entries(root)?.1;
    let mut read = 0;
    while let Some(folder) = pending.pop() {
        read += 1;
        if read > MAX_FOLDERS {
            return Err(format!(
                "{} holds more than 10,000 folders: point at a folder closer to the reports",
                root.display()
            ));
        }
        let (sources, folders) = folder_entries(&folder)?;
        if sources.is_empty() {
            pending.extend(folders);
        } else {
            reports.extend(sources);
        }
    }
    reports.sort();
    Ok(reports)
}

pub(crate) fn inspect_report_folder(
    source: &Path,
    require_source: bool,
) -> Result<richview_core::document::ReportFolder, String> {
    let full = crate::cli::absolute(source)?;
    let root = full.parent().ok_or_else(|| {
        folder_error(
            "report.root.missing",
            "Save the source in its own named report folder.".into(),
            None,
        )
    })?;
    if fs::symlink_metadata(root).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(folder_error(
            "report.root.escape",
            format!(
                "The report folder '{}' is a symlink. Use the actual dedicated folder containing the source and assets instead of an alias to another folder.",
                name_of(root)
            ),
            None,
        ));
    }
    let root = fs::canonicalize(root).map_err(|e| {
        folder_error(
            "report.root.missing",
            format!(
                "Report folder '{}' is missing: {e}. Create a dedicated folder and save one .rmd there.",
                name_of(root)
            ),
            None,
        )
    })?;
    let name = full
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Report filename must be UTF-8")?
        .to_owned();
    if require_source {
        let actual = fs::canonicalize(&full).map_err(|e| {
            folder_error(
                "report.root.missing",
                format!(
                    "Source '{name}' is not saved in its report folder: {e}. Save one .rmd in a dedicated folder before preview/export."
                ),
                Some(&name),
            )
        })?;
        if actual.parent() != Some(root.as_path()) || !actual.is_file() {
            return Err(folder_error(
                "report.root.escape",
                format!(
                    "Source '{name}' escapes its report folder. Move the .rmd into its own folder; do not symlink it to another report."
                ),
                Some(&name),
            ));
        }
    }
    let mut files = vec![];
    let mut pending = vec![root.clone()];
    let mut inspected = 0;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|e| {
            folder_error(
                "report.root.unresolved",
                format!("Cannot inspect report folder: {e}. Use a readable dedicated folder."),
                inside(&root, &directory).as_deref(),
            )
        })? {
            let entry = entry.map_err(|e| e.to_string())?;
            inspected += 1;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                pending.push(path);
            } else if !sidecar(&entry) {
                // A macOS sidecar is another file's attributes, not a file of the report: it
                // stays out of the inventory, so nothing counts, reads or publishes it.
                files.push(
                    path.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
            if inspected > 10000 {
                return Err(folder_error("report.root.unresolved", "Folder inventory exceeds 10,000 entries. Move this report and its required assets into a dedicated smaller folder.".into(), None));
            }
        }
    }
    if !require_source {
        files.push(name.clone());
    }
    let folder = richview_core::document::ReportFolder {
        root: root.display().to_string(),
        source: name,
        files,
    };
    richview_core::document::validate_report_folder(Some(&folder))
        .map_err(|d| serde_json::to_string(&d).unwrap())?;
    Ok(folder)
}

/// Also used by static hosting; percent-decoding occurs before this boundary.
pub fn contained_path(root: &Path, reference: &str) -> Result<PathBuf, String> {
    if !richview_core::document::report_relative_path(reference) {
        return Err(root_error(
            "report.asset.escape",
            reference,
            // Messages reach MCP clients and hosted servers: they name the reference, never the host's path.
            format!(
                "Reference '{reference}' escapes the report folder. Copy the file into that folder and use a relative path."
            ),
        ));
    }
    if richview_core::document::apple_double_sidecar(reference) {
        return Err(root_error(
            "report.asset.missing",
            reference,
            format!(
                "Reference '{reference}' names a macOS sidecar file, which holds another file's attributes: a report never reads it. Name the file it stands beside."
            ),
        ));
    }
    let path = Path::new(reference);
    if path.is_absolute()
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("Reference escapes report directory: {reference}"));
    }
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let resolved = fs::canonicalize(root.join(path)).map_err(|_| root_error("report.asset.missing", reference, format!("Cannot resolve '{reference}' inside the report folder. Add this local file to the report folder.")))?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        return Err(root_error(
            "report.asset.escape",
            reference,
            format!(
                "Reference '{reference}' escapes the report folder through a symlink or is not a regular file. Move the file inside this report folder."
            ),
        ));
    }
    Ok(resolved)
}

pub fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
    bounded_read_limit(path, crate::MAX_INPUT_BYTES, "File grew past 32 MiB limit")
}

fn bounded_read_limit(path: &Path, limit: usize, limit_error: &str) -> Result<Vec<u8>, String> {
    let limit = limit.min(crate::MAX_INPUT_BYTES);
    let metadata = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !metadata.is_file() || metadata.len() > crate::MAX_INPUT_BYTES as u64 {
        return Err(format!(
            "{} is not a regular file below 32 MiB",
            path.display()
        ));
    }
    if metadata.len() > limit as u64 {
        return Err(limit_error.into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(limit_error.into());
    }
    Ok(bytes)
}

/// Loads only the runtime compiled into this native host (ARC-07).
pub fn load_runtime(options: &mut RenderOptions) -> Result<(), String> {
    let runtime = crate::runtime::require()?;
    options.runtime_js = Some(runtime.js.clone());
    options.runtime_wasm = Some(runtime.wasm_gzip_base64.clone());
    options.runtime_wasm_codec = "gzip".into();
    Ok(())
}

pub fn render_file(path: &Path, styles: &[String]) -> Result<RenderedReport, String> {
    let source = String::from_utf8(bounded_read(path)?).map_err(|e| e.to_string())?;
    let (document, options) = resolve_document_options(&source, path, styles)?;
    richview_core::document::render_document(document, &options)
        .map_err(|e| serde_json::to_string(&e).unwrap_or_default())
}

/// Every materialized output stays under the report's exact `Rendered` child.
pub fn output_path(source: &Path, requested: Option<&Path>) -> Result<PathBuf, String> {
    let folder = report_folder(source)?;
    let root = PathBuf::from(folder.root);
    let root_dir = root.clone();
    let rendered = root.join("Rendered");
    let path = match requested {
        None => rendered.join(Path::new(&folder.source).with_extension("html")),
        Some(path) if path.components().count() == 1 => rendered.join(path),
        Some(path) => crate::cli::absolute(path)?,
    };
    let relative = path.strip_prefix(&rendered).map_err(|_| {
        output_error(
            "report.output.escape",
            format!(
                "Output '{}' must be inside the report folder's Rendered folder. Use -o filename.html or a path inside Rendered.",
                name_of(&path)
            ),
            None,
        )
    })?;
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
        || relative.as_os_str().is_empty()
    {
        return Err(output_error("report.output.escape", "Output must use a normal relative filename under Rendered; parent/absolute escapes are forbidden.".into(), None));
    }
    let mut current = root;
    for component in std::iter::once(std::ffi::OsStr::new("Rendered")).chain(
        relative
            .parent()
            .into_iter()
            .flat_map(|p| p.components())
            .map(|c| c.as_os_str()),
    ) {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir()=>return Err(output_error("report.output.escape", "Output directories cannot be symlinks/files. Replace this path with a real Rendered directory inside the report folder.".into(), inside(&root_dir, &current).as_deref())),
            Ok(_)=>{},Err(e) if e.kind()==std::io::ErrorKind::NotFound=>fs::create_dir(&current).map_err(|e|e.to_string())?,Err(e)=>return Err(e.to_string()),
        }
    }
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink() || !m.is_file()) {
        return Err(output_error(
            "report.output.escape",
            "Output cannot overwrite a symlink or directory. Use a regular file inside Rendered."
                .into(),
            inside(&root_dir, &path).as_deref(),
        ));
    }
    Ok(path)
}

/// `Rendered/<source-basename>.pdf`, or a requested `.pdf` name directly inside `Rendered`.
pub fn pdf_output_path(source: &Path, requested: Option<&Path>) -> Result<PathBuf, String> {
    let folder = report_folder(source)?;
    let default = Path::new(&folder.source).with_extension("pdf");
    let target = output_path(source, Some(requested.unwrap_or(&default)))?;
    let root = PathBuf::from(folder.root);
    let rendered = root.join("Rendered");
    if target.parent() != Some(rendered.as_path()) || !crate::cli::names_pdf(&target) {
        return Err(output_error(
            "report.output.invalid",
            "Use a .pdf filename directly inside Rendered for PDF output.".into(),
            inside(&root, &target).as_deref(),
        ));
    }
    Ok(target)
}

/// HTML embeds CSS/fonts/data/media/runtime; copied local resources also keep authored links valid.
/// The report-folder files that Markdown links point to, read from the Markdown itself: a file a
/// block also reads is still a link target.
pub fn linked_paths(
    document: &richview_core::document::Document,
) -> std::collections::BTreeSet<String> {
    let mut references = std::collections::BTreeSet::new();
    for markdown in std::iter::once(document.body.as_str())
        .chain(document.frontmatter["prolog"].as_str())
        .chain(
            document
                .blocks
                .iter()
                .filter(|b| b.tag == "callout")
                .filter_map(|b| b.value["body"].as_str()),
        )
    {
        for event in
            pulldown_cmark::Parser::new_ext(markdown, richview_core::document::MARKDOWN_OPTIONS)
        {
            if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link { dest_url, .. }) = event
            {
                let path = percent_decode(dest_url.split(['?', '#']).next().unwrap_or_default());
                if !path.is_empty() && richview_core::document::report_relative_path(&path) {
                    references.insert(path);
                }
            }
        }
    }
    references
}

/// A link target as the file name it names: `tr%C5%BEby.csv` is `tržby.csv`.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_owned())
}

/// The linked files of a report as base64, keyed by report-relative path, for a publication
/// (FEA-SF-52). A missing file fails with `report.asset.missing`.
pub fn read_linked_files(
    root: &Path,
    document: &richview_core::document::Document,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    use base64::Engine;
    let mut files = std::collections::BTreeMap::new();
    for reference in linked_paths(document) {
        if reference.to_lowercase().ends_with(".rmd") {
            continue;
        }
        let bytes = bounded_read(&contained_path(root, &reference)?)?;
        files.insert(
            reference,
            base64::engine::general_purpose::STANDARD.encode(bytes),
        );
    }
    Ok(files)
}

/// True when `error` carries findings, all of them for files absent from the report folder.
fn only_missing_files(error: &str) -> bool {
    serde_json::from_str::<Vec<richview_core::document::Diagnostic>>(error).is_ok_and(|findings| {
        !findings.is_empty() && findings.iter().all(|f| f.code == "report.asset.missing")
    })
}

pub fn export_report(
    source: &Path,
    requested: Option<&Path>,
    report: &RenderedReport,
    options: &RenderOptions,
) -> Result<PathBuf, String> {
    let target = output_path(source, requested)?;
    if !report.charts.is_empty() && options.runtime_wasm.is_none() {
        return Err(output_error("report.output.runtime", "Interactive chart runtime is missing. Build/package RichView with richview-build before exporting this report.".into(), None));
    }
    let root = PathBuf::from(
        options
            .report_folder
            .as_ref()
            .ok_or("Missing report folder")?
            .root
            .clone(),
    );
    let rendered = root.join("Rendered");
    if target.parent() != Some(rendered.as_path()) || target.extension().is_none_or(|e| e != "html")
    {
        return Err(output_error("report.output.invalid", "Use a .html filename directly inside Rendered so every bundled relative asset link stays portable.".into(), inside(&root, &target).as_deref()));
    }
    let source_text = String::from_utf8(bounded_read(source)?).map_err(|e| e.to_string())?;
    let document = richview_core::document::parse(&source_text)
        .map_err(|d| serde_json::to_string(&d).unwrap())?;
    // Data, styles and media are embedded; only the files that Markdown links point to travel
    // beside the HTML, at the same relative paths (TYP-42, FEA-RP-40).
    let references = linked_paths(&document);
    for reference in references.iter().map(String::as_str) {
        if richview_core::document::report_relative_path(reference) {
            let original = match contained_path(&root, reference) {
                Ok(original) => original,
                // The render already reported this link and left its text unlinked; the draft is
                // still written, so the author sees every finding at once (TYP-54, TYP-94).
                Err(error) if only_missing_files(&error) => continue,
                Err(error) => return Err(error),
            };
            let destination = output_path(source, Some(&rendered.join(reference)))?;
            if destination == target
                || original == target
                || original.starts_with(&rendered)
                || reference.to_lowercase().ends_with(".rmd")
            {
                return Err(output_error("report.output.collision", "An authored asset collides with generated output/source. Keep input assets outside Rendered and use a distinct HTML basename.".into(), Some(reference)));
            }
            fs::copy(original, destination).map_err(|e| e.to_string())?;
        }
    }
    fs::write(&target, &report.html).map_err(|e| e.to_string())?;
    Ok(target)
}

#[cfg(all(test, not(feature = "embedded-runtime")))]
mod runtime_tests {
    #[test]
    fn a_native_host_without_an_embedded_runtime_refuses_output() {
        let mut options = richview_core::document::RenderOptions::default();
        let error = super::load_runtime(&mut options).unwrap_err();
        let findings: Vec<richview_core::document::Diagnostic> =
            serde_json::from_str(&error).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "report.output.runtime");
        assert!(findings[0].message.contains("cargo run -p richview-build"));
        assert!(options.runtime_js.is_none() && options.runtime_wasm.is_none());
    }
}

#[cfg(test)]
mod company_logo_tests {
    use super::*;

    #[test]
    fn logo_reads_only_a_bounded_prefix_for_core_to_diagnose() {
        let root = tempfile::tempdir().unwrap();
        let source = vec![7; richview_core::COMPANY_LOGO_MAX_BYTES + 200];
        fs::write(root.path().join("logo.bin"), source).unwrap();
        let mut data = BTreeMap::new();
        let mut budget = ResourceBudget::default();
        resolve_company_logo(
            "logo.bin",
            root.path(),
            richview_core::COMPANY_LOGO_MAX_BYTES,
            &mut data,
            &mut budget,
        )
        .unwrap();
        let encoded = data["logo.bin"].split_once(";base64,").unwrap().1;
        let bytes = STANDARD.decode(encoded).unwrap();
        assert_eq!(bytes.len(), richview_core::COMPANY_LOGO_MAX_BYTES + 1);
        assert_eq!(
            richview_core::validate_company_logo(&bytes),
            Err(richview_core::CompanyLogoError::TooLarge)
        );
    }

    #[test]
    fn an_oversized_logo_does_not_truncate_the_same_file_used_as_report_media() {
        let root = tempfile::tempdir().unwrap();
        let source = vec![7; richview_core::COMPANY_LOGO_MAX_BYTES + 200];
        fs::write(root.path().join("shared.png"), &source).unwrap();
        let mut data = BTreeMap::new();
        let mut budget = ResourceBudget::default();
        resolve_media("shared.png", root.path(), false, &mut data, &mut budget).unwrap();
        resolve_company_logo(
            "shared.png",
            root.path(),
            richview_core::COMPANY_LOGO_MAX_BYTES,
            &mut data,
            &mut budget,
        )
        .unwrap();
        let encoded = data["shared.png"].split_once(";base64,").unwrap().1;
        assert_eq!(STANDARD.decode(encoded).unwrap(), source);
    }

    #[cfg(unix)]
    #[test]
    fn logo_does_not_read_a_symlink_outside_the_report_folder() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("logo.png")).unwrap();
        let mut data = BTreeMap::new();
        resolve_company_logo(
            "logo.png",
            root.path(),
            richview_core::COMPANY_LOGO_MAX_BYTES,
            &mut data,
            &mut ResourceBudget::default(),
        )
        .unwrap();
        assert!(data.is_empty());
    }
}
