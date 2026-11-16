use richview_chart::scale::{Linear, nice_ticks};

#[test]
fn finite_scale_endpoints_and_midpoint_survive_an_overflowing_span() {
    let scale = Linear::new((-f64::MAX, f64::MAX), (0.0, 720.0));
    for (value, pixel) in [(-f64::MAX, 0.0), (0.0, 360.0), (f64::MAX, 720.0)] {
        assert_eq!(scale.map(value), pixel);
        assert_eq!(scale.invert(pixel), value);
    }
    for (lo, hi) in [
        (-f64::MAX, f64::MAX),
        (f64::MAX, f64::MAX),
        (-1e308, 1e308),
        (1e-308, 2e-308),
        (f64::from_bits(1), f64::from_bits(2)),
    ] {
        let ticks = nice_ticks(lo, hi, 5);
        assert!(
            ticks.lo.is_finite() && ticks.hi.is_finite() && ticks.lo < ticks.hi,
            "{lo}–{hi}: {ticks:?}"
        );
        assert!(ticks.lo <= lo && ticks.hi >= hi, "{lo}–{hi}: {ticks:?}");
        assert!(ticks.values.iter().all(|value| value.is_finite()));
        assert!(ticks.values.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(ticks.values.len() >= 2);
    }
}
