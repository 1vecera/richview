//! Tab controller for `frontmatter.tabs`-driven report views. The interactive report HTML
//! already carries the tab list and panel sections; this module adds deep links, keyboard
//! navigation and visibility events.
//!
//! Keeps panels in the DOM so scroll position, hover readouts and chart runtime state survive
//! tab switches. Dispatches `rv:panel-hidden` and `rv:panel-shown` on each `.rv-chart`
//! descendant and, when present, calls their `pause()`/`resume()` methods. This is the small
//! shared contract with chart playback controls; absent methods are a no-op, so a chart without
//! playback controls keeps its current behaviour.

use super::{document, on, query, window};
use js_sys::{Function, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{CustomEvent, CustomEventInit, Element, HtmlElement, KeyboardEvent};

const TAB_SELECTOR: &str = ".rv-tabs__tab[data-rv-tab]";

pub(crate) fn start() -> Result<(), JsValue> {
    if query(".rv-tabs[role='tablist']").is_none() {
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
    let initial = parse_hash_tab(&ids).or_else(|| server_active_id(&ids));
    if let Some(id) = initial {
        activate(&id, &ids, false);
    }

    // Follow back/forward: if the fragment names a known tab, switch to it without pushing.
    let ids_for_hash = ids.clone();
    on(&window().into(), "hashchange", move |_| {
        if let Some(id) = parse_hash_tab(&ids_for_hash) {
            activate(&id, &ids_for_hash, false);
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

fn server_active_id(ids: &[String]) -> Option<String> {
    let selected = document()
        .query_selector(".rv-tabs__tab[aria-selected='true']")
        .ok()
        .flatten()?;
    let id = selected.get_attribute("data-rv-tab")?;
    ids.iter().find(|known| *known == &id).cloned()
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
