//! The hosted editor, the page of `/reports/{org}/{slug}/edit` and `/reports/new`. It opens the
//! stored report's source in the playground's editor, keeps a draft in this browser, puts each
//! finding on its line, and saves a new version of the report through the publish API. The
//! rendering is the playground's: the same worker, the same runtime and the same preview frame.
use super::*;
use crate::editing;
use base64::Engine as _;
use richview_core::document::{Diagnostic, ReportFolder};
use serde_json::json;
use std::cell::Cell;
use std::collections::BTreeMap;

/// How long the source rests before the draft is written, in milliseconds.
const DRAFT_REST_MS: i32 = 600;
/// How long the note that a version was saved stays, in milliseconds.
const TOAST_MS: i32 = 8000;
/// The key the divider's place is kept under.
const SPLIT_KEY: &str = "richview.editor.split";

/// The report being edited and what the next save needs of it.
pub(super) struct Session {
    /// Who is editing: drafts are kept for this person alone.
    person: String,
    org: String,
    slug: String,
    /// The version the editor opened, or last saved: the one the next save follows.
    version: String,
    csrf: String,
    is_new: bool,
    folder: ReportFolder,
    styles: Vec<String>,
    data: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
    /// The source as it was opened or last saved: what a draft is compared with.
    saved: String,
}

impl Session {
    fn from_page() -> Self {
        Self {
            person: page("data-person"),
            org: page("data-org"),
            slug: page("data-slug"),
            version: page("data-version"),
            csrf: page("data-csrf"),
            is_new: page("data-new") == "true",
            folder: folder_of(None, &BTreeMap::new(), &BTreeMap::new()),
            styles: vec![],
            data: BTreeMap::new(),
            files: BTreeMap::new(),
            saved: String::new(),
        }
    }

    /// A file of the stored version as the render reads it: text as it is, media as a data URL.
    /// `None` when the version holds no such file.
    pub(super) fn resource(&self, path: &str, kind: &str) -> Option<String> {
        if kind == "media" {
            return self
                .files
                .get(path)
                .map(|bytes| format!("data:{};base64,{bytes}", super::media_mime(path)));
        }
        self.data.get(path).cloned().or_else(|| {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(self.files.get(path)?)
                .ok()?;
            String::from_utf8(bytes).ok()
        })
    }

    /// The report's folder as the render and the publication carry it.
    pub(super) fn folder(&self) -> ReportFolder {
        self.folder.clone()
    }

    /// The styles the stored version was published with, which replace the source's own.
    pub(super) fn styles(&self) -> Vec<String> {
        self.styles.clone()
    }

    /// Where the draft of this report is kept in the browser, for this person (`editing::draft_key`).
    fn draft_key(&self) -> Option<String> {
        editing::draft_key(&self.person, &self.org, &self.slug, self.is_new)
    }

    /// The report's own page: its history and its sharing.
    fn page_path(&self) -> String {
        format!("/reports/{}/{}", self.org, self.slug)
    }
}

/// The folder a stored report is rendered from: its own, or one holding the source and every
/// file the version carries.
fn folder_of(
    stored: Option<ReportFolder>,
    data: &BTreeMap<String, String>,
    files: &BTreeMap<String, String>,
) -> ReportFolder {
    let mut folder = stored.unwrap_or_else(|| {
        let mut listed = vec!["report.rmd".to_owned()];
        listed.extend(data.keys().chain(files.keys()).cloned());
        ReportFolder {
            root: String::new(),
            source: "report.rmd".into(),
            files: listed,
        }
    });
    // The stored folder keeps only a name; a render needs a root to name its files under.
    folder.root = "report".into();
    folder
}

/// A finding as the list and the lines show it.
struct Problem {
    level: String,
    message: String,
    fix: Option<String>,
    path: String,
    /// The line, counted from 0, with the stretch of it in UTF-16 units.
    range: Option<(usize, usize, usize)>,
}

thread_local! {
    static PROBLEMS: RefCell<Vec<Problem>> = const { RefCell::new(Vec::new()) };
    static TOUCHED: Cell<bool> = const { Cell::new(false) };
    static ERRORS: Cell<usize> = const { Cell::new(0) };
    static DRAFT_TIMER: Cell<Option<i32>> = const { Cell::new(None) };
    static TOAST_TIMER: Cell<Option<i32>> = const { Cell::new(None) };
    static DRAGGING: Cell<bool> = const { Cell::new(false) };
}

fn page(name: &str) -> String {
    query("body")
        .and_then(|body| body.get_attribute(name))
        .unwrap_or_default()
}

fn by_id(id: &str) -> Option<Element> {
    document().get_element_by_id(id)
}

fn set_text(id: &str, text: &str) {
    if let Some(element) = by_id(id) {
        element.set_text_content(Some(text));
    }
}

fn set_hidden(id: &str, hidden: bool) {
    if let Some(element) = by_id(id) {
        if hidden {
            let _ = element.set_attribute("hidden", "");
        } else {
            let _ = element.remove_attribute("hidden");
        }
    }
}

fn input(id: &str) -> Option<HtmlInputElement> {
    by_id(id).and_then(|e| e.dyn_into().ok())
}

