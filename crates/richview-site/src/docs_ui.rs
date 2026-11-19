//! Small documentation controls: copy, feedback drafts, static style previews and search.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Element, Event, HtmlInputElement, HtmlTextAreaElement};
type EventHandler = Closure<dyn FnMut(Event)>;
thread_local! { static LISTENERS: RefCell<Vec<EventHandler>> = const { RefCell::new(Vec::new()) }; }
fn window() -> web_sys::Window {
    web_sys::window().expect("browser")
}
fn document() -> web_sys::Document {
    window().document().expect("document")
}
fn query(selector: &str) -> Option<Element> {
    document().query_selector(selector).ok().flatten()
}
fn js_error(value: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&value.to_string())
}
fn on(target: &web_sys::EventTarget, name: &str, handler: impl FnMut(Event) + 'static) {
    let handler = Closure::wrap(Box::new(handler) as Box<dyn FnMut(Event)>);
    let _ = target.add_event_listener_with_callback(name, handler.as_ref().unchecked_ref());
    LISTENERS.with(|list| list.borrow_mut().push(handler));
}
fn bind_style_links() {
    let Ok(links) = document().query_selector_all("[data-style-link]") else {
        return;
    };
    for index in 0..links.length() {
        let Some(link) = links
            .item(index)
            .and_then(|node| node.dyn_into::<Element>().ok())
        else {
            continue;
        };
        let selected = link.clone();
        on(&link.into(), "click", move |event| {
            let Some(url) = selected.get_attribute("data-preview-src") else {
                return;
            };
            // Each style row swaps its own frame: a chart page's, or the elements page's type sample.
            let Some(row) = selected.closest(".docs-styles").ok().flatten() else {
                return;
            };
            let Some(frame) = row
                .query_selector(".docs-styles__frame")
                .ok()
                .flatten()
                .and_then(|node| node.dyn_into::<web_sys::HtmlIFrameElement>().ok())
            else {
                return;
            };
            event.prevent_default();
            frame.set_src(&url);
            let _ = frame.remove_attribute("hidden");
            if let Ok(Some(open)) = row.query_selector("[data-style-open]") {
                let _ = open.set_attribute("href", &url);
            }
            if let Ok(all) = row.query_selector_all("[data-style-link]") {
                for i in 0..all.length() {
                    if let Some(node) = all.item(i).and_then(|node| node.dyn_into::<Element>().ok())
                    {
                        let _ = node.remove_attribute("aria-current");
                    }
                }
            }
            let _ = selected.set_attribute("aria-current", "true");
        });
    }
}
/// The elements overview's one live frame, the typography sample, is laid out at a fixed width and
/// scaled to its card; every other gallery card is a picture the build draws.
fn scale_thumbnails() {
    let Ok(frames) = document().query_selector_all("[data-docs-scale]") else {
        return;
    };
    for i in 0..frames.length() {
        if let Some(frame) = frames
            .item(i)
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
            && let Some(parent) = frame.parent_element()
        {
            let scale = f64::from(parent.client_width()) / 1000.0;
            let _ = frame
                .style()
                .set_property("transform", &format!("scale({scale})"));
            // The frame shows once scaled.
            let _ = frame.set_attribute("data-docs-scaled", "");
        }
    }
}
fn bind_thumbnail_scales() {
    scale_thumbnails();
    on(&window().into(), "resize", |_| scale_thumbnails());
}

