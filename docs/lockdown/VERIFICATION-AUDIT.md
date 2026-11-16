# Verification audit

Audit of 26 October 2026, made for Daniel's question: "have we designed the whole verification path?" Inputs were the 11 part files, Daniel's 39 answers up to FEA-CH-37 (18:14 UTC), EVIDENCE.md, DELIVERY.md, `docs/guide/delivery.md`, `docs/guide/performance.md` and the link map. No part, answer or tool file was changed. The proposed cards are in [drafts/verification-cards.json](drafts/verification-cards.json).

Written before DEL-81 (27 October): read `rv-gate dev` as the release run and "per merge" as "per release batch".

## Answer

Partly, and only for the product as it stood on 22 October. The path lives in Part 02's gate cards (DEL-08 to DEL-36, DEL-49 to DEL-64): one `rv-gate dev` command per merge (Rust tests with a mutation check, 21 Chromium suites, data-integrity and cross-style value checks, a visual matrix with an exact pixel diff, hostile input, accessibility, part 07's budgets, a receipt) and `rv-gate prod` per frozen candidate (WebKit, Firefox, PDF, packaging); Part 07 sets the budgets, feature checks sit on FEA-SF-62, FEA-RP-43, FEA-RP-49, FEA-CH-58 to FEA-CH-69, STY-52 and INT-67 to INT-69, and EVIDENCE.md and DELIVERY.md summarize them as 22 October drafts built from proposals. You have answered the principles (INT-67, INT-49, DEL-04, DEL-06) but none of the gate or budget cards, and none of it is built. Today's answers outran it: under WebGL every chart capture, value, interaction and motion check is reported unrun until a geometry-model oracle exists; FEA-CH-37 reverses the data-integrity premise; and the 30 chart types, map, flow and pivot tables, five distinctive styles, Cowork and Publish with roles, connectors, paid hosting, the `dev` change log with its before-and-after report, and the tests behind your `prod` go have no verification design. Of 16 layers, 3 hold, 10 are stale and 3 are missing.

## The path as designed today

