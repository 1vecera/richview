//! What a finding leaves on the source overlay: a wave under its range, a dot in the gutter and
//! its message after the line. The overlay is built again on every edit, so the marks are kept
//! here and drawn again after each build. They sit on the overlay only: the text area under it
//! never holds one, so the caret, the selection and undo are the browser's own.
use super::{document, element, query, style};
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use web_sys::{Element, Node, Text};

/// How serious a mark is; an error outranks a warning on the same line.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    Error,
    Warning,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// One finding on one line: the line counted from 0 and the stretch of it in UTF-16 units.
#[derive(Clone)]
pub(crate) struct Mark {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub level: Level,
    pub message: String,
}

/// The least width, in pixels, a message needs after the line's text to be worth showing there.
const LEAST_ROOM: f64 = 140.0;
/// The most lines drawn at once, so a source with hundreds of findings stays quick to type in.
const MOST_LINES: usize = 40;

thread_local! {static MARKS: RefCell<Vec<Mark>> = const { RefCell::new(Vec::new()) };}

/// Replaces the marks and draws them.
pub(crate) fn set(marks: Vec<Mark>) {
    clear_drawing();
    MARKS.with(|held| *held.borrow_mut() = marks);
    draw();
}

/// Draws the held marks again on a freshly built overlay.
pub(crate) fn apply() {
    if MARKS.with(|held| held.borrow().is_empty()) {
        return;
    }
    draw();
}

/// The marked lines, counted from 0, in order and without repeats.
pub(crate) fn lines() -> Vec<usize> {
    let mut lines: Vec<usize> = MARKS.with(|held| held.borrow().iter().map(|m| m.line).collect());
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// Takes the drawing off the overlay, leaving its text as it was.
fn clear_drawing() {
    let Some(code) = query(".code__hl code") else {
        return;
    };
    if let Ok(chips) = code.query_selector_all(".code__say") {
        for i in 0..chips.length() {
            if let Some(chip) = chips.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                chip.remove();
            }
        }
    }
    if let Ok(waves) = code.query_selector_all(".code__wave") {
        for i in 0..waves.length() {
            let Some(wave) = waves.item(i) else { continue };
            let Some(parent) = wave.parent_node() else {
                continue;
            };
            while let Some(child) = wave.first_child() {
                let _ = parent.insert_before(&child, Some(&wave));
            }
            let _ = parent.remove_child(&wave);
        }
    }
    if let Ok(lines) = code.query_selector_all(".is-bad, .is-warn") {
        for i in 0..lines.length() {
            if let Some(line) = lines.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                let _ = line.class_list().remove_2("is-bad", "is-warn");
            }
        }
    }
    code.normalize();
}

/// Draws the held marks on the overlay's lines.
fn draw() {
    let Some(code) = query(".code__hl code") else {
        return;
    };
    let marks = MARKS.with(|held| held.borrow().clone());
    let children = code.children();
    let mut drawn: Vec<usize> = vec![];
    for mark in &marks {
        if !drawn.contains(&mark.line) && drawn.len() >= MOST_LINES {
            continue;
        }
        let Some(line) = children.item(mark.line as u32) else {
            continue;
        };
        let class = match mark.level {
            Level::Error => "is-bad",
            Level::Warning => "is-warn",
        };
        // An error already on the line keeps the line's look.
        if !(mark.level == Level::Warning && line.class_list().contains("is-bad")) {
            let _ = line.class_list().remove_2("is-bad", "is-warn");
            let _ = line.class_list().add_1(class);
        }
        wave(&line, mark);
        if !drawn.contains(&mark.line) {
            drawn.push(mark.line);
        }
    }
    // The messages come after the waves: wrapping text in them moves nothing, but a message is
    // placed from where the line's text ends.
    for line in drawn {
        let first = marks
            .iter()
            .filter(|m| m.line == line)
            .min_by_key(|m| m.level == Level::Warning);
        if let (Some(mark), Some(element)) = (first, children.item(line as u32)) {
            say(&element, mark);
        }
    }
}

/// The text nodes under `node`, in order.
fn texts(node: &Node, found: &mut Vec<Text>) {
    let mut child = node.first_child();
    while let Some(current) = child {
        if current.node_type() == Node::TEXT_NODE {
            if let Ok(text) = current.clone().dyn_into::<Text>() {
                found.push(text);
            }
        } else {
            texts(&current, found);
        }
        child = current.next_sibling();
    }
}

/// Wraps the mark's stretch of the line's text in a wave, across the highlighter's own spans.
fn wave(line: &Element, mark: &Mark) {
    let mut nodes = vec![];
    texts(line, &mut nodes);
    let mut at = 0;
    let mut spans = vec![];
    for text in nodes {
        let length = text.length() as usize;
        spans.push((text, at, at + length));
        at += length;
    }
    // From the last node back, so splitting one never moves the offsets of those before it.
    for (text, from, to) in spans.into_iter().rev() {
        let (start, end) = (mark.start.max(from), mark.end.min(to));
        if start >= end {
            continue;
        }
        let mut middle = text;
        if start > from {
            match middle.split_text((start - from) as u32) {
                Ok(rest) => middle = rest,
                Err(_) => continue,
            }
        }
        if end < to {
            let _ = middle.split_text((end - start) as u32);
        }
        let Ok(span) = document().create_element("span") else {
            continue;
        };
        span.set_class_name(&format!("code__wave code__wave--{}", mark.level.name()));
        if let Some(parent) = middle.parent_node() {
            let _ = parent.insert_before(&span, Some(&middle));
            let _ = span.append_child(&middle);
        }
    }
}

/// Puts the message after the end of the line's text, where there is room for it.
fn say(line: &Element, mark: &Mark) {
    let Ok(range) = document().create_range() else {
        return;
    };
    if range.select_node_contents(line).is_err() {
        return;
    }
    let Some(rects) = range.get_client_rects() else {
        return;
    };
    // The last row of a wrapped line: the zero-width space the overlay ends each line with
    // has no width and is skipped.
    let last = (0..rects.length())
        .filter_map(|i| rects.item(i))
        .rfind(|rect| rect.width() > 0.0);
    let Some(last) = last else {
        return;
    };
    let base = line.get_bounding_client_rect();
    let left = last.right() - base.left() + 16.0;
    if base.width() - left < LEAST_ROOM {
        return;
    }
    let Ok(chip) = element("span", "code__say", line) else {
        return;
    };
    let _ = chip.set_attribute("data-level", mark.level.name());
    chip.set_text_content(Some(&mark.message));
    style(&chip, "left", &format!("{left}px"));
    style(&chip, "top", &format!("{}px", last.top() - base.top()));
}
