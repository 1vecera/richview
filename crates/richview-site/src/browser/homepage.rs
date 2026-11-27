//! The home page's behaviour: the prompt boxes' Copy buttons, the live example, Vee's small clips
//! and the release-updates signup.
//!
//! The example is a real two-file report folder (`report.rmd` plus optional styling YAML)
//! rendered by the Rust core as the visitor types. Theme controls change the source's own
//! `styles:` line, so the visible source always matches the visible report.
use super::{document, js_error, now, on, query, window};
use richview_core::document::Diagnostic;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    Element, HtmlElement, HtmlIFrameElement, HtmlInputElement, HtmlSelectElement,
    HtmlTextAreaElement, KeyboardEvent,
};

// The live example edits the anchor report. Its folder also holds the style file of the
// second tab and the CSV its chart reads; the page has no folder, so the CSV comes with the page.
const DEMO: &str = include_str!("../../../../assets/landing/anchor/report.rmd");
const STYLE: &str = include_str!("../../../../assets/landing/anchor/report.rstyle.yaml");
const STYLE_FILE: &str = "report.rstyle.yaml";
const DATA_FILE: &str = "data/life-expectancy-by-region.csv";
const DATA: &str =
    include_str!("../../../../assets/landing/anchor/data/life-expectancy-by-region.csv");
/// The style names of the demo's pills, in order. The build writes the pills from core's list
/// (FEA-SF-30), so the page and the engine agree on which styles exist.
fn pills() -> Vec<String> {
    let Ok(buttons) = document().query_selector_all("#demo-themes [role=radio]") else {
        return Vec::new();
    };
    (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<Element>().ok())
        .filter_map(|b| b.get_attribute("data-theme"))
        .collect()
}
/// Long enough to coalesce a burst of keystrokes, short enough to feel live.
const DEBOUNCE_MS: i32 = 140;

#[derive(Default)]
struct Demo {
    timer: Option<i32>,
    tick: Option<Closure<dyn FnMut()>>,
    /// Sequence of the latest successful render; only that render may take the screen.
    latest: u32,
    /// Latest requested edit, including one whose runtime bytes are still loading.
    requested: u32,
    /// Explicit host style for sources without frontmatter.
    fallback_theme: Option<String>,
}
thread_local! {static DEMO_STATE: RefCell<Demo> = RefCell::new(Demo::default());}

fn area(id: &str) -> Option<HtmlTextAreaElement> {
    query(id).and_then(|e| e.dyn_into::<HtmlTextAreaElement>().ok())
}

fn set_status(state: &str, text: &str) {
    if let Some(status) = query("#demo-status") {
        let _ = status.set_attribute("data-state", state);
        if let Ok(Some(span)) = status.query_selector("span") {
            span.set_text_content(Some(text));
        }
    }
}

fn show_diagnostics(items: &[Diagnostic], level: &str) {
    let Some(list) = query("#demo-diagnostics") else {
        return;
    };
    if items.is_empty() {
        let _ = list.set_attribute("hidden", "");
        list.set_inner_html("");
        return;
    }
    let _ = list.set_attribute("data-level", level);
    let mut html = String::new();
    for d in items.iter().take(6) {
        let place = d
            .line
            .map(|l| format!("line {l}"))
            .unwrap_or_else(|| d.path.clone());
        html.push_str(&format!(
            "<li><b>{}</b>{}</li>",
            escape(&place),
            escape(&d.message)
        ));
    }
    if items.len() > 6 {
        html.push_str(&format!("<li>…and {} more</li>", items.len() - 6));
    }
    list.set_inner_html(&html);
    let _ = list.remove_attribute("hidden");
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn veil(visible: bool) {
    if let Some(v) = query("#demo-veil") {
        if visible {
            let _ = v.remove_attribute("hidden");
        } else {
            let _ = v.set_attribute("hidden", "");
        }
    }
}

/// Mark the output as pending and render once typing pauses.
fn schedule() {
    set_status("pending", "Updating the report…");
    DEMO_STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.requested += 1;
        if let Some(timer) = s.timer.take() {
            window().clear_timeout_with_handle(timer);
        }
        let tick = s
            .tick
            .get_or_insert_with(|| Closure::wrap(Box::new(render_now) as Box<dyn FnMut()>));
        s.timer = window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                tick.as_ref().unchecked_ref(),
                DEBOUNCE_MS,
            )
            .ok();
    });
}

fn render_now() {
    let requested = DEMO_STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.requested += 1;
        if let Some(timer) = s.timer.take() {
            window().clear_timeout_with_handle(timer);
        }
        s.requested
    });
    spawn_local(async move {
        // The page does not load the report-face catalog; the first render does.
        super::font_catalog_ready().await;
        let wasm = super::preview_wasm().await;
        let js = super::preview_js().await;
        if DEMO_STATE.with(|s| s.borrow().requested) != requested {
            return;
        }
        match wasm.and_then(|wasm| js.map(|js| (wasm, js))) {
            Ok((wasm, js)) => render_report(wasm, js, requested).await,
            Err(error) => {
                super::error(&error);
                set_status("error", &error.as_string().unwrap_or_default());
                veil(true);
            }
        }
    });
}

/// The example's folder as the core reads it: the source, the style file in the second tab and
/// the CSV the page brings. The render and the completion list both read it, so a field slot
/// offers the columns the chart reads.
pub(crate) fn completion_options() -> richview_core::document::RenderOptions {
    let mut options = richview_core::document::RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "homepage-demo".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into(), STYLE_FILE.into(), DATA_FILE.into()],
        }),
        ..Default::default()
    };
    if let Some(style_area) = area("#demo-style") {
        options.data.insert(STYLE_FILE.into(), style_area.value());
    }
    options.data.insert(DATA_FILE.into(), DATA.into());
    options
}

