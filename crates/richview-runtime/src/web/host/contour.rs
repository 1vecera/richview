//! Selection follows the observation's own mark paths without repainting fills or labels.

use richview_chart::color::{Oklch, parse};
use richview_chart::geometry::{
    Cmd, Fill, Geometry, Item, Panel, Path, Rect, Rgba, Role, Shape, Stroke,
};
use richview_chart::interaction::{Emphasis, emphasis, outlines_from_legend};
use richview_chart::layout::ChartTheme;

/// The filled data mark may be drawn whole at a plot axis.
pub(super) fn dot(center: [f64; 2], level: f64, color: Rgba) -> Item {
    Item::Point {
        center,
        radius: 6.0 * (0.4 + 0.6 * level),
        shape: Shape::Circle,
        fill: Some(Fill::solid(color.with_alpha(color.alpha() * level))),
        stroke: None,
        role: Role::Annotation,
        series: None,
        obs: None,
    }
}

pub(super) fn point_growth(kind: &str, sized: bool) -> f64 {
    match kind {
        "scatter" | "scatter3d" if sized => 0.25,
        "scatter" | "scatter3d" | "strip" | "qq" => 0.6,
        _ => 0.0,
    }
}

const RING_ALPHA: f64 = 0.8;

/// Resolved when laying out the chart. RGB identifies an ink even while its opacity animates.
pub(super) struct RingInks {
    paper: Rgba,
    palette: Vec<String>,
    colours: std::collections::BTreeMap<[u32; 3], Rgba>,
    pub(super) ink: Rgba,
}

impl RingInks {
    fn key(mark: Rgba) -> [u32; 3] {
        [mark.0[0], mark.0[1], mark.0[2]].map(f32::to_bits)
    }

    pub(super) fn new(theme: &ChartTheme, geometry: &Geometry) -> Self {
        let mut inks = Self {
            paper: geometry.background,
            palette: theme.palette.clone(),
            colours: Default::default(),
            ink: Rgba([0.5, 0.5, 0.5, 1.0]),
        };
        for authored in theme.palette.iter().filter_map(|css| parse(css)) {
            inks.register(Rgba::from_oklch(authored, 1.0), Some(authored));
        }
        inks.collect(theme, geometry);
        inks
    }

    fn register(&mut self, mark: Rgba, authored: Option<Oklch>) {
        self.colours
            .entry(Self::key(mark))
            .or_insert_with(|| ring_ink(mark, self.paper, authored));
    }

    fn collect(&mut self, theme: &ChartTheme, geometry: &Geometry) {
        self.ink =
            parse(&theme.ink).map_or(Rgba([0.5, 0.5, 0.5, 1.0]), |c| Rgba::from_oklch(c, 1.0));
        self.register(self.ink, None);
        for series in &geometry.series {
            self.register(series.color, None);
        }
        if matches!(
            geometry.kind.as_str(),
            "bar" | "packed-bubble" | "scatter" | "flow"
        ) {
            for item in geometry
                .panels
                .iter()
                .flat_map(|p| &p.items)
                .filter(|i| i.role() == Role::Mark)
            {
                if let Some(color) = mark_colour(item) {
                    self.register(color, None);
                }
            }
        }
    }

    pub(super) fn refresh(&mut self, theme: &ChartTheme, geometry: &Geometry) {
        if self.paper != geometry.background || self.palette != theme.palette {
            *self = Self::new(theme, geometry);
        } else {
            self.collect(theme, geometry);
        }
    }

    fn get(&self, mark: Rgba) -> Rgba {
        self.colours[&Self::key(mark)]
    }
}

fn mark_colour(mark: &Item) -> Option<Rgba> {
    match mark {
        Item::Point { fill, stroke, .. } => fill
            .map(|f| f.color)
            .or_else(|| stroke.as_ref().map(|s| s.color)),
        Item::Rect { fill, .. } => Some(fill.color),
        Item::Line { stroke, .. } => Some(stroke.color),
        _ => None,
    }
}

pub(super) fn crisp_target(kind: &str, panel: &Panel, observation: usize) -> bool {
    match kind {
        // A flow chart's nodes take the ring and its ribbons do not.
        "flow" => panel.items.iter().any(|item| {
            matches!(item,
            Item::Area { role: Role::Mark, obs: Some(i), .. }
            | Item::Rect { role: Role::Mark, obs: Some(i), .. } if *i == observation)
        }),
        // A scatter matrix's points take the ring. Its diagonal and its correlations are cells
        // with no point, which keep the keyboard's focus outline.
        "splom" => panel.items.iter().any(|item| {
            matches!(item,
            Item::Point { role: Role::Mark, obs: Some(i), .. } if *i == observation)
        }),
        _ => emphasis(kind) == Some(Emphasis::Ring),
    }
}

/// Where the paper between a filled mark and its band ends, outside the mark's edge.
const SEPARATOR: f64 = 1.75;
/// How far a lined mark's band reaches past its separator, and its share of the mark's colour.
const BAND: f64 = 4.0;
const BAND_ALPHA: f64 = 0.25;
/// Where a point's band starts outside the marker, past its paper line and its dark ring.
const POINT_GAP: f64 = 4.0;

/// How far outside its edge a hollow mark takes its lines and band: a bar that is an outline
/// in its colour round a fill of paper, as a rising candle's body is. That outline is all the
/// colour the mark has, so the dark line goes round it, a paper line apart, and not over it.
fn hollow(mark: &Item, paper: Rgba) -> f64 {
    match mark {
        Item::Rect {
            fill,
            stroke: Some(stroke),
            ..
        } if fill.color.alpha() < 0.05
            || (0..3).all(|k| (fill.color.0[k] - paper.0[k]).abs() < 0.02) =>
        {
            stroke.width / 2.0 + SEPARATOR
        }
        _ => 0.0,
    }
}

/// The flat band of a lined mark at `level`: `BAND` CSS pixels of its own colour past the
/// paper separator, `shift` further out round a hollow mark. One filled shape, painted once: a
/// translucent stroke would overlap itself at every join.
fn band(kind: &str, mark: &Item, level: f64, shift: f64) -> Option<Item> {
    let paint = |color: Rgba| color.with_alpha(BAND_ALPHA * color.alpha() * level.clamp(0.0, 1.0));
    let (path, color) = match mark {
        Item::Point {
            center,
            radius,
            shape,
            fill,
            stroke,
            ..
        } => {
            let color = fill.map(|f| f.color).or(stroke.as_ref().map(|s| s.color))?;
            let half = stroke.as_ref().map_or(0.0, |s| s.width / 2.0);
            // A square reaches its corners; the other markers fit inside their radius.
            let extent = match shape {
                Shape::Square => radius * 0.85 * std::f64::consts::SQRT_2,
                _ => *radius,
            };
            let inner = extent + half + POINT_GAP;
            let mut path = Path::polyline(&super::focus::circle_points(*center, inner + BAND));
            path.close();
            path.extend(&Path::polyline(&super::focus::circle_points(
                *center, inner,
            )));
            path.close();
            (path, color)
        }
        Item::Rect {
            rect, fill, stroke, ..
        } => {
            let inner = rect.inflate(shift + SEPARATOR);
            let mut path = Path::new();
            rounded(&mut path, inner, SEPARATOR + BAND);
            rounded(&mut path, inner, SEPARATOR);
            (path, stroke.as_ref().map_or(fill.color, |s| s.color))
        }
        Item::Line { path, stroke, .. }
            if matches!(kind, "boxplot" | "candlestick" | "interval") =>
        {
            let mut band = stroke.clone();
            band.color = paint(stroke.color);
            band.width += 2.0 * (SEPARATOR + BAND);
            return Some(Item::Line {
                path: path.clone(),
                stroke: band,
                role: Role::Annotation,
                series: None,
                obs: None,
            });
        }
        _ => return None,
    };
    Some(Item::Area {
        path,
        fill: Fill::solid(paint(color)),
        role: Role::Annotation,
        series: None,
        obs: None,
    })
}

/// `rect` grown by `reach - SEPARATOR` with corners rounded to `reach`: the outline `reach`
/// away from a rectangle whose own corners are a separator inside it.
fn rounded(path: &mut Path, rect: Rect, reach: f64) {
    let r = rect.inflate(reach - SEPARATOR);
    let radius = reach.min(r.w / 2.0).min(r.h / 2.0);
    let corners = [
        ([r.right() - radius, r.y + radius], -90.0_f64),
        ([r.right() - radius, r.bottom() - radius], 0.0),
        ([r.x + radius, r.bottom() - radius], 90.0),
        ([r.x + radius, r.y + radius], 180.0),
    ];
    let mut first = true;
    for (c, from) in corners {
        for k in 0..=6 {
            let a = (from + 90.0 * f64::from(k) / 6.0).to_radians();
            let point = [c[0] + radius * a.cos(), c[1] + radius * a.sin()];
            if first {
                path.move_to(point);
                first = false;
            } else {
                path.line_to(point);
            }
        }
    }
    path.close();
}

/// The tiles a flush chart outlines for `observation`: the bars and areas it owns, or a
/// gauge's band at the needle. None for an observation that is a point among the tiles, as a
/// map's spot is, which keeps the answer of `items`.
fn flush_tiles<'a>(kind: &str, panel: &'a Panel, observation: usize) -> Option<Vec<&'a Item>> {
    if kind == "gauge" {
        return gauge_band(panel, observation).map(|band| vec![band]);
    }
    let owns = |obs: &Option<usize>, role: &Role| *role == Role::Mark && *obs == Some(observation);
    if panel
        .items
        .iter()
        .any(|item| matches!(item, Item::Point { obs, role, .. } if owns(obs, role)))
    {
        return None;
    }
    let tiles: Vec<&Item> = panel
        .items
        .iter()
        .filter(|item| {
            matches!(item,
                Item::Rect { obs, role, .. } | Item::Area { obs, role, .. } if owns(obs, role))
        })
        .collect();
    (!tiles.is_empty()).then_some(tiles)
}

