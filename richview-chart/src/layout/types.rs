//! The table of chart types (TYP-14). A type is one row: its name, default ratios, entrance, what
//! more width gives it and the painter that lays it out. Adding a type adds a painter module and a
//! row here, and no other engine file changes; core's registry lists the same name and its
//! channels (ARC-09).

use super::alluvial;
use super::band;
use super::bar3d;
use super::bin2d;
use super::bullet;
use super::calendar;
use super::cartesian;
use super::chord;
use super::combo;
use super::contour;
use super::density;
use super::dotplot;
use super::dumbbell;
use super::ecdf;
use super::financial;
use super::flow;
use super::funnel;
use super::gantt;
use super::gauge;
use super::heatmap;
use super::hexbin;
use super::icicle;
use super::lollipop;
use super::map;
use super::network;
use super::packed_bubble;
use super::parallel;
use super::pareto;
use super::pie;
use super::pivot;
use super::qq;
use super::radar;
use super::ribbon;
use super::rose;
use super::sankey;
use super::scatter3d;
use super::splom;
use super::stats;
use super::streamgraph;
use super::strip;
use super::sunburst;
use super::surface;
use super::tree;
use super::treemap;
use super::vector;
use super::venn;
use super::violin;
use super::waffle;
use super::waterfall;
use super::wordcloud;
use super::{Ctx, Painted};
use crate::block::{ChartType, Wider};
use crate::geometry::Entrance;

/// Facet panels in source order, each with its row indices.
pub(crate) type Facets = [(Option<String>, Vec<usize>)];

pub(crate) type Paint = fn(&mut Ctx, &Facets) -> Painted;

pub(crate) struct TypeDef {
    pub name: &'static str,
    /// Width over height when the author gives none.
    pub ratio: f64,
    /// The same for a calendar x axis (line and area only).
    pub calendar_ratio: f64,
    /// The entrance the chart plays once (FEA-CH-51); marks past 2,000 fade in instead.
    pub entrance: Entrance,
    /// What more width gives the chart; only one that shows more may turn on a phone (FEA-CH-68).
    pub wider: Wider,
    pub paint: Paint,
}

const WIDE: f64 = 16.0 / 9.0;

const fn row(name: &'static str, ratio: f64, entrance: Entrance, wider: Wider, paint: Paint) -> TypeDef {
    TypeDef {
        name,
        ratio,
        calendar_ratio: ratio,
        entrance,
        wider,
        paint,
    }
}

