//! What part 06's component cards promise, read from the rendered report of every builtin
//! (STY-34, STY-35, STY-37, STY-38, STY-61). The tests parse the emitted stylesheet into rules
//! and judge each rule's selector and properties; they do not compare against copies of it.

use regex::Regex;
use richview_core::builtin_style_names;
use richview_core::document::{RenderOptions, ReportFolder, render};

fn html(style: &str) -> String {
    render(
        SOURCE,
        &RenderOptions {
            styles: vec![style.to_owned()],
            report_folder: Some(ReportFolder {
                root: "style-rules-fixture".into(),
                source: "report.rmd".into(),
                files: std::iter::once("report.rmd".into()).collect(),
            }),
            ..Default::default()
        },
    )
    .expect("renders")
    .html
}

const SOURCE: &str = "---\ntitle: Rules\n---\n\nText ~~gone~~ and `code`.\n\n> A quote\n\n```\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n| Total | 3 |\n\n::: callout\nNote\n:::\n";

/// Every `selector { declarations }` pair of the report's stylesheets, at-rules flattened.
fn rules(style: &str) -> Vec<(String, String)> {
    let page = html(style);
    let sheet = Regex::new(r"(?s)<style[^>]*>(.*?)</style>").unwrap();
    let comment = Regex::new(r"(?s)/\*.*?\*/").unwrap();
    let rule = Regex::new(r"([^{}]+)\{([^{}]*)\}").unwrap();
    let mut found = vec![];
    for block in sheet.captures_iter(&page) {
        let css = comment.replace_all(&block[1], "");
        for r in rule.captures_iter(&css) {
            let selector = r[1].trim().trim_start_matches('{').trim().to_owned();
            found.push((selector, r[2].trim().to_owned()));
        }
    }
    assert!(found.len() > 50, "{style}: the stylesheet was parsed");
    found
}

fn declares(body: &str, property: &str) -> Option<String> {
    body.split(';').find_map(|d| {
        let (name, value) = d.split_once(':')?;
        (name.trim() == property).then(|| value.trim().to_owned())
    })
}

#[test]
fn only_floating_elements_cast_a_shadow_and_nothing_gains_one_on_hover() {
    // STY-37: the readout, hover dot, menus and dialogs float; report blocks, buttons and
    // code panels do not.
    let floating = Regex::new(r"readout|dialog|menu|tooltip|hover-dot|popover").unwrap();
    for style in builtin_style_names() {
        for (selector, body) in rules(style) {
            let Some(shadow) = declares(&body, "box-shadow") else {
                continue;
            };
            if shadow == "none" {
                continue;
            }
            assert!(
                floating.is_match(&selector),
                "{style}: `{selector}` casts a shadow but does not float"
            );
            assert!(
                !selector.contains(":hover"),
                "{style}: `{selector}` gains a shadow on hover"
            );
        }
    }
}

#[test]
fn hover_and_focus_change_colour_never_position_or_size() {
    // STY-38: nothing lifts, scales, glows or moves layout.
    let moving = [
        "transform",
        "translate",
        "scale",
        "rotate",
        "margin",
        "margin-top",
        "margin-left",
        "margin-right",
        "margin-bottom",
        "padding",
        "top",
        "left",
        "right",
        "bottom",
        "width",
        "height",
        "min-width",
        "min-height",
        "inset",
        "zoom",
        "filter",
    ];
    for style in builtin_style_names() {
        for (selector, body) in rules(style) {
            // A pseudo-element that draws the focus ring is positioned inside its own box.
            if !(selector.contains(":hover") || selector.contains(":focus"))
                || selector.contains("::")
            {
                continue;
            }
            for property in moving {
                assert!(
                    declares(&body, property).is_none(),
                    "{style}: `{selector}` sets {property}"
                );
            }
        }
    }
}