async fn render_report(wasm: String, js: String, requested: u32) {
    let Some(source) = area("#demo-source") else {
        return;
    };
    let source = source.value();
    let fallback = DEMO_STATE.with(|s| s.borrow().fallback_theme.clone());
    let mut options = completion_options();
    options.runtime_js = Some(js);
    options.runtime_wasm = Some(wasm);
    options.runtime_wasm_codec = "raw".into();
    if richview_core::primary_style(&source).is_none()
        && let Some(theme) = fallback
    {
        options.styles = vec![theme];
    }
    if let Some(field) = crate::tools::missing_chart_field(&source, &options) {
        set_status(
            "error",
            &format!("This chart can't be drawn yet: column '{field}' is missing from its data."),
        );
        show_diagnostics(&[], "error");
        veil(false);
        return;
    }
    let started = now();
    match richview_core::document::render(&source, &options) {
        Ok(report) => {
            let report = match super::resolve_report_fonts(report).await {
                Ok(report) => report,
                Err(error) => {
                    if DEMO_STATE.with(|s| s.borrow().requested) != requested {
                        return;
                    }
                    set_status("error", &error.as_string().unwrap_or_default());
                    veil(true);
                    return;
                }
            };
            if DEMO_STATE.with(|s| s.borrow().requested) != requested {
                return;
            }
            let elapsed = now() - started;
            let seq = DEMO_STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.latest += 1;
                s.latest
            });
            if let Some(next) =
                query("#demo-result-next").and_then(|e| e.dyn_into::<HtmlIFrameElement>().ok())
            {
                let _ = next.set_attribute("data-seq", &seq.to_string());
                next.set_srcdoc(&report.html);
            }
            veil(false);
            let issues: Vec<Diagnostic> = report
                .diagnostics
                .into_iter()
                .filter(|d| d.level != "info")
                .collect();
            if issues.is_empty() {
                set_status(
                    "live",
                    &format!("Live · rendered from this exact source in {elapsed:.0} ms."),
                );
            } else {
                set_status(
                    "warn",
                    &format!(
                        "Rendered with {} diagnostic{}. Invalid blocks stay visible as source.",
                        issues.len(),
                        if issues.len() == 1 { "" } else { "s" }
                    ),
                );
            }
            show_diagnostics(&issues, "warning");
        }
        Err(diagnostics) => {
            set_status(
                "error",
                "Can’t render this source. The report on the right was not updated.",
            );
            show_diagnostics(&diagnostics, "error");
            veil(true);
        }
    }
    super::toolbar::rendered();
    super::landing::invalidate();
}

/// Swap the freshly loaded buffer in, so the visible report never flashes blank.
fn swap_buffers(loaded: &HtmlIFrameElement) {
    let seq: u32 = loaded
        .get_attribute("data-seq")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let latest = DEMO_STATE.with(|s| s.borrow().latest);
    if seq == 0 || seq != latest || loaded.get_attribute("data-buffer").as_deref() != Some("next") {
        return;
    }
    if let Some(current) = query("#demo-result") {
        let _ = current.set_attribute("id", "demo-result-next");
        let _ = current.set_attribute("data-buffer", "next");
        let _ = current.set_attribute("aria-hidden", "true");
        let _ = current.set_attribute("tabindex", "-1");
    }
    let _ = loaded.set_attribute("id", "demo-result");
    let _ = loaded.set_attribute("data-buffer", "current");
    let _ = loaded.remove_attribute("aria-hidden");
    // Under the scene's report the live frame stays out of the Tab order until the hand-over.
    let covered = loaded
        .parent_element()
        .is_some_and(|viewport| viewport.has_attribute("data-covered"));
    if covered {
        let _ = loaded.set_attribute("tabindex", "-1");
    } else {
        let _ = loaded.remove_attribute("tabindex");
    }
}

/// Reflect the source's base style in the theme controls.
fn sync_theme_ui() {
    let source = area("#demo-source").map(|a| a.value()).unwrap_or_default();
    let current = richview_core::primary_style(&source)
        .or_else(|| DEMO_STATE.with(|s| s.borrow().fallback_theme.clone()))
        .unwrap_or_default();
    if let Ok(buttons) = document().query_selector_all("#demo-themes [role=radio]") {
        let any = pills().contains(&current);
        for i in 0..buttons.length() {
            if let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) {
                let theme = button.get_attribute("data-theme").unwrap_or_default();
                let checked = theme == current;
                let _ =
                    button.set_attribute("aria-checked", if checked { "true" } else { "false" });
                // Roving focus: the checked pill, or the first one, is the tab stop.
                let stop = checked || (!any && i == 0);
                let _ = button.set_attribute("tabindex", if stop { "0" } else { "-1" });
            }
        }
    }
    if let Some(select) =
        query("#demo-more-themes").and_then(|e| e.dyn_into::<HtmlSelectElement>().ok())
    {
        let other = !pills().contains(&current)
            && select
                .query_selector(&format!("option[value='{current}']"))
                .ok()
                .flatten()
                .is_some();
        select.set_value(if other { &current } else { "" });
        let class = if other { "is-chosen" } else { "" };
        select.set_class_name(class);
    }
}

