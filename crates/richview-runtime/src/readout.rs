//! Choose which observation rows appear in the chart readout.

use richview_chart::geometry::{ReadoutLine, Targeting};

pub(crate) fn shown_lines<'a>(
    lines: &'a [ReadoutLine],
    targeting: Targeting,
    kind: &str,
) -> Vec<(&'a ReadoutLine, bool)> {
    let single = targeting != Targeting::NearestX
        && lines.len() > 1
        && lines[1..].iter().all(|l| l.derived && l.series.is_none());
    if single && lines.len() == 2 {
        return vec![(&lines[0], false), (&lines[1], true)];
    }
    if single && matches!(kind, "sankey" | "alluvial") {
        return vec![(&lines[0], false)];
    }
    lines.iter().map(|l| (l, false)).collect()
}

/// A cue identifies a series, not an individual field or statistic.
pub(crate) fn show_cue(line: &ReadoutLine, lines: &[&ReadoutLine], has_legend: bool) -> bool {
    line.names_series
        && line.series.is_some()
        && (has_legend
            || lines
                .iter()
                .filter(|l| l.names_series)
                .filter_map(|l| l.series)
                .any(|i| Some(i) != line.series))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, derived: bool) -> ReadoutLine {
        ReadoutLine {
            name: name.into(),
            value: name.into(),
            series: None,
            names_series: false,
            derived,
        }
    }

    #[test]
    fn multiple_derived_values_stay_visible_except_for_flows() {
        let lines = [
            line("value", false),
            line("first", true),
            line("second", true),
        ];
        for kind in [
            "qq", "gauge", "ribbon", "bin2d", "hexbin", "venn", "flow", "funnel", "waffle",
        ] {
            assert_eq!(
                shown_lines(&lines, Targeting::Contain, kind),
                vec![(&lines[0], false), (&lines[1], false), (&lines[2], false)],
                "{kind}"
            );
        }
        for kind in ["sankey", "alluvial"] {
            assert_eq!(
                shown_lines(&lines, Targeting::Contain, kind),
                vec![(&lines[0], false)],
                "{kind}"
            );
        }
    }

    #[test]
    fn one_derived_value_is_soft_and_authored_or_series_rows_are_kept() {
        let lines = [
            line("value", false),
            line("first", true),
            line("second", true),
        ];
        assert_eq!(
            shown_lines(&lines[..2], Targeting::Contain, "gauge"),
            vec![(&lines[0], false), (&lines[1], true)]
        );
        assert_eq!(
            shown_lines(&lines, Targeting::NearestX, "line"),
            vec![(&lines[0], false), (&lines[1], false), (&lines[2], false)]
        );
        let authored = [
            line("value", false),
            line("derived", true),
            line("authored", false),
        ];
        assert_eq!(
            shown_lines(&authored, Targeting::Contain, "sankey"),
            vec![
                (&authored[0], false),
                (&authored[1], false),
                (&authored[2], false)
            ]
        );
    }

    #[test]
    fn cues_name_series_only_when_they_help_distinguish_them() {
        let row = |name: &str, value: &str, slot: usize, names_series: bool| ReadoutLine {
            name: name.into(),
            value: value.into(),
            series: Some(slot),
            names_series,
            derived: false,
        };
        let north = row("North", "12", 0, true);
        let south = row("South", "14", 1, true);
        let statistic = row("Upper", "12", 2, false);
        assert!(!show_cue(&north, &[&north], false));
        assert!(show_cue(&north, &[&north], true));
        assert!(show_cue(&north, &[&north, &south], false));
        assert!(!show_cue(&statistic, &[&statistic, &statistic], true));
        let group = row("Region", "North", 0, true);
        assert!(show_cue(&group, &[&group], true));
    }
}
