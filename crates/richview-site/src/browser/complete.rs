//! The live example's completion list (W3 scene 2): a small, light list under the caret
//! while the reader types a key, a value, a block's tag or a data field, from `crate::completion`.
//! Keyboard first: the arrows move, Enter or Tab puts the choice in, Escape closes, and Ctrl+Space
//! asks where nothing is typed yet. A click puts a choice in too; hovering does nothing (INT-59).
//! The text area keeps focus and names the list and active choice with `aria-controls` and
//! `aria-activedescendant`. A status region announces when the list opens and closes. Labels go
//! in as text, never as markup, since a data field's name is the reader's own.
use super::{document, on, style, window};
use crate::completion::{Choice, at};
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlDocument, HtmlElement, HtmlTextAreaElement, InputEvent, KeyboardEvent};

const LIST_ID: &str = "demo-complete";
/// The gap between the caret's line and the list, in CSS pixels.
const GAP: f64 = 2.0;

struct List {
    area: HtmlTextAreaElement,
    list: HtmlElement,
    status: Element,
    choices: Vec<Choice>,
    active: usize,
    /// A press on the list has begun, so the text area's blur is not a reason to close it.
    pressing: bool,
    source: Option<Box<dyn Fn() -> (String, usize)>>,
}

thread_local! {static LIST: RefCell<Option<List>> = const { RefCell::new(None) };}

/// Gives the source's text area its list.
pub(crate) fn start(area: &HtmlTextAreaElement, source: Option<Box<dyn Fn() -> (String, usize)>>) {
    let Some(code) = area.parent_element() else {
        return;
    };
    let Ok(list) = document().create_element("ul") else {
        return;
    };
    list.set_class_name("complete");
    list.set_id(LIST_ID);
    for (name, value) in [
        ("role", "listbox"),
        ("aria-label", "Completions"),
        ("hidden", ""),
    ] {
        let _ = list.set_attribute(name, value);
    }
    if code.append_child(&list).is_err() {
        return;
    }
    let Ok(status) = document().create_element("span") else {
        return;
    };
    status.set_class_name("sr-only");
    status.set_id("demo-complete-status");
    let _ = status.set_attribute("role", "status");
    if code.append_child(&status).is_err() {
        return;
    }
    for (name, value) in [("aria-autocomplete", "list"), ("aria-controls", LIST_ID)] {
        let _ = area.set_attribute(name, value);
    }
    let target: web_sys::EventTarget = list.clone().into();
    // A mouse press keeps the focus in the text area; a touch may not, so the press is noted
    // before the blur it causes.
    on(&target, "mousedown", |event| event.prevent_default());
    on(&target, "pointerdown", |_| {
        with(|list| list.pressing = true);
    });
    on(&target, "click", |event| {
        let index = event
            .target()
            .and_then(|target| target.dyn_into::<Element>().ok())
            .and_then(|element| element.closest("[role=option]").ok().flatten())
            .and_then(|option| option.get_attribute("data-index"))
            .and_then(|index| index.parse().ok());
        with(|list| list.pressing = false);
        if let Some(index) = index {
            accept(index);
        }
    });
    let area_target: web_sys::EventTarget = area.clone().into();
    on(&area_target, "blur", |_| {
        if !with(|list| list.pressing).unwrap_or(false) {
            hide();
        }
    });
    // A click moves the caret off the word, so the list closes. The text area scrolls on its own
    // to keep the caret in view as the reader types near its edge, so the list follows its line
    // there and on a resize, and closes only once that line has left the text area.
    on(&area_target, "click", |_| hide());
    on(&area_target, "scroll", |_| follow());
    let page: web_sys::EventTarget = window().into();
    on(&page, "resize", |_| {
        follow();
    });
    LIST.with(|cell| {
        *cell.borrow_mut() = Some(List {
            area: area.clone(),
            list: list.unchecked_into(),
            status,
            choices: vec![],
            active: 0,
            pressing: false,
            source,
        });
    });
}

fn with<T>(f: impl FnOnce(&mut List) -> T) -> Option<T> {
    LIST.with(|cell| cell.borrow_mut().as_mut().map(f))
}

/// Opens, narrows or closes the list after an edit, as the reader types.
pub(crate) fn typed() {
    offer(false);
}

