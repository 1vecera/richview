//! The schema registry (ARC-09, TYP-03): the one definition of RichView's vocabulary. Tags and
//! their fields, frontmatter fields, chart types and diagnostic codes are static data here;
//! validators, `learn`, completion, MCP schemas and the reference tables in the spec read or are
//! generated from it, and never enumerate the vocabulary themselves.
//!
//! The data holds no behaviour: a field's cross-field rules stay with its validator. Paths follow
//! the diagnostic path grammar (`encoding.y`, `data.inline`), relative to the block or to
//! `frontmatter`.

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    /// Plain text.
    Text,
    /// Inline Markdown text.
    Markdown,
    /// A finite number.
    Number,
    /// A whole number.
    Integer,
    /// Significant digits, not decimal places: a whole number from 2 to 6 (TYP-23, FEA-CH-37).
    Digits,
    Bool,
    /// One of the listed words.
    OneOf(&'static [&'static str]),
    /// A list whose items have the given type.
    List(&'static Ty),
    /// A mapping whose own fields are listed separately under this path.
    Map,
    /// A data source: `inline` rows, a `csv` or `json` file, or a named source.
    Data,
    /// The name of a column in the block's data.
    Column,
    /// A colour in the style colour grammar.
    Color,
    /// A report-folder file path.
    File,
    /// An `http` or `https` address.
    Url,
    /// A date, `YYYY-MM-DD`.
    Date,
    /// A string, a number or a boolean; `encoding.highlight` values match rows by any of them.
    Scalar,
    /// A number or a string; a `big` block's `value` is either.
    NumberOrText,
    /// A positive number, or a `W:H` string such as `16:9`.
    Ratio,
    /// The name of a chart type in `CHART_TYPES`.
    ChartType,
    /// Any value; the field's own validator checks its shape.
    Any,
    /// A value of either type: a bare string for a list, a string for a mapping.
    Either(&'static Ty, &'static Ty),
}

/// One field of a tag or of the frontmatter.
#[derive(Debug, Clone, Copy)]
pub struct Field {
    /// The field's path below its tag, such as `encoding.y`.
    pub path: &'static str,
    pub ty: Ty,
    /// For a nested path, required whenever its parent is given.
    pub required: bool,
    /// The value an omitted field takes, as the author would write it.
    pub default: Option<&'static str>,
    /// One sentence saying what the field does.
    pub summary: &'static str,
}

/// A rich fence tag, such as `chart`.
#[derive(Debug, Clone, Copy)]
pub struct Tag {
    pub name: &'static str,
    pub summary: &'static str,
    pub fields: &'static [Field],
}

/// A chart type and the encoding channels it reads.
#[derive(Debug, Clone, Copy)]
pub struct ChartType {
    pub name: &'static str,
    pub summary: &'static str,
    /// Channels under `encoding`, required ones first.
    pub encodings: &'static [&'static str],
    pub required: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    Info,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warning => "warning",
            Level::Info => "info",
        }
    }
}

/// A diagnostic code (TYP-55): its level never changes, and renaming or removing one is a
/// format change.
#[derive(Debug, Clone, Copy)]
pub struct Code {
    pub code: &'static str,
    pub level: Level,
    /// What the diagnostic means, in one sentence.
    pub meaning: &'static str,
    /// Whether it carries a source line.
    pub line: bool,
}

/// The catalogue entry for `code`.
pub fn code(code: &str) -> Option<&'static Code> {
    CODES.iter().find(|entry| entry.code == code)
}

/// The tag named `name`.
pub fn tag(name: &str) -> Option<&'static Tag> {
    TAGS.iter().find(|tag| tag.name == name)
}

/// The chart type named `name`.
pub(crate) fn chart_type(name: &str) -> Option<&'static ChartType> {
    CHART_TYPES.iter().find(|chart| chart.name == name)
}

/// One `Field`: path, type, required, default, summary.
macro_rules! field {
    ($path:expr, $ty:expr, $required:expr, $default:expr, $summary:expr) => {
        Field {
            path: $path,
            ty: $ty,
            required: $required,
            default: $default,
            summary: $summary,
        }
    };
}

/// One `Code`: code, level, meaning, whether it carries a line.
macro_rules! code {
    ($code:expr, $level:ident, $meaning:expr, $line:expr) => {
        Code {
            code: $code,
            level: Level::$level,
            meaning: $meaning,
            line: $line,
        }
    };
}

const NO: Option<&str> = None;

/// The one value-format vocabulary of table columns, metrics and chart axes (TYP-23); `year`
/// prints a whole number without grouping, so a year never reads `2,024` (FEA-CH-21).
const FORMATS: &[&str] = &["number", "number_compact", "percent", "year"];
/// The scales a table column's `heat` names (FEA-RP-69); `true` is `sequential`.
const HEAT_SCALES: &[&str] = &["sequential", "diverging"];
/// The built-in styles, the values of `styles` and `style.palette` (STY-05).
pub(crate) const BUILTIN_STYLES: &[&str] =
    &["air", "brutalist", "hand-drawn", "scientific", "sleek"];

/// The hosts an embed may load by default; they also get the players' sandbox rights (TYP-33).
pub(crate) const EMBED_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "youtube-nocookie.com",
    "vimeo.com",
    "player.vimeo.com",
    "gist.github.com",
    "codepen.io",
];

const AXIS_LOCALES: &[&str] = &["en-US", "en-GB", "cs-CZ", "de-DE", "ja-JP"];
const TREND: &[&str] = &["up", "down", "flat"];
const FIT: &[&str] = &["contain", "cover", "fill"];

/// The fields of `metric`.
const METRIC_FIELDS: &[Field] = &[
    field!(
        "value",
        Ty::NumberOrText,
        true,
        NO,
        "The headline number or text the block shows."
    ),
    field!("label", Ty::Text, false, NO, "The caption under the value."),
    field!("unit", Ty::Text, false, NO, "A unit shown with the value."),
    field!(
        "format",
        Ty::OneOf(FORMATS),
        false,
        NO,
        "The number format of the value."
    ),
    field!(
        "delta",
        Ty::Number,
        false,
        NO,
        "The change since the last period as a fraction, shown beside the value as a signed percentage: 0.12 prints +12.0%."
    ),
    field!(
        "delta_label",
        Ty::Text,
        false,
        NO,
        "The caption of the change."
    ),
    field!(
        "digits",
        Ty::Digits,
        false,
        NO,
        "Significant digits of the value and the change."
    ),
    field!(
        "trend",
        Ty::OneOf(TREND),
        false,
        NO,
        "The delta's colour, `up` for good news and `down` for bad; it defaults to the sign of the delta."
    ),
];

