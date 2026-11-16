//! Bundled report fonts: catalog faces, payload integrity, required coverage and features.
//!
//! `scripts/fonts/build.py` generates the catalogs and `font-manifest.json` from pinned
//! upstream releases; these tests keep the committed files consistent with each other and
//! with the coverage contract.

use std::collections::BTreeSet;

use regex::Regex;
use richview_core::document::{RenderOptions, render};
use serde_json::Value;

const CATALOGS: [(&str, &str); 5] = [
    ("fonts-sleek.css", include_str!("../assets/fonts-sleek.css")),
    (
        "fonts-scientific.css",
        include_str!("../assets/fonts-scientific.css"),
    ),
    (
        "fonts-brutalist.css",
        include_str!("../assets/fonts-brutalist.css"),
    ),
    (
        "fonts-editorial.css",
        include_str!("../assets/fonts-editorial.css"),
    ),
    (
        "fonts-hand-drawn.css",
        include_str!("../assets/fonts-hand-drawn.css"),
    ),
];
const MANIFEST: &str = include_str!("../assets/font-manifest.json");
const REPORTS: [&str; 3] = [
    include_str!("../../../examples/openai-vs-anthropic/report.rmd"),
    include_str!("../../../examples/sales-report/report.rmd"),
    include_str!("../../../examples/first-report/report.rmd"),
];

/// Every bundled face: catalog, CSS family, style and weight.
const FACES: [(&str, &str, &str, u32); 19] = [
    ("fonts-sleek.css", "Space Grotesk", "normal", 600),
    ("fonts-sleek.css", "Geist", "normal", 400),
    ("fonts-sleek.css", "Geist", "italic", 400),
    ("fonts-sleek.css", "Geist", "normal", 600),
    ("fonts-sleek.css", "Geist Mono", "normal", 400),
    ("fonts-scientific.css", "RichView Serif", "normal", 400),
    ("fonts-scientific.css", "RichView Serif", "italic", 400),
    ("fonts-scientific.css", "RichView Serif", "normal", 600),
    ("fonts-scientific.css", "RichView Sans", "normal", 400),
    ("fonts-scientific.css", "RichView Sans", "normal", 600),
    ("fonts-scientific.css", "RichView Code", "normal", 400),
    ("fonts-brutalist.css", "Big Shoulders", "normal", 800),
    ("fonts-brutalist.css", "Space Mono", "normal", 400),
    ("fonts-editorial.css", "Newsreader", "normal", 400),
    ("fonts-editorial.css", "Newsreader", "italic", 400),
    ("fonts-editorial.css", "Newsreader", "normal", 600),
    ("fonts-editorial.css", "RichView Mono", "normal", 400),
    ("fonts-editorial.css", "RichView Mono", "normal", 600),
    ("fonts-hand-drawn.css", "Patrick Hand", "normal", 400),
];
/// The faces lending a family the script digits or signs it lacks (STY-63): catalog, family,
/// style, weight and lender, one face per lender. RichView Sans alone draws ⁺ ⁻ ₊ ₋, so it lends
/// them to every other face, with Patrick Hand's subscripts; RichView Serif lends Newsreader its
/// digits.
fn lent_faces() -> Vec<(&'static str, &'static str, &'static str, u32, &'static str)> {
    let lent = |lender: &'static str| {
        move |&(catalog, family, style, weight): &(
            &'static str,
            &'static str,
            &'static str,
            u32,
        )| { (catalog, family, style, weight, lender) }
    };
    FACES
        .iter()
        .filter(|face| face.1 != "RichView Sans")
        .map(lent("RichView Sans"))
        .chain(
            FACES
                .iter()
                .filter(|face| face.1 == "Newsreader")
                .map(lent("RichView Serif")),
        )
        .collect()
}
/// The superior digits ¹ ² ³ ⁰ ⁴–⁹, the inferior digits ₀–₉, and the signs ⁺ ⁻ ₊ ₋.
const SCRIPT_SETS: [&[u32]; 3] = [
    &[
        0xB9, 0xB2, 0xB3, 0x2070, 0x2074, 0x2075, 0x2076, 0x2077, 0x2078, 0x2079,
    ],
    &[
        0x2080, 0x2081, 0x2082, 0x2083, 0x2084, 0x2085, 0x2086, 0x2087, 0x2088, 0x2089,
    ],
    &[0x207A, 0x207B, 0x208A, 0x208B],
];

