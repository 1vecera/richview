//! Derive every colour of a style from the author's colours over a preset (STY-71, STY-73):
//! surfaces, inks, rules, accent, link, status colours, a six-slot series palette and ramps, each
//! moved only as far as its contrast floor and the palette rule need.
use super::settings::{GRIDLINES, MEDIUM, TINT};
use richview_chart::color::{self, Oklch, Vision};
use serde::Serialize;
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
pub(super) fn fit(mut color: Oklch) -> Oklch {
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
pub(super) fn reach(original: Oklch, surfaces: &[Oklch], floor: f64, dark: bool) -> Oklch {
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

pub(super) fn hue_gap(a: f64, b: f64) -> f64 {
    ((a - b + 540.0).rem_euclid(360.0) - 180.0).abs()
}

pub(super) const GOLDEN_ANGLE: f64 = 137.508;

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
        let hue = (first.h + GOLDEN_ANGLE * f64::from(step)).rem_euclid(360.0);
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
    /// The page colour, as `#rrggbb`, when it had to move so text can reach 7:1.
    pub background_adjusted: Option<String>,
}

/// The colours an author brings (STY-73): a brand colour, an optional second series colour and an
/// optional page colour. Each is any opaque colour the closed grammar accepts.
#[derive(Clone, Copy, Default, Serialize)]
pub(super) struct Colours<'a> {
    pub brand: Option<&'a str>,
    pub secondary: Option<&'a str>,
    pub background: Option<&'a str>,
}

// A report is rendered repeatedly while the author edits. The bounded per-thread cache also
// works in Wasm, without sharing one report's result across different base tokens or charts.
const CACHE_LIMIT: usize = 32;
thread_local! {
    static CACHE: RefCell<VecDeque<(String, Option<Derived>)>> = const { RefCell::new(VecDeque::new()) };
}

/// Derive every colour of a style from the author's colours over `base`, the merged cascade
/// (STY-73). `tint` and `gridlines` are five-step indices. With no colours and both steps at
/// medium nothing changes, and the function returns `None`.
pub(super) fn derive_colours(
    base: &Value,
    colours: Colours,
    tint: usize,
    gridlines: usize,
) -> Option<Derived> {
    let key = serde_json::to_string(&(
        base.get("tokens"),
        base.get("chart"),
        colours,
        tint,
        gridlines,
    ))
    .ok()?;
    if let Some(hit) = CACHE.with_borrow_mut(|cache| {
        let index = cache.iter().position(|(candidate, _)| candidate == &key)?;
        let entry = cache.remove(index)?;
        let value = entry.1.clone();
        cache.push_front(entry);
        Some(value)
    }) {
        return hit;
    }
    let result = derive_uncached(base, colours, tint, gridlines);
    CACHE.with_borrow_mut(|cache| {
        cache.push_front((key, result.clone()));
        if cache.len() > CACHE_LIMIT {
            cache.pop_back();
        }
    });
    result
}

/// The faintest the chart's soft rules get at `gridlines: xsmall`: they also draw gauge tracks
/// and the cells of a map or calendar that have no data, which must stay visible.
const GRID_FLOOR: f64 = 1.04;

const WHITE: Oklch = Oklch::new(1.0, 0.0, 0.0);
const BLACK: Oklch = Oklch::new(0.0, 0.0, 0.0);

/// Whether a page colour makes a dark scheme: white text stands out on it more than black.
pub(super) fn is_dark(page: Oklch) -> bool {
    color::contrast(WHITE, page) > color::contrast(BLACK, page)
}

/// The colour on `page`'s side of `toward` whose contrast with `page` is closest to `ratio`,
/// keeping hue and chroma: rules and gridlines keep the preset's strength on a new page.
fn at_contrast(page: Oklch, ratio: f64, chroma: f64, hue: f64, dark: bool) -> Oklch {
    let (mut near, mut far) = (page.l, if dark { 1.0 } else { 0.0 });
    for _ in 0..40 {
        let middle = (near + far) / 2.0;
        if color::contrast(fit(Oklch::new(middle, chroma, hue)), page) >= ratio {
            far = middle;
        } else {
            near = middle;
        }
    }
    fit(Oklch::new(far, chroma, hue))
}

