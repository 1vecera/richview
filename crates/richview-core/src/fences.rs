//! Rich fences in Markdown: which are top-level blocks, which sit where a block cannot
//! (FEA-RP-53), and which are a misspelled tag (FEA-RP-57).

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};

/// One fenced code block whose info string is a rich tag.
pub(crate) struct Fence {
    /// Byte range of the whole fence in the scanned text.
    pub start: usize,
    pub end: usize,
    pub tag: String,
    /// The YAML between the fence lines.
    pub raw: String,
    /// Inside a list, a block quote or a footnote, where a rich block cannot render.
    pub nested: bool,
}

/// A fence whose info string is one edit away from a rich tag.
pub(crate) struct NearMiss {
    pub offset: usize,
    pub info: String,
    pub tag: &'static str,
}

fn container(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::List(_)
            | Tag::Item
            | Tag::BlockQuote(_)
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListDefinition
    )
}

fn container_end(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::List(_)
            | TagEnd::Item
            | TagEnd::BlockQuote(_)
            | TagEnd::FootnoteDefinition
            | TagEnd::DefinitionList
            | TagEnd::DefinitionListDefinition
    )
}

/// Every rich fence of `text`, in order, and the near-miss tags. CommonMark decides what is a
/// fence (indentation, longer fences, fences inside examples).
pub(crate) fn scan(text: &str, tags: &[&str]) -> (Vec<Fence>, Vec<NearMiss>) {
    let mut fences = vec![];
    let mut misses = vec![];
    let mut depth = 0usize;
    let mut current: Option<Fence> = None;
    for (event, range) in Parser::new_ext(text, crate::markdown::FLAGS).into_offset_iter() {
        match event {
            Event::Start(ref tag) if container(tag) => depth += 1,
            Event::End(ref tag) if container_end(tag) => depth = depth.saturating_sub(1),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let info = info.trim().to_lowercase();
                if tags.contains(&info.as_str()) {
                    current = Some(Fence {
                        start: range.start,
                        end: range.end,
                        tag: info,
                        raw: String::new(),
                        nested: depth > 0,
                    });
                } else if let Some(tag) = near_miss(&info, tags) {
                    misses.push(NearMiss {
                        offset: range.start,
                        info,
                        tag,
                    });
                }
            }
            Event::Text(raw) if current.is_some() => {
                if let Some(fence) = current.as_mut() {
                    fence.raw.push_str(&raw);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(mut fence) = current.take() {
                    fence.end = fence.end.max(range.end);
                    fences.push(fence);
                }
            }
            _ => {}
        }
    }
    (fences, misses)
}

/// The rich tag `info` is one edit from: one letter different, one missing or extra (a plural
/// `s` included), or two adjacent letters swapped. Other info strings are ordinary code.
fn near_miss(info: &str, tags: &[&str]) -> Option<&'static str> {
    const LIVE: [&str; 8] = [
        "chart", "table", "metric", "callout", "image", "audio", "video", "embed",
    ];
    if info.len() < 4 || !info.is_ascii() || tags.contains(&info) {
        return None;
    }
    LIVE.into_iter().find(|tag| within_one_edit(info, tag))
}

fn within_one_edit(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    match a.len().cmp(&b.len()) {
        std::cmp::Ordering::Equal => {
            let diff: Vec<usize> = (0..a.len()).filter(|&i| a[i] != b[i]).collect();
            match diff.as_slice() {
                [_] => true,
                [i, j] => *j == i + 1 && a[*i] == b[*j] && a[*j] == b[*i],
                _ => false,
            }
        }
        std::cmp::Ordering::Greater if a.len() == b.len() + 1 => skip_one(a, b),
        std::cmp::Ordering::Less if b.len() == a.len() + 1 => skip_one(b, a),
        _ => false,
    }
}

/// Whether removing one byte of `long` gives `short`.
fn skip_one(long: &[u8], short: &[u8]) -> bool {
    let prefix = long.iter().zip(short).take_while(|(x, y)| x == y).count();
    long[prefix + 1..] == short[prefix..]
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAGS: &[&str] = &["chart", "table", "metric", "callout"];

    #[test]
    fn near_misses_are_one_edit_away() {
        for (info, tag) in [
            ("tabel", "table"),
            ("charts", "chart"),
            ("metrc", "metric"),
            ("tble", "table"),
            ("tablee", "table"),
        ] {
            assert_eq!(near_miss(info, TAGS), Some(tag), "{info}");
        }
        for info in ["python", "mermaid", "sql", "json", "yaml", "text", "table"] {
            assert!(near_miss(info, &["table"]).is_none(), "{info}");
        }
        assert!(near_miss("mermaid", TAGS).is_none());
    }

    #[test]
    fn a_fence_in_a_list_or_quote_is_nested() {
        let text = "```chart\na: 1\n```\n\n- item\n\n  ```table\n  b: 2\n  ```\n\n> ```metric\n> c: 3\n> ```\n";
        let (fences, _) = scan(text, TAGS);
        let nested: Vec<(&str, bool)> = fences.iter().map(|f| (f.tag.as_str(), f.nested)).collect();
        assert_eq!(
            nested,
            [("chart", false), ("table", true), ("metric", true)]
        );
    }
}