| Layer | Cards | What it proves | After today's answers |
| --- | --- | --- | --- |
| Unit and engine | DEL-12, DEL-24 to DEL-26, DEL-36, DEL-54, FEA-CH-58, ARC-08 | 438 Rust tests in tiers proven by deliberate mutations; the engine checks its own geometry; core stays pure | **yes**, with one hole: ARC-08's cross-surface contract test has no gate step, and the geometry model must now be the oracle for every chart test |
| Format and validation | DEL-18, DEL-53, DEL-60, TYP-53 to TYP-56, FEA-SF-47, FEA-SF-62 | Schemas, diagnostics, public-surface snapshots, every golden input clean in every style, docs that match the code | **yes**; the vocabulary goes stale until part 05 gains the new types, maps, flows, pivot tables and scrollytelling (OPN-67); no check asserts that diagnostics give a line and a fix (INT-13) |
| Rendering and visual, per style and width | DEL-16, DEL-17, DEL-19, DEL-27 to DEL-29, DEL-31, DEL-52, DEL-64, STY-51, STY-52 | A style reaches every surface; layout holds for 3 cases × 5 styles × 4 widths; exact pixel diff against `dev`; the same values in every style | **stale**: the exact diff was measured on SVG, and WebGL determinism is unmeasured (DEL-28); Chromium only; 3 cases, not 30 types; the theme-switch proof covers 3 of 5 styles and fails; DEL-16 and DEL-52 assert exact source values, which FEA-CH-37 now forbids |
| Animation and motion | DEL-15, DEL-30, FEA-CH-51 to FEA-CH-54, FEA-RP-35 to FEA-RP-37, PRF-21, PRF-27, PRF-09 | One entrance per bar, reduced motion equals settled, frame pacing, filmstrips on motion PRs, smoothness on your phone | **stale**: the checks read CSS animations on SVG and are reported unrun under WebGL; no steppable clock; values during frames are unchecked; INT-17's rich animation outgrows "filmstrips only on motion PRs" |
| Interaction | DEL-15, FEA-BR-37, FEA-BR-47, PRF-25, FEA-CH-39 to FEA-CH-50 | Zoom, keyboard, readouts, table and downloads in Chromium | **stale**: DOM assertions move to a model interface no card names; canvas hit testing is untested; new types, maps, flows, pivot tables and scrollytelling have no interaction spec |
| Browsers | FEA-RP-59, DEL-33, DEL-57, PRF-24, ARC-05 | WebKit (23 suites) and a Firefox loop at each candidate; the no-WebGL notice in Chromium | **stale**: ARC-05 requires Chrome, Firefox and Safari with WebGL. The Firefox loop has no suite list and Firefox is not installed; Playwright's WebKit is not Safari; nothing outside Chromium runs per merge; the 16-context cap and context loss are untested |
| PDF | FEA-RP-43, OPN-52, PRF-16, DEL-33, FEA-CH-67 | Pagination bar, charts as vector paths, tags, embedded fonts, export time bounded against regression | **yes**: already rewritten for the vector conversion; the PDF in each style and its match with the page are unchecked |
| Performance | PRF-01 to PRF-62, DEL-51, OPN-50, OPN-51 | Budgets per merge and in a full capture, with a ratchet, on one reference host | **stale**: frame, zoom, layout-shift and hydration figures are SVG measurements; the GPU path for headless timing is undecided (PRF-24); the runtime size is open (PRF-31) |
| Accessibility | DEL-58, FEA-RP-49, FEA-CH-55, FEA-CH-56, STY-25 | Automated audit and keyboard pass at 390 and 1440 px; VoiceOver pass at the candidate, a standing gap | **stale**: a canvas exposes no marks and FEA-CH-55's mark layer is open; the server pages (approval, source editor) are not audited |
| Security and hostile input | DEL-54 to DEL-56, ARC-12 to ARC-15, ARC-23, ARC-66 | No injection at any render sink, no secret merges, core boundaries hold | **stale**: the report side holds. Server routes, roles, the browser source edit, account isolation and connectors have no hostile cases, and nothing checks FEA-CH-37's "no more precision than shown" |
| End-to-end journeys | FEA-SF-62, DEL-14, DEL-33, DEL-60 | Pieces only: `init` then `generate` in a temporary folder, loopback publish and host checks, MCP `initialize`, editor equals `validate`, VSIX and the VS Code check on Lenovo | **stale**: no card runs a whole journey (CLI or MCP, render, share through Cowork or Publish, read as another role) in real browsers; WebGL in the VS Code webview is unproven |
| Server roles and approvals | FEA-SF-19, FEA-SF-20, FEA-BR-25 to FEA-BR-29, ARC-57, INT-26 | Rules only | **missing** |
| Connectors | OPN-63, ARC-59, FEA-BR-14, FEA-BR-16, FEA-BR-43, DEL-55 | Only the credential scan | **missing**: the engine is open, and PR #66's adapter could read host files |
| Hosting | DEL-40, DEL-48, DEL-38, PRF-37, DEL-31 | richview.ai's live routes and bytes after a deploy; pre-authorized rollback; the audit | **stale**: nothing checks that preview.richview.ai stays behind Access, or checks hosted Cowork, customers' Publish servers, managed media hosting, limits or backups |
| Fresh-agent test | INT-69, INT-21 | The bar: a new agent reaches a clean `check` by the second attempt, with two agent products | **missing**: no harness, schedule, agents or datasets |
| Review packet and design log | DEL-06, DEL-31, DEL-32, DEL-34, DEL-35, DEL-63, STY-51, STY-64 | One card per design or feature change with `prod`-against-candidate crops, contact sheets and a phone preview | **stale**: your DEL-06 answer asks for a feature and design log and a before-and-after report with causes. DEL-34 names both, but no card says who writes the log, what an entry holds, how a cause is recorded or how autonomous-loop work enters it |

Three facts cut across the table. First, the "second, automatic set of tests" in your DEL-04 answer is DEL-33's `rv-gate prod`, written before today's answers; DEL-37 also leaves open how the one-command merge proves your go. Second, INT-49 fits the gate design: DEL-64 retires legacy-only tests with a reason, the pixel diff detects change rather than preserving it, and INT-55's carried fixtures become acceptance tests for accepted items. The SVG-based chart tests cannot carry over unchanged, though. Third, DEL-05's answer lets agents set up scheduled jobs, so the daily full capture (OPN-51), the nightly audit and a fresh-agent schedule no longer need your go, while EVIDENCE.md still says they do. Regenerate EVIDENCE.md and DELIVERY.md once the gate cards are answered.

## Gaps, most blocking first

