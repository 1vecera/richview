//! PDF output: a locally installed Chromium-family browser prints the core's `pdf` HTML.
//!
//! The browser runs headless with a temporary profile and speaks the DevTools protocol over a
//! pipe (`--remote-debugging-pipe`: NUL-terminated JSON on file descriptors 3 and 4). Every
//! request is refused except the report file itself, `data:` URLs and one exception: a GET for
//! the image of a credited photo figure the report shows (an image block or `title_image` on
//! `https://images.unsplash.com/`, the one hotlink origin), so it prints as the page shows it.
//! Anything else the report asks for is refused and reported as `export.network-blocked`. A photo
//! that cannot be loaded (offline, or no answer within a few seconds) prints as its box and
//! credit with an `export.photo-unavailable` warning.
//! Print media and reduced motion are emulated at the width of the printed text column; the
//! page prints once its fonts are loaded and the chart runtime has settled.

use richview_core::document::{Diagnostic, Page};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(15);
pub const DEFAULT_SETTLE_TIMEOUT: Duration = Duration::from_secs(30);
pub const PRINT_TIMEOUT: Duration = Duration::from_secs(60);
/// Paged-media margin boxes (running title, page numbers) need Chrome 131 or later.
const MINIMUM_MAJOR: u32 = 131;
const BROWSERS: [&str; 8] = [
    "chromium",
    "chromium-browser",
    "google-chrome",
    "google-chrome-stable",
    "chrome",
    "microsoft-edge",
    "msedge",
    "chrome-headless-shell",
];
#[cfg(target_os = "macos")]
const MAC_BROWSERS: [&str; 3] = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
];
/// Where Windows installs them, below the folders `WINDOWS_ROOTS` name. Edge comes with Windows.
const WINDOWS_BROWSERS: [&[&str]; 3] = [
    &["Google", "Chrome", "Application", "chrome.exe"],
    &["Chromium", "Application", "chrome.exe"],
    &["Microsoft", "Edge", "Application", "msedge.exe"],
];
const WINDOWS_ROOTS: [&str; 3] = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"];

pub struct PdfRequest<'a> {
    /// HTML rendered with `Output::Pdf`.
    pub html: &'a str,
    pub page: Page,
    pub browser: Option<PathBuf>,
    pub settle_timeout: Duration,
}

pub struct PdfExport {
    pub pdf: Vec<u8>,
    /// Executable, product, revision and whether Chrome's sandbox was on.
    pub browser: Value,
    /// How the page was judged ready: chart count, image decoding, the runtime's report and timings.
    pub settled: Value,
    pub diagnostics: Vec<Diagnostic>,
}

impl PdfExport {
    /// Page count from the page tree's `/Count`.
    pub fn pages(&self) -> u64 {
        let text = String::from_utf8_lossy(&self.pdf);
        text.match_indices("/Type /Pages")
            .chain(text.match_indices("/Type/Pages"))
            .filter_map(|(start, _)| {
                let dictionary = &text[text[..start].rfind("<<")?..];
                let count = &dictionary[dictionary.find("/Count")? + 6..];
                count
                    .trim_start()
                    .split(|c: char| !c.is_ascii_digit())
                    .next()?
                    .parse()
                    .ok()
            })
            .max()
            .unwrap_or(0)
    }
}

fn diagnostic(level: &str, code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(level, code, message, "output")
}

/// Print a report rendered with `Output::Pdf` and write it to `Rendered/<name>.pdf` (or the
/// requested `.pdf` name there). Errors are JSON diagnostics or a message, as for HTML export.
/// The browser runs on a blocking thread.
pub async fn export(
    source: &Path,
    requested: Option<&Path>,
    report: &richview_core::document::RenderedReport,
    options: &richview_core::document::RenderOptions,
    browser: Option<PathBuf>,
    settle_timeout: Duration,
) -> Result<(PathBuf, PdfExport), String> {
    let target = crate::resolver::pdf_output_path(source, requested)?;
    if !report.charts.is_empty() && options.runtime_wasm.is_none() {
        return Err(serde_json::to_string(&[Diagnostic::new("error","report.output.runtime","Interactive chart runtime is missing. Build/package RichView with richview-build before exporting this report.","output")]).unwrap_or_default());
    }
    let (html, page) = (report.html.clone(), options.page);
    let export = tokio::task::spawn_blocking(move || {
        print_pdf(&PdfRequest {
            html: &html,
            page,
            browser,
            settle_timeout,
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|diagnostics| serde_json::to_string(&diagnostics).unwrap_or_default())?;
    write_atomically(&target, &export.pdf)?;
    Ok((target, export))
}

/// The browser to print with: `explicit`, then `RICHVIEW_BROWSER`, then the usual names on
/// `PATH`, then the macOS application bundles and the Windows install folders.
pub fn find_browser(explicit: Option<&Path>) -> Result<PathBuf, Vec<Diagnostic>> {
    let executable = |path: &Path| {
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        path.metadata().is_ok_and(|m| {
            #[cfg(unix)]
            let runnable = m.permissions().mode() & 0o111 != 0;
            #[cfg(not(unix))]
            let runnable = true;
            m.is_file() && runnable
        })
    };
    let named = explicit
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("RICHVIEW_BROWSER").map(PathBuf::from));
    if let Some(path) = named {
        let resolved = if path.components().count() == 1 {
            search_path(&path).unwrap_or(path)
        } else {
            path
        };
        return if executable(&resolved) {
            Ok(resolved)
        } else {
            Err(vec![diagnostic(
                "error",
                "export.browser-missing",
                format!(
                    "Browser '{}' is not an executable file. Pass --browser with the path of Chromium, Google Chrome or Microsoft Edge, or set RICHVIEW_BROWSER.",
                    resolved.display()
                ),
            )])
        };
    }
    for name in BROWSERS {
        if let Some(path) = search_path(Path::new(name)).filter(|p| executable(p)) {
            return Ok(path);
        }
    }
    #[cfg(target_os = "macos")]
    for path in MAC_BROWSERS {
        if executable(Path::new(path)) {
            return Ok(path.into());
        }
    }
    // Only Windows sets these folders, so elsewhere this finds nothing.
    let roots = WINDOWS_ROOTS
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from);
    if let Some(path) = windows_browsers(roots).into_iter().find(|p| executable(p)) {
        return Ok(path);
    }
    Err(vec![diagnostic(
        "error",
        "export.browser-missing",
        format!(
            "PDF output prints through a Chromium-family browser, and none was found. Pass --browser PATH, set RICHVIEW_BROWSER, or install Chromium, Google Chrome or Microsoft Edge so that one of {} is on PATH.",
            BROWSERS.join(", ")
        ),
    )])
}

fn search_path(name: &Path) -> Option<PathBuf> {
    let names = executable_names(name, std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| candidate.is_file())
}

/// `name` as given and, where executables carry a suffix (`.exe` on Windows) and `name` has no
/// extension, `name` with that suffix.
fn executable_names(name: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut names = vec![name.to_path_buf()];
    if !suffix.is_empty() && name.extension().is_none() {
        let mut with_suffix = name.as_os_str().to_owned();
        with_suffix.push(suffix);
        names.push(with_suffix.into());
    }
    names
}

/// Each known browser's install path below each of `roots`, browsers in order of preference.
fn windows_browsers(roots: impl Iterator<Item = PathBuf>) -> Vec<PathBuf> {
    let roots: Vec<PathBuf> = roots.filter(|root| root.is_absolute()).collect();
    WINDOWS_BROWSERS
        .iter()
        .flat_map(|parts| {
            roots.iter().map(move |root| {
                parts
                    .iter()
                    .fold(root.clone(), |path, part| path.join(part))
            })
        })
        .collect()
}

/// The attributes of an `<img ...>` tag as `(name, value)` pairs, with entities in values
/// decoded. The renderer writes double-quoted values and escapes `"`, `<`, `>` and `&` in them.
fn img_attributes(tag: &str) -> Vec<(String, String)> {
    let unescape = |text: &str| {
        text.replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
    };
    let mut attributes = Vec::new();
    let mut rest = tag.trim_start_matches("<img").trim_start();
    while !rest.is_empty() && !rest.starts_with('>') {
        let name_end = rest
            .find(|c: char| c == '=' || c == '>' || c.is_whitespace())
            .unwrap_or(rest.len());
        let name = rest[..name_end].to_ascii_lowercase();
        rest = rest[name_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            let Some(quoted) = after.strip_prefix('"') else {
                break;
            };
            let Some(end) = quoted.find('"') else {
                break;
            };
            value = unescape(&quoted[..end]);
            rest = quoted[end + 1..].trim_start();
        }
        if !name.is_empty() {
            attributes.push((name, value));
        }
    }
    attributes
}

/// The photos the report shows as credited figures, as Chrome writes them in a request. Only the
/// renderer's own `<img ... data-rv-photo="credited">` of an image block or `title_image` counts
/// (sanitized Markdown HTML cannot carry that attribute), never any other `<img>` in the HTML.
fn referenced_photos(html: &str) -> BTreeSet<String> {
    html.match_indices("<img ")
        .filter_map(|(start, _)| {
            let tag = &html[start..start + html[start..].find('>')?];
            let attributes = img_attributes(tag);
            let get = |name: &str| {
                attributes
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.as_str())
            };
            if get("data-rv-photo") != Some("credited") {
                return None;
            }
            let src = get("src")?;
            richview_core::unsplash_image_url(src)
                .then(|| reqwest::Url::parse(src).ok().map(String::from))
                .flatten()
        })
        .collect()
}