pub const TAGS: &[Tag] = &[
    Tag {
        name: "chart",
        summary: "A chart drawn from block-local data.",
        fields: &[
            field!("type", Ty::ChartType, true, NO, "The chart type."),
            field!(
                "title",
                Ty::Text,
                true,
                NO,
                "The title that states the claim; it is the caption heading and the chart's accessible name."
            ),
            field!("subtitle", Ty::Text, false, NO, "A line under the title."),
            field!(
                "stack",
                Ty::Bool,
                false,
                Some("false"),
                "Stacks the series of a bar or area chart; needs `encoding.color`."
            ),
            field!(
                "wide",
                Ty::Bool,
                false,
                Some("false"),
                "Draws the chart at the full report width."
            ),
            field!(
                "auto_annotations",
                Ty::Bool,
                false,
                Some("true"),
                "Prints the values and point names a chart adds by itself, such as bar and line values, heatmap cell values, pie shares and a scatter's point names; `false` prints none of them and leaves the authored `annotations`, the `highlight` names and the series and category names."
            ),
            field!(
                "digits",
                Ty::Digits,
                false,
                NO,
                "Significant digits of the chart's printed numbers, not decimal places. Omit to use the default."
            ),
            field!(
                "readout_digits",
                Ty::Digits,
                false,
                NO,
                "Significant digits of the hover readout, not decimal places; never fewer than `digits`. Omit to use the chart precision."
            ),
            field!(
                "highlight",
                Ty::Map,
                false,
                NO,
                "Makes some series, categories or points the subject of a chart: they keep their colours, names and values, and with `mute` the rest draw in the style's quiet ink without the names and values the chart adds by itself."
            ),
            field!(
                "highlight.by",
                Ty::Column,
                true,
                NO,
                "The column whose values `include` matches."
            ),
            field!(
                "highlight.include",
                Ty::List(&Ty::Scalar),
                true,
                NO,
                "The values to label; at least one."
            ),
            field!(
                "highlight.priority",
                Ty::List(&Ty::Scalar),
                false,
                NO,
                "Values that win a label slot first."
            ),
            field!(
                "highlight.mute",
                Ty::Bool,
                false,
                Some("true; false on scatter"),
                "Whether the marks `include` does not match recede into the quiet ink: on a line or area chart the series without a matching row, on a bar or lollipop chart the bars whose row does not match, on a scatter the points. `false` keeps every mark's colour and only names the highlighted."
            ),
            field!(
                "data",
                Ty::Data,
                true,
                NO,
                "The chart's data; exactly one local source is allowed."
            ),
            field!(
                "data.inline",
                Ty::List(&Ty::Map),
                false,
                NO,
                "Rows written in the block, each a mapping of column to value."
            ),
            field!(
                "data.csv",
                Ty::File,
                false,
                NO,
                "A CSV file inside the report folder."
            ),
            field!(
                "data.json",
                Ty::File,
                false,
                NO,
                "A JSON file of row mappings inside the report folder."
            ),
            field!(
                "data.parquet",
                Ty::File,
                false,
                NO,
                "A Parquet file inside the report folder; the native host reads only the columns the report uses."
            ),
            field!(
                "data.url",
                Ty::Url,
                false,
                NO,
                "An https data source; current native and browser hosts do not load this field."
            ),
            field!(
                "data.sql",
                Ty::Map,
                false,
                NO,
                "A SQL query run through a named connector on a host that has a registry."
            ),
            field!(
                "data.sql.connector",
                Ty::Text,
                true,
                NO,
                "The name of the connector that runs the query."
            ),
            field!(
                "data.sql.query",
                Ty::File,
                false,
                NO,
                "A report-relative file holding the SQL; set this or inline, not both."
            ),
            field!(
                "data.sql.inline",
                Ty::Text,
                false,
                NO,
                "The SQL text itself; set this or query, not both."
            ),
            field!(
                "data.metricflow",
                Ty::Map,
                false,
                NO,
                "A MetricFlow query run through a named connector."
            ),
            field!(
                "data.metricflow.connector",
                Ty::Text,
                true,
                NO,
                "The name of the connector that runs the query."
            ),
            field!(
                "data.metricflow.metrics",
                Ty::List(&Ty::Text),
                true,
                NO,
                "The metric names to query; at least one."
            ),
            field!(
                "data.cache",
                Ty::Map,
                false,
                NO,
                "A reference to a cached connector result."
            ),
            field!(
                "data.cache.key",
                Ty::Text,
                true,
                NO,
                "The key of the cached result."
            ),
            field!(
                "data.sql.params",
                Ty::Map,
                false,
                NO,
                "Named values passed to the query."
            ),
            field!(
                "data.sql.cache",
                Ty::Map,
                false,
                NO,
                "How long the query's result is reused."
            ),
            field!(
                "data.sql.cache.key",
                Ty::Text,
                false,
                NO,
                "The key of the cached result."
            ),
            field!(
                "data.sql.cache.ttl",
                Ty::Text,
                false,
                NO,
                "How long the result stays fresh, such as 1h."
            ),
            field!(
                "data.sql.cache.freshness",
                Ty::Text,
                false,
                NO,
                "A freshness rule for the result."
            ),
            field!(
                "data.sql.cache.refresh",
                Ty::Bool,
                false,
                NO,
                "Whether every run refreshes the result."
            ),
            field!(
                "data.metricflow.group_by",
                Ty::List(&Ty::Text),
                false,
                NO,
                "The dimensions to group by."
            ),
            field!(
                "data.metricflow.where",
                Ty::List(&Ty::Text),
                false,
                NO,
                "Filters on the query."
            ),
            field!(
                "data.metricflow.order_by",
                Ty::List(&Ty::Text),
                false,
                NO,
                "The order of the rows."
            ),
            field!(
                "data.metricflow.limit",
                Ty::Integer,
                false,
                NO,
                "The most rows returned."
            ),
            field!(
                "data.metricflow.cache",
                Ty::Map,
                false,
                NO,
                "How long the query's result is reused."
            ),
            field!(
                "data.metricflow.cache.key",
                Ty::Text,
                false,
                NO,
                "The key of the cached result."
            ),
            field!(
                "data.metricflow.cache.ttl",
                Ty::Text,
                false,
                NO,
                "How long the result stays fresh, such as 15m."
            ),
            field!(
                "data.metricflow.cache.freshness",
                Ty::Text,
                false,
                NO,
                "A freshness rule for the result."
            ),
            field!(
                "data.metricflow.cache.refresh",
                Ty::Bool,
                false,
                NO,
                "Whether every run refreshes the result."
            ),
            field!(
                "data.cache.ttl",
                Ty::Text,
                false,
                NO,
                "How long the result stays fresh."
            ),
            field!(
                "encoding",
                Ty::Map,
                true,
                NO,
                "Maps data columns to chart channels."
            ),
            field!(
                "encoding.x",
                Ty::Column,
                false,
                NO,
                "The column on the x axis or the categories."
            ),
            field!(
                "encoding.y",
                Ty::Column,
                false,
                NO,
                "The column that gives the values."
            ),
            field!(
                "encoding.lower",
                Ty::Column,
                false,
                NO,
                "The column with each interval's lower bound; interval charts only."
            ),
            field!(
                "encoding.upper",
                Ty::Column,
                false,
                NO,
                "The column with each interval's upper bound; interval charts only."
            ),
            field!(
                "encoding.color",
                Ty::Column,
                false,
                NO,
                "The column whose distinct values become series."
            ),
            field!(
                "encoding.size",
                Ty::Column,
                false,
                NO,
                "The column that sizes bubbles on a scatter chart, or gives the length and colour of the arrows of a vector chart."
            ),
            field!(
                "encoding.label",
                Ty::Column,
                false,
                NO,
                "The column whose values label scatter points."
            ),
            field!(
                "encoding.facet",
                Ty::Column,
                false,
                NO,
                "The column whose distinct values split the chart into panels."
            ),
            field!(
                "encoding.target",
                Ty::Column,
                false,
                NO,
                "Bullet: the column that gives each target. Sankey, chord, network and flow: the column with the node each link ends at."
            ),
            field!(
                "encoding.dimensions",
                Ty::List(&Ty::Column),
                false,
                NO,
                "Alluvial: the columns of categories, in order from left to right. Parallel coordinates and scatterplot matrix: the numeric columns drawn as axes. At least two."
            ),
            field!(
                "encoding.source",
                Ty::Column,
                false,
                NO,
                "Sankey, chord, network and flow: the column with the node each link starts at."
            ),
            field!(
                "encoding.open",
                Ty::Column,
                false,
                NO,
                "Candlestick and OHLC: the opening value."
            ),
            field!(
                "encoding.high",
                Ty::Column,
                false,
                NO,
                "Candlestick and OHLC: the highest value."
            ),
            field!(
                "encoding.low",
                Ty::Column,
                false,
                NO,
                "Candlestick and OHLC: the lowest value."
            ),
            field!(
                "encoding.close",
                Ty::Column,
                false,
                NO,
                "Candlestick and OHLC: the closing value."
            ),
            field!(
                "encoding.region",
                Ty::Column,
                false,
                NO,
                "Map only: the column naming each region."
            ),
            field!(
                "encoding.line",
                Ty::Column,
                false,
                NO,
                "Combo only: the column drawn as a line on the second axis."
            ),
            field!(
                "encoding.start",
                Ty::Column,
                false,
                NO,
                "Gantt only: the column holding each task's start, as a date or a number."
            ),
            field!(
                "encoding.end",
                Ty::Column,
                false,
                NO,
                "Gantt only: the column holding each task's end, as a date or a number of the same kind as the start."
            ),
            field!(
                "encoding.rows",
                Ty::Column,
                false,
                NO,
                "Pivot only: the column whose values head the rows of the matrix."
            ),
            field!(
                "encoding.columns",
                Ty::Column,
                false,
                NO,
                "Pivot only: the column whose values head the columns of the matrix."
            ),
            field!(
                "encoding.values",
                Ty::Column,
                false,
                NO,
                "Pivot only: the numeric column summed into each cell."
            ),
            field!(
                "encoding.detail",
                Ty::List(&Ty::Column),
                false,
                NO,
                "Extra columns shown in readouts, Table views and downloads."
            ),
            field!(
                "encoding.measure",
                Ty::Column,
                false,
                NO,
                "The column that marks each waterfall row as `delta` (a step), `start` or `total`."
            ),
            field!(
                "encoding.z",
                Ty::Column,
                false,
                NO,
                "Contour, surface and 3-D scatter: the numeric column drawn as iso-lines or as the third coordinate."
            ),
            field!(
                "encoding.sets",
                Ty::Column,
                false,
                NO,
                "Venn only: the column naming the sets of each row, joined by `&` or a comma, such as `A&B`."
            ),
            field!(
                "encoding.angle",
                Ty::Column,
                false,
                NO,
                "Vector only: the column holding each arrow's direction in degrees, counter-clockwise from east."
            ),
            field!(
                "encoding.lane",
                Ty::Column,
                false,
                NO,
                "Flow only: the column naming the lane, drawn as a band, that each step sits in."
            ),
            field!(
                "encoding.parent",
                Ty::Column,
                false,
                NO,
                "The column that names each row's parent tile in a treemap."
            ),
            field!(
                "controls",
                Ty::List(&Ty::Map),
                false,
                NO,
                "Reader controls drawn above the chart, in author order."
            ),
            field!(
                "controls[n].kind",
                Ty::OneOf(&["playback", "filter", "metric", "variant"]),
                true,
                NO,
                "What the control changes."
            ),
            field!(
                "controls[n].id",
                Ty::Text,
                false,
                NO,
                "The control's name, unique in the chart."
            ),
            field!(
                "controls[n].field",
                Ty::Column,
                false,
                NO,
                "The data column a playback or filter control steps through."
            ),
            field!(
                "controls[n].label",
                Ty::Text,
                false,
                NO,
                "The control's caption."
            ),
            field!(
                "controls[n].options",
                Ty::Any,
                false,
                NO,
                "The choices: values, or mappings with value, label and the field, axis or chart each applies."
            ),
            field!(
                "controls[n].initial",
                Ty::Scalar,
                false,
                NO,
                "The choice shown first."
            ),
            field!(
                "controls[n].all",
                Ty::Text,
                false,
                NO,
                "The label of a filter's choice that shows every row."
            ),
            field!(
                "controls[n].speed",
                Ty::Number,
                false,
                Some("800"),
                "Milliseconds per frame of a playback control."
            ),
            field!(
                "controls[n].loop",
                Ty::Bool,
                false,
                NO,
                "Accepted for older reports. Playback always stops on the last frame."
            ),
            field!(
                "controls[n].as",
                Ty::OneOf(&["buttons", "dropdown"]),
                false,
                NO,
                "How the choices are drawn."
            ),
            field!(
                "binning",
                Ty::Map,
                false,
                NO,
                "Histogram bin options; other chart types refuse it."
            ),
            field!(
                "binning.count",
                Ty::Integer,
                false,
                Some("Sturges' rule"),
                "The number of equal-width bins, 1 to 256."
            ),
            field!(
                "binning.edges",
                Ty::List(&Ty::Number),
                false,
                NO,
                "Explicit bin edges, 2 to 257 increasing values; not combined with count or extent."
            ),
            field!(
                "binning.extent",
                Ty::List(&Ty::Number),
                false,
                NO,
                "The lower and upper bound of the bins, two increasing values."
            ),
            field!(
                "binning.measure",
                Ty::OneOf(&["count", "proportion", "density"]),
                false,
                Some("count"),
                "What bin heights show."
            ),
            field!("axes", Ty::Map, false, NO, "Axis options."),
            field!("axes.x", Ty::Map, false, NO, "Options of the x axis."),
            field!("axes.x.label", Ty::Text, false, NO, "The x axis title."),
            field!(
                "axes.x.format",
                Ty::OneOf(FORMATS),
                false,
                NO,
                "The number format of x values."
            ),
            field!(
                "axes.x.unit",
                Ty::Text,
                false,
                NO,
                "A unit printed with x values, by the unit rule."
            ),
            field!(
                "axes.x.digits",
                Ty::Digits,
                false,
                NO,
                "Significant digits of x values on this axis, from 2 to 6."
            ),
            field!(
                "axes.x.locale",
                Ty::OneOf(AXIS_LOCALES),
                false,
                NO,
                "The locale of x values and calendar labels; the report's lang otherwise."
            ),
            field!("axes.y", Ty::Map, false, NO, "Options of the value axis."),
            field!("axes.y.label", Ty::Text, false, NO, "The value axis title."),
            field!(
                "axes.y.format",
                Ty::OneOf(FORMATS),
                false,
                NO,
                "The number format of values."
            ),
            field!(
                "axes.y.unit",
                Ty::Text,
                false,
                NO,
                "A unit printed with values, by the unit rule."
            ),
            field!(
                "axes.y.digits",
                Ty::Digits,
                false,
                NO,
                "Significant digits of values on this axis, from 2 to 6."
            ),
            field!(
                "axes.y.min",
                Ty::Number,
                false,
                NO,
                "The lowest value the axis shows; below axes.y.max."
            ),
            field!(
                "axes.y.max",
                Ty::Number,
                false,
                NO,
                "The highest value the axis shows."
            ),
            field!(
                "axes.y2",
                Ty::Map,
                false,
                NO,
                "Combo charts: options of the second value axis, at the right."
            ),
            field!(
                "axes.y2.label",
                Ty::Text,
                false,
                NO,
                "The second axis title."
            ),
            field!(
                "axes.y2.format",
                Ty::OneOf(FORMATS),
                false,
                NO,
                "The number format of the second axis."
            ),
            field!(
                "axes.y2.unit",
                Ty::Text,
                false,
                NO,
                "A unit printed with the second axis's values."
            ),
            field!(
                "axes.y2.digits",
                Ty::Integer,
                false,
                NO,
                "Significant digits of values on the second axis."
            ),
            field!(
                "axes.y2.min",
                Ty::Number,
                false,
                NO,
                "The lowest value the second axis shows."
            ),
            field!(
                "axes.y2.max",
                Ty::Number,
                false,
                NO,
                "The highest value the second axis shows."
            ),
            field!(
                "axes.color",
                Ty::Map,
                false,
                NO,
                "Options of the colour scale or key."
            ),
            field!(
                "axes.color.label",
                Ty::Text,
                false,
                NO,
                "The colour key title."
            ),
            field!(
                "axes.color.format",
                Ty::OneOf(FORMATS),
                false,
                NO,
                "The number format of colour values."
            ),
            field!(
                "axes.color.unit",
                Ty::Text,
                false,
                NO,
                "A unit printed with colour values, by the unit rule."
            ),
            field!(
                "axes.color.digits",
                Ty::Digits,
                false,
                NO,
                "Significant digits of colour values on this axis, from 2 to 6."
            ),
            field!(
                "annotations",
                Ty::List(&Ty::Map),
                false,
                NO,
                "Marks added on top of the data."
            ),
            field!(
                "annotations[n].kind",
                Ty::OneOf(&["point", "range", "line", "band", "text"]),
                true,
                NO,
                "What the annotation draws."
            ),
            field!(
                "annotations[n].at",
                Ty::Map,
                false,
                NO,
                "The coordinate of a point, line or text annotation; required for those kinds."
            ),
            field!(
                "annotations[n].from",
                Ty::Map,
                false,
                NO,
                "The start coordinate of a range or band; required for those kinds."
            ),
            field!(
                "annotations[n].to",
                Ty::Map,
                false,
                NO,
                "The end coordinate of a range or band; required for those kinds."
            ),
            field!(
                "annotations[n].text",
                Ty::Text,
                false,
                NO,
                "The annotation's text; required for point and text annotations."
            ),
            field!(
                "annotations[n].rotate",
                Ty::Number,
                false,
                NO,
                "The rotation of a text annotation."
            ),
            field!(
                "direction",
                Ty::OneOf(&["down", "right"]),
                false,
                Some("down"),
                "Flow only: whether the flow runs down the page or across it; across turns down where it does not fit."
            ),
            field!(
                "nodes",
                Ty::List(&Ty::Map),
                false,
                NO,
                "Flow only: the cards, in reading order; with nodes the chart takes no `data` or `encoding`."
            ),
            field!(
                "nodes[n].title",
                Ty::Text,
                true,
                NO,
                "The card's title: a verb and an object for a step, a short question for a decision."
            ),
            field!(
                "nodes[n].id",
                Ty::Text,
                false,
                NO,
                "The name edges and groups use for the card; the title in lower case with hyphens when omitted."
            ),
            field!(
                "nodes[n].lines",
                Ty::Either(&Ty::Text, &Ty::List(&Ty::Text)),
                false,
                NO,
                "The two or three facts under the title, one per line."
            ),
            field!(
                "nodes[n].kind",
                Ty::OneOf(&["step", "decision", "start", "end", "store", "object"]),
                false,
                Some("step"),
                "What the card stands for, which sets its outline: a rounded card, a diamond, a pill for a start or an end, a cylinder for stored data, a sheet with a folded corner for a thing made."
            ),
            field!(
                "nodes[n].tone",
                Ty::OneOf(&["alert", "muted"]),
                false,
                NO,
                "`alert` for what needs the reader's eye, such as a failure; `muted` for what sits outside the subject."
            ),
            field!(
                "edges",
                Ty::List(&Ty::Either(&Ty::Text, &Ty::Map)),
                false,
                NO,
                "Flow only: one line each, `from -> to`, dashed `from ..> to`, with `: label` to name it; ends are ids or titles."
            ),
            field!(
                "edges[n].edge",
                Ty::Text,
                true,
                NO,
                "The edge as `from -> to: label`, when it also takes a tone."
            ),
            field!(
                "edges[n].tone",
                Ty::OneOf(&["alert", "muted"]),
                false,
                NO,
                "`alert` marks the path the reader should follow, such as a failure."
            ),
            field!(
                "groups",
                Ty::List(&Ty::Map),
                false,
                NO,
                "Flow only: cards that belong together, drawn in one box with its label in the border."
            ),
            field!(
                "groups[n].label",
                Ty::Text,
                true,
                NO,
                "What the group is, set into its top border."
            ),
            field!(
                "groups[n].nodes",
                Ty::List(&Ty::Text),
                true,
                NO,
                "The ids or titles of the group's cards; a card sits in one group."
            ),
            field!(
                "groups[n].tone",
                Ty::OneOf(&["alert", "muted"]),
                false,
                NO,
                "The tone of every card in the group."
            ),
            field!("style", Ty::Map, false, NO, "Chart styling."),
            field!(
                "style.palette",
                Ty::OneOf(BUILTIN_STYLES),
                false,
                NO,
                "The colour palette of the marks."
            ),
            field!(
                "style.ratio",
                Ty::Ratio,
                false,
                NO,
                "The width to height ratio of the chart."
            ),
            field!(
                "style.orientation",
                Ty::OneOf(&["auto", "vertical", "horizontal"]),
                false,
                NO,
                "Whether bars and boxes stand or lie."
            ),
            field!(
                "style.curve",
                Ty::OneOf(&["linear", "monotone", "step", "step_before", "step_after"]),
                false,
                NO,
                "How line and area marks connect points; the default is `monotone`."
            ),
            field!(
                "style.curvature",
                Ty::Number,
                false,
                NO,
                "How strongly curves bend, from 0 to 1."
            ),
            field!(
                "style.points",
                Ty::OneOf(&["all", "m4", "band"]),
                false,
                NO,
                "Explicit `all` draws every observation without a ceiling; `m4` keeps per-pixel extrema and `band` dense means/ranges for line and area. When omitted, measured ceilings trigger disclosed representatives. Scatter accepts explicit `all` only."
            ),
            field!(
                "style.facet_scale",
                Ty::OneOf(&["shared", "free"]),
                false,
                NO,
                "Whether facet panels share one value scale (`shared`, the default) or each draws its own (`free`)."
            ),
            field!(
                "style.gaps",
                Ty::OneOf(&["break", "bridge"]),
                false,
                NO,
                "Combo charts: whether a missing line value breaks the line (the default) or is bridged."
            ),
            field!(
                "style.ranges",
                Ty::List(&Ty::Number),
                false,
                NO,
                "Bullet charts: up to three ascending thresholds that shade the bands between them."
            ),
            field!(
                "style.bands",
                Ty::List(&Ty::Map),
                false,
                NO,
                "Gauge charts: a list of `{to, label, color}`, each band running from the end of the one before."
            ),
            field!(
                "style.min",
                Ty::Number,
                false,
                NO,
                "Gauge charts: the value at the start of the arc; `axes.y.min` also sets it."
            ),
            field!(
                "style.max",
                Ty::Number,
                false,
                NO,
                "Gauge charts: the value at the end of the arc."
            ),
            field!(
                "style.cells",
                Ty::Number,
                false,
                NO,
                "Waffle charts: 25, 50 or 100 cells; other numbers draw 100 with a warning."
            ),
            field!(
                "style.inner",
                Ty::Number,
                false,
                NO,
                "Pie charts: the share of the radius left open for a ring, from 0 to 0.9."
            ),
            field!(
                "style.azimuth",
                Ty::Number,
                false,
                NO,
                "Surface, 3-D scatter and 3-D bar: the degrees the view turns round the vertical axis; the default is 35."
            ),
            field!(
                "style.elevation",
                Ty::Number,
                false,
                NO,
                "Surface, 3-D scatter and 3-D bar: the degrees the view looks down from, from 5 to 85; the default is 28."
            ),
            field!(
                "style.shade",
                Ty::Bool,
                false,
                NO,
                "Pivot only: tint each cell along the style's ramp by its value."
            ),
            field!(
                "style.bandwidth",
                Ty::Number,
                false,
                NO,
                "The kernel bandwidth of a density chart, in the units of x; Silverman's rule when absent."
            ),
        ],
    },
    Tag {
        name: "table",
        summary: "A table drawn from block-local data.",
        fields: &[
            field!(
                "title",
                Ty::Text,
                false,
                NO,
                "A caption above the table, also its accessible name."
            ),
            field!(
                "data",
                Ty::Data,
                true,
                NO,
                "The table's data; exactly one local source is allowed."
            ),
            field!(
                "data.inline",
                Ty::List(&Ty::Map),
                false,
                NO,
                "Rows written in the block, each a mapping of column to value."
            ),
            field!(
                "data.csv",
                Ty::File,
                false,
                NO,
                "A CSV file inside the report folder."
            ),
            field!(
                "data.json",
                Ty::File,
                false,
                NO,
                "A JSON file of row mappings inside the report folder."
            ),
            field!(
                "data.parquet",
                Ty::File,
                false,
                NO,
                "A Parquet file inside the report folder; the native host reads only the columns the report uses."
            ),
            field!(
                "data.url",
                Ty::Url,
                false,
                NO,
                "An https data source; current native and browser hosts do not load this field."
            ),
            field!(
                "data.sql",
                Ty::Map,
                false,
                NO,
                "A SQL query run through a named connector on a host that has a registry."
            ),
            field!(
                "data.sql.connector",
                Ty::Text,
                true,
                NO,
                "The name of the connector that runs the query."
            ),
            field!(
                "data.sql.query",
                Ty::File,
                false,
                NO,
                "A report-relative file holding the SQL; set this or inline, not both."
            ),
            field!(
                "data.sql.inline",
                Ty::Text,
                false,
                NO,
                "The SQL text itself; set this or query, not both."
            ),
            field!(
                "data.metricflow",
                Ty::Map,
                false,
                NO,
                "A MetricFlow query run through a named connector."
            ),
            field!(
                "data.metricflow.connector",
                Ty::Text,
                true,
                NO,
                "The name of the connector that runs the query."
            ),
            field!(
                "data.metricflow.metrics",
                Ty::List(&Ty::Text),
                true,
                NO,
                "The metric names to query; at least one."
            ),
            field!(
                "data.cache",
                Ty::Map,
                false,
                NO,
                "A reference to a cached connector result."
            ),
            field!(
                "data.cache.key",
                Ty::Text,
                true,
                NO,
                "The key of the cached result."
            ),
            field!(
                "data.sql.params",
                Ty::Map,
                false,
                NO,
                "Named values passed to the query."
            ),
            field!(
                "data.sql.cache",
                Ty::Map,
                false,
                NO,
                "How long the query's result is reused."
            ),
            field!(
                "data.sql.cache.key",
                Ty::Text,
                false,
                NO,
                "The key of the cached result."
            ),
            field!(
                "data.sql.cache.ttl",
                Ty::Text,
                false,
                NO,
                "How long the result stays fresh, such as 1h."
            ),
            field!(
                "data.sql.cache.freshness",
                Ty::Text,
                false,
                NO,
                "A freshness rule for the result."
            ),
            field!(
                "data.sql.cache.refresh",
                Ty::Bool,
                false,
                NO,
                "Whether every run refreshes the result."
            ),
            field!(
                "data.metricflow.group_by",
                Ty::List(&Ty::Text),
                false,
                NO,
                "The dimensions to group by."
            ),
            field!(
                "data.metricflow.where",
                Ty::List(&Ty::Text),
                false,
                NO,
                "Filters on the query."
            ),
            field!(
                "data.metricflow.order_by",
                Ty::List(&Ty::Text),
                false,
                NO,
                "The order of the rows."
            ),
            field!(
                "data.metricflow.limit",
                Ty::Integer,
                false,
                NO,
                "The most rows returned."
            ),
            field!(
                "data.metricflow.cache",
                Ty::Map,
                false,
                NO,
                "How long the query's result is reused."
            ),
            field!(
                "data.metricflow.cache.key",
                Ty::Text,
                false,
                NO,
                "The key of the cached result."
            ),
            field!(
                "data.metricflow.cache.ttl",
                Ty::Text,
                false,
                NO,
                "How long the result stays fresh, such as 15m."
            ),
            field!(
                "data.metricflow.cache.freshness",
                Ty::Text,
                false,
                NO,
                "A freshness rule for the result."
            ),
            field!(
                "data.metricflow.cache.refresh",
                Ty::Bool,
                false,
                NO,
                "Whether every run refreshes the result."
            ),
            field!(
                "data.cache.ttl",
                Ty::Text,
                false,
                NO,
                "How long the result stays fresh."
            ),
            field!(
                "columns",
                Ty::List(&Ty::Map),
                false,
                NO,
                "The columns to show, in order; all columns when omitted."
            ),
            field!(
                "columns[n].key",
                Ty::Column,
                true,
                NO,
                "The data column this table column shows."
            ),
            field!(
                "columns[n].label",
                Ty::Text,
                false,
                NO,
                "The column heading."
            ),
            field!(
                "columns[n].format",
                Ty::OneOf(FORMATS),
                false,
                NO,
                "The number format of the column's values."
            ),
            field!(
                "columns[n].unit",
                Ty::Text,
                false,
                NO,
                "A unit printed with each value, by the unit rule."
            ),
            field!(
                "columns[n].digits",
                Ty::Digits,
                false,
                NO,
                "Significant digits of the column's numbers, not decimal places."
            ),
            field!(
                "columns[n].align",
                Ty::OneOf(&["left", "center", "right"]),
                false,
                NO,
                "The alignment of the column's cells."
            ),
            field!(
                "columns[n].heat",
                Ty::Either(&Ty::Bool, &Ty::OneOf(HEAT_SCALES)),
                false,
                Some("false"),
                "Shade a numeric column's cells by value in the style's colours: true or sequential from lowest to highest, diverging around zero."
            ),
            field!(
                "heat_scale",
                Ty::OneOf(&["column", "table"]),
                false,
                Some("column"),
                "One scale for each heat column, or one for the table."
            ),
            field!("sort", Ty::Map, false, NO, "The row order."),
            field!("sort.key", Ty::Column, true, NO, "The column to sort by."),
            field!(
                "sort.dir",
                Ty::OneOf(&["asc", "desc"]),
                false,
                Some("asc"),
                "The sort direction."
            ),
            field!(
                "max_rows",
                Ty::Integer,
                false,
                Some("20"),
                "The most rows shown; a positive whole number."
            ),
            field!(
                "sortable",
                Ty::Bool,
                false,
                Some("true"),
                "Whether a reader can sort the rows by a heading."
            ),
            field!(
                "filter",
                Ty::Either(&Ty::Bool, &Ty::OneOf(&["auto"])),
                false,
                Some("auto"),
                "A box that keeps the rows holding the typed text; auto shows it above 10 rows."
            ),
            field!(
                "total",
                Ty::Either(&Ty::Bool, &Ty::List(&Ty::Column)),
                false,
                Some("false"),
                "A total row: true sums the numeric columns that are not percentages or years, a list of columns sums those."
            ),
            field!(
                "wide",
                Ty::Bool,
                false,
                Some("false"),
                "Whether the table takes the report's full width instead of the text measure."
            ),
        ],
    },
    Tag {
        name: "metric",
        summary: "A single large number with its caption and change.",
        fields: METRIC_FIELDS,
    },
    Tag {
        name: "callout",
        summary: "A highlighted note with a direction.",
        fields: &[
            field!(
                "kind",
                Ty::OneOf(&["up", "down", "neutral", "hold"]),
                true,
                NO,
                "The direction the callout signals."
            ),
            field!("title", Ty::Text, false, NO, "The callout heading."),
            field!("body", Ty::Markdown, true, NO, "The callout text."),
        ],
    },
    Tag {
        name: "expand",
        summary: "A titled part of the report that the reader opens; open on paper.",
        fields: &[
            field!(
                "title",
                Ty::Text,
                true,
                NO,
                "The line the reader sees while it is closed, and taps to open it."
            ),
            field!(
                "open",
                Ty::Bool,
                false,
                Some("false"),
                "Whether it starts open on screen; on paper it is always open."
            ),
            field!(
                "body",
                Ty::Markdown,
                true,
                NO,
                "What opens: Markdown text, lists, tables and code."
            ),
        ],
    },
    Tag {
        name: "code",
        summary: "Code with a title, line numbers and marked lines; shown as text, never run.",
        fields: &[
            field!(
                "text",
                Ty::Text,
                false,
                NO,
                "The code; give it or `src`, not both."
            ),
            field!(
                "src",
                Ty::File,
                false,
                NO,
                "A UTF-8 text file of the report folder to show instead of `text`; 400 lines at most."
            ),
            field!(
                "lang",
                Ty::Text,
                false,
                NO,
                "The language, shown as the block's label; rmd, yaml and shell are highlighted."
            ),
            field!(
                "title",
                Ty::Text,
                false,
                NO,
                "A file name or caption in the block's head."
            ),
            field!(
                "lines",
                Ty::Bool,
                false,
                Some("false"),
                "Whether the lines are numbered; the numbers are never copied with the code."
            ),
            field!(
                "mark",
                Ty::List(&Ty::Any),
                false,
                NO,
                "Lines to mark: line numbers and ranges such as `5-6`."
            ),
        ],
    },
    Tag {
        name: "image",
        summary: "An image from the report folder or a credited Unsplash photo.",
        fields: &[
            field!(
                "src",
                Ty::Text,
                true,
                NO,
                "The image: a report-relative path, a data address or a credited Unsplash photo address."
            ),
            field!(
                "alt",
                Ty::Text,
                true,
                NO,
                "The text alternative; `\"\"` marks a decorative image."
            ),
            field!(
                "caption",
                Ty::Text,
                false,
                NO,
                "The caption under the image."
            ),
            field!(
                "fit",
                Ty::OneOf(FIT),
                false,
                Some("contain"),
                "How the image fills its box."
            ),
            field!(
                "width",
                Ty::Integer,
                false,
                NO,
                "The image width in pixels; a positive whole number."
            ),
            field!(
                "height",
                Ty::Integer,
                false,
                NO,
                "The image height in pixels; a positive whole number."
            ),
            field!(
                "credit",
                Ty::Map,
                false,
                NO,
                "The photo credit; an Unsplash photo needs one."
            ),
            field!(
                "credit.author",
                Ty::Text,
                true,
                NO,
                "The photographer's name; a non-empty string."
            ),
            field!(
                "credit.author_url",
                Ty::Text,
                false,
                NO,
                "The photographer's profile; required on an Unsplash photo, which pins it to unsplash.com."
            ),
            field!(
                "credit.source",
                Ty::Text,
                false,
                NO,
                "The site the photo comes from; Unsplash on an Unsplash photo."
            ),
            field!(
                "credit.source_url",
                Ty::Text,
                false,
                NO,
                "The photo's page on the source site."
            ),
            field!(
                "color",
                Ty::Any,
                false,
                NO,
                "A #rrggbb colour that fills the box until the photo loads; another value is dropped with a warning."
            ),
            field!(
                "wide",
                Ty::Bool,
                false,
                Some("false"),
                "Whether the image takes the report's full width instead of the text measure."
            ),
        ],
    },
    Tag {
        name: "embed",
        summary: "An iframe of an allowed https host.",
        fields: &[
            field!(
                "url",
                Ty::Text,
                true,
                NO,
                "The absolute address to embed; the host must be allowed and https."
            ),
            field!(
                "height",
                Ty::Integer,
                false,
                Some("420"),
                "The frame height in pixels; a positive whole number."
            ),
            field!(
                "allow",
                Ty::List(&Ty::Text),
                false,
                Some(
                    "[youtube.com, www.youtube.com, youtube-nocookie.com, vimeo.com, player.vimeo.com, gist.github.com, codepen.io]"
                ),
                "The hosts that may be embedded."
            ),
            field!(
                "caption",
                Ty::Text,
                false,
                NO,
                "The caption under the frame."
            ),
            field!(
                "wide",
                Ty::Bool,
                false,
                Some("false"),
                "Whether the frame takes the report's full width instead of the text measure."
            ),
        ],
    },
    Tag {
        name: "scrolly",
        summary: "One image that stays in view beside narrative steps, each naming the state the figure takes.",
        fields: &[
            field!(
                "title",
                Ty::Text,
                false,
                NO,
                "The heading of the scrollytelling element."
            ),
            field!(
                "chart",
                Ty::Any,
                false,
                NO,
                "A chart as the figure, in place of `image`: the same keys as a chart block."
            ),
            field!(
                "image",
                Ty::Map,
                false,
                NO,
                "The one figure that stays in view beside the steps."
            ),
            field!(
                "image.src",
                Ty::Text,
                true,
                NO,
                "The image file: a report-relative path or a data address."
            ),
            field!("image.alt", Ty::Text, false, NO, "The text alternative."),
            field!(
                "image.caption",
                Ty::Text,
                false,
                NO,
                "The caption under the figure."
            ),
            field!(
                "image.fit",
                Ty::OneOf(FIT),
                false,
                Some("contain"),
                "How the image fills its box."
            ),
            field!(
                "image.width",
                Ty::Integer,
                false,
                NO,
                "The image width in pixels; a positive whole number."
            ),
            field!(
                "image.height",
                Ty::Integer,
                false,
                NO,
                "The image height in pixels; a positive whole number."
            ),
            field!(
                "image.color",
                Ty::Any,
                false,
                NO,
                "A #rrggbb colour that fills the box until the photo loads."
            ),
            field!(
                "image.credit",
                Ty::Map,
                false,
                NO,
                "The photographer's credit; an Unsplash photo needs one."
            ),
            field!(
                "image.credit.author",
                Ty::Text,
                false,
                NO,
                "The photographer's name."
            ),
            field!(
                "image.credit.author_url",
                Ty::Text,
                false,
                NO,
                "The photographer's page."
            ),
            field!(
                "image.credit.source",
                Ty::Text,
                false,
                NO,
                "The photo's source."
            ),
            field!(
                "image.credit.source_url",
                Ty::Text,
                false,
                NO,
                "The source's address."
            ),
            field!(
                "steps",
                Ty::List(&Ty::Map),
                true,
                NO,
                "The narrative steps, in reading order."
            ),
            field!("steps[n].text", Ty::Markdown, true, NO, "The step's text."),
            field!("steps[n].title", Ty::Text, false, NO, "The step's heading."),
            field!(
                "steps[n].view",
                Ty::Map,
                false,
                NO,
                "The part of the image shown when the step scrolls in: x and y as fractions of the image, and a zoom of at least 1."
            ),
            field!(
                "steps[n].view.x",
                Ty::Number,
                false,
                Some("0"),
                "The view's left edge, as a fraction of the image width from 0 to 1."
            ),
            field!(
                "steps[n].view.y",
                Ty::Number,
                false,
                Some("0"),
                "The view's top edge, as a fraction of the image height from 0 to 1."
            ),
            field!(
                "steps[n].view.zoom",
                Ty::Number,
                false,
                Some("1"),
                "How far the view zooms in, from 1 (the whole image) to 20."
            ),
            field!(
                "steps[n].state",
                Ty::Map,
                false,
                NO,
                "For a chart figure: the control options the step sets, as control id to option value."
            ),
            field!(
                "steps[n].highlight",
                Ty::Map,
                false,
                NO,
                "A box on the image, as x, y, w and h fractions."
            ),
            field!(
                "steps[n].highlight.x",
                Ty::Number,
                true,
                NO,
                "The box's left edge, as a fraction of the image width."
            ),
            field!(
                "steps[n].highlight.y",
                Ty::Number,
                true,
                NO,
                "The box's top edge, as a fraction of the image height."
            ),
            field!(
                "steps[n].highlight.w",
                Ty::Number,
                true,
                NO,
                "The box's width, as a fraction of the image width."
            ),
            field!(
                "steps[n].highlight.h",
                Ty::Number,
                true,
                NO,
                "The box's height, as a fraction of the image height."
            ),
        ],
    },
    Tag {
        name: "audio",
        summary: "An audio player.",
        fields: &[
            field!(
                "src",
                Ty::Text,
                true,
                NO,
                "The audio file: a report-relative path or a data address."
            ),
            field!("title", Ty::Text, true, NO, "The player's accessible name."),
            field!(
                "caption",
                Ty::Text,
                false,
                NO,
                "The caption under the player."
            ),
            field!(
                "transcript",
                Ty::File,
                false,
                NO,
                "A report-relative Markdown file shown under the player as its transcript."
            ),
        ],
    },
    Tag {
        name: "video",
        summary: "A video player.",
        fields: &[
            field!(
                "src",
                Ty::Text,
                true,
                NO,
                "The video file: a report-relative path or a data address."
            ),
            field!("title", Ty::Text, true, NO, "The player's accessible name."),
            field!(
                "caption",
                Ty::Text,
                false,
                NO,
                "The caption under the player."
            ),
            field!(
                "poster",
                Ty::Text,
                false,
                NO,
                "A poster image; without one the PDF prints a text card."
            ),
            field!(
                "transcript",
                Ty::File,
                false,
                NO,
                "A report-relative Markdown file shown under the player as its transcript."
            ),
            field!(
                "captions",
                Ty::File,
                false,
                NO,
                "A report-relative WebVTT file embedded as the video's captions track."
            ),
            field!(
                "width",
                Ty::Integer,
                false,
                NO,
                "The player width in pixels; a positive whole number."
            ),
            field!(
                "height",
                Ty::Integer,
                false,
                NO,
                "The player height in pixels; a positive whole number."
            ),
            field!(
                "wide",
                Ty::Bool,
                false,
                Some("false"),
                "Whether the player takes the report's full width instead of the text measure."
            ),
        ],
    },
];

