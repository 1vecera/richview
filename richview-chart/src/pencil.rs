//! Pencil marks: seeded, tapered pen strokes for the non-data marks of hand-drawn styles.
//!
//! Each primitive returns SVG path data: one filled outline per pen stroke, inside the box it is
//! given, give or take the stroke weight. A stroke is a smooth centreline whose width swells from
//! a fine tip and thins where the pen lifts, with small seeded deviations in course and pressure.
//! The same id always draws the same stroke, so renders, exports and screenshots repeat exactly,
//! and a mark changes only when its own id does. The geometry uses arithmetic and square roots
//! only, which IEEE 754 rounds identically on every target, so native and WebAssembly builds emit
//! the same paths. Gridlines, axes and data marks never use these strokes: they stay exact.

use std::fmt::Write;

type Point = (f64, f64);

/// The variation of one mark, seeded from its stable id.
struct Pen(u64);

impl Pen {
    fn new(id: &str) -> Self {
        // FNV-1a over the id's bytes.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in id.bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Pen(hash)
    }

    /// SplitMix64, mapped to [0, 1).
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A value in [-1, 1).
    fn signed(&mut self) -> f64 {
        self.unit() * 2.0 - 1.0
    }

    /// A smooth course in [-1, 1] along a stroke, through `knots` + 1 seeded values.
    fn wander(&mut self, knots: usize) -> Wander {
        Wander((0..=knots).map(|_| self.signed()).collect())
    }
}

struct Wander(Vec<f64>);

impl Wander {
    fn at(&self, t: f64) -> f64 {
        let spans = self.0.len() - 1;
        let x = t.clamp(0.0, 1.0) * spans as f64;
        let i = (x as usize).min(spans - 1);
        let s = smooth(x - i as f64);
        self.0[i] + (self.0[i + 1] - self.0[i]) * s
    }
}

fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn samples(length: f64) -> usize {
    (length / 9.0).ceil().clamp(12.0, 64.0) as usize
}

/// Stroke width along the centreline: a fine tip that swells over the first `land` of the
/// length, small seeded pressure changes, and a thinning end over the last `lift`.
fn profile(pen: &mut Pen, count: usize, weight: f64, land: f64, lift: f64) -> Vec<f64> {
    const TIP: f64 = 0.16;
    let pressure = pen.wander(3);
    (0..count)
        .map(|i| {
            let t = i as f64 / (count - 1) as f64;
            let taper = smooth(t / land) * smooth((1.0 - t) / lift);
            weight * (TIP + (1.0 - TIP) * taper) * (1.0 + 0.08 * pressure.at(t))
        })
        .collect()
}

/// One filled stroke: the centreline offset by half the width on each side, as smooth curves
/// through the offset points, closed across the tips.
fn stroke(out: &mut String, centre: &[Point], widths: &[f64]) {
    let count = centre.len();
    let mut left = Vec::with_capacity(count);
    let mut right = Vec::with_capacity(count);
    for i in 0..count {
        let (a, b) = (centre[i.saturating_sub(1)], centre[(i + 1).min(count - 1)]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt().max(1e-9);
        let (nx, ny) = (-dy / length, dx / length);
        let half = widths[i] / 2.0;
        left.push((centre[i].0 + nx * half, centre[i].1 + ny * half));
        right.push((centre[i].0 - nx * half, centre[i].1 - ny * half));
    }
    right.reverse();
    curve(out, &left, 'M');
    curve(out, &right, 'L');
    out.push('Z');
}

/// Quadratic curves through the midpoints of a polyline, with its points as control points.
fn curve(out: &mut String, points: &[Point], start: char) {
    out.push(start);
    point(out, points[0]);
    for i in 1..points.len() - 1 {
        let (p, q) = (points[i], points[i + 1]);
        out.push('Q');
        point(out, p);
        out.push(' ');
        point(out, ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0));
    }
    out.push('L');
    point(out, points[points.len() - 1]);
}