/// A theme is a one-word change to the visible source.
fn apply_theme(theme: &str) {
    let Some(source) = area("#demo-source") else {
        return;
    };
    super::writing::hand_over();
    super::complete::hide();
    match richview_core::with_primary_style(&source.value(), theme) {
        Some(next) => {
            source.set_value(&next);
            DEMO_STATE.with(|s| s.borrow_mut().fallback_theme = None);
        }
        None => DEMO_STATE.with(|s| s.borrow_mut().fallback_theme = Some(theme.into())),
    }
    super::code::highlight(&source);
    flash_styles_line();
    sync_theme_ui();
    set_status("pending", "Updating the report…");
    render_now();
}

/// Briefly mark the frontmatter `styles:` key so the source change is visible.
fn flash_styles_line() {
    let Ok(nodes) = document().query_selector_all("#panel-report .code__hl .hl-key") else {
        return;
    };
    for i in 0..nodes.length() {
        if let Some(node) = nodes.item(i).and_then(|n| n.dyn_into::<Element>().ok())
            && node.text_content().as_deref() == Some("styles")
        {
            let _ = node.set_attribute("class", "hl-key hl-flash");
            break;
        }
    }
}

fn select_tab(index: usize, focus: bool) {
    super::complete::hide();
    let Ok(tabs) = document().query_selector_all(".demo .tab") else {
        return;
    };
    for i in 0..tabs.length() {
        let Some(tab) = tabs.item(i).and_then(|t| t.dyn_into::<HtmlElement>().ok()) else {
            continue;
        };
        let selected = i as usize == index;
        let _ = tab.set_attribute("aria-selected", if selected { "true" } else { "false" });
        let _ = tab.set_attribute("tabindex", if selected { "0" } else { "-1" });
        if let Some(panel) = tab
            .get_attribute("aria-controls")
            .and_then(|id| query(&format!("#{id}")))
        {
            if selected {
                let _ = panel.remove_attribute("hidden");
            } else {
                let _ = panel.set_attribute("hidden", "");
            }
        }
        if selected && focus {
            let _ = tab.focus();
        }
    }
    for id in ["#demo-source", "#demo-style"] {
        if let Some(a) = area(id) {
            super::code::highlight(&a);
        }
    }
    super::toolbar::show_tools(index == 0);
    super::landing::invalidate();
}

/// Add prose at the same block boundary as the existing report tools.
fn start_prose_tools(source: &HtmlTextAreaElement) {
    let placed = Rc::new(Cell::new(false));
    for event in ["pointerdown", "keydown", "input"] {
        let placed = placed.clone();
        on(&source.clone().into(), event, move |_| placed.set(true));
    }
    for (name, block) in [
        ("heading", "## New heading\n"),
        ("paragraph", "A new paragraph.\n"),
    ] {
        let Some(button) = query(&format!("#demo-tools [data-add='{name}']")) else {
            continue;
        };
        let _ = button.remove_attribute("disabled");
        let source = source.clone();
        let placed = placed.clone();
        on(&button.into(), "click", move |_| {
            super::writing::hand_over();
            super::complete::hide();
            let at = placed
                .get()
                .then(|| source.selection_end().ok().flatten().unwrap_or(0) as usize);
            let (edit, line) = crate::tools::insertion(&source.value(), at, block);
            super::complete::put(&source, edit.start as u32, edit.end as u32, &edit.text);
            super::complete::hide();
            if let Some(status) = query("#demo-tools-status") {
                status.set_text_content(Some(&format!("Added a {name} at line {line}.")));
            }
        });
    }
}

fn arrow_index(key: &str, current: usize, count: usize) -> Option<usize> {
    match key {
        "ArrowRight" | "ArrowDown" => Some((current + 1) % count),
        "ArrowLeft" | "ArrowUp" => Some((current + count - 1) % count),
        "Home" => Some(0),
        "End" => Some(count - 1),
        _ => None,
    }
}