/// The column shares `widths` accepts: equal columns, or one ratio per two- or three-column
/// layout (FEA-RP-68). A closed list, so no author writes a CSS length.
pub const COLUMN_WIDTHS: &[&str] = &[
    "equal", "2-1", "1-2", "3-2", "2-3", "3-1", "1-3", "2-1-1", "1-2-1", "1-1-2",
];

/// The layouts that hold other blocks. They are not fences: `::: columns` opens one, each
/// `::: column` holds the Markdown and rich fences of one column, and `:::` closes either. The
/// options on the opener are a YAML flow mapping that the walker checks against these fields.
pub const LAYOUTS: &[Tag] = &[Tag {
    name: "columns",
    summary: "Two to four columns side by side, each holding prose and rich blocks.",
    fields: &[
        field!(
            "widths",
            Ty::OneOf(COLUMN_WIDTHS),
            false,
            Some("equal"),
            "The columns' shares of the width, left to right; a ratio needs as many parts as there are columns."
        ),
        field!(
            "gap",
            Ty::OneOf(&["small", "medium", "large"]),
            false,
            Some("medium"),
            "The space between the columns: half, one or one and a half lines of body text."
        ),
        field!(
            "align",
            Ty::OneOf(&["top", "center", "bottom"]),
            false,
            Some("top"),
            "Where shorter columns sit beside the tallest one."
        ),
        field!(
            "wide",
            Ty::Bool,
            false,
            Some("true"),
            "Whether the columns take the report's full width; false keeps them within the text measure."
        ),
    ],
}];

