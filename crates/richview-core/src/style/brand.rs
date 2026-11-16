//! Derive a company style from a validated brand colour over a builtin (STY-71 proposal).
use richview_chart::color::{self, Oklch, Vision};
use serde_json::{Map, Value, json};
use std::{cell::RefCell, collections::VecDeque};

const SURFACES: [&str; 3] = ["bg", "bg-elev", "bg-chart"];
const NEUTRALS: [&str; 9] = [
    "bg",
    "bg-elev",
    "bg-chart",
    "rule",
    "rule-soft",
    "ink",
    "chart-ink",
    "ink-soft",
    "ink-faint",
];

/// Reduce chroma at fixed lightness and hue until the colour fits the sRGB gamut.
fn fit(mut color: Oklch) -> Oklch {
    color.l = color.l.clamp(0.0, 1.0);
    if color.in_gamut() {
        return color;
    }
    let (mut low, mut high) = (0.0, color.c.max(0.0));
    while high - low > 0.00001 {
        let middle = (low + high) / 2.0;
        if Oklch::new(color.l, middle, color.h).in_gamut() {
            low = middle;
        } else {
            high = middle;
        }
    }
    color.c = low;
    color
}

/// Move away from the page until every surface reaches the requested contrast.
fn reach(original: Oklch, surfaces: &[Oklch], floor: f64, dark: bool) -> Oklch {
    let mut color = fit(original);
    for _ in 0..200 {
        if surfaces
            .iter()
            .all(|surface| color::contrast(color, *surface) >= floor)
        {
            break;
        }
        color = fit(Oklch::new(
            (color.l + if dark { 0.005 } else { -0.005 }).clamp(0.0, 1.0),
            original.c,
            original.h,
        ));
    }
    color
}

fn separation(a: Oklch, b: Oklch) -> f64 {
    Vision::ALL
        .into_iter()
        .fold(color::delta_e(a, b) / 0.15, |least, vision| {
            least.min(color::delta_e(vision.simulate(a), vision.simulate(b)) / 0.08)
        })
}

/// Compute each vision projection once per candidate rather than once per pair.
fn projections(color: Oklch) -> [Oklch; 4] {
    [
        color,
        Vision::Protan.simulate(color),
        Vision::Deutan.simulate(color),
        Vision::Tritan.simulate(color),
    ]
}

fn projected_separation(a: &[Oklch; 4], b: &[Oklch; 4]) -> f64 {
    (1..4).fold(color::delta_e(a[0], b[0]) / 0.15, |least, i| {
        least.min(color::delta_e(a[i], b[i]) / 0.08)
    })
}

/// The emitted RGB colour, measured after rounding so the checked palette is the displayed one.
fn rgb(color: Oklch) -> Oklch {
    color::parse(&color.hex()).expect("generated hex is a colour")
}

const SEPARATION: f64 = 1.03;

fn hue_gap(a: f64, b: f64) -> f64 {
    ((a - b + 540.0).rem_euclid(360.0) - 180.0).abs()
}

fn safe_pair(candidate: Oklch, palette: &[Oklch]) -> bool {
    let required = if palette.len() < 4 {
        palette
    } else {
        &palette[palette.len() - 1..]
    };
    required
        .iter()
        .all(|&other| separation(candidate, other) >= SEPARATION)
}

