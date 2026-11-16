//! A count and the noun and verb beside it agree: one row is "1 row ... is not shown", never
//! "1 rows ... are not shown" (the notes under a chart and the checks name the same rows).

mod common;

use common::*;
use serde_json::{Value, json};

/// The note under the chart and the messages of its checks.
fn words(spec: &Value, rows: &[Value]) -> Vec<String> {
    let g = draw(spec, rows);
    g.note
        .into_iter()
        .chain(g.checks.iter().map(|c| c.message.clone()))
        .collect()
}

fn says(words: &[String], text: &str) -> bool {
    words.iter().any(|w| w.contains(text))
}

#[test]
fn one_excluded_row_is_a_row_and_two_are_rows() {
    let spec = json!({"type":"lollipop","title":"Orders","encoding":{"x":"region","y":"orders"}});
    let mut rows = vec![
        json!({"region":"North","orders":4}),
        json!({"region":"South","orders":2}),
    ];
    rows.push(json!({"region":null,"orders":7}));
    let one = words(&spec, &rows);
    assert!(
        says(&one, "1 row without a category, group or value is left out"),
        "{one:?}"
    );
    assert!(says(&one, "1 row without a value is not shown"), "{one:?}");
    rows.push(json!({"region":null,"orders":8}));
    let two = words(&spec, &rows);
    assert!(
        says(&two, "2 rows without a category, group or value are left out"),
        "{two:?}"
    );
    assert!(says(&two, "2 rows without a value are not shown"), "{two:?}");
}

#[test]
fn a_missing_value_gap_and_a_pie_without_a_slice_agree_too() {
    let bar = json!({"type":"bar","title":"Share","encoding":{"x":"year","y":"share"}});
    let one = words(
        &bar,
        &[json!({"year":"2023","share":null}), json!({"year":"2024","share":2})],
    );
    assert!(says(&one, "1 row has no share value and is drawn as a gap"), "{one:?}");
    let two = words(
        &bar,
        &[
            json!({"year":"2023","share":null}),
            json!({"year":"2024","share":null}),
            json!({"year":"2025","share":2}),
        ],
    );
    assert!(
        says(&two, "2 rows have no share value and are drawn as gaps"),
        "{two:?}"
    );
    let pie = json!({"type":"pie","title":"Hours","encoding":{"x":"a","y":"h"}});
    let one = words(&pie, &[json!({"a":"a","h":null}), json!({"a":"b","h":2})]);
    assert!(says(&one, "1 row has no h value and gets no slice"), "{one:?}");
}

#[test]
fn a_link_that_closes_a_cycle_agrees() {
    let spec = json!({"type":"sankey","title":"Flow","encoding":{"source":"from","target":"to","y":"n"}});
    let rows = vec![
        json!({"from":"A","to":"B","n":3}),
        json!({"from":"B","to":"C","n":2}),
        json!({"from":"C","to":"A","n":1}),
    ];
    let one = words(&spec, &rows);
    assert!(says(&one, "1 link closes a cycle and is left out"), "{one:?}");
    assert!(says(&one, "1 link that closes a cycle is not shown"), "{one:?}");
}
