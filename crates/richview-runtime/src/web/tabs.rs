//! Tab controller for `frontmatter.tabs` reports (FEA-BR-08 to FEA-BR-10, FEA-BR-40). The static
//! HTML stacks every panel under its label, so a reader without scripts misses nothing. Once this
//! runtime runs it builds the tab row in the place of the first panel's label, hides every panel
//! but the active one, and adds deep links, keyboard navigation and visibility events.
//!
//! Keeps panels in the DOM so scroll position, hover readouts and chart runtime state survive
//! tab switches. Dispatches `rv:panel-hidden` and `rv:panel-shown` on each `.rv-chart`
//! descendant and, when present, calls their `pause()`/`resume()` methods. This is the small
//! shared contract with chart playback controls; absent methods are a no-op, so a chart without
//! playback controls keeps its current behaviour.

use super::{document, el, on, query_all, window};
use js_sys::{Function, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CustomEvent, CustomEventInit, Element, HtmlElement, KeyboardEvent};

const TAB_SELECTOR: &str = ".rv-tabs__tab[data-rv-tab]";

/// The tab row, built from the panels the static HTML carries; none exists before this runs.
fn build_row() -> bool {
    let panels = query_all(".rv-tabpanel[data-rv-tab]");
    let Some(first) = panels.first() else {
        return false;
    };
    if document()
        .query_selector(".rv-tabs[role='tablist']")
        .ok()
        .flatten()
        .is_some()
    {
        return true;
    }
    let row = el("div", "rv-tabs");
    let _ = row.set_attribute("role", "tablist");
    let _ = row.set_attribute("aria-label", "Report views");
    for panel in &panels {
        let id = panel.get_attribute("data-rv-tab").unwrap_or_default();
        let label = panel.get_attribute("data-rv-tab-label").unwrap_or_default();
        let tab = el("button", "rv-tabs__tab");
        let _ = tab.set_attribute("type", "button");
        let _ = tab.set_attribute("role", "tab");
        let _ = tab.set_attribute("id", &format!("tab-{id}"));
        let _ = tab.set_attribute("aria-controls", &format!("panel-{id}"));
        let _ = tab.set_attribute("aria-selected", "false");
        let _ = tab.set_attribute("tabindex", "-1");
        let _ = tab.set_attribute("data-rv-tab", &id);
        tab.set_text_content(Some(&label));
        let _ = row.append_child(&tab);
        let _ = panel.set_attribute("role", "tabpanel");
        let _ = panel.set_attribute("aria-labelledby", &format!("tab-{id}"));
    }
    if let Some(parent) = first.parent_node() {
        let _ = parent.insert_before(&row, Some(first));
    }
    true
}