/// Whether a mark has a fill to outline; a line alone keeps the answer of `items`.
fn filled(mark: &Item) -> bool {
    matches!(
        mark,
        Item::Rect { .. } | Item::Area { .. } | Item::Point { .. }
    )
}

/// Whether `emphasised` answers for this observation, which then takes no faint trace from
/// `items` or `tiles`.
fn outlined(kind: &str, panel: &Panel, observation: usize) -> bool {
    match emphasis(kind) {
        Some(Emphasis::Outline) => flush_tiles(kind, panel, observation).is_some(),
        Some(Emphasis::Line | Emphasis::Contour) => {
            owned_marks(kind, panel, observation).iter().any(filled)
        }
        _ => false,
    }
}

/// The dark outline of a hovered mark at `level`, in drawing order, or `None` where the chart
/// type or the observation has another answer. A tile that touches its neighbours takes one
/// dark line. A tile that gaps set apart takes a faint edge, a paper line and a dark line. A
/// mark with room around it takes one flat band of its own colour, then the paper line and
/// the dark line; a line that is part of it comes back in its own colour between them.
pub(super) fn emphasised(
    kind: &str,
    panel: &Panel,
    observation: usize,
    inks: &RingInks,
    level: f64,
) -> Option<Vec<Item>> {
    drawn(emphasis(kind)?, kind, panel, observation, inks, level)
}

/// What `emphasised` draws for a `kind` that takes `emphasis`.
fn drawn(
    emphasis: Emphasis,
    kind: &str,
    panel: &Panel,
    observation: usize,
    inks: &RingInks,
    level: f64,
) -> Option<Vec<Item>> {
    let lined = match emphasis {
        // `selection_ring` draws the ring, from the mark the host hands it.
        Emphasis::Ring => return None,
        Emphasis::Outline => {
            let tiles = flush_tiles(kind, panel, observation)?;
            if level <= 0.0 {
                return Some(Vec::new());
            }
            let dark = Stroke::solid(inks.ink.with_alpha(0.85 * level), 1.5);
            return Some(
                tiles
                    .into_iter()
                    .map(|tile| outline(tile, dark.clone(), 0.0))
                    .collect(),
            );
        }
        Emphasis::Line => true,
        Emphasis::Contour => false,
    };
    dark_outline(
        lined,
        kind,
        owned_marks(kind, panel, observation),
        inks,
        level,
    )
}

/// The dark outline of the marks a chart's legend picks out, at `level`: every ribbon and node
/// of `series` in a chart that outlines from its legend, drawn as a hovered one is. `None`
/// for a chart that dims the rest instead, and for a series with no filled mark.
pub(super) fn series_outlined(
    kind: &str,
    panel: &Panel,
    series: usize,
    inks: &RingInks,
    level: f64,
) -> Option<Vec<Item>> {
    if !outlines_from_legend(kind) {
        return None;
    }
    let lined = match emphasis(kind)? {
        Emphasis::Line => true,
        Emphasis::Contour => false,
        // The ring and the tile outline belong to one mark, not to a series.
        Emphasis::Ring | Emphasis::Outline => return None,
    };
    let marks = panel
        .items
        .iter()
        .filter(|item| {
            matches!(item,
                Item::Rect { role: Role::Mark, series: Some(s), .. }
                    | Item::Area { role: Role::Mark, series: Some(s), .. } if *s == series)
        })
        .cloned()
        .collect();
    dark_outline(lined, kind, marks, inks, level)
}

/// The dark line over a paper line, and the band for a mark with room around it, of `marks`.
fn dark_outline(
    lined: bool,
    kind: &str,
    marks: Vec<Item>,
    inks: &RingInks,
    level: f64,
) -> Option<Vec<Item>> {
    let (ink, paper) = (inks.ink, inks.paper);
    if !marks.iter().any(filled) {
        return None;
    }
    if level <= 0.0 {
        return Some(Vec::new());
    }
    let boxes = boxes(&marks);
    // A line inside the mark's own box, as a box plot's median is, takes nothing: its band
    // would stripe the box's fill.
    let parts: Vec<(&Item, f64, Option<Item>)> = marks
        .iter()
        .filter(|mark| !inside_box(kind, &boxes, mark))
        .map(|mark| {
            let shift = hollow(mark, paper);
            let band = if lined {
                band(kind, mark, level, shift)
            } else {
                None
            };
            (mark, shift, band)
        })
        .collect();
    let mut items: Vec<Item> = parts.iter().filter_map(|part| part.2.clone()).collect();
    // Paint each line across the whole mark before the next, so one part's paper line cannot
    // erase the dark line of the part it joins. The faint edge stands in for the band on a
    // part that has none.
    for (layer, (extra, color, line_gap)) in [
        (6.0, ink.with_alpha(0.12 * level), 0.5),
        (3.5, paper.with_alpha(level), 0.5),
        (1.5, ink.with_alpha(0.85 * level), 2.5),
    ]
    .into_iter()
    .enumerate()
    {
        for (mark, shift, band) in &parts {
            if layer == 0 && band.is_some() {
                continue;
            }
            let base = match mark {
                Item::Line { stroke, .. } => stroke.width,
                _ => 0.0,
            };
            let stroke = Stroke::solid(color, base + extra);
            items.push(match mark {
                Item::Rect { rect, .. } if *shift > 0.0 => {
                    let mut round = (*mark).clone();
                    if let Item::Rect { rect: moved, .. } = &mut round {
                        *moved = rect.inflate(*shift);
                    }
                    outline(&round, stroke, line_gap)
                }
                _ => outline(mark, stroke, line_gap),
            });
        }
    }
    // A line part comes back in its own colour and width over its paper line.
    for (mark, _, _) in &parts {
        if let Item::Line { stroke, .. } = mark {
            items.push(outline(
                mark,
                Stroke {
                    color: stroke.color.with_alpha(stroke.color.alpha() * level),
                    ..stroke.clone()
                },
                0.0,
            ));
        }
    }
    Some(items)
}

/// The sharp band is blended in sRGB by the renderer, then seen against the paper.
fn ring_ink(mark: Rgba, paper: Rgba, authored: Option<Oklch>) -> Rgba {
    use richview_chart::color::to_linear;

    let luminance = |rgb: [f64; 3]| {
        let [r, g, b] = rgb.map(to_linear);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    };
    let background = [paper.0[0], paper.0[1], paper.0[2]].map(f64::from);
    let paper_lum = luminance(background);
    let contrast = |ink: Rgba| {
        let blended = std::array::from_fn(|i| {
            RING_ALPHA * f64::from(ink.0[i]) + (1.0 - RING_ALPHA) * background[i]
        });
        let y = luminance(blended);
        (y.max(paper_lum) + 0.05) / (y.min(paper_lum) + 0.05)
    };
    if contrast(mark) >= 3.0 {
        return mark.with_alpha(RING_ALPHA);
    }
    let source = authored.unwrap_or_else(|| mark.oklch());
    let mut pass = if paper_lum < 0.18 { 1.0 } else { 0.0 };
    let mut fail = source.l;
    for _ in 0..16 {
        let l = (pass + fail) / 2.0;
        let ink = Rgba::from_oklch(Oklch { l, ..source }, RING_ALPHA);
        if contrast(ink) >= 3.05 {
            pass = l;
        } else {
            fail = l;
        }
    }
    Rgba::from_oklch(Oklch { l: pass, ..source }, RING_ALPHA)
}

/// Append the paper edge and coloured ring outside the visible mark. The soft tail
/// samples a 4 px Gaussian across the edge of the prototype's 12% underlay.
pub(super) fn selection_ring(
    mark: &Item,
    inks: &RingInks,
    level: f64,
    flat: bool,
    items: &mut Vec<Item>,
) {
    const EDGE: f64 = 1.5;
    const RING: f64 = 1.25;
    let Some(color) = mark_colour(mark) else {
        return;
    };
    let half = match mark {
        Item::Point { stroke, .. } | Item::Rect { stroke, .. } => {
            stroke.as_ref().map_or(0.0, |s| s.width / 2.0)
        }
        Item::Line { stroke, .. } => stroke.width / 2.0,
        _ => return,
    };
    let polygon = match mark {
        Item::Line { path, .. } => path.flatten(0.25).into_iter().next().unwrap_or_default(),
        Item::Point {
            center,
            shape,
            radius,
            ..
        } if *shape != Shape::Circle || half > 0.0 => {
            crate::mesh::shape_outline(*shape, *center, *radius).0
        }
        _ => Vec::new(),
    };
    let perimeter = |gap: f64| {
        let by = half + gap;
        match mark {
            Item::Rect { rect, radius, .. } => crate::mesh::rounded_ring(
                rect.inflate(by),
                radius.map(|r| if r > 0.0 { r + by } else { 0.0 }),
                0.125,
            ),
            Item::Point {
                center,
                shape: Shape::Circle,
                radius,
                ..
            } if polygon.is_empty() => super::focus::circle_points(*center, *radius + by),
            _ => super::focus::offset(&polygon, by),
        }
    };
    let mut band = |gap: f64, width: f64, ink: Rgba| {
        let ink = ink.with_alpha(ink.alpha() * level);
        if let Item::Point {
            center,
            radius,
            shape: shape @ (Shape::Plus | Shape::Cross),
            ..
        } = mark
        {
            items.push(cross_ring(
                *center,
                *radius,
                *shape,
                half,
                gap,
                Stroke::solid(ink, width),
            ));
            return;
        }
        // Fill between closed paths once. Overlapping translucent strokes would
        // darken their joins and leave a dotted rim on a round bubble.
        let mut path = Path::polyline(&perimeter(gap + width));
        path.close();
        path.extend(&Path::polyline(&perimeter(gap)));
        path.close();
        items.push(Item::Area {
            path,
            fill: Fill::solid(ink),
            role: Role::Annotation,
            series: None,
            obs: None,
        });
    };
    // The underlay extends 4 px past the paper edge before blur. The flat
    // style keeps that same extent with a hard stop, as in the prototype.
    let outer = EDGE + RING;
    if flat {
        band(outer, 4.0 - RING, color.with_alpha(0.12));
    } else {
        // Coverage at the midpoint of each 2 px band, from a Gaussian with σ = 4 px.
        for (i, coverage) in [0.669, 0.475, 0.287, 0.144, 0.059, 0.020]
            .into_iter()
            .enumerate()
        {
            band(
                outer + 2.0 * i as f64,
                2.0,
                color.with_alpha(0.12 * coverage),
            );
        }
    }
    band(EDGE, RING, inks.get(color));
    band(0.0, EDGE, inks.paper);
}

