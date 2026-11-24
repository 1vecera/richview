//! Heat in table cells (FEA-RP-69): the shade a numeric column's cells take by value. The colours
//! are the style's own heat colours (`chart.ramp` and `chart.negative-ramp`), mixed into the page
//! only as far as the page's ink keeps its contrast floor on them (STY-25), so the number a cell
//! prints is always the carrier and the shade never hides it. A cell names its strength as a
//! whole percent (`--rv-heat`) of the strongest shade, computed from the value the cell shows and
//! never from a finer one (FEA-CH-37).

use crate::style::ResolvedStyle;
use richview_chart::color::{self, Oklch};
use serde_json::Value;

/// The contrast `ink` keeps on the strongest shade: STY-25's floor for text, with room for the
/// rounding of the colour's CSS text.
const INK_FLOOR: f64 = 7.05;

/// The rules of heat cells. The shade is the strongest one at the cell's share, over whatever the
/// cell stands on; paper keeps it.
pub(crate) const HEAT_CSS: &str = ".rv-report__body td.rv-heat{background-color:color-mix(in oklch,var(--rv-heat-up) var(--rv-heat,0%),transparent);-webkit-print-color-adjust:exact;print-color-adjust:exact}\n.rv-report__body td.rv-heat--neg{background-color:color-mix(in oklch,var(--rv-heat-down) var(--rv-heat,0%),transparent)}\n";

