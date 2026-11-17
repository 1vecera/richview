//! Maps: country boundaries shipped with RichView (`world`), regions coloured by the value an
//! author gives them, and points placed by latitude and longitude. A region the data does not
//! name stays plain, a name that matches no boundary is reported, and no value is invented
//! (FEA-CH-42, FEA-CH-76, ARC-11).

use super::heatmap::{Scale, nice_ceil};
use super::types::Facets;
use super::{Ctx, POINT_OPACITY, Painted, Style};
use crate::diag::Level;
use crate::geometry::{
    Anchor, Face, Fill, Item, Observation, Panel, Path, Readout, ReadoutLine, Rect, Role, Shape, Stroke, Swatch,
    Targeting, XScale, YSpan,
};
use crate::numbers;
use crate::rows::{self, Num};
use crate::world::{Projection, world};
use serde_json::json;

const RATIO: f64 = 2.0;
const KEY_W: f64 = 240.0;
const KEY_H: f64 = 8.0;
const POINT: f64 = 4.0;
const BUBBLE: f64 = 14.0;
/// How far a reader zooms in: the boundaries are drawn for a world map, and beyond this a
/// country's outline shows its few corners.
const MAX_ZOOM: f64 = 24.0;

struct Region {
    country: usize,
    row: usize,
    value: Option<f64>,
}

struct Spot {
    row: usize,
    lon: f64,
    lat: f64,
    size: Option<f64>,
    group: usize,
}

/// A point of an observation's outline in projected units.
type Ring = Vec<[f64; 2]>;

