use super::*;

fn card(style: &str, lead: Lead) -> ShareCard {
    ShareCard {
        title: "Why the measurement gap keeps widening & what to do about it".into(),
        byline: "Daniel Vecera".into(),
        style: style.into(),
        accent: None,
        rule: None,
        lead,
    }
}

fn size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let n = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    (n(16), n(20))
}

const CHART: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 480 300\" width=\"480\" height=\"300\"><rect width=\"480\" height=\"300\" fill=\"#fff\"/><rect x=\"40\" y=\"40\" width=\"60\" height=\"200\" fill=\"#c0392b\"/><rect x=\"130\" y=\"110\" width=\"60\" height=\"130\" fill=\"#2a5bff\"/><rect x=\"220\" y=\"70\" width=\"60\" height=\"170\" fill=\"#1a8a5c\"/><text x=\"40\" y=\"280\" font-size=\"14\" font-family=\"IBM Plex Mono\">Jan</text></svg>";

#[test]
fn every_style_and_lead_renders_a_1200_by_630_png() {
    let cover = Lead::Cover {
        bytes: CHART.as_bytes().to_vec(),
        mime: "image/svg+xml".into(),
    };
    for style in STYLES {
        for lead in [Lead::ChartSvg(CHART.into()), cover.clone(), Lead::None] {
            assert_eq!(
                size(&render(&card(style, lead)).unwrap()),
                (1200, 630),
                "{style}"
            );
        }
    }
}

#[test]
fn the_same_card_gives_the_same_bytes_in_every_style_and_the_styles_differ() {
    let mut seen = Vec::new();
    for style in STYLES {
        let a = render(&card(style, Lead::ChartSvg(CHART.into()))).unwrap();
        assert_eq!(
            a,
            render(&card(style, Lead::ChartSvg(CHART.into()))).unwrap(),
            "{style}"
        );
        assert_ne!(
            a,
            render(&card(style, Lead::None)).unwrap(),
            "{style}: a lead changes the card"
        );
        assert!(!seen.contains(&a), "{style} draws like another style");
        seen.push(a);
    }
}

#[test]
fn an_unknown_style_draws_sleek_and_an_accent_overrides_the_styles_own() {
    let sleek = render(&card("sleek", Lead::None)).unwrap();
    assert_eq!(sleek, render(&card("no-such-style", Lead::None)).unwrap());
    let mut tinted = card("sleek", Lead::None);
    tinted.accent = Some("#c0392b".into());
    assert_ne!(sleek, render(&tinted).unwrap());
    tinted.accent = Some("not a colour".into());
    assert_eq!(sleek, render(&tinted).unwrap());
}

#[test]
fn czech_titles_render_in_every_style_with_a_fallback_for_missing_glyphs() {
    for style in STYLES {
        let mut czech = card(style, Lead::None);
        czech.title = "Příliš žluťoučký kůň úpěl ďábelské ódy, řekněte věrně".into();
        let png = render(&czech).unwrap();
        czech.title = "Prilis zlutoucky kun upel dabelske ody, reknete verne".into();
        assert_ne!(png, render(&czech).unwrap(), "{style}");
    }
}

#[test]
fn a_neutral_card_and_hostile_text_still_render() {
    assert_eq!(size(&render(&ShareCard::neutral()).unwrap()), (1200, 630));
    let hostile = ShareCard {
        title: "</text><script>x</script> \u{0}\"&".into(),
        byline: "<b>".into(),
        accent: Some("#12345".into()),
        rule: None,
        ..ShareCard::neutral()
    };
    assert_eq!(size(&render(&hostile).unwrap()), (1200, 630));
}

#[test]
fn a_neutral_card_carries_none_of_a_storys_words() {
    let neutral = render(&ShareCard::neutral()).unwrap();
    for style in STYLES {
        assert_ne!(neutral, render(&card(style, Lead::None)).unwrap());
    }
    assert_eq!(neutral, render(&ShareCard::neutral()).unwrap());
}

#[test]
fn a_cover_of_an_unsupported_type_draws_the_title_card() {
    let odd = card(
        "air",
        Lead::Cover {
            bytes: b"GIF89a".to_vec(),
            mime: "image/gif".into(),
        },
    );
    assert_eq!(
        render(&odd).unwrap(),
        render(&card("air", Lead::None)).unwrap()
    );
}

#[test]
fn a_very_long_title_still_fits_and_ends_in_an_ellipsis() {
    let mut long = card("brutalist", Lead::ChartSvg(CHART.into()));
    long.title = "word ".repeat(200);
    assert_eq!(size(&render(&long).unwrap()), (1200, 630));
    let svg = compose(&long);
    assert!(svg.contains('…'));
}

