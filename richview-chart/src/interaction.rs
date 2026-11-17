//! Chart interaction policy that every host shares: where a measured readout sits beside its
//! mark (FEA-CH-42), when the expanded chart turns on a phone (FEA-CH-68), and the chart actions'
//! order, icons and markup (FEA-CH-43).

/// The side of its observation a tooltip sits on. Remembering it lets the host keep the
/// tooltip on one side while datum widths vary under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooltipSide {
    pub right: bool,
    pub below: bool,
}

const TIP_GAP: f64 = 12.0;
/// The least distance between a readout and the chart's edges (FEA-CH-42).
pub const TIP_EDGE: f64 = 8.0;

/// The media query for a phone held upright, where the expanded chart is laid out in landscape
/// and turned a quarter (FEA-CH-68): a portrait screen with a coarse pointer, at most 600 px wide
/// and 1024 px tall, so tablets, including narrow split views, stay upright. Below 360 px the
/// turned dialog's height could not hold a plot's least height under its title and actions, so
/// the narrowest phones stay upright too. Paper never turns. A macro, so the figure styles can
/// `concat!` it.
#[macro_export]
macro_rules! turn_query {
    () => {
        "screen and (orientation:portrait) and (pointer:coarse) and (min-width:360px) and (max-width:600px) and (max-height:1024px)"
    };
}

/// [`turn_query!`] for hosts that test it at run time.
pub const TURN_QUERY: &str = turn_query!();

/// The media query for a short, wide window, whose expanded dialog is already the shape a turned
/// one takes (FEA-CH-68): landscape, at most 600 px tall and at most 1024 px wide in CSS pixels,
/// whatever the pointer, so a phone held sideways and a laptop zoomed to 200 % both match
/// (S27-871, S27-876). Its dialog takes the turned one's layout without the turn.
pub const SHORT_WIDE_QUERY: &str = "screen and (orientation:landscape) and (max-height:600px) and (max-width:1024px)";

/// Pick the positive (right or below) or negative side along one axis. The previous side is kept
/// while it still fits; the other side is taken only when the previous one does not fit and it
/// does. When neither fits, the previous side stays, or the roomier side is chosen at first.
fn tooltip_axis_side(anchor: f64, reach: [f64; 2], size: f64, bound: f64, previous: Option<bool>) -> bool {
    let fits_positive = anchor + reach[1] + TIP_GAP + size <= bound - TIP_EDGE;
    let fits_negative = anchor - reach[0] - TIP_GAP - size >= TIP_EDGE;
    let fits = |positive: bool| if positive { fits_positive } else { fits_negative };
    let preferred = previous.unwrap_or(true);
    if fits(preferred) {
        preferred
    } else if fits(!preferred) {
        !preferred
    } else {
        previous.unwrap_or(bound - TIP_EDGE - (anchor + reach[1] + TIP_GAP) >= anchor - reach[0] - TIP_GAP - TIP_EDGE)
    }
}

/// Place a measured tooltip beside a mark that reaches `reach[0]` left and `reach[1]` right of
/// its anchor, such as a bar: the tooltip starts past the mark's edge, so it never covers the
/// mark it describes (FEA-CH-42).
pub fn tooltip_beside(
    anchor: [f64; 2],
    reach: [f64; 2],
    size: [f64; 2],
    bounds: [f64; 2],
    previous: Option<TooltipSide>,
) -> ([f64; 2], TooltipSide) {
    let reach = [reach[0].max(0.0), reach[1].max(0.0)];
    let right = tooltip_axis_side(anchor[0], reach, size[0], bounds[0], previous.map(|side| side.right));
    let below = tooltip_axis_side(
        anchor[1],
        [0.0, 0.0],
        size[1],
        bounds[1],
        previous.map(|side| side.below),
    );
    let x = if right {
        anchor[0] + reach[1] + TIP_GAP
    } else {
        anchor[0] - reach[0] - TIP_GAP - size[0]
    };
    let y = if below {
        anchor[1] + TIP_GAP
    } else {
        anchor[1] - TIP_GAP - size[1]
    };
    (
        [
            x.clamp(TIP_EDGE, (bounds[0] - size[0] - TIP_EDGE).max(TIP_EDGE)),
            y.clamp(TIP_EDGE, (bounds[1] - size[1] - TIP_EDGE).max(TIP_EDGE)),
        ],
        TooltipSide { right, below },
    )
}

/// The chart types that zoom along x (FEA-CH-49): those drawn over a continuous or ordered x
/// axis whose marks keep their meaning and the chart its shape in a window. The binned types
/// (histogram, 2D bins, hexbin) stay whole: their bins are the data, and a window would stretch
/// or split them. A contour and a dot plot draw x and y to one scale, so a window along x
/// changes the chart's shape under the reader's fingers; they wait for a zoom along both axes.
pub const ZOOMS_ALONG_X: [&str; 10] = [
    "line",
    "area",
    "scatter",
    "band",
    "density",
    "ecdf",
    "gantt",
    "qq",
    "streamgraph",
    "vector",
];

/// Whether a chart of type `kind` laid out as `geometry` zooms (FEA-CH-49): one of
/// [`ZOOMS_ALONG_X`] whose layout gives an x axis to zoom along, or any chart whose layout gives
/// a second axis, as a map does. Core reserves Reset zoom's place by it and the runtime zooms
/// by it, so the page does not move when the runtime starts.
pub fn zooms(kind: &str, geometry: &crate::geometry::Geometry) -> bool {
    geometry
        .panels
        .first()
        .and_then(|p| p.x.as_ref())
        .is_some_and(|x| x.y.is_some() || ZOOMS_ALONG_X.contains(&kind))
}

