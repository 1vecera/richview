//! Maps: shipped boundaries, projected positions at known coordinates, values at the author's
//! precision, unmatched names, points, and the shared contract (FEA-CH-76, ARC-11).

mod common;

use common::*;
use richview_chart::geometry::Item;
use richview_chart::hit;
use richview_chart::world::{Projection, world};
use serde_json::{Value, json};

fn fixture() -> (Value, Vec<Value>) {
    let v: Value = serde_json::from_str(include_str!("fixtures/types/map.json")).unwrap();
    (v["spec"].clone(), v["rows"].as_array().unwrap().clone())
}

#[test]
fn the_golden_choropleth_meets_the_shared_contract() {
    let (spec, rows) = fixture();
    contract(&spec, &rows);
}

#[test]
fn values_equal_the_source_and_names_match_boundaries() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    assert_eq!(obs.len(), rows.len());
    let cz = obs.iter().find(|o| o.readout.heading == "Czechia").unwrap();
    assert_eq!(line(cz, "Households with broadband"), Some("91%"));
    // An alias and a long name reach the same boundary.
    assert_eq!(world().find("USA"), world().find("United States of America"));
}

#[test]
fn a_region_is_hit_inside_its_outline_and_not_in_its_bounding_box() {
    let (spec, rows) = fixture();
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let norway = obs.iter().find(|o| o.readout.heading == "Norway").unwrap();
    let found = hit::at(&g, norway.at).expect("the anchor is inside the region");
    assert_eq!(
        g.panels[found.panel].observations[found.observation].readout.heading,
        "Norway"
    );
    // The corners of a region's box are mostly outside it: Norway's top-left corner is sea.
    let area = norway.area.unwrap();
    let corner = [area.x + 1.0, area.y + 1.0];
    let at_corner = hit::at(&g, corner).map(|t| g.panels[t.panel].observations[t.observation].readout.heading.clone());
    assert_ne!(at_corner.as_deref(), Some("Norway"));
}

#[test]
fn projected_positions_match_known_coordinates() {
    for p in [
        Projection::NaturalEarth,
        Projection::EqualEarth,
        Projection::Mercator,
        Projection::Equirectangular,
    ] {
        let prague = p.project(14.42, 50.09);
        let sydney = p.project(151.21, -33.87);
        assert!(prague[0] > 0.0 && prague[1] > 0.0 && sydney[0] > prague[0] && sydney[1] < 0.0);
    }
    // Equirectangular is the identity in radians.
    let [x, y] = Projection::Equirectangular.project(90.0, 45.0);
    assert!((x - std::f64::consts::FRAC_PI_2).abs() < 1e-12 && (y - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
    // Points land where the projection says: Prague is above the equator line of the plot.
    let spec = json!({"type":"map","title":"T","encoding":{"lat":"la","lon":"lo"}});
    let g = draw(
        &spec,
        &[json!({"la":50.09,"lo":14.42}), json!({"la":-33.87,"lo":151.21})],
    );
    let obs = observations(&g);
    assert!(obs[0].at[1] < obs[1].at[1], "north is up");
    assert!(obs[0].at[0] < obs[1].at[0], "east is right");
}

#[test]
fn unmatched_names_are_reported_not_guessed() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"country":"Atlantis","share":50}));
    let g = draw(&spec, &rows);
    let c = check(&g, "unmatched-region").unwrap();
    assert!(c.message.contains("Atlantis"));
    assert_eq!(observations(&g).len(), rows.len() - 1);
}

#[test]
fn a_repeated_region_and_bad_values_fail_the_panel() {
    let (spec, mut rows) = fixture();
    rows.push(json!({"country":"Czechia","share":1}));
    assert!(draw(&spec, &rows).failure.is_some());
    let g = draw(&spec, &[json!({"country":"Czechia","share":"a lot"})]);
    assert!(g.failure.is_some() && check(&g, "non-numeric").is_some());
    let g = draw(
        &json!({"type":"map","title":"T","encoding":{"y":"v"}}),
        &[json!({"v":1})],
    );
    assert!(g.failure.is_some());
}

#[test]
fn points_have_radius_targets_and_sizes() {
    let spec = json!({"type":"map","title":"Offices","encoding":{"lat":"la","lon":"lo","size":"staff","label":"city","color":"kind"}});
    let rows = vec![
        json!({"la":50.09,"lo":14.42,"staff":120,"city":"Prague","kind":"hq"}),
        json!({"la":40.71,"lo":-74.0,"staff":40,"city":"New York","kind":"branch"}),
        json!({"la":-33.87,"lo":151.21,"staff":10,"city":"Sydney","kind":"branch"}),
    ];
    contract(&spec, &rows);
    let g = draw(&spec, &rows);
    assert_eq!(g.series.len(), 2);
    let obs = observations(&g);
    assert!(obs[0].radius > obs[2].radius, "the largest bubble is the largest");
    let found = hit::at(&g, obs[1].at).unwrap();
    assert_eq!(
        g.panels[found.panel].observations[found.observation].readout.heading,
        "New York"
    );
}