fn start_demo() -> Result<(), JsValue> {
    let (Some(source), Some(style_area)) = (area("#demo-source"), area("#demo-style")) else {
        return Ok(());
    };
    // The inputs are usable while the site runtime downloads. Catch edits made before listeners
    // attach so the first live render reflects what the reader actually typed.
    let edited_before_start = source.value() != source.default_value()?
        || style_area.value() != style_area.default_value()?;
    // A second iframe receives each render; it becomes visible once loaded.
    if let (Some(current), Some(viewport)) = (query("#demo-result"), query(".report-viewport")) {
        let next = document().create_element("iframe")?;
        for name in ["title", "sandbox"] {
            if let Some(value) = current.get_attribute(name) {
                next.set_attribute(name, &value)?;
            }
        }
        next.set_attribute("id", "demo-result-next")?;
        next.set_attribute("data-buffer", "next")?;
        next.set_attribute("aria-hidden", "true")?;
        next.set_attribute("tabindex", "-1")?;
        current.set_attribute("data-buffer", "current")?;
        viewport.insert_before(&next, current.next_sibling().as_ref())?;
        for frame in [current.clone(), next] {
            let loaded = frame.clone().dyn_into::<HtmlIFrameElement>()?;
            on(&frame.into(), "load", move |_| {
                swap_buffers(&loaded);
                if loaded.id() == "demo-result"
                    && (loaded.get_attribute("src").as_deref() == Some("/landing/demo")
                        || loaded.has_attribute("srcdoc"))
                    && let Some(fallback) = query("#demo-fallback")
                {
                    let _ = fallback.set_attribute("hidden", "");
                }
            });
        }
        current.set_attribute("src", "/landing/demo")?;
        current.remove_attribute("hidden")?;
    }
    super::complete::start(&source, None);
    for a in [source.clone(), style_area.clone()] {
        super::code::highlight(&a);
        let typed = a.clone();
        on(&a.clone().into(), "input", move |_| {
            super::writing::hand_over();
            super::code::highlight(&typed);
            if typed.id() == "demo-source" {
                sync_theme_ui();
                super::complete::typed();
                super::toolbar::typed();
            }
            schedule();
            super::landing::invalidate();
        });
        let scrolled = a.clone();
        on(&a.clone().into(), "scroll", move |_| {
            super::code::sync_scroll(&scrolled);
            super::landing::invalidate();
        });
        // Tab keeps its normal focus behaviour; Escape returns focus to the file tabs.
        let keyed = a.clone();
        on(&a.into(), "keydown", move |event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            if keyed.id() == "demo-source" && super::complete::key(&event) {
                return;
            }
            if event.key() == "Escape" {
                let _ = keyed.blur();
                if let Some(tab) = query(".demo .tab[aria-selected='true']")
                    .and_then(|t| t.dyn_into::<HtmlElement>().ok())
                {
                    let _ = tab.focus();
                }
            }
        });
    }
    if let Ok(buttons) = document().query_selector_all("#demo-themes [role=radio]") {
        for i in 0..buttons.length() {
            let Some(button) = buttons
                .item(i)
                .and_then(|b| b.dyn_into::<HtmlElement>().ok())
            else {
                continue;
            };
            let clicked = button.clone();
            on(&button.clone().into(), "click", move |_| {
                if let Some(theme) = clicked.get_attribute("data-theme") {
                    apply_theme(&theme);
                }
            });
            on(&button.into(), "keydown", move |event| {
                let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                    return;
                };
                let names = pills();
                if let Some(next) = arrow_index(&event.key(), i as usize, names.len()) {
                    event.prevent_default();
                    apply_theme(&names[next]);
                    if let Some(target) =
                        query(&format!("#demo-themes [data-theme='{}']", names[next]))
                            .and_then(|b| b.dyn_into::<HtmlElement>().ok())
                    {
                        let _ = target.focus();
                    }
                }
            });
        }
    }
    if let Some(select) = query("#demo-more-themes") {
        let chosen = select.clone().dyn_into::<HtmlSelectElement>()?;
        on(&select.into(), "change", move |_| {
            let value = chosen.value();
            if !value.is_empty() {
                apply_theme(&value);
            } else {
                apply_theme("sleek");
            }
        });
    }
    if let Ok(tabs) = document().query_selector_all(".demo .tab") {
        let count = tabs.length() as usize;
        for i in 0..tabs.length() {
            let Some(tab) = tabs.item(i) else {
                continue;
            };
            on(&tab.clone().into(), "click", move |_| {
                select_tab(i as usize, false)
            });
            on(&tab.into(), "keydown", move |event| {
                let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                    return;
                };
                if let Some(next) = arrow_index(&event.key(), i as usize, count) {
                    event.prevent_default();
                    select_tab(next, true);
                }
            });
        }
    }
    if let Some(reset) = query("#demo-reset") {
        on(&reset.into(), "click", |_| {
            if let (Some(source), Some(style_area)) = (area("#demo-source"), area("#demo-style")) {
                super::writing::hand_over();
                super::complete::hide();
                source.set_value(DEMO);
                style_area.set_value(STYLE);
                DEMO_STATE.with(|s| s.borrow_mut().fallback_theme = None);
                super::code::highlight(&source);
                super::code::highlight(&style_area);
                sync_theme_ui();
                super::toolbar::typed();
                render_now();
                set_status(
                    "live",
                    "Reset to the original example. Rendered live from this source.",
                );
            }
        });
    }
    sync_theme_ui();
    super::toolbar::start();
    start_prose_tools(&source);
    // Keep the exact prerendered first frame. Only an edit, theme change or reset replaces it.
    if edited_before_start {
        super::writing::hand_over();
        schedule();
    }
    for selector in ["#tab-report", "#tab-style", "#demo-reset"] {
        if let Some(control) = query(selector) {
            control.remove_attribute("disabled")?;
        }
    }
    if let Some(pending) = query("#demo-pending") {
        pending.set_attribute("hidden", "")?;
    }
    if let Some(demo) = query("#demo") {
        demo.remove_attribute("aria-busy")?;
    }
    Ok(())
}

/// The words a prompt box holds, read from the box the button sits in.
fn prompt_text(button: &Element) -> Option<String> {
    button
        .closest("[data-prompt]")
        .ok()
        .flatten()?
        .query_selector(".prompt__text code")
        .ok()
        .flatten()?
        .text_content()
}

/// Say on a prompt box whether its words reached the clipboard: the button reads "Copied" for a
/// moment, and the box's status line (a live region) says what to do next.
fn say_copied(button: &Element, copied: bool) {
    let Some(prompt) = button.closest("[data-prompt]").ok().flatten() else {
        return;
    };
    if let Ok(Some(status)) = prompt.query_selector(".prompt__status") {
        status.set_text_content(Some(if copied {
            "Copied. Paste it into your agent."
        } else {
            "Your browser did not copy it. Press Copy again, or select the text."
        }));
    }
    if !copied {
        return;
    }
    button.set_text_content(Some("Copied"));
    let _ = button.set_attribute("data-state", "copied");
    let restore = button.clone();
    let reset = Closure::once_into_js(move || {
        restore.set_text_content(Some("Copy"));
        let _ = restore.remove_attribute("data-state");
    });
    let _ =
        window().set_timeout_with_callback_and_timeout_and_arguments_0(reset.unchecked_ref(), 2000);
}

