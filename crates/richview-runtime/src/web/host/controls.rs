//! Reader controls in the report runtime (FEA-BR-01 to FEA-BR-06, FEA-BR-32). Core reserves the
//! control row with the initial state; the runtime wires it, moves the chart between states with
//! a glide (FEA-BR-02, FEA-CH-52), runs playback, and keeps the Table, the printed state line and
//! the expanded view on the state the reader chose.

use super::{View, check_settled, draw, layout, reduced_motion, show_readout};
use crate::web::{now, on, window};
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlElement, HtmlInputElement, HtmlSelectElement};

/// Longest glide between two states (FEA-CH-52).
const GLIDE_MS: f64 = 600.0;

/// What moved the controls.
#[derive(Clone, Copy, PartialEq)]
enum Change {
    /// The reader chose an option, stepped or scrubbed: playback pauses.
    Reader,
    /// Playback advanced a frame.
    Tick,
}

fn items(root: &Element, selector: &str) -> Vec<Element> {
    let mut out = Vec::new();
    if let Ok(list) = root.query_selector_all(selector) {
        for i in 0..list.length() {
            if let Some(e) = list.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                out.push(e);
            }
        }
    }
    out
}

fn index(e: &Element, attr: &str) -> Option<usize> {
    e.get_attribute(attr)?.parse().ok()
}

/// Wire the control row core reserved in `view`'s figure, if the chart has controls.
pub(super) fn build(view: &Rc<RefCell<View>>) -> Result<(), JsValue> {
    let (figure, empty) = {
        let v = view.borrow();
        (v.figure.clone(), v.chart.controls.is_empty())
    };
    if empty {
        return Ok(());
    }
    let Some(row) = figure.query_selector(".rv-chart__controls")? else {
        return Ok(());
    };
    let row: HtmlElement = row.unchecked_into();
    for group in items(&row, "[data-rv-control]") {
        let Some(k) = index(&group, "data-rv-control") else {
            continue;
        };
        let playback = view
            .borrow()
            .chart
            .controls
            .list
            .get(k)
            .is_some_and(|c| c.is_playback());
        if playback {
            wire_playback(view, &group, k);
            continue;
        }
        for button in items(&group, ".rv-chart__option") {
            let Some(i) = index(&button, "data-rv-option") else {
                continue;
            };
            let w = Rc::downgrade(view);
            let target: web_sys::EventTarget = button.into();
            on(&target, "click", move |_| {
                if let Some(view) = w.upgrade() {
                    choose(&view, k, i, Change::Reader);
                }
            });
        }
        if let Ok(Some(select)) = group.query_selector("select") {
            let select: HtmlSelectElement = select.unchecked_into();
            let w = Rc::downgrade(view);
            let s = select.clone();
            let target: web_sys::EventTarget = select.into();
            on(&target, "change", move |_| {
                if let (Some(view), Ok(i)) = (w.upgrade(), s.value().parse::<usize>()) {
                    choose(&view, k, i, Change::Reader);
                }
            });
        }
    }
    row.remove_attribute("data-rv-pending")?;
    row.remove_attribute("inert")?;
    view.borrow_mut().controls = Some(row);
    sync(view);
    Ok(())
}

fn wire_playback(view: &Rc<RefCell<View>>, group: &Element, k: usize) {
    let step = |delta: isize| {
        let w = Rc::downgrade(view);
        move |_e: web_sys::Event| {
            if let Some(view) = w.upgrade() {
                step_frame(&view, k, delta);
            }
        }
    };
    if let Ok(Some(b)) = group.query_selector(".rv-chart__control-button--previous") {
        on(&b.into(), "click", step(-1));
    }
    if let Ok(Some(b)) = group.query_selector(".rv-chart__control-button--next") {
        on(&b.into(), "click", step(1));
    }
    if let Ok(Some(b)) = group.query_selector(".rv-chart__control-button--play") {
        let w = Rc::downgrade(view);
        on(&b.into(), "click", move |_| {
            if let Some(view) = w.upgrade() {
                toggle_play(&view, k);
            }
        });
    }
    if let Ok(Some(s)) = group.query_selector(".rv-chart__scrubber") {
        let input: HtmlInputElement = s.unchecked_into();
        let w = Rc::downgrade(view);
        let i2 = input.clone();
        on(&input.into(), "input", move |_| {
            if let (Some(view), Ok(i)) = (w.upgrade(), i2.value().parse::<usize>()) {
                choose(&view, k, i, Change::Reader);
            }
        });
    }
    // ← and → step and Space plays or pauses anywhere in the playback group (FEA-BR-02).
    let w = Rc::downgrade(view);
    on(&group.clone().into(), "keydown", move |e| {
        let Some(view) = w.upgrade() else { return };
        let e: web_sys::KeyboardEvent = e.unchecked_into();
        if e.ctrl_key() || e.meta_key() || e.alt_key() {
            return;
        }
        let on_button = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .is_some_and(|t| t.tag_name() == "BUTTON");
        match e.key().as_str() {
            "ArrowLeft" => {
                e.prevent_default();
                step_frame(&view, k, -1);
            }
            "ArrowRight" => {
                e.prevent_default();
                step_frame(&view, k, 1);
            }
            " " if !on_button => {
                e.prevent_default();
                toggle_play(&view, k);
            }
            _ => {}
        }
    });
}

