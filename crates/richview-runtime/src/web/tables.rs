//! Sortable report tables (FEA-RP-22). A header button cycles its column through ascending,
//! descending and back to the author's order, announced through `aria-sort`. Rows reorder in
//! place, so focus stays on the header and an opened overflow stays open. Numbers sort by the
//! value each cell shows, which core puts on each numeric cell of its typed tables at the
//! precision the cell displays (FEA-CH-37), compared as decimal text; other text sorts by the
//! page's language, and empty cells stay last in both directions. Equal cells, including numbers
//! that show the same value, keep the author's order. A table cut at `max_rows` sorts every row
//! and keeps the first ones in view.

use super::{document, on, query_all, window};
use richview_chart::data::compare_decimal_text;
use std::cmp::Ordering;
use wasm_bindgen::JsCast;
use web_sys::Element;

/// A cell's sort key: empty, a number (as decimal text) or text.
#[derive(Debug, Clone, PartialEq)]
enum Key {
    Empty,
    Number(String),
    Text(String),
}

fn key(cell: Option<Element>) -> Key {
    let Some(cell) = cell else {
        return Key::Empty;
    };
    // Core marks every numeric cell with the value it shows; in its typed tables any other cell
    // is text, whatever it looks like (`"2"` from JSON, `45%` as written).
    if let Some(value) = cell.get_attribute("data-rv-sort-value") {
        return Key::Number(value.trim().to_owned());
    }
    let typed = cell
        .closest("table[data-rv-typed]")
        .ok()
        .flatten()
        .is_some();
    let raw = cell.text_content().unwrap_or_default();
    let raw = raw.trim();
    if raw.is_empty() || raw == "–" || raw == "—" || richview_chart::rows::is_missing_text(raw)
    {
        return Key::Empty;
    }
    if typed {
        return Key::Text(raw.to_owned());
    }
    // Grouped figures (9,007,199,254,740,993 or 9 007 199,5) read as the number they show, in
    // the page's language.
    let bare: String = raw
        .chars()
        .filter(|c| !matches!(c, '\u{202f}' | '\u{a0}' | ' '))
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect();
    let number = |t: &str| compare_decimal_text(t, t).is_some();
    if number(&bare) {
        return Key::Number(bare);
    }
    let comma = decimal_comma();
    let localized: String = bare
        .chars()
        .filter(|c| *c != if comma { '.' } else { ',' })
        .map(|c| if comma && c == ',' { '.' } else { c })
        .collect();
    if number(&localized) {
        Key::Number(localized)
    } else {
        Key::Text(raw.to_owned())
    }
}

/// The page's language, `en` when it names none.
fn lang() -> String {
    let lang = document()
        .document_element()
        .map(|e| e.get_attribute("lang").unwrap_or_default())
        .unwrap_or_default();
    if lang.is_empty() {
        "en".to_owned()
    } else {
        lang
    }
}

/// The page's language as `localeCompare`'s locales argument, so text sorts by the report's
/// language and not the browser's (cs: `ch` after `h`).
fn page_locale() -> js_sys::Array {
    js_sys::Array::of1(&lang().into())
}

/// Whether the page's language writes decimals with a comma (cs: 1,5).
fn decimal_comma() -> bool {
    let lang = lang();
    js_sys::Number::from(1.5)
        .to_locale_string(&lang)
        .as_string()
        .is_some_and(|s| s.contains(','))
}

/// Ascending order of two keys: numbers, then text, then empty cells.
fn compare(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Number(x), Key::Number(y)) => compare_decimal_text(x, y).unwrap_or(Ordering::Equal),
        (Key::Text(x), Key::Text(y)) => js_sys::JsString::from(x.as_str())
            .locale_compare(y, &page_locale(), &js_sys::Object::new())
            .cmp(&0),
        (Key::Number(_), _) | (Key::Text(_), Key::Empty) => Ordering::Less,
        (_, Key::Number(_)) | (Key::Empty, Key::Text(_)) => Ordering::Greater,
        (Key::Empty, Key::Empty) => Ordering::Equal,
    }
}

/// The order a column moves to next: ascending, descending, then the author's.
fn next(current: Option<&str>) -> Option<bool> {
    match current {
        Some("ascending") => Some(true),
        Some("descending") => None,
        _ => Some(false),
    }
}

