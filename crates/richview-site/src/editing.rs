//! The hosted editor's source edits and rules as pure functions on text: the next version number,
//! the frontmatter lines the editor writes, what a style pill does to the `styles` line, the
//! blocks the Insert strip adds and the starter of a new report. The page logic is in the browser
//! module; everything here is tested natively.

/// The source a new report starts from: one finding and one chart with its values inline, so it
/// renders at once and needs no file.
pub(crate) const STARTER: &str = concat!(
    "---\n",
    "title: Untitled report\n",
    "description: What this report shows, in one sentence.\n",
    "version: 0.0.1\n",
    "styles: [sleek]\n",
    "---\n",
    "\n",
    "## What changed\n",
    "\n",
    "Write the finding first, then show the evidence. Change this text and the report on the right follows as you type.\n",
    "\n",
    "```chart\n",
    "type: bar\n",
    "title: Orders by month\n",
    "data:\n",
    "  inline:\n",
    "    - month: Jan\n",
    "      orders: 120\n",
    "    - month: Feb\n",
    "      orders: 150\n",
    "    - month: Mar\n",
    "      orders: 190\n",
    "encoding: { x: month, y: orders }\n",
    "axes:\n",
    "  y: { label: Orders }\n",
    "```\n",
    "\n",
    "*Source: invented example values.*\n",
);

/// The styles the pills offer, in the order the pills show them (STY-07, STY-08).
pub(crate) const PILL_STYLES: [&str; 5] = ["sleek", "air", "scientific", "brutalist", "hand-drawn"];

/// A block the Insert strip adds, with its values inline and its source note, so adding it makes
/// no finding and needs no file.
pub(crate) fn block(name: &str) -> Option<&'static str> {
    Some(match name {
        "chart" => concat!(
            "```chart\n",
            "type: line\n",
            "title: A value over time\n",
            "data:\n",
            "  inline:\n",
            "    - month: 2026-01\n",
            "      value: 10\n",
            "    - month: 2026-02\n",
            "      value: 14\n",
            "    - month: 2026-03\n",
            "      value: 19\n",
            "encoding: { x: month, y: value }\n",
            "axes:\n",
            "  y: { label: Value }\n",
            "```\n",
            "\n",
            "*Source: invented example values.*\n",
        ),
        "table" => concat!(
            "```table\n",
            "title: Sales by region\n",
            "data:\n",
            "  inline:\n",
            "    - region: North\n",
            "      sales: 120\n",
            "    - region: South\n",
            "      sales: 95\n",
            "columns:\n",
            "  - key: region\n",
            "    label: Region\n",
            "  - key: sales\n",
            "    label: Sales\n",
            "    align: right\n",
            "```\n",
            "\n",
            "*Source: invented example values.*\n",
        ),
        "metric" => concat!(
            "```metric\n",
            "value: 1234\n",
            "label: Visits today\n",
            "delta: 0.12\n",
            "trend: up\n",
            "```\n",
        ),
        "callout" => concat!(
            "```callout\n",
            "kind: neutral\n",
            "title: A note\n",
            "body: |\n",
            "  Say what a reader should take from the evidence above.\n",
            "```\n",
        ),
        _ => return None,
    })
}

/// The next patch number after `version`, as the save offers it: `0.0.1` becomes `0.0.2`. A
/// prerelease or build part is dropped. `None` when the text is not three numbers.
pub(crate) fn next_patch(version: &str) -> Option<String> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let (major, minor, patch) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let number = |part: &str| part.parse::<u64>().ok();
    Some(format!(
        "{}.{}.{}",
        number(major)?,
        number(minor)?,
        number(patch)?.checked_add(1)?
    ))
}

/// The three numbers of a plain version, or `None` for anything else.
fn numbers(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let found = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(found)
}

/// Whether version `a` comes after version `b`; versions that are not three numbers never do.
pub(crate) fn newer(a: &str, b: &str) -> bool {
    matches!((numbers(a), numbers(b)), (Some(a), Some(b)) if a > b)
}

