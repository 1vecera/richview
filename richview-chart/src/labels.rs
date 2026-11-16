//! One-dimensional label placement.
//!
//! Direct labels sit in a column beside the plot. Each label wants to stay at its anchor (the
//! value it names) but neighbours need a minimum distance. [`place_column`] finds the positions
//! that move the labels least in total (squared displacement), which keeps every label as close to
//! its value as the column allows. Greedy push-down/shift-up rules can move a whole stack further
//! than necessary; this solve is exact.

/// Place labels of height `h` near their `anchor` (both in pixels, `(anchor, h)` per item) inside
/// `[top, bottom]` with at least `gap` between neighbours. Returns each label's centre in input
/// order, or `None` when the labels cannot fit the band.
///
/// With items sorted by anchor and offsets `cᵢ = cᵢ₋₁ + (hᵢ₋₁ + hᵢ)/2 + gap`, positions are
/// `yᵢ = qᵢ + cᵢ` with non-decreasing `q`. Minimising `Σ(yᵢ − anchorᵢ)²` is then an isotonic
/// regression of `anchorᵢ − cᵢ`, solved by pool-adjacent-violators, and clamping the solution to the
/// band is optimal because every `qᵢ` shares the same bounds.
pub fn place_column(items: &[(f64, f64)], top: f64, bottom: f64, gap: f64) -> Option<Vec<f64>> {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by(|&a, &b| items[a].0.total_cmp(&items[b].0).then(a.cmp(&b)));
    place_column_ordered(items, &order, top, bottom, gap)
}

