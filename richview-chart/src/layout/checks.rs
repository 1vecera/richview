//! Checks the engine runs on its own geometry model (FEA-CH-58, FEA-CH-60, FEA-CH-62,
//! FEA-CH-69). Text bounds come from the bundled fonts' metrics, so these are final.

use super::{Ctx, Painted};
use crate::color::{self, Vision};
use crate::diag::Level;
use crate::geometry::{Item, Rect, Role};
use serde_json::{Value, json};

/// What keeps `colors` (in palette slots `slots`) from passing the palette rule on `surface`:
/// pairs that are too alike or merge under colour-vision simulation, and slots below 3:1.
pub(super) fn palette_problems(colors: &[color::Oklch], slots: &[usize], surface: color::Oklch) -> Vec<String> {
    let mut problems = Vec::new();
    let n = colors.len();
    for i in 0..n {
        for j in i + 1..n {
            let neighbours = j == i + 1;
            if !(j < 4 || neighbours) {
                continue;
            }
            let (a, b) = (colors[i], colors[j]);
            if color::delta_e(a, b) < color::MIN_DELTA_E {
                problems.push(format!("slots {} and {} are too alike", slots[i] + 1, slots[j] + 1));
                continue;
            }
            if let Some(v) = Vision::ALL
                .into_iter()
                .find(|v| color::delta_e(v.simulate(a), v.simulate(b)) < color::MIN_DELTA_E_CVD)
            {
                problems.push(format!(
                    "slots {} and {} merge under {} vision",
                    slots[i] + 1,
                    slots[j] + 1,
                    v.name()
                ));
            }
        }
    }
    for (k, c) in colors.iter().take(4).enumerate() {
        if color::contrast(*c, surface) < color::MIN_CONTRAST {
            problems.push(format!("slot {} is below 3:1 on the chart surface", slots[k] + 1));
        }
    }
    problems
}

/// Every pair among the first four palette colours, and each neighbouring pair after them,
/// must differ by ΔE 15 (8 under colour-vision simulation); the first four reach 3:1 on the
/// surface (STY-22). A failing palette is never altered: it warns and switches decals on.
pub(super) fn palette(ctx: &mut Ctx) {
    let visible: Vec<usize> = ctx.series.iter().filter(|s| !s.hidden).map(|s| s.slot).collect();
    if visible.len() < 2 {
        return;
    }
    let colors: Vec<color::Oklch> = visible
        .iter()
        .map(|slot| ctx.style.palette[slot % ctx.style.palette.len()])
        .collect();
    let problems = palette_problems(&colors, &visible, ctx.style.surface);
    let greyscale = ctx.style.palette.iter().all(|c| c.c < 0.02);
    if !problems.is_empty() || greyscale {
        super::decals_on(&mut ctx.series);
    }
    if !problems.is_empty() {
        ctx.check(
            "palette-rule",
            Level::Warning,
            format!(
                "The palette fails its rule on this chart: {}; patterns tell the series apart",
                problems.join("; ")
            ),
            Some("Pick a palette whose colours differ more, or keep the patterns."),
            Some("chart.style.palette".into()),
            None,
            json!({"problems": problems}),
        );
    }
    if ctx.series.len() > ctx.style.palette.len() {
        ctx.check(
            "series-differentiation",
            Level::Warning,
            format!(
                "{} series share {} palette colours, so colours repeat",
                ctx.series.len(),
                ctx.style.palette.len()
            ),
            Some("Split the chart into facets with encoding.facet."),
            Some("chart.encoding.color".into()),
            None,
            json!({"series": ctx.series.len(), "colors": ctx.style.palette.len()}),
        );
    }
}

/// Findings listed one by one up to this many; beyond it one summary finding names the first.
const LISTED: usize = 8;

