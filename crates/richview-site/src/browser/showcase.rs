//! The flagship page (FEA-SF-36 to FEA-SF-38, FEA-SF-63, STY-43, STY-60).
//!
//! The build renders the report once per style; this module swaps the frame to the chosen render,
//! keeps the reading position, follows the style with the page frame, compares any two styles side
//! by side and opens citation cards on source links. Every key does something a visible control
//! also does.
use super::*;
use std::collections::BTreeMap;
use web_sys::{EventTarget, KeyboardEvent};

struct BoundListener {
    target: EventTarget,
    event: &'static str,
    callback: EventHandler,
}
impl Drop for BoundListener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback(
            self.event,
            self.callback.as_ref().unchecked_ref(),
        );
    }
}

/// Where the reader is in a report: the block, how far into it and how far below the top.
#[derive(Clone, Default)]
struct Anchor {
    index: usize,
    fraction: f64,
    gap: f64,
}

struct Showcase {
    /// The frame tokens of every style, keyed by id: the five builtins and the extended themes.
    tokens: Value,
    sources: BTreeMap<String, BTreeMap<String, String>>,
    /// The five builtin styles, in the order of the pills.
    pills: Vec<String>,
    theme: String,
    compare: Option<String>,
    /// The style each frame (primary, comparison) currently shows.
    shown: [String; 2],
    listeners: [Vec<BoundListener>; 2],
    revision: [u64; 2],
    cite_timer: Option<i32>,
    cite_callback: Option<Closure<dyn FnMut()>>,
    citation_b: Option<bool>,
    syncing: bool,
}
type State = Rc<RefCell<Showcase>>;

fn slot(b: bool) -> usize {
    usize::from(b)
}
fn known(state: &Showcase, id: &str) -> bool {
    state.tokens.get(id).is_some()
}
fn label(state: &Showcase, id: &str) -> String {
    state.tokens[id]["name"].as_str().unwrap_or(id).to_owned()
}
fn frame(b: bool) -> Option<HtmlIFrameElement> {
    query(if b { "#compare-frame" } else { "#report-frame" }).and_then(|e| e.dyn_into().ok())
}
fn frame_document(b: bool) -> Option<Document> {
    frame(b)?.content_document()
}
fn text(selector: &str, value: &str) {
    if let Some(e) = query(selector) {
        e.set_text_content(Some(value));
    }
}
fn shown(selector: &str, show: bool) {
    if let Some(e) = query(selector) {
        if show {
            let _ = e.remove_attribute("hidden");
        } else {
            let _ = e.set_attribute("hidden", "");
        }
    }
}
// A report lives in another DOM realm: its Element and KeyboardEvent constructors are different
// from the page's. Event targets are checked by nodeType instead.
fn event_element(target: EventTarget) -> Option<Element> {
    (js_sys::Reflect::get(&target, &"nodeType".into())
        .ok()?
        .as_f64()
        == Some(1.0))
    .then(|| target.unchecked_into())
}
fn reduced() -> bool {
    window()
        .match_media("(prefers-reduced-motion: reduce)")
        .ok()
        .flatten()
        .map(|m| m.matches())
        .unwrap_or(true)
}
fn announce(value: &str) {
    text("#showcase-live", value);
}
fn after(milliseconds: i32, action: impl FnOnce() + 'static) -> Option<i32> {
    let callback = Closure::once_into_js(action);
    window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.unchecked_ref(),
            milliseconds,
        )
        .ok()
}
fn escape(value: &str) -> String {
    richview_chart::svg::escape(value)
}
fn params() -> web_sys::UrlSearchParams {
    web_sys::UrlSearchParams::new_with_str(&window().location().search().unwrap_or_default())
        .unwrap_or_else(|_| web_sys::UrlSearchParams::new().unwrap())
}
fn read_url(state: &State) {
    let q = params();
    let mut state = state.borrow_mut();
    let default = state.pills.first().cloned().unwrap_or_default();
    state.theme = q
        .get("theme")
        .filter(|v| known(&state, v))
        .unwrap_or(default);
    state.compare = q
        .get("compare")
        .filter(|v| known(&state, v) && *v != state.theme);
}
/// The shareable address of the current view: only what differs from the default is written.
fn update_url(state: &State) {
    let Ok(url) = web_sys::Url::new(&window().location().href().unwrap_or_default()) else {
        return;
    };
    url.set_search("");
    url.set_hash("");
    let q = url.search_params();
    {
        let s = state.borrow();
        if s.pills.first() != Some(&s.theme) {
            q.set("theme", &s.theme);
        }
        if let Some(compare) = &s.compare {
            q.set("compare", compare);
        }
    }
    if let Ok(history) = js_sys::Reflect::get(&window(), &"history".into())
        && let Ok(method) = js_sys::Reflect::get(&history, &"replaceState".into())
        && let Ok(method) = method.dyn_into::<js_sys::Function>()
    {
        let _ = method.call3(&history, &JsValue::NULL, &"".into(), &url.href().into());
    }
}
fn bind(
    state: &State,
    b: bool,
    target: EventTarget,
    event: &'static str,
    handler: impl FnMut(Event) + 'static,
) {
    let callback = Closure::wrap(Box::new(handler) as Box<dyn FnMut(Event)>);
    let _ = target.add_event_listener_with_callback(event, callback.as_ref().unchecked_ref());
    state.borrow_mut().listeners[slot(b)].push(BoundListener {
        target,
        event,
        callback,
    });
}

