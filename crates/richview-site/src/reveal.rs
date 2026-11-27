//! The pure part of the live example's scene (W3 scene 2), which `browser/writing.rs`
//! drives: the blocks of a source, and how far the scroll has written each of them. The site check
//! at the end pins the report structure the scene reveals: the anchor report renders its head and
//! then one top-level block for each block of its source, in the same order.

/// What a block of the source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The front matter, which renders as the report's head.
    Front,
    /// A fenced block, such as a chart or a table.
    Fence,
    /// A run of text between blank lines.
    Text,
}

/// A block of the source: its lines `start..end`, counted from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    pub start: usize,
    pub end: usize,
    pub kind: Kind,
}

impl Block {
    pub(crate) fn lines(&self) -> usize {
        self.end - self.start
    }
}

/// The share of the scene's scroll the finished file holds still at the end, before the scene
/// lets go.
pub(crate) const END_HOLD: f64 = 0.16;
/// The fewest lines a block's stretch of scroll counts, so a one-line paragraph is not over in a
/// flick.
const MIN_LINES: f64 = 3.0;

/// The blocks of `source` in order: the front matter, each fenced block with its fences, and each
/// run of text between blank lines.
pub(crate) fn blocks(source: &str) -> Vec<Block> {
    let lines: Vec<&str> = source.split('\n').collect();
    let blank = |i: usize| lines[i].trim().is_empty();
    let fence = |i: usize| lines[i].trim_start().starts_with("```");
    let mut out = vec![];
    let mut i = 0;
    if lines.first().is_some_and(|line| line.trim_end() == "---")
        && let Some(close) = lines
            .iter()
            .skip(1)
            .position(|line| line.trim_end() == "---")
    {
        out.push(Block {
            start: 0,
            end: close + 2,
            kind: Kind::Front,
        });
        i = close + 2;
    }
    while i < lines.len() {
        if blank(i) {
            i += 1;
            continue;
        }
        let start = i;
        if fence(i) {
            i += 1;
            while i < lines.len() && !fence(i) {
                i += 1;
            }
            i = (i + 1).min(lines.len());
            out.push(Block {
                start,
                end: i,
                kind: Kind::Fence,
            });
            continue;
        }
        while i < lines.len() && !blank(i) && !fence(i) {
            i += 1;
        }
        out.push(Block {
            start,
            end: i,
            kind: Kind::Text,
        });
    }
    out
}

/// How far each block is written at `progress`, 0 where the scene pins and 1 where it lets go:
/// 0 not begun, 1 complete. The blocks are written in order, each over a stretch of scroll as long
/// as its lines (at least `MIN_LINES`), and the finished file holds for the last `END_HOLD`.
pub(crate) fn written(progress: f64, blocks: &[Block]) -> Vec<f64> {
    let weights: Vec<f64> = blocks
        .iter()
        .map(|block| (block.lines() as f64).max(MIN_LINES))
        .collect();
    let total: f64 = weights.iter().sum();
    let at = (progress.clamp(0.0, 1.0) / (1.0 - END_HOLD)).min(1.0) * total;
    let mut start = 0.0;
    weights
        .iter()
        .map(|weight| {
            let t = ((at - start) / weight).clamp(0.0, 1.0);
            start += weight;
            t
        })
        .collect()
}

