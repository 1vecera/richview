//! Chart text measured with the bundled fonts' real metrics (FEA-CH-30).
//!
//! Advance widths and vertical metrics come from the pinned bundled faces
//! (`scripts/fonts/optical.py`), so the geometry model places text where every renderer draws
//! it. A face the bundle does not carry, such as an author's brand font before it is measured,
//! falls back to a stated per-character estimate.

use crate::geometry::{Anchor, Rect};
use crate::optical_metrics::{DATA, FACES, ZERO};

/// Advance of a character the face does not carry, in em: the width of a monospaced figure.
const FALLBACK_ADVANCE: f64 = 0.6;
const FALLBACK_ASCENT: f64 = 0.9;
const FALLBACK_DESCENT: f64 = 0.25;

/// A typeface as the geometry model measures it.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// The CSS family stack; the first family is measured.
    pub family: String,
    face: Option<usize>,
}

impl Font {
    pub fn new(family: &str) -> Self {
        let first = family.split(',').next().unwrap_or("").trim().trim_matches(['\'', '"']);
        let face = FACES.iter().position(|(name, ..)| name.eq_ignore_ascii_case(first));
        Self {
            family: family.to_owned(),
            face,
        }
    }

    /// The CSS `ch` unit of this family's first face in em: the advance of "0" at the regular
    /// weight body text uses, or the measured face's own "0" when no regular face is bundled.
    pub fn ch(&self) -> f64 {
        let first = self
            .family
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(['\'', '"']);
        ZERO.iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(first))
            .map_or_else(|| self.width("0", 1.0), |(_, advance)| *advance as f64 / 10000.0)
    }

    /// Whether the metrics are the face's own rather than the fallback estimate.
    pub fn is_measured(&self) -> bool {
        self.face.is_some()
    }

    /// Ascent and descent in em, as a browser lays out a line of this face.
    pub fn vertical(&self) -> (f64, f64) {
        match self.face {
            Some(i) => {
                let (.., ascent, descent) = FACES[i];
                (ascent as f64 / 10000.0, descent as f64 / 10000.0)
            }
            None => (FALLBACK_ASCENT, FALLBACK_DESCENT),
        }
    }

    fn glyph(&self, ch: char) -> Option<f64> {
        let (_, _, start, count, ..) = FACES[self.face?];
        let records = DATA[start * 8..(start + count) * 8].as_chunks::<8>().0;
        let at = records
            .binary_search_by_key(&(ch as u32), |r| u32::from_le_bytes([r[0], r[1], r[2], r[3]]))
            .ok()?;
        Some(u16::from_le_bytes([records[at][6], records[at][7]]) as f64 / 10000.0)
    }

    /// Advance width of `text` in em. Characters outside the bundled subset take the fallback
    /// advance, as the browser's fallback face would roughly occupy (STY-63).
    pub fn advance_em(&self, text: &str) -> f64 {
        text.chars()
            .map(|ch| {
                self.glyph(ch).unwrap_or(if ch.is_whitespace() {
                    FALLBACK_ADVANCE * 0.5
                } else {
                    FALLBACK_ADVANCE
                })
            })
            .sum()
    }

    /// Advance width of `text` at `size` pixels.
    pub fn width(&self, text: &str, size: f64) -> f64 {
        self.advance_em(text) * size
    }

    /// The box of `text` at `size` with its anchor at `x` and its baseline at `y`.
    pub fn bounds(&self, text: &str, size: f64, x: f64, y: f64, anchor: Anchor) -> Rect {
        let w = self.width(text, size);
        let (ascent, descent) = self.vertical();
        let left = match anchor {
            Anchor::Start => x,
            Anchor::Middle => x - w / 2.0,
            Anchor::End => x - w,
        };
        Rect::new(left, y - ascent * size, w, (ascent + descent) * size)
    }

    /// Line height for stacked lines of this face at `size`: ascent plus descent, at least 1.25 em.
    pub fn line_height(&self, size: f64) -> f64 {
        let (ascent, descent) = self.vertical();
        ((ascent + descent).max(1.25)) * size
    }

    /// Distance from a line box's top to its baseline when the box is `line_height` tall.
    pub fn baseline_offset(&self, size: f64) -> f64 {
        let (ascent, descent) = self.vertical();
        let line = self.line_height(size);
        (line - (ascent + descent) * size) / 2.0 + ascent * size
    }

    /// Break `text` into the fewest lines no wider than `width`, balanced so the lines have
    /// similar widths; breaks fall at spaces or after hyphens. A single word wider than `width`
    /// stays whole: names are never cut (FEA-CH-26).
    pub fn wrap(&self, text: &str, size: f64, width: f64, max_lines: usize) -> Vec<String> {
        let words = break_points(text);
        if words.len() <= 1 || self.width(text, size) <= width {
            return vec![text.to_owned()];
        }
        // Greedy fill decides the line count; then balance by shrinking the target width.
        let greedy = |limit: f64| -> Vec<String> {
            let mut lines: Vec<String> = Vec::new();
            let mut current = String::new();
            for word in &words {
                let candidate = format!("{current}{word}");
                if current.is_empty() || self.width(candidate.trim_end(), size) <= limit {
                    current = candidate;
                } else {
                    lines.push(current.trim_end().to_owned());
                    current = word.to_string();
                }
            }
            if !current.trim().is_empty() {
                lines.push(current.trim_end().to_owned());
            }
            lines
        };
        let lines = greedy(width);
        let count = lines.len();
        if count > max_lines {
            return lines;
        }
        let (mut lo, mut hi) = (0.0, width);
        let mut best = lines;
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            let candidate = greedy(mid);
            if candidate.len() <= count {
                hi = mid;
                best = candidate;
            } else {
                lo = mid;
            }
        }
        best
    }

    /// Width of the widest line.
    pub fn lines_width(&self, lines: &[String], size: f64) -> f64 {
        lines.iter().map(|l| self.width(l, size)).fold(0.0, f64::max)
    }
}