// ---------------------------------------------------------------- reading position

/// The blocks a reading position is kept against, in document order. A `.rv-keep` box holds a
/// chart with its source note and has no box of its own on screen (`display: contents`), so its
/// children stand in for it.
const ANCHORS: &str =
    ".rv-report__header, .rv-report__body > :not(.rv-keep), .rv-report__body > .rv-keep > *";

fn anchor(doc: &Document) -> Anchor {
    let Ok(nodes) = doc.query_selector_all(ANCHORS) else {
        return Anchor::default();
    };
    for i in 0..nodes.length() {
        if let Some(node) = nodes.item(i).map(|n| n.unchecked_into::<Element>()) {
            let r = node.get_bounding_client_rect();
            if r.bottom() > 0.0 {
                return Anchor {
                    index: i as usize,
                    fraction: (-r.top() / r.height().max(1.0)).clamp(0.0, 1.0),
                    gap: r.top().max(0.0),
                };
            }
        }
    }
    Anchor::default()
}
fn restore(doc: &Document, position: &Anchor) -> Option<f64> {
    let node = doc
        .query_selector_all(ANCHORS)
        .ok()?
        .item(position.index as u32)?
        .unchecked_into::<Element>();
    let win = doc.default_view()?;
    let r = node.get_bounding_client_rect();
    let y = win.scroll_y().unwrap_or(0.0) + r.top() + r.height() * position.fraction - position.gap;
    let max = doc
        .document_element()
        .map(|root| (root.scroll_height() - root.client_height()).max(0) as f64)
        .unwrap_or(0.0);
    Some(y.clamp(0.0, max))
}

// ---------------------------------------------------------------- the frame follows the style

/// The page's own tokens take the active style's, so header, rail and footer read as one piece
/// with the report (STY-60). The build writes the same mapping into the page for the first paint.
fn follow_style(state: &Showcase) {
    let tokens = &state.tokens[&state.theme];
    let Some(body) = document().body() else {
        return;
    };
    let body: Element = body.into();
    let value = |key: &str| tokens[key].as_str().unwrap_or("").to_owned();
    for (property, key) in [
        ("--paper", "bg"),
        ("--panel", "bg-elev"),
        ("--ink", "ink"),
        ("--soft", "ink-soft"),
        ("--rule", "rule"),
        ("--blue", "accent"),
        ("--blue-ink", "accent"),
        ("--font-mono", "font-mono"),
    ] {
        style(&body, property, &value(key));
    }
    style(
        &body,
        "--font-ui",
        &quote_font(&value("font-text"), "font-text"),
    );
    style(&body, "color-scheme", &value("mode"));
}