/// General Punctuation, spaces, the true minus, math signs, arrows and the euro, required
/// in every face in addition to Basic Latin, Latin-1 and Latin Extended-A.
const REQUIRED_EXTRA: [u32; 27] = [
    0x2007, 0x2009, 0x2010, 0x2011, 0x2013, 0x2014, 0x2018, 0x2019, 0x201A, 0x201C, 0x201D, 0x201E,
    0x2026, 0x202F, 0x2030, 0x2032, 0x2033, 0x20AC, 0x2190, 0x2191, 0x2192, 0x2193, 0x2212, 0x2248,
    0x2260, 0x2264, 0x2265,
];
/// Archaic letters an upstream design may omit: kra, T and t with stroke, long s.
const GAPS_ALLOWED: [u32; 4] = [0x0138, 0x0166, 0x0167, 0x017F];
/// Compatibility digraphs (IJ, ij, L and l with middle dot, n preceded by apostrophe) that a
/// monospaced face may omit, because two letters cannot share one cell.
const MONOSPACE_GAPS_ALLOWED: [u32; 5] = [0x0132, 0x0133, 0x013F, 0x0140, 0x0149];
const MONOSPACE: [&str; 4] = ["Geist Mono", "RichView Code", "Space Mono", "RichView Mono"];
const FEATURES: [&str; 7] = ["kern", "liga", "tnum", "lnum", "pnum", "onum", "case"];

struct Face {
    catalog: &'static str,
    family: String,
    style: String,
    weight: u32,
    payload: Vec<u8>,
    /// A lent face's unicode-range and the family that lends it.
    lent: Option<(String, String)>,
}

fn catalog_faces() -> Vec<Face> {
    let pattern = Regex::new(
        r#"^@font-face \{ font-family: "([^"]+)"; font-style: (normal|italic); font-weight: (\d+); font-display: block; (?:unicode-range: ([^;]+); )?src: url\(data:font/woff2;base64,([A-Za-z0-9+/=]+)\) format\("woff2"\); \}(?: /\* script (?:digits|signs|digits and signs) from ([^*]+) \*/)?$"#,
    )
    .unwrap();
    let mut faces = Vec::new();
    for (catalog, css) in CATALOGS {
        for line in css.lines().filter(|line| line.starts_with("@font-face")) {
            let captures = pattern
                .captures(line)
                .unwrap_or_else(|| panic!("{catalog}: malformed @font-face line"));
            let range = captures.get(4).map(|m| m.as_str().to_owned());
            let lender = captures.get(6).map(|m| m.as_str().to_owned());
            assert_eq!(
                range.is_some(),
                lender.is_some(),
                "{catalog}: a unicode-range face names its lender"
            );
            faces.push(Face {
                catalog,
                family: captures[1].to_owned(),
                style: captures[2].to_owned(),
                weight: captures[3].parse().unwrap(),
                payload: base64_decode(&captures[5]),
                lent: range.zip(lender),
            });
        }
    }
    faces
}

fn manifest() -> Value {
    serde_json::from_str(MANIFEST).unwrap()
}

fn manifest_face<'a>(manifest: &'a Value, face: &Face) -> &'a Value {
    manifest["faces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["family"] == face.family.as_str()
                && entry["style"] == face.style.as_str()
                && entry["weight"] == face.weight
                && entry["borrowed_from"].as_str() == face.lent.as_ref().map(|(_, l)| l.as_str())
        })
        .unwrap_or_else(|| {
            panic!(
                "{} {} {} is not in the manifest",
                face.family, face.style, face.weight
            )
        })
}

fn codepoints(ranges: &str) -> BTreeSet<u32> {
    ranges
        .split(',')
        .flat_map(|range| {
            let (start, end) = range.split_once('-').unwrap_or((range, range));
            u32::from_str_radix(start, 16).unwrap()..=u32::from_str_radix(end, 16).unwrap()
        })
        .collect()
}

fn hex_list(value: &Value) -> BTreeSet<u32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|code| u32::from_str_radix(code.as_str().unwrap(), 16).unwrap())
        .collect()
}

fn report_html(style: &str) -> String {
    report_html_with(&[style])
}

fn report_html_with(styles: &[&str]) -> String {
    render(
        "# Claim",
        &RenderOptions {
            styles: styles.iter().map(|style| style.to_string()).collect(),
            report_folder: Some(richview_core::document::ReportFolder {
                root: "fonts-fixture".into(),
                source: "report.rmd".into(),
                files: std::iter::once("report.rmd".into()).collect(),
            }),
            ..Default::default()
        },
    )
    .unwrap()
    .html
}

