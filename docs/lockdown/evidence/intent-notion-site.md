# Evidence: intent, the Notion vault and the live sites

Mined 2026-10-21 for the RichView lockdown (S27-677), mainly for part 01 (purpose and what RichView maximizes). Read-only: no Notion page, Linear issue, GitHub object, Cloudflare setting or live form was created, changed or submitted. Screenshots come from the worktree's Playwright in headless Chromium with throwaway profiles that were deleted afterwards.

## Scope, method and tags

- Notion, read through the connector on 2026-10-21; the pages are private, so they are cited by title. RichView area: "richview-ai" (edited 2026-10-18), "richview.ai" (2026-09-05), "data", "Strategy & business model (parked)" (updated 2026-06-09), "research", "Architecture — the five packages" (updated 2026-06-10), "v1 — the original RichView startup (therichview.com, 2023)", "RichView: build provenance and reader checks — PENDING REVIEW" (2026-10-18), "launching-richview-ai" and "Launching richview.ai" (2026-08-17); the charts, core and context children. Daily plans read only for their RichView passages: "2026-10-02 — Friday", "2026-10-03–04 — Weekend", "2026-10-05 — Monday", "2026-10-07 — Wednesday", "2026-10-12 — Monday" (search highlight only), "2026-10-16 — Friday", "2026-10-21 — Wednesday" (no RichView passage). Delivery principles: "Locking down agent deliveries — verification principles — PENDING REVIEW" (2026-10-20).
- Legacy local notes. Notion links several pages as wiki paths into Daniel's older local notes, and those pages do not exist in Notion (brief, evolution, competitive-landscape-2026, positioning-icp, self-validating-charts). Their pre-Notion copies were read from Daniel's local RichView notes on Claw Mini, together with the 2026-06-20 voice dump and Daniel's manual design system note. Citations take the form "Daniel's notes: <title> (<date>)" plus line numbers; the notes used are "RichView — brief & compiled findings" (2026-06-08, updated 2026-06-09), "Positioning, ICP & licensing" (2026-06-09), "RichView voice dump — agented data storytelling layer" (2026-06-20, "voice dump" for short), "2026-06-20 — Design system studio + repo split", "Suggested tasks — from the 2026-06-20 design-system session" and "Daniel's manual design system" (2026-06-21).
- Live sites: Playwright loads (desktop and mobile) and `curl` GETs of 21 richview.ai URLs and 12 news.richview.ai URLs, 2026-10-21 20:12–20:40 CEST. All 33 pages returned 200 in both viewports, with no console errors and no failed requests. [observed]
- Repository: this worktree at `origin/main` `b4db233` (2026-10-16). Other branches read with `git show`. The repository now resolves to private `1vecera/richview` (`gh api repos/richview-universe/richview-v3` → `1vecera/richview`, `private=true`). [observed]
- Linear: the project record, DIR-121, S27-401, S27-416, S27-419, S27-420, S27-427, S27-616 and S27-653 were read directly. Other Linear statements come from [linear.md](linear.md) and are cited as such.
- Tags: [observed] verified now on the live site, in git or by a command; [stated] written in a note, brief, ticket, doc or commit message and not verified; [inferred] my reading across sources.
- Personal content: the daily pages mix RichView with private matters. Only RichView passages are used; contact details and private context are left out.
- Decision IDs reuse [linear.md](linear.md) and [history-prs.md](history-prs.md) (D-SCOPE-CORE, D-OPEN-SOURCE, D-DEFAULT-STYLE, D-STYLE-CATALOG, D-GALLERY-ROUTES, D-PUBLISH-TARGET, D-PROD-BASELINE, D-LANDING-MOTION, D-INTERACTIVITY, D-SITE-NAV, D-TYP-DOC-DRIFT, D-FRONTMATTER, D-TABLE-SYNTAX, D-SIGNUP). This file adds D-POSITIONING, D-AUDIENCE, D-MAXIMIZE, D-AMBITION, D-RELEASE and D-ACCESS-CLAIM.

## Summary

- The definition Daniel has repeated most often since June is a presentation layer for agents and data stories. On 2026-06-20 he said "an agent presentation layer for data stories that allows humans and agents to build … review, improve and share data-driven stories" (vault voice dump `:126`). The live home on 2026-10-21 says "RichView is a presentation layer for your agent." Repo docs instead lead with "turns Rich Markdown (`.rmd`) into sourced, editable reports" (`README.md:3`). [stated; observed live]
- Scope has drifted. June: "we are not doing all kinds of reports. We are doing data stories" and "not dashboards, not generic BI, not a full chat/web-editor product" (S27-420). August: "rather than a sprawling platform" (DIR-121). October: a product family of Reports, Dashboards, Charts and Publish, with a live newsroom demo at news.richview.ai (S27-655). The rewrite has to settle D-SCOPE-CORE and D-POSITIONING first. [stated; observed live]
- Audience has moved from human analysts (v1, 2023) to "only for people who have coding agents" (2026-06-20), to "analysts and developers using coding agents" (2026-10-02), and then to readers, LinkedIn audiences and small newsrooms (S27-616, S27-655). [stated]
- Stated maximization targets, in their latest form: visual and motion craft that withstands comparison with Highcharts, ECharts and editorial critique; truthful, sourced evidence across every theme and export; a short authoring path; a small code footprint; low, explainable operating cost (S27-616 "What to maximize"; voice dump; S27-655). [stated]
- The "beyond anyone" ambitions written down so far are self-checking charts that return machine-readable diagnostics to the agent (June research), one sourced report that transforms coherently across many styles (flagship), Highcharts-level craft from short YAML, and Git-native editorial publishing for a small team (Publish). Most of the "competition-winning" wording comes from a brief-writing convention, not from a product claim. [stated; inferred]
- Release status is inconsistent. The August 31 launch date has passed. Labels in use are "Coming soon" (home), "BETA" (home share image), "v3 alpha" (docs), "3.0.0-alpha.1" (VSIX) and, under the 2026-10-21 model, "no releases". The site says "RichView will be fully open source … It is not public yet", while the repository has just moved to a private personal account. [observed; stated]
- Live richview.ai serves the unmerged `task/s27-653-site` landing; news.richview.ai serves content commit `7e7d9f8` from `publish/news`. Observed defects: zero side gutters on the home page because 26 CSS rules key on a removed body attribute (confirmed by restoring the attribute in a browser); mobile horizontal overflow; 404 gallery links from the docs; a stale "BETA" share image; a docs index that says six chart types while the gallery shows nine. [observed]

## Purpose statements over time

