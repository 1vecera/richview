//! What every chart type beyond the original nine ships with (FEA-CH-75): a golden fixture in
//! `tests/fixtures/types/<type>.json` that meets the shared contract at 320, 390 and 1440 px, an
//! entrance, a ratio, and a reason on screen when it has nothing to draw. The nine older types
//! keep their own suites.

use super::common;

use common::*;
use richview_chart::block::ChartType;
use serde_json::Value;
use std::fs;

const LEGACY: usize = 9;

fn fixture(name: &str) -> Option<(Value, Vec<Value>)> {
    let path = format!("{}/tests/fixtures/types/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).expect("fixture json");
    let rows = v["rows"].as_array().expect("rows").clone();
    Some((v["spec"].clone(), rows))
}

#[test]
fn every_new_type_has_a_golden_fixture_that_meets_the_contract() {
    for kind in ChartType::all().skip(LEGACY) {
        let name = kind.name();
        let (spec, rows) =
            fixture(name).unwrap_or_else(|| panic!("{name}: tests/fixtures/types/{name}.json is missing"));
        assert_eq!(spec["type"], name, "{name}: the fixture draws its own type");
        contract(&spec, &rows);
        stays_visible(&spec, &[]);
        let g = draw(&spec, &rows);
        assert!(
            g.entrance.is_some(),
            "{name} has an entrance when the style allows motion"
        );
        assert!(kind.default_ratio(false) > 0.0);
    }
}

#[test]
fn every_fixture_names_a_type_the_engine_draws() {
    let dir = format!("{}/tests/fixtures/types", env!("CARGO_MANIFEST_DIR"));
    for entry in fs::read_dir(dir).expect("fixtures") {
        let path = entry.expect("entry").path();
        let name = path.file_stem().expect("stem").to_string_lossy().into_owned();
        assert!(ChartType::parse(&name).is_some(), "{name}.json names no chart type");
    }
}

/// The share image draws a story's lead chart at 1200 by 630 without interaction chrome
/// (INT-07, ARC-69): every type draws at that size, clean and inside its figure.
#[test]
fn every_new_type_draws_clean_at_the_share_image_size() {
    for kind in ChartType::all().skip(LEGACY) {
        let name = kind.name();
        let (mut spec, rows) = fixture(name).unwrap_or_else(|| panic!("{name}: fixture is missing"));
        spec["style"]["ratio"] = serde_json::json!(1200.0 / 630.0);
        let g = draw_at(&spec, &rows, 1200.0);
        assert!(g.failure.is_none(), "{name} at 1200 x 630 failed: {:?}", g.failure);
        assert!(nonfinite(&g).is_none(), "{name} has a non-finite coordinate");
        let figure = richview_chart::geometry::Rect::new(0.0, 0.0, g.width, g.height);
        for item in g.items() {
            if let Some(b) = item.bounds() {
                assert!(b.inside(&figure, 1.0), "{name}: {item:?} paints outside {figure:?}");
            }
        }
        for check_kind in ["clipping", "label-collision"] {
            assert!(
                check(&g, check_kind).is_none(),
                "{name} at 1200 x 630: {check_kind}: {:?}",
                check(&g, check_kind)
            );
        }
    }
}