/// The lines of `block` written when it is `t` written: none at 0, all at 1, one more line with
/// each equal step between.
pub(crate) fn lines_written(block: &Block, t: f64) -> usize {
    (t.clamp(0.0, 1.0) * block.lines() as f64).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANCHOR: &str = include_str!("../../../assets/landing/anchor/report.rmd");
    const STYLE: &str = include_str!("../../../assets/landing/anchor/report.rstyle.yaml");
    const DATA: &str =
        include_str!("../../../assets/landing/anchor/data/life-expectancy-by-region.csv");

    #[test]
    fn the_hero_report_is_front_matter_a_chart_three_texts_a_table_and_a_note() {
        let found = blocks(ANCHOR);
        let kinds: Vec<Kind> = found.iter().map(|block| block.kind).collect();
        use Kind::*;
        assert_eq!(kinds, [Front, Fence, Text, Text, Text, Fence, Text]);
        let lines: Vec<&str> = ANCHOR.split('\n').collect();
        assert_eq!(lines[found[0].start], "---");
        assert_eq!(lines[found[0].end - 1], "---");
        assert_eq!(lines[found[1].start], "```chart");
        assert_eq!(lines[found[1].end - 1], "```");
        assert_eq!(lines[found[5].start], "```table");
        assert!(
            found.windows(2).all(|pair| pair[0].end <= pair[1].start),
            "{found:?}"
        );
    }

    #[test]
    fn blocks_need_no_front_matter_and_end_with_an_unclosed_fence() {
        let found = blocks("Text\nmore\n\n```chart\ntype: line");
        assert_eq!(
            found,
            [
                Block {
                    start: 0,
                    end: 2,
                    kind: Kind::Text
                },
                Block {
                    start: 3,
                    end: 5,
                    kind: Kind::Fence
                }
            ]
        );
        assert!(blocks("").is_empty());
    }

    #[test]
    fn the_scroll_writes_the_blocks_in_order_and_the_end_holds_still() {
        let found = blocks(ANCHOR);
        assert!(written(0.0, &found).iter().all(|t| *t == 0.0));
        for progress in [1.0 - END_HOLD, 1.0 - END_HOLD / 2.0, 1.0, 3.0] {
            assert!(written(progress, &found).iter().all(|t| *t == 1.0));
        }
        let mut last = vec![0.0; found.len()];
        for step in 0..=200 {
            let now = written(f64::from(step) / 200.0, &found);
            for (i, t) in now.iter().enumerate() {
                assert!(*t >= last[i], "step {step}: {now:?}");
                // A block starts only once every block before it is complete.
                if *t > 0.0 {
                    assert!(now[..i].iter().all(|before| *before == 1.0), "{now:?}");
                }
            }
            last = now;
        }
    }

    #[test]
    fn a_block_writes_one_more_line_with_each_step() {
        let block = Block {
            start: 10,
            end: 14,
            kind: Kind::Fence,
        };
        assert_eq!(lines_written(&block, 0.0), 0);
        assert_eq!(lines_written(&block, 0.01), 1);
        assert_eq!(lines_written(&block, 0.5), 2);
        assert_eq!(lines_written(&block, 1.0), 4);
    }

    /// The site check: the report the scene reveals has the head and then, under
    /// `main`, one block for each source block after the front matter, in order, with a chart and
    /// its note kept together in one `rv-keep`. The scene relies on exactly this, so a renderer
    /// change that breaks it fails here first.
    #[test]
    fn the_hero_report_renders_one_block_per_source_block_in_order() {
        let mut options = richview_core::document::RenderOptions {
            report_folder: Some(richview_core::document::ReportFolder {
                root: "anchor".into(),
                source: "report.rmd".into(),
                files: vec![
                    "report.rmd".into(),
                    "report.rstyle.yaml".into(),
                    "data/life-expectancy-by-region.csv".into(),
                ],
            }),
            ..Default::default()
        };
        options
            .data
            .insert("report.rstyle.yaml".into(), STYLE.into());
        options
            .data
            .insert("data/life-expectancy-by-region.csv".into(), DATA.into());
        let html = richview_core::document::render(ANCHOR, &options)
            .expect("the anchor report renders")
            .html;
        let body = &html[html.find("<body").expect("a body")..];
        let mut at = 0;
        for marker in [
            "<header class=\"rv-report__header\"",
            "class=\"rv-report__title\"",
            "class=\"rv-report__lede\"",
            "class=\"rv-report__meta\"",
            "<main",
            "<div class=\"rv-keep\"",
            "<figure class=\"rv-chart\"",
            "<p class=\"rv-source\"",
            "<p>",
            "<p>",
            "<div class=\"rv-table\"",
            "<p class=\"rv-source\"",
            "</main>",
        ] {
            let found = body[at..]
                .find(marker)
                .unwrap_or_else(|| panic!("{marker} after byte {at}"));
            at += found + marker.len();
        }
        // Nothing else sits at the top of `main`: its direct children are the keep, two
        // paragraphs, the table and the note.
        let main = &body[body.find("<main").unwrap()..body.find("</main>").unwrap()];
        assert_eq!(main.matches("<div class=\"rv-keep\"").count(), 1, "{main}");
        assert_eq!(main.matches("<figure class=\"rv-chart\"").count(), 1);
        assert_eq!(main.matches("<div class=\"rv-table\"").count(), 1);
        assert_eq!(main.matches("<p class=\"rv-source\"").count(), 2);
    }
}