/// Place the labels in a supplied reading order, retaining the same least-squares solve.
/// The exact-row layout supplies this order when nearby grid anchors could exchange places.
pub(crate) fn place_column_ordered(
    items: &[(f64, f64)],
    order: &[usize],
    top: f64,
    bottom: f64,
    gap: f64,
) -> Option<Vec<f64>> {
    if !top.is_finite() || !bottom.is_finite() || !gap.is_finite() || gap < 0.0 || top > bottom {
        return None;
    }
    if items.is_empty() {
        return Some(Vec::new());
    }
    if items.iter().any(|(a, h)| !a.is_finite() || !h.is_finite() || *h < 0.0) {
        return None;
    }
    if order.len() != items.len() {
        return None;
    }
    let mut seen = vec![false; items.len()];
    for &i in order {
        if i >= items.len() || seen[i] {
            return None;
        }
        seen[i] = true;
    }
    let h = |k: usize| items[order[k]].1;
    let mut offsets = vec![0.0; order.len()];
    for k in 1..order.len() {
        offsets[k] = offsets[k - 1] + (h(k - 1) + h(k)) / 2.0 + gap;
    }
    let last = order.len() - 1;
    let lo = top + h(0) / 2.0;
    let hi = bottom - h(last) / 2.0 - offsets[last];
    if lo > hi + 1e-9 {
        return None;
    }
    // Pool adjacent violators: blocks of (sum, count) whose means are non-decreasing.
    let mut blocks: Vec<(f64, usize)> = Vec::with_capacity(order.len());
    for k in 0..order.len() {
        blocks.push((items[order[k]].0 - offsets[k], 1));
        while blocks.len() > 1 {
            let (s1, n1) = blocks[blocks.len() - 1];
            let (s0, n0) = blocks[blocks.len() - 2];
            if s0 / n0 as f64 <= s1 / n1 as f64 {
                break;
            }
            blocks.pop();
            let merged = blocks.last_mut().expect("two blocks");
            *merged = (s0 + s1, n0 + n1);
        }
    }
    let mut out = vec![0.0; order.len()];
    let mut k = 0;
    for (sum, count) in blocks {
        let q = (sum / count as f64).clamp(lo, hi.max(lo));
        for _ in 0..count {
            out[order[k]] = q + offsets[k];
            k += 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cost(items: &[(f64, f64)], ys: &[f64]) -> f64 {
        items.iter().zip(ys).map(|((a, _), y)| (y - a).powi(2)).sum()
    }

    fn assert_valid(items: &[(f64, f64)], ys: &[f64], top: f64, bottom: f64, gap: f64) {
        let mut placed: Vec<(f64, f64)> = ys.iter().copied().zip(items.iter().map(|i| i.1)).collect();
        placed.sort_by(|a, b| a.0.total_cmp(&b.0));
        for w in placed.windows(2) {
            assert!(
                w[1].0 - w[0].0 >= (w[0].1 + w[1].1) / 2.0 + gap - 1e-9,
                "neighbours keep the gap: {placed:?}"
            );
        }
        for (y, h) in &placed {
            assert!(
                y - h / 2.0 >= top - 1e-9 && y + h / 2.0 <= bottom + 1e-9,
                "inside the band"
            );
        }
    }

    /// Exhaustive reference: every partition of the sorted labels into contiguous blocks that
    /// touch internally, each block at its own best (mean, clamped) position. The optimum is one
    /// of these candidates, so the cheapest feasible candidate is the exact answer.
    fn brute_force(items: &[(f64, f64)], top: f64, bottom: f64, gap: f64) -> Option<f64> {
        let mut sorted = items.to_vec();
        sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
        let n = sorted.len();
        let mut offsets = vec![0.0; n];
        for k in 1..n {
            offsets[k] = offsets[k - 1] + (sorted[k - 1].1 + sorted[k].1) / 2.0 + gap;
        }
        let lo = top + sorted[0].1 / 2.0;
        let hi = bottom - sorted[n - 1].1 / 2.0 - offsets[n - 1];
        if lo > hi {
            return None;
        }
        let mut best: Option<f64> = None;
        for mask in 0..(1u32 << (n - 1)) {
            let mut q = vec![0.0; n];
            let mut start = 0;
            for k in 0..n {
                if k == n - 1 || mask & (1 << k) != 0 {
                    let block = start..=k;
                    let mean = block.clone().map(|i| sorted[i].0 - offsets[i]).sum::<f64>() / (k + 1 - start) as f64;
                    for i in block {
                        q[i] = mean.clamp(lo, hi);
                    }
                    start = k + 1;
                }
            }
            if q.windows(2).all(|w| w[0] <= w[1] + 1e-12) {
                let total: f64 = (0..n).map(|i| (q[i] + offsets[i] - sorted[i].0).powi(2)).sum();
                best = Some(best.map_or(total, |b| b.min(total)));
            }
        }
        best
    }

    #[test]
    fn separated_labels_stay_at_their_anchors() {
        let items = [(20.0, 12.0), (60.0, 12.0), (100.0, 12.0)];
        assert_eq!(place_column(&items, 0.0, 200.0, 2.0).unwrap(), vec![20.0, 60.0, 100.0]);
    }

    #[test]
    fn coincident_labels_split_symmetrically_around_their_anchor() {
        let ys = place_column(&[(50.0, 12.0), (50.0, 12.0)], 0.0, 200.0, 2.0).unwrap();
        assert!((ys[0] - 43.0).abs() < 1e-9 && (ys[1] - 57.0).abs() < 1e-9, "{ys:?}");
    }

    #[test]
    fn labels_keep_input_order_and_the_band() {
        let items = [(195.0, 12.0), (2.0, 12.0), (199.0, 12.0)];
        let ys = place_column(&items, 0.0, 200.0, 2.0).unwrap();
        assert_valid(&items, &ys, 0.0, 200.0, 2.0);
        assert!(ys[1] < ys[0] && ys[0] < ys[2], "{ys:?}");
        assert!((ys[1] - 6.0).abs() < 1e-9, "the low label clamps to the band top");
        assert!((ys[2] - 194.0).abs() < 1e-9, "the high labels clamp to the band bottom");
    }

    #[test]
    fn infeasible_columns_return_none() {
        let items = vec![(50.0, 12.0); 8];
        assert!(place_column(&items, 0.0, 100.0, 2.0).is_none());
        assert!(place_column(&items, 0.0, 110.0, 2.0).is_some());
        assert!(place_column(&[(f64::NAN, 12.0)], 0.0, 100.0, 2.0).is_none());
    }

    #[test]
    fn minimises_total_displacement_against_brute_force() {
        // Deterministic pseudo-random cases (xorshift), small enough to enumerate every partition.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        for case in 0..400 {
            let n = 1 + case % 6;
            let bottom = 60.0 + next() * 140.0;
            let gap = next() * 3.0;
            let items: Vec<(f64, f64)> = (0..n)
                .map(|_| (-10.0 + next() * (bottom + 20.0), 8.0 + next() * 10.0))
                .collect();
            let solved = place_column(&items, 0.0, bottom, gap);
            let reference = brute_force(&items, 0.0, bottom, gap);
            assert_eq!(solved.is_some(), reference.is_some(), "feasibility of {items:?}");
            if let (Some(ys), Some(best)) = (solved, reference) {
                assert_valid(&items, &ys, 0.0, bottom, gap);
                assert!(
                    (cost(&items, &ys) - best).abs() <= 1e-6 * best.max(1.0),
                    "{items:?}: {} vs optimum {best}",
                    cost(&items, &ys)
                );
            }
        }
    }
}