fn sync_controls(state: &Showcase) {
    let extended = !state.pills.contains(&state.theme);
    if let Ok(buttons) = document().query_selector_all("#style-pills [role=radio]") {
        for i in 0..buttons.length() {
            let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) else {
                continue;
            };
            let id = button.get_attribute("data-theme").unwrap_or_default();
            let checked = id == state.theme;
            let _ = button.set_attribute("aria-checked", if checked { "true" } else { "false" });
            let stop = checked || (extended && i == 0);
            let _ = button.set_attribute("tabindex", if stop { "0" } else { "-1" });
        }
    }
    for (selector, value) in [
        (
            "#theme",
            if extended {
                state.theme.clone()
            } else {
                String::new()
            },
        ),
        ("#compare-theme", state.compare.clone().unwrap_or_default()),
    ] {
        if let Some(select) = query(selector).and_then(|e| e.dyn_into::<HtmlSelectElement>().ok()) {
            select.set_value(&value);
        }
    }
    if let Some(button) = query("#compare-toggle") {
        let _ = button.set_attribute(
            "aria-pressed",
            if state.compare.is_some() {
                "true"
            } else {
                "false"
            },
        );
    }
    if let Some(panes) = query("#showcase-panes") {
        let _ = panes.set_attribute(
            "data-compare",
            if state.compare.is_some() {
                "true"
            } else {
                "false"
            },
        );
    }
    text("#pane-a-name", &label(state, &state.theme));
    shown("#compare-pane", state.compare.is_some());
    if let Some(root) = document().document_element() {
        let _ = root.set_attribute("data-rv-theme", &state.theme);
    }
}

/// Bring both frames and every control in line with the state.
fn apply(state: &State) {
    {
        let s = state.borrow();
        follow_style(&s);
        sync_controls(&s);
    }
    let (primary, comparison) = {
        let s = state.borrow();
        (s.theme.clone(), s.compare.clone())
    };
    if state.borrow().shown[0] != primary {
        load_style(state, false, &primary);
    }
    match comparison {
        Some(id) => {
            if state.borrow().shown[1] != id {
                load_style(state, true, &id);
            }
        }
        None => {
            if let Some(old) = frame(true) {
                old.remove();
            }
            let mut s = state.borrow_mut();
            s.shown[1].clear();
            s.listeners[1].clear();
        }
    }
}

/// Show the render of style `id` in a frame: the new render loads on top of the old one and
/// fades in over 200 ms, the reading position carried across. Reduced motion swaps at once.
fn load_style(state: &State, b: bool, id: &str) {
    let host = query(if b { "#frame-host-b" } else { "#frame-host-a" });
    let Some(host) = host else { return };
    let Ok(created) = document().create_element("iframe") else {
        return;
    };
    let Ok(new) = created.dyn_into::<HtmlIFrameElement>() else {
        return;
    };
    let old = frame(b);
    let position = old
        .as_ref()
        .and_then(|f| f.content_document())
        .map(|doc| anchor(&doc));
    let animate = old.is_some() && !reduced();
    let revision = {
        let mut s = state.borrow_mut();
        s.shown[slot(b)] = id.to_owned();
        s.revision[slot(b)] = s.revision[slot(b)].wrapping_add(1);
        s.revision[slot(b)]
    };
    new.set_title(&format!("Sourced report in {}", label(&state.borrow(), id)));
    new.set_src(&format!("Rendered/{id}.html"));
    if animate {
        let _ = new.set_attribute("data-entering", "");
    }
    let weak = Rc::downgrade(state);
    let loading = new.clone();
    on(&new.clone().into(), "load", move |_| {
        let Some(state) = weak.upgrade() else { return };
        if state.borrow().revision[slot(b)] != revision {
            loading.remove();
            return;
        }
        let Some(doc) = loading.content_document() else {
            return;
        };
        let fonts = loading_fonts(&loading);
        let state = state.clone();
        let loading = loading.clone();
        let old = old.clone();
        let position = position.clone();
        let reveal = move || {
            if let (Some(position), Some(win)) = (position.as_ref(), doc.default_view())
                && let Some(y) = restore(&doc, position)
            {
                win.scroll_to_with_x_and_y(0.0, y);
            }
            let _ = loading.remove_attribute("data-entering");
            let finish = {
                let loading = loading.clone();
                let state = state.clone();
                move || {
                    if let Some(old) = old {
                        old.remove();
                    }
                    loading.set_id(if b { "compare-frame" } else { "report-frame" });
                    bind_frame(&state, b);
                }
            };
            if animate {
                after(220, finish);
            } else {
                finish();
            }
        };
        spawn_local(async move {
            if let Some(ready) = fonts {
                let _ = JsFuture::from(ready).await;
            }
            reveal();
        });
    });
    let _ = host.append_child(&new);
}
fn loading_fonts(frame: &HtmlIFrameElement) -> Option<js_sys::Promise> {
    frame.content_document()?.fonts().ready().ok()
}