fn step_frame(view: &Rc<RefCell<View>>, k: usize, delta: isize) {
    let (current, frames) = {
        let v = view.borrow();
        let Some(c) = v.chart.controls.list.get(k) else {
            return;
        };
        (v.chart.state[k], c.options.len())
    };
    let next = (current as isize + delta).clamp(0, frames as isize - 1) as usize;
    choose(view, k, next, Change::Reader);
}

/// Speed and looping of control `k`, if it is a playback.
fn playback_of(view: &Rc<RefCell<View>>, k: usize) -> Option<(f64, bool, usize)> {
    let v = view.borrow();
    let c = v.chart.controls.list.get(k)?;
    match c.kind {
        richview_chart::reader_controls::Kind::Playback { speed, looping, .. } => {
            Some((speed, looping, c.options.len()))
        }
        _ => None,
    }
}

fn toggle_play(view: &Rc<RefCell<View>>, k: usize) {
    // One playback per chart: the inline and expanded views never both run a timer.
    let other = partner(view).filter(|p| p.borrow().playing.is_some());
    if view.borrow().playing.is_some() || other.is_some() {
        pause(view);
        if let Some(p) = other {
            pause(&p);
        }
        return;
    }
    let Some((speed, looping, frames)) = playback_of(view, k) else {
        return;
    };
    // Under reduced motion play advances one frame per press (FEA-BR-02).
    if reduced_motion() {
        let current = view.borrow().chart.state[k];
        let next = if current + 1 >= frames {
            if looping { 0 } else { current }
        } else {
            current + 1
        };
        choose(view, k, next, Change::Tick);
        return;
    }
    // A playback that stopped on its last frame restarts from the first.
    if view.borrow().chart.state[k] + 1 >= frames {
        choose(view, k, 0, Change::Tick);
    }
    schedule(view, k, speed);
    sync(view);
}

fn schedule(view: &Rc<RefCell<View>>, k: usize, speed: f64) {
    let w: Weak<RefCell<View>> = Rc::downgrade(view);
    let tick = Closure::once_into_js(move || {
        if let Some(view) = w.upgrade() {
            tick(&view, k);
        }
    });
    let id = window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(tick.unchecked_ref(), speed as i32)
        .unwrap_or(0);
    view.borrow_mut().playing = Some(id);
}

fn tick(view: &Rc<RefCell<View>>, k: usize) {
    if view.borrow().playing.is_none() {
        return;
    }
    // A chart that left the page, as in a closed tab panel, stops playing (FEA-BR-10).
    if view.borrow().frame.client_width() == 0 {
        pause(view);
        return;
    }
    let Some((speed, looping, frames)) = playback_of(view, k) else {
        return;
    };
    let current = view.borrow().chart.state[k];
    let next = if current + 1 >= frames {
        if !looping {
            pause(view);
            return;
        }
        0
    } else {
        current + 1
    };
    choose(view, k, next, Change::Tick);
    if !looping && next + 1 >= frames {
        pause(view);
    } else {
        schedule(view, k, speed);
    }
}

pub(super) fn pause(view: &Rc<RefCell<View>>) {
    if let Some(id) = view.borrow_mut().playing.take() {
        window().clear_timeout_with_handle(id);
    }
    sync(view);
}

/// Set control `k` to option `i`, in this view and in its inline or expanded partner.
fn choose(view: &Rc<RefCell<View>>, k: usize, i: usize, how: Change) {
    if how == Change::Reader {
        pause(view);
        if let Some(p) = partner(view) {
            pause(&p);
        }
    }
    let state = {
        let v = view.borrow();
        let mut state = v.chart.state.clone();
        match state.get_mut(k) {
            Some(s) if *s != i => *s = i,
            _ => return,
        }
        state
    };
    // Playback moves at one pace through every frame: each glide lasts the whole frame and runs
    // straight on into the next, so the marks never stop between years (FEA-BR-02).
    let (glide, steady) = match (how, playback_of(view, k)) {
        (Change::Tick, Some((speed, _, _))) => (speed, true),
        _ => (GLIDE_MS, false),
    };
    apply(view, state.clone(), glide, steady);
    if let Some(p) = partner(view) {
        apply(&p, state, 0.0, false);
    }
}

/// The inline chart's expanded view, or the expanded view's inline chart.
fn partner(view: &Rc<RefCell<View>>) -> Option<Rc<RefCell<View>>> {
    let v = view.borrow();
    v.expanded
        .clone()
        .or_else(|| v.owner.as_ref().and_then(Weak::upgrade))
}

