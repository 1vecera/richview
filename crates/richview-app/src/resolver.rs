use crate::input_limits::{InputSizes, encoded, locate, locate_encoded};
use base64::{Engine, engine::general_purpose::STANDARD};
use richview_core::document::{Document, RenderOptions, RenderedReport};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// The folder of the report a command is working on, opened once for the whole command: the source
/// is read from it and every file the report names is looked up in it, so a folder that is
/// replaced, or renamed to another place, while the command runs cannot send it to read another
/// folder than the one it began with. Held per thread, for the length of one call of [`pinned`].
#[cfg(unix)]
mod pin {
    use crate::beneath::Dir;
    use std::{
        cell::RefCell,
        ffi::OsString,
        path::{Path, PathBuf},
        rc::Rc,
    };

    pub(super) struct Pinned {
        /// The path the command was given for the source.
        pub key: PathBuf,
        /// The folder's own path and the source's name in it, as they were when it was opened.
        pub root: PathBuf,
        pub name: OsString,
        pub dir: Dir,
    }

    thread_local! {
        static PINNED: RefCell<Option<Rc<Pinned>>> = const { RefCell::new(None) };
    }

    /// Puts `pinned` in force until the returned guard is dropped.
    pub(super) struct Guard(Option<Rc<Pinned>>);

    impl Guard {
        pub(super) fn set(pinned: Pinned) -> Self {
            Self(PINNED.with(|slot| slot.replace(Some(Rc::new(pinned)))))
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            PINNED.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }

    pub(super) fn current() -> Option<Rc<Pinned>> {
        PINNED.with(|slot| slot.borrow().clone())
    }

    /// A handle on the pinned folder when `root` is the folder, else none.
    pub(super) fn folder_at(root: &Path) -> Option<std::io::Result<Dir>> {
        current()
            .filter(|pinned| pinned.root == root)
            .map(|pinned| pinned.dir.try_clone())
    }
}

/// Runs `run` with the folder of the report at `source` pinned: opened once, and the source and
/// everything the report names read from that one handle. A call inside it for the same `source`
/// uses the folder already pinned. A source or folder that cannot be opened is not pinned, and
/// `run` finds and reports what is wrong with it, as it always did; where the system has no
/// directory handles, `run` is all that happens.
pub(crate) fn pinned<T>(source: &Path, run: impl FnOnce() -> T) -> T {
    #[cfg(unix)]
    {
        if pin::current().is_some_and(|held| held.key == source) {
            return run();
        }
        let opened = fs::canonicalize(source).ok().and_then(|canonical| {
            let root = canonical.parent()?.to_path_buf();
            let name = canonical.file_name()?.to_owned();
            let dir = crate::beneath::Dir::open_path(&root, true).ok()?;
            Some(pin::Pinned {
                key: source.to_path_buf(),
                root,
                name,
                dir,
            })
        });
        let _held = opened.map(pin::Guard::set);
        run()
    }
    #[cfg(not(unix))]
    {
        let _ = source;
        run()
    }
}

/// The source file of a pinned report, opened in its pinned folder, when `source` is the one
/// pinned.
pub(crate) fn pinned_source(source: &Path) -> Option<std::io::Result<fs::File>> {
    #[cfg(unix)]
    {
        pin::current()
            .filter(|held| held.key == source)
            .map(|held| held.dir.open_file(&held.name))
    }
    #[cfg(not(unix))]
    {
        let _ = source;
        None
    }
}

/// The source file's path and its folder's, canonical: those of the pinned report where there is
/// one, so that nothing is looked up again by a path that may name something else by now.
fn located(source: &Path) -> Result<(PathBuf, PathBuf), String> {
    #[cfg(unix)]
    {
        if let Some(held) = pin::current().filter(|held| held.key == source) {
            return Ok((held.root.join(&held.name), held.root.clone()));
        }
    }
    let canonical = fs::canonicalize(source).map_err(|e| format!("{}: {e}", source.display()))?;
    let root = canonical
        .parent()
        .ok_or("Source file has no parent directory")?
        .to_path_buf();
    Ok((canonical, root))
}

/// Aggregate retained native data, encoded media and style contents per report.
pub const MAX_RESOLVED_RESOURCE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Default)]
struct ResourceBudget {
    retained: usize,
    inputs: InputSizes,
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
    resolve_version(
        source,
        source_path,
        styles,
        allow_network,
        include_runtime,
        registry,
    )
    .map(|(doc, options, _)| (doc, options))
}

/// Resolve a version and keep its input accounting for the files that accompany its output.
pub fn resolve_version(
    source: &str,
    source_path: &Path,
    styles: &[String],
    allow_network: bool,
    include_runtime: bool,
    registry: Option<&crate::connectors::Registry>,
) -> Result<(Document, RenderOptions, InputSizes), String> {
    resolve_inner(
        source,
        source_path,
        styles,
        allow_network,
        include_runtime,
        Connectors::Attached(registry),
    )
}

/// Resolution for `validate`: everything is read and checked except connector-backed blocks,
/// which need a warehouse and are passed over, so every other finding is still reported.
pub fn resolve_for_validation(
    source: &str,
    source_path: &Path,
    styles: &[String],
) -> Result<(Document, RenderOptions), String> {
    resolve_inner(
        source,
        source_path,
        styles,
        false,
        false,
        Connectors::Validate,
    )
    .map(|(doc, options, _)| (doc, options))
}

