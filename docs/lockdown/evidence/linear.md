# Evidence: Linear, project RichView (workspace 935)

Mined 2026-10-21 for the RichView lockdown (S27-677). Read-only: no Linear issue, comment, status, relation or project was created or changed.

## Scope, method and how to read this file

- Workspace check: `get_workspace` returned `935`, Daniel's personal Linear workspace; `S27-677` resolves to project RichView (`P-S27-32`, team Life, key S27). [observed]
- Read in full: the project record, all 65 issues in the project (none archived), every description, every comment, relations, state history and attachment metadata. The project has no milestones, documents or status updates. [observed]
- Primary range: S27-572 to S27-655 plus S27-677, 28 issues. The 37 older issues in the project (S27-198, S27-398 to S27-433) arrived in the 2026-10-12 migration from an earlier workspace; they are summarized in the older-issues appendix because several are parents of, or decisions behind, the primary range. Identifiers in the range that are not listed belong to other projects: a workspace search for "RichView" finds no other RichView issue in the range, and S27-617 explicitly says not to substitute RichView. [observed]
- Cross-checks run on 2026-10-21: PR states with `gh pr list -R 1vecera/richview` (the repo was transferred from `richview-universe/richview-v3`); code on `origin/main` `b4db233` in this worktree; read-only GETs of `richview.ai` and `news.richview.ai`. [observed]
- Tags: [observed] verified now in code, git/GitHub or a live GET; [stated] written in Linear (or a PR body) and not verified here; [inferred] my reading across sources.
- Proposed entity IDs (`F-`, `C-`, `T-`, `S-`, `P-`, `G-`, `D-`) appear only where an issue defines a clear entity; the proposed-IDs appendix lists them for reconciliation with the other evidence files.
- Linear state names are written without their emoji suffixes (Que, Brain Dump).

### Who wrote what

- Issue descriptions were written by Daniel's coordinator agent from his dictation, voice notes, Notion edits and live answers. Dated sections inside them ("Daniel's review — corrections requested, 18 October 2026 16:03 Prague", S27-653) are the record of his reviews. Everything attributed to Daniel below is [stated] in that sense: his intent as relayed by the coordinator. Quotation marks mark words the issue itself presents as his.
- Comments imported on 2026-10-12 all show author "Daniel Vecera", but their content shows agent handoffs (pickup notes naming the executing host, "# S27-572 result…") and bot triage ("PM RichView triage"). After the migration, executors post as dedicated Claude and Codex app accounts, and supervisor or monitor checkpoints post under Daniel's account. No comment in the project is verifiably Daniel's own words. [observed: author fields and content; [inferred]: authorship]

## Project record (P-S27-32)

- Summary: "Focused August launch: lock architecture, polish the narrow product, and publish a clear try path." Status In Progress, priority Urgent, start 2026-04-02, target 2026-08-31 (passed), no lead or members. [observed]
- Description "Current reset · August 8, 2026": all earlier open execution moved to Icebox; old-workspace VEN-4470 (S27-401 here [inferred]) is the single launch surface; sequence: understand the architecture and lock a narrow scope, polish one end-to-end example, publish positioning and a try path by 31 August; "Operating rule: finish the focused release before reopening Icebox work." Product context: "RichView is an AI-native report and visualization product." [stated]
- The description was never updated after August; the 17 and 18 October briefs say they override "older August scope locks" (see Conflicts, scope). [observed]

## Issue index (primary range)

States as of 2026-10-21. PR states observed with `gh` on 2026-10-21.

