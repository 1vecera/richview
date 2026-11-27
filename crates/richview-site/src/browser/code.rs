//! The shared Rich Markdown editor overlay.
use super::document;
use richview_core::HighlightLanguage;
use web_sys::HtmlTextAreaElement;

/// Paint the highlighted copy and line numbers under a transparent text area.
pub(crate) fn highlight(area: &HtmlTextAreaElement) {
    // Keep the textarea readable until the browser runtime starts the overlay.
    if document().document_element().is_none_or(|root| {
        !root
            .class_name()
            .split_ascii_whitespace()
            .any(|name| name == "js")
    }) {
        return;
    }
    let Some(code) = area.parent_element() else {
        return;
    };
    let language =
        HighlightLanguage::from_name(&code.get_attribute("data-language").unwrap_or_default());
    let value = area.value();
    if let Ok(Some(target)) = code.query_selector(".code__hl code") {
        let html = richview_core::highlight_html(&value, language);
        // Every source line is a block: its number follows the height of preceding wraps.
        let lines = html
            .split('\n')
            .map(|line| format!("<span class=\"code__line\">{line}&#8203;</span>"))
            .collect::<String>();
        target.set_inner_html(&lines);
    }
    let mut classes: Vec<String> = code
        .class_name()
        .split_whitespace()
        .filter(|c| *c != "is-live")
        .map(str::to_string)
        .collect();
    classes.push("is-live".into());
    code.set_class_name(&classes.join(" "));
    // Findings the host has placed are drawn again on the rebuilt lines.
    super::marks::apply();
    sync_scroll(area);
}

pub(crate) fn sync_scroll(area: &HtmlTextAreaElement) {
    let Some(code) = area.parent_element() else {
        return;
    };
    if let Ok(Some(pre)) = code.query_selector(".code__hl") {
        pre.set_scroll_top(area.scroll_top());
        pre.set_scroll_left(area.scroll_left());
    }
}
