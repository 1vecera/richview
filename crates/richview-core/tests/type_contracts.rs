//! Author-facing type boundaries that are independent of chart drawing and host resolution.
use richview_core::document::{
    Document, Output, RenderOptions, ReportFolder, check, parse, render,
};
use serde_json::{Value, json};

fn block(tag: &str, value: &Value) -> Document {
    parse(&format!("```{tag}\n{value}\n```\n")).unwrap()
}

fn chart() -> Value {
    json!({"type":"bar","title":"A comparison","encoding":{"x":"name","y":"value"},"data":{"inline":[{"name":"A","value":1}]}})
}

#[test]
fn document_framing_keeps_bom_frontmatter_recovery_and_ordinary_fences_distinct() {
    let bom = parse("\u{feff}---\ntitle: A report\n...\n\nBody.\n").unwrap();
    assert_eq!(bom.frontmatter["title"], "A report");
    assert!(bom.body.contains("Body.") && !bom.body.contains("title:"));
    assert!(bom.diagnostics.is_empty());
    for source in ["# A draft\n", "\n---\ntitle: Body text\n---\n"] {
        let document = parse(source).unwrap();
        assert!(document.frontmatter.as_object().unwrap().is_empty());
        assert_eq!(document.body, source);
    }
    let unclosed = parse("---\ntitle: Body text\n\nBody.\n").unwrap();
    assert!(unclosed.body.contains("title: Body text"));
    let finding = unclosed
        .diagnostics
        .iter()
        .find(|d| d.code == "frontmatter.unclosed")
        .unwrap();
    assert_eq!((finding.level.as_str(), finding.line), ("warning", Some(1)));
    // Accepted FEA-RP-58 makes closed unreadable frontmatter an error, with a body that survives.
    for (header, code) in [
        ("- not\n- a mapping", "frontmatter.mapping"),
        ("title: [unclosed", "frontmatter.yaml"),
    ] {
        let document = parse(&format!("---\n{header}\n---\n\nBody survives.\n")).unwrap();
        assert!(document.body.contains("Body survives.") && !document.body.contains(header));
        assert!(
            document
                .diagnostics
                .iter()
                .any(|d| d.code == code && d.level == "error")
        );
    }
    let fences = parse("~~~ MeTrIc  \nvalue: 2\n~~~\n\n```rust\nlet ordinary = true;\n```\n\n```metric\nvalue: []\n```\n").unwrap();
    assert_eq!(fences.blocks.len(), 1);
    assert_eq!(fences.blocks[0].tag, "metric");
    assert_eq!(fences.blocks[0].value["value"], 2);
    assert!(fences.body.contains("let ordinary = true;"));
    assert_eq!(fences.failed.len(), 1);
    assert!(fences.failed[0].source.contains("value: []"));
    assert!(
        fences
            .diagnostics
            .iter()
            .any(|d| d.code == "block.schema" && d.level == "error")
    );
}

#[test]
fn engine_spec_and_reader_interaction_options_cannot_enter_an_authored_chart() {
    for key in [
        "interaction",
        "navigator",
        "panes",
        "view",
        "time",
        "transforms",
        "spec",
        "demo",
        "max_gap",
        "end_label",
        "notes",
    ] {
        let mut value = chart();
        value[key] = json!({"enabled":false,"source":"not an authored capability"});
        let document = block("chart", &value);
        assert_eq!(
            document.blocks.len(),
            1,
            "{key}: {:?}",
            document.diagnostics
        );
        assert!(document.blocks[0].value.get(key).is_none(), "{key}");
        assert!(
            document
                .diagnostics
                .iter()
                .any(|d| d.code == "block.unknown-key" && d.path == format!("chart.{key}")),
            "{key}: {:?}",
            document.diagnostics
        );
    }
}

