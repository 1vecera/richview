//! Sortable report tables (FEA-RP-22). A header button cycles its column through ascending,
//! descending and back to the author's order, announced through `aria-sort`. Rows reorder in
//! place, so focus stays on the header and an opened overflow stays open. Numbers sort by the
//! value each cell shows, which core puts on each numeric cell of its typed tables at the
//! precision the cell displays (FEA-CH-37), compared as decimal text; other text sorts by the
//! page's language, and empty cells stay last in both directions. Equal cells, including numbers
//! that show the same value, keep the author's order. A table cut at `max_rows` sorts every row
//! and keeps the first ones in view.
//!
//! A table marked for it gets a filter box (FEA-RP-70): a search field above its rows that keeps
//! the rows whose shown text holds every typed word, whatever the case or accents, and says how
//! many it kept. The static page has no field, so no control is dead before the runtime starts.
//! Sorting and filtering arrange the rows through one function, so each keeps the other.

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

/// The verdict on a table block's short labels, `line` or `wrap`.
const LABELS: &str = "data-rv-labels";

/// Every table block that holds a short label (`td.label`, such as `Store 40`).
fn label_tables() -> Vec<Element> {
    query_all("div.rv-table")
        .into_iter()
        .filter(|t| t.query_selector("td.label").ok().flatten().is_some())
        .collect()
}

/// Decide whether each table block's short labels stand on one line (FEA-RP-19). One line when
/// the table then fits its box and is no taller than with them wrapped, or when it scrolls
/// sideways either way and is no taller. Wrapped at their words otherwise: when that is what
/// makes the table fit, so a table that fits never starts to scroll sideways for its labels,
/// and when whole labels would squeeze another column onto more lines than they save. The
/// verdict is `data-rv-labels` on the block.
///
/// The tables are tried together: each is measured as it stands, all are set the other way
/// and measured again, and only those whose verdict is the first way are set back. So a pass
/// lays the page out twice however many tables it measures.
fn fit_labels(tables: &[Element]) {
    /// A block's height and whether its table is wider than it.
    fn measure(t: &Element) -> (f64, bool) {
        (
            t.get_bounding_client_rect().height(),
            t.scroll_width() > t.client_width() + 1,
        )
    }
    let tables: Vec<&Element> = tables
        .iter()
        .filter(|t| t.is_connected() && t.query_selector("td.label").ok().flatten().is_some())
        .collect();
    // A table the runtime has not judged starts on one line.
    for t in &tables {
        if !matches!(t.get_attribute(LABELS).as_deref(), Some("line" | "wrap")) {
            let _ = t.set_attribute(LABELS, "line");
        }
    }
    let first: Vec<(bool, (f64, bool))> = tables
        .iter()
        .map(|t| {
            (
                t.get_attribute(LABELS).as_deref() == Some("line"),
                measure(t),
            )
        })
        .collect();
    for (t, (line, _)) in tables.iter().zip(&first) {
        let _ = t.set_attribute(LABELS, if *line { "wrap" } else { "line" });
    }
    let second: Vec<(f64, bool)> = tables.iter().map(|t| measure(t)).collect();
    for ((t, (was_line, one)), other) in tables.iter().zip(first).zip(second) {
        let ((line_height, line_over), (wrap_height, wrap_over)) =
            if was_line { (one, other) } else { (other, one) };
        let line = line_height <= wrap_height + 0.5 && (!line_over || wrap_over);
        // The table stands the other way now: set it back when the verdict is the first way.
        if line == was_line {
            let _ = t.set_attribute(LABELS, if line { "line" } else { "wrap" });
        }
    }
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
        node.query_selector("caption, .rv-table__title")
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
    /// opens, the window narrows, a dialog opens), its labels and every box's marks are measured
    /// again on the next frame.
    static WATCHER: std::cell::RefCell<Option<web_sys::ResizeObserver>> = const { std::cell::RefCell::new(None) };
    /// What the next frame measures again.
    static PENDING: std::cell::RefCell<Pending> = const {
        std::cell::RefCell::new(Pending { scheduled: false, all: false, tables: Vec::new() })
    };
}

/// The label tables the next frame measures again: all of them, or the ones named.
struct Pending {
    scheduled: bool,
    all: bool,
    tables: Vec<Element>,
}

/// Which tables' labels a change can have moved.
enum Changed<'a> {
    /// The window, the fonts: any table.
    Any,
    /// One table block: its rows, its headings or its box.
    Table(&'a Element),
    /// A box that holds no label table: only the scroll marks follow.
    Marks,
}