/// Geometry checks over the finished model.
pub(super) fn run(ctx: &mut Ctx, out: &mut Painted) {
    let width = ctx.opts.width;
    let chart = Rect::new(0.0, 0.0, width, out.height);
    for panel in &out.panels {
        if panel.failure.is_some() {
            continue;
        }
        let texts: Vec<&crate::geometry::Text> = panel
            .items
            .iter()
            .filter_map(|i| if let Item::Text(t) = i { Some(t) } else { None })
            .collect();
        // Clipped text: one finding per text when few, one summary when many.
        let clipped: Vec<&&crate::geometry::Text> = texts
            .iter()
            .filter(|t| t.rotate == 0.0 && !t.bounds.inside(&chart, 0.5))
            .collect();
        if clipped.len() <= LISTED {
            for t in &clipped {
                ctx.check(
                    "clipping",
                    Level::Warning,
                    format!("'{}' extends beyond the chart", t.text),
                    Some("Shorten the text or give the chart more room."),
                    None,
                    Some(t.bounds),
                    Value::Null,
                );
            }
        } else {
            let names: Vec<&str> = clipped.iter().take(LISTED).map(|t| t.text.as_str()).collect();
            ctx.check(
                "clipping",
                Level::Warning,
                format!(
                    "{} texts extend beyond the chart, among them '{}'",
                    clipped.len(),
                    names.join("', '")
                ),
                Some("Shorten the text or give the chart more room."),
                None,
                Some(
                    clipped
                        .iter()
                        .skip(1)
                        .fold(clipped[0].bounds, |r, t| r.union(&t.bounds)),
                ),
                json!({"count": clipped.len(), "texts": names}),
            );
        }
        // Overlapping text, found by a sweep over boxes sorted by their left edge.
        let mut level: Vec<&&crate::geometry::Text> = texts.iter().filter(|t| t.rotate == 0.0).collect();
        level.sort_by(|a, b| a.bounds.x.total_cmp(&b.bounds.x));
        let mut pairs: Vec<(&str, &str, Rect)> = Vec::new();
        let mut collisions = 0usize;
        let mut involved = vec![false; level.len()];
        for (i, a) in level.iter().enumerate() {
            for (j, b) in level.iter().enumerate().skip(i + 1) {
                if b.bounds.x >= a.bounds.right() - 1.0 {
                    break;
                }
                if a.bounds.overlaps(&b.bounds, 1.0) {
                    collisions += 1;
                    involved[i] = true;
                    involved[j] = true;
                    if pairs.len() < LISTED {
                        pairs.push((&a.text, &b.text, a.bounds.union(&b.bounds)));
                    }
                }
            }
        }
        if collisions == 1 {
            let (a, b, region) = pairs[0];
            ctx.check(
                "label-collision",
                Level::Warning,
                format!("'{a}' overlaps '{b}'"),
                Some("Shorten labels or widen the chart."),
                None,
                Some(region),
                json!({"texts": [a, b]}),
            );
        } else if collisions > 1 {
            let listed: Vec<String> = pairs.iter().map(|(a, b, _)| format!("'{a}' and '{b}'")).collect();
            ctx.check(
                "label-collision",
                Level::Warning,
                format!(
                    "{} labels overlap other labels, among them {}",
                    involved.iter().filter(|x| **x).count(),
                    listed.join(", ")
                ),
                Some("Shorten labels or widen the chart."),
                None,
                Some(pairs.iter().skip(1).fold(pairs[0].2, |r, p| r.union(&p.2))),
                json!({"count": collisions, "pairs": pairs.iter().map(|(a, b, _)| [a, b]).collect::<Vec<_>>()}),
            );
        }
        let marks = panel.items.iter().filter(|i| i.role() == Role::Mark).count();
        if marks == 0 && panel.observations.is_empty() {
            ctx.check(
                "empty-region",
                Level::Warning,
                "The plot draws nothing",
                Some("Check the data and hidden series."),
                None,
                Some(panel.plot),
                Value::Null,
            );
        }
    }
    // Chart text contrast against the chart surface (FEA-CH-60).
    for (name, ink) in [("chart-ink", ctx.style.ink), ("ink-soft", ctx.style.ink_soft)] {
        let ratio = color::contrast(ink, ctx.style.surface);
        if ratio < 4.5 {
            ctx.check(
                "contrast",
                Level::Warning,
                format!("Chart text in {name} reaches {ratio:.2}:1 on the chart surface, below 4.5:1"),
                Some("Darken or lighten the ink token."),
                Some(format!("styles.tokens.{name}")),
                None,
                json!({"ratio": ratio}),
            );
        }
    }
    ctx.checks.extend(dropped_checks(&ctx.dropped));
}

/// The same drop diagnostics before rendering and after canonical readout replay.
pub(crate) fn dropped_checks(dropped: &[crate::geometry::Dropped]) -> Vec<crate::geometry::Check> {
    let check = |message, region, details| crate::geometry::Check {
        kind: "label-dropped".into(),
        level: Level::Info,
        message,
        fix: None,
        path: None,
        region,
        facet: None,
        details,
    };
    let reason_of = |r: crate::geometry::DropReason| match r {
        crate::geometry::DropReason::Collision => "no clear position",
        crate::geometry::DropReason::OutOfBounds => "outside the chart",
        crate::geometry::DropReason::Density => "label density limit",
    };
    if dropped.len() <= LISTED {
        dropped
            .iter()
            .map(|d| {
                check(
                    format!("'{}' was not drawn: {}", d.text, reason_of(d.reason)),
                    d.would_be_at,
                    json!({"text": d.text, "reason": d.reason}),
                )
            })
            .collect()
    } else {
        let names: Vec<&str> = dropped.iter().take(LISTED).map(|d| d.text.as_str()).collect();
        vec![check(
            format!(
                "{} labels were not drawn, among them '{}'",
                dropped.len(),
                names.join("', '")
            ),
            None,
            json!({"count": dropped.len(), "texts": names}),
        )]
    }
}