/// The layout named `name`.
pub fn layout(name: &str) -> Option<&'static Tag> {
    LAYOUTS.iter().find(|layout| layout.name == name)
}

pub static FRONTMATTER: &[Field] = &[
    field!(
        "title",
        Ty::Text,
        false,
        NO,
        "The report title; publishing requires it, and a draft without one shows `Untitled report`."
    ),
    field!(
        "description",
        Ty::Text,
        false,
        NO,
        "The deck under the title; publishing requires it."
    ),
    field!("author", Ty::Text, false, NO, "The author, as plain text."),
    field!(
        "date",
        Ty::Text,
        false,
        NO,
        "The report's date, `YYYY-MM-DD` or RFC 3339."
    ),
    field!(
        "slug",
        Ty::Text,
        false,
        NO,
        "The address name the report is published under."
    ),
    field!(
        "version",
        Ty::Text,
        false,
        NO,
        "The semver version; publishing requires it."
    ),
    field!(
        "lang",
        Ty::Text,
        false,
        Some("en"),
        "The report's language as a BCP 47 tag: it sets the page language, quotes and number separators."
    ),
    field!(
        "format",
        Ty::Text,
        false,
        Some("rmd/1"),
        "The format version the report was written for; a newer major version is refused."
    ),
    field!(
        "tags",
        Ty::Either(&Ty::Text, &Ty::List(&Ty::Text)),
        false,
        NO,
        "Labels for the report; one string is one tag."
    ),
    field!(
        "styles",
        Ty::Either(&Ty::Text, &Ty::List(&Ty::Text)),
        false,
        NO,
        "Style names or report-relative style files; the first is the base and later ones override it. One string is one entry."
    ),
    field!(
        "background",
        Ty::Either(&Ty::Text, &Ty::Map),
        false,
        NO,
        "The background behind the report, over every style: one word (grain, dots, grid, lines, soft, glow or a picture file in the folder), `none` for a flat page, or a mapping with texture, gradient, picture, place and strength."
    ),
    field!(
        "collapsed_sections",
        Ty::List(&Ty::Text),
        false,
        NO,
        "Heading anchors for sections initially closed on screen; their content remains available in print."
    ),
    field!(
        "title_image",
        Ty::Either(&Ty::Text, &Ty::Map),
        false,
        NO,
        "The header image; a bare source string stands for `{src}`."
    ),
    field!(
        "title_image.src",
        Ty::Text,
        true,
        NO,
        "The image: a report-relative path, a data address or a credited Unsplash photo address."
    ),
    field!(
        "title_image.alt",
        Ty::Text,
        false,
        NO,
        "The text alternative."
    ),
    field!(
        "title_image.caption",
        Ty::Text,
        false,
        NO,
        "The caption under the image."
    ),
    field!(
        "title_image.fit",
        Ty::OneOf(FIT),
        false,
        Some("contain"),
        "How the image fills its box."
    ),
    field!(
        "title_image.width",
        Ty::Integer,
        false,
        NO,
        "The image width in pixels; a positive whole number."
    ),
    field!(
        "title_image.height",
        Ty::Integer,
        false,
        NO,
        "The image height in pixels; a positive whole number."
    ),
    field!(
        "title_image.credit",
        Ty::Map,
        false,
        NO,
        "The photo credit; an Unsplash photo needs one."
    ),
    field!(
        "title_image.credit.author",
        Ty::Text,
        true,
        NO,
        "The photographer's name."
    ),
    field!(
        "title_image.credit.author_url",
        Ty::Text,
        false,
        NO,
        "The photographer's profile; required on an Unsplash photo."
    ),
    field!(
        "title_image.credit.source",
        Ty::Text,
        false,
        NO,
        "The site the photo comes from."
    ),
    field!(
        "title_image.credit.source_url",
        Ty::Text,
        false,
        NO,
        "The photo's page on the source site."
    ),
    field!(
        "title_image.color",
        Ty::Any,
        false,
        NO,
        "A #rrggbb colour that fills the box until the photo loads."
    ),
    field!(
        "publish",
        Ty::Any,
        false,
        NO,
        "Publication metadata for RichView Publish; other hosts ignore it."
    ),
    field!(
        "tabs",
        Ty::Map,
        false,
        NO,
        "Splits the body into tab panels at heading anchors."
    ),
    field!(
        "tabs.default",
        Ty::Text,
        false,
        NO,
        "The panel shown first; an omitted or unknown string selects the first panel."
    ),
    field!(
        "tabs.export",
        Ty::OneOf(&["all", "active"]),
        false,
        Some("all"),
        "Whether printed and PDF output holds every panel or only the active one."
    ),
    field!(
        "tabs.panels",
        Ty::List(&Ty::Map),
        true,
        NO,
        "The panels, in document order; at least one."
    ),
    field!(
        "tabs.panels[n].id",
        Ty::Text,
        true,
        NO,
        "The panel id, unique, using the report slug grammar; it names the #tab link."
    ),
    field!(
        "tabs.panels[n].label",
        Ty::Text,
        true,
        NO,
        "The text on the panel's tab."
    ),
    field!(
        "tabs.panels[n].from",
        Ty::Text,
        true,
        NO,
        "The explicit heading anchor where the panel starts."
    ),
    field!(
        "toc",
        Ty::Bool,
        false,
        Some("true"),
        "Lists the report's sections beside the body, following the scroll, or under the header on a narrow screen; a report with fewer than three sections, or with tabs, has none."
    ),
    field!(
        "progress",
        Ty::Bool,
        false,
        Some("true"),
        "Shows how far the reader has scrolled as a thin bar in the report's colours along the bottom of the window."
    ),
    field!(
        "slides",
        Ty::Bool,
        false,
        Some("false"),
        "`true` turns the source into slides: the title slide, then one slide for each top-level heading or `---` of the body; the style's stage sets the look."
    ),
];

/// What the automatic migration does with a retired name (TYP-83).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(crate) enum Migration {
    /// The key is renamed in place.
    Rename(&'static str),
    /// The key and its value are removed.
    Remove,
    /// Renamed in place unless the mapping already has the new key, then removed.
    RenameElseRemove(&'static str),
    /// `subtitle`: renamed to `description` when that is empty, else left for the author.
    ToDescription,
    /// `prolog`: its text moves to the top of the body.
    ToBody,
    /// No mechanical fix; the author decides.
    Manual,
}

/// A name the format no longer accepts: it fails with `code`, whose fix says what replaces it
/// (TYP-83). `scope` is `frontmatter` or a tag; `path` is the retired field's path below it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Retired {
    pub scope: &'static str,
    pub path: &'static str,
    pub code: &'static str,
    /// What the author writes instead.
    pub fix: &'static str,
    #[cfg(test)]
    pub migration: Migration,
}

macro_rules! retired {
    ($scope:expr, $path:expr, $code:expr, $migration:expr, $fix:expr) => {
        Retired {
            scope: $scope,
            path: $path,
            code: $code,
            fix: $fix,
            #[cfg(test)]
            migration: $migration,
        }
    };
}

pub(crate) static RETIRED: &[Retired] = &[
    retired!(
        "chart",
        "data.id",
        "data.local",
        Migration::Manual,
        "Write the data in this block: `data.inline`, a report-relative `data.csv` or `data.json`, or a connector configured in `report.connectors.yaml`."
    ),
    retired!(
        "chart",
        "data.headers",
        "data.local",
        Migration::Manual,
        "Save remote data inside the report folder and use `data.csv` or `data.json`, or use a configured connector; credentials stay outside the report."
    ),
    retired!(
        "table",
        "data.id",
        "data.local",
        Migration::Manual,
        "Write the data in this block: `data.inline`, a report-relative `data.csv` or `data.json`, or a connector configured in `report.connectors.yaml`."
    ),
    retired!(
        "table",
        "data.headers",
        "data.local",
        Migration::Manual,
        "Save remote data inside the report folder and use `data.csv` or `data.json`, or use a configured connector; credentials stay outside the report."
    ),
    retired!(
        "chart",
        "axes.x.suffix",
        "block.renamed",
        Migration::RenameElseRemove("unit"),
        "Rename `suffix` to `unit` (TYP-16)."
    ),
    retired!(
        "chart",
        "axes.y.suffix",
        "block.renamed",
        Migration::RenameElseRemove("unit"),
        "Rename `suffix` to `unit` (TYP-16)."
    ),
    retired!(
        "chart",
        "axes.color.suffix",
        "block.renamed",
        Migration::RenameElseRemove("unit"),
        "Rename `suffix` to `unit` (TYP-16)."
    ),
    retired!(
        "chart",
        "encoding.highlight",
        "block.renamed",
        Migration::Manual,
        "Move `encoding.highlight` to the top-level `highlight` field (TYP-78)."
    ),
    retired!(
        "chart",
        "style.line",
        "block.renamed",
        Migration::Manual,
        "Replace `style.line.smoothing` with `style.curve`: straight is `linear`, monotone is `monotone` (TYP-17)."
    ),
    retired!(
        "image",
        "title",
        "block.renamed",
        Migration::RenameElseRemove("alt"),
        "Rename `title` to `alt`, the image's text alternative (TYP-29)."
    ),
    retired!(
        "audio",
        "alt",
        "block.renamed",
        Migration::RenameElseRemove("title"),
        "Rename `alt` to `title`, the clip's accessible name (TYP-30)."
    ),
    retired!(
        "video",
        "alt",
        "block.renamed",
        Migration::RenameElseRemove("title"),
        "Rename `alt` to `title`, the video's accessible name (TYP-31)."
    ),
    retired!(
        "frontmatter",
        "title-image",
        "frontmatter.renamed",
        Migration::Rename("title_image"),
        "Rename `title-image` to `title_image`."
    ),
    retired!(
        "frontmatter",
        "image",
        "frontmatter.renamed",
        Migration::Rename("title_image"),
        "Rename `image` to `title_image`."
    ),
    retired!(
        "frontmatter",
        "subtitle",
        "frontmatter.renamed",
        Migration::ToDescription,
        "Move the text into `description`, the one deck under the title."
    ),
    retired!(
        "frontmatter",
        "prolog",
        "frontmatter.renamed",
        Migration::ToBody,
        "Move the text to the top of the body; the opening text is the first paragraph."
    ),
    retired!(
        "frontmatter",
        "theme",
        "frontmatter.renamed",
        Migration::Remove,
        "Remove `theme`; a style is one colour scheme, so choose `sleek` (dark) or `air` (light) in `styles`."
    ),
    retired!(
        "frontmatter",
        "data",
        "frontmatter.data",
        Migration::Manual,
        "Move each dataset into the `data` mapping of the block that uses it."
    ),
];

/// A value the format no longer accepts in a field that otherwise stays: `currency` formats
/// are a number format plus a currency `unit` (TYP-87). The migration rewrites the value and
/// adds the unit.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RetiredValue {
    pub scope: &'static str,
    pub path: &'static str,
    pub value: &'static str,
    pub replacement: &'static str,
    /// The `unit` written beside the replacement when the mapping has none.
    pub unit: &'static str,
}

macro_rules! currency {
    ($scope:expr, $path:expr) => {
        [
            RetiredValue {
                scope: $scope,
                path: $path,
                value: "currency",
                replacement: "number",
                unit: "$",
            },
            RetiredValue {
                scope: $scope,
                path: $path,
                value: "currency_compact",
                replacement: "number_compact",
                unit: "$",
            },
        ]
    };
}

/// Money formats of every field that takes a value format.
pub(crate) const RETIRED_VALUES: &[RetiredValue] = &{
    let groups = [
        currency!("chart", "axes.x.format"),
        currency!("chart", "axes.y.format"),
        currency!("chart", "axes.color.format"),
        currency!("table", "columns[n].format"),
        currency!("metric", "format"),
    ];
    let mut out = [groups[0][0]; 10];
    let mut i = 0;
    while i < 5 {
        out[2 * i] = groups[i][0];
        out[2 * i + 1] = groups[i][1];
        i += 1;
    }
    out
};

/// The retired value `value` of the field at `path` in `scope`.
pub(crate) fn retired_value(scope: &str, path: &str, value: &str) -> Option<&'static RetiredValue> {
    RETIRED_VALUES
        .iter()
        .find(|r| r.scope == scope && r.path == path && r.value == value)
}

/// Fence tags the format retired, each with the tag that replaces it (TYP-09).
pub(crate) const RETIRED_TAGS: &[(&str, &str)] = &[("big", "metric")];

/// Retired builtin names and the canonical styles that replace them (TYP-52).
pub(crate) const RETIRED_STYLES: &[(&str, &str)] = &[
    ("sleek-dark", "sleek"),
    ("sleek-light", "air"),
    ("editorial", "brutalist"),
];

/// Retired style leaves and the explanation returned by the style loader.
pub(crate) const RETIRED_STYLE_FIELDS: &[(&str, &str)] = &[
    (
        "motion.delay",
        "motion has no delay: blocks and marks arrive on their own timing (motion.duration, motion.stagger)",
    ),
    (
        "components.callout.left-bar-width",
        "callouts have no side bar; components.callout.border-width sets the rule above",
    ),
];

/// The tag that replaces the retired fence tag `name`.
pub(crate) fn retired_tag(name: &str) -> Option<&'static str> {
    RETIRED_TAGS
        .iter()
        .find(|(old, _)| *old == name)
        .map(|(_, new)| *new)
}