#[test]
fn reports_carry_no_gradients_stripes_or_side_rules() {
    // STY-34: rules mark structure; no other dividers, side rules, stripes or gradients.
    for style in builtin_style_names() {
        for (selector, body) in rules(style) {
            // Charts paint their own surfaces, and a code panel may hint at its scroll edge. A
            // table box that scrolls fades its cut side into the surface it sits on, the edge cue
            // the lead required (S27-879, 4 Nov 07:24 CET); only those two pseudo-elements may.
            if selector.contains("rv-chart")
                || selector.contains("rv-scrolly")
                || selector.split(',').all(|part| part.trim().ends_with("pre"))
                || selector.split(',').all(|part| {
                    matches!(
                        part.trim(),
                        "[data-rv-cues]::before" | "[data-rv-cues]::after"
                    )
                })
            {
                continue;
            }
            assert!(
                !body.contains("gradient("),
                "{style}: `{selector}` paints a gradient"
            );
            if selector.contains("blockquote") || selector.contains("rv-callout") {
                for side in ["border-left", "border-inline-start", "border-right"] {
                    let value = declares(&body, side).unwrap_or_default();
                    assert!(
                        value.is_empty() || value == "0" || value.starts_with("none"),
                        "{style}: `{selector}` has a side rule ({side}: {value})"
                    );
                }
            }
        }
    }
}

#[test]
fn quotes_code_and_total_rows_follow_the_notes_card() {
    // STY-61: quotes in ink-soft without a side rule; code on a bg-elev panel in the data
    // face; a total row at weight 600 under a rule.
    for style in builtin_style_names() {
        let all = rules(style);
        let of = |needle: &str, property: &str| {
            all.iter()
                .filter(|(selector, _)| selector.split(',').any(|s| s.trim().ends_with(needle)))
                .filter_map(|(_, body)| declares(body, property))
                .next_back()
        };
        assert_eq!(
            of("blockquote", "color").as_deref(),
            Some("var(--rv-ink-soft)"),
            "{style}: quote ink"
        );
        let pre = of("pre", "background").unwrap_or_default();
        assert!(pre.contains("--rv-bg-elev"), "{style}: code panel is {pre}");
        let font = of("pre", "font").unwrap_or_default();
        assert!(
            font.contains("--rv-font-mono"),
            "{style}: code face is {font}"
        );
    }
}

#[test]
fn hand_drawn_strikes_through_in_pencil_and_the_others_keep_a_straight_line() {
    // STY-35: only Hand-drawn draws its marks in pencil.
    for style in builtin_style_names() {
        let page = html(style);
        let pencil = rules(style).iter().any(|(selector, body)| {
            selector.contains("del")
                && body.contains("text-decoration:none")
                && body.contains("svg+xml")
        });
        assert!(
            page.contains("<del>") || page.contains("<s>"),
            "{style}: strikethrough renders"
        );
        assert_eq!(pencil, *style == "hand-drawn", "{style}: pencil strike");
    }
}

fn token(style: &str, name: &str) -> Option<String> {
    let resolved = richview_core::resolve_styles(&[style.to_owned()], &mut vec![]);
    resolved.document["tokens"][name]
        .as_str()
        .map(str::to_owned)
}