pub(crate) fn start() -> Result<(), JsValue> {
    if !build_row() {
        return Ok(());
    }
    let doc = document();
    let tabs_nodes = doc.query_selector_all(TAB_SELECTOR)?;
    if tabs_nodes.length() == 0 {
        return Ok(());
    }
    let ids: Vec<String> = (0..tabs_nodes.length())
        .filter_map(|i| tabs_nodes.item(i))
        .filter_map(|n| n.dyn_into::<Element>().ok())
        .filter_map(|e| e.get_attribute("data-rv-tab"))
        .collect();
    if ids.is_empty() {
        return Ok(());
    }

    for i in 0..tabs_nodes.length() {
        let Some(tab) = tabs_nodes
            .item(i)
            .and_then(|n| n.dyn_into::<Element>().ok())
        else {
            continue;
        };
        let Some(id) = tab.get_attribute("data-rv-tab") else {
            continue;
        };
        let activate_id = id.clone();
        let ids_for_click = ids.clone();
        on(&tab.clone().into(), "click", move |event| {
            event.prevent_default();
            activate(&activate_id, &ids_for_click, true);
        });
        let ids_for_key = ids.clone();
        let own_id = id.clone();
        on(&tab.into(), "keydown", move |event| {
            let Ok(key_event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            if key_event.ctrl_key() || key_event.meta_key() || key_event.alt_key() {
                return;
            }
            match key_event.key().as_str() {
                "ArrowRight" => {
                    key_event.prevent_default();
                    let next = step(&own_id, &ids_for_key, 1);
                    activate(&next, &ids_for_key, true);
                    focus_tab(&next);
                }
                "ArrowLeft" => {
                    key_event.prevent_default();
                    let next = step(&own_id, &ids_for_key, -1);
                    activate(&next, &ids_for_key, true);
                    focus_tab(&next);
                }
                "Home" => {
                    key_event.prevent_default();
                    if let Some(first) = ids_for_key.first() {
                        activate(first, &ids_for_key, true);
                        focus_tab(first);
                    }
                }
                "End" => {
                    key_event.prevent_default();
                    if let Some(last) = ids_for_key.last() {
                        activate(last, &ids_for_key, true);
                        focus_tab(last);
                    }
                }
                "Enter" | " " | "Spacebar" => {
                    key_event.prevent_default();
                    activate(&own_id, &ids_for_key, true);
                }
                _ => {}
            }
        });
    }

    // Deep link resolution: `#tab=<id>` wins over the server-side active panel. Fall back to
    // whatever button was marked `aria-selected="true"` in the rendered HTML.
    let initial = parse_hash_tab(&ids)
        .or_else(|| server_active_id(&ids))
        .or_else(|| ids.first().cloned());
    if let Some(id) = initial {
        activate(&id, &ids, false);
    }
    // A fragment that names something inside a panel opens that panel (FEA-BR-40).
    reveal_fragment(&ids);
    let ids_for_links = ids.clone();
    on(&doc.clone().into(), "click", move |event| {
        let Some(link) = event
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest("a[href^='#']").ok().flatten())
        else {
            return;
        };
        let href = link.get_attribute("href").unwrap_or_default();
        let fragment = decode(href.trim_start_matches('#'));
        if fragment.starts_with("tab=") || fragment.is_empty() {
            return;
        }
        if hidden_target(&fragment).is_some() {
            event.prevent_default();
            if let Ok(history) = window().history() {
                let base = window().location().href().unwrap_or_default();
                let base = base.split('#').next().unwrap_or("").to_owned();
                let _ = history.push_state_with_url(
                    &JsValue::NULL,
                    "",
                    Some(&format!("{base}#{}", href.trim_start_matches('#'))),
                );
            }
            reveal(&fragment, &ids_for_links);
        }
    });
    // Print keeps what `export: active` promises: name the views left out.
    let target: web_sys::EventTarget = window().into();
    on(&target, "beforeprint", |_| export_note());

    // Follow back/forward: if the fragment names a known tab, switch to it without pushing.
    let ids_for_hash = ids.clone();
    on(&window().into(), "hashchange", move |_| {
        if let Some(id) = parse_hash_tab(&ids_for_hash) {
            activate(&id, &ids_for_hash, false);
        } else {
            reveal_fragment(&ids_for_hash);
        }
    });
    Ok(())
}

fn parse_hash_tab(ids: &[String]) -> Option<String> {
    let hash = window().location().hash().ok()?;
    let fragment = hash.trim_start_matches('#');
    let id = fragment.strip_prefix("tab=")?;
    ids.iter().find(|known| *known == id).cloned()
}

/// The panel the report names as its default.
fn server_active_id(ids: &[String]) -> Option<String> {
    let panel = document()
        .query_selector(".rv-tabpanel[data-rv-default]")
        .ok()
        .flatten()?;
    let id = panel.get_attribute("data-rv-tab")?;
    ids.iter().find(|known| *known == &id).cloned()
}

fn decode(fragment: &str) -> String {
    js_sys::decode_uri_component(fragment)
        .ok()
        .and_then(|s| s.as_string())
        .unwrap_or_else(|| fragment.to_owned())
}

/// The element a fragment names when it sits in a panel that is hidden now.
fn hidden_target(fragment: &str) -> Option<Element> {
    let target = document().get_element_by_id(fragment)?;
    let panel = target.closest(".rv-tabpanel[hidden]").ok().flatten()?;
    panel.get_attribute("data-rv-tab").map(|_| target)
}

/// Open the panel that holds `fragment`'s element, then scroll to it and move focus there.
fn reveal(fragment: &str, ids: &[String]) {
    let Some(target) = document().get_element_by_id(fragment) else {
        return;
    };
    if let Ok(Some(panel)) = target.closest(".rv-tabpanel[data-rv-tab]")
        && let Some(id) = panel.get_attribute("data-rv-tab")
        && panel.has_attribute("hidden")
    {
        activate(&id, ids, false);
    }
    if !target.has_attribute("tabindex") && !matches!(target.tag_name().as_str(), "A" | "BUTTON") {
        let _ = target.set_attribute("tabindex", "-1");
    }
    target.scroll_into_view();
    if let Ok(html) = target.dyn_into::<HtmlElement>() {
        let _ = html.focus();
    }
}