/// Paint a narrow band in the mark's own colour along its edge, with no separator.
fn trace(mark: &Item, level: f64) -> Option<Item> {
    let (color, width) = match mark {
        Item::Line { stroke, .. } => (stroke.color, stroke.width + 2.0),
        Item::Area { fill, .. } => (fill.color, 2.0),
        Item::Rect { fill, stroke, .. } => (stroke.as_ref().map_or(fill.color, |s| s.color), 2.0),
        Item::Point { fill, stroke, .. } => (
            fill.map(|f| f.color).or(stroke.as_ref().map(|s| s.color))?,
            1.5,
        ),
        Item::Text(_) => return None,
    };
    Some(outline(
        mark,
        Stroke::solid(color.with_alpha(color.alpha() * 0.22 * level), width),
        0.0,
    ))
}

/// The marks that an observation's feedback follows, a candlestick's wick cut to its ends.
fn owned_marks(kind: &str, panel: &Panel, observation: usize) -> Vec<Item> {
    let line = if matches!(kind, "network" | "parallel" | "flow") {
        panel.items.iter().find(|item| {
            matches!(item,
                Item::Line { role: Role::Mark, obs: Some(index), .. } if *index == observation
            )
        })
    } else {
        None
    };
    let mut marks: Vec<Item> = panel
        .items
        .iter()
        .filter(|mark| match mark {
            Item::Rect { obs, role, .. }
            | Item::Line { obs, role, .. }
            | Item::Area { obs, role, .. }
            | Item::Point { obs, role, .. } => *role == Role::Mark && *obs == Some(observation),
            Item::Text(_) => false,
        })
        .filter(|mark| line.is_none_or(|line| std::ptr::eq(*mark, line)))
        // Grouped observations can own many shapes; existing single-shape families keep
        // their established selection, including the three components of an OHLC glyph.
        .take(match kind {
            "bar" | "combo" | "dumbbell" | "waffle" | "lollipop" | "boxplot" | "candlestick"
            | "interval" | "pareto" | "dotplot" => usize::MAX,
            "ohlc" => 3,
            _ => 1,
        })
        .cloned()
        .collect();
    // The body is painted over the middle of a candlestick's wick. Keep its feedback
    // on the exposed ends too, so the overlay does not draw through the body's fill.
    if kind == "candlestick"
        && let Some(body) = marks.iter().find_map(|mark| match mark {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
    {
        for mark in &mut marks {
            if let Item::Line { path, .. } = mark {
                *path = exposed_wick(path, body);
            }
        }
    }
    marks
}

/// The boxes among `marks` that may enclose a line of the same mark.
fn boxes(marks: &[Item]) -> Vec<Rect> {
    marks
        .iter()
        .filter_map(|m| match m {
            Item::Rect { rect, .. } => Some(rect.inflate(0.5)),
            _ => None,
        })
        .collect()
}

/// Whether `mark` is a line inside its own mark's box, as a boxplot's median is.
fn inside_box(kind: &str, boxes: &[Rect], mark: &Item) -> bool {
    match mark {
        Item::Line { path, .. } if matches!(kind, "boxplot" | "candlestick" | "interval") => {
            path.bounds().is_some_and(|b| {
                boxes
                    .iter()
                    .any(|r| r.contains([b.x, b.y]) && r.contains([b.right(), b.bottom()]))
            })
        }
        _ => false,
    }
}

/// Fade a contour along owned marks; other families retain their existing feedback.
pub(super) fn items(
    kind: &str,
    panel: &Panel,
    observation: usize,
    surface: Rgba,
    level: f64,
    sized: bool,
    keyboard: bool,
) -> Option<Vec<Item>> {
    if kind == "gauge" {
        return Some(if level <= 0.0 || keyboard {
            Vec::new()
        } else {
            gauge_band(panel, observation)
                .and_then(|mark| trace(mark, level))
                .into_iter()
                .collect()
        });
    }
    // These families identify the target through emphasis in the base chart: the ring or the
    // dark outline `emphasised` draws.
    if matches!(
        kind,
        "bar" | "bar3d" | "packed-bubble" | "sankey" | "alluvial"
    ) {
        return Some(Vec::new());
    }
    if kind == "map" && !panel.items.iter().any(|item| matches!(item, Item::Point { role: Role::Mark, obs: Some(index), .. } if *index == observation)) {
        return None;
    }
    if matches!(kind, "hexbin" | "waffle") {
        return None;
    }
    if !matches!(
        kind,
        "ohlc"
            | "histogram"
            | "treemap"
            | "sunburst"
            | "scatter"
            | "packed-bubble"
            | "rose"
            | "pie"
            | "bar"
            | "combo"
            | "dumbbell"
            | "lollipop"
            | "sankey"
            | "alluvial"
            | "chord"
            | "boxplot"
            | "bullet"
            | "candlestick"
            | "interval"
            | "pareto"
            | "waterfall"
            | "gantt"
            | "funnel"
            | "violin"
            | "dotplot"
            | "qq"
            | "strip"
            | "scatter3d"
            | "network"
            | "parallel"
            | "map"
            | "flow"
            | "venn"
    ) {
        return None;
    }
    // The dark outline of `emphasised` answers in place of a trace.
    if level <= 0.0 || outlined(kind, panel, observation) {
        return Some(Vec::new());
    }
    let mut marks = owned_marks(kind, panel, observation);
    // A hovered scatter point grows while its contour follows the resized mark.
    // Put a disc of paper below it so its translucent fill does not double over the
    // original.
    let mut grown = Vec::new();
    let growth = point_growth(kind, sized);
    if growth > 0.0 {
        for mark in &mut marks {
            if let Item::Point { center, radius, .. } = mark {
                *radius *= 1.0 + growth * level;
                grown.push(Item::Point {
                    center: *center,
                    radius: *radius,
                    shape: Shape::Circle,
                    fill: Some(Fill::solid(surface.with_alpha(surface.alpha() * level))),
                    stroke: None,
                    role: Role::Annotation,
                    series: None,
                    obs: None,
                });
                let mut copy = mark.clone();
                if let Item::Point { role, .. } = &mut copy {
                    *role = Role::Annotation;
                }
                grown.push(copy);
            }
        }
    }
    // Leave a median inside its own box alone. The contour follows the enclosing mark.
    let boxes = boxes(&marks);
    if growth > 0.0 || (keyboard && kind != "flow") {
        return Some(grown);
    }
    if kind == "flow" {
        // A node has an owned fill and a visible border. Reinforce only that existing line.
        if !crisp_target(kind, panel, observation) {
            return Some(Vec::new());
        }
        return Some(
            marks
                .iter()
                .filter_map(|mark| {
                    let Item::Line { stroke, .. } = mark else {
                        return None;
                    };
                    let mut stronger = stroke.clone();
                    stronger.width += 0.5 * level;
                    stronger.color = stronger.color.with_alpha(stronger.color.alpha() * level);
                    Some(outline(mark, stronger, 0.0))
                })
                .collect(),
        );
    }
    let items = marks
        .iter()
        .filter(|mark| !inside_box(kind, &boxes, mark))
        .filter_map(|mark| trace(mark, level))
        .collect();
    Some(items)
}

/// Find the band at the needle without changing the gauge's larger pointer target.
fn gauge_band(panel: &Panel, observation: usize) -> Option<&Item> {
    let point = panel.observations.get(observation)?.at;
    let mut track = None;
    let mut best: Option<(&Item, bool, f64)> = None;
    for item in &panel.items {
        match item {
            Item::Area {
                role: Role::Band, ..
            } if track.is_none() => track = Some(item),
            Item::Area {
                path,
                role: Role::Mark,
                ..
            } if track.is_some() => {
                let (inside, distance) = path_distance(path, point);
                let replace = best.is_none_or(|(_, was_inside, old)| {
                    (inside && !was_inside) || (inside == was_inside && distance <= old + 0.1)
                });
                if replace {
                    best = Some((item, inside, distance));
                }
            }
            Item::Line {
                role: Role::Mark, ..
            } if track.is_some() => break,
            _ => {}
        }
    }
    best.map(|(item, _, _)| item).or(track)
}

/// Whether a point lies in a filled path and its distance to the path's boundary.
fn path_distance(path: &Path, point: [f64; 2]) -> (bool, f64) {
    let mut inside = false;
    let mut distance = f64::INFINITY;
    for ring in path.flatten(0.25) {
        for pair in ring.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let edge = [b[0] - a[0], b[1] - a[1]];
            let length = edge[0] * edge[0] + edge[1] * edge[1];
            let t = if length > 0.0 {
                (((point[0] - a[0]) * edge[0] + (point[1] - a[1]) * edge[1]) / length)
                    .clamp(0.0, 1.0)
            } else {
                0.0
            };
            distance =
                distance.min((a[0] + t * edge[0] - point[0]).hypot(a[1] + t * edge[1] - point[1]));
            if (a[1] > point[1]) != (b[1] > point[1])
                && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]
            {
                inside = !inside;
            }
        }
    }
    (inside, distance)
}