/// Handles a key in the text area before anything else does; returns whether it was the list's.
pub(crate) fn key(event: &KeyboardEvent) -> bool {
    if event.is_composing() {
        return false;
    }
    if event.key() == " " && event.ctrl_key() {
        event.prevent_default();
        offer(true);
        return true;
    }
    let open = with(|list| !list.choices.is_empty()).unwrap_or(false);
    if !open {
        return false;
    }
    match event.key().as_str() {
        "ArrowDown" | "ArrowUp" => {
            event.prevent_default();
            let step = if event.key() == "ArrowDown" { 1 } else { -1 };
            with(|list| {
                let count = list.choices.len() as isize;
                list.active = (list.active as isize + step).rem_euclid(count) as usize;
                mark(list);
            });
            true
        }
        "Enter" | "Tab" if !event.shift_key() => {
            event.prevent_default();
            if let Some(index) = with(|list| list.active) {
                accept(index);
            }
            true
        }
        "Escape" => {
            event.prevent_default();
            hide();
            true
        }
        // Moving the caret leaves the word, and a shifted Tab leaves the text area.
        "ArrowLeft" | "ArrowRight" | "Home" | "End" | "PageUp" | "PageDown" | "Tab" => {
            hide();
            false
        }
        _ => false,
    }
}

/// Keeps an open list under its line, or closes it once the line is out of view.
fn follow() {
    if with(|list| list.choices.is_empty() || place(list)) == Some(false) {
        hide();
    }
}

/// Closes the list.
pub(crate) fn hide() {
    with(|list| {
        let was_open = !list.choices.is_empty();
        list.choices.clear();
        list.pressing = false;
        let _ = list.list.set_attribute("hidden", "");
        let _ = list.area.remove_attribute("aria-activedescendant");
        if was_open {
            list.status.set_text_content(Some("Suggestions closed."));
        }
    });
}

fn offer(asked: bool) {
    let Some(area) = with(|list| list.area.clone()) else {
        return;
    };
    let caret = area.selection_start().ok().flatten();
    let collapsed = caret == area.selection_end().ok().flatten();
    let (source, start) = with(|list| list.source.as_ref().map(|source| source()))
        .flatten()
        .unwrap_or_else(|| (area.value(), 0));
    let visible_len = area.value().encode_utf16().count();
    let full_caret = caret.map(|caret| caret as usize + start);
    let choices = match full_caret {
        Some(caret) if collapsed => at(
            &source,
            caret,
            &super::homepage::completion_options(),
            asked,
        )
        .into_iter()
        .filter_map(|mut choice| {
            if choice.start < start || choice.end > start + visible_len {
                return None;
            }
            choice.start -= start;
            choice.end -= start;
            Some(choice)
        })
        .collect::<Vec<_>>(),
        _ => vec![],
    };
    if !asked
        && caret.is_some_and(|caret| {
            choices
                .first()
                .is_some_and(|choice| choice.start == caret as usize)
                && bare_fence(&source, caret as usize + start)
        })
    {
        hide();
        return;
    }
    if choices.is_empty() {
        hide();
        return;
    }
    with(|list| {
        let was_open = !list.choices.is_empty();
        // The highlighted choice stays highlighted while it still matches.
        let kept = list.choices.get(list.active).map(|c| c.label.clone());
        list.active = kept
            .and_then(|label| choices.iter().position(|c| c.label == label))
            .unwrap_or(0);
        list.choices = choices;
        list.list.set_text_content(None);
        for (index, choice) in list.choices.iter().enumerate() {
            let Ok(option) = document().create_element("li") else {
                continue;
            };
            let _ = option.set_attribute("role", "option");
            let _ = option.set_attribute("id", &format!("{LIST_ID}-{index}"));
            let _ = option.set_attribute("data-index", &index.to_string());
            option.set_text_content(Some(&choice.label));
            let _ = list.list.append_child(&option);
        }
        let _ = list.list.remove_attribute("hidden");
        if !was_open {
            list.status.set_text_content(Some("Suggestions available."));
        }
        place(list);
        mark(list);
    });
}

/// A fence without a tag stays plain unless the reader explicitly asks for completions.
fn bare_fence(source: &str, caret: usize) -> bool {
    let (line, character) = crate::completion::position(source, caret);
    let current = source.split('\n').nth(line).unwrap_or_default();
    let before =
        String::from_utf16_lossy(&current.encode_utf16().take(character).collect::<Vec<_>>());
    before.trim_start() == "```"
}

