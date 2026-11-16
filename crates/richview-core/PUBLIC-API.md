# Core public API

The document pipeline is `richview_core::document`. Document types, options, diagnostics, resources and operations are defined or explicitly reexported there; the crate root does not provide duplicate public aliases. Worker serialization retains private ownership and source-position metadata.

Cargo metadata identifies the workspace dependents; repository searches identify their actual item uses. Every retained crate-root exception has a reason below. Supporting types remain public when a public result or field requires them.

| Root item | Reason to remain public |
| --- | --- |
| `ASSET_FINDINGS` | App resolution preserves the shared missing/escaping resource diagnostic families. |
| `AssetRef` | App upload and resolver code share the validated external asset reference. |
| `AssetUse` | App upload policy uses the same allowed media categories as validation. |
| `CHART_TYPES` | Build documentation, editor completion and the site share author-visible chart names. |
| `HighlightLanguage` | Build and site highlighting select the same supported language. |
| `PublishRequest` | The publication request returned to app/site callers has a named public type. |
| `RICH_TAGS` | Build highlighting, editor fences and site authoring share rich fence names. |
| `ResolvedStyle` | Build chrome, app fonts and share cards consume the resolved style and its public fields. |
| `SLUG_GRAMMAR` | App refusal diagnostics report the shared publication identity grammar. |
| `StyleGrammar` | The public style field record exposes this named grammar type. |
| `StyleLeaf` | The public style vocabulary returns these named field records. |
| `StyleMark` | The public style field record exposes this conformance classification. |
| `Visibility` | App uploads and asset storage share visibility metadata. |
| `authoring_fields` | Editor completion derives data fields from the parsed document and supplied resources. |
| `authoring_keys` | Editor completion reads allowed keys from the shared vocabulary. |
| `authoring_values` | Editor completion reads allowed values from the shared vocabulary. |
| `brand_font_files` | App resolution discovers the fonts a resolved style requires. |
| `builtin_style_names` | Build, app, editor and site enumerate builtins from the producer. |
| `builtin_style_source` | Site authoring loads a selected builtin source. |
| `bundled_font_catalog` | The native build stages the producer font catalog. |
| `create_publish_request` | App/site callers derive and validate publication identity without performing I/O. |
| `custom_properties_to_css` | The site style adapter validates CSS variables through core. |
| `document` | Hosts parse, validate, resolve resources, render, check and serialize reports through this module. |
| `highlight_html` | Build documentation and site authoring render escaped highlighted source. |
| `image_color` | App photo suggestions validate the author-visible image color grammar. |
| `is_loopback` | App URL resolution shares the loopback policy with asset parsing. |
| `is_retired_style_name` | App style resolution recognizes retired names consistently. |
| `media_type_for_extension` | App assets derive allowed media types from one mapping. |
| `parse_schema` | The site schema adapter uses the same validation and normalization as core. |
| `primary_style` | The site authoring controls inspect the authored primary style. |
| `provide_font_catalog` | The Wasm site host supplies the staged producer font catalog. |
| `registry` | App reference output, editor snippets and vocabulary conformance tests read the shared schema. |
| `render_compat` | The site worker adapter consumes the established JSON render envelope. |
| `render_table` | The site table adapter renders the same semantic table as a report. |
| `resolve_styles` | Build chrome, app fonts and share cards use the same cascade. |
| `resolve_styles_compat` | The site style adapter retains the established JSON cascade envelope. |
| `slug_problem` | App publication and storage validate slugs with the same explanation. |
| `spec` | The specification generation contract checks and blesses the registry-derived reference tables. |
| `style_checks` | The independent style conformance suite measures styles through the producer checks. |
| `style_vocabulary` | The independent style conformance suite checks the closed authoring vocabulary. |
| `unsplash_image_url` | App resolution recognizes the supported photo origin without fetching it. |
| `version_problem` | App publication and storage validate versions with the same explanation. |
| `with_primary_style` | Build previews and site authoring change only the authored primary style. |
| `yaml` | App connector/asset configuration, editor formatting, Publish and benchmarks use the same strict YAML profile. |

The document, YAML and specification interfaces are checked by `tools/api-contracts`. The core integration test `tests/registry.rs::core_public_exports_match_the_reviewed_snapshot` checks all registered API/resource contracts and uses the same declaration parser to check `snapshots/core-root.json`, pinning every root name/module/reexport target and the curated host/style utility definitions. Registry data generated by macros remains outside that source snapshot; no whole-core dependent-check reduction is authorized. Core keeps the full affected-dependent checks.

Positioned YAML loading, numeric formatting internals, retired registry rules and source-rewrite helpers are private. Slug derivation is tested inside its implementation module; publication behavior remains covered through public requests. The obsolete unused wikilink-target helper is removed; the renderer already rejects that retired Markdown spelling.