/// Words with their trailing break: "North-east region" → ["North-", "east ", "region"].
fn break_points(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    for (k, (i, ch)) in bytes.iter().enumerate() {
        let end = i + ch.len_utf8();
        let breaks_after = *ch == ' ' || (*ch == '-' && k > 0 && k + 1 < bytes.len() && bytes[k + 1].1 != ' ');
        if breaks_after {
            out.push(&text[start..end]);
            start = end;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_faces_measure_with_their_own_advances() {
        let mono = Font::new("Geist Mono, ui-monospace, monospace");
        assert!(mono.is_measured());
        let one = mono.width("0", 12.0);
        assert!(
            (mono.width("0000", 12.0) - 4.0 * one).abs() < 1e-9,
            "monospaced figures"
        );
        let hand = Font::new("'Patrick Hand', cursive");
        assert!(hand.is_measured());
        assert!(hand.width("iiii", 12.0) < hand.width("MMMM", 12.0), "proportional face");
        let (ascent, descent) = hand.vertical();
        assert!(ascent > 0.5 && descent > 0.1);
    }

    #[test]
    fn lent_script_digits_measure_as_the_face_that_draws_them() {
        // Newsreader draws every script digit with RichView Serif's outlines, and Patrick Hand
        // its subscripts with RichView Sans's (scripts/fonts/README.md).
        let news = Font::new("Newsreader, Georgia, serif");
        let serif = Font::new("\"RichView Serif\", Georgia, serif");
        for digit in "²⁶₂".chars() {
            assert!(news.glyph(digit).is_some(), "{digit}");
            assert_eq!(news.glyph(digit), serif.glyph(digit), "{digit}");
        }
        let hand = Font::new("Patrick Hand, Newsreader, Georgia, serif");
        assert!("₀₁₂₃₄₅₆₇₈₉".chars().all(|digit| hand.glyph(digit).is_some()));
    }

    #[test]
    fn lent_script_signs_measure_as_the_face_that_draws_them() {
        // RichView Sans lends ⁺ ⁻ ₊ ₋ to every face without them, so a label such as `1.2 × 10⁻⁶`
        // (FEA-CH-23) is measured rather than estimated; in a monospaced face each sign takes the
        // cell (scripts/fonts/README.md).
        for (family, ..) in FACES {
            let font = Font::new(&format!("\"{family}\", serif"));
            for sign in "⁺⁻₊₋".chars() {
                assert!(font.glyph(sign).is_some(), "{family} {sign}");
            }
        }
        let mono = Font::new("\"Geist Mono\", monospace");
        assert!("⁺⁻₊₋".chars().all(|sign| mono.glyph(sign) == mono.glyph('0')));
    }

    #[test]
    fn unknown_faces_fall_back_to_a_stated_estimate() {
        let brand = Font::new("Brand Sans, sans-serif");
        assert!(!brand.is_measured());
        assert!((brand.width("abc", 10.0) - 18.0).abs() < 1e-9);
    }

    #[test]
    fn wrapping_balances_lines_and_never_cuts_a_word() {
        let font = Font::new("Geist Mono");
        let lines = font.wrap("North-east coastal region", 12.0, 130.0, 2);
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines.join(" ").replace("- ", "-"), "North-east coastal region");
        let a = font.width(&lines[0], 12.0);
        let b = font.width(&lines[1], 12.0);
        assert!((a - b).abs() < font.width("coastal", 12.0), "{lines:?}");
        assert_eq!(
            font.wrap("Supercalifragilistic", 12.0, 20.0, 2),
            vec!["Supercalifragilistic"]
        );
    }
}
