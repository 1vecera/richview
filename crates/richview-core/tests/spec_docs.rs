//! The spec's reference tables match the schema registry (ARC-09, TYP-04, DEL-18). Run with
//! `RICHVIEW_BLESS=1` to rewrite the marked regions from the registry.
use richview_core::spec::{fragment_names, marked, splice};
use std::path::Path;

#[test]
fn the_spec_tables_are_the_registry() {
    for (file, expected) in [
        ("docs/RMD-SPEC.md", fragment_names()),
        ("docs/public/styles.md", vec!["styles".to_owned()]),
    ] {
        check_reference(file, expected);
    }
}

fn check_reference(file: &str, mut expected: Vec<String>) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(file);
    let doc = std::fs::read_to_string(&path).unwrap();
    let fresh = splice(&doc).unwrap_or_else(|error| panic!("{file}: {error}"));
    if std::env::var_os("RICHVIEW_BLESS").is_some() {
        std::fs::write(&path, &fresh).unwrap();
    } else if fresh != doc {
        let line = doc
            .lines()
            .zip(fresh.lines())
            .position(|(old, new)| old != new)
            .unwrap_or(doc.lines().count().min(fresh.lines().count()))
            + 1;
        panic!(
            "{file} differs from the schema registry from line {line}. Edit the registry, then run \
             `RICHVIEW_BLESS=1 cargo test -p richview-core --test integration -- spec_docs::`."
        );
    }
    // Every generated table appears exactly once, so none can be dropped or duplicated by hand.
    let mut found = marked(&fresh);
    found.sort_unstable();
    expected.sort_unstable();
    assert_eq!(
        found, expected,
        "{file} must mark each registry fragment once"
    );
}