#[test]
fn authored_control_fields_survive_worker_documents_and_use_reader_defaults() {
    use richview_chart::reader_controls::{Controls, Kind, Presentation};

    for (as_value, presentation) in [
        ("buttons", Presentation::Buttons),
        ("dropdown", Presentation::Dropdown),
    ] {
        let mut value = chart();
        value["controls"] = json!([
            {"kind":"playback","id":"frame","field":"name","label":"Frame","as":as_value},
            {"kind":"filter","id":"group","field":"name","label":"Group","options":["A"],"initial":"A","as":as_value},
            {"kind":"metric","id":"measure","label":"Measure","options":[{"value":"count","label":"Count","field":"value","axis":{"label":"Count","unit":"people","format":"number","min":0,"max":10}}],"as":as_value},
            {"kind":"variant","id":"view","label":"View","options":[{"value":"bars","label":"Bars","chart":{"type":"bar"}}],"as":as_value}
        ]);
        let parsed = block("chart", &value);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let worker: Document =
            serde_json::from_slice(&serde_json::to_vec(&parsed).unwrap()).unwrap();
        let authored = &worker.blocks[0].value;
        assert_eq!(authored["controls"], value["controls"]);
        let rows = authored["data"]["inline"].as_array().unwrap();
        let controls = Controls::parse(authored, rows);
        assert_eq!(controls.list.len(), 4);
        assert!(controls.list.iter().all(|c| c.presentation == presentation));
        assert!(matches!(
            controls.list[0].kind,
            Kind::Playback { speed: 800.0, .. }
        ));
        // Explicit speed overrides the default. Older playback options still parse.
        let mut once = authored.clone();
        once["controls"][0]["speed"] = json!(1250);
        once["controls"][0]["loop"] = json!(false);
        let once = block("chart", &once);
        assert!(once.diagnostics.is_empty(), "{:?}", once.diagnostics);
        assert!(matches!(
            Controls::parse(&once.blocks[0].value, rows).list[0].kind,
            Kind::Playback { speed: 1250.0, .. }
        ));

        value["controls"][0].as_object_mut().unwrap().remove("as");
        value["controls"][0]["as_"] = json!(as_value);
        let retired = block("chart", &value);
        assert!(retired.blocks[0].value["controls"][0].get("as_").is_none());
        assert!(
            retired
                .diagnostics
                .iter()
                .any(|d| d.code == "block.unknown-key" && d.path == "chart.controls[0].as_")
        );
        value["controls"][0]["as"] = json!("select");
        let invalid = block("chart", &value);
        assert!(invalid.blocks.is_empty());
        assert!(
            invalid
                .diagnostics
                .iter()
                .any(|d| d.code == "block.schema" && d.path == "chart.controls[0].as")
        );
    }
}

#[test]
fn authored_stack_axis_bounds_and_digits_keep_their_declared_nesting() {
    for kind in ["bar", "area"] {
        let mut value = chart();
        value["type"] = json!(kind);
        value["stack"] = json!(true);
        value["digits"] = json!(4);
        value["encoding"]["color"] = json!("name");
        value["axes"] = json!({"y":{"label":"Count","unit":"people","format":"number_compact","digits":3,"min":0,"max":10}});
        let parsed = block("chart", &value);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        assert_eq!(parsed.blocks[0].value["stack"], true);
        assert_eq!(parsed.blocks[0].value["digits"], 4);
        assert_eq!(parsed.blocks[0].value["axes"], value["axes"]);
        for key in [
            "scale",
            "zero",
            "decimals",
            "secondary",
            "max_gap",
            "end_label",
        ] {
            let mut obsolete = value.clone();
            obsolete["axes"]["y"][key] = json!(1);
            let parsed = block("chart", &obsolete);
            assert!(
                parsed.blocks[0].value["axes"]["y"].get(key).is_none(),
                "{key}"
            );
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|d| d.code == "block.unknown-key"
                        && d.path == format!("chart.axes.y.{key}")),
                "{key}: {:?}",
                parsed.diagnostics
            );
        }
    }
}

