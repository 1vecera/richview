//! A report written to make the findings about it expensive is checked in time that follows its
//! size, not the square of it. The server renders and locates such a report while it holds the
//! lock every publisher shares, so the cost is everyone's.
use richview_core::document::{Diagnostic, RenderOptions, ReportFolder};
use std::time::{Duration, Instant};

fn options(files: Vec<String>) -> RenderOptions {
    RenderOptions {
        report_folder: Some(ReportFolder {
            root: "fixture".into(),
            source: "report.rmd".into(),
            files,
        }),
        ..Default::default()
    }
}

#[test]
fn thousands_of_findings_on_one_line_are_each_given_a_column_without_rereading_the_line() {
    // A remote image is a finding where it is written; a line of them has one at every 13th
    // character, and counting each column from the start of the line was quadratic.
    let images = 40_000;
    let source = format!(
        "---\ntitle: T\n---\n\n{}\n",
        "![](http://x.test/a.png)".repeat(images)
    );
    let started = Instant::now();
    let report = richview_core::document::render(&source, &options(vec!["report.rmd".into()]))
        .expect("the draft renders");
    let took = started.elapsed();
    let at_columns: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.tag.as_deref() == Some("markdown") && d.line == Some(5))
        .filter_map(|d| d.column)
        .collect();
    assert!(
        at_columns.len() >= images,
        "{} findings with a column",
        at_columns.len()
    );
    // Each is where its image starts: the line's first character is column 1.
    let mut columns = at_columns;
    columns.sort_unstable();
    assert_eq!(columns[0], 1);
    assert_eq!(columns[1], 25);
    assert_eq!(columns[images - 1], 24 * (images - 1) + 1);
    assert!(took < Duration::from_secs(30), "rendering took {took:?}");
}

#[test]
fn a_file_named_by_a_hundred_thousand_links_is_located_at_the_first_of_them_in_linear_time() {
    let source = format!(
        "---\ntitle: T\n---\n\nFirst [a](a).\n\n{}\n",
        "[a](a) ".repeat(100_000)
    );
    let document = richview_core::document::parse(&source).expect("parses");
    let mut finding =
        Diagnostic::new("error", "report.file-limit", "too big", "source").in_file("a");
    let started = Instant::now();
    richview_core::document::locate_report_input(&document, &mut finding);
    let took = started.elapsed();
    assert_eq!(finding.line, Some(5));
    assert_eq!(finding.column, Some(7));
    assert!(took < Duration::from_secs(10), "locating took {took:?}");
}

#[test]
fn a_file_named_after_a_hundred_thousand_remote_images_is_located_without_a_finding_for_each() {
    // A remote image is a finding where it is written, and locating a file's reference is for one
    // place: none of those findings is built to find it.
    let images = 100_000;
    let source = format!(
        "---\ntitle: T\n---\n\n{}\n\nThe file [a](a).\n",
        "![](http://x.test/a.png)".repeat(images)
    );
    let document = richview_core::document::parse(&source).expect("parses");
    let mut finding =
        Diagnostic::new("error", "report.file-limit", "too big", "source").in_file("a");
    let started = Instant::now();
    richview_core::document::locate_report_input(&document, &mut finding);
    let took = started.elapsed();
    assert_eq!(finding.line, Some(7));
    assert_eq!(finding.column, Some(10));
    assert!(took < Duration::from_secs(10), "locating took {took:?}");
}

#[test]
fn a_link_to_a_file_the_folder_lacks_is_looked_up_not_searched_for_among_a_long_list() {
    // Each of these links used to canonicalize every name of the inventory again: with names this
    // long and links this many, hours.
    let long = "a/".repeat(60_000) + "a.csv";
    let files = vec!["report.rmd".into(), long.clone(), format!("x/{long}")];
    let source = format!("---\ntitle: T\n---\n\n{}\n", "[b](b.csv) ".repeat(30_000));
    let started = Instant::now();
    let report =
        richview_core::document::render(&source, &options(files)).expect("the draft renders");
    let took = started.elapsed();
    let missing = report
        .diagnostics
        .iter()
        .filter(|d| d.code == "report.asset.missing")
        .count();
    assert_eq!(missing, 30_000);
    assert!(took < Duration::from_secs(30), "rendering took {took:?}");
}