enum Connectors<'a> {
    Attached(Option<&'a crate::connectors::Registry>),
    Configured(Option<&'a Path>),
    Validate,
}

pub(crate) fn resolve_configured(
    source: &str,
    input: &crate::ops::Input,
    runtime: bool,
) -> Result<(Document, RenderOptions, InputSizes), String> {
    resolve_inner(
        source,
        &input.path,
        &input.styles,
        false,
        runtime,
        Connectors::Configured(input.connectors.as_deref()),
    )
}

fn resolve_inner(
    source: &str,
    source_path: &Path,
    styles: &[String],
    allow_network: bool,
    include_runtime: bool,
    connectors: Connectors,
) -> Result<(Document, RenderOptions, InputSizes), String> {
    pinned(source_path, || {
        resolve_pinned(
            source,
            source_path,
            styles,
            allow_network,
            include_runtime,
            connectors,
        )
    })
}

fn resolve_pinned(
    source: &str,
    source_path: &Path,
    styles: &[String],
    allow_network: bool,
    include_runtime: bool,
    connectors: Connectors,
) -> Result<(Document, RenderOptions, InputSizes), String> {
    let mut inputs = InputSizes::new(&name_of(source_path), source.len()).map_err(encoded)?;
    let folder = report_folder(source_path)?;
    let (source_path, root) = located(source_path)?;
    let root = root.as_path();
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
    let uses_connectors = doc.blocks.iter().any(|b| connector_block(&b.value["data"]));
    let declaration = if uses_connectors {
        inputs
            .local(root, "report.connectors.yaml")
            .map_err(encoded)?;
        let parsed = crate::connectors::config::declaration(root);
        if matches!(connectors, Connectors::Configured(_)) {
            parsed.map_err(|e| root_error("resource.resolve", "connectors", e))?
        } else {
            parsed.unwrap_or_default()
        }
    } else {
        None
    };
    inputs
        .referenced(root, &doc, styles, declaration.as_ref())
        .map_err(|e| encoded(locate(e, &doc)))?;
    let declared = declaration.as_ref().map(|d| d.names()).unwrap_or_default();
    let loaded;
    let registry = match connectors {
        Connectors::Attached(registry) => registry,
        Connectors::Configured(config) if uses_connectors => {
            loaded = crate::connectors::config::bind_registry(root, config, declaration)
                .map_err(|e| root_error("resource.resolve", "connectors", e))?;
            loaded.as_ref()
        }
        _ => None,
    };
    if let Some(registry) = registry {
        let mut extra = Vec::new();
        crate::connectors::resolve::resolve_with_limits(
            &mut doc,
            root,
            registry,
            &mut inputs,
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
        inputs,
        skip_connectors: matches!(connectors, Connectors::Validate),
        declared,
        ..Default::default()
    };
    // How many styles the list names, before one is read: each name is a read and a copy.
    if let Some(problem) = richview_core::style_list_problem(&doc.frontmatter, |_| None) {
        return Err(serde_json::to_string(&[problem]).unwrap());
    }
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
    // What the files add up to, each counted as often as the list names it.
    if let Some(problem) = richview_core::style_list_problem(&doc.frontmatter, |name| {
        options.data.get(name).map(String::len)
    }) {
        return Err(serde_json::to_string(&[problem]).unwrap());
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
            )
            .map_err(|e| locate_encoded(e, &doc))?;
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
            )
            .map_err(|e| locate_encoded(e, &doc))?;
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
    for (text, _) in doc.deck_markdown() {
        resolve_markdown_media(text, root, allow_network, &mut options.data, &mut budget)?;
    }
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
    budget.inputs.check().map_err(encoded)?;
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
    Ok((doc, options, budget.inputs))
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
    let bytes = budget.inputs.read(root, reference)?;
    insert_media(reference, &prefix, &bytes, data, budget)?;
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
    if budget.inputs.is_shared(reference) {
        let bytes = budget.inputs.read(root, reference)?;
        return insert_media(
            reference,
            &prefix,
            &bytes[..bytes.len().min(max_bytes + 1)],
            data,
            budget,
        );
    }
    let mut bytes = Vec::new();
    let Ok(file) = open_beneath(root, reference) else {
        return Ok(());
    };
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    budget.inputs.record(reference, size).map_err(encoded)?;
    if file
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Ok(());
    }
    insert_media(reference, &prefix, &bytes, data, budget)
}

