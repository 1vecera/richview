use super::*;
use crate::source_buffer::SourceBuffer;
use serde_json::json;
use std::rc::Rc;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    HtmlAnchorElement, HtmlIFrameElement, HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement,
    KeyboardEvent,
};

#[path = "hosted_editor.rs"]
mod hosted;
#[path = "showcase.rs"]
mod showcase;

#[derive(Clone)]
struct SourceArea {
    area: HtmlTextAreaElement,
    buffer: Rc<RefCell<SourceBuffer>>,
    controls: Element,
    range: Element,
    previous: Element,
    next: Element,
    jump: HtmlInputElement,
    whole_selection: Rc<std::cell::Cell<bool>>,
    skip_ingest: Rc<std::cell::Cell<bool>>,
}
impl SourceArea {
    fn new(area: HtmlTextAreaElement) -> Result<Self, JsValue> {
        let parent = area
            .parent_element()
            .ok_or_else(|| js_error("source editor needs a parent"))?;
        let (parent, before): (Element, Element) = if parent
            .class_name()
            .split_ascii_whitespace()
            .any(|name| name == "pg__code")
        {
            (
                parent
                    .parent_element()
                    .ok_or_else(|| js_error("source editor needs a container"))?,
                parent,
            )
        } else {
            (parent, area.clone().unchecked_into())
        };
        let controls = element("div", "rvc-source-window", &parent)?;
        parent.insert_before(&controls, Some(&before))?;
        let range = element("span", "rvc-source-range", &controls)?;
        range.set_attribute("role", "status")?;
        let previous = element("button", "", &controls)?;
        previous.set_attribute("type", "button")?;
        previous.set_text_content(Some("Previous"));
        let next = element("button", "", &controls)?;
        next.set_attribute("type", "button")?;
        next.set_text_content(Some("Next"));
        let label = element("label", "", &controls)?;
        label.set_text_content(Some("Go to line "));
        let jump = element("input", "rvc-source-jump", &label)?.dyn_into::<HtmlInputElement>()?;
        jump.set_type("number");
        jump.set_min("1");
        jump.set_attribute("aria-label", "Go to source line")?;
        let copy = element("button", "", &controls)?;
        copy.set_attribute("type", "button")?;
        copy.set_attribute("data-source-copy", "")?;
        copy.set_text_content(Some("Copy all"));
        let save = element("button", "", &controls)?;
        save.set_attribute("type", "button")?;
        save.set_attribute("data-source-download", "")?;
        save.set_text_content(Some("Download all"));
        let this = Self {
            buffer: Rc::new(RefCell::new(SourceBuffer::new(area.value()))),
            area,
            controls,
            range,
            previous,
            next,
            jump,
            whole_selection: Rc::new(std::cell::Cell::new(false)),
            skip_ingest: Rc::new(std::cell::Cell::new(false)),
        };
        for (button, forward) in [(this.previous.clone(), false), (this.next.clone(), true)] {
            let state = this.clone();
            on(&button.into(), "click", move |_| {
                if forward {
                    state.buffer.borrow_mut().next_page();
                } else {
                    state.buffer.borrow_mut().previous_page();
                }
                state.reset_history();
                state.area.set_scroll_top(0);
            });
        }
        let state = this.clone();
        on(&this.jump.clone().into(), "change", move |_| {
            if let Ok(line) = state.jump.value().parse() {
                state.buffer.borrow_mut().go_to_line(line);
            }
            state.reset_history();
            state.area.set_scroll_top(0);
        });
        let state = this.clone();
        on(&copy.into(), "click", move |_| {
            let _ = window()
                .navigator()
                .clipboard()
                .write_text(state.buffer.borrow().full());
        });
        let state = this.clone();
        on(&save.into(), "click", move |_| {
            let _ = download("report.rmd", state.buffer.borrow().full(), "text/plain");
        });
        let state = this.clone();
        on(&this.area.clone().into(), "keydown", move |event| {
            if let Some(event) = event.dyn_ref::<web_sys::KeyboardEvent>() {
                if (event.ctrl_key() || event.meta_key()) && event.key().eq_ignore_ascii_case("a") {
                    state.whole_selection.set(true);
                    state.area.select();
                    event.prevent_default();
                } else if ((event.ctrl_key() || event.meta_key())
                    && matches!(event.key().to_ascii_lowercase().as_str(), "z" | "y"))
                    || matches!(
                        event.key().as_str(),
                        "ArrowLeft"
                            | "ArrowRight"
                            | "ArrowUp"
                            | "ArrowDown"
                            | "Home"
                            | "End"
                            | "PageUp"
                            | "PageDown"
                            | "Escape"
                    )
                {
                    state.whole_selection.set(false);
                }
            }
        });
        let state = this.clone();
        on(&this.area.clone().into(), "pointerdown", move |_| {
            state.whole_selection.set(false)
        });
        let state = this.clone();
        on(&this.area.clone().into(), "copy", move |event| {
            if state.whole_selection.get()
                && state.buffer.borrow().is_paged()
                && let Some(event) = event.dyn_ref::<web_sys::ClipboardEvent>()
                && let Some(data) = event.clipboard_data()
            {
                let _ = data.set_data("text/plain", state.buffer.borrow().full());
                event.prevent_default();
            }
        });
        let state = this.clone();
        on(&this.area.clone().into(), "cut", move |event| {
            if !state.whole_selection.get() || !state.buffer.borrow().is_paged() {
                return;
            }
            // Never fall back to cutting just the DOM page of a full selection.
            event.prevent_default();
            if let Some(event) = event.dyn_ref::<web_sys::ClipboardEvent>()
                && let Some(data) = event.clipboard_data()
            {
                let full = state.full();
                if data.set_data("text/plain", &full).is_ok() {
                    state.import_input(String::new());
                }
            }
        });
        let state = this.clone();
        on(&this.area.clone().into(), "paste", move |event| {
            if (state.whole_selection.get() || state.buffer.borrow().full().is_empty())
                && let Some(event) = event.dyn_ref::<web_sys::ClipboardEvent>()
                && let Some(data) = event.clipboard_data()
                && let Ok(source) = data.get_data("text/plain")
            {
                event.prevent_default();
                state.import_input(source);
            }
        });
        this.refresh();
        Ok(this)
    }
    fn full(&self) -> String {
        self.buffer.borrow().full().to_owned()
    }
    fn replace_full(&self, source: String) {
        self.buffer.borrow_mut().replace_full(source);
        self.whole_selection.set(false);
        self.reset_history();
    }
    fn ingest(&self, event: &Event) {
        if self.skip_ingest.replace(false) {
            return;
        }
        // A multi-megabyte textarea would force Blink to lay out every line.
        // Replace it with the retained buffer's small window in the same input task.
        let hiding = self.area.text_length() > 64 * 1024;
        if hiding {
            self.area.set_hidden(true);
        }
        let selection = (
            self.area.selection_start().ok().flatten(),
            self.area.selection_end().ok().flatten(),
        );
        let value = self.area.value();
        let old_start = self.buffer.borrow().visible_bytes().0;
        let typed = event.dyn_ref::<web_sys::InputEvent>();
        let history = typed.is_some_and(|event| {
            matches!(event.input_type().as_str(), "historyUndo" | "historyRedo")
        });
        let whole = self.whole_selection.replace(false);
        let replace = (whole && !history) || typed.is_none();
        if replace {
            self.buffer.borrow_mut().replace_full(value);
        } else {
            self.buffer.borrow_mut().update_textarea(value);
        }
        if replace || self.buffer.borrow().visible_bytes().0 != old_start {
            self.reset_history();
        } else {
            self.refresh();
        }
        if hiding {
            self.area.set_hidden(false);
        }
        if !replace {
            let length = self.area.text_length();
            let _ = self
                .area
                .set_selection_start(selection.0.map(|v| v.min(length)));
            let _ = self
                .area
                .set_selection_end(selection.1.map(|v| v.min(length)));
        }
    }
    fn import_input(&self, source: String) {
        self.replace_full(source);
        self.skip_ingest.set(true);
        if let Ok(input) = Event::new("input") {
            let _ = self.area.dispatch_event(&input);
        }
    }
    fn refresh(&self) {
        self.whole_selection.set(false);
        let buffer = self.buffer.borrow();
        let (first, last, total) = buffer.visible_lines();
        let visible = buffer.visible_textarea();
        if self.area.text_length() > 64 * 1024 || self.area.value() != visible {
            self.area.set_value(&visible);
        }
        let playground = self.area.id() == "source";
        self.area.set_wrap(if playground || buffer.is_paged() {
            "off"
        } else {
            "soft"
        });
        if playground {
            super::code::highlight(&self.area);
        }
        let _ = self
            .area
            .set_attribute("data-source-lines", &total.to_string());
        let _ = self
            .area
            .set_attribute("data-source-start", &first.to_string());
        let _ = self
            .area
            .set_attribute("data-source-end", &last.to_string());
        let _ = self.area.set_attribute(
            "data-source-paged",
            if buffer.is_paged() { "true" } else { "false" },
        );
        if buffer.is_paged() {
            let _ = self.controls.remove_attribute("hidden");
        } else {
            let _ = self.controls.set_attribute("hidden", "");
        }
        // In reader words: which lines the box shows, and that the rest is still rendered.
        self.range.set_text_content(Some(&format!(
            "Lines {first}–{last} of {total}. The report renders the whole source; Previous and Next move through it."
        )));
        self.jump.set_value(&first.to_string());
        self.jump.set_max(&total.to_string());
        for (button, disabled) in [
            (&self.previous, !buffer.has_previous()),
            (&self.next, !buffer.has_next()),
        ] {
            if disabled {
                let _ = button.set_attribute("disabled", "");
            } else {
                let _ = button.remove_attribute("disabled");
            }
        }
    }
    fn reset_history(&self) {
        // A changed programmatic value clears Chromium's native undo stack.
        // Force a change even when two windows contain identical text.
        self.area.set_value(if self.area.text_length() == 0 {
            " "
        } else {
            ""
        });
        self.refresh();
    }
}