#[test]
fn titles_wrap_by_measured_width_cut_long_words_and_end_in_an_ellipsis() {
    let ruler = Ruler {
        primary: Measure::new(PLEX.data),
        fallback: None,
        tracking: 0.0,
    };
    // Plex Mono is 0.6 em wide: ten characters at 10 px are 60 px (plus the 2% allowance).
    let rows = wrap(&ruler, "one two three", 10.0, 62.0, 5).rows;
    assert_eq!(rows, ["one two", "three"]);
    assert_eq!(
        wrap(&ruler, "abcdefghij", 10.0, 25.0, 5).rows,
        ["abcd", "efgh", "ij"]
    );
    let cut = wrap(&ruler, "aaa bbb ccc ddd", 10.0, 25.0, 2);
    assert!(cut.cut);
    assert_eq!(cut.rows, ["aaa", "bbb…"]);
}

#[test]
fn alt_text_names_the_title_and_the_byline() {
    assert_eq!(alt_text("A title", "A. Writer"), "A title, by A. Writer");
    assert_eq!(alt_text(" A title ", " "), "A title");
}

#[test]
fn every_bundled_face_is_loaded_once_and_has_its_name() {
    let db = fonts();
    assert_eq!(db.faces().count(), FACES.len());
    for face in FACES {
        assert!(
            db.faces()
                .any(|f| f.families.iter().any(|(name, _)| name == face.family)),
            "{} is not loaded",
            face.family
        );
    }
}

/// A golden per builtin style (STY-70): the pixels of two cards, with and without a lead chart, must
/// match the stored PNGs. The review agent judges how a card looks; this only guards it from drifting.
/// `RICHVIEW_UPDATE_GOLDEN=1 cargo test -p richview-share` rewrites the files for a design change.
#[test]
fn each_builtin_style_matches_its_golden_card() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("golden");
    let update = std::env::var("RICHVIEW_UPDATE_GOLDEN").is_ok();
    for style in STYLES {
        for (name, lead) in [
            ("chart", Lead::ChartSvg(CHART.into())),
            ("title", Lead::None),
        ] {
            let png = render(&card(style, lead)).unwrap();
            let file = dir.join(format!("{style}-{name}.png"));
            if update {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&file, &png).unwrap();
                continue;
            }
            let stored = std::fs::read(&file).unwrap_or_else(|_| {
                panic!(
                    "{} is missing; run with RICHVIEW_UPDATE_GOLDEN=1",
                    file.display()
                )
            });
            let (a, b) = (
                tiny_skia::Pixmap::decode_png(&png).unwrap(),
                tiny_skia::Pixmap::decode_png(&stored).unwrap(),
            );
            assert_eq!((a.width(), a.height()), (b.width(), b.height()));
            let off = a
                .data()
                .iter()
                .zip(b.data())
                .filter(|(x, y)| x.abs_diff(**y) > 8)
                .count();
            // Anti-aliasing may differ by a hair between platforms; a design change moves far more.
            assert!(
                off * 2000 < a.data().len(),
                "{style}-{name}: {off} channel values differ from {}",
                file.display()
            );
        }
    }
}

#[test]
fn the_rule_token_is_parsed_and_changes_the_card() {
    assert_eq!(Rule::parse("none"), Some(Rule::None));
    assert_eq!(Rule::parse("hairline"), Some(Rule::Hairline));
    assert_eq!(Rule::parse("ink-bars"), Some(Rule::InkBars));
    assert_eq!(Rule::parse("pencil"), Some(Rule::Pencil));
    assert_eq!(Rule::parse("double"), None);
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for rule in [Rule::None, Rule::Hairline, Rule::InkBars, Rule::Pencil] {
        let mut c = card("sleek", Lead::ChartSvg(CHART.into()));
        c.rule = Some(rule);
        let png = render(&c).unwrap();
        assert!(!seen.contains(&png), "{rule:?} draws like another rule");
        seen.push(png);
    }
    // A builtin's own rule is its default: Scientific's hairline needs no override.
    let mut explicit = card("scientific", Lead::None);
    explicit.rule = Some(Rule::Hairline);
    assert_eq!(
        render(&explicit).unwrap(),
        render(&card("scientific", Lead::None)).unwrap()
    );
}

#[test]
fn card_png_leads_with_the_first_chart_and_falls_back_to_the_title() {
    let source = "# Rates\n\n```chart\ntype: bar\ntitle: Rate\ndata:\n  inline:\n    - {month: Jan, rate: 1}\n    - {month: Feb, rate: 2}\nencoding: {x: month, y: rate}\n```\n";
    let options = richview_core::document::RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "lead".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let report = richview_core::document::render(source, &options).unwrap();
    assert!(!report.charts.is_empty());
    let with = card_png("Rates", "Daniel", "scientific", &report.charts).unwrap();
    let without = card_png("Rates", "Daniel", "scientific", &[]).unwrap();
    assert!(with.starts_with(b"\x89PNG") && without.starts_with(b"\x89PNG"));
    assert_ne!(with, without);
    assert_eq!(
        with,
        card_png("Rates", "Daniel", "scientific", &report.charts).unwrap()
    );
}