pub(super) fn layout(ctx: &mut Ctx, facets: &Facets) -> Painted {
    let block = ctx.block;
    let region_field = block.channel("region");
    let lat_field = block.channel("lat");
    let lon_field = block.channel("lon");
    let value_field = block.encoding.y.clone();
    let size_field = block.encoding.size.clone();
    let projection = block
        .style_text("projection")
        .and_then(|p| Projection::parse(&p))
        .unwrap_or(Projection::NaturalEarth);
    if region_field.is_none() && (lat_field.is_none() || lon_field.is_none()) {
        let reason = "Cannot draw this map · It needs encoding.region, or encoding.lat and encoding.lon";
        ctx.check(
            "source-error",
            Level::Error,
            "The map has neither regions nor coordinates",
            Some("Give encoding.region (country names or ISO codes), or encoding.lat and encoding.lon."),
            Some("chart.encoding".into()),
            None,
            serde_json::Value::Null,
        );
        return super::kit::failed(ctx, reason, RATIO);
    }
    let world = world();
    let groups: Vec<String> = match &block.encoding.color {
        Some(f) if lat_field.is_some() => rows::distinct(ctx.rows, f),
        _ => vec![String::new()],
    };
    ctx.set_series(&groups);
    let readout = block.readout_format(&block.y, ctx.locale);
    let mut failures: Vec<Option<String>> = vec![None; facets.len()];
    let mut regions: Vec<Vec<Region>> = Vec::new();
    let mut spots: Vec<Vec<Spot>> = Vec::new();
    let mut unmatched: Vec<String> = Vec::new();
    let mut excluded = 0usize;
    for (fi, (_, members)) in facets.iter().enumerate() {
        let mut rs: Vec<Region> = Vec::new();
        let mut ss: Vec<Spot> = Vec::new();
        for &i in members {
            let row = &ctx.rows[i];
            if let Some(field) = &region_field
                && let Some(name) = rows::text(row, field)
            {
                let Some(country) = world.find(&name) else {
                    unmatched.push(name);
                    continue;
                };
                if rs.iter().any(|r| r.country == country) {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!("Row {} repeats the region '{}'", i + 1, world.countries[country].name),
                        Some("Give each region one row."),
                        Some(Ctx::row_path(i, field)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi].get_or_insert_with(|| "Cannot draw this map · A region repeats".into());
                    continue;
                }
                let value = match value_field.as_deref().map(|f| (f, rows::num(row, f))) {
                    None | Some((_, Num::Missing)) => None,
                    Some((_, Num::Value(v))) => Some(v),
                    Some((f, Num::Underflow(t))) => {
                        ctx.underflow(i, f, &t);
                        failures[fi].get_or_insert_with(|| {
                            format!("Cannot draw this map · {f} has a value too small to represent")
                        });
                        continue;
                    }
                    Some((f, Num::Invalid(t))) => {
                        ctx.check(
                            "non-numeric",
                            Level::Error,
                            format!("Row {} has {f} '{t}', which is not a number", i + 1),
                            None,
                            Some(Ctx::row_path(i, f)),
                            None,
                            json!({"row": i}),
                        );
                        failures[fi]
                            .get_or_insert_with(|| format!("Cannot draw this map · {f} has text that is not a number"));
                        continue;
                    }
                };
                rs.push(Region { country, row: i, value });
            } else if let (Some(lat_f), Some(lon_f)) = (&lat_field, &lon_field) {
                let (lat, lon) = (rows::num(row, lat_f), rows::num(row, lon_f));
                let (Num::Value(lat), Num::Value(lon)) = (lat, lon) else {
                    excluded += 1;
                    continue;
                };
                if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                    ctx.check(
                        "source-error",
                        Level::Error,
                        format!("Row {} has the position {lat}, {lon}, outside the globe", i + 1),
                        Some("Latitude runs from -90 to 90 and longitude from -180 to 180."),
                        Some(Ctx::row_path(i, lat_f)),
                        None,
                        json!({"row": i}),
                    );
                    failures[fi].get_or_insert_with(|| "Cannot draw this map · A position is off the globe".into());
                    continue;
                }
                let size = size_field.as_ref().and_then(|f| match rows::num(row, f) {
                    Num::Value(v) if v >= 0.0 => Some(v),
                    _ => None,
                });
                let group = match &block.encoding.color {
                    Some(f) => rows::text(row, f)
                        .and_then(|g| groups.iter().position(|x| *x == g))
                        .unwrap_or(0),
                    None => 0,
                };
                ss.push(Spot {
                    row: i,
                    lon,
                    lat,
                    size,
                    group,
                });
            } else {
                // A region map's row with no region name.
                excluded += 1;
            }
        }
        regions.push(rs);
        spots.push(ss);
    }
    if failures.iter().all(Option::is_some) {
        let reason = failures.iter().flatten().next().cloned().unwrap_or_default();
        return super::kit::failed(ctx, &reason, RATIO);
    }
    if excluded > 0 {
        let what = match (&region_field, &lat_field) {
            (Some(_), None) => "a region",
            (None, _) => "a numeric latitude and longitude",
            _ => "a region or a numeric latitude and longitude",
        };
        ctx.check(
            "excluded-observations",
            Level::Warning,
            format!(
                "{} without {what} {} left out",
                super::count(excluded, "row", "rows"),
                super::verb(excluded, "is", "are")
            ),
            Some("Give every row a region name, or a latitude and longitude."),
            Some("chart.encoding".into()),
            None,
            json!({"count": excluded}),
        );
        ctx.note.push(format!(
            "{} without {what} {} not shown",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ));
    }
    if !unmatched.is_empty() {
        unmatched.dedup();
        let shown: Vec<&str> = unmatched.iter().take(8).map(String::as_str).collect();
        ctx.check(
            "unmatched-region",
            Level::Warning,
            format!(
                "{} regions match no boundary and are left out: {}",
                unmatched.len(),
                shown.join(", ")
            ),
            Some("Use country names or ISO 3166 codes (alpha-2 or alpha-3)."),
            Some("chart.encoding.region".into()),
            None,
            json!({"regions": unmatched}),
        );
        ctx.note.push(format!(
            "{} regions have no boundary and are not shown",
            unmatched.len()
        ));
    }
    // Values share one colour scale across panels.
    let top = nice_ceil(
        regions
            .iter()
            .flatten()
            .filter_map(|r| r.value)
            .fold(0.0f64, |m, v| m.max(v.abs())),
    );
    let scale = Scale::new(&ctx.style, top);
    let has_values = regions.iter().flatten().any(|r| r.value.is_some());
    let biggest = spots.iter().flatten().filter_map(|s| s.size).fold(0.0f64, f64::max);
    // Project every boundary once; Antarctica stays off the base map unless the data names it.
    let named: Vec<usize> = regions.iter().flatten().map(|r| r.country).collect();
    let drawn: Vec<usize> = (0..world.countries.len())
        .filter(|&c| world.countries[c].iso3 != "ATA" || named.contains(&c))
        .collect();
    let projected: Vec<Vec<Ring>> = drawn
        .iter()
        .map(|&c| {
            world.countries[c]
                .rings
                .iter()
                .map(|ring| ring.iter().map(|p| projection.project(p[0], p[1])).collect())
                .collect()
        })
        .collect();
    let enclosed = enclosing(&projected);
    // Each country's box in projected units, to tell what a zoom leaves in the plot.
    let boxes: Vec<[f64; 4]> = projected
        .iter()
        .map(|rings| {
            rings.iter().flatten().fold(
                [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
                |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])],
            )
        })
        .collect();
    let [mut x0, mut y0, mut x1, mut y1] = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for p in projected.iter().flatten().flatten() {
        x0 = x0.min(p[0]);
        x1 = x1.max(p[0]);
        y0 = y0.min(p[1]);
        y1 = y1.max(p[1]);
    }
    let width = ctx.opts.width;
    let lh = ctx.line_height();
    let title_room = if block.encoding.facet.is_some() { lh + 6.0 } else { 0.0 };
    let columns = super::facet_columns(facets.len(), width);
    let pw = (width - 24.0 * (columns as f64 - 1.0)) / columns as f64;
    let ratio = block.style.ratio.unwrap_or(RATIO);
    let panel_h = pw / ratio + title_room;
    let frames = super::grid_frames(facets.len(), width, 0.0, panel_h);
    let surface = Style::rgba(ctx.style.surface);
    let plain = Style::rgba(ctx.style.rule_soft);
    let mut panels = Vec::new();
    for (fi, ((name, _), frame)) in facets.iter().zip(&frames).enumerate() {
        if let Some(reason) = &failures[fi] {
            panels.push(super::failed_facet(ctx, *frame, title_room, name, reason));
            continue;
        }
        let plot = Rect::new(frame.x, frame.y + title_room, frame.w, frame.h - title_room);
        let fit = (plot.w / (x1 - x0)).min(plot.h / (y1 - y0));
        let (k, centre) = view(ctx.opts.zoom, ctx.opts.zoom_y, [x0, x1], [y0, y1], plot, fit);
        let zoomed = k > fit;
        let middle = plot.center();
        let at = |p: [f64; 2]| [middle[0] + (p[0] - centre[0]) * k, middle[1] - (p[1] - centre[1]) * k];
        let ring_path = |ring: &Ring| {
            let mut path = Path::polyline(&ring.iter().map(|p| at(*p)).collect::<Vec<_>>());
            path.close();
            path
        };
        // Zoomed in, only what reaches into the plot is drawn and read out (FEA-CH-49): what a
        // country or a point covers of the plot, or `None` when the zoom has moved it out.
        let within = |r: Rect| -> Option<Rect> {
            if !zoomed {
                return Some(r);
            }
            let (x, y) = (r.x.max(plot.x), r.y.max(plot.y));
            let (w, h) = (r.right().min(plot.right()) - x, r.bottom().min(plot.bottom()) - y);
            (w > 0.0 && h > 0.0).then(|| Rect::new(x, y, w, h))
        };
        let seen = |pi: usize| {
            let [bx0, by0, bx1, by1] = boxes[pi];
            let (a, b) = (at([bx0, by1]), at([bx1, by0]));
            within(Rect::new(a[0], a[1], b[0] - a[0], b[1] - a[1])).is_some()
        };
        let mut items: Vec<Item> = Vec::new();
        let mut observations: Vec<Observation> = Vec::new();
        let mine = &regions[fi];
        let valued = |pi: usize| mine.iter().find(|r| r.country == drawn[pi] && r.value.is_some());
        // Every outline of a country, the box around them and an anchor inside the largest ring,
        // so a region is reached where it is drawn.
        let shape = |pi: usize| -> Option<(Vec<Path>, Rect, [f64; 2])> {
            if !seen(pi) {
                return None;
            }
            let paths: Vec<Path> = projected[pi].iter().map(&ring_path).collect();
            let whole = paths.iter().filter_map(|p| p.bounds()).reduce(|a, b| a.union(&b))?;
            let area = within(whole)?;
            let largest = projected[pi]
                .iter()
                .max_by(|a, b| ring_area(a).total_cmp(&ring_area(b)))
                .map(|ring| ring.iter().map(|p| at(*p)).collect::<Vec<_>>());
            let anchor = largest
                .as_deref()
                .and_then(interior_point)
                .unwrap_or_else(|| whole.center());
            // A country cut by the plot's edge is read out from the part that shows.
            let anchor = if plot.contains(anchor) || !zoomed {
                anchor
            } else {
                area.center()
            };
            Some((paths, area, anchor))
        };
        // Plain countries first, except those enclosed by another (Lesotho inside South Africa).
        for (pi, _) in drawn.iter().enumerate() {
            if enclosed[pi].is_some() || valued(pi).is_some() || !seen(pi) {
                continue;
            }
            for ring in &projected[pi] {
                items.push(Item::Area {
                    path: ring_path(ring),
                    fill: Fill::solid(plain),
                    role: Role::Band,
                    series: None,
                    obs: None,
                });
            }
        }
        // Regions with a value, on top of the plain ones; enclosed regions last, so they paint
        // and are hit over the country around them.
        let mut order: Vec<(usize, &Region)> = mine
            .iter()
            .filter(|r| r.value.is_some())
            .filter_map(|r| drawn.iter().position(|&c| c == r.country).map(|pi| (pi, r)))
            .collect();
        order.sort_by_key(|(pi, _)| enclosed[*pi].is_some());
        for (pi, region) in order {
            let Some(value) = region.value else { continue };
            let color = Style::rgba(scale.color(value));
            let index = observations.len();
            let Some((paths, area, anchor)) = shape(pi) else {
                continue;
            };
            for path in paths {
                items.push(Item::Area {
                    path,
                    fill: Fill::solid(color),
                    role: Role::Mark,
                    series: None,
                    obs: Some(index),
                });
            }
            let country = &world.countries[region.country];
            observations.push(Observation {
                row: Some(region.row),
                dots: vec![],
                series: None,
                at: anchor,
                wedge: None,
                area: Some(area),
                radius: 0.0,
                x: anchor[0],
                readout: Readout {
                    heading: country.name.clone(),
                    lines: vec![ReadoutLine {
                        name: block
                            .y
                            .label
                            .clone()
                            .or_else(|| value_field.clone())
                            .unwrap_or_default(),
                        value: numbers::value(value, &readout),
                        series: None,
                        names_series: false,
                        derived: false,
                    }],
                }
                .into(),
            });
        }
        // Enclosed countries without a value stay plain over a valued host, and a pointer on
        // them reads their own name rather than the host's value.
        for (pi, host) in enclosed.iter().enumerate() {
            let Some(host) = *host else { continue };
            if valued(pi).is_some() {
                continue;
            }
            let covered = valued(host).is_some();
            let index = observations.len();
            let Some((paths, area, anchor)) = shape(pi) else {
                continue;
            };
            for path in paths {
                items.push(Item::Area {
                    path,
                    fill: Fill::solid(plain),
                    role: if covered { Role::Mark } else { Role::Band },
                    series: None,
                    obs: covered.then_some(index),
                });
            }
            if covered {
                observations.push(Observation {
                    row: None,
                    dots: vec![],
                    series: None,
                    at: anchor,
                    wedge: None,
                    area: Some(area),
                    radius: 0.0,
                    x: anchor[0],
                    readout: Readout {
                        heading: world.countries[drawn[pi]].name.clone(),
                        lines: vec![ReadoutLine {
                            name: block
                                .y
                                .label
                                .clone()
                                .or_else(|| value_field.clone())
                                .unwrap_or_default(),
                            value: "No value".into(),
                            series: None,
                            names_series: false,
                            derived: false,
                        }],
                    }
                    .into(),
                });
            }
        }
        // Borders over the fills, in the surface colour.
        for (pi, _) in drawn.iter().enumerate() {
            if !seen(pi) {
                continue;
            }
            for ring in &projected[pi] {
                items.push(Item::Line {
                    path: ring_path(ring),
                    stroke: Stroke::solid(surface, 0.6),
                    // Zoomed in, borders leave the plot with their countries and are cut there.
                    role: if zoomed { Role::Band } else { Role::Grid },
                    series: None,
                    obs: None,
                });
            }
        }
        for s in &spots[fi] {
            if ctx.is_hidden(s.group) {
                continue;
            }
            let center = at(projection.project(s.lon, s.lat));
            let radius = match s.size {
                Some(v) if biggest > 0.0 => BUBBLE * (v / biggest).sqrt(),
                _ => POINT,
            };
            if zoomed && !plot.contains(center) {
                continue;
            }
            let color = Style::rgba(ctx.series_color(s.group));
            let shape = ctx.series[s.group].shape.unwrap_or(Shape::Circle);
            let index = observations.len();
            items.push(Item::Point {
                center,
                radius,
                shape,
                fill: Some(Fill::solid(color.with_alpha(POINT_OPACITY))),
                stroke: Some(Stroke::solid(color, 1.0)),
                role: Role::Mark,
                series: Some(s.group),
                obs: Some(index),
            });
            let mut lines = vec![
                ReadoutLine {
                    name: "Latitude".into(),
                    value: numbers::grouped(s.lat, 2, ctx.locale, false),
                    series: None,
                    names_series: false,
                    derived: false,
                },
                ReadoutLine {
                    name: "Longitude".into(),
                    value: numbers::grouped(s.lon, 2, ctx.locale, false),
                    series: None,
                    names_series: false,
                    derived: false,
                },
            ];
            if let (Some(v), Some(f)) = (s.size, &size_field) {
                lines.insert(
                    0,
                    ReadoutLine {
                        name: f.clone(),
                        value: numbers::value(v, &readout),
                        series: Some(s.group),
                        names_series: false,
                        derived: false,
                    },
                );
            }
            let heading = block
                .encoding
                .label
                .as_ref()
                .and_then(|f| rows::text(&ctx.rows[s.row], f))
                .unwrap_or_else(|| format!("Row {}", s.row + 1));
            observations.push(Observation {
                row: Some(s.row),
                dots: vec![],
                series: Some(s.group),
                at: center,
                wedge: None,
                area: None,
                radius,
                x: center[0],
                readout: Readout { heading, lines }.into(),
            });
        }
        let mut panel = Panel {
            facet: None,
            frame: *frame,
            plot,
            items,
            observations,
            targeting: Targeting::Contain,
            // What the plot shows of the map, in projected units, for the reader's zoom and pan.
            x: Some(XScale {
                domain: [centre[0] - plot.w / (2.0 * k), centre[0] + plot.w / (2.0 * k)],
                extent: [x0, x1],
                range: [plot.x, plot.right()],
                calendar: false,
                least: None,
                y: Some(YSpan {
                    domain: [centre[1] - plot.h / (2.0 * k), centre[1] + plot.h / (2.0 * k)],
                    extent: [y0, y1],
                    range: [plot.bottom(), plot.y],
                }),
            }),
            failure: None,
        };
        super::panel_title(ctx, &mut panel, name);
        panels.push(panel);
    }
    let mut height = super::grid_height(facets.len(), columns, panel_h, super::FACET_GAP_Y);
    let mut figure_items = Vec::new();
    if has_values {
        // A key that decodes the colour: the ramp from zero to the scale's top.
        let y = height + 12.0;
        let key_w = KEY_W.min(width);
        let steps = 32;
        for s in 0..steps {
            let t = s as f64 / steps as f64;
            figure_items.push(Item::Rect {
                rect: Rect::new(t * key_w, y, key_w / steps as f64 + 0.3, KEY_H),
                fill: Fill::solid(Style::rgba(scale.color(t * top))),
                stroke: None,
                radius: [0.0; 4],
                role: Role::Key,
                series: None,
                obs: None,
            });
        }
        let format = block.label_format(&block.y, ctx.locale);
        let baseline = y + KEY_H + 4.0 + ctx.style.font.vertical().0 * ctx.size();
        for (v, x, anchor) in [(0.0, 0.0, Anchor::Start), (top, key_w, Anchor::End)] {
            figure_items.push(Item::Text(ctx.text(
                &numbers::value(v, &format),
                x,
                baseline,
                anchor,
                Face::Figures,
                Role::Key,
                ctx.style.ink,
            )));
        }
        height = baseline + ctx.style.font.vertical().1 * ctx.size() + 4.0;
    }
    let shown: usize = regions.iter().flatten().filter(|r| r.value.is_some()).count();
    let points: usize = spots.iter().map(Vec::len).sum();
    ctx.summary = match (shown, points) {
        (0, 0) => "A map with nothing placed on it.".into(),
        (r, 0) => format!("Map with {r} regions coloured by value."),
        (0, p) => format!("Map with {p} points."),
        (r, p) => format!("Map with {r} regions coloured by value and {p} points."),
    };
    ctx.note.push("boundaries: Natural Earth, public domain".into());
    let legend = (groups.len() > 1)
        .then(|| super::kit::legend(ctx, Swatch::Dot))
        .flatten();
    Painted {
        height,
        panels,
        figure_items,
        legend,
        failure: None,
    }
}

