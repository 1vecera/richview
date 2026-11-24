//! Syntax inks from the resolved accent and chart series, measured on the code panel.

use super::{ResolvedStyle, brand};
use richview_chart::{
    color::{self, Oklch},
    layout::ChartTheme,
};
use std::fmt::Write;

// Allow for rounding of the OKLCH declaration and the browser's sRGB compositing.
const CONTRAST: f64 = 4.55;
const HUE_GAP: f64 = 45.1;

fn token(style: &ResolvedStyle, name: &str) -> Option<Oklch> {
    color::parse(style.custom_properties.get(name)?)
}

fn surfaces(style: &ResolvedStyle) -> Option<[Oklch; 2]> {
    let panel = token(style, "--rv-bg-elev")?;
    let link = token(style, "--rv-link")?;
    // report.css puts the link ink at 11% opacity on the elevated panel.
    let bg = panel.srgb();
    let tint = link.srgb();
    let marked = Oklch::from_srgb(std::array::from_fn(|i| bg[i] * 0.89 + tint[i] * 0.11));
    Some([panel, marked])
}

fn coloured(seed: Oklch, surfaces: &[Oklch], dark: bool) -> Option<Oklch> {
    let floor = if dark { 0.072 } else { 0.092 };
    let seed = Oklch::new(seed.l, seed.c.max(floor + 0.002), seed.h);
    let safe = |ink: Oklch| {
        [0.0, 0.001, 0.002]
            .into_iter()
            .filter_map(|trim| color::parse(&Oklch::new(ink.l, ink.c - trim, ink.h).css()))
            .find(|ink| {
                let displayed = color::parse(&ink.hex()).expect("generated hex is a colour");
                ink.c >= floor
                    && ink.in_gamut()
                    && displayed.c >= floor - 0.001
                    && surfaces
                        .iter()
                        .all(|&surface| color::contrast(displayed, surface) >= CONTRAST)
            })
    };
    let ink = brand::reach(seed, surfaces, CONTRAST, dark);
    if let Some(ink) = safe(ink) {
        return Some(ink);
    }
    // At the edge of sRGB, moving toward the page can restore the required chroma.
    for step in 1..=200 {
        let offset = f64::from(step) * 0.005;
        for lightness in [seed.l - offset, seed.l + offset] {
            if (0.0..=1.0).contains(&lightness) {
                let candidate = brand::fit(Oklch::new(lightness, seed.c, seed.h));
                if let Some(candidate) = safe(candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn rotated(seed: Oklch) -> impl Iterator<Item = Oklch> {
    (1..40).map(move |step| {
        Oklch::new(
            seed.l,
            seed.c,
            (seed.h + brand::GOLDEN_ANGLE * f64::from(step)).rem_euclid(360.0),
        )
    })
}

fn inks(
    style: &ResolvedStyle,
    palette: &[String],
    surfaces: &[Oklch],
    dark: bool,
) -> Option<[Oklch; 3]> {
    // Hand-drawn uses the same graphite scale for code that it uses for charts.
    if style.chart["sign-cue"].as_str() == Some("hatch") {
        return None;
    }
    let accent = token(style, "--rv-accent")?;
    let down = token(style, "--rv-down")?;
    let hold = token(style, "--rv-hold")?;
    let series = || palette.iter().filter_map(|css| color::parse(css));
    let adapt = |seed| coloured(seed, surfaces, dark);
    let key = std::iter::once(accent)
        .chain(series())
        .filter(|seed| seed.c >= 0.001)
        .find_map(adapt)?;
    let warm = |includes: fn(f64) -> bool| {
        series()
            .chain([down, hold])
            .chain(rotated(down))
            .filter(|seed| includes(seed.h) && brand::hue_gap(seed.h, key.h) >= HUE_GAP)
            .find_map(adapt)
    };
    let string = warm(|hue| (20.0..=85.0).contains(&hue))
        .or_else(|| warm(|hue| hue <= 100.0 || hue >= 350.0))?;
    let literal = series()
        .chain(rotated(series().next()?))
        .filter(|seed| {
            [key, string]
                .iter()
                .all(|ink| brand::hue_gap(seed.h, ink.h) >= HUE_GAP)
        })
        .find_map(adapt)?;
    Some([key, string, literal])
}

fn declarations(style: &ResolvedStyle, palette: &[String]) -> String {
    let Some(surfaces) = surfaces(style) else {
        return String::new();
    };
    let dark = brand::is_dark(surfaces[0]);
    let names = ["--rv-code-key", "--rv-code-string", "--rv-code-literal"];
    let mut css = String::new();
    if let Some(inks) = inks(style, palette, &surfaces, dark) {
        for (name, ink) in names.into_iter().zip(inks) {
            let _ = write!(css, "{name}:{};", ink.css());
        }
    } else {
        for name in names {
            let _ = write!(css, "{name}:initial;");
        }
    }
    if let Some(soft) = token(style, "--rv-ink-soft") {
        let ink = brand::reach(soft, &surfaces, CONTRAST, dark);
        let _ = write!(css, "--rv-code-soft:{};", ink.css());
    }
    css
}

pub(crate) fn css(
    screen: &ResolvedStyle,
    screen_palette: &[String],
    paper: Option<(&ResolvedStyle, &ChartTheme)>,
) -> String {
    let screen = declarations(screen, screen_palette);
    let mut css = format!(":root{{{screen}}}\n");
    if let Some((paper, theme)) = paper {
        let paper = declarations(paper, &theme.palette);
        let _ = writeln!(css, "@media print{{:root{{{paper}}}}}");
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        figure::chart_theme,
        style::{print_cascade, resolve_styles},
    };

    fn check(style: &ResolvedStyle, has_colour: bool) {
        let theme = chart_theme(style);
        let surfaces = surfaces(style).expect("code surfaces");
        let dark = brand::is_dark(surfaces[0]);
        let inks = inks(style, &theme.palette, &surfaces, dark);
        if !has_colour {
            assert!(inks.is_none(), "Hand-drawn keeps graphite code");
            return;
        }
        let inks = inks.expect("three syntax inks");
        let inks = inks.map(|ink| color::parse(&ink.hex()).unwrap());
        for (i, ink) in inks.iter().enumerate() {
            assert!(ink.c >= if dark { 0.07 } else { 0.09 }, "{ink:?}");
            for &surface in &surfaces {
                assert!(
                    color::contrast(*ink, surface) >= 4.5,
                    "{ink:?} on {surface:?}"
                );
            }
            for other in &inks[..i] {
                assert!(
                    brand::hue_gap(ink.h, other.h) >= 45.0,
                    "{ink:?} and {other:?}"
                );
            }
        }
    }

    #[test]
    fn syntax_inks_hold_contrast_chroma_and_hue_separation_on_screen_and_paper() {
        for &preset in crate::builtin_style_names() {
            for layer in [
                "",
                "colors:\n  background: '#182033'\n",
                "colors:\n  background: '#FAF8F3'\n",
                "colors:\n  brand: '#A43F17'\n  secondary: '#166B57'\n",
                "colors:\n  brand: '#696969'\n  background: '#182033'\n",
            ] {
                let refs = [preset.to_owned(), layer.to_owned()];
                let screen = resolve_styles(&refs, &mut Vec::new());
                check(&screen, preset != "hand-drawn");
                let (paper, _) = print_cascade(&refs, &screen);
                check(paper.as_ref().unwrap_or(&screen), preset != "hand-drawn");
            }
        }
    }
}