/// Keep only the two ends of an axis-aligned wick that are outside the candle body.
fn exposed_wick(path: &Path, body: Rect) -> Path {
    let [Cmd::M { p: a }, Cmd::L { p: b }] = path.cmds.as_slice() else {
        return path.clone();
    };
    let vertical = (a[0] - b[0]).abs() < 1e-9;
    let horizontal = (a[1] - b[1]).abs() < 1e-9;
    if !vertical && !horizontal {
        return path.clone();
    }
    let (axis, low_edge, high_edge) = if vertical {
        (1, body.y, body.bottom())
    } else {
        (0, body.x, body.right())
    };
    let low = a[axis].min(b[axis]);
    let high = a[axis].max(b[axis]);
    let point = |value| {
        let mut p = *a;
        p[axis] = value;
        p
    };
    let mut result = Path::new();
    if low < low_edge {
        result.move_to(point(low));
        result.line_to(point(high.min(low_edge)));
    }
    if high > high_edge {
        result.move_to(point(low.max(high_edge)));
        result.line_to(point(high));
    }
    result
}

/// Trace tiled observations in their own colour, without a halo over their neighbours. A tile
/// that takes the dark outline of `emphasised` asks for nothing here.
pub(super) fn tiles(
    kind: &str,
    panel: &Panel,
    observation: usize,
    level: f64,
) -> Option<Vec<Item>> {
    if !matches!(
        kind,
        "heatmap"
            | "calendar"
            | "bin2d"
            | "hexbin"
            | "pivot"
            | "icicle"
            | "waffle"
            | "map"
            | "surface"
    ) {
        return None;
    }
    if level <= 0.0 || outlined(kind, panel, observation) {
        return Some(Vec::new());
    }
    Some(
        panel
            .items
            .iter()
            .filter(|item| match item {
                Item::Rect {
                    role: Role::Mark,
                    obs: Some(index),
                    ..
                }
                | Item::Area {
                    role: Role::Mark,
                    obs: Some(index),
                    ..
                } => *index == observation,
                _ => false,
            })
            .filter_map(|mark| trace(mark, level))
            .collect(),
    )
}

/// Trace a mark’s shape, keeping point contours beyond the original marker.
fn outline(mark: &Item, mut stroke: Stroke, point_gap: f64) -> Item {
    match mark {
        Item::Rect { rect, radius, .. } => Item::Rect {
            rect: *rect,
            fill: Fill::solid(Rgba([0.0; 4])),
            stroke: Some(stroke),
            radius: *radius,
            role: Role::Annotation,
            series: None,
            obs: None,
        },
        Item::Line {
            path,
            stroke: original,
            ..
        } => {
            stroke.cap = original.cap;
            stroke.dash = original.dash.clone();
            Item::Line {
                path: path.clone(),
                stroke,
                role: Role::Annotation,
                series: None,
                obs: None,
            }
        }
        Item::Area { path, .. } => Item::Line {
            path: path.clone(),
            stroke,
            role: Role::Annotation,
            series: None,
            obs: None,
        },
        Item::Point {
            center,
            radius,
            shape,
            stroke: original,
            ..
        } => {
            let original_half = original.as_ref().map_or(0.0, |s| s.width / 2.0);
            // Leave the original mark and its stroke untouched.
            if matches!(shape, Shape::Plus | Shape::Cross) {
                return cross_ring(*center, *radius, *shape, original_half, point_gap, stroke);
            }
            let inradius = match shape {
                Shape::Circle => 1.0,
                Shape::Square => 0.85,
                Shape::Triangle => 0.48,
                Shape::Diamond => std::f64::consts::FRAC_1_SQRT_2,
                Shape::Plus | Shape::Cross => unreachable!(),
            };
            Item::Point {
                center: *center,
                radius: radius + (original_half + point_gap + stroke.width / 2.0) / inradius,
                shape: *shape,
                fill: None,
                stroke: Some(stroke),
                role: Role::Annotation,
                series: None,
                obs: None,
            }
        }
        Item::Text(_) => unreachable!("only owned marks have contours"),
    }
}