/// Measure again on the next frame: the labels of the tables a change can have moved, then
/// every scroll box's marks. Window resizes, the size observer's notes, fonts that finish
/// loading and a filter or sort that changes a table's rows all ask here, and one frame answers
/// however many asked, so a burst of resize events costs one pass.
fn refit(changed: Changed) {
    let first = PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        match changed {
            Changed::Any => pending.all = true,
            Changed::Table(table) => {
                if !pending.all && !pending.tables.iter().any(|t| t == table) {
                    pending.tables.push(table.clone());
                }
            }
            Changed::Marks => {}
        }
        !std::mem::replace(&mut pending.scheduled, true)
    });
    if !first {
        return;
    }
    let next = wasm_bindgen::closure::Closure::once_into_js(|| {
        let (all, tables) = PENDING.with(|pending| {
            let mut pending = pending.borrow_mut();
            pending.scheduled = false;
            (
                std::mem::take(&mut pending.all),
                std::mem::take(&mut pending.tables),
            )
        });
        if all {
            fit_labels(&label_tables());
        } else {
            fit_labels(&tables);
        }
        mark_scrollers();
    });
    let _ = window().request_animation_frame(next.unchecked_ref());
}

/// Watch a table box and its tables.
fn watch(node: &Element) {
    WATCHER.with(|w| {
        let mut w = w.borrow_mut();
        if w.is_none() {
            let callback = wasm_bindgen::closure::Closure::<dyn FnMut(js_sys::Array)>::new(
                |entries: js_sys::Array| {
                    // Measure on the next frame, not while the observer delivers its notes: the
                    // table blocks whose box or table changed size, and every box's marks.
                    for entry in entries.iter() {
                        let block = js_sys::Reflect::get(&entry, &"target".into())
                            .ok()
                            .and_then(|target| target.dyn_into::<Element>().ok())
                            .and_then(|target| target.closest("div.rv-table").ok().flatten());
                        match block {
                            Some(block) => refit(Changed::Table(&block)),
                            None => refit(Changed::Marks),
                        }
                    }
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

/// The filter box's words in the page's language: its label, "{n} of {m} rows", the line when
/// no row matches, and the note that a total row still sums every row.
struct Words {
    label: &'static str,
    count: fn(usize, usize) -> String,
    none: &'static str,
    total: fn(usize) -> String,
}

fn words() -> Words {
    match lang().split('-').next().unwrap_or("en") {
        "cs" => Words {
            label: "Filtrovat řádky",
            count: |n, m| format!("{n} z {m} řádků"),
            none: "Žádný řádek neodpovídá",
            total: |m| format!("součet je ze všech {m}"),
        },
        "sk" => Words {
            label: "Filtrovať riadky",
            count: |n, m| format!("{n} z {m} riadkov"),
            none: "Žiadny riadok nezodpovedá",
            total: |m| format!("súčet je zo všetkých {m}"),
        },
        "de" => Words {
            label: "Zeilen filtern",
            count: |n, m| format!("{n} von {m} Zeilen"),
            none: "Keine Zeile passt",
            total: |m| format!("die Summe gilt für alle {m}"),
        },
        "fr" => Words {
            label: "Filtrer les lignes",
            count: |n, m| format!("{n} sur {m} lignes"),
            none: "Aucune ligne ne correspond",
            total: |m| format!("le total porte sur les {m}"),
        },
        "es" => Words {
            label: "Filtrar filas",
            count: |n, m| format!("{n} de {m} filas"),
            none: "Ninguna fila coincide",
            total: |m| format!("el total es de las {m}"),
        },
        _ => Words {
            label: "Filter rows",
            count: |n, m| format!("{n} of {m} rows"),
            none: "No rows match",
            total: |m| format!("the total is of all {m}"),
        },
    }
}

/// Text as the filter compares it: lower case, without accents, so `cafe` finds `Café`.
fn fold(text: &str) -> String {
    let decomposed = String::from(js_sys::JsString::from(text).normalize("NFD"));
    decomposed
        .chars()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// A row's text for the filter: what its cells show, one per line, folded.
fn row_text(tr: &Element) -> String {
    let cells = tr.children();
    (0..cells.length())
        .filter_map(|i| cells.item(i))
        .map(|cell| fold(cell.text_content().unwrap_or_default().trim()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One row of a table block, kept so that a keystroke reads nothing back from the page.
struct Row {
    tr: Element,
    /// The row's text as the filter compares it, folded the first time a filter needs it.
    text: Option<String>,
    hidden: bool,
}

/// A table block's rows in the author's order, and where they stand now.
struct Rows {
    main: Element,
    rows: Vec<Row>,
    /// The places of the main table: the rows it held before any moved.
    places: usize,
    /// The reader's sort that `order` was computed for, and the rows in that order.
    sorted: Option<(u32, bool)>,
    order: Vec<usize>,
    /// The rows the main table and the folded part hold now, in order.
    body: Vec<usize>,
    rest: Vec<usize>,
}

thread_local! {
    /// The table blocks arranged so far. A sort reads its column once; typing in the filter box
    /// then reads nothing and writes only what changed, so a long table keeps up with the keys.
    static TABLES: std::cell::RefCell<Vec<Rows>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A table block's rows as the page holds them before the first sort or filter.
fn read_rows(main: &Element, body: &Element, rest: Option<&Element>) -> Rows {
    let mut rows = Vec::new();
    let mut places = 0;
    for (part, in_main) in std::iter::once((body, true)).chain(rest.map(|r| (r, false))) {
        let mut next = part.first_element_child();
        while let Some(tr) = next {
            next = tr.next_element_sibling();
            // A table read again while it is filtered keeps what the page shows.
            let hidden = tr.has_attribute("hidden");
            rows.push(Row {
                tr,
                text: None,
                hidden,
            });
            if in_main {
                places += 1;
            }
        }
    }
    let count = rows.len();
    Rows {
        main: main.clone(),
        rows,
        places,
        sorted: None,
        order: (0..count).collect(),
        body: (0..places).collect(),
        rest: (places..count).collect(),
    }
}

/// Make `order` the children of `parent`, in one call where the browser has `replaceChildren`.
fn place(parent: &Element, rows: &[Row], order: &[usize]) {
    let nodes = js_sys::Array::new_with_length(order.len() as u32);
    for (k, &i) in order.iter().enumerate() {
        nodes.set(k as u32, rows[i].tr.clone().into());
    }
    let placed = js_sys::Reflect::get(parent, &"replaceChildren".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .is_some_and(|f| f.apply(parent, &nodes).is_ok());
    if !placed {
        // Every row belongs to one of the two parts, so appending in order leaves each part
        // with its rows once both are placed.
        for &i in order {
            let _ = parent.append_child(&rows[i].tr);
        }
    }
}

/// Arrange a table block's rows by the reader's sort (`data-rv-sorted` on the main table) and
/// its filter box. The rows that match stand in order: in a table with a fold they fill the main
/// table's places and then the folded part, with the others hidden after them; in a table
/// without one every row keeps its sorted place and the others hide there. Hidden rows leave the
/// tab order and the accessibility tree. The status line and the fold's summary say what is
/// shown.
fn arrange(main: &Element) {
    let wrap = main.closest(".rv-table").ok().flatten();
    let Ok(Some(body)) = main.query_selector("tbody") else {
        return;
    };
    let rest = wrap
        .as_ref()
        .and_then(|w| w.query_selector(".rv-table__overflow tbody").ok().flatten());
    let want = main.get_attribute("data-rv-sorted").and_then(|state| {
        let (column, direction) = state.split_once(':')?;
        Some((column.parse::<u32>().ok()?, direction == "desc"))
    });
    let input = wrap.as_ref().and_then(|w| {
        w.query_selector(".rv-table__filter input")
            .ok()
            .flatten()
            .and_then(|i| i.dyn_into::<web_sys::HtmlInputElement>().ok())
    });
    let typed = input.as_ref().map(|i| fold(&i.value())).unwrap_or_default();
    let needles: Vec<&str> = typed.split_whitespace().collect();
    let (all, places, matching) = TABLES.with(|tables| {
        let mut tables = tables.borrow_mut();
        // A table the page no longer holds (a re-render replaced it) is forgotten.
        tables.retain(|t| t.main.is_connected());
        let at = match tables.iter().position(|t| t.main == *main) {
            Some(at) => at,
            None => {
                tables.push(read_rows(main, &body, rest.as_ref()));
                tables.len() - 1
            }
        };
        let table = &mut tables[at];
        if table.sorted != want {
            table.order = (0..table.rows.len()).collect();
            if let Some((column, descending)) = want {
                let keys: Vec<Key> = table
                    .rows
                    .iter()
                    .map(|row| key(row.tr.children().item(column)))
                    .collect();
                table.order.sort_by(|&a, &b| {
                    let by_value = match (&keys[a], &keys[b]) {
                        // Empty cells stay last in both directions.
                        (Key::Empty, Key::Empty) => Ordering::Equal,
                        (Key::Empty, _) => Ordering::Greater,
                        (_, Key::Empty) => Ordering::Less,
                        (x, y) if descending => compare(y, x),
                        (x, y) => compare(x, y),
                    };
                    // Equal cells keep the author's order.
                    by_value.then(a.cmp(&b))
                });
            }
            table.sorted = want;
        }
        let (mut kept, mut dropped) = (Vec::new(), Vec::new());
        for &i in &table.order {
            let row = &mut table.rows[i];
            let keep = needles.is_empty() || {
                let text = row.text.get_or_insert_with(|| row_text(&row.tr));
                needles.iter().all(|needle| text.contains(needle))
            };
            if row.hidden == keep {
                if keep {
                    let _ = row.tr.remove_attribute("hidden");
                } else {
                    let _ = row.tr.set_attribute("hidden", "");
                }
                row.hidden = !keep;
            }
            (if keep { &mut kept } else { &mut dropped }).push(i);
        }
        let matching = kept.len();
        let (body_now, rest_now) = if rest.is_some() {
            let split = matching.min(table.places);
            let folded: Vec<usize> = kept[split..].iter().chain(&dropped).copied().collect();
            kept.truncate(split);
            (kept, folded)
        } else {
            (table.order.clone(), Vec::new())
        };
        if body_now != table.body {
            place(&body, &table.rows, &body_now);
            table.body = body_now;
        }
        if let Some(rest) = &rest
            && rest_now != table.rest
        {
            place(rest, &table.rows, &rest_now);
            table.rest = rest_now;
        }
        (table.rows.len(), table.places, matching)
    });
    let Some(wrap) = wrap else {
        return;
    };
    // Other rows or headings in the same box can fit differently: measure the labels again.
    refit(Changed::Table(&wrap));
    let filtering = !needles.is_empty();
    let words = words();
    // The fold says how many of the matching rows are in view, and leaves when all are.
    if let Ok(Some(overflow)) = wrap.query_selector(".rv-table__overflow") {
        if let Ok(Some(summary)) = overflow.query_selector("summary") {
            let author = match summary.get_attribute("data-rv-summary") {
                Some(text) => text,
                None => {
                    let text = summary.text_content().unwrap_or_default();
                    let _ = summary.set_attribute("data-rv-summary", &text);
                    text
                }
            };
            // A fold that leaves (every matching row is in view) keeps its own line.
            let text = if !filtering || matching <= places {
                author
            } else if lang().starts_with("en") {
                format!("Showing {places} of {matching} matching rows")
            } else {
                (words.count)(places, matching)
            };
            if summary.text_content().unwrap_or_default() != text {
                summary.set_text_content(Some(&text));
            }
        }
        if filtering && matching <= places {
            let _ = overflow.set_attribute("hidden", "");
        } else {
            let _ = overflow.remove_attribute("hidden");
        }
    }
    if let Ok(Some(status)) = wrap.query_selector(".rv-table__status") {
        let kept = if matching == 0 {
            words.none.to_owned()
        } else {
            (words.count)(matching, all)
        };
        let total = wrap
            .query_selector("tfoot, .rv-table__total")
            .ok()
            .flatten()
            .is_some();
        // A total row stays the total of every row, also when no row matches.
        let text = if !filtering {
            String::new()
        } else if total {
            format!("{kept} · {}", (words.total)(all))
        } else {
            kept
        };
        if status.text_content().unwrap_or_default() != text {
            status.set_text_content(Some(&text));
        }
    }
    // Paper shows the box only while it filters.
    if let Ok(Some(line)) = wrap.query_selector(".rv-table__filter") {
        if filtering {
            let _ = line.set_attribute("data-rv-on", "");
        } else {
            let _ = line.remove_attribute("data-rv-on");
        }
    }
}

/// Give each table marked for it (`data-rv-filter`) its filter box (FEA-RP-70): a labelled search
/// field and a status line that screen readers hear politely, above the rows and under the
/// table's title. The title leaves the caption for a paragraph that still names the table, so the
/// box stands between the title and the headings.
fn add_filters() {
    let words = words();
    for (n, wrap) in query_all("div.rv-table[data-rv-filter]")
        .into_iter()
        .enumerate()
    {
        if wrap.get_attribute("data-rv-filter").as_deref() == Some("ready") {
            continue;
        }
        let Ok(Some(main)) = wrap.query_selector(":scope > table") else {
            continue;
        };
        let doc = document();
        let (Ok(line), Ok(field), Ok(label), Ok(input), Ok(status)) = (
            doc.create_element("p"),
            doc.create_element("span"),
            doc.create_element("label"),
            doc.create_element("input"),
            doc.create_element("span"),
        ) else {
            continue;
        };
        if let Ok(Some(caption)) = main.query_selector(":scope > caption")
            && let Ok(title) = doc.create_element("p")
        {
            let id = format!("rv-table-title-{n}");
            let _ = title.set_attribute("id", &id);
            let _ = title.set_attribute("class", "rv-table__title");
            while let Some(child) = caption.first_child() {
                let _ = title.append_child(&child);
            }
            let _ = wrap.insert_before(&title, Some(&main));
            caption.remove();
            let _ = main.set_attribute("aria-labelledby", &id);
        }
        let id = format!("rv-table-filter-{n}");
        let _ = line.set_attribute("class", "rv-table__filter");
        let _ = label.set_attribute("for", &id);
        label.set_text_content(Some(words.label));
        let _ = input.set_attribute("id", &id);
        let _ = input.set_attribute("type", "search");
        let _ = input.set_attribute("autocomplete", "off");
        let _ = input.set_attribute("spellcheck", "false");
        let _ = input.set_attribute("enterkeyhint", "search");
        let _ = status.set_attribute("class", "rv-table__status");
        let _ = status.set_attribute("role", "status");
        let _ = status.set_attribute("aria-live", "polite");
        // The label and its field are one item of the line: in a narrow table the field
        // shrinks beside its label, so the empty box keeps to the one line its room holds.
        let _ = field.set_attribute("class", "rv-table__field");
        let _ = field.append_child(&label);
        let _ = field.append_child(&input);
        let _ = line.append_child(&field);
        let _ = line.append_child(&status);
        let _ = wrap.insert_before(&line, Some(&main));
        let _ = wrap.set_attribute("data-rv-filter", "ready");
    }
    let target: web_sys::EventTarget = document().into();
    let filter_of = |event: &web_sys::Event| {
        let input = event.target()?.dyn_into::<Element>().ok()?;
        if !input.matches(".rv-table__filter input").unwrap_or(false) {
            return None;
        }
        let main = input
            .closest(".rv-table")
            .ok()
            .flatten()?
            .query_selector(":scope > table")
            .ok()
            .flatten()?;
        Some((input, main))
    };
    on(&target, "input", move |event| {
        if let Some((_, main)) = filter_of(&event) {
            arrange(&main);
        }
    });
    // Escape empties the box, as the field's own clear button does.
    on(&target, "keydown", move |event| {
        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
            return;
        };
        if key.key() != "Escape" {
            return;
        }
        if let Some((input, main)) = filter_of(&event)
            && let Some(input) = input.dyn_ref::<web_sys::HtmlInputElement>()
            && !input.value().is_empty()
        {
            input.set_value("");
            event.prevent_default();
            arrange(&main);
        }
    });
}

pub fn start() {
    fit_labels(&label_tables());
    mark_scrollers();
    for node in query_all(".rv-table-scroll, div.rv-table") {
        watch(&node);
    }
    on(&super::window().into(), "resize", |_| refit(Changed::Any));
    // A face that arrives late changes what fits without changing a box's size: when the
    // first faces are in, and whenever more finish loading.
    let fonts = document().fonts();
    if let Ok(ready) = fonts.ready() {
        let again = wasm_bindgen::closure::Closure::once(|_: wasm_bindgen::JsValue| {
            refit(Changed::Any);
        });
        let _ = ready.then(&again);
        again.forget();
    }
    on(&fonts.into(), "loadingdone", |_| refit(Changed::Any));
    upgrade_headers();
    add_filters();
    // Sort buttons and filter boxes change what fits.
    refit(Changed::Any);
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
        // The reader's order, kept on the table for the filter to arrange by.
        match order {
            Some(descending) => {
                let _ = main.set_attribute(
                    "data-rv-sorted",
                    &format!("{column}:{}", if descending { "desc" } else { "asc" }),
                );
            }
            None => {
                let _ = main.remove_attribute("data-rv-sorted");
            }
        }
        arrange(&main);
        // Some browsers blur a clicked button before activation. Keep the sort header
        // focused on every activation, as on keyboard sorting, without moving the page.
        if let Some(button) = button.dyn_ref::<web_sys::HtmlElement>() {
            let options = web_sys::FocusOptions::new();
            options.set_prevent_scroll(true);
            let _ = button.focus_with_options(&options);
        }
    });
}
