use richview_chart::box_summary::summarize;

fn sample(values: &[f64]) -> Vec<(usize, f64)> {
    values.iter().copied().enumerate().collect()
}

#[test]
fn r7_quartiles_use_linear_positions_in_the_sorted_sample() {
    let result = summarize(&sample(&[30.0, 0.0, 10.0])).unwrap();
    assert_eq!(
        [result.min, result.q1, result.median, result.q3, result.max],
        [0.0, 5.0, 10.0, 20.0, 30.0]
    );
    assert_eq!([result.lower_whisker, result.upper_whisker], [0.0, 30.0]);
    assert!(result.outliers.is_empty());
    let even = summarize(&sample(&[1.0, 2.0, 3.0, 4.0])).unwrap();
    assert_eq!([even.q1, even.median, even.q3], [1.75, 2.5, 3.25]);
}

#[test]
fn inclusive_fences_use_observed_whiskers_and_preserve_outlier_source_identity() {
    let values = [100.0, 1.0, 2.0, 3.0, 4.0, -100.0, 5.0, 6.0, 7.0];
    let input = sample(&values);
    let result = summarize(&input).unwrap();
    assert_eq!(input, sample(&values));
    assert_eq!([result.q1, result.median, result.q3], [2.0, 4.0, 6.0]);
    assert_eq!([result.lower_whisker, result.upper_whisker], [1.0, 7.0]);
    assert_eq!(result.outliers, [0, 5]);
    let boundary = summarize(&sample(&[-6.0, 0.0, 0.0, 2.0, 4.0, 4.0, 10.0])).unwrap();
    assert_eq!([boundary.q1, boundary.q3], [0.0, 4.0]);
    assert_eq!([boundary.lower_whisker, boundary.upper_whisker], [-6.0, 10.0]);
    assert!(boundary.outliers.is_empty());
    let outside = summarize(&sample(&[
        (-6.0f64).next_down(),
        0.0,
        0.0,
        2.0,
        4.0,
        4.0,
        10.0f64.next_up(),
    ]))
    .unwrap();
    assert_eq!(outside.outliers, [0, 6]);
}

#[test]
fn singleton_and_repeated_values_do_not_create_false_box_width_or_whiskers() {
    for values in [vec![7.0], vec![7.0; 20]] {
        let result = summarize(&sample(&values)).unwrap();
        assert_eq!(
            [
                result.min,
                result.q1,
                result.median,
                result.q3,
                result.max,
                result.lower_whisker,
                result.upper_whisker
            ],
            [7.0; 7]
        );
        assert!(result.outliers.is_empty());
    }
    let result = summarize(&sample(&[0.0, 0.0, 0.0, 0.0, 1.0])).unwrap();
    assert_eq!([result.q1, result.q3], [0.0, 0.0]);
    assert_eq!(result.outliers, [4]);
}

#[test]
fn finite_extremes_and_adjacent_floats_retain_finite_ordered_statistics() {
    for values in [
        vec![-f64::MAX, f64::MAX],
        vec![-1e308, 0.0, 1e308],
        vec![1.0, 1.0f64.next_up()],
        vec![f64::from_bits(1), f64::from_bits(2)],
    ] {
        let result = summarize(&sample(&values)).unwrap();
        let summary = [result.min, result.q1, result.median, result.q3, result.max];
        assert!(summary.iter().all(|value| value.is_finite()));
        assert!(summary.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(result.lower_whisker <= result.upper_whisker);
    }
}

#[test]
fn fence_comparisons_survive_overflow_and_subnormal_intervals() {
    // Q1=.3*MAX, Q3=MAX: 1.5*IQR overflows, but -MAX is still below the lower fence.
    let m = f64::MAX;
    let result = summarize(&sample(&[-m, m * 0.3, m * 0.3, m * 0.5, m, m, m])).unwrap();
    assert_eq!(result.outliers, [0]);
    let t = f64::from_bits(1);
    let result = summarize(&sample(&[0.0, 0.0, 0.0, t, t, t, 3.0 * t])).unwrap();
    assert_eq!(result.outliers, [6]);
}

#[test]
fn empty_and_nonfinite_inputs_are_explicit_errors() {
    assert!(summarize(&[]).is_err());
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(summarize(&[(0, value)]).is_err());
    }
}