/// The static HTML has plain headers, so no button is dead before the runtime starts. Once it
/// runs, each header of a table's main head becomes a sort button (FEA-RP-22).
fn upgrade_headers() {
    for th in query_all(".rv-table th[data-rv-key]") {
        if th.query_selector("button").ok().flatten().is_some() {
            continue;
        }
        let Ok(button) = document().create_element("button") else {
            continue;
        };
        let label = th.text_content().unwrap_or_default();
        let _ = button.set_attribute("type", "button");
        let _ = button.set_attribute(
            "data-rv-sort",
            &th.get_attribute("data-rv-key").unwrap_or_default(),
        );
        let _ = button.set_attribute("aria-label", &format!("Sort by {}", label.trim()));
        while let Some(child) = th.first_child() {
            let _ = button.append_child(&child);
        }
        let _ = th.append_child(&button);
        if !th.has_attribute("aria-sort") {
            let _ = th.set_attribute("aria-sort", "none");
        }
    }
}

/// Folded rows, transcripts and every other disclosure print in full: open each for the print and close the
/// ones that were closed afterwards.
fn open_for_print() {
    const FOLDS: &str = ".rv-report details";
    let target: web_sys::EventTarget = window().into();
    on(&target, "beforeprint", |_| {
        for details in query_all(FOLDS) {
            if !details.has_attribute("open") {
                let _ = details.set_attribute("open", "");
                let _ = details.set_attribute("data-rv-print-opened", "");
            }
        }
    });
    on(&target, "afterprint", |_| {
        for details in query_all("details[data-rv-print-opened]") {
            let _ = details.remove_attribute("open");
            let _ = details.remove_attribute("data-rv-print-opened");
        }
    });
}

/// "Table" in the page's language, the name of a scroll box that overflows.
fn table_name() -> &'static str {
    match lang().split('-').next().unwrap_or("en") {
        "cs" | "sk" => "Tabulka",
        "de" => "Tabelle",
        "fr" => "Tableau",
        "es" => "Tabla",
        _ => "Table",
    }
}

/// "Equation" in the page's language, the name of a display formula that overflows.
fn equation_name() -> &'static str {
    match lang().split('-').next().unwrap_or("en") {
        "cs" => "Rovnice",
        "sk" => "Rovnica",
        "de" => "Gleichung",
        "fr" => "Équation",
        "es" => "Ecuación",
        _ => "Equation",
    }
}

/// "Code" in the page's language, the name of a code block that overflows.
fn code_name() -> &'static str {
    match lang().split('-').next().unwrap_or("en") {
        "cs" | "sk" => "Kód",
        "de" => "Code",
        "fr" => "Code",
        "es" => "Código",
        _ => "Code",
    }
}

/// A scroll box that overflows becomes a keyboard-reachable named region; one that fits is plain
/// markup, so it adds no tab stop (WCAG 2.1.1, 4.1.2). Tables, display formulas and code blocks
/// scroll; a table's region takes its caption as its name, as a chart's Table does ("Data for"
/// its title), and a table box shows on each side that has more that it scrolls.
fn mark_scrollers() {
    for (selector, name) in [
        (
            ".rv-table-scroll, div.rv-table, .rv-chart__table",
            table_name(),
        ),
        (".rv-report__body .rv-math", equation_name()),
        (".rv-report__body pre", code_name()),
    ] {
        for node in super::query_all(selector) {
            mark_scroller(&node, name);
        }
    }
}

/// A chart's Table, once it is on the page or rebuilt: a named region while it scrolls, with its
/// edge cues (S27-879).
pub fn mark_table(node: &Element) {
    mark_scroller(node, table_name());
    watch(node);
}

fn mark_scroller(node: &Element, name: &str) {
    let table = node
        .matches(".rv-table-scroll, div.rv-table, .rv-chart__table")
        .unwrap_or(false);
    // A chart's Table scrolls down as well as across (24rem high).
    let overflows = node.scroll_width() > node.client_width() + 1
        || (node.class_list().contains("rv-chart__table")
            && node.scroll_height() > node.client_height() + 1);
    let name = if table {
        node.query_selector("caption")
            .ok()
            .flatten()
            .and_then(|c| c.text_content())
            .map(|t| t.trim().to_owned())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| name.to_owned())
    } else {
        name.to_owned()
    };
    if overflows {
        let _ = node.set_attribute("tabindex", "0");
        let _ = node.set_attribute("role", "region");
        if !node.has_attribute("aria-label") {
            let _ = node.set_attribute("aria-label", &name);
        }
    } else {
        let _ = node.remove_attribute("tabindex");
        let _ = node.remove_attribute("role");
        if node.get_attribute("aria-label").as_deref() == Some(name.as_str()) {
            let _ = node.remove_attribute("aria-label");
        }
    }
    if table {
        cue(node);
    }
}