fn reveal_fragment(ids: &[String]) {
    let hash = window().location().hash().unwrap_or_default();
    let fragment = decode(hash.trim_start_matches('#'));
    if !fragment.is_empty() && !fragment.starts_with("tab=") && hidden_target(&fragment).is_some() {
        reveal(&fragment, ids);
    }
}

/// Fill the print-only note of a report that exports only the active view.
fn export_note() {
    let Some(note) = document()
        .query_selector(".rv-tabs__export-note")
        .ok()
        .flatten()
    else {
        return;
    };
    let panels = query_all(".rv-tabpanel[data-rv-tab]");
    let label = |p: &Element| p.get_attribute("data-rv-tab-label").unwrap_or_default();
    let (shown, left): (Vec<_>, Vec<_>) = panels.iter().partition(|p| !p.has_attribute("hidden"));
    let names: Vec<String> = left.iter().map(|p| label(p)).collect();
    let list = match names.as_slice() {
        [] => return,
        [one] => one.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    };
    let verb = if names.len() == 1 { "is" } else { "are" };
    note.set_text_content(Some(&format!(
        "Showing {}, 1 of {} views. {list} {verb} in the live report.",
        shown.first().map(|p| label(p)).unwrap_or_default(),
        panels.len()
    )));
}

fn step(current: &str, ids: &[String], delta: i32) -> String {
    let n = ids.len() as i32;
    if n == 0 {
        return current.to_owned();
    }
    let index = ids
        .iter()
        .position(|id| id == current)
        .map(|i| i as i32)
        .unwrap_or(0);
    let next = ((index + delta) % n + n) % n;
    ids[next as usize].clone()
}

fn focus_tab(id: &str) {
    if let Some(tab) = tab_element(id)
        && let Ok(html) = tab.dyn_into::<HtmlElement>()
    {
        let _ = html.focus();
    }
}

fn activate(id: &str, ids: &[String], push_history: bool) {
    for known in ids {
        let selected = known == id;
        if let Some(tab) = tab_element(known) {
            let _ = tab.set_attribute("aria-selected", if selected { "true" } else { "false" });
            let _ = tab.set_attribute("tabindex", if selected { "0" } else { "-1" });
        }
        if let Some(panel) = panel_element(known) {
            let was_hidden = panel.has_attribute("hidden");
            if selected {
                let _ = panel.remove_attribute("hidden");
                if was_hidden {
                    notify_panel(&panel, "rv:panel-shown", "resume");
                }
            } else {
                let _ = panel.set_attribute("hidden", "");
                if !was_hidden {
                    notify_panel(&panel, "rv:panel-hidden", "pause");
                }
            }
        }
    }
    super::host::refresh();
    if push_history {
        let new_hash = format!("#tab={id}");
        let history = window().history();
        if let Ok(history) = history {
            let href = window().location().href().unwrap_or_default();
            // Replace-state keeps the back button useful for prior navigations instead of
            // filling it with tab switches.
            let base = href.split('#').next().unwrap_or("").to_owned();
            let _ = history.replace_state_with_url(
                &JsValue::NULL,
                "",
                Some(&format!("{base}{new_hash}")),
            );
        }
    }
}

fn tab_element(id: &str) -> Option<Element> {
    document()
        .query_selector(&format!(".rv-tabs__tab[data-rv-tab='{id}']"))
        .ok()
        .flatten()
}

fn panel_element(id: &str) -> Option<Element> {
    document().get_element_by_id(&format!("panel-{id}"))
}

fn notify_panel(panel: &Element, event_name: &str, method: &str) {
    let init = CustomEventInit::new();
    init.set_bubbles(true);
    if let Ok(nodes) = panel.query_selector_all(".rv-chart") {
        for i in 0..nodes.length() {
            let Some(node) = nodes.item(i) else { continue };
            let Ok(element) = node.dyn_into::<Element>() else {
                continue;
            };
            if let Ok(ev) = CustomEvent::new_with_event_init_dict(event_name, &init) {
                let _ = element.dispatch_event(&ev);
            }
            // Call the optional playback method if a playback control attached one.
            if let Ok(func) = Reflect::get(&element, &JsValue::from_str(method))
                && let Ok(func) = func.dyn_into::<Function>()
            {
                let _ = func.call0(&element);
            }
        }
    }
    // Also surface a panel-level event so non-chart playable widgets can hook in.
    if let Ok(ev) = CustomEvent::new_with_event_init_dict(event_name, &init) {
        let _ = panel.dispatch_event(&ev);
    }
}
