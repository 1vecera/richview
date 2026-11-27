//! The live example's two tools, over the source: a picker for the type of the chart the caret is
//! in (else the first chart), and buttons that add a chart, a table, a metric or a callout. Both
//! edit the source as typing would, so the report renders as it does on typing and the browser's
//! undo takes the change back. What fits is the core's answer, from `crate::tools`: the picker
//! lists only the types the core draws for this chart without a finding. Asking costs a render per
//! type, so it waits for the reader to pause, runs in slices between frames and is kept until the
//! chart changes.
use super::{document, now, on, query, window};
use crate::tools::{self, Add, Chart};
use std::cell::RefCell;
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{HtmlElement, HtmlSelectElement, HtmlTextAreaElement};

/// How long the source rests before the types are asked, so typing in a chart's title does not
/// render the chart as every type after every pause.
const REST_MS: i32 = 400;
/// The longest the types are asked for before the browser gets a frame.
const SLICE_MS: f64 = 12.0;
/// How near the example comes to the screen, in screens, before the report-face catalog is
/// requested. The catalog is what the core measures labels with, so the answer waits for it, and a
/// first visit does not download it (PRF-36).
const NEAR: f64 = 1.5;

#[derive(Default)]
struct State {
    /// The reader has put the caret in the source: before that it has no place, and a block goes
    /// at the end.
    caret: bool,
    /// The report-face catalog is in, so the core's label measures are the reader's.
    ready: bool,
    asked: bool,
    /// The latest request for the fitting types; an older one stops where it is.
    request: u32,
    /// The latest answer: the types that fit one chart.
    held: Option<Held>,
    /// What the last action did, shown in place of the count until the next edit.
    said: Option<String>,
    timer: Option<i32>,
    tick: Option<Closure<dyn FnMut()>>,
}

/// The types the core found for a chart, and which chart it was: the answer is good while the
/// chart's key stays, and stands in while the reader is still typing in the chart.
#[derive(Clone)]
struct Held {
    key: String,
    first: usize,
    types: Vec<&'static str>,
}

thread_local! {static TOOLS: RefCell<State> = RefCell::new(State::default());}

fn with<T>(f: impl FnOnce(&mut State) -> T) -> T {
    TOOLS.with(|state| f(&mut state.borrow_mut()))
}

fn area() -> Option<HtmlTextAreaElement> {
    query("#demo-source").and_then(|e| e.dyn_into().ok())
}

fn picker() -> Option<HtmlSelectElement> {
    query("#demo-chart-type").and_then(|e| e.dyn_into().ok())
}

/// Says what the last action did, to assistive technology and in the note beside the picker,
/// until the next edit.
fn announce(text: &str) {
    if let Some(status) = query("#demo-tools-status") {
        status.set_text_content(Some(text));
    }
    with(|state| state.said = (!text.is_empty()).then(|| text.to_owned()));
    sync();
}

/// The text area's caret, once the reader has put one there.
fn caret(area: &HtmlTextAreaElement) -> Option<usize> {
    with(|state| state.caret)
        .then(|| area.selection_start().ok().flatten())
        .flatten()
        .map(|caret| caret as usize)
}

/// The chart the tools act on: the one the caret is in, else the first.
fn target(area: &HtmlTextAreaElement, source: &str) -> Option<Chart> {
    let line = caret(area).map(|caret| crate::completion::position(source, caret).0);
    tools::chart_at(source, line)
}

/// Gives the tools their listeners and turns them on.
pub(crate) fn start() {
    let Some(source) = area() else {
        return;
    };
    let target: web_sys::EventTarget = source.clone().into();
    // A press or a key is the reader's own; a focus the page gave the text area is not.
    for event in ["pointerdown", "keydown", "input"] {
        on(&target, event, |_| with(|state| state.caret = true));
    }
    // Moving the caret to another chart changes which one the picker shows.
    for event in ["keyup", "click"] {
        on(&target, event, |_| sync());
    }
    if let Some(select) = picker() {
        let chosen = select.clone();
        on(&select.into(), "change", move |_| retype(&chosen.value()));
    }
    if let Ok(buttons) = document().query_selector_all("#demo-tools [data-add]") {
        for i in 0..buttons.length() {
            let Some(button) = buttons
                .item(i)
                .and_then(|b| b.dyn_into::<HtmlElement>().ok())
            else {
                continue;
            };
            let Some(add) = button
                .get_attribute("data-add")
                .and_then(|n| Add::named(&n))
            else {
                continue;
            };
            let _ = button.remove_attribute("disabled");
            on(&button.into(), "click", move |_| add_block(add));
        }
    }
    // The catalog is requested once the example is near, as the best-elements report is made.
    let page: web_sys::EventTarget = window().into();
    on(&page, "scroll", |_| near());
    near();
    sync();
}