/// The scale and the projected point at the plot's middle for the reader's window (FEA-CH-49).
/// The window is fitted whole into the plot, so its shape need not match; the scale stays between
/// the whole map's `fit` and [`MAX_ZOOM`] times it. Along an axis where the plot shows more than
/// the map, the map stays centred; elsewhere the window stops at the map's edge, so zooming and
/// panning never bring in more empty paper than the whole map has.
fn view(
    zoom_x: Option<[f64; 2]>,
    zoom_y: Option<[f64; 2]>,
    ex: [f64; 2],
    ey: [f64; 2],
    plot: Rect,
    fit: f64,
) -> (f64, [f64; 2]) {
    let whole = [(ex[0] + ex[1]) / 2.0, (ey[0] + ey[1]) / 2.0];
    let valid = |w: &[f64; 2]| w[0].is_finite() && w[1].is_finite() && w[0] < w[1];
    let (wx, wy) = (zoom_x.filter(valid), zoom_y.filter(valid));
    if (wx.is_none() && wy.is_none()) || !(fit.is_finite() && fit > 0.0) {
        return (fit, whole);
    }
    let (wx, wy) = (wx.unwrap_or(ex), wy.unwrap_or(ey));
    let k = (plot.w / (wx[1] - wx[0]))
        .min(plot.h / (wy[1] - wy[0]))
        .clamp(fit, fit * MAX_ZOOM);
    let along = |want: [f64; 2], e: [f64; 2], shown: f64| {
        if shown >= e[1] - e[0] {
            (e[0] + e[1]) / 2.0
        } else {
            ((want[0] + want[1]) / 2.0).clamp(e[0] + shown / 2.0, e[1] - shown / 2.0)
        }
    };
    (k, [along(wx, ex, plot.w / k), along(wy, ey, plot.h / k)])
}