// ---------------------------------------------------------------- compare

fn set_theme(state: &State, id: &str) {
    {
        let mut s = state.borrow_mut();
        if !known(&s, id) || s.theme == id {
            return;
        }
        s.theme = id.to_owned();
        if s.compare.as_deref() == Some(id) {
            s.compare = None;
        }
    }
    apply(state);
    update_url(state);
    announce(&format!("{} style", label(&state.borrow(), id)));
}
fn set_compare(state: &State, id: Option<String>) {
    {
        let mut s = state.borrow_mut();
        s.compare = id.filter(|id| known(&s, id) && *id != s.theme);
    }
    apply(state);
    update_url(state);
}
fn toggle_compare(state: &State) {
    let next = {
        let s = state.borrow();
        if s.compare.is_some() {
            None
        } else {
            // The style after the current one among the pills.
            let at = s.pills.iter().position(|p| *p == s.theme).unwrap_or(0);
            s.pills.get((at + 1) % s.pills.len().max(1)).cloned()
        }
    };
    set_compare(state, next);
    announce(if state.borrow().compare.is_some() {
        "Comparing two styles"
    } else {
        "Comparison closed"
    });
}
/// Keep the two reports at the same relative reading position.
fn sync_scroll(state: &State, from_b: bool) {
    if state.borrow().compare.is_none() || state.borrow().syncing {
        return;
    }
    let (Some(source), Some(target)) = (frame_document(from_b), frame_document(!from_b)) else {
        return;
    };
    let range = |doc: &Document| {
        doc.document_element()
            .map(|root| (root.scroll_height() - root.client_height()).max(1) as f64)
            .unwrap_or(1.0)
    };
    let (Some(from), Some(to)) = (source.default_view(), target.default_view()) else {
        return;
    };
    let ratio = from.scroll_y().unwrap_or(0.0) / range(&source);
    state.borrow_mut().syncing = true;
    to.scroll_to_with_x_and_y(0.0, ratio * range(&target));
    let weak = Rc::downgrade(state);
    after(60, move || {
        if let Some(state) = weak.upgrade() {
            state.borrow_mut().syncing = false;
        }
    });
}

// ---------------------------------------------------------------- keys and controls

