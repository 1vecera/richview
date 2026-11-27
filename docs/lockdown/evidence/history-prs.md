# Evidence: decision history from git and GitHub

Mined 2026-10-21 for the RichView lockdown. Read-only: no commit, push, comment, label, merge, deployment or setting change. Sources are the git history of every branch in this repository, all 75 GitHub pull requests with their bodies, commits, comments, reviews and inline review comments, the GitHub Actions run history, the decision surface `public/architecture.html` in the private `richview-universe/richview-design-system` repository, and the task briefs that the open PRs cite. Linear is covered in the Linear evidence (not published); this file reuses its decision IDs where the decision is the same. Branch names are written without their Linear issue key.

Tags: **O** observed (verified now in code, git, the GitHub API or a live GET), **S** stated (written in a PR body, commit message, brief or doc, not verified here), **I** inferred (my reading across sources). PR metadata (number, state, author, dates, branches, merge commits, diff sizes) is observed; a one-line PR summary restates its body and is stated unless tagged otherwise.

## Snapshot

- The repository now resolves to `1vecera/richview` (private): `gh api repos/richview-universe/richview-v3` returns `full_name: 1vecera/richview`, same id `1245394404`. Links inside PR bodies still use the old `richview-universe/richview-v3` URLs, which redirect. PR numbers below refer to this repository. **O**
- `origin/main` is `b4db233` (merge of PR #61, 2026-10-16 17:18 CEST). It has 370 commits: 3 in May, 12 in June, 1 in July, 8 in September and 346 in October 2026; 368 by Daniel Vecera's git identity, 2 by Steven Oliver (PR #1). **O**
- 75 PRs: 61 merged (59 into `main`, #71 and #72 into `publish/news`), 9 open drafts (#62–#70), 5 closed unmerged (#2, #3, #73–#75). 74 were authored and 60 merged by the `1vecera` account; PR #1 was authored and merged by `stevenoliver7` 69 seconds after it opened. **O** Agents worked through Daniel's account, so GitHub cannot distinguish Daniel's own clicks from authorized agent actions. **I**
- Daniel's primary local checkout, which also holds the task worktrees, sits on `main` at `5ceb7d4` (PR #4, 2026-07-07), 354 commits behind `origin/main`; its `CLAUDE.md` describes the obsolete TypeScript product. **O**
- Hosted CI: the last three Actions runs that executed steps were on 2026-10-13 (two on `main` after the PR #58 and #59 merges, one on the #59 branch), and all three failed at `node scripts/verify-browser.mjs`. Every run from 2026-10-14 to 2026-10-19 (138 runs) ended `failure` with zero steps executed, the billing/spending block that PRs #60, #61, #64, #68 and #70 report. **O**
- Branch protection cannot be read or set on this private repository: `GET /repos/1vecera/richview/branches/main/protection` returns 403 "Upgrade to GitHub Pro or make this repository public to enable this feature." **O**
- One tag and one GitHub pre-release exist: `chart-refinements-2026-10-12` at `5d47fbd` ("Chart refinements — 12 October 2026", pre-release). **O**

## Eras

| Era | Dates | Defining evidence | Stack and chart engine | What changed and why |
| --- | --- | --- | --- | --- |
| Before v3 | to May 2026 | `richview-universe/richview-v2` (private, last push 2026-05-11, "AI-native report creation platform"); older private repos `mvp`, `RichViewSDK`, `RichViewPublicRestAPI`, `user_management`, `mlservice` and the public `richview-docs` ("RichView Platform Documentation"), last pushes 2026-02-25/26; scaffold commit `3c808bd` | v2: 9 packages and a web editor | v3 began as "a clean rewrite that cuts v2's 9 packages down to 4 and drops the web editor entirely" (`3c808bd`). **O** for repos, **S** for v2 content |
| v3 TypeScript scaffold | 2026-05-21 → 06-09 | `3c808bd`, `ae2a1cb`, `91f2fe6`, `9e33305`, `71c749d`, `52303a8`, `4cab1f5`, `ce68a8d` | pnpm monorepo: `@richview/core`, `cli`, `server` (Hono on Fly.io, `richview-v3.fly.dev`), VS Code extension; ECharts inline; then a D3 self-measuring line engine | 2026-06-08 reposition memo ("charts that check their own work") drove the diagnostics contract, D3 line engine, `richview_diagnose` and the human report card; 2026-06-09 chart code moved to `@richview/charts` and licences became MIT except the Elastic-2.0 server. **S** |
| vNext architecture | 2026-06-20 → 07-07 | PR #1; `architecture.html` (83 decisions, 2026-06-21); 2026-06-27 feedback; PRs #2, #3 (closed), #4 | Pure-TS SVG engine (PR #1) replaced within 17 days by ECharts 6.1 SSR-to-SVG (PR #4); `richview-charts` split to its own public repo | Daniel's 2026-06-20 lock reverted charting to ECharts in its own repo, kept the scope narrow and MCP-first; the 2026-06-27 feedback restored motion, added `.rstyle.yaml`, `richview learn`, a playground and "agentic" wording; PR #4 merged both. **S** |
| Simplification | 2026-09-05 | PR #5 (draft) | Same TS/ECharts stack | Render each chart once, source-only publishing, generated assets, retire agent scaffolding; merged only through PR #10 on 10-04. **S** |
| Rust rewrite | 2026-10-02 → 10-04 | PRs #6–#9 (drafts), PR #10, `tasks/richview-rust-migration.md` (in `2c926b0`, removed in `c53f7b6`) | Rust core, CLI, server, MCP, editor backend; Wasm in browser; `richview-chart` Rust engine for all chart types; ECharts and TS engines removed | "This is Daniel's bet on Rust for the full product, superseding the earlier instruction to keep the Rust charting experiment separate"; hard constraint "Rust owns all first-party product behavior". PR #10: +276,031/−17,155 lines, 529 files. **S**; diff size **O** |
| Public teaser and release hardening | 2026-10-04 → 10-11 | PRs #11–#22 | Cloudflare Workers Static Assets at `richview.ai` | Teaser launch (Daniel authorized merge and deploy 2026-10-04), Highcharts benchmarks, report folders and VS Code authoring, docs, homepage and signup, report design, OKLCH, hand-drawn style, PDF export. **S** |
| Chart correctness campaign | 2026-10-11 → 10-12 | PRs #23–#56, integration PRs #28, #33, label/curve PR #40; tag `chart-refinements-2026-10-12` | Rust engine | 34 single-defect PRs, each with matched before/after evidence and independent review, then a fixed 60-page/540-figure review matrix; deployed 2026-10-12. **S**; tag **O** |
| Feedback rounds and quality sprint | 2026-10-12 → 10-16 | PRs #57–#61 | Rust engine; 9 chart types (histogram, box plot, interval added in #60) | Daniel's 2026-10-12 and 2026-10-13 feedback on landing, showcase, measure, tooltips; the Linear chart and imagery issues. Hosted CI stops executing from 10-14. **S**; CI **O** |
| Parallel product tracks (unmerged) | 2026-10-17 → 10-19 | Draft PRs #62–#70; content PRs #71–#75 | New crates and JS gateways on top of `b4db233` | Daniel's 2026-10-17 and 2026-10-18 batches opened playback controls, tabs, connectors, a voice dashboard, galleries, chart polish, a dev harness, a site repair and RichView Publish "despite older August scope locks"; three were deployed live from unmerged branches. **S**; live state **O** |
| Lockdown | 2026-10-21 | This branch; repository transfer | — | Repository moved to `1vecera/richview`; rewrite planned from scratch. **O** |

## Chart engine lineage

| Date | Engine | Evidence | Stated reason |
| --- | --- | --- | --- |
| 2026-05-21 | Apache ECharts, CDN script plus client init, inline in the HTML | `3c808bd` ("HTML renderer with inline ECharts and OKLCH theming"); PR #1 ("Replaces ECharts (CDN script + client init)") | Scaffold default. **S** |
| 2026-06-08 | D3 self-measuring engine for line charts only; other types ECharts and honestly `unmeasured` | `91f2fe6`, `9e33305`; reposition memo D7 | "D3 gives the scales/positions for free; ECharts hides them, so checks would be guesswork." **S** |
| 2026-06-20 | Zero-dependency pure-TS SVG engine for all six types; ECharts and D3 dropped; zero client JS; every chart measured | PR #1 (Steven Oliver) | "avoid external dependencies and build something both agents and humans can understand." **S** |
| 2026-06-20/21 | Decision to revert to ECharts behind a thin contract in its own repo; self-measuring engine demoted | `architecture.html` dec-38, dec-43, dec-44, dec-48, dec-50, dec-80 | "supplier stability + agent-editable contract outweigh the minimum-code principle for the chart surface specifically" (Daniel's 2026-06-20 lock). **S** |
| 2026-07-06 | PR #2 kept the pure-TS engine ("the June 2026 ECharts revert stands") | PR #2 body | Conflicting reading of the same lock; PR #2 closed unmerged. **S** |
| 2026-07-07 | ECharts 6.1 SSR-to-SVG (`renderToSVGString`), analytic diagnostics layer kept, zero `<script>` in output; `richview-charts` repo split by `git filter-repo` | PR #4 (`82de97f`, `5c40357`); `richview-universe/richview-charts` (public, description still "Apache ECharts SSR-to-SVG renderer", last push 2026-07-07) | dec-38/44/48; diagnostics kept against dec-50 ("your call"). **S**; repo **O** |
| 2026-09-05 | ECharts kept, only used modules bundled, each chart rendered once | PR #5 | Simplification. **S** |
| 2026-10-02 | `richview-chart`: separate Rust time-series engine with Wasm runtime (zoom, exact hover, brush, M4 decimation, UTC calendar) | PR #9 | Static ECharts SVG gave no zoom, exact values or stepping. **S** |
| 2026-10-04 | Shared Rust/Wasm engine renders every chart type natively and in the browser; ECharts and TS engines removed | PR #10 (`2c926b0`, `c53f7b6`) | Daniel's full-Rust bet. **S**; no `echarts` in `crates/` or `richview-chart/` **O** |
| 2026-10-04/05 | Highcharts 13.1.1, then ECharts 6.1.0, Plotly 4.1.1 and ggplot2 4.0.3 used as local reference benchmarks only | PRs #14, #16; `benchmarks/` | Quality bar; references stay local and out of production. **S** |
| 2026-10-15 | Nine source-format types: line, bar, scatter, area, pie, heatmap, histogram, boxplot, interval | PR #60; `CLAUDE.md:70`; `docs/RMD-SPEC.md` | Quality sprint. **S**; nine types in docs **O** |

The "zero client JavaScript" property followed the same path: introduced as a goal by PR #1, named as a deliberate reversal by dec-39, reasserted by PR #4 ("zero `<script>`"), then replaced in the Rust era by static first frames plus a Wasm runtime that hydrates interaction (PR #22, PR #19 "Native canonical exports pass with JavaScript disabled"). **S**

## Merged pull requests by theme

Dates are merge dates. "via #N" means GitHub marked the PR merged because integration PR #N carried its head into `main` while it was still a draft (20 PRs: #5–#9 via #10; #17, #20 via #21; #23–#27, #29, #30 via #28; #31, #32, #34–#37 via #33). Squash merges: #4, #15, #16, #18, #58, #59. **O**

Merge and deployment authority, as PR bodies record it: "Daniel authorized merging and deploying the prepared release on 4 October 2026" (#12, #13); "Daniel authorized merge and publication" (#18); "Daniel authorized merging this reviewed PR batch on 11 October 2026" (#22); "Daniel explicitly authorized merging sensible open PRs and deploying the resulting release" (#33); deployment "following the explicit request to deploy all finished work" (#40); public publication of the galleries and of the 2026-10-18 site repairs from unmerged branches (the site-repair briefs). Batch authorizations, integration PRs and deploys from task branches were the working delivery model; there was no `dev`/`prod` split. **S**, model **I**

### Engine, architecture and native stack

| PR | Merged | Summary | Ticket |
| --- | --- | --- | --- |
| #1 | 06-20 | Pure-TS SVG engine for all six chart types; drops ECharts CDN and D3; zero client JS; every chart self-measured. | — |
| #4 | 07-07 | Merges #2 and #3, then conforms to `architecture.html`: Newsreader and IBM Plex Mono inlined, MCP tools 6→5, `description`/`version` frontmatter, motion section, append-only versioned server, ECharts SSR engine, chart contract slim plus rich, callouts `up/down/neutral/hold`, `richview-charts` split, three builtins (sleek, scientific, brutalist), `editorial` alias. | — (`architecture.html`) |
| #5 | 10-04 via #10 | Render each chart once, source-only publishing, assets generated from canonical sources, agent scaffolding retired, 79 behavioral tests. | `tasks/simplify-toolset.md` |
| #9 | 10-04 via #10 | `richview-chart` Rust time-series engine and interactive showcase (NOAA CO₂, BLS CPI, SMARD quarter-hours). | Linear: Rust chart library |
| #10 | 10-04 | Ports the whole product to shared Rust/Wasm and integrates #5–#9 through one commit per PR; 25 fixed performance budgets. | Rust migration brief |
| #11 | 10-04 | Installed native commands find packaged web assets (`<prefix>/share/richview/web`). | — |

### Flagship report and showcase

| PR | Merged | Summary | Ticket |
| --- | --- | --- | --- |
| #6 | 10-04 via #10 | OpenAI-vs-Anthropic flagship: 13 sections, 18 charts, reproducible research pipeline and source ledger, no hand-typed numbers. | Linear: flagship research |
| #8 | 10-04 via #10 | 23-style theme showcase of the flagship: live switching, comparison, style-file drawer, colour-blind-safe company colours. | Linear: flagship repair |
| #19 | 10-11 | Repairs the 150 px flagship iframe, citation focus and black chart text on dark themes. | Linear: flagship repair |
| #17 | 10-11 via #21 | Polishes and publishes the flagship and docs through a bounded staging tool; deployed to richview.ai. | Linear: repository clean-up and flagship publication |

### Website, docs and deployment

| PR | Merged | Summary | Ticket |
| --- | --- | --- | --- |
| #7 | 10-04 via #10 | Scroll-driven beta landing where the RichView mark becomes a chart, plus a 15-second reel; fixes static-file 404s, cropped charts and styles that did not style charts. | Linear: scrolling beta website |
| #12 | 10-04 | Teaser copy describes the Rust beta; legal pages corrected. "Daniel authorized merging and deploying the prepared release on 4 October 2026." | Linear: teaser co-work |
| #13 | 10-04 | Cloudflare Workers Static Assets configuration; richview.ai released. | Linear: teaser co-work |
| #18 | 10-05 | Six tested guides and a first-report tutorial; `/docs/` deployed. | Linear: docs publication |
| #20 | 10-11 via #21 | Coming-soon homepage with a live source-to-report editor and a private D1 email signup. | Linear: coming-soon homepage |
| #21 | 10-11 | Restores the scroll story around the live example; theme buttons rewrite the visible `styles:` line; flagship house-style buttons and captioned photos; copy "RichView will be fully open source." | Linear: visual motion restore |
| #57 | 10-12 | Keeps homepage theme controls inside 390 px phones. | — |
| #58 | 10-13 | Applies the 2026-10-12 voice feedback to the landing (drops Coming soon, background mark animation and playground links; new lede; five themes), drops the showcase header controls and defaults charts to the text measure (2026-10-13). | — |

### Authoring and editor

| PR | Merged | Summary | Ticket |
| --- | --- | --- | --- |
| #15 | 10-05 | Dedicated report folders (`report.rmd`, optional `report.rstyle.yaml`, `Rendered/`), chart-local inline YAML by default, header datasets rejected, Rust linting/formatting/completion, VS Code providers, VSIX 3.0.0-alpha.1; table-versus-series choice left pending. | Linear: the feedback issue, its chart-data, table-default and report-folder sub-issues, and VS Code tooling |

### Report design, motion and export

| PR | Merged | Summary | Ticket |
| --- | --- | --- | --- |
| #22 | 10-11 | Report typography and figure frame; honest category labels, gaps and axes; static-first chart mounting with structural entrances; hand-drawn fifth treatment; one shared OKLCH module; `generate --to pdf`; resumable S3-compatible asset uploads in CLI and MCP. | Linear: the product brief |
| #59 | 10-13 | Translucent tooltip (82% surface, 6 px blur) and source captions tight under their chart. | 2026-10-13 feedback |
| #60 | 10-16 | Histogram, R-7 box plot and authored interval charts; exact source values; Table/SVG/CSV/Expand on the subtitle row; ink alignment; rounded bar ends; monotone cubic default; flagship stock images replaced. | Linear: expand toggle, ink alignment, bar rounding, line curvature and flagship imagery issues |
| #61 | 10-16 | Caption-aligned titles, distinct Table/SVG/CSV icons, landscape phone expansion dialog, print-settlement fix. | — |

### Chart craft and benchmarks

| PR | Merged | Summary | Ticket |
| --- | --- | --- | --- |
| #14 | 10-05 | Rust layout, tooltip and temporal navigation polish against a local Highcharts 13.1.1 benchmark; browser glyph issues 110 → 45 → 3 → 0. | Linear: Highcharts-quality iteration |
| #16 | 10-05 | "Daniel's chart feedback": clearer axes, attached units, in-chart series filtering, numeric zoom, pie emphasis, stronger zero lines, rounded bars, gapless heatmaps; Sleek Light added; four-engine benchmark and Figma review file. | Linear: Highcharts-quality iteration |
| #40 | 10-12 | Collision-aware labels, bounded monotone curves, one calendar-label row, plain zero, direct line names, heatmap colour bars removed, hand-drawn lettering; fixed review matrix glyph findings 115 → 0; deployed to richview.ai "following the explicit request to deploy all finished work". | — |

### Chart correctness refinements (2026-10-11/12)

| PR | Summary |
| --- | --- |
| #23 | Exact readouts honour authored currency and percent formats with all source digits. |
| #24 | Crowded pies keep complete labels by moving to a wrapped key and growing the chart. |
| #25 | Bubble area proportional to the source quantity; zero has no area. |
| #26 | Small percentage ticks keep enough precision to stay distinct. |
| #27 | Scatter plots keep one consistent zero rule. |
| #28 | Integrates #23–#27, #29, #30. |
| #29 | Authored missing heatmap observations show an outlined `n/a` cell. |
| #30 | Invalid time series show "Cannot draw this time series" instead of falling back to a categorical chart. |
| #31 | Percent unit printed once (no `15.0%%`). |
| #32 | Pies with no positive values say so. |
| #33 | Integrates #31, #32, #34–#37; "Daniel explicitly authorized merging sensible open PRs and deploying the resulting release." |
| #34 | Motion verifier race fix (test only). |
| #35 | Numeric-string X values survive zoom. |
| #36 | Pie annotations stay on their authored category. |
| #37 | Bars coloured by their own category use one slot per category. |
| #38 | Legend toggles work for numeric and boolean series. |
| #39 | Percentage scatters drop unused padding across zero. |
| #41 | Conflicting numeric heatmap cells are rejected rather than overpainted. |
| #42 | Tiny pie shares read `<0.1%` and `>99.9%`, not `0.0%`/`100.0%`. |
| #43 | Exact zero bars have no filled length. |
| #44 | Repeated bar observations are rejected (`chart.ambiguous-bar-slot`). |
| #45 | Values without coordinates stay out of the heatmap colour scale (`chart.unlocated-color-values`). |
| #46 | Bubble charts get one local size reference. |
| #47 | Tiny pie values are inspectable through their key entry. |
| #48 | Line and area segments stay continuous through numeric zoom. |
| #49 | Facet category colours stay stable after hydration. |
| #50 | Heatmap key keeps both endpoints when they fit (the key itself was removed by #40). |
| #51 | Step-aware scientific notation for tiny and huge axes. |
| #52 | SVG paint resources scoped to each live chart. |
| #53 | Pie decals match bar hatches. |
| #54 | Tiny heatmap values are not printed as `0`. |
| #55 | Heatmap cell text picks black or white from the cell's actual colour. |
| #56 | Bar legend swatches show their hatches. |

### Publish content and closed PRs

| PR | State | Summary |
| --- | --- | --- |
| #2 | closed 07-07 | 22-item 2026-06-27 feedback implementation; superseded by #4. Comment: "Superseded by #4, which merges this branch + #3 … into conformance with richview-design-system/public/architecture.html". |
| #3 | closed 07-07 | Chart design system and a Fraunces display title; superseded by #4. Comment: "The Fraunces/title work from this PR is replaced in #4 by the spec-mandated Newsreader + IBM Plex Mono inlined fonts (dec-23/24) per Daniel's confirmation". |
| #71, #72 | merged into `publish/news` 10-18 | Browser-desk edits of `publications/news/items/publishing-loop/report.rmd` as RichView Publish acceptance. |
| #73–#75 | closed 10-18 | Publish acceptance of edit, hide and restore; closed by design. |

## Decision records

### June 2026 reposition memo, D1–D10 (2026-06-08)

Source: `docs/reposition/index.html` added in `6489a06`, removed from `main` in `c53f7b6` (PR #10). The page preset each question to its recommendation and asked Daniel to paste back his picks; no recorded answer exists in git. The commits that followed implemented D3, D4, D5 and D7 as recommended (`9e33305`, `52303a8` "D4 visual surface", `71c749d`). **O** for content, **I** for adoption.

| ID | Question | Preset recommendation | Later fate |
| --- | --- | --- | --- |
| D1 | First user | The AI coding agent | Kept by dec-2, later broadened (see D-SCOPE-CORE). **I** |
| D2 | Headline | "Charts that check their own work" | Replaced by "agented/agentic presentation layer" (dec-1, 2026-06-27 feedback). **S** |
| D3 | Chart library as hero | Yes | Demoted by dec-79; diagnostics kept but not the headline. **S** |
| D4 | Validation surface | JSON for agents plus a visual report card | Both shipped 2026-06-09; report card demoted (architecture migration table). **S** |
| D5 | Chart authoring | Tiny YAML vocabulary, never chart code | Still the contract ("agents never write raw chart JSON"). **S** |
| D6 | v1 scope | line/bar/scatter first | Superseded: six types by PR #1, nine by PR #60. **O** |
| D7 | Engine | D3 self-measuring engine | Reverted 2026-06-20 (dec-38). **S** |
| D8 | Two-audience story | Agents create, humans curate | Current docs: "Analysts and coding agents" (`docs/OVERVIEW.md:3`). **O** |
| D9 | Success signal | Agents reach "verified clean" unattended and a human keeps the report | No later record. **I** |
| D10 | Brand | Keep RichView and richview.uk with a heritage nod | Live product moved to richview.ai (PR #13); CLI still defaults to `https://richview.uk` (`crates/richview-app/src/cli.rs:122`). **O** |

### Architecture decisions dec-1 to dec-83 (2026-06-21)

Source: `richview-universe/richview-design-system@8c6a8e0:public/architecture.html`, table "Consolidated decisions", rows `id="dec-1"` to `id="dec-83"`, synthesized from one parent ticket, ten child tickets and "Daniel's 2026-06-20 voice dump". The PR #4 body says "A full-codebase audit (all 83 decisions + all 22 feedback items …) drove this pass". Status is at `b4db233`. **O** for the decision text.

| ID | Decision (short) | Status at `b4db233` | Evidence | Tag |
| --- | --- | --- | --- | --- |
| dec-1 | Lead word "agented"; "The agented presentation layer for data stories" | Superseded: "agentic" (2026-06-27 feedback); landing h1 now "RichView is a presentation layer for your agent." | PR #2; `assets/landing/index.html:64` | O |
| dec-2 | Single audience: coding-agent drivers | Broadened to analysts, newsrooms, dashboard users | `docs/OVERVIEW.md:3`; Publish brief | S |
| dec-3 | Build→review→improve→share loop | Kept as edit→preview→diagnose→improve | `docs/OVERVIEW.md:3` | S |
| dec-4 | Keep `.rmd`; optional `format: rmd/0.1`; languageId `richview` | `.rmd` kept; no `rmd/0.1` discriminator anywhere in the crates or spec | `grep rmd/0` over `crates/`, `richview-chart/`, `docs/RMD-SPEC.md` | O |
| dec-5 | Web surface is a read-only showcase; "no chat in the face" | Partly reversed: live editable homepage demo (#21); chat/voice dashboard open (#64) | PRs #20, #21, #64 | S |
| dec-6 | Minimum code as a framing anchor | In tension: 194 tracked files at `5ceb7d4`, 985 at `b4db233` | `git ls-tree` | O |
| dec-7 | Six tags; `media` collapses audio and video | Changed: nine tags incl. `metric` alias and separate `audio`, `video` | `parser.rs:9` | O |
| dec-8 | Exactly three required keys: title, description, version | Changed: title and version required to publish; missing description only warns | `parser.rs:735-749`; `CLAUDE.md` | O |
| dec-9 | `format` optional | Moot: the key is not implemented (see dec-4) | — | O |
| dec-10 | Assertion-style title; description as lede | Kept as guidance: chart without a title warns "Add a chart title that states the claim" | `parser.rs:139-141`; PR #4 | O |
| dec-11 | YAML-only fences; unknown info-string stays code | Kept | `CLAUDE.md` ("Preserve … unknown metadata/fences") | S |
| dec-12 | Diagnostics `{level, message, tag?, line?}` | Extended: `{level, code, message, path, line?, tag?, details?}` | `crates/richview-core/src/lib.rs:59-71` | O |
| dec-13 | Style file = `tokens`, `layout`, `chart` | Extended: `tokens`, `layout`, `motion`, `components`, `chart` | `crates/richview-core/src/style.rs:554-642` | O |
| dec-14 | `chart:` is an opaque blob passed to ECharts | Obsolete: chart keys validated in core | `style.rs:632-642` | O |
| dec-15 | Two style sources: builtin or explicit path/URL | Builtins plus source-relative files through the bounded resolver | `CLAUDE.md` | S |
| dec-16 | Composition = ordered list | Kept (`styles: [sleek, report.rstyle.yaml]`) | PR #21 | S |
| dec-17 | Shallow last-wins cascade | Kept per docs | PR #2; `RMD-SPEC.md` | S |
| dec-18 | Unknown leaf keys rejected | Leaf keys validated per section | `style.rs:554-642` | I |
| dec-19 | Core pure; resolution in hosts | Kept | `CLAUDE.md` ("keep core free of filesystem, network and process I/O") | S |
| dec-20 | CSS validator is a security boundary | Kept | `CLAUDE.md` ("Keep HTML/CSS/URL safety") | S |
| dec-21 | Dark mode: `prefers-color-scheme` plus `theme:` | Kept (`theme` option `auto/light/dark`); light and dark are also separate styles | `crates/richview-app/src/cli.rs:20-24,64`; `style.rs:29-33` | O |
| dec-22 | System fonts only; no remote font URLs | Changed: bundled licensed faces per style | `crates/richview-core/assets/fonts-*.css` | O |
| dec-23 | Editorial = Newsreader + IBM Plex Mono; Inter retired | Superseded by per-style faces (#4) and `RichView Serif/Sans/Code/Mono` aliases (#22); `fonts-editorial.css` remains | PR #4, PR #22 | S |
| dec-24 | 48 KB base64 font ceiling per style | Retired 2026-07-07 for variant styles; "the real budget is rendered-report size (≤5MB hard ceiling)" | `5ceb7d4` message | S |
| dec-25 | OKLCH in files; hex only at chart boundary | Extended: OKLCH throughout, enforced by test | `crates/richview-core/tests/palettes.rs:85` | O |
| dec-26 | Style emits `chartDefaults`, does not own the schema | Obsolete | — | I |
| dec-27 | Motion dropped from v1 | Reversed by 2026-06-27 feedback; `motion` section exists | PR #4; `style.rs:572` | O |
| dec-28 | One ship gate: `richview check --anti-slop` linter | Not implemented; anti-slop lives in `learn` and the skill | `crates/richview-core/src/knowledge.rs:17`; `skill/reference/anti-slop.md` | O |
| dec-29 | One accent hue; callouts `up/down/neutral/hold` | Callout kinds kept; accent lint not found | `parser.rs:615-621` | O |
| dec-30 | Type weights capped at 3 | Kept; the only `dec-` reference left in the code | `assets/styles/tokens.css:72-74` | O |
| dec-31 | Light and dark paired in one style file | Superseded: `sleek` and `sleek-light` are separate builtins | `style.rs:31-32` | O |
| dec-32 | Layout limited to a margin-column switch and full-bleed | `layout.margin-column` boolean exists | `style.rs:569` | O |
| dec-33 | Two repos at alpha (charts split) | Executed 2026-07-07, reversed by the Rust monorepo; public `richview-charts` left stale; no npm package published | `gh repo view`; `npm view @richview/charts` 404 | O |
| dec-34 | Website split parked post-alpha | Site still in the monorepo (`assets/landing/`) | tree | O |
| dec-35 | One chart package with contract/engine directories | Obsolete (Rust crate `richview-chart/`) | tree | O |
| dec-36 | No engine interface or registry | One shared engine | `docs/RUST-ARCHITECTURE.md` | I |
| dec-37 | Engine returns `{svg, diagnostics}` only | Changed: also a runtime payload for browser hydration | PR #9, PR #62 | S |
| dec-38 | ECharts SSR-to-SVG, tree-shaken, inlined | Reversed 2026-10-04 | PR #10 | O |
| dec-39 | ECharts revert reintroduces client JS | Superseded: Wasm runtime | PR #10 | S |
| dec-40 | Ship `unmeasured` honestly | Superseded: native estimates and browser glyph measurement | `CLAUDE.md` | S |
| dec-41 | `<rv-chart>` render and resize only | Superseded: zoom, brush, keyboard, table, export | PRs #9, #14, #16 | S |
| dec-42 | Four required chart keys; subtitle into title; axes and annotations into style; `verify` removed | Partly reversed: subtitle, annotations, axes first-class (#4); missing title warns instead of failing | PR #4; `parser.rs:139` | O |
| dec-43 | `chartToEChartsOption()` as the only bridge | Obsolete | PR #10 | O |
| dec-44 | SSR-SVG only; no hydration or cross-filtering | Reversed | PRs #10, #62, #64 | S |
| dec-45 | Core imports only `ChartSchema` | Obsolete (core renders through `richview-chart`) | `RUST-ARCHITECTURE.md` | I |
| dec-46 | Theme resolved at render; no light/dark toggle | Changed: runtime theme switching in showcase and homepage | PRs #8, #21 | S |
| dec-47 | `editorial` palette only | Revised 2026-07-07 to per-style palettes; validated categorical and sequential/diverging ramps (#22) | `5ceb7d4` message; PR #22 | S |
| dec-48 | Charting is an MIT leaf with no diagnostics API | Reversed (diagnostics kept) | PR #4 | S |
| dec-49 | Canvas opt-in out of v1 | SVG kept; PNG only as a download rasteriser in open #67 | PR #9, PR #67 | I |
| dec-50 | Diagnostics surface removed from v1 | Reversed: "diagnostics KEPT (your call)" | PR #4 body | S |
| dec-51 | Progressive enhancement; meaningful without JS | Kept | PR #19, PR #22 | S |
| dec-52 | ECharts interactivity plus vanilla TS custom elements | Superseded by Rust/Wasm DOM code | PR #10 | S |
| dec-53 | No React, htmx, Alpine, Preact | Kept | no such dependencies | I |
| dec-54 | ECharts inlined once per document | Superseded: Wasm runtime embedded in offline exports | `RUST-ARCHITECTURE.md:21` | S |
| dec-55 | Vitest plus Playwright test split | Superseded: `cargo test` plus Node/Playwright verifiers | `CLAUDE.md` | S |
| dec-56 | Four MCP tools | Five after the 2026-06-27 feedback added `learn`; seven today with `richview_asset_put` and `richview_asset_verify` | `crates/richview-app/src/mcp.rs:12-43`; `asset_upload/mod.rs:315,326` | O |
| dec-57 | Rename `diagnose` to `check` | Kept | `mcp.rs:24` | O |
| dec-58 | `isError` only for hard failures | Kept per PR #4; invalid sources exit 1 (PR #30) | PR #4, PR #30 | S |
| dec-59 | `render` returns summary or file, never an HTML string | Kept, file only: render writes into `Rendered/` and returns `output_path`, counts and diagnostics; no summary/file switch | `crates/richview-app/src/mcp.rs:13,289` | O |
| dec-60 | One anti-slop ruleset | Kept | `skill/reference/anti-slop.md` | O |
| dec-61 | Publish needs a token; `confirm: true` on first publish | Token kept; no `confirm` handling found | `crates/richview-app/src/server.rs:225-234`; `mcp.rs` | O |
| dec-62 | One CLI/MCP vocabulary | Kept and extended (`learn`, `asset`, `server`, `generate --to pdf`) | `cli.rs` | O |
| dec-63 | JSON envelope `{ok, command, result, diagnostics, version}` | Not as specified: `{ok, output, diagnostics, renderDiagnostics, …}` | `cli.rs:211-300` | O |
| dec-64 | `styles` single value on render | Changed: an array of style names or files in MCP and CLI | `mcp.rs:8,168-170` ("styles must be an array of strings"); `cli.rs:66` | O |
| dec-65 | Structured errors `{code, message, location, expected?, fix?, retry}` | Partly: diagnostics carry code, path and line; chart errors carry JSON-pointer paths and hints | `parser.rs`; PR #9 | S |
| dec-66 | VS Code: three affordances | Expanded: linting, formatting, completion, outline, folding, live styling, export | PRs #15, #60 | S |
| dec-67 | Slug with append-only immutable versions | Kept | `crates/richview-app/src/storage.rs:54` | O |
| dec-68 | Full rendered HTML per version | Kept; 50 versions retained | `storage.rs:13` | O |
| dec-69 | Publish key `{slug, version}` semver | Kept | `CLAUDE.md` | S |
| dec-70 | Collections derived; route deferred | Open: RichView Publish adds sections and archives (#70) | PR #70 | S |
| dec-71 | Marketing and playground move to a website repo; server is a pure report host | Not done: native server serves landing, playground, showcase and reel; public site on Cloudflare | `CLAUDE.md`; PR #13 | S |
| dec-72 | Server always re-renders from submitted source | Kept ("Source-only publication requests") | PR #10 | S |
| dec-73 | Idempotent publish per `{slug, version}` | Kept | `storage.rs:54` | O |
| dec-74 | Single bearer token; public by default | Kept for the native server; Publish uses Cloudflare Access | `server.rs:225-234`; PR #70 | O |
| dec-75 | Playground ships one chart-free `.rmd` | Superseded | PRs #20, #21 | S |
| dec-76 | Client render waits for chart migration | Superseded (Wasm) | PR #10 | S |
| dec-77 | Playground with six affordances | Partly: playground links dropped from the landing (2026-10-12); `/playground` still served | PR #58; `GET richview.ai/playground` 200 | O |
| dec-78 | "the rendering engine for data stories that coding agents write and humans review" | Evolved: "turns Rich Markdown (`.rmd`) into sourced, editable reports" | `README.md:3` | O |
| dec-79 | Wedge = loop + taste + MCP; diagnostics demoted | Partly reversed (diagnostics kept) | PR #4 | S |
| dec-80 | Accept ECharts as a code increase | Obsolete | PR #10 | O |
| dec-81 | Source-available (ELv2) go-to-market | Contradicted: crates MIT, live copy "fully open source", orphan ELv2 file | `Cargo.toml:8`; `assets/landing/index.html:368`; `packages/server/LICENSE` | O |
| dec-82 | RV2 mining is triage only | Historical | — | I |
| dec-83 | Linear protected-status protocol (HC-2) | Process rule; present in the old `CLAUDE.md`, absent from the current one | `5ceb7d4:CLAUDE.md`; `CLAUDE.md` | O |

The same document lists open questions for Daniel by dimension (headline, license copy, builtin set, chart-bundle budget, data binding, review surface, server license FSL/BSL/ELv2) and its biggest unverified bets: the rendered-versus-source review gap, the ECharts bundle size, the diagnostics demotion and the un-derived editorial palette. **O**

### Later decisions recorded in commits and PR bodies

| Date | Decision | Source | Current status |
| --- | --- | --- | --- |
| 2026-06-09 | Keep the monorepo; separate concerns as packages with an MIT-except-server licence gradient | `4cab1f5`, `ce68a8d` | Licence file contradicts current crates (see D-OPEN-SOURCE). **O** |
| 2026-06-27 | Motion is required: "we need interactivity and motion — antislop motion — beautiful soft animations of chart and texts" | `5ceb7d4` message (quoting the feedback) | Kept and extended. **S** |
| 2026-06-27 | Fonts: "Newsreader + IBM Plex Mono, inlined" (Q1); Fraunces dropped "per Daniel's confirmation" | `5ceb7d4`; PR #3 comment | Superseded by per-style faces. **S** |
| 2026-07-07 | Three builtins distilled 1:1 from studio variants v21-sleek, v22-scientific, v17-brutalist; `editorial` becomes a deprecated alias of brutalist (resolves oq-13) | `5ceb7d4` | Grew to five; `editorial` alias kept (`style.rs:40`). **O** |
| 2026-07-07 | Per-style palettes replace the single editorial palette; 48 KB font ceiling retired | `5ceb7d4` | Kept. **S** |
| 2026-07-07 | Copy "open source" changed to "source-available" because Elastic-2.0 is not OSI-approved | `5ceb7d4` | Reversed by "fully open source" on 2026-10-05 (PR #21). **S** |
| 2026-10-04 | Full Rust bet; Rust owns all product behavior; JS only for unavoidable host APIs | Rust migration brief in `2c926b0` | Softened by later briefs and PRs (see contradictions). **S** |
| 2026-10-04 | Host the public site on Cloudflare (Daniel "selected Cloudflare as the host") | PR #13 | Live. **O** |
| 2026-10-05 | Table authoring: keep row mappings (recommendation), series form "pending Daniel's decision" | `docs/TABLE-AUTHORING-CHOICE.md:1-5` | Still pending. **O** |
| 2026-10-05 | Frontmatter `data:` sources removed; data lives in each block | PR #15; `docs/RMD-SPEC.md:46` ("Hard migration error") | Kept. **O** |
| 2026-10-05 | Sleek Light joins Sleek, Scientific, Brutalist; `sleek` stays the dark compatibility name | PR #16 | Kept (`style.rs:29-33`). **O** |
| 2026-10-11 | Hand-drawn fifth treatment; custom styles must use `RichView Serif/Sans/Code/Mono` aliases | PR #22 | Kept. **S** |
| 2026-10-12 | Five canonical themes listed on the landing without a dropdown | PR #58 | Landing radio group lists sleek, sleek-light, scientific, brutalist, hand-drawn (`assets/landing/index.html:120-128`). **O** |
| 2026-10-13 | Charts and images default to the text measure, `--full` opt-in; tables keep full width | `429e54f` | Contradicted on 2026-10-18 for tables (see D-MEASURE). **S** |
| 2026-10-17 | New tracks authorized "despite older August scope locks"; galleries publication authorized | `tasks/richview-2026-10-17/*.md` | Unmerged. **S** |
| 2026-10-18 | "all the changes on the richview.ai should be published right away and pushed to completion, because the current state is very unsatisfactory." | `tasks/richview-2026-10-18/briefs/website.md` (Daniel quoted) | Deployed from an unmerged branch. **S** |
| 2026-10-18 | Product name "RichView Publish (the final correction from 'RichView Publishing')"; ecosystem: Rich Markdown, Charts, Reports, Dashboards, Publish, Server, VS Code | `tasks/richview-publish.md` on PR #70 | Unmerged. **S** |

### Decision references in the current tree

The only `dec-NN` reference left on `main` is `assets/styles/tokens.css:72` (dec-30). At `5ceb7d4` there were 56 matching lines across 21 files (code comments, tests, `CLAUDE.md`, `RMD-SPEC.md`); PR #10 removed the rest along with `docs/reposition/` and the task specs (`c53f7b6`). `docs/TABLE-AUTHORING-CHOICE.md` is the only document on `main` that records a decision as pending. **O**

## Review comments and change requests

### What GitHub records

Across all 75 PRs GitHub holds 2 issue comments, 0 reviews, 0 inline review comments and no review decision. Both comments come from the `1vecera` account on 2026-07-07 and close #2 and #3 as superseded by #4. Daniel's change requests therefore never arrived as GitHub reviews; they came through Linear, dictated briefs and feedback rounds, and survive in PR bodies and commit messages. **O** for counts, **I** for the channel.

### Feedback rounds encoded in PRs and commits

| Date | Channel | What Daniel asked for (quotes where the source quotes him) | Landed in |
| --- | --- | --- | --- |
| 2026-06-08 | Reposition memo D1–D10 | Recommendations preset for his review (see above) | `9e33305`, `52303a8` |
| 2026-06-20 | Voice dump and lock | Charting reverts to ECharts in its own repo; website in its own repo; minimum code; data stories only; "no chat in the face" (`tasks/richview-vnext-synthesis-handoff.md` hard constraint 2, in `5ceb7d4`) | `architecture.html`, PR #4 |
| 2026-06-27 | Review document with 22 items and screenshots (`richview-review-2026-06-27.md`, not in any branch) | "agented"→"agentic"; motto "Give your agent charting and data-presentation skills"; `.rstyle.yaml`; motion; "no pixel settings of height — responsive by default"; "Allow for subtitle in charts"; "Do a proper research on chart styling"; "allow annotations in a smart and simple way"; "amazing default designs"; `richview learn`; playground; "sharing through the richview server"; VS Code in the hero | PRs #2, #3, #4 |
| 2026-10-05 | Chart feedback | Clearer axes, compact calendar labels, attached and scaled units, in-chart series filtering, small action icons, continuous numeric zoom, pie emphasis, stronger zero lines, rounded bars, gapless heatmaps | PR #16 |
| 2026-10-11 | Requested refinements | Collision-aware names and annotations, configurable monotone curves, one calendar-label row, plain zero, direct line names, no heatmap colour bars, handwritten lettering, Brutalist heading gap; "deploy all finished work" | PR #40 |
| 2026-10-12 | Voice feedback | Drop the Coming soon pill, eyebrow and background mark animation; lede "Your agent organizes the text. RichView makes it a story."; align the logo dot; five themes, no dropdown; drop demo downloads and playground links; "Style Decided Once · Reused Every Time"; publishing server as a menu of finished reports with share/copy/protect; VS Code as live rendering and highlighting. Follow-ups: soft rounded bars, bars starting where the axis line ends, no "ticking" in entrance animation, all five themes end to end, clearer YAML-editor affordance, Marketplace copy, publishing-server UI | PR #58 |
| 2026-10-13 | Feedback | Showcase header buttons are "noise above the report"; charts and images "much wider than the prose column, which breaks the left reading spine" (FT.com-style measure); tooltips "feel heavy"; source captions float away. Open: expand affordance, left-ink alignment, line curvature, better flagship imagery, Table/SVG/CSV/Expand on the subtitle line | PRs #58, #59, #60 |
| 2026-10-17 | Batch of five tracks | Galleries publicly; playable charts with button-group selectors ("not a dropdown and not a single ambiguous toggle"); tabs; connectors; voice dashboard | Draft PRs #62–#66 |
| 2026-10-18 | Batch of three tracks | Consistent measure "including tables and headings"; replace the broken header chart with a finished infographic; five styles on top, "Other styles" dropdown; one download icon offering PNG or SVG ("the user's corrected formats are PNG/SVG, not PDF"); "DROP expand/collapse and the bottom 'Inspect chart' affordance. Provide ONE larger near-fullscreen/modal chart view"; years never shown as `1.70K`; scatter labels "much too dense", sparse defaults, author highlights with priorities, leaders only for forced highlights; line names without forced cues; stable, "slightly more transparent and visually lighter" tooltips; keep the praised "red nose" animation; restore the site's animation, cut navigation, put the chart gallery in the docs | Draft PRs #67, #68, #69 |
| 2026-10-18 16:03 | Review of PR #69's published state | Rejected: the docs gallery showed source without rendered charts; mobile landing scroll had "visible ticking and flicker" | `fdfb71a` |
| 2026-10-18 | Publish brief | RichView Publish and RichView News at `news.richview.ai`; Git canonical; one site YAML; editor, super editor and reader roles; urgent hide | Draft PR #70 |

Sources for the 2026-10-17/18 rows: `tasks/richview-2026-10-17/{showcase,playable-charts,report-tabs,data-connectors,voice-dashboard}.md` and `tasks/richview-2026-10-18/briefs/{charts,website,harness}.md` (coordination root on Claw Mini, outside the repository). These are coordinator briefs written from Daniel's dictation, so his intent is stated, not verbatim, except where quoted. **S**

## Open draft PRs #62–#70

All nine are drafts based on current `main` `b4db233`, 0 commits behind, and each merges cleanly into `main` on its own (`git merge-tree`). Pairwise they collide: #63×#67 (`crates/richview-web/Cargo.toml`), #64×#66 and #64×#70 (`Cargo.lock`), #65×#69 (`assets/landing/index.html`), #67×#69 (`richview-chart/src/svg.rs`). #67 and #69 each contain a different patch titled "Clamp SVG rect widths and heights to non-negative values" (`1d209a8` vs `52d07db`); #68 and #70 contain the identical "Make actual worker reply gating deterministic" patch (`41946e3` = `3e10460`, same patch-id). Hosted CI did not run on any of them. **O**

| PR | Branch | Ticket | Scope | State and evidence | Last update | Divergence | Live? |
| --- | --- | --- | --- | --- | --- | --- | --- |
| #62 | `feat/playable-charts` | Linear: playable charts | Declarative `controls:` on a chart block (`playback`, `filter`, `metric`, `variant`), button group then dropdowns, `rv:control` / `rv:control-set` events, reduced-motion floor; Gapminder and indicator examples; `docs/CONTROLS.md` | Feature complete per its brief; 20/20 Playwright checks and chart crate tests per body; `cargo test --workspace` skipped | 2026-10-18 09:56Z | +13 commits, 14 files, +2,751/−6 | No |
| #63 | `feat/multi-tab-reports` | Linear: multi-tab reports | `tabs:` frontmatter slicing the body at `## Title {#id}` anchors; static-first (all panels visible without JS); `export: all/active`; deep links and keyboard | WIP; body says no Rust gate run; body is stale (branch already has `scripts/verify-tabs.mjs` and contract tests) | 2026-10-18 03:07Z | +6, 10 files, +781/−2 | No |
| #64 | `voice-dashboard` | Linear: voice dashboard | Sibling crate `richview-voice-dashboard` (axum server, 347-line `app.js`): voice or text prompt → planner → six tiles over Okurka MetricFlow, cross-selection, local save; ElevenLabs Scribe server-side | Demo complete per body; cloud save returns "Unavailable"; remote MCP OAuth not done | 2026-10-18 04:15Z | +9, 18 files, +3,174 | No |
| #65 | `publish-galleries` | Linear: galleries | `/typography/`, `/chart-gallery/`, `/famous-charts/` (Gapminder, Playfair, Rosling, Snow, Nightingale) in five styles; landing Gallery link | Deployed 2026-10-17 under explicit authorization, then superseded: all three routes return 404 on 2026-10-21 | 2026-10-17 15:45Z | +4, 8 files, +1,142 | Was; now 404 (O) |
| #66 | `feat/data-connectors` | Linear: data connectors | Chart-local `data.sql`, `data.metricflow`, `data.cache`; DuckDB adapter; MetricFlow via `mf query`; SHA-256 snapshot store; `--connectors` / `connectors.toml` | Live SQL and MetricFlow tests pass per body; `verify-browser` failed three times under load, labelled unrelated | 2026-10-18 08:04Z | +6, 24 files, +2,731/−14 (713 lines `Cargo.lock`) | No |
| #67 | `task/chart-polish` | Linear: flagship and chart polish | Integer-year axes (`format: year`), lighter tooltip, labelled action buttons, one Expand modal, Download menu (PNG/SVG), five style pills plus Other, sparse scatter labels with `encoding.highlight` and priorities, opt-in `layout.measure-wide-blocks` | "in progress"; header infographic, tooltip stability, series cues and Playfair density not done per body; one unpushed local commit `ba394dd` in its worktree | 2026-10-19 00:54Z | +12 (13 locally), 9 files, +340/−56 | Unknown |
| #68 | `feat/development-harness` | Linear: development harness | `scripts/dev-harness/run.py`, build-lock and artifact-ownership checks, deterministic worker gating, `docs/architecture-current.md`, `docs/development-harness.md` | Done per body; original integration receipt FAILED on cleanup, fix then passed | 2026-10-18 13:44Z | +4, 7 files, +946/−8 | No |
| #69 | `task/site` | Linear: site motion and navigation | Restores `#rv-field`/`#rv-stage` landing motion, nav cut to Flagship report · Chart gallery · Docs, `/docs/charts` with all nine types, mobile chapter hysteresis | Live: richview.ai nav and `rv-field`/`rv-stage` match this branch, not `main`; `/docs/charts` serves nine type headings | 2026-10-19 00:45Z | +6, 9 files, +650/−24 | Yes (O) |
| #70 | `feat/richview-publish` | Linear: Publish and News | RichView Publish: `crates/richview-publish` (~1,000 Rust lines), Cloudflare gateway and browser editor (~4,000 lines `.mjs`), Access/R2/Durable Object, Git-PR editorial loop, one site YAML, two layouts, `publications/news/` | Live: `news.richview.ai` returns 200 "Latest perspectives — RichView News" without sign-in (O); content branch `publish/news` = 5 PR #70 commits + #71/#72 (O); "a trusted content-only polling worker" publishes approved commits (S) | 2026-10-18 22:11Z | +26, 52 files, +7,243/−8 | Yes (O) |

Other unmerged refs: `publish/news` (News content; keep while News runs), `review/validation-{main,pr40}-20261012{,-r2}` (four PR #40 validation compositions, conflict with `main`), `synth/vnext-architecture` (points at `18241a1`, no unique commits), `feat/visual-feedback-styling` (closed #3). Head branches of 56 merged PRs also remain on origin (51 contained in `main` by ancestry, 5 squash-merged: #15, #16, #18, #58, #59). **O**

Proposed disposition (each needs Daniel's decision; all **I**):

- **#62 → fold into the rewrite.** Keep the `controls:` contract, event names, reduced-motion floor and examples as a spec; do not merge code into a codebase being replaced. Decide first whether chart-local playback is in scope (D-CONTROLS-PR62).
- **#63 → fold or abandon.** Unverified WIP; the `tabs:` design (static-first, export control, h2 anchors) is a small, testable spec if multi-view reports are in scope (D-TABS-PR63).
- **#64 → abandon in this repo.** It is a sibling product with private-data dependencies (Okurka MetricFlow, ElevenLabs) and a JS front end; keep its evidence as a demo reference and decide dashboards under D-SCOPE-CORE (D-DASHBOARD-PR64).
- **#65 → abandon the PR, fold the content.** Its routes are already gone from production and the 2026-10-18 brief moved the gallery into the docs; the famous-chart recreations and typography specimen are reusable examples (D-GALLERIES-PR65; D-GALLERY-ROUTES of the Linear evidence (not published)).
- **#66 → fold as a later-phase spec.** The host-resolved, chart-local, snapshot-cached connector contract is coherent, but DuckDB adds heavy builds and live connectors widen scope (D-CONNECTORS-PR66).
- **#67 → fold.** Its behaviours are Daniel's latest dictated design calls and belong in the rewrite spec; the code is incomplete and conflicts with #69 (D-CHART-POLISH-PR67).
- **#68 → fold into the delivery gates.** Build-lock, artifact provenance and evidence-outside-worktree rules answer real failures; tooling, not product (D-HARNESS-PR68).
- **#69 → merge before the rewrite, then fold.** Production already runs it; merging keeps `main` equal to what is live and resolves the `svg.rs` overlap with #67 deliberately (D-SITE-PR69).
- **#70 → decide the product first.** News is live from an unmerged branch with a polling publisher; merge, freeze or retire depends on whether Publish is part of RichView (D-PUBLISH-PR70 and D-PUBLISH-TARGET of the Linear evidence (not published)). The repository transfer may affect the publisher's fixed repository bridge; verify it still fetches. **I**

## Repeated corrections (what agents kept getting wrong)

- **Styles did not reach the charts.** Hard-coded chart colours ignored the cascade (#3, 2026-07-06); ratio settings overrode the palette (#5); "House styles did not style charts … `sleek` drew chart text dark on dark" (#7); palette inheritance fixed again (#15, #16); dark themes left chart text black at 1.14:1 (#17, #19); one chart inherited another theme's hatch ink (#52). Seven rounds. **S**
- **Font declarations broke silently.** Unquoted `Source Serif 4` / `Source Sans 3` found independently by #7, #8 and #9 and fixed again in #15; Lora's tabular figures widened prose (#8); a loaded-font check was added because fallback was silent (#22). **S**
- **Charts clipped, cropped or collided.** Cropped axis labels and vanished short bars on `main` (#6, #7); glyph issues 110 → 0 (#14); hidden category names on phones (#22); clipped pie labels (#24); colliding percent ticks (#26); 115 → 0 overlaps (#40); then on 2026-10-18 scatter labels "much too dense" and years printed as `1.70K`. **S**
- **Too many controls.** 2026-10-12 dropped nav links, downloads and playground links; 2026-10-13 removed the showcase header buttons; 2026-10-18 cut the nav to three links and replaced unclear SVG/CSV glyphs with one labelled Download. **S**
- **Width and measure.** 2026-10-13: charts wider than the prose column; 2026-10-18: the measure must also hold for tables and headings. **S**
- **Tooltip weight, twice.** 2026-10-13 made it translucent (#59); 2026-10-18 asked for "slightly more transparent and visually lighter" (#67 `06d8691`). **S**
- **Motion.** Per-shape transforms distorted bars and legend swatches (#2 hardening); entrance "ticking" (2026-10-12 follow-up); the landing animation was removed (#58) and then requested back (2026-10-18); the restored scroll story "ticked and flickered" on mobile (2026-10-18 16:03 rejection). **S**
- **Requests misread or extended.** The chart gallery was built as one long report ("Replace the mistaken chart-gallery-as-report interpretation", 2026-10-18 website brief); an agent raised the tooltip blur as "my own addition … unrelated to Daniel's … request" and reverted it (`1d209a8`); briefs had to carry dictation self-corrections explicitly ("the user's corrected formats are PNG/SVG, not PDF"; "Do not implement the abandoned three-mode expansion request"). **S**
- **Preferences that changed between rounds (not agent errors).** Expand/Contract toggle (shipped in #60) → one modal (2026-10-18); background landing animation dropped (2026-10-12) → restored (2026-10-18); tables full width (2026-10-13) → tables at the text measure (2026-10-18); "source-available" copy (2026-07-07) → "fully open source" (2026-10-05). Each needs one recorded final answer. **S**
- **Silent no-ops.** Removing DOM nodes that Rust binders skip without error disabled the landing animation (`crates/richview-web/src/browser/landing.rs:2505-2507` returns `Ok(())` when `#rv-field`/`#rv-stage` are missing; `main`'s landing has neither) and hid showcase shortcuts (#58); docs charts stayed static because the Wasm runtime was never loaded (`6ccab91`). **O** for the landing, **S** otherwise.
- **Claims ahead of evidence.** #2's "closed vocabulary" was false (`.passthrough()`); PR docs claimed publish-required keys the schema lacked (#4); docs claimed a dependency-free self-measuring renderer while code used ECharts (Rust brief); #58 merged with its test plan unchecked and `main` CI then failed at `verify-browser` (2026-10-13); briefs had to say "To Review does not establish that every quality gate passed". **S**, CI **O**
- **Stale bases.** The primary checkout is 354 commits behind; every 2026-10-17/18 brief repeats "The local checked-out README describes obsolete TypeScript code" / "The primary checkout is stale"; the current `CLAUDE.md:76` still names three canonical styles while code ships five (`style.rs:16-25`). **O**
- **Parallel agents collided.** A shared Cargo target served a builder compiled for another worktree, so "a successful command can stage and verify the wrong source" (#68, #69 provenance note); duplicate and conflicting fixes (#67/#69, #68/#70); file conflicts across five pairs. **O** for conflicts, **S** for causes.
- **Production drifted from `main`.** #65, #69 and #70 were deployed from unmerged branches under scoped authorizations; the galleries were then removed from production; richview.ai and news.richview.ai now serve code that is not on `main`. **O**

## Contradictions

Each bullet carries the tag of its evidence; mixed bullets name which part is stated.

- **Chart engine.** PR #1 removed ECharts on 2026-06-20, the same day Daniel's lock reverted to ECharts (dec-38); PR #2 kept the TS engine, PR #4 switched to ECharts the next day; PR #10 replaced ECharts with Rust. `richview-charts` still describes the ECharts engine publicly. **O**. Forces D-CHART-SUPPLIER and D-REPO-SPLIT.
- **Zero JavaScript vs interactivity.** PR #1 and PR #4 promised zero client JS; the Rust era hydrates every chart with Wasm and keeps static first frames. **S**. Forces D-INTERACTIVITY.
- **Diagnostics.** The reposition memo made them the hero; dec-50 removed them; PR #4 kept them as "the product's IP"; the Rust brief requires geometry-based readability diagnostics. **S**. Forces D-DIAGNOSTICS.
- **Licence and openness.** `LICENSING.md` and `packages/server/LICENSE` describe an MIT/Elastic-2.0 gradient for TypeScript packages that no longer exist; `Cargo.toml:8` and the crates say MIT; PR #4 changed copy to "source-available"; the live landing promises "RichView will be fully open source" (`assets/landing/index.html:368`); dec-81 chose ELv2; the repository is private. **O** (copy change **S**). Forces D-OPEN-SOURCE.
- **Style set.** `CLAUDE.md:76` says three canonical styles; core registers five (`style.rs:16-25`) and `docs/CHART-REVIEW.md:5` reviews five; the showcase exposes 23 (`assets/showcase/themes.json`: sleek, scientific, brutalist and studio variants v01–v20, without sleek-light or hand-drawn). **O**. Forces D-STYLE-CATALOG and D-DEFAULT-STYLE.
- **Scope.** `architecture.html` "RichView IS NOT … A BI tool, dashboard builder … A chat product … A full WYSIWYG web editor"; the Rust brief excluded "a full browser editor, chat/BYOK, real-time multi-user collaboration and a marketplace"; the 2026-10-17/18 tracks added a voice/chat dashboard, live data connectors, a newsroom CMS with a browser editor and multi-tab reports. **O**. Forces D-SCOPE-CORE.
- **Landing motion.** PR #58 removed the background mark animation as 2026-10-12 feedback; PR #69 calls that removal accidental ("silently disabling the scroll choreography") and restores it per 2026-10-18; `main` has no animation, production has it. **O** (PR #69's "accidental" reading **S**). Forces D-LANDING-MOTION.
- **Measure.** `429e54f` (2026-10-13) keeps tables full-width; the 2026-10-18 brief wants one measure including tables and headings; #67 implements it only as an opt-in `layout.measure-wide-blocks`. **S**. Forces D-MEASURE.
- **Chart expansion.** The chart expand issue (#60) shipped an Expand/Contract toggle and #61 a landscape phone dialog; the 2026-10-18 brief drops both for one modal; `main` still has the toggle. **S**. Forces D-CHART-MODAL of the Linear evidence (not published).
- **Rust-only behaviour.** The Rust brief made Rust own all product behaviour; #70 adds ~4,000 lines of `.mjs` gateway and editor, #64 a JS app, and the 2026-10-18 website brief allows "a narrow site animation layer" in JavaScript. **O**. Forces D-RUNTIME.
- **Publishing.** The native server keeps immutable slug/version storage with a bearer token (dec-67–74, `storage.rs`, `server.rs`); the CLI publishes to `https://richview.uk` by default; RichView Publish uses Git PRs and Cloudflare Access; the 2026-10-12 copy describes a "menu of finished reports with share / copy link / access control". **O** for the native server and CLI, **S** for Publish and the copy. Forces D-PUBLISH-TARGET of the Linear evidence (not published).
- **Data binding.** The TypeScript-era `data:` frontmatter map (primary checkout `CLAUDE.md`) is now a hard migration error (`docs/RMD-SPEC.md:46`); #66 adds host-resolved `data.sql`/`data.metricflow`/`data.cache`; the architecture left "where does data binding live" open. **O**. Forces D-DATA-BINDING.
- **MCP surface.** dec-56 fixed four tools; PR #4 shipped five; `main` defines seven. **O**. Forces D-MCP-SURFACE.
- **Frontmatter.** dec-8 requires title, description and version; code requires title and version only at publication. **O**. Forces D-FRONTMATTER.
- **Gates.** `docs/CHART-REVIEW.md:17` says "CI runs this matrix on every PR"; hosted CI has executed no step since 2026-10-14 and branch protection is unavailable on the private repo. **O**. Forces D-CI-HOST.
- **Production vs `main`.** Live richview.ai matches `task/site`, news.richview.ai runs `feat/richview-publish`; `main` matches neither. **O** for richview.ai; **I** for News, because only #70 contains its code. Forces D-PROD-BASELINE.

## Proposed decision IDs

IDs shared with the Linear evidence (not published) keep its names; the others are new. Recommendations are proposals for Daniel (**I**).

| ID | Decision | Options | History and evidence | Proposed |
| --- | --- | --- | --- | --- |
| D-SCOPE-CORE | Which product areas the rewrite includes | Narrow data-story layer / product family (Charts, Reports, Dashboards, Publish) / core now, siblings later in their own repos | dec-5, `architecture.html` IS-NOT box; Rust brief exclusions; 2026-10-17 "despite older August scope locks"; the Publish brief's ecosystem | Core now (format, charts, reports, authoring, native publish); decide each sibling separately |
| D-POSITIONING | Headline and noun | "agentic presentation layer for data stories" / "a presentation layer for your agent" + "Your agent organizes the text. RichView makes it a story." / README "sourced, editable reports" | dec-1, PR #2, PR #58, `README.md:3` | Latest dictated landing copy (2026-10-12) |
| D-OPEN-SOURCE | Licence and when source opens | MIT everywhere / MIT + ELv2 server / other; private until release | dec-81, `ce68a8d`, PR #4, PR #21, `Cargo.toml:8`, `LICENSING.md` | Daniel's call; remove the stale gradient doc either way |
| D-REPO-SPLIT | Repository topology | One private monorepo / split charts or site; archive or update public `richview-charts`; keep `richview-design-system` | dec-33/34, PR #4, PR #10, transfer 2026-10-21 | One monorepo; archive `richview-charts` |
| D-RUNTIME | Implementation language and the JS boundary | Rust/Wasm shared core with JS only for host adapters / allow JS product layers / other | Rust brief 2026-10-04; PRs #10, #60 performance; #64, #70 JS; 2026-10-18 site brief | Rust/Wasm core; JS limited to host adapters and site-only animation |
| D-CHART-SUPPLIER | Chart engine | Own Rust engine / third-party (ECharts, Highcharts) | Lineage table above | Own engine; third-party libraries as local references only |
| D-CHART-TYPES | Supported chart types | Nine current / fewer / more (playback) | PR #60, `RMD-SPEC.md` | Nine |
| D-INTERACTIVITY | Static vs interactive output | Static-first SVG plus Wasm enhancement / zero JS / canvas | PR #1, dec-39/44/51, PR #22 | Static-first plus Wasm |
| D-DIAGNOSTICS | Readability diagnostics in the product | Keep native estimates and browser glyph checks / drop | dec-50, PR #4, Rust brief, `docs/self-validating-charts.md` | Keep |
| D-RMD-TAGS | Rich block vocabulary | Nine tags (incl. `metric` alias, `audio`, `video`) / collapse to `media` / drop alias | dec-7, `parser.rs:9` | Daniel's call |
| D-FRONTMATTER | Required metadata | Title + version at publish, description warns / all three required | dec-8, `parser.rs:735-749` | Current rule |
| D-CHART-CONTRACT | Chart block contract | Current (title warns, subtitle/annotations/axes first-class, unknown keys kept) / dec-42 slim | dec-42, PR #4, `RMD-SPEC.md:56` | Current |
| D-TABLE-SYNTAX | Table authoring | Row mappings / series form | `docs/TABLE-AUTHORING-CHOICE.md` | Row mappings (repo recommendation) |
| D-DATA-BINDING | Where data comes from | Chart-local inline/CSV/JSON in the report folder / plus host connectors / frontmatter sources | PR #15, `RMD-SPEC.md:46`, PR #66 | Chart-local now; connectors later as snapshots |
| D-STYLE-CATALOG | Public style catalogue | Five canonical / plus 20 studio variants under "Other styles" | PRs #4, #8, #16, #22, #58, #67 | Five; studio variants Daniel's call |
| D-DEFAULT-STYLE | Default style | Sleek dark everywhere / Scientific for the flagship | PR #8 pending judgement; the Linear evidence (not published) | Daniel's call |
| D-FONTS | Type system | Per-style bundled OFL faces with `RichView Serif/Sans/Code/Mono` aliases / one house pair | dec-22–24, PR #4, PR #22; "Font selection remains open" (#16) | Per-style faces; size budget explicit |
| D-MOTION | Chart and text motion policy | Static first frame, calm structural entrances, reduced-motion floor / minimal | dec-27, 2026-06-27 feedback, PRs #22, #60, 2026-10-12 "ticking" | Keep, with a no-ticking acceptance check |
| D-LANDING-MOTION | Landing scroll story | Restore (2026-10-18) / drop (2026-10-12) | PR #58, PR #69, rejection 2026-10-18 16:03 | Restore, mobile-stable |
| D-MEASURE | Block widths | One text measure for all blocks incl. tables and headings, explicit full-width opt-in / tables full-width | `429e54f`, 2026-10-18 brief, #67 `4e029af` | One measure by default |
| D-CHART-ACTIONS | Chart action row | Table, CSV, Download (PNG/SVG), Expand (one modal) / current Table/SVG/CSV/Expand toggle | PRs #60, #61, #67; D-CHART-MODAL of the Linear evidence (not published) | 2026-10-18 set |
| D-TOOLTIP | Tooltip look and behaviour | Light translucent, stable, exact values only | PR #59, #67 `06d8691`, 2026-10-18 brief | As dictated |
| D-LABEL-DENSITY | Direct labels | Sparse defaults, author highlights with priority, leaders only when forced | #67 `65cc81d`, `0bb85e9` | As dictated |
| D-SERIES-CUES | Line/area identification | Name alone unless a cue disambiguates | 2026-10-18 brief | As dictated |
| D-CURVES | Line interpolation | Monotone cubic default with straight/step options / straight default | PR #40, PR #60 | Monotone default |
| D-AXIS-FORMATS | Number and date labels | Years as years, step-aware scientific notation, unit once | PRs #26, #31, #51, #67 `cf4af8d` | Keep all |
| D-MCP-SURFACE | MCP tools | Five core / seven with asset tools / four | dec-56, PR #4, PR #22, `mcp.rs` | Daniel's call on asset tools |
| D-PDF-EXPORT | Report PDF output | Keep `generate --to pdf` / drop | PR #22 (eleven pagination findings open) | Keep only with a pagination gate |
| D-ASSET-UPLOAD | Remote asset upload commands | Keep / drop until sources consume them | PR #22 ("Report sources do not consume those references yet") | Drop from the rewrite's first cut |
| D-VSCODE | Editor scope and distribution | Current authoring features, Marketplace later / minimal | dec-66, PRs #15, #60; unpublished | Daniel's call |
| D-PUBLISH-TARGET | Publishing model | Native immutable server / RichView Publish / both | dec-67–74, PR #70, `cli.rs:122` | See the Linear evidence (not published) |
| D-HOSTING | Hosting | Cloudflare (live) / AWS credits / Fly.io (TS-era `richview-v3.fly.dev`, `packages/server/fly.toml`) | PRs #13, #40, `ae2a1cb`; the Linear evidence (not published) | See the Linear evidence (not published) |
| D-SITE-NAV | Site navigation and gallery placement | Flagship report · Chart gallery · Docs with gallery in docs / standalone galleries | PR #69, PR #65; D-GALLERY-ROUTES of the Linear evidence (not published) | #69 layout |
| D-PLAYGROUND | Public playground | Keep `/playground` unlinked / link it / remove | dec-5, dec-77, PR #58 | Daniel's call |
| D-SHOWCASE-CONTROLS | Showcase compare, style-file, tour, share, embed, source controls | Delete the dormant code / restore some | `9d328b5` (code still binds removed IDs) | Delete unless restored |
| D-SIGNUP | Email opt-in on the site | Keep D1 signup / drop | PRs #20, #40 | Keep |
| D-REEL | 15-second motion reel | Keep / drop | PR #7, PR #10 | Daniel's call |
| D-PROD-BASELINE | What counts as `prod` at lockdown | Merge #69 (and decide #70) so `main` = live / redeploy `main` / freeze both | Live checks 2026-10-21 | Merge #69 before the rewrite |
| D-CI-HOST | Where gates run and how merges are enforced | Paid GitHub plan or minutes / self-hosted runner on Claw Mini / local gate receipts | CI blocked since 2026-10-14; protection 403; "Hosted CI blocked solely by billing is waived … do not spend money or retry paid jobs" (`tasks/simplify-toolset.md` in `a4bd68a`); "not spent to unblock per standing constraint" (#64) | Self-hosted or local receipts; any spending is Daniel's call |
| D-VISUAL-GATE | Visual regression gate | Fixed 60-page/540-figure matrix plus matched before/after evidence / lighter | `docs/CHART-REVIEW.md`, PRs #23–#56 | Keep as an auto-merge gate |
| D-EVIDENCE-RETENTION | Evidence in Git | Compact manifests in Git, artifacts outside / full captures in Git | 328 files under `docs/evidence/`; PR #40 removed 118k generated lines | Manifests in Git |
| D-BRANCH-CLEANUP | Stale refs | Delete merged and validation branches, `synth/vnext-architecture`; keep or delete the `chart-refinements-2026-10-12` pre-release | Ref inventory above; brief "no releases" | Clean up after the PR decisions |
| D-CONTROLS-PR62 | Playable charts | Fold / merge / abandon | PR #62 | Fold |
| D-TABS-PR63 | Multi-tab reports | Fold / abandon | PR #63 | Fold if in scope |
| D-DASHBOARD-PR64 | Voice dashboard | Separate repo / abandon / merge | PR #64; D-DASHBOARD-SAVE of the Linear evidence (not published) | Abandon here |
| D-GALLERIES-PR65 | Standalone galleries | Abandon PR, reuse content / merge | PR #65 | Abandon, reuse examples |
| D-CONNECTORS-PR66 | SQL/MetricFlow/cache connectors | Fold later / merge / abandon | PR #66 | Fold later |
| D-CHART-POLISH-PR67 | Dictated chart polish | Fold / finish and merge | PR #67 | Fold |
| D-HARNESS-PR68 | Development harness | Fold into gates / merge | PR #68 | Fold |
| D-SITE-PR69 | Site repair (live) | Merge / fold | PR #69 | Merge, then fold |
| D-PUBLISH-PR70 | RichView Publish (live) | Merge / freeze / retire | PR #70; the Linear evidence (not published) | After D-SCOPE-CORE |

## Gaps

- The 2026-06-27 review document (`richview-review-2026-06-27.md`) and its screenshots are not in any branch of this repository; its 22 items are known only through PR #2, PR #3 and `5ceb7d4`.
- The 2026-10-12 and 2026-10-13 feedback exists here only as agent summaries in PR #58, PR #59 and commit messages; the original dictation was not read.
- Linear tickets and comments, including the "Final review" comment cited by PR #62, are covered by the Linear evidence (not published), not re-read here.
- Whether live richview.ai also includes PR #67 commits was not checked; only the landing nav and animation nodes were compared.
- The state of the Fly.io app `richview-v3.fly.dev`, the Cloudflare Workers and the News publisher after the repository transfer was not inspected.
- The vault and Notion copies of the vNext architecture referenced by the synthesis handoff were not read; `architecture.html` at `8c6a8e0` is the only decision source used.
- Several dec rows are tagged **S** or **I** because only PR text or `CLAUDE.md` supports them; they were not traced to code.
- `richview-v2` and the older February 2026 repositories were listed, not mined.
