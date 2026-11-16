//! Report identity: slug and version grammars, derivation and slug choice (TYP-60, TYP-61, TYP-62).
use richview_core::document::{Diagnostic, parse, validate, validate_publish};
use richview_core::{create_publish_request, slug_problem, version_problem};

fn codes(diagnostics: &[Diagnostic]) -> Vec<(&str, &str)> {
    diagnostics
        .iter()
        .map(|d| (d.level.as_str(), d.code.as_str()))
        .collect()
}

#[test]
fn slug_grammar_is_hyphen_only_lowercase_ascii() {
    for ok in ["a", "report-2026", "0", &"a".repeat(80)] {
        assert!(slug_problem(ok).is_none(), "{ok}");
    }
    for bad in [
        "",
        "My-Report",
        "my_report",
        "-lead",
        "trail-",
        "dou--ble",
        "has space",
        "pruzkum-ž",
        "a/b",
        "..",
        &"a".repeat(81),
    ] {
        let problem = slug_problem(bad).unwrap_or_else(|| panic!("{bad} accepted"));
        assert!(problem.ends_with('.'), "plain sentence: {problem}");
    }
}

#[test]
fn version_grammar_is_strict_semver_without_build_metadata() {
    for ok in [
        "0.0.1",
        "10.20.30",
        "1.2.3-beta.1",
        "1.0.0-0",
        "1.0.0-x-y.7",
        &format!("1.0.0-{}", "a".repeat(40)),
    ] {
        assert!(version_problem(ok).is_none(), "{ok}");
    }
    for bad in [
        "",
        "1",
        "1.0",
        "1.2.3.4",
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.3-01",
        "1.2.3-",
        "1.2.3-a..b",
        "1.2.3+build",
        "1.2.3-rc.1+b",
        " 1.2.3",
        "1.2.3 ",
        "1.2.3\n",
        "v1.2.3",
        "1.2.3/4",
        "1.a.3",
        &format!("1.0.0-{}", "a".repeat(60)),
    ] {
        assert!(version_problem(bad).is_some(), "{bad:?} accepted");
    }
}

#[test]
fn a_yaml_number_version_hints_to_quote_it() {
    let diagnostics = validate("---\ntitle: T\nversion: 1.0\n---\n");
    let version: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.path == "frontmatter.version")
        .collect();
    assert_eq!(
        codes(
            &diagnostics
                .iter()
                .filter(|d| d.path == "frontmatter.version")
                .cloned()
                .collect::<Vec<_>>()
        ),
        vec![("warning", "frontmatter.type")]
    );
    assert!(version[0].fix.as_deref().unwrap().contains("Quote"));
    let publish = validate_publish("---\ntitle: T\ndescription: D\nversion: 1.0\n---\n");
    assert!(
        publish
            .iter()
            .any(|d| d.code == "publish.metadata" && d.level == "error")
    );
    let leading_zero = validate("---\nversion: \"01.2.3\"\n---\n");
    assert_eq!(
        codes(&leading_zero),
        vec![("warning", "frontmatter.version")]
    );
    assert!(leading_zero[0].message.contains("leading zero"));
    assert!(
        validate_publish("---\ntitle: T\ndescription: D\nversion: \"0.0.1\"\n---\n").is_empty()
    );
}

#[test]
fn slug_order_is_override_then_frontmatter_then_title() {
    let source = "---\ntitle: Revenue rose\nslug: authored-slug\n---\n";
    assert_eq!(
        create_publish_request(source, None).unwrap().slug,
        "authored-slug"
    );
    assert_eq!(
        create_publish_request(source, Some("explicit"))
            .unwrap()
            .slug,
        "explicit"
    );
    assert_eq!(
        create_publish_request("---\ntitle: Revenue rose\n---\n", None)
            .unwrap()
            .slug,
        "revenue-rose"
    );
}

#[test]
fn an_unrelated_frontmatter_problem_never_changes_the_slug() {
    for extra in [
        "tags: [1]",
        "theme: purple",
        "author: 42",
        "version: 1.0",
        "styles: nope",
    ] {
        let source = format!("---\ntitle: Anything\nslug: authored-slug\n{extra}\n---\n");
        assert_eq!(
            create_publish_request(&source, None).unwrap().slug,
            "authored-slug",
            "{extra}"
        );
        let source = format!("---\ntitle: Title Slug\n{extra}\n---\n");
        assert_eq!(
            create_publish_request(&source, None).unwrap().slug,
            "title-slug",
            "{extra}"
        );
    }
}

#[test]
fn there_is_no_untitled_fallback_and_a_bad_authored_slug_is_an_error() {
    for (source, over, path) in [
        ("# Local draft", None, "frontmatter.slug"),
        ("---\ntitle: 東京\n---\n", None, "frontmatter.title"),
        (
            "---\ntitle: T\nslug: My_Report\n---\n",
            None,
            "frontmatter.slug",
        ),
        ("---\ntitle: T\nslug: 42\n---\n", None, "frontmatter.slug"),
        ("---\ntitle: T\nslug: ok\n---\n", Some("../escape"), "slug"),
        ("---\ntitle: T\nslug: ok\n---\n", Some(""), "slug"),
    ] {
        let errors = create_publish_request(source, over).unwrap_err();
        assert_eq!(
            codes(&errors),
            vec![("error", "publish.slug")],
            "{source:?}"
        );
        assert_eq!(errors[0].path, path);
        assert!(errors[0].expected.is_some() && errors[0].fix.is_some());
    }
}

#[test]
fn publish_validation_reports_the_slug_once_and_drafts_only_warn() {
    let source = "---\ntitle: T\ndescription: D\nversion: \"0.0.1\"\nslug: Bad_Slug\n---\n";
    assert_eq!(
        codes(&validate(source)),
        vec![("warning", "frontmatter.slug")]
    );
    let publish = validate_publish(source);
    assert!(codes(&publish).contains(&("error", "publish.slug")));
    let titleless = validate_publish("---\ndescription: D\nversion: \"0.0.1\"\n---\n");
    assert!(
        titleless.iter().all(|d| d.code != "publish.slug"),
        "{titleless:?}"
    );
    assert!(
        parse("---\ntitle: T\nslug: fine-slug\n---\n")
            .unwrap()
            .diagnostics
            .is_empty()
    );
}