/// Requests the report-face catalog once the example is near the screen.
fn near() {
    if with(|state| state.asked) {
        return;
    }
    let Some(demo) = query("#demo") else {
        return;
    };
    let screen = window()
        .inner_height()
        .ok()
        .and_then(|height| height.as_f64())
        .unwrap_or(0.0);
    if demo.get_bounding_client_rect().top() >= NEAR * screen {
        return;
    }
    with(|state| state.asked = true);
    spawn_local(async {
        super::font_catalog_ready().await;
        with(|state| state.ready = true);
        sync();
    });
}

/// The source changed: what the last action said is old, and its chart may be another, or ask
/// other types.
pub(crate) fn typed() {
    announce("");
}

/// A report rendered, so the catalog is in.
pub(crate) fn rendered() {
    with(|state| state.ready = true);
    sync();
}

/// The tools belong to the report's source, not to the style file.
pub(crate) fn show_tools(source_tab: bool) {
    if let Some(tools) = query("#demo-tools") {
        if source_tab {
            let _ = tools.remove_attribute("hidden");
        } else {
            let _ = tools.set_attribute("hidden", "");
        }
    }
}

/// Shows the picker for the chart the caret is in, and asks the types if they are not known.
fn sync() {
    let Some(area) = area() else {
        return;
    };
    let source = area.value();
    let chart = target(&area, &source);
    let held = with(|state| state.held.clone());
    let key = chart.as_ref().map(|chart| tools::key(&source, chart));
    // While the reader types in a chart, the list it had stands until the new answer comes.
    let shown = match (&chart, &held) {
        (Some(chart), Some(held)) if held.first == chart.first => Some(held.types.as_slice()),
        _ => None,
    };
    show(chart.as_ref(), shown);
    let fresh = held.zip(key).is_some_and(|(held, key)| held.key == key);
    if chart.is_some() && !fresh && with(|state| state.ready) {
        wait();
    }
}

/// Asks the types once the source has rested.
fn wait() {
    with(|state| {
        state.request += 1;
        if let Some(timer) = state.timer.take() {
            window().clear_timeout_with_handle(timer);
        }
        let tick = state
            .tick
            .get_or_insert_with(|| Closure::wrap(Box::new(ask) as Box<dyn FnMut()>));
        state.timer = window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                tick.as_ref().unchecked_ref(),
                REST_MS,
            )
            .ok();
    });
}

/// Lets the browser draw a frame.
async fn frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0);
    });
    let _ = JsFuture::from(promise).await;
}

/// Asks the core which types fit the chart, a few at a time between frames.
fn ask() {
    let request = with(|state| {
        state.timer = None;
        state.request
    });
    spawn_local(async move {
        let Some(area) = area() else {
            return;
        };
        let source = area.value();
        let Some(chart) = target(&area, &source) else {
            return;
        };
        let key = tools::key(&source, &chart);
        let options = super::homepage::completion_options();
        let mut found = vec![];
        let mut slice = now();
        // A chart that already draws with an error is not tried against the types.
        for kind in tools::candidates(&source, &chart, &options) {
            if now() - slice > SLICE_MS {
                frame().await;
                if with(|state| state.request) != request {
                    return;
                }
                slice = now();
            }
            if tools::fits(&source, &chart, kind, &options) {
                found.push(kind);
            }
        }
        // The reader may have typed while the slices ran.
        if with(|state| state.request) != request {
            return;
        }
        with(|state| {
            state.held = Some(Held {
                key,
                first: chart.first,
                types: found,
            });
        });
        sync();
    });
}