fn point(out: &mut String, (x, y): Point) {
    number(out, x);
    out.push(' ');
    number(out, y);
}

fn number(out: &mut String, value: f64) {
    let rounded = (value * 100.0).round() / 100.0;
    let _ = write!(out, "{}", if rounded == 0.0 { 0.0 } else { rounded });
}

/// A ruled line across a `width` × `height` box, drawn freehand: a slight tilt and bow, a
/// gentle wander, a fine start and a lifted end.
pub fn rule(id: &str, width: f64, height: f64, weight: f64) -> String {
    line(id, width, height, weight, 0.0, (0.05, 0.1))
}

/// A marker underline under a `width` × `height` box: the pen lands low at the left, rises a
/// little to the right and flicks off.
pub fn underline(id: &str, width: f64, height: f64, weight: f64) -> String {
    line(id, width, height, weight, 0.55, (0.03, 0.18))
}

/// A strike-through across text in a `width` × `height` box: one thin, slightly rising line
/// through the middle of the lowercase letters, which stay legible beneath it.
pub fn strike(id: &str, width: f64, height: f64, weight: f64) -> String {
    let mut out = String::new();
    let mut pen = Pen::new(id);
    let room = ((height - weight) / 2.0).max(0.0);
    let rise = (0.6 + 0.3 * pen.unit()) * room * 0.5;
    let wobble = pen.wander(4);
    let count = samples(width);
    let centre: Vec<Point> = (0..count)
        .map(|i| {
            let t = i as f64 / (count - 1) as f64;
            let y = height * 0.58 - rise * (t - 0.5) + 0.12 * room * wobble.at(t);
            (width * t, y.clamp(weight / 2.0, height - weight / 2.0))
        })
        .collect();
    let widths = profile(&mut pen, count, weight, 0.04, 0.08);
    stroke(&mut out, &centre, &widths);
    out
}

/// A near-horizontal stroke through the middle of its box. `rise` lifts the right end by that
/// share of the free height; `taper` is the landing and lifting share of the length.
fn line(id: &str, width: f64, height: f64, weight: f64, rise: f64, taper: (f64, f64)) -> String {
    let mut out = String::new();
    let mut pen = Pen::new(id);
    let room = ((height - weight) / 2.0).max(0.0);
    let start = pen.unit() * 0.5 * weight;
    let end = pen.unit() * 0.5 * weight;
    let tilt = pen.signed() * 0.3 * room;
    let bow = pen.signed() * 0.35 * room;
    let wobble = pen.wander(5);
    let count = samples(width);
    let centre: Vec<Point> = (0..count)
        .map(|i| {
            let t = i as f64 / (count - 1) as f64;
            let x = start + (width - start - end) * t;
            let y = height / 2.0 + rise * room * (0.5 - t) + tilt * (t - 0.5) - bow * 4.0 * t * (1.0 - t)
                + 0.2 * room * wobble.at(t);
            (x, y.clamp(weight / 2.0, height - weight / 2.0))
        })
        .collect();
    let widths = profile(&mut pen, count, weight, taper.0, taper.1);
    stroke(&mut out, &centre, &widths);
    out
}