#[test]
fn edge_fixtures_draw_or_say_why() {
    let (spec, _) = fixture();
    stays_visible(&spec, &[]);
    contract(&spec, &[json!({"country":"Czechia","share":1})]);
    contract(
        &spec,
        &[
            json!({"country":"Czechia","share":-5}),
            json!({"country":"Norway","share":5}),
        ],
    );
    let g = draw(
        &spec,
        &[
            json!({"country":"Czechia","share":1e300}),
            json!({"country":"Norway","share":1e-300}),
        ],
    );
    assert!(nonfinite(&g).is_none());
    let off = draw(
        &json!({"type":"map","title":"T","encoding":{"lat":"la","lon":"lo"}}),
        &[json!({"la":95,"lo":0})],
    );
    assert!(off.failure.is_some());
    for projection in ["mercator", "equal-earth", "equirectangular"] {
        let mut s = spec.clone();
        s["style"] = json!({"projection": projection});
        let (_, rows) = fixture();
        contract(&s, &rows);
    }
}

#[test]
fn writes_the_gallery_when_asked() {
    let (spec, rows) = fixture();
    gallery("map", &spec, &rows);
}

#[test]
fn an_enclosed_country_stays_plain_and_is_hit_over_the_country_around_it() {
    let (spec, _) = fixture();
    let rows = vec![
        json!({"country":"South Africa","share":50}),
        json!({"country":"Norway","share":10}),
    ];
    let g = draw(&spec, &rows);
    let panel = &g.panels[0];
    let index = |name: &str| panel.observations.iter().position(|o| o.readout.heading == name);
    let (sa, lesotho) = (
        index("South Africa").unwrap(),
        index("Lesotho").expect("Lesotho is listed"),
    );
    assert_eq!(
        line(&panel.observations[lesotho], "Households with broadband"),
        Some("No value")
    );
    let found = hit::at(&g, panel.observations[lesotho].at).unwrap();
    assert_eq!(panel.observations[found.observation].readout.heading, "Lesotho");
    let found = hit::at(&g, panel.observations[sa].at).unwrap();
    assert_eq!(panel.observations[found.observation].readout.heading, "South Africa");
    // Lesotho paints after South Africa, so South Africa's colour does not cover it.
    let first = |idx: usize| {
        panel
            .items
            .iter()
            .position(|i| matches!(i, Item::Area { obs: Some(k), .. } if *k == idx))
            .unwrap()
    };
    assert!(first(sa) < first(lesotho));
}

#[test]
fn an_enclosed_country_with_its_own_value_reads_its_own_value() {
    let (spec, _) = fixture();
    let rows = vec![
        json!({"country":"South Africa","share":50}),
        json!({"country":"Lesotho","share":20}),
    ];
    let g = draw(&spec, &rows);
    let obs = observations(&g);
    let lesotho = obs.iter().find(|o| o.readout.heading == "Lesotho").unwrap();
    assert_eq!(line(lesotho, "Households with broadband"), Some("20%"));
    let found = hit::at(&g, lesotho.at).unwrap();
    assert_eq!(
        g.panels[found.panel].observations[found.observation].readout.heading,
        "Lesotho"
    );
}

#[test]
fn rows_without_a_position_or_a_region_are_counted_not_silently_skipped() {
    let spec = json!({"type":"map","title":"T","encoding":{"lat":"la","lon":"lo"}});
    let rows = vec![
        json!({"la":50.09,"lo":14.42}),
        json!({"la":null,"lo":14.42}),
        json!({"la":"north","lo":14.42}),
    ];
    let g = draw(&spec, &rows);
    let c = check(&g, "excluded-observations").expect("reported");
    assert!(c.message.starts_with("2 rows"), "{}", c.message);
    assert_eq!(observations(&g).len(), 1);
    assert!(g.note.as_deref().is_some_and(|n| n.contains("2 rows")));
    let (spec, mut rows) = fixture();
    rows.push(json!({"country":"","share":3}));
    let g = draw(&spec, &rows);
    assert!(
        check(&g, "excluded-observations")
            .unwrap()
            .message
            .starts_with("1 row ")
    );
}