/// Whether a paused request may go out: the report file, `data:` URLs, and a GET for an image
/// of a credited photo figure that the report shows. Nothing else leaves the browser.
fn request_allowed(
    page_url: &str,
    photos: &BTreeSet<String>,
    method: &str,
    resource_type: &str,
    url: &str,
) -> bool {
    url == page_url
        || url.starts_with("data:")
        || (method == "GET"
            && resource_type == "Image"
            && richview_core::unsplash_image_url(url)
            && photos.contains(url))
}

/// Print `request.html` to PDF bytes.
#[cfg(any(unix, windows))]
pub fn print_pdf(request: &PdfRequest) -> Result<PdfExport, Vec<Diagnostic>> {
    let browser = find_browser(request.browser.as_deref())?;
    let mut session = cdp::Session::launch(&browser)?;
    let result = session.print(request);
    let cleanup = session.close();
    let (pdf, settled, mut diagnostics) = match (result, cleanup) {
        (Ok(output), Ok(())) => output,
        (Err(mut errors), Err(cleanup)) => {
            errors.extend(cleanup);
            return Err(errors);
        }
        (Err(errors), _) | (_, Err(errors)) => return Err(errors),
    };
    diagnostics.splice(0..0, session.diagnostics());
    Ok(PdfExport {
        pdf,
        browser: session.browser_info(&browser),
        settled,
        diagnostics,
    })
}

#[cfg(not(any(unix, windows)))]
pub fn print_pdf(_request: &PdfRequest) -> Result<PdfExport, Vec<Diagnostic>> {
    Err(vec![diagnostic(
        "error",
        "export.platform",
        "PDF output is not available on this platform; it runs on Linux, macOS and Windows.",
    )])
}

/// Write `bytes` to `target` through a temporary file in the same directory, so a reader never
/// sees a partial PDF.
pub fn write_atomically(target: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let dir = target.parent().ok_or("Output has no parent directory")?;
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Output name must be UTF-8")?;
    let temporary = dir.join(format!(
        ".{name}.{}.{:016x}.tmp",
        std::process::id(),
        fastrand::u64(..)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("{}: {error}", target.display()))?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, target)
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("{}: {error}", target.display()));
    }
    Ok(())
}

/// Resolves to `{charts, images, report}` once the page is ready to print: fonts loaded, images
/// decoded and, when the report has charts, the runtime's `window.__richview.settled()` resolved.
/// Only reports with charts carry the runtime, so only they wait for its hook.
const SETTLE: &str = r#"(async () => {
  const frame = () => new Promise(done => requestAnimationFrame(() => done()));
  const charts = document.querySelectorAll('figure[data-rv-chart]').length;
  // Margin boxes first exist during printing. Load the embedded faces they may use before
  // waiting for the document, otherwise a face used only by the footer can print blank.
  await Promise.all([...document.fonts].map(face => face.load()));
  await document.fonts.ready;
  // A hotlinked photo that does not load (offline, removed, no answer) is reported, not fatal:
  // the page keeps its box, colour and credit. It gets its own short timeout so a network that
  // never answers cannot use up the settle timeout. Any other image that cannot be decoded stops
  // the export.
  const hotlink = 'https://images.unsplash.com/';
  const within = (promise, ms) => Promise.race([promise, new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), ms))]);
  const decoded = await Promise.all([...document.images].map(async (image, index) => {
    const url = image.src;
    const photo = url.startsWith(hotlink);
    try { await (photo ? within(image.decode(), 8000) : image.decode()); return null; }
    catch {
      if (!photo) return { name: image.alt || `image ${index + 1}` };
      image.removeAttribute('src');
      return { photo: url };
    }
  }));
  const images = {
    decoded: decoded.filter(value => value === null).length,
    failed: decoded.filter(value => value?.name !== undefined).map(value => value.name),
    unavailable: decoded.filter(value => value?.photo !== undefined).map(value => value.photo),
  };
  if (images.failed.length || !charts) return { charts, images, report: null };
  // The embedded Wasm may start after the fonts are loaded.
  while (typeof window.__richview?.settled !== 'function') await frame();
  const report = await window.__richview.settled();
  await frame(); await frame();
  return { charts, images, report: report ?? null };
})()"#;

