use super::*;

fn card(style: &str, lead: Lead) -> ShareCard {
    ShareCard {
        title: "Why the measurement gap keeps widening & what to do about it".into(),
        byline: "Daniel Vecera".into(),
        style: style.into(),
        accent: None,
        rule: None,
        lead,
        company: None,
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

fn company_logo() -> richview_core::CompanyBrand {
    use std::collections::BTreeMap;
    let mut style = richview_core::resolve_styles(&["air".into()], &mut Vec::new());
    style.document["company"] = serde_json::json!({"name":"Example & Co", "logo":"logo.bin"});
    let mut image = tiny_skia::Pixmap::new(2, 2).unwrap();
    image.fill(tiny_skia::Color::from_rgba8(20, 20, 20, 255));
    let png = image.encode_png().unwrap();
    let data = BTreeMap::from([(
        "logo.bin".to_owned(),
        format!("data:application/octet-stream;base64,{}", base64(&png)),
    )]);
    let mut brand = richview_core::resolve_company_brand(&style, &data, &mut Vec::new());
    brand.accent = None;
    brand
}

#[test]
fn company_mark_sits_above_the_title_and_preserves_the_card_frame() {
    for style in STYLES {
        for lead in [
            Lead::None,
            Lead::ChartSvg("<svg xmlns=\"http://www.w3.org/2000/svg\"/>".into()),
        ] {
            let mut card = card(style, lead);
            let before = compose(&card);
            let before_png = render(&card).unwrap();
            let company = company_logo();
            let (mark, height) = company_mark(Some(&company), look(style).dark, 72.0).unwrap();
            assert!(mark.contains("width=\"64\""), "{mark}");
            assert!(mark.contains(if look(style).dark {
                "y=\"80\""
            } else {
                "y=\"72\""
            }));
            assert_eq!(height, if look(style).dark { 80.0 } else { 64.0 });
            assert_eq!(mark.contains("fill=\"#fff\""), look(style).dark);
            card.company = Some(company);
            let after = compose(&card);
            assert!(after.contains(&mark));
            assert!(before.contains(&byline_mark(
                card.rule.unwrap_or(look(style).rule),
                look(style).accent
            )));
            assert!(after.contains(&byline_mark(
                card.rule.unwrap_or(look(style).rule),
                look(style).accent
            )));
            let after_png = render(&card).unwrap();
            assert_eq!(size(&after_png), (1200, 630));
            assert!(
                after_png != before_png,
                "{style} logo must appear in the PNG"
            );
        }
    }
}

#[test]
fn share_card_refuses_a_forged_company_logo() {
    let mut card = card("air", Lead::None);
    let mut company = company_logo();
    company.logo.as_mut().unwrap().bytes = b"<svg/>".to_vec();
    card.company = Some(company);
    assert!(render(&card).is_err());
}

#[test]
fn wide_company_mark_fits_the_card_width_and_sets_title_clearance() {
    let mut company = company_logo();
    let logo = company.logo.as_mut().unwrap();
    logo.width = 12;
    logo.height = 1;
    let (markup, height) = company_mark(Some(&company), false, 72.0).unwrap();
    assert!(markup.contains("x=\"64\" y=\"72\" width=\"300\" height=\"25\""));
    assert_eq!(height, 25.0);
}

#[test]
fn company_accent_colors_the_card_frame_even_without_a_logo() {
    let mut card = card("brutalist", Lead::None);
    let original = tiny_skia::Pixmap::decode_png(&render(&card).unwrap()).unwrap();
    card.company = Some(richview_core::CompanyBrand {
        accent: Some("#0b6e69".into()),
        ..Default::default()
    });
    let branded = tiny_skia::Pixmap::decode_png(&render(&card).unwrap()).unwrap();
    let pixel = ((28 * 1200 + 100) * 4) as usize;
    assert_ne!(
        &original.data()[pixel..pixel + 4],
        &branded.data()[pixel..pixel + 4]
    );
    assert_eq!(&branded.data()[pixel..pixel + 4], &[11, 110, 105, 255]);
}

#[test]
fn jpeg_and_webp_logos_are_decoded_into_the_share_image() {
    for (bytes, mime) in [
        (
            include_bytes!("../tests/fixtures/company-logo.jpg").as_slice(),
            "image/jpeg",
        ),
        (
            include_bytes!("../tests/fixtures/company-logo.webp").as_slice(),
            "image/webp",
        ),
    ] {
        let checked = richview_core::validate_company_logo(bytes).unwrap();
        assert_eq!(checked.mime, mime);
        let mut card = card("air", Lead::None);
        card.company = Some(richview_core::CompanyBrand {
            logo: Some(richview_core::CompanyLogo {
                bytes: bytes.to_vec(),
                mime: checked.mime,
                width: checked.width,
                height: checked.height,
            }),
            ..Default::default()
        });
        let png = tiny_skia::Pixmap::decode_png(&render(&card).unwrap()).unwrap();
        let pixel = ((104 * 1200 + 128) * 4) as usize;
        let rgba = &png.data()[pixel..pixel + 4];
        assert!(
            u16::from(rgba[1]) > u16::from(rgba[0]) + 40
                && u16::from(rgba[2]) > u16::from(rgba[0]) + 40
                && rgba[3] == 255,
            "{mime} logo was not drawn: {rgba:?}"
        );
    }
}

#[test]
fn share_card_uses_the_dark_mark_without_a_plate_on_dark_paper() {
    let mut company = company_logo();
    let mut image = tiny_skia::Pixmap::new(3, 2).unwrap();
    image.fill(tiny_skia::Color::from_rgba8(255, 255, 255, 255));
    let bytes = image.encode_png().unwrap();
    let checked = richview_core::validate_company_logo(&bytes).unwrap();
    let dark = richview_core::CompanyLogo {
        bytes,
        mime: checked.mime,
        width: checked.width,
        height: checked.height,
    };
    let dark_uri = dark.data_uri();
    let light_uri = company.logo.as_ref().unwrap().data_uri();
    company.logo_dark = Some(dark);
    let (on_dark, _) = company_mark(Some(&company), true, 72.0).unwrap();
    let (on_light, _) = company_mark(Some(&company), false, 72.0).unwrap();
    assert!(on_dark.contains(&dark_uri));
    assert!(!on_dark.contains("fill=\"#fff\""));
    assert!(on_light.contains(&light_uri));
}

#[test]
fn company_mark_does_not_change_the_neutral_card() {
    let mut card = card("air", Lead::None);
    card.title.clear();
    card.byline.clear();
    let neutral = compose(&card);
    let mut company = company_logo();
    company.accent = Some("#0b6e69".into());
    card.company = Some(company);
    assert_eq!(compose(&card), neutral);
}

#[test]
fn a_thumbnail_keeps_the_cards_aspect_and_its_ink() {
    let card = render(&card(
        "scientific",
        Lead::Cover {
            bytes: CHART.as_bytes().to_vec(),
            mime: "image/svg+xml".into(),
        },
    ))
    .unwrap();
    let small = thumbnail(&card, 120).unwrap();
    assert_eq!(size(&small), (120, 63));
    // The chart's coloured bars survive the reduction: the copy is not one flat colour.
    let pixmap = tiny_skia::Pixmap::decode_png(&small).unwrap();
    let first = pixmap.pixels()[0];
    assert!(pixmap.pixels().iter().any(|pixel| *pixel != first));
    assert!(thumbnail(&card, 0).is_err());
    assert!(thumbnail(&card, 1200).is_err());
    assert!(thumbnail(b"not a png", 120).is_err());
}

/// The chart payloads of a small report, rendered by core as a host would.
fn charts_of(source: &str) -> Vec<serde_json::Value> {
    let options = richview_core::document::RenderOptions {
        report_folder: Some(richview_core::document::ReportFolder {
            root: "top".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    richview_core::document::render(source, &options)
        .unwrap()
        .charts
}

const RATES: &str = "---\ntitle: Rates\nstyles: [scientific]\n---\n\n```chart\ntype: bar\ntitle: Rates rose every month\nsubtitle: Percent, by month\ndata:\n  inline:\n    - {month: Jan, rate: 1}\n    - {month: Feb, rate: 2}\n    - {month: Mar, rate: 3}\nencoding: {x: month, y: rate}\n```\n";

#[test]
fn the_top_of_the_report_leads_with_the_title_over_the_whole_first_chart() {
    let charts = charts_of(RATES);
    let first = top::first(&charts, 1072.0).unwrap();
    let svg = top::card_svg("Rates", "Daniel", "scientific", &first, None).unwrap();
    // The title, the chart's own title and subtitle and the byline, each once; the chart is whole.
    for text in ["Rates rose every month", "Percent, by month", "Daniel"] {
        assert_eq!(svg.matches(&format!(">{text}</text>")).count(), 1, "{text}");
    }
    assert!(svg.contains("font-size=\"58\"") && svg.contains("data:image/svg+xml;base64,"));
    assert!(!svg.contains('…'));
    let png = card_png("Rates", "Daniel", "scientific", &charts).unwrap();
    assert_eq!(size(&png), (WIDTH, HEIGHT));
    assert_eq!(
        png,
        card_png("Rates", "Daniel", "scientific", &charts).unwrap()
    );
    assert_ne!(png, card_png("Rates", "Daniel", "scientific", &[]).unwrap());
}

#[test]
fn a_long_title_steps_down_to_48_px_and_is_never_cut() {
    let charts = charts_of(RATES);
    let first = top::first(&charts, 1072.0).unwrap();
    let two = "Rates rose in every month of the first quarter";
    assert!(
        top::card_svg(two, "", "scientific", &first, None)
            .unwrap()
            .contains("font-size=\"58\"")
    );
    let three = "Rates rose in every month of the first quarter, faster than in any quarter since the series began, and fastest in March";
    let svg = top::card_svg(three, "", "scientific", &first, None).unwrap();
    assert!(svg.contains("font-size=\"48\"") && !svg.contains("font-size=\"58\""));
    let words: String = svg
        .split("font-size=\"48\"")
        .skip(1)
        .map(|part| part.split('>').nth(1).unwrap().split('<').next().unwrap())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(words, three);
    // A title that leaves the chart no room gets the title card instead.
    let endless = "A very long title ".repeat(20);
    assert!(top::card_svg(&endless, "", "scientific", &first, None).is_none());
    assert_eq!(
        size(&card_png(&endless, "", "scientific", &charts).unwrap()),
        (WIDTH, HEIGHT)
    );
}

#[test]
fn a_faceted_chart_shows_its_first_row_of_panels() {
    let source = "# Sets\n\n```chart\ntype: scatter\ntitle: Four sets\ndata:\n  inline:\n    - {set: I, x: 1, y: 2}\n    - {set: II, x: 1, y: 3}\n    - {set: III, x: 2, y: 2}\n    - {set: IV, x: 2, y: 4}\n    - {set: I, x: 3, y: 5}\nencoding: {x: x, y: y, facet: set}\n```\n";
    let charts = charts_of(source);
    let sets = |width: f64| {
        let rows = top::first(&charts, width).unwrap().rows;
        richview_chart::rows::distinct(&rows, "set")
    };
    assert_eq!(sets(1072.0), ["I", "II"]);
    assert_eq!(sets(400.0), ["I"]);
}

#[test]
fn a_gallery_picture_is_the_first_chart_at_the_asked_size_or_nothing() {
    let charts = charts_of(RATES);
    let picture = |width, height, scale| {
        chart_picture_png("scientific", &charts, width, height, scale)
            .unwrap()
            .unwrap()
    };
    let png = picture(1000, 750, 1.0);
    assert_eq!(size(&png), (1000, 750));
    assert_eq!(png, picture(1000, 750, 1.0));
    // Laid out at half the width and drawn twice as large, for a card that shows it bigger.
    assert_eq!(size(&picture(500, 310, 2.0)), (1000, 620));
    assert!(
        chart_picture_png("scientific", &[], 1000, 750, 1.0)
            .unwrap()
            .is_none()
    );
    // The picture is named by the chart it shows, not by the report.
    assert_eq!(
        chart_picture_title(&charts).as_deref(),
        Some("Rates rose every month")
    );
    assert_eq!(chart_picture_title(&[]), None);
}

#[test]
fn a_small_card_is_the_card_drawn_small_at_its_aspect() {
    let charts = charts_of(RATES);
    let svg = card_svg_with_company("Rates", "Daniel", "scientific", &charts, None).unwrap();
    // The full card is this SVG drawn at its own size.
    assert_eq!(
        card_png("Rates", "Daniel", "scientific", &charts).unwrap(),
        raster(&svg, WIDTH, HEIGHT).unwrap()
    );
    let small = card_png_small(&svg, 120).unwrap();
    assert_eq!(size(&small), (120, 63));
    // The chart survives at this size: the copy is not one flat colour.
    let pixmap = tiny_skia::Pixmap::decode_png(&small).unwrap();
    let first = pixmap.pixels()[0];
    assert!(pixmap.pixels().iter().any(|pixel| *pixel != first));
    let plain = card_svg_with_company("Rates", "Daniel", "scientific", &[], None).unwrap();
    assert_ne!(small, card_png_small(&plain, 120).unwrap());
    assert!(card_png_small(&svg, 0).is_err());
    assert!(card_png_small(&svg, WIDTH).is_err());
}