fn insert_media(
    reference: &str,
    prefix: &str,
    bytes: &[u8],
    data: &mut BTreeMap<String, String>,
    budget: &mut ResourceBudget,
) -> Result<(), String> {
    let length = prefix.len() + bytes.len().div_ceil(3) * 4;
    budget.charge(reference, length + reference.len())?;
    let mut encoded = String::with_capacity(length);
    encoded.push_str(prefix);
    STANDARD.encode_string(bytes, &mut encoded);
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
    // The reference is checked here for its findings; the file is opened beneath the folder.
    contained_path(root, reference)?;
    let bytes = budget.inputs.read(root, reference)?;
    let content = String::from_utf8(std::sync::Arc::unwrap_or_clone(bytes)).map_err(|error| {
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
                // The report carries this text, not the file: count it as the server does.
                budget
                    .inputs
                    .table(reference, table.text.len())
                    .map_err(encoded)?;
                budget.inputs.check().map_err(encoded)?;
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
        // The reference is checked here for its findings; the file is opened beneath the folder.
        if let Err(e) = contained_path(root, reference) {
            budget.tolerate(e)?;
            continue;
        }
        let bytes = budget.inputs.read(root, reference)?;
        let content = match String::from_utf8(std::sync::Arc::unwrap_or_clone(bytes)) {
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

fn connector_block(data: &Value) -> bool {
    ["sql", "metricflow", "cache"]
        .iter()
        .any(|kind| data.get(kind).is_some())
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
        } else if path.is_file()
            && path
                .extension()
                .is_some_and(|e| e == "rmd" || e == "rslide")
            && !sidecar(&entry)
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

/// The checks of a reference that need no folder: a report-relative name that is no macOS sidecar
/// and has no parent or absolute part.
fn checked_reference(reference: &str) -> Result<&Path, String> {
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
    Ok(path)
}

fn unresolved_file(reference: &str) -> String {
    root_error(
        "report.asset.missing",
        reference,
        format!(
            "Cannot resolve '{reference}' inside the report folder. Add this local file to the report folder."
        ),
    )
}

fn escaping_file(reference: &str) -> String {
    root_error(
        "report.asset.escape",
        reference,
        format!(
            "Reference '{reference}' escapes the report folder through a symlink or is not a regular file. Move the file inside this report folder."
        ),
    )
}

/// Also used by static hosting; percent-decoding occurs before this boundary.
pub fn contained_path(root: &Path, reference: &str) -> Result<PathBuf, String> {
    let path = checked_reference(reference)?;
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let resolved = fs::canonicalize(root.join(path)).map_err(|_| unresolved_file(reference))?;
    if !resolved.starts_with(&root) || !resolved.is_file() {
        return Err(escaping_file(reference));
    }
    Ok(resolved)
}

/// Opens the file `reference` names beneath `root`, and returns that open file: what is read is the
/// file that is inside the folder, not whatever the same path names a moment later. The folder is
/// opened once, and each part of the reference is looked up in the folder before it, never through
/// a link. A link that stays inside the folder is followed by hand (at most 16 of them), so a
/// report may still name a file through one; a link that leaves the folder, or a `..` that climbs
/// out of it, is `report.asset.escape`. Where the system has no directory handles, this is
/// [`contained_path`] and an open.
#[cfg(unix)]
pub(crate) fn open_beneath(root: &Path, reference: &str) -> Result<fs::File, String> {
    use crate::beneath::{Dir, Kind};
    use std::collections::VecDeque;
    use std::ffi::OsString;
    enum Step {
        Up,
        Name(OsString),
    }
    // The parts of a path, or none when it starts at the top of the file system.
    let steps = |path: &Path| -> Option<VecDeque<Step>> {
        path.components()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(Some(Step::Name(name.to_owned()))),
                Component::ParentDir => Some(Some(Step::Up)),
                Component::CurDir => None,
                Component::RootDir | Component::Prefix(_) => Some(None),
            })
            .collect()
    };
    let mut pending =
        steps(checked_reference(reference)?).ok_or_else(|| escaping_file(reference))?;
    let io_error = |error: std::io::Error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            unresolved_file(reference)
        } else {
            escaping_file(reference)
        }
    };
    // The folder the command pinned, when it is that one, else the folder at the path.
    let top = match pin::folder_at(root) {
        Some(held) => held,
        None => Dir::open_path(root, true),
    };
    let mut folders = vec![top.map_err(|e| e.to_string())?];
    let mut links = 0;
    while let Some(step) = pending.pop_front() {
        let name = match step {
            Step::Up => {
                if folders.len() == 1 {
                    return Err(escaping_file(reference));
                }
                folders.pop();
                continue;
            }
            Step::Name(name) => name,
        };
        let folder = folders.last().ok_or_else(|| escaping_file(reference))?;
        match folder.kind(&name).map_err(io_error)? {
            Kind::Link => {
                links += 1;
                let target = folder.read_link(&name).map_err(io_error)?;
                if links > 16 {
                    return Err(escaping_file(reference));
                }
                // A link written with the folder's whole address, as the folder is named or as
                // the system spells it, stays inside it: its rest is looked up from the top.
                let target = match within(root, Path::new(&target)) {
                    Some(rest) => {
                        folders.truncate(1);
                        steps(&rest)
                    }
                    None => steps(Path::new(&target)),
                }
                .ok_or_else(|| escaping_file(reference))?;
                for step in target.into_iter().rev() {
                    pending.push_front(step);
                }
            }
            Kind::Folder if !pending.is_empty() => {
                let inner = folder.open_dir(&name).map_err(io_error)?;
                folders.push(inner);
            }
            Kind::File(_) if pending.is_empty() => {
                return folder.open_file(&name).map_err(io_error);
            }
            _ => return Err(escaping_file(reference)),
        }
    }
    Err(escaping_file(reference))
}