struct Editor {
    source: SourceArea,
    /// The stored report of the hosted editor page, when this is that page.
    hosted: Option<hosted::Session>,
    /// The style pill the reader chose; empty renders the styles the source declares.
    style: String,
    preview: HtmlIFrameElement,
    status: Option<Element>,
    html: String,
    origin: String,
    runtime_js: Option<String>,
    runtime_wasm: Option<String>,
    runtime_wasm_codec: String,
    generation: u64,
    worker: Option<web_sys::Worker>,
    worker_busy: bool,
    worker_pending: Option<(u64, String, richview_core::document::RenderOptions)>,
    started: f64,
    options: richview_core::document::RenderOptions,
    /// Check was pressed: the next finished render says what it found, in the status line.
    announce: bool,
}
thread_local! {static EDITOR:RefCell<Option<Rc<RefCell<Editor>>>>=const{RefCell::new(None)};}

pub(crate) fn start() -> Result<(), JsValue> {
    if document()
        .get_element_by_id("richview-source-window-css")
        .is_none()
    {
        let sheet = document().create_element("style")?;
        sheet.set_id("richview-source-window-css");
        sheet.set_text_content(Some(".rvc-source-window{display:flex;flex-wrap:wrap;align-items:center;gap:6px;padding:8px 12px;font:12px/1.4 var(--rv-font-ui,ui-monospace,monospace);color:var(--rv-ink-soft,inherit)}.rvc-source-window[hidden]{display:none}.rvc-source-window span{flex-basis:100%}.rvc-source-window button,.rvc-source-window input{font:inherit;color:inherit;background:transparent;border:1px solid var(--rv-rule,oklch(0.738 0 0));padding:4px 7px}.rvc-source-window button:disabled{opacity:.4}.rvc-source-window label{padding:0;display:inline-flex;align-items:center;gap:5px;font:inherit}.rvc-source-window input{width:6em}.edit-body .rvc-source-window{grid-column:1/-1}"));
        if let Some(head) = document().head() {
            head.append_child(&sheet)?;
        }
    }
    if let (Some(src), Some(preview)) =
        (query("#source, #rv-source"), query("#preview, #rv-preview"))
    {
        let editor = Rc::new(RefCell::new(Editor {
            source: SourceArea::new(src.clone().dyn_into::<HtmlTextAreaElement>()?)?,
            hosted: None,
            style: String::new(),
            preview: preview.dyn_into::<HtmlIFrameElement>()?,
            status: query("#diagnostics, #rv-status"),
            html: String::new(),
            origin: window().location().href()?,
            runtime_js: None,
            runtime_wasm: None,
            runtime_wasm_codec: "raw".into(),
            generation: 0,
            worker: None,
            worker_busy: false,
            worker_pending: None,
            started: 0.0,
            options: Default::default(),
            announce: false,
        }));
        if let Ok(worker) = web_sys::Worker::new(&runtime_asset("worker.js")) {
            let weak = Rc::downgrade(&editor);
            let target: web_sys::EventTarget = worker.clone().into();
            on(&target, "message", move |event| {
                let Some(editor) = weak.upgrade() else { return };
                let Ok(event) = event.dyn_into::<web_sys::MessageEvent>() else {
                    return;
                };
                let Ok(encoded) = js_sys::JSON::stringify(&event.data()) else {
                    return;
                };
                let Ok(message) =
                    serde_json::from_str::<Value>(&encoded.as_string().unwrap_or_default())
                else {
                    return;
                };
                let mut e = editor.borrow_mut();
                e.worker_busy = false;
                if message["id"].as_u64() != Some(e.generation) {
                    drop(e);
                    flush_worker(&editor);
                    return;
                }
                if let Some(error) = message["error"].as_str() {
                    match serde_json::from_str::<Vec<richview_core::document::Diagnostic>>(error) {
                        Ok(found) => show_findings(&mut e, &found),
                        Err(_) => show_failure(&mut e, error),
                    }
                    drop(e);
                    flush_worker(&editor);
                    return;
                }
                if let Some(result) = message["result"].as_str()
                    && let Ok(result) = serde_json::from_str::<Value>(result)
                {
                    if let Some(references) = result["references"].as_array() {
                        let refs = references.clone();
                        let generation = e.generation;
                        e.worker_busy = true;
                        drop(e);
                        let editor = editor.clone();
                        spawn_local(resolve_worker_references(editor, generation, refs));
                        return;
                    } else if let Ok(report) = serde_json::from_value::<
                        richview_core::document::RenderedReport,
                    >(result["report"].clone())
                    {
                        let generation = e.generation;
                        let started = e.started;
                        e.worker_busy = true;
                        drop(e);
                        let editor = editor.clone();
                        spawn_local(async move {
                            let resolved = resolve_report_fonts(report).await;
                            let mut e = editor.borrow_mut();
                            if e.generation == generation {
                                match resolved {
                                    Ok(report) => set_report(&mut e, report, now() - started, None),
                                    Err(error) => {
                                        show_failure(&mut e, &error.as_string().unwrap_or_default())
                                    }
                                }
                            }
                            drop(e);
                            flush_worker(&editor);
                        });
                        return;
                    }
                }
                drop(e);
                flush_worker(&editor);
            });
            editor.borrow_mut().worker = Some(worker);
        }
        EDITOR.with(|e| *e.borrow_mut() = Some(editor.clone()));
        let playground = src.id() == "source";
        if playground {
            let state = editor.borrow().source.clone();
            let buffer = state.buffer.clone();
            super::complete::start(
                &state.area,
                Some(Box::new(move || {
                    let buffer = buffer.borrow();
                    let start = buffer.visible_bytes().0;
                    let normalize = |text: &str| text.replace("\r\n", "\n").replace('\r', "\n");
                    let prefix = normalize(&buffer.full()[..start]).encode_utf16().count();
                    (normalize(buffer.full()), prefix)
                })),
            );
            let area = state.area.clone();
            on(&state.area.clone().into(), "scroll", move |_| {
                super::code::sync_scroll(&area)
            });
            on(&state.area.into(), "keydown", move |event| {
                if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
                    super::complete::key(event);
                }
            });
        }
        let ed = editor.clone();
        on(&src.into(), "input", move |event| {
            ed.borrow().source.ingest(&event);
            if playground {
                super::complete::typed();
            }
            // What the last action said (Checked, Link copied) no longer describes the edited source.
            pg_status("");
            render_later(ed.clone());
        });
        bind_style_pills(&editor);
        for (id, mode) in [
            ("#copy-source, #rv-copy-md", 0),
            ("#download-source, #rv-download-md", 1),
            ("#download-html, #rv-download-html", 2),
            ("#validate", 3),
            ("#copy-link", 4),
        ] {
            if let Some(b) = query(id) {
                let ed = editor.clone();
                on(&b.into(), "click", move |_| {
                    let e = ed.borrow();
                    match mode {
                        0 => copy_then_say(e.source.full(), "Source copied."),
                        1 => {
                            if download("report.rmd", &e.source.full(), "text/markdown").is_ok() {
                                pg_status("Downloaded report.rmd.");
                            }
                        }
                        2 => {
                            drop(e);
                            export_report(ed.clone());
                        }
                        4 => {
                            let page = window().location().href().unwrap_or_default();
                            let page = page.split('#').next().unwrap_or_default().to_owned();
                            match crate::share::encode(&e.source.full(), &e.style, &page) {
                                Some(fragment) => copy_then_say(
                                    format!("{page}#{fragment}"),
                                    "Link copied. It holds the whole draft; nothing was uploaded.",
                                ),
                                None => pg_status(
                                    "This draft is too large for a link. Use Download .rmd to share it.",
                                ),
                            }
                        }
                        _ => {
                            drop(e);
                            // Check renders the source again, reading the folder's files anew, and
                            // says what it found once that render ends.
                            ed.borrow_mut().announce = true;
                            pg_status("Checking…");
                            render_later(ed.clone());
                        }
                    }
                });
            }
        }
        if query("body[data-richview-page='editor']").is_some() {
            hosted::start(&editor);
            return Ok(());
        }
        let ed = editor.clone();
        spawn_local(async move {
            let result = async {
                let params =
                    web_sys::UrlSearchParams::new_with_str(&window().location().search()?)?;
                let hash = window().location().hash().unwrap_or_default();
                if let Some((source, style)) = crate::share::decode(&hash) {
                    let mut e = ed.borrow_mut();
                    e.source.replace_full(source);
                    e.style = style;
                } else if let Some(source) = params.get("src") {
                    let url =
                        web_sys::Url::new_with_base(&source, &window().location().href()?)?.href();
                    let raw = fetch_text(&url).await?;
                    let mut e = ed.borrow_mut();
                    e.source.replace_full(raw);
                    e.origin = url;
                    // A link may name a builtin style, as the chart pages' style row does.
                    if let Some(style) = params.get("style")
                        && richview_core::builtin_style_names().contains(&style.as_str())
                    {
                        e.style = style;
                    }
                }
                sync_style_pills(&ed.borrow().style);
                render_editor(ed.clone()).await;
                Ok::<_, JsValue>(())
            }
            .await;
            if let Err(e) = result {
                error(&e);
                // The link's source could not be read, so there is nothing to render.
                show_failure(
                    &mut ed.borrow_mut(),
                    &format!(
                        "The playground could not open this source: {}",
                        e.as_string().unwrap_or_default()
                    ),
                );
            }
        });
    }
    if query("#report-frame").is_some() {
        showcase::start();
    }
    setup_tables();
    Ok(())
}
/// Say what a playground action did, in the status line under the toolbar.
fn pg_status(text: &str) {
    if let Some(status) = query("#pg-status") {
        status.set_text_content(Some(text));
    }
}