/// A table box whose content is wider than it shows so (S27-879): on each side that has more, an
/// edge cue fades the content into the surface the box sits on, and leaves when the reader reaches
/// that end, so a value the box cuts is never read as whole. The box is marked while it is too
/// narrow (`data-rv-cues`, which makes it a grid whose cues stick to its sides) and with the sides
/// that have more (`data-rv-more`), kept as the reader scrolls. What stands with the table (a
/// chart Table's status line, Show all) stays in place while only the columns scroll, as wide as
/// the box at most (`--rv-box-w`).
fn cue(node: &Element) {
    if node.scroll_width() <= node.client_width() + 1 {
        let _ = node.remove_attribute("data-rv-cues");
        let _ = node.remove_attribute("data-rv-more");
        return;
    }
    if !node.has_attribute("data-rv-cues") {
        let _ = node.set_attribute("data-rv-cues", "");
    }
    if let Some(html) = node.dyn_ref::<web_sys::HtmlElement>() {
        // The cue fades into the surface the box sits on: the page, a chart's card, the dialog.
        if let Some(surface) = surface(node) {
            let _ = html.style().set_property("--rv-cue-surface", &surface);
        }
        let width = format!("{}px", node.client_width());
        let _ = html.style().set_property("--rv-box-w", &width);
    }
    if !node.has_attribute("data-rv-cue-wired") {
        let _ = node.set_attribute("data-rv-cue-wired", "");
        let box_ = node.clone();
        let target: web_sys::EventTarget = node.clone().into();
        on(&target, "scroll", move |_| sides(&box_));
    }
    sides(node);
}

/// The colour of the nearest surface under `node`, its own background included.
fn surface(node: &Element) -> Option<String> {
    let mut at = Some(node.clone());
    while let Some(e) = at {
        let colour = window()
            .get_computed_style(&e)
            .ok()
            .flatten()
            .and_then(|s| s.get_property_value("background-color").ok())
            .unwrap_or_default();
        let clear = colour.is_empty()
            || colour == "transparent"
            || colour.ends_with(", 0)")
            || colour.ends_with("/ 0)");
        if !clear {
            return Some(colour);
        }
        at = e.parent_element();
    }
    None
}

/// Mark the sides of a cued box that have more: left and right as the reader sees them, in either
/// writing direction.
fn sides(node: &Element) {
    if !node.has_attribute("data-rv-cues") {
        return;
    }
    let max = f64::from(node.scroll_width() - node.client_width());
    // Every engine counts a right-to-left box's scroll from 0 at its start down to -max.
    let at = f64::from(node.scroll_left()).abs();
    let rtl = window()
        .get_computed_style(node)
        .ok()
        .flatten()
        .and_then(|s| s.get_property_value("direction").ok())
        .is_some_and(|d| d == "rtl");
    let (behind, ahead) = (at > 1.0, at < max - 1.0);
    let (left, right) = if rtl {
        (ahead, behind)
    } else {
        (behind, ahead)
    };
    let more = match (left, right) {
        (true, true) => "left right",
        (true, false) => "left",
        (false, true) => "right",
        (false, false) => "",
    };
    if node.get_attribute("data-rv-more").as_deref() != Some(more) {
        let _ = node.set_attribute("data-rv-more", more);
    }
}

thread_local! {
    /// Watches table boxes and their tables: when one grows or shrinks (its fonts arrive, a fold
    /// opens, the window narrows, a dialog opens), the boxes are marked again on the next frame.
    static WATCHER: std::cell::RefCell<Option<web_sys::ResizeObserver>> = const { std::cell::RefCell::new(None) };
    static MARKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Watch a table box and its tables.
fn watch(node: &Element) {
    WATCHER.with(|w| {
        let mut w = w.borrow_mut();
        if w.is_none() {
            let callback = wasm_bindgen::closure::Closure::<dyn FnMut(js_sys::Array)>::new(
                |_: js_sys::Array| {
                    // Mark on the next frame, not while the observer delivers its notes.
                    if MARKING.with(|m| m.replace(true)) {
                        return;
                    }
                    let next = wasm_bindgen::closure::Closure::once_into_js(|| {
                        MARKING.with(|m| m.set(false));
                        mark_scrollers();
                    });
                    let _ = window().request_animation_frame(next.unchecked_ref());
                },
            );
            *w = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()).ok();
            callback.forget();
        }
        if let Some(observer) = w.as_ref()
            && let Ok(tables) = node.query_selector_all("table")
        {
            observer.observe(node);
            for i in 0..tables.length() {
                if let Some(t) = tables.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                    observer.observe(&t);
                }
            }
        }
    });
}