#[test]
fn authored_ratios_enforce_both_limits_and_do_not_accept_the_old_top_level_key() {
    for ratio in [json!(0.5), json!(6), json!("1:2"), json!("6:1")] {
        let mut value = chart();
        value["style"] = json!({"ratio":ratio});
        let document = block("chart", &value);
        assert_eq!(
            document.blocks.len(),
            1,
            "{ratio}: {:?}",
            document.diagnostics
        );
        assert_eq!(document.blocks[0].value["style"]["ratio"], ratio);
    }
    for ratio in [
        json!(0),
        json!(-1),
        json!(0.4999),
        json!(6.0001),
        json!("wide"),
        json!("0:1"),
        json!("1:0"),
        json!("-1:2"),
    ] {
        let mut value = chart();
        value["style"] = json!({"ratio":ratio});
        let document = block("chart", &value);
        assert!(document.blocks.is_empty(), "{ratio}");
        assert!(document.diagnostics.iter().any(|d| d.code == "block.schema"
            && d.path == "chart.style.ratio"
            && d.level == "error"));
    }
    let mut value = chart();
    value["ratio"] = json!(2);
    let document = block("chart", &value);
    assert!(document.blocks[0].value.get("ratio").is_none());
    assert!(
        document
            .diagnostics
            .iter()
            .any(|d| d.code == "block.unknown-key" && d.path == "chart.ratio")
    );
}

#[test]
fn callout_and_video_required_fields_fail_at_the_authored_field() {
    for (tag, value, field) in [
        ("callout", json!({"body":"A note"}), "kind"),
        ("callout", json!({"kind":"neutral"}), "body"),
        ("callout", json!({"kind":"aside","body":"A note"}), "kind"),
        ("callout", json!({"kind":"neutral","body":42}), "body"),
        ("video", json!({"title":"A clip"}), "src"),
        ("video", json!({"src":"clip.mp4"}), "title"),
        (
            "video",
            json!({"src":"clip.mp4","title":"A clip","width":0}),
            "width",
        ),
        (
            "video",
            json!({"src":"clip.mp4","title":"A clip","height":1.5}),
            "height",
        ),
    ] {
        let document = block(tag, &value);
        assert!(document.blocks.is_empty(), "{tag}.{field}");
        assert_eq!(document.failed.len(), 1);
        assert!(
            document.diagnostics.iter().any(|d| d.code == "block.schema"
                && d.level == "error"
                && d.path == format!("{tag}.{field}")),
            "{:?}",
            document.diagnostics
        );
    }
    for kind in ["up", "down", "neutral", "hold"] {
        assert_eq!(
            block("callout", &json!({"kind":kind,"body":"**Note**"}))
                .blocks
                .len(),
            1
        );
    }
    let video = block(
        "video",
        &json!({"src":"clip.mp4","title":"A clip","poster":"poster.png","width":1920,"height":1080,"caption":"Caption"}),
    );
    assert_eq!(video.blocks.len(), 1);
    assert_eq!(video.blocks[0].value["title"], "A clip");
}

#[test]
fn retired_money_formats_name_the_number_format_and_currency_unit_replacement() {
    for format in ["currency", "currency_compact"] {
        let mut chart = chart();
        chart["axes"] = json!({"y":{"format":format}});
        for (tag, value, path) in [
            (
                "metric",
                json!({"value":12.34,"format":format}),
                "metric.format",
            ),
            (
                "table",
                json!({"data":{"inline":[{"amount":12.34}]},"columns":[{"key":"amount","format":format}]}),
                "table.columns[0].format",
            ),
            ("chart", chart, "chart.axes.y.format"),
        ] {
            let document = block(tag, &value);
            assert!(document.blocks.is_empty(), "{tag}.{format}");
            let finding = document
                .diagnostics
                .iter()
                .find(|d| d.code == "block.renamed" && d.path == path)
                .unwrap_or_else(|| panic!("{:?}", document.diagnostics));
            assert_eq!(finding.level, "error");
            let guidance = format!(
                "{} {}",
                finding.message,
                finding.fix.as_deref().unwrap_or_default()
            );
            assert!(
                guidance.contains("number") && guidance.contains("unit"),
                "{guidance}"
            );
        }
    }
}