/// Copy `text`, then say `done` once the browser has taken it, or how to share another way when
/// it refuses the clipboard.
fn copy_then_say(text: String, done: &'static str) {
    let write = window().navigator().clipboard().write_text(&text);
    spawn_local(async move {
        match JsFuture::from(write).await {
            Ok(_) => pg_status(done),
            Err(_) => pg_status("The browser did not allow copying. Use Download .rmd instead."),
        }
    });
}

/// The findings a reader acts on, one line each with its source line: the host's own note that the
/// preview loads the site's cached runtime is the playground's doing, not the source's.
fn reader_findings(diagnostics: &[richview_core::document::Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|d| d.code != "assets.runtime-external")
        .map(|d| match d.line {
            Some(line) => format!("{} {}: {} (line {line})", d.level, d.path, d.message),
            None => format!("{} {}: {}", d.level, d.path, d.message),
        })
        .collect()
}

/// What Check found, for the status line under the toolbar: the counts of errors and warnings,
/// or that the source is clean. Notes (`info`) are listed but never counted as problems.
fn check_summary(diagnostics: &[richview_core::document::Diagnostic]) -> String {
    let count = |level: &str| diagnostics.iter().filter(|d| d.level == level).count();
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (count("error"), count("warning")) {
        (0, 0) => "Checked: no problems found.".into(),
        (0, w) => format!(
            "Checked: {}, listed under the source.",
            plural(w, "warning")
        ),
        (e, 0) => format!("Checked: {}, listed under the source.", plural(e, "error")),
        (e, w) => format!(
            "Checked: {} and {}, listed under the source.",
            plural(e, "error"),
            plural(w, "warning")
        ),
    }
}