pub fn start() {
    mark_scrollers();
    for node in query_all(".rv-table-scroll, div.rv-table") {
        watch(&node);
    }
    on(&super::window().into(), "resize", |_| mark_scrollers());
    upgrade_headers();
    open_for_print();
    let target: web_sys::EventTarget = document().into();
    on(&target, "click", |event| {
        let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        let Ok(Some(button)) = target.closest("[data-rv-sort]") else {
            return;
        };
        let Ok(Some(th)) = button.closest("th") else {
            return;
        };
        let Ok(Some(table)) = th.closest("table") else {
            return;
        };
        // The block's main table and, past `max_rows`, the rows under "Show remaining rows":
        // a sort orders all of them, and the first rows stay in the main table.
        let wrap = table.closest(".rv-table").ok().flatten();
        let main = wrap
            .as_ref()
            .and_then(|w| w.query_selector(":scope > table").ok().flatten())
            .unwrap_or_else(|| table.clone());
        let Ok(Some(body)) = main.query_selector("tbody") else {
            return;
        };
        let rest = wrap
            .as_ref()
            .and_then(|w| w.query_selector(".rv-table__overflow tbody").ok().flatten());
        let Some(row) = th.parent_element() else {
            return;
        };
        let column = (0..row.children().length())
            .position(|i| row.children().item(i).as_ref() == Some(&th))
            .unwrap_or(0);
        // `Some(descending)` sorts; `None` returns to the author's order.
        let order = next(th.get_attribute("aria-sort").as_deref());
        let label = match order {
            Some(false) => Some("ascending"),
            Some(true) => Some("descending"),
            None => None,
        };
        let heads = wrap
            .as_ref()
            .and_then(|w| w.query_selector_all("thead tr").ok());
        if let Some(heads) = heads {
            for i in 0..heads.length() {
                let Some(head) = heads.item(i).and_then(|n| n.dyn_into::<Element>().ok()) else {
                    continue;
                };
                for k in 0..head.children().length() {
                    if let Some(h) = head.children().item(k) {
                        // The author's sort state, to come back to.
                        if !h.has_attribute("data-rv-author-sort") {
                            let initial = h
                                .get_attribute("aria-sort")
                                .unwrap_or_else(|| "none".into());
                            let _ = h.set_attribute("data-rv-author-sort", &initial);
                        }
                        let state = match label {
                            Some(label) if k as usize == column => label.to_owned(),
                            Some(_) => "none".to_owned(),
                            None => h
                                .get_attribute("data-rv-author-sort")
                                .unwrap_or_else(|| "none".into()),
                        };
                        let _ = h.set_attribute("aria-sort", &state);
                    }
                }
            }
        } else if let Some(label) = label {
            let _ = th.set_attribute("aria-sort", label);
        } else {
            let _ = th.remove_attribute("aria-sort");
        }
        let mut rows: Vec<(Element, Key, usize)> = Vec::new();
        let shown = body.children().length() as usize;
        for part in std::iter::once(&body).chain(rest.as_ref()) {
            let children = part.children();
            for i in 0..children.length() {
                if let Some(tr) = children.item(i) {
                    // The author's position, recorded the first time the table sorts.
                    let n = rows.len();
                    let author = match tr
                        .get_attribute("data-rv-order")
                        .and_then(|o| o.parse().ok())
                    {
                        Some(o) => o,
                        None => {
                            let _ = tr.set_attribute("data-rv-order", &n.to_string());
                            n
                        }
                    };
                    let cell = tr.children().item(column as u32);
                    rows.push((tr, key(cell), author));
                }
            }
        }
        rows.sort_by(|a, b| {
            let by_value = match order {
                None => Ordering::Equal,
                Some(descending) => match (&a.1, &b.1) {
                    // Empty cells stay last in both directions.
                    (Key::Empty, Key::Empty) => Ordering::Equal,
                    (Key::Empty, _) => Ordering::Greater,
                    (_, Key::Empty) => Ordering::Less,
                    (x, y) if descending => compare(y, x),
                    (x, y) => compare(x, y),
                },
            };
            by_value.then(a.2.cmp(&b.2))
        });
        for (k, (tr, ..)) in rows.into_iter().enumerate() {
            let into = match &rest {
                Some(rest) if k >= shown => rest,
                _ => &body,
            };
            let _ = into.append_child(&tr);
        }
        // Some browsers blur a clicked button before activation. Keep the sort header
        // focused on every activation, as on keyboard sorting, without moving the page.
        if let Some(button) = button.dyn_ref::<web_sys::HtmlElement>() {
            let options = web_sys::FocusOptions::new();
            options.set_prevent_scroll(true);
            let _ = button.focus_with_options(&options);
        }
    });
}
