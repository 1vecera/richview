//! Reading aids for long reports: the table of contents (FEA-RP-63) and the reading-progress bar
//! (FEA-RP-64). Both are on by default; `toc: false` and `progress: false` turn them off. The
//! contents are plain links, so they work before and without scripts (FEA-RP-39); the runtime
//! fills the bar and marks the section being read.

use serde_json::Value;

/// A report with fewer sections than this has no table of contents.
const MIN_SECTIONS: usize = 3;

/// The body's top-level sections, as `(id, text)`: each `<h2 id>` of the rendered Markdown body,
/// whose blocks are still placeholders, so a heading inside a callout or tab is never one. The
/// footnotes' own heading is not a section. The text keeps the heading's escaping and drops its
/// inline markup.
pub(crate) fn sections(body: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find("<h2 id=\"") {
        rest = &rest[at + "<h2 id=\"".len()..];
        let Some(end) = rest.find('"') else { break };
        let id = &rest[..end];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</h2>") else {
            break;
        };
        if close < open {
            continue;
        }
        let text = strip_tags(&rest[open + 1..close]);
        rest = &rest[close..];
        if id.starts_with("rv-notes-title") || text.trim().is_empty() {
            continue;
        }
        found.push((id.to_owned(), text.trim().to_owned()));
    }
    found
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for c in html.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

/// Whether the report shows its table of contents: on unless `toc: false`, and only for a
/// report with at least three sections and no tabs, whose panels hide their headings.
pub(crate) fn wants_contents(frontmatter: &Value, sections: usize) -> bool {
    frontmatter.get("toc").and_then(Value::as_bool) != Some(false)
        && frontmatter.get("tabs").is_none()
        && sections >= MIN_SECTIONS
}

/// Whether the report shows the reading-progress bar: on unless `progress: false`.
pub(crate) fn wants_progress(frontmatter: &Value) -> bool {
    frontmatter.get("progress").and_then(Value::as_bool) != Some(false)
}

/// The contents twice: a rail beside the body on a wide screen, and a closed fold under the
/// header on a narrow one. The stylesheet shows one of them, so a reader meets one list.
pub(crate) fn contents(sections: &[(String, String)]) -> String {
    let items: String = sections
        .iter()
        .map(|(id, text)| format!("<li><a href=\"#{}\">{text}</a></li>", crate::escape(id)))
        .collect();
    format!(
        "<nav class=\"rv-toc\" aria-label=\"Contents\"><p class=\"rv-toc__title\" aria-hidden=\"true\">Contents</p><ol>{items}</ol></nav><details class=\"rv-toc-fold\"><summary>Contents</summary><nav aria-label=\"Contents\"><ol>{items}</ol></nav></details>"
    )
}

/// The progress bar: empty until the runtime starts it. The read part is in link ink over a track
/// in the soft hairline colour, always quieter than the read part, and a report with contents has a 2 px gap in the page colour where each
/// section after the first starts. One colour, because in a chart report the palette means series.
pub(crate) const PROGRESS: &str = "<div class=\"rv-progress\" aria-hidden=\"true\"></div>";