/// A render that produced no report: the preview clears so it never shows stale content, the
/// reasons go under the source, and a pending Check says the report cannot render.
fn show_failure(e: &mut Editor, reasons: &str) {
    if e.hosted.is_some() {
        // The hosted editor keeps the last good report under a banner (FEA-SF-08).
        let found = [richview_core::document::Diagnostic::new(
            "error",
            "editor.render",
            reasons,
            "preview",
        )];
        hosted::found(e, &found, None);
        return;
    }
    e.html.clear();
    e.preview.set_srcdoc("");
    if let Some(status) = &e.status {
        status.set_text_content(Some(reasons));
    }
    if std::mem::take(&mut e.announce) {
        pg_status("Checked: the report cannot render; the reasons are listed under the source.");
    }
}

/// A render that produced findings and no report. The hosted editor places each on its line.
fn show_findings(e: &mut Editor, found: &[richview_core::document::Diagnostic]) {
    if e.hosted.is_some() {
        hosted::found(e, found, None);
    } else {
        show_failure(e, &reader_findings(found).join("\n"));
    }
}

/// Reflect the chosen style in the pills: one checked at most, one tab stop.
fn sync_style_pills(chosen: &str) {
    let Ok(buttons) = document().query_selector_all("#playground-styles [role=radio]") else {
        return;
    };
    for i in 0..buttons.length() {
        let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) else {
            continue;
        };
        let theme = button.get_attribute("data-theme").unwrap_or_default();
        let checked = !chosen.is_empty() && theme == chosen;
        let _ = button.set_attribute("aria-checked", if checked { "true" } else { "false" });
        let stop = checked || (chosen.is_empty() && i == 0);
        let _ = button.set_attribute("tabindex", if stop { "0" } else { "-1" });
    }
}