| ID | Title | Linear state | Created | Last update | Parent / executor | PRs (observed state) |
| --- | --- | --- | --- | --- | --- | --- |
| S27-572 | Build a new Rust charting library and exceptional time-series showcase | To Review | 2026-10-02 | 2026-10-12 | Claude | #9 merged 2026-10-04 while draft |
| S27-573 | Repair the RichView flagship report and theme switching | To Review | 2026-10-02 | 2026-10-12 | Claude, then Codex | #8 merged 2026-10-04 while draft; #19 merged 2026-10-11 |
| S27-577 | Co-work on getting the RichView.ai teaser ready to launch | To Review | 2026-10-04 | 2026-10-12 | parent S27-401; Codex cowork | #12, #13 merged 2026-10-04 |
| S27-578 | RichView Feedback | To Review | 2026-10-04 | 2026-10-12 | Codex | #15 merged 2026-10-05 |
| S27-579 | Keep chart data in chart-local YAML and default small inputs to inline | Que | 2026-10-04 | 2026-10-12 | parent S27-578 | delivered in #15 |
| S27-580 | Improve YAML table defaults and compare readable series syntax | Que | 2026-10-04 | 2026-10-12 | parent S27-578 | defaults in #15; syntax decision pending |
| S27-581 | Require a dedicated folder for every RichView report and fail fast otherwise | Que | 2026-10-04 | 2026-10-12 | parent S27-578 | delivered in #15 |
| S27-582 | Iterate RichView charts to Highcharts visual quality using 100 gallery references | To Review | 2026-10-04 | 2026-10-12 | Codex | #14 merged 2026-10-05 |
| S27-590 | Revise RichView coming-soon homepage and add Cloudflare email signup | To Review | 2026-10-05 | 2026-10-18 | parent S27-577; Codex | #20 merged 2026-10-11 while draft |
| S27-591 | Generate tested RichView docs and publish them on richview.ai | To Review | 2026-10-05 | 2026-10-12 | parent S27-401; Codex | #18 merged 2026-10-05 |
| S27-593 | Create RichView coming-soon LinkedIn carousel and post with Claude Opus | To Review | 2026-10-05 | 2026-10-12 | Claude | no PR (local design assets) |
| S27-594 | Restore RichView visual motion and polish the live source-to-report experience | To Review | 2026-10-05 | 2026-10-18 | parent S27-590; Claude | #21 merged 2026-10-11 |
| S27-616 | RichView AI: elevate report design, animated charts, exports, storytelling and hosting | Working on | 2026-10-10 | 2026-10-17 | Claude design loop | #22 merged 2026-10-11; rest of scope unstarted |
| S27-632 | RichView · chart expand/contract affordance on subtitle row (richview-web) | Brain Dump | 2026-10-13 | 2026-10-18 | none | implemented by #60, merged 2026-10-16 |
| S27-633 | RichView · perfect left ink alignment of chart title, axis and first bar (richview-chart) | Brain Dump | 2026-10-13 | 2026-10-18 | none | implemented by #60 |
| S27-634 | RichView · soft rounding on bars + bar/zero-line offset + remove entrance tick (richview-chart) | Brain Dump | 2026-10-13 | 2026-10-13 | none | implemented by #60 |
| S27-635 | RichView · refine line curvature (richview-chart) | Brain Dump | 2026-10-13 | 2026-10-18 | none | implemented by #60 |
| S27-637 | RichView · better imagery for OpenAI vs Anthropic flagship (asset research + swap) | Brain Dump | 2026-10-13 | 2026-10-18 | none | implemented by #60 |
| S27-647 | Publish RichView typography, chart and famous-chart galleries | To Review | 2026-10-17 | 2026-10-18 | Claude | #65 open draft (deployed from branch) |
| S27-648 | Add playable charts and beautiful chart-version selectors | To Review | 2026-10-17 | 2026-10-18 | Claude | #62 open draft |
| S27-649 | Add first-class multi-tab RichView reports | To Review | 2026-10-17 | 2026-10-18 | Claude | #63 open draft |
| S27-650 | Shipped SQL + MetricFlow + cached-data connectors behind richview --connectors (PR #66) | To Review | 2026-10-17 | 2026-10-18 | Claude | #66 open draft |
| S27-651 | Build a voice-to-dashboard RichView sibling-product demo | To Review | 2026-10-17 | 2026-10-18 | Claude | #64 open draft |
| S27-652 | RichView: audit project history and implement an end-to-end development harness | To Review | 2026-10-18 | 2026-10-18 | Codex | #68 open draft |
| S27-653 | Restore richview.ai motion and simplify navigation; move chart gallery into docs | Working on | 2026-10-18 | 2026-10-19 | Claude | #69 open draft (deployed from branch) |
| S27-654 | Polish RichView flagship and chart labels, ticks, tooltips, downloads and fullscreen | Working on | 2026-10-18 | 2026-10-19 | Claude | #67 open draft, unfinished |
| S27-655 | Build RichView Publish and launch Git-managed RichView News | To Review | 2026-10-18 | 2026-10-19 | Codex | #70 open draft; content PRs #71, #72 merged into `publish/news` |
| S27-677 | Lock down RichView and write its guiding documents | Working on | 2026-10-21 | 2026-10-21 | this lockdown | none yet |

Sources: Linear `list_issues`/`get_issue` for each ID; `gh pr list -R 1vecera/richview --state all`; `gh pr list --search S27-6xx` (PR #60 body names S27-632, 633, 634, 635 and 637). [observed]

## Daniel's review and dictation record

Chronological. Each line is [stated] in the cited issue; the voice is the coordinator's summary of Daniel unless quoted.

| Date | Issue | What Daniel asked, decided or judged |
| --- | --- | --- |
| 2026-06-20 | S27-410 | Daniel's own description: move charting to a separate repo and revert to ECharts as the chart supplier: "I want to simplify things to reduce the scope of handling charting later on". |
| 2026-06-20 | S27-420, S27-419, S27-416 | Voice dump: RichView is "agented data storytelling" / an "agented presentation layer"; "we are not doing all kinds of reports; we are doing data stories"; core stays small (Rich Markdown, styling layer, renderer/CLI, server, VS Code); web surface is a playground, "let's not make any chatting in the face there". |
| 2026-06-20 | S27-417, S27-418, S27-413, S27-412, S27-414, S27-415 | A report is two kinds of files: `.rmd` content plus cascading style files (later wins, linked by URL or name); required `title`, `description`, `version` from `0.0.1`; title picture, prolog, charts, labels, audio, video, images, tables; server hosts full HTML first with versions, dedupe runtime later; VS Code grows from highlighting to "add chart" and chart-block commands; minimum-code interactivity; humans and agents collaborate on charts through text. |
| 2026-07-07 | S27-198 comment | RichView "still feels important but elusive"; get a basic version released. |
| 2026-08-08 | S27-401, DIR-121 | Life-review voice note: launch a focused product by 31 August; RichView is "a curated JSON/data-to-report rendering layer with strong components at hand", not a sprawling platform; one clear path to understand and try it. |
| 2026-10-02 | S27-572, S27-573, S27-398, S27-399 | Kickoff dictation and Notion edits: new Rust chart library judged against Highcharts; competition-winning flagship report (OpenAI vs Anthropic, 80% analysis, 20% presentation); theme-switching showcase of his existing themes; scrolling website with a 15-second motion reel ("make a dynamic 15-second motion graphics video that shows what an incredible motion designer you are… go all out"). |
| 2026-10-04 | S27-430, S27-577 | Test richview.ai; evening review: the teaser "does not meet Daniel's expectations yet"; he authorizes "merge and deploy everything" and selects Cloudflare (S27-577 comment). |
| 2026-10-04 | S27-578, S27-579, S27-580, S27-581, S27-582, S27-412 | Authoring feedback: chart data belongs inside the chart's YAML, never the header; table source is unreadable, tables "maybe [as] series" (tentative); a new VS Code extension version with full linting, formatting and suggestions; every report in its own folder and it should "crash" otherwise; output in `Rendered`; optional styling YAML; verify that theme changes really work; iterate to Highcharts quality with at least 100 gallery pictures, naming zoom and tooltips. Phrases "Escapism" and "Where to Render" stayed unclarified. |
| 2026-10-04 night | S27-430 | RichView and its launch site are "much rougher than expected"; showcase PRs "far from a finished product"; clean up into a usable preview, then LinkedIn posts; polish reference "Mlouva" (identity unconfirmed). |
| 2026-10-05 | S27-573, S27-590, S27-591, S27-430 | Flagship "does not work"; Daniel chose "Clean up the existing repo (recommended)" over a fresh repo; authorizes public docs and flagship publication; homepage review (positioning, source beside report, five elements, "Coming soon" never "beta", Cloudflare signup). |
| 2026-10-05 | S27-593 | Approves a seven-slide "coming soon" storyboard; awareness only, nothing posted. |
| 2026-10-05 | S27-594 | Likes the new story and editor "because they explain the product", but the visuals lost the earlier seamless transitions, background narrative and logo choreography; Sleek default; "RichView will be fully open source" is his product direction; happy the original flagship is back. |
| 2026-10-05 | S27-591 comment | Explicit "merge and publish" for the docs redesign (PR #18). |
| 2026-10-10 | S27-616 | Product brief: exceptional report design, five themes including a new hand-drawn one, hero imagery, storytelling marks, Highcharts/ECharts-grade motion, readable PDF and LinkedIn carousel PDF, output markers, scrollytelling, data and media hosting, AWS hosting and startup credits. "Fully interactive" was immediately clarified to animation quality; "Sleek black" means Sleek dark; "the right number of lines". |
| 2026-10-12 to 13 | S27-632 to S27-637 | Dictated chart feedback: Expand/Contract per chart; ink alignment of title, axis and first bar ("ink-vs-box problem"); soft bar rounding, bars clear of the zero line, no entrance "ticking"; curves look "dodgy"; flagship imagery weak. |
| 2026-10-17 | S27-647 to S27-651 | New tracks "despite older August scope locks": typography, chart and famous-chart galleries (published immediately); playable charts and chart-version selectors; multi-tab reports; SQL, dbt MetricFlow and cache connectors; voice-to-dashboard sibling demo. |
| 2026-10-18 | S27-652, S27-653, S27-654 | History audit plus a development harness; restore richview.ai motion, cut buttons and CTAs, move the chart gallery into docs: "all the changes on the richview.ai should be published right away and pushed to completion, because the current state is very unsatisfactory"; flagship and chart corrections (one measure, header infographic, five styles on top, PNG/SVG download, drop expand for one modal, tick density, years as years, sparse labels, lighter smooth tooltips). |
| 2026-10-18 | S27-655 | RichView Publish (final name, corrected from "RichView Publishing") and RichView News at news.richview.ai, FT.com-level editorial completeness for a small team, Git-managed. |
| 2026-10-18 16:03 | S27-653, S27-654 | Review: the published site "is not accepted yet"; mobile scroll animations "look jumpy/weird with visible ticking"; the docs chart gallery must visibly render real RichView charts with source and working interactions for all nine types; "do not assume 'ticking' means axis tick labels". |
| 2026-10-21 | S27-677 | Rewrite RichView; agents merge freely into `dev`, only Daniel merges `dev` into `prod`, no releases; he reviews only design-quality and feature decisions in batches; the lockdown is the exact reviewed description that makes this safe. |

## Product intent and positioning

### Positioning statements over time

| Date | Statement | Source | Tag |
| --- | --- | --- | --- |
| 2026-04-10 | "position as agentic layer for data-driven reports (not more, not less)" (title only; "RidgeView" is apparently a transcription of RichView [inferred]) | S27-427 | stated |
| 2026-06-08 | Keep the `.rmd` core; the one material new thing is a self-validating chart library: the agent renders a chart, gets machine-readable readability checks (overlap, collision, occlusion, clipping) and fixes it before a human sees it; focus on who uses it and why, park the business model | S27-423 comment | stated |
| 2026-06-20 | "an agented presentation layer for data stories where humans and agents collaborate on `.rmd` stories, chart styling, review, and sharing"; "data stories for coding-agent users, not dashboards, not generic BI, not a full chat/web-editor product" | S27-420, S27-419 | stated |
| 2026-08-08 | "a curated JSON/data-to-report rendering layer with strong components at hand"; "rather than a sprawling platform" | S27-401, DIR-121 | stated |
| 2026-10-02 | Win over "analysts and developers using coding agents" with charts and reports they want to inspect, reuse and discuss; explain "why RichView exists and its JSON-renderer inspiration" | S27-572, S27-398 | stated |
| 2026-10-05 | "RichView is a presentation layer for your agent."; "an engine for humans and agents that turns text and data into beautiful reports"; "Your agent writes the text. RichView renders the story."; "Coming soon", never "beta" | S27-590 | stated; landing `<title>` "RichView — a presentation layer for your agent" and "Coming soon." [observed: `assets/landing/index.html:6`, `:349`] |
| 2026-10-05 | "a presentation layer for agent analysis: text and data become beautiful, readable, consistent, shareable reports"; brought back "as an open-source endeavor, using Rust"; slide 1 must say "Open source · Built with Rust · Coming soon" | S27-593 | stated |
| 2026-10-05 | "RichView will be fully open source" (direction); describe honestly that the source is not public yet | S27-594 | stated; landing copy "RichView will be fully open source… It is not public yet." [observed: `assets/landing/index.html:368-369`] |
| 2026-10-10 | Reports that "feel exceptionally well designed"; "Highcharts-level visual craft in RichView's own rendering system, paired with excellent exports and straightforward, economical hosting for large datasets and media" | S27-616 | stated |
| 2026-10-17 | Make richview.ai "the strongest proof of how beautiful RichView can be"; a chart should feel like "an explorable, beautifully animated analytical object" | S27-647, S27-648 | stated |
| 2026-10-18 | Ecosystem: Rich Markdown is the authored format, Charts the rendering capability, Reports and Dashboards the publishable experiences, Publish the publication/editorial/access/deployment layer, Server existing hosting to reuse and maybe absorb, VS Code an authoring surface | S27-655 | stated |
| 2026-10-21 | Rewrite with fast agent development and an exact, reviewed product description | S27-677 | stated |

### Audience

- Coding-agent users as the single audience (S27-408 synthesis, 2026-06-20; S27-420). [stated]
- Analysts and developers using coding agents, for interest and feedback (S27-572, S27-573, S27-398, S27-582, 2026-10-02 to 04). [stated]
- Analysts and agent-assisted authors, their readers on desktop and mobile, and people discovering the work through LinkedIn carousels (S27-616, 2026-10-10). [stated]
- Someone dictating "Create a dashboard that shows how my sales are performing" (S27-651, 2026-10-17); authors used to dbt-oriented chart tools (S27-650). [stated]
- A small intelligence or specialist news publisher, "a small team and hundreds of items" (S27-655, 2026-10-18). [stated]

### What RichView maximizes (as stated)

- Analytical clarity and honesty: precise scales, dates and units, minimal ink, clear hierarchy; numerical correctness "non-negotiable" (S27-572). [stated]
- Visual and motion craft comparable to Highcharts and ECharts while keeping RichView's own identity: "equivalent craft rather than copied pixels" (S27-572, S27-582, S27-616). [stated]
- Immediate comprehension: the paired source and rendered report explain the product at first encounter (S27-590, S27-594). [stated]
- A short authoring path: source to chosen output, uploaded asset to working embed (S27-616); "expressive simplicity: short authoring syntax, excellent defaults" (S27-648). [stated]
- Truthfulness of claims and data provenance across every theme, frame, variant and export (S27-399, S27-573, S27-616, S27-648). [stated]
- Low, explainable operating cost and provider portability (S27-616). [stated]

### Product shape

- C-RICH-MARKDOWN, C-THEMES, C-RENDERER, C-SERVER, C-VSCODE: the "five elements" used on the homepage, docs and carousel: Rich Markdown; theming with existing and custom styling YAML; the rendering engine that produces beautiful HTML; a server for publishing; a VS Code extension for human-agent collaboration (S27-590, S27-591, S27-593, S27-594). MCP and CLI are "rendering interfaces where currently supported" (S27-593). [stated]
- C-CHART-ENGINE: Charts as a named capability and Rust library, `richview-chart/` (S27-572, S27-655). [stated; directory present [observed]]
- C-PUBLISH and C-DASHBOARDS: sibling products in the same monorepo, sharing the engine and styles; no fork of the renderer, no new repository (S27-651, S27-655). [stated]
- C-WEBSITE: richview.ai, the public teaser, docs, flagship and galleries (S27-577, S27-591, S27-647, S27-653); news.richview.ai belongs to C-PUBLISH (S27-655). [stated]

### Non-goals named in Linear

- No full web editor and no chat in the playground (S27-416, 2026-06-20); no WYSIWYG editor in Publish v1 (S27-655). [stated]
- Not dashboards, not generic BI (S27-420, 2026-06-20), later contradicted by S27-651 (see Conflicts). [stated]
- Publish v1 excludes editions, newsletters, e-paper/print, WYSIWYG, paywall, ad tech, comments, CRM, multi-tenancy, CMS marketplace (S27-655). [stated]
- No map projections or dashboards just to match the Highcharts corpus (S27-582); chart-local version selection only, dashboard-wide cross-selection belongs to the dashboard track (S27-648). [stated]

## Feature requests and acceptance criteria

Each row is the requirement as the issue states it, the source, and the status found on 2026-10-21.

### Authoring contract

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| G-REPORT-FOLDER | Every report lives in its own dedicated folder. Missing, shared or escaping report roots fail fast: nonzero CLI exit, structured host error naming the path and the fix, no partial render, no fallback to another folder or the working directory; the editor stays usable to fix it. Daniel's "crash" is read as fail-fast, not a panic. Data, style and media subfolders are allowed inside the root. | S27-581, S27-578 (2026-10-04) | Implemented by PR #15 [observed: PR merged; folder diagnostics in `crates/richview-core/src/folder.rs:206`]; Linear still Que |
| G-CHART-LOCAL-DATA | Data definitions and loading references live in the YAML of the chart or table that consumes them, never in frontmatter. Small data is inline by default; larger data uses a chart-local csv/json path inside the report folder. Header data produces an actionable diagnostic and migration guidance. This explicitly overrides the earlier compatibility preference. | S27-579 scope correction, S27-578, S27-412 (2026-10-04) | Implemented [observed: `crates/richview-core/src/parser.rs:265` "Report header data is no longer supported…"]; Linear still Que |
| G-RENDERED-OUTPUT | Output goes to a subfolder named exactly `Rendered`; default name is the source basename plus `.html`; CSS, fonts, chart runtime, data and media are embedded or copied with correct relative references; output-path and symlink escape are refused; verify the generated file itself, not a dev server. | S27-578, S27-581 (2026-10-04) | Implemented [observed: `crates/richview-app/src/resolver.rs:554`, `crates/richview-app/src/cli.rs:81`] |
| G-OPTIONAL-STYLE | An optional styling YAML sits beside the source; without it the report renders in the default style; no separate theme project. Earlier: one `.rmd` plus one or more style files that cascade, later wins, referenced by name or URL. | S27-578, S27-581 (2026-10-04); S27-417 (2026-06-20) | Implemented per the S27-578 handoff [stated]; spec section "Optional styling YAML" [observed: `docs/RMD-SPEC.md:249`] |
| F-TABLE-YAML, D-TABLE-SYNTAX | Tables default to a structured YAML `table` block with readable source. Compare row mappings with a series/column form on the same example, recommend one, and leave Daniel's choice pending; no breaking schema change. | S27-580, S27-578 (2026-10-04) | Defaults shipped in PR #15; choice still pending, recommendation "expanded row mappings" [observed: `docs/TABLE-AUTHORING-CHOICE.md:1-5`] |
| T-FRONTMATTER | Markdown plus rich fences, unknown fences stay code; required `title`, `description`, `version` starting at `0.0.1`; title picture or prolog, charts, labels, audio, video, images, tables. | S27-418 (2026-06-20, Brain Dump) | Partly: publication needs a title and semver version, a missing description only warns, local drafts need no metadata [observed: `docs/RMD-SPEC.md:9`] |
| F-REPORT-TABS, T-TABS | One report with several coherent views (demo: Overview, Sales, Customers); single-view reports keep working without migration; default tab, deep links, readable narrow-screen overflow, keyboard and focus, state kept when switching back; hidden-tab charts don't run wasteful animations; themes coherent across tabs; export and no-script behaviour explicit, nothing silently omitted; smallest native format extension. | S27-649 (2026-10-17) | PR #63 open draft; final handoff reports frontmatter panels, default and export behaviour, deep links, keyboard navigation, preserved DOM state, chart visibility events, 8 Chromium checks [stated]; not on main [observed] |

### Chart engine and chart types

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| C-CHART-ENGINE | A Rust charting library whose first showcase is time series; Highcharts polish, calculation quality and fluid interaction with a restrained RichView identity; Highcharts is a benchmark, not code to copy; honest data, labelled demo data, no misleading scales; an API coding agents use correctly; a thin style/data interface shared with the themes. Initially "keep the new library separate; do not switch the current RichView package to it". | S27-572 (2026-10-02) | Now the production engine on main [observed: `richview-chart/`; PR #9 and #10 merged 2026-10-04]. The handoff reported `--rv-*` CSS tokens, M4 decimation, calendar-exact UTC ticks, wasm exports and a CLI with JSON-pointer diagnostics [stated] |
| F-CHART-TYPES | Nine source-format chart types: line, bar, scatter, area, pie, heatmap, histogram, boxplot, interval ("verify current head"). | S27-647, S27-653 (2026-10-17/18) | [observed: `crates/richview-core/src/parser.rs:325`]; histogram, boxplot and interval arrived with PR #60 [stated in PR #60] |
| F-HIGHCHARTS-PARITY | Iterate to Highcharts craft: local pinned Highcharts install; at least 100 distinct verified gallery pictures with a provenance index; a baseline and at least three improvement rounds with paired, matched-data, matched-viewport evidence; typography, axes and tick density, margins, gridlines, colour hierarchy, marks, legends, labels, annotations; zoom, pan, reset, navigator, crosshair and tooltips near boundaries, touch and keyboard; time series central; do not claim parity while gaps remain. | S27-582 (2026-10-04) | PR #14 merged 2026-10-05 [observed]; 125 references, Highcharts 13.1.1, glyph issues 110→45→3→0, tooltips contained 48/48; remaining gaps: narrow label abbreviation and duplicated legends, continuous-domain navigation, dense aggregation, standalone SVG font fallback, accessibility [stated] |

### Chart interaction and presentation

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-CHART-PLAYBACK | A declarative control for an external dimension, time first: play/pause, seeking, restart, visible current value, smooth transitions with stable mark identity; changing membership, missing values, interrupted playback and reduced motion handled; static render/export stays deliberate and legible; Gapminder-style example. | S27-648 (2026-10-17) | PR #62 open draft: `controls:` block with `playback`, scrubber, prev/play/next, `aria-live` readout; reduced motion advances one frame without latching [stated] |
| F-CHART-SELECTORS, T-CHART-CONTROLS | Chart-local versions: filter a dimension; metric, unit or absolute-versus-relative switches with correct labels, scales, values and tooltips; named variants replacing a whole chart or a composed group. Default selector for one dimension is "one visible group of option buttons above the chart, not a dropdown"; additional selectors default to dropdowns; authors can override. A small stable control interface for peers. | S27-648 (2026-10-17) | PR #62: `filter`, `metric` (swaps `encoding.y` and `axes.y` together) and named full-chart variants; `data-rvc-controls` with `rv:control`/`rv:control-set` events; composed chart groups not implemented [stated]. A supervisor first returned the work because variants and dropdown defaults were missing (comment 2026-10-18 07:22) [stated] |
| F-CHART-MODAL (supersedes F-CHART-EXPAND) | Daniel "revised his expand idea within the dictation: DROP expand/collapse and the bottom 'Inspect chart' affordance"; provide ONE larger near-fullscreen modal with clear close, Escape, focus trap and restoration, responsive overflow; do not implement the abandoned three-mode request or the S27-632 toggle. | S27-654 (2026-10-18), superseding S27-632 (2026-10-13) | Not on main: main still has the Expand toggle and "Inspect chart" [observed: `crates/richview-web/src/browser/charts/expansion.rs:26-58`, `crates/richview-web/src/browser/showcase.rs:928`]; PR #67 open, unfinished |
| F-CHART-DOWNLOADS | Replace unclear SVG/CSV/table glyphs with labelled accessible controls; one image-download icon offering PNG or SVG ("the user's corrected formats are PNG/SVG, not PDF"); CSV and table access with correct payloads. | S27-654 (2026-10-18) | No PNG path found in `crates/richview-web/src/browser/charts*.rs` [observed]; PR #67 open |
| F-CHART-TOOLTIP | Tooltips placed well near boundaries with legible numbers and units (S27-582); move smoothly and stay stable as the pointer crosses the chart or the chart type changes; fix values that appear and disappear; default slightly more transparent and lighter; never interpolate data while animating. | S27-582, S27-654 | Translucent tooltips merged in PR #59 (2026-10-13) [observed: PR]; stability work open in PR #67 [stated] |
| F-CHART-LABELS, T-CHART-HIGHLIGHTS | Direct labels "much too dense": default to sparse extreme or interesting points with collision-aware placement; YAML author controls for explicit highlights by dimension or category and a list of values, with priorities; leader lines only for forced highlights that cannot otherwise be placed; colliding ordinary labels are hidden; selected points stay identifiable; fixtures prove it. | S27-654 (2026-10-18) | Open (PR #67) [stated] |
| F-CHART-TICKS | Stable responsive tick density and formatting across inline, modal and resize; years formatted as years, "never e.g. 1.70K"; fix the Playfair commercial-balance example's repeated years and x-axis abbreviation without losing its animation. | S27-654 (2026-10-18) | Open (PR #67) [stated] |
| F-CHART-SERIES-ID | Line and area series show the series name alone where enough; a small line cue only when it disambiguates; no forced dot-plus-line; keep colour-independent identification. Earlier: "each series named once (end labels or a compact legend above the plot)". | S27-654 (2026-10-18); S27-616 round 1 (2026-10-11) | Partly shipped via PR #22 [stated]; rest open |
| F-CHART-INK-ALIGN | Title, first tick label and first bar, line or point on the same pixel column (ink, not box) across all five themes and both Latin and tabular numerals; verify within 1 px at 2× zoom. | S27-633 (2026-10-13) | Implemented by PR #60 (merged 2026-10-16) [stated in PR #60]; Linear still Brain Dump |
| F-CHART-BAR-SHAPE | Soft top rounding proportional to bar width, flat at the baseline, as an opt-in theme token; bars start one stroke above the zero line so the axis stays visible; remove per-bar entrance "ticking" and keep one staggered fade-up; square bars where a theme lacks the token. | S27-634 (dictated 2026-10-12) | Token exists with default 0 [observed: `richview-chart/src/rmd_render.rs:665`, `:1148`]; PR #60 reports rounding, zero-line clearance and stable entrances [stated]; Linear still Brain Dump |
| F-CHART-CURVE | Curves look "dodgy": monotone cubic (Steffen or Fritsch-Carlson), no overshoot, local extrema equal the data's, `style.line.smoothing` enum with `monotone` default. S27-616 adds "attractive line curvature and area treatments without inventing observations or misleading overshoot". | S27-635 (2026-10-13), S27-616 | Monotone handles exist [observed: `richview-chart/src/path.rs:79-83`]; PR #40 (merged 2026-10-12, before the dictation) and PR #60 (2026-10-16) touched curvature [observed: PR states] |
| F-CHART-MOTION | Smooth chart entrances and reveal/layout transitions, restrained hover "pop", well-judged shadows, coordinated movement; study Highcharts and ECharts animation from real examples; show live motion and hover, not only stills; preserve the animation Daniel liked and the praised "red nose" example if identifiable. | S27-616 (2026-10-10), S27-654 | PR #22 reports static first frames, lazy mounting, scroll entrances and settled exports; motion sampler met 22 of 29 targets [stated in PR #22]; "red nose" unidentified |
| F-CHART-ZOOM-NAV | Useful zooming, stable hover modes, brush/pinch/keyboard, navigator, table view. | S27-572, S27-582 | Reported in S27-572 and S27-582 handoffs [stated]; navigator only on demand after S27-616 round 1 [stated] |

### Themes and style switching

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| G-THEME-SWITCH-PROOF | Render the same content with the canonical styles, inspect browser-computed styles and chart appearance, change the local styling YAML and re-render, switch back, and cover the no-YAML case, for both generated HTML and the editor/browser switching surface. "A selector label or changed option alone is insufficient evidence." | S27-578, S27-581, S27-582, S27-412 (2026-10-04) | S27-578 handoff reports this verified across native, browser and VSIX previews [stated] |
| F-STYLE-SELECTOR | 2026-10-05: Sleek default with Scientific and Brutalist as floating controls and a dropdown for others. 2026-10-17: a prominent selector for all five identities that restyles typography and charts while keeping data and selection. 2026-10-18: "Expose all five basic styles at the top of the style selector; place additional styles behind an Other styles dropdown." | S27-594, S27-647, S27-654 | Flagship and landing still offer Sleek, Scientific and Brutalist only [observed: `assets/landing/index.html:330`]; showcase list has 23 styles without sleek-light or hand-drawn [observed: `assets/showcase/themes.json`] |

Theme identities are in "Design and style feedback".

### Report design, imagery and storytelling

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-REPORT-LAYOUT | Improve gaps, spacing, alignment, line breaks, line lengths, typography, section rhythm, and the relation of text, charts, captions, illustrations and whitespace, as one coherent pass rather than "unrelated cosmetic patches". One consistent text and content measure through the flagship, tables and headings; deliberate full-width artwork stays explicit. | S27-616 (2026-10-10), S27-654 (2026-10-18) | Round 1 shipped one type scale and measure per theme and no filled cards, stripes or fades [stated: S27-616 comment 2026-10-11]; flagship measure open in PR #67 |
| F-HERO-IMAGERY | Illustrative header or hero imagery as a normal choice, with author control over selection, replacement, crop and omission; images licensed and attributed, illustration distinguished from evidence; flagship: replace the broken header chart with a finished infographic or header image, keep provenance, add no research facts. | S27-616, S27-594, S27-637, S27-654 | Four flagship images replaced in PR #60 with credits and ledger [stated]; header infographic open in PR #67 |
| F-STORY-MARKS | Reusable soft callouts, emphasis and pencil-like annotation lines, usable outside the hand-drawn theme. | S27-616 | Hand-drawn pencil strokes shipped in PR #22 [stated]; standalone marks unverified |

### Outputs

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-EXPORT-PDF | A readable report PDF: comfortable text, intentional page breaks, complete charts, legible labels and sources, reliable images and fonts, no clipping or accidental gaps; animated charts export in a settled state; poster, still or link treatments for video, audio and scrolling interactions. | S27-616 | `richview generate --to pdf` exists [observed: `crates/richview-app/src/cli.rs:78-97`]; PR #22 reports eleven remaining pagination findings [stated] |
| F-EXPORT-CAROUSEL | A separately composed LinkedIn carousel PDF: portrait, phone-legible text and chart labels, focused slides, intentional breaks, same claims and evidence; authors preview and adjust theme, slide boundaries, images and content selection; check LinkedIn's current document limits at execution. | S27-616 | Not implemented: "No `outputs.*` frontmatter, carousel or story output is implemented by this contract" [observed: `docs/RMD-SPEC.md:243`] |
| F-OUTPUT-MARKERS | Code or configuration markers choosing what appears in each output and hiding chosen metadata, documented with defaults and an example; the dictated metadata request was incomplete, so expose intentional controls rather than silently stripping context. | S27-616 | Not implemented [observed: `docs/RMD-SPEC.md:243`] |
| F-SCROLLY-STORY | A separate optional scrollytelling output: start with one large chart, scroll through a narrative, zoom into a point, reveal a focused annotation; editable steps for focus, zoom, annotation and transitions; coherent forward and backward; mobile; readable under reduced motion; study Remotion and identify the remembered Rust alternative without assuming either fits. | S27-616 | Not implemented [observed: `docs/RMD-SPEC.md:243`] |

### Data, connectors, assets and hosting

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-ASSET-HOSTING, T-ASSET-REF | Externally hosted large datasets, video, pictures and audio instead of embedded payloads; straightforward upload-to-embed for RichView-managed hosting and the author's own storage; an asset reference and loading contract; agent-usable upload with progress, errors and resumable retry; avoid proxying large transfers through the app server; AWS, Azure and Google Cloud guides with copyable setup prompts on the website explaining what is created, permissions, visibility and cost drivers; explicit public delivery; credentials protected. | S27-616 | Upload, verify and abort exist as `richview asset` and MCP `richview_asset_put`/`richview_asset_verify` [observed: `crates/richview-app/src/cli.rs:141-144`, `crates/richview-app/src/mcp.rs:44`, `crates/richview-app/src/asset_upload/mod.rs:315-326`]; report sources do not read the references yet and cloud guides are unstarted [stated in PR #22] |
| D-HOSTING | Prepare the server for AWS using the reported startup credits, verify balance and eligibility, keep a migration path, evaluate Cloudflare "as a preferred possible destination", prepare (not submit) credit applications for AWS, Azure, Google and others, and model costs with and without credits, including large video and datasets, with limits and lifecycle rules. | S27-616 | No progress recorded in Linear [inferred] |
| F-DATA-CONNECTORS, T-DATA-SOURCE | A connector contract for read-only SQL, dbt MetricFlow semantic queries and cached or materialized results; the source/query step stays distinct from chart encodings while chart-local data ownership stays; one real local SQL path and one real MetricFlow path end to end; connection config, parameters, result schema and field mapping, useful failures, cache refresh with explicit freshness, no result leakage between connections or users, deterministic offline snapshots; credentials only in the trusted host, never in HTML, bundles, source or logs; bounded read-only queries; "not another chart-specific query language". | S27-650 (2026-10-17) | PR #66 open draft behind a `--connectors` flag; reports real DuckDB and real `mf query` end-to-end tests and snapshot reuse; `verify-browser.mjs` flaky, 8 of 9 verifiers pass [stated]. Main still rejects remote/SQL sources [observed: `crates/richview-core/src/compat.rs:315`] |

### Authoring tools: VS Code, CLI and MCP

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-VSCODE | A new extension version: live semantic diagnostics with useful source ranges for malformed YAML, unsupported keys or values and invalid data bindings; safe, idempotent formatting that keeps prose, unknown fences, metadata, comments and data, with optional format-on-save; context-aware completion of keys, enumerated values, chart types and data fields; keep insertion and live preview; a real installable VSIX; product logic in Rust, `extension.cjs` a thin adapter. June baseline: insert chart, validate or diagnose the file, preview or report-card action. | S27-412 (2026-06-20, 2026-10-04), S27-578 | PR #15 merged with VSIX 3.0.0-alpha.1 for Linux x64 [stated]; PR #60 added outline, folding and recovery [stated in PR #60]; not published to the Marketplace [stated] |
| C-CLI-MCP | MCP and CLI are rendering interfaces; documentation must use only real commands and tools, no "nonexistent MCP tools, publishing commands or untested installation claims". | S27-593, S27-591 | Tools on main: `richview_render`, `richview_validate`, `richview_check`, `richview_publish`, `richview_learn`, `richview_asset_put`, `richview_asset_verify` [observed: `crates/richview-app/src/mcp.rs:12-44`, `crates/richview-app/src/asset_upload/mod.rs:315-326`]; commands: generate, validate, check, init, learn, mcp, publish, serve, server, asset [observed: `crates/richview-app/src/cli.rs:78-142`] |
| F-HUMAN-AGENT-LOOP | Chart YAML, preview, diagnostics or report card, human and agent edits, shareable story; agents are better at text than images, so chart intent and diagnostics stay textual and editable; keep dropped-label provenance. | S27-415 (2026-06-20, Brain Dump) | `richview check`/`richview_check` exist [observed]; the loop as a product feature is unverified |

### Publishing

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-REPORT-VERSIONS | The server first hosts full HTML; each report has a version for consistency as the format changes; strip or unify repeated runtime later; correctness and shareability before deduplication; keep raw `.rmd` and style sources for re-rendering. | S27-413 (2026-06-20, Brain Dump) | Context only for S27-655 [stated] |
| C-PUBLISH, F-PUBLISH-NEWS | RichView Publish, an open-source publishing product for a small publisher, demonstrated as RichView News at news.richview.ai: edited front page with masthead and lead stories, sections, archives, search, stories, dashboards and static pages; stable IDs and slugs, bylines, sections and tags, cover assets, dates, draft/published/hidden state, audience, corrections and link-preserving updates. | S27-655 (2026-10-18) | Live [observed: `https://news.richview.ai/_status` returns `state: live`, generation 5, commit `7e7d9f8`]; PR #70 open draft |
| T-SITE-CONFIG | Publication-wide layout, navigation, sections, theme and composition in at most one site YAML file; per-report folders stay; at least two genuinely distinct built-in editorial layouts, "not a new YAML programming language". | S27-655 | Two layouts reported [stated] |
| F-PUBLISH-GIT-LOOP | Git is canonical: author or agent opens a PR, gets validation and a protected preview, required approval, merge to the publication branch, CI publishes; status from commit to live; listings, links, search and feeds update for additions, deletions, renames and audience changes; immutable releases; a failed or late build never replaces a newer release; repeatable rollback; rebuild from pinned source and toolchain. | S27-655 | Content PRs #71 and #72 merged into `publish/news` [observed]; failed-build preservation and rollback reported [stated] |
| F-PUBLISH-ACCESS | Reader, Editor and Super editor roles; delegated identity through an existing provider, no home-made passwords; restricted content protected at the content level, including assets, previews, search and old releases; a small browser editorial desk whose edits become attributed commits or PRs with conflict handling. | S27-655 | Reported with temporary service identities; human email or GitHub sign-in and independent human review not exercised [stated] |
| F-URGENT-HIDE | Urgent hide makes content unavailable on all delivery routes immediately, even if a build fails; audited, reconciled to Git; rollback or later merges cannot undo an active hide; precise restore behaviour. | S27-655 | Reported across direct, asset, preview and historical routes [stated] |

### Dashboards (sibling product)

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-VOICE-DASHBOARD | A separate module in the monorepo sharing the chart engine and styles: user-initiated ElevenLabs Scribe capture with an editable transcript and full text fallback; governed semantic metrics (Okurka, MetricFlow); about six complementary charts with units, periods, source and freshness; cross-selection with reset and an explanation of the active selection; grounded follow-ups (time window, metric, segment); save and reopen with query, spec and interaction state, private by default; live and offline modes labelled. | S27-651 (2026-10-17) | PR #64 open draft; limits: live cross-selection narrows two of six tiles, saves are local, cloud save unconnected, Scribe tested on a generated audio file [stated] |

### Website, docs and galleries (richview.ai)

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-SITE-HOMEPAGE | Order: large existing logo above "RichView is a presentation layer for your agent."; editable Rich Markdown and chart-local YAML beside the real rendered report (source left, output right on desktop, stacked on phones); "What is RichView?"; five elements together, then each grows into focus on scroll in a restrained 3D carousel with accessible fallbacks; the real authoring, rendering and publishing workflow (no one-click publishing unless it exists); "Why RichView?" later; legible headings instead of tiny eyebrow titles. "Big view" tagline is only a note. | S27-590 (2026-10-05) | PR #20 and #21 merged 2026-10-11 [observed] |
| F-SITE-EDITOR-DEMO | An airy, floating editor with syntax highlighting for Rich Markdown, chart YAML and styling YAML; renders automatically as you type without hurting typing; honest diagnostics; no stale output shown as current; Reset; Sleek default, Scientific and Brutalist floating controls with keyboard access; theme changes alter the real rendering. | S27-594 (2026-10-05) | PR #21 merged; reports a Rust highlighter in core and auto-render [stated] |
| F-SITE-SIGNUP | "Coming soon", never "beta"; email interest stored through Cloudflare with minimum fields, clear opt-in, honest success and error, deduplication, proportionate abuse handling; subscriber data private; no emails without permission. | S27-590 | Workers/D1 implementation merged in PR #20 [observed: PR]; it stores email, consent time and version only, no read API, no send path [stated in S27-594] |
| F-SITE-MOTION | Scroll story built from the mark: the logo breaks into pieces that connect to charts and product elements, a chart-like reveal, a background that tells the story, seamless transitions; complete content under reduced motion, keyboard, phones and no JavaScript. 18 Oct review: mobile scrolling "jumpy/weird with visible ticking"; check slow and fast scrolling both ways, sticky boundaries, resize and viewport-height changes, touch and WebKit. | S27-398, S27-594, S27-653 | Main lacks the `#rv-field` and `#rv-stage` anchors the Rust stage needs, so main's landing motion is dormant [observed: `assets/landing/index.html` has neither; `crates/richview-web/src/browser/landing.rs:2505-2507` returns early]; live richview.ai has them (PR #69 build) [observed]; WebKit desktop restore drift remains [stated, S27-653 r5] |
| F-SITE-NAV | "Reduce the top-level buttons and CTAs substantially"; keep a simple route to docs and the flagship. | S27-653 (2026-10-18) | Live nav: Flagship report, Chart gallery, Docs [observed]; main nav: Live example, Product, Workflow, Flagship report, Docs, Get updates [observed: `assets/landing/index.html:50-57`] |
| F-DOCS | Short reliable path for a person or agent to produce a report: installation, Markdown and rich blocks, chart-local data, strict folders and optional styling YAML, `Rendered` output, themes that really change output, preview and export, real CLI and MCP commands, publication and versions, VS Code, troubleshooting; tested minimal example from a clean environment; shipped kept apart from planned. | S27-591 (2026-10-05) | Six guides [observed: `crates/richview-build/src/documentation.rs:14-21`]; live since 2026-10-05 [stated] |
| F-DOCS-CHART-GALLERY | Inside the docs: an index of ALL supported chart types, each clickable into a focused example with source, authoring guidance and working interactions; not one long report; flagship and famous examples kept distinct; redirects for old gallery routes. 18 Oct review: must "visibly display real RichView-rendered charts with matching source and functioning relevant interactions for every supported type"; "source-only entries… do not meet the requirement". | S27-653 (2026-10-18) | `/docs/charts` live [observed]; r5 harness: keyboard readout 36/36, mouse tooltip 20/36, modal 0/36, SVG export 0/36 [stated]; not on main [observed] |
| F-GALLERY-TYPOGRAPHY, F-GALLERY-FAMOUS | Three connected pages: typography specimen, chart-type gallery, famous charts recreated through RichView (flagship: Gapminder life expectancy versus GDP per capita, population as bubble area, time where available), each with an analytical point, copyable source, provenance and labelled simplifications; a five-style selector; no card grids or gradients. | S27-647 (2026-10-17) | Published 2026-10-17 [stated]; `/typography/`, `/chart-gallery/` and `/famous-charts/` return 404 on 2026-10-21 [observed]; information architecture superseded by S27-653 |

### Flagship report

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| F-FLAGSHIP | The OpenAI-versus-Anthropic report: "Analytical substance (80%) and presentation (20%)"; Apify and Exa evidence; a dated source ledger and reproducible pipeline; never fabricate or treat a scraped sample as representative; data and conclusions invariant across themes; the 2 October research snapshot stays fixed; repaired reading, citations and dark labels; polished opening, hierarchy, density, labels and units, source access; licensed images captioned as illustration; a header infographic; one measure; five styles on top. | S27-399, S27-573, S27-430, S27-594, S27-637, S27-654 | Public at `/showcase/openai-vs-anthropic/` since 2026-10-05 [stated]; landing links its default (no theme parameter) as "Scientific" [observed: `assets/landing/index.html:330`] |

### Development harness

| ID | Requirement and acceptance (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| G-DEV-HARNESS | An evidenced end-to-end understanding of RichView and a minimal "software factory": isolated task and worktree ownership, reproducible build entry points, resource-aware build coordination, artifact provenance, functional and visual end-to-end verification, compact evidence retention, review and release gates matching Daniel's publication authority, regression detection and recovery, a runbook; exercised on a real path with a controlled failure and recovery; automated signals for motion, site/gallery/report distinction, interactions and screen widths; no new orchestrator framework. | S27-652 (2026-10-18) | PR #68 open draft; 438 Rust test executions, 9 harness tests, offline interactive export at 320/390/1440 px [stated] |

The LinkedIn carousel (S27-593) is a marketing artifact rather than a product feature; its rules appear under quality and delivery.

## Design and style feedback

### Theme identities

| ID | Intended character (as stated) | Source | Status 2026-10-21 |
| --- | --- | --- | --- |
| S-SLEEK-DARK | Default. "Calm, precise, polished dark presentation with disciplined contrast and depth." "Sleek black" and "sleek dark" are the same theme. | S27-616 (2026-10-10); S27-594 (Sleek default) | Builtin `sleek`, alias `sleek-dark` [observed: `crates/richview-core/src/style.rs:16-36`] |
| S-SLEEK-LIGHT | "An equally considered light treatment with clear hierarchy and comfortable reading." | S27-616 | Builtin `sleek-light` [observed]; on 2026-10-05 Sleek with `--theme light` drew near-white chart labels on a light page [stated: S27-593] |
| S-SCIENTIFIC | "Precise, restrained presentation that supports dense evidence, units, annotations and citations." | S27-616 | Builtin [observed] |
| S-BRUTALIST | "Deliberate, confident typography and structure with a distinctive visual character." | S27-616 | Builtin [observed] |
| S-HAND-DRAWN | New: "predominantly black and white, with pencil-like marks, crossed-out elements and expressive annotations inspired by the Scott Galloway chart references"; designed throughout, "rather than applying a handwriting font to the same template". | S27-616 | Builtin `hand-drawn` [observed]; shipped in PR #22 [stated] |
| S-STUDIO-VARIANTS | The 20 design-system studio variants shown in the flagship showcase; whether they stay public is Daniel's call; later placed behind "Other styles". | S27-573 (2026-10-02), S27-594, S27-654 | `assets/showcase/themes.json` lists 23 styles: sleek, scientific, brutalist and `v01` to `v20` [observed] |
| S-BRAND-BLUE | Canonical brand accent `#1A6DAC` from the original product; v3's indigo called "a drift". | S27-404 (2026-06-20, Brain Dump); S27-407 | Used only by the logo mark, favicon, landing styles and reel [observed: `assets/landing/index.html:40`, `assets/landing/favicon.svg:1`, `assets/landing/landing.css:64`, `crates/richview-web/src/reel.rs:11`]; report themes use their own OKLCH accents, e.g. sleek `oklch(0.5464 0.2508 263.51)` [observed: `crates/richview-core/assets/sleek.rstyle.yaml:13`] |

### Visual rules stated in Linear

- S-ANTI-GENERIC: avoid "generic gradient panels, indiscriminate rounded cards, arbitrary shadows, inconsistent spacing and decoration that overwhelms the evidence" (S27-616); "generic card grids, a wall of logos or ornamental headings" (S27-590); "repetitive dashboard-card grids, generic gradients and tiny decorative plots" (S27-647); "a generic SaaS dashboard, a wall of identical cards or a thin reskin" (S27-655). Use the Impeccable anti-generic-design skill for visual review (S27-616, S27-653, S27-654). [stated]
- Restraint: "five distinctive, coherent identities with restrained, purposeful detail" (S27-616); restrained chart chrome and "the right number of lines", meaning no over-gridded charts (S27-616); restrained 3D motion with a static fallback (S27-590). [stated]
- Typography and composition: legible meaningful headings instead of eyebrow titles (S27-590); readable article measures and a strong masthead for News (S27-655); one measure per report (S27-654); typography specimen page as proof (S27-647). [stated]
- Composition of the homepage: the logo participates in the narrative; the editor floats rather than sitting in "a heavy bordered two-column box"; larger, more legible product cards with "the pop and transitions of the original solution"; clearer visual hierarchy for signup and try actions (S27-594); fewer CTAs (S27-653). [stated]
- Chart craft details: years never abbreviated, sparse labels, series name without forced dot-plus-line, lighter translucent tooltips (S27-654); bar rounding, zero-line clearance and no entrance ticking (S27-634); monotone curves (S27-635); ink alignment (S27-633). [stated]
- Quality references: "competition-winning" (S27-572, S27-573, S27-398, S27-399); Highcharts and ECharts for chart craft and motion (S27-572, S27-582, S27-616); FT.com for publication completeness and craft, not branding (S27-655); "Mlouva" polish, identity unconfirmed (S27-430). [stated]

### Daniel's praise to preserve

- The informative homepage story and the live editor (S27-594). [stated]
- The repaired original flagship (S27-594). [stated]
- "The animation Daniel liked" on the Playfair commercial-balance example, and a praised "red nose" example whose identity is unknown (S27-654). [stated]

## Quality and verification expectations

### Truthfulness rules

- G-DATA-INTEGRITY: preserve values, units, scales, analytical meaning and provenance across themes, interpolation, storytelling, frames, variants and exports; never fabricate observations, sentiment scores, findings or coverage; label demo and synthetic data (S27-572, S27-573, S27-399, S27-582, S27-616, S27-648, S27-654, S27-655). [stated]
- G-HONEST-CLAIMS: claims match tested capabilities; no invented one-click publishing, raw-HTML grammar, unavailable themes or public source release (S27-590, S27-594); docs derived from exercised examples, shipped kept apart from planned (S27-591); demonstrations render through the real engine, not screenshots or copied code (S27-647); live and offline modes labelled (S27-651); "Coming soon", never "beta", and no implied launch (S27-590, S27-593). [stated]
- G-HONEST-STATUS: report partial work honestly; no parity claim with material gaps (S27-582); record baseline failures and do not suppress warnings (S27-594); "To Review does not establish that every quality gate passed" (S27-652, S27-653, S27-654); "Do not mark completion from a successful build alone" (S27-655). [stated]

### Evidence standards

- G-ARTIFACT-PROOF: verify the generated artifact, because "an unrelated dev-server preview does not establish that the exported HTML works" (S27-578, S27-581). [stated]
- G-MOTION-PROOF: prove motion in a real browser with recordings or frame sequences; "a static screenshot or a successful Rust build alone is insufficient"; "Proving animated elements exist or checking opacity samples is not proof that motion looks smooth"; cover slow and fast scrolling both ways, boundaries, resize, viewport-height changes, touch and WebKit, and state device limits (S27-653). [stated]
- G-DESIGN-REVIEW: independent adversarial design critiques with revisions; "A passing test suite or an agent's self-assessment alone does not establish design quality" (S27-616); paired matched-data comparisons against references (S27-582); a substantial visual review of actual output (S27-654). [stated]
- Coverage expected in briefs: desktop and narrow widths (320, 390, 420, 1440 px), keyboard, reduced motion, no-JS reading, zero console errors, all five themes (S27-632, S27-633, S27-647, S27-653). [stated]
- Acceptance was enforced against results: supervisors sent S27-648 (missing variants and dropdown defaults), S27-650 (no host entry point, no real MetricFlow render) and S27-651 (screenshots not showing settled selection, ignored time window, unreadable labels) back to the same executor before review (comments 2026-10-17 to 18). [stated]

### Required checks named in Linear

- Rust: `cargo fmt --check`, Clippy native and `wasm32` with `-D warnings`, `cargo test --workspace --locked`, release build `cargo run --release --locked -p richview-build` (S27-572, S27-634, S27-635, S27-637, S27-648, S27-650). [stated]
- Browser verifiers: `scripts/verify-browser.mjs`, `verify-showcase.mjs`, `verify-homepage.mjs` and the editor, native, compatibility, landing, flagship, reel, docs and local Cloudflare checks (S27-590, S27-594, S27-632, S27-633, S27-637, S27-653). [stated]
- Hosted CI is billing-blocked: run equivalent local checks and do not spend to unblock it (S27-578, S27-651, S27-652, S27-655). [stated]
- The 2 October handoffs still used the TypeScript gate `pnpm build && pnpm test && pnpm lint` (S27-398, S27-399, S27-573); it no longer applies to the Rust product [inferred].

### Measurements reported in Linear (performance candidates)

- P-CHART-FRAME: richview-chart full-year frames median 1.2 to 1.6 ms, p95 3.7 to 4.6 ms (S27-572 comment, 2026-10-02). [stated]
- P-LAYOUT-SHIFT: flagship desktop CLS fell from 0.072 to 0.014 (S27-616 comment, 2026-10-11). [stated]
- P-WASM-SIZE: delivered Wasm was 191 KB over its gzip budget because fonts were compiled in (S27-616 comment, 2026-10-11); PR #22 later reports 1,547,127 bytes against a 1,572,864-byte limit, then 1,601,334 bytes after integration, still over [stated in PR #22].
- P-PUBLISH-SCALE: hundreds of items, a correct bounded full build is acceptable for v1, 300 varied fixtures passed (S27-655). [stated]
- P-HOSTING-COST: low, explainable costs under large video and dataset use, with scenarios during and after credits (S27-616). [stated goal]
- Obsolete targets: RV2 "LCP < 1.5s on 3G" (S27-432, April) and the ECharts "170 KB budget" (S27-408, June). [stated]

### Known defects and limits left open in Linear

- Negative SVG `<rect>` height console warnings (S27-590, S27-594, S27-653, S27-654); S27-653 r4 reports them gone on the live site after a cherry-picked S27-654 fix; main is unverified. [stated]
- `verify-browser.mjs` load-sensitive flake (S27-650) and the `verify-homepage.mjs:46` pending-versus-live race (S27-653). [stated]
- WebKit desktop chapter drift after viewport restore on the landing page (S27-653 r5). [stated]
- Docs gallery interactions incomplete in the r5 harness (S27-653). [stated]
- Category axis labels truncate below about 600 px; Sleek light labels; the landing font subset missing a space glyph (S27-593, 2026-10-05; current status unknown). [stated]
- Remaining Highcharts gaps (S27-582); variants cannot replace composed chart groups (S27-648); dashboard limits (S27-651); human sign-in and human review unexercised on News (S27-655). [stated]
- S27-653 and S27-654 executors stopped on 2026-10-19 with revoked or missing Claude sign-in; S27-654's flagship, interaction verification and integration are unfinished (comments 2026-10-19). [stated]

## Delivery and process rules

### Authority and publication

- Default boundary in almost every brief: preparation only, in an isolated worktree, task branch and draft PR; no merge, deployment, publication, visibility change, messages, posts, reviewer requests or spending without Daniel's explicit authorization for that concrete action (S27-572, S27-573, S27-577, S27-578, S27-582, S27-590, S27-593, S27-594, S27-648 to S27-652). [stated]
- Explicit authorizations recorded in Linear: 2026-10-04 "merge and deploy everything", Cloudflare selected (S27-577 comment); 2026-10-05 publish docs and flagship on richview.ai, homepage and signup excluded (S27-430, S27-591); 2026-10-05 "merge and publish" the docs redesign (S27-591 comment); 2026-10-17 immediate publication of the galleries batch (S27-647); 2026-10-18 publish all richview.ai repairs right away, "not blanket authority to merge core PRs" (S27-653); 2026-10-18 publish RichView News with safe content (S27-655). [stated]
- The 2026-10-11 merges (#17, #19 to #22) have no authorization record in Linear; PR #22 states "Daniel authorized merging this reviewed PR batch on 11 October 2026" [stated in PR #22; merges observed].
- G-DEPLOY-OWNER: one deployment owner per batch: S27-430 (2026-10-05), S27-647 (2026-10-17), S27-653 for richview.ai and S27-655 for news.richview.ai (2026-10-18); keep a rollback target and an exact source and version receipt (S27-430, S27-647, S27-653). [stated]
- Privacy: subscriber data stays private and nothing is emailed without permission (S27-590, S27-594); dashboard saves private by default (S27-651); restricted News content protected; private host paths and credentials kept out of public docs (S27-655). [stated]

### Execution rules

- G-WORKTREE-ISOLATION: work in `.claude/worktrees/<slug>` from the fresh remote default; the primary checkout is stale; never edit peer worktrees; preserve user edits. Exception: cowork mode uses Daniel's active checkout without reset, stash or branch switch (S27-577). [stated]
- Coordination: one writer per handoff manifest; a shared coordination root; claim files before touching shared modules; consume peer commits deliberately; final handoffs start with "Status: final" or "Status: needs Daniel" (S27-573, S27-590, S27-652 to S27-655). [stated]
- Resources: heavy builds serialized through a shared build helper with a 5 GiB disk floor; no cache or worktree cleanup by tasks (S27-647 to S27-654). [stated]
- Linear lifecycle: pickup acknowledgement, `Fixes S27-NNN` in the PR, one dispatcher review command per run, supervisors return incomplete work to the same session, no duplicate executors, and the chief of staff records project status (S27-578, S27-582, S27-648, S27-652 to S27-655). [stated]
- GUI checks run offscreen or headless; Daniel's visible desktop stays still (S27-578, S27-582, S27-594). [stated]

### Repository, licensing and architecture rules

- G-RUST-OWNS-BEHAVIOR: product behaviour lives in Rust, shared by native and WebAssembly hosts; generated JavaScript packages and `packages/vscode/extension.cjs` are adapters; a narrow site animation layer may use JavaScript when that is the right repair (S27-578, S27-582, S27-647 to S27-651, S27-653). [stated; layout consistent with `crates/` and `richview-chart/` [observed]]
- Repository: on 2026-10-05 Daniel chose "Clean up the existing repo (recommended)" over a fresh repo or archive (S27-430, S27-573, S27-590, S27-591); the repo stays private and public source is a future release commitment (S27-594). The repo now lives at `1vecera/richview` [observed: `git remote -v`].
- Monorepo: keep one repository and separate packages rather than four repos (S27-422 decision, 2026-06-09); dashboards and Publish stay in the monorepo (S27-651, S27-655). [stated]
- Licensing: MIT for libraries and tooling, Elastic-2.0 for the server (S27-422, 2026-06-09) [observed: `LICENSING.md:3-14`]; "The repository carries an MIT license" (S27-594) [observed: `LICENSE:1`].
- Older rule HC-2: do not edit Icebox or Human Review tickets without approval (S27-411, S27-420); workspace 935 has no such states now [observed: team Life states].
- "Older tickets are context, not inherited unfinished scope" (S27-647 to S27-651); keep the existing report-folder, optional styling YAML, `Rendered` and chart-local data conventions in all new tracks (S27-647 to S27-651). [stated]

### The new delivery model (S27-677, 2026-10-21)

- G-DEV-PROD: agents merge freely into `dev`; only Daniel merges `dev` into `prod`; no releases; Daniel reviews only design-quality and feature decisions, in batches, after bug fixes, regressions, ideas and visual improvements are done. [stated]
- Deliverables: a guided lockdown app with prefilled answers and IDs in parallel batches, delivered in parts; lockdown setup (goals, rules, boundaries, delivery evidence, the dev/prod model); guiding documents (purpose and maximization, architecture, each component's features, behaviour and look, styles and types, performance, screenshots). [stated]

## Open vs done state

- No issue in the project has a completed state: every issue is To Review, Working on, Que, Brain Dump or Duplicate. [observed]
- Linear lags the code: eleven issues whose PRs merged into main between 2026-10-04 and 2026-10-11 still say To Review (S27-398, S27-399, S27-430, S27-572, S27-573, S27-577, S27-578, S27-582, S27-590, S27-591, S27-594); S27-579, S27-580, S27-581 and S27-412 say Que though PR #15 delivered them; S27-632, S27-633, S27-634, S27-635 and S27-637 say Brain Dump though PR #60 implemented them on 2026-10-16. [observed]
- Live is ahead of main: richview.ai serves the PR #69 branch build (three-link nav, motion anchors present) and news.richview.ai runs from the PR #70 branch; neither PR is merged. [observed: live GETs and PR states; branch origin [stated] in S27-653 and S27-655]
- Merged on main and observed: Rust/Wasm product; report folders, chart-local data and `Rendered` output; five builtin themes; nine chart types; PDF export; asset upload commands; MCP tools; six docs pages; landing copy with "Coming soon" and the open-source promise. [observed]
- Open work by issue: S27-616 (carousel, output markers, scrollytelling, asset references in reports, cloud guides, AWS hosting and credits); S27-647 to S27-655 (open draft PRs #62 to #70); S27-653 (WebKit drift, gallery interactions, needs Daniel's re-review); S27-654 (unfinished); S27-400 and S27-401 (August scope never closed). [observed/stated]

### Open decisions for Daniel

| ID | Decision | Sources |
| --- | --- | --- |
| D-SCOPE-CORE | Which of dashboards, connectors, Publish/News, tabs, playback, scrollytelling, carousel export and hosting belong in the rewritten core, which stay sibling products, and which drop. | S27-420, S27-401 vs S27-647 to S27-655 |
| D-TABLE-SYNTAX | Row mappings (recommended) or a series/column form for tables. | S27-580, `docs/TABLE-AUTHORING-CHOICE.md` |
| D-DEFAULT-STYLE | Sleek dark as the default everywhere, or Scientific for the flagship (PR #8 judgement call still pending). | S27-616, S27-594 vs S27-573 comment 2026-10-02 |
| D-STYLE-CATALOG | Keep the 20 studio variants public behind "Other styles", or ship only the five. | S27-573, S27-654 |
| D-BRAND-ACCENT | `#1A6DAC` as the brand colour in reports, or per-theme accents; and whether the unanswered house-style questions (v05 Blueprint) still matter. | S27-404, S27-407 |
| D-SERIES-PALETTE | Adopt validated per-theme series slots because "existing chart palettes fail CVD checks". | S27-572 comment 2026-10-02 |
| D-CHART-MODAL | Confirm one near-fullscreen modal replaces the shipped Expand toggle and "Inspect chart". | S27-654 vs S27-632, PR #60 |
| D-FAMOUS-SIMPLIFICATIONS | Show labelled simplifications (Rosling bins, Snow density grid, Nightingale as pie) or only charts the engine renders faithfully. | S27-647 |
| D-GALLERY-ROUTES | Restore, redirect or drop `/typography/`, `/chart-gallery/` and `/famous-charts/` now that they return 404. | S27-647, S27-653 |
| D-OPEN-SOURCE | When and how the source goes public, and whether "fully open source" means changing the Elastic-2.0 server licence. | S27-594, S27-593, S27-422 |
| D-PUBLISH-TARGET | The CLI publishes to `https://richview.uk` by default while the live product is on richview.ai and news.richview.ai; decide the canonical service and the Server-versus-Publish relationship. | `crates/richview-app/src/cli.rs:122`, S27-577, S27-655 |
| D-HOSTING | AWS with credits versus Cloudflare as the preferred host. | S27-616 |
| D-FLAGSHIP-CLAIMS | Whether to publish the flagship's findings under Daniel's name in marketing. | S27-593 comment 2026-10-05 |
| D-DASHBOARD-SAVE | Where dashboard saves go in the cloud. | S27-651 |
| D-PUBLISH-PR70 | Disposition of draft PR #70 and who verifies human sign-in. | S27-655 checkpoint 2026-10-19 |
| D-CHART-SUPPLIER | Close S27-410 (ECharts as supplier, separate charting repo) now that the Rust engine is in production. | S27-410, S27-572 |
| D-REPO-SPLIT | Close or act on S27-409 (website in its own repo) and S27-410 (charting in its own repo) given the monorepo decision. | S27-409, S27-410, S27-422 |
| D-LINEAR-STATE | Reconcile Linear states with merged PRs before the rewrite uses Linear as a source of truth. | Issue index above |
| Minor wording | Unclarified dictation: "Escapism" and "Where to Render" (S27-582); "Big view" tagline (S27-590); "Mlouva" reference (S27-430); "red nose" example (S27-654); the remembered Rust Remotion alternative and a second design resource (S27-616); the intended dbt Charts reference (S27-650). | as cited |

## Conflicts

- **Scope: narrow layer vs product family.** S27-420 (2026-06-20) keeps RichView to "data stories for coding-agent users, not dashboards, not generic BI, not a full chat/web-editor product"; S27-401 and DIR-121 (2026-08-08) say "rather than a sprawling platform"; the project rule says finish the focused release first. The 17 and 18 October briefs add a chat-driven voice dashboard (S27-651), warehouse connectors (S27-650), multi-tab reports (S27-649) and a publishing product with roles, identity and an editorial desk (S27-655), and say they override the August scope. Forces D-SCOPE-CORE.
- **Chart supplier.** S27-410 (Daniel, 2026-06-20) and the S27-408 synthesis choose ECharts behind a small contract in a separate repo; PR #1 (2026-06-20) dropped ECharts and D3 for a TypeScript SVG engine; S27-572 (2026-10-02) built a separate Rust library and said not to switch production; main now renders with the Rust `richview-chart` [observed]; S27-616 asks to explore WebGL and WebAssembly raster rendering. S27-410 is still open. Forces D-CHART-SUPPLIER.
- **Repository topology.** S27-409 and S27-410 want the website and charting in separate repos; S27-422 decided to keep the monorepo; main holds the website (`assets/landing/`) and the chart crate [observed]. Forces D-REPO-SPLIT.
- **"Fully open source" vs licence and visibility.** S27-594, S27-593 and S27-655 promise open source; S27-422 put the server under Elastic-2.0, which is source-available rather than an open-source licence [inferred]; the root `LICENSE` is MIT and `LICENSING.md` keeps the Elastic-2.0 server [observed]; the repo is private. Forces D-OPEN-SOURCE.
- **Expand vs modal.** S27-632 (2026-10-13) asked for an Expand/Contract toggle, which PR #60 shipped (2026-10-16); S27-654 (2026-10-18) drops expand and "Inspect chart" for one modal; main still has both [observed]. Forces D-CHART-MODAL.
- **Label leaders.** PR #9 (S27-572, merged 2026-10-04) built "end labels with leader lines" into the engine [observed: PR #9 body]; S27-654 (2026-10-18) calls direct labels "much too dense" and allows leader lines only for forced highlights that cannot otherwise be placed [stated]. The rewrite needs one labelling default (F-CHART-LABELS).
- **Default theme.** S27-616 makes Sleek dark the default and S27-594 defaults the homepage editor to Sleek; the flagship's default stayed Scientific (PR #8 judgement call) and the landing links it as such [observed: `assets/landing/index.html:330`]. Forces D-DEFAULT-STYLE.
- **Theme catalogue.** Core ships five builtins [observed: `crates/richview-core/src/style.rs:16-25`]; the flagship showcase lists 23 styles and lacks sleek-light and hand-drawn [observed: `assets/showcase/themes.json`]; S27-654 wants five on top plus "Other styles". Forces D-STYLE-CATALOG.
- **Style selector layout.** S27-594 (2026-10-05) asked for Sleek by default with Scientific and Brutalist as floating controls and a dropdown for the rest; S27-654 (2026-10-18) puts all five basic styles on top and the rest behind "Other styles" [stated]; the landing still links three [observed: `assets/landing/index.html:330`]. The later instruction supersedes the earlier one [inferred].
- **Brand colour and fonts.** S27-404 calls `#1A6DAC` canonical and indigo a drift; report themes use per-theme OKLCH accents (sleek near 263°) and `#1A6DAC` survives only in the logo mark, favicon, landing styles and reel [observed: `git grep -i 1A6DAC`]. The S27-408 synthesis proposed Newsreader and IBM Plex Mono; themes now use Space Grotesk, Geist and Geist Mono (sleek, sleek-light), RichView Serif, Sans and Code (scientific), Newsreader, Big Shoulders and Space Mono (brutalist), and Newsreader, Patrick Hand and RichView Mono (hand-drawn) [observed: `crates/richview-core/assets/*.rstyle.yaml`]. Forces D-BRAND-ACCENT.
- **Diagnostics.** S27-423 (2026-06-08) made self-validating charts the hero; the S27-408 synthesis (2026-06-20, never accepted) removed diagnostics from v1; main ships `richview check` and `richview_check` [observed]. The rewrite must say whether diagnostics are core.
- **MCP surface.** S27-408 proposed four tools; main has seven, including `richview_learn` and two asset tools [observed: `crates/richview-app/src/mcp.rs:12-44`, `crates/richview-app/src/asset_upload/mod.rs:315-326`].
- **Publish target.** Linear treats richview.ai (Cloudflare) as the live product and richview.uk on Fly as legacy (S27-577, S27-398); the CLI still publishes to `https://richview.uk` by default [observed: `crates/richview-app/src/cli.rs:122`]; RichView Publish is a separate Git-managed path (S27-655). Forces D-PUBLISH-TARGET.
- **Gallery information architecture.** S27-647 published standalone typography, chart and famous-chart pages on 2026-10-17; S27-653 the next day called the chart gallery "mistaken" and moved it into docs with redirects; on 2026-10-21 the three old routes return 404 [observed]. Forces D-GALLERY-ROUTES.
- **Frontmatter requirements.** S27-418 makes `title`, `description` and `version` required; the spec requires title and version only for publication and only warns on description [observed: `docs/RMD-SPEC.md:9`].
- **Compatibility vs strict contracts.** S27-579 originally said to preserve frontmatter datasets and schema compatibility; Daniel's 2026-10-04 correction makes header data an error, explicitly overriding it (S27-579, S27-578). Recorded as resolved by Daniel, with migration guidance [stated].
- **Delivery model.** Every earlier brief forbids merging without Daniel's explicit authorization; S27-677 lets agents merge freely into `dev`. Meanwhile PRs #5 to #9, #17, #20, #23 to #27, #29 to #32 and #34 to #37 were merged while still marked draft [observed: `gh pr list`], and the 2026-10-11 merge authorization is recorded only in PR #22.
- **Linear vs code vs live.** Linear states lag merged code and live deployments run unmerged branches (see Open vs done state). Forces D-LINEAR-STATE.
- **Beta vs coming soon.** S27-398 (2026-10-02) built a "beta" site with a beta status table; S27-590 (2026-10-05) requires "Coming soon", never "beta"; main and the live homepage now say "Coming soon" and no longer say "beta" [observed]. Resolved.
- **Audience drift.** From a single coding-agent audience (S27-408, S27-420) to analysts, developers, report readers, LinkedIn audiences, sales users dictating dashboards and small newsrooms (S27-572, S27-616, S27-651, S27-655). Part of D-SCOPE-CORE.
- **Repo strategy vs rewrite.** On 2026-10-05 Daniel chose cleaning the existing repo over a fresh one; on 2026-10-21 he plans to rewrite RichView from scratch (S27-677) in the transferred repo [inferred: same repo]. The lockdown should state whether the rewrite reuses the repository, history and branches.

## Appendix: older issues in the project (S27-198, S27-398 to S27-433)

All migrated on 2026-10-12 (label "moved" on most). States as of 2026-10-21.

| ID | Title | State | Created | Content and decisions |
| --- | --- | --- | --- | --- |
| S27-198 | RichView: kick off next phase with one concrete next-step plan | Duplicate of S27-401 | 2026-05-11 | Pick one next-phase target; 2026-07-07 comment: "important but elusive", release a basic version. |
| S27-398 | Build the RichView scrolling beta website and motion showcase | To Review | 2026-08-08 (brief 2026-10-02) | Scroll story from the mark, 15-second reel at 1080p60 with procedural sound, beta status table, feedback CTA to a public GitHub issue, fal.ai paper grain; PR #7 merged 2026-10-04. Comment flagged richview.ai without DNS and Fly serving an older build at the time. |
| S27-399 | Research OpenAI versus Anthropic for the flagship RichView report | To Review | 2026-08-08 (brief 2026-10-02) | 13 sections, 18 charts, 643-row dated source ledger, reproducible Python pipeline, unaudited commercial figures labelled; PR #6 merged 2026-10-04. |
| S27-400 | Map current RichView architecture and lock the August scope | Que (due 2026-08-11) | 2026-08-08 | Done when packages, data flow, renderer, server, CLI, editor and deployments are mapped, working/broken/obsolete paths explicit, launch surface in one sentence. Never closed; S27-652 partly covers it. |
| S27-401 | RichView August launch — focused public release | Que (due 2026-08-31) | 2026-08-08 | Scope lock "curated JSON/data-to-report rendering layer"; checklist all unchecked; reset rule: Icebox items return only when the launch proves them necessary. |
| S27-402 | Research YAML/JSON spec → HTML website tools for RichView | Brain Dump | 2026-07-10 | Stackwright, json-render, OpenPage, UIGen; 2026-08-30 comment: use json-render as a comparison input via a bake-off, not a replacement. |
| S27-403 | Clean up stray untracked items in richview-v3 | Brain Dump | 2026-06-20 | Commit or ignore `project-summary.html`, `tasks/`, `tmp/`. |
| S27-404 | Correct v3 accent: indigo → RichView blue #1A6DAC | Brain Dump | 2026-06-20 | See S-BRAND-BLUE. |
| S27-405 | Wire the studio's charts to the real @richview/charts engine | Brain Dump | 2026-06-20 | Design-system studio previews used a reimplementation; "fix the interface, swap the engine". TypeScript-era. |
| S27-406 | Deploy the design-system studio to GitHub Pages | Brain Dump | 2026-06-20 | Make the studio shareable. |
| S27-407 | Lock the RichView house style (answer the 10 design questions) | Brain Dump | 2026-06-20 | Ten questions unanswered; recommended "v05 Blueprint" (serif/sans hybrid, `#1A6DAC`, comfortable density, stacked layout); accent role, serif commitment and layout open. |
| S27-408 | Design RichView vNext architecture: 3 deep-research runs + synthesis workflow | Brain Dump | 2026-06-20 | Synthesis left for Daniel's review, never accepted: `format: rmd/0.1` optional, six fences (audio and video as `media`), `.rvstyle.yaml` with tokens/layout/chart, ECharts behind a four-key contract, SSR-to-SVG first paint, diagnostics removed from v1, four MCP tools, server Elastic-2.0 with a 50-version cap, read-only playground, MCP plus Agent Skill as the wedge; flagged the `.rmd` vs R Markdown `.Rmd` name collision. |
| S27-409 | Move richview website to a separate repo | Brain Dump | 2026-06-20 | No description. |
| S27-410 | Move the charting to a separate repo and revert to echarts as a supplier of the charts | Brain Dump | 2026-06-20 | Daniel: "I want to simplify things to reduce the scope of handling charting later on". |
| S27-411 | Mine old RichView/RV2 review tickets without reviving stale scope | Brain Dump | 2026-06-20 | Old review work "may contain gold"; produce a keep/mine/discard matrix; HC-2 protects Icebox and Human Review tickets. |
| S27-412 | Deliver RichView VS Code linting, formatting and auto-suggestions | Que | 2026-06-20 | See F-VSCODE; delivered through S27-578. |
| S27-413 | Plan RichView report publishing and version model | Brain Dump | 2026-06-20 | See F-REPORT-VERSIONS. |
| S27-414 | Research minimum-code interactivity for RichView charts | Brain Dump | 2026-06-20 | Compare custom elements, ECharts hooks, a tiny runtime and React; criteria: bundle size, self-contained HTML, testability, agent-editability. |
| S27-415 | Design the human-agent chart collaboration workflow | Brain Dump | 2026-06-20 | See F-HUMAN-AGENT-LOOP. |
| S27-416 | Scope the RichView web surface to a playground, not a full editor | Brain Dump | 2026-06-20 | Example `.rmd`, style selector, preview, copy/download/share; server surface: landing, playground, `/r/:slug`, `/raw/:slug`, publish API; bring-your-own-key chat parked. |
| S27-417 | Add content-plus-style rendering: `.rmd` file + cascading style files | Brain Dump | 2026-06-20 | See G-OPTIONAL-STYLE. |
| S27-418 | Define Rich Markdown vNext frontmatter, versioning, and media blocks | Brain Dump | 2026-06-20 | See T-FRONTMATTER. |
| S27-419 | Reframe RichView copy around agented data storytelling | Brain Dump | 2026-06-20 | "Agents create data stories; humans curate, review, improve, and share them"; avoid SaaS and BI language. |
| S27-420 | RichView vNext: agented data storytelling layer | Brain Dump | 2026-06-20 | Parent of S27-408 to S27-419; narrow product. |
| S27-421 | Deepen RichView KB to full v1→v2→v3 scope | Brain Dump | 2026-06-09 | History: v1 was a 2023 multi-service SaaS on therichview.com; v2 a nine-package monorepo; v3 a four-package TypeScript repo (later replaced by Rust). |
| S27-422 | Decide whether to split RichView v3 monorepo into 4 separate repos | Brain Dump | 2026-06-09 | Decision "after grilling": keep the monorepo, extract `@richview/charts`, MIT libraries and Elastic-2.0 server, changesets; repo set private, "keep private until ready to limelight"; no npm publish. |
| S27-423 | RichView revival: positioning research + scrollable pitch/architecture SPA | Brain Dump | 2026-06-08 | Self-validating chart library shipped as the hero: diagnostics contract with no false "clean", dropped-label provenance, `richview_diagnose`, report card. |
| S27-424 | RichView: prune Symphonia complexity — PMs should oversee Claude, not do work themselves | Brain Dump | 2026-04-12 | Title only: orchestration agents supervise rather than implement. |
| S27-425 to S27-429 | RidgeView: deep research integration; infographic headings + Apify trending topics; position as agentic layer for data-driven reports (not more, not less); recruit international testers; icebox → backlog → up next pipeline | Brain Dump | 2026-04-10 | Titles only; acceptance criteria in comments are bot proposals ("PM RichView triage"), not decisions. |
| S27-430 | Clean existing RichView repo and polish/publish the flagship report and docs | To Review | 2026-04-10 (briefs 2026-10-04/05) | Release owner of the 2026-10-05 docs and flagship publication; PR #17 merged 2026-10-11. |
| S27-431 to S27-433 | RV2: i18n; server-side rendering; real-time collaborative editing | Brain Dump | 2026-04-10 | RV2-era SaaS ideas moved back to Icebox on 2026-04-10. |

## Appendix: related issues outside the project

- DIR-121 "Launch the focused RichView product by August 31" (project Purpose, Committed, created 2026-08-08): same launch definition as S27-401. [stated]
- S27-364 "Review every open Linear project: keep or kill" (Done 2026-09-13): RichView marked Keep. [stated]
- S27-287 "Launch richview" (Done 2026-07-26, no description) and S27-308 "RichAI launch — next steps" (2026-06-30): merge RichAI, buy the domain, make things work, dogfood daily reports. [stated]
- S27-183 "RichView: do focused product work at ML Prague conference" (Done 2026-05-08). [stated]
- S27-617 is a hackathon website and explicitly not RichView. [stated]

## Appendix: proposed entity IDs

All proposals; reconcile with the other evidence files before they enter parts.

- Components: C-RICH-MARKDOWN, C-THEMES, C-RENDERER, C-SERVER, C-VSCODE (the five elements, S27-590); C-CHART-ENGINE (S27-572); C-CLI-MCP (S27-593); C-PUBLISH (S27-655); C-DASHBOARDS (S27-651, S27-655); C-WEBSITE (S27-577, S27-653).
- Features: F-TABLE-YAML, F-REPORT-TABS, F-CHART-TYPES, F-HIGHCHARTS-PARITY, F-CHART-PLAYBACK, F-CHART-SELECTORS, F-CHART-MODAL, F-CHART-DOWNLOADS, F-CHART-TOOLTIP, F-CHART-LABELS, F-CHART-TICKS, F-CHART-SERIES-ID, F-CHART-INK-ALIGN, F-CHART-BAR-SHAPE, F-CHART-CURVE, F-CHART-MOTION, F-CHART-ZOOM-NAV, F-STYLE-SELECTOR, F-REPORT-LAYOUT, F-HERO-IMAGERY, F-STORY-MARKS, F-EXPORT-PDF, F-EXPORT-CAROUSEL, F-OUTPUT-MARKERS, F-SCROLLY-STORY, F-ASSET-HOSTING, F-DATA-CONNECTORS, F-VSCODE, F-HUMAN-AGENT-LOOP, F-REPORT-VERSIONS, F-PUBLISH-NEWS, F-PUBLISH-GIT-LOOP, F-PUBLISH-ACCESS, F-URGENT-HIDE, F-VOICE-DASHBOARD, F-SITE-HOMEPAGE, F-SITE-EDITOR-DEMO, F-SITE-SIGNUP, F-SITE-MOTION, F-SITE-NAV, F-DOCS, F-DOCS-CHART-GALLERY, F-GALLERY-TYPOGRAPHY, F-GALLERY-FAMOUS, F-FLAGSHIP. F-CHART-EXPAND (the S27-632 toggle) is superseded by F-CHART-MODAL and not proposed.
- Types: T-FRONTMATTER (S27-418), T-TABS (S27-649), T-CHART-CONTROLS (S27-648), T-CHART-HIGHLIGHTS (S27-654), T-ASSET-REF (S27-616), T-DATA-SOURCE (S27-650), T-SITE-CONFIG (S27-655).
- Styles: S-SLEEK-DARK, S-SLEEK-LIGHT, S-SCIENTIFIC, S-BRUTALIST, S-HAND-DRAWN (S27-616); S-STUDIO-VARIANTS (S27-573); S-BRAND-BLUE (S27-404); S-ANTI-GENERIC (S27-590, S27-616, S27-647, S27-655).
- Performance: P-CHART-FRAME (S27-572), P-LAYOUT-SHIFT and P-WASM-SIZE (S27-616), P-PUBLISH-SCALE (S27-655), P-HOSTING-COST (S27-616).
- Gates and rules: G-REPORT-FOLDER, G-CHART-LOCAL-DATA, G-RENDERED-OUTPUT, G-OPTIONAL-STYLE, G-THEME-SWITCH-PROOF, G-DATA-INTEGRITY, G-HONEST-CLAIMS, G-HONEST-STATUS, G-ARTIFACT-PROOF, G-MOTION-PROOF, G-DESIGN-REVIEW, G-DEPLOY-OWNER, G-WORKTREE-ISOLATION, G-RUST-OWNS-BEHAVIOR, G-DEV-HARNESS, G-DEV-PROD.
- Decisions: D-SCOPE-CORE, D-TABLE-SYNTAX, D-DEFAULT-STYLE, D-STYLE-CATALOG, D-BRAND-ACCENT, D-SERIES-PALETTE, D-CHART-MODAL, D-FAMOUS-SIMPLIFICATIONS, D-GALLERY-ROUTES, D-OPEN-SOURCE, D-PUBLISH-TARGET, D-HOSTING, D-FLAGSHIP-CLAIMS, D-DASHBOARD-SAVE, D-PUBLISH-PR70, D-CHART-SUPPLIER, D-REPO-SPLIT, D-LINEAR-STATE.