/// The rail sits in its own column beside the header and body from 1100 px, so the reader sees
/// the map from the first screen; below that, the fold takes its place under the header. Paper has neither: a printed page has no scroll to follow.
pub(crate) const READING_CSS: &str = r#".rv-toc,.rv-toc-fold{font-size:var(--rv-size-small);line-height:1.4}
.rv-toc ol,.rv-toc-fold ol{list-style:none;margin:0;padding:0}
.rv-toc a,.rv-toc-fold a{position:relative;display:block;color:var(--rv-ink-soft);text-decoration:none}
.rv-toc a:hover,.rv-toc-fold a:hover{color:var(--rv-ink)}
.rv-toc{display:none}
.rv-toc-fold{width:100%;max-width:var(--rv-measure-w);border-block-end:1px solid var(--rv-rule)}
.rv-toc-fold summary{cursor:pointer;padding-block:.5rem;color:var(--rv-ink-soft);font-weight:600}
.rv-toc-fold ol{padding-block:0 .6rem}
.rv-toc-fold a{padding-block:.3rem}
@media screen{.rv-report .rv-toc-fold{margin-inline:auto}}
@media screen and (min-width:1100px){
.rv-report--toc{max-width:calc(var(--rv-report-max-width,980px) + 15rem);grid-template-columns:minmax(0,1fr) 12rem;column-gap:3rem}
.rv-report--toc>:is(.rv-report__header,.rv-report__body,.rv-report__footer){grid-column:1}
.rv-report--toc>.rv-toc-fold{display:none}
.rv-report--toc>.rv-toc{display:block;grid-column:2;grid-row:1 / span 2;align-self:start;position:sticky;top:2rem;max-height:calc(100vh - 4rem);overflow-y:auto}
.rv-toc__title{margin:0 0 .5rem;font-weight:600;color:var(--rv-ink)}
.rv-toc a{padding-block:.3rem;padding-inline-start:.9rem}
.rv-toc a[aria-current]{color:var(--rv-ink)}
.rv-toc a[aria-current]::before{content:"";position:absolute;inset-inline-start:0;top:calc(.3rem + .5lh - .2rem);width:.4rem;height:.4rem;border-radius:50%;background:currentColor}
}
.rv-progress{position:fixed;inset:auto 0 0;height:3px;z-index:20;pointer-events:none}
.rv-progress[data-live]{background:var(--rv-rule-soft,var(--rv-rule))}
.rv-progress::before{content:"";position:absolute;inset:0;background:var(--rv-link);clip-path:inset(0 calc(100% - var(--rv-read,0) * 100%) 0 0)}
.rv-progress>span{position:absolute;inset-block:0;width:2px;margin-inline-start:-1px;background:var(--rv-bg)}
@media print{.rv-toc,.rv-toc-fold,.rv-progress{display:none}}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sections_are_the_top_level_headings_without_markup_or_notes() {
        let body = "<h2 id=\"verdict\">The <em>verdict</em></h2>\n<p>x</p><h3 id=\"sub\">Sub</h3><h2 id=\"money\">Money &amp; risk</h2><section class=\"rv-notes\" aria-labelledby=\"rv-notes-title\"><h2 id=\"rv-notes-title\">Notes</h2></section>";
        assert_eq!(
            sections(body),
            vec![
                ("verdict".to_owned(), "The verdict".to_owned()),
                ("money".to_owned(), "Money &amp; risk".to_owned())
            ]
        );
    }

    #[test]
    fn contents_need_three_sections_no_tabs_and_no_opt_out() {
        assert!(wants_contents(&json!({}), 3));
        assert!(!wants_contents(&json!({}), 2));
        assert!(!wants_contents(&json!({"toc": false}), 9));
        assert!(!wants_contents(&json!({"tabs": {"panels": []}}), 9));
        assert!(wants_progress(&json!({})));
        assert!(!wants_progress(&json!({"progress": false})));
    }

    #[test]
    fn the_contents_link_each_section_in_the_rail_and_the_fold() {
        let html = contents(&[("a".into(), "A".into()), ("b".into(), "B".into())]);
        assert_eq!(html.matches("<a href=\"#a\">A</a>").count(), 2);
        assert!(html.starts_with("<nav class=\"rv-toc\" aria-label=\"Contents\">"));
        assert!(html.contains("<details class=\"rv-toc-fold\"><summary>Contents</summary>"));
    }

    #[test]
    fn the_bar_is_one_ink_on_a_soft_track() {
        // One colour, not the palette: in a chart report the palette means series (FEA-BR-34
        // keeps accent fills for chart controls). The track shows only once the runtime runs.
        assert!(
            READING_CSS
                .contains(".rv-progress[data-live]{background:var(--rv-rule-soft,var(--rv-rule))}")
        );
        assert!(READING_CSS.contains(
            "background:var(--rv-link);clip-path:inset(0 calc(100% - var(--rv-read,0) * 100%) 0 0)"
        ));
        assert!(READING_CSS.contains(".rv-progress>span{position:absolute;inset-block:0;width:2px;margin-inline-start:-1px;background:var(--rv-bg)}"));
        assert!(!READING_CSS.contains("gradient("));
        assert!(!READING_CSS.contains("--rv-accent"));
    }
}