#[test]
fn each_builtin_sets_its_measure_and_the_report_column_is_one_width() {
    // STY-30, widened by a fifth on Daniel's call of 2026-11-15 (S27-986): Sleek and Air 42rem,
    // Scientific 78ch, Brutalist 38rem, Hand-drawn 41rem.
    // STY-31: one report width for every style, centred (margin auto on the report).
    let measures = [
        ("sleek", "42rem"),
        ("air", "42rem"),
        ("scientific", "78ch"),
        ("brutalist", "38rem"),
        ("hand-drawn", "41rem"),
    ];
    for (style, measure) in measures {
        assert_eq!(token(style, "measure").as_deref(), Some(measure), "{style}");
        let page = rules(style);
        let report = page
            .iter()
            .find(|(selector, body)| selector == ".rv-report" && body.contains("max-width"))
            .expect("the report column rule");
        assert!(
            declares(&report.1, "margin").is_some_and(|m| m.contains("auto")),
            "{style}: the column is centred"
        );
    }
    // The width is the layout token every builtin sets, 880 px with 24 px gutters (16 px on
    // phones) and a 4 rem top margin (1.5 rem on phones).
    for style in builtin_style_names() {
        let resolved = richview_core::resolve_styles(&[style.to_string()], &mut vec![]);
        assert_eq!(
            resolved.document["layout"]["report-max-width"].as_str(),
            Some("880px"),
            "{style}: the report column is 880 px"
        );
        let report: Vec<(String, String)> = rules(style)
            .into_iter()
            .filter(|(selector, body)| selector == ".rv-report" && body.contains("padding"))
            .collect();
        let find = |top: &str| {
            report
                .iter()
                .find(|(_, body)| declares(body, "margin").is_some_and(|m| m.starts_with(top)))
                .map(|(_, body)| body.clone())
                .unwrap_or_else(|| panic!("{style}: no report rule with a {top} top margin"))
        };
        let (desktop, phone) = (find("4rem auto"), find("1.5rem auto"));
        assert!(
            declares(&desktop, "padding").is_some_and(|p| p.starts_with("0 1.5rem")),
            "{style}: 24 px gutters"
        );
        assert!(
            declares(&phone, "padding").is_some_and(|p| p.starts_with("0 1rem")),
            "{style}: 16 px phone gutters"
        );
    }
}

#[test]
fn the_colophon_shows_by_default_and_a_style_file_can_hide_it() {
    // STY-04.
    let render_with = |styles: Vec<String>| {
        render(
            SOURCE,
            &RenderOptions {
                styles,
                report_folder: Some(ReportFolder {
                    root: "colophon-fixture".into(),
                    source: "report.rmd".into(),
                    files: std::iter::once("report.rmd".into()).collect(),
                }),
                ..Default::default()
            },
        )
        .unwrap()
        .html
    };
    for style in builtin_style_names() {
        let shown = render_with(vec![style.to_string()]);
        assert!(
            shown.contains("richview.ai"),
            "{style}: the colophon links home"
        );
        let hidden = render_with(vec![style.to_string(), "colophon: hide".into()]);
        assert!(
            !hidden.contains("richview.ai"),
            "{style}: colophon: hide removes it"
        );
    }
}

#[test]
fn chart_actions_are_quiet_text_buttons_with_reachable_hit_areas() {
    // STY-42: ink-soft in the UI face, turning ink on hover; a pressed toggle is underlined, never
    // filled; hit areas at least 32 px.
    for style in builtin_style_names() {
        let all = rules(style);
        let action = |pseudo: &str| {
            all.iter()
                .filter(|(s, _)| {
                    s.split(',')
                        .any(|p| p.trim() == format!(".rv-chart__action{pseudo}"))
                })
                .map(|(_, b)| b.clone())
                .collect::<Vec<_>>()
                .join(";")
        };
        let base = action("");
        assert_eq!(
            declares(&base, "color").as_deref(),
            Some("var(--rv-ink-soft)"),
            "{style}"
        );
        assert!(base.contains("--rv-font-ui"), "{style}: UI face");
        assert!(
            declares(&base, "min-height").is_some_and(|h| h
                .trim_end_matches("px")
                .parse::<f64>()
                .unwrap_or(0.0)
                >= 32.0),
            "{style}: 32 px hit area ({base})"
        );
        assert_eq!(
            declares(&action(":hover"), "color").as_deref(),
            Some("var(--rv-ink)"),
            "{style}: hover turns ink"
        );
        let pressed = all
            .iter()
            .filter(|(s, _)| s.contains(".rv-chart__action") && s.contains("aria-pressed"))
            .map(|(_, b)| b.clone())
            .collect::<Vec<_>>()
            .join(";");
        assert!(
            pressed.contains("underline"),
            "{style}: pressed is underlined"
        );
        assert!(
            !pressed.contains("background:var") && !pressed.contains("background: var"),
            "{style}: pressed is never filled"
        );
    }
}

