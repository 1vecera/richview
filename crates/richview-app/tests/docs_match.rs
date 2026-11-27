//! Countable claims in the agent-facing docs match the build (DEL-18, INT-66).
//!
//! The vocabularies come from the code: fence tags and chart types from the schema registry, MCP
//! tools from the server's tool list and CLI commands from `richview --help`. A doc that states a
//! different count ("six chart types") fails. Reference tables in the spec are generated from the
//! registry and checked by `richview-core`'s `spec_docs` test.
//!
//! The guide and the lockdown are not scanned: they quote the pre-rewrite state as evidence, with
//! dated sources, and only the lockdown app writes them.
use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use regex::Regex;
use richview_core::registry::{CHART_TYPES, TAGS};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The vocabularies a doc may count, keyed by the noun it counts them with.
fn vocabularies() -> BTreeMap<&'static str, usize> {
    let help = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    let commands = help
        .split("Commands:\n")
        .nth(1)
        .expect("`richview --help` lists its commands")
        .lines()
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "help")
        .count();
    let tools = richview_app::mcp::tool_definitions().len();
    BTreeMap::from([
        ("chart types", CHART_TYPES.len()),
        ("fence tags", TAGS.len()),
        ("rich tags", TAGS.len()),
        ("block tags", TAGS.len()),
        ("mcp tools", tools),
        ("cli commands", commands),
    ])
}

/// Every `<number> <noun>` claim in `text` whose count differs from `counts`, as
/// `line: claim (the build has n)`.
fn wrong_claims(text: &str, counts: &BTreeMap<&str, usize>) -> Vec<String> {
    const WORDS: [&str; 21] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ];
    let nouns: Vec<String> = counts.keys().map(|noun| regex::escape(noun)).collect();
    let claim = Regex::new(&format!(
        r"(?i)\b(\d+|{})\s+({})\b",
        WORDS.join("|"),
        nouns.join("|")
    ))
    .unwrap();
    let mut wrong = vec![];
    for (index, line) in text.lines().enumerate() {
        for found in claim.captures_iter(line) {
            let number = found[1].to_lowercase();
            let stated = number
                .parse()
                .ok()
                .or_else(|| WORDS.iter().position(|word| *word == number))
                .unwrap();
            let noun = found[2].to_lowercase();
            let actual = counts[noun.as_str()];
            if stated != actual {
                wrong.push(format!(
                    "{}: \"{}\" (the build has {actual})",
                    index + 1,
                    &found[0]
                ));
            }
        }
    }
    wrong
}

/// The agent-facing and public docs the gate scans.
fn scanned() -> Vec<PathBuf> {
    let root = workspace();
    let mut files: Vec<PathBuf> = [
        "AGENTS.md",
        "CLAUDE.md",
        "README.md",
        "docs/RMD-SPEC.md",
        "skill/SKILL.md",
    ]
    .iter()
    .map(|file| root.join(file))
    .collect();
    for dir in [".claude/rules", "docs/public"] {
        let mut found: Vec<PathBuf> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .collect();
        found.sort();
        files.extend(found);
    }
    files
}

#[test]
fn time_series_example_downloads_do_not_create_a_second_report_source() {
    for name in [
        "co2-mauna-loa-monthly",
        "us-cpi-components",
        "de-power-15min",
    ] {
        let file = workspace().join(format!("docs/public/examples/{name}/report.rmd"));
        let source = std::fs::read_to_string(file).unwrap();
        let document = richview_core::document::parse(&source).unwrap();
        let linked = richview_app::resolver::linked_paths(&document);
        assert!(linked.contains("data.csv"), "{name}: keep the CSV download");
        assert!(
            !linked
                .iter()
                .any(|path| path.to_lowercase().ends_with(".rmd")),
            "{name}: a linked .rmd cannot be copied into Rendered without adding a second report source; keep the website's source link in its documentation chrome"
        );
    }
}