/// Floats figures the way LaTeX does, so no page ends more than a quarter empty (FEA-RP-43, as
/// Daniel chose on 2026-10-27 and the lead clarified in 1528b5e4). The document is laid out in
/// page-sized columns, which fragment by the same break rules as printing. A figure that starts a
/// page and left more than a quarter of the page before it blank moves past the text after it, so
/// that text fills the page and the figure starts the next one. The page is settled front to back:
/// a position wins when the first page with blank after it comes later, or, with the same first
/// page, when it leaves less blank in all. A figure moves only past
/// paragraphs, lists, quotes, code, tables and media cards: never past a heading (sections are
/// float barriers), or a footnote reference when it holds one itself, and never up past text
/// before it; figures are never reordered. Its source note moves with it, and so do the figures that follow it
/// directly, in their order: the run floats as one. A block it passes keeps the source note under
/// it: the figure lands below that note. A page may still end more than a
/// quarter empty where the next block opens a section, or is a figure that ends its section, and
/// does not fit.
///
/// Where floating leaves a page more than a quarter empty, the block that opens the next page is
/// fitted onto it the way a typesetter saves a page: the space above that block gives up to half,
/// and then a picture on the short page or in that block gives up to a tenth of its height. A
/// chart is never scaled. Nothing changes where that does not fill the page. Resolves to
/// `{floated, fitted, pages}`.
///
/// Host reason (ARC-03): it measures the export browser's own print layout, which only the
/// browser has, and runs once over CDP before printing.
const FLOAT: &str = r#"(() => {
  const body = document.querySelector('.rv-report__body');
  const W = window.innerWidth, H = window.innerHeight, quarter = H * 0.23; // a quarter, less what the page measure and the raster read apart
  if (!body || !W || !H) return { floated: [], pages: 0 };
  const figure = el => el.matches('.rv-keep, figure, .rv-chart, .rv-image, .rv-video, .rv-embed');
  const text = el => el.matches('p:not(.rv-source), ul, ol, dl, blockquote, pre, table, .rv-table, math, .rv-media-card');
  const heading = el => /^H[1-6]$/.test(el.tagName);
  const notes = el => el.querySelector('.footnote-reference') !== null;
  const columns = document.createElement('style');
  columns.textContent = `body{height:${H}px;column-width:${W}px;column-gap:0;column-fill:auto;overflow:visible}`;
  document.head.append(columns);
  const probe = document.createElement('div');
  document.body.prepend(probe);
  // Page and top of an element's first fragment, and how far down each page is filled.
  const origin = () => { const r = probe.getBoundingClientRect(); return { left: r.left, top: r.top }; };
  const pageOf = (r, o) => Math.floor((r.left - o.left + 1) / W);
  const start = (el, o) => { const r = el.getClientRects()[0]; return r ? pageOf(r, o) : -1; };
  const end = (el, o) => { const rects = el.getClientRects(); return rects.length ? pageOf(rects[rects.length - 1], o) : -1; };
  const filled = (o) => {
    const fill = [];
    for (const el of [document.querySelector('.rv-report__header'), ...body.children, document.querySelector('.rv-report__footer')]) {
      if (!el) continue;
      for (const r of el.getClientRects()) {
        if (!r.height) continue;
        const page = pageOf(r, o);
        fill[page] = Math.max(fill[page] ?? 0, r.bottom - o.top);
      }
    }
    return fill;
  };
  const floated = [];
  // A run of figures, each with its source note, from block `k`: the blocks it holds and the index
  // of the first block after it. The first figure that does not fit takes the whole run along.
  const runOf = (blocks, k) => {
    const unit = [];
    let j = k;
    while (j < blocks.length && figure(blocks[j])) {
      unit.push(blocks[j++]);
      if (blocks[j]?.matches('p.rv-source')) unit.push(blocks[j++]);
    }
    return { unit, next: j };
  };
  // The blank this layout leaves beyond a quarter page at the foot of its pages, in pixels, except
  // where the next page opens a section whose opening unit (heading and the block after it) is taller
  // than the gap: that block does not fit, and the gap is allowed; and except where the next page
  // opens with a figure that ends its section and is taller than the gap.
  const blank = () => {
    const o = origin();
    const fill = filled(o);
    const blocks = [...body.children];
    let total = 0, front = Infinity;
    const short = [];
    for (let p = 0; p < fill.length - 1; p++) {
      const gap = H - (fill[p] ?? 0);
      if (gap <= quarter) continue;
      const k = blocks.findIndex(block => end(block, o) >= p + 1);
      const first = blocks[k];
      let excused = false;
      if (first && heading(first) && start(first, o) === p + 1) {
        const next = blocks[k + 1];
        const unit = first.getBoundingClientRect().height + (next ? Math.min(next.getBoundingClientRect().height, H) : 0);
        excused = unit > gap;
      } else if (first && figure(first) && start(first, o) === p + 1) {
        // Sections are float barriers: a run of figures that ends its section cannot float on, so
        // when its first figure does not fit the page ends with the gap.
        const note = blocks[k + 1]?.matches('p.rv-source') ? blocks[k + 1] : null;
        const after = blocks[runOf(blocks, k).next];
        const unit = first.getBoundingClientRect().height + (note ? note.getBoundingClientRect().height : 0);
        excused = (!after || heading(after)) && unit > gap;
      }
      if (!excused) { total += gap - quarter; front = Math.min(front, p); short.push(p); }
    }
    return { total, front, short };
  };
  for (let round = 0; round < 60; round++) {
    const o = origin();
    const blocks = [...body.children];
    const base = blank();
    if (base.total === 0) break;
    let best = null;
    for (let i = 0; i < blocks.length; i++) {
      const fig = blocks[i];
      if (!figure(fig)) continue;
      // The figure, or the headings kept with it, must open its page: the block before them
      // ended on an earlier page.
      let first = i;
      while (first > 0 && heading(blocks[first - 1])) first--;
      const page = start(blocks[first], o);
      if (page < 1 || start(fig, o) !== page || first === 0 || end(blocks[first - 1], o) >= page) continue;
      if (H - (filled(o)[page - 1] ?? 0) <= quarter) continue;
      // A figure travels with its source note, and with the figures that follow it directly: a
      // queue, in its order, past the text after the last of them.
      const { unit, next: after } = runOf(blocks, i);
      const below = [];
      // A block and the source note under it are one unit: the figure lands below the note, never
      // between the two, where the note would read as the figure's.
      for (let j = after; j < blocks.length && text(blocks[j]) && below.length < 4; j++) {
        if (unit.some(notes) && notes(blocks[j])) break;
        below.push(blocks[j + 1]?.matches('p.rv-source') ? blocks[++j] : blocks[j]);
      }
      const next = unit[unit.length - 1].nextElementSibling;
      const restore = () => (next ? next.before(...unit) : body.append(...unit));
      // Where it could go: where it is, or below the text after it. The position that leaves the
      // least blank in the whole document wins, when it leaves less than it does now.
      const positions = [['here', null], ...below.map(block => ['after', block])];
      for (const [how, block] of positions) {
        restore();
        if (how === 'after') block.after(...unit);
        const score = blank();
        const ref = best ? best.score : base;
        if (score.front > ref.front || (score.front === ref.front && score.total < ref.total - 1)) best = { score, fig, unit, how, block, next };
      }
      restore();
    }
    if (!best) break;
    const { fig, unit, how, block } = best;
    if (how === 'after') block.after(...unit);
    floated.push(fig.querySelector('[id]')?.id ?? fig.className);
  }
  // Fit: a page still more than a quarter empty takes the block that opens the next page when
  // little is missing. The space above that block gives first, to half; then the pictures on the
  // short page and in the opening unit, to nine tenths of their height. The least that fills the
  // page is kept, and a block that cannot be fitted is left as it was.
  const fitted = [];
  const tried = new Set();
  const picture = block => (block.matches('.rv-image, .rv-video') ? block : block.querySelector(':scope > .rv-image, :scope > .rv-video'))?.querySelector(':scope > img') ?? null;
  // An element with the first (or last) elements nested in it, whose margins may collapse with its own.
  const edge = (el, side) => { const out = []; for (let at = el; at && out.length < 3; at = side === 'top' ? at.firstElementChild : at.lastElementChild) out.push(at); return out; };
  for (let round = 0; round < 20; round++) {
    const o = origin();
    const blocks = [...body.children];
    const base = blank();
    const opening = page => blocks.findIndex(block => start(block, o) === page + 1);
    const page = base.short.find(p => opening(p) > 0 && !tried.has(blocks[opening(p)]));
    if (page === undefined) break;
    const k = opening(page);
    tried.add(blocks[k]);
    let last = k;
    while (heading(blocks[last]) && last + 1 < blocks.length) last++;
    const pictures = [...blocks.slice(0, k).filter(block => start(block, o) === page && end(block, o) === page), ...blocks.slice(k, last + 1)]
      .map(picture).filter(Boolean);
    const heights = pictures.map(img => img.getBoundingClientRect().height);
    const above = edge(blocks[k - 1], 'bottom'), below = edge(blocks[k], 'top');
    const margins = [...above.map(el => parseFloat(getComputedStyle(el).marginBottom) || 0), ...below.map(el => parseFloat(getComputedStyle(el).marginTop) || 0)];
    const space = Math.max(...margins);
    const apply = (share, scale) => {
      above.forEach((el, i) => { el.style.marginBottom = share === 1 ? '' : `${Math.min(margins[i], space * share)}px`; });
      below.forEach((el, i) => { el.style.marginTop = share === 1 ? '' : `${Math.min(margins[above.length + i], space * share)}px`; });
      pictures.forEach((img, i) => { img.style.maxHeight = scale === 1 ? '' : `${heights[i] * scale}px`; });
    };
    let kept = null;
    for (const [share, scale] of [[0.5, 1], [0.5, 0.97], [0.5, 0.94], [0.5, 0.9]]) {
      if (scale < 1 && !pictures.length) break;
      apply(share, scale);
      const score = blank();
      if (score.front > base.front || (score.front === base.front && score.total < base.total - 1)) { kept = { page: page + 1, space: share, picture: scale }; break; }
    }
    if (kept) fitted.push(kept); else apply(1, 1);
  }
  const pages = filled(origin()).length;
  probe.remove();
  columns.remove();
  return { floated, fitted, pages };
})()"#;

/// Labels of the work still pending when the page did not settle in time.
const PENDING: &str = r#"(() => {
  const hook = window.__richview;
  if (hook && typeof hook.pending === 'function') return hook.pending();
  const pending = document.querySelector('figure[data-rv-chart]') ? ['chart runtime'] : [];
  if (document.fonts.status !== 'loaded') pending.push('fonts');
  return pending;
})()"#;

/// Links into the report folder cannot travel inside a PDF: they print as their text. Returns
/// the authored targets.
const LOCAL_LINKS: &str = r#"(() => {
  const local = [];
  for (const link of document.querySelectorAll('a[href]')) {
    const href = link.getAttribute('href');
    if (href.startsWith('#') || new URL(link.href).protocol !== 'file:') continue;
    link.removeAttribute('href');
    local.push(href);
  }
  return local;
})()"#;

/// Elements whose own text is checked for faces the report does not embed.
const FONT_SAMPLE: &str = "h1,h2,h3,h4,p,li,td,th,dt,dd,figcaption span,a,em,strong,b,code,time,.rv-metric__value,.rv-metric__label,.rv-chart__legend-name,svg text,math,mi,mn,mo,mtext,ms";
const FONT_SAMPLE_LIMIT: usize = 800;

/// The browser's switches on every platform; the profile folder and `about:blank` follow.
const BROWSER_SWITCHES: [&str; 12] = [
    "--headless",
    "--remote-debugging-pipe",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-background-networking",
    "--disable-component-update",
    "--disable-default-apps",
    "--disable-sync",
    "--mute-audio",
    "--hide-scrollbars",
    "--force-color-profile=srgb",
];

#[cfg(any(unix, windows))]
mod cdp {
    use super::*;
    use std::{
        fs::File,
        io::{Read, Write},
        sync::{Arc, Mutex, mpsc},
    };
    #[cfg(unix)]
    use std::{
        os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
        os::unix::process::CommandExt,
        process::{Child, Command, Stdio},
    };

    pub struct Session {
        #[cfg(unix)]
        child: Child,
        /// Set before Chrome executes, so its startup forks belong to this launch alone.
        #[cfg(unix)]
        group: Option<libc::pid_t>,
        #[cfg(windows)]
        child: crate::win_process::Process,
        /// Holds the browser before it runs, so every process it starts belongs to this
        /// launch alone; closing it ends them all.
        #[cfg(windows)]
        group: Option<crate::win_process::Job>,
        dir: PathBuf,
        writer: File,
        messages: mpsc::Receiver<Value>,
        stderr: Arc<Mutex<Vec<u8>>>,
        next_id: u64,
        session: Option<String>,
        page_url: String,
        /// Photos the report may load while printing, see [`referenced_photos`].
        photos: BTreeSet<String>,
        blocked: Vec<String>,
        /// Page events seen while waiting for replies.
        events: Vec<String>,
        version: Value,
        sandbox: bool,
    }

    enum CallError {
        TimedOut,
        Failed(String),
    }

    impl From<String> for CallError {
        fn from(message: String) -> Self {
            Self::Failed(message)
        }
    }