fn bind_search() {
    let Some(input) =
        query("[data-docs-search]").and_then(|node| node.dyn_into::<HtmlInputElement>().ok())
    else {
        return;
    };
    let entries = Rc::new(RefCell::new(None::<Vec<serde_json::Value>>));
    let generation = Rc::new(Cell::new(0_u64));
    let field = input.clone();
    on(&input.clone().into(), "input", move |_| {
        generation.set(generation.get().wrapping_add(1));
        let token = generation.get();
        let generation = generation.clone();
        let entries = entries.clone();
        let term = field.value().to_lowercase();
        spawn_local(async move {
            let Some(results) = query("[data-docs-search-results]") else {
                return;
            };
            results.set_text_content(None);
            if term.trim().len() < 2 {
                return;
            }
            if entries.borrow().is_none() {
                let loaded = async {
                    let response = JsFuture::from(window().fetch_with_str("/docs/search.json"))
                        .await?
                        .dyn_into::<web_sys::Response>()?;
                    if !response.ok() {
                        return Err(js_error("Search unavailable"));
                    }
                    let text = JsFuture::from(response.text()?)
                        .await?
                        .as_string()
                        .unwrap_or_default();
                    serde_json::from_str::<Vec<serde_json::Value>>(&text).map_err(js_error)
                }
                .await;
                match loaded {
                    Ok(found) => *entries.borrow_mut() = Some(found),
                    Err(_) => {
                        results.set_text_content(Some(
                            "Search is unavailable. Use the guide and reference navigation below.",
                        ));
                        return;
                    }
                }
            }
            if generation.get() != token {
                return;
            }
            let data = entries.borrow();
            let Some(data) = data.as_ref() else { return };
            let words: Vec<_> = term.split_whitespace().collect();
            let mut found: Vec<_> = data
                .iter()
                .filter(|entry| {
                    words.iter().all(|word| {
                        entry["title"]
                            .as_str()
                            .unwrap_or_default()
                            .to_lowercase()
                            .contains(word)
                    })
                })
                .collect();
            found.sort_by_key(|entry| {
                (
                    !entry["text"]
                        .as_str()
                        .unwrap_or_default()
                        .to_lowercase()
                        .starts_with(term.trim()),
                    entry["title"].as_str().unwrap_or_default().len(),
                )
            });
            for entry in found.iter().take(12) {
                let Some(url) = entry["url"]
                    .as_str()
                    .filter(|url| url.starts_with("/docs/"))
                else {
                    continue;
                };
                let Ok(row) = document().create_element("li") else {
                    continue;
                };
                let Ok(link) = document().create_element("a") else {
                    continue;
                };
                let _ = link.set_attribute("href", url);
                link.set_text_content(entry["title"].as_str());
                let _ = row.append_child(&link);
                let _ = results.append_child(&row);
            }
            if found.is_empty() {
                results.set_text_content(Some("No matching field, chart or guide."));
            }
        });
    });
    on(&document().into(), "keydown", move |event| {
        let Ok(key) = event.dyn_into::<web_sys::KeyboardEvent>() else {
            return;
        };
        if key.key().eq_ignore_ascii_case("k") && (key.meta_key() || key.ctrl_key()) {
            key.prevent_default();
            let _ = input.focus();
        }
        if key.key() == "Escape"
            && let Some(results) = query("[data-docs-search-results]")
        {
            results.set_text_content(None);
        }
    });
}

/// Whether a link moves the reader within the page at `pathname`: it names a part of the page,
/// with no address before the `#` or with this page's own (clean, or as the file is named).
fn stays_on_page(href: &str, pathname: &str) -> bool {
    let Some((path, part)) = href.split_once('#') else {
        return false;
    };
    let page = |path: &str| {
        let path = path.strip_suffix(".html").unwrap_or(path);
        path.strip_suffix('/').unwrap_or(path).to_owned()
    };
    !part.is_empty() && (path.is_empty() || page(path) == page(pathname))
}

/// On a phone the documentation menu opens over the page. An entry that moves within the page
/// closes it, so the reader sees where the tap took them; an entry that opens another page
/// leaves with the page.
fn bind_menu() {
    let Some(menu) = query(".docs-mobile-menu") else {
        return;
    };
    let opened = menu.clone();
    on(&menu.into(), "click", move |event| {
        let link = event
            .target()
            .and_then(|target| target.dyn_into::<Element>().ok())
            .and_then(|target| target.closest("a[href]").ok().flatten());
        let Some(href) = link.and_then(|link| link.get_attribute("href")) else {
            return;
        };
        let here = window().location().pathname().unwrap_or_default();
        if stays_on_page(&href, &here) {
            let _ = opened.remove_attribute("open");
        }
    });
}