fn area() -> Option<HtmlTextAreaElement> {
    query("#source").and_then(|e| e.dyn_into().ok())
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn storage() -> Option<web_sys::Storage> {
    window().local_storage().ok().flatten()
}

/// One request to the app's own API: the status and the JSON the server answered, or `Null`.
async fn api(
    method: &str,
    path: &str,
    csrf: &str,
    body: Option<String>,
) -> Result<(u16, Value), JsValue> {
    let request = web_sys::RequestInit::new();
    request.set_method(method);
    let headers = web_sys::Headers::new()?;
    headers.set("Accept", "application/json")?;
    if body.is_some() {
        headers.set("Content-Type", "application/json")?;
    }
    if method != "GET" {
        headers.set("x-richview-csrf", csrf)?;
    }
    request.set_headers(&headers);
    if let Some(body) = &body {
        request.set_body(&JsValue::from_str(body));
    }
    let response = JsFuture::from(window().fetch_with_str_and_init(path, &request))
        .await?
        .dyn_into::<web_sys::Response>()?;
    let text = JsFuture::from(response.text()?)
        .await?
        .as_string()
        .unwrap_or_default();
    Ok((
        response.status(),
        serde_json::from_str(&text).unwrap_or(Value::Null),
    ))
}

/// What a refusal says, in the server's words when it has some.
fn refusal(status: u16, body: &Value) -> String {
    body["error"]
        .as_str()
        .or_else(|| body["diagnostics"][0]["message"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| match status {
            401 | 403 => "Your session ended. Sign in again, then save.".into(),
            _ => format!("The server answered {status}."),
        })
}

/// Opens the page's report in the editor and starts the page's controls.
pub(super) fn start(editor: &Rc<RefCell<Editor>>) {
    // A Rust panic would otherwise end in a bare "unreachable"; the console gets its words.
    std::panic::set_hook(Box::new(|info| {
        web_sys::console::error_1(&JsValue::from_str(&info.to_string()));
    }));
    let session = Session::from_page();
    forget_other_drafts(&session.person);
    editor.borrow_mut().hosted = Some(session);
    bind(editor);
    show_save_key();
    spawn_local(open(editor.clone()));
}

/// Removes every draft in this browser that is not this person's, whoever left it and whichever
/// version of the editor wrote it: a draft is private to the person who wrote it, and a browser
/// two people sign in to keeps nobody's for the other.
fn forget_other_drafts(person: &str) {
    let Some(store) = storage() else { return };
    let count = store.length().unwrap_or(0);
    let others: Vec<String> = (0..count)
        .filter_map(|index| store.key(index).ok().flatten())
        .filter(|key| editing::is_another_persons_draft(key, person))
        .collect();
    for key in others {
        let _ = store.remove_item(&key);
    }
}

/// The save button names its shortcut as the person's keyboard writes it.
fn show_save_key() {
    let apple = window()
        .navigator()
        .user_agent()
        .is_ok_and(|agent| agent.contains("Mac"));
    if !apple && let Some(key) = query("#ed-save kbd") {
        key.set_text_content(Some("Ctrl S"));
    }
}

fn bind(editor: &Rc<RefCell<Editor>>) {
    bind_input(editor);
    bind_styles(editor);
    bind_insert();
    bind_problems();
    bind_save(editor);
    bind_keys(editor);
    bind_split();
    bind_tabs();
}

/// Loads the stored version (or the starter), offers a kept draft, and renders.
async fn open(editor: Rc<RefCell<Editor>>) {
    let (org, slug, version, is_new) = {
        let e = editor.borrow();
        let Some(s) = e.hosted.as_ref() else { return };
        (s.org.clone(), s.slug.clone(), s.version.clone(), s.is_new)
    };
    let mut source = editing::STARTER.to_owned();
    if !is_new {
        let path = format!("/api/reports/{org}/{slug}/v/{version}");
        match api("GET", &path, "", None).await {
            Ok((200, bundle)) => {
                source = bundle["source"].as_str().unwrap_or_default().to_owned();
                let map = |key: &str| -> BTreeMap<String, String> {
                    bundle[key]
                        .as_object()
                        .map(|object| {
                            object
                                .iter()
                                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let (data, files) = (map("data"), map("files"));
                let stored = serde_json::from_value(bundle["reportFolder"].clone()).ok();
                let mut e = editor.borrow_mut();
                if let Some(s) = e.hosted.as_mut() {
                    s.folder = folder_of(stored, &data, &files);
                    s.styles = bundle["styles"]
                        .as_array()
                        .map(|list| {
                            list.iter()
                                .filter_map(|v| v.as_str().map(str::to_owned))
                                .collect()
                        })
                        .unwrap_or_default();
                    s.data = data;
                    s.files = files;
                }
            }
            Ok((status, body)) => {
                cannot_open(&refusal(status, &body));
                return;
            }
            Err(_) => {
                cannot_open("The server could not be reached. Reload the page to try again.");
                return;
            }
        }
    }
    let draft = {
        let e = editor.borrow();
        e.hosted.as_ref().and_then(|s| {
            let kept = storage()?.get_item(&s.draft_key()?).ok().flatten()?;
            let kept: Value = serde_json::from_str(&kept).ok()?;
            // A draft belongs to the version it was started from.
            (kept["base"].as_str()? == s.version)
                .then(|| kept["source"].as_str().map(str::to_owned))
                .flatten()
        })
    };
    let restored = draft.as_ref().is_some_and(|kept| *kept != source);
    {
        let mut e = editor.borrow_mut();
        if let Some(s) = e.hosted.as_mut() {
            s.saved = source.clone();
        }
        e.source
            .replace_full(draft.filter(|_| restored).unwrap_or(source));
    }
    refresh(&editor);
    if restored {
        say("Your draft from this browser was restored. Save version publishes it.");
    }
    render_editor(editor.clone()).await;
}

/// The report could not be opened: the page says so where the preview would be.
fn cannot_open(reason: &str) {
    set_text(
        "ed-banner-text",
        &format!("Could not open this report. {reason}"),
    );
    set_hidden("ed-banner-go", true);
    set_hidden("ed-banner", false);
    if let Some(save) = by_id("ed-save") {
        let _ = save.set_attribute("disabled", "");
    }
}

/// Says something to assistive technology and in the short note under the bar.
fn say(text: &str) {
    set_text("ed-live", text);
}

/// Brings what the page shows about the source up to date: the title, the style pills, the
/// state and the kept draft.
fn refresh(editor: &Rc<RefCell<Editor>>) {
    let (source, state) = {
        let e = editor.borrow();
        let source = e.source.full();
        let state = e.hosted.as_ref().map(|s| {
            if source != s.saved {
                "Unsaved changes".to_owned()
            } else if s.is_new {
                "New report".to_owned()
            } else {
                format!("Editing v{}", s.version)
            }
        });
        (source, state)
    };
    set_text(
        "ed-title",
        &editing::frontmatter_value(&source, "title").unwrap_or_else(|| "Untitled report".into()),
    );
    if let Some(state) = state {
        set_text("ed-state", &state);
        if let Some(element) = by_id("ed-state") {
            let _ = element
                .class_list()
                .toggle_with_force("ed-state--dirty", state == "Unsaved changes");
        }
    }
    sync_pills(&source);
    schedule_draft(editor);
}

/// One pill checked at most: the style the source declares.
fn sync_pills(source: &str) {
    let declared = editing::shown_style(source);
    let Ok(buttons) = document().query_selector_all("#ed-styles [data-style]") else {
        return;
    };
    for i in 0..buttons.length() {
        let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) else {
            continue;
        };
        let checked = declared.as_deref() == button.get_attribute("data-style").as_deref();
        // The group is one stop for the keyboard: the checked pill, or the first when the source
        // names a style of its own that no pill stands for.
        let stop = checked || (declared.is_none() && i == 0);
        let _ = button.set_attribute("aria-checked", if checked { "true" } else { "false" });
        let _ = button.set_attribute("tabindex", if stop { "0" } else { "-1" });
    }
}

/// Writes the draft once the source has rested.
fn schedule_draft(editor: &Rc<RefCell<Editor>>) {
    if let Some(timer) = DRAFT_TIMER.with(Cell::take) {
        window().clear_timeout_with_handle(timer);
    }
    let editor = editor.clone();
    let callback = Closure::once_into_js(move || write_draft(&editor));
    let timer = window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.unchecked_ref(),
            DRAFT_REST_MS,
        )
        .ok();
    DRAFT_TIMER.with(|slot| slot.set(timer));
}

/// Keeps the source in this browser while it differs from the saved one, and forgets it when it
/// does not. The draft never leaves the browser.
fn write_draft(editor: &Rc<RefCell<Editor>>) {
    let e = editor.borrow();
    let (Some(s), Some(store)) = (e.hosted.as_ref(), storage()) else {
        return;
    };
    let Some(key) = s.draft_key() else {
        return;
    };
    let source = e.source.full();
    if source == s.saved {
        let _ = store.remove_item(&key);
        set_text("ed-draft", "");
        return;
    }
    let kept = json!({"base": s.version, "source": source}).to_string();
    set_text(
        "ed-draft",
        if store.set_item(&key, &kept).is_ok() {
            "Draft kept in this browser"
        } else {
            "This browser would not keep a draft"
        },
    );
}

/// Shows what a render found: on the lines, in the list and the count, and, when the render did
/// not finish, under a banner above the last good report. `elapsed` is the render's time when it
/// did finish.
pub(super) fn found(e: &mut Editor, diagnostics: &[Diagnostic], elapsed: Option<f64>) {
    let source = e.source.full();
    let problems = problems(&source, diagnostics);
    super::marks::set(
        problems
            .iter()
            .filter_map(|p| {
                let (line, start, end) = p.range?;
                let level = match p.level.as_str() {
                    "error" => super::marks::Level::Error,
                    "warning" => super::marks::Level::Warning,
                    _ => return None,
                };
                Some(super::marks::Mark {
                    line,
                    start,
                    end,
                    level,
                    message: p.message.clone(),
                })
            })
            .collect(),
    );
    let count = |level: &str| problems.iter().filter(|p| p.level == level).count();
    let (errors, warnings) = (count("error"), count("warning"));
    set_text("ed-count-text", &count_words(errors, warnings));
    if let Some(button) = by_id("ed-count") {
        let state = match (errors, warnings) {
            (0, 0) => "ok",
            (0, _) => "warning",
            _ => "error",
        };
        let _ = button.set_attribute("data-state", state);
    }
    render_list(&problems);
    match elapsed {
        Some(ms) => {
            set_hidden("ed-banner", true);
            set_text("ed-time", &format!("Rendered in {ms:.0} ms"));
        }
        None => banner(&problems, !e.html.is_empty()),
    }
    if ERRORS.with(|held| held.replace(errors)) != errors {
        let first = problems.iter().find(|p| p.level == "error");
        say(&match first {
            Some(p) => format!("{}. {}", count_words(errors, warnings), p.message),
            None => count_words(errors, warnings),
        });
    }
    PROBLEMS.with(|held| *held.borrow_mut() = problems);
}

/// The count as the status bar words it.
fn count_words(errors: usize, warnings: usize) -> String {
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    match (errors, warnings) {
        (0, 0) => "No problems".into(),
        (0, w) => plural(w, "warning"),
        (e, 0) => plural(e, "error"),
        (e, w) => format!("{}, {}", plural(e, "error"), plural(w, "warning")),
    }
}

/// The banner of a render that did not finish: the report below it is the last good one.
fn banner(problems: &[Problem], has_report: bool) {
    let first = problems
        .iter()
        .find(|p| p.level == "error")
        .and_then(|p| p.range.map(|r| r.0));
    let lead = if has_report {
        "Showing the last good version."
    } else {
        "The report cannot render yet."
    };
    let tail = match first {
        Some(line) => format!(
            "Line {} needs a fix before the report can update.",
            line + 1
        ),
        None => "The problem needs a fix before the report can update.".into(),
    };
    set_text("ed-banner-text", &format!("{lead} {tail}"));
    if let Some(go) = by_id("ed-banner-go") {
        match first {
            Some(line) => {
                let _ = go.set_attribute("data-line", &line.to_string());
                go.set_text_content(Some(&format!("Go to line {}", line + 1)));
                let _ = go.remove_attribute("hidden");
            }
            None => {
                let _ = go.set_attribute("hidden", "");
            }
        }
    }
    set_hidden("ed-banner", false);
}

/// The findings of a render with their lines: errors, then warnings, then notes. The editor
/// service places each (ARC-08); a finding it cannot place takes its own line, or none.
fn problems(source: &str, found: &[Diagnostic]) -> Vec<Problem> {
    let found: Vec<Diagnostic> = found
        .iter()
        .filter(|d| d.code != "assets.runtime-external")
        .cloned()
        .collect();
    let located: Vec<Value> = richview_core::document::parse(source)
        .ok()
        .and_then(|document| {
            richview_authoring::diagnostics(source, found.clone(), &document, &Default::default())
                .as_array()
                .cloned()
        })
        .unwrap_or_default();
    let lines: Vec<&str> = source.split('\n').collect();
    let mut problems: Vec<Problem> = found
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let range = located
                .get(i)
                .and_then(|value| range_of(value, &lines))
                .or_else(|| {
                    d.line
                        .and_then(|line| whole_line(&lines, line.saturating_sub(1)))
                });
            Problem {
                level: d.level.clone(),
                message: d.message.clone(),
                fix: d.fix.clone(),
                path: d.path.clone(),
                range,
            }
        })
        .collect();
    let rank = |p: &Problem| match p.level.as_str() {
        "error" => 0,
        "warning" => 1,
        _ => 2,
    };
    problems.sort_by_key(|p| (rank(p), p.range.map(|r| r.0)));
    problems
}

/// The length of a line in UTF-16 units, without its carriage return.
fn width(line: &str) -> usize {
    line.trim_end_matches('\r').encode_utf16().count()
}

/// The range the editor service gave a finding, as its line and the stretch of that line.
fn range_of(value: &Value, lines: &[&str]) -> Option<(usize, usize, usize)> {
    let range = value.get("range")?;
    let number = |v: &Value| v.as_u64().map(|n| n as usize);
    let (line, start) = (
        number(&range["start"]["line"])?,
        number(&range["start"]["character"])?,
    );
    let (end_line, end) = (
        number(&range["end"]["line"])?,
        number(&range["end"]["character"])?,
    );
    let end = if end_line == line {
        end
    } else {
        lines.get(line).map_or(start + 1, |l| width(l))
    };
    Some((line, start, end.max(start + 1)))
}

/// A finding with only a line: the line's text without its indentation.
fn whole_line(lines: &[&str], line: usize) -> Option<(usize, usize, usize)> {
    let text = lines.get(line)?.trim_end_matches('\r');
    let trimmed = text.trim_start();
    let start = text[..text.len() - trimmed.len()].encode_utf16().count();
    Some((line, start, width(text).max(start + 1)))
}

/// The problems list under the editor: one row each, the line a button.
fn render_list(problems: &[Problem]) {
    let Some(list) = by_id("ed-problems-list") else {
        return;
    };
    let rows: String = problems
        .iter()
        .map(|p| {
            let (class, icon) = match p.level.as_str() {
                "error" => ("error", "alert"),
                "warning" => ("warning", "alert"),
                _ => ("note", "check"),
            };
            let at = match p.range {
                Some((line, ..)) => format!(
                    "<button type=\"button\" class=\"at\" data-line=\"{line}\">Line {}</button>",
                    line + 1
                ),
                None => "<span class=\"at\"></span>".into(),
            };
            let fix = p
                .fix
                .as_deref()
                .map(|fix| format!(" <em>{}</em>", escape(fix)))
                .unwrap_or_default();
            format!(
                "<li class=\"ed-prob ed-prob--{class}\"><svg class=\"ic\" aria-hidden=\"true\"><use href=\"#i-{icon}\"/></svg>{at}<span class=\"why\"><b>{}</b> <span>{}</span>{fix}</span></li>",
                escape(&p.path),
                escape(&p.message)
            )
        })
        .collect();
    list.set_inner_html(&rows);
    set_hidden("ed-problems-empty", !problems.is_empty());
}

/// The caret's line, counted from 0.
fn caret_line() -> usize {
    area().map_or(0, |area| {
        let caret = area.selection_start().ok().flatten().unwrap_or(0) as usize;
        crate::completion::position(&area.value(), caret).0
    })
}

fn bind_input(editor: &Rc<RefCell<Editor>>) {
    let Some(area) = area() else { return };
    let target: web_sys::EventTarget = area.into();
    for event in ["pointerdown", "keydown", "input"] {
        on(&target, event, |_| {
            TOUCHED.with(|touched| touched.set(true))
        });
    }
    let ed = editor.clone();
    on(&target, "input", move |_| refresh(&ed));
    for event in ["keyup", "click", "select", "input", "focus"] {
        on(&target, event, |_| show_position());
    }
}

/// The caret's line and column in the status bar.
fn show_position() {
    let Some(area) = area() else { return };
    let caret = area.selection_start().ok().flatten().unwrap_or(0) as usize;
    let (line, column) = crate::completion::position(&area.value(), caret);
    set_text("ed-pos", &format!("Ln {}, Col {}", line + 1, column + 1));
}

/// Replaces the text area's text with `new` by changing only what differs, as typing would, so
/// undo takes it back and the caret and scroll stay where they were.
fn replace(old: &str, new: &str) {
    if let (Some(area), Some((start, end, text))) = (area(), editing::change(old, new)) {
        complete::put(&area, start as u32, end as u32, &text);
    }
}

fn bind_styles(editor: &Rc<RefCell<Editor>>) {
    let Ok(buttons) = document().query_selector_all("#ed-styles [data-style]") else {
        return;
    };
    let names: Vec<String> = (0..buttons.length())
        .filter_map(|i| buttons.item(i))
        .filter_map(|b| b.dyn_into::<Element>().ok())
        .filter_map(|b| b.get_attribute("data-style"))
        .collect();
    for i in 0..buttons.length() {
        let Some(button) = buttons
            .item(i)
            .and_then(|b| b.dyn_into::<HtmlElement>().ok())
        else {
            continue;
        };
        let choose = {
            let editor = editor.clone();
            move |name: &str| {
                let old = editor.borrow().source.full();
                replace(&old, &editing::with_style(&old, name));
                if let Some(pill) = query(&format!("#ed-styles [data-style='{name}']"))
                    .and_then(|b| b.dyn_into::<HtmlElement>().ok())
                {
                    let _ = pill.focus();
                }
            }
        };
        let name = names.get(i as usize).cloned().unwrap_or_default();
        let click = choose.clone();
        on(&button.clone().into(), "click", move |_| click(&name));
        let names = names.clone();
        on(&button.into(), "keydown", move |event| {
            let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
                return;
            };
            let step = match event.key().as_str() {
                "ArrowRight" | "ArrowDown" => 1,
                "ArrowLeft" | "ArrowUp" => names.len() - 1,
                _ => return,
            };
            event.prevent_default();
            choose(&names[(i as usize + step) % names.len()]);
        });
    }
}

