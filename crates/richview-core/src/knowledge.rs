use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnSection {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub body: String,
}

/// One line per field, nested keys under their parent in registry order:
/// `- path* (type): summary`, with the value an omitted field takes. Agents guess a key that
/// learn does not print (`sort.by`, `style.labels`, `data.query`), so every key is printed.
fn field_lines(fields: &[crate::registry::Field], skip: impl Fn(&str) -> bool) -> String {
    fields
        .iter()
        .filter(|field| !skip(field.path))
        .map(|field| {
            let kind = crate::walker::expected(&field.ty);
            let star = if field.required { "*" } else { "" };
            let default = field
                .default
                .map(|value| format!(" Default: {value}."))
                .unwrap_or_default();
            format!(
                "- {}{star} ({kind}): {}{default}",
                field.path, field.summary
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Declared all-points ceilings, generated from the same limits the engine reads.
fn point_limits() -> String {
    richview_chart::limits::LIMITS
        .iter()
        .filter(|limit| limit.what == "all-points observations")
        .map(|limit| format!("{}: {} observations", limit.chart, limit.max))
        .collect::<Vec<_>>()
        .join("; ")
}

/// One chart type per line: its channels, the required ones first and starred, and what it
/// draws. Required channels lead because `bar (y*, x*)` read as "y is the category".
fn chart_types() -> String {
    crate::registry::CHART_TYPES
        .iter()
        .map(|chart| {
            let required = chart.required.iter().map(|c| format!("{c}*"));
            let optional = chart
                .encodings
                .iter()
                .filter(|c| !chart.required.contains(c))
                .map(|c| (*c).to_owned());
            let channels = required.chain(optional).collect::<Vec<_>>().join(", ");
            format!("- {} ({channels}): {}", chart.name, chart.summary)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Fills the placeholders of the hand-written guide from the registry, so no list of tags,
/// fields or types is written twice (ARC-09).
fn from_registry(text: &str) -> String {
    let tags = crate::registry::TAGS
        .iter()
        .map(|tag| tag.name)
        .collect::<Vec<_>>()
        .join(", ");
    let chart = crate::registry::tag("chart")
        .map(|tag| tag.fields)
        .unwrap_or(&[]);
    // Every block but the chart, with each key it reads. A block's `data` takes the chart's
    // sources, which the chart keys list once.
    let blocks = crate::registry::TAGS
        .iter()
        .filter(|tag| tag.name != "chart")
        .map(|tag| {
            format!(
                "#### {}\n\n{}\n\n{}",
                tag.name,
                tag.summary,
                field_lines(tag.fields, |path| path.starts_with("data."))
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    text.replace("@BLOCKFIELDS@", &blocks)
        .replace("@TAGS@", &tags)
        .replace(
            "@FRONTMATTER@",
            &field_lines(crate::registry::FRONTMATTER, |_| false),
        )
        .replace("@CHARTS@", &chart_types())
        .replace("@POINTLIMITS@", &point_limits())
        .replace("@CHARTFIELDS@", &field_lines(chart, |_| false))
        .replace(
            "@COLUMNFIELDS@",
            &field_lines(
                crate::registry::layout("columns").map_or(&[], |layout| layout.fields),
                |_| false,
            ),
        )
}

// The guide's prose, one file per section; `@NAME@` placeholders take the registry's lists.
const GRAMMAR: &str = include_str!("../knowledge/grammar.md");
const CHART_CONTRACT: &str = include_str!("../knowledge/chart-contract.md");
const STYLE: &str = include_str!("../knowledge/style.md");
const COMPANY_STYLE: &str = include_str!("../knowledge/company-style.md");
const WORKFLOW: &str = include_str!("../knowledge/workflow.md");
const ANTI_SLOP: &str = include_str!("../knowledge/anti-slop.md");

pub fn learn_sections() -> Vec<LearnSection> {
    let style = format!("{}\n\n{}", STYLE.trim_end(), COMPANY_STYLE.trim_end());
    [
        ("grammar", "The .rmd grammar", GRAMMAR),
        ("chart-contract", "The chart contract", CHART_CONTRACT),
        ("style", "Content plus cascading styles", style.as_str()),
        ("workflow", "The source-centered authoring loop", WORKFLOW),
        ("anti-slop", "RichView's design principles", ANTI_SLOP),
    ]
    .into_iter()
    .map(|(kind, title, body)| LearnSection {
        kind: kind.into(),
        title: title.into(),
        body: from_registry(body.trim_end()),
    })
    .collect()
}

pub fn learn_section(kind: &str) -> Option<LearnSection> {
    learn_sections()
        .into_iter()
        .find(|section| section.kind == kind)
}
pub fn learn_bundle() -> String {
    learn_sections()
        .into_iter()
        .map(|section| {
            format!(
                "---\ntype: {}\ntitle: {}\n---\n\n## {}\n\n{}",
                section.kind, section.title, section.title, section.body
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}