fn palette_from(seed: &[Oklch], chart: &[Oklch], base: &[Oklch], dark: bool) -> Option<Vec<Oklch>> {
    let mut palette = seed.to_vec();
    for &color in base {
        if palette.len() == 6 {
            return Some(palette);
        }
        let c = rgb(fit(color));
        if c.c >= 0.03
            && palette
                .iter()
                .any(|p| p.c >= 0.03 && hue_gap(c.h, p.h) < 30.0)
        {
            continue;
        }
        if palette.len() < 4 && chart.iter().any(|s| color::contrast(c, *s) < 3.02) {
            continue;
        }
        if safe_pair(c, &palette) {
            palette.push(c);
        }
    }
    let first = palette[0];
    let (band_lo, band_hi, search_lo, search_steps) = if dark {
        (0.66_f64, 0.84_f64, 0.60_f64, 16)
    } else {
        (0.45_f64, 0.62_f64, 0.34_f64, 17)
    };
    let mid = first.l.clamp(band_lo + 0.07, band_hi - 0.07);
    let chroma = if first.c < 0.03 {
        0.15
    } else {
        first.c.clamp(0.11, 0.15)
    };
    for step in 1..40 {
        if palette.len() == 6 {
            return Some(palette);
        }
        let hue = (first.h + 137.508 * f64::from(step)).rem_euclid(360.0);
        if palette
            .iter()
            .any(|p| p.c >= 0.03 && hue_gap(hue, p.h) < 30.0)
        {
            continue;
        }
        let target_l = mid + if palette.len() % 2 == 1 { 0.07 } else { -0.07 };
        // Hue and lightness contribute a lower bound to the full distance. Visit candidates
        // by that bound and stop once no later candidate can beat the best result. Original
        // order breaks exact ties, preserving the exhaustive recipe's chosen palette.
        let mut order = Vec::new();
        for dh in (-60..=60).step_by(5) {
            for l_index in 0..=search_steps {
                let lightness = search_lo + f64::from(l_index) * 0.02;
                let bound =
                    (f64::from(dh) / 60.0).powi(2) + ((lightness - target_l) / 0.10).powi(2);
                for dc in [0.0, 0.03, -0.03] {
                    order.push((bound, order.len(), dh, lightness, dc));
                }
            }
        }
        order.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let projected: Vec<_> = palette.iter().copied().map(projections).collect();
        let mut chosen = None;
        for strict in [true, false] {
            let mut cost = f64::INFINITY;
            let mut best_index = usize::MAX;
            for &(bound, index, dh, lightness, dc) in &order {
                if bound > cost {
                    break;
                }
                let c = rgb(fit(Oklch::new(
                    lightness,
                    chroma + dc,
                    (hue + f64::from(dh)).rem_euclid(360.0),
                )));
                if c.c < 0.06
                    || (palette.len() < 4 && chart.iter().any(|s| color::contrast(c, *s) < 3.02))
                {
                    continue;
                }
                if strict
                    && palette
                        .iter()
                        .any(|p| p.c >= 0.03 && hue_gap(c.h, p.h) < 30.0)
                {
                    continue;
                }
                let projection = projections(c);
                let required = if strict || palette.len() < 4 {
                    &projected[..]
                } else {
                    &projected[projected.len() - 1..]
                };
                if required
                    .iter()
                    .any(|p| projected_separation(&projection, p) < SEPARATION)
                {
                    continue;
                }
                let d = bound + ((c.c - chroma) / 0.05).powi(2);
                if d < cost || (d == cost && index < best_index) {
                    chosen = Some(c);
                    cost = d;
                    best_index = index;
                }
            }
            if chosen.is_some() {
                break;
            }
        }
        if chosen.is_none() {
            let mut best = -1.0;
            for candidate_hue in (0..360).step_by(5) {
                for l_index in 0..=search_steps {
                    let c = rgb(fit(Oklch::new(
                        search_lo + f64::from(l_index) * 0.02,
                        chroma,
                        f64::from(candidate_hue),
                    )));
                    if c.c < 0.06
                        || (palette.len() < 4
                            && chart.iter().any(|s| color::contrast(c, *s) < 3.02))
                    {
                        continue;
                    }
                    let projection = projections(c);
                    let required = if palette.len() < 4 {
                        &projected[..]
                    } else {
                        &projected[projected.len() - 1..]
                    };
                    if required
                        .iter()
                        .any(|p| projected_separation(&projection, p) < SEPARATION)
                    {
                        continue;
                    }
                    let score = projected
                        .iter()
                        .map(|p| projected_separation(&projection, p))
                        .fold(f64::INFINITY, f64::min);
                    if score > best {
                        chosen = Some(c);
                        best = score;
                    }
                }
            }
        }
        if let Some(c) = chosen {
            palette.push(c);
        }
    }
    if palette.len() == 6 {
        Some(palette)
    } else {
        None
    }
}