/// The strongest shade toward one of `stops` on `page`: the stop mixed into the page as far as
/// `ink` keeps its floor, and of those the one that stands out most from the page.
pub(crate) fn strongest(page: Oklch, ink: Oklch, stops: &[Oklch]) -> Option<Oklch> {
    stops
        .iter()
        .map(|stop| {
            let at = |t: f64| color::mix(page, *stop, t);
            if color::contrast(ink, at(1.0)) >= INK_FLOOR {
                return at(1.0);
            }
            // The page itself holds the floor, so the furthest mix that does is found by halving.
            let (mut low, mut high) = (0.0, 1.0);
            for _ in 0..24 {
                let mid = (low + high) / 2.0;
                if color::contrast(ink, at(mid)) >= INK_FLOOR {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            at(low)
        })
        .max_by(|a, b| color::delta_e(page, *a).total_cmp(&color::delta_e(page, *b)))
}

/// The two strongest shades of a style, for values above and below a diverging scale's zero.
pub(crate) fn shades(style: &ResolvedStyle) -> Option<(Oklch, Oklch)> {
    let token = |name: &str| {
        style
            .custom_properties
            .get(&format!("--rv-{name}"))
            .and_then(|css| color::parse(css))
    };
    let stops = |key: &str| -> Vec<Oklch> {
        style
            .chart
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(color::parse)
            .collect()
    };
    let (page, ink) = (token("bg")?, token("ink")?);
    let up = strongest(page, ink, &stops("ramp"))
        .or_else(|| strongest(page, ink, &token("accent").into_iter().collect::<Vec<_>>()))?;
    let down = strongest(page, ink, &stops("negative-ramp"))
        .or_else(|| strongest(page, ink, &token("down").into_iter().collect::<Vec<_>>()))
        .unwrap_or(up);
    Some((up, down))
}

/// The least distance between the two arms' shades at which a reader tells them apart by colour.
const ARMS_APART: f64 = 0.04;

/// A style whose two arms are one colour, as Hand-drawn's graphite is, marks the negative arm
/// with a hatch instead, as its charts do: lines of the shade over a lighter fill, so the
/// darkest point behind a number is still the shade the ink keeps its floor on.
const HATCH_CSS: &str = ".rv-report__body td.rv-heat--neg{background-color:color-mix(in oklch,var(--rv-heat-down) calc(var(--rv-heat,0%)*.35),transparent);background-image:repeating-linear-gradient(135deg,color-mix(in oklch,var(--rv-heat-down) var(--rv-heat,0%),transparent) 0 1px,transparent 1px 5px)}";
const NO_HATCH_CSS: &str = ".rv-report__body td.rv-heat--neg{background-color:color-mix(in oklch,var(--rv-heat-down) var(--rv-heat,0%),transparent);background-image:none}";

/// Whether a style's negative arm needs the hatch to differ from its positive one.
pub(crate) fn hatched(style: &ResolvedStyle) -> bool {
    shades(style).is_some_and(|(up, down)| color::delta_e(up, down) < ARMS_APART)
}

/// The rules of heat cells and the custom properties they read, on screen and, when the page
/// prints with another cascade, on paper.
pub(crate) fn css(screen: &ResolvedStyle, paper: Option<&ResolvedStyle>) -> String {
    let declare = |style: &ResolvedStyle| {
        shades(style)
            .map(|(up, down)| format!("--rv-heat-up:{};--rv-heat-down:{}", up.css(), down.css()))
    };
    let mut css = String::new();
    if let Some(declarations) = declare(screen) {
        css.push_str(&format!(":root{{{declarations}}}\n"));
    }
    css.push_str(HEAT_CSS);
    if hatched(screen) {
        css.push_str(HATCH_CSS);
        css.push('\n');
    }
    if let Some(paper) = paper
        && let Some(declarations) = declare(paper)
    {
        let arm = match (hatched(screen), hatched(paper)) {
            (false, true) => HATCH_CSS,
            (true, false) => NO_HATCH_CSS,
            _ => "",
        };
        css.push_str(&format!("@media print{{:root{{{declarations}}}{arm}}}\n"));
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_strongest_shade_keeps_the_ink_floor_and_leaves_the_page() {
        let page = Oklch::new(0.99, 0.0, 0.0);
        let ink = Oklch::new(0.2, 0.0, 0.0);
        let ramp = [
            Oklch::new(0.94, 0.012, 255.0),
            Oklch::new(0.67, 0.08, 256.0),
            Oklch::new(0.37, 0.115, 258.0),
        ];
        let shade = strongest(page, ink, &ramp).expect("a shade");
        assert!(color::contrast(ink, shade) >= 7.0);
        assert!(color::delta_e(page, shade) > 0.1, "a shade a reader sees");
        // A dark page with pale ink is capped from the other side.
        let (page, ink) = (Oklch::new(0.2, 0.02, 262.0), Oklch::new(0.96, 0.0, 0.0));
        let shade = strongest(page, ink, &[Oklch::new(0.80, 0.10, 255.0)]).expect("a shade");
        assert!(color::contrast(ink, shade) >= 7.0);
        assert!(shade.l > page.l);
    }

    #[test]
    fn ink_keeps_its_floor_on_both_shades_in_every_style_on_screen_and_paper() {
        let mut cascades: Vec<String> = crate::style::builtin_style_names()
            .iter()
            .chain(&["midnight", "chalk"])
            .map(|name| format!("preset: {name}\n"))
            .collect();
        cascades.push("preset: air\ncolors:\n  brand: \"#8E2C3A\"\n".into());
        cascades.push(
            "preset: scientific\ncolors:\n  brand: \"#E0A040\"\n  background: \"#0B1630\"\n".into(),
        );
        let hand_drawn =
            crate::style::resolve_styles(&["preset: hand-drawn\n".into()], &mut vec![]);
        assert!(css(&hand_drawn, None).contains("repeating-linear-gradient"));
        let sleek = crate::style::resolve_styles(&["preset: sleek\n".into()], &mut vec![]);
        assert!(!css(&sleek, None).contains("repeating-linear-gradient"));
        for source in cascades {
            let refs = vec![source.clone()];
            let screen = crate::style::resolve_styles(&refs, &mut vec![]);
            let (paper, _) = crate::style::print_cascade(&refs, &screen);
            for (medium, style) in [("screen", Some(&screen)), ("paper", paper.as_ref())] {
                let Some(style) = style else { continue };
                let token = |name: &str| {
                    color::parse(&style.custom_properties[&format!("--rv-{name}")]).unwrap()
                };
                let (page, ink) = (token("bg"), token("ink"));
                let (up, down) = shades(style).unwrap_or_else(|| panic!("{source}: no shades"));
                for (arm, shade) in [("up", up), ("down", down)] {
                    // The shade is read back from its CSS text, as a browser reads it.
                    let shade = color::parse(&shade.css()).unwrap();
                    let contrast = color::contrast(ink, shade);
                    assert!(
                        contrast >= 7.0,
                        "{source:?} {medium} {arm}: ink {contrast:.2}:1"
                    );
                    assert!(
                        color::delta_e(page, shade) >= 0.04,
                        "{source:?} {medium} {arm}: the shade is too close to the page"
                    );
                }
                // Arms of one colour differ by the hatch.
                assert_eq!(
                    hatched(style),
                    color::delta_e(up, down) < ARMS_APART,
                    "{source:?} {medium}"
                );
            }
        }
    }
}
