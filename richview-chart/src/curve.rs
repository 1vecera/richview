//! Line geometry: curves, steps, breaks and dense-series reduction (FEA-CH-03, FEA-CH-17,
//! FEA-CH-38).

use crate::block::Curve;
use crate::geometry::Path;

/// Append one run of points (sorted by x) to `path` as the curve asks. Monotone curves use
/// Fritsch–Butland slopes with bounded handles, so each segment stays between its two
/// observations and never overshoots; `strength` (0–1) scales the handles.
pub fn append_run(path: &mut Path, points: &[[f64; 2]], curve: Curve, strength: f64, continue_from: bool) {
    let Some(&first) = points.first() else {
        return;
    };
    if continue_from {
        path.line_to(first)
    } else {
        path.move_to(first)
    }
    match curve {
        Curve::Step | Curve::StepBefore | Curve::StepAfter => {
            for pair in points.windows(2) {
                let [a, b] = [pair[0], pair[1]];
                match curve {
                    Curve::StepBefore => path.line_to([a[0], b[1]]),
                    Curve::StepAfter => path.line_to([b[0], a[1]]),
                    _ => {
                        let middle = (a[0] + b[0]) / 2.0;
                        path.line_to([middle, a[1]]);
                        path.line_to([middle, b[1]]);
                    }
                }
                path.line_to(b);
            }
        }
        Curve::Linear => {
            for p in &points[1..] {
                path.line_to(*p);
            }
        }
        Curve::Monotone => {
            let curved = strength > 0.0 && points.len() > 2 && points.windows(2).all(|p| p[1][0] > p[0][0]);
            if !curved {
                for p in &points[1..] {
                    path.line_to(*p);
                }
                return;
            }
            for (pair, [a, b]) in points.windows(2).zip(controls(points, strength.clamp(0.0, 1.0))) {
                path.curve_to(a, b, pair[1]);
            }
        }
    }
}

/// The run's points in reverse, for closing an area back along its lower edge.
pub fn reversed(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    points.iter().rev().copied().collect()
}

fn controls(points: &[[f64; 2]], strength: f64) -> Vec<[[f64; 2]; 2]> {
    let widths: Vec<f64> = points.windows(2).map(|p| p[1][0] - p[0][0]).collect();
    let slopes: Vec<f64> = points
        .windows(2)
        .zip(&widths)
        .map(|(p, h)| (p[1][1] - p[0][1]) / h)
        .collect();
    let mut tangents = vec![0.0; points.len()];
    tangents[0] = slopes[0];
    tangents[points.len() - 1] = slopes[slopes.len() - 1];
    for i in 1..points.len() - 1 {
        let (a, b) = (slopes[i - 1], slopes[i]);
        if a * b > 0.0 {
            let (w1, w2) = (2.0 * widths[i] + widths[i - 1], widths[i] + 2.0 * widths[i - 1]);
            tangents[i] = (w1 + w2) / (w1 / a + w2 / b);
        }
    }
    points
        .windows(2)
        .enumerate()
        .map(|(i, p)| {
            let h = widths[i] * strength / 3.0;
            let (lo, hi) = (p[0][1].min(p[1][1]), p[0][1].max(p[1][1]));
            [
                [p[0][0] + h, (p[0][1] + tangents[i] * h).clamp(lo, hi)],
                [p[1][0] - h, (p[1][1] - tangents[i + 1] * h).clamp(lo, hi)],
            ]
        })
        .collect()
}

/// M4 reduction of one run in pixel space: per pixel column keep the first, lowest, highest and
/// last point, which draws identically to the full run (FEA-CH-17).
pub fn m4(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if points.len() <= 4 {
        return points.to_vec();
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < points.len() {
        let column = points[i][0].floor();
        let start = i;
        let (mut lo, mut hi) = (i, i);
        while i < points.len() && points[i][0].floor() == column {
            if points[i][1] < points[lo][1] {
                lo = i;
            }
            if points[i][1] > points[hi][1] {
                hi = i;
            }
            i += 1;
        }
        let mut picks = [start, lo, hi, i - 1];
        picks.sort_unstable();
        for p in picks {
            if out.last() != Some(&points[p]) {
                out.push(points[p]);
            }
        }
    }
    out
}

/// One pixel column of a dense run: its mean and range (FEA-CH-17).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column {
    pub x: f64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub count: usize,
}

/// Aggregate a run in data space into pixel columns, given each point's pixel x.
pub fn columns(px: &[f64], values: &[f64]) -> Vec<Column> {
    let mut out: Vec<Column> = Vec::new();
    for (x, v) in px.iter().zip(values) {
        let column = x.floor();
        match out.last_mut() {
            Some(c) if c.x.floor() == column => {
                c.mean += (v - c.mean) / (c.count + 1) as f64;
                c.min = c.min.min(*v);
                c.max = c.max.max(*v);
                c.count += 1;
            }
            _ => out.push(Column {
                x: column + 0.5,
                mean: *v,
                min: *v,
                max: *v,
                count: 1,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Cmd;

    #[test]
    fn monotone_segments_never_overshoot() {
        let points = [[0.0, 10.0], [10.0, 0.0], [20.0, 0.0], [30.0, 20.0], [40.0, 5.0]];
        let mut path = Path::new();
        append_run(&mut path, &points, Curve::Monotone, 1.0, false);
        let mut previous = points[0];
        for cmd in &path.cmds {
            if let Cmd::C { c1, c2, p } = cmd {
                let (lo, hi) = (previous[1].min(p[1]), previous[1].max(p[1]));
                assert!((lo..=hi).contains(&c1[1]) && (lo..=hi).contains(&c2[1]));
                previous = *p;
            }
        }
    }

    #[test]
    fn steps_hold_values() {
        let mut path = Path::new();
        append_run(&mut path, &[[0.0, 1.0], [10.0, 2.0]], Curve::StepAfter, 0.0, false);
        assert_eq!(
            path.cmds,
            vec![
                Cmd::M { p: [0.0, 1.0] },
                Cmd::L { p: [10.0, 1.0] },
                Cmd::L { p: [10.0, 2.0] }
            ]
        );
    }

    #[test]
    fn m4_keeps_every_spike() {
        let points: Vec<[f64; 2]> = (0..1000)
            .map(|i| [i as f64 / 100.0, if i == 517 { 99.0 } else { (i % 7) as f64 }])
            .collect();
        let reduced = m4(&points);
        assert!(reduced.len() <= 40);
        assert!(reduced.contains(&[5.17, 99.0]));
        let cols = columns(&[0.2, 0.7, 1.1], &[1.0, 3.0, 5.0]);
        assert_eq!(cols.len(), 2);
        assert_eq!((cols[0].mean, cols[0].min, cols[0].max), (2.0, 1.0, 3.0));
    }
}