/// Puts the picker's options and note in order for the chart, with its state.
fn show(chart: Option<&Chart>, fitting: Option<&[&'static str]>) {
    let (Some(select), Some(note)) = (picker(), query("#demo-fit")) else {
        return;
    };
    let total = richview_core::CHART_TYPES.len();
    let (options, said, enabled): (Vec<String>, String, bool) = match (chart, fitting) {
        (None, _) => (vec![], "This report has no chart.".into(), false),
        (Some(chart), None) => (
            vec![chart.kind.clone()],
            if with(|state| state.ready) {
                "Checking which types fit…".into()
            } else {
                String::new()
            },
            false,
        ),
        (Some(chart), Some([])) => (
            vec![chart.kind.clone()],
            "This chart has findings. Fix them to change its type.".into(),
            false,
        ),
        (Some(chart), Some(types)) => {
            let mut options: Vec<String> = types.iter().map(|kind| kind.to_string()).collect();
            // The type the source writes stays in the list, fitting or not.
            if !options.contains(&chart.kind) {
                options.insert(0, chart.kind.clone());
            }
            let said = format!("{} of {total} fit this data", types.len());
            let enabled = options.len() > 1;
            (options, said, enabled)
        }
    };
    let current = chart.map(|chart| chart.kind.clone()).unwrap_or_default();
    // Rebuilding an open list would close it, so only a changed list is rebuilt.
    let signature = options.join(",");
    if select.get_attribute("data-options").as_deref() != Some(signature.as_str()) {
        select.set_text_content(None);
        for kind in &options {
            if let Ok(option) = document().create_element("option") {
                let _ = option.set_attribute("value", kind);
                option.set_text_content(Some(kind));
                let _ = select.append_child(&option);
            }
        }
        if options.is_empty()
            && let Ok(option) = document().create_element("option")
        {
            option.set_text_content(Some("none"));
            let _ = select.append_child(&option);
        }
        let _ = select.set_attribute("data-options", &signature);
    }
    select.set_value(&current);
    select.set_disabled(!enabled);
    match with(|state| state.said.clone()) {
        Some(said) => {
            let _ = note.set_attribute("data-said", "");
            note.set_text_content(Some(&said));
        }
        None => {
            let _ = note.remove_attribute("data-said");
            note.set_text_content(Some(&said));
        }
    }
}

/// The reader chose a type. The core is asked again, so a list that has gone stale while the
/// reader typed never lets an unfit type in.
fn retype(kind: &str) {
    let Some(area) = area() else {
        return;
    };
    super::writing::hand_over();
    let source = area.value();
    let Some(chart) = target(&area, &source) else {
        return;
    };
    if chart.kind == kind {
        return;
    }
    if !tools::fits(
        &source,
        &chart,
        kind,
        &super::homepage::completion_options(),
    ) {
        announce(&format!("A {kind} chart does not fit this data."));
        sync();
        return;
    }
    let edit = tools::retype(&source, &chart, kind);
    let (from, to) = (
        area.selection_start().ok().flatten().unwrap_or(0) as usize,
        area.selection_end().ok().flatten().unwrap_or(0) as usize,
    );
    let (top, left) = (area.scroll_top(), area.scroll_left());
    let focused = document().active_element();
    let placed = with(|state| state.caret);
    super::complete::put(&area, edit.start as u32, edit.end as u32, &edit.text);
    super::complete::hide();
    // The change is the picker's, not the reader's: their caret, if they have one, and their place
    // in the source stay.
    with(|state| state.caret = placed);
    let _ = area.set_selection_range(
        tools::remap(from, &edit) as u32,
        tools::remap(to, &edit) as u32,
    );
    area.set_scroll_top(top);
    area.set_scroll_left(left);
    if let Some(focused) = focused.and_then(|e| e.dyn_into::<HtmlElement>().ok()) {
        let _ = focused.focus();
    }
    announce(&format!("Chart type changed to {kind}."));
}

/// Adds a block to the source, after the paragraph or block the caret is in, or at the end.
fn add_block(add: Add) {
    let Some(area) = area() else {
        return;
    };
    super::writing::hand_over();
    super::complete::hide();
    let source = area.value();
    let at = caret(&area).map(|_| area.selection_end().ok().flatten().unwrap_or(0) as usize);
    let (edit, line) = tools::insertion(&source, at, add.block());
    super::complete::put(&area, edit.start as u32, edit.end as u32, &edit.text);
    super::complete::hide();
    reveal(&area, line);
    announce(&format!("Added a {} at line {line}.", add.name()));
}

/// Scrolls the text area to put `line`, counted from 1, near its top, so the block just added is in
/// view whole: the caret sits at its end, and a phone's text area does not follow it.
fn reveal(area: &HtmlTextAreaElement, line: usize) {
    let declaration = window().get_computed_style(area).ok().flatten();
    let length = |property: &str| {
        declaration
            .as_ref()
            .and_then(|d| d.get_property_value(property).ok())
            .and_then(|value| value.trim_end_matches("px").parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    let height = length("line-height").max(1.0);
    let top = length("padding-top") + line.saturating_sub(1) as f64 * height - 2.0 * height;
    area.set_scroll_top(top.max(0.0) as i32);
}