/// What follows `root` in the absolute path `target`, when it names a place beneath `root`: the
/// folder as it was named to this command, or as its real address is.
#[cfg(unix)]
fn within(root: &Path, target: &Path) -> Option<PathBuf> {
    if !target.is_absolute() {
        return None;
    }
    if let Ok(rest) = target.strip_prefix(root) {
        return Some(rest.to_path_buf());
    }
    let real = fs::canonicalize(root).ok()?;
    target.strip_prefix(real).ok().map(Path::to_path_buf)
}

#[cfg(not(unix))]
pub(crate) fn open_beneath(root: &Path, reference: &str) -> Result<fs::File, String> {
    fs::File::open(contained_path(root, reference)?).map_err(|_| unresolved_file(reference))
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
    let (document, options) = pinned(path, || {
        let source = crate::cli::read_source(path)?;
        resolve_document_options(&source, path, styles)
    })?;
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
    make_output_folders(&root_dir, relative.parent().unwrap_or(Path::new("")))?;
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

/// Makes `Rendered` in the report folder `root` and the folders `below` it that an output is named
/// in, and refuses a link or file standing in any of them. Where the system has directory handles,
/// each folder is made and opened in the one before, never through a link, so a folder replaced by
/// a link after it was checked cannot make a folder anywhere else.
#[cfg(unix)]
fn make_output_folders(root: &Path, below: &Path) -> Result<(), String> {
    use crate::beneath::{Dir, Kind};
    let refused = |path: &Path| {
        output_error("report.output.escape", "Output directories cannot be symlinks/files. Replace this path with a real Rendered directory inside the report folder.".into(), inside(root, path).as_deref())
    };
    let mut folder = Dir::open_path(root, true).map_err(|e| e.to_string())?;
    let mut current = root.to_path_buf();
    for part in std::iter::once(std::ffi::OsStr::new("Rendered"))
        .chain(below.components().map(|part| part.as_os_str()))
    {
        current.push(part);
        match folder.kind(part) {
            Ok(Kind::Folder) => {}
            Ok(_) => return Err(refused(&current)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => match folder.make_dir(part) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            },
            Err(e) => return Err(e.to_string()),
        }
        folder = folder.open_dir(part).map_err(|_| refused(&current))?;
    }
    Ok(())
}

/// The same where the system has no directory handles to use: each folder is checked and made by
/// its pathname.
#[cfg(not(unix))]
fn make_output_folders(root: &Path, below: &Path) -> Result<(), String> {
    let mut current = root.to_path_buf();
    for component in std::iter::once(std::ffi::OsStr::new("Rendered"))
        .chain(below.components().map(|part| part.as_os_str()))
    {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => return Err(output_error("report.output.escape", "Output directories cannot be symlinks/files. Replace this path with a real Rendered directory inside the report folder.".into(), inside(root, &current).as_deref())),
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&current).map_err(|e| e.to_string())?,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
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
                .filter(|b| b.tag == "callout" || b.tag == "expand")
                .filter_map(|b| b.value["body"].as_str()),
        )
        .chain(document.deck_markdown().into_iter().map(|(text, _)| text))
    {
        for event in
            pulldown_cmark::Parser::new_ext(markdown, richview_core::document::MARKDOWN_OPTIONS)
        {
            if let pulldown_cmark::Event::Start(pulldown_cmark::Tag::Link {
                link_type,
                dest_url,
                ..
            }) = event
                && link_type != pulldown_cmark::LinkType::Email
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
    InputSizes::new("report.rmd", 0)
        .map_err(encoded)?
        .encoded_downloads(root, document)
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
    inputs: &mut InputSizes,
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
    // Data, styles and media are embedded; only the files that Markdown links point to travel
    // beside the HTML, at the same relative paths (TYP-42, FEA-RP-40).
    let mut copies = Vec::new();
    let references = inputs.links.clone();
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
                || richview_core::document::is_source_file(reference)
            {
                return Err(output_error("report.output.collision", "An authored asset collides with generated output/source. Keep input assets outside Rendered and use a distinct HTML basename.".into(), Some(reference)));
            }
            let bytes = inputs.read(&root, reference)?;
            // The permissions came with the bytes, from the same open file.
            copies.push((destination, bytes, inputs.mode_read(reference)));
        }
    }
    for (destination, bytes, mode) in copies {
        // The place was checked before every file was read, which can take a while: check it
        // again as the file goes in, and give it the permissions its source has.
        let destination = output_path(source, Some(&destination))?;
        write_output_in(&rendered, &destination, bytes.as_slice(), mode)?;
    }
    write_output_in(
        &rendered,
        &output_path(source, Some(&target))?,
        report.html.as_bytes(),
        None,
    )?;
    Ok(target)
}