/// Pills choose one of the builtin styles; choosing the checked pill again clears it.
fn bind_style_pills(editor: &Rc<RefCell<Editor>>) {
    let Ok(buttons) = document().query_selector_all("#playground-styles [role=radio]") else {
        return;
    };
    let names: Vec<String> = (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<Element>().ok())
        .filter_map(|b| b.get_attribute("data-theme"))
        .collect();
    for i in 0..buttons.length() {
        let Some(button) = buttons
            .item(i)
            .and_then(|b| b.dyn_into::<HtmlElement>().ok())
        else {
            continue;
        };
        let choose = {
            let editor = editor.clone();
            move |name: String| {
                {
                    let mut e = editor.borrow_mut();
                    e.style = if e.style == name { String::new() } else { name };
                    sync_style_pills(&e.style);
                }
                render_later(editor.clone());
            }
        };
        let name = names.get(i as usize).cloned().unwrap_or_default();
        let click = choose.clone();
        let clicked = name.clone();
        on(&button.clone().into(), "click", move |_| {
            click(clicked.clone())
        });
        let names = names.clone();
        on(&button.into(), "keydown", move |event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            let step = match event.key().as_str() {
                "ArrowRight" | "ArrowDown" => 1,
                "ArrowLeft" | "ArrowUp" => names.len() - 1,
                _ => return,
            };
            event.prevent_default();
            let next = names[(i as usize + step) % names.len()].clone();
            choose(next.clone());
            if let Some(target) = query(&format!("#playground-styles [data-theme='{next}']"))
                .and_then(|b| b.dyn_into::<HtmlElement>().ok())
            {
                let _ = target.focus();
            }
        });
    }
}