#[test]
fn docs_count_the_vocabularies_the_build_has() {
    let counts = vocabularies();
    let mut wrong = vec![];
    for file in scanned() {
        let text = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
        let name = file
            .strip_prefix(workspace())
            .unwrap()
            .display()
            .to_string();
        wrong.extend(
            wrong_claims(&text, &counts)
                .into_iter()
                .map(|claim| format!("{name}:{claim}")),
        );
    }
    wrong.extend(
        wrong_claims(&richview_core::document::learn_bundle(), &counts)
            .into_iter()
            .map(|claim| format!("richview learn:{claim}")),
    );
    assert!(
        wrong.is_empty(),
        "Docs state counts the build does not have:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_known_drifts_are_caught() {
    // TYP-03's drift fixtures: six versus nine chart types, five versus seven MCP tools.
    let counts = BTreeMap::from([("chart types", 9), ("mcp tools", 7), ("cli commands", 10)]);
    let text = "All six chart types render.\nThe server lists 5 MCP tools.\nNine chart types; seven MCP tools; ten CLI commands.\nCompare two styles.";
    assert_eq!(
        wrong_claims(text, &counts),
        [
            "1: \"six chart types\" (the build has 9)",
            "2: \"5 MCP tools\" (the build has 7)"
        ]
    );
}

/// Every `richview <command>` and backticked `richview_<tool>` in `text` that the build lacks, as
/// `line: message`. Fenced blocks of Rich Markdown, charts and YAML are examples of the format, not
/// of the tools, so they are skipped; a fence closes only on a line of at least as many backticks
/// as opened it, so a three-backtick block inside a four-backtick one does not end it.
fn unknown_names(text: &str, commands: &[String], tools: &[String]) -> Vec<String> {
    let command = Regex::new(r"(?:^|[`\s$])richview ([a-z][a-z-]*)\b").unwrap();
    let tool = Regex::new(r"`(richview_[a-z_]+)`").unwrap();
    let mut wrong = Vec::new();
    // The open fence: its backtick count and whether its content is skipped.
    let mut fence: Option<(usize, bool)> = None;
    for (number, line) in text.lines().enumerate() {
        let ticks = line.chars().take_while(|c| *c == '`').count();
        if ticks >= 3 {
            let info = line[ticks..].trim();
            match fence {
                Some((open, _)) if info.is_empty() && ticks >= open => fence = None,
                Some(_) => {}
                None => {
                    let skip = matches!(info, "rmd" | "chart" | "yaml" | "yml" | "json" | "csv");
                    fence = Some((ticks, skip));
                }
            }
            continue;
        }
        if fence.is_some_and(|(_, skip)| skip) {
            continue;
        }
        // A line that tells the reader what not to invent names things that do not exist.
        if line.contains("Do not invent") {
            continue;
        }
        for found in command.captures_iter(line) {
            if !commands.iter().any(|c| c == &found[1]) {
                wrong.push(format!(
                    "{}: `richview {}` is not a command",
                    number + 1,
                    &found[1]
                ));
            }
        }
        for found in tool.captures_iter(line) {
            if !tools.iter().any(|t| t == &found[1]) {
                wrong.push(format!(
                    "{}: `{}` is not an MCP tool",
                    number + 1,
                    &found[1]
                ));
            }
        }
    }
    wrong
}

fn public_doc_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            public_doc_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            files.push(path);
        }
    }
}