/// Writes a file of the export, inside `rendered`. The bytes go into a new file beside it that only
/// its owner can read, which is given its final permissions and then renamed over the name:
/// nobody who had the old file open reads the new bytes, a symbolic link standing at the name is
/// replaced and never followed, and a half-written file is never at the name.
///
/// The final permissions are the source file's when `mode` is one: its owner, group and other bits
/// are kept, so a private attachment stays private, and its set-user-ID, set-group-ID and sticky
/// bits never are, so an attachment cannot become a program that runs as whoever exports the
/// report. Without a `mode` (the HTML itself), a file that was there keeps its permissions, so a
/// report made private by its owner stays private when it is exported again, and a new file takes
/// the default of a new file.
///
/// A file that is to be more private than a new file is made so from its first moment: it is
/// created for its owner alone, so nobody who could open it between its creation and its final
/// permissions reads a byte of it. A file whose final permissions are those of a new file is
/// created with them, which shows nobody anything the final file would not. Group bits mean the
/// group of the file they were taken from, so the new file is handed to that group first, and when
/// it may not be, it takes no group bits at all.
///
/// Where the system has directory handles, the folder is reached from `rendered` one handle at a
/// time, none of them through a link, so a folder replaced by a link after it was checked cannot
/// send the file anywhere else: the name is looked up relative to the folder that was opened, not
/// again from the top.
#[cfg(unix)]
fn write_output_in(
    rendered: &Path,
    destination: &Path,
    bytes: &[u8],
    mode: Option<crate::input_limits::Perm>,
) -> Result<(), String> {
    use crate::beneath::{Dir, Kind};
    use std::io::Write;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let relative = destination
        .strip_prefix(rendered)
        .map_err(|_| "An output file belongs inside Rendered".to_owned())?;
    let name = relative.file_name().ok_or("An output file needs a name")?;
    let mut dir = Dir::open_path(rendered, false).map_err(|e| e.to_string())?;
    for part in relative.parent().into_iter().flat_map(Path::components) {
        dir = dir.open_dir(part.as_os_str()).map_err(|e| e.to_string())?;
    }
    let before = match dir.kind(name) {
        Ok(Kind::File(permissions)) => Some(permissions),
        Ok(_) => None,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    // The source's permissions for an attachment, else those of the file this one replaces.
    let wanted = mode.or(before);
    // A short name of its own, so a destination near the longest name a folder allows still has one.
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let temp = std::ffi::OsString::from(format!(".rv-{:016x}.tmp", u64::from_be_bytes(nonce)));
    let mut file = create_staging(&dir, &temp, wanted).map_err(|e| e.to_string())?;
    let install = |file: &mut fs::File| -> std::io::Result<()> {
        file.write_all(bytes)?;
        if let Some(wanted) = wanted {
            let mut final_mode = wanted.mode & 0o777;
            if let Some(group) = wanted.group
                && group != file.metadata()?.gid()
                && std::os::unix::fs::fchown(&*file, None, Some(group)).is_err()
            {
                final_mode &= !0o070;
            }
            file.set_permissions(fs::Permissions::from_mode(final_mode))?;
        }
        dir.rename(&temp, name)
    };
    let written = install(&mut file).map_err(|e| e.to_string());
    if written.is_err() {
        let _ = dir.remove_file(&temp);
    }
    written
}

/// The new file the bytes of an export go into. When its final permissions are to be private, it is
/// closed to everyone but its owner from the moment it exists; a file that is to take the default
/// of a new file is made with it, and shows nobody anything the final file would not.
#[cfg(unix)]
fn create_staging(
    dir: &crate::beneath::Dir,
    temp: &std::ffi::OsStr,
    wanted: Option<crate::input_limits::Perm>,
) -> std::io::Result<fs::File> {
    dir.create_new(temp, if wanted.is_some() { 0o600 } else { 0o666 })
}

/// The same where the system has no directory handles to use: the file is written beside its name
/// and renamed over it.
#[cfg(not(unix))]
fn write_output_in(
    _rendered: &Path,
    destination: &Path,
    bytes: &[u8],
    _mode: Option<crate::input_limits::Perm>,
) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or("An output file needs a folder")?;
    let name = destination
        .file_name()
        .ok_or("An output file needs a name")?;
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|e| e.to_string())?;
    let temp = parent.join(format!(".rv-{:016x}.tmp", u64::from_be_bytes(nonce)));
    let written = fs::write(&temp, bytes)
        .and_then(|()| fs::rename(&temp, destination))
        .map_err(|e| e.to_string());
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
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