/// The smallest change that turns `old` into `new`: the stretch of `old` to replace, counted in
/// UTF-16 units as a text area counts, and the text that replaces it. `None` when they are equal.
pub(crate) fn change(old: &str, new: &str) -> Option<(usize, usize, String)> {
    if old == new {
        return None;
    }
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let old_rest: Vec<char> = old.chars().skip(prefix).collect();
    let new_rest: Vec<char> = new.chars().skip(prefix).collect();
    let suffix = old_rest
        .iter()
        .rev()
        .zip(new_rest.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let start: usize = old.chars().take(prefix).map(char::len_utf16).sum();
    let removed = &old_rest[..old_rest.len() - suffix];
    let end = start + removed.iter().map(|c| c.len_utf16()).sum::<usize>();
    let text = new_rest[..new_rest.len() - suffix].iter().collect();
    Some((start, end, text))
}

/// The byte range of the frontmatter's body: from the line after the opening fence to the start
/// of the closing fence line. `None` when the source has no frontmatter.
fn frontmatter(source: &str) -> Option<(usize, usize)> {
    let mut lines = source.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let start = first.len();
    let mut at = start;
    for line in lines {
        if line.trim_end() == "---" {
            return Some((start, at));
        }
        at += line.len();
    }
    None
}

/// The line ending the frontmatter uses, so an inserted line matches its neighbours.
fn newline(source: &str, body_start: usize) -> &'static str {
    if source[..body_start].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// A scalar without its quotes.
fn unquote(text: &str) -> &str {
    let text = text.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner;
        }
    }
    text
}

/// The text of a frontmatter key written on one line, without quotes. `None` when the key is
/// absent or its value is not on the key's line (a block or a list).
pub(crate) fn frontmatter_value(source: &str, key: &str) -> Option<String> {
    let (start, close) = frontmatter(source)?;
    let prefix = format!("{key}:");
    source[start..close].lines().find_map(|line| {
        let value = unquote(line.strip_prefix(&prefix)?);
        (!value.is_empty() && !value.starts_with(['|', '>', '[', '{'])).then(|| value.to_owned())
    })
}

/// The source with the frontmatter key set on one line: the key's line is replaced, or the line
/// is added before the closing fence. A source with no frontmatter gets one.
pub(crate) fn set_frontmatter(source: &str, key: &str, value: &str) -> String {
    let Some((start, close)) = frontmatter(source) else {
        return format!("---\n{key}: {value}\n---\n\n{source}");
    };
    let nl = newline(source, start);
    let prefix = format!("{key}:");
    let mut out = String::with_capacity(source.len() + key.len() + value.len() + 4);
    out.push_str(&source[..start]);
    let mut done = false;
    for line in source[start..close].split_inclusive('\n') {
        if !done && line.starts_with(&prefix) {
            let ending = &line[line.trim_end().len()..];
            out.push_str(&format!("{key}: {value}{ending}"));
            done = true;
        } else {
            out.push_str(line);
        }
    }
    if !done {
        out.push_str(&format!("{key}: {value}{nl}"));
    }
    out.push_str(&source[close..]);
    out
}

/// The entries of a `styles` value written on its key's line: a list in brackets or one scalar.
fn flow_entries(value: &str) -> Vec<String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(value);
    inner
        .split(',')
        .map(|entry| unquote(entry).to_owned())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// The entries of the frontmatter's `styles`, whether written in brackets, as one scalar or as a