fn render_later(editor: Rc<RefCell<Editor>>) {
    let generation = {
        let mut e = editor.borrow_mut();
        e.generation += 1;
        e.generation
    };
    let callback = Closure::once_into_js(move || {
        if editor.borrow().generation == generation {
            spawn_local(render_editor(editor));
        }
    });
    let _ = window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 32);
}
async fn render_editor(editor: Rc<RefCell<Editor>>) {
    let generation = editor.borrow().generation;
    font_catalog_ready().await;
    let wasm = super::preview_wasm().await;
    let js = super::preview_js().await;
    if editor.borrow().generation != generation {
        return;
    }
    let (wasm, js) = match wasm.and_then(|wasm| js.map(|js| (wasm, js))) {
        Ok(assets) => assets,
        Err(err) => {
            error(&err);
            show_failure(
                &mut editor.borrow_mut(),
                &format!(
                    "The preview's runtime did not load ({}). Reload the page to try again.",
                    err.as_string().unwrap_or_default()
                ),
            );
            return;
        }
    };
    let (source, theme, origin, generation) = {
        let e = editor.borrow();
        (
            e.source.full(),
            e.style.clone(),
            e.origin.clone(),
            e.generation,
        )
    };
    // A stored report published with styles of its own keeps them in the editor, as the host
    // renders it again with them.
    let kept_styles = editor
        .borrow()
        .hosted
        .as_ref()
        .map(|s| s.styles())
        .unwrap_or_default();
    let mut options = richview_core::document::RenderOptions {
        styles: if theme.is_empty() {
            kept_styles
        } else {
            vec![theme]
        },
        source: Some(origin.clone()),
        // The parent supplies both assets: an opaque preview cannot send Access cookies with
        // a CORS/SRI request, or reliably fetch Wasm under the inherited policy.
        runtime_js: Some(js),
        runtime_wasm: Some(wasm),
        runtime_wasm_codec: "raw".into(),
        ..Default::default()
    };
    let hosted_folder = editor.borrow().hosted.as_ref().map(|s| s.folder());
    let folder = if let Some(folder) = hosted_folder {
        folder
    } else if origin
        .split('?')
        .next()
        .is_some_and(|s| s.ends_with(".rmd") || s.ends_with(".rslide"))
    {
        let result = async {
            let manifest=web_sys::Url::new_with_base("report-folder.json",&origin)?;
            let bytes=fetch_text(&manifest.href()).await?;
            let mut folder: richview_core::document::ReportFolder=serde_json::from_str(&bytes).map_err(js_error)?;
            let source_url=web_sys::Url::new(&origin)?;
            if source_url.pathname().rsplit('/').next()!=Some(folder.source.as_str()) {return Err(js_error("Report-folder inventory does not name this source. Give each report its own folder and matching report-folder.json."));}
            folder.root=web_sys::Url::new_with_base(".",&origin)?.href();
            // Each finding by its code, so the reader can look it up.
            richview_core::document::validate_report_folder(Some(&folder)).map_err(|d|js_error(d.iter().map(|d|format!("{}: {}",d.code,d.message)).collect::<Vec<_>>().join("\n")))?;
            Ok::<_,JsValue>(folder)
        }.await;
        match result {
            Ok(folder) => folder,
            Err(error) => {
                show_failure(
                    &mut editor.borrow_mut(),
                    &format!(
                        "report.root: {}\nGive the report its own folder, with a report-folder.json that names this source.",
                        error.as_string().unwrap_or_default()
                    ),
                );
                return;
            }
        }
    } else {
        // The host explicitly creates one isolated in-memory folder for a new inline draft.
        richview_core::document::ReportFolder {
            root: "playground-report".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }
    };
    options.report_folder = Some(folder.clone());
    // A small source is prepared here; a dense one goes to the worker below, which does the same.
    let mut prepared = None;
    if source.len() <= 128 * 1024 {
        // A missing, escaping or remote file is a finding on its element and the rest renders, as
        // in the native host (FEA-RP-30). Only the files the folder holds are fetched.
        let (document, files) = match crate::prepare_draft(&source, &folder) {
            Ok(draft) => draft,
            Err(found) => {
                let mut e = editor.borrow_mut();
                if e.generation == generation {
                    show_findings(&mut e, &found);
                }
                return;
            }
        };
        // The document's own files first, then the files its styles name: the company logos,
        // which a style file fetched in the first round may ask for (STY-72).
        let mut round = files;
        for styles_read in [false, true] {
            for reference in std::mem::take(&mut round) {
                match report_resource(&editor, &reference.path, reference.kind, &origin).await {
                    Ok(bytes) => {
                        options.data.insert(reference.path, bytes);
                    }
                    Err(error) => {
                        let mut e = editor.borrow_mut();
                        if e.generation == generation {
                            show_failure(
                                &mut e,
                                &format!(
                                    "error {}: this report folder names the file, but it could not be read ({}).",
                                    reference.path,
                                    error.as_string().unwrap_or_default()
                                ),
                            );
                        }
                        return;
                    }
                }
            }
            if !styles_read {
                round = crate::style_files(&document, &folder, &options);
            }
        }
        prepared = Some(document);
    }
    let started = now();
    {
        let mut e = editor.borrow_mut();
        if e.generation != generation {
            return;
        }
        e.options = options.clone();
    }
    if source.len() > 128 * 1024 && editor.borrow().worker.is_some() {
        queue_worker(&editor, generation, source, options);
        return;
    }
    let rendered = match prepared {
        Some(document) => richview_core::document::render_document(document, &options),
        None => richview_core::document::render(&source, &options),
    };
    match rendered {
        Ok(report) => {
            let report = match resolve_report_fonts(report).await {
                Ok(report) => report,
                Err(error) => {
                    let mut e = editor.borrow_mut();
                    if e.generation == generation {
                        show_failure(&mut e, &error.as_string().unwrap_or_default());
                    }
                    return;
                }
            };
            let elapsed = now() - started;
            // Check also runs the chart geometry checks (FEA-SF-34), so it lists what
            // `richview check` reports for the same source.
            let checked = editor
                .borrow()
                .announce
                .then(|| check_findings(&source, &options))
                .flatten();
            let mut e = editor.borrow_mut();
            if e.generation != generation {
                return;
            }
            set_report(&mut e, report, elapsed, checked);
        }
        Err(errors) => {
            let mut e = editor.borrow_mut();
            if e.generation == generation {
                show_findings(&mut e, &errors);
            }
        }
    }
}

/// The findings of `check` on a source that renders: its own, its files' and its charts' geometry.
fn check_findings(
    source: &str,
    options: &richview_core::document::RenderOptions,
) -> Option<Vec<richview_core::document::Diagnostic>> {
    serde_json::from_value(crate::check_result(source, options)["diagnostics"].clone()).ok()
}
fn queue_worker(
    editor: &Rc<RefCell<Editor>>,
    generation: u64,
    source: String,
    options: richview_core::document::RenderOptions,
) {
    let mut e = editor.borrow_mut();
    if e.worker_busy {
        e.worker_pending = Some((generation, source, options));
        if let Some(status) = &e.status {
            status.set_text_content(Some(
                "Waiting for the current Rust render; newest source will follow.",
            ));
        }
        return;
    }
    let Some(worker) = e.worker.clone() else {
        return;
    };
    e.worker_busy = true;
    e.started = now();
    e.options = options.clone();
    let message = json!({"id":generation,"source":source,"options":serde_json::to_string(&options).unwrap_or_default()});
    let result =
        js_sys::JSON::parse(&message.to_string()).and_then(|value| worker.post_message(&value));
    if let Err(error) = result {
        e.worker_busy = false;
        if let Some(status) = &e.status {
            status.set_text_content(error.as_string().as_deref());
        }
    } else if let Some(status) = &e.status {
        status.set_text_content(Some("Rendering dense source in Rust worker…"));
    }
}
fn flush_worker(editor: &Rc<RefCell<Editor>>) {
    let pending = {
        let mut e = editor.borrow_mut();
        e.worker_busy = false;
        e.worker_pending.take().filter(|p| p.0 == e.generation)
    };
    if let Some((generation, source, options)) = pending {
        queue_worker(editor, generation, source, options);
    }
}