fn cycle(state: &State, step: isize) {
    let next = {
        let s = state.borrow();
        let count = s.pills.len() as isize;
        if count == 0 {
            return;
        }
        let at = s
            .pills
            .iter()
            .position(|p| *p == s.theme)
            .map_or(0, |i| i as isize);
        s.pills[(at + step).rem_euclid(count) as usize].clone()
    };
    set_theme(state, &next);
    if let Some(button) = query(&format!("#style-pills [data-theme='{next}']"))
        .and_then(|b| b.dyn_into::<HtmlElement>().ok())
    {
        let _ = button.focus();
    }
}
fn keyboard(state: &State, event: Event) {
    let key = event.unchecked_into::<KeyboardEvent>();
    if key.default_prevented() || key.ctrl_key() || key.meta_key() || key.alt_key() {
        return;
    }
    if let Some(target) = key.target().and_then(event_element)
        && matches!(target.tag_name().as_str(), "SELECT" | "INPUT" | "TEXTAREA")
    {
        return;
    }
    match key.key().as_str() {
        "ArrowRight" | "ArrowDown"
            if key
                .target()
                .and_then(event_element)
                .is_some_and(|t| t.closest("#style-pills").ok().flatten().is_some()) =>
        {
            key.prevent_default();
            cycle(state, 1);
        }
        "ArrowLeft" | "ArrowUp"
            if key
                .target()
                .and_then(event_element)
                .is_some_and(|t| t.closest("#style-pills").ok().flatten().is_some()) =>
        {
            key.prevent_default();
            cycle(state, -1);
        }
        "ArrowRight" => {
            key.prevent_default();
            cycle(state, 1);
        }
        "ArrowLeft" => {
            key.prevent_default();
            cycle(state, -1);
        }
        "c" | "C" => {
            key.prevent_default();
            toggle_compare(state);
        }
        "Escape" => shown("#citation-card", false),
        _ => {}
    }
}
fn wire_controls(state: &State) {
    if let Ok(buttons) = document().query_selector_all("#style-pills [role=radio]") {
        for i in 0..buttons.length() {
            let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) else {
                continue;
            };
            let id = button.get_attribute("data-theme").unwrap_or_default();
            let st = state.clone();
            on(&button.into(), "click", move |_| set_theme(&st, &id));
        }
    }
    for (selector, compare) in [("#theme", false), ("#compare-theme", true)] {
        if let Some(select) = query(selector).and_then(|e| e.dyn_into::<HtmlSelectElement>().ok()) {
            let st = state.clone();
            let chosen = select.clone();
            on(&select.into(), "change", move |_| {
                let value = chosen.value();
                if compare {
                    set_compare(&st, (!value.is_empty()).then_some(value));
                } else if !value.is_empty() {
                    set_theme(&st, &value);
                }
            });
        }
    }
    if let Some(button) = query("#compare-toggle") {
        let st = state.clone();
        on(&button.into(), "click", move |_| toggle_compare(&st));
    }
    let st = state.clone();
    on(&document().into(), "keydown", move |event| {
        keyboard(&st, event)
    });
}

// ---------------------------------------------------------------- citation cards

fn focused_citation() -> Option<(Element, bool)> {
    let b = match document().active_element()?.id().as_str() {
        "report-frame" => false,
        "compare-frame" => true,
        _ => return None,
    };
    let link = frame_document(b)?
        .active_element()?
        .closest("[data-source-id]")
        .ok()??;
    Some((link, b))
}
fn clear_cite_timer(state: &State) {
    if let Some(id) = state.borrow_mut().cite_timer.take() {
        window().clear_timeout_with_handle(id);
    }
}
fn hide_cite_later(state: &State) {
    clear_cite_timer(state);
    if focused_citation().is_some() {
        return;
    }
    let id = {
        let s = state.borrow();
        s.cite_callback.as_ref().and_then(|c| {
            window()
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    c.as_ref().unchecked_ref(),
                    180,
                )
                .ok()
        })
    };
    state.borrow_mut().cite_timer = id;
}
fn show_citation(state: &State, link: &Element, b: bool) {
    clear_cite_timer(state);
    let Some(id) = link.get_attribute("data-source-id") else {
        return;
    };
    let source = state.borrow().sources.get(&id).cloned();
    let Some(source) = source else { return };
    state.borrow_mut().citation_b = Some(b);
    let Some(card) = query("#citation-card") else {
        return;
    };
    let value = |key: &str| source.get(key).map(String::as_str).unwrap_or("");
    let date = if value("published_date").is_empty() {
        value("accessed_date")
    } else {
        value("published_date")
    };
    let _ = link.set_attribute(
        "aria-description",
        &format!(
            "{}; {}; {}; {} {}",
            value("title"),
            value("publisher"),
            date,
            value("quote"),
            value("notes")
        ),
    );
    let quote = if value("quote").is_empty() {
        String::new()
    } else {
        format!("<blockquote>{}</blockquote>", escape(value("quote")))
    };
    let notes = if value("notes").is_empty() {
        String::new()
    } else {
        format!("<p>{}</p>", escape(value("notes")))
    };
    let href = value("url");
    let open = if href.starts_with("https://") || href.starts_with("http://") {
        format!(
            "<p><a href=\"{}\" target=\"_blank\" rel=\"noopener\">Open the source</a></p>",
            escape(href)
        )
    } else {
        String::new()
    };
    card.set_inner_html(&format!(
        "<p class=\"citation-kind\">{} · {}</p><p class=\"citation-title\">{}</p><p>{} · {}</p>{quote}{notes}{open}",
        escape(&id),
        escape(value("type")),
        escape(value("title")),
        escape(value("publisher")),
        escape(date),
    ));
    let _ = card.remove_attribute("hidden");
    let outer = frame(b).map(|e| e.get_bounding_client_rect());
    let rect = link.get_bounding_client_rect();
    let width = window()
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1280.0);
    let height = window()
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(720.0);
    let x = rect.left() + outer.as_ref().map(|r| r.left()).unwrap_or(0.0);
    let y = rect.bottom() + outer.as_ref().map(|r| r.top()).unwrap_or(0.0) + 8.0;
    let card_width = 340.0_f64.min(width - 24.0);
    style(&card, "width", &format!("{card_width}px"));
    style(
        &card,
        "left",
        &format!("{}px", x.clamp(12.0, (width - card_width - 12.0).max(12.0))),
    );
    let card_height = card.get_bounding_client_rect().height();
    style(
        &card,
        "top",
        &format!("{}px", y.min(height - card_height - 12.0).max(12.0)),
    );
}

