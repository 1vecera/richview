//! Motion policy shared by every host: the named curves, the durations, the stagger and the
//! per-mark timing of entrances (docs/MOTION.md). Hosts schedule frames and play these values as
//! CSS animations or per-frame geometry; they never choose their own timing.

/// The named curves a style's `motion.easing` picks, as CSS cubic-bezier control points.
pub fn easing(name: &str) -> [f64; 4] {
    match name {
        "out" => [0.33, 1.0, 0.68, 1.0],
        "soft" => [0.25, 0.8, 0.25, 1.0],
        // A small overshoot: the playful settle.
        "spring" => [0.34, 1.4, 0.64, 1.0],
        "snap" => [0.2, 0.0, 0.0, 1.0],
        _ => MOVE,
    }
}

/// `move` (docs/MOTION.md): eases in briefly, then decelerates over most of its time.
pub const MOVE: [f64; 4] = [0.4, 0.0, 0.2, 1.0];

/// A style's motion profile: how long an entrance takes, its curve and the stagger between
/// marks. The chart engine's own timings above are the defaults.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub character: String,
    pub duration_ms: f64,
    pub easing: [f64; 4],
    pub stagger_ms: f64,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            character: "gentle".into(),
            duration_ms: GROW_MS,
            easing: easing("standard"),
            stagger_ms: STAGGER_MS,
        }
    }
}

impl Profile {
    /// The factor every duration of the engine's 500 ms grow scales by.
    pub fn scale(&self) -> f64 {
        self.duration_ms.max(1.0) / GROW_MS
    }

    /// The curve data marks enter on: the style's own when it starts at rest and never passes
    /// its end, else [`MOVE`]. A curve that moves on its first frame front-loads the entrance
    /// (most of a fan drawn in its first third), and one that overshoots shows values past the
    /// data, so neither reaches the marks.
    pub fn entrance_curve(&self) -> [f64; 4] {
        let [_, y1, _, y2] = self.easing;
        if y1.abs() < 1e-9 && (0.0..=1.0).contains(&y2) {
            self.easing
        } else {
            MOVE
        }
    }

    /// Progress of an entrance at time share `t` (0 to 1) along [`Profile::entrance_curve`];
    /// `t` outside 0..1 clamps.
    pub fn ease(&self, t: f64) -> f64 {
        bezier(self.entrance_curve(), t)
    }

    /// The whole stagger of `count` marks: `stagger_ms` apart, compressed to fit under
    /// [`STAGGER_CAP_MS`]. A lone mark and a style without stagger have none.
    pub fn spread(&self, count: usize) -> f64 {
        if count < 2 {
            return 0.0;
        }
        ((count - 1) as f64 * self.stagger_ms.max(0.0)).min(STAGGER_CAP_MS)
    }

    /// Start of mark `rank` in reading order: evenly spaced across [`Profile::spread`], so a
    /// long row of bars keeps rolling instead of starting together once the cap is reached.
    pub fn delay(&self, rank: usize, count: usize) -> f64 {
        if count < 2 {
            return 0.0;
        }
        self.spread(count) * rank.min(count - 1) as f64 / (count - 1) as f64
    }

    /// When the last mark of `entrance` starts and how long it takes.
    fn last_mark(&self, entrance: Option<crate::geometry::Entrance>, marks: usize) -> Option<[f64; 2]> {
        use crate::geometry::Entrance as E;
        let k = self.scale();
        Some(match entrance? {
            E::Grow => [self.spread(marks), self.duration_ms.max(1.0)],
            E::Sweep | E::Fan => [0.0, SWEEP_MS * k],
            E::Settle => [0.0, SETTLE_MS * k],
            E::Fade => [0.0, DENSE_FADE_MS * k],
        })
    }