async fn resolve_worker_references(
    editor: Rc<RefCell<Editor>>,
    generation: u64,
    references: Vec<Value>,
) {
    let initial = {
        let e = editor.borrow();
        if e.generation != generation {
            None
        } else {
            Some((e.origin.clone(), e.options.clone()))
        }
    };
    let Some((origin, mut options)) = initial else {
        flush_worker(&editor);
        return;
    };
    for reference in references {
        let path = reference["path"].as_str().unwrap_or_default();
        let result = report_resource(
            &editor,
            path,
            reference["kind"].as_str().unwrap_or("data"),
            &origin,
        )
        .await;
        match result {
            Ok(bytes) => {
                options.data.insert(path.into(), bytes);
            }
            Err(error) => {
                let mut e = editor.borrow_mut();
                if e.generation == generation {
                    show_failure(
                        &mut e,
                        &format!(
                            "error {path}: this report folder names the file, but it could not be read ({}).",
                            error.as_string().unwrap_or_default()
                        ),
                    );
                }
                drop(e);
                flush_worker(&editor);
                return;
            }
        }
    }
    let mut e = editor.borrow_mut();
    if e.generation != generation {
        drop(e);
        flush_worker(&editor);
        return;
    }
    e.options = options.clone();
    if let Some(worker) = &e.worker {
        let message = json!({"id":generation,"source":"","options":serde_json::to_string(&options).unwrap_or_default()});
        if let Ok(message) = js_sys::JSON::parse(&message.to_string())
            && worker.post_message(&message).is_err()
        {
            drop(e);
            flush_worker(&editor);
        }
    }
}