/// Copy a prompt box's words. The write starts inside the press, so the browser counts it as the
/// press's own.
fn copy_prompt(button: &Element) {
    let Some(text) = prompt_text(button) else {
        return;
    };
    let write = window().navigator().clipboard().write_text(&text);
    let button = button.clone();
    spawn_local(async move {
        say_copied(&button, JsFuture::from(write).await.is_ok());
    });
}

/// Answer the Copy buttons. The site runtime starts on the reader's first intent, and for a reader who
/// came to press Copy the press is that intent: the bootstrap marked the button, and the press is
/// answered here, a moment later, still within the activation the press gave the page.
fn start_prompts() {
    let Ok(buttons) = document().query_selector_all("[data-copy]") else {
        return;
    };
    for i in 0..buttons.length() {
        let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) else {
            continue;
        };
        let pressed = button.has_attribute("data-copy-pressed");
        let _ = button.remove_attribute("data-copy-pressed");
        let target = button.clone();
        on(&button.clone().into(), "click", move |_| {
            copy_prompt(&target)
        });
        if pressed {
            copy_prompt(&button);
        }
    }
}

/// How far below the screen Vee's small clips load, in screens: none loads with the first screen
/// (PRF-36), and each has its bytes before the reader reaches it.
const CLIP_AHEAD: f64 = 1.5;
/// The types of a clip's two files: the HEVC laid on the site's paper first, for Safari, which cannot
/// draw VP9's alpha; then the transparent VP9 for the browsers without HEVC.
const CLIP_SOURCES: [(&str, &str); 2] = [
    ("video/mp4; codecs=\"hvc1.1.6.L63.90\"", ".paper.mp4"),
    ("video/webm; codecs=\"vp9\"", ".webm"),
];

/// A clip the reader has come near: its slot and its video.
struct Clip {
    slot: Element,
    video: Element,
}

/// Vee's small clips (W4): typing by the editor, dipping a brush beside the example company's report
/// and waving at the close. A slot (`.pip-slot[data-clip]`) holds a still of the clip's first frame; as
/// the reader comes within `CLIP_AHEAD` screens it gets its clip, which plays once it is on screen, as
/// often as `data-plays` says, and holds its last frame, so no clip moves for 5 s or more (WCAG 2.2.2).
/// The still shows until the clip's first frame, which is the same picture, so the hand-over never
/// shows. With reduced motion no clip loads, and the slot's picture shows Vee at rest. Once a clip has
/// ended, Vee also blinks over its held frame (see `Blinker`).
fn start_clips() {
    let reduced = window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    let Ok(list) = document().query_selector_all(".pip-slot[data-clip]") else {
        return;
    };
    let mut waiting: Vec<Element> = (0..list.length())
        .filter_map(|i| list.item(i))
        .filter_map(|node| node.dyn_into::<Element>().ok())
        .collect();
    if reduced {
        return;
    }
    start_blinks();
    if waiting.is_empty() {
        return;
    }
    let mut loaded: Vec<Clip> = Vec::new();
    let mut tend = move || {
        let screen = window()
            .inner_height()
            .ok()
            .and_then(|height| height.as_f64())
            .unwrap_or(0.0);
        waiting.retain(|slot| {
            let place = slot.get_bounding_client_rect();
            let near = place.top() < CLIP_AHEAD * screen && place.bottom() > -screen;
            if near && let Some(clip) = make_clip(slot) {
                loaded.push(clip);
            }
            !near
        });
        // A loaded clip starts when most of it is on screen.
        loaded.retain(|clip| {
            let place = clip.slot.get_bounding_client_rect();
            let seen = place.height() * 0.4;
            let shown = place.top() < screen - seen && place.bottom() > seen;
            if shown {
                play(&clip.video);
            }
            !shown
        });
    };
    tend();
    let target: web_sys::EventTarget = window().into();
    on(&target, "scroll", move |_| tend());
}

/// Gives a slot its clip: a muted, inline video of the slot's two files, over the still, playing
/// `data-plays` times when told to and then holding its last frame.
fn make_clip(slot: &Element) -> Option<Clip> {
    let base = slot.get_attribute("data-clip")?;
    let page = document();
    let video = page.create_element("video").ok()?;
    for (name, value) in [
        ("muted", ""),
        ("playsinline", ""),
        ("preload", "auto"),
        ("disablepictureinpicture", ""),
        ("disableremoteplayback", ""),
        ("aria-hidden", "true"),
    ] {
        let _ = video.set_attribute(name, value);
    }
    // The attribute sets the default; the property is what lets a browser start it without a press.
    let _ = js_sys::Reflect::set(&video, &"muted".into(), &true.into());
    for (kind, suffix) in CLIP_SOURCES {
        let source = page.create_element("source").ok()?;
        let _ = source.set_attribute("type", kind);
        let _ = source.set_attribute("src", &format!("{base}{suffix}"));
        let _ = video.append_child(&source);
    }
    let plays = Rc::new(Cell::new(
        slot.get_attribute("data-plays")
            .and_then(|count| count.parse::<u32>().ok())
            .unwrap_or(1)
            .clamp(1, 2),
    ));
    // The video goes in first and Vee's eyes after it, so the eyes lie over the held last frame.
    slot.append_child(&video).ok()?;
    let blinker = eyes_crop(slot).and_then(|crop| add_blinker(slot, crop));
    let shown = slot.clone();
    on(&video.clone().into(), "playing", move |_| {
        let _ = shown.set_attribute("data-shown", "");
    });
    let again = video.clone();
    let left = plays.clone();
    on(&video.clone().into(), "ended", move |_| {
        left.set(left.get().saturating_sub(1));
        if left.get() > 0 {
            let _ = js_sys::Reflect::set(&again, &"currentTime".into(), &0.into());
            play(&again);
        } else if let Some(blinker) = &blinker {
            settle(blinker);
        }
    });
    Some(Clip {
        slot: slot.clone(),
        video,
    })
}