fn second_slot(second: Oklch, first: Oklch, chart: &[Oklch], dark: bool) -> Option<Oklch> {
    let original = reach(second, chart, 3.02, dark);
    for step in 0..=10 {
        for sign in if step == 0 {
            &[0.0][..]
        } else {
            &[1.0, -1.0][..]
        } {
            let l = original.l + sign * f64::from(step) * 0.02;
            if !(0.05..0.98).contains(&l) {
                continue;
            }
            let c = rgb(fit(Oklch::new(l, second.c, second.h)));
            if chart.iter().all(|s| color::contrast(c, *s) >= 3.02)
                && separation(c, first) >= SEPARATION
            {
                return Some(c);
            }
        }
    }
    None
}

/// Retint the positive ramp only when its meaning and all existing ramp checks survive.
fn brand_ramp(brand: Oklch, base: &Value, chart: &[Oklch]) -> Option<Vec<String>> {
    let list = |key: &str| -> Option<Vec<Oklch>> {
        base["chart"][key]
            .as_array()?
            .iter()
            .map(|v| color::parse(v.as_str()?))
            .collect()
    };
    let original = list("ramp")?;
    let negative = list("negative-ramp")?;
    if original.len() != 3
        || negative.len() != 3
        || brand.c < 0.03
        || (original[0].c < 0.03 && original[2].c < 0.03)
    {
        return None;
    }
    let down = base["tokens"]["down"].as_str().and_then(color::parse)?;
    if hue_gap(brand.h, negative[1].h) < 40.0 || hue_gap(brand.h, down.h) < 40.0 {
        return None;
    }
    let ramp: Vec<String> = original
        .into_iter()
        .map(|stop| fit(Oklch::new(stop.l, stop.c, brand.h)).css())
        .collect();
    let ramp_ref: Vec<&str> = ramp.iter().map(String::as_str).collect();
    let negative: Vec<String> = negative.into_iter().map(Oklch::css).collect();
    let negative_ref: Vec<&str> = negative.iter().map(String::as_str).collect();
    let surfaces = [("bg-chart", chart[0]), ("bg", chart[1])];
    if !color::ramp_issues(&ramp_ref, &surfaces).is_empty()
        || (base["chart"]["sign-cue"].as_str() != Some("hatch")
            && !color::diverging_issues(&ramp_ref, &negative_ref).is_empty())
    {
        return None;
    }
    Some(ramp)
}

#[derive(Clone)]
pub(super) struct Derived {
    pub layer: Value,
    pub adjusted: bool,
    pub secondary_skipped: bool,
    pub secondary_adjusted: Option<String>,
    pub palette_failed: bool,
}

// A report is rendered repeatedly while the author edits. The bounded per-thread cache also
// works in Wasm, without sharing one report's result across different base tokens or charts.
const CACHE_LIMIT: usize = 32;
thread_local! {
    static CACHE: RefCell<VecDeque<(String, Option<Derived>)>> = const { RefCell::new(VecDeque::new()) };
}

pub(super) fn derive(base: &Value, primary: &str, secondary: Option<&str>) -> Option<Derived> {
    let key =
        serde_json::to_string(&(base.get("tokens"), base.get("chart"), primary, secondary)).ok()?;
    if let Some(hit) = CACHE.with_borrow_mut(|cache| {
        let index = cache.iter().position(|(candidate, _)| candidate == &key)?;
        let entry = cache.remove(index)?;
        let value = entry.1.clone();
        cache.push_front(entry);
        Some(value)
    }) {
        return hit;
    }
    let result = derive_uncached(base, primary, secondary);
    CACHE.with_borrow_mut(|cache| {
        cache.push_front((key, result.clone()));
        if cache.len() > CACHE_LIMIT {
            cache.pop_back();
        }
    });
    result
}