fn bind_insert() {
    let Ok(buttons) = document().query_selector_all("#ed-insert [data-add]") else {
        return;
    };
    for i in 0..buttons.length() {
        let Some(button) = buttons.item(i).and_then(|b| b.dyn_into::<Element>().ok()) else {
            continue;
        };
        let Some(name) = button.get_attribute("data-add") else {
            continue;
        };
        on(&button.into(), "click", move |_| add_block(&name));
    }
}

/// Adds a block after the paragraph or block the caret is in, or at the end when the author has
/// not yet put a caret in the source, and scrolls to it.
fn add_block(name: &str) {
    let (Some(area), Some(block)) = (area(), editing::block(name)) else {
        return;
    };
    complete::hide();
    let source = area.value();
    let caret = TOUCHED
        .with(Cell::get)
        .then(|| area.selection_end().ok().flatten().unwrap_or(0) as usize);
    let (edit, line) = crate::tools::insertion(&source, caret, block);
    complete::put(&area, edit.start as u32, edit.end as u32, &edit.text);
    scroll_to(&area, line.saturating_sub(1));
    say(&format!("Added a {name} at line {line}."));
}

/// Scrolls the source to put `line`, counted from 0, a third of the way down its box.
fn scroll_to(area: &HtmlTextAreaElement, line: usize) {
    let Some(row) = query(".code__hl code").and_then(|code| code.children().item(line as u32))
    else {
        return;
    };
    let top = row
        .dyn_into::<HtmlElement>()
        .map_or(0, |row| row.offset_top());
    area.set_scroll_top((top - area.client_height() / 3).max(0));
    code::sync_scroll(area);
}