/// block list.
fn style_entries(source: &str) -> Vec<String> {
    let Some((start, close)) = frontmatter(source) else {
        return vec![];
    };
    let lines: Vec<&str> = source[start..close].lines().collect();
    let Some(at) = lines.iter().position(|line| line.starts_with("styles:")) else {
        return vec![];
    };
    let mut entries = flow_entries(lines[at]["styles:".len()..].trim());
    for line in &lines[at + 1..] {
        match line
            .strip_prefix(' ')
            .or_else(|| line.strip_prefix('\t'))
            .map(str::trim_start)
            .and_then(|rest| rest.strip_prefix("- "))
        {
            Some(entry) => entries.push(unquote(entry).to_owned()),
            None => break,
        }
    }
    entries
}

/// The pill style the source declares first, if it declares one.
pub(crate) fn declared_style(source: &str) -> Option<String> {
    style_entries(source)
        .into_iter()
        .find(|entry| PILL_STYLES.contains(&entry.as_str()))
}

/// The pill to show checked: the style the source declares, or the default one when it names no
/// style at all. A source that names only style files of its own has no pill checked.
pub(crate) fn shown_style(source: &str) -> Option<String> {
    declared_style(source).or_else(|| {
        style_entries(source)
            .is_empty()
            .then(|| PILL_STYLES[0].to_owned())
    })
}