fn derive_uncached(base: &Value, primary: &str, secondary: Option<&str>) -> Option<Derived> {
    let brand = fit(color::parse(primary)?);
    let dark = base.pointer("/tokens/scheme").and_then(Value::as_str) == Some("dark");
    let grey = brand.c < 0.03;
    let mut colors = std::collections::BTreeMap::new();
    for name in NEUTRALS {
        let old = base
            .pointer(&format!("/tokens/{name}"))
            .and_then(Value::as_str)
            .and_then(color::parse)?;
        let (share, cap) = if name.starts_with("bg") {
            (0.08, 0.012)
        } else if name.starts_with("rule") {
            (0.15, 0.020)
        } else {
            (0.25, 0.035)
        };
        let l = if grey {
            old.l
        } else if !dark && name.starts_with("bg") {
            old.l.min(0.985)
        } else if !dark && !name.starts_with("rule") {
            old.l.max(0.20)
        } else {
            old.l
        };
        let next = if grey {
            old
        } else {
            Oklch::new(l, (brand.c * share).min(cap), brand.h)
        };
        colors.insert(name, fit(next));
    }
    let surfaces: Vec<_> = SURFACES.iter().map(|name| colors[name]).collect();
    for (name, floor) in [
        ("ink", 7.0),
        ("chart-ink", 7.0),
        ("ink-soft", 4.5),
        ("ink-faint", 3.0),
    ] {
        colors.insert(name, reach(colors[name], &surfaces, floor + 0.02, dark));
    }
    let accent = reach(brand, &surfaces, 3.02, dark);
    let link = reach(brand, &surfaces, 4.52, dark);
    let chart = [colors["bg-chart"], colors["bg"]];
    let first = rgb(reach(brand, &chart, 3.02, dark));
    let adjusted = first.hex() != rgb(brand).hex()
        || accent.hex() != rgb(brand).hex()
        || link.hex() != rgb(brand).hex();
    let mut palette = vec![first];
    let mut secondary_skipped = false;
    let mut secondary_adjusted = None;
    if let Some(secondary) = secondary.and_then(color::parse) {
        if let Some(candidate) = second_slot(secondary, first, &chart, dark) {
            if candidate.hex() != rgb(secondary).hex() {
                secondary_adjusted = Some(candidate.hex());
            }
            palette.push(candidate);
        } else {
            secondary_skipped = true;
        }
    }
    let base_palette: Vec<Oklch> = match &base["chart"]["palette"] {
        Value::String(name) => richview_chart::rmd_render::palette(name)
            .iter()
            .filter_map(|s| color::parse(s))
            .collect(),
        Value::Array(values) => values
            .iter()
            .filter_map(Value::as_str)
            .filter_map(color::parse)
            .collect(),
        _ => Vec::new(),
    };
    let mut selected = palette_from(&palette, &chart, &base_palette, dark);
    if selected.is_none() && palette.len() > 1 {
        secondary_skipped = true;
        secondary_adjusted = None;
        selected = palette_from(&palette[..1], &chart, &base_palette, dark);
    }
    let mut tokens = Map::new();
    for (name, value) in colors {
        tokens.insert(name.into(), json!(value.css()));
    }
    tokens.insert("accent".into(), json!(accent.css()));
    tokens.insert(
        "accent-soft".into(),
        json!(format!("{} / 0.12)", accent.css().trim_end_matches(')'))),
    );
    tokens.insert("link".into(), json!(link.css()));
    let palette_failed = selected.is_none();
    let mut layer = json!({"tokens": tokens});
    if let Some(palette) = selected {
        layer["chart"] =
            json!({"palette": palette.into_iter().map(Oklch::hex).collect::<Vec<_>>()});
    }
    if let Some(ramp) = brand_ramp(brand, base, &chart) {
        if !layer["chart"].is_object() {
            layer["chart"] = json!({});
        }
        layer["chart"]["ramp"] = json!(ramp);
    }
    Some(Derived {
        layer,
        adjusted,
        secondary_skipped,
        secondary_adjusted,
        palette_failed,
    })
}