#[test]
fn media_blocks_share_one_walk_of_a_long_file_list() {
    // Each media block asks whether its file is in the folder; the answer came from a walk of the
    // whole list for each block, so a long list and many blocks multiplied.
    let blocks = 1_500;
    let source = format!(
        "---\ntitle: T\n---\n\n{}",
        "```image\nsrc: b.png\nalt: A\n```\n\n".repeat(blocks)
    );
    let names = |count: usize| {
        let mut files = vec!["report.rmd".to_owned(), "b.png".to_owned()];
        files.extend((0..count).map(|n| format!("{}{n}.csv", "d/".repeat(40))));
        files
    };
    let time = |files: Vec<String>| {
        let started = Instant::now();
        richview_core::document::render(&source, &options(files)).expect("the draft renders");
        started.elapsed()
    };
    let short = time(names(10));
    let long = time(names(40_000));
    // The list is read once for the whole render: a long one costs its length once, and not once
    // for each of the blocks.
    assert!(
        long < short * 4 + Duration::from_secs(3),
        "{long:?} with a long list against {short:?} with a short one"
    );
}

#[test]
fn a_list_that_names_one_style_file_over_and_over_is_refused_before_anything_is_copied() {
    // Each name in the list is a copy of the file the cascade makes and parses: a list of a
    // hundred thousand names of one file, or a few of a large one, was a request that cost far more
    // than it was long.
    let source = |names: usize| {
        format!(
            "---\ntitle: T\nstyles: [{}]\n---\n\nBody.\n",
            vec!["a.rstyle.yaml"; names].join(", ")
        )
    };
    let with_style = |text: String| {
        let mut options = options(vec!["report.rmd".into(), "a.rstyle.yaml".into()]);
        options.data.insert("a.rstyle.yaml".into(), text);
        options
    };
    let said = |refused: Vec<Diagnostic>| {
        assert!(
            refused.iter().any(|d| d.code == "style.limit"),
            "{refused:?}"
        );
        assert_eq!(
            refused.len(),
            1,
            "one finding says it, not one for each name"
        );
    };
    let started = Instant::now();
    said(
        richview_core::document::render(&source(100_000), &with_style("tokens: {}\n".into()))
            .expect_err("a hundred thousand names are refused"),
    );
    // A few names of a file that is large add up to more than the limit allows.
    said(
        richview_core::document::render(
            &source(5),
            &with_style(format!("# {}\n", "x".repeat(4_000_000))),
        )
        .expect_err("what the names add up to is refused"),
    );
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    // A report that names its styles as a report does is untouched, a file twice included.
    richview_core::document::render(&source(3), &with_style("tokens: {}\n".into()))
        .expect("three names of a small file render");
}

#[test]
fn the_company_brand_of_a_report_reads_no_more_styles_than_a_render_allows() {
    // A card is drawn from a stored report, one stored before the limit included: its styles are
    // read as far as the limit allows, and the rest of a long list is not copied.
    let frontmatter = serde_json::json!({
        "styles": vec!["a.rstyle.yaml"; 300_000],
    });
    let mut options = options(vec!["report.rmd".into(), "a.rstyle.yaml".into()]);
    options.data.insert(
        "a.rstyle.yaml".into(),
        format!("# {}\n", "x".repeat(4_000_000)),
    );
    let started = Instant::now();
    let _ = richview_core::report_company_brand(&frontmatter, &options);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_frontmatter_that_names_a_million_fields_under_long_keys_is_refused_in_a_moment() {
    // Inside the five-million-byte limit on a file: sixty keys of a thousand characters, each
    // inside the last, and a list of a million items at the bottom. Each item is a field that is
    // kept under the whole path above it, and the frontmatter's lines and columns are kept again,
    // so the cost was the item count times the sixty kilobytes, not the size of the file.
    let mut source = String::from("---\ntitle: T\n");
    for level in 0..60 {
        source.push_str(&" ".repeat(level));
        source.push_str(&"k".repeat(1_000));
        source.push_str(":\n");
    }
    source.push_str(&" ".repeat(60));
    source.push_str(&format!("[{}]\n", "1,".repeat(1_000_000)));
    source.push_str("---\n\nBody.\n");
    assert!(source.len() < 5_000_000, "{}", source.len());
    let started = Instant::now();
    let refusal = richview_core::document::parse(&source);
    let took = started.elapsed();
    assert!(took < Duration::from_secs(5), "refusing it took {took:?}");
    let found = match refusal {
        Ok(document) => document.diagnostics,
        Err(found) => found,
    };
    assert!(
        found
            .iter()
            .any(|d| d.code == "yaml.unsupported" && d.message.contains("more fields")),
        "{found:?}"
    );
}
