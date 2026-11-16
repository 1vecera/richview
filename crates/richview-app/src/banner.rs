//! The findings banner above a preview (FEA-SF-08, FEA-SF-23): outside the report's own HTML, so a
//! report that fails to render keeps its last good render with the reason on top.
use richview_core::document::Diagnostic;

const MAX_LINES: usize = 20;

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `html` with the banner right after the opening `<body>` tag; `html` unchanged when there is
/// nothing to say or no body to put it in.
pub fn inject(html: &str, diagnostics: &[Diagnostic]) -> String {
    let shown: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.level != "info").collect();
    if shown.is_empty() {
        return html.to_owned();
    }
    let Some(start) = html.find("<body") else {
        return html.to_owned();
    };
    let Some(end) = html[start..].find('>') else {
        return html.to_owned();
    };
    let at = start + end + 1;
    let mut lines: Vec<String> = shown
        .iter()
        .take(MAX_LINES)
        .map(|d| {
            // A finding in a data or style file names that file (TYP-56).
            let place = match (&d.file, d.line) {
                (Some(file), Some(l)) => format!("{} line {l} ", escape(file)),
                (None, Some(l)) => format!("line {l} "),
                _ => String::new(),
            };
            format!(
                "<div>{place}{}: {}</div>",
                escape(&d.code),
                escape(&d.message)
            )
        })
        .collect();
    if shown.len() > MAX_LINES {
        lines.push(format!("<div>and {} more</div>", shown.len() - MAX_LINES));
    }
    let tone = if shown.iter().any(|d| d.level == "error") {
        "background:#fff1ef;color:#7a1a0c;border-color:#e0a79d"
    } else {
        "background:#fff9e8;color:#5c4400;border-color:#e3cd8a"
    };
    let banner = format!(
        "<div id=\"richview-banner\" role=\"alert\" style=\"position:sticky;top:0;z-index:2147483647;margin:0;padding:8px 16px;border-bottom:1px solid;font:13px/1.4 ui-monospace,monospace;max-height:30vh;overflow:auto;{tone}\">{}</div>",
        lines.join("")
    );
    format!("{}{banner}{}", &html[..at], &html[at..])
}

/// The diagnostics a render failure carries: its JSON-encoded list, or the text as one finding.
pub fn diagnostics_of(failure: &str) -> Vec<Diagnostic> {
    serde_json::from_str(failure).unwrap_or_else(|_| {
        vec![Diagnostic::new(
            "error",
            "render.failed",
            failure.to_owned(),
            "source",
        )]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_banner_sits_after_the_body_tag_and_escapes_the_message() {
        let diagnostics = vec![Diagnostic::new(
            "error",
            "block.schema",
            "<b>bad</b>",
            "chart",
        )];
        let html = inject(
            "<html><body class=\"x\"><h1>Report</h1></body></html>",
            &diagnostics,
        );
        assert!(html.starts_with("<html><body class=\"x\"><div id=\"richview-banner\""));
        assert!(html.contains("&lt;b&gt;bad&lt;/b&gt;"));
        assert!(html.ends_with("<h1>Report</h1></body></html>"));
    }

    #[test]
    fn a_finding_in_a_data_file_names_the_file_escaped() {
        let mut finding = Diagnostic::new("error", "data.csv", "Ragged.", "chart.data.csv")
            .in_file("data/<a>.csv");
        finding.line = Some(3);
        let html = inject("<body></body>", &[finding]);
        assert!(
            html.contains("<div>data/&lt;a&gt;.csv line 3 data.csv: Ragged.</div>"),
            "{html}"
        );
    }

    #[test]
    fn nothing_to_report_leaves_the_html_alone() {
        let info = vec![Diagnostic::new("info", "x", "fine", "p")];
        assert_eq!(inject("<body>a</body>", &info), "<body>a</body>");
        assert_eq!(inject("<body>a</body>", &[]), "<body>a</body>");
    }

    #[test]
    fn a_failure_text_becomes_one_finding() {
        assert_eq!(diagnostics_of("disk full")[0].code, "render.failed");
        let encoded = serde_json::to_string(&[Diagnostic::new("error", "a.b", "m", "p")]).unwrap();
        assert_eq!(diagnostics_of(&encoded)[0].code, "a.b");
    }
}