/// Registry order. The first nine are the constants of [`ChartType`]; do not reorder them.
pub(crate) static DEFS: &[TypeDef] = &[
    TypeDef {
        calendar_ratio: 2.0,
        ..row("line", WIDE, Entrance::Sweep, Wider::ShowsMore, cartesian::layout)
    },
    TypeDef {
        calendar_ratio: 2.0,
        ..row("area", WIDE, Entrance::Sweep, Wider::ShowsMore, cartesian::layout)
    },
    row("bar", WIDE, Entrance::Grow, Wider::ShowsMore, cartesian::layout),
    row(
        "scatter",
        4.0 / 3.0,
        Entrance::Settle,
        Wider::ShowsMore,
        cartesian::layout,
    ),
    row("pie", 1.0, Entrance::Fan, Wider::Enlarges, pie::layout),
    row("heatmap", WIDE, Entrance::Grow, Wider::ShowsMore, heatmap::layout),
    row("histogram", WIDE, Entrance::Grow, Wider::ShowsMore, stats::layout),
    row("boxplot", WIDE, Entrance::Grow, Wider::ShowsMore, stats::layout),
    row("interval", WIDE, Entrance::Grow, Wider::ShowsMore, stats::layout),
    row("waterfall", WIDE, Entrance::Grow, Wider::ShowsMore, waterfall::layout),
    row("map", 2.0, Entrance::Fade, Wider::Enlarges, map::layout),
    row("band", WIDE, Entrance::Sweep, Wider::ShowsMore, band::layout),
    row("treemap", 4.0 / 3.0, Entrance::Grow, Wider::Enlarges, treemap::layout),
    row("lollipop", WIDE, Entrance::Grow, Wider::ShowsMore, lollipop::layout),
    row("dumbbell", WIDE, Entrance::Grow, Wider::ShowsMore, dumbbell::layout),
    row("funnel", 4.0 / 3.0, Entrance::Grow, Wider::Enlarges, funnel::layout),
    row("sunburst", 1.0, Entrance::Fan, Wider::Enlarges, sunburst::layout),
    row("radar", 1.0, Entrance::Settle, Wider::Enlarges, radar::layout),
    row("tree", WIDE, Entrance::Fade, Wider::Enlarges, tree::layout),
    row("icicle", WIDE, Entrance::Grow, Wider::Enlarges, icicle::layout),
    row("ecdf", WIDE, Entrance::Sweep, Wider::ShowsMore, ecdf::layout),
    row("bullet", WIDE, Entrance::Grow, Wider::ShowsMore, bullet::layout),
    row(
        "candlestick",
        WIDE,
        Entrance::Grow,
        Wider::ShowsMore,
        financial::candlestick,
    ),
    row("ohlc", WIDE, Entrance::Grow, Wider::ShowsMore, financial::ohlc),
    row("pareto", WIDE, Entrance::Grow, Wider::ShowsMore, pareto::layout),
    row("waffle", 1.0, Entrance::Grow, Wider::Enlarges, waffle::layout),
    row("rose", 1.0, Entrance::Fan, Wider::Enlarges, rose::layout),
    row("combo", WIDE, Entrance::Grow, Wider::ShowsMore, combo::layout),
    row("density", WIDE, Entrance::Sweep, Wider::ShowsMore, density::layout),
    row("qq", 1.0, Entrance::Settle, Wider::ShowsMore, qq::layout),
    row("strip", WIDE, Entrance::Settle, Wider::ShowsMore, strip::layout),
    row("violin", WIDE, Entrance::Grow, Wider::ShowsMore, violin::layout),
    row("dotplot", 1.6, Entrance::Settle, Wider::ShowsMore, dotplot::layout),
    row("bin2d", WIDE, Entrance::Grow, Wider::ShowsMore, bin2d::layout),
    row("hexbin", WIDE, Entrance::Grow, Wider::ShowsMore, hexbin::layout),
    row("gauge", 1.2, Entrance::Fan, Wider::Enlarges, gauge::layout),
    row("calendar", 3.0, Entrance::Grow, Wider::ShowsMore, calendar::layout),
    row(
        "streamgraph",
        WIDE,
        Entrance::Sweep,
        Wider::ShowsMore,
        streamgraph::layout,
    ),
    row("contour", WIDE, Entrance::Fade, Wider::ShowsMore, contour::layout),
    row(
        "packed-bubble",
        4.0 / 3.0,
        Entrance::Fade,
        Wider::Enlarges,
        packed_bubble::layout,
    ),
    row("wordcloud", WIDE, Entrance::Fade, Wider::Enlarges, wordcloud::layout),
    row("venn", 4.0 / 3.0, Entrance::Fade, Wider::Enlarges, venn::layout),
    row("vector", WIDE, Entrance::Fade, Wider::ShowsMore, vector::layout),
    row("ribbon", WIDE, Entrance::Grow, Wider::ShowsMore, ribbon::layout),
    row("gantt", WIDE, Entrance::Grow, Wider::ShowsMore, gantt::layout),
    row("parallel", WIDE, Entrance::Sweep, Wider::ShowsMore, parallel::layout),
    row("splom", 1.0, Entrance::Settle, Wider::ShowsMore, splom::layout),
    row("sankey", WIDE, Entrance::Sweep, Wider::Enlarges, sankey::layout),
    row("alluvial", WIDE, Entrance::Sweep, Wider::Enlarges, alluvial::layout),
    row("chord", 4.0 / 3.0, Entrance::Fan, Wider::Enlarges, chord::layout),
    row("network", WIDE, Entrance::Settle, Wider::Enlarges, network::layout),
    row("flow", 4.0 / 3.0, Entrance::Fade, Wider::Enlarges, flow::layout),
    row("pivot", 2.0, Entrance::Fade, Wider::ShowsMore, pivot::layout),
    row("surface", 4.0 / 3.0, Entrance::Fade, Wider::Enlarges, surface::layout),
    row(
        "scatter3d",
        4.0 / 3.0,
        Entrance::Fade,
        Wider::Enlarges,
        scatter3d::layout,
    ),
    row("bar3d", 4.0 / 3.0, Entrance::Fade, Wider::Enlarges, bar3d::layout),
];

pub(crate) fn def(kind: ChartType) -> &'static TypeDef {
    &DEFS[kind.0 as usize]
}
