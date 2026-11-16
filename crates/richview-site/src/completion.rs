//! What the live example's editor offers at the caret (W3 scene 2, S27-931). Keys, values and a
//! block's data fields come from `richview_authoring::complete`, the editor service's own
//! completion, and the tag after a fence from core's registry, so the page never offers what the
//! check rejects (ARC-09). `browser/complete.rs` shows the list; this part is pure, and its tests
//! run natively.
use richview_core::document::RenderOptions;

/// One choice: the text the list shows, the text it puts in, and the stretch of the source it
/// replaces, in UTF-16 units from the start of the source, as a text area counts them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Choice {
    pub label: String,
    pub insert: String,
    pub start: usize,
    pub end: usize,
}

/// The line of a UTF-16 offset into `source`, and its character in that line.
pub(crate) fn position(source: &str, caret: usize) -> (usize, usize) {
    let (mut line, mut character, mut units) = (0, 0, 0);
    for c in source.chars() {
        if units >= caret {
            break;
        }
        units += c.len_utf16();
        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += c.len_utf16();
        }
    }
    (line, character)
}

/// The UTF-16 offset of a line and a character in it; past the end of either, its end.
pub(crate) fn offset(source: &str, line: usize, character: usize) -> usize {
    let mut units = 0;
    for (index, text) in source.split('\n').enumerate() {
        let length = text.encode_utf16().count();
        if index == line {
            return units + character.min(length);
        }
        units += length + 1;
    }
    units.saturating_sub(1)
}

/// What the editor offers at `caret`. While the reader types, the list opens once a word has
/// begun, and for a value or a fence's tag as soon as its slot is open (after `key: ` or three
/// backticks); `asked` (Ctrl+Space) opens it wherever there is something to offer. A word typed
/// out in full closes it, since there is nothing left to complete.
pub(crate) fn at(source: &str, caret: usize, options: &RenderOptions, asked: bool) -> Vec<Choice> {
    let (line, character) = position(source, caret);
    let current = source.split('\n').nth(line).unwrap_or_default();
    let before: String =
        String::from_utf16_lossy(&current.encode_utf16().take(character).collect::<Vec<_>>());
    // On a fence line only an opening fence's tag is offered: a closing fence takes nothing.
    let choices = if current.trim_start().starts_with("```") {
        fence_tags(source, line, &before)
            .unwrap_or_default()
            .into_iter()
            .map(|(label, from)| Choice {
                insert: label.clone(),
                label,
                start: offset(source, line, from),
                end: offset(source, line, word_end(current, character)),
            })
            .collect()
    } else {
        let document = richview_core::document::parse(source)
            .or_else(|_| richview_core::document::parse(""))
            .expect("an empty source parses");
        let found = richview_authoring::complete(source, line, character, &document, options);
        found
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| {
                let at = |end: &str| {
                    let point = &item["range"][end];
                    Some(offset(
                        source,
                        point["line"].as_u64()? as usize,
                        point["character"].as_u64()? as usize,
                    ))
                };
                Some(Choice {
                    label: item["label"].as_str()?.to_owned(),
                    insert: item["insert_text"].as_str()?.to_owned(),
                    start: at("start")?,
                    end: at("end")?,
                })
            })
            .collect::<Vec<_>>()
    };
    let Some(first) = choices.first() else {
        return choices;
    };
    let typed: String = String::from_utf16_lossy(
        &source
            .encode_utf16()
            .skip(first.start)
            .take(caret.saturating_sub(first.start))
            .collect::<Vec<_>>(),
    );
    let slot_open = before.trim_start().starts_with("```") || before.ends_with(": ");
    if !asked && typed.is_empty() && !slot_open {
        return vec![];
    }
    if !asked && choices.len() == 1 && choices[0].label == typed {
        return vec![];
    }
    choices
}

/// The block tags that may follow an opening fence the caret is typing, with the character where
/// the tag starts; none on a closing fence or anywhere else.
fn fence_tags(source: &str, line: usize, before: &str) -> Option<Vec<(String, usize)>> {
    let indent = before.len() - before.trim_start().len();
    let tag = before.trim_start().strip_prefix("```")?;
    if !tag
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return None;
    }
    // An odd number of fences above means this one closes a block.
    let fences = source
        .split('\n')
        .take(line)
        .filter(|text| text.trim_start().starts_with("```"))
        .count();
    if fences % 2 == 1 {
        return None;
    }
    let from = before[..indent].encode_utf16().count() + 3;
    Some(
        richview_core::RICH_TAGS
            .iter()
            .filter(|name| name.starts_with(tag))
            .map(|name| ((*name).to_owned(), from))
            .collect(),
    )
}

