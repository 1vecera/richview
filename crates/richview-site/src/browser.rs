use serde_json::Value;
use std::cell::RefCell;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, Event, HtmlElement, Response};

mod authoring;
mod code;
mod complete;
mod docs;
mod homepage;
mod landing;
mod tabs;
mod toolbar;
mod voices;
mod writing;

pub(crate) type EventHandler = Closure<dyn FnMut(Event)>;
thread_local! {pub(crate) static LISTENERS:RefCell<Vec<EventHandler>>=const{RefCell::new(vec![])};}
pub(crate) fn window() -> web_sys::Window {
    web_sys::window().expect("browser window")
}
pub(crate) fn document() -> Document {
    window().document().expect("browser document")
}
pub(crate) fn now() -> f64 {
    window().performance().map(|p| p.now()).unwrap_or(0.0)
}
pub(crate) fn element(tag: &str, class: &str, parent: &Element) -> Result<Element, JsValue> {
    let e = document().create_element(tag)?;
    e.set_class_name(class);
    parent.append_child(&e)?;
    Ok(e)
}
pub(crate) fn style(e: &Element, name: &str, value: &str) {
    // These are DOM elements with a style declaration. An instanceof cast against
    // this window's HTMLElement rejects elements in the same-origin report iframe.
    let _ = e
        .unchecked_ref::<HtmlElement>()
        .style()
        .set_property(name, value);
}
pub(crate) fn on(e: &web_sys::EventTarget, event: &str, handler: impl FnMut(Event) + 'static) {
    let c = Closure::wrap(Box::new(handler) as Box<dyn FnMut(Event)>);
    let _ = e.add_event_listener_with_callback(event, c.as_ref().unchecked_ref());
    LISTENERS.with(|l| l.borrow_mut().push(c));
}
pub(crate) async fn fetch_text(url: &str) -> Result<String, JsValue> {
    let resp = wasm_bindgen_futures::JsFuture::from(window().fetch_with_str(url))
        .await?
        .dyn_into::<Response>()?;
    if !resp.ok() {
        return Err(JsValue::from_str(&format!("{} {url}", resp.status())));
    }
    let v = wasm_bindgen_futures::JsFuture::from(resp.text()?).await?;
    Ok(v.as_string().unwrap_or_default())
}
pub(crate) fn query(selector: &str) -> Option<Element> {
    document().query_selector(selector).ok().flatten()
}
/// Bind sandboxed previews to the trusted parent host: their own origin is opaque.
pub(crate) fn preview_runtime() -> String {
    web_sys::Url::new_with_base(
        crate::share::PREVIEW_RUNTIME,
        &window().location().href().unwrap_or_default(),
    )
    .map(|url| url.href())
    .unwrap_or_else(|_| crate::share::PREVIEW_RUNTIME.into())
}

thread_local! {static PREVIEW_WASM:RefCell<Option<js_sys::Promise>>=const{RefCell::new(None)};}
thread_local! {static PREVIEW_JS:RefCell<Option<js_sys::Promise>>=const{RefCell::new(None)};}

/// Cache the exact binding bytes used to pin the external script in each preview.
pub(crate) async fn preview_js() -> Result<String, JsValue> {
    use base64::Engine as _;
    let load = PREVIEW_JS.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| {
                wasm_bindgen_futures::future_to_promise(async {
                    let encoded =
                        authoring::fetch_base64(&format!("{}.js", preview_runtime())).await?;
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .map_err(js_error)?;
                    let js = String::from_utf8(bytes).map_err(js_error)?;
                    if js.is_empty() {
                        return Err(js_error("Preview runtime bindings are empty"));
                    }
                    Ok(JsValue::from_str(&js))
                })
            })
            .clone()
    });
    let result = wasm_bindgen_futures::JsFuture::from(load.clone())
        .await
        .and_then(|value| {
            value
                .as_string()
                .ok_or_else(|| js_error("Preview runtime bindings are unavailable"))
        });
    if result.is_err() {
        PREVIEW_JS.with(|slot| forget_failed(&mut slot.borrow_mut(), &load));
    }
    result
}

/// Cache base64 raw runtime bytes fetched by the parent, so opaque previews need no connection.
pub(crate) async fn preview_wasm() -> Result<String, JsValue> {
    let load = PREVIEW_WASM.with(|slot| {
        slot.borrow_mut()
            .get_or_insert_with(|| {
                wasm_bindgen_futures::future_to_promise(async {
                    let bytes =
                        authoring::fetch_base64(&format!("{}_bg.wasm", preview_runtime())).await?;
                    if !bytes.starts_with("AGFzbQ") {
                        return Err(js_error("Preview runtime response is not WebAssembly"));
                    }
                    Ok(JsValue::from_str(&bytes))
                })
            })
            .clone()
    });
    let result = wasm_bindgen_futures::JsFuture::from(load.clone())
        .await
        .and_then(|value| {
            value
                .as_string()
                .ok_or_else(|| js_error("Preview runtime bytes are unavailable"))
        });
    if result.is_err() {
        PREVIEW_WASM.with(|slot| forget_failed(&mut slot.borrow_mut(), &load));
    }
    result
}