/// The retired entry for `path` in `scope`.
pub(crate) fn retired(scope: &str, path: &str) -> Option<&'static Retired> {
    RETIRED
        .iter()
        .find(|entry| entry.scope == scope && entry.path == path)
}

pub const CHART_TYPES: &[ChartType] = &[
    ChartType {
        name: "line",
        summary: "Values over a time axis, one line per series.",
        encodings: &["x", "y", "color", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "bar",
        summary: "Values per category, one bar per series.",
        encodings: &["y", "x", "color", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "scatter",
        summary: "Points on two numeric axes, optionally sized and labelled.",
        encodings: &["x", "y", "color", "size", "label", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "area",
        summary: "Values over a time axis, filled to the baseline.",
        encodings: &["x", "y", "color", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "pie",
        summary: "Shares of a total by category.",
        encodings: &["y", "x", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "heatmap",
        summary: "A grid of cells coloured by a numeric value.",
        encodings: &["color", "x", "y", "facet", "detail"],
        required: &["x", "y", "color"],
    },
    ChartType {
        name: "histogram",
        summary: "Counts of one numeric field in bins.",
        encodings: &["x", "color", "facet", "detail"],
        required: &["x"],
    },
    ChartType {
        name: "boxplot",
        summary: "The distribution of a numeric field per category.",
        encodings: &["y", "x", "color", "facet", "detail"],
        required: &["y"],
    },
    ChartType {
        name: "interval",
        summary: "Authored lower and upper bounds per category, with an optional estimate.",
        encodings: &["x", "lower", "upper", "y", "color", "facet", "detail"],
        required: &["x", "lower", "upper"],
    },
    ChartType {
        name: "waterfall",
        summary: "Signed steps from a start to a total, each step drawn from the running level.",
        encodings: &["x", "y", "measure", "color", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "treemap",
        summary: "A hierarchy of named parts whose tile areas are their values.",
        encodings: &["label", "parent", "y", "color", "facet", "detail"],
        required: &["label", "y"],
    },
    ChartType {
        name: "funnel",
        summary: "Stages from first to last as centred bands as wide as their values.",
        encodings: &["x", "y", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "dumbbell",
        summary: "A line per category between two values, with a hollow start and a filled end.",
        encodings: &["x", "lower", "upper", "color", "facet", "detail"],
        required: &["x", "lower", "upper"],
    },
    ChartType {
        name: "band",
        summary: "A filled range between a lower and an upper value over x, with an optional line for a central value.",
        encodings: &["x", "lower", "upper", "y", "color", "facet", "detail"],
        required: &["x", "lower", "upper"],
    },
    ChartType {
        name: "bullet",
        summary: "A value bar against a target, with optional shaded ranges, per category.",
        encodings: &["x", "y", "target", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "candlestick",
        summary: "Open, high, low and close per period, drawn as a body with wicks.",
        encodings: &["x", "open", "high", "low", "close", "facet", "detail"],
        required: &["x", "open", "high", "low", "close"],
    },
    ChartType {
        name: "icicle",
        summary: "A hierarchy as stacked bands by depth; widths are shares of the whole.",
        encodings: &["label", "parent", "y", "color", "facet", "detail"],
        required: &["label", "y"],
    },
    ChartType {
        name: "map",
        summary: "Regions shaded by value.",
        encodings: &["region", "y", "facet", "detail"],
        required: &["region", "y"],
    },
    ChartType {
        name: "ohlc",
        summary: "Open, high, low and close per period, drawn as ticks on a vertical bar.",
        encodings: &["x", "open", "high", "low", "close", "facet", "detail"],
        required: &["x", "open", "high", "low", "close"],
    },
    ChartType {
        name: "radar",
        summary: "Spokes at equal angles with one polygon per group.",
        encodings: &["x", "y", "color", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "sunburst",
        summary: "A hierarchy as rings by depth; angles are shares of the whole.",
        encodings: &["label", "parent", "y", "color", "facet", "detail"],
        required: &["label", "y"],
    },
    ChartType {
        name: "pareto",
        summary: "Bars sorted from the largest down with a running share of the total on a second axis.",
        encodings: &["x", "y", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "tree",
        summary: "A hierarchy as nodes joined by elbow curves, with an optional value per node.",
        encodings: &["label", "parent", "y", "facet", "detail"],
        required: &["label"],
    },
    ChartType {
        name: "combo",
        summary: "Bars for one measure with a line for another on a second axis.",
        encodings: &["x", "y", "line", "facet", "detail"],
        required: &["x", "y", "line"],
    },
    ChartType {
        name: "rose",
        summary: "Bars around a circle, one per category, with radius as the value.",
        encodings: &["x", "y", "color", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "waffle",
        summary: "A grid of squares in which each category fills a share of the whole.",
        encodings: &["x", "y", "color", "facet", "detail"],
        required: &["x", "y"],
    },
    ChartType {
        name: "gauge",
        summary: "One value on an arc, with optional bands that split the scale.",
        encodings: &["y", "x", "facet"],
        required: &["y"],
    },
    ChartType {
        name: "calendar",
        summary: "A square for each day in week columns, coloured by value, one strip per year.",
        encodings: &["x", "y", "facet"],
        required: &["x", "y"],
    },
    ChartType {
        name: "streamgraph",
        summary: "Stacked areas around a moving centre line, one stream per group, thickness as the value.",
        encodings: &["x", "y", "color", "facet"],
        required: &["x", "y"],
    },
    ChartType {
        name: "ribbon",
        summary: "Stacked bars ranked by value in each category, with ribbons joining each group across categories.",
        encodings: &["x", "y", "color", "facet"],
        required: &["x", "y"],
    },
    ChartType {
        name: "sankey",
        summary: "Flows between named nodes as ribbons as wide as their values, in columns by longest path from a source.",
        encodings: &["source", "target", "y", "facet"],
        required: &["source", "target", "y"],
    },
    ChartType {
        name: "alluvial",
        summary: "Rows as paths through several categorical fields, drawn as ribbons between the categories of neighbouring columns.",
        encodings: &["dimensions", "y", "facet"],
        required: &["dimensions"],
    },
    ChartType {
        name: "chord",
        summary: "Nodes as arcs around a circle sized by their flow, with ribbons between them as wide as the flow they carry.",
        encodings: &["source", "target", "y", "facet"],
        required: &["source", "target", "y"],
    },
    ChartType {
        name: "network",
        summary: "Nodes joined by lines in a fixed force layout, with node size by number of links and an optional value that widens each line.",
        encodings: &["source", "target", "y", "facet"],
        required: &["source", "target"],
    },
    ChartType {
        name: "lollipop",
        summary: "A stem from zero with a dot at the value, per category.",
        encodings: &["x", "y", "color", "facet", "detail"],
        required: &["y"],
    },
    ChartType {
        name: "violin",
        summary: "The distribution of a numeric field per category as a mirrored density shape with quartiles and median.",
        encodings: &["x", "y", "color", "facet"],
        required: &["x", "y"],
    },
    ChartType {
        name: "dotplot",
        summary: "Counts of one numeric field in equal-width intervals, one stacked dot for every observation.",
        encodings: &["x", "color", "facet"],
        required: &["x"],
    },
    ChartType {
        name: "bin2d",
        summary: "Counts of observations in a grid of rectangles over two numeric fields, coloured by count.",
        encodings: &["x", "y", "facet"],
        required: &["x", "y"],
    },
    ChartType {
        name: "hexbin",
        summary: "Counts of observations in a tiling of hexagons over two numeric fields, coloured by count.",
        encodings: &["x", "y", "facet"],
        required: &["x", "y"],
    },
    ChartType {
        name: "density",
        summary: "A smooth estimate of how values are spread, one filled curve per group.",
        encodings: &["x", "color", "facet", "detail"],
        required: &["x"],
    },
    ChartType {
        name: "ecdf",
        summary: "A step line of the share of values at or below each value, per group.",
        encodings: &["x", "color", "facet", "detail"],
        required: &["x"],
    },
    ChartType {
        name: "qq",
        summary: "Sample values against normal quantiles, with a quartile reference line.",
        encodings: &["y", "color", "facet", "detail"],
        required: &["y"],
    },
    ChartType {
        name: "strip",
        summary: "Every observation as a point, spread sideways so none hide, per category.",
        encodings: &["x", "y", "color", "facet", "detail"],
        required: &["y"],
    },
    ChartType {
        name: "splom",
        summary: "Every pair of several numeric fields as a small scatterplot in a grid, with the field names on the diagonal.",
        encodings: &["dimensions", "label", "color", "facet"],
        required: &["dimensions"],
    },
    ChartType {
        name: "parallel",
        summary: "A vertical axis for each of several numeric fields, scaled to its own range, with one line per row across them.",
        encodings: &["dimensions", "label", "color", "facet"],
        required: &["dimensions"],
    },
    ChartType {
        name: "gantt",
        summary: "One bar per task from its start to its end on a time axis, tasks in source order.",
        encodings: &["label", "start", "end", "color", "facet"],
        required: &["label", "start", "end"],
    },
    ChartType {
        name: "contour",
        summary: "Iso-lines of a value over the plane at round levels, coloured along the style's ramp, with a key.",
        encodings: &["x", "y", "z", "facet"],
        required: &["x", "y", "z"],
    },
    ChartType {
        name: "packed-bubble",
        summary: "Circles packed together, each as large in area as its value, coloured by group.",
        encodings: &["label", "y", "color", "facet"],
        required: &["label", "y"],
    },
    ChartType {
        name: "wordcloud",
        summary: "Words sized by their values, packed together without overlap.",
        encodings: &["label", "y", "facet"],
        required: &["label", "y"],
    },
    ChartType {
        name: "venn",
        summary: "Two or three overlapping circles whose areas are the sets' sizes and whose overlaps are drawn at their own sizes.",
        encodings: &["sets", "y", "facet"],
        required: &["sets", "y"],
    },
    ChartType {
        name: "vector",
        summary: "An arrow at each position pointing in the direction of its angle, optionally as long and as coloured as its value.",
        encodings: &["x", "y", "angle", "size", "facet"],
        required: &["x", "y", "angle"],
    },
    ChartType {
        name: "flow",
        summary: "Cards in ranks along the flow, written as `nodes` and `edges` or drawn from data, joined by right-angled lines that never cross a card.",
        encodings: &["label", "source", "target", "lane", "y", "facet"],
        required: &["label"],
    },
    ChartType {
        name: "pivot",
        summary: "A matrix of sums with row, column and grand totals, as in a spreadsheet pivot table.",
        encodings: &["rows", "columns", "values"],
        required: &["rows", "columns", "values"],
    },
    ChartType {
        name: "surface",
        summary: "A mesh of z over the x-y plane seen from a set angle, coloured by z.",
        encodings: &["x", "y", "z"],
        required: &["x", "y", "z"],
    },
    ChartType {
        name: "scatter3d",
        summary: "Points in a box seen from a set angle, nearer dots larger, coloured by group and sized by a value.",
        encodings: &["x", "y", "z", "color", "size", "label"],
        required: &["x", "y", "z"],
    },
    ChartType {
        name: "bar3d",
        summary: "Bars as boxes on a grid of two categories, seen from a set angle, with the height as the value.",
        encodings: &["x", "y", "color"],
        required: &["x", "y"],
    },
];

pub static CODES: &[Code] = &[
    // Source and frontmatter.
    code!(
        "source.limit",
        Error,
        "The source exceeds the 16 MiB limit.",
        false
    ),
    code!(
        "frontmatter.yaml",
        Error,
        "The frontmatter is not YAML.",
        true
    ),
    code!(
        "block.yaml",
        Error,
        "A rich block's YAML is not YAML.",
        true
    ),
    code!(
        "yaml.unsupported",
        Error,
        "The YAML uses a feature outside the RichView profile, such as anchors, tags or several documents.",
        true
    ),
    code!(
        "yaml.duplicate-key",
        Error,
        "A mapping repeats a key.",
        true
    ),
    code!(
        "frontmatter.type",
        Warning,
        "A frontmatter field has the wrong type and is dropped.",
        true
    ),
    code!(
        "frontmatter.mapping",
        Error,
        "Closed frontmatter is not a mapping, so the title block cannot be read.",
        true
    ),
    code!(
        "frontmatter.unknown-key",
        Warning,
        "A frontmatter key the registry does not know; it stays in the metadata with no effect.",
        true
    ),
    code!(
        "frontmatter.unclosed",
        Warning,
        "The frontmatter has no closing line, so it is rendered as Markdown.",
        true
    ),
    code!(
        "frontmatter.version",
        Warning,
        "The version is not semver-shaped and is dropped.",
        true
    ),
    code!(
        "frontmatter.renamed",
        Error,
        "A frontmatter key the format retired; the fix names its replacement.",
        true
    ),
    code!(
        "frontmatter.date",
        Warning,
        "The date is not YYYY-MM-DD or RFC 3339; it renders as written.",
        true
    ),
    code!(
        "frontmatter.lang",
        Warning,
        "The language is not a supported locale; the page language is set but formatting stays English.",
        true
    ),
    code!(
        "frontmatter.format",
        Error,
        "The format marker is not rmd/<major>, or names a newer major version than this renderer reads.",
        true
    ),
    code!(
        "frontmatter.tabs",
        Warning,
        "The tabs setting is malformed, so the body renders without tabs.",
        true
    ),
    code!(
        "frontmatter.data",
        Error,
        "Report header data is no longer supported; data belongs in each block.",
        true
    ),
    code!(
        "publish.metadata",
        Error,
        "Publishing needs a title, a description and a version.",
        false
    ),
    // Blocks.
    code!(
        "block.type",
        Error,
        "A rich block is not a YAML mapping.",
        true
    ),
    code!(
        "block.schema",
        Error,
        "A block field is missing or has the wrong type or value.",
        true
    ),
    code!(
        "block.field",
        Error,
        "A field reference names a column the block's data does not have.",
        true
    ),
    code!(
        "block.renamed",
        Error,
        "A block key, tag or value the format retired; the fix names its replacement.",
        true
    ),
    code!(
        "block.unknown-key",
        Warning,
        "A block key the registry does not know; it is dropped before rendering.",
        true
    ),
    code!(
        "schema.invalid",
        Error,
        "A value fails the public schema parse.",
        false
    ),
    code!(
        "data.local",
        Error,
        "A block does not define exactly one local data source, or uses a header id or remote address.",
        true
    ),
    code!(
        "data.unresolved",
        Error,
        "A block's data file was not resolved before rendering.",
        true
    ),
    code!(
        "data.limit",
        Error,
        "A resolved dataset exceeds 16 MiB, or a Parquet file one million rows.",
        true
    ),
    code!("data.csv", Error, "A CSV dataset cannot be read.", true),
    code!(
        "data.parquet",
        Error,
        "A Parquet dataset cannot be read.",
        true
    ),
    code!(
        "data.parquet.non-finite",
        Warning,
        "A Parquet column holds NaN or infinite values, read as missing.",
        true
    ),
    code!(
        "data.csv.header",
        Error,
        "A CSV dataset's headers are empty or repeat.",
        true
    ),
    code!(
        "data.inline",
        Error,
        "Inline rows are not flat row mappings.",
        true
    ),
    code!(
        "frontmatter.slug",
        Warning,
        "The slug is not valid, so publishing fails until it is fixed.",
        true
    ),
    code!(
        "publish.slug",
        Error,
        "The slug is not valid for publishing.",
        false
    ),
    code!(
        "publish.version-exists",
        Error,
        "This version is already published with different content.",
        false
    ),
    code!(
        "publish.version-deleted",
        Error,
        "This version was deleted and its number cannot be published again.",
        false
    ),
    code!(
        "publish.version-missing",
        Error,
        "The pinned version is not published for this report.",
        false
    ),
    code!(
        "publish.new-slug",
        Error,
        "This publish expected an existing report address.",
        false
    ),
    code!(
        "publish.slug-derived",
        Warning,
        "The slug was derived from the title because the frontmatter has none.",
        false
    ),
    code!(
        "publish.quota",
        Error,
        "Publishing would exceed the storage quota.",
        false
    ),
    code!(
        "data.json",
        Error,
        "A JSON dataset is not an array of row mappings.",
        true
    ),
    code!(
        "data.provenance",
        Info,
        "A connector query was run and its result stands in for the block's data.",
        true
    ),
    code!(
        "data.cache.stale",
        Warning,
        "A cached snapshot exceeded its freshness limit; the reader's data-source note marks it stale.",
        true
    ),
    code!(
        "data.connector.unknown",
        Error,
        "A data block names a connector that is not registered.",
        true
    ),
    code!(
        "data.connector.rejected",
        Error,
        "A connector refused the query.",
        true
    ),
    code!(
        "data.connector.failed",
        Error,
        "A connector failed while running the query.",
        true
    ),
    code!(
        "data.connector.io",
        Error,
        "A connector hit a file error.",
        true
    ),
    code!(
        "data.cache.miss",
        Error,
        "A data block references a cached result that does not exist.",
        true
    ),
    code!(
        "image.credit-missing",
        Error,
        "An Unsplash photo has no credit, so the block stays source.",
        true
    ),
    code!(
        "image.credit-invalid",
        Error,
        "An Unsplash photo's credit breaks the rules, so the block stays source.",
        true
    ),
    code!(
        "image.credit-incomplete",
        Warning,
        "An optional image credit has no non-empty author, so no credit is shown.",
        true
    ),
    code!(
        "image.credit-url",
        Warning,
        "A credit link is not an http(s) address, so the name is not linked.",
        true
    ),
    code!(
        "image.color-invalid",
        Warning,
        "An image color is not #rrggbb and is dropped.",
        true
    ),
    code!(
        "security.media",
        Warning,
        "An audio or video source was blocked as unsafe.",
        true
    ),
    code!(
        "security.embed",
        Error,
        "An embed was blocked because its URL is not https, carries credentials or names a host that is not allowed.",
        true
    ),
    code!(
        "export.video-poster",
        Warning,
        "A video has no poster, so the PDF prints a text card.",
        true
    ),
    code!(
        "assets.external-reference",
        Info,
        "The report needs an external file that is not embedded.",
        true
    ),
    code!(
        "assets.network-dependent",
        Info,
        "The report needs network access to load an asset.",
        true
    ),
    // Markdown.
    code!(
        "markdown.math",
        Warning,
        "Math could not be rendered.",
        false
    ),
    code!(
        "security.html",
        Warning,
        "Unsafe raw HTML was removed.",
        false
    ),
    code!(
        "security.image",
        Warning,
        "A Markdown image protocol was blocked.",
        false
    ),
    code!(
        "security.link",
        Warning,
        "A Markdown link protocol was blocked.",
        false
    ),
    // Assets.
    code!(
        "asset.mapping",
        Error,
        "A remote asset reference is not a mapping.",
        false
    ),
    code!(
        "asset.unknown-key",
        Warning,
        "A remote asset reference has a key it does not take.",
        false
    ),
    code!(
        "asset.url",
        Error,
        "A remote asset reference has a missing or unusable url.",
        false
    ),
    code!(
        "asset.presigned",
        Error,
        "A remote asset url carries a signing parameter.",
        false
    ),
    code!(
        "asset.bytes",
        Error,
        "A remote asset reference lacks a valid byte size.",
        false
    ),
    code!(
        "asset.sha256",
        Error,
        "A remote asset reference lacks a valid SHA-256.",
        false
    ),
    code!(
        "asset.type",
        Error,
        "A remote asset reference has a media type its use does not allow.",
        false
    ),
    code!(
        "asset.visibility",
        Error,
        "A remote asset reference is neither public nor private.",
        false
    ),
    code!(
        "asset.duration",
        Error,
        "A remote asset's duration is invalid or does not apply.",
        false
    ),
    code!(
        "asset.dimensions",
        Error,
        "A remote asset's width or height is invalid or does not apply.",
        false
    ),
    // Styles.
    code!(
        "style.unresolved",
        Error,
        "A styles file is not in the report folder.",
        false
    ),
    code!(
        "style.renamed",
        Error,
        "A style names a builtin that was renamed.",
        false
    ),
    code!(
        "style.print-dark",
        Warning,
        "The style is still dark after the print swap, so the page prints with Air's surfaces and inks.",
        false
    ),
    code!(
        "style.unknown",
        Error,
        "A styles entry names no built-in style.",
        false
    ),
    code!(
        "style.type",
        Warning,
        "A style file is not a YAML mapping.",
        false
    ),
    code!(
        "style.retired",
        Warning,
        "A retired author style key or motion value is ignored.",
        false
    ),
    code!("style.schema", Warning, "A style leaf is invalid.", false),
    code!(
        "style.company-adjusted",
        Info,
        "The brand colour was adjusted to meet the contrast floor.",
        false
    ),
    code!(
        "style.company-secondary",
        Info,
        "The secondary brand colour was too similar for the chart palette.",
        false
    ),
    code!(
        "style.company-palette",
        Warning,
        "A distinct brand palette could not be derived.",
        false
    ),
    code!(
        "style.logo-path",
        Error,
        "A company logo path is not report-relative.",
        false
    ),
    code!(
        "style.logo-missing",
        Error,
        "A company logo is not available.",
        false
    ),
    code!(
        "style.logo-format",
        Error,
        "A company logo is not a valid raster image.",
        false
    ),
    code!(
        "style.logo-size",
        Error,
        "A company logo exceeds the file size limit.",
        false
    ),
    code!(
        "style.logo-dimensions",
        Error,
        "A company logo exceeds the image dimension limit.",
        false
    ),
    code!(
        "style.logo-animated",
        Error,
        "A company logo is animated.",
        false
    ),
    code!(
        "style.logo-name",
        Warning,
        "A company logo has no name and is decorative.",
        false
    ),
    code!(
        "style.background-path",
        Error,
        "A background picture path is not report-relative.",
        false
    ),
    code!(
        "style.background-missing",
        Error,
        "A background picture is not available.",
        false
    ),
    code!(
        "style.background-format",
        Error,
        "A background picture is not a valid raster image.",
        false
    ),
    code!(
        "style.background-size",
        Error,
        "A background picture exceeds the file size limit.",
        false
    ),
    code!(
        "style.background-dimensions",
        Error,
        "A background picture exceeds the image dimension limit.",
        false
    ),
    code!(
        "style.background-animated",
        Error,
        "A background picture is animated.",
        false
    ),
    code!(
        "style.background-conflict",
        Warning,
        "A background picture is set with a texture or gradient; the picture stands alone.",
        false
    ),
    code!(
        "style.background-adjusted",
        Info,
        "The background was made fainter so that text keeps its contrast over it.",
        false
    ),
    code!(
        "style.properties",
        Error,
        "Custom style properties are invalid.",
        false
    ),
    code!(
        "style.fonts-unavailable",
        Warning,
        "The host has no bundled fonts, so text falls back to system fonts.",
        false
    ),
    code!(
        "style.unknown-key",
        Warning,
        "A style leaf is not in the style vocabulary and is skipped.",
        false
    ),
    code!(
        "style.experimental",
        Warning,
        "A style leaf is experimental: it can break the layout in ways nobody expects.",
        false
    ),
    code!(
        "style.internal",
        Warning,
        "A style leaf fixes the page's mechanics and is ignored.",
        false
    ),
    code!(
        "style.contrast",
        Warning,
        "A style colour is below its contrast floor on a surface.",
        false
    ),
    code!(
        "style.type-scale",
        Warning,
        "Two neighbouring type sizes are closer than the minimum ratio.",
        false
    ),
    code!(
        "style.weights",
        Warning,
        "The style embeds more than three font weights.",
        false
    ),
    code!(
        "style.font-missing",
        Warning,
        "A declared font family has no bundled face and no file in the report folder.",
        false
    ),
    code!(
        "style.glyph-missing",
        Warning,
        "Characters of the report fall outside the faces the style embeds.",
        false
    ),
    // Render options and output.
    code!(
        "options.style",
        Error,
        "The render options' styles are not an ordered array of style references.",
        false
    ),
    code!(
        "options.width",
        Error,
        "A chart width is non-finite or outside 240 to 4096.",
        false
    ),
    code!(
        "render.output.invalid",
        Error,
        "The render output option is invalid.",
        false
    ),
    code!(
        "render.page.invalid",
        Error,
        "The render page option is invalid.",
        false
    ),
    code!(
        "render.output-limit",
        Error,
        "The generated report exceeds the 64 MiB HTML limit.",
        false
    ),
    code!(
        "assets.runtime-external",
        Info,
        "The preview loads the chart runtime from outside the report.",
        false
    ),
    code!(
        "security.id",
        Warning,
        "A heading id or footnote label reserved for the report runtime was dropped or renamed.",
        true
    ),
    code!(
        "security.class",
        Warning,
        "Heading classes and attributes from source were dropped.",
        true
    ),
    code!(
        "image.alt-missing",
        Warning,
        "The header image has no alt text, so it is treated as decorative.",
        false
    ),
    code!(
        "editor.service",
        Error,
        "The editor service could not answer a request.",
        false
    ),
    code!(
        "block.nested",
        Error,
        "A rich block sits inside a list, quote or note.",
        true
    ),
    code!(
        "code.mark",
        Warning,
        "A code block marks a line past its last line.",
        false
    ),
    code!(
        "block.unknown-tag",
        Warning,
        "A fence names no rich tag; a close match is suggested.",
        true
    ),
    code!(
        "callout.title",
        Warning,
        "A callout has no title, so colour alone carries the finding.",
        true
    ),
    code!(
        "block.source",
        Warning,
        "A chart or table has no source note directly after it.",
        false
    ),
    code!(
        "table.title",
        Info,
        "A table has no title or accessible name.",
        false
    ),
    code!(
        "image.context",
        Info,
        "The header image has neither a caption nor a credit.",
        false
    ),
    code!(
        "markdown.heading-id",
        Warning,
        "An explicit heading id is not named by a tab panel and is ignored.",
        true
    ),
    code!(
        "markdown.table-header",
        Warning,
        "A Markdown table repeats a column header, making its columns ambiguous.",
        true
    ),
    code!(
        "table.markdown",
        Info,
        "An inline table cell contains Markdown syntax and is shown as written.",
        false
    ),
    code!(
        "media.captions",
        Info,
        "A video has no captions file.",
        false
    ),
    code!(
        "media.transcript",
        Info,
        "An audio or video block has no transcript file.",
        false
    ),
    code!(
        "markdown.footnote",
        Warning,
        "A footnote reference has no definition and shows as text.",
        true
    ),
    code!(
        "markdown.heading-depth",
        Warning,
        "A sixth-level heading cannot move below the page title and stays at level 6.",
        true
    ),
    code!(
        "markdown.quote",
        Warning,
        "The pull-quote marker is not supported and shows as text.",
        true
    ),
    code!(
        "markdown.wikilink",
        Warning,
        "A wikilink is not supported and its label shows as text.",
        true
    ),
    code!(
        "render.failed",
        Error,
        "The report could not be rendered.",
        false
    ),
    code!(
        "style.sink",
        Error,
        "A style value holds characters the CSS output refuses.",
        false
    ),
    code!(
        "table.total",
        Warning,
        "A table total skips columns it cannot sum.",
        true
    ),
    code!(
        "table.heat",
        Warning,
        "A table column asks for heat but is not all numbers.",
        true
    ),
    code!(
        "security.runtime-url",
        Warning,
        "The chart runtime address is not allowed.",
        false
    ),
    code!(
        "tabs.anchor-missing",
        Warning,
        "A tab panel's anchor is not in the body, so the body renders without tabs.",
        false
    ),
    code!(
        "section.anchor-missing",
        Warning,
        "A collapsed section names no top-level heading, so it stays unfolded.",
        false
    ),
    code!(
        "section.heading-interactive",
        Warning,
        "A collapsed section heading contains a link, so the section stays unfolded.",
        false
    ),
    code!(
        "tabs.order",
        Warning,
        "Tab anchors are not distinct and in document order, so the body renders without tabs.",
        false
    ),
    code!(
        "columns.count",
        Warning,
        "Columns hold two to four columns, so these markers stay text and their content renders in one column.",
        true
    ),
    code!(
        "columns.empty",
        Warning,
        "A column holds nothing, so these markers stay text and the content renders in one column.",
        true
    ),
    code!(
        "columns.nested",
        Warning,
        "Columns cannot sit inside a column, so these markers stay text and the content renders in one column.",
        true
    ),
    code!(
        "columns.heading",
        Warning,
        "A column holds a section heading or a tab's start, so these markers stay text and the content renders in one column.",
        true
    ),
    code!(
        "columns.block",
        Warning,
        "A column holds a block that needs the page's width, so these markers stay text and the content renders in one column.",
        true
    ),
    code!(
        "columns.unclosed",
        Warning,
        "A columns or column marker has no closing `:::`, so the markers stay text and the content renders in one column.",
        true
    ),
    code!(
        "columns.stray",
        Warning,
        "A column marker or text sits outside a column, so the markers stay text and the content renders in one column.",
        true
    ),
    code!(
        "columns.widths",
        Warning,
        "The widths name a different number of columns than there are, so the columns are equal.",
        true
    ),
    code!(
        "columns.options",
        Warning,
        "The options after `::: columns` are not a YAML mapping, so the columns take their defaults.",
        true
    ),
    code!(
        "slides.tabs",
        Warning,
        "A slides source has no tabs, so the tabs setting has no effect.",
        true
    ),
    code!(
        "slides.density",
        Warning,
        "A slide holds more than one screen shows well: too many numbers, figures, words or table rows.",
        true
    ),
    // Charts.
    code!(
        "chart.invalid",
        Error,
        "A chart block could not be read.",
        true
    ),
    code!("chart.failed", Error, "A chart failed to draw.", true),
    code!(
        "chart.states-unchecked",
        Warning,
        "Only the first reader states of a chart were checked.",
        true
    ),
    code!(
        "chart.aggregation",
        Info,
        "The chart aggregates rows before drawing.",
        true
    ),
    code!(
        "chart.decimation",
        Info,
        "The chart draws representative positions for several observations and reports the kept and total counts.",
        true
    ),
    code!(
        "chart.ambiguous-bar-slot",
        Error,
        "Several numeric rows share a bar category and series.",
        true
    ),
    code!(
        "chart.ambiguous-missing-cell",
        Error,
        "Missing heatmap observations repeat an existing coordinate.",
        true
    ),
    code!(
        "chart.ambiguous-numeric-cell",
        Error,
        "Several numeric rows share a heatmap coordinate.",
        true
    ),
    code!(
        "chart.binning",
        Info,
        "The histogram's bins and measure, with the observations they hold.",
        true
    ),
    code!(
        "chart.boxplot-summary",
        Info,
        "How the box plot was summarised: quartiles, whiskers and outliers.",
        true
    ),
    code!(
        "chart.clipped-interval",
        Warning,
        "Axis bounds clip supplied intervals or estimates.",
        true
    ),
    code!(
        "chart.clipping",
        Warning,
        "A label exceeds the chart's viewport.",
        true
    ),
    code!(
        "chart.collapsed-interval",
        Warning,
        "An interval has equal bounds and collapses to a point.",
        true
    ),
    code!(
        "chart.contrast",
        Warning,
        "Chart text falls below 4.5:1 contrast on the chart surface.",
        true
    ),
    code!(
        "chart.empty-data",
        Error,
        "The chart has no rows to draw.",
        true
    ),
    code!(
        "chart.empty-region",
        Warning,
        "The plot draws nothing.",
        true
    ),
    code!(
        "chart.missing-observations",
        Info,
        "Some rows have no value and are drawn as gaps.",
        true
    ),
    code!(
        "chart.excluded-observations",
        Warning,
        "Some observations were left out of the chart.",
        true
    ),
    code!(
        "chart.histogram",
        Error,
        "The histogram options or data are invalid.",
        true
    ),
    code!(
        "chart.invalid-bubble-size",
        Error,
        "Bubble sizes are negative or not numbers.",
        true
    ),
    code!(
        "chart.invalid-color-values",
        Error,
        "Heatmap colour values are neither numbers nor recognised missing values.",
        true
    ),
    code!(
        "chart.label-collision",
        Warning,
        "Two labels overlap.",
        true
    ),
    code!(
        "chart.label-dropped",
        Info,
        "A label was left out for lack of room.",
        true
    ),
    code!(
        "chart.label-thinned",
        Info,
        "Only some category names are shown.",
        true
    ),
    code!(
        "chart.label-unfit",
        Warning,
        "A label does not fit at this width.",
        true
    ),
    code!(
        "chart.missing-bubble-size",
        Warning,
        "Some observations have no size, so their bubbles are omitted.",
        true
    ),
    code!(
        "chart.missing-color-values",
        Error,
        "The heatmap's colour field has no numeric values.",
        true
    ),
    code!(
        "chart.negative-pie-value",
        Error,
        "A pie slice has a negative value.",
        true
    ),
    code!(
        "chart.non-numeric",
        Error,
        "A value that must be a number is not.",
        true
    ),
    code!(
        "chart.limit-exceeded",
        Error,
        "The chart has more of something than its type takes; nothing is dropped, so it is not drawn.",
        true
    ),
    code!(
        "chart.palette-rule",
        Warning,
        "The palette fails its rule on this chart, so patterns tell series apart.",
        true
    ),
    code!(
        "chart.points-overlap",
        Info,
        "Points overlap because their strips are too narrow to spread them.",
        true
    ),
    code!(
        "chart.series-differentiation",
        Warning,
        "More than eight series reuse colour slots.",
        true
    ),
    code!(
        "chart.source-error",
        Error,
        "A source value cannot be represented.",
        true
    ),
    code!(
        "chart.style-invalid",
        Warning,
        "A chart style setting has a value the type cannot use, so its default applies.",
        true
    ),
    code!(
        "chart.too-few-observations",
        Warning,
        "A group has too few distinct values for the chart to draw it.",
        true
    ),
    code!(
        "chart.unlocated-color-values",
        Error,
        "Heatmap colour values lack x or y coordinates.",
        true
    ),
    code!(
        "chart.unresolved-annotation",
        Warning,
        "An annotation's coordinates do not resolve against the chart.",
        true
    ),
    code!(
        "chart.unresolved-highlight",
        Warning,
        "A highlighted value matches no row.",
        true
    ),
    code!(
        "chart.value-overflow",
        Error,
        "A total exceeds the finite numeric range.",
        true
    ),
    // Reports and export.
    code!(
        "report.root.missing",
        Error,
        "No dedicated report folder, or its source is missing.",
        false
    ),
    code!(
        "report.root.invalid",
        Error,
        "The report root or source name is invalid.",
        false
    ),
    code!(
        "report.root.escape",
        Error,
        "A report path escapes the report folder or is ambiguous.",
        false
    ),
    code!(
        "report.root.shared",
        Error,
        "The report folder holds duplicate paths or several .rmd sources.",
        false
    ),
    code!(
        "report.root.unresolved",
        Error,
        "The report folder cannot be inspected.",
        false
    ),
    code!(
        "scrolly.state",
        Warning,
        "A scrollytelling step sets a chart control or option the chart does not have.",
        true
    ),
    code!(
        "chart.print-height",
        Warning,
        "A chart is taller on paper than the share of the page it may take.",
        true
    ),
    code!(
        "report.asset.remote",
        Error,
        "An image, audio, video, poster, header image or Markdown image names a remote address.",
        true
    ),
    code!(
        "report.asset.escape",
        Error,
        "An asset path escapes the report folder.",
        true
    ),
    code!(
        "report.asset.missing",
        Error,
        "An asset is missing from the report folder.",
        false
    ),
    code!(
        "report.output.escape",
        Error,
        "The output path escapes the Rendered folder.",
        false
    ),
    code!(
        "report.output.invalid",
        Error,
        "The output file name is not allowed.",
        false
    ),
    code!(
        "report.output.collision",
        Error,
        "An authored asset collides with a generated output.",
        false
    ),
    code!(
        "report.output.runtime",
        Error,
        "The interactive chart runtime is missing, so the report cannot be exported.",
        false
    ),
    code!(
        "export.browser-missing",
        Error,
        "No Chromium-based browser was found for PDF export.",
        false
    ),
    code!(
        "export.platform",
        Error,
        "PDF export is not supported on this platform.",
        false
    ),
    code!(
        "export.browser-failed",
        Error,
        "The browser failed while printing.",
        false
    ),
    code!(
        "export.settle-timeout",
        Error,
        "The report did not settle before the timeout.",
        false
    ),
    code!(
        "export.image-failed",
        Error,
        "Images could not be decoded for printing.",
        false
    ),
    code!(
        "export.browser-version",
        Warning,
        "The browser is older than the supported Chrome version.",
        false
    ),
    code!(
        "export.photo-unavailable",
        Warning,
        "A photo could not be loaded while printing.",
        false
    ),
    code!(
        "export.font-fallback",
        Warning,
        "Glyphs were drawn in a system font instead of the embedded ones.",
        false
    ),
    code!(
        "export.pdf-untagged",
        Warning,
        "The PDF has no tags or outline.",
        false
    ),
    code!(
        "export.network-blocked",
        Warning,
        "The report requested an address while printing that PDF output does not load.",
        false
    ),
    code!(
        "export.local-link",
        Info,
        "A link to a report file prints as text.",
        false
    ),
    code!(
        "export.font-check",
        Info,
        "The font check did not complete.",
        false
    ),
    code!(
        "export.browser-sandbox",
        Info,
        "Chrome printed with its sandbox off because RichView runs as root.",
        false
    ),
    // Editor.
    code!(
        "resource.resolve",
        Error,
        "A report resource could not be resolved.",
        false
    ),
    code!(
        "authoring.key",
        Warning,
        "A key is not used by the block.",
        false
    ),
    code!(
        "authoring.field",
        Error,
        "A key names a data field that does not exist.",
        false
    ),
];

/// One-sentence fixes for the error codes (TYP-53, TYP-69), keyed by code.
pub const FIXES: &[(&str, &str)] = &[
    (
        "frontmatter.mapping",
        "Write the frontmatter as a YAML mapping, such as `title: A report`, or remove its delimiters to render it as body text.",
    ),
    (
        "source.limit",
        "Split the report or shrink its inline data below 16 MiB.",
    ),
    (
        "frontmatter.yaml",
        "Rewrite the frontmatter between the --- lines as valid YAML.",
    ),
    (
        "block.yaml",
        "Align sibling keys at the same indentation, indent nested fields consistently, use `key: value` mappings, and quote a text value that contains a colon followed by a space or starts with a symbol such as `%`.",
    ),
    (
        "frontmatter.date",
        "Use an ISO date such as `date: 2026-10-30`, or an RFC 3339 date-time such as `date: 2026-10-30T08:00:00Z`.",
    ),
    (
        "frontmatter.version",
        "Use a quoted semantic version such as `version: \"1.0.0\"`; remove literal backslashes around the quotes.",
    ),
    (
        "yaml.unsupported",
        "Remove anchors, aliases, tags and extra documents; write plain YAML.",
    ),
    (
        "yaml.duplicate-key",
        "Delete or rename the repeated key so each key appears once per mapping.",
    ),
    (
        "frontmatter.renamed",
        "Replace the retired frontmatter key with the replacement the message names.",
    ),
    (
        "frontmatter.format",
        "Set `format: rmd/<major>` to a major version this renderer reads.",
    ),
    (
        "frontmatter.data",
        "Remove the header data and give each block its own `data` source.",
    ),
    (
        "publish.metadata",
        "Add a title, a description and a version to the frontmatter.",
    ),
    (
        "block.type",
        "Make the block's YAML a mapping of keys to values.",
    ),
    (
        "block.field",
        "Name a column that exists in the block's data, or add that column.",
    ),
    (
        "block.renamed",
        "Replace the retired key, tag or value with the replacement the message names.",
    ),
    (
        "schema.invalid",
        "Fix the value so it matches the public schema for that field.",
    ),
    (
        "data.local",
        "Keep exactly one `data` source (`inline`, `csv`, `json`, `parquet`, `url`, `sql`, `metricflow` or `cache`) and remove the others.",
    ),
    (
        "data.unresolved",
        "Resolve the data file before rendering, or render through the CLI or host.",
    ),
    (
        "data.limit",
        "Reduce the dataset below 16 MiB by filtering rows or dropping columns; aggregate a large Parquet file with SQL through a `parquet` connector.",
    ),
    (
        "data.csv",
        "Repair the CSV so it parses: consistent columns, valid quoting, UTF-8.",
    ),
    (
        "data.parquet",
        "Export the file again as unencrypted Parquet with scalar columns, or save the data as CSV and use `data.csv`.",
    ),
    (
        "data.parquet.non-finite",
        "Write those values as nulls when exporting the file, or check the calculation that produced them.",
    ),
    (
        "markdown.table-header",
        "Give each table column a distinct header.",
    ),
    (
        "data.csv.header",
        "Give every CSV column a non-empty, unique header.",
    ),
    (
        "data.inline",
        "Write inline data as a list of flat row mappings, one value per key.",
    ),
    (
        "data.json",
        "Make the JSON file an array of flat row mappings.",
    ),
    (
        "data.connector.unknown",
        "Use a registered connector name, or register the connector first.",
    ),
    (
        "data.connector.rejected",
        "Correct the query or its parameters to what the connector accepts.",
    ),
    (
        "data.connector.failed",
        "Check the connector's access and the query, then run it again.",
    ),
    (
        "data.connector.io",
        "Check that the connector's file exists and is readable.",
    ),
    (
        "data.cache.miss",
        "Run the query again to fill the cache, or remove the cached reference.",
    ),
    (
        "image.credit-missing",
        "Add the photographer's credit to the Unsplash image.",
    ),
    (
        "image.credit-invalid",
        "Fix the Unsplash credit to follow the credit rules.",
    ),
    (
        "security.embed",
        "Use an https embed URL on an allowed host.",
    ),
    (
        "asset.mapping",
        "Write the remote asset as a mapping with url, bytes, sha256 and type.",
    ),
    ("asset.url", "Set a valid https `url` on the remote asset."),
    (
        "asset.presigned",
        "Use a stable public URL without signing parameters.",
    ),
    (
        "asset.bytes",
        "Set `bytes` to the asset's exact size as a positive integer.",
    ),
    (
        "asset.sha256",
        "Set `sha256` to the asset's 64-character hex digest.",
    ),
    (
        "asset.type",
        "Set a media type that this use of the asset allows.",
    ),
    ("asset.visibility", "Set `visibility` to public or private."),
    (
        "asset.duration",
        "Set a valid positive `duration`, or remove it where it does not apply.",
    ),
    (
        "asset.dimensions",
        "Set valid positive `width` and `height`, or remove them where they do not apply.",
    ),
    (
        "publish.slug",
        "Use a slug of lowercase letters, digits and hyphens.",
    ),
    (
        "publish.version-exists",
        "Bump the version, or publish content identical to the existing one.",
    ),
    (
        "publish.version-deleted",
        "Publish under a new, higher version number.",
    ),
    (
        "publish.version-missing",
        "Pin a version that is published for this report, or drop the pin.",
    ),
    (
        "publish.new-slug",
        "Check the slug or pass --new to create a separate report.",
    ),
    (
        "publish.quota",
        "Delete old versions or shrink the report to fit the storage quota.",
    ),
    (
        "style.unresolved",
        "Put the styles file inside the report folder and name it correctly.",
    ),
    (
        "style.renamed",
        "Replace the style name with the one the message names.",
    ),
    (
        "style.unknown",
        "Use a built-in style name, or a styles file in the report folder.",
    ),
    (
        "style.properties",
        "Fix the custom style properties to the documented names and values.",
    ),
    (
        "style.sink",
        "Remove quotes, braces, semicolons and other CSS-breaking characters from the style value.",
    ),
    (
        "options.width",
        "Set every chart width to a finite number from 240 through 4096.",
    ),
    (
        "options.style",
        "Pass the render options' styles as an ordered array of style references.",
    ),
    (
        "render.output.invalid",
        "Set the output option to a value the renderer accepts.",
    ),
    (
        "render.page.invalid",
        "Set the page option to a value the renderer accepts.",
    ),
    (
        "render.output-limit",
        "Shrink the report's data, images or charts below the 64 MiB HTML limit.",
    ),
    (
        "render.failed",
        "Fix the errors reported with this one and render again.",
    ),
    (
        "editor.service",
        "Retry the request; restart the editor service if it keeps failing.",
    ),
    (
        "block.nested",
        "Move the rich block out of the list, quote or note to the top level.",
    ),
    (
        "code.mark",
        "Mark only lines the code has, or add the lines you meant.",
    ),
    (
        "chart.invalid",
        "Fix the chart block's fields to match the chart schema.",
    ),
    (
        "chart.failed",
        "Check the chart's data and encodings, then fix the errors reported with it.",
    ),
    (
        "chart.ambiguous-bar-slot",
        "Aggregate the data so each category and series has one row.",
    ),
    (
        "chart.ambiguous-missing-cell",
        "Remove the duplicate missing-value rows so each x and y appears once.",
    ),
    (
        "chart.ambiguous-numeric-cell",
        "Aggregate the data so each x and y coordinate has one row.",
    ),
    (
        "chart.empty-data",
        "Give the chart at least one data row, or remove the chart.",
    ),
    (
        "chart.histogram",
        "Use a numeric field with valid bin options and non-empty data.",
    ),
    (
        "chart.limit-exceeded",
        "Filter or aggregate the rows to fit the limit named in the message, or choose a chart type that takes more.",
    ),
    (
        "chart.invalid-bubble-size",
        "Make every bubble size a non-negative number.",
    ),
    (
        "chart.invalid-color-values",
        "Make the colour field numeric, or leave missing values empty.",
    ),
    (
        "chart.missing-color-values",
        "Give the heatmap's colour field at least one numeric value.",
    ),
    (
        "chart.negative-pie-value",
        "Make every pie value zero or positive.",
    ),
    (
        "chart.non-numeric",
        "Make the field a number: remove units, text and thousands separators.",
    ),
    (
        "chart.source-error",
        "Replace the unreadable source value with a plain number or text.",
    ),
    (
        "chart.unlocated-color-values",
        "Give every heatmap row both an x and a y value.",
    ),
    (
        "chart.value-overflow",
        "Use smaller values or rescale the data so totals stay finite.",
    ),
    (
        "report.root.missing",
        "Put the report in its own folder with a single .rmd source.",
    ),
    (
        "report.root.invalid",
        "Use a valid report folder and a valid .rmd source name.",
    ),
    (
        "report.root.escape",
        "Keep every report path inside the report folder, without .. or links.",
    ),
    (
        "report.root.shared",
        "Remove duplicate paths and keep one .rmd source per report folder.",
    ),
    (
        "report.root.unresolved",
        "Check that the report folder exists and is readable.",
    ),
    (
        "report.asset.remote",
        "Download the file into the report folder and reference it by relative path.",
    ),
    (
        "report.asset.escape",
        "Move the asset into the report folder and reference it by relative path.",
    ),
    (
        "report.asset.missing",
        "Add the missing asset to the report folder or correct its path.",
    ),
    (
        "report.output.escape",
        "Write output inside the Rendered folder, without .. in the path.",
    ),
    (
        "report.output.invalid",
        "Use an allowed output file name with a plain relative path.",
    ),
    (
        "report.output.collision",
        "Rename the authored asset so it does not match a generated output.",
    ),
    (
        "report.output.runtime",
        "Build the chart runtime first, or export without interactive charts.",
    ),
    (
        "export.browser-missing",
        "Install a Chromium-based browser or point the export at one.",
    ),
    (
        "export.platform",
        "Export the PDF on a supported platform, or use the HTML report.",
    ),
    (
        "export.browser-failed",
        "Retry the export; check that the browser starts and can print.",
    ),
    (
        "export.settle-timeout",
        "Retry the export, or reduce the report's charts and images.",
    ),
    (
        "export.image-failed",
        "Replace the undecodable images with valid PNG, JPEG or SVG files.",
    ),
    (
        "resource.resolve",
        "Check that the resource exists in the report folder and is readable.",
    ),
    (
        "authoring.field",
        "Use a field name that exists in the data.",
    ),
    (
        "frontmatter.type",
        "Give the field a value of the type the message names, or remove it.",
    ),
    (
        "frontmatter.unknown-key",
        "Remove the key, or rename it to the known key the message names.",
    ),
    (
        "frontmatter.unclosed",
        "Close the frontmatter with a `---` line after its last key.",
    ),
    (
        "frontmatter.lang",
        "Set `lang` to `en`, `en-GB`, `cs`, `de` or `ja` to print numbers in that language's format; any other tag only names the page language.",
    ),
    (
        "frontmatter.tabs",
        "Write `tabs` as a mapping whose `panels` give each tab a unique `id`, a `label` and the heading anchor it starts `from`.",
    ),
    (
        "block.schema",
        "Give the field the value the message asks for; `richview learn` lists each block's fields and their types.",
    ),
    (
        "block.unknown-key",
        "Remove the key, or rename it to the known key the message names.",
    ),
    (
        "frontmatter.slug",
        "Write `slug` as 1 to 80 characters of a-z, 0-9 and single hyphens, with no hyphen at either end, such as `q3-sales`.",
    ),
    (
        "publish.slug-derived",
        "Add a `slug` to the frontmatter so the report keeps its address when its title changes.",
    ),
    (
        "data.provenance",
        "Nothing to change; this records which connector query supplied the block's data, and the rows are reused on later renders until the query changes unless `cache: {refresh: true}` under `sql` or `metricflow` runs it every time.",
    ),
    (
        "data.cache.stale",
        "Run the query again to refresh the snapshot, or keep it; the reader's source note says it is stale.",
    ),
    (
        "image.credit-incomplete",
        "Give `credit.author` the creator's name, or remove `credit`.",
    ),
    (
        "image.credit-url",
        "Make the credit's `author_url` and `source_url` http or https addresses, or remove them.",
    ),
    (
        "image.color-invalid",
        "Write `color` as a `#rrggbb` hex colour, such as `#1f6feb`, or remove it.",
    ),
    (
        "security.media",
        "Use an audio or video file in the report folder, or a `data:` URI of an audio or video type.",
    ),
    (
        "export.video-poster",
        "Add a `poster` image from the report folder so the PDF shows a frame of the video.",
    ),
    (
        "assets.external-reference",
        "Generate the report with `richview generate`, which embeds the report folder's files, so it needs nothing beside it.",
    ),
    (
        "assets.network-dependent",
        "Nothing to change if readers are online; offline, the photo or embed does not load and its placeholder shows.",
    ),
    (
        "markdown.math",
        "Correct the TeX the message quotes, or write the formula as plain text.",
    ),
    (
        "security.html",
        "Write the content in Markdown or a rich block; raw HTML never renders.",
    ),
    (
        "security.image",
        "Use an image file in the report folder or a `data:` image URI.",
    ),
    (
        "security.link",
        "Link to an http(s), mailto or tel address, a # anchor or a file in the report folder.",
    ),
    (
        "asset.unknown-key",
        "Remove the key; the message lists the keys a remote asset reference takes.",
    ),
    (
        "style.print-dark",
        "Give the style light surfaces and dark inks to print in its own colours; otherwise paper uses Air's.",
    ),
    (
        "style.type",
        "Write the style file as a YAML mapping of style keys to values.",
    ),
    (
        "style.retired",
        "Remove the retired setting or use the replacement named in the message.",
    ),
    (
        "style.schema",
        "Give the leaf a value of the type the message names, or remove it.",
    ),
    (
        "style.company-adjusted",
        "Keep the adjusted colour for legibility or choose a brand colour with enough contrast.",
    ),
    (
        "style.company-secondary",
        "Choose a second colour that is distinguishable from the first, or keep the derived palette.",
    ),
    (
        "style.company-palette",
        "Choose another brand colour or supply a checked explicit palette.",
    ),
    (
        "style.logo-path",
        "Use a file path inside the report folder.",
    ),
    (
        "style.logo-missing",
        "Put the logo file inside the report folder.",
    ),
    (
        "style.logo-format",
        "Use a valid still PNG, JPEG or WebP logo.",
    ),
    ("style.logo-size", "Use a logo no larger than 512 KiB."),
    (
        "style.logo-dimensions",
        "Use a logo no larger than 1024 pixels on either side.",
    ),
    ("style.logo-animated", "Use a still PNG, JPEG or WebP logo."),
    (
        "style.logo-name",
        "Set company.name to the company name, or leave the mark decorative.",
    ),
    (
        "style.background-path",
        "Use a file path inside the report folder.",
    ),
    (
        "style.background-missing",
        "Put the background picture file inside the report folder.",
    ),
    (
        "style.background-format",
        "Use a valid still PNG, JPEG or WebP image.",
    ),
    (
        "style.background-size",
        "Use a file no larger than 400 KiB.",
    ),
    (
        "style.background-dimensions",
        "Use an image no larger than 2400 pixels on either side.",
    ),
    (
        "style.background-animated",
        "Use a still PNG, JPEG or WebP image.",
    ),
    (
        "style.background-conflict",
        "Keep the picture, or remove it to use the texture and gradient.",
    ),
    (
        "style.background-adjusted",
        "Keep it, or choose a lower strength or a lighter page colour.",
    ),
    (
        "style.fonts-unavailable",
        "Generate the report with the CLI, which embeds the bundled fonts.",
    ),
    (
        "style.unknown-key",
        "Remove the leaf, or rename it to the style key the message names.",
    ),
    (
        "style.experimental",
        "Remove the leaf, or keep it only after checking the report at phone and desktop widths.",
    ),
    (
        "style.internal",
        "Remove the leaf; the page sets it itself.",
    ),
    (
        "style.contrast",
        "Darken or lighten the colour until it reaches the floor the message names on that surface.",
    ),
    (
        "style.type-scale",
        "Move the two sizes apart until each step is at least the ratio the message names.",
    ),
    (
        "style.weights",
        "Use at most three font weights across the style.",
    ),
    (
        "style.font-missing",
        "Add the font's files to the report folder, or choose a bundled family.",
    ),
    (
        "style.glyph-missing",
        "Choose a face that covers these characters, or add one to the report folder that does.",
    ),
    (
        "assets.runtime-external",
        "Nothing to change in a preview; `richview generate` embeds the runtime for offline readers.",
    ),
    (
        "security.id",
        "Choose an id or label that does not start with rv, richview, tab- or panel-.",
    ),
    (
        "security.class",
        "Remove the classes and attributes from the heading, keeping only `{#id}`.",
    ),
    (
        "image.alt-missing",
        "Describe the image in `alt`, or write `alt: \"\"` to mark it decorative.",
    ),
    (
        "block.unknown-tag",
        "Change the fence tag to the rich tag the message suggests, or keep the fence as code.",
    ),
    ("callout.title", "Add a `title` that names the finding."),
    (
        "block.source",
        "Add a source note directly after the block, such as *Source: data/sales.csv.*, and name synthetic data as such.",
    ),
    (
        "table.title",
        "Add a `title` that names the table's subject.",
    ),
    (
        "image.context",
        "Add a `caption` or a `credit` to the header image.",
    ),
    (
        "markdown.heading-id",
        "Name the anchor in a tab panel's `from`, or remove the `{#id}`.",
    ),
    (
        "table.markdown",
        "Write the cell as plain text, or use a Markdown table in the prose when cells need emphasis or links.",
    ),
    (
        "media.captions",
        "Add `captions` with a WebVTT file from the report folder.",
    ),
    (
        "media.transcript",
        "Add `transcript` with a text or Markdown file from the report folder.",
    ),
    (
        "markdown.footnote",
        "Add the footnote's definition, `[^label]: text`, or remove the reference.",
    ),
    (
        "markdown.heading-depth",
        "Use at most five heading levels, with # for the top level.",
    ),
    (
        "markdown.quote",
        "Write the quotation as a block quote and end it with — Name.",
    ),
    (
        "markdown.wikilink",
        "Write a Markdown link, `[text](target)`, in place of the wikilink.",
    ),
    (
        "table.total",
        "List the columns to sum, such as `total: [accounts, revenue]`.",
    ),
    (
        "table.heat",
        "Set `heat` only on a column whose cells are all numbers, or remove it.",
    ),
    (
        "security.runtime-url",
        "Use the embedded runtime, or a root-relative path or an https base without credentials, query or fragment.",
    ),
    (
        "tabs.anchor-missing",
        "Give the body a heading with the panel's anchor, such as `## Costs {#costs}`, or correct the panel's `from`.",
    ),
    (
        "section.anchor-missing",
        "Use the generated anchor of a top-level heading in collapsed_sections, or remove the entry.",
    ),
    (
        "section.heading-interactive",
        "Move links out of the section heading, or remove its entry from collapsed_sections.",
    ),
    (
        "tabs.order",
        "List the panels in the order their headings appear, each starting from a different anchor.",
    ),
    (
        "columns.count",
        "Write two to four `::: column` sections between `::: columns` and its closing `:::`.",
    ),
    (
        "columns.empty",
        "Put prose or a block in every column, or remove the empty `::: column` and its `:::`.",
    ),
    (
        "columns.nested",
        "Close the outer columns before opening the next, or put the blocks in the outer columns.",
    ),
    (
        "columns.heading",
        "Move the level 1 or 2 heading, or the heading a tab starts at, above the `::: columns` line.",
    ),
    (
        "columns.block",
        "Move the scrolly block out of the columns.",
    ),
    (
        "columns.unclosed",
        "Close each `::: column` and the `::: columns` itself with a line of `:::`.",
    ),
    (
        "columns.stray",
        "Put every line between `::: columns` and its closing `:::` inside a `::: column`, and open columns before a `::: column`.",
    ),
    (
        "columns.widths",
        "Give `widths` one part per column, such as `2-1` for two columns, or remove it.",
    ),
    (
        "columns.options",
        "Write the options as a mapping, such as `::: columns {widths: 2-1, gap: large}`.",
    ),
    (
        "slides.tabs",
        "Remove `tabs`, or remove `slides: true` to render the report with its tabs.",
    ),
    (
        "slides.density",
        "Start a new slide: put a top-level heading, or a `---` line, before the content that does not fit.",
    ),
    (
        "chart.states-unchecked",
        "Nothing to change if the checked states are typical; fewer control values let every state be checked.",
    ),
    (
        "chart.aggregation",
        "Nothing to change if means and ranges are intended; choose `style.points: all` to draw every observation, accepting its rendering cost.",
    ),
    (
        "chart.decimation",
        "Nothing to change if reduction is intended; choose `style.points: all` to draw every observation, accepting its rendering cost.",
    ),
    (
        "chart.binning",
        "Nothing to change; set `binning.count` or `binning.edges` to choose other bins.",
    ),
    (
        "chart.boxplot-summary",
        "Nothing to change; this records how the boxes and whiskers summarise the data.",
    ),
    (
        "chart.clipped-interval",
        "Widen `axes.y.min` and `axes.y.max` to cover every interval, or remove them.",
    ),
    (
        "chart.clipping",
        "Shorten the text or give the chart more room.",
    ),
    (
        "chart.collapsed-interval",
        "Check the row's bounds; an interval narrower than the chart can show is drawn as a point.",
    ),
    (
        "chart.contrast",
        "Darken or lighten the ink token the message names.",
    ),
    (
        "chart.empty-region",
        "Check that the data has values in the encoded fields and that not every series is hidden.",
    ),
    (
        "chart.missing-observations",
        "Nothing to change if the values are missing; fill them in the data to draw them.",
    ),
    (
        "chart.excluded-observations",
        "Give every row the values the message names, or remove the rows.",
    ),
    (
        "chart.label-collision",
        "Shorten the labels or widen the chart.",
    ),
    (
        "chart.label-dropped",
        "Nothing to change for a value or axis label, whose value stays in the readout, the Table view and the CSV; shorten or move an annotation's text, and read `details.width` for the chart width it was left out at, often the phone layout.",
    ),
    (
        "chart.label-thinned",
        "Nothing to change; shorter category names or a wider chart show more of them.",
    ),
    (
        "chart.label-unfit",
        "Shorten the names, or give the chart more room.",
    ),
    (
        "chart.missing-bubble-size",
        "Give the row a size value, or remove the row.",
    ),
    (
        "chart.palette-rule",
        "Pick a palette whose colours differ more, or keep the patterns.",
    ),
    (
        "chart.points-overlap",
        "Widen the chart, filter the data, or facet it.",
    ),
    (
        "chart.series-differentiation",
        "Show fewer series, or split the chart into facets with `encoding.facet`.",
    ),
    (
        "chart.style-invalid",
        "Give the style setting a value this chart type uses, or remove it.",
    ),
    (
        "chart.too-few-observations",
        "Give the group at least two different values, or leave it out.",
    ),
    (
        "chart.unresolved-annotation",
        "Place the annotation at values inside the chart's data range; a range or band needs `from` and `to` at two different positions.",
    ),
    (
        "chart.unresolved-highlight",
        "Match the highlighted value to one in the data, checking its spelling.",
    ),
    (
        "scrolly.state",
        "Use a control id and option value from the chart's `controls`.",
    ),
    (
        "chart.print-height",
        "Show fewer rows or categories, or give the chart a wider `style.ratio`, such as `16:9`, so it prints shorter.",
    ),
    (
        "export.browser-version",
        "Use a current Chromium, Google Chrome or Microsoft Edge.",
    ),
    (
        "export.photo-unavailable",
        "Check that the photo's address loads, then export again.",
    ),
    (
        "export.font-fallback",
        "Choose a style whose faces cover these characters, then export again.",
    ),
    (
        "export.pdf-untagged",
        "Export with a current Chromium, Google Chrome or Microsoft Edge, which writes PDF tags and an outline.",
    ),
    (
        "export.network-blocked",
        "Put the file in the report folder so the report embeds it, then export again.",
    ),
    (
        "export.local-link",
        "Nothing to change; link to a public address if PDF readers should open the file.",
    ),
    (
        "export.font-check",
        "Export again; until the check completes, the PDF may depend on this machine's fonts.",
    ),
    (
        "export.browser-sandbox",
        "Run RichView as an ordinary user so Chrome prints inside its sandbox.",
    ),
    (
        "authoring.key",
        "Remove the key, or move it to where the block reads it.",
    ),
];
/// The catalogue's fix for `code`, if it has one.
pub fn fix(code: &str) -> Option<&'static str> {
    FIXES.iter().find(|(c, _)| *c == code).map(|(_, f)| *f)
}
