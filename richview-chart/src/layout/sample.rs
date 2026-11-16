//! What the distribution and binned types share: reading one number of a row the way every
//! painter reports it, and the plain statistics of a sample (FEA-CH-42, FEA-CH-70).

use super::{Ctx, Painted, Style};
use crate::color::{self, Oklch};
use crate::diag::Level;
use crate::geometry::{Anchor, Face, Fill, Item, Rect, Role};
use crate::numbers::{self, Format};
use crate::rows::{self, Num};
use serde_json::json;

/// The number in `field` of row `i`: `Ok(Some)` for a value, `Ok(None)` when it is missing and
/// `Err` when it cannot be used. An unusable cell is already reported as a check and in
/// `failure`, which keeps the first reason; `noun` names the chart in that reason.
pub(super) fn read_number(
    ctx: &mut Ctx,
    noun: &str,
    failure: &mut Option<String>,
    i: usize,
    field: &str,
) -> Result<Option<f64>, ()> {
    match rows::num(&ctx.rows[i], field) {
        Num::Value(v) => Ok(Some(v)),
        Num::Missing => Ok(None),
        Num::Underflow(t) => {
            ctx.underflow(i, field, &t);
            failure
                .get_or_insert_with(|| format!("Cannot draw this {noun} · {field} has a value too small to represent"));
            Err(())
        }
        Num::Invalid(t) => {
            ctx.check(
                "non-numeric",
                Level::Error,
                format!("Row {} has {field} '{t}', which is not a number", i + 1),
                Some("Write plain numbers; missing values may be empty or NA."),
                Some(Ctx::row_path(i, field)),
                None,
                json!({"row": i}),
            );
            failure.get_or_insert_with(|| format!("Cannot draw this {noun} · {field} has text that is not a number"));
            Err(())
        }
    }
}

/// Disclose rows left out for lack of a value, category or group.
pub(super) fn disclose_excluded(ctx: &mut Ctx, excluded: usize, what: &str) {
    if excluded == 0 {
        return;
    }
    ctx.check(
        "excluded-observations",
        Level::Warning,
        format!(
            "{} without {what} {} left out",
            super::count(excluded, "row", "rows"),
            super::verb(excluded, "is", "are")
        ),
        Some("Give every row a value for each channel."),
        None,
        None,
        json!({"count": excluded}),
    );
    ctx.note.push(format!(
        "{} {} not shown",
        super::count(excluded, "row", "rows"),
        super::verb(excluded, "is", "are")
    ));
}

/// The `p` quantile (0 to 1) of an ascending sample, by linear interpolation between the
/// closest ranks. The sample is not empty.
pub(super) fn quantile(sorted: &[f64], p: f64) -> f64 {
    let at = p * (sorted.len() - 1) as f64;
    let (lo, hi) = (at.floor() as usize, at.ceil() as usize);
    let (a, b) = (sorted[lo], sorted[hi]);
    // Written as a weighted sum so a huge pair cannot overflow the difference.
    a + (b / 2.0 - a / 2.0) * 2.0 * (at - lo as f64)
}

/// Where the ramp starts, so that a cell of one observation is still visible, and how far along
/// a chart goes when no cell holds more than one.
const RAMP_FROM: f64 = 0.2;
const RAMP_ONLY: f64 = 0.6;
const KEY_W: f64 = 240.0;
const KEY_ONE: f64 = 24.0;
const KEY_H: f64 = 8.0;

/// Position on the ramp of a cell holding `count` of at most `top` observations.
fn ramp_at(count: usize, top: usize) -> f64 {
    if top <= 1 {
        RAMP_ONLY
    } else {
        RAMP_FROM + (1.0 - RAMP_FROM) * count.saturating_sub(1) as f64 / (top - 1) as f64
    }
}

/// The colour of a cell holding `count` of at most `top` observations on the style's ramp.
pub(super) fn shade(ramp: &[Oklch; 3], count: usize, top: usize) -> Oklch {
    color::ramp(ramp, ramp_at(count, top))
}

/// A key under the figure that decodes the colour of counts from one observation to `top`, which
/// makes the figure taller.
pub(super) fn add_count_key(ctx: &Ctx, painted: &mut Painted, top: usize, format: &Format) {
    let ramp = ctx.style.ramp;
    let y = painted.height + 12.0;
    let key_w = if top > 1 { KEY_W.min(ctx.opts.width) } else { KEY_ONE };
    let steps = if top > 1 { 32 } else { 1 };
    for s in 0..steps {
        let count = 1 + (top.saturating_sub(1) * s) / steps;
        painted.figure_items.push(Item::Rect {
            rect: Rect::new(s as f64 * key_w / steps as f64, y, key_w / steps as f64 + 0.3, KEY_H),
            fill: Fill::solid(Style::rgba(shade(&ramp, count, top))),
            stroke: None,
            radius: [0.0; 4],
            role: Role::Key,
            series: None,
            obs: None,
        });
    }
    let baseline = y + KEY_H + 4.0 + ctx.style.font.vertical().0 * ctx.size();
    let mut ends = vec![(1usize, 0.0, Anchor::Start)];
    if top > 1 {
        ends.push((top, key_w, Anchor::End));
    }
    for (count, x, anchor) in ends {
        painted.figure_items.push(Item::Text(ctx.text(
            &numbers::value(count as f64, format),
            x,
            baseline,
            anchor,
            Face::Figures,
            Role::Key,
            ctx.style.ink,
        )));
    }
    painted.height = baseline + ctx.style.font.vertical().1 * ctx.size() + 4.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_runs_from_just_after_its_start_to_its_end() {
        assert!((ramp_at(1, 10) - RAMP_FROM).abs() < 1e-12);
        assert!((ramp_at(10, 10) - 1.0).abs() < 1e-12);
        assert!((ramp_at(1, 1) - RAMP_ONLY).abs() < 1e-12);
    }

    #[test]
    fn quantiles_interpolate_between_ranks() {
        let v = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(quantile(&v, 0.0), 1.0);
        assert_eq!(quantile(&v, 1.0), 4.0);
        assert!((quantile(&v, 0.5) - 2.5).abs() < 1e-12);
    }
}