- **WebGL determinism, the chart oracle and cross-browser capture**: until a model interface, a deterministic capture path and per-engine baselines exist, every chart capture, value, interaction and motion check is reported unrun, so the visual layer cannot see charts (DEL-72, DEL-73, DEL-74).
- **The autonomous `dev` loop's design and feature log and its before-and-after report**: design and feature changes now merge without you, so this report is your only view of them, and no card says how it is written or what it shows (DEL-68, DEL-69, DEL-70).
- **What "prod tests" are for DEL-04**: your go rests on DEL-33's set, which has no rows for WebGL across browsers, the new elements, the servers, connectors or hosted services (DEL-71).
- **Animation testing in WebGL**: rich animation is now part of the top target, and today's motion checks read CSS animations that will no longer exist, so motion could only be judged by eye (DEL-75).
- **End-to-end tests for Cowork and Publish with roles**: a leaked private report never shows in a screenshot review, and no test proves a reader sees only what is shared with them (DEL-76, FEA-SF-64).
- **Evidence for each style's design review**: under INT-17, "if it looks bad we failed", yet the only review defined per style compares a typography specimen; nothing shows a style's structure across every element (STY-68, STY-69).
- **Tests for about 30 chart types**: the matrix covers 3 cases and the checks table 9 types, so without a contract per type, every new type is checked by hand (FEA-CH-75).
- **Values at the author's precision**: FEA-CH-37 turns DEL-16 and DEL-52 around, and nothing yet proves that more precise values stay out of the output (DEL-80).
- **Traceability from items to checks**: a green gate proves only what someone remembered to test, and INT-67's "done" needs every accepted item matched to its evidence (DEL-79).
- **Map, flow and pivot-table tests**: no feature card specifies these elements, so they cannot be tested yet, and pivot aggregation is a new way to show a wrong number (FEA-CH-76).
- **Checks for hosted services**: the paid services and the private preview have no check that they run, stay private and keep customers apart (DEL-77).
- **Connector tests**: the path is secondary, but PR #66 showed that a connector can pull host files into a report, and the engine is still unchosen (FEA-BR-50).
- **The fresh-agent harness**: the agent-native bar exists, but nothing runs it, so the only way to know whether a new agent succeeds is to try one yourself (DEL-78).
- **Chart accessibility on canvas**: WCAG 2.2 AA is a gate (FEA-RP-49) that a canvas cannot pass until FEA-CH-55 chooses a mark layer (FEA-CH-77).
- **PDF in each style**: DEL-17 excludes the PDF, so a style that breaks on paper passes (FEA-RP-61).

## Proposed cards

[drafts/verification-cards.json](drafts/verification-cards.json) holds 21 drafts in the part schema. Each draft has a suggested `part`, and its `conflicts` field names the existing cards it changes. Four are key because only you can decide them: DEL-68 (what you trust without looking), DEL-69 (the before-and-after report), DEL-71 (the prod tests behind your go) and STY-68 (what you see before accepting a style). The other 17 are important. Several follow open answers on ARC-05's subquestions: FEA-CH-30 (how chart text is drawn), FEA-CH-55 (accessibility) and ARC-18 (the WebGL stack). Answering those settles what the chart tests check. Answer the four key drafts together with DEL-28, DEL-30, DEL-33, DEL-34 and OPN-50, OPN-51, OPN-52 and OPN-66. Agents then move each accepted draft into its part, update the cards it changes and regenerate EVIDENCE.md and DELIVERY.md.

| Draft | Suggested part | Importance | Title |
| --- | --- | --- | --- |
| DEL-68 | 02-delivery | key | What you trust without looking |
| DEL-69 | 02-delivery | key | The before-and-after report you review |
| DEL-70 | 02-delivery | important | The dev change log and the autonomous loop's trail |
| DEL-71 | 02-delivery | key | The prod tests behind your go |
| DEL-72 | 02-delivery | important | The geometry model is the chart test oracle |
| DEL-73 | 02-delivery | important | Deterministic WebGL captures and per-engine baselines |
| DEL-74 | 02-delivery | important | Chrome, Firefox and Safari for WebGL charts |
| DEL-75 | 02-delivery | important | Motion tests for WebGL animation |
| STY-68 | 06-styles | key | What you see before accepting a style |
| STY-69 | 06-styles | important | Safe style settings are tested; experimental ones are labelled |
| FEA-CH-75 | 04a-features-chart | important | What every chart type ships with |
| FEA-CH-76 | 04a-features-chart | important | Map, flow and pivot-table checks |
| DEL-76 | 02-delivery | important | End-to-end journeys on built artifacts |
| FEA-SF-64 | 04c-features-surfaces | important | Roles, approvals and tenant isolation are tested on every route |
| FEA-BR-50 | 04d-features-branches | important | Connector checks for the live-data path |
| DEL-77 | 02-delivery | important | Checks for hosted services and previews |
| DEL-78 | 02-delivery | important | The fresh-agent run: harness and schedule |
| FEA-CH-77 | 04a-features-chart | important | Chart accessibility checks with WebGL |
| DEL-79 | 02-delivery | important | Every accepted item names its check |
| FEA-RP-61 | 04b-features-report | important | PDF in every style, drawn from the same model |
| DEL-80 | 02-delivery | important | Values at the author's precision, and nothing more precise |