/// Puts the caret on the line, selects it and brings it into view, on the source pane.
fn jump(line: usize) {
    pane("source");
    let Some(area) = area() else { return };
    let source = area.value();
    let from = crate::completion::offset(&source, line, 0);
    let length = source.split('\n').nth(line).map_or(0, width);
    let _ = area.focus();
    let _ = area.set_selection_range(from as u32, (from + length) as u32);
    scroll_to(&area, line);
}

/// Goes to the next or the previous marked line, round the source.
fn next_problem(backwards: bool) {
    let lines = super::marks::lines();
    if lines.is_empty() {
        return;
    }
    let here = caret_line();
    let target = if backwards {
        lines.iter().rev().find(|l| **l < here).or(lines.last())
    } else {
        lines.iter().find(|l| **l > here).or(lines.first())
    };
    if let Some(line) = target {
        jump(*line);
    }
}

fn bind_problems() {
    if let Some(list) = by_id("ed-problems-list") {
        on(&list.into(), "click", |event| {
            let line = event
                .target()
                .and_then(|t| t.dyn_into::<Element>().ok())
                .and_then(|t| t.closest("[data-line]").ok().flatten())
                .and_then(|t| t.get_attribute("data-line"))
                .and_then(|l| l.parse().ok());
            if let Some(line) = line {
                jump(line);
            }
        });
    }
    if let Some(go) = by_id("ed-banner-go") {
        let source = go.clone();
        on(&go.into(), "click", move |_| {
            if let Some(line) = source
                .get_attribute("data-line")
                .and_then(|l| l.parse().ok())
            {
                jump(line);
            }
        });
    }
    if let Some(count) = by_id("ed-count") {
        let button = count.clone();
        on(&count.into(), "click", move |_| {
            let open = by_id("ed-problems").is_some_and(|d| !d.has_attribute("hidden"));
            set_hidden("ed-problems", open);
            let _ = button.set_attribute("aria-expanded", if open { "false" } else { "true" });
        });
    }
}