/// From the moment Vee has something to blink for, its routine ending or its coming into view, to the
/// blink: long enough for the eye to have found it.
const BLINK_BEAT_MS: i32 = 400;
/// A reader scrolls Vee in and out of view; it blinks for coming back at most this often.
const BLINK_REST_MS: f64 = 10_000.0;
/// While Vee stays in view it keeps blinking, each blink a random time between these after the last, as a person
/// does. Out of view or in a hidden tab it stops, and starts again when it is back.
const BLINK_IDLE_MS: (f64, f64) = (3_000.0, 6_000.0);

/// The crops in a strip of Vee's eyes: closing, shut, opening, nearly open. The stylesheet steps through them
/// in the same order (`vee-blink` in homepage.css).
const STRIP_CROPS: u32 = 4;

/// Where Vee's eyes are in a placement's frame, and the strip that holds them: the strip is `STRIP_CROPS` crops of
/// the eyes, one beside the other, each `size` pixels, cut from the render the held last frame comes from. `frame`
/// and `at` are in the pixels of that render, so they hold whatever size the frame is shown at.
struct Crop {
    frame: (u32, u32),
    at: (u32, u32),
    size: (u32, u32),
    strip: &'static str,
}

const TYPING_EYES: Crop = Crop {
    frame: (400, 400),
    at: (136, 140),
    size: (94, 50),
    strip: "/landing/vee/typing-eyes.webp",
};
const BRUSH_EYES: Crop = Crop {
    frame: (400, 400),
    at: (149, 119),
    size: (88, 53),
    strip: "/landing/vee/brand-eyes.webp",
};
const WAVE_EYES: Crop = Crop {
    frame: (400, 400),
    at: (150, 128),
    size: (93, 52),
    strip: "/landing/vee/wave-eyes.webp",
};
const HERO_EYES: Crop = Crop {
    frame: (2560, 1440),
    at: (1857, 194),
    size: (202, 115),
    strip: "/landing/hero/pip-eyes.webp",
};
/// The phones' portrait cut of the hero, which has its own frame, its own Vee and so its own eyes.
const HERO_TALL_EYES: Crop = Crop {
    frame: (1170, 1560),
    at: (863, 215),
    size: (158, 98),
    strip: "/landing/hero/pip-portrait-eyes.webp",
};
/// Where the stylesheet switches the hero to its portrait cut.
const HERO_TALL: &str = "(max-width: 699px)";

/// The eyes of the clip in a slot, by the placement the slot is.
fn eyes_crop(slot: &Element) -> Option<&'static Crop> {
    match slot.get_attribute("data-pip")?.as_str() {
        "typing" => Some(&TYPING_EYES),
        "brush" => Some(&BRUSH_EYES),
        "wave" => Some(&WAVE_EYES),
        _ => None,
    }
}

/// One of Vee's four placements, with the layer of eyes (`.vee-eyes`) that blinks over its held last frame.
/// The layer is a crop of the eyes cut from the render the held frame comes from, so it can only be shown once
/// the routine has ended and the clip holds that frame.
struct Blinker {
    eyes: Element,
    /// The routine has ended and its last frame is held.
    settled: Cell<bool>,
    /// The eyes were in view at the last look, so coming into view is news.
    seen: Cell<bool>,
    /// When Vee last blinked, in `now()` milliseconds.
    last: Cell<f64>,
    /// Counts the blinks planned, so a plan the reader scrolled away from can be called off.
    plan: Cell<u32>,
}

thread_local! {static BLINKERS: RefCell<Vec<Rc<Blinker>>> = const { RefCell::new(Vec::new()) };}

/// An SVG element, made with its attributes.
fn svg_element(name: &str, attributes: &[(&str, String)]) -> Option<Element> {
    let element = document()
        .create_element_ns(Some("http://www.w3.org/2000/svg"), name)
        .ok()?;
    for (key, value) in attributes {
        element.set_attribute(key, value).ok()?;
    }
    Some(element)
}

/// Sets the layer to the eyes in `crop`: an SVG in the frame's own pixels, as big as the frame so the eyes
/// land where the clip's eyes do whatever size the frame is shown at, holding the strip in a window of one crop.
fn fit_eyes(layer: &Element, crop: &Crop) -> Option<()> {
    let ((width, height), (x, y), (frame_width, frame_height)) = (crop.size, crop.at, crop.frame);
    layer.set_text_content(None);
    layer
        .set_attribute("viewBox", &format!("0 0 {frame_width} {frame_height}"))
        .ok()?;
    let window = svg_element(
        "svg",
        &[
            ("x", x.to_string()),
            ("y", y.to_string()),
            ("width", width.to_string()),
            ("height", height.to_string()),
            ("viewBox", format!("0 0 {width} {height}")),
        ],
    )?;
    let strip = svg_element(
        "image",
        &[
            ("href", crop.strip.to_string()),
            ("width", (STRIP_CROPS * width).to_string()),
            ("height", height.to_string()),
            ("preserveAspectRatio", "none".to_string()),
        ],
    )?;
    window.append_child(&strip).ok()?;
    layer.append_child(&window).ok()?;
    Some(())
}