pub(crate) fn start() -> Result<(), JsValue> {
    bind_menu();
    bind_style_links();
    bind_search();
    bind_thumbnail_scales();
    bind_feedback()?;
    let Ok(buttons) = document().query_selector_all("[data-copy-source], [data-copy-code]") else {
        return Ok(());
    };
    for i in 0..buttons.length() {
        let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) else {
            continue;
        };
        let label = button.text_content().unwrap_or_default();
        let target = button.clone();
        on(&button.into(), "click", move |_| {
            let code = target
                .closest("figure.docs-code")
                .ok()
                .flatten()
                .and_then(|figure| figure.query_selector("code").ok().flatten())
                .or_else(|| query("figure.docs-code code.language-rmd"));
            let Some(code) = code else {
                return;
            };
            let text = code.text_content().unwrap_or_default();
            let copied = window().navigator().clipboard().write_text(&text);
            let restore = target.clone();
            let label = label.clone();
            spawn_local(async move {
                restore.set_text_content(Some(if JsFuture::from(copied).await.is_ok() {
                    "Copied"
                } else {
                    "Select and copy"
                }));
                let reset = Closure::once_into_js(move || restore.set_text_content(Some(&label)));
                let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(
                    reset.unchecked_ref(),
                    1500,
                );
            });
        });
    }
    Ok(())
}

fn field(form: &Element, selector: &str) -> Option<HtmlInputElement> {
    form.query_selector(selector)
        .ok()
        .flatten()?
        .dyn_into()
        .ok()
}

fn feedback_message(form: &Element) -> Option<HtmlTextAreaElement> {
    form.query_selector("[data-feedback-message]")
        .ok()
        .flatten()?
        .dyn_into()
        .ok()
}

fn feedback_payload(form: &Element) -> serde_json::Value {
    serde_json::json!({
        "page_url": field(form, "[data-feedback-page]").map(|input| input.value()).unwrap_or_default(),
        "message": feedback_message(form).map(|input| input.value().trim().to_owned()).unwrap_or_default(),
        "email": field(form, "[data-feedback-email]").map(|input| input.value().trim().to_owned()).unwrap_or_default(),
        "website": field(form, "[data-feedback-website]").map(|input| input.value()).unwrap_or_default(),
    })
}

fn sync_feedback_draft(form: &Element) {
    let payload = feedback_payload(form);
    if let Some(download) = query("[data-feedback-download]") {
        if payload["message"]
            .as_str()
            .is_some_and(|message| message.chars().count() >= 5)
        {
            let encoded: String = js_sys::encode_uri_component(&payload.to_string()).into();
            let _ = download.set_attribute(
                "href",
                &format!("data:application/json;charset=utf-8,{encoded}"),
            );
            let _ = download.remove_attribute("hidden");
        } else {
            let _ = download.set_attribute("hidden", "");
        }
    }
    if let Some(issue) = query("[data-feedback-issue]") {
        let body = format!(
            "Page: {}\n\n{}",
            payload["page_url"].as_str().unwrap_or_default(),
            payload["message"].as_str().unwrap_or_default()
        );
        let encoded: String = js_sys::encode_uri_component(&body).into();
        if let Some(base) = issue.get_attribute("data-feedback-issue-base") {
            let _ = issue.set_attribute("href", &format!("{base}&body={encoded}"));
        }
    }
}

async fn send_feedback(payload: String) -> Result<(), JsValue> {
    let request = web_sys::RequestInit::new();
    request.set_method("POST");
    let headers = web_sys::Headers::new()?;
    headers.set("Content-Type", "application/json")?;
    request.set_headers(&headers);
    request.set_body(&JsValue::from_str(&payload));
    let controller = web_sys::AbortController::new()?;
    request.set_signal(Some(&controller.signal()));
    let timed = controller.clone();
    let timeout = Closure::<dyn FnMut()>::new(move || timed.abort());
    let timer = window().set_timeout_with_callback_and_timeout_and_arguments_0(
        timeout.as_ref().unchecked_ref(),
        8000,
    )?;
    let result = async {
        let response = JsFuture::from(window().fetch_with_str_and_init("/api/feedback", &request))
            .await?
            .dyn_into::<web_sys::Response>()?;
        let text = JsFuture::from(response.text()?)
            .await?
            .as_string()
            .unwrap_or_default();
        let body: serde_json::Value =
            serde_json::from_str(&text).map_err(|_| js_error("Feedback service unavailable"))?;
        if response.ok() && body["ok"] == true {
            Ok(())
        } else {
            Err(js_error("Feedback was not acknowledged"))
        }
    }
    .await;
    window().clear_timeout_with_handle(timer);
    result
}