| Date | Statement | Source | Origin | Tag |
| --- | --- | --- | --- | --- |
| 2023 (Feb–Sep) | Interactive web reports to replace "dull PDFs": a human analyst is the author, AI is a chart-drawing assistant inside a block editor, and a pandas SDK re-issues recurring reports. "A human analyst is the author … That assumption is the single biggest thing v3 later inverted." | Notion "v1 — the original RichView startup (therichview.com, 2023)" | research summary, 2026-06-09 | stated |
| 2026-04-10 | "position as agentic layer for data-driven reports (not more, not less)" (title reads "RidgeView", apparently a transcription of RichView) | S27-427 | Daniel, via coordinator | stated |
| 2026-06-08 | "RichView is the presentation layer for AI agents: an agent writes Rich Markdown (`.rmd`), RichView renders a beautiful, self-contained HTML report — and (the new bet) ships a chart library that checks its own work" | Daniel's notes: RichView — brief & compiled findings (2026-06-08), line 23 | agent-compiled brief | stated |
| 2026-06-20 | "RichView is an agented presentation layer for data stories. It lets humans and agents build, review, improve, and share data-driven stories." Transcript: "it's an agent presentation layer for data stories that allows humans and agents to build … review, improve and share data-driven stories" | Daniel's notes: RichView voice dump — agented data storytelling layer (2026-06-20), lines 36 and 126; S27-420 | Daniel, voice | stated |
| 2026-06-20 | Headline candidates "Agented Presentation Layer" and "Storytelling for Agents" | voice dump `:306`, `:311`, `:56` | Daniel, voice | stated |
| 2026-07-07 | "Goal of v3: cut RichView down to the minimum surface that lets an AI agent produce a beautiful, interactive web report." "Vision: RichView is the presentation layer for AI agents." | `git show 5ceb7d4:docs/OVERVIEW.md` lines 3, 63 (PR #4) | repo doc | observed text |
| 2026-08-08 | "RichView is a curated JSON/data-to-report rendering layer with strong components at hand." "Present RichView as a curated JSON/data-to-report rendering layer rather than a sprawling platform." Project context: "RichView is an AI-native report and visualization product." | S27-401, DIR-121, Linear project P-S27-32 | Daniel's life-review voice note, via coordinator | stated |
| 2026-10-02 | The website should "explain why RichView exists and its JSON-renderer inspiration" | Notion "2026-10-02 — Friday" (S27-398 brief) | Daniel's dictation, via coordinator | stated |
| 2026-10-05 | "RichView is a presentation layer for your agent." "An engine for humans and agents that turns text and data into beautiful reports." | S27-590 via [linear.md](linear.md); live home H1 and section 01 | Daniel's review | stated; observed live |
| 2026-10-05 | Carousel opening "Open source · Built with Rust · Coming soon"; the post "connects the presentation-layer idea with agentic analytics" | Notion "2026-10-05 — Monday" | Daniel | stated |
| 2026-10-13 | Lede "Your agent organizes the text. RichView makes it a story." | commit `bcea882` (PR #58, "landing: apply 2026-10-12 feedback"); live hero | Daniel's feedback, applied by an agent | observed |
| 2026-10-16 | "RichView turns Rich Markdown (`.rmd`) into sourced, editable reports." | `README.md:3`, `CLAUDE.md:3`, `llms.txt:3` | repo docs | observed text |
| 2026-10-16 | "RichView is a source-centered report product built in Rust. Analysts and coding agents edit Markdown, chart YAML, data and ordered styles, preview the rendered report, inspect actionable diagnostics and improve the source." | `docs/OVERVIEW.md:3` | repo doc | observed text |
| 2026-10-18 | Ecosystem: "Rich Markdown is the authored format; Charts is the rendering capability; Reports and Dashboards are publishable experiences; Publish is the publication/editorial/access/deployment layer; Server is existing hosting/runtime functionality to reuse and potentially absorb later; the VS Code integration remains an authoring surface" | `tasks/richview-publish.md:39` on `origin/feat/s27-655-richview-publish` (S27-655) | coordinator brief from Daniel's request | stated |
| 2026-10-21 (live) | `<title>` "RichView — a presentation layer for your agent"; meta description "RichView is the presentation layer for analysis done with coding agents: a plain-text .rmd file in, a typeset, self-contained HTML data story out."; eyebrow "Agentic data stories"; og:title "RichView — your agent organizes the text, RichView makes it a story" | https://richview.ai/ | live site | observed |
| 2026-10-21 (live) | "A working notebook on evidence, systems and better decisions." "RichView News is the maintained demonstration of RichView Publish." | https://news.richview.ai/, `/_content/about/report.html` | live site | observed |
| 2026-10-21 | Rewrite RichView from scratch, guided by "an exact description of all features, all intents, all styles, all types" | the chief of staff's lockdown task brief (2026-10-21); S27-677 | Daniel, via chief of staff | stated |

## Audience over time

| Date | Audience | Source | Tag |
| --- | --- | --- | --- |
| 2023 | Finance and commodity analysts composing branded web reports; AI only drafted charts | Notion v1 page | stated |
| 2026-06-08 | User table marked "Daniel's current focus": AI coding agent, engineer/operator, report recipient, consultant/founder, analyst curating agent output | Daniel's notes: RichView — brief & compiled findings (2026-06-08), lines 45-53 | stated (agent-compiled) |
| 2026-06-09 | Layered ICP proposal: agents (distribution), engineers/operators (adoption), consultants/agencies/founders (revenue), analysts (quality and trust) | Daniel's notes: Positioning, ICP & licensing (2026-06-09), lines 34-45 | stated (research) |
| June 2026 site | "AI agents create data stories, analysts curate them." | Notion "Strategy & business model (parked)", quoting the then-live site | stated |
| 2026-06-20 | "this whole product is only for people who have coding agents and who use coding agents" | voice dump `:251`; S27-416 | stated (Daniel) |
| 2026-10-02 | "analysts and developers using coding agents"; purpose "interest and feedback" | Notion "2026-10-02 — Friday" | stated (Daniel, via coordinator) |
| 2026-10-10 | "analysts and agent-assisted authors creating reports, their readers on desktop and mobile, and people discovering the work through LinkedIn carousels" | S27-616 | stated |
| 2026-10-16 | "Analysts and coding agents" | `docs/OVERVIEW.md:3` | observed text |
| 2026-10-18 | "a small intelligence or specialist news publisher … a small team and hundreds of items" | `tasks/richview-publish.md:5` (S27-655) | stated |
| 2026-10-21 (live) | "your agent" (home H1); "An engine for humans and agents" (home section 01); "analysis done with coding agents" (meta) | https://richview.ai/ | observed |

## Positioning and differentiation

- Live "Why RichView?" (home section 04): "Good analysis deserves a readable form." It makes three claims: "Keep design consistent." ("Typography, chart conventions and themes are reusable decisions."), "Make the handoff inspectable." ("The rendered page and its source travel together."), and "Stay in the workflow you know." ("Write with your agent, revise in your editor and version the report alongside its evidence."). [observed]
- Live claim about the division of labour: "Charts, layout and typography belong to the renderer. Your agent can concentrate on the analysis." The home also says "Style Decided Once. Reused Every Time." and "RichView gives agents clean themes which work every time". [observed]
- Problems Daniel named on 2026-06-20: agents generate huge HTML files that are hard to version ("everyone wants markdown files"); people who do not know JavaScript are stuck with "boring static charts"; agents are better with structured text and config than with pixels. Sources: voice dump `:139-144`, summary `:44`. [stated]
- June research wedge: "No tool in any category returns post-render, machine-readable chart-quality diagnostics to the caller." The "Open quadrant" is "agent-authored & agent-judged × closed render→diagnose→fix loop × zero-infra single-file artifact" (Notion "Strategy & business model (parked)"). Its own kill-review warns: "Geometry is a weekend feature, not a moat", and "Reframe the differentiator: cost + exactness, not 'VLMs are blind.'" [stated, research]
- Alternatives framing (June): versus screenshots with eyeballing, raw chart libraries, Claude Artifacts and code interpreters, and BI/notebooks: "zero infra, single file; render-layer verification vs their data-layer-only MCP" (Notion strategy page; Daniel's notes: RichView — brief & compiled findings (2026-06-08), lines 55-60). [stated, research]
- Current repo stance on diagnostics: "Diagnostics do not certify analytical truth" (`llms.txt:17`; `skill/SKILL.md:14`). The home overlays a `richview check --format json` card on its example chart. [observed]
- Brand risk named in research: "RichView" collides with TRichView, a Delphi rich-text component (Daniel's notes: Positioning, ICP & licensing (2026-06-09), line 111). It is not addressed anywhere current. [stated, research]

## What RichView maximizes (as stated)

| Date | Maximize | Source | Tag |
| --- | --- | --- | --- |
| 2026-06-20 | "Minimize code footprint … less code means fewer errors and smaller agent context" | voice dump `:43`, `:123` | stated (Daniel) |
| 2026-06-21 | Few, opinionated choices: three templates, three options per setting; "it vastly reduces the information overload and also the surfaces we need to test"; the default line spacing "needs to slap" | Daniel's notes: Daniel's manual design system (2026-06-21), lines 4, 22 and 30 | stated (Daniel) |
| 2026-10-02 | Quality priority "80% content, 20% style"; charts with "visual polish, calculation quality and fluid interaction expected from Highcharts, while giving RichView a distinctive, restrained design language" | Notion "2026-10-02 — Friday" | stated (Daniel, via coordinator) |
| 2026-10-10 | "Visual craft and readability that withstand comparison with the cited chart references and experienced editorial-design critique, across desktop, mobile and exported pages"; "Five distinctive, coherent identities with restrained, purposeful detail"; "Smooth, meaningful motion and strong storytelling while preserving correct data, units, scales and source access"; "A short, understandable authoring path from report source to the chosen output and from uploaded asset to working embed"; "Low, explainable operating costs and practical provider portability" | S27-616 "What to maximize" | stated |
| 2026-10-18 | "A complete, repeatable publishing loop"; "Small, reusable open-source architecture" (Publish) | `tasks/richview-publish.md:45-49` | stated |
| 2026-10-21 (live) | Consistent design, an inspectable handoff, the existing workflow (home section 04) | https://richview.ai/ | observed |

Hard constraints repeated across these sources: preserve data values, units, analytical meaning and provenance across themes, interpolation, storytelling and export (S27-616); "never invent findings or change data to flatter a style" (S27-573 brief in Notion 2026-10-02); synthetic examples must be labelled (`llms.txt:11`, `skill/reference/anti-slop.md`). [stated]

## Ambitions: where RichView should go beyond existing tools

- Self-checking charts: "a charting engine whose differentiated output is not just the picture but a machine-readable critique of the picture", including "dropped-label provenance (the exact labels suppressed and why — the one thing no existing library reports)" (Daniel's notes: RichView — brief & compiled findings (2026-06-08), lines 35-37). The research claim is "first renderer that hands the agent a structured fix-loop diagnostic at render time" (Daniel's notes: Positioning, ICP & licensing (2026-06-09), line 80). [stated, research]
- Chart craft: a new Rust charting library with a first showcase "strong enough to win a serious data-visualization competition", benchmarked against Highcharts; "Highcharts is a quality benchmark, not source code to copy" (Notion 2026-10-02, S27-572). "Highcharts-level visual craft in RichView's own rendering system" (S27-616). [stated]
- Expressive range on fixed evidence: "Make one report a competition-winning demonstration of RichView's expressive range: a visitor selects a theme or charting style and sees the entire presentation transform coherently while the evidence and analytical meaning stay intact" (Notion 2026-10-02, S27-573). The live flagship delivers this with 23 styles ("One report. Many voices."). [stated; observed]
- Analysis quality: "an unusually deep, decision-useful investigation … a flagship demonstration that could win a competition for rigorous, compelling business analysis produced with coding agents" (Notion 2026-10-02, S27-399). [stated]
- Website and motion: "a remarkable, working RichView beta website experience that could win a design showcase"; the motion-reel prompt "make a dynamic 15-second motion graphics video that shows what an incredible motion designer you are … go all out" (Notion 2026-10-02, S27-398). Polish target: "the quality of 'Mlouva' (name as dictated); having reached that level before makes me think I can get RichView there too" (Notion 2026-10-03–04). It most likely means Daniel's own product Mluva [inferred]. Linear adds "the strongest proof of how beautiful RichView can be" (S27-647, via [linear.md](linear.md)). [stated]
- Publishing: "the publishing experience and editorial control of a serious publication such as FT.com, scaled to a small team and hundreds of items, with Git and coding agents as the natural authoring and approval interface" (`tasks/richview-publish.md:5`). [stated]
- Caveat [inferred]: the same 2026-10-02 page revises Daniel's task-spec skill to "frame work as worthy of winning a relevant competition and name the judging criteria" (Notion 2026-10-02, S27-571). "Competition-winning" is therefore partly a prompting device. The lockdown should ask Daniel which of these ambitions are product claims to own (D-AMBITION).

## Non-goals and boundaries

- Data stories only: "we are not doing all kinds of reports. We are doing data stories" (voice dump `:128`). "Keep the product narrow: data stories for coding-agent users, not dashboards, not generic BI, not a full chat/web-editor product" (S27-420). June-era site line: "Built for data stories, not dashboards." (Notion strategy page). [stated]
- Web surface: a playground, not an editor and not a chat. "let's not make any chatting in the face there" (S27-416); "Web page: playground for language/style examples, not a full editor by default" (voice dump `:87`). [stated]
- No sprawling platform: "rather than a sprawling platform" (DIR-121, 2026-08-08). [stated]
- Monetization parked: "Per Daniel's 2026-06-08 steer, focus is user value, not monetization" (Notion strategy page; Notion: Launching richview.ai (2026-08-17)). [stated]
- Theme adjustment: "v1 should have no way to adj. it in any heavier way" (Daniel's notes: Daniel's manual design system (2026-06-21), line 22). [stated]
- Customization: "Arbitrary CSS/JavaScript is not a source customization interface." (`docs/public/styles.md:44`). [observed text]
- Distribution: "A public binary installer, npm installation and Marketplace release are not promised by these docs." (`docs/public/index.md:23`). VSIX is "local packaging, not Marketplace publication" (`docs/public/tools.md:76`). [observed text]
- Hosting: the site "does not expose the native report storage API" (`docs/public/publishing.md:7`); "There is no tested `richview.ai` self-service publishing endpoint in this guide." (`docs/public/index.md:79`); "This teaser does not publish reports on your behalf." (live home). [observed]
- Truth claims: "These checks do not certify analytical truth." (`skill/SKILL.md:14`). [observed text]
- Scope of S27-616: "fully interactive" was "immediately clarified to animation quality … without turning this into an unrelated dashboard-feature programme". [stated]
- Publish v1 deferrals: editions, newsletters, e-paper/print, a full WYSIWYG editor. Not required: payments/paywall, ad tech, comments, CRM, multi-tenancy, a CMS marketplace (`tasks/richview-publish.md:43`). [stated]

## Release plan and status over time

| Date | Plan or status | Source | Tag |
| --- | --- | --- | --- |
| 2026-06-10 | npm publish and changesets "deferred until launch (repo is private)" | Notion "Architecture — the five packages" | stated |
| 2026-08-08 | Ship a focused public launch by 2026-08-31: understand the architecture, lock a narrow scope, polish one end-to-end example, publish positioning and one "try it" path; everything else to Icebox; "finish the focused release before reopening Icebox work" | DIR-121, S27-401, Linear project P-S27-32 | stated; date passed [observed] |
| 2026-08-17 | "Ship richview.ai to real users: landing, onboarding, publish flow, and first feedback loop." Next action: "Define the launch wedge: first users, first artifact they publish, and the loop that brings them back." | Notion "Launching richview.ai" | stated; no answer found [inferred] |
| 2026-10-02 | Considering keeping richview.ai "available as a beta, with a clear message that something new is coming, and rebuilding it later"; a series of LinkedIn and X posts (charts, themes, an unexpected custom-report use case) | Notion "2026-10-02 — Friday" | stated |
| 2026-10-04 | Teaser public on Cloudflare; Daniel authorized "merge and deploy everything" | Notion "2026-10-05 — Monday" ("The existing teaser is already public."); S27-577 via [linear.md](linear.md) | stated |
| 2026-10-05 | Coming-soon homepage with signup prepared for review; publication of finished docs and flagship authorized; seven-slide carousel approved as an awareness teaser, nothing posted | Notion "2026-10-05 — Monday" | stated |
| 2026-10-07 | Post only after reviewing what the slides claim and what public links support; "Keep 'coming soon' distinct from code that is already publicly available"; handoff warning "open source is still a promise while the repository is private" | Notion "2026-10-07 — Wednesday" | stated |
| 2026-10-18 | "all the changes on the richview.ai should be published right away and pushed to completion, because the current state is very unsatisfactory"; RichView News launched at news.richview.ai | S27-653; news `/_status` | stated; News live [observed] |
| 2026-10-18 16:03 | "The published result is not accepted yet … mobile scroll animations look jumpy/weird with visible ticking and other glitches." | S27-653 | stated |
| 2026-10-21 | Rewrite; agents auto-merge into `dev`, only Daniel merges `dev` into `prod`, "no releases"; ask before deploying or publishing richview.ai; repository moved to private `1vecera/richview` | task brief; S27-677; `gh api` | stated; transfer observed |
| 2026-10-21 (live labels) | "Coming soon." and "The coming release includes the CLI, MCP tools, report server and extension." (home); "The source will be published with the release. It is not public yet." (home); "RichView v3 alpha" (docs sidebar); "current Rust/Wasm alpha" (`docs/public/index.md:7`); `richview-vscode-linux-x64-3.0.0-alpha.1.vsix` (`docs/public/tools.md:76`); share image "BETA · richview.ai" (`/landing/og.png`) | live site; repo docs | observed |

## Daniel's decisions versus proposals

### Decided by Daniel (dated)

| Date | Decision | Source | Status on 2026-10-21 |
| --- | --- | --- | --- |
| 2026-06-08 | Focus on user value; park monetization and the business-model work | Notion "Strategy & business model (parked)"; Daniel's notes: RichView — brief & compiled findings (2026-06-08), line 19 | Still parked; the AWS credits and cost scenarios in S27-616 do not reopen pricing [inferred] |
| 2026-06-09 | Keep the brand RichView and richview.uk ("Daniel: keep") | Daniel's notes: RichView — brief & compiled findings (2026-06-08, updated 2026-06-09), line 76 | The product now lives on richview.ai; the CLI default server is still `https://richview.uk` (`crates/richview-app/src/cli.rs:122`; D-PUBLISH-TARGET in [linear.md](linear.md)) [observed] |
| 2026-06-20 | Narrow to data stories; minimize code footprint; keep custom charting as a core bet; separate content (`.rmd`) from cascading style files; web is a playground, not an editor or chat; version the format with required `title`, `description`, `version` from `0.0.1`; responsive; media (audio, video, images); for coding-agent users only | voice dump `:36-48`, `:63-87`, `:126-251`; S27-416, S27-419, S27-420 | Mostly reflected in code; scope later widened (D-SCOPE-CORE). For publication, `description` only warns when missing (`docs/RMD-SPEC.md:9`, `docs/public/publishing.md:55`), yet `llms.txt:21` says publication validates it (D-FRONTMATTER in [history-prs.md](history-prs.md)) [observed] |
| 2026-06-21 | Ship three templates: Brutalist as default ("Tell your story in style", "heavy, opinionated, full of soul"), Sleek (LampLight-based, dark, airy, large type, "Wow your readers"), Scientific (working paper, "Communicate your findings confidently."); three options per setting; all accents blue ("as Claude is orange and we're blue": sleek about `#0433ff`, brutalist a fuller `#144e95`, paper `#1A6DAC`) | Daniel's notes: Daniel's manual design system (2026-06-21), lines 4-35 (date from the embedded image name `20260621`) | Expanded to five canonical styles and a 23-style flagship; default changed (D-DEFAULT-STYLE, D-STYLE-CATALOG) [observed] |
| 2026-08-08 | Focused public launch by 2026-08-31 as a curated data-to-report layer with one try path; earlier work to Icebox | DIR-121, S27-401 | Date passed; superseded by the October batches "despite older August scope locks" ([linear.md](linear.md)) |
| 2026-10-02 | Kickoff: interest and feedback from analysts and developers using coding agents; OpenAI-versus-Anthropic flagship (80% content, 20% style); a new Rust chart library benchmarked against Highcharts, kept separate at first; a theme-switching showcase; a scrolling beta website with a 15-second motion reel | Notion "2026-10-02 — Friday" | Delivered and live: flagship, `/charts/`, `/reel/`. The Rust engine became the production engine (PR #10, see [history-prs.md](history-prs.md)) [observed] |
| 2026-10-04 | Every report in its own folder, with hard failure otherwise; chart data and loading definitions inside each chart's YAML, not the header; iterate charts to Highcharts quality. Rust "feels much less tedious than I expected and very useful; I want to work mostly with it going forward." RichView "is much rougher and further from a finished product than I expected. I do not like the launch site" | Notion "2026-10-03–04 — Weekend" | Folder rule and chart-local data shipped (PR #15 per [linear.md](linear.md)); the live home shows the folder workflow [observed] |
| 2026-10-05 | Clean up the existing repo rather than start fresh; "The source repo retains its current visibility; no fresh repo/archive is planned"; publish the finished docs and flagship; homepage and signup for review; report folders, optional styling YAML, chart-local data and `Rendered` output remain required | Notion "2026-10-05 — Monday" | Superseded on 2026-10-21 by the rewrite and the move to a private repo [observed transfer; inferred supersession] |
| 2026-10-05 | "Coming soon", never "beta"; five elements; Sleek as the homepage default; "RichView will be fully open source" as direction | S27-590, S27-594 via [linear.md](linear.md) | Live, except the share image that still says BETA [observed] |
| 2026-10-07 | Keep "coming soon" distinct from code that is publicly available; software quality and a truthful account ahead of posting | Notion "2026-10-07 — Wednesday" | Live copy complies ("It is not public yet.") [observed] |
| 2026-10-10 | Product brief: five themes with Sleek dark as default and a new hand-drawn theme; Highcharts and ECharts-grade motion; readable PDF and LinkedIn carousel PDF; output markers; scrollytelling; data and media hosting; AWS hosting and credits | S27-616 | Partly shipped (five builtins, hand-drawn, PDF, asset upload); "Working on" [stated] |
| 2026-10-12/13 | Landing feedback: new lede; drop the coming-soon pill, eyebrow and background mark animation; five themes in the demo; publishing server shown as a menu with share/copy/protect; drop Playground nav links; strip showcase header controls; charts default to the text measure | commit `bcea882` (PR #58) | Partly reversed on 2026-10-18 (motion restored, D-LANDING-MOTION); the removed body attribute broke the gutters [observed] |
| 2026-10-16 | "My RichView review today showed that the long-running work is bearing fruit."; Mondays and Fridays are own-project days, mainly RichView.ai, AgenticAnalytics.cz and Mluva | Notion "2026-10-16 — Friday" | Context for review cadence |
| 2026-10-18 | Restore site motion, cut buttons and CTAs, move the chart gallery into docs with "ALL actually supported chart types" (nine), "Retain sensible redirects/links for existing public gallery routes"; publish right away. Later the same day: not accepted, mobile scrolling jumpy | S27-653 | Deployed from unmerged PR #69; old gallery routes return 404 [observed] |
| 2026-10-18 | Name "RichView Publish" (final correction from "RichView Publishing"); demo "RichView News" at news.richview.ai; FT.com-level editorial completeness for a small team; Git-managed | `tasks/richview-publish.md:5-7` (S27-655) | Live demo; PR #70 still a draft ([linear.md](linear.md)) [observed live] |
| 2026-10-21 | Rewrite from scratch; lockdown with prefilled IDs; `dev`/`prod`, Daniel merges to `prod`; he reviews only design-quality and feature decisions after fixes; move to private `1vecera/richview` | task brief; S27-677 | Transfer done [observed]; the rest is this lockdown |

### Proposed by drafts, research or agents (not decided)

| Date | Proposal | Source | Fate |
| --- | --- | --- | --- |
| 2026-06-08 | "The ONE material new thing — a self-validating chart library"; an open-decisions list (first user, headline value, chart library as hero or feature, validation surface, YAML vs escape hatch, v1 chart scope, D3 engine bet, two-audience framing, the one "users love it" signal) | Daniel's notes: RichView — brief & compiled findings (2026-06-08), lines 33 and 76 | Diagnostics shipped as `richview check`; most open decisions were never closed explicitly [inferred] |
| 2026-06-08 | Business models: open-core diagnostics cloud, prosumer hosted reports ($9–19/mo), OEM/embedding; Elastic-2.0; kill-review | Notion "Strategy & business model (parked)" | Parked by Daniel |
| 2026-06-08 | `docs/reposition/` user-value and strategy SPAs ("The presentation layer for AI agents — now the charts check their own work.") | Notion research page; repo history | Deleted in `c53f7b6` (2026-10-04) [observed] |
| 2026-06-09 | Layered ICP; "Never market RichView as 'open source'" under ELv2 (later softened by the MIT gradient); fix the TRichView brand collision; close the PDF/white-label export gap | Daniel's notes: Positioning, ICP & licensing (2026-06-09), lines 34-112 | PDF export shipped (`docs/public/publishing.md:19-32`); open-source wording unresolved (D-OPEN-SOURCE) |
| 2026-06-09 | Keep the monorepo; licence gradient: MIT everywhere except an Elastic-2.0 `@richview/server` (ADR VEN-1193 "accepted"; the decider is not named on the page) | Notion "Architecture — the five packages"; `LICENSING.md` (`ce68a8d`) | Monorepo kept. The Rust server crate is MIT (`crates/richview-app/Cargo.toml:6`), so the gradient no longer maps to code [observed] |
| 2026-06-20 | Design-system studio: 20 variants of one report, "`v05 Blueprint` = recommended house pick"; "Lock the RichView house style (answer the 10 design questions)" (VEN-2157); accent `#1A6DAC` | Daniel's notes: 2026-06-20 — Design system studio + repo split, lines 30 and 64; Daniel's notes: Suggested tasks — from the 2026-06-20 design-system session, lines 14-17 | Daniel answered with three templates on 2026-06-21. The live flagship still offers all 20 as "Design explorations", labelling v05 "editorial (house pick)" [observed] |
| 2026-07-07 | "Goal of v3: … minimum surface"; ECharts SSR-to-SVG with "zero client-side JS and no external library" | `git show 5ceb7d4:docs/OVERVIEW.md:3`, `:65` | Replaced by Rust/Wasm with an embedded runtime (D-INTERACTIVITY) [observed] |
| 2026-10-18 | Product family architecture, roles (Reader, Editor, Super editor), urgent hide, Git-to-live loop | `tasks/richview-publish.md` (coordinator-written; the name correction is attributed to Daniel) | Live demo; core-versus-sibling scope open (D-SCOPE-CORE) |
| 2026-10-18 | "Core merge/release authority remains separate from task preparation and the website owner's scoped deployment authority."; "A review status is not a test result." | Notion "RichView: build provenance and reader checks — PENDING REVIEW" (agent-written) | Input to part 02 |
| 2026-10-20 | "Deterministic checks decide; review agents and LLM judges advise." Corrections move into checks; sample merged deliveries instead of blocking review | Notion "Locking down agent deliveries — verification principles — PENDING REVIEW" | Input to part 02; generic, not RichView-specific |

## richview.ai page inventory (live, 2026-10-21)

Live richview.ai serves the landing template from unmerged `origin/task/s27-653-site` (PR #69). Its navigation (Flagship report, Chart gallery → `/docs/charts.html`, Docs) matches that branch. `origin/main` has a different navigation (Live example, Product, Workflow, Flagship report, Docs, Get updates). [observed: link lists of the live HTML and both branches' `assets/landing/index.html`] The production baseline is D-PROD-BASELINE.

### Home `/`

- Head: `<title>` "RichView — a presentation layer for your agent"; meta description as quoted above; og:description "Markdown plus a few validated YAML blocks in; a typeset, self-contained HTML data story out. Rich Markdown, themes, a rendering engine, a publishing server and a VS Code extension."; og:image `/landing/og.png` reads "Your agent writes text. RichView renders the story." and "BETA · richview.ai". [observed]
- Hero: eyebrow "Agentic data stories"; H1 "RichView is a presentation layer for your agent."; lede "Your agent organizes the text. RichView makes it a story."; CTAs "Get release updates" (`#signup`), "Read the flagship report", "Edit the live example ↓". [observed]
- Demo, "One file in. One report out.": "Markdown with a few validated YAML blocks goes in; a typeset, self-contained HTML report comes out. Edit the source and the report follows as you type, rendered in your browser by the same Rust engine as the command line." Editable `report.rmd` and optional `report.rstyle.yaml` tabs; Reset example; theme radio Sleek / Sleek Light / Scientific / Brutalist / Hand‑drawn (Sleek preselected); rendered iframe `/landing/generated/Rendered/demo-sleek.html` ("Who Hacker News mentions"). Provenance note: "the fixed October 2 research snapshot of Hacker News comments … One community's conversation, not market share." It shows in-browser Wasm rendering of real source in five themes. [observed]
- 01 What is RichView: "An engine for humans and agents that turns text and data into beautiful reports."; "Style Decided Once. Reused Every Time."; "The report source remains editable and versioned. Render it to HTML, then share an immutable version through a RichView server." A bar chart carries a `richview check --format json` card ("data: 4 sourced rows", "geometry: inspect check output", "values: all numeric"). [observed]
- 02 The product, "Five elements. One way to tell the story.": Rich Markdown ("Readable by people. Writable by agents. Diffable in Git."; "A schema checks the block; useful diagnostics point back to the source."). Themes ("Same words and numbers. Your treatment."; start with sleek, scientific or brutalist; an optional `.rstyle.yaml` sets "your own type, colour and spacing"). Rendering engine ("A rendering engine with a point of view"; "Text and data in. HTML with real SVG charts out."; "Rust · native + Wasm"). Publishing server ("Publish to an authenticated RichView server and get a menu of your finished reports. Each one gets a stable URL you can share, copy or lock down with access control. Versions are immutable"; a mock menu `/r/sales-review` shared, `/r/okurka-weekly` private, `/r/vltava-forecast`). VS Code ("Live rendering. Syntax highlighting. Chart snippets. One shortcut to publish."). [observed]
- 03 Workflow: "Write the story. Link the evidence." (one folder per report, CSV linked inside the chart); "Choose the treatment."; "Render and review." (`richview_render` through MCP or the CLI; output in `Rendered/`); "Publish an explicit version." (server plus bearer token, `richview_publish`, `RICHVIEW_TOKEN`); "The coming release includes the CLI, MCP tools, report server and extension. This teaser does not publish reports on your behalf." [observed]
- 04 Why RichView: see Positioning. 05 Flagship: "The background was the data all along." (background dots are monthly Hacker News comments, "one of 643 dated sources behind the flagship report"); the verdict quote; links to Sleek, Scientific and Brutalist renders, `report.rmd`, sources and collection-runs CSVs, Chart gallery, Docs. [observed]
- 06 Release updates: "Coming soon." "A better way to share what your agent finds." Email field plus consent checkbox ("I agree to receive RichView release and product updates by email."); "No email is sent when you sign up."; aside "RichView will be fully open source. The source will be published with the release. It is not public yet." The form posts to `/api/interest` (GET returns 405); not submitted (D-SIGNUP). Footer "RichView · Daniel Vecera", Impressum, Privacy. [observed]
- Defect, gutters [observed]: section content starts at x = 0 at both 1440 and 390 px. Computed `main` padding is 0 and max-width `none`. Setting `data-homepage` on `<body>` in headless Chromium restores 58 px (desktop) and 21 px (mobile) gutters. Cause: 26 `body[data-homepage]` rules in `assets/landing/homepage.css` (for example line 83, `max-width: var(--measure); margin: 0 auto; padding: 0 var(--gutter)`) stopped matching when `bcea882` (PR #58, 2026-10-13) removed `data-homepage="coming-soon"`. `origin/main` has the same rules and body, so the defect is not branch-specific.
- Other defects [observed]: horizontal overflow on mobile (document width 426 px at a 390 px viewport); "Docs" cut off in the mobile nav; the "05 The flagship report" eyebrow overlaps the H2 at 1440 px; the check card hides the tallest bar's value label in section 01; the signup aside runs to the right viewport edge. Full-page captures show the scroll-driven element stages as long blank stretches, so use the per-section captures to judge the look. Daniel's "jumpy" mobile scrolling (S27-653) was not reproduced here because only static frames were taken.

### Flagship `/showcase/openai-vs-anthropic/`

- Shell: H1 "One report. Many voices."; meta "One sourced report, 23 styles. Compare the evidence, inspect the style files and follow every claim to its source."; og:title "One report. Every RichView style." Style pills Sleek / Scientific / Brutalist plus an "All styles" select: three house styles and 20 "Design explorations" (v01 "The Original" … v05 "editorial (house pick)" … v20 "Swiss Grid"). Neither sleek-light nor hand-drawn is offered. Default Scientific. Footer: "The wording, figures and evidence stay fixed."; "643 dated ledger entries" and "Collection runs" CSV links; a fingerprint; a style drawer ("What changed", "The style file · .rstyle.yaml", "Copy style file", "Signature rules", "Provenance"); an interactive chart dialog. [observed]
- Report (`Rendered/report.html`): H1 "Anthropic is winning the people who build with AI. OpenAI still has everyone else"; sections from "The verdict" to "Who is winning, by dimension"; 96 SVGs; per-chart Table / SVG / CSV / Expand controls and series toggles. Desktop height 30,706 px, mobile 46,902 px. [observed]
- It shows a fixed, sourced research snapshot rendered under many presentations with constant evidence, which is Daniel's 2026-10-02 showcase intent. [observed; stated intent]

### Docs `/docs/` (index, authoring, charts, styles, tools, publishing, recovery)

- Sidebar: Getting started, Writing Rich Markdown, Chart gallery, Themes & styling, CLI/MCP & VS Code, Export & publishing, Troubleshooting; "RichView v3 alpha · Readable source. Reports you can inspect." [observed]
- Index "Your first RichView report": the "Five pieces, one workflow" table (Rich Markdown; Themes and styling YAML; Renderer; Publishing server, "the public website currently serves static examples"; VS Code human-agent authoring). Build from source with `git clone git@github.com:richview-universe/richview-v3.git` (now a redirect to a private repo). "Source-format charts support line, bar, scatter, area, pie and heatmap." [observed; `docs/public/index.md:11-26`, `:77`]
- Chart gallery `/docs/charts`: "RichView renders nine chart types from the same chart fence in Rich Markdown", each with a rendered example and its source. It links "the decorative chart-gallery and famous-charts showcases" and typography at `/chart-gallery/`, `/famous-charts/` and `/typography/`, all of which return 404. `docs/public/charts.md` exists only on `origin/task/s27-653-site`. [observed] See D-GALLERY-ROUTES.
- Styles, tools, publishing and recovery match `docs/public/*.md` at `b4db233` (canonical five styles, seven MCP tools, PDF export, loopback publishing, asset upload). [observed headings; text in repo]

### Playground `/playground/`

- "Source → story": a Style select (Source / default, Scientific, Sleek, Sleek light, Brutalist, Hand-drawn), Check, Copy source, Download .rmd, Download report, and a diagnostics pane ("Rust render 15.5 ms · 1 charts", "glyph bounds remain estimates until measured by the browser"). [observed]
- It shows the zero-install inline-data try path that the docs promise (`docs/public/index.md:9`). [observed]
- Defect [observed]: the default example's monthly data (`2026-01` to `2026-03`) renders with weekly tick labels "Jan 5 … Feb 23". `docs/public/authoring.md:49` says periods such as `2025-08` "print as `Aug 2025`".

### Other richview.ai routes

- `/charts/`, "RichView Chart · time series": "A new charting engine for RichView", "Time series that hold up under scrutiny.", Rust compiled to Wasm; "One JSON spec drives the static SVG in a report and the live chart on this page"; `richview-chart validate` / `render`; theme buttons Editorial / Brutalist / Scientific / Sleek; CO₂, CPI and German power charts with range, zero toggle and downloads. Its "Rules the engine enforces" include "Nothing is interpolated." and "No independent dual axes." The live home and docs navigation do not link it; `README.md:17` does. It shows the 2026-10-02 Rust chart library brief. [observed]
- `/reel/`, "RichView — 15-second motion reel": 45 months of Hacker News comments, "Jul 2025 — Anthropic passes OpenAI", a beat counter ("Bar 5/8 · 120 BPM"), a scrubber with keyboard hints. It is the motion showcase from S27-398 (`README.md:19`). [observed]
- `/docs/example/Rendered/{report,scientific,sleek,brutalist,hand-drawn}.html`: one small report ("Orders rose each month") rendered in five treatments; it shows that styles change presentation and keep the observations. [observed]
- `/privacy.html` (read, not captured): the signup stores email and consent in private Cloudflare D1, no emails are sent, no tracking cookies; "Anyone with access to a report address may be able to read its report and source"; its nav is stale (How it works / Get updates / Playground); footer "A hobby project by Daniel Vecera." [observed]
- `/sitemap.xml`, `/internal`, `/chart-gallery/`, `/famous-charts/`, `/typography/` return 404. [observed]

## news.richview.ai inventory (live, 2026-10-21)

- Home `/`: "Latest perspectives — RichView News"; utility line "Independent perspectives · Interactive evidence"; "Editorial desk" (`/_editor/`, Cloudflare Access login); masthead tagline "A working notebook on evidence, systems and better decisions."; nav Research, Systems, Field notes, About, Archive, Search, Member edition (`/internal`, Cloudflare Access login); "The briefing — Ideas worth examining. Evidence you can explore."; seven stories and one dashboard snapshot by "RichView Example Desk", dated 2026-10-13 to 2026-10-18. Footer: "An ongoing example publication, made with RichView Publish." The link goes to `github.com/richview-universe/richview-v3`, which returns 404 to anonymous visitors (private). Also About & corrections, RSS (`/feed.xml`, 200), and "Release 33843a62a65c" → `/_status`. [observed]
- `/_status`: `{"state":"live","release":"33843a62…","generation":5,"paused":false,"commit":"7e7d9f8…"}`. `7e7d9f8` is the merge of PR #72 into `publish/news` (2026-10-18 22:55 CEST). [observed]
- Sections: Research ("What the numbers can — and cannot — tell us."), Systems ("How useful evidence makes its way into the world."), Field notes ("Small observations from the editorial workbench."). Archive and Search list all eight items. [observed]
- Stories (for example `/stories/measurement-gap`, "An illustrative study of how a change in the denominator can reverse the story a metric appears to tell."): headline, dek, byline, "Open the full report", and an iframe `/_content/<id>/report.html?embed=1` with the RichView-rendered report. `/stories/returning-to-git` adds "Corrections & updates". [observed]
- Dashboard `/dashboards/example-trading-desk`: "Published snapshot · Synthetic demonstration"; "This is a fixed dashboard capture … Values do not refresh from a live service."; "Captured 2026-10-18T09:00:00Z · Adapter: offline-capture". Six tiles, each with a "Source & query receipt" JSON (metric requests; "Synthetic demonstration from the bundled offline fixture. No real business data."). Defects: black filled wedges between series in "Revenue trend" and "EBITDA"; a truncated axis label "plan gap lo". [observed]
- About: "A maintained example publication for RichView Publish."; "All numerical observations are synthetic unless a story explicitly states a dated source."; "The publication brings together Rich Markdown, the RichView Charts renderer, report experiences and dashboard snapshots. Git holds the authored sources and ordinary editorial configuration."; editors "can urgently withdraw content in their section", super editors "review a different actor's change"; "The platform source uses the monorepo's MIT license. The repository's current visibility is independent of that license." [observed]
- It shows RichView Publish: Git-managed publication, sections, archive, search, RSS, embedded reports, a dashboard snapshot with receipts, and Access-protected member and editor areas. The member and editor areas were not inspected. [observed; gap]

## Repository documents: intent-relevant claims at `b4db233`

| Location | Claim | Note |
| --- | --- | --- |
| `README.md:3` | "turns Rich Markdown (`.rmd`) into sourced, editable reports. Rust owns parsing, validation, data transforms, chart layout, SVG rendering, diagnostics, browser interaction, authoring and version storage." | Product as engine, not as presentation layer |
| `README.md:17` | `/showcase/openai-vs-anthropic/`, "the sourced 18-chart report with 23 presentation styles"; `/charts/` for the time-series examples | `/charts/` is not linked from the live site |
| `README.md:62`; `CLAUDE.md:70` | Nine source-format chart types | Conflicts with six elsewhere |
| `README.md:77`, `:85` | "No deployment, release or hosted publication is part of the local build."; "The source repository retains its existing visibility … deployment requires explicit authorization" | Matches the 2026-10-05 decision; superseded by the 2026-10-21 move |
| `docs/OVERVIEW.md:3-5` | "source-centered report product built in Rust"; "Publication stores immutable versions and retained source/resources for regeneration." | |
| `llms.txt:11`, `:13`, `:25` | Six chart types; "Select scientific, sleek or brutalist"; seven MCP tools | Three styles, six types |
| `skill/SKILL.md:12`, `:14`, `:18` | Six chart types; scientific, sleek or brutalist; "These checks do not certify analytical truth."; "Publish only when explicitly requested" | |
| `skill/reference/anti-slop.md` | Assertion titles, synthetic data labelled, no gradients, glass or indiscriminate rounded cards | Same vocabulary as the S27-616 maximize list |
| `docs/public/index.md:7`, `:18`, `:23`, `:77`, `:79` | "current Rust/Wasm alpha"; "the public website currently serves static examples"; no public installer, npm or Marketplace promised; six chart types; no tested richview.ai self-service publishing endpoint | Served live at `/docs/` |
| `docs/public/styles.md:7`, `:44`, `:75` | Canonical `scientific`, `sleek`, `sleek-light`, `brutalist`, `hand-drawn`, default `brutalist`; "`editorial` is a deprecated alias"; the flagship selector "has more treatments than the canonical source styles" | Five styles; the flagship still labels "editorial" as the house pick |
| `docs/public/publishing.md:7`, `:128`, `:130` | The site does not expose the storage API; "bearer authentication protects writes, not report reads"; no CLI command "makes the private source repository public" | Contradicts the home's "access control" |
| `docs/public/tools.md:38`, `:76`, `:82` | "seven shipped tools"; VSIX local packaging; "Insert Chart Block (all six types)" | Six types |
| `docs/AGENT-QUICKSTART.md:13` | "All six chart types use the `type/data/encoding/axes/annotations` vocabulary." | Six types |
| `docs/TABLE-AUTHORING-CHOICE.md:1-5` | "Table authoring choice — pending Daniel's decision"; row mappings recommended | Open (D-TABLE-SYNTAX) |
| `docs/MOTION.md:3`, `:7` | "Nothing moves without one of these reasons."; "Report text, headings, metrics and callouts never animate" | Report motion policy; the landing page runs its own scroll motion |
| `docs/FEATURES.md:20` | "Five builtin treatments and showcase variants" | |
| `LICENSE`; `LICENSING.md:4-17`; `packages/server/LICENSE`; `crates/richview-app/Cargo.toml:6` | Root MIT; "everything is MIT except the server" (ELv2); the ELv2 file sits beside only `fly.toml`; the Rust server crate declares MIT | The gradient no longer maps to code |

## Contradictions

- Product definition (D-POSITIONING). "Presentation layer for data stories" (2026-06-20, live H1) versus "curated JSON/data-to-report rendering layer" (2026-08-08) versus "turns `.rmd` into sourced, editable reports" and "source-centered report product built in Rust" (`README.md:3`, `docs/OVERVIEW.md:3`) versus a family of Reports, Dashboards, Charts and Publish (S27-655). Taglines also differ: live lede "Your agent organizes the text. RichView makes it a story."; share image "Your agent writes text. RichView renders the story."; eyebrow "Agentic data stories"; headline candidate "Storytelling for Agents". One sentence and one tagline need to win.
- Scope (D-SCOPE-CORE). "not dashboards, not generic BI, not a full chat/web-editor product" (S27-420) and "Built for data stories, not dashboards." (June site) versus the live News dashboard snapshot from "the RichView Dashboard runtime", Publish's browser editorial desk with roles, and the S27-651 voice dashboard ([linear.md](linear.md)).
- Audience (D-AUDIENCE). "only for people who have coding agents" (2026-06-20) versus "An engine for humans and agents" (live) versus "Analysts and coding agents" (`docs/OVERVIEW.md:3`) versus "a small intelligence or specialist news publisher" (S27-655) versus LinkedIn carousel audiences (S27-616).
- Open source, licence and visibility (D-OPEN-SOURCE). The live home says "RichView will be fully open source … It is not public yet." The carousel brief says "Open source · Built with Rust" (2026-10-05). The Publish brief says "open-source", and News About says "MIT license … visibility independent". Against these: the repository moved to private `1vecera/richview` on 2026-10-21, after the 2026-10-05 decision that "The source repo retains its current visibility". `LICENSING.md` keeps an ELv2 server while the server crate is MIT. June research said never to market ELv2 as open source. Privacy calls it "A hobby project".
- Release state (D-RELEASE). The 2026-08-31 launch date passed (Linear project still targets it). Live labels mix "Coming soon", "BETA" (share image, despite Daniel's 2026-10-05 "never beta"), "v3 alpha" and `3.0.0-alpha.1`. The home promises "The coming release includes the CLI, MCP tools, report server and extension", while the 2026-10-21 model says "no releases".
- Chart-type count (D-TYP-DOC-DRIFT). Six in `llms.txt:11`, `skill/SKILL.md:12`, `docs/public/index.md:77` (live `/docs/`), `docs/AGENT-QUICKSTART.md:13`, `docs/public/tools.md:82`. Nine in `README.md:62`, `CLAUDE.md:70` and the live `/docs/charts` on the same site.
- Style inventory and default (D-STYLE-CATALOG, D-DEFAULT-STYLE). Three canonical (`CLAUDE.md:76`, `llms.txt:13`, `skill/SKILL.md:12`, home copy and VS Code snippets) versus five (`docs/public/styles.md:7`, demo radio, playground, S27-616) versus 23 on the flagship. The flagship's 20 studio variants include v05 "editorial (house pick)", while `editorial` is a deprecated alias (`docs/public/styles.md:44`) and `/charts/` offers "Editorial". Defaults: Brutalist (Daniel 2026-06-21; `docs/public/styles.md:7`), Sleek dark (S27-616), Scientific (flagship, observed), Sleek (home demo, observed).
- Customization depth (D-STYLE-CATALOG). Daniel on 2026-06-21: three options per setting and "v1 should have no way to adj. it in any heavier way". Current code: the open `.rstyle.yaml` grammar (`tokens`, `layout`, `chart`, `components`, `motion`; `docs/public/styles.md:44`), and the home invites users "to set your own type, colour and spacing".
- Access control (D-ACCESS-CLAIM, D-PUBLISH-TARGET). The home says reports can be shared, copied "or lock down with access control", showing a "private" badge and "a menu of your finished reports". The docs say "bearer authentication protects writes, not report reads" (`docs/public/publishing.md:128`) and list no all-reports menu route. Privacy says anyone with a report address may read it. Only Publish (News) has read protection, through Cloudflare Access. [observed; menu absence inferred from the route table]
- Client-side code (D-INTERACTIVITY). "zero client-side JS and no external library" (`5ceb7d4:docs/OVERVIEW.md:65`) versus "Generated complete HTML embeds its runtime" (`skill/SKILL.md:16`, `llms.txt:19`).
- Authoring format. `/charts/` advertises "One JSON spec", and DIR-121 and S27-398 speak of a "JSON/data-to-report" layer with a "JSON-renderer inspiration". Everything else is YAML fences in Rich Markdown. Decide whether the JSON chart contract survives beside the YAML fences (part 05) and whether the unlinked `/charts/` page stays (D-SITE-NAV).
- Chart rules (to parts 04 and 05). `/charts/` states "No independent dual axes. A second axis must be a declared conversion of the first". The live docs gallery says "`encoding.y2:` adds a dual axis when the units differ". The docs promise month labels for period data (`docs/public/authoring.md:49`), but the playground's default example shows weekly ticks.
- Live versus main (D-PROD-BASELINE). Live richview.ai matches unmerged PR #69. News runs content commit `7e7d9f8` on `publish/news` beside draft PR #70. `main` matches neither. The gutter defect is on both.
- Gallery routes (D-GALLERY-ROUTES). S27-653 required "Retain sensible redirects/links for existing public gallery routes". `/chart-gallery/`, `/famous-charts/` and `/typography/` return 404, and the live docs gallery still links them.
- Landing motion (D-LANDING-MOTION). PR #58 (2026-10-12 feedback) dropped the background mark animation. S27-653 (2026-10-18) restored the motion, and the same day Daniel did not accept it ("jumpy/weird").
- Repository strategy. 2026-10-05: "Clean up the existing repo"; no fresh repo; visibility unchanged. 2026-10-21: rewrite from scratch in a private personal repository. Live docs still instruct `git clone git@github.com:richview-universe/richview-v3.git`, and the News footer links the old URL.
- Brand colour. Daniel on 2026-06-21: "All 3 are blueish as Claude is orange and we're blue". The live demo's style example suggests `accent: "#C4613F"` (clay), and the flagship colours Anthropic in clay. [observed; whether semantic data colour counts as brand colour is inferred]

## Suggested items for part 01

Proposals only; the part file assigns final `INT-` IDs.

| Topic | Proposed answer for Daniel to accept or edit | Basis | Confidence |
| --- | --- | --- | --- |
| One-line definition (D-POSITIONING) | "RichView is a presentation layer for your agent: plain-text Rich Markdown with chart-local data in, a sourced, typeset data story out." | 2026-06-20 dump; S27-590; live H1 and meta | inferred |
| Tagline | "Your agent organizes the text. RichView makes it a story." Retire "writes text … renders the story" and the BETA share image | PR #58; live hero; og.png | inferred |
| Unit of work (D-SCOPE-CORE) | Data stories (reports) are the core; Dashboards and Publish are siblings that reuse the engine and are scoped separately, or dropped | S27-420, DIR-121, S27-655 | inferred |
| Primary audience (D-AUDIENCE) | Authors: analysts and developers who work with coding agents. Secondary: the readers of their stories on desktop, mobile and PDF | 2026-06-20; 2026-10-02; S27-616 | stated sources; choice inferred |
| What RichView maximizes (D-MAXIMIZE) | Truthful, sourced evidence first; editorial and chart craft that withstands Highcharts, ECharts and FT-level critique; a short agent-friendly authoring path; a small code footprint; reusable style decided once; low operating cost | S27-616; dump `:43`; home section 04; 2026-10-02 (80/20) | stated sources; ranking inferred |
| Ambitions to own (D-AMBITION) | Pick from: self-checking charts with machine-readable diagnostics; one sourced report across many coherent styles; Highcharts-level craft from short YAML; Git-native editorial publishing | June research; 2026-10-02 briefs; S27-655 | inferred |
| Non-goals | Not dashboards or BI in the core; no chat or WYSIWYG editor; no arbitrary CSS or JS; no promise of public installers or hosted self-service publishing; checks do not certify analytical truth; monetization parked | see Non-goals | stated |
| Release posture (D-RELEASE) | No public "release" during the rewrite; richview.ai says "Coming soon" until Daniel merges `dev` to `prod`; drop "beta" and "alpha" from public surfaces | 2026-10-05; 2026-10-21 brief | inferred |
| Open source (D-OPEN-SOURCE) | Decide whether "fully open source" stays a public promise while the repo is private, and which licence applies (all-MIT or an ELv2 server) | home; LICENSING; transfer | unknown |
| Access claim (D-ACCESS-CLAIM) | Either implement private reads or remove "lock down with access control" and "private" from the home | home vs `publishing.md:128` | observed conflict |
| Website role | richview.ai: proof of how beautiful RichView is, with try path, docs and flagship; news.richview.ai: the Publish demo, or retired if Publish is out of scope | S27-647, S27-653, S27-655 | stated |
| Evidence policy | Every claim on the site and in reports traces to a dated source; synthetic data labelled; the same evidence under every style | flagship; anti-slop; S27-573 | stated |

## Screenshots

Captured 2026-10-21 between 20:27 and 20:31 CEST in headless Chromium. Desktop is a 1440 × 900 CSS viewport at device scale 1. Mobile is 390 × 844 CSS at device scale 2 with touch emulation, so images are 780 px wide. "Full" is a full-page capture; captures taller than 8,000 CSS px (desktop) or 6,000 CSS px (mobile) are split into numbered parts. Home section captures are viewport shots scrolled to each section anchor. Paths are relative to `docs/lockdown/screenshots/live/`. The home captures include the public contact line that the site itself shows.

| File | URL | Viewport | Capture | Note |
| --- | --- | --- | --- | --- |
| `news-about-desktop-full.jpg` | https://news.richview.ai/about | desktop | full page |  |
| `news-about-mobile-full.jpg` | https://news.richview.ai/about | mobile | full page |  |
| `news-archive-desktop-full.jpg` | https://news.richview.ai/archive | desktop | full page |  |
| `news-archive-mobile-full.jpg` | https://news.richview.ai/archive | mobile | full page |  |
| `news-dashboard-trading-desk-desktop-full.jpg` | https://news.richview.ai/dashboards/example-trading-desk | desktop | full page | black wedges in Revenue trend and EBITDA; "plan gap lo" |
| `news-dashboard-trading-desk-mobile-full.jpg` | https://news.richview.ai/dashboards/example-trading-desk | mobile | full page |  |
| `news-home-desktop-full.jpg` | https://news.richview.ai/ | desktop | full page |  |
| `news-home-mobile-full.jpg` | https://news.richview.ai/ | mobile | full page |  |
| `news-search-desktop-full.jpg` | https://news.richview.ai/search | desktop | full page |  |
| `news-search-mobile-full.jpg` | https://news.richview.ai/search | mobile | full page |  |
| `news-section-research-desktop-full.jpg` | https://news.richview.ai/sections/research | desktop | full page |  |
| `news-section-research-mobile-full.jpg` | https://news.richview.ai/sections/research | mobile | full page |  |
| `news-story-measurement-gap-desktop-full.jpg` | https://news.richview.ai/stories/measurement-gap | desktop | full page |  |
| `news-story-measurement-gap-mobile-full.jpg` | https://news.richview.ai/stories/measurement-gap | mobile | full page |  |
| `news-story-returning-to-git-desktop-full.jpg` | https://news.richview.ai/stories/returning-to-git | desktop | full page |  |
| `news-story-returning-to-git-mobile-full.jpg` | https://news.richview.ai/stories/returning-to-git | mobile | full page |  |
| `richview-charts-timeseries-desktop-full.jpg` | https://richview.ai/charts/ | desktop | full page |  |
| `richview-charts-timeseries-mobile-full-part1of2.jpg` | https://richview.ai/charts/ | mobile | full page, part 1 of 2 (CSS y 0–6000) |  |
| `richview-charts-timeseries-mobile-full-part2of2.jpg` | https://richview.ai/charts/ | mobile | full page, part 2 of 2 (CSS y 6000–9286) |  |
| `richview-docs-authoring-desktop-full.jpg` | https://richview.ai/docs/authoring | desktop | full page |  |
| `richview-docs-authoring-mobile-full.jpg` | https://richview.ai/docs/authoring | mobile | full page |  |
| `richview-docs-chart-gallery-desktop-full-part1of2.jpg` | https://richview.ai/docs/charts | desktop | full page, part 1 of 2 (CSS y 0–8000) |  |
| `richview-docs-chart-gallery-desktop-full-part2of2.jpg` | https://richview.ai/docs/charts | desktop | full page, part 2 of 2 (CSS y 8000–11390) |  |
| `richview-docs-chart-gallery-mobile-full-part1of2.jpg` | https://richview.ai/docs/charts | mobile | full page, part 1 of 2 (CSS y 0–6000) |  |
| `richview-docs-chart-gallery-mobile-full-part2of2.jpg` | https://richview.ai/docs/charts | mobile | full page, part 2 of 2 (CSS y 6000–9737) |  |
| `richview-docs-index-desktop-full.jpg` | https://richview.ai/docs/ | desktop | full page |  |
| `richview-docs-index-mobile-full.jpg` | https://richview.ai/docs/ | mobile | full page |  |
| `richview-docs-publishing-desktop-full.jpg` | https://richview.ai/docs/publishing | desktop | full page |  |
| `richview-docs-publishing-mobile-full-part1of2.jpg` | https://richview.ai/docs/publishing | mobile | full page, part 1 of 2 (CSS y 0–6000) |  |
| `richview-docs-publishing-mobile-full-part2of2.jpg` | https://richview.ai/docs/publishing | mobile | full page, part 2 of 2 (CSS y 6000–9518) |  |
| `richview-docs-recovery-desktop-full.jpg` | https://richview.ai/docs/recovery | desktop | full page |  |
| `richview-docs-recovery-mobile-full.jpg` | https://richview.ai/docs/recovery | mobile | full page |  |
| `richview-docs-styles-desktop-full.jpg` | https://richview.ai/docs/styles | desktop | full page |  |
| `richview-docs-styles-mobile-full.jpg` | https://richview.ai/docs/styles | mobile | full page |  |
| `richview-docs-tools-desktop-full.jpg` | https://richview.ai/docs/tools | desktop | full page |  |
| `richview-docs-tools-mobile-full-part1of2.jpg` | https://richview.ai/docs/tools | mobile | full page, part 1 of 2 (CSS y 0–6000) |  |
| `richview-docs-tools-mobile-full-part2of2.jpg` | https://richview.ai/docs/tools | mobile | full page, part 2 of 2 (CSS y 6000–6058) | 116 px tail of the page |
| `richview-example-brutalist-desktop-full.jpg` | https://richview.ai/docs/example/Rendered/brutalist.html | desktop | full page |  |
| `richview-example-brutalist-mobile-full.jpg` | https://richview.ai/docs/example/Rendered/brutalist.html | mobile | full page |  |
| `richview-example-hand-drawn-desktop-full.jpg` | https://richview.ai/docs/example/Rendered/hand-drawn.html | desktop | full page |  |
| `richview-example-hand-drawn-mobile-full.jpg` | https://richview.ai/docs/example/Rendered/hand-drawn.html | mobile | full page |  |
| `richview-example-report-desktop-full.jpg` | https://richview.ai/docs/example/Rendered/report.html | desktop | full page |  |
| `richview-example-report-mobile-full.jpg` | https://richview.ai/docs/example/Rendered/report.html | mobile | full page |  |
| `richview-example-scientific-desktop-full.jpg` | https://richview.ai/docs/example/Rendered/scientific.html | desktop | full page |  |
| `richview-example-scientific-mobile-full.jpg` | https://richview.ai/docs/example/Rendered/scientific.html | mobile | full page |  |
| `richview-example-sleek-desktop-full.jpg` | https://richview.ai/docs/example/Rendered/sleek.html | desktop | full page |  |
| `richview-example-sleek-mobile-full.jpg` | https://richview.ai/docs/example/Rendered/sleek.html | mobile | full page |  |
| `richview-flagship-brutalist-desktop-viewport.png` | https://richview.ai/showcase/openai-vs-anthropic/?theme=brutalist | desktop | viewport |  |
| `richview-flagship-report-desktop-full-part1of4.jpg` | https://richview.ai/showcase/openai-vs-anthropic/Rendered/report.html | desktop | full page, part 1 of 4 (CSS y 0–8000) |  |
| `richview-flagship-report-desktop-full-part2of4.jpg` | https://richview.ai/showcase/openai-vs-anthropic/Rendered/report.html | desktop | full page, part 2 of 4 (CSS y 8000–16000) |  |
| `richview-flagship-report-desktop-full-part3of4.jpg` | https://richview.ai/showcase/openai-vs-anthropic/Rendered/report.html | desktop | full page, part 3 of 4 (CSS y 16000–24000) |  |
| `richview-flagship-report-desktop-full-part4of4.jpg` | https://richview.ai/showcase/openai-vs-anthropic/Rendered/report.html | desktop | full page, part 4 of 4 (CSS y 24000–30706) |  |
| `richview-flagship-report-mobile-full-part1of8.jpg` | https://richview.ai/showcase/openai-vs-anthropic/Rendered/report.html | mobile | full page, part 1 of 8 (CSS y 0–6000) | only parts 1–2 of 8 captured |
| `richview-flagship-report-mobile-full-part2of8.jpg` | https://richview.ai/showcase/openai-vs-anthropic/Rendered/report.html | mobile | full page, part 2 of 8 (CSS y 6000–12000) | only parts 1–2 of 8 captured |
| `richview-flagship-scientific-desktop-viewport.png` | https://richview.ai/showcase/openai-vs-anthropic/ | desktop | viewport | default style |
| `richview-flagship-scientific-mobile-viewport.png` | https://richview.ai/showcase/openai-vs-anthropic/ | mobile | viewport |  |
| `richview-flagship-sleek-desktop-viewport.png` | https://richview.ai/showcase/openai-vs-anthropic/?theme=sleek | desktop | viewport |  |
| `richview-flagship-sleek-mobile-viewport.png` | https://richview.ai/showcase/openai-vs-anthropic/?theme=sleek | mobile | viewport |  |
| `richview-home-desktop-01-hero.jpg` | https://richview.ai/ | desktop | viewport at `hero` |  |
| `richview-home-desktop-02-demo.jpg` | https://richview.ai/#demo | desktop | viewport at `#demo` |  |
| `richview-home-desktop-03-what.jpg` | https://richview.ai/#what | desktop | viewport at `#what` | check card hides tallest bar label; zero left gutter |
| `richview-home-desktop-04-elements.jpg` | https://richview.ai/#elements | desktop | viewport at `#elements` |  |
| `richview-home-desktop-04a-element-markdown.jpg` | https://richview.ai/#element-markdown | desktop | viewport at `#element-markdown` |  |
| `richview-home-desktop-04b-element-themes.jpg` | https://richview.ai/#element-themes | desktop | viewport at `#element-themes` |  |
| `richview-home-desktop-04c-element-engine.jpg` | https://richview.ai/#element-engine | desktop | viewport at `#element-engine` |  |
| `richview-home-desktop-04d-element-server.jpg` | https://richview.ai/#element-server | desktop | viewport at `#element-server` |  |
| `richview-home-desktop-04e-element-editor.jpg` | https://richview.ai/#element-editor | desktop | viewport at `#element-editor` |  |
| `richview-home-desktop-05-workflow.jpg` | https://richview.ai/#how | desktop | viewport at `#how` |  |
| `richview-home-desktop-06-why.jpg` | https://richview.ai/#why | desktop | viewport at `#why` |  |
| `richview-home-desktop-07-flagship.jpg` | https://richview.ai/#flagship | desktop | viewport at `#flagship` | eyebrow overlaps H2 |
| `richview-home-desktop-08-signup.jpg` | https://richview.ai/#signup | desktop | viewport at `#signup` | aside text reaches right edge |
| `richview-home-desktop-full-part1of2.jpg` | https://richview.ai/ | desktop | full page, part 1 of 2 (CSS y 0–8000) | scroll stages appear blank in full-page capture |
| `richview-home-desktop-full-part2of2.jpg` | https://richview.ai/ | desktop | full page, part 2 of 2 (CSS y 8000–13319) | scroll stages appear blank in full-page capture |
| `richview-home-mobile-01-hero.jpg` | https://richview.ai/ | mobile | viewport at `hero` | nav "Docs" cut off; zero gutter |
| `richview-home-mobile-02-demo.jpg` | https://richview.ai/#demo | mobile | viewport at `#demo` |  |
| `richview-home-mobile-03-what.jpg` | https://richview.ai/#what | mobile | viewport at `#what` |  |
| `richview-home-mobile-04-elements.jpg` | https://richview.ai/#elements | mobile | viewport at `#elements` |  |
| `richview-home-mobile-05-workflow.jpg` | https://richview.ai/#how | mobile | viewport at `#how` |  |
| `richview-home-mobile-06-why.jpg` | https://richview.ai/#why | mobile | viewport at `#why` |  |
| `richview-home-mobile-07-flagship.jpg` | https://richview.ai/#flagship | mobile | viewport at `#flagship` |  |
| `richview-home-mobile-08-signup.jpg` | https://richview.ai/#signup | mobile | viewport at `#signup` |  |
| `richview-home-mobile-full-part1of3.jpg` | https://richview.ai/ | mobile | full page, part 1 of 3 (CSS y 0–6000) |  |
| `richview-home-mobile-full-part2of3.jpg` | https://richview.ai/ | mobile | full page, part 2 of 3 (CSS y 6000–12000) |  |
| `richview-home-mobile-full-part3of3.jpg` | https://richview.ai/ | mobile | full page, part 3 of 3 (CSS y 12000–15312) |  |
| `richview-playground-desktop-full.jpg` | https://richview.ai/playground/ | desktop | full page | monthly data with weekly ticks |
| `richview-playground-mobile-full.jpg` | https://richview.ai/playground/ | mobile | full page |  |
| `richview-reel-desktop-t2.5s.png` | https://richview.ai/reel/ | desktop | viewport 2.5 s after load |  |
| `richview-reel-desktop-t9.0s.png` | https://richview.ai/reel/ | desktop | viewport 9 s after load |  |
| `richview-reel-mobile-t5.0s.png` | https://richview.ai/reel/ | mobile | viewport 5 s after load |  |

## Gaps

- Notion holds only stubs or wiki-path links for the June research (brief, evolution, competitive landscape, positioning/ICP, self-validating charts); their content was read from the legacy local vault, which may differ from what Notion would show.
- The 2026-06-20 voice transcript is low-quality speech recognition. Its summary and the S27-416/419/420 tickets attribute the wishes to Daniel; the note's author field names a vault writer identity, not Daniel.
- Most Linear content (comments, S27-572 to S27-655 except S27-616 and S27-653) was not re-read here; those statements rely on [linear.md](linear.md). S27-655 was read through its branch brief, whose authorship (coordinator versus Daniel) is unverified beyond the attributed name correction.
- The 2026-10-12/13 dictated feedback was seen only as a search highlight and through commit `bcea882`, not in full.
- `news.richview.ai/internal` and `/_editor/` redirect to Cloudflare Access login. Member mode, the editorial desk, roles and urgent hide were not inspected.
- The mobile flagship report capture covers only 2 of 8 segments; the brutalist flagship has no mobile capture; per-style flagship captures are viewport-only.
- Motion was judged from static frames. The reel was captured at 2.5 s, 5 s and 9 s, with no video. Daniel's "jumpy" mobile scrolling was not reproduced.
- The signup form was not submitted, so D1 storage is unverified (only GET `/api/interest` → 405). Privacy was read for product claims only; Privacy and Impressum were not captured, to keep personal details out.
- Cloudflare deployment records were not read. The live-to-branch mapping comes from HTML-to-template comparison and `/_status`.
- Daily pages were read only for RichView passages. Notion search is semantic, so a RichView decision recorded in an unrelated page may have been missed.
- The 2023 v1 material (Notion v1 page, OneNote archive) was used only for product shape; customer interviews and pitch material were not read in detail.
- "Mlouva/Mluva" is taken as Daniel's own product from his project list; the specific polish qualities he meant were not verified.