#[test]
fn the_margin_column_is_gone_from_the_vocabulary_and_the_page() {
    // STY-32: footnotes are endnotes and nothing floats right of the measure.
    for style in builtin_style_names() {
        assert!(
            !html(style).contains("margin-column"),
            "{style}: no margin-column hook"
        );
    }
    let mut diagnostics = vec![];
    richview_core::resolve_styles(
        &["layout: {margin-column: true}".to_string()],
        &mut diagnostics,
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "style.unknown-key" && d.path.ends_with("layout.margin-column")),
        "{diagnostics:?}"
    );
}

#[test]
fn only_images_embeds_code_the_readout_and_bar_ends_have_a_radius() {
    // STY-36: "Nothing else has a radius"; the readout's is at most 4 px.
    let allowed = Regex::new(r"img|iframe|\bpre\b|code|readout|swatch|thumb").unwrap();
    for style in builtin_style_names() {
        for (selector, body) in rules(style) {
            let Some(radius) = declares(&body, "border-radius") else {
                continue;
            };
            if radius == "0" || radius == "0px" || radius == "50%" {
                continue;
            }
            // A shaded chart frame is a style's own design choice (STY-01).
            if selector.contains("rv-chart") && !selector.contains("__") {
                continue;
            }
            assert!(
                allowed.is_match(&selector),
                "{style}: `{selector}` has a radius ({radius})"
            );
            if selector.contains("readout") {
                assert!(radius.contains("4px"), "{style}: readout radius {radius}");
            }
        }
    }
}

#[test]
fn the_readout_is_a_small_raised_hairline_panel_in_every_builtin() {
    // STY-41 (assumed): the raised surface at 100% with no blur, a 1 px hairline in `rule`, 8 px padding,
    // 12 px text, values in the data face, at most 300 px wide.
    for style in builtin_style_names() {
        let all = rules(style);
        let body: String = all
            .iter()
            .filter(|(s, _)| s == ".rv-chart__readout")
            .map(|(_, b)| b.clone())
            .collect::<Vec<_>>()
            .join(";");
        let background = declares(&body, "background").unwrap_or_default();
        assert!(
            background.contains("100%") && background.contains("--rv-bg-elev"),
            "{style}: {background}"
        );
        assert!(
            !body.contains("backdrop-filter") && !body.contains("blur("),
            "{style}: no blur"
        );
        assert_eq!(
            declares(&body, "border").as_deref(),
            Some("1px solid var(--rv-rule)"),
            "{style}"
        );
        assert_eq!(
            declares(&body, "padding").as_deref(),
            Some("8px"),
            "{style}"
        );
        assert!(
            declares(&body, "font").is_some_and(|f| f.starts_with("12px")),
            "{style}: 12 px text"
        );
        let width = declares(&body, "max-width").unwrap_or_default();
        // A content-sized panel also fits inside the chart's 8 px inset on each side.
        let cap = width
            .strip_prefix("min(")
            .and_then(|v| v.strip_suffix(')'))
            .and_then(|v| v.split_once(','))
            .filter(|(_, available)| available.trim() == "calc(100% - 16px)")
            .map_or(width.as_str(), |(cap, _)| cap.trim());
        let px = cap
            .strip_suffix("rem")
            .and_then(|n| n.parse::<f64>().ok())
            .map(|n| n * 16.0);
        assert!(
            px.is_some_and(|px| px <= 300.0),
            "{style}: max-width {width}"
        );
        let value: String = all
            .iter()
            .filter(|(s, _)| s == ".rv-chart__readout-value")
            .map(|(_, b)| b.clone())
            .collect();
        assert!(
            value.contains("--rv-font-mono"),
            "{style}: values in the data face"
        );
    }
}

/// The selector parts of a rule whose subject (last compound) is exactly `class`.
fn targets(selector: &str, class: &str) -> bool {
    let subject = Regex::new(&format!(
        r"(?:^|[\s>+~]){}(?:::?[\w-]+(?:\([^)]*\))?)*$",
        regex::escape(class)
    ))
    .unwrap();
    selector
        .split(',')
        .any(|part| subject.is_match(part.trim()))
}