/// The source with `styles` set to the builtin `name`, keeping the style files it lists.
pub(crate) fn with_style(source: &str, name: &str) -> String {
    let files: Vec<String> = style_entries(source)
        .into_iter()
        .filter(|entry| !richview_core::builtin_style_names().contains(&entry.as_str()))
        .collect();
    let value = format!(
        "[{}]",
        std::iter::once(name)
            .chain(files.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let Some((start, close)) = frontmatter(source) else {
        return set_frontmatter(source, "styles", &value);
    };
    let nl = newline(source, start);
    let lines: Vec<&str> = source[start..close].split_inclusive('\n').collect();
    let mut out = String::from(&source[..start]);
    let mut done = false;
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        at += 1;
        if line.starts_with("styles:") && !done {
            done = true;
            out.push_str(&format!("styles: {value}{nl}"));
            // A block list's items belong to the key and go with it.
            while at < lines.len() && is_item(lines[at]) {
                at += 1;
            }
        } else {
            out.push_str(line);
        }
    }
    if !done {
        out.push_str(&format!("styles: {value}{nl}"));
    }
    out.push_str(&source[close..]);
    out
}

/// What the key of every editor draft in a browser starts with.
pub(crate) const DRAFT_PREFIX: &str = "richview.draft.";

/// The key a person's unsaved draft of a report is kept under in the browser: theirs alone, then
/// the organization, then the report's address. A report not saved yet is kept under a name no
/// address can have (an address is letters, digits and dashes), so it never meets the draft of a
/// stored report whose address is `new`. `None` when the page did not say who is editing, which
/// keeps no draft.
pub(crate) fn draft_key(person: &str, org: &str, slug: &str, is_new: bool) -> Option<String> {
    if person.is_empty() || person.contains('/') {
        return None;
    }
    let name = if is_new { "+new" } else { slug };
    Some(format!("{DRAFT_PREFIX}{person}/{org}/{name}"))
}

/// Whether `key` is a draft that is not `person`'s, written by whichever version of the editor:
/// theirs begin with their id and a slash, anything else under the prefix is somebody else's or
/// from before drafts were kept per person.
pub(crate) fn is_another_persons_draft(key: &str, person: &str) -> bool {
    key.starts_with(DRAFT_PREFIX)
        && (person.is_empty() || !key[DRAFT_PREFIX.len()..].starts_with(&format!("{person}/")))
}

/// Whether the line is an item of a block list.
fn is_item(line: &str) -> bool {
    (line.starts_with(' ') || line.starts_with('\t')) && line.trim_start().starts_with("- ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draft_is_kept_under_the_person_who_wrote_it_and_a_new_report_never_meets_a_stored_one() {
        let ada = draft_key("p-ada", "org1", "q3", false).unwrap();
        assert_eq!(ada, "richview.draft.p-ada/org1/q3");
        // Another person's draft of the same report is another key.
        assert_ne!(draft_key("p-bob", "org1", "q3", false).unwrap(), ada);
        // A new report is a key no address can be, whatever the address field held: a stored
        // report that is called `new` has its own draft.
        let new = draft_key("p-ada", "org1", "", true).unwrap();
        assert_eq!(new, "richview.draft.p-ada/org1/+new");
        assert_ne!(new, draft_key("p-ada", "org1", "new", false).unwrap());
        assert_eq!(
            new,
            draft_key("p-ada", "org1", "typed-address", true).unwrap()
        );
        // Nobody is nobody's: no key, so nothing is kept.
        assert_eq!(draft_key("", "org1", "q3", false), None);
        assert_eq!(draft_key("a/b", "org1", "q3", false), None);
    }

    #[test]
    fn only_the_persons_own_drafts_are_kept_when_the_editor_opens() {
        let theirs = "richview.draft.p-ada/org1/q3";
        assert!(!is_another_persons_draft(theirs, "p-ada"));
        assert!(is_another_persons_draft(theirs, "p-bob"));
        // A person whose id starts with another's is not that person.
        assert!(is_another_persons_draft(
            "richview.draft.p-ada2/org1/q3",
            "p-ada"
        ));
        assert!(is_another_persons_draft(theirs, "p-a"));
        // What an earlier editor wrote, with no person in it, is nobody's now.
        assert!(is_another_persons_draft("richview.draft.org1/q3", "p-ada"));
        assert!(is_another_persons_draft("richview.draft.org1/new", "p-ada"));
        // Other keys of the page are not drafts.
        assert!(!is_another_persons_draft("richview.editor.split", "p-ada"));
        // Without a person every draft goes.
        assert!(is_another_persons_draft(theirs, ""));
    }

    #[test]
    fn the_next_patch_follows_the_version() {
        assert_eq!(next_patch("0.0.1").as_deref(), Some("0.0.2"));
        assert_eq!(next_patch("1.4.9").as_deref(), Some("1.4.10"));
        assert_eq!(next_patch("2.0.0-rc.1").as_deref(), Some("2.0.1"));
        assert_eq!(next_patch("1.2").as_deref(), None);
        assert_eq!(next_patch("1.2.3.4").as_deref(), None);
        assert_eq!(next_patch("one.two.three").as_deref(), None);
        assert_eq!(next_patch(&format!("0.0.{}", u64::MAX)), None);
    }

    #[test]
    fn versions_are_compared_by_their_numbers() {
        assert!(newer("0.0.10", "0.0.9"));
        assert!(newer("1.0.0", "0.9.9"));
        assert!(!newer("0.0.1", "0.0.1"));
        assert!(!newer("0.0.1", "0.0.2"));
        assert!(!newer("x", "0.0.1"));
        assert!(!newer("0.0.1", "1.2"));
    }

    #[test]
    fn a_change_is_the_smallest_stretch_that_differs() {
        assert_eq!(change("same", "same"), None);
        assert_eq!(
            change("a: 1\nb: 2\n", "a: 1\nb: 3\n"),
            Some((8, 9, "3".to_owned()))
        );
        assert_eq!(change("ab", "abc"), Some((2, 2, "c".to_owned())));
        assert_eq!(change("abc", "ab"), Some((2, 3, String::new())));
        assert_eq!(change("aa", "aaa"), Some((2, 2, "a".to_owned())));
        // Units are UTF-16, as a text area counts: the emoji is two.
        assert_eq!(change("a📈b", "a📈cb"), Some((3, 3, "c".to_owned())));
        let old = "---\ntitle: T\n---\nbody\n";
        let new = with_style(old, "air");
        let (start, end, text) = change(old, &new).unwrap();
        let units: Vec<u16> = old.encode_utf16().collect();
        let mut rebuilt: Vec<u16> = units[..start].to_vec();
        rebuilt.extend(text.encode_utf16());
        rebuilt.extend(&units[end..]);
        assert_eq!(String::from_utf16(&rebuilt).unwrap(), new);
    }

    /// The options the editor renders with: one report in its own folder.
    fn options() -> richview_core::document::RenderOptions {
        richview_core::document::RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "report".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn the_starter_is_a_publishable_report_that_renders_clean() {
        let report =
            richview_core::document::render(STARTER, &options()).expect("the starter renders");
        let found: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code != "assets.runtime-external")
            .map(|d| format!("{} {}", d.level, d.code))
            .collect();
        assert!(found.is_empty(), "{found:?}");
        assert_eq!(
            frontmatter_value(STARTER, "version").as_deref(),
            Some("0.0.1")
        );
        assert_eq!(declared_style(STARTER).as_deref(), Some("sleek"));
    }

    #[test]
    fn every_insert_block_renders_without_a_finding() {
        for name in ["chart", "table", "metric", "callout"] {
            let source = format!(
                "---\ntitle: T\ndescription: D\nversion: 0.0.1\n---\n\n{}",
                block(name).unwrap()
            );
            let report = richview_core::document::render(&source, &options())
                .unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let found: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|d| d.code != "assets.runtime-external")
                .map(|d| format!("{} {}", d.level, d.code))
                .collect();
            assert!(found.is_empty(), "{name}: {found:?}");
        }
        assert_eq!(block("image"), None);
    }

    #[test]
    fn a_line_added_to_the_front_matter_is_an_edit_the_buffer_takes() {
        let new = set_frontmatter(STARTER, "slug", "debug-fail-1");
        let (start, end, text) = change(STARTER, &new).expect("the slug is new");
        // The browser may report the insertion in one input event or split at the line break.
        let (head, tail) = text.split_at(text.len() - 1);
        for steps in [vec![text.as_str()], vec![head, tail]] {
            let mut buffer = crate::source_buffer::SourceBuffer::new(STARTER.to_owned());
            let mut at = start;
            for step in steps {
                let mut value = buffer.visible_textarea();
                value.replace_range(at..at, step);
                buffer.update_textarea(value);
                // What the editor reads back after each edit.
                let _ = buffer.visible_lines();
                let _ = buffer.visible_bytes();
                let _ = buffer.visible_textarea();
                at += step.len();
            }
            assert_eq!(buffer.full(), new);
            assert_eq!(end, start, "an insertion removes nothing");
        }
    }

    #[test]
    fn every_state_on_the_way_to_a_new_front_matter_line_can_be_read_and_drawn() {
        // The browser takes an insertion as one input event or two; between them the closing
        // fence can share a line with the text just inserted.
        let new = set_frontmatter(STARTER, "slug", "debug-fail-1");
        let (start, _, text) = change(STARTER, &new).expect("the slug is new");
        for cut in 0..=text.len() {
            let mut state = STARTER.to_owned();
            state.insert_str(start, &text[..cut]);
            let _ = richview_core::highlight_html(
                &state,
                richview_core::HighlightLanguage::RichMarkdown,
            );
            let _ = frontmatter_value(&state, "title");
            let _ = shown_style(&state);
            let _ = declared_style(&state);
            let _ = with_style(&state, "air");
            let _ = richview_core::document::render(&state, &options());
            // The completion list is asked at the caret the insertion leaves.
            let caret = start + cut;
            for asked in [false, true] {
                let _ = crate::completion::at(&state, caret, &options(), asked);
            }
        }
    }

    #[test]
    fn frontmatter_values_are_read_from_their_own_line() {
        let source = "---\ntitle: \"A: quoted title\"\nversion: 0.1.0\nstyles: [air]\ndescription: |\n  long\n---\n\nbody\n";
        assert_eq!(
            frontmatter_value(source, "title").as_deref(),
            Some("A: quoted title")
        );
        assert_eq!(
            frontmatter_value(source, "version").as_deref(),
            Some("0.1.0")
        );
        assert_eq!(frontmatter_value(source, "description"), None);
        assert_eq!(frontmatter_value(source, "styles"), None);
        assert_eq!(frontmatter_value(source, "slug"), None);
        assert_eq!(frontmatter_value("no frontmatter", "title"), None);
    }

    #[test]
    fn a_frontmatter_line_is_replaced_or_added() {
        let source = "---\ntitle: T\nversion: 0.0.1\n---\n\nbody\n";
        assert_eq!(
            set_frontmatter(source, "version", "0.0.2"),
            "---\ntitle: T\nversion: 0.0.2\n---\n\nbody\n"
        );
        assert_eq!(
            set_frontmatter(source, "slug", "revenue"),
            "---\ntitle: T\nversion: 0.0.1\nslug: revenue\n---\n\nbody\n"
        );
        let crlf = "---\r\ntitle: T\r\nversion: 0.0.1\r\n---\r\n\r\nbody\r\n";
        assert_eq!(
            set_frontmatter(crlf, "version", "0.0.2"),
            "---\r\ntitle: T\r\nversion: 0.0.2\r\n---\r\n\r\nbody\r\n"
        );
        assert_eq!(
            set_frontmatter(crlf, "slug", "a"),
            "---\r\ntitle: T\r\nversion: 0.0.1\r\nslug: a\r\n---\r\n\r\nbody\r\n"
        );
        assert_eq!(
            set_frontmatter("body\n", "version", "0.0.1"),
            "---\nversion: 0.0.1\n---\n\nbody\n"
        );
        // A key that only starts like the line's key is another key.
        let near = "---\nversions: x\n---\n";
        assert_eq!(
            set_frontmatter(near, "version", "1"),
            "---\nversions: x\nversion: 1\n---\n"
        );
    }

    #[test]
    fn a_style_pill_sets_the_styles_line_and_keeps_style_files() {
        let flow = "---\ntitle: T\nstyles: [scientific, company.rstyle.yaml]\n---\n\nbody\n";
        assert_eq!(declared_style(flow).as_deref(), Some("scientific"));
        assert_eq!(
            with_style(flow, "air"),
            "---\ntitle: T\nstyles: [air, company.rstyle.yaml]\n---\n\nbody\n"
        );
        let scalar = "---\ntitle: T\nstyles: sleek\n---\n";
        assert_eq!(
            with_style(scalar, "brutalist"),
            "---\ntitle: T\nstyles: [brutalist]\n---\n"
        );
        let block = "---\ntitle: T\nstyles:\n  - scientific\n  - \"company.rstyle.yaml\"\ndescription: D\n---\n";
        assert_eq!(declared_style(block).as_deref(), Some("scientific"));
        assert_eq!(
            with_style(block, "hand-drawn"),
            "---\ntitle: T\nstyles: [hand-drawn, company.rstyle.yaml]\ndescription: D\n---\n"
        );
        let none = "---\ntitle: T\n---\n\nbody\n";
        assert_eq!(declared_style(none), None);
        // No style named is the default one; only style files of its own leave no pill checked.
        assert_eq!(shown_style(none).as_deref(), Some("sleek"));
        assert_eq!(shown_style("body\n").as_deref(), Some("sleek"));
        assert_eq!(shown_style(flow).as_deref(), Some("scientific"));
        let own = "---\ntitle: T\nstyles: [company.rstyle.yaml]\n---\n";
        assert_eq!(shown_style(own), None);
        assert_eq!(
            with_style(none, "sleek"),
            "---\ntitle: T\nstyles: [sleek]\n---\n\nbody\n"
        );
        assert_eq!(
            with_style("body\n", "air"),
            "---\nstyles: [air]\n---\n\nbody\n"
        );
        // Choosing the style the source already has changes nothing else.
        assert_eq!(
            with_style(&with_style(flow, "air"), "air"),
            with_style(flow, "air")
        );
    }
}