#[test]
fn core_width_options_share_one_finite_range_and_never_fall_back() {
    let base = RenderOptions {
        report_folder: Some(ReportFolder {
            root: "type-contract".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    for output in [Output::Report, Output::Pdf] {
        for width in [240.0, 4096.0] {
            let options = RenderOptions {
                width,
                chart_widths: vec![240.0, 4096.0],
                output,
                ..base.clone()
            };
            assert!(render("# Draft", &options).is_ok());
            assert!(check("# Draft", &options).is_ok());
        }
        for width in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            239.9,
            4096.1,
            8192.0,
        ] {
            for field in ["width", "chart_widths"] {
                let mut options = RenderOptions {
                    output,
                    ..base.clone()
                };
                if field == "width" {
                    options.width = width;
                } else {
                    options.chart_widths = vec![240.0, 4096.0, width];
                }
                let Err(rendered) = render("# Draft", &options) else {
                    panic!("invalid width must stop rendering");
                };
                let Err(checked) = check("# Draft", &options) else {
                    panic!("invalid width must stop checking");
                };
                assert_eq!(rendered, checked);
                assert!(
                    rendered.iter().any(|d| d.code == "options.width"
                        && d.level == "error"
                        && d.path.starts_with(&format!("options.{field}"))
                        && d.message.contains("240")
                        && d.message.contains("4096")),
                    "{field}={width}: {rendered:?}"
                );
            }
        }
    }
}

#[test]
fn chart_type_defaults_keep_the_declared_aspect_table() {
    // These independent defaults belong to the type contract. A figure may grow to fit labels.
    for (kind, encoding, ordinary, calendar) in [
        ("line", json!({"x":"x","y":"y"}), 16.0 / 9.0, 2.0),
        ("area", json!({"x":"x","y":"y"}), 16.0 / 9.0, 2.0),
        ("bar", json!({"x":"x","y":"y"}), 16.0 / 9.0, 16.0 / 9.0),
        ("scatter", json!({"x":"x","y":"y"}), 4.0 / 3.0, 4.0 / 3.0),
        ("pie", json!({"x":"x","y":"y"}), 1.0, 1.0),
        (
            "heatmap",
            json!({"x":"x","y":"y","color":"value"}),
            16.0 / 9.0,
            16.0 / 9.0,
        ),
        ("histogram", json!({"x":"x"}), 16.0 / 9.0, 16.0 / 9.0),
        ("boxplot", json!({"x":"x","y":"y"}), 16.0 / 9.0, 16.0 / 9.0),
        (
            "interval",
            json!({"x":"x","y":"y","lower":"lower","upper":"upper"}),
            16.0 / 9.0,
            16.0 / 9.0,
        ),
    ] {
        let document = block(
            "chart",
            &json!({"type":kind,"title":"A chart","encoding":encoding,"data":{"inline":[{"x":1,"y":2,"value":3,"lower":1,"upper":3}]}}),
        );
        assert_eq!(
            document.blocks.len(),
            1,
            "{kind}: {:?}",
            document.diagnostics
        );
        let chart =
            richview_chart::block::ChartBlock::from_value(&document.blocks[0].value).unwrap();
        assert_eq!(chart.kind.default_ratio(false), ordinary, "{kind}");
        assert_eq!(chart.kind.default_ratio(true), calendar, "{kind} calendar");
    }
}

#[test]
fn dropped_label_records_keep_schema_reasons_and_one_info_per_small_drop_set() {
    let words = ["a".repeat(500), "b".repeat(500)];
    let source = format!(
        "```chart\n{}\n```",
        json!({"type":"wordcloud","title":"Two words","encoding":{"label":"word","y":"value"},"data":{"inline":[{"word":words[0],"value":2},{"word":words[1],"value":1}]}})
    );
    let options = RenderOptions {
        width: 240.0,
        report_folder: Some(ReportFolder {
            root: "type-contract".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let result = check(&source, &options).unwrap();
    assert_eq!(result.charts.len(), 1, "{:?}", result.diagnostics);
    let dropped = result.charts[0]["droppedLabels"].as_array().unwrap();
    assert_eq!(dropped.len(), 2, "{dropped:?}");
    let notices: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.code == "chart.label-dropped")
        .collect();
    assert_eq!(notices.len(), 2, "{:?}", result.diagnostics);
    for word in &words {
        let matches: Vec<_> = dropped
            .iter()
            .filter(|item| item["text"] == *word)
            .collect();
        assert_eq!(matches.len(), 1, "{word}: {dropped:?}");
        let dropped = matches[0];
        assert!(
            ["collision", "out-of-bounds", "density"]
                .contains(&dropped["reason"].as_str().unwrap())
        );
        assert_eq!(
            notices
                .iter()
                .filter(|d| d.level == "info" && d.message.contains(word))
                .count(),
            1
        );
    }
}