    /// When labels and annotations start to fade in and how long they take: from the moment
    /// the last mark has covered [`LABELS_AT`] of its travel until it lands, so labels arrive
    /// with their marks, never before them by more than that last stretch and never after a
    /// still pause. The entrance ends when they do.
    pub fn labels(&self, entrance: Option<crate::geometry::Entrance>, marks: usize) -> [f64; 2] {
        let Some([start, own]) = self.last_mark(entrance, marks) else {
            return [0.0, 0.0];
        };
        let lead = time_at(self.entrance_curve(), LABELS_AT) * own;
        [start + lead, own - lead]
    }

    /// The four durations the runtime clock needs for `entrance` over `marks` marks: how long
    /// a bar or tile grows, the sweep or fan runs, and a point settles or a dense chart fades,
    /// and when labels start.
    pub fn durations(&self, entrance: Option<crate::geometry::Entrance>, marks: usize) -> [f64; 4] {
        use crate::geometry::Entrance as E;
        let k = self.scale();
        let grow = self.duration_ms.max(1.0);
        let sweep = SWEEP_MS * k;
        let settle = if entrance == Some(E::Fade) {
            DENSE_FADE_MS * k
        } else {
            SETTLE_MS * k
        };
        [grow, sweep, settle, self.labels(entrance, marks)[0]]
    }
}

/// A CSS cubic-bezier timing function `[x1, y1, x2, y2]` at time `x` (clamped to 0..1). The x
/// control points stay in 0..1, so x is monotone in the curve parameter and bisection finds it;
/// y may leave 0..1 and overshoot.
pub fn bezier([x1, y1, x2, y2]: [f64; 4], x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0 || x >= 1.0 {
        return x;
    }
    let (x1, x2) = (x1.clamp(0.0, 1.0), x2.clamp(0.0, 1.0));
    let at = |a: f64, b: f64, t: f64| 3.0 * a * t * (1.0 - t).powi(2) + 3.0 * b * t * t * (1.0 - t) + t.powi(3);
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) * 0.5;
        if at(x1, x2, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at(y1, y2, (lo + hi) * 0.5)
}