/// Lays Vee's eyes over `place`, after the clip it holds, and counts the placement among those that blink.
fn add_blinker(place: &Element, crop: &Crop) -> Option<Rc<Blinker>> {
    let eyes = svg_element(
        "svg",
        &[
            ("class", "vee-eyes".to_string()),
            ("preserveAspectRatio", "none".to_string()),
            ("aria-hidden", "true".to_string()),
            ("focusable", "false".to_string()),
        ],
    )?;
    fit_eyes(&eyes, crop)?;
    let done = eyes.clone();
    on(&eyes.clone().into(), "animationend", move |_| {
        let _ = done.remove_attribute("data-blink");
    });
    place.append_child(&eyes).ok()?;
    let blinker = Rc::new(Blinker {
        eyes,
        settled: Cell::new(false),
        seen: Cell::new(false),
        last: Cell::new(f64::NEG_INFINITY),
        plan: Cell::new(0),
    });
    BLINKERS.with(|list| list.borrow_mut().push(blinker.clone()));
    Some(blinker)
}

/// Whether the reader can see Vee's eyes: the tab is shown and the eyes are inside the screen, a little in
/// from its edges.
fn eyes_in_view(eyes: &Element) -> bool {
    let Some(window_of_eyes) = eyes.first_element_child() else {
        return false;
    };
    if document().hidden() {
        return false;
    }
    let screen = window()
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(0.0);
    let place = window_of_eyes.get_bounding_client_rect();
    place.width() > 0.0 && place.bottom() > 0.08 * screen && place.top() < 0.92 * screen
}

/// Plans a blink after `wait` milliseconds, and after it the next. It happens only if the plan still stands then
/// and Vee is in view; a blink still running then puts it off to the next.
fn blink_in(blinker: &Rc<Blinker>, wait: i32) {
    let plan = blinker.plan.get().wrapping_add(1);
    blinker.plan.set(plan);
    let blinker = blinker.clone();
    let task = Closure::once_into_js(move || {
        if blinker.plan.get() != plan || !eyes_in_view(&blinker.eyes) {
            return;
        }
        if !blinker.eyes.has_attribute("data-blink") {
            // The stylesheet runs the blink and the layer takes the attribute off when it ends.
            let _ = blinker.eyes.set_attribute("data-blink", "");
            blinker.last.set(now());
        }
        idle_next(&blinker);
    });
    let _ =
        window().set_timeout_with_callback_and_timeout_and_arguments_0(task.unchecked_ref(), wait);
}

/// Plans the next idle blink.
fn idle_next(blinker: &Rc<Blinker>) {
    let (soonest, latest) = BLINK_IDLE_MS;
    let wait = soonest + (latest - soonest) * js_sys::Math::random();
    blink_in(blinker, wait as i32);
}

/// Looks at every placement that has ended its routine. One that has just come into view blinks, unless it
/// blinked a moment ago; one that has gone out of view drops its plan.
fn tend_blinks() {
    BLINKERS.with(|list| {
        for blinker in list.borrow().iter() {
            let in_view = blinker.settled.get() && eyes_in_view(&blinker.eyes);
            if in_view && !blinker.seen.get() {
                blinker.seen.set(true);
                if now() - blinker.last.get() >= BLINK_REST_MS {
                    blink_in(blinker, BLINK_BEAT_MS);
                } else {
                    idle_next(blinker);
                }
            } else if !in_view && blinker.seen.get() {
                blinker.seen.set(false);
                blinker.plan.set(blinker.plan.get().wrapping_add(1));
            }
        }
    });
}

/// A placement's routine has ended, and its last frame is held. If Vee is in view, that is a moment to blink for.
fn settle(blinker: &Rc<Blinker>) {
    blinker.settled.set(true);
    tend_blinks();
}

/// Vee's blink (the hero and the three small clips), not with reduced motion: the first screen's clip is
/// native, so its end is read from the video, which may have ended before the runtime started.
fn start_blinks() {
    let tall = window().match_media(HERO_TALL).ok().flatten();
    let hero_crop = move || {
        if tall.as_ref().is_some_and(|query| query.matches()) {
            &HERO_TALL_EYES
        } else {
            &HERO_EYES
        }
    };
    if let (Some(frame), Some(video)) = (query(".hero__frame"), query(".hero__clip"))
        && let Some(blinker) = add_blinker(&frame, hero_crop())
    {
        // A phone turned on its side, or a window resized across the break, changes the cut.
        if let Ok(Some(query)) = window().match_media(HERO_TALL) {
            let layer = blinker.eyes.clone();
            on(&query.into(), "change", move |_| {
                let _ = fit_eyes(&layer, hero_crop());
            });
        }
        let ended = js_sys::Reflect::get(&video, &"ended".into())
            .ok()
            .and_then(|state| state.as_bool())
            .unwrap_or(false);
        if ended {
            settle(&blinker);
        } else {
            on(&video.into(), "ended", move |_| settle(&blinker));
        }
    }
    let target: web_sys::EventTarget = window().into();
    on(&target, "scroll", |_| tend_blinks());
    let page: web_sys::EventTarget = document().into();
    on(&page, "visibilitychange", |_| tend_blinks());
}

