//! SVG text helpers the painters share: XML escaping, and resource names kept apart when several
//! drawings share one page.

/// `text` with the five XML-special characters escaped, for text and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Isolate the resources in one generated SVG before placing it beside other snapshots.
/// Generated identifiers are ASCII tokens; captions and text already use XML escaping.
pub fn namespace_resources(svg: &str, prefix: &str) -> String {
    let ids: std::collections::BTreeMap<_, _> = svg
        .split(" id=\"")
        .skip(1)
        .filter_map(|part| part.split('"').next())
        .map(|id| (id, format!("{prefix}-{id}")))
        .collect();
    if ids.is_empty() {
        return svg.to_owned();
    }
    let resource_urls = |value: &str| {
        let mut parts = value.split("url(#");
        let mut output = parts.next().unwrap_or_default().to_owned();
        for part in parts {
            output.push_str("url(#");
            if let Some((id, rest)) = part.split_once(')') {
                output.push_str(ids.get(id).map(String::as_str).unwrap_or(id));
                output.push(')');
                output.push_str(rest);
            } else {
                output.push_str(part);
            }
        }
        output
    };
    // Only resource-bearing attributes change. A title, description or accessible label may
    // legitimately contain the literal text "url(#clip)" and must keep its authored meaning.
    let mut output = String::with_capacity(svg.len());
    let mut remaining = svg;
    while let Some(start) = remaining.find('<') {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        let Some(end) = remaining.find('>') else { break };
        let mut tag = remaining[..=end].to_owned();
        for attribute in [
            "id",
            "href",
            "xlink:href",
            "fill",
            "stroke",
            "clip-path",
            "mask",
            "filter",
            "marker-start",
            "marker-mid",
            "marker-end",
            "style",
            "aria-labelledby",
            "aria-describedby",
        ] {
            let needle = format!(" {attribute}=\"");
            let Some(start) = tag.find(&needle).map(|at| at + needle.len()) else {
                continue;
            };
            let Some(end) = tag[start..].find('"').map(|len| start + len) else {
                continue;
            };
            let value = &tag[start..end];
            let replacement = match attribute {
                "id" => ids.get(value).cloned().unwrap_or_else(|| value.into()),
                "href" | "xlink:href" => value
                    .strip_prefix('#')
                    .and_then(|id| ids.get(id))
                    .map(|id| format!("#{id}"))
                    .unwrap_or_else(|| value.into()),
                "aria-labelledby" | "aria-describedby" => value
                    .split_whitespace()
                    .map(|id| ids.get(id).map(String::as_str).unwrap_or(id))
                    .collect::<Vec<_>>()
                    .join(" "),
                _ => resource_urls(value),
            };
            tag.replace_range(start..end, &replacement);
        }
        output.push_str(&tag);
        remaining = &remaining[end + 1..];
    }
    output.push_str(remaining);
    output
}
