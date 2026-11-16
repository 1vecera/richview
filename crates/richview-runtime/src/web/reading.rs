//! Reading aids (FEA-RP-63, FEA-RP-64): the progress bar fills with the share of the page read,
//! and the contents mark the section being read. In a report with contents the bar has a gap where
//! each section after the first starts, so the sections still show in one colour. All of it is set
//! once per animation frame after a scroll or resize; nothing else on the page changes.

use super::{document, on, query_all, window};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlElement};

/// A section is being read once its heading is above this share of the window's height.
const READ_LINE: f64 = 0.3;

pub(super) fn start() {
    let bar = document()
        .query_selector(".rv-progress")
        .ok()
        .flatten()
        .and_then(|e| e.dyn_into::<HtmlElement>().ok());
    let links = query_all(".rv-toc a[href^='#'], .rv-toc-fold a[href^='#']");
    let mut headings: Vec<(String, Element)> = Vec::new();
    for link in &links {
        let Some(id) = link
            .get_attribute("href")
            .and_then(|h| h.strip_prefix('#').map(str::to_owned))
        else {
            continue;
        };
        if headings.iter().any(|(seen, _)| *seen == id) {
            continue;
        }
        if let Some(heading) = document().get_element_by_id(&id) {
            headings.push((id, heading));
        }
    }
    if bar.is_none() && headings.is_empty() {
        return;
    }
    let mut gaps = Vec::new();
    if let Some(bar) = &bar {
        // The track shows only from here: without scripts the bar stays empty.
        let _ = bar.set_attribute("data-live", "");
        for _ in 1..headings.len() {
            let gap = super::el("span", "");
            let _ = bar.append_child(&gap);
            gaps.push(gap);
        }
    }
    let state = Rc::new(Reading {
        bar,
        links,
        headings,
        gaps,
        edges: RefCell::new(Vec::new()),
        current: Cell::new(None),
        queued: Cell::new(false),
    });
    state.update();
    let target: web_sys::EventTarget = window().into();
    for event in ["scroll", "resize"] {
        let state = state.clone();
        on(&target, event, move |_| state.clone().queue());
    }
}

struct Reading {
    bar: Option<HtmlElement>,
    links: Vec<Element>,
    headings: Vec<(String, Element)>,
    /// One per heading after the first; empty without a bar.
    gaps: Vec<HtmlElement>,
    /// Where each gap sits, in hundredths of a percent of the bar, as last laid out.
    edges: RefCell<Vec<i64>>,
    current: Cell<Option<usize>>,
    queued: Cell<bool>,
}

impl Reading {
    /// Each section after the first starts at the scroll position where its heading crosses the
    /// reading line, so the fill reaches a gap as the contents move to that section. Charts and
    /// images change the page's height as they load, so the gaps follow the layout on every update.
    fn lay_gaps(&self, top: f64, height: f64, room: f64) {
        let edges: Vec<i64> = self
            .headings
            .iter()
            .skip(1)
            .map(|(_, heading)| {
                let at = heading.get_bounding_client_rect().top() + top - height * READ_LINE;
                ((at / room).clamp(0.0, 1.0) * 10_000.0).round() as i64
            })
            .collect();
        if *self.edges.borrow() == edges {
            return;
        }
        for (gap, edge) in self.gaps.iter().zip(&edges) {
            let _ = gap
                .style()
                .set_property("left", &format!("{}%", *edge as f64 / 100.0));
        }
        *self.edges.borrow_mut() = edges;
    }

    fn queue(self: Rc<Self>) {
        if self.queued.replace(true) {
            return;
        }
        let state = self.clone();
        let frame = Closure::once_into_js(move || {
            state.queued.set(false);
            state.update();
        });
        let _ = window().request_animation_frame(frame.unchecked_ref());
    }

    fn update(&self) {
        let window = window();
        let height = window
            .inner_height()
            .ok()
            .and_then(|h| h.as_f64())
            .unwrap_or(0.0);
        let top = window.scroll_y().unwrap_or(0.0);
        let page = document()
            .document_element()
            .map_or(0.0, |e| f64::from(e.scroll_height()));
        let room = page - height;
        let read = if room > 1.0 {
            (top / room).clamp(0.0, 1.0)
        } else {
            0.0
        };
        if let Some(bar) = &self.bar {
            let _ = bar.style().set_property("--rv-read", &format!("{read:.4}"));
        }
        if self.headings.is_empty() {
            return;
        }
        if !self.gaps.is_empty() && room > 1.0 {
            self.lay_gaps(top, height, room);
        }
        // The last heading above the reading line; at the end of the page, the last section,
        // which may be too short for its heading to reach the line.
        let current = if read >= 0.999 && room > 1.0 {
            Some(self.headings.len() - 1)
        } else {
            self.headings
                .iter()
                .rposition(|(_, h)| h.get_bounding_client_rect().top() <= height * READ_LINE)
        };
        if self.current.replace(current) == current {
            return;
        }
        let id = current.map(|i| self.headings[i].0.as_str());
        for link in &self.links {
            let href = link.get_attribute("href").unwrap_or_default();
            if id.is_some_and(|id| href.strip_prefix('#') == Some(id)) {
                let _ = link.set_attribute("aria-current", "location");
            } else {
                let _ = link.remove_attribute("aria-current");
            }
        }
    }
}