#[test]
fn consecutive_metrics_stay_stacked_stat_lines_at_every_width() {
    // FEA-RP-24 as proposed: no builtin turns a run of metrics into a row. A row needs a metric
    // that floats, sits inline or takes less than the full line, or a body in columns.
    let row_display =
        Regex::new(r"^(inline|inline-block|inline-flex|inline-grid|grid|table-cell)$").unwrap();
    for style in builtin_style_names() {
        for (selector, body) in rules(style) {
            if targets(&selector, ".rv-metric") {
                assert!(
                    declares(&body, "float").is_none_or(|v| v == "none"),
                    "{style}: `{selector}` floats a metric"
                );
                assert!(
                    declares(&body, "display").is_none_or(|v| !row_display.is_match(&v)),
                    "{style}: `{selector}` lays a metric inline or in a grid"
                );
                for property in ["width", "flex-basis"] {
                    assert!(
                        declares(&body, property)
                            .is_none_or(|v| matches!(v.as_str(), "100%" | "auto")),
                        "{style}: `{selector}` narrows a metric below the line"
                    );
                }
            }
            if targets(&selector, ".rv-report__body") {
                for property in ["column-count", "columns", "column-width"] {
                    assert!(
                        declares(&body, property).is_none(),
                        "{style}: `{selector}` sets the body in columns"
                    );
                }
                assert!(
                    declares(&body, "display")
                        .is_none_or(|v| !v.contains("grid") && !v.contains("flex")),
                    "{style}: `{selector}` lays the body out as a row or grid"
                );
            }
        }
    }
}

#[test]
fn a_metric_value_is_a_tabular_figure_at_the_h1_size_in_every_builtin() {
    // FEA-RP-23: the value takes the style's h1 size, smaller than the title, with tabular figures.
    for style in builtin_style_names() {
        let value: Vec<String> = rules(style)
            .into_iter()
            .filter(|(selector, _)| targets(selector, ".rv-metric__value"))
            .map(|(_, body)| body)
            .collect();
        let last = |property: &str| value.iter().rev().find_map(|body| declares(body, property));
        let size = last("font-size").unwrap_or_default();
        assert!(
            size.contains("--rv-size-h1"),
            "{style}: the metric value is set at `{size}`, not the h1 size"
        );
        assert!(
            last("font-variant-numeric").is_some_and(|v| v.contains("tabular-nums")),
            "{style}: the metric value has no tabular figures"
        );
    }
}

#[test]
fn numeric_table_columns_are_right_aligned_with_tabular_figures() {
    // FEA-RP-19: numeric cells line up on the right edge, in every builtin.
    for style in builtin_style_names() {
        let cell: Vec<String> = rules(style)
            .into_iter()
            .filter(|(selector, _)| targets(selector, "td.num"))
            .map(|(_, body)| body)
            .collect();
        let last = |property: &str| cell.iter().rev().find_map(|body| declares(body, property));
        assert_eq!(
            last("text-align").as_deref(),
            Some("right"),
            "{style}: numeric cells align right"
        );
        assert!(
            last("font-variant-numeric").is_some_and(|v| v.contains("tabular-nums")),
            "{style}: numeric cells have no tabular figures"
        );
    }
}

#[test]
fn a_callout_title_is_set_in_the_display_face_and_an_image_never_outgrows_its_column() {
    // FEA-RP-25 and FEA-RP-26: the run-in title takes the display face; the picture is at most
    // its column wide with its height following the aspect ratio, and print caps its height.
    for style in builtin_style_names() {
        let all = rules(style);
        assert!(
            all.iter()
                .filter(|(selector, _)| targets(selector, ".rv-callout__title"))
                .filter_map(|(_, body)| declares(body, "font-family"))
                .any(|family| family.contains("--rv-font-display")),
            "{style}: the callout title is not in the display face"
        );
        let image: Vec<&String> = all
            .iter()
            .filter(|(selector, _)| targets(selector, ".rv-image img"))
            .map(|(_, body)| body)
            .collect();
        let last = |property: &str| image.iter().rev().find_map(|body| declares(body, property));
        assert_eq!(
            last("max-width").as_deref(),
            Some("100%"),
            "{style}: an image is at most its column wide"
        );
        assert_eq!(
            last("height").as_deref(),
            Some("auto"),
            "{style}: an image keeps its aspect ratio"
        );
    }
}