#[cfg(all(test, unix))]
mod export_write_tests {
    use super::*;
    use crate::input_limits::Perm;
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn an_exported_attachment_keeps_the_permissions_of_the_file_it_came_from() {
        let dir = tempfile::tempdir().unwrap();
        let private = dir.path().join("private.csv");
        fs::write(&private, "secret").unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o600)).unwrap();
        let mut inputs = crate::input_limits::InputSizes::new("report.rmd", 0).unwrap();
        inputs.read(dir.path(), "private.csv").unwrap();
        let mode = inputs.mode_read("private.csv");
        // Whatever the umask, a file whose source others cannot read is not readable by them.
        let copy = dir.path().join("copy.csv");
        write_output_in(dir.path(), &copy, b"secret", mode).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"secret");
        assert_eq!(mode_of(&copy), 0o600);
        // Nothing is left beside it.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn a_file_replaced_by_an_export_is_a_new_file_that_an_open_reader_never_sees() {
        let dir = tempfile::tempdir().unwrap();
        let open = dir.path().join("open.csv");
        fs::write(&open, "old").unwrap();
        fs::set_permissions(&open, fs::Permissions::from_mode(0o644)).unwrap();
        // Someone who could read the old file has it open.
        let mut reader = fs::File::open(&open).unwrap();
        write_output_in(dir.path(), &open, b"secret", Some(Perm::bits(0o600))).unwrap();
        let mut seen = String::new();
        reader.read_to_string(&mut seen).unwrap();
        assert_eq!(seen, "old");
        assert_eq!(fs::read(&open).unwrap(), b"secret");
        assert_eq!(mode_of(&open), 0o600);
    }

    #[test]
    fn an_exported_file_never_takes_the_set_id_bits_of_its_source() {
        let dir = tempfile::tempdir().unwrap();
        for special in [0o4755, 0o2755, 0o6755, 0o1755] {
            let copy = dir.path().join(format!("program-{special:o}"));
            write_output_in(dir.path(), &copy, b"#!/bin/sh\n", Some(Perm::bits(special))).unwrap();
            assert_eq!(mode_of(&copy), 0o755, "{special:o}");
        }
    }

    #[test]
    fn a_symbolic_link_standing_at_the_name_is_replaced_and_never_followed() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere.txt");
        fs::write(&elsewhere, "keep").unwrap();
        let name = dir.path().join("report.html");
        std::os::unix::fs::symlink(&elsewhere, &name).unwrap();
        write_output_in(dir.path(), &name, b"<html>", None).unwrap();
        assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "keep");
        assert!(
            !fs::symlink_metadata(&name)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read(&name).unwrap(), b"<html>");
    }

    #[test]
    fn a_folder_replaced_by_a_link_after_it_was_checked_sends_the_file_nowhere() {
        let dir = tempfile::tempdir().unwrap();
        let rendered = dir.path().join("Rendered");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&rendered).unwrap();
        fs::create_dir_all(&outside).unwrap();
        // `Rendered/data` was a folder when the path was checked and is a link to somewhere else
        // by the time the file is written.
        std::os::unix::fs::symlink(&outside, rendered.join("data")).unwrap();
        let refused = write_output_in(
            &rendered,
            &rendered.join("data/a.csv"),
            b"x",
            Some(Perm::bits(0o600)),
        );
        assert!(refused.is_err());
        assert!(!outside.join("a.csv").exists());
        // A folder that is one is written into as before, and nothing is left beside the file.
        fs::create_dir_all(rendered.join("real")).unwrap();
        write_output_in(
            &rendered,
            &rendered.join("real/a.csv"),
            b"x",
            Some(Perm::bits(0o600)),
        )
        .unwrap();
        assert_eq!(fs::read(rendered.join("real/a.csv")).unwrap(), b"x");
        assert_eq!(fs::read_dir(rendered.join("real")).unwrap().count(), 1);
        // And a path that does not lie inside Rendered is refused.
        assert!(write_output_in(&rendered, &dir.path().join("a.csv"), b"x", None).is_err());
    }

    #[test]
    fn a_report_exported_again_keeps_the_permissions_it_had() {
        let dir = tempfile::tempdir().unwrap();
        // A report its owner made private stays private, and one shared with a group stays that.
        for kept in [0o600, 0o640, 0o644, 0o400] {
            let html = dir.path().join(format!("report-{kept:o}.html"));
            fs::write(&html, "old").unwrap();
            fs::set_permissions(&html, fs::Permissions::from_mode(kept)).unwrap();
            write_output_in(dir.path(), &html, b"<html>new", None).unwrap();
            assert_eq!(fs::read(&html).unwrap(), b"<html>new");
            assert_eq!(mode_of(&html), kept, "{kept:o}");
        }
        // A new report takes what a new file does, whatever the umask: its owner can read and write
        // it, and it is no program.
        let fresh = dir.path().join("fresh.html");
        write_output_in(dir.path(), &fresh, b"<html>", None).unwrap();
        assert_eq!(mode_of(&fresh) & 0o600, 0o600);
        assert_eq!(mode_of(&fresh) & 0o7111, 0);
        // Nothing is left beside them.
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 5);
    }

    #[test]
    fn a_destination_named_nearly_as_long_as_a_folder_allows_is_still_written() {
        let dir = tempfile::tempdir().unwrap();
        let long = dir.path().join(format!("{}.csv", "a".repeat(250)));
        write_output_in(dir.path(), &long, b"x", Some(Perm::bits(0o644))).unwrap();
        assert_eq!(fs::read(&long).unwrap(), b"x");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_private_export_is_created_closed_to_everyone_but_its_owner() {
        let dir = tempfile::tempdir().unwrap();
        let top = crate::beneath::Dir::open_path(dir.path(), true).unwrap();
        for private in [0o600, 0o640, 0o644, 0o400] {
            let name = std::ffi::OsString::from(format!(".rv-{private:o}.tmp"));
            create_staging(&top, &name, Some(Perm::bits(private))).unwrap();
            // Whatever the umask, no bit of the group or of everyone else is ever set.
            assert_eq!(mode_of(&dir.path().join(&name)) & 0o077, 0, "{private:o}");
        }
        // A file that takes the default of a new file is made with it.
        create_staging(&top, std::ffi::OsStr::new(".rv-new.tmp"), None).unwrap();
        assert_eq!(mode_of(&dir.path().join(".rv-new.tmp")) & 0o600, 0o600);
    }

    #[test]
    fn a_staging_file_is_never_seen_wider_than_its_owner_while_a_private_export_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let watched = dir.path().to_path_buf();
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let widest = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let watcher = {
            let (done, widest) = (done.clone(), widest.clone());
            std::thread::spawn(move || {
                while !done.load(std::sync::atomic::Ordering::SeqCst) {
                    for entry in fs::read_dir(&watched).into_iter().flatten().flatten() {
                        let name = entry.file_name();
                        if name.to_string_lossy().ends_with(".tmp")
                            && let Ok(meta) = entry.metadata()
                        {
                            widest.fetch_max(
                                meta.mode() & 0o077,
                                std::sync::atomic::Ordering::SeqCst,
                            );
                        }
                    }
                }
            })
        };
        let body = vec![7u8; 256 * 1024];
        for at in 0..150 {
            let copy = dir.path().join(format!("copy-{at}.csv"));
            write_output_in(dir.path(), &copy, &body, Some(Perm::bits(0o600))).unwrap();
        }
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        watcher.join().unwrap();
        assert_eq!(widest.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn a_re_export_keeps_the_group_of_the_file_it_replaces_or_gives_up_the_group_bits() {
        let dir = tempfile::tempdir().unwrap();
        // Another group of this person's, where there is one: the replacement is handed to it.
        let own = unsafe { libc::getegid() };
        let mut groups = vec![0 as libc::gid_t; 64];
        let count = unsafe { libc::getgroups(groups.len() as i32, groups.as_mut_ptr()) };
        let other = groups[..count.max(0) as usize]
            .iter()
            .copied()
            .find(|g| *g != own);
        if let Some(group) = other {
            let html = dir.path().join("shared.html");
            fs::write(&html, "old").unwrap();
            std::os::unix::fs::chown(&html, None, Some(group)).unwrap();
            fs::set_permissions(&html, fs::Permissions::from_mode(0o640)).unwrap();
            write_output_in(dir.path(), &html, b"<html>new", None).unwrap();
            assert_eq!(fs::metadata(&html).unwrap().gid(), group);
            assert_eq!(mode_of(&html), 0o640);
        }
        // A group this person is not in cannot be given, and then its bits are not either.
        if unsafe { libc::geteuid() } != 0 {
            let copy = dir.path().join("copy.csv");
            let stranger = Perm {
                mode: 0o664,
                group: Some(own.wrapping_add(7919)),
            };
            write_output_in(dir.path(), &copy, b"x", Some(stranger)).unwrap();
            assert_eq!(mode_of(&copy), 0o604);
        }
    }

    fn escapes(error: &str) -> bool {
        error.contains("report.asset.escape")
    }

    #[test]
    fn an_input_is_opened_beneath_the_report_folder_and_never_through_a_link_that_leaves_it() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("report");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/a.csv"), "inside").unwrap();
        fs::write(outer.path().join("secret.csv"), "outside").unwrap();
        let read = |reference: &str| -> Result<String, String> {
            let mut text = String::new();
            open_beneath(&root, reference)?
                .read_to_string(&mut text)
                .map_err(|e| e.to_string())?;
            Ok(text)
        };
        let link = |target: &str, name: &str| {
            std::os::unix::fs::symlink(target, root.join(name)).unwrap();
        };
        assert_eq!(read("data/a.csv").unwrap(), "inside");
        // Links that stay inside the folder are followed, to a file and to a folder.
        link("data/a.csv", "latest.csv");
        link("data", "alias");
        link("../data/a.csv", "data/up.csv");
        assert_eq!(read("latest.csv").unwrap(), "inside");
        assert_eq!(read("alias/a.csv").unwrap(), "inside");
        assert_eq!(read("data/up.csv").unwrap(), "inside");
        // A link written with the folder's whole address stays inside it, as it always has.
        link(root.join("data/a.csv").to_str().unwrap(), "whole.csv");
        link(root.join("data").to_str().unwrap(), "whole-folder");
        assert_eq!(read("whole.csv").unwrap(), "inside");
        assert_eq!(read("whole-folder/a.csv").unwrap(), "inside");
        // Even one that goes through the real address of a folder the system spells otherwise.
        if let Ok(real) = fs::canonicalize(&root) {
            link(real.join("data/a.csv").to_str().unwrap(), "real.csv");
            assert_eq!(read("real.csv").unwrap(), "inside");
        }
        // The same address that climbs out of the folder, and then back in, is still out.
        link(
            &format!("{}/../secret.csv", root.display()),
            "whole-sideways.csv",
        );
        assert!(escapes(&read("whole-sideways.csv").unwrap_err()));
        // Links that leave it are refused: up out of it, by an absolute name, and through a folder.
        link("../secret.csv", "out.csv");
        link(
            outer.path().join("secret.csv").to_str().unwrap(),
            "absolute.csv",
        );
        link("..", "outside");
        link("data/../../secret.csv", "sideways.csv");
        for reference in [
            "out.csv",
            "absolute.csv",
            "outside/secret.csv",
            "sideways.csv",
        ] {
            let error = read(reference).unwrap_err();
            assert!(escapes(&error), "{reference}: {error}");
        }
        // A loop of links ends, and a name that is not there is missing, not an escape.
        link("loop-b", "loop-a");
        link("loop-a", "loop-b");
        assert!(escapes(&read("loop-a").unwrap_err()));
        assert!(
            read("absent.csv")
                .unwrap_err()
                .contains("report.asset.missing")
        );
        // A folder, and a pipe someone put at the name, are not files to read and do not block.
        assert!(escapes(&read("data").unwrap_err()));
        let pipe = std::ffi::CString::new(root.join("pipe.csv").to_str().unwrap()).unwrap();
        // SAFETY: `pipe` is a valid C string.
        assert_eq!(unsafe { libc::mkfifo(pipe.as_ptr(), 0o600) }, 0);
        assert!(escapes(&read("pipe.csv").unwrap_err()));
    }

    #[test]
    fn a_folder_replaced_while_a_command_runs_is_not_read_instead_of_the_one_it_began_with() {
        let outer = tempfile::tempdir().unwrap();
        // The folder as the system spells it, as a command that resolved its source has it.
        let base = fs::canonicalize(outer.path()).unwrap();
        let root = base.join("report");
        let elsewhere = base.join("elsewhere");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(root.join("report.rmd"), "# T\n").unwrap();
        fs::write(root.join("a.csv"), "inside").unwrap();
        fs::write(elsewhere.join("report.rmd"), "# Other\n").unwrap();
        fs::write(elsewhere.join("a.csv"), "outside").unwrap();
        let source = root.join("report.rmd");
        let read = |name: &str| {
            let mut text = String::new();
            open_beneath(&root, name)
                .unwrap()
                .read_to_string(&mut text)
                .unwrap();
            text
        };
        let read_source =
            |source: &Path| crate::input_limits::read_input(source, "report.rmd").unwrap();
        let outcome = pinned(&source, || {
            let before = read_source(&source);
            // Someone with a say over the folder above moves it and puts a link to another folder
            // where it was.
            fs::rename(&root, base.join("moved")).unwrap();
            std::os::unix::fs::symlink(&elsewhere, &root).unwrap();
            let after = read_source(&source);
            (before, after, read("a.csv"))
        });
        // The source and the file it names are those of the folder the command began with.
        assert_eq!(outcome.0, b"# T\n");
        assert_eq!(outcome.1, b"# T\n");
        assert_eq!(outcome.2, "inside");
        // Without the pin, the same name now leads somewhere else.
        assert_eq!(read("a.csv"), "outside");
        assert_eq!(read_source(&source), b"# Other\n");
    }

    #[test]
    fn a_file_swapped_for_a_link_after_it_was_checked_is_never_read() {
        let outer = tempfile::tempdir().unwrap();
        let root = outer.path().join("report");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.csv"), "inside").unwrap();
        fs::write(outer.path().join("secret.csv"), "outside").unwrap();
        // The path passes its check as a file inside the folder ...
        assert!(contained_path(&root, "a.csv").is_ok());
        // ... and is a link to another file when it is read.
        fs::remove_file(root.join("a.csv")).unwrap();
        std::os::unix::fs::symlink(outer.path().join("secret.csv"), root.join("a.csv")).unwrap();
        let mut inputs = crate::input_limits::InputSizes::new("report.rmd", 0).unwrap();
        let error = inputs.read(&root, "a.csv").unwrap_err();
        assert!(escapes(&error), "{error}");
        // The same for a folder of the reference.
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data/b.csv"), "inside").unwrap();
        assert!(contained_path(&root, "data/b.csv").is_ok());
        fs::rename(root.join("data"), root.join("moved")).unwrap();
        std::os::unix::fs::symlink(outer.path(), root.join("data")).unwrap();
        fs::write(outer.path().join("b.csv"), "outside").unwrap();
        let error = inputs.read(&root, "data/b.csv").unwrap_err();
        assert!(escapes(&error), "{error}");
    }

    #[test]
    fn a_folder_of_the_output_standing_as_a_link_makes_no_folder_behind_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("report");
        let outside = dir.path().join("outside");
        fs::create_dir_all(root.join("Rendered")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("Rendered/data")).unwrap();
        let refused = make_output_folders(&root, Path::new("data/deeper")).unwrap_err();
        assert!(refused.contains("report.output.escape"), "{refused}");
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        // Folders that are not there are made, one in the one before; ones that are, are kept.
        make_output_folders(&root, Path::new("a/b")).unwrap();
        make_output_folders(&root, Path::new("a/b")).unwrap();
        assert!(root.join("Rendered/a/b").is_dir());
        // `Rendered` itself as a link, or a file standing at a folder's name, is refused too.
        let other = dir.path().join("other");
        fs::create_dir_all(&other).unwrap();
        std::os::unix::fs::symlink(&outside, other.join("Rendered")).unwrap();
        assert!(make_output_folders(&other, Path::new("x")).is_err());
        assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
        fs::write(root.join("Rendered/file"), "x").unwrap();
        assert!(make_output_folders(&root, Path::new("file/inner")).is_err());
    }
}