/// A left square bracket filling a `width` × `height` box: a short hook along the top, a long
/// stroke down the left side and a hook back along the bottom, drawn in one movement. The hooks
/// stay within the top and bottom `width` of the box, so the middle can stretch vertically.
pub fn bracket(id: &str, width: f64, height: f64, weight: f64) -> String {
    let mut out = String::new();
    let mut pen = Pen::new(id);
    let half = weight / 2.0;
    let x = weight * (0.7 + 0.2 * pen.unit());
    let slant = pen.signed() * 0.25 * weight;
    let (top, bottom) = (
        half + 0.2 * weight * pen.unit(),
        height - half - 0.2 * weight * pen.unit(),
    );
    let radius = (width - x) * 0.45;
    let tips = (
        (width - half) - 0.15 * width * pen.unit(),
        (width - half) - 0.15 * width * pen.unit(),
    );
    let mut path: Vec<Point> = vec![];
    segment(
        &mut path,
        (tips.0, top + 0.12 * weight * pen.signed()),
        (x + radius, top),
        6,
    );
    corner(&mut path, (x + radius, top), (x, top), (x, top + radius), 6);
    let wobble = pen.wander(4);
    let run = samples(bottom - top - 2.0 * radius).max(8);
    for i in 1..run {
        let t = i as f64 / run as f64;
        let y = top + radius + (bottom - top - 2.0 * radius) * t;
        path.push((x + slant * t + 0.18 * weight * wobble.at(t), y));
    }
    let foot = x + slant;
    corner(
        &mut path,
        (foot, bottom - radius),
        (foot, bottom),
        (foot + radius, bottom),
        6,
    );
    segment(
        &mut path,
        (foot + radius, bottom),
        (tips.1, bottom - 0.12 * weight * pen.signed()),
        6,
    );
    let widths = profile(&mut pen, path.len(), weight, 0.05, 0.06);
    stroke(&mut out, &path, &widths);
    out
}

