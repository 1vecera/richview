# Entity IDs

Every entity ID (`refId`) defined in `parts/`, with its title, the item that defines it, its part and, for items in parts 04a to 04d, the part 03 component card it names as `component`: 771 IDs from 772 items (FEA-SF-62 defines no entity), derived from the part files on 2026-10-27. Item IDs (`INT-`, `DEL-`, `ARC-`, `FEA-`, `TYP-`, `STY-`, `PRF-`, `OPN-`) are the cards themselves; the prefix table in `README.md` lists the entity prefixes each part uses.

After the entity tables come the features of each component, the IDs that appear without a defining item, and the evidence names the guide cites without a card of their own. Every entity or item ID cited in `docs/guide/`, the lockdown files and the part files is either defined in a part or listed in one of these tables.

## C- Components (41)

| ID | Title | Item | Part |
| --- | --- | --- | --- |
| C-AGENT-HARNESS | Agent instructions and definitions | ARC-51 | 03-architecture |
| C-APP-ASSET-UPLOAD | Asset uploader | ARC-34 | 03-architecture |
| C-APP-CLI | CLI | ARC-28 | 03-architecture |
| C-APP-CONNECTORS | Data connectors (PR #66) | ARC-59 | 03-architecture |
| C-APP-HTTP-SERVER | HTTP host | ARC-31 | 03-architecture |
| C-APP-MCP | MCP server | ARC-33 | 03-architecture |
| C-APP-PDF-EXPORT | PDF printer and browser host service | ARC-30 | 03-architecture |
| C-APP-REPORT-STORE | Report store | ARC-32 | 03-architecture |
| C-APP-RESOLVER | Resolver and exporter | ARC-29 | 03-architecture |
| C-BENCH-LABS | Comparison labs and chart corpora (`benchmarks/`) | ARC-64 | 03-architecture |
| C-CI | CI workflow | ARC-50 | 03-architecture |
| C-CLOUDFLARE-TEASER | Cloudflare site Worker and signup | ARC-55 | 03-architecture |
| C-CONTAINER-DEPLOY | Container image and Fly config | ARC-56 | 03-architecture |
| C-DEV-HARNESS | Development harness (PR #68) | ARC-61 | 03-architecture |
| C-DOCS-SITE | Documentation site | ARC-43 | 03-architecture |
| C-EXAMPLES | Example reports | ARC-54 | 03-architecture |
| C-FONT-PIPELINE | Font pipeline and catalogs | ARC-62 | 03-architecture |
| C-GALLERIES | Standalone galleries (PR #65) | ARC-60 | 03-architecture |
| C-LLMS-TXT | llms.txt | ARC-53 | 03-architecture |
| C-LOCKDOWN-TOOLING | Lockdown, guide and review app | ARC-65 | 03-architecture |
| C-PKG-CHARTS | npm package @richview/charts | ARC-49 | 03-architecture |
| C-PKG-CORE | npm package @richview/core | ARC-48 | 03-architecture |
| C-RICHVIEW-APP | richview-app: native host crate | ARC-27 | 03-architecture |
| C-RICHVIEW-BENCH | Benchmark crate | ARC-47 | 03-architecture |
| C-RICHVIEW-BUILD | Build orchestrator | ARC-46 | 03-architecture |
| C-RICHVIEW-CHART | richview-chart: chart engine | ARC-25 | 03-architecture |
| C-RICHVIEW-CORE | richview-core: format, styles and renderer | ARC-26 | 03-architecture |
| C-RICHVIEW-EDITOR | Editor service and its protocol | ARC-44 | 03-architecture |
| C-RICHVIEW-PUBLISH | RichView Publish and News (PR #70) | ARC-57 | 03-architecture |
| C-RICHVIEW-WEB | richview-web: browser host | ARC-35 | 03-architecture |
| C-SKILL | Agent skill | ARC-52 | 03-architecture |
| C-VERIFY-HARNESS | Verification scripts (`scripts/`) | ARC-63 | 03-architecture |
| C-VOICE-DASHBOARD | Voice-to-dashboard demo (PR #64) | ARC-58 | 03-architecture |
| C-VSCODE-EXTENSION | VS Code extension | ARC-45 | 03-architecture |
| C-WEB-CHART-EXAMPLES | Chart examples page (/charts/) | ARC-40 | 03-architecture |
| C-WEB-CHART-HOST | Live chart chrome | ARC-37 | 03-architecture |
| C-WEB-LANDING | Landing page | ARC-39 | 03-architecture |
| C-WEB-PLAYGROUND | Playground | ARC-38 | 03-architecture |
| C-WEB-REEL | Reel | ARC-42 | 03-architecture |
| C-WEB-SHOWCASE | Showcase | ARC-41 | 03-architecture |
| C-WEB-WASM-API | Wasm JavaScript API | ARC-36 | 03-architecture |

## F- Features (204)

| ID | Title | Item | Part | Component |
| --- | --- | --- | --- | --- |
| F-A11Y-FLOOR | Accessibility standard and its check | FEA-RP-49 | 04b-features-report | C-RICHVIEW-CORE |
| F-AGENT-USE-MEASURE | Measuring how well agents use RichView | FEA-SF-65 | 04c-features-surfaces | C-RICHVIEW-CORE |
| F-APP-ASSET-PUT | Asset upload commands and tools | FEA-SF-50 | 04c-features-surfaces | C-APP-ASSET-UPLOAD |
| F-APP-CLI-CHECK | `richview check`: rendered chart geometry | FEA-SF-04 | 04c-features-surfaces | C-APP-CLI |
| F-APP-CLI-GENERATE | `richview generate`: write the self-contained report | FEA-SF-05 | 04c-features-surfaces | C-APP-CLI |
| F-APP-CLI-INIT | `richview init`: start a report folder | FEA-SF-02 | 04c-features-surfaces | C-APP-CLI |
| F-APP-CLI-PUBLISH | Publishing from the CLI, MCP and VS Code | FEA-SF-12 | 04c-features-surfaces | C-APP-CLI |
| F-APP-CLI-SERVE | `richview serve`: live preview in a browser | FEA-SF-08 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-APP-CLI-SERVER | `richview server`: RichView Cowork, the self-hosted server | FEA-SF-20 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-APP-CLI-VALIDATE | `richview validate`: structure without rendering | FEA-SF-03 | 04c-features-surfaces | C-APP-CLI |
| F-APP-CONNECTORS-CACHE | Snapshots, freshness and provenance (Publish, secondary) | FEA-BR-16 | 04d-features-branches | C-APP-CONNECTORS |
| F-APP-CONNECTORS-METRICFLOW | dbt MetricFlow connector (Publish, secondary) | FEA-BR-15 | 04d-features-branches | C-APP-CONNECTORS |
| F-APP-CONNECTORS-PARAMS | Query parameters (Publish, secondary) | FEA-BR-43 | 04d-features-branches | C-APP-CONNECTORS |
| F-APP-CONNECTORS-SQL | SQL connector (Publish, secondary) | FEA-BR-14 | 04d-features-branches | C-APP-CONNECTORS |
| F-APP-EXPORT-RENDERED | The `Rendered/` folder | FEA-RP-40 | 04b-features-report | C-APP-RESOLVER |
| F-APP-MCP-RESOURCES | MCP resources | FEA-SF-11 | 04c-features-surfaces | C-APP-MCP |
| F-APP-MCP-TRANSPORT | MCP connection and setup | FEA-SF-10 | 04c-features-surfaces | C-APP-MCP |
| F-APP-PDF-EXPORT | PDF output | FEA-SF-06 | 04c-features-surfaces | C-APP-PDF-EXPORT |
| F-APP-RESOLVE-MEDIA | Media embedded from the report folder | FEA-RP-42 | 04b-features-report | C-APP-RESOLVER |
| F-APP-SERVER-READ-API | Report list and bundle API | FEA-SF-15 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-APP-SERVER-REGENERATE | Regenerate a stored version | FEA-SF-16 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-APP-SERVER-RENDER-API | HTTP render API | FEA-SF-17 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-APP-SERVER-REPORT-PAGES | Published report pages and raw source | FEA-SF-14 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-BLOCK-LOCAL-DATA | Data next to the block that uses it | FEA-RP-41 | 04b-features-report | C-RICHVIEW-CORE |
| F-BROWSER-SUPPORT | Browsers a report supports | FEA-RP-59 | 04b-features-report | C-RICHVIEW-CORE |
| F-BUILTIN-STRINGS | RichView’s own words in a report | FEA-RP-60 | 04b-features-report | C-RICHVIEW-CORE |
| F-CHART-A11Y | Accessible chart structure | FEA-CH-55 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-ANNOTATIONS | Annotations | FEA-CH-32 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-AREA | Area charts | FEA-CH-04 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-AXES | Value range and zero | FEA-CH-19 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-AXIS-NOTATION | Percentages, scientific notation and units | FEA-CH-23 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-BAR | Bar charts | FEA-CH-06 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-BAR-SELF-COLOUR | Bars coloured by their own category | FEA-CH-72 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-BOXPLOT | Box plots | FEA-CH-12 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-BROWSER-MEASUREMENT | Browser glyph measurement in `check` | FEA-CH-59 | 04a-features-chart | C-APP-PDF-EXPORT |
| F-CHART-CATEGORY-LABELS | Category names are never cut | FEA-CH-26 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-CATEGORY-PERIODS | Period names on category axes | FEA-CH-73 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-CHECKS-BY-TYPE | Checks per chart type | FEA-CH-69 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-CONTROLS | Authored chart controls | FEA-BR-01 | 04d-features-branches | C-RICHVIEW-CHART |
| F-CHART-CONTROLS-FILTER | Filter | FEA-BR-03 | 04d-features-branches | C-RICHVIEW-CHART |
| F-CHART-CONTROLS-METRIC | Metric switch | FEA-BR-04 | 04d-features-branches | C-RICHVIEW-CHART |
| F-CHART-CONTROLS-PLAYBACK | Playback | FEA-BR-02 | 04d-features-branches | C-WEB-CHART-HOST |
| F-CHART-CONTROLS-VARIANT | Chart variants | FEA-BR-05 | 04d-features-branches | C-RICHVIEW-CHART |
| F-CHART-DECALS | Colour is never the only cue | FEA-CH-57 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-DECIMATION | Dense series | FEA-CH-17 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-DOWNLOADS | PNG download | FEA-CH-47 | 04a-features-chart | C-WEB-CHART-HOST |
| F-CHART-ENTITY-COLOURS | Stable colours for named entities | FEA-CH-63 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-EXACT-READOUTS | Exact source values | FEA-CH-37 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-FACETS | Facet layout and identity | FEA-CH-35 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-HEATMAP | Heatmaps | FEA-CH-10 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-HISTOGRAM | Histograms | FEA-CH-11 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-HIT-TESTING | Pointer and touch targeting | FEA-CH-41 | 04a-features-chart | C-WEB-CHART-HOST |
| F-CHART-INK-ALIGN | Ink alignment of title, first label and first mark | FEA-CH-31 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-INTERACTION-MOTION | Interaction motion | FEA-CH-52 | 04a-features-chart | C-WEB-CHART-HOST |
| F-CHART-INTERACTION-POLICY | Reader interactions per chart type | FEA-CH-39 | 04a-features-chart | C-WEB-CHART-HOST |
| F-CHART-INTERVAL | Interval charts | FEA-CH-13 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-LABEL-PLACEMENT | One placement search; dropped labels are reported | FEA-CH-29 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-LINE | Line charts | FEA-CH-02 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-MISSING-DATA | Missing data stays visible | FEA-CH-38 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-NATIVE-DIAGNOSTICS | Native geometry checks | FEA-CH-58 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-NUMERIC-LIMITS | Numbers at the edge of the representable range | FEA-CH-70 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-PALETTES | How palettes apply to charts | FEA-CH-61 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-PIE | Pie charts | FEA-CH-08 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-RATIO | How charts grow beyond their ratio | FEA-CH-65 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-READOUT-CONTENT | What a readout shows | FEA-CH-40 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-RECT-CLAMP | No negative SVG geometry | FEA-BR-19 | 04d-features-branches | C-RICHVIEW-CHART |
| F-CHART-SCATTER | Scatter plots and bubbles | FEA-CH-07 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-SELECTORS | Selector presentation | FEA-BR-06 | 04d-features-branches | C-WEB-CHART-HOST |
| F-CHART-SETTLED-STATE | The settled chart | FEA-CH-54 | 04a-features-chart | C-WEB-CHART-HOST |
| F-CHART-SOURCE-FAILURE | Charts that cannot be drawn stay visible | FEA-CH-18 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-STACKING | Stacked bars and areas | FEA-CH-05 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-STANDALONE-SVG | Chart SVG renders correctly anywhere | FEA-CH-66 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-TICKS | Tick density | FEA-CH-24 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-TIMESERIES-SPEC | Calendar charts share one painter and one look | FEA-CH-15 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CHART-TRANSFORMS | Time-series features only the JSON spec reaches | FEA-CH-16 | 04a-features-chart | C-RICHVIEW-CHART |
| F-CLI-COMMANDS | Command list | FEA-SF-01 | 04c-features-surfaces | C-APP-CLI |
| F-CLI-HUMAN-OUTPUT | Readable terminal output | FEA-SF-07 | 04c-features-surfaces | C-APP-CLI |
| F-CODE-BLOCKS | Code blocks | FEA-RP-17 | 04b-features-report | C-RICHVIEW-CORE |
| F-COLOPHON | “Made with RichView” colophon | FEA-RP-06 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-BIG-NUMBER | Metric stat line | FEA-RP-23 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-CALLOUT | Callout | FEA-RP-25 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-EMBED | Embedded pages | FEA-RP-28 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-HTML-DOCUMENT | HTML document shell | FEA-RP-02 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-LEARN | `richview learn` and `richview_learn` | FEA-SF-47 | 04c-features-surfaces | C-RICHVIEW-CORE |
| F-CORE-MARKDOWN | CommonMark and GFM prose | FEA-RP-07 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-MEASURE-WIDE-BLOCKS | No `layout.measure-wide-blocks` key | FEA-BR-18 | 04d-features-branches | C-RICHVIEW-CORE |
| F-CORE-MEDIA | Image block | FEA-RP-26 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-MOTION | What moves in a report | FEA-RP-35 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-PDF-OPTIONS | Media, embeds and links on paper | FEA-RP-45 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-PRINT | Charts in print and PDF | FEA-CH-67 | 04a-features-chart | C-RICHVIEW-CORE |
| F-CORE-REPORT-CARD | Render report card inside the report | FEA-RP-31 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-SOURCE-NOTES | Source notes under blocks | FEA-RP-09 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-STATIC-CHART-FRAMES | Static frames at the widths figures occupy | FEA-CH-64 | 04a-features-chart | C-RICHVIEW-CORE |
| F-CORE-STYLE-CASCADE | Styles as the author meets them | FEA-RP-32 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-TABLE | Table block rendering | FEA-RP-19 | 04b-features-report | C-RICHVIEW-CORE |
| F-CORE-TABS | Tabs without the runtime, in print and in export | FEA-BR-09 | 04d-features-branches | C-RICHVIEW-CORE |
| F-DEVHARNESS-RUNTIME-DRILL | #68 development harness: what the rewrite carries | FEA-BR-20 | 04d-features-branches | C-DEV-HARNESS |
| F-DOCS-CHART-GALLERY | Docs chart gallery | FEA-SF-43 | 04c-features-surfaces | C-DOCS-SITE |
| F-DOCS-CHART-TYPE-PAGES | One focused docs page per chart type | FEA-BR-22 | 04d-features-branches | C-DOCS-SITE |
| F-DOCS-GUIDES | Documentation guides | FEA-SF-41 | 04c-features-surfaces | C-DOCS-SITE |
| F-DOCS-TUTORIAL-EXAMPLE | Downloadable first report | FEA-SF-42 | 04c-features-surfaces | C-DOCS-SITE |
| F-EDITOR-TABS | Tabs in the editor and playground | FEA-BR-38 | 04d-features-branches | C-RICHVIEW-EDITOR |
| F-EMBED-LOADING | When an embed loads | FEA-RP-56 | 04b-features-report | C-RICHVIEW-WEB |
| F-EXPORT-CAROUSEL | LinkedIn carousel PDF | FEA-RP-46 | 04b-features-report | C-APP-PDF-EXPORT |
| F-EXPORT-PDF | PDF export | FEA-RP-43 | 04b-features-report | C-APP-PDF-EXPORT |
| F-FOOTNOTES | Footnotes | FEA-RP-10 | 04b-features-report | C-RICHVIEW-CORE |
| F-FRONTMATTER-FAILURE | A draft whose frontmatter cannot be read | FEA-RP-58 | 04b-features-report | C-RICHVIEW-CORE |
| F-GALLERY-TYPOGRAPHY | Typography specimen as a style fixture | FEA-BR-13 | 04d-features-branches | C-GALLERIES |
| F-HEADER-DECK | One deck under the title | FEA-RP-04 | 04b-features-report | C-RICHVIEW-CORE |
| F-HEADINGS | Headings, outline and section anchors | FEA-RP-18 | 04b-features-report | C-RICHVIEW-CORE |
| F-HERO-IMAGERY | Header imagery: choice, crop and credit | FEA-RP-52 | 04b-features-report | C-RICHVIEW-CORE |
| F-LINK-PREVIEW-META | Link-preview and search tags | FEA-RP-51 | 04b-features-report | C-RICHVIEW-CORE |
| F-LINKS | Links | FEA-RP-15 | 04b-features-report | C-RICHVIEW-CORE |
| F-MARKDOWN-IMAGES | Images in Markdown | FEA-RP-16 | 04b-features-report | C-RICHVIEW-CORE |
| F-MATH | TeX math as MathML | FEA-RP-11 | 04b-features-report | C-RICHVIEW-CORE |
| F-MEDIA-CAPTIONS | Captions and transcripts for audio and video | FEA-RP-55 | 04b-features-report | C-RICHVIEW-CORE |
| F-MEDIA-PLAYERS | Audio and video | FEA-RP-27 | 04b-features-report | C-RICHVIEW-CORE |
| F-METRIC-ROW | Consecutive metrics | FEA-RP-24 | 04b-features-report | C-RICHVIEW-CORE |
| F-NEAR-MISS-TAG | Misspelled rich fence tags | FEA-RP-57 | 04b-features-report | C-RICHVIEW-CORE |
| F-NESTED-BLOCKS | Rich blocks inside callouts, lists and quotes | FEA-RP-53 | 04b-features-report | C-RICHVIEW-CORE |
| F-NOT-RENDERED | What a failed block looks like in a draft | FEA-RP-29 | 04b-features-report | C-RICHVIEW-CORE |
| F-OUTPUT-MARKERS | Per-output markers | FEA-RP-47 | 04b-features-report | C-RICHVIEW-CORE |
| F-PARTIAL-RENDER | A missing input fails only its element | FEA-RP-30 | 04b-features-report | C-APP-RESOLVER |
| F-PDF-ERRORS | PDF of a report with errors | FEA-RP-44 | 04b-features-report | C-APP-PDF-EXPORT |
| F-PKG-CORE-API | Generated npm packages | FEA-SF-49 | 04c-features-surfaces | C-PKG-CORE |
| F-PROLOG | Frontmatter `prolog` | FEA-RP-05 | 04b-features-report | C-RICHVIEW-CORE |
| F-PUBLISH-ACCESS | Identity, roles and audiences (Publish) | FEA-BR-25 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-DASHBOARD-SNAPSHOT | Dashboard snapshot pages (Publish) | FEA-BR-30 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-EDITORIAL-DESK | Browser editorial desk (Publish) | FEA-BR-27 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-FEEDS-SEARCH | Public-only feeds, sitemap and search (Publish) | FEA-BR-24 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-GIT-LOOP | Git as the editorial system (Publish) | FEA-BR-26 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-LAYOUTS-THEMES | Publication layouts and themes (Publish) | FEA-BR-23 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-LIMITS | Publication size limits (Publish) | FEA-BR-49 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-MODE | External and internal publications (Publish) | FEA-BR-45 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-NEW-ADDRESS | Confirmation before a new report address | FEA-SF-51 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-PUBLISH-RELEASES | Immutable releases and rollback (Publish) | FEA-BR-29 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-STABLE-LINKS | Stable links, renames and corrections (Publish) | FEA-BR-44 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISH-STATUS | Release status endpoint (Publish) | FEA-BR-46 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-PUBLISHED-LINKED-FILES | Links to report-folder files once published | FEA-SF-52 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-PULL-QUOTE | Pull quotes `> [!quote] Author` | FEA-RP-13 | 04b-features-report | C-RICHVIEW-CORE |
| F-RAW-HTML | Raw HTML and comments in Markdown | FEA-RP-14 | 04b-features-report | C-RICHVIEW-CORE |
| F-READABLE-WITHOUT-SCRIPTS | Readable before and without scripts | FEA-RP-39 | 04b-features-report | C-RICHVIEW-CORE |
| F-REDUCED-MOTION | Reduced-motion floor | FEA-RP-36 | 04b-features-report | C-RICHVIEW-CORE |
| F-REPORT-HEADER | Report header | FEA-RP-03 | 04b-features-report | C-RICHVIEW-CORE |
| F-REPORT-LOCALE | Language of the rendered report | FEA-RP-50 | 04b-features-report | C-RICHVIEW-CORE |
| F-REPORT-MENU | Report menu for the host owner | FEA-SF-18 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| F-REPORT-OWNER-ACTIONS | Owner actions on stored reports | FEA-SF-53 | 04c-features-surfaces | C-APP-CLI |
| F-REPORT-PRINT | Printing from the browser | FEA-RP-34 | 04b-features-report | C-RICHVIEW-CORE |
| F-REPORT-SLUG-STABILITY | A report keeps its address when its title changes | FEA-SF-13 | 04c-features-surfaces | C-RICHVIEW-CORE |
| F-REPORT-STORE-BEHAVIOUR | What the report host stores and keeps | FEA-SF-54 | 04c-features-surfaces | C-APP-REPORT-STORE |
| F-REPORT-TABS | Multi-tab reports | FEA-BR-08 | 04d-features-branches | C-RICHVIEW-CORE |
| F-REPRODUCIBLE-RENDER | The source stays canonical; renders are reproducible | FEA-RP-01 | 04b-features-report | C-RICHVIEW-CORE |
| F-SCROLLY-STORY | Scrollytelling element | FEA-RP-48 | 04b-features-report | C-RICHVIEW-CORE |
| F-SELF-CONTAINED-HTML | One self-contained file that works offline | FEA-RP-38 | 04b-features-report | C-RICHVIEW-CORE |
| F-SITE-CRAWLERS | Crawler and AI-use policy | FEA-SF-61 | 04c-features-surfaces | C-CLOUDFLARE-TEASER |
| F-SITE-DEMO-DIAGNOSTICS | “Inspect the example's chart diagnostics” on the home page | FEA-SF-57 | 04c-features-surfaces | C-WEB-LANDING |
| F-SITE-DEMO-STYLE-PAGES | Per-style demo pages linked from the home page | FEA-SF-58 | 04c-features-surfaces | C-WEB-LANDING |
| F-SITE-HEADERS | Security headers on richview.ai | FEA-SF-60 | 04c-features-surfaces | C-CLOUDFLARE-TEASER |
| F-SITE-HOME-SECTIONS | Home-page sections and their order | FEA-SF-56 | 04c-features-surfaces | C-WEB-LANDING |
| F-SITE-HOMEPAGE | Home-page statements that do not match the product | FEA-SF-30 | 04c-features-surfaces | C-WEB-LANDING |
| F-SITE-ROUTES | Public routes on richview.ai | FEA-SF-46 | 04c-features-surfaces | C-CLOUDFLARE-TEASER |
| F-SITE-SHARE-IMAGES | Share images and link previews | FEA-SF-55 | 04c-features-surfaces | C-RICHVIEW-WEB |
| F-SKILL-AGENT-GUIDE | Agent skill and `llms.txt` | FEA-SF-48 | 04c-features-surfaces | C-SKILL |
| F-SMART-PUNCTUATION | Smart punctuation | FEA-RP-08 | 04b-features-report | C-RICHVIEW-CORE |
| F-TABLE-CELL-TEXT | Markdown in table-block cells | FEA-RP-54 | 04b-features-report | C-RICHVIEW-CORE |
| F-TABLE-TITLE | Table title and accessible name | FEA-RP-20 | 04b-features-report | C-RICHVIEW-CORE |
| F-TABLE-TOTALS | Table totals | FEA-RP-21 | 04b-features-report | C-RICHVIEW-CORE |
| F-TABS-HIDDEN-LINKS | Links into hidden tab panels | FEA-BR-40 | 04d-features-branches | C-RICHVIEW-WEB |
| F-TABS-STYLE-COHERENCE | One style across every tab | FEA-BR-39 | 04d-features-branches | C-RICHVIEW-WEB |
| F-THEME-OPTION | `theme: light\|dark` and `--theme` | FEA-RP-33 | 04b-features-report | C-RICHVIEW-CORE |
| F-UNSPLASH-IMAGES | Unsplash photos with attribution | FEA-RP-62 | 04b-features-report | C-RICHVIEW-CORE |
| F-URGENT-HIDE | Urgent hide and restore (Publish) | FEA-BR-28 | 04d-features-branches | C-RICHVIEW-PUBLISH |
| F-VOICE-DASHBOARD | Voice-to-dashboard demo: the reference record | FEA-BR-11 | 04d-features-branches | C-VOICE-DASHBOARD |
| F-VSCODE-EXPORT-VALIDATE | Export and Validate commands | FEA-SF-27 | 04c-features-surfaces | C-VSCODE-EXTENSION |
| F-VSCODE-GRAMMAR | Syntax highlighting in VS Code | FEA-SF-59 | 04c-features-surfaces | C-VSCODE-EXTENSION |
| F-VSCODE-INSERT-BLOCKS | Snippets and block insertion | FEA-SF-26 | 04c-features-surfaces | C-VSCODE-EXTENSION |
| F-VSCODE-LANGUAGE | Language id and file association | FEA-SF-22 | 04c-features-surfaces | C-VSCODE-EXTENSION |
| F-VSCODE-LINT | Live diagnostics in the editor | FEA-SF-24 | 04c-features-surfaces | C-RICHVIEW-EDITOR |
| F-VSCODE-PREVIEW | Live preview beside the source | FEA-SF-23 | 04c-features-surfaces | C-VSCODE-EXTENSION |
| F-VSCODE-PROVIDERS | Completion, formatting, outline, folding and hover | FEA-SF-25 | 04c-features-surfaces | C-RICHVIEW-EDITOR |
| F-WEB-CHART-CSV-DOWNLOAD | CSV download | FEA-CH-48 | 04a-features-chart | C-WEB-CHART-HOST |
| F-WEB-CHART-INSPECTION | Keyboard inspection | FEA-CH-56 | 04a-features-chart | C-WEB-CHART-HOST |
| F-WEB-CHART-LEGEND | Legend | FEA-CH-33 | 04a-features-chart | C-WEB-CHART-HOST |
| F-WEB-CHART-OVERVIEW | Overview strip after zoom | FEA-CH-50 | 04a-features-chart | C-WEB-CHART-HOST |
| F-WEB-CHART-SVG-DOWNLOAD | SVG download | FEA-CH-46 | 04a-features-chart | C-WEB-CHART-HOST |
| F-WEB-CHART-TABLE | Table view | FEA-CH-45 | 04a-features-chart | C-WEB-CHART-HOST |
| F-WEB-CHART-ZOOM-PAN | Zoom and pan | FEA-CH-49 | 04a-features-chart | C-WEB-CHART-HOST |
| F-WEB-CHARTS-EXAMPLES | Chart examples page (`/charts/`) | FEA-SF-44 | 04c-features-surfaces | C-WEB-CHART-EXAMPLES |
| F-WEB-CONTROL-EVENTS | Control state events | FEA-BR-07 | 04d-features-branches | C-WEB-WASM-API |
| F-WEB-LANDING-LIVE-DEMO | Live two-file demo on the home page | FEA-SF-29 | 04c-features-surfaces | C-WEB-LANDING |
| F-WEB-LANDING-MOTION-RESTORED | Landing scroll motion: the bar it must clear | FEA-BR-21 | 04d-features-branches | C-WEB-LANDING |
| F-WEB-LEGAL-PAGES | Impressum and privacy pages | FEA-SF-32 | 04c-features-surfaces | C-RICHVIEW-WEB |
| F-WEB-PLAYGROUND-LIVE-RENDER | Playground features | FEA-SF-34 | 04c-features-surfaces | C-WEB-PLAYGROUND |
| F-WEB-PLAYGROUND-SHARE | Share a playground draft by link | FEA-SF-35 | 04c-features-surfaces | C-WEB-PLAYGROUND |
| F-WEB-SETTLE-HOOK | Settled state and `window.__richview` | FEA-RP-37 | 04b-features-report | C-WEB-WASM-API |
| F-WEB-SHOWCASE-CHART-REVIEW | Chart review pages | FEA-SF-40 | 04c-features-surfaces | C-BENCH-LABS |
| F-WEB-SHOWCASE-CITATION | Citation cards on flagship sources | FEA-SF-38 | 04c-features-surfaces | C-WEB-SHOWCASE |
| F-WEB-SHOWCASE-FLAGSHIP | Flagship report page | FEA-SF-36 | 04c-features-surfaces | C-WEB-SHOWCASE |
| F-WEB-SHOWCASE-INSPECT | “Inspect chart” lightbox | FEA-SF-39 | 04c-features-surfaces | C-WEB-SHOWCASE |
| F-WEB-SHOWCASE-SHELL | Flagship page copy and content fingerprint | FEA-SF-63 | 04c-features-surfaces | C-WEB-SHOWCASE |
| F-WEB-SHOWCASE-STYLE-PICKER | Showcase style pills: decided elsewhere | FEA-BR-17 | 04d-features-branches | C-WEB-SHOWCASE |
| F-WEB-TABLE-SORT | Sorting a table | FEA-RP-22 | 04b-features-report | C-RICHVIEW-WEB |
| F-WEB-TABS-NAV | Tab navigation, deep links and hidden charts | FEA-BR-10 | 04d-features-branches | C-RICHVIEW-WEB |
| F-WIKILINKS | Wikilinks `[[Report]]` | FEA-RP-12 | 04b-features-report | C-RICHVIEW-CORE |

## T- Types (65)

| ID | Title | Item | Part |
| --- | --- | --- | --- |
| T-BLOCK-AUDIO | Audio block | TYP-30 | 05-types |
| T-BLOCK-CALLOUT | Callout block | TYP-28 | 05-types |
| T-BLOCK-CHART | Chart block fields | TYP-12 | 05-types |
| T-BLOCK-EMBED | Embed block | TYP-32 | 05-types |
| T-BLOCK-IMAGE | Image block | TYP-29 | 05-types |
| T-BLOCK-METRIC | Metric block | TYP-26 | 05-types |
| T-BLOCK-TABLE | Table block | TYP-24 | 05-types |
| T-BLOCK-VIDEO | Video block | TYP-31 | 05-types |
| T-CHART-ANNOTATION | Annotations | TYP-19 | 05-types |
| T-CHART-AXES | Axis fields | TYP-16 | 05-types |
| T-CHART-BINNING | Histogram binning | TYP-20 | 05-types |
| T-CHART-CHECK | Chart check record | TYP-58 | 05-types |
| T-CHART-CONTROLS | Chart controls (PR #62) | TYP-77 | 05-types |
| T-CHART-DIAGNOSTICS | Per-chart diagnostics object | TYP-57 | 05-types |
| T-CHART-ENCODING | Chart types and their channels | TYP-14 | 05-types |
| T-CHART-HIGHLIGHTS | Label highlights (PR #67) | TYP-78 | 05-types |
| T-CHART-INTERACTION | Reader interactions are not authored | TYP-21 | 05-types |
| T-CHART-RUNTIME-PAYLOAD | Embedded chart runtime payload | TYP-74 | 05-types |
| T-CHART-STYLE | Chart-local style fields | TYP-17 | 05-types |
| T-CLI-JSON | CLI JSON output | TYP-68 | 05-types |
| T-CSS-CUSTOM-PROPERTIES | Style-to-CSS custom properties | TYP-99 | 05-types |
| T-DATA-CONNECTOR | Data connectors (PR #66) | TYP-79 | 05-types |
| T-DATA-CSV | CSV typing rules | TYP-37 | 05-types |
| T-DATA-JSON | JSON data files | TYP-38 | 05-types |
| T-DATA-PARQUET | Parquet data files | TYP-102 | 05-types |
| T-DATA-SOURCE | Block data source | TYP-35 | 05-types |
| T-DIAG-CODES | Diagnostic code catalogue | TYP-55 | 05-types |
| T-DIAG-STATUS | Levels, status, `ok` and exit codes | TYP-54 | 05-types |
| T-DIAGNOSTIC | Diagnostic record | TYP-53 | 05-types |
| T-DROPPED-LABEL | Dropped-label record | TYP-59 | 05-types |
| T-EDITOR-PROTOCOL | Editor service methods | TYP-98 | 05-types |
| T-ENGINE-CHARTSPEC | The engine's JSON spec stays internal | TYP-22 | 05-types |
| T-FRONTMATTER | Frontmatter fields | TYP-06 | 05-types |
| T-HTTP-API | Server HTTP routes | TYP-70 | 05-types |
| T-JS-API | JavaScript facade | TYP-75 | 05-types |
| T-MARKDOWN-EXT | Markdown extensions | TYP-34 | 05-types |
| T-MCP-TOOLS | MCP tool contracts | TYP-69 | 05-types |
| T-OPERATION-SCOPE | Which checks each operation runs | TYP-94 | 05-types |
| T-PUBLISH-META | Publication metadata (PR #70) | TYP-80 | 05-types |
| T-PUBLISH-REQUEST | Publish request | TYP-66 | 05-types |
| T-RENDER-METADATA | Render metadata | TYP-73 | 05-types |
| T-RENDER-OPTIONS | Render options | TYP-71 | 05-types |
| T-RENDERED-OUTPUT | Rendered output | TYP-42 | 05-types |
| T-REPORT-FOLDER | Report folder and path rules | TYP-39 | 05-types |
| T-REPORT-LANG | Report language | TYP-08 | 05-types |
| T-REPORT-RUNTIME-API | Report runtime API, events and readiness | TYP-97 | 05-types |
| T-REPORT-SLUG | Slug grammar and derivation | TYP-60 | 05-types |
| T-REPORT-VERSION | Version grammar | TYP-62 | 05-types |
| T-RESOURCE-LIMITS | Size and count limits | TYP-43 | 05-types |
| T-RICH-FENCE | Rich block tags | TYP-09 | 05-types |
| T-RMD-DOCUMENT | The `.rmd` document | TYP-01 | 05-types |
| T-SITE-CONFIG | Publication `site.yaml` (PR #70) | TYP-81 | 05-types |
| T-STORED-REPORT | Stored report record | TYP-65 | 05-types |
| T-STYLE-BUILTINS | Builtin style names | TYP-52 | 05-types |
| T-STYLE-CHART | Style chart section | TYP-50 | 05-types |
| T-STYLE-COMPONENTS | Style components section | TYP-48 | 05-types |
| T-STYLE-FILE | Style file structure and merge | TYP-44 | 05-types |
| T-STYLE-LAYOUT | Style layout section | TYP-47 | 05-types |
| T-STYLE-MOTION | Style motion section | TYP-49 | 05-types |
| T-STYLE-REF | Style references | TYP-45 | 05-types |
| T-STYLE-TOKENS | Style tokens and value grammars | TYP-46 | 05-types |
| T-TABS | Tabs (PR #63) | TYP-76 | 05-types |
| T-TEMPORAL-VALUE | Date and time values | TYP-90 | 05-types |
| T-VALUE-FORMAT | Value formats | TYP-23 | 05-types |
| T-YAML-DIALECT | YAML profile | TYP-02 | 05-types |

## S- Styles (67)

| ID | Title | Item | Part | Component |
| --- | --- | --- | --- | --- |
| S-ACTION-LOOK | How chart actions look | STY-42 | 06-styles |  |
| S-ANTI-GENERIC | RichView's design rules | STY-50 | 06-styles |  |
| S-AUTHOR-COLOURS | Colours authors bring | STY-53 | 06-styles |  |
| S-BIG | Metric styles and Brutalist's knockout | STY-33 | 06-styles |  |
| S-BRAND-FONTS | Authors' own fonts | STY-16 | 06-styles |  |
| S-BRUTALIST | Brutalist | STY-12 | 06-styles |  |
| S-CASCADE | Every cascade starts from a builtin | STY-17 | 06-styles |  |
| S-CHART-CHROME | Gridlines, axes and fills | STY-40 | 06-styles |  |
| S-CHART-CONTROLS | How chart controls look | FEA-BR-34 | 04d-features-branches | C-WEB-CHART-HOST |
| S-CHART-KEYS | Component and chart keys | STY-67 | 06-styles |  |
| S-CHART-TEXT | Chart typography | STY-39 | 06-styles |  |
| S-COLOPHON-SETTING | Colophon setting and look | STY-04 | 06-styles |  |
| S-COLOR-SCHEME | One colour scheme per style | STY-20 | 06-styles |  |
| S-CORNERS | Corners | STY-36 | 06-styles |  |
| S-ELEMENT-DECK | The five-element deck on the home page | STY-58 | 06-styles |  |
| S-FLAGSHIP-FRAME | The flagship page's frame | STY-60 | 06-styles |  |
| S-FONT-CATALOG | Faces per style | STY-27 | 06-styles |  |
| S-FONT-LOADING | How fonts load | STY-29 | 06-styles |  |
| S-FONT-NAMING | One font build for reports and site | STY-28 | 06-styles |  |
| S-GLYPH-COVERAGE | Characters outside the bundled subset | STY-63 | 06-styles |  |
| S-HAND-DRAWN | Hand-drawn | STY-13 | 06-styles |  |
| S-HAND-DRAWN-ACCENT | Hand-drawn's accent | STY-55 | 06-styles |  |
| S-HEADER-IMAGE | Header images and illustrations | STY-62 | 06-styles |  |
| S-HOVER-FOCUS | Hover and focus change colour, not position | STY-38 | 06-styles |  |
| S-IDENTITY | One brand, five report voices | STY-01 | 06-styles |  |
| S-IMPECCABLE | Impeccable as a review aid | STY-64 | 06-styles |  |
| S-MARGIN-COLUMN | No margin column | STY-32 | 06-styles |  |
| S-MARK | The mark and the wordmark | STY-03 | 06-styles |  |
| S-OKLCH | OKLCH everywhere | STY-19 | 06-styles |  |
| S-PALETTE-CHECKS | The palette rule | STY-22 | 06-styles |  |
| S-PALETTES | Builtin palettes and ramps | STY-23 | 06-styles |  |
| S-PAPER-GRAIN | Paper grain on the site | STY-65 | 06-styles |  |
| S-PENCIL-MARKS | Pencil strikes, circles and arrows | STY-35 | 06-styles |  |
| S-PRINT | Paper is always light | STY-21 | 06-styles |  |
| S-RAMPS-DIVERGING | Negative values in heatmaps | STY-56 | 06-styles |  |
| S-REPORT-WIDTH | Report column and vertical rhythm | STY-31 | 06-styles |  |
| S-REVIEW-QUESTIONS | Three questions for every visual change | STY-51 | 06-styles |  |
| S-RULES | Rules mark structure | STY-34 | 06-styles |  |
| S-SCHEME-FLAG | Each builtin names its scheme | STY-57 | 06-styles |  |
| S-SCIENTIFIC | Scientific | STY-11 | 06-styles |  |
| S-SEMANTIC-COLOURS | Status colours | STY-24 | 06-styles |  |
| S-SETTINGS-MATRIX | Safe style settings are tested; experimental ones are labelled | STY-69 | 06-styles |  |
| S-SETTINGS-VALUES | The settings and their choices | STY-15 | 06-styles |  |
| S-SHADOWS | Shadows only on floating elements | STY-37 | 06-styles |  |
| S-SITE-CODE | Code highlighting on the site | STY-59 | 06-styles |  |
| S-SITE-CORNERS | Corners on the site | STY-66 | 06-styles |  |
| S-SITE-LOOK | One look for the whole site | STY-46 | 06-styles |  |
| S-SITE-MOTION | Other motion on the site | STY-45 | 06-styles |  |
| S-SITE-SCHEME | The site is light only | STY-47 | 06-styles |  |
| S-SITE-SECTIONS | Site sections lead with headings | STY-49 | 06-styles |  |
| S-SITE-SURFACES | Opaque site surfaces, no glass | STY-48 | 06-styles |  |
| S-SLEEK-DARK | Sleek | STY-09 | 06-styles |  |
| S-SLEEK-LIGHT | Air | STY-10 | 06-styles |  |
| S-STORY-THUMBNAIL | Link-preview thumbnails follow the story's style | STY-70 | 06-styles |  |
| S-STYLE-CHECKS | Measured style checks | STY-52 | 06-styles |  |
| S-STYLE-NAMES | One name per style | STY-07 | 06-styles |  |
| S-STYLE-REVIEW | What you see before accepting a style | STY-68 | 06-styles |  |
| S-STYLE-SELECTOR | How a style choice looks | STY-08 | 06-styles |  |
| S-STYLE-SETTINGS | Settings, not free tokens | STY-14 | 06-styles |  |
| S-TAB-ROW | How the tab row looks | FEA-BR-35 | 04d-features-branches | C-RICHVIEW-CORE |
| S-TEXT-CONTRAST | Contrast floors for every ink | STY-25 | 06-styles |  |
| S-TEXT-EXTRAS | Quotes, code, notes and table extras | STY-61 | 06-styles |  |
| S-THEME-TRANSITION | Switching style on a page | STY-43 | 06-styles |  |
| S-TOKEN-USES | Where accent and status colours appear | STY-54 | 06-styles |  |
| S-TOKENS | The token set every builtin defines | STY-18 | 06-styles |  |
| S-TOOLTIP | Readout panel values | STY-41 | 06-styles |  |
| S-TYPE-SCALE | One type scale per style, phones included | STY-26 | 06-styles |  |

## P- Performance (50)

| ID | Title | Item | Part | Component |
| --- | --- | --- | --- | --- |
| P-BROWSER-MEMORY | Browser memory per page | PRF-43 | 07-performance |  |
| P-BUILD-TIME | Optimized build time | PRF-50 | 07-performance |  |
| P-CHART-FRAME | Script per animated frame | PRF-21 | 07-performance |  |
| P-CHART-KERNEL | Chart kernel | PRF-19 | 07-performance |  |
| P-CLI-COLD | Process-cold CLI commands | PRF-11 | 07-performance |  |
| P-COLD-MOUNT | Cold gallery mount | PRF-33 | 07-performance |  |
| P-CONTROL-CHANGE | Response to a control change | FEA-BR-36 | 04d-features-branches | C-WEB-CHART-HOST |
| P-DATA-SCALE | Ten years of quarter-hours in one chart | PRF-62 | 07-performance |  |
| P-EDIT | Small edit to live chart | PRF-23 | 07-performance |  |
| P-EDIT-INSTANT | Edits live within 100 ms | PRF-61 | 07-performance |  |
| P-EDIT-LARGE | Large-source edit and main-thread blocking | PRF-29 | 07-performance |  |
| P-EDITOR-MEMORY | Editor memory over a session | PRF-41 | 07-performance |  |
| P-EDITOR-RPC | Editor preview call | PRF-13 | 07-performance |  |
| P-ENGINES | Browser engines with timing budgets | PRF-24 | 07-performance |  |
| P-EVIDENCE-INTEGRITY | Evidence integrity, sample sizes and harnesses | PRF-06 | 07-performance |  |
| P-FIRST-PAINT | First paint on a phone | PRF-34 | 07-performance |  |
| P-FIXTURES | Benchmark fixtures and device profiles | PRF-07 | 07-performance |  |
| P-FRAME-PACING | Frame pacing in motion and scrolling | PRF-27 | 07-performance |  |
| P-GATE-SCOPE | Which budgets run in the fast gate, the release run and the full run | PRF-04 | 07-performance |  |
| P-HTML-SIZE | Report HTML size | PRF-32 | 07-performance |  |
| P-HTTP-READ | Serving a published report | PRF-15 | 07-performance |  |
| P-HTTP-RENDER | HTTP render API | PRF-14 | 07-performance |  |
| P-HYDRATION-MATCH | Static frame matches the hydrated chart | PRF-38 | 07-performance |  |
| P-INSTALL-SIZE | Install and package size | PRF-39 | 07-performance |  |
| P-INTERACTION | Hover, toggle, table and chart actions | PRF-25 | 07-performance |  |
| P-LAYOUT-SHIFT | Layout shift through every chart mount | PRF-35 | 07-performance |  |
| P-LOAD-FRAMES | Long frames while a page loads | PRF-26 | 07-performance |  |
| P-MCP-MEMORY | MCP server memory over a session | PRF-44 | 07-performance |  |
| P-MCP-TOOLS | MCP tool latency | PRF-12 | 07-performance |  |
| P-MOBILE-FIRST-VISIT | Interactive on a phone in 3 seconds | PRF-60 | 07-performance |  |
| P-NATIVE-MEMORY | CLI render memory | PRF-40 | 07-performance |  |
| P-NATIVE-RENDER | Warm native render | PRF-10 | 07-performance |  |
| P-NODE-BOUNDARY | Node package boundary | PRF-17 | 07-performance |  |
| P-OS-SCOPE | Memory and timing measures on macOS | PRF-03 | 07-performance |  |
| P-PDF-EXPORT | PDF export | PRF-16 | 07-performance |  |
| P-PREVIEW-LIVE | Edit to preview outside the playground | PRF-28 | 07-performance |  |
| P-PUBLISH-SCALE | Publish build at site scale | PRF-18 | 07-performance |  |
| P-QUIET-CPU | Timing captures on a busy host | PRF-02 | 07-performance |  |
| P-REAL-DEVICE | Real-phone check | PRF-09 | 07-performance |  |
| P-REFERENCE-HOST | Reference host for every budget | PRF-01 | 07-performance |  |
| P-REGRESSION-GATE | Regression gate against an accepted baseline | PRF-05 | 07-performance |  |
| P-REWRITE-BASELINE | First baseline and the rewrite period | PRF-08 | 07-performance |  |
| P-RUNTIME-DELIVERY | Report runtime size and delivery | PRF-31 | 07-performance |  |
| P-SERVER-MEMORY | Server memory under publishes and reads | PRF-42 | 07-performance |  |
| P-SITE-DELIVERY | Site caching, compression and bytes per view | PRF-37 | 07-performance |  |
| P-SITE-FIRST-VISIT | Landing and playground first visit | PRF-36 | 07-performance |  |
| P-TEST-TIERS | Verification tier durations | PRF-51 | 07-performance |  |
| P-THEME-SWITCH | Theme and style switch | PRF-22 | 07-performance |  |
| P-WASM-GZIP | Site runtime payload | PRF-30 | 07-performance |  |
| P-ZOOM | Dense zoom response | PRF-20 | 07-performance |  |

## G- Gates and rules (101)

| ID | Title | Item | Part | Component |
| --- | --- | --- | --- | --- |
| G-A11Y | Accessibility audit | DEL-58 | 02-delivery |  |
| G-AGENT-DOCS-CURRENT | Agent-facing docs derive from the code | INT-66 | 01-intent |  |
| G-AGENT-FLOW | No in-repo multi-agent flow | DEL-67 | 02-delivery |  |
| G-AGENT-INSTRUCTIONS | Agent instruction files | DEL-46 | 02-delivery |  |
| G-ANSWERS-PATH | How your answers reach dev | DEL-66 | 02-delivery |  |
| G-ARCH-BOUNDARIES | Architecture boundary checks | DEL-54 | 02-delivery |  |
| G-AS-DECIDED | Build what was decided, nothing more | INT-62 | 01-intent |  |
| G-AUDIT | Merge and deploy audit | DEL-38 | 02-delivery |  |
| G-AUTHORIZATION | What agents may do without asking | DEL-05 | 02-delivery |  |
| G-BASELINE | Delivery baseline | DEL-01 | 02-delivery |  |
| G-BATCH | Batches and the frozen prod candidate | DEL-63 | 02-delivery |  |
| G-BRANCHES | Branch names and the default branch | DEL-02 | 02-delivery |  |
| G-BROWSER-CHROMIUM | Chromium behaviour suites | DEL-15 | 02-delivery |  |
| G-BROWSER-CONTROLS-TABS | Browser suites for controls and tabs | FEA-BR-37 | 04d-features-branches | C-VERIFY-HARNESS |
| G-BROWSER-MATRIX | Chrome, Firefox and Safari for WebGL charts | DEL-74 | 02-delivery |  |
| G-BUILD | Release build and fixtures | DEL-13 | 02-delivery |  |
| G-CHANGE-LOG | The dev change log and the autonomous loop's trail | DEL-70 | 02-delivery |  |
| G-CHANGE-REPORT | The before-and-after report you review | DEL-69 | 02-delivery |  |
| G-CHART-A11Y-CHECK | Chart accessibility checks with WebGL | FEA-CH-77 | 04a-features-chart |  |
| G-CHART-TYPE-CONTRACT | What every chart type ships with | FEA-CH-75 | 04a-features-chart |  |
| G-CHROME-RECEDES | Controls and chrome recede | INT-59 | 01-intent |  |
| G-CLIPPY | Native lint | DEL-10 | 02-delivery |  |
| G-CLIPPY-WASM | Wasm lint | DEL-11 | 02-delivery |  |
| G-CONNECTOR-TESTS | Connector checks for the live-data path | FEA-BR-50 | 04d-features-branches |  |
| G-CONTROL-STATES | Every reachable state is checked | FEA-BR-33 | 04d-features-branches | C-RICHVIEW-CHART |
| G-CROSS-STYLE-VALUES | Cross-style value check | DEL-52 | 02-delivery |  |
| G-DATA-INTEGRITY | Data integrity | DEL-16 | 02-delivery |  |
| G-DECISION-CLASS | What counts as a design or feature decision | DEL-32 | 02-delivery |  |
| G-DEPLOY-OWNER | What deploys, from where, on whose nod | DEL-40 | 02-delivery |  |
| G-DESIGN-REVIEW | Review agents advise; they do not gate | DEL-35 | 02-delivery |  |
| G-DEV-GATE | The gate commands: fast, release and full | DEL-08 | 02-delivery |  |
| G-DEV-PROD | Merging into dev | DEL-03 | 02-delivery |  |
| G-DOCS-MATCH-CODE | Docs match the code | DEL-18 | 02-delivery |  |
| G-ENFORCEMENT | Enforcement without GitHub branch protection | DEL-37 | 02-delivery |  |
| G-EVIDENCE-STORE | Where receipts and captures live | DEL-61 | 02-delivery |  |
| G-FAST-DEV | Fast parallel development: dev, pre-prod and a release agent | DEL-81 | 02-delivery |  |
| G-FMT | Formatting | DEL-09 | 02-delivery |  |
| G-FONTS-LOAD | Fonts load or fail loudly | INT-57 | 01-intent |  |
| G-FRESH-AGENT-RUN | The fresh-agent run: harness and schedule | DEL-78 | 02-delivery |  |
| G-GATE-HOST | Gate host | DEL-22 | 02-delivery |  |
| G-GATE-RECEIPT | Gate receipts | DEL-20 | 02-delivery |  |
| G-GEOMETRY-ORACLE | The geometry model is the chart test oracle | DEL-72 | 02-delivery |  |
| G-GOLDEN-INPUTS | Golden inputs, snippets and docs commands | DEL-60 | 02-delivery |  |
| G-HONEST-CLAIMS | Claims match what ships | INT-65 | 01-intent |  |
| G-HONEST-STATUS | Honest status | DEL-21 | 02-delivery |  |
| G-HOST | Host checks | DEL-14 | 02-delivery |  |
| G-HOST-CAPACITY | Agent concurrency on the gate host | DEL-62 | 02-delivery |  |
| G-HOSTED-CI | Hosted CI | DEL-39 | 02-delivery |  |
| G-HOSTED-SMOKE | Checks for hosted services and previews | DEL-77 | 02-delivery |  |
| G-HOSTILE-INPUT | Hostile-input corpus | DEL-56 | 02-delivery |  |
| G-JOURNEYS | End-to-end journeys on built artifacts | DEL-76 | 02-delivery |  |
| G-LABELS-INTACT | No clipped, colliding or garbled labels | INT-58 | 01-intent |  |
| G-LEGACY-HOTFIX | Legacy hotfixes while dev holds the rewrite | DEL-65 | 02-delivery |  |
| G-LINEAR-TRACKING | Linear tracking | DEL-44 | 02-delivery |  |
| G-LOCKDOWN-HOME | Where the lockdown and the guide live | DEL-45 | 02-delivery |  |
| G-MAP-FLOW-PIVOT | Map, flow and pivot-table checks | FEA-CH-76 | 04a-features-chart |  |
| G-MERGE-QUEUE | Merge order, rebase and conflicts | DEL-43 | 02-delivery |  |
| G-MOTION-PROOF | Motion evidence | DEL-30 | 02-delivery |  |
| G-MOTION-SMOOTH | Motion is smooth or absent | INT-61 | 01-intent |  |
| G-MOTION-TESTS | Motion tests for WebGL animation | DEL-75 | 02-delivery |  |
| G-MUTATION-CHECK | Deliberate-mutation check | DEL-26 | 02-delivery |  |
| G-NO-JS-DRILL | No-JS reading and the runtime drill | DEL-57 | 02-delivery |  |
| G-NO-RELEASES | What the pipeline publishes besides deploys | DEL-41 | 02-delivery |  |
| G-NO-SILENT-NOOP | Missing pieces fail loudly | INT-64 | 01-intent |  |
| G-NONCI | No check outside a tier | DEL-23 | 02-delivery |  |
| G-ONE-ANSWER | One recorded answer per preference | INT-63 | 01-intent |  |
| G-ORACLE-PROTECTION | Changes that weaken the gate | DEL-36 | 02-delivery |  |
| G-PDF-STYLES | PDF in every style, drawn from the same model | FEA-RP-61 | 04b-features-report |  |
| G-PERF-BUDGETS | Performance step | DEL-51 | 02-delivery |  |
| G-PRECISION | Values at the author's precision, and nothing more precise | DEL-80 | 02-delivery |  |
| G-PROD-CANDIDATE | Prod-candidate gate | DEL-33 | 02-delivery |  |
| G-PROD-MERGE | Merging into prod | DEL-04 | 02-delivery |  |
| G-PROD-TESTS | The prod tests behind your go | DEL-71 | 02-delivery |  |
| G-READING-MEASURE | One reading measure | INT-60 | 01-intent |  |
| G-REVIEW-PACKET | The review packet before a prod merge | DEL-34 | 02-delivery |  |
| G-REWRITE-PHASE | Gates and baselines during the rewrite | DEL-64 | 02-delivery |  |
| G-ROLES-MATRIX | Roles, approvals and tenant isolation are tested on every route | FEA-SF-64 | 04c-features-surfaces |  |
| G-ROLLBACK-DEV | Rolling back a bad dev merge | DEL-47 | 02-delivery |  |
| G-ROLLBACK-PROD | Rolling back a bad prod merge | DEL-48 | 02-delivery |  |
| G-SCOPE-GUARD | Work the lockdown does not cover | DEL-06 | 02-delivery |  |
| G-SCREENSHOT-EVIDENCE | Screenshot evidence per release, and where you see it | DEL-31 | 02-delivery |  |
| G-SECRET-SCAN | Secrets and private data | DEL-55 | 02-delivery |  |
| G-SETUP | One-time setup before the rewrite | DEL-07 | 02-delivery |  |
| G-SITE | Site gate | DEL-59 | 02-delivery |  |
| G-STYLE-REACHES-CHARTS | A style reaches everything it renders | INT-56 | 01-intent |  |
| G-SURFACE-SNAPSHOT | Public-surface snapshots | DEL-53 | 02-delivery |  |
| G-TEST-FAST | Fast tier: what moves out and what stays covered | DEL-25 | 02-delivery |  |
| G-TEST-FULL | All Rust tests | DEL-12 | 02-delivery |  |
| G-TEST-TIERS | Test tiers | DEL-24 | 02-delivery |  |
| G-THEME-SWITCH-PROOF | Theme-switch proof | DEL-17 | 02-delivery |  |
| G-TIER-HOST | Lenovo, Claw Mini and the tier rule | DEL-50 | 02-delivery |  |
| G-TOOLCHAIN-PINS | Pinned toolchain | DEL-49 | 02-delivery |  |
| G-TRACEABILITY | Every accepted item names its check | DEL-79 | 02-delivery |  |
| G-TRUST-CONTRACT | What you trust without looking | DEL-68 | 02-delivery |  |
| G-VISUAL-BASELINE | Baselines and who approves visual changes | DEL-29 | 02-delivery |  |
| G-VISUAL-DIFF | Diff method and threshold | DEL-28 | 02-delivery |  |
| G-VISUAL-MATRIX | Visual review matrix | DEL-19 | 02-delivery |  |
| G-VISUAL-SCOPE | What the visual gate captures | DEL-27 | 02-delivery |  |
| G-WEBGL-CAPTURE | Deterministic WebGL captures and per-engine baselines | DEL-73 | 02-delivery |  |
| G-WORKER-REPLY-HOLD | Deterministic ordering in browser checks | FEA-BR-47 | 04d-features-branches | C-DEV-HARNESS |
| G-WORKTREE-ISOLATION | Worktrees and isolation | DEL-42 | 02-delivery |  |

## D- Decisions (246)

| ID | Title | Item | Part | Component |
| --- | --- | --- | --- | --- |
| D-ACCESS-CLAIM | Private reports and access control | FEA-SF-19 | 04c-features-surfaces | C-APP-HTTP-SERVER |
| D-AGENT-CONTRACTS | Agent-facing contracts during the rewrite | INT-81 | 01-intent |  |
| D-ALTERNATIVES | What RichView is chosen over | INT-76 | 01-intent |  |
| D-AMBITION | Which ambitions become public claims | INT-23 | 01-intent |  |
| D-ARTIFACT | The artifact: plain-text source, self-contained output | INT-04 | 01-intent |  |
| D-AUDIENCE | Primary audience: authors who work through a coding agent | INT-06 | 01-intent |  |
| D-AUDIENCE-READERS | Secondary audience: readers of the stories | INT-07 | 01-intent |  |
| D-AXIS-COMPACT | Compact numbers, zero and currency on axes | FEA-CH-22 | 04a-features-chart | C-RICHVIEW-CHART |
| D-AXIS-FORMATS | Years print as years | FEA-CH-21 | 04a-features-chart | C-RICHVIEW-CHART |
| D-AXIS-TITLES | Axis titles only when authored | FEA-CH-20 | 04a-features-chart | C-RICHVIEW-CHART |
| D-BET-AGENT-NATIVE | Agent-native authoring | INT-21 | 01-intent |  |
| D-BET-CRAFT-FROM-YAML | Reference-grade charts from the defaults | INT-20 | 01-intent |  |
| D-BET-GIT-PUBLISHING | Git-native editorial publishing | INT-22 | 01-intent |  |
| D-BET-SELF-CHECKING | Self-checking charts | INT-18 | 01-intent |  |
| D-BET-STYLE-RANGE | One sourced report, many coherent styles | INT-19 | 01-intent |  |
| D-BRANCH-CLEANUP | Delete merged and closed branches after the PR decisions | OPN-29 | 08-open-work |  |
| D-BRAND-ACCENT | Brand blue and style accents | STY-02 | 06-styles |  |
| D-BRAND-NAME | Names: RichView, Rich Markdown, `.rmd` | INT-05 | 01-intent |  |
| D-BROWSER-SOURCES | Browser hosts load report inputs only from their own origin | ARC-66 | 03-architecture |  |
| D-CALENDAR-LABELS | Calendar label text | FEA-CH-25 | 04a-features-chart | C-RICHVIEW-CHART |
| D-CHART-ACTIONS | Chart action row | FEA-CH-43 | 04a-features-chart | C-WEB-CHART-HOST |
| D-CHART-CALENDAR-MERGE | What merging the calendar painter changes | FEA-CH-74 | 04a-features-chart | C-RICHVIEW-CHART |
| D-CHART-CHECK-GAPS | Defects `check` must catch | FEA-CH-60 | 04a-features-chart | C-RICHVIEW-CHART |
| D-CHART-METHOD-NOTES | Method notes under computed charts | FEA-CH-14 | 04a-features-chart | C-RICHVIEW-CHART |
| D-CHART-MODAL | One expanded view | FEA-CH-44 | 04a-features-chart | C-WEB-CHART-HOST |
| D-CHART-MODAL-PORTRAIT | Expanded chart on a phone held upright | FEA-CH-68 | 04a-features-chart | C-WEB-CHART-HOST |
| D-CHART-ORDER | Categories and series follow source order | FEA-CH-09 | 04a-features-chart | C-RICHVIEW-CHART |
| D-CHART-PIPELINE | One public chart contract: Rich Markdown blocks | ARC-19 | 03-architecture |  |
| D-CHART-POLISH-PR67 | PR #67 chart polish: fold Daniel's chart calls, do not finish in legacy | OPN-11 | 08-open-work |  |
| D-CHART-RENDER-PATH | Chart rendering stays inside the engine | ARC-18 | 03-architecture |  |
| D-CHART-SOURCE-UPDATE | Chart state when the source changes | FEA-CH-71 | 04a-features-chart | C-WEB-CHART-HOST |
| D-CHART-SUPPLIER | The chart engine | INT-53 | 01-intent |  |
| D-CHART-TEXT-METRICS | Chart text measured with real font metrics | FEA-CH-30 | 04a-features-chart | C-RICHVIEW-CHART |
| D-CHART-TYPES | Nine chart types | FEA-CH-01 | 04a-features-chart | C-RICHVIEW-CHART |
| D-CONNECTORS-PR66 | PR #66 data connectors: close; its contract informs Publish's connectors | OPN-10 | 08-open-work |  |
| D-CONTROLS-ACTIONS | Table, downloads, Expand and zoom on charts with controls | FEA-BR-32 | 04d-features-branches | C-WEB-CHART-HOST |
| D-CONTROLS-PR62 | PR #62 playable charts: fold into the spec, do not merge | OPN-06 | 08-open-work |  |
| D-CORE-JOBS | Core jobs | INT-08 | 01-intent |  |
| D-CORE-PURE | Core and engine are pure | ARC-02 | 03-architecture |  |
| D-CURVES | Curves: monotone by default, straight and steps on request | FEA-CH-03 | 04a-features-chart | C-RICHVIEW-CHART |
| D-DASHBOARD-PR64 | PR #64 voice dashboard: close and keep as a reference demo | OPN-08 | 08-open-work |  |
| D-DEC-CONTESTED | Sixteen June decisions, dec-29's accent rule and five memo recommendations are decided elsewhere | OPN-39 | 08-open-work |  |
| D-DEC-KEEP | Nineteen June decisions and the D5 recommendation still hold | OPN-36 | 08-open-work |  |
| D-DEC-PROCESS | Two June process decisions are retired | OPN-40 | 08-open-work |  |
| D-DEC-SUPERSEDED | Thirty-nine June decisions and four memo recommendations are superseded | OPN-37 | 08-open-work |  |
| D-DEC-UNBUILT | Seven June decisions were never built | OPN-38 | 08-open-work |  |
| D-DECISION-RECORDS | The lockdown replaces the June decision records | OPN-35 | 08-open-work |  |
| D-DEFAULT-PUBLISH-HOST | richview.uk, the legacy publish default, redirects to richview.ai now | OPN-04 | 08-open-work |  |
| D-DEFAULT-STYLE | The default style | STY-06 | 06-styles |  |
| D-DEPENDENCIES | Third-party dependencies and their licences | ARC-68 | 03-architecture |  |
| D-DEPLOY-TOPOLOGY | Deployment topology | ARC-22 | 03-architecture |  |
| D-DICTATION-OPEN | Unclarified dictation is not a requirement | OPN-42 | 08-open-work |  |
| D-DISTRIBUTION | How people install RichView | INT-71 | 01-intent |  |
| D-DOCS-CHART-EXAMPLES | Sourced examples for the chart-type pages | FEA-BR-41 | 04d-features-branches | C-GALLERIES |
| D-DOMAIN | Canonical domain | INT-46 | 01-intent |  |
| D-DONE | When the rewrite is done | INT-67 | 01-intent |  |
| D-DONE-AGENT-PATH | The fresh-agent bar | INT-69 | 01-intent |  |
| D-DONE-FLAGSHIP | The flagship bar | INT-68 | 01-intent |  |
| D-DONE-MEASURED | Measured against today | INT-70 | 01-intent |  |
| D-EXAMPLES-CARRY | Which existing corpora carry over | INT-55 | 01-intent |  |
| D-FACET-SCALES | Facet scales | FEA-CH-36 | 04a-features-chart | C-RICHVIEW-CHART |
| D-FAMOUS-SIMPLIFICATIONS | Famous-chart recreations | FEA-BR-12 | 04d-features-branches | C-GALLERIES |
| D-FIX-HOME-GUTTERS | Home page has no side gutters: hotfix the gutter rules | OPN-17 | 08-open-work |  |
| D-FIX-MOBILE-OVERFLOW | Home page scrolls sideways on phones and tablets: hotfix the overflowing content | OPN-18 | 08-open-work |  |
| D-FIX-SHARE-IMAGE | Share image still says BETA: replace it in the hotfix | OPN-20 | 08-open-work |  |
| D-FIX-STYLE-INJECTION | Style-file script injection: no legacy patch; the rewrite fixes it | OPN-16 | 08-open-work |  |
| D-FIX-THEME-SWITCH-CHECK | Broken theme-switch check: not repaired in legacy | OPN-21 | 08-open-work |  |
| D-FLAGSHIP-CLAIMS | Flagship findings in marketing | INT-45 | 01-intent |  |
| D-FLAGSHIP-HEADER | The flagship’s header graphic | INT-75 | 01-intent |  |
| D-FLAGSHIP-STORY | Flagship story | INT-10 | 01-intent |  |
| D-FLY-LEGACY | Destroy the May Fly app now | OPN-05 | 08-open-work |  |
| D-GALLERIES-PR65 | PR #65 galleries: close; reuse the famous-chart content | OPN-09 | 08-open-work |  |
| D-GALLERY-ROUTES | Old gallery routes redirect only where their content lives | OPN-19 | 08-open-work |  |
| D-GUIDE-AGENT-DOCS | Stale agent docs leave `dev` until they are generated | OPN-31 | 08-open-work |  |
| D-GUIDE-AGENT-HARNESS | The orchestrator default and agent definitions leave `dev` at its first commit | OPN-33 | 08-open-work |  |
| D-GUIDE-CLAUDE-MD | From its first change, `dev` carries interim agent instructions in AGENTS.md | OPN-30 | 08-open-work |  |
| D-GUIDE-DESIGN-RULE | One design-rule source instead of four | OPN-32 | 08-open-work |  |
| D-GUIDE-EXTERNAL | Guidance outside the repository follows the lockdown | OPN-49 | 08-open-work |  |
| D-HARNESS-PR68 | PR #68 development harness: fold its rules into the gates | OPN-12 | 08-open-work |  |
| D-HEATMAP-KEY | Heatmap colour key | FEA-CH-34 | 04a-features-chart | C-RICHVIEW-CHART |
| D-HOSTING | Hosting provider | INT-48 | 01-intent |  |
| D-HOTFIX-CONTENTS | The legacy hotfix: contents, timing and evidence | OPN-43 | 08-open-work |  |
| D-HTML-SAFETY | No code or raw HTML from source; escaping at every sink | ARC-13 | 03-architecture |  |
| D-INTERACTIVITY | Static-first SVG with Wasm hydration | ARC-05 | 03-architecture |  |
| D-JS-BOUNDARY | Where JavaScript is allowed | ARC-03 | 03-architecture |  |
| D-LABEL-DENSITY | Sparse point labels and author highlights | FEA-CH-28 | 04a-features-chart | C-RICHVIEW-CHART |
| D-LANDING-MOTION | Landing motion | STY-44 | 06-styles |  |
| D-LAUNCH | Launch | INT-43 | 01-intent |  |
| D-LAYERING | Layers and dependency direction | ARC-01 | 03-architecture |  |
| D-LEGACY-DEFECTS | Every other mined defect becomes a rewrite fixture | OPN-22 | 08-open-work |  |
| D-LEGACY-DOCS | Legacy documentation is evidence, not specification | OPN-34 | 08-open-work |  |
| D-LEGACY-DOMAINS | Legacy hosts: richview.uk, richview-v3.fly.dev, therichview.com | INT-47 | 01-intent |  |
| D-LEGACY-FREEZE | Legacy code is frozen except one bounded hotfix | OPN-02 | 08-open-work |  |
| D-LEGACY-REPOS | Legacy public repositories | INT-40 | 01-intent |  |
| D-LINEAR-STATE | Reconcile Linear with reality before it guides the rewrite | OPN-41 | 08-open-work |  |
| D-LINK-PREVIEW-RENDER | Link-preview thumbnails are rendered by the server core | ARC-69 | 03-architecture |  |
| D-MAX-AUTHORING | Short authoring path | INT-13 | 01-intent |  |
| D-MAX-COST | Low, explainable operating cost | INT-15 | 01-intent |  |
| D-MAX-CRAFT | Visual and motion craft | INT-12 | 01-intent |  |
| D-MAX-EVIDENCE | Truthful, sourced evidence | INT-11 | 01-intent |  |
| D-MAX-EXTENSIBILITY | Extensibility without a plugin system | INT-78 | 01-intent |  |
| D-MAX-FOOTPRINT | Small footprint | INT-14 | 01-intent |  |
| D-MAX-SPEED | Speed: a floor, not a target | INT-16 | 01-intent |  |
| D-MAX-STORY | Storytelling | INT-77 | 01-intent |  |
| D-MAXIMIZE | Order when targets conflict | INT-17 | 01-intent |  |
| D-MCP-SURFACE | MCP tools | FEA-SF-09 | 04c-features-surfaces | C-APP-MCP |
| D-MEASURE | One measure for every block | STY-30 | 06-styles |  |
| D-MOTION | Chart entrances | FEA-CH-51 | 04a-features-chart | C-WEB-CHART-HOST |
| D-MOTION-UNBUILT | Motion described but never built | FEA-CH-53 | 04a-features-chart | C-WEB-CHART-HOST |
| D-NETWORK-POLICY | Hosts never fetch report inputs | ARC-11 | 03-architecture |  |
| D-NEWS-DEMO | RichView News during the rewrite | INT-74 | 01-intent |  |
| D-NEWS-LIVE | news.richview.ai is improved now; its publishing path is repaired in that work | OPN-03 | 08-open-work |  |
| D-NEWS-PUBLISHER | News publisher job and GitHub credential: owned by the News session | OPN-46 | 08-open-work |  |
| D-NOT-BI | Not a BI tool or query engine | INT-24 | 01-intent |  |
| D-NOT-CHAT | Not a chat product or agent runtime | INT-25 | 01-intent |  |
| D-NOT-CODE-CUSTOMIZATION | No raw CSS, JavaScript, HTML or chart specs | INT-28 | 01-intent |  |
| D-NOT-MONETIZATION | Monetization only through paid hosted services | INT-30 | 01-intent |  |
| D-NOT-SAAS | Single-author core; one hosted service for data and media | INT-27 | 01-intent |  |
| D-NOT-TRACKING | No reader tracking | INT-80 | 01-intent |  |
| D-NOT-TRUTH-CERT | Checks do not certify the analysis | INT-29 | 01-intent |  |
| D-NOT-WEB-EDITOR | No WYSIWYG or full browser editor | INT-26 | 01-intent |  |
| D-OCTOBER-AUTOMATION | The October supervisor, its deploy authority and the executor delegations are retired | OPN-47 | 08-open-work |  |
| D-ONE-OPERATION-LAYER | One operation layer behind every surface | ARC-08 | 03-architecture |  |
| D-OPEN-SOURCE | Licence | INT-37 | 01-intent |  |
| D-OPEN-SOURCE-CLAIM | Public wording about open source | INT-39 | 01-intent |  |
| D-ORG-ARCHIVE | Keep the old organization as it is; never reuse the old repository name | OPN-26 | 08-open-work |  |
| D-ORG-PUBLIC-REPOS | Archive richview-charts and richview-docs now, with a pointer | OPN-25 | 08-open-work |  |
| D-PALETTE-ENFORCEMENT | Every palette is checked at render | FEA-CH-62 | 04a-features-chart | C-RICHVIEW-CHART |
| D-PLATFORMS | Supported platforms | ARC-67 | 03-architecture |  |
| D-PLAYBACK-AXES | Axes during playback | FEA-BR-31 | 04d-features-branches | C-RICHVIEW-CHART |
| D-PLAYGROUND | Where the playground lives and how people find it | FEA-SF-33 | 04c-features-surfaces | C-WEB-PLAYGROUND |
| D-POSITIONING | One-sentence definition | INT-01 | 01-intent |  |
| D-PRERELEASE | The prerelease goes; its tag stays as a history marker | OPN-28 | 08-open-work |  |
| D-PROD-BASELINE | Production is what richview.ai serves; `main` is made equal to it first | OPN-01 | 08-open-work |  |
| D-PUBLIC-IDENTITY | Who RichView presents as its maker | INT-73 | 01-intent |  |
| D-PUBLISH-CONTRACT | One publish contract with server-side rendering | ARC-20 | 03-architecture |  |
| D-PUBLISH-CREDENTIALS | Publish and upload credentials stay out of source and agent context | ARC-23 | 03-architecture |  |
| D-PUBLISH-PR70 | PR #70 RichView Publish: merged into `main` as it is; the rewrite carries over its behaviour | OPN-14 | 08-open-work |  |
| D-PUBLISH-TARGET | How stories are shared | INT-36 | 01-intent |  |
| D-REEL | The 15-second reel | FEA-SF-45 | 04c-features-surfaces | C-WEB-REEL |
| D-RELEASE | Releases: builds from `prod` and two extension listings only | INT-41 | 01-intent |  |
| D-REPO-SPLIT | Repository and history | INT-50 | 01-intent |  |
| D-REPO-TOPOLOGY | Workspace and publication content | ARC-16 | 03-architecture |  |
| D-REPO-URLS | No public link to a private repository | OPN-27 | 08-open-work |  |
| D-RESOLVER-BOUNDARY | The report folder is the only file boundary | ARC-10 | 03-architecture |  |
| D-REWRITE-BASIS | What “rewrite” means for existing code | INT-49 | 01-intent |  |
| D-RICHVIEW-LEAD | Who leads RichView, and telling Steven first | OPN-48 | 08-open-work |  |
| D-RMD-COMPAT | Existing sources keep working through migration | INT-51 | 01-intent |  |
| D-RUNTIME | Rust and WebAssembly | INT-52 | 01-intent |  |
| D-RUNTIME-EMBED | The CLI carries its own report runtime | ARC-07 | 03-architecture |  |
| D-RUNTIME-SPLIT | Report and site runtimes, and how reports load them | ARC-06 | 03-architecture |  |
| D-SAFETY-FLOOR | Safety: a floor that never trades | INT-79 | 01-intent |  |
| D-SANDBOX | Embed and preview sandbox policy | ARC-14 | 03-architecture |  |
| D-SCOPE-CHARTS | Charts: internal engine or standalone library | INT-32 | 01-intent |  |
| D-SCOPE-CORE | What the rewrite builds | INT-31 | 01-intent |  |
| D-SCOPE-COWORK | RichView Cowork: the self-hostable internal server | INT-83 | 01-intent |  |
| D-SCOPE-DASHBOARDS | Dashboards | INT-33 | 01-intent |  |
| D-SCOPE-PUBLISH | RichView Publish | INT-35 | 01-intent |  |
| D-SCOPE-VOICE-DASHBOARD | Voice-to-dashboard demo | INT-34 | 01-intent |  |
| D-SERIES-CUES | Series named at their line ends | FEA-CH-27 | 04a-features-chart | C-RICHVIEW-CHART |
| D-SERVE-ISOLATION | Content Security Policy and report origin | ARC-15 | 03-architecture |  |
| D-SHOWCASE-CONTROLS | Removed showcase controls and their dormant code | FEA-SF-37 | 04c-features-surfaces | C-WEB-SHOWCASE |
| D-SIGNUP | Email signup for updates | FEA-SF-31 | 04c-features-surfaces | C-CLOUDFLARE-TEASER |
| D-SINGLE-SOURCE | Generated vocabularies from one source | ARC-09 | 03-architecture |  |
| D-SITE-NAV | Site navigation | FEA-SF-28 | 04c-features-surfaces | C-RICHVIEW-WEB |
| D-SITE-PR69 | PR #69 site repair: merge as the record of production | OPN-13 | 08-open-work |  |
| D-SITE-SPLIT | The report host serves reports only | ARC-24 | 03-architecture |  |
| D-SOURCE-VISIBILITY | When the source becomes public | INT-38 | 01-intent |  |
| D-STATUS-LABEL | Public status label | INT-42 | 01-intent |  |
| D-STEVEN-ACCESS | Steven's access to the transferred repository | OPN-24 | 08-open-work |  |
| D-STORE-MODEL | Report identity and storage model | ARC-21 | 03-architecture |  |
| D-STYLE-BOUNDARY | Every style value passes a closed grammar | ARC-12 | 03-architecture |  |
| D-STYLE-CATALOG | Exactly five styles | STY-05 | 06-styles |  |
| D-STYLE-PERSISTENCE | Remembered style choice | FEA-BR-42 | 04d-features-branches | C-GALLERIES |
| D-STYLES-CARRY | Style identities carry over | INT-54 | 01-intent |  |
| D-SUBSCRIBERS | The update list and its consent | INT-72 | 01-intent |  |
| D-SUCCESS-SIGNAL | How RichView is judged after launch | INT-82 | 01-intent |  |
| D-TABS-ANCHORS | Panel heading anchors | FEA-BR-48 | 04d-features-branches | C-RICHVIEW-CORE |
| D-TABS-PR63 | PR #63 multi-tab reports: fold, do not merge | OPN-07 | 08-open-work |  |
| D-TAGLINE | Tagline | INT-02 | 01-intent |  |
| D-TOOLTIP | Readout panel: light, stable, never invented | FEA-CH-42 | 04a-features-chart | C-WEB-CHART-HOST |
| D-TYP-ASSET-REF | Pinned remote asset references | TYP-41 | 05-types |  |
| D-TYP-BLOCK-WIDTH | Full-width opt-in per block | TYP-89 | 05-types |  |
| D-TYP-COLUMN-DECIMALS | A column from a typed numeric source shows one number of decimals | TYP-104 | 05-types |  |
| D-TYP-COLUMN-ORDER | Column and key order | TYP-25 | 05-types |  |
| D-TYP-CONTENT-IDENTITY | What makes two publications the same content | TYP-95 | 05-types |  |
| D-TYP-CURRENCY | Money formats | TYP-87 | 05-types |  |
| D-TYP-DATA-FORMAT-SNIFF | The source key decides the data format | TYP-92 | 05-types |  |
| D-TYP-DATA-PROJECTION | Unused data columns in reports | TYP-36 | 05-types |  |
| D-TYP-DESCRIPTION | Is `description` required to publish? | TYP-07 | 05-types |  |
| D-TYP-DOC-DRIFT | The generated spec is the format reference | TYP-04 | 05-types |  |
| D-TYP-DOMAIN | Publish server option and canonical host | TYP-67 | 05-types |  |
| D-TYP-EMBED-LIST | Default embed hosts | TYP-33 | 05-types |  |
| D-TYP-FIELD-CHECKS | Checking that referenced fields exist | TYP-15 | 05-types |  |
| D-TYP-FORMAT-VERSION | Format version marker | TYP-05 | 05-types |  |
| D-TYP-FRONTMATTER-KEYS | Unknown frontmatter keys | TYP-84 | 05-types |  |
| D-TYP-INTERNAL-KEYS | Host-internal `_` keys in source | TYP-11 | 05-types |  |
| D-TYP-LATEST | Which version is latest | TYP-96 | 05-types |  |
| D-TYP-LINES | Source lines on diagnostics | TYP-56 | 05-types |  |
| D-TYP-METRIC-TREND | Metric trend default | TYP-88 | 05-types |  |
| D-TYP-METRIC-UNIT | Metric units | TYP-27 | 05-types |  |
| D-TYP-MIGRATIONS | Breaking changes and their migration | TYP-83 | 05-types |  |
| D-TYP-MISSING-VALUE | Missing values and numeric text | TYP-91 | 05-types |  |
| D-TYP-NAMING-CASE | Letter case of names | TYP-82 | 05-types |  |
| D-TYP-RATIO | Chart aspect ratios | TYP-18 | 05-types |  |
| D-TYP-RATIO-GRAMMAR | Valid `style.ratio` values and the top-level `ratio` | TYP-85 | 05-types |  |
| D-TYP-REMOTE-MEDIA | Remote images, audio and video | TYP-40 | 05-types |  |
| D-TYP-REPUBLISH | Republishing an existing version | TYP-63 | 05-types |  |
| D-TYP-SLUG-FALLBACK | Slug when frontmatter has a problem | TYP-61 | 05-types |  |
| D-TYP-STATISTIC-PAYLOAD | Statistic charts over many rows embed their structure, not their observations | TYP-103 | 05-types |  |
| D-TYP-STYLE-FORMATS | The showcase theme format | TYP-93 | 05-types |  |
| D-TYP-STYLE-INJECTION | Style values reaching CSS unchecked | TYP-51 | 05-types |  |
| D-TYP-TEMPORAL-FEATURES | Which time-series engine features block YAML reaches | TYP-86 | 05-types |  |
| D-TYP-UNKNOWN-KEYS | Unknown keys and invalid values in blocks | TYP-10 | 05-types |  |
| D-TYP-UNTITLED-CHART | Charts without a title | TYP-13 | 05-types |  |
| D-TYP-VERSION-RETENTION | Version retention | TYP-64 | 05-types |  |
| D-TYP-VOCABULARIES | Every type here lives in the one schema registry | TYP-03 | 05-types |  |
| D-TYP-WIDTH-RANGE | Chart width range | TYP-72 | 05-types |  |
| D-USE-CASE | Reference journey | INT-09 | 01-intent |  |
| D-V2-STACK | The v2 app, docs and API on richview.uk subdomains: export, then retire | OPN-45 | 08-open-work |  |
| D-VSCODE | How the extension is installed | FEA-SF-21 | 04c-features-surfaces | C-VSCODE-EXTENSION |
| D-WASM-BOUNDARY | Wasm boundary and build profile | ARC-04 | 03-architecture |  |
| D-WEBSITE-ROLE | What richview.ai is for | INT-44 | 01-intent |  |
| D-WHY | Why RichView: three value claims | INT-03 | 01-intent |  |
| D-WORKERS-DEV-HOST | The workers.dev copy of richview.ai goes off with the hotfix | OPN-44 | 08-open-work |  |
| D-WORKSPACE-LAYOUT | Target workspace layout | ARC-17 | 03-architecture |  |
| D-WORKTREE-RESIDUE | Unpushed and uncommitted work in task worktrees is preserved, then removed | OPN-15 | 08-open-work |  |
| D-XPART-CHART-BINARY | The `richview-chart` binary is removed | OPN-68 | 08-open-work |  |
| D-XPART-CONNECTORS | Connectors: none in the core; a secondary path owned by RichView Publish | OPN-63 | 08-open-work |  |
| D-XPART-DIAGNOSTIC-CODES | The code catalogue takes every code an accepted card introduces | OPN-58 | 08-open-work |  |
| D-XPART-DIAGNOSTIC-RECORD | One diagnostic record: TYP-53's | OPN-57 | 08-open-work |  |
| D-XPART-HOST-ACCESS | Report list behind the token; no render API | OPN-54 | 08-open-work |  |
| D-XPART-JSON-FEATURES | JSON-only chart features are not built | OPN-56 | 08-open-work |  |
| D-XPART-METRIC-VARIANT | The hairline metric variant is named `rule` | OPN-61 | 08-open-work |  |
| D-XPART-MOTION-SETTING | The motion switch is `motion: on\|off` | OPN-60 | 08-open-work |  |
| D-XPART-PDF-FIXTURES | PDF pagination fixtures run in the release run | OPN-52 | 08-open-work |  |
| D-XPART-PERF-GATE | Per-batch performance follows part 07: no load wait, WebKit in the full capture | OPN-50 | 08-open-work |  |
| D-XPART-PHONE-CHECK | The real-phone check uses the private Worker preview | OPN-66 | 08-open-work |  |
| D-XPART-PROD-EVIDENCE | The prod receipt covers the frozen candidate; the full capture also runs daily | OPN-51 | 08-open-work |  |
| D-XPART-REEL-TAGLINE | The reel's one legacy render carries the new tagline | OPN-64 | 08-open-work |  |
| D-XPART-REPORT-RUNTIME | One report runtime in every report | OPN-53 | 08-open-work |  |
| D-XPART-RICHTEMPLATES | RichTemplates is archived on OPN-25's schedule | OPN-65 | 08-open-work |  |
| D-XPART-STYLE-DEPTH | Authors style through rich, validated style files | OPN-59 | 08-open-work |  |
| D-XPART-TABULAR-FIGURES | Tabular metric figures only where the display face has them | OPN-62 | 08-open-work |  |
| D-XPART-TIER-HOST | The fast tier is judged on Lenovo and on Claw Mini | OPN-55 | 08-open-work |  |
| D-XPART-TYPES-FOLLOW-FEATURES | Part 05 follows Daniel's accepted feature answers | OPN-67 | 08-open-work |  |

## Features by component

For each of the 41 part 03 component cards, the features (`F-`) whose items in parts 04a to 04d name it as `component`, and the other entities those items define: 204 features on 28 components. The 13 components without features are settled by their part 03 card, the other entities listed, and items in parts 01 to 03 and 05 to 08, which carry no `component` field.

| Component | Title | Features | Feature IDs | Other entities |
| --- | --- | --- | --- | --- |
| C-AGENT-HARNESS | Agent instructions and definitions | 0 | none |  |
| C-APP-ASSET-UPLOAD | Asset uploader | 1 | F-APP-ASSET-PUT |  |
| C-APP-CLI | CLI | 8 | F-APP-CLI-CHECK, F-APP-CLI-GENERATE, F-APP-CLI-INIT, F-APP-CLI-PUBLISH, F-APP-CLI-VALIDATE, F-CLI-COMMANDS, F-CLI-HUMAN-OUTPUT, F-REPORT-OWNER-ACTIONS |  |
| C-APP-CONNECTORS | Data connectors (PR #66) | 4 | F-APP-CONNECTORS-CACHE, F-APP-CONNECTORS-METRICFLOW, F-APP-CONNECTORS-PARAMS, F-APP-CONNECTORS-SQL |  |
| C-APP-HTTP-SERVER | HTTP host | 9 | F-APP-CLI-SERVE, F-APP-CLI-SERVER, F-APP-SERVER-READ-API, F-APP-SERVER-REGENERATE, F-APP-SERVER-RENDER-API, F-APP-SERVER-REPORT-PAGES, F-PUBLISH-NEW-ADDRESS, F-PUBLISHED-LINKED-FILES, F-REPORT-MENU | D-ACCESS-CLAIM |
| C-APP-MCP | MCP server | 2 | F-APP-MCP-RESOURCES, F-APP-MCP-TRANSPORT | D-MCP-SURFACE |
| C-APP-PDF-EXPORT | PDF printer and browser host service | 5 | F-APP-PDF-EXPORT, F-CHART-BROWSER-MEASUREMENT, F-EXPORT-CAROUSEL, F-EXPORT-PDF, F-PDF-ERRORS |  |
| C-APP-REPORT-STORE | Report store | 1 | F-REPORT-STORE-BEHAVIOUR |  |
| C-APP-RESOLVER | Resolver and exporter | 3 | F-APP-EXPORT-RENDERED, F-APP-RESOLVE-MEDIA, F-PARTIAL-RENDER |  |
| C-BENCH-LABS | Comparison labs and chart corpora (`benchmarks/`) | 1 | F-WEB-SHOWCASE-CHART-REVIEW |  |
| C-CI | CI workflow | 0 | none |  |
| C-CLOUDFLARE-TEASER | Cloudflare site Worker and signup | 3 | F-SITE-CRAWLERS, F-SITE-HEADERS, F-SITE-ROUTES | D-SIGNUP |
| C-CONTAINER-DEPLOY | Container image and Fly config | 0 | none |  |
| C-DEV-HARNESS | Development harness (PR #68) | 1 | F-DEVHARNESS-RUNTIME-DRILL | G-WORKER-REPLY-HOLD |
| C-DOCS-SITE | Documentation site | 4 | F-DOCS-CHART-GALLERY, F-DOCS-CHART-TYPE-PAGES, F-DOCS-GUIDES, F-DOCS-TUTORIAL-EXAMPLE |  |
| C-EXAMPLES | Example reports | 0 | none |  |
| C-FONT-PIPELINE | Font pipeline and catalogs | 0 | none |  |
| C-GALLERIES | Standalone galleries (PR #65) | 1 | F-GALLERY-TYPOGRAPHY | D-DOCS-CHART-EXAMPLES, D-FAMOUS-SIMPLIFICATIONS, D-STYLE-PERSISTENCE |
| C-LLMS-TXT | llms.txt | 0 | none |  |
| C-LOCKDOWN-TOOLING | Lockdown, guide and review app | 0 | none |  |
| C-PKG-CHARTS | npm package @richview/charts | 0 | none |  |
| C-PKG-CORE | npm package @richview/core | 1 | F-PKG-CORE-API |  |
| C-RICHVIEW-APP | richview-app: native host crate | 0 | none |  |
| C-RICHVIEW-BENCH | Benchmark crate | 0 | none |  |
| C-RICHVIEW-BUILD | Build orchestrator | 0 | none |  |
| C-RICHVIEW-CHART | richview-chart: chart engine | 41 | F-CHART-A11Y, F-CHART-ANNOTATIONS, F-CHART-AREA, F-CHART-AXES, F-CHART-AXIS-NOTATION, F-CHART-BAR, F-CHART-BAR-SELF-COLOUR, F-CHART-BOXPLOT, F-CHART-CATEGORY-LABELS, F-CHART-CATEGORY-PERIODS, F-CHART-CHECKS-BY-TYPE, F-CHART-CONTROLS, F-CHART-CONTROLS-FILTER, F-CHART-CONTROLS-METRIC, F-CHART-CONTROLS-VARIANT, F-CHART-DECALS, F-CHART-DECIMATION, F-CHART-ENTITY-COLOURS, F-CHART-EXACT-READOUTS, F-CHART-FACETS, F-CHART-HEATMAP, F-CHART-HISTOGRAM, F-CHART-INK-ALIGN, F-CHART-INTERVAL, F-CHART-LABEL-PLACEMENT, F-CHART-LINE, F-CHART-MISSING-DATA, F-CHART-NATIVE-DIAGNOSTICS, F-CHART-NUMERIC-LIMITS, F-CHART-PALETTES, F-CHART-PIE, F-CHART-RATIO, F-CHART-READOUT-CONTENT, F-CHART-RECT-CLAMP, F-CHART-SCATTER, F-CHART-SOURCE-FAILURE, F-CHART-STACKING, F-CHART-STANDALONE-SVG, F-CHART-TICKS, F-CHART-TIMESERIES-SPEC, F-CHART-TRANSFORMS | D-AXIS-COMPACT, D-AXIS-FORMATS, D-AXIS-TITLES, D-CALENDAR-LABELS, D-CHART-CALENDAR-MERGE, D-CHART-CHECK-GAPS, D-CHART-METHOD-NOTES, D-CHART-ORDER, D-CHART-TEXT-METRICS, D-CHART-TYPES, D-CURVES, D-FACET-SCALES, D-HEATMAP-KEY, D-LABEL-DENSITY, D-PALETTE-ENFORCEMENT, D-PLAYBACK-AXES, D-SERIES-CUES, G-CONTROL-STATES |
| C-RICHVIEW-CORE | richview-core: format, styles and renderer | 60 | F-A11Y-FLOOR, F-AGENT-USE-MEASURE, F-BLOCK-LOCAL-DATA, F-BROWSER-SUPPORT, F-BUILTIN-STRINGS, F-CODE-BLOCKS, F-COLOPHON, F-CORE-BIG-NUMBER, F-CORE-CALLOUT, F-CORE-EMBED, F-CORE-HTML-DOCUMENT, F-CORE-LEARN, F-CORE-MARKDOWN, F-CORE-MEASURE-WIDE-BLOCKS, F-CORE-MEDIA, F-CORE-MOTION, F-CORE-PDF-OPTIONS, F-CORE-PRINT, F-CORE-REPORT-CARD, F-CORE-SOURCE-NOTES, F-CORE-STATIC-CHART-FRAMES, F-CORE-STYLE-CASCADE, F-CORE-TABLE, F-CORE-TABS, F-FOOTNOTES, F-FRONTMATTER-FAILURE, F-HEADER-DECK, F-HEADINGS, F-HERO-IMAGERY, F-LINK-PREVIEW-META, F-LINKS, F-MARKDOWN-IMAGES, F-MATH, F-MEDIA-CAPTIONS, F-MEDIA-PLAYERS, F-METRIC-ROW, F-NEAR-MISS-TAG, F-NESTED-BLOCKS, F-NOT-RENDERED, F-OUTPUT-MARKERS, F-PROLOG, F-PULL-QUOTE, F-RAW-HTML, F-READABLE-WITHOUT-SCRIPTS, F-REDUCED-MOTION, F-REPORT-HEADER, F-REPORT-LOCALE, F-REPORT-PRINT, F-REPORT-SLUG-STABILITY, F-REPORT-TABS, F-REPRODUCIBLE-RENDER, F-SCROLLY-STORY, F-SELF-CONTAINED-HTML, F-SMART-PUNCTUATION, F-TABLE-CELL-TEXT, F-TABLE-TITLE, F-TABLE-TOTALS, F-THEME-OPTION, F-UNSPLASH-IMAGES, F-WIKILINKS | D-TABS-ANCHORS, S-TAB-ROW |
| C-RICHVIEW-EDITOR | Editor service and its protocol | 3 | F-EDITOR-TABS, F-VSCODE-LINT, F-VSCODE-PROVIDERS |  |
| C-RICHVIEW-PUBLISH | RichView Publish and News (PR #70) | 12 | F-PUBLISH-ACCESS, F-PUBLISH-DASHBOARD-SNAPSHOT, F-PUBLISH-EDITORIAL-DESK, F-PUBLISH-FEEDS-SEARCH, F-PUBLISH-GIT-LOOP, F-PUBLISH-LAYOUTS-THEMES, F-PUBLISH-LIMITS, F-PUBLISH-MODE, F-PUBLISH-RELEASES, F-PUBLISH-STABLE-LINKS, F-PUBLISH-STATUS, F-URGENT-HIDE |  |
| C-RICHVIEW-WEB | richview-web: browser host | 7 | F-EMBED-LOADING, F-SITE-SHARE-IMAGES, F-TABS-HIDDEN-LINKS, F-TABS-STYLE-COHERENCE, F-WEB-LEGAL-PAGES, F-WEB-TABLE-SORT, F-WEB-TABS-NAV | D-SITE-NAV |
| C-SKILL | Agent skill | 1 | F-SKILL-AGENT-GUIDE |  |
| C-VERIFY-HARNESS | Verification scripts (`scripts/`) | 0 | none | FEA-SF-62 (no entity), G-BROWSER-CONTROLS-TABS |
| C-VOICE-DASHBOARD | Voice-to-dashboard demo (PR #64) | 1 | F-VOICE-DASHBOARD |  |
| C-VSCODE-EXTENSION | VS Code extension | 5 | F-VSCODE-EXPORT-VALIDATE, F-VSCODE-GRAMMAR, F-VSCODE-INSERT-BLOCKS, F-VSCODE-LANGUAGE, F-VSCODE-PREVIEW | D-VSCODE |
| C-WEB-CHART-EXAMPLES | Chart examples page (/charts/) | 1 | F-WEB-CHARTS-EXAMPLES |  |
| C-WEB-CHART-HOST | Live chart chrome | 14 | F-CHART-CONTROLS-PLAYBACK, F-CHART-DOWNLOADS, F-CHART-HIT-TESTING, F-CHART-INTERACTION-MOTION, F-CHART-INTERACTION-POLICY, F-CHART-SELECTORS, F-CHART-SETTLED-STATE, F-WEB-CHART-CSV-DOWNLOAD, F-WEB-CHART-INSPECTION, F-WEB-CHART-LEGEND, F-WEB-CHART-OVERVIEW, F-WEB-CHART-SVG-DOWNLOAD, F-WEB-CHART-TABLE, F-WEB-CHART-ZOOM-PAN | D-CHART-ACTIONS, D-CHART-MODAL, D-CHART-MODAL-PORTRAIT, D-CHART-SOURCE-UPDATE, D-CONTROLS-ACTIONS, D-MOTION, D-MOTION-UNBUILT, D-TOOLTIP, P-CONTROL-CHANGE, S-CHART-CONTROLS |
| C-WEB-LANDING | Landing page | 6 | F-SITE-DEMO-DIAGNOSTICS, F-SITE-DEMO-STYLE-PAGES, F-SITE-HOME-SECTIONS, F-SITE-HOMEPAGE, F-WEB-LANDING-LIVE-DEMO, F-WEB-LANDING-MOTION-RESTORED |  |
| C-WEB-PLAYGROUND | Playground | 2 | F-WEB-PLAYGROUND-LIVE-RENDER, F-WEB-PLAYGROUND-SHARE | D-PLAYGROUND |
| C-WEB-REEL | Reel | 0 | none | D-REEL |
| C-WEB-SHOWCASE | Showcase | 5 | F-WEB-SHOWCASE-CITATION, F-WEB-SHOWCASE-FLAGSHIP, F-WEB-SHOWCASE-INSPECT, F-WEB-SHOWCASE-SHELL, F-WEB-SHOWCASE-STYLE-PICKER | D-SHOWCASE-CONTROLS |
| C-WEB-WASM-API | Wasm JavaScript API | 2 | F-WEB-CONTROL-EVENTS, F-WEB-SETTLE-HOOK |  |

## IDs without a defining item (59)

These appear in part text, the guide or the lockdown files, but no item defines them. Most are names from the evidence files or Linear; use the lockdown ID they resolve to. FEA-SF-62 defines no entity. F-CORE-CODE-HIGHLIGHT and F-CORE-PARSE are also evidence names, listed again in the next table.

| ID | Used in | Resolves to |
| --- | --- | --- |
| C-RICH-MARKDOWN, C-THEMES, C-RENDERER | 03 summary, ARC-26; guide architecture.md, components/core.md | Linear component labels for C-RICHVIEW-CORE (ARC-26) |
| C-CHART-ENGINE | 03 summary, ARC-25; guide architecture.md | Linear label for C-RICHVIEW-CHART (ARC-25) |
| C-SERVER | 03 summary, ARC-31; guide architecture.md | Linear label for C-APP-HTTP-SERVER (ARC-31) with C-APP-REPORT-STORE (ARC-32) |
| C-CLI-MCP | 03 summary, ARC-28, FEA-SF-01 sources; guide architecture.md | Linear label for C-APP-CLI (ARC-28) with C-APP-MCP (ARC-33) |
| C-VSCODE | 03 summary, ARC-45; guide architecture.md | Linear label for C-VSCODE-EXTENSION (ARC-45) |
| C-PUBLISH | 03 summary, ARC-57; guide architecture.md | Linear label for C-RICHVIEW-PUBLISH (ARC-57) |
| C-DASHBOARDS | 03 summary, ARC-58; guide architecture.md | Linear label for C-VOICE-DASHBOARD (ARC-58) |
| C-WEBSITE | 03 summary; guide architecture.md | Linear label for ARC-38 to ARC-43 and ARC-55, with ARC-64 for the chart-review pages |
| F-REPORT-VERSIONS | FEA-SF-14 sources | Linear tag; F-APP-SERVER-REPORT-PAGES (FEA-SF-14) cites it |
| F-VSCODE | FEA-SF-21 sources, FEA-SF-24 sources, FEA-SF-25 sources | Linear tag; D-VSCODE, F-VSCODE-LINT and F-VSCODE-PROVIDERS cite it |
| F-SITE-EDITOR-DEMO | FEA-SF-29 sources | Linear tag; F-WEB-LANDING-LIVE-DEMO (FEA-SF-29) cites it |
| F-SITE-SIGNUP | FEA-SF-31 sources | Linear tag; D-SIGNUP (FEA-SF-31) cites it |
| F-FLAGSHIP | FEA-SF-38 sources | Linear tag; F-WEB-SHOWCASE-CITATION (FEA-SF-38) cites it |
| F-DOCS | FEA-SF-41 sources, FEA-SF-42 sources | Linear tag; F-DOCS-GUIDES and F-DOCS-TUTORIAL-EXAMPLE cite it |
| F-ASSET-HOSTING | FEA-SF-50 sources | Linear tag; F-APP-ASSET-PUT (FEA-SF-50) cites it |
| F-CORE-CODE-HIGHLIGHT | FEA-RP-17 conflicts; guide components/core.md | `evidence/product.md` feature; F-CODE-BLOCKS (FEA-RP-17) decides highlighting |
| F-CORE-PARSE | guide components/core.md | `evidence/product.md` feature and FEA-RP-01's draft refId; FEA-RP-01 is F-REPRODUCIBLE-RENDER, and TYP-01, TYP-02 and TYP-10 decide parsing |
| G-ARTIFACT-PROOF | DEL-13, TYP-42; guide delivery.md | The Linear evidence (not published) rule; enforced by G-BUILD (DEL-13) |
| G-DEV-HARNESS | none in the current files | The Linear evidence (not published) goal; the gate is G-DEV-GATE (DEL-08) |
| G-REPORT-FOLDER | TYP-39 sources; guide delivery.md | The Linear evidence (not published) rule; T-REPORT-FOLDER (TYP-39) |
| G-RENDERED-OUTPUT | TYP-42 sources; guide delivery.md | The Linear evidence (not published) rule; T-RENDERED-OUTPUT (TYP-42) |
| G-OPTIONAL-STYLE | TYP-45 sources; guide delivery.md | The Linear evidence (not published) rule; T-STYLE-REF (TYP-45) |
| G-BROWSER-WEBKIT | DEL-33; guide delivery.md | `evidence/checks-performance.md` check; the WebKit loop inside G-PROD-CANDIDATE (DEL-33) |
| G-WASM-SIZE | DEL-51; guide delivery.md | `evidence/checks-performance.md` check; the Wasm size check inside G-PERF-BUDGETS (DEL-51), budgeted by P-WASM-GZIP (PRF-30) and P-RUNTIME-DELIVERY (PRF-31) |
| D-RMD-TAGS | TYP-09 | `evidence/history-prs.md` decision; settled by T-RICH-FENCE (TYP-09) |
| D-FRONTMATTER | TYP-07 | `evidence/history-prs.md` decision; settled by D-TYP-DESCRIPTION (TYP-07) |
| D-CHART-CONTRACT | TYP-10 conflicts, TYP-13 conflicts; guide format-and-types.md | `evidence/history-prs.md` decision; D-TYP-UNKNOWN-KEYS (TYP-10) and D-TYP-UNTITLED-CHART (TYP-13) |
| D-TABLE-SYNTAX | TYP-24 | `evidence/history-prs.md` decision; settled by T-BLOCK-TABLE (TYP-24) |
| D-DATA-BINDING | none in the current files | `evidence/history-prs.md` decision; T-DATA-CONNECTOR (TYP-79), C-APP-CONNECTORS (ARC-59) and D-CONNECTORS-PR66 (OPN-10) |
| D-ASSET-UPLOAD | ARC-34 | Decided inside C-APP-ASSET-UPLOAD (ARC-34); no item of its own |
| D-DASHBOARD-SAVE | ARC-58 | A decision ARC-58 leaves to a future dashboard lockdown; not defined |
| D-GT, D-HN | guide components/web.md | Ledger link labels in the flagship showcase report, quoted from a screenshot; not entities |
| P-NATIVE-SMALL, P-NATIVE-SIX, P-NATIVE-FLAGSHIP, P-NATIVE-DENSE | PRF-10 | Replaced by P-NATIVE-RENDER (PRF-10) |
| P-CLI-SMALL, P-CLI-FLAGSHIP, P-CLI-DENSE | PRF-11 | Replaced by P-CLI-COLD (PRF-11) |
| P-EDITOR-SMALL, P-EDITOR-FLAGSHIP, P-EDITOR-DENSE | PRF-13 | Replaced by P-EDITOR-RPC (PRF-13) |
| P-HTTP1-SMALL, P-HTTP1-DENSE, P-HTTP4-SMALL, P-HTTP4-DENSE, P-HTTP-TPUT-SMALL, P-HTTP-TPUT-DENSE | PRF-14 | Replaced by P-HTTP-RENDER (PRF-14) |
| P-ZOOM-LAYOUT, P-ZOOM-FRAMES | PRF-20 | Replaced by P-ZOOM (PRF-20) |
| P-THEME-FIRST, P-THEME-LATEST | PRF-22 | Replaced by P-THEME-SWITCH (PRF-22) |
| P-EDIT-SMALL | PRF-23 | Replaced by P-EDIT (PRF-23) |
| P-EDIT-DENSE | PRF-29 | Replaced by P-EDIT-LARGE (PRF-29) |
| P-WASM-SIZE | PRF-30 | Replaced by P-WASM-GZIP (PRF-30) |
| OPN-23 | 08 summary | Withdrawn item; its number stays a gap because other parts cite OPN items by number, and INT-50 (D-REPO-SPLIT) decides the rewrite's repository and history |

## Evidence names (113)

Entity names from the evidence files that 7 component pages of the guide cite in their tables of evidence entities without their own card (on `components/core.md`, the table under "Format, data and diagnostics"): the name, its title and line in `evidence/`, the cards that decide it, and where it is cited. The guide abbreviates some lists (`F-APP-MCP-RENDER, -VALIDATE`); the 20 abbreviated names are written out here. These names are not lockdown IDs: cite the covering card instead.

| ID | Title in evidence | Evidence | Covered by | Cited in |
| --- | --- | --- | --- | --- |
| F-APP-ASSET-RESUME-ABORT | Resumable journal and abort | product.md:226 | FEA-SF-50 (F-APP-ASSET-PUT), ARC-34 (C-APP-ASSET-UPLOAD) | guide components/app.md |
| F-APP-ASSET-VERIFY | Verify stored object | product.md:225 | FEA-SF-50 (F-APP-ASSET-PUT), ARC-34 (C-APP-ASSET-UPLOAD) | guide components/app.md |
| F-APP-CLI-ASSET | `richview asset` | product.md:161 | FEA-SF-50 (F-APP-ASSET-PUT), ARC-34 (C-APP-ASSET-UPLOAD) | guide components/app.md |
| F-APP-CLI-JSON-ERRORS | Machine-readable failures | product.md:162 | TYP-68 (T-CLI-JSON), FEA-SF-07 (F-CLI-HUMAN-OUTPUT) | guide components/app.md |
| F-APP-CLI-LEARN | `richview learn` | product.md:156 | FEA-SF-47 (F-CORE-LEARN) | guide components/app.md |
| F-APP-CLI-MCP | `richview mcp` | product.md:160 | FEA-SF-09 (D-MCP-SURFACE), FEA-SF-10 (F-APP-MCP-TRANSPORT) | guide components/app.md |
| F-APP-MCP-ASSET-TOOLS | `richview_asset_put`, `richview_asset_verify` | product.md:216 | FEA-SF-50 (F-APP-ASSET-PUT), ARC-34 (C-APP-ASSET-UPLOAD) | guide components/app.md |
| F-APP-MCP-CHECK | `richview_check` | product.md:213 | FEA-SF-09 (D-MCP-SURFACE), TYP-69 (T-MCP-TOOLS) | guide components/app.md |
| F-APP-MCP-LEARN | `richview_learn` | product.md:215 | FEA-SF-09 (D-MCP-SURFACE), TYP-69 (T-MCP-TOOLS) | guide components/app.md |
| F-APP-MCP-PROGRESS | Progress notifications | product.md:218 | FEA-SF-50 (F-APP-ASSET-PUT), ARC-34 (C-APP-ASSET-UPLOAD) | guide components/app.md |
| F-APP-MCP-PUBLISH | `richview_publish` | product.md:214 | FEA-SF-09 (D-MCP-SURFACE), TYP-69 (T-MCP-TOOLS), FEA-SF-12 (F-APP-CLI-PUBLISH) | guide components/app.md |
| F-APP-MCP-RENDER | `richview_render` | product.md:211 | FEA-SF-09 (D-MCP-SURFACE), TYP-69 (T-MCP-TOOLS) | guide components/app.md |
| F-APP-MCP-VALIDATE | `richview_validate` | product.md:212 | FEA-SF-09 (D-MCP-SURFACE), TYP-69 (T-MCP-TOOLS) | guide components/app.md |
| F-APP-RESOLVE-DATA | Local data | product.md:169 | ARC-29 (C-APP-RESOLVER), ARC-10 (D-RESOLVER-BOUNDARY), TYP-39 (T-REPORT-FOLDER), TYP-43 (T-RESOURCE-LIMITS) | guide components/app.md |
| F-APP-RESOLVE-NETWORK | Remote loading (documented, absent) | product.md:172 | ARC-11 (D-NETWORK-POLICY) | guide components/app.md |
| F-APP-RESOLVE-REPORT-FOLDER | Report-folder inventory | product.md:168 | ARC-29 (C-APP-RESOLVER), ARC-10 (D-RESOLVER-BOUNDARY), TYP-39 (T-REPORT-FOLDER), TYP-43 (T-RESOURCE-LIMITS) | guide components/app.md |
| F-APP-RESOLVE-STYLES | Style files | product.md:171 | ARC-29 (C-APP-RESOLVER), ARC-10 (D-RESOLVER-BOUNDARY), TYP-39 (T-REPORT-FOLDER), TYP-43 (T-RESOURCE-LIMITS) | guide components/app.md |
| F-APP-SERVER-HARDENING | Limits and headers | product.md:195 | ARC-31 (C-APP-HTTP-SERVER), ARC-15 (D-SERVE-ISOLATION), TYP-43 (T-RESOURCE-LIMITS) | guide components/app.md |
| F-APP-SERVER-HEALTH | Health check | product.md:185 | FEA-SF-20 (F-APP-CLI-SERVER), TYP-70 (T-HTTP-API), INT-47 (D-LEGACY-DOMAINS) | guide components/app.md |
| F-APP-SERVER-LEGACY-REDIRECT | therichview.com redirect | product.md:194 | FEA-SF-20 (F-APP-CLI-SERVER), TYP-70 (T-HTTP-API), INT-47 (D-LEGACY-DOMAINS) | guide components/app.md |
| F-APP-SERVER-PREVIEW | Preview revision and reload | product.md:193 | FEA-SF-08 (F-APP-CLI-SERVE) | guide components/app.md |
| F-APP-SERVER-PUBLISH-API | Publish API | product.md:187 | ARC-20 (D-PUBLISH-CONTRACT), TYP-66 (T-PUBLISH-REQUEST), TYP-70 (T-HTTP-API) | guide components/app.md |
| F-APP-SERVER-RAW-SOURCE | Raw source | product.md:191 | FEA-SF-14 (F-APP-SERVER-REPORT-PAGES) | guide components/app.md |
| F-APP-SERVER-STATIC-SITE | Static site and aliases | product.md:192 | ARC-24 (D-SITE-SPLIT), FEA-SF-20 (F-APP-CLI-SERVER) | guide components/app.md |
| F-APP-STORE-IDENTIFIERS | Slug and version rules | product.md:204 | FEA-SF-54 (F-REPORT-STORE-BEHAVIOUR), ARC-21 (D-STORE-MODEL), ARC-32 (C-APP-REPORT-STORE), TYP-60 to TYP-64, TYP-96 (D-TYP-LATEST) | guide components/app.md |
| F-APP-STORE-IMMUTABLE-VERSIONS | Immutable versions | product.md:201 | FEA-SF-54 (F-REPORT-STORE-BEHAVIOUR), ARC-21 (D-STORE-MODEL), ARC-32 (C-APP-REPORT-STORE), TYP-60 to TYP-64, TYP-96 (D-TYP-LATEST) | guide components/app.md |
| F-APP-STORE-LATEST-POINTER | Latest pointer | product.md:202 | FEA-SF-54 (F-REPORT-STORE-BEHAVIOUR), ARC-21 (D-STORE-MODEL), ARC-32 (C-APP-REPORT-STORE), TYP-60 to TYP-64, TYP-96 (D-TYP-LATEST) | guide components/app.md |
| F-APP-STORE-RETENTION | Version retention | product.md:203 | FEA-SF-54 (F-REPORT-STORE-BEHAVIOUR), ARC-21 (D-STORE-MODEL), ARC-32 (C-APP-REPORT-STORE), TYP-60 to TYP-64, TYP-96 (D-TYP-LATEST) | guide components/app.md |
| F-BENCH-WORKLOADS | Render benchmark | product.md:351 | ARC-47 (C-RICHVIEW-BENCH), PRF-07 (P-FIXTURES), PRF-10 (P-NATIVE-RENDER) | guide components/build.md |
| F-BUILD-NATIVE | Native binaries | product.md:346 | ARC-46 (C-RICHVIEW-BUILD), ARC-04 (D-WASM-BOUNDARY) | guide components/build.md |
| F-BUILD-NPM | Package contents | product.md:348 | ARC-48 (C-PKG-CORE), ARC-49 (C-PKG-CHARTS), FEA-SF-49 (F-PKG-CORE-API) | guide components/build.md |
| F-BUILD-REPORTS | Report staging | product.md:350 | ARC-46 (C-RICHVIEW-BUILD), FEA-SF-46 (F-SITE-ROUTES), FEA-SF-40 (F-WEB-SHOWCASE-CHART-REVIEW), ARC-64 (C-BENCH-LABS) | guide components/build.md |
| F-BUILD-SITE | Site staging | product.md:349 | ARC-46 (C-RICHVIEW-BUILD), FEA-SF-46 (F-SITE-ROUTES), FEA-SF-40 (F-WEB-SHOWCASE-CHART-REVIEW), ARC-64 (C-BENCH-LABS) | guide components/build.md |
| F-BUILD-WASM | Wasm bundle | product.md:347 | ARC-46 (C-RICHVIEW-BUILD), ARC-04 (D-WASM-BOUNDARY) | guide components/build.md |
| F-CF-INTEREST-SIGNUP | Update signup endpoint | product.md:362 | ARC-55 (C-CLOUDFLARE-TEASER), FEA-SF-31 (D-SIGNUP), FEA-SF-46 (F-SITE-ROUTES) | guide components/cloud.md |
| F-CF-RATE-LIMIT | Signup rate limit | product.md:363 | ARC-55 (C-CLOUDFLARE-TEASER), FEA-SF-31 (D-SIGNUP), FEA-SF-46 (F-SITE-ROUTES) | guide components/cloud.md |
| F-CF-STATIC-SITE | richview.ai static site | product.md:361 | ARC-55 (C-CLOUDFLARE-TEASER), FEA-SF-31 (D-SIGNUP), FEA-SF-46 (F-SITE-ROUTES) | guide components/cloud.md |
| F-CHART-BROWSER-API | Chart view API | product.md:95 | ARC-25 (C-RICHVIEW-CHART), ARC-36 (C-WEB-WASM-API), TYP-97 (T-REPORT-RUNTIME-API) | guide components/chart.md |
| F-CHART-BUBBLE | Bubble sizing | product.md:73 | FEA-CH-07 (F-CHART-SCATTER) | guide components/chart.md |
| F-CHART-CLI | `richview-chart` binary | product.md:96 | ARC-25 (C-RICHVIEW-CHART), ARC-19 (D-CHART-PIPELINE) | guide components/chart.md |
| F-CHART-SOURCE-CSV | Original-data CSV | product.md:91 | FEA-CH-48 (F-WEB-CHART-CSV-DOWNLOAD), TYP-36 (D-TYP-DATA-PROJECTION) | guide components/chart.md |
| F-CI-GATES | CI gates | product.md:381 | ARC-50 (C-CI), DEL-39 (G-HOSTED-CI) | guide components/build.md |
| F-CI-PRERELEASE | GitHub prerelease | product.md:382 | OPN-28 (D-PRERELEASE) | guide components/build.md |
| F-CORE-ASSET-REFERENCE | Integrity-pinned asset reference type | product.md:140 | TYP-41 (D-TYP-ASSET-REF), ARC-34 (C-APP-ASSET-UPLOAD) | guide components/core.md |
| F-CORE-AUTHORING-TABLES | Authoring vocabulary | product.md:142 | ARC-09 (D-SINGLE-SOURCE), TYP-03 (D-TYP-VOCABULARIES) | guide components/core.md |
| F-CORE-BLOCK-DATA | Block-local data | product.md:115 | TYP-35 to TYP-38, TYP-92 (D-TYP-DATA-FORMAT-SNIFF) | guide components/core.md |
| F-CORE-BUILTIN-STYLES | Builtin styles | product.md:130 | TYP-52 (T-STYLE-BUILTINS), STY-05 (D-STYLE-CATALOG), STY-07 (S-STYLE-NAMES) | guide components/core.md |
| F-CORE-CHART-VALIDATION | Chart fence schema | product.md:114 | TYP-12 to TYP-17 | guide components/core.md |
| F-CORE-CODE-HIGHLIGHT | Code highlighting | product.md:125 | FEA-RP-17 (F-CODE-BLOCKS) | FEA-RP-17 conflicts; guide components/core.md |
| F-CORE-COMPAT-API | Compatibility API | product.md:141 | ARC-26 (C-RICHVIEW-CORE), ARC-36 (C-WEB-WASM-API), TYP-75 (T-JS-API) | guide components/core.md |
| F-CORE-DIAGNOSTICS | Diagnostic contract | product.md:135 | TYP-53 to TYP-56 | guide components/core.md |
| F-CORE-FONTS | Bundled fonts | product.md:131 | STY-27 to STY-29, ARC-62 (C-FONT-PIPELINE) | guide components/core.md |
| F-CORE-FRONTMATTER | Document metadata | product.md:111 | TYP-06 (T-FRONTMATTER), TYP-84 (D-TYP-FRONTMATTER-KEYS), TYP-07 (D-TYP-DESCRIPTION), TYP-08 (T-REPORT-LANG) | guide components/core.md |
| F-CORE-LIMITS | Size limits | product.md:136 | TYP-43 (T-RESOURCE-LIMITS) | guide components/core.md |
| F-CORE-NO-HEADER-DATA | Header data removed | product.md:112 | TYP-35 (T-DATA-SOURCE) | guide components/core.md |
| F-CORE-PARSE | Rich Markdown parsing | product.md:110 | TYP-01 (T-RMD-DOCUMENT), TYP-02 (T-YAML-DIALECT), TYP-10 (D-TYP-UNKNOWN-KEYS) | guide components/core.md |
| F-CORE-PUBLISH-VALIDATION | Publication checks | product.md:137 | TYP-07 (D-TYP-DESCRIPTION), TYP-60 (T-REPORT-SLUG), TYP-62 (T-REPORT-VERSION) | guide components/core.md |
| F-CORE-REPORT-FOLDER | Report-folder contract | product.md:138 | TYP-39 (T-REPORT-FOLDER), TYP-42 (T-RENDERED-OUTPUT) | guide components/core.md |
| F-CORE-RICH-TAGS | Nine rich fence tags | product.md:113 | TYP-09 (T-RICH-FENCE) | guide components/core.md |
| F-CORE-RUNTIME-EMBED | Embedded interactive runtime | product.md:127 | ARC-06 (D-RUNTIME-SPLIT), ARC-07 (D-RUNTIME-EMBED), PRF-31 (P-RUNTIME-DELIVERY), TYP-74 (T-CHART-RUNTIME-PAYLOAD) | guide components/core.md |
| F-CORE-WIKILINKS-QUOTES | Wikilinks and pull quotes | product.md:123 | FEA-RP-12 (F-WIKILINKS), FEA-RP-13 (F-PULL-QUOTE) | guide components/core.md |
| F-DEPLOY-CONTAINER | Server image | product.md:379 | ARC-56 (C-CONTAINER-DEPLOY), OPN-05 (D-FLY-LEGACY) | guide components/cloud.md |
| F-DEPLOY-FLY | Fly app definition | product.md:380 | ARC-56 (C-CONTAINER-DEPLOY), OPN-05 (D-FLY-LEGACY) | guide components/cloud.md |
| F-DEVHARNESS-ARCHITECTURE-MAP | Current-architecture document | product.md:461 | ARC-61 (C-DEV-HARNESS), FEA-BR-20 (F-DEVHARNESS-RUNTIME-DRILL), FEA-BR-47 (G-WORKER-REPLY-HOLD), OPN-12 (D-HARNESS-PR68) | guide components/build.md |
| F-DEVHARNESS-BUILD-OWNERSHIP | Shared-target build ownership | product.md:460 | ARC-61 (C-DEV-HARNESS), FEA-BR-20 (F-DEVHARNESS-RUNTIME-DRILL), FEA-BR-47 (G-WORKER-REPLY-HOLD), OPN-12 (D-HARNESS-PR68) | guide components/build.md |
| F-DEVHARNESS-JOURNEY | End-to-end journey | product.md:459 | ARC-61 (C-DEV-HARNESS), FEA-BR-20 (F-DEVHARNESS-RUNTIME-DRILL), FEA-BR-47 (G-WORKER-REPLY-HOLD), OPN-12 (D-HARNESS-PR68) | guide components/build.md |
| F-DOCS-HIGHLIGHT | Highlighted code samples | product.md:317 | ARC-43 (C-DOCS-SITE) | guide components/web.md |
| F-DOCS-REFERENCE | Format reference | product.md:315 | FEA-SF-41 (F-DOCS-GUIDES), TYP-04 (D-TYP-DOC-DRIFT) | guide components/web.md |
| F-EDITOR-COMPLETION | Completion | product.md:327 | FEA-SF-25 (F-VSCODE-PROVIDERS) | guide components/editor.md |
| F-EDITOR-DIAGNOSTICS | Live diagnostics | product.md:326 | FEA-SF-24 (F-VSCODE-LINT), ARC-08 (D-ONE-OPERATION-LAYER) | guide components/editor.md |
| F-EDITOR-EXPORT-PUBLISH | Export and publish | product.md:332 | FEA-SF-27 (F-VSCODE-EXPORT-VALIDATE), FEA-SF-12 (F-APP-CLI-PUBLISH) | guide components/editor.md |
| F-EDITOR-FORMAT | Formatting | product.md:328 | FEA-SF-25 (F-VSCODE-PROVIDERS) | guide components/editor.md |
| F-EDITOR-OUTLINE | Outline and folding | product.md:329 | FEA-SF-25 (F-VSCODE-PROVIDERS) | guide components/editor.md |
| F-EDITOR-PREVIEW | Preview render | product.md:331 | FEA-SF-23 (F-VSCODE-PREVIEW) | guide components/editor.md |
| F-EDITOR-RPC | JSON-RPC over stdio | product.md:325 | ARC-44 (C-RICHVIEW-EDITOR), TYP-98 (T-EDITOR-PROTOCOL) | guide components/editor.md |
| F-EDITOR-SNIPPETS | Snippets | product.md:330 | FEA-SF-26 (F-VSCODE-INSERT-BLOCKS) | guide components/editor.md |
| F-EXAMPLES-FLAGSHIP | Flagship report | product.md:371 | ARC-54 (C-EXAMPLES), INT-55 (D-EXAMPLES-CARRY), FEA-SF-29 (F-WEB-LANDING-LIVE-DEMO), FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-42 (F-DOCS-TUTORIAL-EXAMPLE) | guide components/web.md |
| F-EXAMPLES-HOMEPAGE-DEMO | Homepage demo source | product.md:373 | ARC-54 (C-EXAMPLES), INT-55 (D-EXAMPLES-CARRY), FEA-SF-29 (F-WEB-LANDING-LIVE-DEMO), FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-42 (F-DOCS-TUTORIAL-EXAMPLE) | guide components/web.md |
| F-EXAMPLES-TUTORIALS | Example set | product.md:372 | ARC-54 (C-EXAMPLES), INT-55 (D-EXAMPLES-CARRY), FEA-SF-29 (F-WEB-LANDING-LIVE-DEMO), FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-42 (F-DOCS-TUTORIAL-EXAMPLE) | guide components/web.md |
| F-GALLERY-CHARTS | `/chart-gallery/` | product.md:424 | ARC-60 (C-GALLERIES), FEA-BR-12 (D-FAMOUS-SIMPLIFICATIONS), FEA-BR-41 (D-DOCS-CHART-EXAMPLES), OPN-19 (D-GALLERY-ROUTES) | guide components/web.md |
| F-GALLERY-FAMOUS | `/famous-charts/` | product.md:425; the Linear evidence (not published) | ARC-60 (C-GALLERIES), FEA-BR-12 (D-FAMOUS-SIMPLIFICATIONS), FEA-BR-41 (D-DOCS-CHART-EXAMPLES), OPN-19 (D-GALLERY-ROUTES) | guide components/web.md |
| F-LLMS-TXT-SUMMARY | llms.txt | product.md:370 | ARC-53 (C-LLMS-TXT), FEA-SF-48 (F-SKILL-AGENT-GUIDE) | guide components/build.md |
| F-PKG-CHARTS-API | `@richview/charts` | product.md:353 | ARC-48 (C-PKG-CORE), ARC-49 (C-PKG-CHARTS), FEA-SF-49 (F-PKG-CORE-API) | guide components/build.md |
| F-PUBLISH-MEMBER-EDITION | Member edition | product.md:480 | FEA-BR-25 (F-PUBLISH-ACCESS), FEA-BR-45 (F-PUBLISH-MODE) | guide components/cloud.md |
| F-PUBLISH-NEWS-SITE | RichView News | product.md:483 | INT-74 (D-NEWS-DEMO), OPN-03 (D-NEWS-LIVE) | guide components/cloud.md |
| F-PUBLISH-SITE-CONFIG | Publication config | product.md:477 | FEA-BR-23 (F-PUBLISH-LAYOUTS-THEMES), TYP-81 (T-SITE-CONFIG) | guide components/cloud.md |
| F-VSCODE-BINARY-TRUST | Native binary and trust | product.md:340 | FEA-SF-21 (D-VSCODE) | guide components/editor.md |
| F-VSCODE-PUBLISH | Publish to server | product.md:338 | FEA-SF-12 (F-APP-CLI-PUBLISH), FEA-SF-51 (F-PUBLISH-NEW-ADDRESS) | guide components/editor.md |
| F-WEB-CHART-SPEC-CONTROLS | Range and transform controls | product.md:249 | FEA-CH-16 (F-CHART-TRANSFORMS) | guide components/web.md |
| F-WEB-CHARTS-EDITOR | "Edit this chart" | product.md:285 | FEA-SF-44 (F-WEB-CHARTS-EXAMPLES), ARC-40 (C-WEB-CHART-EXAMPLES) | guide components/web.md |
| F-WEB-CHARTS-THEME-BUTTONS | Theme buttons | product.md:286 | FEA-SF-44 (F-WEB-CHARTS-EXAMPLES), ARC-40 (C-WEB-CHART-EXAMPLES) | guide components/web.md |
| F-WEB-LANDING-NAV-SIMPLIFIED | Simplified nav | product.md:468 | FEA-SF-28 (D-SITE-NAV) | guide components/web.md |
| F-WEB-LANDING-PAGE | Homepage | product.md:274 | FEA-SF-56 (F-SITE-HOME-SECTIONS), ARC-39 (C-WEB-LANDING), STY-44 (D-LANDING-MOTION), FEA-BR-21 (F-WEB-LANDING-MOTION-RESTORED) | guide components/web.md |
| F-WEB-LANDING-SCROLL-STAGE | Scroll choreography | product.md:277 | FEA-SF-56 (F-SITE-HOME-SECTIONS), ARC-39 (C-WEB-LANDING), STY-44 (D-LANDING-MOTION), FEA-BR-21 (F-WEB-LANDING-MOTION-RESTORED) | guide components/web.md |
| F-WEB-LANDING-SIGNUP | Update signup form | product.md:276 | FEA-SF-31 (D-SIGNUP), ARC-55 (C-CLOUDFLARE-TEASER) | guide components/web.md |
| F-WEB-PAGE-BOOT | Page boot | product.md:241 | ARC-36 (C-WEB-WASM-API), TYP-75 (T-JS-API), TYP-97 (T-REPORT-RUNTIME-API) | guide components/web.md |
| F-WEB-PLAYGROUND-CHECK | Check button | product.md:265 | FEA-SF-34 (F-WEB-PLAYGROUND-LIVE-RENDER), ARC-66 (D-BROWSER-SOURCES) | guide components/web.md |
| F-WEB-PLAYGROUND-EXPORT | Copy and downloads | product.md:266 | FEA-SF-34 (F-WEB-PLAYGROUND-LIVE-RENDER), ARC-66 (D-BROWSER-SOURCES) | guide components/web.md |
| F-WEB-PLAYGROUND-LARGE-SOURCE | Bounded editing windows | product.md:268 | FEA-SF-34 (F-WEB-PLAYGROUND-LIVE-RENDER), ARC-66 (D-BROWSER-SOURCES) | guide components/web.md |
| F-WEB-PLAYGROUND-LOAD-SOURCE | Load by URL or example | product.md:267 | FEA-SF-34 (F-WEB-PLAYGROUND-LIVE-RENDER), ARC-66 (D-BROWSER-SOURCES) | guide components/web.md |
| F-WEB-PLAYGROUND-STYLE-SELECT | Style selector | product.md:264 | FEA-SF-34 (F-WEB-PLAYGROUND-LIVE-RENDER), ARC-66 (D-BROWSER-SOURCES) | guide components/web.md |
| F-WEB-REEL-EXPORT | Reel exporter | product.md:308 | FEA-SF-45 (D-REEL), ARC-42 (C-WEB-REEL) | guide components/web.md |
| F-WEB-REEL-PLAYER | `/reel/` player | product.md:307 | FEA-SF-45 (D-REEL), ARC-42 (C-WEB-REEL) | guide components/web.md |
| F-WEB-SHOWCASE-COMPARE | Side-by-side compare | product.md:294 | FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-37 (D-SHOWCASE-CONTROLS), STY-05 (D-STYLE-CATALOG) | guide components/web.md |
| F-WEB-SHOWCASE-KEYBOARD | Keyboard control | product.md:297 | FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-37 (D-SHOWCASE-CONTROLS), STY-05 (D-STYLE-CATALOG) | guide components/web.md |
| F-WEB-SHOWCASE-SHARE-EMBED | Share and embed | product.md:296 | FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-37 (D-SHOWCASE-CONTROLS), STY-05 (D-STYLE-CATALOG) | guide components/web.md |
| F-WEB-SHOWCASE-STYLE-DRAWER | Style drawer | product.md:298 | FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-37 (D-SHOWCASE-CONTROLS), STY-05 (D-STYLE-CATALOG) | guide components/web.md |
| F-WEB-SHOWCASE-THEMES | 23 visual treatments | product.md:293 | FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-37 (D-SHOWCASE-CONTROLS), STY-05 (D-STYLE-CATALOG) | guide components/web.md |
| F-WEB-SHOWCASE-TOUR | Auto tour | product.md:295 | FEA-SF-36 (F-WEB-SHOWCASE-FLAGSHIP), FEA-SF-37 (D-SHOWCASE-CONTROLS), STY-05 (D-STYLE-CATALOG) | guide components/web.md |
| F-WEB-WASM-CHART-API | Chart functions in JS | product.md:237 | ARC-36 (C-WEB-WASM-API), TYP-75 (T-JS-API), TYP-97 (T-REPORT-RUNTIME-API) | guide components/web.md |
| F-WEB-WASM-RENDER-API | Render functions in JS | product.md:236 | ARC-36 (C-WEB-WASM-API), TYP-75 (T-JS-API), TYP-97 (T-REPORT-RUNTIME-API) | guide components/web.md |
| F-WEB-WASM-STYLE-LEARN-API | Style and learn functions | product.md:238 | ARC-36 (C-WEB-WASM-API), TYP-75 (T-JS-API), TYP-97 (T-REPORT-RUNTIME-API) | guide components/web.md |
| F-WEB-WORKER-RENDER | Off-main-thread rendering | product.md:239 | ARC-36 (C-WEB-WASM-API), TYP-75 (T-JS-API), TYP-97 (T-REPORT-RUNTIME-API) | guide components/web.md |
