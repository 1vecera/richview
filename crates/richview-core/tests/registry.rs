//! The schema registry (ARC-09) ties to today's code: every tag, chart type and diagnostic
//! literal the sources use has an entry, and the entries are well formed.
use regex::Regex;
use richview_core::registry::{self, Level};
use richview_core::{CHART_TYPES, RICH_TAGS, authoring_keys};

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every `.rs` file under the `src` trees of the workspace's crates and of `richview-chart`.
fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut roots = vec![workspace().join("richview-chart/src")];
    for krate in std::fs::read_dir(workspace().join("crates"))
        .unwrap()
        .flatten()
    {
        roots.push(krate.path().join("src"));
    }
    let mut files = vec![];
    for root in roots.iter().filter(|root| root.is_dir()) {
        walk(root, &mut files);
    }
    files.sort();
    files
}

#[test]
fn every_literal_diagnostic_has_a_catalogue_entry_at_its_level() {
    let call = Regex::new(r#"Diagnostic::new\(\s*"(\w+)"\s*,\s*"([^"]+)""#).unwrap();
    let mut missing = BTreeSet::new();
    let mut seen = 0;
    for file in sources() {
        let text = std::fs::read_to_string(&file).unwrap();
        // Unit tests sit at the end of each file and invent their own codes.
        let live = &text[..text.find("#[cfg(test)]").unwrap_or(text.len())];
        for found in call.captures_iter(live) {
            let (level, code) = (&found[1], &found[2]);
            seen += 1;
            match registry::code(code) {
                None => {
                    missing.insert(format!("{code} ({level}) in {}", file.display()));
                }
                Some(entry) if entry.level.as_str() != level => {
                    missing.insert(format!(
                        "{code} is {} in the catalogue but {level} in {}",
                        entry.level.as_str(),
                        file.display()
                    ));
                }
                Some(_) => {}
            }
        }
    }
    assert!(seen > 50, "the scan found only {seen} calls");
    assert!(missing.is_empty(), "uncatalogued: {missing:#?}");
}

#[test]
fn parser_and_renderer_findings_keep_the_catalogued_level_without_exceptions() {
    use richview_core::document::{RenderOptions, ReportFolder, check, parse, render};
    let options = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "diagnostic-levels".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let mut seen = BTreeSet::new();
    for source in [
        "---\n- not\n- a mapping\n---\n\nBody survives.\n",
        "---\ntitle: 42\nslug: 123\nversion: 1.0\n---\n\nBody.\n",
        "```image\nsrc: data:image/png;base64,AA==\nalt: Local image\ncredit: {author: ''}\n```\n",
        "---\ntitle_image: {src: 'data:image/png;base64,AA==', alt: Local image, credit: {author: ''}}\n---\n\nBody.\n",
        "```image\nsrc: https://images.unsplash.com/photo-test\nalt: Photo\ncredit: {author: Person, author_url: 'https://elsewhere.invalid/', source: Unsplash, source_url: 'https://unsplash.com/photos/test'}\n```\n",
    ] {
        // The rendered path catches codes emitted through helpers, which a literal scan cannot.
        let parsed = parse(source).unwrap().diagnostics;
        let rendered = render(source, &options).unwrap().diagnostics;
        let checked = check(source, &options).unwrap().diagnostics;
        for (operation, findings) in [("parse", parsed), ("render", rendered), ("check", checked)] {
            for finding in findings {
                let entry = registry::code(&finding.code)
                    .unwrap_or_else(|| panic!("uncatalogued {operation}: {finding:?}"));
                assert_eq!(
                    finding.level,
                    entry.level.as_str(),
                    "{operation}: {finding:?}"
                );
                seen.insert(finding.code);
            }
        }
    }
    for code in [
        "frontmatter.mapping",
        "frontmatter.type",
        "image.credit-invalid",
        "image.credit-incomplete",
    ] {
        assert!(seen.contains(code), "fixture did not exercise {code}");
    }
}

#[test]
fn the_tags_are_the_rich_tags() {
    let registered: BTreeSet<_> = registry::TAGS.iter().map(|tag| tag.name).collect();
    let parsed: BTreeSet<_> = RICH_TAGS.iter().copied().collect();
    assert_eq!(
        registered.len(),
        registry::TAGS.len(),
        "a tag is listed twice"
    );
    assert_eq!(registered, parsed);
}

#[test]
fn the_chart_types_are_the_authoring_chart_types() {
    let registered: BTreeSet<_> = registry::CHART_TYPES
        .iter()
        .map(|chart| chart.name)
        .collect();
    let authored: BTreeSet<_> = CHART_TYPES.iter().copied().collect();
    assert_eq!(
        registered.len(),
        registry::CHART_TYPES.len(),
        "a type is listed twice"
    );
    assert_eq!(registered, authored);
    for chart in registry::CHART_TYPES {
        for channel in chart.required {
            assert!(
                chart.encodings.contains(channel),
                "{} requires {channel}, which it does not read",
                chart.name
            );
        }
        for channel in chart.encodings {
            assert!(
                registry::tag("chart")
                    .unwrap()
                    .fields
                    .iter()
                    .any(|field| field.path == format!("encoding.{channel}")),
                "{} reads encoding.{channel}, which the chart tag lacks",
                chart.name
            );
        }
    }
}

#[test]
fn every_authoring_key_is_a_field() {
    let top = |fields: &[registry::Field]| -> BTreeSet<String> {
        fields
            .iter()
            .map(|field| field.path.split(['.', '[']).next().unwrap().to_owned())
            .collect()
    };
    for tag in registry::TAGS {
        let offered = authoring_keys(tag.name, &[])
            .unwrap_or_else(|| panic!("authoring_keys offers nothing for {}", tag.name));
        let known = top(tag.fields);
        for key in offered {
            assert!(
                known.contains(*key),
                "{}: authoring offers `{key}`",
                tag.name
            );
        }
    }
    let offered = authoring_keys("frontmatter", &[]).unwrap();
    let known = top(registry::FRONTMATTER);
    for key in offered {
        assert!(
            known.contains(*key),
            "frontmatter: authoring offers `{key}`"
        );
    }
}

#[test]
fn field_paths_are_unique_and_nest_under_a_declared_parent() {
    let sets = registry::TAGS
        .iter()
        .map(|tag| (tag.name, tag.fields))
        .chain([("frontmatter", registry::FRONTMATTER)]);
    for (name, fields) in sets {
        let paths: BTreeSet<_> = fields.iter().map(|field| field.path).collect();
        assert_eq!(paths.len(), fields.len(), "{name} repeats a path");
        for field in fields {
            if let Some(cut) = field.path.rfind('.') {
                let parent = field.path[..cut].trim_end_matches("[n]");
                assert!(
                    paths.contains(parent) || paths.contains(&field.path[..cut]),
                    "{name}.{} has no parent field",
                    field.path
                );
            }
            assert!(
                !field.summary.is_empty(),
                "{name}.{} has no summary",
                field.path
            );
        }
    }
}

#[test]
fn codes_are_unique_and_well_named() {
    let pattern = Regex::new(r"^[a-z]+(\.[a-z0-9-]+)+$").unwrap();
    let mut seen = BTreeSet::new();
    for entry in registry::CODES {
        assert!(seen.insert(entry.code), "{} is listed twice", entry.code);
        assert!(
            pattern.is_match(entry.code),
            "{} breaks the code pattern",
            entry.code
        );
        assert!(!entry.meaning.is_empty(), "{} has no meaning", entry.code);
    }
    let levels: BTreeSet<_> = registry::CODES
        .iter()
        .map(|entry| entry.level.as_str())
        .collect();
    assert_eq!(
        levels,
        BTreeSet::from([Level::Error, Level::Warning, Level::Info].map(Level::as_str))
    );
}

/// TYP-55: every code is catalogued with a fix, one sentence that a finding without its own fix
/// carries, so no finding leaves an author without a next step.
#[test]
fn every_code_has_a_one_sentence_fix_and_no_fix_is_orphaned() {
    let sentence_break = Regex::new(r"\.\s+\p{Lu}").unwrap();
    for entry in registry::CODES {
        let fix = registry::fix(entry.code).unwrap_or_else(|| panic!("{} has no fix", entry.code));
        assert!(
            fix.ends_with('.') && !sentence_break.is_match(fix),
            "{}'s fix is not one sentence: {fix}",
            entry.code
        );
    }
    let mut seen = BTreeSet::new();
    for (code, _) in registry::FIXES {
        assert!(
            registry::code(code).is_some(),
            "fix for unknown code {code}"
        );
        assert!(seen.insert(*code), "{code} has two fixes");
    }
}

/// A finding whose emitter names no fix carries its code's (TYP-53, TYP-55).
#[test]
fn a_finding_without_its_own_fix_carries_its_codes_fix() {
    let document =
        richview_core::document::parse("```table\ndata:\n  inline: [{a: 1}]\nmax_rows: 0\n```\n")
            .unwrap();
    let max_rows = document
        .diagnostics
        .iter()
        .find(|d| d.path == "table.max_rows")
        .unwrap_or_else(|| panic!("{:#?}", document.diagnostics));
    assert_eq!(max_rows.fix.as_deref(), registry::fix("block.schema"));
}

/// The docs teach only what the registry knows: every rich block in docs/public parses without
/// an unknown or retired key, so a page cannot teach a key the engine would drop.
#[test]
fn the_public_docs_use_only_registered_keys() {
    let fence = Regex::new(
        r"(?ms)^```(?:chart|table|metric|callout|image|embed|audio|video|scrolly)\n.*?^```\n",
    )
    .unwrap();
    let mut found = Vec::new();
    let mut stack = vec![workspace().join("docs/public")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                let text = std::fs::read_to_string(&path).unwrap();
                let blocks: String = fence.find_iter(&text).map(|m| m.as_str()).collect();
                let source = format!("---\ntitle: T\ndescription: D\n---\n\n{blocks}");
                let diagnostics = match richview_core::document::parse(&source) {
                    Ok(doc) => doc.diagnostics,
                    Err(d) => d,
                };
                for d in diagnostics {
                    if matches!(d.code.as_str(), "block.unknown-key" | "block.renamed") {
                        found.push(format!("{}: {}", path.display(), d.message));
                    }
                }
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn learn_teaches_every_chart_type_and_every_tag() {
    let learn = richview_core::document::learn_bundle();
    for chart in registry::CHART_TYPES {
        assert!(
            learn.contains(chart.name),
            "learn lacks chart type {}",
            chart.name
        );
    }
    for tag in registry::TAGS {
        assert!(learn.contains(tag.name), "learn lacks tag {}", tag.name);
        for field in tag
            .fields
            .iter()
            .filter(|f| !f.path.contains('.') && !f.path.contains('['))
        {
            assert!(
                learn.contains(field.summary),
                "learn loses the registry guidance for {}.{}",
                tag.name,
                field.path
            );
        }
    }
}

#[test]
fn learn_gives_every_list_entry_key_its_type() {
    // Fresh agents wrote a table column's `digits: 1` for one decimal place when learn named the
    // key alone (candidate/4); each key of a list of mappings now carries its type.
    let learn = richview_core::document::learn_bundle();
    for tag in registry::TAGS {
        for field in tag.fields {
            let Some((_, key)) = field.path.split_once("[n].") else {
                continue;
            };
            if key.contains(['.', '[']) {
                continue;
            }
            let star = if field.required { "*" } else { "" };
            let entry = format!("{key}{star} (");
            assert!(
                learn.contains(&entry),
                "learn names {}.{} without its type",
                tag.name,
                field.path
            );
        }
    }
    assert!(
        learn.contains("digits (a whole number of significant digits from 2 to 6)"),
        "a table column's digits say what they count"
    );
}

// The public interface contract runs with every core edit through the existing fast registry suite.
use serde_json::Value;
use std::process::Command;

// Host reason (ARC-03): development-only source inspection through the shared API checker.
const SNAPSHOT: &str = r#"
import {readFileSync} from 'node:fs';
import {publicSurface} from './tools/api-contracts/lib/rust-surface.mjs';
import {repositoryFile} from './tools/api-contracts/lib/resources.mjs';
import {checkContracts} from './tools/api-contracts/check.mjs';
const root = process.cwd();
const checked = checkContracts({root, manifest: 'tools/api-contracts/contracts.json'});
if (checked.status !== 'pass') throw new Error(JSON.stringify(checked.findings));
const expected = JSON.parse(readFileSync(repositoryFile(root, 'tools/api-contracts/snapshots/core-root.json'), 'utf8'));
const actual = {...expected, surface: {
    exports: publicSurface(readFileSync(repositoryFile(root, 'crates/richview-core/src/lib.rs'), 'utf8'), {moduleDeclarations: true}),
    declarations: expected.surface.declarations.map(({source, items}) => ({source, items,
        definitions: publicSurface(readFileSync(repositoryFile(root, source), 'utf8'), {select: items})
    }))
}};
process.stdout.write(JSON.stringify(actual));
"#;

#[test]
fn core_public_exports_match_the_reviewed_snapshot() {
    let root = workspace();
    let output = Command::new("node")
        .args(["--input-type=module", "-e", SNAPSHOT])
        .current_dir(root)
        .output()
        .expect("Node is required by the repository's API checker");
    assert!(
        output.status.success(),
        "Public API inspection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: Value = serde_json::from_slice(&output.stdout).expect("canonical API snapshot");
    let expected: Value = serde_json::from_str(include_str!(
        "../../../tools/api-contracts/snapshots/core-root.json"
    ))
    .unwrap();
    assert_eq!(expected["schema"], "interface-v1");
    assert_eq!(expected["id"], "core.root");
    assert_eq!(
        actual, expected,
        "Review public exports and their definitions before updating the snapshot"
    );
}