/// Points from `a` (exclusive unless the path is empty) to `b` (inclusive).
fn segment(path: &mut Vec<Point>, a: Point, b: Point, count: usize) {
    if path.is_empty() {
        path.push(a);
    }
    for i in 1..=count {
        let t = i as f64 / count as f64;
        path.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
}

/// A rounded corner from `a` to `c` around the control point `b` (a quadratic curve).
fn corner(path: &mut Vec<Point>, a: Point, b: Point, c: Point, count: usize) {
    for i in 1..=count {
        let t = i as f64 / count as f64;
        let u = 1.0 - t;
        path.push((
            u * u * a.0 + 2.0 * u * t * b.0 + t * t * c.0,
            u * u * a.1 + 2.0 * u * t * b.1 + t * t * c.1,
        ));
    }
}

/// An ellipse drawn round a `width` × `height` box: it starts at the upper right, goes once
/// round and runs on past its start, spiralling slightly outwards, as a pen circle does.
pub fn circle(id: &str, width: f64, height: f64, weight: f64) -> String {
    let mut out = String::new();
    let mut pen = Pen::new(id);
    let centre = circle_centreline(&mut pen, width, height, weight);
    let widths = profile(&mut pen, centre.len(), weight, 0.04, 0.12);
    stroke(&mut out, &centre, &widths);
    out
}

/// Rotation by the angle whose half-angle tangent is `t`, as an exact rational point on the
/// unit circle: (cos, sin).
fn turn(t: f64) -> Point {
    let d = 1.0 + t * t;
    ((1.0 - t * t) / d, 2.0 * t / d)
}

fn circle_centreline(pen: &mut Pen, width: f64, height: f64, weight: f64) -> Vec<Point> {
    const STEPS: usize = 72;
    // The radii leave room for the outward run past the start, and for the stroke.
    let (rx, ry) = (
        ((width / 2.0 - weight) / 1.06).max(weight),
        ((height / 2.0 - weight) / 1.06).max(weight),
    );
    let (cx, cy) = (width / 2.0, height / 2.0);
    // Start near 60° above the right-hand side (screen y points down).
    let mut heading = turn(-0.58 + 0.12 * pen.signed());
    // One turn plus an overrun of 30–50°, in equal steps: the step's half-angle tangent from
    // its series, which needs no trigonometric function.
    let sweep = 2.0 * std::f64::consts::PI * (1.0 + (30.0 + 20.0 * pen.unit()) / 360.0);
    let half = -sweep / STEPS as f64 / 2.0;
    let step = turn(half + half * half * half / 3.0 + 2.0 * half.powi(5) / 15.0);
    let wobble = pen.wander(5);
    (0..=STEPS)
        .map(|i| {
            let t = i as f64 / STEPS as f64;
            let scale = 0.95 + 0.1 * t + 0.01 * wobble.at(t);
            let point = (cx + rx * scale * heading.0, cy + ry * scale * heading.1);
            heading = (
                heading.0 * step.0 - heading.1 * step.1,
                heading.0 * step.1 + heading.1 * step.0,
            );
            point
        })
        .collect()
}

/// An arrow from a note at `from` to its target at `to`: a gently curved shaft and an open
/// head of two short strokes. Its box is the rectangle of both ends, widened by the head and
/// the shaft's bend.
pub fn arrow(id: &str, from: Point, to: Point, weight: f64) -> String {
    let mut out = String::new();
    let mut pen = Pen::new(id);
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt().max(1e-9);
    let (nx, ny) = (-dy / length, dx / length);
    let bend = (0.08 + 0.06 * pen.unit()) * length * if pen.unit() < 0.5 { -1.0 } else { 1.0 };
    let control = ((from.0 + to.0) / 2.0 + nx * bend, (from.1 + to.1) / 2.0 + ny * bend);
    let mut shaft = vec![from];
    corner(&mut shaft, from, control, to, samples(length));
    let widths = profile(&mut pen, shaft.len(), weight, 0.08, 0.04);
    stroke(&mut out, &shaft, &widths);
    // The head opens about 28° either side of the shaft's direction at the tip.
    let (bx, by) = (control.0 - to.0, control.1 - to.1);
    let back = (bx * bx + by * by).sqrt().max(1e-9);
    let (bx, by) = (bx / back, by / back);
    let barb = head_length(length, weight);
    for sign in [1.0, -1.0] {
        let (c, s) = turn(0.25 * sign);
        let direction = (bx * c - by * s, bx * s + by * c);
        let end = (
            to.0 + direction.0 * barb * (0.9 + 0.2 * pen.unit()),
            to.1 + direction.1 * barb * (0.9 + 0.2 * pen.unit()),
        );
        let mut line = vec![end];
        segment(&mut line, end, to, 8);
        let widths = profile(&mut pen, line.len(), weight, 0.3, 0.12);
        stroke(&mut out, &line, &widths);
    }
    out
}

fn head_length(length: f64, weight: f64) -> f64 {
    (length * 0.2).clamp(3.0 * weight, 7.0 * weight)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEIGHT: f64 = 1.6;

    /// A primitive drawn into a box: id, width, height and weight to path data.
    type Draw = fn(&str, f64, f64, f64) -> String;

    fn boxed() -> Vec<(&'static str, f64, f64, Draw)> {
        vec![
            ("rule", 640.0, 8.0, rule),
            ("underline", 180.0, 10.0, underline),
            ("strike", 220.0, 18.0, strike),
            ("bracket", 14.0, 120.0, bracket),
            ("circle", 90.0, 44.0, circle),
        ]
    }

    /// Commands and coordinates of a path, checked as SVG path data: M, L and Q with the right
    /// number of finite coordinates, every subpath closed.
    fn parse(d: &str) -> Vec<Point> {
        let mut points = vec![];
        let mut chars = d.chars().peekable();
        let mut open = false;
        while let Some(command) = chars.next() {
            let arity = match command {
                'M' => {
                    assert!(!open, "subpath not closed before M in {d}");
                    open = true;
                    1
                }
                'L' => 1,
                'Q' => 2,
                'Z' => {
                    assert!(open, "Z without a subpath");
                    open = false;
                    0
                }
                other => panic!("unexpected command {other:?} in {d}"),
            };
            assert!(open || command == 'Z', "{command} outside a subpath");
            let mut text = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphabetic() {
                    break;
                }
                text.push(c);
                chars.next();
            }
            let numbers: Vec<f64> = text
                .split_whitespace()
                .map(|n| n.parse::<f64>().expect("number"))
                .collect();
            assert_eq!(numbers.len(), arity * 2, "{command} arguments: {text:?}");
            assert!(numbers.iter().all(|n| n.is_finite()));
            points.extend(numbers.chunks(2).map(|p| (p[0], p[1])));
        }
        assert!(!open, "last subpath not closed");
        points
    }

    #[test]
    fn the_same_id_draws_the_same_stroke_and_other_ids_draw_others() {
        for (name, w, h, draw) in boxed() {
            let a = draw("report.rule.header", w, h, WEIGHT);
            assert_eq!(a, draw("report.rule.header", w, h, WEIGHT), "{name}");
            assert_ne!(a, draw("report.rule.footer", w, h, WEIGHT), "{name}");
        }
        let arrow_a = arrow("note-1", (0.0, 0.0), (80.0, 30.0), WEIGHT);
        assert_eq!(arrow_a, arrow("note-1", (0.0, 0.0), (80.0, 30.0), WEIGHT));
        assert_ne!(arrow_a, arrow("note-2", (0.0, 0.0), (80.0, 30.0), WEIGHT));
    }

    #[test]
    fn strokes_stay_in_their_box() {
        for (name, w, h, draw) in boxed() {
            for id in ["a", "b", "c", "section-3", "callout-0"] {
                for (x, y) in parse(&draw(id, w, h, WEIGHT)) {
                    assert!(
                        (-WEIGHT..=w + WEIGHT).contains(&x) && (-WEIGHT..=h + WEIGHT).contains(&y),
                        "{name} {id}: ({x}, {y}) outside {w} × {h}"
                    );
                }
            }
        }
        let (from, to) = ((10.0, 60.0), (120.0, 20.0));
        for id in ["a", "b", "c"] {
            let reach = head_length(117.0, WEIGHT) * 1.1 + 0.14 * 117.0 / 2.0 + WEIGHT;
            for (x, y) in parse(&arrow(id, from, to, WEIGHT)) {
                assert!(
                    (from.0 - reach..=to.0 + reach).contains(&x) && (to.1 - reach..=from.1 + reach).contains(&y),
                    "arrow {id}: ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn circles_overshoot_their_start_and_do_not_close() {
        for id in ["a", "b", "c", "focal-point"] {
            let centre = circle_centreline(&mut Pen::new(id), 90.0, 44.0, WEIGHT);
            let angle = |(x, y): Point| (y - 22.0).atan2((x - 45.0) / 90.0 * 44.0);
            let mut swept = 0.0;
            for pair in centre.windows(2) {
                let mut delta = angle(pair[1]) - angle(pair[0]);
                if delta > std::f64::consts::PI {
                    delta -= 2.0 * std::f64::consts::PI;
                } else if delta < -std::f64::consts::PI {
                    delta += 2.0 * std::f64::consts::PI;
                }
                swept += delta;
            }
            let degrees = swept.abs().to_degrees();
            assert!((388.0..=412.0).contains(&degrees), "{id}: swept {degrees}°");
            // Distance from the centre in units of the nominal radii, so an ellipse reads as 1.
            let (rx, ry) = ((45.0 - WEIGHT) / 1.06, (22.0 - WEIGHT) / 1.06);
            let radius = |(x, y): Point| (((x - 45.0) / rx).powi(2) + ((y - 22.0) / ry).powi(2)).sqrt();
            let (first, last) = (centre[0], centre[centre.len() - 1]);
            assert!(
                radius(last) > radius(first) * 1.06,
                "{id}: the end runs outside the start"
            );
        }
    }

    #[test]
    fn strokes_are_tapered_at_both_ends() {
        let widths = profile(&mut Pen::new("taper"), 40, WEIGHT, 0.05, 0.1);
        assert!(widths[0] < 0.3 * WEIGHT && widths[39] < 0.3 * WEIGHT);
        assert!(widths[20] > 0.85 * WEIGHT && widths[20] < 1.1 * WEIGHT);
    }

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        let mut out = String::new();
        for value in [-0.001, 12.5, 3.0, 0.07, -4.256] {
            number(&mut out, value);
            out.push(' ');
        }
        assert_eq!(out, "0 12.5 3 0.07 -4.26 ");
    }
}