/// Starts a video. A browser that refuses (Low Power Mode, data saving) leaves the still in place.
fn play(video: &Element) {
    if let Ok(start) = js_sys::Reflect::get(video, &"play".into())
        && let Ok(start) = start.dyn_into::<js_sys::Function>()
        && let Ok(started) = start.call0(video)
        && let Ok(promise) = started.dyn_into::<js_sys::Promise>()
    {
        spawn_local(async move {
            let _ = JsFuture::from(promise).await;
        });
    }
}

async fn submit_interest(email: String, consent: bool, website: String) -> Result<String, JsValue> {
    let request = web_sys::RequestInit::new();
    request.set_method("POST");
    let headers = web_sys::Headers::new()?;
    headers.set("Content-Type", "application/json")?;
    request.set_headers(&headers);
    request.set_body(&JsValue::from_str(
        &serde_json::json!({"email":email,"consent":consent,"website":website}).to_string(),
    ));
    let controller = web_sys::AbortController::new()?;
    request.set_signal(Some(&controller.signal()));
    let timed_controller = controller.clone();
    let timeout = Closure::<dyn FnMut()>::new(move || timed_controller.abort());
    let timer = window().set_timeout_with_callback_and_timeout_and_arguments_0(
        timeout.as_ref().unchecked_ref(),
        12000,
    )?;
    let result = async {
        let response = JsFuture::from(window().fetch_with_str_and_init("/api/interest", &request))
            .await?.dyn_into::<web_sys::Response>()?;
        let body = JsFuture::from(response.text()?).await?.as_string().unwrap_or_default();
        let payload: serde_json::Value = serde_json::from_str(&body).map_err(|_| {
            js_error("We could not confirm signup. Please try again later.")
        })?;
        if response.ok() && payload["ok"] == true {
            Ok("You’re on the list. Your email and consent have been saved. No email has been sent.".into())
        } else {
            Err(js_error(payload["message"].as_str().unwrap_or(
                "We could not confirm your email was saved. Please try again later.",
            )))
        }
    }.await;
    window().clear_timeout_with_handle(timer);
    result
}

fn signup_field(id: &str) -> Option<HtmlInputElement> {
    query(id).and_then(|element| element.dyn_into::<HtmlInputElement>().ok())
}

/// The form asks for an email only: the line above its button says what the reader signs up for, and
/// pressing it is that consent (Daniel, 9 Nov).
fn signup_valid() -> bool {
    signup_field("#interest-email")
        .is_some_and(|email| !email.value().trim().is_empty() && email.check_validity())
}

fn sync_signup_button(pending: bool) {
    if let Some(button) = query("#interest-form button") {
        if pending || !signup_valid() {
            button.set_attribute("disabled", "").ok();
        } else {
            button.remove_attribute("disabled").ok();
        }
    }
}

fn start_signup() {
    let Some(form) = query("#interest-form") else {
        return;
    };
    let pending = Rc::new(Cell::new(false));
    for event in ["input", "change"] {
        let state = pending.clone();
        on(&form.clone().into(), event, move |_| {
            sync_signup_button(state.get())
        });
    }
    // The HTTP endpoint accepts JSON only. Never navigate to a success page on a static host.
    on(&form.into(), "submit", move |event| {
        event.prevent_default();
        if pending.get() || !signup_valid() {
            return;
        }
        let email = signup_field("#interest-email")
            .map(|input| input.value())
            .unwrap_or_default();
        let consent = true;
        let website = signup_field("#interest-website")
            .map(|input| input.value())
            .unwrap_or_default();
        pending.set(true);
        sync_signup_button(true);
        if let Some(status) = query("#interest-status") {
            status.set_text_content(Some("Saving your email…"));
        }
        let state = pending.clone();
        spawn_local(async move {
            let result = submit_interest(email, consent, website).await;
            if let Some(status) = query("#interest-status") {
                let message = match result {
                    Ok(message) => message,
                    Err(error) => error.as_string().unwrap_or_else(|| {
                        "We could not confirm your email was saved. Please try again later.".into()
                    }),
                };
                status.set_text_content(Some(&message));
            }
            state.set(false);
            sync_signup_button(false);
        });
    });
    sync_signup_button(false);
}

/// Whether the report in a landing frame, a same-origin page the build renders, has started its
/// runtime, and whether it has also given every chart its canvas.
pub(crate) fn report_state(frame: &HtmlIFrameElement) -> (bool, bool) {
    let Some(report) = frame.content_document() else {
        return (false, false);
    };
    let started = report
        .document_element()
        .and_then(|root| root.get_attribute("data-rv-runtime"))
        .is_some_and(|state| state == "started");
    let count = |selector: &str| {
        report
            .query_selector_all(selector)
            .map_or(0, |list| list.length())
    };
    let charts = count("figure[data-rv-chart]");
    (
        started,
        started && charts > 0 && count("figure[data-rv-chart] canvas") >= charts,
    )
}

pub(crate) fn start() -> Result<(), JsValue> {
    if query("body[data-richview-page='landing']").is_none() {
        return Ok(());
    }
    // The press that started the runtime is answered first, while its activation lasts.
    start_prompts();
    start_clips();
    if let Err(error) = start_demo() {
        if let Some(demo) = query("#demo") {
            let _ = demo.remove_attribute("aria-busy");
            let _ = demo.set_attribute("data-engine-failed", "");
        }
        return Err(error);
    }
    start_signup();
    if let Some(root) = document().document_element() {
        root.set_attribute("data-homepage-engine", "rust")?;
    }
    Ok(())
}