/// The time share at which `curve` reaches `progress`, for curves that rise monotonically
/// from 0 to 1.
pub fn time_at(curve: [f64; 4], progress: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..40 {
        let mid = (lo + hi) * 0.5;
        if bezier(curve, mid) < progress {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

/// Line and area entrance: a clip sweeps from the left plot edge to the right.
pub const SWEEP_MS: f64 = 700.0;
/// Bars grow from their baseline.
pub const GROW_MS: f64 = 500.0;
/// Bars start this far apart in reading order …
pub const STAGGER_MS: f64 = 40.0;
/// … and the whole stagger never exceeds this.
pub const STAGGER_CAP_MS: f64 = 320.0;
/// Points settle (opacity and radius 0.5r → r).
pub const SETTLE_MS: f64 = 400.0;
/// Labels start once the last mark has covered this share of its travel.
pub const LABELS_AT: f64 = 0.85;
/// The fade of a dense chart.
pub const DENSE_FADE_MS: f64 = 300.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_style_curve_is_evaluated_with_overshoot() {
        let spring = easing("spring");
        assert!(bezier(spring, 0.0).abs() < 1e-9);
        assert!((bezier(spring, 1.0) - 1.0).abs() < 1e-9);
        let peak = (1..100).map(|i| bezier(spring, i as f64 / 100.0)).fold(0.0, f64::max);
        assert!(peak > 1.02 && peak < 1.2, "a slight overshoot, got {peak}");
        for name in ["out", "soft", "standard", "snap"] {
            let c = easing(name);
            let samples: Vec<f64> = (0..=100).map(|i| bezier(c, i as f64 / 100.0)).collect();
            assert!(
                samples.iter().all(|v| (0.0..=1.0 + 1e-9).contains(v)),
                "{name} stays inside"
            );
            assert!(samples.windows(2).all(|w| w[1] >= w[0] - 1e-9), "{name} is monotone");
        }
        assert_eq!(bezier(spring, -1.0), 0.0);
        assert_eq!(bezier(spring, 3.0), 1.0);
    }

    #[test]
    fn a_profile_scales_durations_and_spreads_the_stagger() {
        use crate::geometry::Entrance as E;
        let default = Profile::default();
        let [grow, sweep, settle, labels] = default.durations(Some(E::Grow), 8);
        assert_eq!([grow, sweep, settle], [500.0, 700.0, 400.0]);
        let [start, length] = default.labels(Some(E::Grow), 8);
        assert_eq!(labels, start);
        assert!(
            (start + length - (280.0 + 500.0)).abs() < 1e-6,
            "labels end as the last bar lands"
        );
        assert!(
            start > 280.0 + 250.0 && length > 100.0,
            "labels overlap only the last stretch"
        );
        let [start, length] = default.labels(Some(E::Sweep), 3);
        assert!((start + length - 700.0).abs() < 1e-6);
        assert_eq!(default.durations(Some(E::Fade), 3)[2], 300.0);
        let [start, length] = default.labels(Some(E::Fade), 3);
        assert!((start + length - 300.0).abs() < 1e-6);
        let air = Profile {
            duration_ms: 640.0,
            stagger_ms: 50.0,
            ..Profile::default()
        };
        assert_eq!(air.scale(), 1.28);
        let [grow, sweep, settle, _] = air.durations(Some(E::Grow), 4);
        assert_eq!((grow, sweep, settle), (640.0, 896.0, 512.0));
        assert_eq!(air.spread(4), 150.0);
        assert_eq!(air.delay(3, 4), 150.0);
        assert_eq!(air.delay(0, 1), 0.0);
        assert_eq!(air.durations(None, 4)[3], 0.0);
        assert_eq!(air.labels(None, 4), [0.0, 0.0]);
    }

    #[test]
    fn a_long_row_keeps_rolling_under_the_cap() {
        let air = Profile {
            stagger_ms: 50.0,
            ..Profile::default()
        };
        assert_eq!(air.spread(40), STAGGER_CAP_MS, "the whole stagger stays capped");
        assert_eq!(air.delay(39, 40), STAGGER_CAP_MS);
        let steps: Vec<f64> = (1..40).map(|i| air.delay(i, 40) - air.delay(i - 1, 40)).collect();
        assert!(
            steps.iter().all(|d| (d - STAGGER_CAP_MS / 39.0).abs() < 1e-9),
            "every bar starts a little after the one before, not all at once at the cap"
        );
    }

    #[test]
    fn a_crisp_profile_shows_every_mark_at_once() {
        let crisp = Profile {
            duration_ms: 160.0,
            stagger_ms: 0.0,
            easing: easing("snap"),
            ..Profile::default()
        };
        assert!((0..50).all(|i| crisp.delay(i, 50) == 0.0));
        let grow = crate::geometry::Entrance::Grow;
        assert_eq!(crisp.durations(Some(grow), 50)[0], 160.0);
        let [start, length] = crisp.labels(Some(grow), 50);
        assert!((start + length - 160.0).abs() < 1e-6);
        assert_eq!(
            crisp.entrance_curve(),
            easing("snap"),
            "a curve that starts at rest stays"
        );
        assert!(crisp.ease(0.3) > 0.3, "a snap curve is ahead of linear early on");
    }

    #[test]
    fn marks_enter_on_a_curve_that_starts_at_rest_and_never_passes_its_end() {
        for name in ["out", "soft", "spring"] {
            let style = Profile {
                easing: easing(name),
                ..Profile::default()
            };
            assert_eq!(style.entrance_curve(), MOVE, "{name} gives way to move");
            assert!(style.ease(0.4) < 0.65, "{name}: no front-loaded entrance");
            assert!(
                (1..100).all(|i| style.ease(i as f64 / 100.0) <= 1.0),
                "{name}: no overshoot"
            );
        }
        for name in ["standard", "snap"] {
            let style = Profile {
                easing: easing(name),
                ..Profile::default()
            };
            assert_eq!(style.entrance_curve(), easing(name), "{name} keeps its character");
        }
        assert!((time_at(MOVE, bezier(MOVE, 0.37)) - 0.37).abs() < 1e-6);
    }
}