/// Which pane a phone shows; wider screens show both, whatever this says.
fn pane(name: &str) {
    if let Some(body) = query("body") {
        let _ = body.set_attribute("data-pane", name);
    }
    for tab in ["source", "report"] {
        if let Some(button) = by_id(&format!("ed-tab-{tab}")) {
            let _ =
                button.set_attribute("aria-selected", if tab == name { "true" } else { "false" });
        }
    }
}

fn bind_tabs() {
    for name in ["source", "report"] {
        if let Some(tab) = by_id(&format!("ed-tab-{name}")) {
            on(&tab.into(), "click", move |_| pane(name));
        }
    }
}

/// Puts the divider at `percent` of the work area's width.
fn set_split(percent: f64) {
    let percent = percent.clamp(25.0, 75.0);
    if let Some(work) = by_id("ed-work") {
        style(&work, "--ed-split", &format!("{percent:.1}"));
    }
    if let Some(split) = by_id("ed-split") {
        let _ = split.set_attribute("aria-valuenow", &format!("{}", percent.round()));
    }
    if let Some(store) = storage() {
        let _ = store.set_item(SPLIT_KEY, &format!("{percent:.1}"));
    }
}

fn split_now() -> f64 {
    by_id("ed-split")
        .and_then(|split| split.get_attribute("aria-valuenow"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(50.0)
}

/// The divider between the panes: drag it with a pointer, move it with the arrow keys, double-click
/// it to put it back in the middle.
fn bind_split() {
    let (Some(split), Some(work)) = (by_id("ed-split"), by_id("ed-work")) else {
        return;
    };
    if let Some(kept) = storage()
        .and_then(|store| store.get_item(SPLIT_KEY).ok().flatten())
        .and_then(|value| value.parse::<f64>().ok())
    {
        set_split(kept);
    }
    let target: web_sys::EventTarget = split.clone().into();
    let handle = split.clone();
    on(&target, "pointerdown", move |event| {
        if let Some(event) = event.dyn_ref::<web_sys::PointerEvent>() {
            let _ = handle.set_pointer_capture(event.pointer_id());
            DRAGGING.with(|dragging| dragging.set(true));
            event.prevent_default();
        }
    });
    on(&target, "pointermove", move |event| {
        if !DRAGGING.with(Cell::get) {
            return;
        }
        if let Some(event) = event.dyn_ref::<web_sys::PointerEvent>() {
            let rect = work.get_bounding_client_rect();
            if rect.width() > 0.0 {
                set_split((f64::from(event.client_x()) - rect.left()) / rect.width() * 100.0);
            }
        }
    });
    for end in ["pointerup", "pointercancel"] {
        let handle = split.clone();
        on(&target, end, move |event| {
            DRAGGING.with(|dragging| dragging.set(false));
            if let Some(event) = event.dyn_ref::<web_sys::PointerEvent>() {
                let _ = handle.release_pointer_capture(event.pointer_id());
            }
        });
    }
    on(&target, "dblclick", |_| set_split(50.0));
    on(&target, "keydown", |event| {
        let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
            return;
        };
        let now = split_now();
        let next = match event.key().as_str() {
            "ArrowLeft" => now - 2.0,
            "ArrowRight" => now + 2.0,
            "Home" => 25.0,
            "End" => 75.0,
            _ => return,
        };
        event.prevent_default();
        set_split(next);
    });
}

fn bind_keys(editor: &Rc<RefCell<Editor>>) {
    let ed = editor.clone();
    on(&document().into(), "keydown", move |event| {
        let Ok(event) = event.dyn_into::<KeyboardEvent>() else {
            return;
        };
        let key = event.key();
        if (event.meta_key() || event.ctrl_key()) && key.eq_ignore_ascii_case("s") {
            event.prevent_default();
            open_save(&ed);
        } else if key == "F8" {
            event.prevent_default();
            next_problem(event.shift_key());
        } else if key == "Escape" && by_id("ed-pop").is_some_and(|p| !p.has_attribute("hidden")) {
            close_save();
        }
    });
}

fn bind_save(editor: &Rc<RefCell<Editor>>) {
    if let Some(button) = by_id("ed-save") {
        let ed = editor.clone();
        on(&button.into(), "click", move |_| open_save(&ed));
    }
    if let Some(button) = by_id("ed-pop-cancel") {
        on(&button.into(), "click", |_| close_save());
    }
    if let Some(button) = by_id("ed-pop-go") {
        let ed = editor.clone();
        on(&button.into(), "click", move |_| {
            spawn_local(save(ed.clone()))
        });
    }
    if let Some(button) = by_id("ed-reload") {
        let ed = editor.clone();
        on(&button.into(), "click", move |_| discard_and_reload(&ed));
    }
    for id in ["ed-version", "ed-slug"] {
        if let Some(field) = by_id(id) {
            let ed = editor.clone();
            on(&field.into(), "keydown", move |event| {
                if event
                    .dyn_ref::<KeyboardEvent>()
                    .is_some_and(|e| e.key() == "Enter")
                {
                    event.prevent_default();
                    spawn_local(save(ed.clone()));
                }
            });
        }
    }
}

fn close_save() {
    set_hidden("ed-pop", true);
    if let Some(save) = by_id("ed-save").and_then(|b| b.dyn_into::<HtmlElement>().ok()) {
        let _ = save.focus();
    }
}

/// The version a save proposes: the one the author wrote into the source when it is newer than
/// the one opened, else the next patch after it.
fn proposed_version(base: &str, written: &str, is_new: bool) -> String {
    if is_new {
        return if written.is_empty() {
            "0.0.1".into()
        } else {
            written.into()
        };
    }
    if editing::newer(written, base) {
        return written.into();
    }
    editing::next_patch(base).unwrap_or_else(|| base.into())
}

/// Opens the save box with the version, and for a new report the address, filled in.
fn open_save(editor: &Rc<RefCell<Editor>>) {
    let (source, base, is_new) = {
        let e = editor.borrow();
        let Some(s) = e.hosted.as_ref() else { return };
        (e.source.full(), s.version.clone(), s.is_new)
    };
    if by_id("ed-save").is_some_and(|b| b.has_attribute("disabled")) {
        return;
    }
    let written = editing::frontmatter_value(&source, "version").unwrap_or_default();
    let proposal = proposed_version(&base, &written, is_new);
    if let Some(field) = input("ed-version") {
        field.set_value(&proposal);
    }
    set_hidden("ed-slug-row", !is_new);
    if is_new && let Some(field) = input("ed-slug") {
        field.set_value(
            &richview_core::create_publish_request(&source, None)
                .map(|request| request.slug)
                .unwrap_or_default(),
        );
    }
    set_text("ed-pop-note", "");
    set_hidden("ed-pop-note", true);
    set_hidden("ed-reload", true);
    set_text("ed-pop-error", "");
    set_hidden("ed-pop-error", true);
    set_hidden("ed-pop", false);
    if let Some(field) = input("ed-version") {
        let _ = field.focus();
        field.select();
    }
    if !is_new {
        spawn_local(check_newer(editor.clone(), base, proposal));
    }
}

/// Asks which version is newest, and says so when another was saved after this one was opened.
async fn check_newer(editor: Rc<RefCell<Editor>>, base: String, proposal: String) {
    let (org, slug, csrf) = {
        let e = editor.borrow();
        let Some(s) = e.hosted.as_ref() else { return };
        (s.org.clone(), s.slug.clone(), s.csrf.clone())
    };
    let path = format!("/api/reports/{org}/{slug}/versions");
    let Ok((200, list)) = api("GET", &path, &csrf, None).await else {
        return;
    };
    let Some(newest) = list[0].as_str() else {
        return;
    };
    if !editing::newer(newest, &base) {
        return;
    }
    let next = editing::next_patch(newest).unwrap_or_else(|| newest.to_owned());
    if let Some(field) = input("ed-version")
        && field.value() == proposal
    {
        field.set_value(&next);
    }
    set_text(
        "ed-pop-note",
        &format!(
            "Version {newest} was saved after you opened this report. Saving publishes your source on top of it."
        ),
    );
    set_hidden("ed-pop-note", false);
    set_hidden("ed-reload", false);
}

/// Drops the draft and the page's source for the stored latest version.
fn discard_and_reload(editor: &Rc<RefCell<Editor>>) {
    if let (Some(store), Some(key)) = (
        storage(),
        editor.borrow().hosted.as_ref().and_then(Session::draft_key),
    ) {
        let _ = store.remove_item(&key);
    }
    let _ = window().location().reload();
}

fn pop_error(text: &str) {
    set_text("ed-pop-error", text);
    set_hidden("ed-pop-error", text.is_empty());
}

/// Publishes the source as a new version of the report, as `richview publish` does.
async fn save(editor: Rc<RefCell<Editor>>) {
    let Some(version) = input("ed-version").map(|f| f.value().trim().to_owned()) else {
        return;
    };
    let (is_new, org, csrf, current_slug) = {
        let e = editor.borrow();
        let Some(s) = e.hosted.as_ref() else { return };
        (s.is_new, s.org.clone(), s.csrf.clone(), s.slug.clone())
    };
    if let Some(problem) = richview_core::version_problem(&version) {
        pop_error(&problem);
        return;
    }
    let slug = if is_new {
        input("ed-slug")
            .map(|f| f.value().trim().to_owned())
            .unwrap_or_default()
    } else {
        current_slug
    };
    if let Some(problem) = richview_core::slug_problem(&slug) {
        pop_error(&problem);
        return;
    }
    let old = editor.borrow().source.full();
    let mut source = editing::set_frontmatter(&old, "version", &version);
    if is_new {
        source = editing::set_frontmatter(&source, "slug", &slug);
    }
    replace(&old, &source);
    let body = {
        let e = editor.borrow();
        let Some(s) = e.hosted.as_ref() else { return };
        json!({
            "slug": slug,
            "org": org,
            "source": source,
            "reportFolder": s.folder,
            "styles": s.styles,
            "data": s.data,
            "files": s.files,
            "new": is_new,
            // A first save makes a report: an address that is taken is refused, never published into.
            "createOnly": is_new,
        })
        .to_string()
    };
    pop_error("");
    let go = by_id("ed-pop-go");
    if let Some(go) = &go {
        let _ = go.set_attribute("disabled", "");
        go.set_text_content(Some("Saving…"));
    }
    let result = api("POST", "/api/reports", &csrf, Some(body)).await;
    if let Some(go) = &go {
        let _ = go.remove_attribute("disabled");
        go.set_text_content(Some("Save version"));
    }
    match result {
        Ok((200, answer)) => {
            let saved = answer["version"].as_str().unwrap_or(&version).to_owned();
            let kept = {
                let mut e = editor.borrow_mut();
                e.hosted.as_mut().map(|s| {
                    if let (Some(store), Some(key)) = (storage(), s.draft_key()) {
                        let _ = store.remove_item(&key);
                    }
                    s.version = saved.clone();
                    s.slug = slug.clone();
                    s.is_new = false;
                    s.saved = source.clone();
                    (s.page_path(), is_new)
                })
            };
            if let Some((page_path, was_new)) = kept {
                finish_save(&org, &slug, &saved, &page_path, was_new);
            }
            refresh(&editor);
        }
        Ok((status, answer)) => pop_error(&refusal(status, &answer)),
        Err(_) => pop_error("The server could not be reached. Your draft is kept; try again."),
    }
}

/// The save went through: the box closes, the page's links point at the report, and the address
/// bar names the report once a new one has an address.
fn finish_save(org: &str, slug: &str, version: &str, page_path: &str, was_new: bool) {
    close_save();
    for id in ["ed-history", "ed-share"] {
        if let Some(link) = by_id(id) {
            let _ = link.set_attribute("href", page_path);
            let _ = link.remove_attribute("hidden");
        }
    }
    if let Some(body) = query("body") {
        let _ = body.set_attribute("data-version", version);
        let _ = body.set_attribute("data-slug", slug);
        let _ = body.set_attribute("data-new", "false");
    }
    if was_new && let Ok(history) = window().history() {
        let _ = history.replace_state_with_url(
            &JsValue::NULL,
            "",
            Some(&format!("/reports/{org}/{slug}/edit")),
        );
    }
    set_text("ed-draft", "");
    set_text("ed-toast-text", &format!("Saved version {version}."));
    if let Some(link) = by_id("ed-toast-link") {
        let _ = link.set_attribute("href", page_path);
    }
    set_hidden("ed-toast", false);
    say(&format!("Saved version {version}."));
    if let Some(timer) = TOAST_TIMER.with(Cell::take) {
        window().clear_timeout_with_handle(timer);
    }
    let callback = Closure::once_into_js(|| set_hidden("ed-toast", true));
    let timer = window()
        .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), TOAST_MS)
        .ok();
    TOAST_TIMER.with(|slot| slot.set(timer));
}