/// A sequential ramp at `hue`: its low stop just beyond the surfaces, its high stop at 3:1,
/// lightness moving steadily away from every surface.
fn generate_ramp(hue: f64, surfaces: &[Oklch], dark: bool) -> Vec<Oklch> {
    let edge = if dark {
        surfaces.iter().map(|s| s.l).fold(0.0, f64::max) + 0.07
    } else {
        surfaces.iter().map(|s| s.l).fold(1.0, f64::min) - 0.035
    };
    let low = fit(Oklch::new(edge.clamp(0.02, 0.98), 0.03, hue));
    let start = Oklch::new(if dark { 0.72 } else { 0.48 }, 0.15, hue);
    let mut high = reach(start, surfaces, 3.2, dark);
    if (dark && high.l <= low.l + 0.2) || (!dark && high.l >= low.l - 0.2) {
        high = fit(Oklch::new(
            if dark { low.l + 0.45 } else { low.l - 0.45 },
            0.15,
            hue,
        ));
    }
    let middle = fit(Oklch::new((low.l + high.l) / 2.0, 0.12, hue));
    vec![low, middle, high]
}

fn ramp_passes(ramp: &[String], negative: &[String], surfaces: &[Oklch], hatch: bool) -> bool {
    let ramp: Vec<&str> = ramp.iter().map(String::as_str).collect();
    let negative: Vec<&str> = negative.iter().map(String::as_str).collect();
    let named = [("bg-chart", surfaces[0]), ("bg", surfaces[1])];
    color::ramp_issues(&ramp, &named).is_empty()
        && (negative.is_empty() || color::ramp_issues(&negative, &named).is_empty())
        && (hatch || negative.is_empty() || color::diverging_issues(&ramp, &negative).is_empty())
}