#[test]
fn math_embeds_its_pinned_face_and_notice_only_for_rendered_math() {
    for style in ["air", "sleek", "scientific", "brutalist", "hand-drawn"] {
        for output in [
            richview_core::document::Output::Report,
            richview_core::document::Output::Pdf,
        ] {
            let options = RenderOptions {
                styles: vec![style.into()],
                output,
                report_folder: Some(richview_core::document::ReportFolder {
                    root: "math-fonts-fixture".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            };
            for source in [
                "Inline $x^2 + \\alpha$.",
                "$$\\sum_{i=1}^n i = \\frac{n(n+1)}{2}$$",
                "```callout\nkind: neutral\ntitle: Method\nbody: 'Inline $x^2$.'\n```",
            ] {
                let report = render(source, &options).unwrap();
                assert!(
                    report.diagnostics.iter().all(|d| d.level != "error"),
                    "{:?}",
                    report.diagnostics
                );
                let html = report.html;
                assert!(
                    html.contains("<math"),
                    "{style}/{output:?}: {source} must render MathML"
                );
                let face = html
                    .split("font-family: \"STIX Two Math\"")
                    .nth(1)
                    .expect("MathML must carry its embedded math face");
                let payload = face
                    .split("base64,")
                    .nth(1)
                    .unwrap()
                    .split(')')
                    .next()
                    .unwrap();
                assert_eq!(
                    sha256(&base64_decode(payload)),
                    "5f8bdddc067dd82689d7247cc96e0be46fe4cec39974c649866aa391d8d5badd"
                );
                let notices: Value = serde_json::from_str(
                    html.split("<script id=\"richview-font-notices\" type=\"application/json\">")
                        .nth(1)
                        .unwrap()
                        .split("</script>")
                        .next()
                        .unwrap(),
                )
                .unwrap();
                assert!(notices["fonts"].as_array().unwrap().iter().any(|entry| {
                    entry["font"] == "STIX Two Math"
                        && entry["license"]
                            .as_str()
                            .unwrap()
                            .contains("SIL OPEN FONT LICENSE")
                }));
            }
            for source in [
                "Plain prose.",
                "Currency $42.00.",
                "`$x^2$`",
                "```text\n$x^2$\n```",
                "Broken $\\frac{1$.",
                "<math>x</math>",
            ] {
                let html = render(source, &options).unwrap().html;
                assert!(!html.contains("STIX Two Math"), "{style}: {source}");
            }
        }
    }
}

fn report_characters() -> BTreeSet<u32> {
    REPORTS
        .iter()
        .flat_map(|source| source.chars())
        .map(u32::from)
        .filter(|&code| code >= 0x20 && code != 0x7F)
        .collect()
}

#[test]
fn catalogs_bundle_the_planned_faces() {
    let found: BTreeSet<_> = catalog_faces()
        .iter()
        .map(|face| {
            (
                face.catalog,
                face.family.clone(),
                face.style.clone(),
                face.weight,
                face.lent.as_ref().map(|(_, lender)| lender.clone()),
            )
        })
        .collect();
    let expected: BTreeSet<_> = FACES
        .iter()
        .map(|&(catalog, family, style, weight)| {
            (catalog, family.to_owned(), style.to_owned(), weight, None)
        })
        .chain(
            lent_faces()
                .into_iter()
                .map(|(catalog, family, style, weight, lender)| {
                    (
                        catalog,
                        family.to_owned(),
                        style.to_owned(),
                        weight,
                        Some(lender.to_owned()),
                    )
                }),
        )
        .collect();
    assert_eq!(found, expected);
    assert_eq!(
        catalog_faces().len(),
        FACES.len() + lent_faces().len(),
        "a face is duplicated"
    );
    let manifest = manifest();
    let faces = manifest["faces"].as_array().unwrap();
    let math: Vec<_> = faces
        .iter()
        .filter(|face| face["catalog"] == "fonts-math.css")
        .collect();
    assert_eq!(math.len(), 1);
    assert_eq!(math[0]["family"], "STIX Two Math");
    assert_eq!(math[0]["postscript"], "STIXTwoMath-Regular");
    assert_eq!(math[0]["unmodified"], true);
    assert_eq!(faces.len(), FACES.len() + lent_faces().len() + math.len());
}

#[test]
fn manifest_describes_each_payload() {
    let manifest = manifest();
    for face in catalog_faces() {
        let entry = manifest_face(&manifest, &face);
        let bytes = &face.payload;
        assert_eq!(&bytes[..4], b"wOF2", "{} is not WOFF2", face.family);
        assert_eq!(
            &bytes[4..8],
            &[0, 1, 0, 0],
            "{} is not TrueType",
            face.family
        );
        let declared = u32::from_be_bytes(bytes[8..12].try_into().unwrap()) as usize;
        assert_eq!(declared, bytes.len(), "{} length", face.family);
        assert_eq!(entry["catalog"], face.catalog);
        assert_eq!(entry["woff2_bytes"], bytes.len());
        assert_eq!(
            entry["sha256"],
            sha256(bytes).as_str(),
            "{} payload",
            face.family
        );
        let source = &manifest["sources"][entry["source"].as_str().unwrap()];
        assert_eq!(source["commit"].as_str().unwrap().len(), 40);
        assert!(
            source["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file["path"] == entry["file"]
                    && file["sha256"].as_str().unwrap().len() == 64)
        );
        assert_eq!(source["license"]["sha256"].as_str().unwrap().len(), 64);
        // A Modified Version may not carry a Reserved Font Name in its own name.
        for reserved in source["license"]["reserved_font_names"].as_array().unwrap() {
            let reserved = reserved.as_str().unwrap();
            for name in [&entry["family"], &entry["name"], &entry["postscript"]] {
                assert!(
                    !name.as_str().unwrap().contains(reserved),
                    "{name} uses the Reserved Font Name {reserved}"
                );
            }
        }
    }
}

#[test]
fn report_notices_carry_the_pinned_upstream_copyright() {
    let manifest = manifest();
    let mut all = BTreeSet::new();
    // Each builtin's report carries the notices of its own faces; together they cover every face.
    for style in ["sleek", "scientific", "brutalist", "hand-drawn"] {
        let html = report_html(style);
        let (_, block) = html
            .split_once(r#"<script id="richview-font-notices" type="application/json">"#)
            .unwrap();
        let notices: Value =
            serde_json::from_str(block.split_once("</script>").unwrap().0).unwrap();
        let mut families = BTreeSet::new();
        for notice in notices["fonts"].as_array().unwrap() {
            let family = notice["font"].as_str().unwrap();
            // Notices retain the original family name; bundled Modified Versions use their own name.
            let bundled_family = match family {
                "Source Serif 4" => "RichView Serif",
                "Source Sans 3" => "RichView Sans",
                "Source Code Pro" => "RichView Code",
                "IBM Plex Mono" => "RichView Mono",
                other => other,
            };
            let entry = manifest["faces"]
                .as_array()
                .unwrap()
                .iter()
                .find(|face| face["family"] == bundled_family)
                .unwrap_or_else(|| panic!("{family} has no bundled face"));
            let license = &manifest["sources"][entry["source"].as_str().unwrap()]["license"];
            // The copyright line names the holder and any Reserved Font Name.
            assert!(
                notice["license"]
                    .as_str()
                    .unwrap()
                    .starts_with(license["copyright"].as_str().unwrap()),
                "{family} notice"
            );
            families.insert(bundled_family);
        }
        // A notice for every face the report embeds, a lent face's lender included, and for no
        // other (FEA-RP-38).
        let embedded: BTreeSet<&str> = FACES
            .iter()
            .map(|face| face.1)
            .filter(|family| html.contains(&format!("font-family: \"{family}\"")))
            .chain(
                lent_faces()
                    .into_iter()
                    .map(|face| face.4)
                    .filter(|lender| html.contains(&format!(" from {lender} */"))),
            )
            .collect();
        assert_eq!(families, embedded, "{style}");
        all.extend(embedded);
    }
    let bundled: BTreeSet<&str> = FACES.iter().map(|face| face.1).collect();
    assert_eq!(all, bundled);
}

#[test]
fn every_face_covers_the_required_characters_and_report_sources() {
    let manifest = manifest();
    let reports = report_characters();
    let mut required: BTreeSet<u32> = [0x20..=0x7E, 0xA0..=0xFF, 0x100..=0x17F]
        .into_iter()
        .flatten()
        .collect();
    required.extend(REQUIRED_EXTRA);
    required.extend(&reports);
    assert!(reports.contains(&0x2212) && reports.contains(&0x2192) && reports.contains(&0xD7));
    // A lent face draws only its family's missing script digits (the next test).
    for face in catalog_faces()
        .into_iter()
        .filter(|face| face.lent.is_none())
    {
        let entry = manifest_face(&manifest, &face);
        let covered = codepoints(entry["cmap"].as_str().unwrap());
        let gaps = hex_list(&entry["upstream_gaps"]);
        let missing: BTreeSet<u32> = required.difference(&covered).copied().collect();
        assert_eq!(
            missing, gaps,
            "{} {} {} coverage",
            face.family, face.style, face.weight
        );
        let monospace = MONOSPACE.contains(&face.family.as_str());
        assert_eq!(entry["monospace"], monospace, "{}", face.family);
        let allowed = |code: &u32| {
            GAPS_ALLOWED.contains(code) || (monospace && MONOSPACE_GAPS_ALLOWED.contains(code))
        };
        assert!(gaps.iter().all(allowed), "{} gaps", face.family);
        assert!(reports.is_subset(&covered), "{} report text", face.family);
    }
}

/// Every bundled family draws all ten superior and all ten inferior digits and the signs ⁺ ⁻ ₊ ₋,
/// its own or lent by another bundled face (STY-63), so `m²`, `10⁻⁶` and `CO₂` are never set in a
/// fallback font. Lent faces directly follow their family's face, so browsers try them first for
/// their ranges; each maps exactly its range, comes from its lender's upstream file of the family's
/// style (upright when the lender has no italic) at the family's weight, and lends a set whole or
/// not at all, so one number never mixes two designs.
#[test]
fn every_family_draws_the_script_digits_and_signs_one_design_per_set() {
    let manifest = manifest();
    let faces = catalog_faces();
    let same_face = |a: &Face, b: &Face| {
        (a.catalog, &a.family, &a.style, a.weight) == (b.catalog, &b.family, &b.style, b.weight)
    };
    let mut lent = 0;
    for (index, face) in faces.iter().enumerate() {
        let entry = manifest_face(&manifest, face);
        let name = format!("{} {} {}", face.family, face.style, face.weight);
        if let Some((range, lender)) = &face.lent {
            let own = faces[..index]
                .iter()
                .rev()
                .find(|f| f.lent.is_none())
                .unwrap();
            assert!(
                same_face(own, face)
                    && faces[..index]
                        .iter()
                        .rev()
                        .take_while(|f| f.lent.is_some())
                        .all(|f| same_face(f, face)),
                "{name}: the lent face follows its family's face"
            );
            let drawn = codepoints(entry["cmap"].as_str().unwrap());
            let css: BTreeSet<u32> = codepoints(&range.replace("U+", "").replace(", ", ","));
            assert_eq!(
                css, drawn,
                "{name}: the unicode-range is what the face maps"
            );
            // The lender's outlines: its upstream file of the family's style, instanced at the
            // family's weight. An italic takes the lender's upright when the lender bundles no
            // italic (Source Sans 3 Italic is not pinned).
            let lender_faces: Vec<&Value> = manifest["faces"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|f| f["family"] == lender.as_str() && f.get("borrowed_from").is_none())
                .collect();
            let lender_face = lender_faces
                .iter()
                .find(|f| f["style"] == face.style.as_str())
                .or_else(|| lender_faces.iter().find(|f| f["style"] == "normal"))
                .unwrap_or_else(|| panic!("{name}: {lender} has no face"));
            for key in ["source", "file"] {
                assert_eq!(entry[key], lender_face[key], "{name} {key}");
            }
            let mut location = lender_face["location"].clone();
            location["wght"] = face.weight.into();
            assert_eq!(entry["location"], location, "{name} location");
            let own_cmap = codepoints(manifest_face(&manifest, own)["cmap"].as_str().unwrap());
            for set in SCRIPT_SETS {
                let borrowed = set.iter().filter(|cp| drawn.contains(cp)).count();
                assert!(
                    borrowed == 0
                        || (borrowed == set.len() && !set.iter().all(|cp| own_cmap.contains(cp))),
                    "{name}: lends a set its family lacks, whole"
                );
            }
            lent += 1;
            continue;
        }
        let mut drawn = codepoints(entry["cmap"].as_str().unwrap());
        for next in faces[index + 1..]
            .iter()
            .take_while(|next| next.lent.is_some() && same_face(next, face))
        {
            drawn.extend(codepoints(
                manifest_face(&manifest, next)["cmap"].as_str().unwrap(),
            ));
        }
        for cp in SCRIPT_SETS.iter().copied().flatten() {
            assert!(drawn.contains(cp), "{name} does not draw U+{cp:04X}");
        }
    }
    assert_eq!(lent, lent_faces().len());
}

#[test]
fn faces_keep_kerning_figure_and_case_features() {
    let manifest = manifest();
    for entry in manifest["faces"].as_array().unwrap() {
        // A lent face is its lender's outlines alone; its family's own face carries the features.
        if entry.get("borrowed_from").is_some() {
            assert_eq!(entry["features"], serde_json::json!([]));
            continue;
        }
        let list = |key: &str| -> BTreeSet<String> {
            entry[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|tag| tag.as_str().unwrap().to_owned())
                .collect()
        };
        let (source, kept, idle) = (
            list("source_features"),
            list("features"),
            list("features_without_effect"),
        );
        for feature in FEATURES {
            assert!(
                !source.contains(feature) || kept.contains(feature) || idle.contains(feature),
                "{} {} {} lost {feature}",
                entry["family"],
                entry["style"],
                entry["weight"]
            );
        }
        if [
            "Geist",
            "RichView Serif",
            "RichView Sans",
            "Newsreader",
            "Big Shoulders",
            "Patrick Hand",
        ]
        .contains(&entry["family"].as_str().unwrap())
        {
            assert_eq!(entry["tabular_digits"], true, "{}", entry["family"]);
        }
    }
}

#[test]
fn builtin_styles_embed_every_face_of_their_families() {
    let face_rule = Regex::new(
        r#"@font-face \{ font-family: "([^"]+)"; font-style: (\w+); font-weight: (\d+); font-display: block; (unicode-range)?"#,
    )
    .unwrap();
    for (style, families) in [
        ("sleek", vec!["Geist", "Space Grotesk", "Geist Mono"]),
        ("air", vec!["Geist", "Space Grotesk", "Geist Mono"]),
        (
            "scientific",
            vec!["RichView Serif", "RichView Sans", "RichView Code"],
        ),
        (
            "brutalist",
            vec!["Newsreader", "Big Shoulders", "Space Mono"],
        ),
        (
            "hand-drawn",
            vec!["Patrick Hand", "Newsreader", "RichView Mono"],
        ),
    ] {
        let embedded: BTreeSet<(String, String, u32, bool)> = face_rule
            .captures_iter(&report_html(style))
            .map(|c| {
                (
                    c[1].to_owned(),
                    c[2].to_owned(),
                    c[3].parse().unwrap(),
                    c.get(4).is_some(),
                )
            })
            .collect();
        // A family's lent script digits come with it.
        let expected: BTreeSet<(String, String, u32, bool)> = FACES
            .iter()
            .map(|&(_, family, style, weight)| (family, style, weight, false))
            .chain(
                lent_faces()
                    .into_iter()
                    .map(|(_, family, style, weight, _)| (family, style, weight, true)),
            )
            .filter(|face| families.contains(&face.0))
            .map(|(family, style, weight, lent)| {
                (family.to_owned(), style.to_owned(), weight, lent)
            })
            .collect();
        assert_eq!(embedded, expected, "{style}");
    }
}

fn base64_decode(text: &str) -> Vec<u8> {
    let value = |byte: u8| -> u32 {
        match byte {
            b'A'..=b'Z' => u32::from(byte - b'A'),
            b'a'..=b'z' => u32::from(byte - b'a') + 26,
            b'0'..=b'9' => u32::from(byte - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("invalid base64"),
        }
    };
    let bytes = text.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut word = 0;
        for (index, &byte) in chunk.iter().enumerate() {
            word |= value(byte) << (18 - 6 * index);
        }
        out.extend_from_slice(&word.to_be_bytes()[1..chunk.len()]);
    }
    out
}

/// FIPS 180-4 SHA-256, so the payload hashes are checked without another dependency.
fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&(data.len() as u64 * 8).to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (index, word) in block.chunks(4).enumerate() {
            w[index] = u32::from_be_bytes(word.try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
        }
        for (word, add) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *word = word.wrapping_add(add);
        }
    }
    state.iter().map(|word| format!("{word:08x}")).collect()
}

#[test]
fn sha256_matches_the_standard_vectors() {
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}
