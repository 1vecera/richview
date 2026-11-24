//! Moving through slides (FEA-RP-65). The slides are one vertical sequence that
//! the page's stylesheet snaps to each slide, so the wheel, a swipe and the browser's own keys
//! work without this module. It adds the keys a presenter expects, the counter, full screen and
//! the slide in the address. Gliding is the stylesheet's (`scroll-behavior`), so reduced motion
//! jumps without a word here.

use super::{document, el, on, query_all, window};
use std::cell::Cell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlElement, KeyboardEvent};

/// What a key press asks for.
#[derive(Clone, Copy)]
enum Step {
    On,
    Back,
    First,
    Last,
}

/// The share of the window a slide taller than it moves by, so a line is read twice at most.
const PAGE: f64 = 0.9;

struct Deck {
    slides: Vec<Element>,
    count: HtmlElement,
    current: Cell<usize>,
    queued: Cell<bool>,
    /// The address carries the slide only once the reader has moved.
    moved: Cell<bool>,
}

pub(super) fn start() {
    let slides = query_all("html[data-rv-slides] .rv-slide");
    if slides.is_empty() {
        return;
    }
    // The slides announce their own place ("Slide 4 of 12"); the counter is for the eye.
    let count = el("div", "rv-slides__count");
    let _ = count.set_attribute("aria-hidden", "true");
    if let Some(body) = document().body() {
        let _ = body.append_child(&count);
    }
    let deck = Rc::new(Deck {
        slides,
        count,
        current: Cell::new(0),
        queued: Cell::new(false),
        moved: Cell::new(false),
    });
    if let Some(at) = addressed(deck.slides.len()) {
        deck.go(at, true);
    }
    deck.update();
    let target: web_sys::EventTarget = window().into();
    for event in ["scroll", "resize"] {
        let deck = deck.clone();
        on(&target, event, move |_| deck.clone().queue());
    }
    {
        let deck = deck.clone();
        on(&target, "hashchange", move |_| {
            if let Some(at) = addressed(deck.slides.len())
                && at != deck.current.get()
            {
                deck.go(at, false);
            }
        });
    }
    let target: web_sys::EventTarget = document().into();
    on(&target, "keydown", move |event| {
        let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
            return;
        };
        deck.key(&event);
    });
}

/// The slide the address names (`#slide=4`, counted from 1), as an index.
fn addressed(slides: usize) -> Option<usize> {
    let hash = window().location().hash().unwrap_or_default();
    let number: usize = hash
        .trim_start_matches('#')
        .strip_prefix("slide=")?
        .parse()
        .ok()?;
    (1..=slides).contains(&number).then(|| number - 1)
}

fn window_height() -> f64 {
    window()
        .inner_height()
        .ok()
        .and_then(|h| h.as_f64())
        .unwrap_or(0.0)
}

/// Whether the keys belong to what has focus: a chart, table or control reads the arrows and
/// Space itself, and a field takes every key.
fn held(target: Option<&Element>, key: &str) -> bool {
    let Some(target) = target else {
        return false;
    };
    let within = |selector: &str| target.closest(selector).ok().flatten().is_some();
    if within("input, select, textarea, [contenteditable], dialog[open]") {
        return true;
    }
    let paging = matches!(key, "PageDown" | "PageUp" | "Home" | "End");
    !paging && within("a[href], button, summary, [tabindex]")
}

impl Deck {
    fn queue(self: Rc<Self>) {
        if self.queued.replace(true) {
            return;
        }
        let deck = self.clone();
        let frame = Closure::once_into_js(move || {
            deck.queued.set(false);
            deck.update();
        });
        let _ = window().request_animation_frame(frame.unchecked_ref());
    }

