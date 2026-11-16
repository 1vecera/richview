# Chart diagnostics

`richview check report.rmd --format json` returns source findings and chart layout findings from the same Rust render used to create the SVG. It preserves source lines, chart IDs, dimensions and dropped-label provenance. Input validation alone does not establish readability.

Native checks compare emitted mark geometry, label boxes, clipping regions and collisions. Generic SVG labels constrain their horizontal advances with `textLength`; vertical font bounds still use layout estimates. Temporal layout also uses font metrics supplied by its caller. Native output reports this limitation explicitly through `geometryMeasured` and its `measurement` description. A `clean` native result means no defect was found by those checks; it is not proof that every browser/font combination is readable.

Browser evidence should accompany font, responsive-layout or annotation changes.

Read `checks` and `droppedLabels`, revise the source, and re-render. Findings include severity, kind/message, regions and related labels where available. Thinning and M4/aggregation decisions are disclosed. Exact source values remain available through hover and the table even when the displayed path contains fewer points.

`validate` checks syntax/schema. `check` adds render/layout diagnostics. `generate --report-card` and VS Code preview can show the same findings in the report. Recoverable failures remain visible and actionable; publication rejects hard source errors. No automatic check authorizes external publication.