fn commands_and_tools() -> (Vec<String>, Vec<String>) {
    let help = Command::new(env!("CARGO_BIN_EXE_richview"))
        .arg("--help")
        .output()
        .unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    let commands = help
        .split("Commands:\n")
        .nth(1)
        .expect("`richview --help` lists its commands")
        .lines()
        .take_while(|line| line.starts_with("  "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_owned)
        .collect();
    let tools = richview_app::mcp::tool_definitions()
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect();
    (commands, tools)
}

/// The public docs, chart pages included, name only commands and MCP tools the build has (FEA-SF-41).
#[test]
fn public_docs_name_only_commands_and_tools_that_exist() {
    let (commands, tools) = commands_and_tools();
    let mut files = Vec::new();
    public_doc_files(&workspace().join("docs/public"), &mut files);
    files.push(workspace().join("skill/SKILL.md"));
    let mut wrong = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        if let Err(message) = richview_app::reference::validate_cli_examples(&text) {
            wrong.push(format!("{}: {message}", file.display()));
        }
        for message in unknown_names(&text, &commands, &tools) {
            wrong.push(format!("{}:{message}", file.display()));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_command_after_a_nested_fence_is_still_checked() {
    let (commands, tools) = commands_and_tools();
    let text = "````rmd\n```chart\ntype: bar\n```\n````\n\nRun `richview frobnicate report.rmd`.\n\n```sh\nrichview check report.rmd\n```\n";
    let wrong = unknown_names(text, &commands, &tools);
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(wrong[0].contains("frobnicate"));
}

fn authoring_docs() -> Vec<PathBuf> {
    let mut files = Vec::new();
    public_doc_files(&workspace().join("docs/public"), &mut files);
    files.extend(["README.md", "skill/SKILL.md", "docs/RMD-SPEC.md"].map(|p| workspace().join(p)));
    files
}

/// Read the generated migration reference, rather than maintaining another retired-name list.
fn retired_names() -> Vec<(String, String, String)> {
    richview_core::spec::fragment("retired")
        .expect("the registry supplies the migration reference")
        .lines()
        .skip(2)
        .map(|line| {
            let cells: Vec<_> = line.split('|').map(str::trim).collect();
            (
                cells[1].into(),
                cells[2].trim_matches('`').into(),
                cells[3].trim_matches('`').into(),
            )
        })
        .collect()
}

fn prose_retirements(text: &str) -> Vec<String> {
    let text = text.replace("\r\n", "\n");
    // Examples are checked as input below; arbitrary row labels are not vocabulary names.
    let mut prose = text.as_bytes().to_vec();
    let mut opened = None;
    for (event, range) in Parser::new(&text).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => opened = Some(range.start),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(start) = opened.take() {
                    for byte in &mut prose[start..range.end] {
                        if *byte != b'\n' {
                            *byte = b' ';
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let text = String::from_utf8(prose).unwrap();
    let names = retired_names();
    let removed =
        Regex::new(r"(?i)\b(retired|renamed|removed|rejected)\b|no longer (accepted|supported)")
            .unwrap();
    let style_names: Vec<_> = names
        .iter()
        .filter(|(kind, _, _)| kind == "style-name")
        .map(|(_, _, name)| {
            (
                name,
                Regex::new(&format!(
                    r"(^|[^\w.-]){}($|[^\w.-]|\.(?:$|\s))",
                    regex::escape(name)
                ))
                .unwrap(),
            )
        })
        .collect();
    let inline = Regex::new(r"`([^`\n]+)`").unwrap();
    let flag = Regex::new(r"--[a-z][a-z0-9-]*").unwrap();
    let flags: BTreeSet<String> = richview_app::reference::cli_markdown()
        .lines()
        .filter(|line| line.starts_with("| `"))
        .flat_map(|line| {
            flag.find_iter(line.split('|').nth(1).unwrap())
                .map(|f| f.as_str().to_owned())
        })
        .collect();
    let mut wrong = Vec::new();
    let mut section = String::new();
    let mut offset = 0;
    for paragraph in text.split("\n\n") {
        let line = text[..offset].bytes().filter(|b| *b == b'\n').count() + 1;
        let heading = paragraph.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&heading) && paragraph[heading..].starts_with(' ') {
            section = paragraph.lines().next().unwrap().to_lowercase();
        }
        offset += paragraph.len() + 2;
        if removed.is_match(paragraph) || removed.is_match(&section) {
            continue;
        }
        for found in inline.captures_iter(paragraph) {
            let code = &found[1];
            for (kind, scope, name) in &names {
                let qualified = format!("{scope}.{name}");
                let matches_path =
                    |path: &str| code == path || code.starts_with(&format!("{path}."));
                let retired = match kind.as_str() {
                    "field" => {
                        matches_path(&qualified)
                            || (name.contains('.') && matches_path(name))
                            || (scope == "frontmatter"
                                && section.contains("metadata")
                                && code == name)
                    }
                    "style-field" => matches_path(name),
                    "tag" | "style-name" | "value" => code == name,
                    _ => unreachable!("unknown retirement kind {kind}"),
                };
                if retired {
                    wrong.push(format!("{line}: retired {scope} name `{name}` needs an explicit renamed/removed explanation"));
                }
            }
            let executable = code
                .trim_start_matches("$ ")
                .split_whitespace()
                .next()
                .unwrap_or("");
            let richview = Path::new(executable)
                .file_name()
                .is_some_and(|name| name == "richview");
            if code.starts_with("--") || richview {
                for found in flag.find_iter(code) {
                    let name = found.as_str();
                    if !name.starts_with("--rv-") && !flags.contains(name) {
                        wrong.push(format!(
                            "{line}: `{name}` is not a current RichView CLI flag"
                        ));
                    }
                }
            }
        }
        for found in flag.find_iter(&inline.replace_all(paragraph, "")) {
            if !flags.contains(found.as_str()) {
                wrong.push(format!(
                    "{line}: `{}` is not a current RichView CLI flag",
                    found.as_str()
                ));
            }
        }
        // Style names still need a removal notice when prose omits code formatting.
        for (name, pattern) in &style_names {
            if pattern.is_match(paragraph) {
                wrong.push(format!(
                    "{line}: retired style `{name}` needs an explicit renamed/removed explanation"
                ));
            }
        }
    }
    wrong
}

/// Retired spellings are checked here; claims about current capabilities still need source review.
#[test]
fn authoring_docs_explain_retired_names_instead_of_teaching_them() {
    let mut wrong = Vec::new();
    for file in authoring_docs() {
        let text = std::fs::read_to_string(&file).unwrap();
        wrong.extend(
            prose_retirements(&text)
                .into_iter()
                .map(|m| format!("{}:{m}", file.display())),
        );
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[derive(Debug)]
struct Example {
    line: usize,
    info: String,
    source: String,
}

fn examples(text: &str) -> Vec<Example> {
    let mut examples = Vec::new();
    let mut current = None;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                current = Some(Example {
                    line: text[..range.start].bytes().filter(|b| *b == b'\n').count() + 1,
                    info: info.into_string(),
                    source: String::new(),
                });
            }
            Event::Text(text) if current.is_some() => {
                current.as_mut().unwrap().source.push_str(&text)
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(example) = current.take() {
                    examples.push(example);
                }
            }
            _ => {}
        }
    }
    examples
}

/// Use the real downloadable report's resources for snippets explicitly naming that context.
fn example_report(example: &Example, dir: &Path) -> Result<Option<String>, String> {
    let info: Vec<_> = example.info.split_whitespace().collect();
    let language = info.first().copied().unwrap_or("");
    let context = info.iter().find_map(|word| word.strip_prefix("context="));
    let base = if let Some(context) = context {
        let path = Path::new(context);
        if !path.starts_with("examples")
            || path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(format!("invalid example context {context}"));
        }
        fn copy(from: &Path, to: &Path) -> Result<(), String> {
            std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
            for entry in std::fs::read_dir(from).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    if entry.file_name() != "Rendered" {
                        copy(&entry.path(), &to.join(entry.file_name()))?;
                    }
                } else {
                    std::fs::copy(entry.path(), to.join(entry.file_name()))
                        .map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        }
        copy(&workspace().join(path), dir)?;
        Some(std::fs::read_to_string(dir.join("report.rmd")).map_err(|e| e.to_string())?)
    } else {
        None
    };
    // A deck is checked as it is published, from its own text and its own file name.
    if language == "rmd" || language == "rslide" {
        return Ok(Some(example.source.clone()));
    }
    if TAGS.iter().any(|tag| tag.name == language) {
        return Ok(Some(format!("```{language}\n{}```\n", example.source)));
    }
    match language {
        "yaml" | "yml" => {
            richview_core::yaml::from_str(&example.source).map_err(|e| format!("{e:?}"))?;
            match info.get(1).copied() {
                // A documented style reaches the report the way a real one does, as a file the
                // frontmatter names, so the media it names (a company's logos) are read from the
                // example's folder and checked.
                Some("style") => {
                    std::fs::write(dir.join("example.rstyle.yaml"), &example.source)
                        .map_err(|e| e.to_string())?;
                    Ok(Some(
                        "---\nstyles: [example.rstyle.yaml]\n---\n\nExample style.\n".into(),
                    ))
                }
                Some("frontmatter") => {
                    let body = base.as_deref().ok_or("frontmatter snippet needs a real report context")?
                        .splitn(3, "---").nth(2).ok_or("context must contain frontmatter")?;
                    Ok(Some(format!("---\n{}---\n{body}", example.source)))
                }
                Some("chart-data") => {
                    let document = richview_core::document::parse(base.as_deref().ok_or("data snippet needs a real chart context")?)
                        .map_err(|e| format!("{e:?}"))?;
                    let mut chart = document.blocks.into_iter().find(|b| b.tag == "chart")
                        .ok_or("context needs a chart")?.value;
                    let patch = richview_core::yaml::from_str(&example.source).map_err(|e| format!("{e:?}"))?;
                    let data = patch.get("data").ok_or("data snippet must set data")?;
                    chart["data"] = data.clone();
                    Ok(Some(format!("```chart\n{}\n```\n", chart)))
                }
                Some(tag) if TAGS.iter().any(|t| t.name == tag) => Ok(Some(format!("```{tag}\n{}```\n", example.source))),
                Some("config") => Ok(None),
                _ => Err("YAML examples must name their context: style, frontmatter, chart-data, a rich tag or config".into()),
            }
        }
        "json" => {
            serde_json::from_str::<serde_json::Value>(&example.source)
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        "jsonl" => {
            for line in example
                .source
                .lines()
                .filter(|line| !line.trim().is_empty())
            {
                serde_json::from_str::<serde_json::Value>(line).map_err(|e| e.to_string())?;
            }
            Ok(None)
        }
        "text" => Ok(None), // Literal output and directory trees, not report input.
        "sh" | "shell" | "bash" => {
            let mut child = Command::new("bash")
                .args(["-n", "-s"])
                .stdin(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(example.source.as_bytes())
                .map_err(|e| e.to_string())?;
            let output = child.wait_with_output().map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned());
            }
            let joined = example
                .source
                .replace("\\\n", " ")
                .replace("target/release/richview ", "richview ");
            richview_app::reference::validate_cli_examples(&format!("```sh\n{joined}\n```"))?;
            Ok(None)
        }
        _ => Err(format!("unclassified fenced example {language:?}")),
    }
}

fn check_example(example: &Example) -> Result<(), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let Some(source) = example_report(example, dir.path())? else {
        return Ok(());
    };
    let name = if example.info.split_whitespace().next() == Some("rslide") {
        "deck.rslide"
    } else {
        "report.rmd"
    };
    let file = dir.path().join(name);
    std::fs::write(&file, &source).map_err(|e| e.to_string())?;
    let (document, options) =
        richview_app::resolver::resolve_document_options_policy(&source, &file, &[], false, false)?;
    let report = richview_core::document::check_document(document, &options)
        .map_err(|e| format!("{e:?}"))?;
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| {
            d.level == "error"
                || matches!(
                    d.code.as_str(),
                    "style.retired"
                        | "style.unknown-key"
                        | "style.schema"
                        | "block.unknown-key"
                        | "frontmatter.unknown-key"
                )
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{errors:?}"))
    }
}

#[test]
fn every_documented_report_example_passes_check_in_its_declared_context() {
    let mut wrong = Vec::new();
    for file in authoring_docs() {
        for example in examples(&std::fs::read_to_string(&file).unwrap()) {
            if let Err(error) = check_example(&example) {
                wrong.push(format!(
                    "{}:{} ({}): {error}",
                    file.display(),
                    example.line,
                    example.info
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn retired_prose_and_broken_examples_fail_without_rejecting_migration_guidance() {
    for old in [
        "Choose `sleek-dark` as the default.",
        "Use `editorial` for reports.",
        "Set `style.line.smoothing` to `straight`.",
        "Use the `big` fence.",
        "Load assets with `--public-dir`.",
        "Run `richview generate report.rmd --theme dark`.",
        "Use `--allow-network` to load a remote dataset.",
    ] {
        assert!(!prose_retirements(old).is_empty(), "missed {old}");
    }
    assert!(prose_retirements("Removed names: `editorial`, `big` and `--public-dir`; use their replacements.\n\nAn `image` block is still accepted.").is_empty());
    assert!(!prose_retirements("Removed `old`.\r\n\r\nUse sleek-dark.\r\n").is_empty());
    assert!(prose_retirements("The file is `editorial.rstyle.yaml`.").is_empty());
    assert!(
        prose_retirements("Run `other-tool deploy --name richview --foreign-option yes`.")
            .is_empty()
    );
    check_example(&Example {
        line: 1,
        info: "jsonl".into(),
        source: "{\"event\":\"progress\"}\n{\"event\":\"done\"}\n".into(),
    })
    .unwrap();
    assert!(prose_retirements("```json\n{\"label\":\"editorial\"}\n```\n").is_empty());
    let nested = examples("````rmd\n```metric\nvalue: 2\n```\n````\n\n~~~metric\nvalue: 3\n~~~\n");
    assert_eq!(nested.len(), 2);
    for example in &nested {
        check_example(example).unwrap();
    }
    for (info, source, expected) in [
        (
            "rmd",
            "---\nstyles: [editorial]\n---\nExample.\n",
            "style.renamed",
        ),
        ("metric", "value: [1, 2]\n", "block.schema"),
        ("yaml style", "motion: {delay: 100}\n", "style.retired"),
        (
            "sh",
            "richview generate report.rmd \\\n --public-dir old-runtime\n",
            "--public-dir",
        ),
        ("json", "{broken", "key"),
        ("jsonl", "{\"event\":\"progress\"}\n{broken\n", "key"),
        ("yaml metric", "value: [\n", ""),
    ] {
        let example = Example {
            line: 1,
            info: info.into(),
            source: source.into(),
        };
        let error = check_example(&example).expect_err(source);
        assert!(error.contains(expected), "{source}: {error}");
    }
}