fn bind_feedback() -> Result<(), JsValue> {
    let Some(form) = query("[data-docs-feedback]") else {
        return Ok(());
    };
    let location = window().location();
    let parameters = web_sys::UrlSearchParams::new_with_str(&location.search()?)?;
    let page = parameters
        .get("feedback_for")
        .filter(|path| path.starts_with("/docs/") && !path.starts_with("//"))
        .map(|path| format!("{}{path}", location.origin().unwrap_or_default()))
        .unwrap_or(
            location
                .href()?
                .split('#')
                .next()
                .unwrap_or_default()
                .into(),
        );
    if let Some(input) = field(&form, "[data-feedback-page]") {
        input.set_value(&page);
    }
    form.remove_attribute("hidden")?;
    sync_feedback_draft(&form);
    let edited = form.clone();
    on(&form.clone().into(), "input", move |_| {
        sync_feedback_draft(&edited)
    });
    let pending = Rc::new(Cell::new(false));
    let submitted = form.clone();
    on(&form.into(), "submit", move |event| {
        event.prevent_default();
        if pending.get()
            || feedback_message(&submitted).is_none_or(|message| {
                !message.check_validity() || message.value().trim().chars().count() < 5
            })
            || field(&submitted, "[data-feedback-email]")
                .is_some_and(|email| !email.check_validity())
        {
            return;
        }
        if !submitted.has_attribute("data-feedback-enabled") {
            return;
        }
        let payload = feedback_payload(&submitted).to_string();
        pending.set(true);
        if let Some(message) = feedback_message(&submitted) {
            message.set_read_only(true);
        }
        if let Some(email) = field(&submitted, "[data-feedback-email]") {
            email.set_read_only(true);
        }
        let button = submitted
            .query_selector("button[type=submit]")
            .ok()
            .flatten();
        if let Some(button) = &button {
            let _ = button.set_attribute("disabled", "");
        }
        if let Some(status) = query("[data-feedback-status]") {
            status.set_text_content(Some("Sending feedback… Your note remains available here."));
        }
        let pending = pending.clone();
        let completed = submitted.clone();
        spawn_local(async move {
            let sent = send_feedback(payload).await.is_ok();
            if let Some(status) = query("[data-feedback-status]") {
                status.set_text_content(Some(if sent {
                    "Thank you. Your feedback was received."
                } else {
                    "Feedback was not sent. Download your note; your message is still here."
                }));
            }
            pending.set(false);
            if let Some(message) = feedback_message(&completed) {
                message.set_read_only(false);
            }
            if let Some(email) = field(&completed, "[data-feedback-email]") {
                email.set_read_only(false);
            }
            if let Some(button) = button {
                let _ = button.remove_attribute("disabled");
            }
        });
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::stays_on_page;

    #[test]
    fn a_link_to_a_part_of_this_page_stays_on_it() {
        assert!(stays_on_page("#sankey-diagram", "/docs/charts"));
        assert!(stays_on_page("/docs/elements#tables", "/docs/elements"));
        assert!(stays_on_page(
            "/docs/elements#tables",
            "/docs/elements.html"
        ));
        assert!(stays_on_page("/docs/elements#tables", "/docs/elements/"));
        // Another page's part, a page of its own and a bare `#` leave or do nothing.
        assert!(!stays_on_page(
            "/docs/elements#tables",
            "/docs/charts/pivot"
        ));
        assert!(!stays_on_page(
            "/docs/reference#chart-specific-fields",
            "/docs/"
        ));
        assert!(!stays_on_page("/docs/charts/flow", "/docs/charts/flow"));
        assert!(!stays_on_page("#", "/docs/charts"));
    }
}