    impl std::fmt::Display for CallError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::TimedOut => f.write_str("timed out"),
                Self::Failed(message) => f.write_str(message),
            }
        }
    }

    #[cfg(unix)]
    fn pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
        let mut fds = [0 as RawFd; 2];
        #[cfg(target_os = "linux")]
        // SAFETY: `fds` has room for the two descriptors pipe2 returns.
        let result = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
        #[cfg(not(target_os = "linux"))]
        // SAFETY: `fds` has room for the two descriptors pipe returns.
        let result = unsafe { libc::pipe(fds.as_mut_ptr()) };
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: pipe succeeded, so both descriptors are open and owned by nobody else.
        let ends = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        #[cfg(not(target_os = "linux"))]
        for fd in fds {
            // SAFETY: the descriptor belongs to `ends` and remains open through this call.
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(ends)
    }

    /// A new folder only this user may open, as the Unix folder's mode 0700: its DACL grants
    /// the user alone and inherits nothing from the temporary folder.
    #[cfg(windows)]
    fn private_dir() -> std::io::Result<PathBuf> {
        use crate::win_process::{self, Grant};
        use windows_sys::Win32::Security::{CONTAINER_INHERIT_ACE, OBJECT_INHERIT_ACE};
        use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
        let dir = std::env::temp_dir().join(format!(
            "richview-pdf-{}-{:08x}",
            std::process::id(),
            fastrand::u32(..)
        ));
        let user = win_process::current_user()?;
        win_process::create_private_dir(
            &dir,
            &[Grant {
                sid: &user,
                mask: FILE_ALL_ACCESS,
                inherit: OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
            }],
        )?;
        Ok(dir)
    }

    #[cfg(unix)]
    fn private_dir() -> std::io::Result<PathBuf> {
        use std::os::unix::fs::DirBuilderExt;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or_default();
        let dir =
            std::env::temp_dir().join(format!("richview-pdf-{}-{nanos:08x}", std::process::id()));
        std::fs::DirBuilder::new().mode(0o700).create(&dir)?;
        Ok(dir)
    }

    fn failed(message: impl Into<String>) -> Diagnostic {
        diagnostic("error", "export.browser-failed", message)
    }

    fn record_relaunch() -> Result<(), Vec<Diagnostic>> {
        let event = json!({
            "version": 1,
            "event": "browser-startup-relaunch",
            "surface": "pdf",
            "engine": "chromium",
            "attempt": 2,
            "maxAttempts": 2,
            "timeoutMs": LAUNCH_TIMEOUT.as_millis() as u64,
            "pid": std::process::id(),
        });
        let line = format!("{event}\n");
        if let Some(path) = std::env::var_os("RICHVIEW_BROWSER_EVENT_LOG") {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(vec![failed(
                    "Cannot record browser startup relaunch: RICHVIEW_BROWSER_EVENT_LOG must be an absolute path",
                )]);
            }
            let mut options = std::fs::OpenOptions::new();
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
            options
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut file| file.write_all(line.as_bytes()))
                .map_err(|e| {
                    vec![failed(format!(
                        "Cannot record browser startup relaunch: {e}"
                    ))]
                })?;
        }
        eprint!("RICHVIEW_BROWSER_RELAUNCH {line}");
        Ok(())
    }

    impl Session {
        pub fn launch(browser: &Path) -> Result<Session, Vec<Diagnostic>> {
            let mut attempt = 1;
            loop {
                let dir = private_dir().map_err(|e| {
                    vec![failed(format!(
                        "Cannot create a temporary browser profile: {e}"
                    ))]
                })?;
                let launch = Self::spawn(browser, &dir);
                if launch.is_err() {
                    let _ = std::fs::remove_dir_all(&dir);
                }
                let mut session = launch?;
                match session.call(
                    "Browser.getVersion",
                    json!({}),
                    false,
                    Instant::now() + LAUNCH_TIMEOUT,
                ) {
                    Ok(version) => {
                        session.version = version;
                        return Ok(session);
                    }
                    Err(error) => {
                        let timed_out = matches!(error, CallError::TimedOut);
                        let detail = session.stderr_tail();
                        let failure = failed(format!(
                            "{} did not start: {error}{detail}",
                            browser.display()
                        ));
                        // A timed-out launch cannot answer Browser.close. Kill and confirm
                        // its complete group is gone before any new profile or attempt.
                        let cleanup = if timed_out {
                            session.terminate().map_err(|e| vec![failed(e)])
                        } else {
                            session.close()
                        };
                        if let Err(mut cleanup) = cleanup {
                            cleanup.insert(0, failure);
                            return Err(cleanup);
                        }
                        if !timed_out || attempt == 2 {
                            return Err(vec![failure]);
                        }
                        record_relaunch()?;
                        attempt += 1;
                    }
                }
            }
        }

        /// Chrome reads commands from descriptor 3 and writes replies to descriptor 4. On
        /// Windows those are C runtime descriptors, which the launch passes in the table the
        /// runtime reads at startup; the browser starts inside a job that ends its whole tree.
        #[cfg(windows)]
        fn spawn(browser: &Path, dir: &Path) -> Result<Session, Vec<Diagnostic>> {
            use crate::win_process::{self, Job, Launch};
            use std::{ffi::OsStr, os::windows::io::AsRawHandle};
            let pipe_error = |e: std::io::Error| vec![failed(format!("Cannot open a pipe: {e}"))];
            let (browser_reads, commands) = win_process::pipe().map_err(pipe_error)?;
            let (replies, browser_writes) = win_process::pipe().map_err(pipe_error)?;
            let (errors, browser_errors) = win_process::pipe().map_err(pipe_error)?;
            let null = win_process::null_device().map_err(pipe_error)?;
            let job = Job::kill_on_close()
                .map_err(|e| vec![failed(format!("Cannot create a job for the browser: {e}"))])?;
            let profile = format!("--user-data-dir={}", dir.join("profile").display());
            let mut args: Vec<&OsStr> = BROWSER_SWITCHES.iter().map(OsStr::new).collect();
            args.extend([OsStr::new(&profile), OsStr::new("about:blank")]);
            let child = win_process::launch(&Launch {
                program: browser,
                args: &args,
                env: None,
                cwd: None,
                descriptors: &[
                    null.as_raw_handle(),
                    null.as_raw_handle(),
                    browser_errors.as_raw_handle(),
                    browser_reads.as_raw_handle(),
                    browser_writes.as_raw_handle(),
                ],
                job: &job,
                tokens: None,
                no_child_processes: false,
            })
            .map_err(|e| vec![failed(format!("Cannot start {}: {e}", browser.display()))])?;
            drop((browser_reads, browser_writes, browser_errors, null));
            let (stderr, messages) = Self::read_browser(File::from(errors), File::from(replies));
            Ok(Session {
                group: Some(job),
                child,
                dir: dir.to_path_buf(),
                writer: File::from(commands),
                messages,
                stderr,
                next_id: 0,
                session: None,
                page_url: String::new(),
                photos: BTreeSet::new(),
                blocked: Vec::new(),
                events: Vec::new(),
                version: Value::Null,
                // Chrome keeps its own sandbox on Windows: RichView never runs it as root there.
                sandbox: true,
            })
        }

        /// Threads that keep the tail of the browser's error output and turn its replies,
        /// NUL-terminated JSON, into messages.
        fn read_browser(
            mut errors: impl Read + Send + 'static,
            mut reader: impl Read + Send + 'static,
        ) -> (Arc<Mutex<Vec<u8>>>, mpsc::Receiver<Value>) {
            let stderr = Arc::new(Mutex::new(Vec::new()));
            let tail = stderr.clone();
            std::thread::spawn(move || {
                let mut buffer = [0u8; 4096];
                while let Ok(n) = errors.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    let mut tail = tail.lock().unwrap_or_else(|e| e.into_inner());
                    tail.extend_from_slice(&buffer[..n]);
                    let excess = tail.len().saturating_sub(4096);
                    tail.drain(..excess);
                }
            });
            let (sender, messages) = mpsc::channel();
            std::thread::spawn(move || {
                let mut pending = Vec::new();
                let mut buffer = vec![0u8; 1 << 16];
                while let Ok(n) = reader.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    pending.extend_from_slice(&buffer[..n]);
                    while let Some(end) = pending.iter().position(|b| *b == 0) {
                        let message: Vec<u8> = pending.drain(..=end).collect();
                        if let Ok(value) = serde_json::from_slice(&message[..end])
                            && sender.send(value).is_err()
                        {
                            return;
                        }
                    }
                }
            });
            (stderr, messages)
        }

        #[cfg(unix)]
        fn spawn(browser: &Path, dir: &Path) -> Result<Session, Vec<Diagnostic>> {
            let pipe_error = |e: std::io::Error| vec![failed(format!("Cannot open a pipe: {e}"))];
            let (browser_reads, commands) = pipe().map_err(pipe_error)?;
            let (replies, browser_writes) = pipe().map_err(pipe_error)?;
            // SAFETY: geteuid has no preconditions.
            let root = unsafe { libc::geteuid() } == 0;
            let mut command = Command::new(browser);
            command
                .process_group(0)
                .args(BROWSER_SWITCHES)
                .arg(format!("--user-data-dir={}", dir.join("profile").display()));
            // Chrome refuses to run its sandbox as root, for example in a container.
            if root {
                command.arg("--no-sandbox");
            }
            command
                .arg("about:blank")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            let (read_fd, write_fd) = (browser_reads.as_raw_fd(), browser_writes.as_raw_fd());
            // SAFETY: the closure runs between fork and exec and calls only async-signal-safe
            // functions. It moves the browser's pipe ends to descriptors 3 and 4 without the
            // close-on-exec flag; every other descriptor of ours keeps that flag.
            unsafe {
                command.pre_exec(move || {
                    let read = libc::fcntl(read_fd, libc::F_DUPFD, 10);
                    let write = libc::fcntl(write_fd, libc::F_DUPFD, 10);
                    if read < 0 || write < 0 || libc::dup2(read, 3) < 0 || libc::dup2(write, 4) < 0
                    {
                        return Err(std::io::Error::last_os_error());
                    }
                    libc::close(read);
                    libc::close(write);
                    Ok(())
                });
            }
            let mut child = command
                .spawn()
                .map_err(|e| vec![failed(format!("Cannot start {}: {e}", browser.display()))])?;
            drop((browser_reads, browser_writes));
            let errors: Box<dyn Read + Send> = match child.stderr.take() {
                Some(pipe) => Box::new(pipe),
                None => Box::new(std::io::empty()),
            };
            let (stderr, messages) = Self::read_browser(errors, File::from(replies));
            Ok(Session {
                group: Some(child.id() as libc::pid_t),
                child,
                dir: dir.to_path_buf(),
                writer: File::from(commands),
                messages,
                stderr,
                next_id: 0,
                session: None,
                page_url: String::new(),
                photos: BTreeSet::new(),
                blocked: Vec::new(),
                events: Vec::new(),
                version: Value::Null,
                sandbox: !root,
            })
        }

        fn stderr_tail(&self) -> String {
            let tail = self.stderr.lock().unwrap_or_else(|e| e.into_inner());
            let text = String::from_utf8_lossy(&tail);
            let text = text.trim();
            if text.is_empty() {
                String::new()
            } else {
                format!(". Browser output: {}", text.lines().last().unwrap_or(text))
            }
        }

        fn send(&mut self, method: &str, params: Value, scoped: bool) -> Result<u64, String> {
            self.next_id += 1;
            let mut message = json!({"id": self.next_id, "method": method, "params": params});
            if scoped && let Some(session) = &self.session {
                message["sessionId"] = json!(session);
            }
            let mut bytes = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
            bytes.push(0);
            self.writer
                .write_all(&bytes)
                .map_err(|e| format!("browser pipe closed ({e})"))?;
            Ok(self.next_id)
        }

        /// Receive one message, answering paused requests on the way: the report file,
        /// `data:` URLs and the report's own Unsplash photos continue (see [`request_allowed`]),
        /// everything else is refused and recorded.
        fn receive(&mut self, deadline: Instant) -> Result<Value, CallError> {
            let wait = deadline.saturating_duration_since(Instant::now());
            let message = self.messages.recv_timeout(wait).map_err(|e| match e {
                mpsc::RecvTimeoutError::Timeout => CallError::TimedOut,
                mpsc::RecvTimeoutError::Disconnected => {
                    CallError::Failed("the browser exited".into())
                }
            })?;
            if let Some(event) = message["method"]
                .as_str()
                .filter(|m| m.starts_with("Page."))
            {
                self.events.push(event.to_owned());
            }
            if message["method"] == "Fetch.requestPaused" {
                let id = message["params"]["requestId"].clone();
                let request = &message["params"]["request"];
                let url = request["url"].as_str().unwrap_or("");
                if request_allowed(
                    &self.page_url,
                    &self.photos,
                    request["method"].as_str().unwrap_or(""),
                    message["params"]["resourceType"].as_str().unwrap_or(""),
                    url,
                ) {
                    self.send("Fetch.continueRequest", json!({"requestId": id}), true)?;
                } else {
                    if !self.blocked.iter().any(|seen| seen == url) {
                        self.blocked.push(url.to_owned());
                    }
                    self.send(
                        "Fetch.failRequest",
                        json!({"requestId": id, "errorReason": "BlockedByClient"}),
                        true,
                    )?;
                }
            }
            Ok(message)
        }

        fn call(
            &mut self,
            method: &str,
            params: Value,
            scoped: bool,
            deadline: Instant,
        ) -> Result<Value, CallError> {
            let id = self.send(method, params, scoped)?;
            loop {
                let message = self.receive(deadline)?;
                if message["id"] == id {
                    if let Some(error) = message.get("error") {
                        return Err(CallError::Failed(format!(
                            "{method}: {}",
                            error["message"].as_str().unwrap_or("failed")
                        )));
                    }
                    return Ok(message["result"].clone());
                }
            }
        }

        fn page(
            &mut self,
            method: &str,
            params: Value,
            deadline: Instant,
        ) -> Result<Value, String> {
            self.call(method, params, true, deadline)
                .map_err(|e| e.to_string())
        }

        fn wait_for(&mut self, event: &str, deadline: Instant) -> Result<(), String> {
            while !self.events.iter().any(|seen| seen == event) {
                self.receive(deadline).map_err(|e| e.to_string())?;
            }
            Ok(())
        }

        fn evaluate(&mut self, expression: &str, deadline: Instant) -> Result<Value, String> {
            let result = self.page(
                "Runtime.evaluate",
                json!({"expression": expression, "awaitPromise": true, "returnByValue": true}),
                deadline,
            )?;
            if let Some(exception) = result.get("exceptionDetails") {
                return Err(exception["exception"]["description"]
                    .as_str()
                    .or_else(|| exception["text"].as_str())
                    .unwrap_or("script error")
                    .to_owned());
            }
            Ok(result["result"]["value"].clone())
        }

        pub fn print(
            &mut self,
            request: &PdfRequest,
        ) -> Result<(Vec<u8>, Value, Vec<Diagnostic>), Vec<Diagnostic>> {
            let mut diagnostics = Vec::new();
            let product = self.version["product"].as_str().unwrap_or("").to_owned();
            let major = product
                .split('/')
                .nth(1)
                .and_then(|v| v.split('.').next())
                .and_then(|v| v.parse::<u32>().ok());
            if major.is_none_or(|major| major < MINIMUM_MAJOR) {
                diagnostics.push(diagnostic("warning", "export.browser-version", format!("{product} is older than Chrome {MINIMUM_MAJOR}; the running title and page numbers may not print. Use a current Chromium, Google Chrome or Microsoft Edge.")));
            }
            let file = self.dir.join("report.html");
            std::fs::write(&file, request.html)
                .map_err(|e| vec![failed(format!("Cannot stage the report: {e}"))])?;
            self.page_url = reqwest::Url::from_file_path(&file)
                .map_err(|_| vec![failed("Cannot form the temporary report URL")])?
                .into();
            self.photos = referenced_photos(request.html);
            let setup = Instant::now() + LAUNCH_TIMEOUT;
            let prepared = (|| {
                let target = self
                    .call(
                        "Target.createTarget",
                        json!({"url": "about:blank"}),
                        false,
                        setup,
                    )
                    .map_err(|e| e.to_string())?;
                let attached = self
                    .call(
                        "Target.attachToTarget",
                        json!({"targetId": target["targetId"], "flatten": true}),
                        false,
                        setup,
                    )
                    .map_err(|e| e.to_string())?;
                self.session = attached["sessionId"].as_str().map(str::to_owned);
                self.page(
                    "Fetch.enable",
                    json!({"patterns": [{"urlPattern": "*"}]}),
                    setup,
                )?;
                self.page("Page.enable", json!({}), setup)?;
                self.page("Emulation.setEmulatedMedia", json!({"media": "print", "features": [{"name": "prefers-reduced-motion", "value": "reduce"}, {"name": "prefers-color-scheme", "value": "light"}]}), setup)?;
                // Charts lay out at the width they print at; the height is the page area, so
                // viewport units mean the same on screen and on paper.
                let (width, height) = request.page.text_area();
                self.page("Emulation.setDeviceMetricsOverride", json!({"width": width.round(), "height": height.round(), "deviceScaleFactor": 1, "mobile": false}), setup)?;
                self.events.clear();
                let navigation =
                    self.page("Page.navigate", json!({"url": self.page_url}), setup)?;
                if let Some(error) = navigation["errorText"].as_str() {
                    return Err(format!("the report did not load: {error}"));
                }
                Ok(())
            })();
            prepared.map_err(|e| vec![failed(format!("Cannot open the report: {e}"))])?;
            let started = Instant::now();
            let settle_deadline = started.checked_add(request.settle_timeout).ok_or_else(|| {
                vec![diagnostic(
                    "error",
                    "export.settle-timeout",
                    "The requested settle timeout is too large",
                )]
            })?;
            self.wait_for("Page.loadEventFired", settle_deadline)
                .map_err(|e| {
                    vec![diagnostic(
                        "error",
                        "export.settle-timeout",
                        format!(
                            "The report did not finish loading within {} s ({e}).",
                            request.settle_timeout.as_secs()
                        ),
                    )]
                })?;
            let settled = self.evaluate(SETTLE, settle_deadline).map_err(|error| {
                let pending = self
                    .evaluate(PENDING, Instant::now() + Duration::from_secs(5))
                    .ok()
                    .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
                    .unwrap_or_default();
                vec![diagnostic("error", "export.settle-timeout", format!("The report did not settle within {} s ({error}); still pending: {}. Raise --settle-timeout or check the chart runtime.", request.settle_timeout.as_secs(), if pending.is_empty() { "unknown".into() } else { pending.join(", ") }))]
            })?;
            let mut settled = settled;
            settled["ms"] = json!(started.elapsed().as_millis() as u64);
            if let Some(failed) = settled["images"]["failed"]
                .as_array()
                .filter(|failed| !failed.is_empty())
            {
                return Err(vec![diagnostic(
                    "error",
                    "export.image-failed",
                    format!(
                        "Images could not be decoded: {}. Replace the invalid image assets before exporting.",
                        failed
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )]);
            }
            for photo in settled["images"]["unavailable"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                diagnostics.push(diagnostic("warning", "export.photo-unavailable", format!("The photo '{photo}' could not be loaded while printing, so the PDF shows its box and credit without it.")));
            }
            let after = Instant::now() + PRINT_TIMEOUT;
            settled["float"] = self.evaluate(FLOAT, after).map_err(|e| vec![failed(e)])?;
            let local = self
                .evaluate(LOCAL_LINKS, after)
                .map_err(|e| vec![failed(e)])?;
            for target in local
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                diagnostics.push(diagnostic("info", "export.local-link", format!("Link to '{target}' prints as text: a PDF cannot carry the files of the report folder.")));
            }
            diagnostics.extend(self.font_fallbacks(after));
            let pdf = self.print_to_pdf(&mut diagnostics)?;
            Ok((pdf, settled, diagnostics))
        }

        /// System faces that drew text in place of the report's embedded faces.
        fn font_fallbacks(&mut self, deadline: Instant) -> Vec<Diagnostic> {
            let mut families = std::collections::BTreeMap::<String, u64>::new();
            let sampled = (|| -> Result<(), String> {
                self.page("DOM.enable", json!({}), deadline)?;
                self.page("CSS.enable", json!({}), deadline)?;
                let root = self.page("DOM.getDocument", json!({"depth": 0}), deadline)?;
                let nodes = self.page(
                    "DOM.querySelectorAll",
                    json!({"nodeId": root["root"]["nodeId"], "selector": FONT_SAMPLE}),
                    deadline,
                )?;
                let nodes = nodes["nodeIds"].as_array().cloned().unwrap_or_default();
                let step = nodes.len().div_ceil(FONT_SAMPLE_LIMIT).max(1);
                for node in nodes.iter().step_by(step) {
                    let fonts = self.page(
                        "CSS.getPlatformFontsForNode",
                        json!({"nodeId": node}),
                        deadline,
                    )?;
                    for font in fonts["fonts"].as_array().into_iter().flatten() {
                        if font["isCustomFont"] == false {
                            *families
                                .entry(font["familyName"].as_str().unwrap_or("?").to_owned())
                                .or_default() += font["glyphCount"].as_u64().unwrap_or(0);
                        }
                    }
                }
                Ok(())
            })();
            let mut diagnostics: Vec<_> = families.into_iter().map(|(family, glyphs)| diagnostic("warning", "export.font-fallback", format!("{glyphs} sampled glyphs were drawn in the system face '{family}' instead of the report's embedded faces, so this PDF depends on the fonts of the machine that printed it."))).collect();
            if let Err(error) = sampled {
                diagnostics.push(diagnostic(
                    "info",
                    "export.font-check",
                    format!("The font check did not complete: {error}"),
                ));
            }
            diagnostics
        }

        fn print_to_pdf(
            &mut self,
            diagnostics: &mut Vec<Diagnostic>,
        ) -> Result<Vec<u8>, Vec<Diagnostic>> {
            let deadline = Instant::now() + PRINT_TIMEOUT;
            let mut params = json!({"printBackground": true, "preferCSSPageSize": true, "displayHeaderFooter": false, "marginTop": 0, "marginBottom": 0, "marginLeft": 0, "marginRight": 0, "transferMode": "ReturnAsStream", "generateTaggedPDF": true, "generateDocumentOutline": true});
            let printed = match self.page("Page.printToPDF", params.clone(), deadline) {
                Ok(printed) => printed,
                Err(error) if !error.contains("timed out") && !error.contains("exited") => {
                    let map = params.as_object_mut().expect("object");
                    map.remove("generateTaggedPDF");
                    map.remove("generateDocumentOutline");
                    diagnostics.push(diagnostic("warning", "export.pdf-untagged", format!("This browser printed without PDF tags and outline ({error}); screen readers and bookmarks get less structure.")));
                    self.page("Page.printToPDF", params, deadline)
                        .map_err(|e| vec![failed(format!("Printing failed: {e}"))])?
                }
                Err(error) => return Err(vec![failed(format!("Printing failed: {error}"))]),
            };
            let handle = printed["stream"].clone();
            let mut pdf = Vec::new();
            loop {
                let chunk = self
                    .page(
                        "IO.read",
                        json!({"handle": handle, "size": 1 << 20}),
                        deadline,
                    )
                    .map_err(|e| vec![failed(format!("Reading the PDF failed: {e}"))])?;
                let data = chunk["data"].as_str().unwrap_or("");
                if chunk["base64Encoded"] == true {
                    use base64::Engine;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|e| vec![failed(format!("Reading the PDF failed: {e}"))])?;
                    pdf.extend_from_slice(&bytes);
                } else {
                    pdf.extend_from_slice(data.as_bytes());
                }
                if chunk["eof"] == true {
                    break;
                }
            }
            let _ = self.page("IO.close", json!({"handle": handle}), deadline);
            if !pdf.starts_with(b"%PDF-") {
                return Err(vec![failed(
                    "The browser returned something other than a PDF",
                )]);
            }
            Ok(pdf)
        }

        /// Refused requests, and a note when Chrome ran without its sandbox.
        pub fn diagnostics(&self) -> Vec<Diagnostic> {
            let mut notes: Vec<_> = self.blocked.iter().map(|url| {
                diagnostic("warning", "export.network-blocked", format!("The report requested '{url}' while printing; PDF output only loads the credited Unsplash photos the report shows, so it printed without it."))
            }).collect();
            if !self.sandbox {
                notes.push(diagnostic("info", "export.browser-sandbox", "Chrome printed with its sandbox off because RichView runs as root, where Chrome does not support it. Run as an ordinary user to keep the sandbox."));
            }
            notes
        }

        pub fn browser_info(&self, path: &Path) -> Value {
            json!({"path": path, "product": self.version["product"], "revision": self.version["revision"], "protocol": self.version["protocolVersion"], "sandbox": self.sandbox})
        }

        /// End every process of the launch's job and wait until none is left.
        #[cfg(windows)]
        fn terminate(&mut self) -> Result<(), String> {
            let Some(job) = &self.group else {
                return Ok(());
            };
            let deadline = Instant::now() + Duration::from_secs(5);
            job.terminate()
                .map_err(|e| format!("Cannot stop the browser's processes: {e}"))?;
            while job
                .active_processes()
                .map_err(|e| format!("Cannot check the browser's processes: {e}"))?
                > 0
            {
                if Instant::now() >= deadline {
                    return Err("The browser's processes did not stop".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = self.child.try_wait();
            self.group = None;
            // Windows releases an ended process's files a moment later.
            while self.dir.exists()
                && std::fs::remove_dir_all(&self.dir).is_err()
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(())
        }

        #[cfg(unix)]
        fn terminate(&mut self) -> Result<(), String> {
            let Some(group) = self.group else {
                return Ok(());
            };
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut signalled = false;
            loop {
                if !signalled {
                    // SAFETY: process_group(0) created this positive group ID from our child.
                    // Signal descendants even when the direct child has already exited.
                    if unsafe { libc::kill(-group, libc::SIGKILL) } == 0 {
                        signalled = true;
                    } else {
                        let error = std::io::Error::last_os_error();
                        match error.raw_os_error() {
                            Some(libc::ESRCH) => signalled = true,
                            // macOS may report EPERM while an exiting group becomes a
                            // zombie. It remains present; reap/poll within the same bound.
                            Some(libc::EPERM) => {}
                            _ => {
                                return Err(format!(
                                    "Cannot stop the browser process group: {error}"
                                ));
                            }
                        }
                    }
                }
                let reaped = self
                    .child
                    .try_wait()
                    .map_err(|e| format!("Cannot reap the browser: {e}"))?
                    .is_some();
                // SAFETY: signal zero only checks this launch's group; it changes no process.
                if unsafe { libc::kill(-group, 0) } < 0 {
                    let error = std::io::Error::last_os_error();
                    if reaped && error.raw_os_error() == Some(libc::ESRCH) {
                        self.group = None;
                        let _ = std::fs::remove_dir_all(&self.dir);
                        return Ok(());
                    }
                    // A descendant may also remain as an unreaped zombie briefly. EPERM
                    // still means present: wait within the same bound for ESRCH, never
                    // treat permission failure itself as proof that the group drained.
                    if !matches!(error.raw_os_error(), Some(libc::EPERM | libc::ESRCH)) {
                        return Err(format!("Cannot check the browser process group: {error}"));
                    }
                }
                if Instant::now() >= deadline {
                    return Err("The browser process group did not stop".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        pub fn close(&mut self) -> Result<(), Vec<Diagnostic>> {
            if self.group.is_none() {
                return Ok(());
            }
            let _ = self.send("Browser.close", json!({}), false);
            let deadline = Instant::now() + Duration::from_secs(5);
            // Keep the leader unreaped until the group is signalled, so its PID cannot be
            // reused during graceful shutdown. EOF also covers inherited DevTools pipes.
            while Instant::now() < deadline {
                if self
                    .messages
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .is_err()
                {
                    break;
                }
            }
            self.terminate().map_err(|e| vec![failed(e)])
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            let _ = self.terminate();
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[cfg(all(test, windows))]
    mod windows_tests {
        use super::*;

        #[test]
        fn the_installed_browser_answers_on_the_pipe_and_its_whole_tree_ends_on_close() {
            let browser = find_browser(None).expect("Windows ships Microsoft Edge");
            let mut session = Session::launch(&browser).unwrap();
            assert!(
                session.version["product"]
                    .as_str()
                    .is_some_and(|product| product.contains('/')),
                "{}",
                session.version
            );
            let job = session.group.as_ref().unwrap();
            assert!(
                job.active_processes().unwrap() >= 2,
                "a Chromium browser runs as several processes"
            );
            let dir = session.dir.clone();
            assert!(dir.exists());
            session.close().unwrap();
            assert!(session.group.is_none());
            assert!(!dir.exists());
        }
    }

    #[cfg(all(test, unix))]
    mod tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        #[cfg(target_os = "macos")]
        fn root_os_state(pid: u32) -> String {
            // SAFETY: both are plain kernel-output records, initialized before proc_pidinfo.
            let mut bsd: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
            let mut task: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
            // SAFETY: inspect only the fixture's own root; each buffer has the supplied size.
            let bsd_bytes = unsafe {
                libc::proc_pidinfo(
                    pid as i32,
                    libc::PROC_PIDTBSDINFO,
                    0,
                    (&mut bsd as *mut libc::proc_bsdinfo).cast(),
                    std::mem::size_of_val(&bsd) as i32,
                )
            };
            let bsd_error = if bsd_bytes <= 0 {
                Some(std::io::Error::last_os_error())
            } else {
                None
            };
            // SAFETY: same owned PID, correctly sized task-info output buffer.
            let task_bytes = unsafe {
                libc::proc_pidinfo(
                    pid as i32,
                    libc::PROC_PIDTASKINFO,
                    0,
                    (&mut task as *mut libc::proc_taskinfo).cast(),
                    std::mem::size_of_val(&task) as i32,
                )
            };
            let task_error = if task_bytes <= 0 {
                Some(std::io::Error::last_os_error())
            } else {
                None
            };
            let name: Vec<u8> = bsd
                .pbi_comm
                .iter()
                .take_while(|byte| **byte != 0)
                .map(|byte| *byte as u8)
                .collect();
            format!(
                "pid={pid}; bsd_bytes={bsd_bytes}; bsd_error={bsd_error:?}; observed_pid={}; ppid={}; pgid={}; status={}; comm={}; started={}.{:06}; task_bytes={task_bytes}; task_error={task_error:?}; task_user_raw={}; task_system_raw={}; threads={}; running={}; faults={}; pageins={}; switches={}",
                bsd.pbi_pid,
                bsd.pbi_ppid,
                bsd.pbi_pgid,
                bsd.pbi_status,
                String::from_utf8_lossy(&name),
                bsd.pbi_start_tvsec,
                bsd.pbi_start_tvusec,
                task.pti_total_user,
                task.pti_total_system,
                task.pti_threadnum,
                task.pti_numrunning,
                task.pti_faults,
                task.pti_pageins,
                task.pti_csw
            )
        }

        #[cfg(not(target_os = "macos"))]
        fn root_os_state(pid: u32) -> String {
            format!(
                "pid={pid}; stat={:?}; wait_channel={:?}",
                std::fs::read_to_string(format!("/proc/{pid}/stat")),
                std::fs::read_to_string(format!("/proc/{pid}/wchan"))
            )
        }

        struct Processes {
            child: Child,
            descendants: Vec<libc::pid_t>,
        }

        impl Drop for Processes {
            fn drop(&mut self) {
                for &pid in &self.descendants {
                    // SAFETY: each PID was written by this test's own shell launch.
                    unsafe { libc::kill(pid, libc::SIGKILL) };
                }
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }

        #[test]
        fn an_exited_browser_group_is_not_a_cleanup_error() {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
            std::fs::create_dir_all(&scratch).unwrap();
            let fixture = tempfile::tempdir_in(scratch).unwrap();
            let dir = fixture.path().join("profile");
            std::fs::create_dir(&dir).unwrap();
            let browser = fixture.path().join("browser");
            std::fs::write(&browser, "#!/bin/sh\nexit 0\n").unwrap();
            std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
            let mut session = Session::spawn(&browser, &dir).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                let mut state: libc::siginfo_t = unsafe { std::mem::zeroed() };
                // SAFETY: inspect only our own child, without reaping its zombie exit.
                let observed = unsafe {
                    libc::waitid(
                        libc::P_PID,
                        session.child.id(),
                        &mut state,
                        libc::WEXITED | libc::WNOWAIT | libc::WNOHANG,
                    )
                };
                assert_eq!(
                    observed,
                    0,
                    "waitid failed: {}; root={:?}; reader={:?}; stderr={}",
                    std::io::Error::last_os_error(),
                    session.child.try_wait(),
                    session.messages.try_recv(),
                    session.stderr_tail()
                );
                if state.si_signo == libc::SIGCHLD {
                    assert_eq!(
                        state.si_code,
                        libc::CLD_EXITED,
                        "root did not exit normally; reader={:?}; stderr={}",
                        session.messages.try_recv(),
                        session.stderr_tail()
                    );
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "root exit timed out; os={}; signal={}; code={}; root={:?}; reader={:?}; stderr={}",
                    root_os_state(session.child.id()),
                    state.si_signo,
                    state.si_code,
                    session.child.try_wait(),
                    session.messages.try_recv(),
                    session.stderr_tail()
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            let before = session.messages.try_recv();
            assert!(
                before.is_err(),
                "exit fixture sent an unexpected message: {before:?}"
            );
            eprintln!(
                "PDF exit fixture: root={} exited/unreaped; reader_before_close={before:?}",
                session.child.id()
            );
            // A root can exit before its pipe reader observes EOF. Exercise cleanup first;
            // retain EOF without giving it a fresh wait budget.
            let result = session.close();
            let received = session
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()));
            let root = session.child.wait();
            assert!(
                root.as_ref().is_ok_and(|status| status.success()),
                "root={root:?}; cleanup={result:?}; receive={received:?}"
            );
            assert!(
                result.is_ok(),
                "cleanup={result:?}; root={root:?}; receive={received:?}; stderr={}",
                session.stderr_tail()
            );
            assert!(
                matches!(received, Err(mpsc::RecvTimeoutError::Disconnected)),
                "receive={received:?}; root={root:?}; cleanup={result:?}; reader={:?}; stderr={}",
                session.messages.try_recv(),
                session.stderr_tail()
            );
            assert!(!dir.exists());
        }

        #[test]
        fn cleanup_waits_for_an_exited_group_member_to_be_reaped() {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
            std::fs::create_dir_all(&scratch).unwrap();
            let fixture = tempfile::tempdir_in(scratch).unwrap();
            let dir = fixture.path().join("profile");
            std::fs::create_dir(&dir).unwrap();
            let browser = fixture.path().join("browser");
            // One real CDP command byte closes this process; no fixture implements cleanup.
            std::fs::write(
                &browser,
                "#!/bin/sh\ndd bs=1 count=1 <&3 >/dev/null 2>/dev/null\n",
            )
            .unwrap();
            std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
            let session = Session::spawn(&browser, &dir).unwrap();
            let leader = session.child.id() as libc::pid_t;
            // Hold a real exited member in the launch's group until its leader is reaped.
            // macOS reports EPERM for this zombie-only group; it is not yet ESRCH.
            let mut member = Command::new("true")
                .process_group(leader)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut bytes = Vec::new();
            member
                .stdout
                .take()
                .unwrap()
                .read_to_end(&mut bytes)
                .unwrap();
            let close = std::thread::spawn(move || {
                let mut session = session;
                session.close()
            });
            let deadline = Instant::now() + Duration::from_secs(2);
            // SAFETY: signal zero only observes our launch leader; it sends no signal.
            while unsafe { libc::kill(leader, 0) } == 0 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            // Reap our separate fixture member before joining, even when close is broken.
            assert!(member.wait().unwrap().success());
            let result = close.join().unwrap();
            assert!(result.is_ok(), "{result:?}");
            assert!(!dir.exists());
        }

        #[test]
        fn close_and_drop_remove_descendants_after_the_browser_leader_exits() {
            let scratch = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
            std::fs::create_dir_all(&scratch).unwrap();
            let fixture = tempfile::tempdir_in(scratch).unwrap();
            // This independent group must survive cleanup of both browser launches.
            let mut owned = Processes {
                child: Command::new("sleep")
                    .arg("60")
                    .process_group(0)
                    .spawn()
                    .unwrap(),
                descendants: Vec::new(),
            };
            for explicit_close in [true, false] {
                let dir = fixture
                    .path()
                    .join(if explicit_close { "close" } else { "drop" });
                std::fs::create_dir(&dir).unwrap();
                let pid_file = fixture.path().join("descendant.pid");
                let browser = fixture.path().join("browser");
                let quoted = pid_file.display().to_string().replace('\'', "'\\''");
                std::fs::write(
                    &browser,
                    format!("#!/bin/sh\nsleep 60 &\nprintf '%s' \"$!\" > '{quoted}'\nexit 0\n"),
                )
                .unwrap();
                std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o700)).unwrap();
                let mut session = Session::spawn(&browser, &dir).unwrap();
                assert!(session.child.wait().unwrap().success());
                let descendant: libc::pid_t =
                    std::fs::read_to_string(&pid_file).unwrap().parse().unwrap();
                owned.descendants.push(descendant);
                // SAFETY: signal zero only checks the live process recorded by our fixture.
                assert_eq!(unsafe { libc::kill(descendant, 0) }, 0);
                if explicit_close {
                    let _ = session.close();
                } else {
                    drop(session);
                }
                let deadline = Instant::now() + Duration::from_secs(2);
                // SAFETY: this is the fixture's own descendant, never a discovered host PID.
                while unsafe { libc::kill(descendant, 0) } == 0 && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(10));
                }
                // SAFETY: signal zero does not change the process.
                assert_ne!(
                    unsafe { libc::kill(descendant, 0) },
                    0,
                    "owned descendant survived"
                );
                owned.descendants.pop();
                assert!(
                    owned.child.try_wait().unwrap().is_none(),
                    "unrelated group was killed"
                );
                assert!(!dir.exists());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_browser_name_is_also_tried_with_the_platform_suffix() {
        let names = |name: &str, suffix: &str| executable_names(Path::new(name), suffix);
        assert_eq!(names("msedge", ""), [PathBuf::from("msedge")]);
        assert_eq!(
            names("msedge", ".exe"),
            [PathBuf::from("msedge"), PathBuf::from("msedge.exe")]
        );
        assert_eq!(names("chrome.exe", ".exe"), [PathBuf::from("chrome.exe")]);
    }

    #[test]
    fn windows_install_folders_are_searched_by_browser_then_folder() {
        // Absolute on every platform; nothing is created.
        let base = std::env::temp_dir();
        let (system, user) = (base.join("Program Files"), base.join("Local"));
        let found = windows_browsers([system.clone(), user.clone(), "relative".into()].into_iter());
        assert_eq!(
            found,
            [
                system.join("Google/Chrome/Application/chrome.exe"),
                user.join("Google/Chrome/Application/chrome.exe"),
                system.join("Chromium/Application/chrome.exe"),
                user.join("Chromium/Application/chrome.exe"),
                system.join("Microsoft/Edge/Application/msedge.exe"),
                user.join("Microsoft/Edge/Application/msedge.exe"),
            ]
        );
        assert!(windows_browsers(std::iter::empty()).is_empty());
    }

    const PAGE: &str = "file:///tmp/richview-pdf-1/report.html";
    const PHOTO: &str = "https://images.unsplash.com/photo-1?ixid=M3w&w=1600&q=80";

    fn figure(src: &str, extra: &str) -> String {
        format!(
            "<figure class=\"rv-image\"><img src=\"{}\"{extra} alt=\"Sea\" width=\"4\"></figure>",
            src.replace('&', "&amp;")
        )
    }

    fn photos() -> BTreeSet<String> {
        referenced_photos(&figure(PHOTO, " data-rv-photo=\"credited\""))
    }

    #[test]
    fn photos_are_the_credited_figures_the_renderer_marked() {
        assert_eq!(photos(), BTreeSet::from([PHOTO.to_owned()]));
        // An unmarked <img>, even inside a figure that looks like an image block.
        assert!(referenced_photos(&figure(PHOTO, "")).is_empty());
        assert!(
            referenced_photos(&format!(
                "<figure class=\"rv-image\"><img src=\"{PHOTO}\" alt=\"data-rv-photo=&quot;credited&quot;\"><p class=\"rv-credit\">x</p></figure>"
            ))
            .is_empty()
        );
        let html = [
            figure("https://example.org/a.jpg", " data-rv-photo=\"credited\""),
            figure("http://images.unsplash.com/a.jpg", " data-rv-photo=\"credited\""),
            figure("https://images.unsplash.com.evil.test/a.jpg", " data-rv-photo=\"credited\""),
            figure("https://Images.unsplash.com:8443/a.jpg", " data-rv-photo=\"credited\""),
            "<img data-src=\"https://images.unsplash.com/lazy.jpg\" data-rv-photo=\"credited\">".into(),
            "<a href=\"https://images.unsplash.com/link.jpg\">x</a>".into(),
            "<p>&lt;img src=\"https://images.unsplash.com/text.jpg\" data-rv-photo=\"credited\"&gt;</p>".into(),
            "<img src=\"https://images.unsplash.com/ok.jpg\" alt=\"A\" data-rv-photo=\"credited\">".into(),
        ]
        .concat();
        assert_eq!(
            referenced_photos(&html),
            BTreeSet::from(["https://images.unsplash.com/ok.jpg".to_owned()])
        );
    }

    #[test]
    fn the_rendered_report_marks_exactly_its_credited_photos() {
        let credit = "credit: {author: Ada, author_url: 'https://unsplash.com/@ada'}";
        let source = format!(
            "---\ntitle: T\ntitle_image: {{src: https://images.unsplash.com/header, {credit}}}\n---\n\n```image\nsrc: {PHOTO}\nalt: A\n{credit}\n```\n"
        );
        let report = richview_core::document::render(
            &source,
            &richview_core::document::RenderOptions {
                output: richview_core::document::Output::Pdf,
                report_folder: Some(richview_core::document::ReportFolder {
                    root: "r".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            referenced_photos(&report.html),
            BTreeSet::from([
                PHOTO.to_owned(),
                "https://images.unsplash.com/header".to_owned()
            ])
        );
    }

    #[test]
    fn only_the_report_file_data_and_its_own_photos_may_load() {
        let allowed = |method, kind, url| request_allowed(PAGE, &photos(), method, kind, url);
        assert!(allowed("GET", "Document", PAGE));
        assert!(allowed("GET", "Image", "data:image/png;base64,AAAA"));
        assert!(allowed("GET", "Image", PHOTO));

        // A photo the report does not name, another path or query on the same host.
        assert!(!allowed(
            "GET",
            "Image",
            "https://images.unsplash.com/photo-2"
        ));
        assert!(!allowed(
            "GET",
            "Image",
            "https://images.unsplash.com/photo-1"
        ));
        // Only GET, and only as an image.
        for method in ["POST", "PUT", "HEAD", "OPTIONS", ""] {
            assert!(!allowed(method, "Image", PHOTO), "{method}");
        }
        for kind in [
            "Script",
            "XHR",
            "Fetch",
            "Font",
            "Stylesheet",
            "Media",
            "Document",
            "",
        ] {
            assert!(!allowed("GET", kind, PHOTO), "{kind}");
        }
        // Everything else, including lookalike hosts and other schemes.
        for url in [
            "https://example.org/a.jpg",
            "https://plus.unsplash.com/photo-1",
            "https://images.unsplash.com.evil.test/photo-1?ixid=M3w&w=1600&q=80",
            "http://images.unsplash.com/photo-1?ixid=M3w&w=1600&q=80",
            "https://images.unsplash.com:8443/photo-1?ixid=M3w&w=1600&q=80",
            "file:///etc/passwd",
            "ws://images.unsplash.com/photo-1",
            "https://api.unsplash.com/photos/1/download",
        ] {
            assert!(!allowed("GET", "Image", url), "{url}");
        }
        // A report that names no photo allows none.
        assert!(!request_allowed(
            PAGE,
            &BTreeSet::new(),
            "GET",
            "Image",
            PHOTO
        ));
    }
}