/// Wire a freshly loaded report frame: source links open citation cards (and nothing outside the
/// flagship does), external links leave in a new tab, scrolling keeps a comparison in step and
/// the page's keys work from inside the frame.
fn bind_frame(state: &State, b: bool) {
    state.borrow_mut().listeners[slot(b)].clear();
    let Some(doc) = frame_document(b) else { return };
    if let Ok(links) = doc.query_selector_all("a[href]") {
        for i in 0..links.length() {
            let Some(link) = links.item(i).map(|n| n.unchecked_into::<Element>()) else {
                continue;
            };
            let id = link.text_content().unwrap_or_default().trim().to_owned();
            if !id.is_empty()
                && id.len() < 16
                && (id.starts_with("D-")
                    || id.chars().next().is_some_and(|c| c.is_ascii_uppercase())
                        && id.chars().any(|c| c.is_ascii_digit()))
            {
                let _ = link.set_attribute("data-source-id", &id);
                let _ = link.set_attribute(
                    "aria-label",
                    &format!("Source {id}; focus for its dated citation details"),
                );
            }
            if link
                .get_attribute("href")
                .is_some_and(|v| v.starts_with("https://") || v.starts_with("http://"))
            {
                let _ = link.set_attribute("target", "_blank");
                let _ = link.set_attribute("rel", "noopener");
            }
        }
    }
    if let Some(win) = doc.default_view() {
        let st = state.clone();
        bind(state, b, win.clone().into(), "scroll", move |_| {
            if st.borrow().citation_b == Some(b)
                && query("#citation-card").is_some_and(|card| !card.has_attribute("hidden"))
            {
                let focused = frame_document(b)
                    .and_then(|doc| doc.active_element())
                    .filter(|link| link.has_attribute("data-source-id"));
                if let Some(link) = focused {
                    show_citation(&st, &link, b);
                } else {
                    shown("#citation-card", false);
                }
            }
            sync_scroll(&st, b);
        });
        let st = state.clone();
        bind(state, b, win.into(), "keydown", move |event| {
            keyboard(&st, event)
        });
    }
    for event in ["pointerover", "focusin"] {
        let st = state.clone();
        let kind = event;
        bind(state, b, doc.clone().into(), event, move |event| {
            // Layout or scroll can put a different citation under a stationary pointer.
            // Keyboard focus owns the card until that focus leaves the report link.
            if kind == "pointerover"
                && let Some((link, focused_b)) = focused_citation()
            {
                show_citation(&st, &link, focused_b);
            } else if let Some(link) = event
                .target()
                .and_then(event_element)
                .and_then(|e| e.closest("[data-source-id]").ok().flatten())
            {
                show_citation(&st, &link, b);
            }
        });
    }
    for event in ["pointerout", "focusout"] {
        let st = state.clone();
        bind(state, b, doc.clone().into(), event, move |event| {
            if event
                .target()
                .and_then(event_element)
                .and_then(|e| e.closest("[data-source-id]").ok().flatten())
                .is_some()
            {
                hide_cite_later(&st);
            }
        });
    }
}