/// `oklch()` text precise enough that a colour kept as given returns to its own `#rrggbb`.
fn fine_css(c: Oklch) -> String {
    let trim = |v: f64, decimals: usize| {
        let text = format!("{v:.decimals$}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    format!(
        "oklch({} {} {})",
        trim(c.l, 5),
        trim(c.c, 5),
        trim(if c.c < 1e-4 { 0.0 } else { c.h }, 3)
    )
}

/// The page, raised and chart surfaces for a page colour: the preset's lightness steps, turned
/// toward the middle when the page changes scheme, in the page's own hue and chroma.
fn page_surfaces(base: &Value, page: Oklch, dark: bool) -> Option<[Oklch; 3]> {
    let token = |name: &str| {
        base.pointer(&format!("/tokens/{name}"))
            .and_then(Value::as_str)
            .and_then(color::parse)
    };
    let preset_dark = base.pointer("/tokens/scheme").and_then(Value::as_str) == Some("dark");
    let (raised, chart) = if dark != preset_dark {
        if dark { (0.045, 0.03) } else { (-0.023, 0.0) }
    } else {
        let bg = token("bg")?;
        (token("bg-elev")?.l - bg.l, token("bg-chart")?.l - bg.l)
    };
    let at = |delta: f64| fit(Oklch::new((page.l + delta).clamp(0.0, 1.0), page.c, page.h));
    Some([page, at(raised), at(chart)])
}

/// Whether the scheme's darkest (or lightest) text reaches 7:1 on every surface of a page.
fn text_fits(base: &Value, page: Oklch, dark: bool) -> bool {
    let ink = if dark { WHITE } else { BLACK };
    page_surfaces(base, page, dark)
        .is_some_and(|surfaces| surfaces.iter().all(|s| color::contrast(ink, *s) >= 7.1))
}

fn derive_uncached(
    base: &Value,
    colours: Colours,
    tint: usize,
    gridlines: usize,
) -> Option<Derived> {
    let Some(text) = colours.background else {
        return derive_at(base, colours, tint, gridlines, None);
    };
    // The author's page stays exact unless the scheme's text cannot reach 7:1 on it, or four
    // distinct series cannot reach 3:1; then it moves, keeping its hue, only as far as they need.
    let page = fit(color::parse(text)?);
    let dark = is_dark(page);
    let toward = if dark { -1.0 } else { 1.0 };
    let mut l = page.l;
    while !text_fits(base, fit(Oklch::new(l, page.c, page.h)), dark) && (0.0..=1.0).contains(&l) {
        l += toward * 0.005;
    }
    loop {
        let l_now = l.clamp(0.0, 1.0);
        let candidate = if (l_now - page.l).abs() < 1e-9 {
            page
        } else {
            fit(Oklch::new(l_now, page.c, page.h))
        };
        let mut derived = derive_at(base, colours, tint, gridlines, Some(candidate))?;
        let at_edge = if dark { l_now <= 0.0 } else { l_now >= 1.0 };
        if !derived.palette_failed || at_edge {
            if (candidate.l - page.l).abs() > 1e-9 {
                derived.background_adjusted = Some(candidate.hex());
            }
            return Some(derived);
        }
        l = l_now + toward * 0.02;
    }
}

fn derive_at(
    base: &Value,
    colours: Colours,
    tint: usize,
    gridlines: usize,
    background: Option<Oklch>,
) -> Option<Derived> {
    let brand = match colours.brand {
        Some(text) => Some(fit(color::parse(text)?)),
        None => None,
    };
    let css = fine_css;
    let secondary = colours.secondary.and_then(color::parse);
    if brand.is_none()
        && background.is_none()
        && secondary.is_none()
        && tint == MEDIUM
        && gridlines == MEDIUM
    {
        return None;
    }
    let preset_dark = base.pointer("/tokens/scheme").and_then(Value::as_str) == Some("dark");
    let dark = background.map_or(preset_dark, is_dark);
    let flipped = dark != preset_dark;
    let tint_factor = TINT[tint];
    let token = |name: &str| {
        base.pointer(&format!("/tokens/{name}"))
            .and_then(Value::as_str)
            .and_then(color::parse)
    };
    let mut old = std::collections::BTreeMap::new();
    for name in NEUTRALS {
        old.insert(name, token(name)?);
    }
    // The hue the neutrals lean to: the author's page when it has a hue, so text and rules
    // belong to it; otherwise the brand's (STY-71).
    let tinted = background
        .filter(|page| page.c >= 0.02)
        .or(brand.filter(|brand| brand.c >= 0.03));
    let mut colors = std::collections::BTreeMap::new();
    for name in NEUTRALS {
        let previous = old[name];
        let (share, cap) = if name.starts_with("bg") {
            (0.08, 0.012)
        } else if name.starts_with("rule") {
            (0.15, 0.020)
        } else {
            (0.25, 0.035)
        };
        let next = if let Some(brand) = tinted {
            let l = if !dark && name.starts_with("bg") {
                previous.l.min(0.985)
            } else if !dark && !name.starts_with("rule") {
                previous.l.max(0.20)
            } else {
                previous.l
            };
            Oklch::new(l, (brand.c * share).min(cap) * tint_factor, brand.h)
        } else if tint == MEDIUM {
            previous
        } else {
            Oklch::new(previous.l, previous.c * tint_factor, previous.h)
        };
        colors.insert(name, fit(next));
    }
    if let Some(page) = background {
        // The page as given (or as moved above); raised and chart surfaces keep the preset's
        // lightness steps.
        let [page, raised, chart] = page_surfaces(base, page, dark)?;
        colors.insert("bg", page);
        colors.insert("bg-elev", raised);
        colors.insert("bg-chart", chart);
        // Rules keep the preset's strength over its page.
        for name in ["rule", "rule-soft"] {
            let ratio = color::contrast(old[name], old["bg"]).max(1.05);
            let lean = colors[name];
            colors.insert(name, at_contrast(page, ratio, lean.c, lean.h, dark));
        }
        if flipped {
            // The preset's inks sit on the wrong side of the new page: start from the scheme's own.
            let start = if dark {
                [
                    ("ink", 0.95),
                    ("chart-ink", 0.99),
                    ("ink-soft", 0.76),
                    ("ink-faint", 0.56),
                ]
            } else {
                [
                    ("ink", 0.16),
                    ("chart-ink", 0.10),
                    ("ink-soft", 0.40),
                    ("ink-faint", 0.56),
                ]
            };
            for (name, l) in start {
                let lean = colors[name];
                colors.insert(name, fit(Oklch::new(l, lean.c, lean.h)));
            }
        }
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
    // The chart's soft rules (gridlines, and the tracks and empty cells some charts draw) take
    // their own colour, so the page's light separators keep `rule-soft`.
    let grid = (gridlines != MEDIUM).then(|| {
        let page = colors["bg-chart"];
        let soft = colors["rule-soft"];
        let ratio = color::contrast(soft, page);
        let target = (1.0 + (ratio - 1.0) * GRIDLINES[gridlines]).max(GRID_FLOOR);
        at_contrast(page, target, soft.c, soft.h, dark)
    });
    let chart = [colors["bg-chart"], colors["bg"]];
    let mut tokens = Map::new();
    let mut adjusted = false;
    let mut palette = vec![];
    let mut secondary_skipped = false;
    let mut secondary_adjusted = None;
    let recoloured = brand.is_some() || background.is_some();
    if let Some(brand) = brand {
        let accent = reach(brand, &surfaces, 3.02, dark);
        let link = reach(brand, &surfaces, 4.52, dark);
        let first = rgb(reach(brand, &chart, 3.02, dark));
        adjusted = first.hex() != rgb(brand).hex()
            || accent.hex() != rgb(brand).hex()
            || link.hex() != rgb(brand).hex();
        tokens.insert("accent".into(), json!(css(accent)));
        tokens.insert("link".into(), json!(css(link)));
        palette.push(first);
    } else {
        // The preset's accent and link, moved only as far as the new surfaces need.
        for (name, floor) in [("accent", 3.0), ("link", 4.5)] {
            if let Some(previous) = token(name)
                && surfaces
                    .iter()
                    .any(|surface| color::contrast(previous, *surface) < floor)
            {
                tokens.insert(
                    name.into(),
                    json!(css(reach(previous, &surfaces, floor + 0.02, dark))),
                );
            }
        }
    }
    if let Some(accent) = tokens
        .get("accent")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        tokens.insert(
            "accent-soft".into(),
            json!(format!("{} / 0.12)", accent.trim_end_matches(')'))),
        );
    }
    // Status colours and the wordmark set text: each one that misses its floor on the new
    // surfaces moves only as far as it must. The company derivation never moved them.
    for (name, floor) in [("up", 4.5), ("down", 4.5), ("hold", 4.5), ("brand", 4.5)] {
        if let Some(previous) = token(name)
            && surfaces
                .iter()
                .any(|surface| color::contrast(previous, *surface) < floor)
        {
            tokens.insert(
                name.into(),
                json!(css(reach(previous, &surfaces, floor + 0.02, dark))),
            );
        }
    }
    if background.is_some() {
        tokens.insert("scheme".into(), json!(if dark { "dark" } else { "light" }));
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
    if palette.is_empty()
        && (background.is_some() || secondary.is_some())
        && let Some(seed) = base_palette.first()
    {
        // No brand: the preset's first series leads, moved to the new page.
        palette.push(rgb(reach(*seed, &chart, 3.02, dark)));
    }
    let mut palette_failed = false;
    let mut chosen = None;
    if !palette.is_empty() {
        if let Some(secondary) = secondary {
            if let Some(candidate) = second_slot(secondary, palette[0], &chart, dark) {
                if candidate.hex() != rgb(secondary).hex() {
                    secondary_adjusted = Some(candidate.hex());
                }
                palette.push(candidate);
            } else {
                secondary_skipped = true;
            }
        }
        chosen = palette_from(&palette, &chart, &base_palette, dark);
        if chosen.is_none() && palette.len() > 1 {
            secondary_skipped = true;
            secondary_adjusted = None;
            chosen = palette_from(&palette[..1], &chart, &base_palette, dark);
        }
        palette_failed = chosen.is_none();
    }
    for (name, value) in colors {
        tokens.insert(name.into(), json!(css(value)));
    }
    if let Some(grid) = grid {
        tokens.insert("grid".into(), json!(css(grid)));
    }
    let mut layer = json!({"tokens": tokens});
    if let Some(palette) = chosen {
        layer["chart"] =
            json!({"palette": palette.into_iter().map(Oklch::hex).collect::<Vec<_>>()});
    }
    if recoloured {
        let hatch = base["chart"]["sign-cue"].as_str() == Some("hatch");
        let list = |key: &str| -> Vec<String> {
            base["chart"][key]
                .as_array()
                .map(|stops| {
                    stops
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        let negative = list("negative-ramp");
        // A sequential ramp at `hue` and a negative arm that passes the diverging rule with it:
        // the preset's arm hue when it stands far enough from `hue`, else the first opposite hue
        // that passes.
        let generated = |hue: f64| -> (Option<Vec<String>>, Option<Vec<String>>) {
            let positive: Vec<String> = generate_ramp(hue, &chart, dark)
                .into_iter()
                .map(Oklch::css)
                .collect();
            if !ramp_passes(&positive, &[], &chart, hatch) {
                return (None, None);
            }
            if negative.is_empty() {
                return (Some(positive), None);
            }
            let preset_hue = negative
                .last()
                .and_then(|stop| color::parse(stop))
                .map(|stop| stop.h);
            let arm = preset_hue
                .into_iter()
                .chain([180.0, 150.0, 210.0, 120.0, 240.0].map(|turn| hue + turn))
                .map(|h| h.rem_euclid(360.0))
                .map(|candidate| -> Vec<String> {
                    generate_ramp(candidate, &chart, dark)
                        .into_iter()
                        .map(Oklch::css)
                        .collect()
                })
                .find(|stops| ramp_passes(&positive, stops, &chart, hatch));
            (Some(positive), arm)
        };
        let (ramp, arm) = match (brand, background) {
            (Some(brand), None) => match brand_ramp(brand, base, &chart) {
                Some(ramp) => (Some(ramp), None),
                // The preset's ramp re-hued fails the diverging rule (a green brand against an
                // orange arm, say): a brand-hue pair that passes, or the preset's own pair.
                None if brand.c >= 0.03 => match generated(brand.h) {
                    (Some(ramp), Some(arm)) => (Some(ramp), Some(arm)),
                    (Some(ramp), None) if hatch => (Some(ramp), None),
                    _ => (None, None),
                },
                None => (None, None),
            },
            _ => generated(
                brand
                    .filter(|brand| brand.c >= 0.03)
                    .map(|brand| brand.h)
                    .or_else(|| {
                        list("ramp")
                            .last()
                            .and_then(|stop| color::parse(stop))
                            .map(|stop| stop.h)
                    })
                    .unwrap_or(262.0),
            ),
        };
        if let Some(arm) = arm {
            if !layer["chart"].is_object() {
                layer["chart"] = json!({});
            }
            layer["chart"]["negative-ramp"] = json!(arm);
        }
        if let Some(ramp) = ramp {
            if !layer["chart"].is_object() {
                layer["chart"] = json!({});
            }
            layer["chart"]["ramp"] = json!(ramp);
        }
    }
    Some(Derived {
        layer,
        adjusted,
        secondary_skipped,
        secondary_adjusted,
        palette_failed,
        background_adjusted: None,
    })
}