pub(crate) fn error(err: &JsValue) {
    web_sys::console::error_1(err);
}
pub(crate) fn js_error(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}

/// The font catalogs are not compiled into this runtime. A host that renders reports here
/// provides the staged `richview-fonts.css` once, before its first render.
#[wasm_bindgen]
pub fn provide_font_catalog(css: String) -> bool {
    richview_core::provide_font_catalog(css)
}

struct FontCatalogLoads {
    base: Option<js_sys::Promise>,
    math: Option<js_sys::Promise>,
}
thread_local! {static FONT_CATALOG:RefCell<FontCatalogLoads>=const{RefCell::new(FontCatalogLoads {base:None,math:None})};}
/// Load the catalog staged beside the runtime once; every report render on this page awaits it.
/// Without it, reports fall back to system fonts and carry a `style.fonts-unavailable` warning.
pub(crate) async fn font_catalog_ready() {
    let load = FONT_CATALOG.with(|slot| {
        slot.borrow_mut()
            .base
            .get_or_insert_with(|| {
                wasm_bindgen_futures::future_to_promise(async {
                    let css = fetch_text("/richview-fonts.css").await.inspect_err(error)?;
                    Ok(JsValue::from_bool(provide_font_catalog(css)))
                })
            })
            .clone()
    });
    if wasm_bindgen_futures::JsFuture::from(load.clone())
        .await
        .is_err()
    {
        // A failed fetch is asked for again by the next render; a catalog that loaded but did not
        // validate stays as it is.
        FONT_CATALOG.with(|slot| forget_failed(&mut slot.borrow_mut().base, &load));
    }
}

/// Drop the cached `load` after it failed, unless a newer load has replaced it meanwhile.
fn forget_failed(cached: &mut Option<js_sys::Promise>, load: &js_sys::Promise) {
    if cached
        .as_ref()
        .is_some_and(|value| js_sys::Object::is(value.as_ref(), load.as_ref()))
    {
        cached.take();
    }
}

/// Resolve the fixed math asset in the trusted parent before inserting an inline-only report.
pub(crate) async fn resolve_report_fonts(
    mut report: richview_core::document::RenderedReport,
) -> Result<richview_core::document::RenderedReport, JsValue> {
    const MATH_LINK: &str = "<link rel=\"stylesheet\" href=\"/fonts-math.css\">";
    if !report.html.contains(MATH_LINK) {
        return Ok(report);
    }
    let load = FONT_CATALOG.with(|slot| {
        slot.borrow_mut()
            .math
            .get_or_insert_with(|| {
                wasm_bindgen_futures::future_to_promise(async {
                    let css = fetch_text(&authoring::runtime_asset("fonts-math.css"))
                        .await
                        .map_err(|_| js_error("The report's math font could not be loaded."))?;
                    if !css.contains("@font-face { font-family: \"STIX Two Math\";")
                        || !css.contains("src: url(data:font/woff2;base64,")
                        || css.contains(['<', '>'])
                    {
                        return Err(js_error("The report's math font could not be loaded."));
                    }
                    Ok(JsValue::from_str(&css))
                })
            })
            .clone()
    });
    let css = match wasm_bindgen_futures::JsFuture::from(load.clone()).await {
        Ok(css) => css
            .as_string()
            .ok_or_else(|| js_error("The report's math font could not be loaded."))?,
        Err(error) => {
            // Only a loaded font stays cached: after a failed fetch the next render asks again.
            FONT_CATALOG.with(|slot| forget_failed(&mut slot.borrow_mut().math, &load));
            return Err(error);
        }
    };
    report.html = report
        .html
        .replace(MATH_LINK, &format!("<style>{css}</style>"));
    Ok(report)
}

#[wasm_bindgen]
pub fn render_source(source: &str, options_json: &str) -> Result<String, JsValue> {
    let options: richview_core::document::RenderOptions =
        serde_json::from_str(options_json).map_err(js_error)?;
    match richview_core::document::render(source, &options) {
        Ok(report) => serde_json::to_string(&report).map_err(js_error),
        Err(errors) => Err(js_error(serde_json::to_string(&errors).unwrap_or_default())),
    }
}

/// Return the same company style a report resolves, for the site's live colour demo.
#[wasm_bindgen(js_name = deriveCompanyStyle)]
pub fn derive_company_style(input: &str) -> String {
    crate::company_style_json(input)
}