/// Twice the unsigned area of a ring.
fn ring_area(ring: &[[f64; 2]]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| ring[i][0] * ring[(i + 1) % n][1] - ring[(i + 1) % n][0] * ring[i][1])
        .sum::<f64>()
        .abs()
}

/// A point inside the ring: the middle of the widest stretch the ring covers on the horizontal
/// line through its vertical middle.
fn interior_point(ring: &[[f64; 2]]) -> Option<[f64; 2]> {
    let (lo, hi) = ring.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
        (lo.min(p[1]), hi.max(p[1]))
    });
    let mut best: Option<[f64; 2]> = None;
    let mut widest = 0.0;
    for step in [0.5, 0.4, 0.6, 0.3, 0.7] {
        let y = lo + (hi - lo) * step;
        let mut xs: Vec<f64> = Vec::new();
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if (a[1] > y) != (b[1] > y) {
                xs.push(a[0] + (y - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        xs.sort_by(f64::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            if pair[1] - pair[0] > widest {
                widest = pair[1] - pair[0];
                best = Some([(pair[0] + pair[1]) / 2.0, y]);
            }
        }
    }
    best
}

/// Whether `p` lies inside `ring`.
fn inside_ring(ring: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut inside = false;
    let mut j = ring.len().wrapping_sub(1);
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// For each country, the country whose outer ring surrounds it, if any: the boundaries pack no
/// holes, so Lesotho lies inside South Africa's ring. The smallest ring that holds a point inside
/// the country's largest ring and is larger than it wins.
fn enclosing(projected: &[Vec<Ring>]) -> Vec<Option<usize>> {
    projected
        .iter()
        .enumerate()
        .map(|(i, rings)| {
            let own = rings.iter().max_by(|a, b| ring_area(a).total_cmp(&ring_area(b)))?;
            let size = ring_area(own);
            let p = interior_point(own)?;
            let mut best: Option<(usize, f64)> = None;
            for (j, other) in projected.iter().enumerate() {
                if j == i {
                    continue;
                }
                for ring in other {
                    let area = ring_area(ring);
                    if area > size && best.is_none_or(|(_, a)| area < a) && inside_ring(ring, p) {
                        best = Some((j, area));
                    }
                }
            }
            best.map(|(j, _)| j)
        })
        .collect()
}
