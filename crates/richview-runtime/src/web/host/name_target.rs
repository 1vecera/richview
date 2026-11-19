//! Where a series name's button lies over the name the canvas draws (FEA-CH-27): pure geometry,
//! so it is tested without a browser.

use richview_chart::geometry::Rect;

/// The margin of a name button around its text.
pub(super) const MARGIN: f64 = 2.0;

/// A name button's target over the text at `r`: the name with its margin, and at least 24 by 24
/// CSS pixels (WCAG 2.5.8). The hit area beyond the name is centred on it, except beside a dot of
/// the name's own line: a name stands 5 px from its line, so a centred target would take the
/// pointer on the dot it names, and whoever points at that value would get the series, not the
/// value. There the target starts `CLEAR` past the dot, at the name's own edge at the latest, and
/// its extra area grows away from the line.
pub(super) fn of(r: Rect, dots: &[[f64; 2]]) -> Rect {
    const MIN_TARGET: f64 = 24.0;
    // How far the target keeps from a dot's centre: a pointer on the dot is off it by more than
    // the fraction of a pixel a browser rounds a pointer by.
    const CLEAR: f64 = 3.0;
    let name = r.inflate(MARGIN);
    let (w, h) = (name.w.max(MIN_TARGET), name.h.max(MIN_TARGET));
    let mut target = Rect::new(
        name.x + name.w / 2.0 - w / 2.0,
        name.y + name.h / 2.0 - h / 2.0,
        w,
        h,
    );
    for dot in dots {
        if !target.inflate(CLEAR).contains(*dot) {
            continue;
        }
        if dot[1] <= name.y {
            target.y = target.y.max((dot[1] + CLEAR).min(name.y));
        } else if dot[1] >= name.bottom() {
            target.y = target.y.min((dot[1] - CLEAR).max(name.bottom()) - h);
        } else if dot[0] <= name.x {
            target.x = target.x.max((dot[0] + CLEAR).min(name.x));
        } else if dot[0] >= name.right() {
            target.x = target.x.min((dot[0] - CLEAR).max(name.right()) - w);
        }
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_under_its_line_keeps_its_target_off_the_dot_it_names() {
        // The flagship's Epoch chart: Anthropic stands 5 px under its line at April 2025.
        let name = Rect::new(299.9, 125.5, 64.8, 15.6);
        let dot = [299.1, 120.5];
        let centred = of(name, &[]);
        assert!(
            (centred.y - 121.3).abs() < 1e-9 && centred.h == 24.0,
            "{centred:?}"
        );
        assert!(
            centred.inflate(1.0).contains(dot),
            "the centred target reaches the dot"
        );
        let target = of(name, &[dot, [316.8, 116.1]]);
        assert!(!target.inflate(2.9).contains(dot), "{target:?}");
        assert_eq!((target.w, target.h), (centred.w, 24.0));
        assert!(name.inflate(MARGIN).inside(&target, 1e-9), "{target:?}");
    }

    #[test]
    fn a_name_over_its_line_or_beside_its_end_grows_away_from_the_dot() {
        let name = Rect::new(100.0, 50.0, 12.0, 15.6);
        let over = of(name, &[[106.0, 71.0]]);
        assert!(over.bottom() <= 68.0 + 1e-9 && over.h == 24.0, "{over:?}");
        let beside = of(name, &[[95.0, 58.0]]);
        assert!(beside.x >= 98.0 - 1e-9 && beside.w == 24.0, "{beside:?}");
        assert!(name.inflate(MARGIN).inside(&beside, 1e-9), "{beside:?}");
        // A dot the centred target does not come near changes nothing.
        assert_eq!(of(name, &[[60.0, 58.0]]), of(name, &[]));
    }
}