/// The media type a report file's extension stands for.
fn media_mime(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

/// A file the render asks for: the stored version's own when the hosted editor holds one, else
/// the page's folder fetched from the network.
async fn report_resource(
    editor: &Rc<RefCell<Editor>>,
    path: &str,
    kind: &str,
    origin: &str,
) -> Result<String, JsValue> {
    if let Some(session) = &editor.borrow().hosted {
        return session
            .resource(path, kind)
            .ok_or_else(|| js_error("this version of the report holds no such file"));
    }
    fetch_report_resource(path, kind, origin).await
}

async fn fetch_report_resource(path: &str, kind: &str, origin: &str) -> Result<String, JsValue> {
    if !richview_core::document::report_relative_path(path) {
        return Err(js_error("Resource escapes the dedicated report folder"));
    }
    let root = web_sys::Url::new_with_base(".", origin)?;
    let url = web_sys::Url::new_with_base(path, &root.href())?;
    if url.origin() != root.origin() || !url.pathname().starts_with(&root.pathname()) {
        return Err(js_error("Resource URL escapes report root"));
    }
    if kind == "media" {
        let bytes = fetch_base64(&url.href()).await?;
        Ok(format!("data:{};base64,{bytes}", media_mime(path)))
    } else {
        fetch_text(&url.href()).await
    }
}

/// Show a finished render: the report in the preview, its findings under the source, or Check's
/// findings when Check ran, and Check's summary in the status line.
fn set_report(
    e: &mut Editor,
    report: richview_core::document::RenderedReport,
    elapsed: f64,
    checked: Option<Vec<richview_core::document::Diagnostic>>,
) {
    let found = checked.as_deref().unwrap_or(&report.diagnostics);
    let diagnostics = reader_findings(found).join("\n");
    let charts = report
        .metadata
        .pointer("/_richview/chart_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if let Some(status) = &e.status {
        status.set_text_content(Some(&format!(
            "Rust render {elapsed:.1} ms · {charts} chart{}\n{}",
            if charts == 1 { "" } else { "s" },
            if diagnostics.is_empty() {
                "No source diagnostics."
            } else {
                &diagnostics
            }
        )));
    }
    if std::mem::take(&mut e.announce) {
        pg_status(&check_summary(found));
    }
    // The hosted editor does not replace a good report with one that has errors in it: the last
    // good report stays under a banner until the source is fixed (FEA-SF-08). The first render
    // has no earlier report to keep, so it shows what it has.
    if e.hosted.is_some() && !e.html.is_empty() && found.iter().any(|d| d.level == "error") {
        hosted::found(e, found, None);
        return;
    }
    e.preview.set_srcdoc(&report.html);
    e.html = report.html;
    if e.hosted.is_some() {
        hosted::found(e, found, Some(elapsed));
    }
}
// Runtime assets are siblings of the trusted host script. Versioned report
// authoring can then coexist with an unchanged playground at the site root.
pub(super) fn runtime_asset(name: &str) -> String {
    let script = query("script[src$='richview_site.js']")
        .and_then(|e| e.get_attribute("src"))
        .unwrap_or_else(|| "/richview_site.js".into());
    web_sys::Url::new_with_base(&script, &window().location().href().unwrap_or_default())
        .and_then(|script| web_sys::Url::new_with_base(name, &script.href()))
        .map(|url| url.href())
        .unwrap_or_else(|_| format!("/{name}"))
}
fn export_report(editor: Rc<RefCell<Editor>>) {
    let (source, mut options) = {
        let e = editor.borrow();
        let mut options = e.options.clone();
        options.runtime_url = None;
        (e.source.full(), options)
    };
    options.runtime_url = None;
    spawn_local(async move {
        font_catalog_ready().await;
        let result = async {
            let document = richview_core::document::parse(&source)
                .map_err(|errors| js_error(serde_json::to_string(&errors).unwrap_or_default()))?;
            if richview_core::document::report_resources(&document).iter().any(|r| r.kind == "link") {
                return Err(js_error("This report links local files. Use native Generate or VS Code Export to copy those files together with HTML into Rendered; a browser HTML download cannot bundle them."));
            }
            if editor.borrow().runtime_wasm.is_none() {
                // Preview uses cached external bindings. Offline bytes are needed only
                // after an explicit export, and compressed bytes stay compressed in HTML.
                let url = runtime_asset("richview_runtime.js");
                let js = fetch_text(&url).await?;
                let wasm_url = url.replace("richview_runtime.js", "richview_runtime_bg.wasm");
                let (wasm, codec) = match fetch_base64(&format!("{wasm_url}.gz")).await {
                    Ok(bytes) if bytes.starts_with("H4sI") => (bytes, "gzip"),
                    // A host may transparently decode its gzip response.
                    Ok(bytes) if bytes.starts_with("AGFzbQ") => (bytes, "raw"),
                    _ => (fetch_base64(&wasm_url).await?, "raw"),
                };
                let mut e = editor.borrow_mut();
                e.runtime_js = Some(js);
                e.runtime_wasm = Some(wasm);
                e.runtime_wasm_codec = codec.into();
            }
            {
                let e = editor.borrow();
                options.runtime_js = e.runtime_js.clone();
                options.runtime_wasm = e.runtime_wasm.clone();
                options.runtime_wasm_codec = e.runtime_wasm_codec.clone();
            }
            let report = richview_core::document::render(&source, &options)
                .map_err(|errors| js_error(serde_json::to_string(&errors).unwrap_or_default()))?;
            let report = resolve_report_fonts(report).await?;
            let name=options.report_folder.as_ref().map(|f| f.source.trim_end_matches(".rmd").trim_end_matches(".rslide")).unwrap_or("report");
            download(&format!("{name}.html"), &report.html, "text/html")?;
            if let Some(status)=&editor.borrow().status {status.set_text_content(Some("Downloaded self-contained HTML. Save it in the report folder’s Rendered subfolder. Use native/VS Code Export to bundle local document links."));}
            Ok::<_,JsValue>(())
        }
        .await;
        if let Err(error) = result
            && let Some(status) = &editor.borrow().status
        {
            status.set_text_content(Some(&format!(
                "Export: {}",
                error.as_string().unwrap_or_default()
            )));
        }
    });
}
pub(crate) fn download(name: &str, text: &str, mime: &str) -> Result<(), JsValue> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime);
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;
    let a = document()
        .create_element("a")?
        .dyn_into::<HtmlAnchorElement>()?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    web_sys::Url::revoke_object_url(&url)?;
    Ok(())
}
pub(super) async fn fetch_base64(url: &str) -> Result<String, JsValue> {
    let response = JsFuture::from(window().fetch_with_str(url))
        .await?
        .dyn_into::<Response>()?;
    if !response.ok() {
        return Err(js_error(format!("{} {url}", response.status())));
    }
    let buffer = JsFuture::from(response.array_buffer()?).await?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for chunk in bytes.chunks(3) {
        let bits = (chunk[0] as u32) << 16
            | (chunk.get(1).copied().unwrap_or(0) as u32) << 8
            | chunk.get(2).copied().unwrap_or(0) as u32;
        output.push(TABLE[((bits >> 18) & 63) as usize] as char);
        output.push(TABLE[((bits >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((bits >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(bits & 63) as usize] as char
        } else {
            '='
        });
    }
    Ok(output)
}

fn quote_font(v: &str, key: &str) -> String {
    if !key.starts_with("font-") {
        return v.into();
    }
    v.split(',')
        .map(|n| {
            let n = n.trim();
            if n.contains(' ') && !n.starts_with(['\'', '"']) {
                format!("'{n}'")
            } else {
                n.into()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn setup_tables() {
    let doc_target: web_sys::EventTarget = document().into();
    on(&doc_target, "click", |event| {
        let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        let Ok(Some(button)) = target.closest("[data-rv-sort]") else {
            return;
        };
        let Ok(Some(table)) = button.closest(".rv-table") else {
            return;
        };
        let Ok(Some(payload)) = table.query_selector("script[data-rv-table]") else {
            return;
        };
        let Ok(mut config) =
            serde_json::from_str::<Value>(&payload.text_content().unwrap_or_default())
        else {
            return;
        };
        let key = button.get_attribute("data-rv-sort").unwrap_or_default();
        let descending = config["sort"]["key"] == key && config["sort"]["dir"] == "asc";
        config["sort"] = json!({"key":key,"dir":if descending{"desc"}else{"asc"}});
        let html = richview_core::render_table(&config);
        if let Ok(temp) = document().create_element("div") {
            temp.set_inner_html(&html);
            if let (Some(replacement), Some(parent)) =
                (temp.first_element_child(), table.parent_node())
            {
                let _ = parent.replace_child(&replacement, &table);
            }
        }
    });
}