/// Draw the outside of a stroked plus or cross without painting either original bar.
fn cross_ring(
    center: [f64; 2],
    radius: f64,
    shape: Shape,
    original_half: f64,
    gap: f64,
    stroke: Stroke,
) -> Item {
    let rotated = shape == Shape::Cross;
    let reach = if rotated {
        radius * 0.75 * std::f64::consts::SQRT_2
    } else {
        radius
    };
    let mut path = Path::new();
    for offset in [gap + stroke.width, gap] {
        let half = original_half + offset;
        let end = reach + offset;
        let points = [
            [-half, -end],
            [half, -end],
            [half, -half],
            [end, -half],
            [end, half],
            [half, half],
            [half, end],
            [-half, end],
            [-half, half],
            [-end, half],
            [-end, -half],
            [-half, -half],
        ];
        for (i, [x, y]) in points.into_iter().enumerate() {
            let [x, y] = if rotated {
                [
                    (x - y) * std::f64::consts::FRAC_1_SQRT_2,
                    (x + y) * std::f64::consts::FRAC_1_SQRT_2,
                ]
            } else {
                [x, y]
            };
            let point = [center[0] + x, center[1] + y];
            if i == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        path.close();
    }
    Item::Area {
        path,
        fill: Fill::solid(stroke.color),
        role: Role::Annotation,
        series: None,
        obs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use richview_chart::block::ChartBlock;
    use richview_chart::layout::{ChartTheme, Options, layout};

    #[test]
    fn the_settled_ring_reaches_three_to_one_on_every_builtin_palette() {
        use richview_chart::color::{Oklch, contrast, parse};
        use richview_core::document::{RenderOptions, ReportFolder, render};

        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../scripts/fixtures/selection-ring-colours.json"
        ))
        .unwrap();
        for &style in richview_core::builtin_style_names() {
            let block = r#"{"type":"bar","title":"A","encoding":{"x":"x","y":"y"},"data":{"inline":[{"x":"A","y":1}]}}"#;
            let source =
                format!("---\ntitle: Contrast\nstyles: [{style}]\n---\n\n```chart\n{block}\n```");
            let options = RenderOptions {
                report_folder: Some(ReportFolder {
                    root: "contrast".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            };
            let report = render(&source, &options).unwrap();
            let payload = report
                .html
                .split_once("data-rv-payload>")
                .unwrap()
                .1
                .split_once("</script>")
                .unwrap()
                .0;
            let chart = crate::payload::Chart::parse(payload).unwrap();
            let theme = &chart.payload.theme;
            let geometry = chart.geometry(640.0, &[], None);
            let mut inks = RingInks::new(theme, &geometry);
            assert_eq!(
                expected[style]["marks"].as_array().unwrap().len(),
                theme.palette.len()
            );
            let paper = Rgba::from_oklch(parse(&theme.surface).unwrap(), 1.0);
            assert_eq!(expected[style]["paper"], paper.hex());
            for (slot, css) in theme.palette.iter().enumerate() {
                let mark = Rgba::from_oklch(parse(css).unwrap(), 1.0);
                let ink = inks.get(mark);
                assert_eq!(expected[style]["marks"][slot], mark.hex());
                let blended = |ink: Rgba| {
                    Oklch::from_srgb(std::array::from_fn(|i| {
                        0.8 * f64::from(ink.0[i]) + 0.2 * f64::from(paper.0[i])
                    }))
                };
                assert_eq!(expected[style]["rings"][slot], blended(ink).hex());
                let original = contrast(blended(mark), paper.oklch());
                let adjusted = contrast(blended(ink), paper.oklch());
                assert_eq!(ink.0[3], 0.8);
                if original >= 3.0 {
                    assert_eq!(ink.0[..3], mark.0[..3], "{style} {css}");
                } else {
                    assert!(
                        (3.0499..3.051).contains(&adjusted),
                        "{style} {css}: {adjusted}"
                    );
                }
            }
            // Redrawing and relaying out a corrected slot preserve its cache entry regardless of fade alpha.
            let mark = Rgba::from_oklch(parse(&theme.palette[1]).unwrap(), 1.0);
            let sentinel = Rgba([0.25; 4]);
            inks.colours.insert(RingInks::key(mark), sentinel);
            inks.refresh(theme, &geometry);
            for alpha in [0.0, 0.4, 0.8, 1.0] {
                assert_eq!(inks.get(mark.with_alpha(alpha)), sentinel);
            }
            let mut other_paper = geometry;
            other_paper.background = Rgba([0.3, 0.3, 0.3, 1.0]);
            inks.refresh(theme, &other_paper);
            assert_ne!(inks.get(mark), sentinel);
        }
    }

    #[test]
    fn a_stroked_bubble_ring_keeps_its_gap_at_vertices_and_facet_midpoints() {
        let mut mark = dot([0.0, 0.0], 1.0, Rgba([0.0, 0.0, 0.0, 1.0]));
        let Item::Point { radius, stroke, .. } = &mut mark else {
            unreachable!()
        };
        *radius = 200.0;
        *stroke = Some(Stroke::solid(Rgba([1.0; 4]), 2.0));
        let color = mark_colour(&mark).unwrap();
        let inks = RingInks {
            paper: Rgba([1.0; 4]),
            palette: Vec::new(),
            colours: [(RingInks::key(color), color.with_alpha(RING_ALPHA))].into(),
            ink: color,
        };
        let mut items = Vec::new();
        selection_ring(&mark, &inks, 1.0, true, &mut items);
        let Item::Area { path, .. } = &items[items.len() - 2] else {
            panic!("sharp ring")
        };
        let inner = &path.flatten(0.125)[1];
        let (facet, _) = crate::mesh::shape_outline(Shape::Circle, [0.0, 0.0], 200.0);
        // Project onto the outward normal of the first facet, then onto its first vertex.
        for [x, y] in [
            [
                (facet[0][0] + facet[1][0]) / 2.0,
                (facet[0][1] + facet[1][1]) / 2.0,
            ],
            facet[0],
        ] {
            let r = x.hypot(y);
            let farthest = inner
                .iter()
                .map(|p| (p[0] * x + p[1] * y) / r)
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(
                (farthest - r - 1.0 - 1.5).abs() < 0.02,
                "gap at {x},{y}: {}",
                farthest - r - 1.0
            );
        }
    }

    #[test]
    fn selection_ring_starts_at_the_mark_and_preserves_its_fill() {
        let paper = Rgba([1.0; 4]);
        let color = Rgba([0.1, 0.3, 0.6, 0.5]);
        let mark = dot([20.0, 30.0], 1.0, color);
        for flat in [false, true] {
            let mut items = Vec::new();
            let inks = RingInks {
                paper,
                palette: Vec::new(),
                colours: [(RingInks::key(color), color.with_alpha(RING_ALPHA))].into(),
                ink: color,
            };
            selection_ring(&mark, &inks, 1.0, flat, &mut items);
            let bands: Vec<_> = items
                .iter()
                .map(|item| {
                    let Item::Area { path, fill, .. } = item else {
                        panic!("expected a band")
                    };
                    let loops = path.flatten(0.125);
                    // Both circles start on the positive x axis.
                    assert_eq!(loops.len(), 2);
                    (loops[1][0][0] - 20.0, loops[0][0][0] - 20.0, fill.color)
                })
                .collect();
            let (edge, rest) = bands.split_last().unwrap();
            let (ring, tail) = rest.split_last().unwrap();
            assert_eq!(*edge, (6.0, 7.5, paper));
            assert_eq!(*ring, (7.5, 8.75, color.with_alpha(0.8)));
            assert!(!tail.is_empty());
            assert!(
                tail.iter()
                    .all(|(inner, _, c)| *inner >= ring.1 && c.alpha() <= 0.121)
            );
            assert!(
                tail.windows(2)
                    .all(|w| w[0].1 == w[1].0 && w[0].2.alpha() > w[1].2.alpha())
            );
        }
    }

    #[test]
    fn separated_marks_follow_their_observation_geometry() {
        for (kind, fixture) in [
            (
                "qq",
                include_str!("../../../../../richview-chart/tests/fixtures/types/qq.json"),
            ),
            (
                "strip",
                include_str!("../../../../../richview-chart/tests/fixtures/types/strip.json"),
            ),
            (
                "scatter3d",
                include_str!("../../../../../richview-chart/tests/fixtures/types/scatter3d.json"),
            ),
        ] {
            let geometry = laid_out(fixture);
            let panel = &geometry.panels[0];
            for index in 0..panel.observations.len() {
                let marks = owned(panel, index);
                assert!(
                    !marks.is_empty(),
                    "{kind} observation {index} has no owned mark"
                );
                let contours = items(kind, panel, index, geometry.background, 1.0, false, false)
                    .unwrap_or_else(|| panic!("{kind} has no contour"));
                let follows = |mark: &Item, item: &Item| match (mark, item) {
                    (Item::Rect { rect: a, .. }, Item::Rect { rect: b, .. }) => a == b,
                    (Item::Line { path: a, .. }, Item::Line { path: b, .. }) => a == b,
                    (Item::Area { path: a, .. }, Item::Line { path: b, .. }) => a == b,
                    (Item::Point { center: a, .. }, Item::Point { center: b, .. }) => a == b,
                    _ => false,
                };
                for mark in &marks {
                    assert!(
                        contours.iter().any(|item| follows(mark, item)),
                        "{kind} observation {index} misses a mark: {mark:?}"
                    );
                }
                assert!(
                    !contours.is_empty(),
                    "{kind} observation {index} shows no feedback"
                );
                for item in &contours {
                    assert!(
                        marks.iter().any(|mark| follows(mark, item)),
                        "{kind} observation {index} contours an unrelated shape: {item:?}"
                    );
                }
            }
        }
    }

    const BOXPLOT: &str = r#"{"spec":{"type":"boxplot","title":"T","encoding":{"x":"k","y":"v"}},"rows":[{"k":"a","v":1},{"k":"a","v":2},{"k":"a","v":3},{"k":"a","v":4},{"k":"a","v":50},{"k":"b","v":2},{"k":"b","v":3}]}"#;
    const INTERVAL: &str = r#"{"spec":{"type":"interval","title":"T","encoding":{"x":"k","y":"v","lower":"lo","upper":"hi"}},"rows":[{"k":"a","v":2,"lo":1,"hi":3},{"k":"b","v":4,"lo":3,"hi":5}]}"#;

    fn laid_out(fixture: &str) -> Geometry {
        let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
        layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        )
    }

    fn owned(panel: &Panel, index: usize) -> Vec<&Item> {
        panel.items.iter().filter(|item| matches!(item, Item::Rect { role: Role::Mark, obs: Some(k), .. } | Item::Line { role: Role::Mark, obs: Some(k), .. } | Item::Area { role: Role::Mark, obs: Some(k), .. } | Item::Point { role: Role::Mark, obs: Some(k), .. } if *k == index)).collect()
    }

    const PIE: &str = r#"{"spec":{"type":"pie","title":"T","encoding":{"x":"c","y":"v"}},"rows":[{"c":"a","v":2},{"c":"b","v":3},{"c":"c","v":5}]}"#;
    const HEATMAP: &str = r#"{"spec":{"type":"heatmap","title":"T","encoding":{"x":"hour","y":"day","color":"value"}},"rows":[{"hour":"9","day":"Mon","value":2},{"hour":"12","day":"Mon","value":4},{"hour":"9","day":"Tue","value":3}]}"#;
    const HISTOGRAM: &str = r#"{"spec":{"type":"histogram","title":"T","encoding":{"x":"v"}},"rows":[{"v":1},{"v":2},{"v":2},{"v":3},{"v":3},{"v":3},{"v":7},{"v":8},{"v":9}]}"#;

    /// Whether `item` is a line in the dark ink of `emphasised`.
    fn dark(item: &Item, inks: &RingInks) -> bool {
        let ink = inks.ink.with_alpha(0.85);
        match item {
            Item::Rect {
                stroke: Some(stroke),
                ..
            }
            | Item::Line { stroke, .. }
            | Item::Point {
                stroke: Some(stroke),
                ..
            } => stroke.color == ink,
            _ => false,
        }
    }

    #[test]
    fn every_outlined_type_draws_a_dark_line_on_its_marks_and_keeps_no_trace() {
        for (kind, fixture) in [
            ("boxplot", BOXPLOT),
            ("interval", INTERVAL),
            ("pie", PIE),
            ("heatmap", HEATMAP),
            ("histogram", HISTOGRAM),
            (
                "funnel",
                include_str!("../../../../../richview-chart/tests/fixtures/types/funnel.json"),
            ),
            (
                "bullet",
                include_str!("../../../../../richview-chart/tests/fixtures/types/bullet.json"),
            ),
            (
                "candlestick",
                include_str!("../../../../../richview-chart/tests/fixtures/types/candlestick.json"),
            ),
            (
                "dumbbell",
                include_str!("../../../../../richview-chart/tests/fixtures/types/dumbbell.json"),
            ),
            (
                "waterfall",
                include_str!("../../../../../richview-chart/tests/fixtures/types/waterfall.json"),
            ),
            (
                "pareto",
                include_str!("../../../../../richview-chart/tests/fixtures/types/pareto.json"),
            ),
            (
                "combo",
                include_str!("../../../../../richview-chart/tests/fixtures/types/combo.json"),
            ),
            (
                "gantt",
                include_str!("../../../../../richview-chart/tests/fixtures/types/gantt.json"),
            ),
            (
                "lollipop",
                include_str!("../../../../../richview-chart/tests/fixtures/types/lollipop.json"),
            ),
            (
                "dotplot",
                include_str!("../../../../../richview-chart/tests/fixtures/types/dotplot.json"),
            ),
            (
                "tree",
                include_str!("../../../../../richview-chart/tests/fixtures/types/tree.json"),
            ),
            (
                "network",
                include_str!("../../../../../richview-chart/tests/fixtures/types/network.json"),
            ),
            (
                "treemap",
                include_str!("../../../../../richview-chart/tests/fixtures/types/treemap.json"),
            ),
            (
                "sunburst",
                include_str!("../../../../../richview-chart/tests/fixtures/types/sunburst.json"),
            ),
            (
                "rose",
                include_str!("../../../../../richview-chart/tests/fixtures/types/rose.json"),
            ),
            (
                "violin",
                include_str!("../../../../../richview-chart/tests/fixtures/types/violin.json"),
            ),
            (
                "icicle",
                include_str!("../../../../../richview-chart/tests/fixtures/types/icicle.json"),
            ),
            (
                "map",
                include_str!("../../../../../richview-chart/tests/fixtures/types/map.json"),
            ),
            (
                "calendar",
                include_str!("../../../../../richview-chart/tests/fixtures/types/calendar.json"),
            ),
            (
                "bin2d",
                include_str!("../../../../../richview-chart/tests/fixtures/types/bin2d.json"),
            ),
            (
                "hexbin",
                include_str!("../../../../../richview-chart/tests/fixtures/types/hexbin.json"),
            ),
            (
                "pivot",
                include_str!("../../../../../richview-chart/tests/fixtures/types/pivot.json"),
            ),
            (
                "surface",
                include_str!("../../../../../richview-chart/tests/fixtures/types/surface.json"),
            ),
            (
                "venn",
                include_str!("../../../../../richview-chart/tests/fixtures/types/venn.json"),
            ),
            (
                "gauge",
                include_str!("../../../../../richview-chart/tests/fixtures/types/gauge.json"),
            ),
            (
                "bar3d",
                include_str!("../../../../../richview-chart/tests/fixtures/types/bar3d.json"),
            ),
            (
                "sankey",
                include_str!("../../../../../richview-chart/tests/fixtures/types/sankey.json"),
            ),
            (
                "alluvial",
                include_str!("../../../../../richview-chart/tests/fixtures/types/alluvial.json"),
            ),
        ] {
            assert!(
                matches!(
                    emphasis(kind),
                    Some(Emphasis::Line | Emphasis::Contour | Emphasis::Outline)
                ),
                "{kind}"
            );
            let geometry = laid_out(fixture);
            let inks = RingInks::new(&ChartTheme::default(), &geometry);
            let mut answered = 0;
            for panel in &geometry.panels {
                for index in 0..panel.observations.len() {
                    let Some(drawn) = emphasised(kind, panel, index, &inks, 1.0) else {
                        assert!(!outlined(kind, panel, index), "{kind} observation {index}");
                        continue;
                    };
                    answered += 1;
                    assert!(outlined(kind, panel, index), "{kind} observation {index}");
                    assert!(
                        drawn.iter().any(|item| dark(item, &inks)),
                        "{kind} observation {index} has no dark line: {drawn:?}"
                    );
                    assert_eq!(
                        emphasised(kind, panel, index, &inks, 0.0),
                        Some(Vec::new()),
                        "{kind} observation {index} leaves nothing at rest"
                    );
                    // A gauge's trace is the shape of its keyboard ring, so `items` keeps it.
                    if kind != "gauge" {
                        let traced =
                            items(kind, panel, index, geometry.background, 1.0, false, false);
                        assert!(
                            traced.is_none_or(|trace| trace.is_empty()),
                            "{kind} observation {index} keeps a trace under its outline"
                        );
                    }
                    assert!(
                        tiles(kind, panel, index, 1.0).is_none_or(|trace| trace.is_empty()),
                        "{kind} observation {index} keeps a tile trace under its outline"
                    );
                }
            }
            assert!(answered > 0, "{kind} outlines none of its observations");
        }
    }

    /// Every ribbon and node of a flow chart's series, as the legend outlines them.
    fn series_marks(panel: &Panel, series: usize) -> usize {
        panel
            .items
            .iter()
            .filter(|item| {
                matches!(item,
                    Item::Rect { role: Role::Mark, series: Some(s), .. }
                        | Item::Area { role: Role::Mark, series: Some(s), .. } if *s == series)
            })
            .count()
    }

    #[test]
    fn a_legend_outlines_every_ribbon_and_node_of_one_series_and_no_other() {
        for (kind, fixture) in [
            (
                "sankey",
                include_str!("../../../../../richview-chart/tests/fixtures/types/sankey.json"),
            ),
            (
                "alluvial",
                include_str!("../../../../../richview-chart/tests/fixtures/types/alluvial.json"),
            ),
        ] {
            let geometry = laid_out(fixture);
            let inks = RingInks::new(&ChartTheme::default(), &geometry);
            assert!(geometry.series.len() > 1, "{kind} has series to pick from");
            let mut outlined = 0;
            for panel in &geometry.panels {
                for series in 0..geometry.series.len() {
                    let marks = series_marks(panel, series);
                    let drawn = series_outlined(kind, panel, series, &inks, 1.0);
                    if marks == 0 {
                        assert_eq!(drawn, None, "{kind} series {series} has no mark here");
                        continue;
                    }
                    let drawn = drawn.expect("a series with marks is outlined");
                    // One dark line for each of its ribbons and nodes, and none for another
                    // series': a mark of another series would add to the count.
                    let lines = drawn.iter().filter(|item| dark(item, &inks)).count();
                    assert_eq!(lines, marks, "{kind} series {series}");
                    outlined += marks;
                    assert_eq!(
                        series_outlined(kind, panel, series, &inks, 0.0),
                        Some(Vec::new()),
                        "{kind} series {series} leaves nothing at rest"
                    );
                }
            }
            let all: usize = geometry
                .panels
                .iter()
                .map(|p| {
                    p.items
                        .iter()
                        .filter(|i| {
                            matches!(
                                i,
                                Item::Rect {
                                    role: Role::Mark,
                                    series: Some(_),
                                    ..
                                } | Item::Area {
                                    role: Role::Mark,
                                    series: Some(_),
                                    ..
                                }
                            )
                        })
                        .count()
                })
                .sum();
            assert_eq!(
                outlined, all,
                "{kind}: every series mark belongs to a series that outlines"
            );
        }
    }

    #[test]
    fn only_the_charts_that_outline_from_the_legend_answer_with_a_series_outline() {
        let geometry = laid_out(PIE);
        let inks = RingInks::new(&ChartTheme::default(), &geometry);
        for kind in ["pie", "bar", "treemap", "heatmap", "chord", "flow", "line"] {
            for panel in &geometry.panels {
                assert_eq!(series_outlined(kind, panel, 0, &inks, 1.0), None, "{kind}");
            }
        }
    }

    #[test]
    fn the_legend_outline_reads_against_the_paper_on_every_builtin_style_light_and_dark() {
        use richview_chart::color::{Oklch, contrast, parse};
        use richview_core::document::{RenderOptions, ReportFolder, render};

        let block = r#"{"type":"sankey","title":"A","encoding":{"source":"a","target":"b","y":"v"},"data":{"inline":[{"a":"X","b":"M","v":3},{"a":"Y","b":"M","v":2},{"a":"M","b":"Z","v":5}]}}"#;
        let source = format!("---\ntitle: Contrast\n---\n\n```chart\n{block}\n```");
        let styles = richview_core::builtin_style_names();
        let (mut dark_pages, mut light_pages) = (0, 0);
        for style in styles {
            let options = RenderOptions {
                styles: vec![(*style).to_string()],
                report_folder: Some(ReportFolder {
                    root: "contrast".into(),
                    source: "report.rmd".into(),
                    files: vec!["report.rmd".into()],
                }),
                ..Default::default()
            };
            let report = render(&source, &options).unwrap();
            let payload = report
                .html
                .split_once("data-rv-payload>")
                .unwrap()
                .1
                .split_once("</script>")
                .unwrap()
                .0;
            let chart = crate::payload::Chart::parse(payload).unwrap();
            let theme = &chart.payload.theme;
            let geometry = chart.geometry(640.0, &[], None);
            let inks = RingInks::new(theme, &geometry);
            let paper = Rgba::from_oklch(parse(&theme.surface).unwrap(), 1.0);
            dark_pages += usize::from(paper.oklch().l < 0.5);
            light_pages += usize::from(paper.oklch().l >= 0.5);
            // The dark line at the strength it settles on, seen against the paper line under it.
            let line = Oklch::from_srgb(std::array::from_fn(|i| {
                0.85 * f64::from(inks.ink.0[i]) + 0.15 * f64::from(paper.0[i])
            }));
            let reach = contrast(line, paper.oklch());
            assert!(
                reach >= 4.5,
                "{style}: the outline reaches {reach}:1 on the paper"
            );
            for panel in &geometry.panels {
                for series in 0..geometry.series.len() {
                    let drawn = series_outlined("sankey", panel, series, &inks, 1.0).unwrap();
                    // The paper line is drawn under the dark one and wider than it.
                    let widths: Vec<f64> = drawn
                        .iter()
                        .filter_map(|item| match item {
                            Item::Line { stroke, .. }
                            | Item::Rect {
                                stroke: Some(stroke),
                                ..
                            } => Some(stroke.width),
                            _ => None,
                        })
                        .collect();
                    assert!(widths.iter().any(|w| (*w - 1.5).abs() < 1e-9), "{style}");
                    assert!(widths.iter().any(|w| *w >= 3.5), "{style}: no paper line");
                }
            }
        }
        assert!(
            dark_pages >= 1 && light_pages >= 1,
            "dark {dark_pages}, light {light_pages}"
        );
    }

    #[test]
    fn a_mark_with_room_takes_one_flat_band_a_paper_line_and_a_dark_line() {
        let geometry = laid_out(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/funnel.json"
        ));
        let inks = RingInks::new(&ChartTheme::default(), &geometry);
        let panel = &geometry.panels[0];
        let Item::Rect { rect, fill, .. } = &owned_marks("funnel", panel, 1)[0] else {
            panic!("a funnel stage is a bar")
        };
        let lined = emphasised("funnel", panel, 1, &inks, 1.0).unwrap();
        let [
            Item::Area {
                path, fill: band, ..
            },
            paper,
            dark,
        ] = lined.as_slice()
        else {
            panic!("a band and two lines: {lined:?}")
        };
        assert_eq!(
            *band,
            Fill::solid(fill.color.with_alpha(0.25 * fill.color.alpha()))
        );
        // The band starts past the paper separator and reaches four pixels further.
        let rings = path.flatten(0.125);
        let reach = |ring: &Vec<[f64; 2]>| {
            let xs = ring.iter().map(|p| p[0]);
            rect.x - xs.fold(f64::INFINITY, f64::min)
        };
        assert!((reach(&rings[0]) - SEPARATOR - BAND).abs() < 1e-9);
        assert!((reach(&rings[1]) - SEPARATOR).abs() < 1e-9);
        for (line, stroke) in [
            (paper, Stroke::solid(inks.paper, 3.5)),
            (dark, Stroke::solid(inks.ink.with_alpha(0.85), 1.5)),
        ] {
            assert!(
                matches!(line, Item::Rect { rect: r, stroke: Some(s), .. } if r == rect && *s == stroke),
                "{line:?}"
            );
        }
        // A box plot's whiskers come back in their own colour over their paper line, and its
        // median stays as it is.
        let geometry = laid_out(BOXPLOT);
        let inks = RingInks::new(&ChartTheme::default(), &geometry);
        let panel = &geometry.panels[0];
        let marks = owned_marks("boxplot", panel, 0);
        let boxes = boxes(&marks);
        let median = marks
            .iter()
            .find_map(|mark| match mark {
                Item::Line { path, .. } if inside_box("boxplot", &boxes, mark) => {
                    Some(path.clone())
                }
                _ => None,
            })
            .expect("a boxplot draws its median inside the box");
        let parts: Vec<&Item> = marks
            .iter()
            .filter(|mark| !inside_box("boxplot", &boxes, mark))
            .collect();
        let whiskers = parts
            .iter()
            .filter(|mark| matches!(mark, Item::Line { .. }))
            .count();
        let lined = emphasised("boxplot", panel, 0, &inks, 1.0).unwrap();
        assert_eq!(lined.len(), 3 * parts.len() + whiskers);
        assert!(
            !lined
                .iter()
                .any(|item| matches!(item, Item::Line { path, .. } if *path == median)),
            "the median takes nothing inside its own box"
        );
        for (mark, redrawn) in parts
            .iter()
            .filter(|mark| matches!(mark, Item::Line { .. }))
            .zip(&lined[3 * parts.len()..])
        {
            let (
                Item::Line {
                    path: a,
                    stroke: own,
                    ..
                },
                Item::Line {
                    path: b, stroke, ..
                },
            ) = (mark, redrawn)
            else {
                panic!("a whisker is a line")
            };
            assert!(a == b && stroke == own);
        }
    }

    #[test]
    fn a_hollow_candle_keeps_its_own_outline_inside_the_dark_line() {
        let geometry = laid_out(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/candlestick.json"
        ));
        let inks = RingInks::new(&ChartTheme::default(), &geometry);
        let panel = &geometry.panels[0];
        let mut seen = [false; 2];
        for index in 0..panel.observations.len() {
            let marks = owned_marks("candlestick", panel, index);
            let Some(body) = marks.iter().find(|mark| matches!(mark, Item::Rect { .. })) else {
                continue;
            };
            let Item::Rect { rect, stroke, .. } = body else {
                unreachable!()
            };
            let shift = hollow(body, inks.paper);
            // A rising candle is an outline round paper; a falling one is filled.
            assert_eq!(shift > 0.0, stroke.is_some(), "candle {index}");
            if let Some(own) = stroke {
                assert!((shift - own.width / 2.0 - SEPARATOR).abs() < 1e-9);
            }
            seen[usize::from(shift > 0.0)] = true;
            let drawn = emphasised("candlestick", panel, index, &inks, 1.0).unwrap();
            let lines: Vec<&Rect> = drawn
                .iter()
                .filter_map(|item| match item {
                    Item::Rect {
                        rect,
                        stroke: Some(_),
                        ..
                    } => Some(rect),
                    _ => None,
                })
                .collect();
            assert_eq!(lines, [&rect.inflate(shift), &rect.inflate(shift)]);
        }
        assert_eq!(seen, [true, true], "the fixture has a candle of each kind");
    }

    #[test]
    fn gapped_tiles_take_a_dark_line_over_a_paper_line() {
        for (kind, fixture) in [
            (
                "treemap",
                include_str!("../../../../../richview-chart/tests/fixtures/types/treemap.json"),
            ),
            (
                "sunburst",
                include_str!("../../../../../richview-chart/tests/fixtures/types/sunburst.json"),
            ),
        ] {
            let geometry = laid_out(fixture);
            let inks = RingInks::new(&ChartTheme::default(), &geometry);
            let panel = &geometry.panels[0];
            for index in 0..panel.observations.len() {
                let Some(mark) = owned_marks(kind, panel, index).into_iter().next() else {
                    continue;
                };
                let drawn = emphasised(kind, panel, index, &inks, 1.0).unwrap();
                let strokes: Vec<&Stroke> = drawn
                    .iter()
                    .map(|item| match (&mark, item) {
                        (
                            Item::Rect { rect: a, .. },
                            Item::Rect {
                                rect: b,
                                stroke: Some(s),
                                ..
                            },
                        ) if a == b => s,
                        (
                            Item::Area { path: a, .. },
                            Item::Line {
                                path: b, stroke, ..
                            },
                        ) if a == b => stroke,
                        _ => panic!("{kind} observation {index} outlines another shape: {item:?}"),
                    })
                    .collect();
                assert_eq!(
                    strokes,
                    [
                        &Stroke::solid(inks.ink.with_alpha(0.12), 6.0),
                        &Stroke::solid(inks.paper, 3.5),
                        &Stroke::solid(inks.ink.with_alpha(0.85), 1.5),
                    ],
                    "{kind} observation {index}"
                );
                assert_eq!(
                    items(kind, panel, index, geometry.background, 1.0, false, false),
                    Some(Vec::new())
                );
            }
        }
    }

    #[test]
    fn flush_tiles_take_one_dark_line_each() {
        for (kind, fixture) in [
            (
                "icicle",
                include_str!("../../../../../richview-chart/tests/fixtures/types/icicle.json"),
            ),
            (
                "map",
                include_str!("../../../../../richview-chart/tests/fixtures/types/map.json"),
            ),
        ] {
            let geometry = laid_out(fixture);
            let inks = RingInks::new(&ChartTheme::default(), &geometry);
            let dark = Stroke::solid(inks.ink.with_alpha(0.85), 1.5);
            for panel in &geometry.panels {
                for index in 0..panel.observations.len() {
                    let tiles_owned: Vec<&Item> = owned(panel, index)
                        .into_iter()
                        .filter(|item| matches!(item, Item::Rect { .. } | Item::Area { .. }))
                        .collect();
                    assert!(
                        !tiles_owned.is_empty(),
                        "{kind} observation {index} has a tile"
                    );
                    let drawn = emphasised(kind, panel, index, &inks, 1.0).unwrap();
                    assert_eq!(drawn.len(), tiles_owned.len(), "{kind} observation {index}");
                    for (tile, line) in tiles_owned.into_iter().zip(&drawn) {
                        assert!(
                            match (tile, line) {
                                (
                                    Item::Rect { rect: a, .. },
                                    Item::Rect {
                                        rect: b,
                                        stroke: Some(s),
                                        ..
                                    },
                                ) => a == b && *s == dark,
                                (
                                    Item::Area { path: a, .. },
                                    Item::Line {
                                        path: b, stroke, ..
                                    },
                                ) => a == b && *stroke == dark,
                                _ => false,
                            },
                            "{kind} observation {index} traces its own tile in ink: {line:?}"
                        );
                    }
                    assert_eq!(tiles(kind, panel, index, 1.0), Some(Vec::new()));
                }
            }
        }
    }

    #[test]
    fn types_with_another_answer_take_no_dark_outline() {
        let bars = laid_out(
            r#"{"spec":{"type":"bar","title":"T","encoding":{"x":"c","y":"v"}},"rows":[{"c":"a","v":2},{"c":"b","v":3}]}"#,
        );
        let inks = RingInks::new(&ChartTheme::default(), &bars);
        // The ring is `selection_ring`'s, and these types are crisp targets for it: every
        // line-based type among them, as a line is.
        for kind in [
            "line",
            "area",
            "band",
            "density",
            "ecdf",
            "streamgraph",
            "bar",
            "scatter",
            "packed-bubble",
            "strip",
            "qq",
            "scatter3d",
        ] {
            assert_eq!(
                emphasised(kind, &bars.panels[0], 0, &inks, 1.0),
                None,
                "{kind}"
            );
            assert!(crisp_target(kind, &bars.panels[0], 0), "{kind}");
        }
        // A scatter matrix's points are crisp targets; its diagonal and correlation cells own
        // no point and keep the keyboard's focus outline.
        let matrix = laid_out(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/splom.json"
        ));
        let inks = RingInks::new(&ChartTheme::default(), &matrix);
        let mut seen = [0, 0];
        for panel in &matrix.panels {
            for index in 0..panel.observations.len() {
                let point = owned(panel, index)
                    .iter()
                    .any(|item| matches!(item, Item::Point { .. }));
                assert_eq!(crisp_target("splom", panel, index), point);
                assert_eq!(emphasised("splom", panel, index, &inks, 1.0), None);
                seen[usize::from(point)] += 1;
            }
        }
        assert!(seen[0] > 0 && seen[1] > 0, "points and cells: {seen:?}");
        for kind in ["flow", "waffle", "radar", "parallel", "chord", "ribbon"] {
            assert_eq!(emphasis(kind), None, "{kind}");
        }
        // A map's spot is a point: it keeps the contour of other points.
        let spot = r#"{"spec":{"type":"map","title":"Offices","encoding":{"lat":"la","lon":"lo"}},"rows":[{"la":50.09,"lo":14.42},{"la":-33.87,"lo":151.21}]}"#;
        let geometry = laid_out(spot);
        let inks = RingInks::new(&ChartTheme::default(), &geometry);
        let panel = &geometry.panels[0];
        for index in 0..panel.observations.len() {
            assert_eq!(emphasised("map", panel, index, &inks, 1.0), None);
        }
        // A network's link is a line: it keeps its own colour, and only the nodes are outlined.
        let network = laid_out(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/network.json"
        ));
        let inks = RingInks::new(&ChartTheme::default(), &network);
        let panel = &network.panels[0];
        let lined = (0..panel.observations.len())
            .filter(|index| emphasised("network", panel, *index, &inks, 1.0).is_some())
            .count();
        let nodes = panel
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item,
                    Item::Point {
                        role: Role::Mark,
                        obs: Some(_),
                        ..
                    }
                )
            })
            .count();
        assert!(nodes > 0 && lined == nodes && lined < panel.observations.len());
    }

    #[test]
    fn scatter_grows_with_a_tight_contour_and_packed_bubbles_keep_size() {
        let scatter = r#"{"spec":{"type":"scatter","title":"T","encoding":{"x":"a","y":"b"}},"rows":[{"a":1,"b":2},{"a":2,"b":4},{"a":3,"b":1}]}"#;
        let input: serde_json::Value = serde_json::from_str(scatter).unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        let radius = panel
            .items
            .iter()
            .find_map(|item| match item {
                Item::Point {
                    role: Role::Mark,
                    obs: Some(0),
                    radius,
                    ..
                } => Some(*radius),
                _ => None,
            })
            .unwrap();
        for (sized, grow) in [(false, 0.6), (true, 0.25)] {
            let drawn = items("scatter", panel, 0, geometry.background, 1.0, sized, false).unwrap();
            assert_eq!(drawn.len(), 2);
            assert!(drawn.iter().all(|item| matches!(item, Item::Point { .. })));
            for item in &drawn[..2] {
                assert!(matches!(item, Item::Point { radius: r, fill: Some(_), .. }
                    if (r - radius * (1.0 + grow)).abs() < 1e-9));
            }
        }
        assert_eq!(point_growth("packed-bubble", false), 0.0);
    }

    #[test]
    fn candlestick_wick_feedback_stays_outside_the_body() {
        let body = Rect::new(10.0, 20.0, 8.0, 12.0);
        for (wick, expected) in [
            (
                Path::polyline(&[[14.0, 10.0], [14.0, 40.0]]),
                vec![
                    vec![[14.0, 10.0], [14.0, 20.0]],
                    vec![[14.0, 32.0], [14.0, 40.0]],
                ],
            ),
            (
                Path::polyline(&[[0.0, 26.0], [25.0, 26.0]]),
                vec![
                    vec![[0.0, 26.0], [10.0, 26.0]],
                    vec![[18.0, 26.0], [25.0, 26.0]],
                ],
            ),
        ] {
            assert_eq!(exposed_wick(&wick, body).flatten(0.5), expected);
        }
    }

    #[test]
    fn tiled_observations_outline_only_their_own_tiles_without_halos() {
        for (kind, fixture) in [(
            "waffle",
            include_str!("../../../../../richview-chart/tests/fixtures/types/waffle.json"),
        )] {
            let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            for panel in &geometry.panels {
                for index in 0..panel.observations.len() {
                    let marks: Vec<_> = panel.items.iter().filter(|item| matches!(item,
                        Item::Rect { role: Role::Mark, obs: Some(k), .. } | Item::Area { role: Role::Mark, obs: Some(k), .. } if *k == index)).collect();
                    assert!(
                        !marks.is_empty(),
                        "{kind} observation {index} has no owned tile"
                    );
                    let outlines = tiles(kind, panel, index, 1.0).expect("a tiled family");
                    assert_eq!(
                        outlines.len(),
                        marks.len(),
                        "{kind} observation {index} has extra feedback or misses a tile"
                    );
                    for (mark, outline) in marks.into_iter().zip(outlines.iter()) {
                        match (mark, outline) {
                            (
                                Item::Rect {
                                    rect: a,
                                    fill,
                                    stroke: original,
                                    ..
                                },
                                Item::Rect {
                                    rect: b,
                                    stroke: Some(stroke),
                                    ..
                                },
                            ) => {
                                assert_eq!(a, b);
                                let own = original.as_ref().map_or(fill.color, |s| s.color);
                                assert_eq!(
                                    *stroke,
                                    Stroke::solid(own.with_alpha(own.alpha() * 0.22), 2.0)
                                );
                            }
                            (
                                Item::Area { path: a, fill, .. },
                                Item::Line {
                                    path: b, stroke, ..
                                },
                            ) => {
                                assert_eq!(a, b);
                                assert_eq!(
                                    *stroke,
                                    Stroke::solid(
                                        fill.color.with_alpha(fill.color.alpha() * 0.22),
                                        2.0
                                    )
                                );
                            }
                            _ => panic!("{kind} observation {index} has the wrong contour"),
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parallel_and_network_lines_keep_their_own_colour_on_hover() {
        for (kind, fixture) in [
            (
                "parallel",
                include_str!("../../../../../richview-chart/tests/fixtures/types/parallel.json"),
            ),
            (
                "network",
                include_str!("../../../../../richview-chart/tests/fixtures/types/network.json"),
            ),
        ] {
            let input: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            let panel = &geometry.panels[0];
            let lines: Vec<_> = panel
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Line {
                        path,
                        stroke,
                        role: Role::Mark,
                        obs: Some(index),
                        ..
                    } => Some((*index, path, stroke)),
                    _ => None,
                })
                .collect();
            assert!(!lines.is_empty(), "{kind} has selectable lines");
            for (index, path, stroke) in lines {
                let items =
                    items(kind, panel, index, geometry.background, 1.0, false, false).unwrap();
                assert!(
                    matches!(items.as_slice(),
                        [Item::Line { path: selected_path, stroke: selected, .. }]
                        if selected_path == path && (selected.width - stroke.width - 2.0).abs() < 1e-9
                            && selected.color == stroke.color.with_alpha(stroke.color.alpha() * 0.22)
                    ),
                    "{kind} must trace its path in its own colour: {items:?}"
                );
            }
        }
    }

    #[test]
    fn map_spots_answer_with_their_own_contour() {
        let spec = serde_json::json!({"type":"map", "title":"Offices", "encoding":{"lat":"la", "lon":"lo"}});
        let rows = vec![
            serde_json::json!({"la":50.09,"lo":14.42}),
            serde_json::json!({"la":-33.87,"lo":151.21}),
        ];
        let geometry = layout(
            &ChartBlock::from_value(&spec).unwrap(),
            &rows,
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        for index in 0..panel.observations.len() {
            let outlines =
                items("map", panel, index, geometry.background, 1.0, false, false).unwrap();
            assert_eq!(outlines.len(), 1);
            assert!(outlines.iter().all(|item| matches!(item,
                Item::Point { center, stroke: Some(_), fill: None, .. } if *center == panel.observations[index].at
            )));
        }
    }

    #[test]
    fn contour_grid_feedback_is_a_dot_at_the_selected_node() {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/contour.json"
        ))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let color = Rgba([0.3, 0.4, 0.5, 1.0]);
        for panel in &geometry.panels {
            for observation in &panel.observations {
                let mark = dot(observation.at, 1.0, color);
                assert!(
                    matches!(mark, Item::Point { center, radius: 6.0, shape: Shape::Circle, stroke: None, .. }
                    if center == observation.at)
                );
            }
        }
    }

    #[test]
    fn gauge_contours_the_band_named_by_the_needle() {
        let mut input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/gauge.json"
        ))
        .unwrap();
        for (value, expected) in [(25, 0), (50, 1), (72, 1), (100, 2)] {
            input["rows"][0]["score"] = serde_json::json!(value);
            let geometry = layout(
                &ChartBlock::from_value(&input["spec"]).unwrap(),
                input["rows"].as_array().unwrap(),
                &ChartTheme::default(),
                &Options::default(),
            );
            let panel = &geometry.panels[0];
            assert!(
                richview_chart::hit::at(&geometry, panel.observations[0].at).is_some(),
                "the gauge needle stays reachable at {value}"
            );
            assert!(
                richview_chart::hit::at(&geometry, panel.observations[0].area.unwrap().center())
                    .is_some(),
                "the gauge retains its original pointer area at {value}"
            );
            let bands: Vec<_> = panel
                .items
                .iter()
                .filter(|item| {
                    matches!(
                        item,
                        Item::Area {
                            role: Role::Mark,
                            ..
                        }
                    )
                })
                .take(3)
                .collect();
            assert_eq!(bands.len(), 3);
            let outlines =
                items("gauge", panel, 0, geometry.background, 1.0, false, false).unwrap();
            assert!(
                matches!((bands[expected], outlines.as_slice()), (Item::Area { path, .. }, [Item::Line { path: actual, .. }]) if path == actual)
            );
        }
        input["spec"]["style"]
            .as_object_mut()
            .unwrap()
            .remove("bands");
        input["rows"][0]["score"] = serde_json::json!(0);
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        let track = panel
            .items
            .iter()
            .find_map(|item| match item {
                Item::Area {
                    path,
                    role: Role::Band,
                    ..
                } => Some(path),
                _ => None,
            })
            .unwrap();
        let outlines = items("gauge", panel, 0, geometry.background, 1.0, false, false).unwrap();
        assert!(matches!(outlines.as_slice(), [Item::Line { path, .. }] if path == track));
    }

    #[test]
    fn flow_card_contour_traces_the_card_outline() {
        let input: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../richview-chart/tests/fixtures/types/flow.json"
        ))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let mut checked = 0;
        for panel in &geometry.panels {
            for index in 0..panel.observations.len() {
                // A card is a filled shape with its outline; a connection is a line alone.
                let filled = panel.items.iter().any(|item| {
                    matches!(item, Item::Area { role: Role::Mark, obs: Some(owner), .. } if *owner == index)
                });
                let Some(outline) = panel.items.iter().find_map(|item| match item {
                    Item::Line {
                        path,
                        role: Role::Mark,
                        obs: Some(owner),
                        ..
                    } if *owner == index && filled => Some(path),
                    _ => None,
                }) else {
                    continue;
                };
                checked += 1;
                let contour =
                    items("flow", panel, index, geometry.background, 1.0, false, false).unwrap();
                assert!(
                    contour
                        .iter()
                        .any(|item| matches!(item, Item::Line { path, .. } if path == outline)),
                    "card {index} is traced along its own outline"
                );
            }
        }
        assert!(checked > 0, "the fixture has cards");
    }

    #[test]
    fn chord_outlines_follow_the_selected_arc_and_ribbon() {
        let input: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../richview-chart/tests/fixtures/types/chord.json"
        )))
        .unwrap();
        let geometry = layout(
            &ChartBlock::from_value(&input["spec"]).unwrap(),
            input["rows"].as_array().unwrap(),
            &ChartTheme::default(),
            &Options::default(),
        );
        let panel = &geometry.panels[0];
        let mut matched = [false; 2];
        for (observation, o) in panel.observations.iter().enumerate() {
            let Some(path) = panel.items.iter().find_map(|item| match item {
                Item::Area {
                    path,
                    role: Role::Mark,
                    obs: Some(index),
                    ..
                } if *index == observation => Some(path),
                _ => None,
            }) else {
                continue;
            };
            matched[usize::from(o.wedge.is_some())] = true;
            let outlines = items(
                "chord",
                panel,
                observation,
                geometry.background,
                1.0,
                false,
                false,
            )
            .unwrap();
            assert!(
                outlines
                    .iter()
                    .any(|item| matches!(item, Item::Line { path: actual, .. } if actual == path)),
                "observation {observation}"
            );
        }
        assert_eq!(matched, [true, true]);
    }
}
