//! Maps: shipped boundaries, projected positions at known coordinates, values at the author's
//! precision, unmatched names, points, and the shared contract (FEA-CH-76, ARC-11).

use super::common;

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

fn zoomed(spec: &Value, rows: &[Value], x: [f64; 2], y: [f64; 2]) -> richview_chart::geometry::Geometry {
    use richview_chart::layout::{ChartTheme, Options, layout};
    let block = richview_chart::block::ChartBlock::from_value(spec).expect("a chart block");
    layout(
        &block,
        rows,
        &ChartTheme::default(),
        &Options {
            width: 640.0,
            zoom: Some(x),
            zoom_y: Some(y),
            ..Options::default()
        },
    )
}

/// Where a projected point is drawn, through the scales the runtime zooms and pans by.
fn drawn(scale: &richview_chart::geometry::XScale, p: [f64; 2]) -> [f64; 2] {
    let y = scale.y.expect("a map zooms along both axes");
    let along = |v: f64, domain: [f64; 2], range: [f64; 2]| {
        range[0] + (v - domain[0]) / (domain[1] - domain[0]) * (range[1] - range[0])
    };
    [along(p[0], scale.domain, scale.range), along(p[1], y.domain, y.range)]
}

#[test]
fn a_reader_s_window_is_drawn_whole_and_its_scales_say_where() {
    let (spec, rows) = fixture();
    let whole = draw(&spec, &rows);
    let all = whole.panels[0].x.clone().expect("the map reports what it shows");
    let ally = all.y.unwrap();
    // The whole map: both scales cover the boundaries, and a known place is where they say.
    assert!(all.domain[0] <= all.extent[0] && all.domain[1] >= all.extent[1]);
    assert!(ally.domain[0] <= ally.extent[0] && ally.domain[1] >= ally.extent[1]);
    let prague = Projection::NaturalEarth.project(14.42, 50.09);
    let cz = observations(&whole)
        .into_iter()
        .find(|o| o.readout.heading == "Czechia")
        .unwrap();
    let at = drawn(&all, prague);
    assert!(cz.area.unwrap().inflate(1.0).contains(at), "{at:?} in {:?}", cz.area);

    // Europe: a window a quarter of the map wide around Prague.
    let (hw, hh) = (
        (all.extent[1] - all.extent[0]) / 8.0,
        (ally.extent[1] - ally.extent[0]) / 8.0,
    );
    let g = zoomed(
        &spec,
        &rows,
        [prague[0] - hw, prague[0] + hw],
        [prague[1] - hh, prague[1] + hh],
    );
    let panel = &g.panels[0];
    let scale = panel.x.clone().unwrap();
    let y = scale.y.unwrap();
    // The window is inside what the plot shows, and the plot is the same size as before.
    assert_eq!(panel.plot, whole.panels[0].plot);
    assert_eq!(g.height, whole.height);
    assert!(scale.domain[0] <= prague[0] - hw + 1e-9 && scale.domain[1] >= prague[0] + hw - 1e-9);
    assert!(y.domain[0] <= prague[1] - hh + 1e-9 && y.domain[1] >= prague[1] + hh - 1e-9);
    // Four times closer, the same country is four times as wide, and Prague is at the middle.
    let cz_in = observations(&g)
        .into_iter()
        .find(|o| o.readout.heading == "Czechia")
        .unwrap();
    let grown = cz_in.area.unwrap().w / cz.area.unwrap().w;
    assert!((grown - 4.0).abs() < 0.01, "{grown}");
    let at = drawn(&scale, prague);
    let middle = panel.plot.center();
    assert!(
        (at[0] - middle[0]).abs() < 0.5 && (at[1] - middle[1]).abs() < 0.5,
        "{at:?}"
    );
    assert!(cz_in.area.unwrap().inflate(1.0).contains(at));
    // A country is still hit where it is drawn.
    let found = hit::at(&g, cz_in.at).expect("the anchor is inside the region");
    assert_eq!(panel.observations[found.observation].readout.heading, "Czechia");
    // Readouts offer what the window shows: Europe's countries stay, India has left the plot.
    let names: Vec<&str> = observations(&g).iter().map(|o| o.readout.heading.as_str()).collect();
    for kept in ["Czechia", "Germany", "Poland", "France", "Italy"] {
        assert!(names.contains(&kept), "{kept} in {names:?}");
    }
    assert!(!names.contains(&"India"), "{names:?}");
    // Every readout is anchored inside the plot, on a part of its country that shows.
    for o in observations(&g) {
        assert!(
            panel.plot.inflate(0.5).contains(o.at),
            "{} at {:?}",
            o.readout.heading,
            o.at
        );
        assert!(o.area.unwrap().inside(&panel.plot, 0.5), "{}", o.readout.heading);
    }
    assert!(nonfinite(&g).is_none());
}

#[test]
fn a_window_stops_at_the_map_s_edge_and_at_the_closest_zoom() {
    let (spec, rows) = fixture();
    let whole = draw(&spec, &rows);
    let all = whole.panels[0].x.clone().unwrap();
    let ally = all.y.unwrap();
    let (w, h) = (all.extent[1] - all.extent[0], ally.extent[1] - ally.extent[0]);
    // A window hanging off the top right corner is moved back onto the map.
    let g = zoomed(
        &spec,
        &rows,
        [all.extent[1] - w / 8.0, all.extent[1] + w / 8.0],
        [ally.extent[1] - h / 8.0, ally.extent[1] + h / 8.0],
    );
    let scale = g.panels[0].x.clone().unwrap();
    let y = scale.y.unwrap();
    assert!((scale.domain[1] - all.extent[1]).abs() < 1e-6, "{:?}", scale.domain);
    assert!((y.domain[1] - ally.extent[1]).abs() < 1e-6, "{:?}", y.domain);
    // A window far smaller than the closest zoom is drawn at the closest zoom, around its middle.
    let mid = [
        (all.extent[0] + all.extent[1]) / 2.0,
        (ally.extent[0] + ally.extent[1]) / 2.0,
    ];
    let g = zoomed(
        &spec,
        &rows,
        [mid[0] - w / 1e4, mid[0] + w / 1e4],
        [mid[1] - h / 1e4, mid[1] + h / 1e4],
    );
    let scale = g.panels[0].x.clone().unwrap();
    let closest = (all.domain[1] - all.domain[0]) / (scale.domain[1] - scale.domain[0]);
    assert!((closest - 24.0).abs() < 1e-6, "{closest}");
    assert!(((scale.domain[0] + scale.domain[1]) / 2.0 - mid[0]).abs() < 1e-6);
    // A window as large as the map, or larger, is the whole map again.
    let g = zoomed(
        &spec,
        &rows,
        [all.extent[0] - w, all.extent[1] + w],
        [ally.extent[0] - h, ally.extent[1] + h],
    );
    assert_eq!(g.panels[0].x, whole.panels[0].x);
    assert_eq!(g.panels[0].observations.len(), whole.panels[0].observations.len());
}
