use richview_chart::svg::namespace_resources;

#[test]
fn resource_names_are_translated_once_even_when_a_prefix_matches_an_existing_name() {
    let svg = r##"<svg viewBox="0 0 240 100"><defs><pattern id="clip"/><pattern id="panel-clip"/></defs><rect fill="url(#clip)"/><circle fill="url(#panel-clip)"/></svg>"##;
    let output = namespace_resources(svg, "panel");
    let document = roxmltree::Document::parse(&output).unwrap();
    let fills: Vec<_> = document
        .descendants()
        .filter_map(|node| node.attribute("fill"))
        .collect();
    assert_eq!(fills, ["url(#panel-clip)", "url(#panel-panel-clip)"]);
}