/// Marks the highlighted choice, names it to assistive technology and keeps it in view.
fn mark(list: &List) {
    let Ok(options) = list.list.query_selector_all("[role=option]") else {
        return;
    };
    for index in 0..options.length() {
        if let Some(option) = options
            .item(index)
            .map(|node| node.unchecked_into::<Element>())
        {
            let active = index as usize == list.active;
            let _ = option.set_attribute("aria-selected", if active { "true" } else { "false" });
            if active {
                let _ = list
                    .area
                    .set_attribute("aria-activedescendant", &option.id());
                let option: &HtmlElement = option.unchecked_ref();
                let (top, height) = (option.offset_top(), option.offset_height());
                let (scrolled, view) = (list.list.scroll_top(), list.list.client_height());
                if top < scrolled {
                    list.list.set_scroll_top(top);
                } else if top + height > scrolled + view {
                    list.list.set_scroll_top(top + height - view);
                }
            }
        }
    }
}

/// Puts the list under the caret's line, or above it where the editor has no room below, and
/// says whether that line is in view.
fn place(list: &mut List) -> bool {
    let area = &list.area;
    let Some(caret) = area.selection_start().ok().flatten() else {
        return false;
    };
    // A mirror uses the browser's own wrapping, tabs and glyph metrics to locate the caret.
    let Some(code) = area.parent_element() else {
        return false;
    };
    let Ok(mirror) = document().create_element("div") else {
        return false;
    };
    mirror.set_class_name("code__mirror");
    let value = area.value();
    let prefix = String::from_utf16_lossy(
        &value
            .encode_utf16()
            .take(caret as usize)
            .collect::<Vec<_>>(),
    );
    mirror.set_text_content(Some(&prefix));
    let Ok(marker) = document().create_element("span") else {
        return false;
    };
    marker.set_text_content(Some("\u{200b}"));
    let _ = mirror.append_child(&marker);
    let _ = code.append_child(&mirror);
    let marker_rect = marker.get_bounding_client_rect();
    let mirror_rect = mirror.get_bounding_client_rect();
    let x =
        marker_rect.left() - mirror_rect.left() - f64::from(area.scroll_left()) - inset(&list.list);
    let line_top = marker_rect.top() - mirror_rect.top() - f64::from(area.scroll_top());
    mirror.remove();
    let line_height = px(area, "line-height").max(1.0);
    let height = f64::from(list.list.offset_height());
    let width = f64::from(list.list.offset_width());
    let room = f64::from(area.client_height());
    let below = line_top + line_height + GAP;
    let top = if below + height > room && line_top - GAP - height >= 0.0 {
        line_top - GAP - height
    } else {
        below
    };
    let left = x.min(f64::from(area.client_width()) - width - 8.0).max(4.0);
    style(&list.list, "left", &format!("{left:.1}px"));
    style(&list.list, "top", &format!("{top:.1}px"));
    line_top + line_height > 0.0 && line_top < room
}

/// Puts a choice in place of the word it completes, as typing would, so the browser's undo
/// takes it back and the page hears an input; a key's choice ends with `: `, which opens the
/// list of its values next.
fn accept(index: usize) {
    let Some((area, choice)) = with(|list| (list.area.clone(), list.choices.get(index).cloned()))
    else {
        return;
    };
    let Some(choice) = choice else {
        return;
    };
    hide();
    put(
        &area,
        choice.start as u32,
        choice.end as u32,
        &choice.insert,
    );
}

/// Puts `text` in place of the text area's stretch `start..end` and leaves the caret after it, as
/// typing would, so the browser's undo takes it back and the page hears an input.
pub(crate) fn put(area: &HtmlTextAreaElement, start: u32, end: u32, text: &str) {
    let _ = area.focus();
    let _ = area.set_selection_range(start, end);
    let inserted = document()
        .dyn_ref::<HtmlDocument>()
        .and_then(|page| {
            page.exec_command_with_show_ui_and_value("insertText", false, text)
                .ok()
        })
        .unwrap_or(false);
    if !inserted {
        let _ = area.set_range_text_with_start_and_end_and_mode(text, start, end, "end");
        if let Ok(input) = InputEvent::new("input") {
            let _ = area.dispatch_event(&input);
        }
    }
}

/// How far an option's text sits from the list's outer left edge, in CSS pixels.
fn inset(list: &HtmlElement) -> f64 {
    let Some(option) = list.query_selector("[role=option]").ok().flatten() else {
        return 0.0;
    };
    let left = list.client_left() + option.unchecked_ref::<HtmlElement>().offset_left();
    f64::from(left) + px(&option, "padding-left")
}

/// A length the stylesheet resolved for an element, in CSS pixels.
fn px(element: &Element, property: &str) -> f64 {
    window()
        .get_computed_style(element)
        .ok()
        .flatten()
        .and_then(|declaration| declaration.get_property_value(property).ok())
        .and_then(|value| value.trim_end_matches("px").parse().ok())
        .unwrap_or(0.0)
}