/// Move `view` to `state`, gliding over `glide` milliseconds when motion is allowed, at one
/// `steady` pace or eased.
pub(super) fn apply(view: &Rc<RefCell<View>>, state: Vec<usize>, glide: f64, steady: bool) {
    view.borrow_mut().retired = None;
    // A new state during a glide sets off from where the marks are now, not where they were
    // going, so playback runs on without a jump.
    let from = {
        let v = view.borrow();
        match &v.glide {
            Some((from, start, length, steady)) => {
                super::glide_frame(from, &v.geometry, *start, *length, *steady)
            }
            None => v.geometry.clone(),
        }
    };
    {
        let mut v = view.borrow_mut();
        v.chart.set_state(state);
        // A zoom glide on its way lands first; the new state then decides whether it stays.
        if let Some(g) = v.zoom_glide.take() {
            v.zoom = g.end();
        }
        // The zoom window stays while it lies inside the new state's x range (FEA-BR-32).
        if let Some([a, b]) = v.zoom {
            let width = v.width.max(240.0);
            let whole = v.chart.geometry(width, &v.hidden.clone(), None);
            let inside = whole
                .panels
                .first()
                .and_then(|p| p.x.as_ref())
                .is_some_and(|s| a >= s.extent[0] && b <= s.extent[1]);
            if !inside {
                v.zoom = None;
                if let Some(reset) = &v.reset {
                    reset.set_hidden(true);
                }
            }
        }
        // Readouts belong to the state on screen.
        v.target = None;
        v.pinned = false;
        show_readout(&mut v, false);
    }
    layout(view, false);
    let motion = glide > 0.0
        && !reduced_motion()
        && view.borrow().geometry.entrance.is_some()
        && view.borrow().frame.client_width() > 0;
    if motion {
        view.borrow_mut().glide = Some((from, now(), glide, steady));
        glide_loop(Rc::downgrade(view));
    }
    sync(view);
    super::refresh_table(view);
}

fn glide_loop(weak: Weak<RefCell<View>>) {
    let Some(view) = weak.upgrade() else { return };
    let done = view
        .borrow()
        .glide
        .as_ref()
        .is_none_or(|(_, start, length, _)| now() - start >= *length);
    if done {
        view.borrow_mut().glide = None;
        draw(&view);
        check_settled();
        return;
    }
    draw(&view);
    let next = Closure::once_into_js(move || glide_loop(weak));
    let _ = window().request_animation_frame(next.unchecked_ref());
}

/// Show the state in the control row, the printed state line and the play button.
pub(super) fn sync(view: &Rc<RefCell<View>>) {
    let v = view.borrow();
    let controls = &v.chart.controls;
    if let Ok(Some(line)) = v.figure.query_selector(".rv-chart__state") {
        line.set_text_content(Some(&controls.describe(&v.chart.state)));
    }
    let Some(row) = &v.controls else { return };
    for group in items(row, "[data-rv-control]") {
        let Some(k) = index(&group, "data-rv-control") else {
            continue;
        };
        let (Some(c), Some(&chosen)) = (controls.list.get(k), v.chart.state.get(k)) else {
            continue;
        };
        let label = c
            .options
            .get(chosen)
            .map(|o| o.label.clone())
            .unwrap_or_default();
        if c.is_playback() {
            if let Ok(Some(s)) = group.query_selector(".rv-chart__scrubber") {
                let s: HtmlInputElement = s.unchecked_into();
                s.set_value(&chosen.to_string());
                let _ = s.set_attribute("aria-valuetext", &label);
            }
            if let Ok(Some(out)) = group.query_selector(".rv-chart__frame-value") {
                out.set_text_content(Some(&label));
            }
            if let Ok(Some(b)) = group.query_selector(".rv-chart__control-button--play") {
                let playing = v.playing.is_some();
                let _ = b.set_attribute("aria-pressed", if playing { "true" } else { "false" });
                let _ = b.set_attribute("aria-label", if playing { "Pause" } else { "Play" });
            }
            continue;
        }
        for button in items(&group, ".rv-chart__option") {
            let on = index(&button, "data-rv-option") == Some(chosen);
            let _ = button.set_attribute("aria-pressed", if on { "true" } else { "false" });
        }
        if let Ok(Some(select)) = group.query_selector("select") {
            select
                .unchecked_into::<HtmlSelectElement>()
                .set_value(&chosen.to_string());
        }
    }
}

/// The control row for an expanded view, built from the chart's own controls in their current
/// state (FEA-BR-32).
pub(super) fn row_html(view: &Rc<RefCell<View>>, id: &str) -> String {
    let v = view.borrow();
    if v.chart.controls.is_empty() {
        return String::new();
    }
    v.chart.controls.html(&v.chart.state, id)
}

/// The state in a few words for file names and the Table, or `None` without controls.
pub(super) fn state_name(v: &View) -> Option<String> {
    (!v.chart.controls.is_empty()).then(|| v.chart.controls.name(&v.chart.state))
}