/// Quoted commas, escaped quotes and newlines are part of the source ledger contract.
fn source_rows(csv: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut records = vec![];
    let mut record = vec![];
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = csv.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                record.push(std::mem::take(&mut cell));
            }
            '\n' | '\r' if !quoted => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut record));
            }
            _ => cell.push(c),
        }
    }
    if !cell.is_empty() || !record.is_empty() {
        record.push(cell);
        records.push(record);
    }
    let Some(header) = records.first() else {
        return BTreeMap::new();
    };
    records
        .iter()
        .skip(1)
        .filter_map(|record| {
            let row: BTreeMap<_, _> = header.iter().cloned().zip(record.iter().cloned()).collect();
            Some((row.get("id")?.clone(), row))
        })
        .collect()
}

pub(super) fn start() {
    spawn_local(async {
        let result = async {
            let tokens: Value = query("#showcase-tokens")
                .and_then(|e| serde_json::from_str(&e.text_content().unwrap_or_default()).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            let pills: Vec<String> = document()
                .query_selector_all("#style-pills [role=radio]")
                .map(|buttons| {
                    (0..buttons.length())
                        .filter_map(|i| buttons.item(i))
                        .filter_map(|b| b.dyn_into::<Element>().ok())
                        .filter_map(|b| b.get_attribute("data-theme"))
                        .collect()
                })
                .unwrap_or_default();
            let first = pills.first().cloned().unwrap_or_default();
            let state = Rc::new(RefCell::new(Showcase {
                tokens,
                sources: BTreeMap::new(),
                pills,
                theme: first.clone(),
                compare: None,
                shown: [first, String::new()],
                listeners: [vec![], vec![]],
                revision: [0, 0],
                cite_timer: None,
                cite_callback: None,
                citation_b: None,
                syncing: false,
            }));
            read_url(&state);
            wire_controls(&state);
            let weak = Rc::downgrade(&state);
            let callback = Closure::wrap(Box::new(move || {
                if weak.upgrade().is_some() {
                    shown("#citation-card", false);
                }
            }) as Box<dyn FnMut()>);
            state.borrow_mut().cite_callback = Some(callback);
            // The page ships showing the first style; a shared address may name another.
            let first_frame = frame(false);
            if let Some(frame) = &first_frame {
                let st = state.clone();
                on(&frame.clone().into(), "load", move |_| {
                    bind_frame(&st, false)
                });
                if frame
                    .content_document()
                    .is_some_and(|d| d.ready_state() == "complete")
                {
                    bind_frame(&state, false);
                }
            }
            apply(&state);
            let ledger = fetch_text("ledger/sources.csv").await?;
            state.borrow_mut().sources = source_rows(&ledger);
            text(
                "#source-count",
                &format!("{} dated ledger entries", state.borrow().sources.len()),
            );
            Ok::<(), JsValue>(())
        }
        .await;
        if let Err(e) = result {
            error(&e);
            announce(
                "The report remains readable. Presentation controls could not finish loading.",
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_rows_keep_quoted_commas_quotes_and_newlines() {
        let csv = "id,title,quote\nD-1,\"Plain, with comma\",\"He said \"\"yes\"\"\nand left\"\nS065,Second,\n";
        let rows = source_rows(csv);
        assert_eq!(rows["D-1"]["title"], "Plain, with comma");
        assert_eq!(rows["D-1"]["quote"], "He said \"yes\"\nand left");
        assert_eq!(rows["S065"]["title"], "Second");
    }
}