#[test]
fn the_colophon_never_drags_a_figure_to_the_next_page() {
    // FEA-RP-43: a one-line colophon kept with the last block must not push a chart or table, and
    // half a page of blank, onto a new page. The keep rule applies only after a text block.
    for style in builtin_style_names() {
        for (selector, body) in rules(style) {
            if targets(&selector, ".rv-report__footer")
                && declares(&body, "break-before").is_some_and(|v| v == "avoid")
            {
                assert!(
                    selector.contains(":has(") && selector.contains("p,"),
                    "{style}: `{selector}` keeps the colophon with any last block, a figure included"
                );
            }
        }
    }
}

#[test]
fn legend_swatches_are_small_and_a_hidden_series_strikes_through() {
    // STY-40: legend swatches 9 px squares, 14 x 2 px lines or 9 px dots; a hidden series shows at
    // 45% with a strike. The rest of the chart chrome (gridlines, the zero line, ticks, data
    // lines, areas and bands) is geometry: `richview-chart` tests/chart_chrome.rs.
    for style in builtin_style_names() {
        let all = rules(style);
        let last = |class: &str, property: &str| {
            all.iter()
                .rev()
                .filter(|(selector, _)| targets(selector, class))
                .find_map(|(_, body)| declares(body, property))
        };
        for (class, property, value) in [
            (".rv-chart__swatch", "width", "9px"),
            (".rv-chart__swatch", "height", "9px"),
            (".rv-chart__swatch--line", "width", "14px"),
            (".rv-chart__swatch--line", "height", "2px"),
            (".rv-chart__swatch--dot", "border-radius", "50%"),
        ] {
            assert_eq!(
                last(class, property).as_deref(),
                Some(value),
                "{style}: {class} {property}"
            );
        }
        assert!(
            all.iter()
                .any(|(selector, body)| selector.contains("[aria-pressed=false]")
                    && selector.contains("rv-chart__legend-entry")
                    && declares(body, "opacity").as_deref() == Some(".45")
                    && declares(body, "text-decoration")
                        .is_some_and(|d| d.contains("line-through"))),
            "{style}: a hidden series shows at 45% with a strike"
        );
    }
}

#[test]
fn style_accents_follow_the_table_and_air_is_not_sleeks_blue() {
    // STY-02: Sleek electric blue, Scientific the brand blue, Brutalist deep blue; Air a shade
    // of the brand blue distinct from Sleek's.
    use richview_chart::color;
    let accent = |name: &str| {
        let mut diagnostics = vec![];
        let style = richview_core::resolve_styles(&[name.to_owned()], &mut diagnostics);
        style.document["tokens"]["accent"]
            .as_str()
            .and_then(color::parse)
            .unwrap_or_else(|| panic!("{name} has an accent"))
    };
    let close = |a: color::Oklch, l: f64, c: f64, h: f64| {
        (a.l - l).abs() < 0.01 && (a.c - c).abs() < 0.01 && (a.h - h).abs() < 1.5
    };
    assert!(
        close(accent("sleek"), 0.5464, 0.2508, 263.51),
        "sleek accent"
    );
    assert!(
        close(accent("scientific"), 0.5196, 0.1248, 247.06),
        "scientific accent is the brand blue"
    );
    assert!(
        close(accent("brutalist"), 0.4128, 0.1233, 255.65),
        "brutalist accent"
    );
    let (air, sleek) = (accent("air"), accent("sleek"));
    assert!(
        (air.l - sleek.l).abs() > 0.02
            || (air.c - sleek.c).abs() > 0.02
            || (air.h - sleek.h).abs() > 3.0,
        "air shares sleek's blue"
    );
}
