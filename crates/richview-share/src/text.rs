//! Text measuring and wrapping with the bundled faces' real advances, so a title in a proportional
//! face fills its column the way the page's title would.
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
};

/// One face's advances, in em, for the characters it covers.
pub(crate) struct Measure<'a> {
    font: FontRef<'a>,
    upem: f32,
}

impl<'a> Measure<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Option<Self> {
        let font = FontRef::new(data).ok()?;
        let upem = f32::from(
            font.metrics(Size::unscaled(), LocationRef::default())
                .units_per_em,
        );
        Some(Self { font, upem })
    }

    /// The advance of `c` in em, or `None` when the face has no glyph for it.
    pub(crate) fn advance(&self, c: char) -> Option<f32> {
        let glyph = self.font.charmap().map(c)?;
        let width = self
            .font
            .glyph_metrics(Size::unscaled(), LocationRef::default())
            .advance_width(glyph)?;
        Some(width / self.upem)
    }
}

/// Measures a string with a primary face and a fallback for the characters the primary lacks,
/// which is how the rasterizer chooses glyphs too.
pub(crate) struct Ruler<'a> {
    pub(crate) primary: Option<Measure<'a>>,
    pub(crate) fallback: Option<Measure<'a>>,
    /// Extra advance per character, in em (letter spacing).
    pub(crate) tracking: f32,
}

impl Ruler<'_> {
    /// The width of `text` at `size` px.
    pub(crate) fn width(&self, text: &str, size: f32) -> f32 {
        text.chars()
            .map(|c| {
                let em = self
                    .primary
                    .as_ref()
                    .and_then(|m| m.advance(c))
                    .or_else(|| self.fallback.as_ref().and_then(|m| m.advance(c)))
                    .unwrap_or(0.6);
                (em + self.tracking) * size
            })
            .sum::<f32>()
            * 1.02
    }
}

/// The wrapped rows of a text, and whether any text was dropped.
pub(crate) struct Rows {
    pub(crate) rows: Vec<String>,
    pub(crate) cut: bool,
}

/// Greedy wrap at `width` px; a word wider than a row is cut by characters, and text beyond
/// `max_lines` ends in an ellipsis.
pub(crate) fn wrap(ruler: &Ruler, text: &str, size: f32, width: f32, max_lines: usize) -> Rows {
    let mut rows: Vec<String> = Vec::new();
    let mut row = String::new();
    for word in text.split_whitespace() {
        let mut rest: Vec<char> = word.chars().collect();
        loop {
            let tail: String = rest.iter().collect();
            let candidate = if row.is_empty() {
                tail.clone()
            } else {
                format!("{row} {tail}")
            };
            if ruler.width(&candidate, size) <= width {
                row = candidate;
                break;
            }
            if !row.is_empty() {
                rows.push(std::mem::take(&mut row));
                continue;
            }
            // The word alone is too wide: take the longest prefix that fits (at least one char).
            let mut take = 1;
            while take < rest.len()
                && ruler.width(&rest[..=take].iter().collect::<String>(), size) <= width
            {
                take += 1;
            }
            rows.push(rest.drain(..take).collect());
            if rest.is_empty() {
                break;
            }
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    let cut = rows.len() > max_lines;
    if cut {
        rows.truncate(max_lines);
        let last = rows.last_mut().expect("at least one row");
        let mut chars: Vec<char> = last.chars().collect();
        while !chars.is_empty()
            && ruler.width(
                &format!("{}…", chars.iter().collect::<String>().trim_end()),
                size,
            ) > width
        {
            chars.pop();
        }
        *last = format!("{}…", chars.iter().collect::<String>().trim_end());
    }
    Rows { rows, cut }
}

/// The largest size from `max` down to `min` (steps of 2 px) at which `text` fits the rows
/// `lines_at(size)` allows, uncut; the smallest size when none does.
pub(crate) fn fit(
    ruler: &Ruler,
    text: &str,
    max: f32,
    min: f32,
    width: f32,
    lines_at: impl Fn(f32) -> usize,
) -> (f32, Rows) {
    let mut size = max;
    loop {
        let rows = wrap(ruler, text, size, width, lines_at(size).max(1));
        if !rows.cut || size <= min {
            return (size, rows);
        }
        size = (size - 2.0).max(min);
    }
}