thread_local! {static WORKER_DOCUMENT:RefCell<Option<richview_core::document::Document>>=const{RefCell::new(None)};}
/// Dense ingestion, reference discovery and render continuation all remain inside Rust.
#[wasm_bindgen]
pub fn worker_request(source: &str, options_json: &str) -> Result<String, JsValue> {
    let options: richview_core::document::RenderOptions =
        serde_json::from_str(options_json).map_err(js_error)?;
    richview_core::document::validate_report_folder(options.report_folder.as_ref())
        .map_err(|d| js_error(serde_json::to_string(&d).unwrap()))?;
    if !source.is_empty() {
        let mut parsed = richview_core::document::parse(source)
            .map_err(|d| js_error(serde_json::to_string(&d).unwrap_or_default()))?;
        // A missing or escaping file is a finding on the report, as in the native host: the rest renders.
        richview_core::document::attach_asset_findings(&mut parsed, options.report_folder.as_ref())
            .map_err(|d| js_error(serde_json::to_string(&d).unwrap()))?;
        WORKER_DOCUMENT.with(|d| *d.borrow_mut() = Some(parsed));
    }
    let references = WORKER_DOCUMENT
        .with(|d| {
            d.borrow()
                .as_ref()
                .map(|parsed| source_references(parsed, options.report_folder.as_ref()))
        })
        .ok_or_else(|| js_error("Worker request has no source document"))?;
    let pending: Vec<_> = references
        .into_iter()
        .filter(|r| {
            r["path"]
                .as_str()
                .is_some_and(|p| !options.data.contains_key(p))
        })
        .collect();
    if !pending.is_empty() {
        return Ok(serde_json::json!({"references":pending}).to_string());
    }
    let parsed = WORKER_DOCUMENT.with(|d| d.borrow_mut().take()).unwrap();
    let report = richview_core::document::render_document(parsed, &options)
        .map_err(|errors| js_error(serde_json::to_string(&errors).unwrap_or_default()))?;
    Ok(serde_json::json!({"report":report}).to_string())
}
/// The files the parent fetches for the worker: only those the folder holds. A file the folder
/// lacks already carries its finding and fails only its element (FEA-RP-30).
pub(crate) fn source_references(
    parsed: &richview_core::document::Document,
    folder: Option<&richview_core::document::ReportFolder>,
) -> Vec<Value> {
    folder
        .map(|folder| crate::folder_files(parsed, folder))
        .unwrap_or_default()
        .into_iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect()
}

fn from_js(value: JsValue) -> Result<Value, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(
        &js_sys::JSON::stringify(&value)?
            .as_string()
            .unwrap_or_default(),
    )
    .map_err(js_error)
}
fn into_js(value: &impl serde::Serialize) -> Result<JsValue, JsValue> {
    js_sys::JSON::parse(&serde_json::to_string(value).map_err(js_error)?)
}
#[wasm_bindgen(js_name=render)]
pub fn render_legacy(source: &str, options: JsValue) -> Result<JsValue, JsValue> {
    let report = richview_core::render_compat(source, &from_js(options)?)
        .map_err(|errors| js_error(serde_json::to_string(&errors).unwrap_or_default()))?;
    into_js(&report)
}
#[wasm_bindgen(js_name=check)]
pub fn check_legacy(source: &str, options: JsValue) -> Result<JsValue, JsValue> {
    let options: richview_core::document::RenderOptions =
        serde_json::from_value(from_js(options)?).map_err(js_error)?;
    into_js(&crate::check_result(source, &options))
}
#[wasm_bindgen(js_name=validate)]
pub fn validate_legacy(source: &str, options: JsValue) -> Result<JsValue, JsValue> {
    let options: richview_core::document::RenderOptions =
        serde_json::from_value(from_js(options)?).map_err(js_error)?;
    into_js(&crate::validate_result(source, options.report_folder))
}
#[wasm_bindgen]
pub fn core_constants() -> Result<JsValue, JsValue> {
    into_js(
        &serde_json::json!({"VERSION":richview_core::document::VERSION,"RICH_TAGS":richview_core::RICH_TAGS,"BUILTIN_STYLE_NAMES":richview_core::builtin_style_names()}),
    )
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    if web_sys::window().is_none() {
        return Ok(());
    }
    let doc = document();
    if doc
        .document_element()
        .and_then(|e| e.get_attribute("data-richview-started"))
        .is_some()
    {
        return Ok(());
    }
    if let Some(e) = doc.document_element() {
        e.set_attribute("data-richview-started", "rust")?;
        // Scripts run: the demo, its style buttons and its highlighting start on their own,
        // whatever the motion layer does (FEA-SF-29).
        if !e
            .class_name()
            .split_ascii_whitespace()
            .any(|name| name == "js")
        {
            e.set_class_name(format!("{} js", e.class_name()).trim());
        }
    }
    tabs::start()?;
    docs::start()?;
    authoring::start()?;
    let landing_page = query("body[data-richview-page='landing']").is_some();
    if landing_page {
        // The live example's scene pins its track first, so the motion layer measures its
        // chapters on the page as the reader will scroll it.
        writing::start();
    }
    landing::start()?;
    // Authoring remains available when the landing page has no animated canvas/stage.
    if landing_page {
        voices::start();
        // The live example's renders load the report-face catalog when they first need it, so a
        // first visit that never edits the example does not download it (S27-927, PRF-36).
        if let Err(error) = homepage::start() {
            self::error(&error);
        }
    }
    Ok(())
}