    /// The slide across the middle of the window is the current one.
    fn update(&self) {
        let middle = window_height() / 2.0;
        let at = self
            .slides
            .iter()
            .position(|slide| slide.get_bounding_client_rect().bottom() > middle)
            .unwrap_or(self.slides.len() - 1);
        let previous = self.current.get();
        if at != previous {
            self.current.set(at);
            self.moved.set(true);
            let _ = self.slides[previous].remove_attribute("data-rv-current");
        }
        // The stage reads it, as the counter's colour over a slide set on a field.
        if !self.slides[at].has_attribute("data-rv-current") {
            let _ = self.slides[at].set_attribute("data-rv-current", "");
        }
        let label = format!("{} / {}", at + 1, self.slides.len());
        if self.count.text_content().as_deref() != Some(&label) {
            self.count.set_text_content(Some(&label));
        }
        if self.moved.get() {
            self.address(at);
        }
    }

    /// Replace-state, as the tabs do, so the back button leaves the slides in one step.
    fn address(&self, at: usize) {
        let hash = format!("#slide={}", at + 1);
        if window().location().hash().unwrap_or_default() == hash {
            return;
        }
        let Ok(history) = window().history() else {
            return;
        };
        let href = window().location().href().unwrap_or_default();
        let base = href.split('#').next().unwrap_or("");
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&format!("{base}{hash}")));
    }

    /// Bring a slide to the top of the window; `at_once` skips the stylesheet's glide, as for
    /// the slide the address opens on.
    fn go(&self, at: usize, at_once: bool) {
        let Some(slide) = self.slides.get(at) else {
            return;
        };
        let root = document()
            .document_element()
            .and_then(|root| root.dyn_into::<HtmlElement>().ok());
        if at_once && let Some(root) = &root {
            let _ = root.style().set_property("scroll-behavior", "auto");
        }
        slide.scroll_into_view();
        if at_once && let Some(root) = &root {
            let _ = root.style().remove_property("scroll-behavior");
        }
    }

    fn key(&self, event: &KeyboardEvent) {
        if event.default_prevented() || event.ctrl_key() || event.meta_key() || event.alt_key() {
            return;
        }
        let key = event.key();
        let target = event.target().and_then(|t| t.dyn_into::<Element>().ok());
        if held(target.as_ref(), &key) {
            return;
        }
        let step = match (key.as_str(), event.shift_key()) {
            (" ", false) | ("PageDown" | "ArrowRight" | "ArrowDown", _) => Step::On,
            (" ", true) | ("PageUp" | "ArrowLeft" | "ArrowUp", _) => Step::Back,
            ("Home", _) => Step::First,
            ("End", _) => Step::Last,
            ("f" | "F", _) => {
                event.prevent_default();
                full_screen();
                return;
            }
            _ => return,
        };
        event.prevent_default();
        self.step(step);
    }

    fn step(&self, step: Step) {
        let at = self.current.get();
        let last = self.slides.len() - 1;
        let height = window_height();
        let rect = self.slides[at].get_bounding_client_rect();
        match step {
            // A slide taller than the window (a phone, or one that grew) is read to its end
            // before the next one comes.
            Step::On if rect.bottom() > height + 1.0 => {
                window().scroll_by_with_x_and_y(0.0, (rect.bottom() - height).min(height * PAGE));
            }
            Step::Back if rect.top() < -1.0 => {
                window().scroll_by_with_x_and_y(0.0, rect.top().max(-height * PAGE));
            }
            Step::On if at < last => self.go(at + 1, false),
            Step::Back if at > 0 => self.go(at - 1, false),
            Step::First => self.go(0, false),
            Step::Last => self.go(last, false),
            _ => {}
        }
    }
}

/// Toggle full screen for the whole page; a browser that refuses (an embedded frame without the
/// permission) leaves the page as it is.
fn full_screen() {
    if document().fullscreen_element().is_some() {
        document().exit_fullscreen();
    } else if document().fullscreen_enabled()
        && let Some(root) = document().document_element()
    {
        let _ = root.request_fullscreen();
    }
}