/// The character where the word at `character` ends, as the authoring crate counts a word.
fn word_end(line: &str, character: usize) -> usize {
    let mut units = 0;
    let mut end = None;
    for c in line.chars() {
        if units >= character && end.is_none() && !(c.is_alphanumeric() || matches!(c, '_' | '-')) {
            end = Some(units);
        }
        units += c.len_utf16();
    }
    end.unwrap_or(units)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANCHOR: &str = include_str!("../../../assets/landing/anchor/report.rmd");
    const DATA: &str =
        include_str!("../../../assets/landing/anchor/data/life-expectancy-by-region.csv");

    /// The options the live example renders with: its folder and the CSV the page brings.
    fn options() -> RenderOptions {
        let mut options = RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "homepage-demo".into(),
                source: "report.rmd".into(),
                files: vec![
                    "report.rmd".into(),
                    "report.rstyle.yaml".into(),
                    "data/life-expectancy-by-region.csv".into(),
                ],
            }),
            ..Default::default()
        };
        options
            .data
            .insert("data/life-expectancy-by-region.csv".into(), DATA.into());
        options
    }

    /// The anchor with `typed` put in place of `replace`, and the caret at the end of `typed`.
    fn typing(replace: &str, typed: &str) -> (String, usize) {
        let at = ANCHOR
            .find(replace)
            .expect("the anchor has the text to replace");
        let source = format!("{}{typed}{}", &ANCHOR[..at], &ANCHOR[at + replace.len()..]);
        let caret = ANCHOR[..at].encode_utf16().count() + typed.encode_utf16().count();
        (source, caret)
    }

    fn labels(choices: &[Choice]) -> Vec<&str> {
        choices.iter().map(|choice| choice.label.as_str()).collect()
    }

    #[test]
    fn positions_count_utf16_units_as_a_text_area_does() {
        let source = "a📈b\nčd\n";
        assert_eq!(position(source, 0), (0, 0));
        assert_eq!(position(source, 3), (0, 3));
        assert_eq!(position(source, 5), (1, 0));
        assert_eq!(position(source, 7), (1, 2));
        // Every caret between characters round-trips; a text area never puts one inside a pair.
        let carets = source
            .char_indices()
            .map(|(at, _)| source[..at].encode_utf16().count());
        for caret in carets.chain([source.encode_utf16().count()]) {
            let (line, character) = position(source, caret);
            assert_eq!(offset(source, line, character), caret, "caret {caret}");
        }
        assert_eq!(offset(source, 9, 0), source.encode_utf16().count());
    }

    #[test]
    fn a_chart_type_is_offered_after_type_and_every_one_is_registered() {
        let (source, caret) = typing("type: line", "type: ");
        let choices = at(&source, caret, &options(), false);
        let names: Vec<&str> = richview_core::registry::CHART_TYPES
            .iter()
            .map(|chart| chart.name)
            .collect();
        assert!(labels(&choices).contains(&"line"), "{choices:?}");
        assert!(labels(&choices).iter().all(|label| names.contains(label)));
        let (source, caret) = typing("type: line", "type: li");
        let choices = at(&source, caret, &options(), false);
        assert!(labels(&choices).iter().all(|label| label.starts_with("li")));
        let start = source[..source.find("type: li").unwrap() + 6]
            .encode_utf16()
            .count();
        assert!(
            choices
                .iter()
                .all(|choice| choice.start == start && choice.end == caret)
        );
    }

    #[test]
    fn a_field_slot_offers_the_columns_of_the_charts_csv() {
        let (source, caret) = typing("x: year", "x: ");
        let choices = at(&source, caret, &options(), false);
        for column in ["year", "life", "region"] {
            assert!(labels(&choices).contains(&column), "{column}: {choices:?}");
        }
    }

    #[test]
    fn a_block_offers_only_keys_it_takes_and_does_not_have() {
        let (source, caret) = typing("style: { ratio: \"2:1\" }", "s");
        let choices = at(&source, caret, &options(), false);
        let keys = richview_core::authoring_keys("chart", &[]).expect("chart keys");
        assert!(!choices.is_empty());
        for choice in &choices {
            assert!(choice.label.starts_with('s'), "{choice:?}");
            assert!(keys.contains(&choice.label.as_str()), "{choice:?}");
            assert_ne!(choice.label, "subtitle", "the chart has a subtitle already");
        }
    }

    #[test]
    fn a_fence_offers_the_registered_tags_and_a_closing_fence_nothing() {
        let (source, caret) = typing("```table", "```");
        let choices = at(&source, caret, &options(), false);
        assert!(labels(&choices).contains(&"table") && labels(&choices).contains(&"chart"));
        assert!(
            labels(&choices)
                .iter()
                .all(|label| richview_core::RICH_TAGS.contains(label))
        );
        let (source, caret) = typing("```table", "```ta");
        assert_eq!(labels(&at(&source, caret, &options(), false)), ["table"]);
        let (source, caret) = typing("```table", "```tab");
        let tag = at(&source, caret, &options(), false);
        assert_eq!(
            tag[0].end - tag[0].start,
            3,
            "the whole word is replaced: {tag:?}"
        );
        // The chart's closing fence.
        let close = ANCHOR.find("```\n\n*Source").unwrap() + 3;
        let caret = ANCHOR[..close].encode_utf16().count();
        assert!(at(ANCHOR, caret, &options(), true).is_empty());
    }

    #[test]
    fn prose_a_finished_word_and_an_empty_slot_open_nothing_until_asked() {
        let prose = ANCHOR.find("A newborn").unwrap() + 3;
        let caret = ANCHOR[..prose].encode_utf16().count();
        assert!(at(ANCHOR, caret, &options(), true).is_empty());
        let (source, caret) = typing("type: line", "type: line");
        assert!(at(&source, caret, &options(), false).is_empty());
        // The start of an empty line in the chart block lists its keys only when asked.
        let (source, caret) = typing("style: { ratio: \"2:1\" }", "");
        assert!(at(&source, caret, &options(), false).is_empty());
        assert!(!at(&source, caret, &options(), true).is_empty());
    }
}