/// The chart actions in their order, as `(kind, label)` (FEA-CH-43). Reset zoom joins only while
/// the chart is zoomed.
pub const ACTIONS: [(&str, &str); 4] = [
    ("table", "Table"),
    ("png", "Download PNG"),
    ("csv", "CSV"),
    ("expand", "Expand"),
];

/// A 16-unit stroke icon for an action, drawn in the text colour beside its label.
pub fn action_icon(kind: &str) -> &'static str {
    match kind {
        "table" => "M2 3h12v10H2zM2 6.5h12M2 10h12M6.5 3v10",
        "png" => "M8 2v8M4.5 6.5 8 10l3.5-3.5M2.5 13.5h11",
        "csv" => "M4 2h5l3 3v9H4zM9 2v3h3M6 8.5h4M6 11h4",
        "expand" => "M9.5 2.5h4v4M13.5 2.5 9 7M6.5 13.5h-4v-4M2.5 13.5 7 9",
        "close" => "M3.5 3.5l9 9M12.5 3.5l-9 9",
        _ => "M3 8a5 5 0 1 0 1.5-3.5M3 2.5v2.5h2.5",
    }
}

/// One action button's markup. Core reserves the group with these same buttons, so the runtime
/// fills it without moving anything (FEA-CH-64); `label` must already be HTML-safe.
pub fn action_html(kind: &str, label: &str) -> String {
    format!(
        "<button type=\"button\" class=\"rv-chart__action rv-chart__action--{kind}\"><svg viewBox=\"0 0 16 16\" aria-hidden=\"true\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><path d=\"{}\"/></svg><span>{label}</span></button>",
        action_icon(kind)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A readout beside a point mark, which reaches nowhere past its anchor.
    fn beside(
        anchor: [f64; 2],
        size: [f64; 2],
        bounds: [f64; 2],
        previous: Option<TooltipSide>,
    ) -> ([f64; 2], TooltipSide) {
        tooltip_beside(anchor, [0.0, 0.0], size, bounds, previous)
    }

    #[test]
    fn measured_tooltips_flip_and_stay_inside_narrow_charts() {
        for anchor in [[0.0, 0.0], [320.0, 250.0], [160.0, 125.0]] {
            let [x, y] = beside(anchor, [240.0, 110.0], [320.0, 250.0], None).0;
            assert!(x >= 8.0 && x + 240.0 <= 312.0);
            assert!(y >= 8.0 && y + 110.0 <= 242.0);
        }
        assert!(beside([300.0, 220.0], [90.0, 40.0], [320.0, 250.0], None).0[0] < 300.0);
    }
    #[test]
    fn a_tooltip_beside_a_bar_starts_past_its_edge_on_either_side() {
        // A 60 px bar centred at x = 200: the readout clears its right edge.
        let ([x, _], side) = tooltip_beside([200.0, 100.0], [30.0, 30.0], [150.0, 60.0], [600.0, 300.0], None);
        assert!(side.right && x >= 230.0 + 12.0, "{x}");
        // Near the right edge it flips and clears the bar's left edge instead.
        let ([x, _], side) = tooltip_beside([500.0, 100.0], [30.0, 30.0], [150.0, 60.0], [600.0, 300.0], None);
        assert!(!side.right && x + 150.0 <= 470.0 - 12.0, "{x}");
    }
    #[test]
    fn tooltips_keep_their_side_until_it_stops_fitting() {
        let bounds = [600.0, 300.0];
        let (_, first) = beside([100.0, 100.0], [200.0, 80.0], bounds, None);
        assert!(first.right && first.below);
        // Moving right then back left: it flips once when the right side runs out and then
        // stays on the left until the left side stops fitting, instead of jumping with each
        // datum's width.
        let mut side = Some(first);
        let mut flips = 0;
        for step in 0..=60 {
            let width = if step % 2 == 0 { 120.0 } else { 220.0 };
            let anchor = [100.0 + step as f64 * 7.0, 100.0];
            let (position, next) = beside(anchor, [width, 80.0], bounds, side);
            flips += usize::from(side.is_some_and(|s| s.right != next.right));
            assert!(position[0] >= 8.0 && position[0] + width <= 592.0);
            side = Some(next);
        }
        assert_eq!(flips, 1, "a rightward sweep flips once");
        for step in (0..=60).rev() {
            let width = if step % 2 == 0 { 120.0 } else { 220.0 };
            let anchor = [100.0 + step as f64 * 7.0, 100.0];
            let (_, next) = beside(anchor, [width, 80.0], bounds, side);
            flips += usize::from(side.is_some_and(|s| s.right != next.right));
            side = Some(next);
        }
        assert!(flips <= 2, "the sweep back does not flicker: {flips}");
    }
    #[test]
    fn a_tooltip_that_fits_nowhere_keeps_its_previous_side() {
        let bounds = [320.0, 250.0];
        let kept = TooltipSide {
            right: false,
            below: true,
        };
        let (position, side) = beside([160.0, 125.0], [240.0, 110.0], bounds, Some(kept));
        assert_eq!(side.right, kept.right);
        assert!(position[0] >= 8.0 && position[0] + 240.0 <= 312.0);
    }
}
