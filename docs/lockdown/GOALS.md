# Goals

Status: regenerated on 2026-10-27 from Daniel's answers in the lockdown app (77 of 769 cards answered, 70 of the 134 key cards among them) and the current proposals in `parts/`.

What RichView is for, whom it serves, what it maximizes, what it refuses and how the rewrite is judged.

How to read this file:

- A plain card ID marks a decision: Daniel answered that card, and it binds. An `edit` answer is restated in his words; an `accept` answer binds the proposal as it stood. Where a later answer changes an earlier one, the later one wins and both IDs are named; [RULES.md](RULES.md#authority) lists the known cases.
- **Open** marks a proposal, not a decision. It names its card and who decides it.
- **Open (agents):** a technical or implementation detail. The migration agent may decide it, consistent with every decision, and records its choice in the PR.
- **Open (Daniel):** a value, feature, design or business choice. Agents may build the proposal on `dev` as a declared feature or design change, and it reaches `prod` only after Daniel accepts it in the before-and-after report (DEL-06, DEL-69). Website changes merge into `dev` like any other change and show on the hidden preview; only going public waits for his go (DEL-06, amended 2026-10-27).
- Outward actions wait for Daniel's explicit go, even when their card is accepted. [DELIVERY.md](DELIVERY.md#outward-actions-waiting-for-daniels-go) lists them in one place.
- By default, Daniel decides open cards in parts 01, 04a to 04d and 06 and the non-technical cards of part 08; agents decide open cards in parts 02, 03, 05 and 07 and part 08's technical cross-part cards. A card that spends money, publishes something or changes what Daniel reviews stays his. The 70 cards raised to key on 2026-10-26 are next for Daniel, in the order of [review-queue.md](review-queue.md).
- `node tools/lockdown-app/cli.mjs show <ID>` prints any card in full.

## Definition

- RichView is a presentation layer for your agent (INT-01). Daniel's reason: the agent is the aim, and every tool around RichView is a way to consume and publish what the agent makes, so people present things in one unified manner and can manage them.
- The tagline is "Let your agent present." with the line "Designed, reproducible, versioned." beneath it (INT-02). This answer of 2026-10-26 replaces Daniel's 2026-10-23 rejection of the earlier proposal.
- The three words are product properties, and each must stay true (INT-02):
  - **Designed:** every output comes from a designed style.
  - **Reproducible:** any report can be rendered again from its source.
  - **Versioned:** every published version is kept, so a report can be managed over the long term.
- The page lede, `og:title`, share images, posts and the reel use both lines verbatim. Both earlier taglines are retired, the share image's with its BETA label. The hero has no eyebrow above the H1: "Agentic data stories" is dropped. (INT-02; the reel follows it, OPN-64)
- RichView makes three value claims (INT-03):
  - **Keep design consistent:** typography, chart conventions and styles are reusable decisions, made once and applied to every analysis.
  - **Make the handoff inspectable:** words, chart definitions and data are plain text, and the rendered page travels with its source.
  - **Stay in the workflow you know:** write with your agent, revise in your editor and version the report beside its evidence in Git.
- The renderer owns charts, layout and typography so the agent can concentrate on the analysis (INT-03). Today's one-off agent pages look good, but colleagues cannot re-edit them, store them in a meaningful way or run them through company CI/CD. (INT-03 note)
- The artifact is self-contained by default and linked on a server (INT-04). A report renders to one HTML file that can be shared and works offline (INT-27). On a RichView server, pages link the runtime and fonts instead, so served pages are smaller. Both output modes are built and tested.
- **Open (Daniel):** INT-05 proposes the names RichView, Rich Markdown, `.rmd` and `.rstyle.yaml`, with RichView Cowork and RichView Publish as the two servers and RichView News as one Publish instance; the collisions with TRichView and R Markdown's `.Rmd` are accepted.
- **Open (Daniel):** INT-76 proposes what RichView is chosen over: screenshot-and-eyeball checks, raw chart libraries, chat artifacts, and BI tools or notebooks. Public pages name categories, not products.

## Audience

- RichView is built first for analysts, above all data analysts in every industry, who publish internal or external analysis and do that work with a coding agent such as Claude Code, Codex or Cursor (INT-06).
- The agent writes and revises the source; the analyst directs, reviews the rendered story and owns its claims. Every authoring capability works for an agent without a GUI. Most of RichView's communication targets analysts. (INT-06)
- Writers in general can use it too: analysts and journalists, someone hosting their own small RichView server as a personal site or blog, and later a newsroom (INT-06).
- Roles and approvals belong to RichView's servers, never to the core (INT-06). FEA-SF-19 (later) sets their level: full, enterprise-grade roles and access control on both servers, replacing the smaller role set INT-06 and INT-35 named.
- RichView is not built for zero-code business users (INT-06).
- Readers open a story on a phone, a desktop or as a PDF with nothing to install, and with no account unless its author shared it only with signed-in readers, such as their organization on RichView Cowork (INT-07, OPN-54, INT-83). Every story reads completely from 320 px wide upward, works with a keyboard, gives charts and controls accessible names, honours reduced motion and keeps text and tables legible with scripts disabled. Charts render with WebGL (INT-31) and must work in recent Safari, Firefox and Chrome; where JavaScript or WebGL is unavailable, each chart's frame keeps its title, caption and source note and shows a notice that the chart cannot be displayed, with no snapshot, image or table in its place (ARC-05). Readers can reach each chart's values, at the precision the author chose (FEA-CH-37), and its sources. (INT-07)
- Every story with an address also unfurls as a rich link preview wherever its link is shared, in WhatsApp, Slack, iMessage, email or a social post: a designed thumbnail of the story, in its own style, with its title, like the card a shared song gets (INT-07; FEA-SF-55, FEA-RP-51). A story shared only with signed-in readers shows its title and image only to them, so a link preview never reveals a private story. People who meet RichView through shared stories or posts are readers first. (INT-07)
- **Open (Daniel):** ARC-69 proposes that the server core renders each version's thumbnail natively from the engine's geometry model, with no browser or GPU, stores it with the immutable version and serves it under the story's own access rule; STY-70 proposes how the thumbnail looks in each of the five styles and how it joins the style review sheet (STY-68).

## Jobs, journey and flagship

INT-08 is decided; the other three cards are open, and Daniel decides them.

- RichView does five jobs (INT-08):
  - **Write:** the agent turns finished analysis into Rich Markdown with chart-local data, guided by `richview learn` and the skill.
  - **Check:** validation and chart diagnostics tell the agent, in machine-readable form, what is broken or unreadable.
  - **Present:** one source renders into a typeset, interactive story in any accepted style.
  - **Revise:** the human reviews the rendered story beside its source in the editor or browser, and the agent fixes it.
  - **Share:** the story leaves as a self-contained file, a PDF or a stable versioned URL.
- RichView is not a BI tool: it doesn't run the analysis, and the core never queries data. The one exception is on a RichView server. There, a report can link datasets through the server's connectors (OPN-63), so the server loads the data for it. (INT-08)
- **Open (Daniel):** INT-09 proposes the reference journey: an agent runs `richview learn`, writes a weekly sales report with three charts and a table, fixes what `richview_check` reports, the analyst previews in VS Code and exports a PDF, the agent publishes version 0.1.0 to the team's server, and a colleague reads it on a phone; shared in a chat, the link unfurls as a card with the story's thumbnail (INT-07). Everything except publishing and the link preview works offline.
- **Open (Daniel):** INT-10 proposes keeping the OpenAI-versus-Anthropic investigation as the one flagship, refreshed to an 18 November 2026 snapshot with data to October 2026 (9 sections, 9 charts, a dated source ledger) and re-rendered in every style; a refresh changes a number only with a new dated source. It also shows how to use the storytelling tools well (INT-77).
- **Open (Daniel):** INT-75 proposes replacing the flagship's broken header chart with a header infographic drawn from the report's own evidence, following the active style.

## What RichView maximizes

- When targets conflict, visual and motion craft wins (INT-17, INT-12): if a report looks bad, RichView has failed, while a report that is a bit slower or a file that is larger is something RichView survives (INT-17).
- People come to RichView for how their analysis looks: perfect text rendering, beautiful charts and images, rich animation. The old "80% content, 20% style" framing does not describe why people choose RichView. (INT-17)
- RichView maximizes visual and motion craft until its charts withstand side-by-side comparison with Highcharts and ECharts, and its typography and layout with FT-grade and Wall Street Journal editorial design, on desktop, phone and print. Those are the bar and Daniel's inspirations; they are quality benchmarks, never code or styling to copy. (INT-12)
- Animation is delightful and smooth: the whimsical quality of Studio Ghibli meets the polish of Apple's website animations, and that is what RichView strives for. Motion that explains structure or change and gives pleasure belongs to the craft (floor INT-61, controls INT-59). Daniel's own polish bar is Mluva. Restraint is part of craft: the right number of lines, marks and colours. (INT-12)
- A review agent on the model Daniel uses at the time judges craft on each design change, and Daniel sees every change in the before-and-after report before `prod` (INT-12, INT-67, DEL-68, DEL-69). INT-67 to INT-70 are the bars for done. (INT-12)
- RichView does not maximize storytelling, and it doesn't claim to. The story is the author's: the most beautiful pen does not write a great novel. RichView gives authors and their agents the right tools to argue clearly: headings, charts with their interpretation, annotations, callouts, emphasis, header imagery and the scrollytelling element (FEA-RP-48). `richview learn`, the skill and the flagship (INT-10) show how to use those tools well. (INT-77)
- Beyond syntax-level analysis (structure, missing sources, unlabelled charts), RichView makes no judgment of a story's quality, and nothing promises that a RichView report argues well. The LinkedIn carousel is dropped (FEA-RP-46). (INT-77)
- RichView does not maximize how little an author or agent writes; saving tokens is not a goal. It maximizes trust in the output: the same source renders consistently and to a high standard every time, so an author can rely on it without checking every page. (INT-13)
- A chart still needs its type, data, encodings, a title that states its claim and a source note under it; `richview check` warns when the note is missing (TYP-12, TYP-13, FEA-RP-09). Good defaults and styles carry quality, so agents don't have to tune every chart. They may write as much as they need, and extra source never costs quality. Errors name the line, the problem and the fix. An agent that has read `richview learn` writes a correct first draft without outside examples. Field-level rules belong to part 05. (INT-13)
- The order Daniel answered is (INT-17): visual and motion craft (INT-12); storytelling (INT-77); a short authoring path (INT-13); extensibility (INT-78); low operating cost (INT-15); small footprint (INT-14); speed beyond the budgets (INT-16). INT-77 and INT-13 (2026-10-27, later) restate its second and third slots, and the later answers win: storytelling is not maximized, RichView gives the tools for it (INT-77), and the authoring-path slot is trust in the output, not brevity (INT-13). Read together, craft comes first, then trust in the output, then extensibility, operating cost, footprint and speed.
- Footprint, file size and speed yield to craft. A size or speed budget may be raised when craft needs it, and the change description records the trade. (INT-17)
- Three guarantees sit outside the order (INT-17): RichView is feature-complete for what it accepts and always works; traceability and editability hold (INT-11); safety is never traded (INT-79).
- Truthfulness of the evidence is not something RichView maximizes (INT-11). What RichView guarantees is that a story stays traceable and editable: every chart and figure can show where its data came from, and the author can inspect and change every presentation choice. Choices that trade literal precision for readability, such as smoothed curves on a line chart, are the author's to make.
- Readers never get more precision than the author chose to show (FEA-CH-37). Numbers follow a three-digit rule by default, at most four with a scale word, such as 1.23 million, 167.5 thousand or 167 thousand. Readouts, tooltips, the Table view, CSV and every export show the author's precision; the more exact source values cannot be retrieved. The author can set more or fewer digits per chart or element, and tooltips can be more precise than labels. There is no warning at publish time.
- Safety is a floor that is never traded (INT-79, accepted as proposed). No `.rmd`, data or style file can make RichView emit executable HTML, CSS or script. Embeds run sandboxed. Outputs never carry credentials, tokens or files from outside the report folder. Private uploads stay private, and public delivery is an explicit choice. Every server endpoint that renders, publishes or regenerates requires a token and is bounded, except the loopback preview. A hostile-input fixture proves this before anything ships.
- Data-source credentials stay in the server's configuration and never appear in `.rmd` files, report folders, rendered output, MCP arguments or agent context (OPN-63). INT-79 said the Publish host holds them; OPN-63 (later) puts connectors on every RichView server, so whichever server runs the connector holds them.

The four targets below are open; Daniel decides each. The proposals already follow INT-17's order and the later INT-12, INT-13 and INT-77 answers.

| Target | Proposal | Card |
| --- | --- | --- |
| Extensibility | A new chart type, block or builtin style touches one shared implementation used by native and browser hosts, with a worked example in the guide. No plugin API in the rewrite. | INT-78, open (Daniel) |
| Low operating cost | Static hosting where possible, a written cost estimate for each hosted service before it serves anyone, portable hosting, and Cowork self-hostable as one container with local disk. | INT-15, open (Daniel) |
| Small footprint | Growth is explained by an accepted feature or by craft; removing code no accepted item needs is progress; measured against `b4db233`, with no line-count target. | INT-14, open (Daniel) |
| Speed | A floor: smooth on a mid-range phone, rendering and preview feel immediate, every part 07 budget holds unless craft raises it. INT-12 sets the aim above the floor: delightful as well as smooth. No code for benchmark wins beyond the budgets. | INT-16, open (Daniel) |

## Bets beyond existing tools

INT-21 is decided; the other five cards are open, and Daniel decides them.

- Agent-native authoring is a headline bet, at 100% (INT-21). RichView is agent-native: an agent learns, writes, renders, checks and publishes RichView on its own. It learns the format from `richview learn` or the `richview_learn` MCP tool, works through MCP or CLI JSON output, and every error names the line and the fix. The learn bundle, skill and MCP tool descriptions are generated from, or tested against, the same code as the renderer (INT-66), so today's drift (six chart types and three styles in `llms.txt` and the skill, against nine and five in the code) is fixed and can't come back.
- RichView invests heavily in how agents discover, learn and use it. Measuring how well agents use RichView is a feature: fresh-agent runs on real tasks, scored and tracked over time, show where agents stumble, and RichView is changed so agents understand it as easily as possible. A fresh agent with only these surfaces produces a clean report (INT-69). (INT-21)
- **Open (Daniel):** FEA-SF-65 proposes that measurement: a harness runs fresh agents on real tasks outside every checkout, scores each run, and keeps the scores per agent product and RichView build, so a change to `richview learn`, the skill, an MCP description, a diagnostic or a default shows in the history. The score judges nothing about whether a story argues well (INT-77) and rewards neither short nor long source (INT-13). DEL-78 sets the gate-side harness and schedule, and INT-69 is the bar.
- **Open (Daniel):** INT-18 proposes self-checking charts: every chart reports, machine-readably, what it could not show well, and RichView never reports clean for a chart it cannot bind or read. Checks measure the engine's geometry model, which the WebGL renderer and the PDF's vector renderer both draw, and stay syntax- and geometry-level (INT-77).
- **Open (Daniel):** INT-19 proposes one sourced report in many coherent styles, with identical data, wording and sources, and each style transforming the whole story.
- **Open (Daniel):** INT-20 proposes reference-grade charts from the defaults, at INT-12's craft bar and with no per-chart tuning; brevity is not the goal (INT-13). It covers every type FEA-CH-01 brings in, the map and the flow element included.
- **Open (Daniel):** INT-22 proposes Git-native editorial publishing as Publish's loop (FEA-BR-26).
- **Open (Daniel):** INT-23 proposes that public pages claim only bets demonstrated on richview.ai with real output, agent-native authoring only once scored fresh-agent runs show it (FEA-SF-65); "competition-winning" and "beyond anything made" stay internal review bars.

## Non-goals

- RichView is a tool the author's agent calls (INT-25). People bring their own agent. RichView has no chat interface, makes no LLM or speech-to-text calls, stores no model keys (no BYOK), runs no agent loop and does not generate slides or decks from prompts. The author's own agent does all language work.
- The main way to work is local: the author's agent uses RichView through its CLI and MCP server. A RichView server may at most expose MCP to an agent; it never adds a chat interface. (INT-25)
- RichView has no WYSIWYG or full browser editor, and never will: people should see and touch what is underneath, the Rich Markdown source (INT-26). Real authoring happens in the author's editor (VS Code with live preview, or possibly a RichView desktop app later) and through their agent, with Git as the history.
- The browser playground stays: a source pane with live preview for trying the format and styles, with loading, copying and downloading, and no accounts (INT-26).
- On a RichView server the browser offers only two actions (INT-26): approving what gets published (open the pending change and click yes or no), and a quick source edit by the page's signed-in author in an embedded VS Code-style editor with syntax highlighting, where the server checks that the edited report still validates and renders before it is saved.
- Authors customize only through declared, validated vocabulary: block YAML, frontmatter and `.rstyle.yaml` keys (INT-28). No report or style file can inject raw CSS, JavaScript or HTML, and charts accept no engine-level specs or raw SVG. A hostile-input test on every style key and sink proves it. When authors need more, RichView first adds a validated option or a better default.
- Later, RichView may allow custom TypeScript components embedded in a report, with their own spec validation and agent skills that steer agents to the defined vocabulary first. This is not part of the rewrite. (INT-28)
- RichView is not built for zero-code business users (INT-06).
- RichView is not a BI tool: it doesn't run the analysis, and the core never queries data (INT-08). A RichView server can load linked datasets through its connectors (OPN-63).
- RichView passes no judgment on a story's quality beyond syntax-level analysis, and nothing promises that a report argues well (INT-77).
- The voice-to-dashboard demo (PR #64) does not enter RichView. It lives outside this repository, possibly in a repository of its own. (INT-34)
- The LinkedIn carousel output is dropped (INT-31, INT-77, FEA-RP-46).
- **Open (Daniel):** INT-33 proposes no dashboards in the rewrite: no dashboard grids, cross-filtering, refresh schedules or saved views; reconsidered after launch as a report layout.
- **Open (Daniel):** INT-32 proposes that the chart engine stays internal: no standalone library, npm package or public JSON chart API during the rewrite.
- **Open (Daniel):** INT-80 proposes no tracking of readers or authors: no tracking cookies, analytics events or third-party scripts, and no usage data from the CLI, MCP server or editor.
- **Open (Daniel):** INT-29 proposes that checks never certify the analysis; beyond syntax-level analysis they make no judgment of a story's quality (INT-77), and the author owns every claim.

## Data

- RichView presents analysis and does not run it; the core never queries data (INT-08, INT-24). By default it reads data the agent or the author's pipeline has exported into the report folder. Exported data stays the default for every report.
- Live data connectors are standard on every RichView server, Cowork and Publish, whether local or hosted (OPN-63). On a RichView server a report can link datasets through the server's connectors, so the server loads the data for it (INT-08). This widens INT-24 and INT-31, which placed live connections in Publish only. RichView builds the connector core, and PR #66's work enters it. Connectors come from an existing third-party engine, so RichView does not build each connector from scratch (INT-24).
- Each server offers a set of connectors to set up. Named people manage them, and specific people are granted access to each one, under the governance roles (OPN-63, FEA-SF-19).
- A report can use SQL against a connector where today it would link to a CSV or another file (OPN-63).
- Publish's publishing setup is a light workflow, not a full CMS (OPN-63, OPN-54).
- A self-contained HTML file never queries live data. The one exception is a data source pointing to a URL: the file downloads the data from that address, so it updates when the target changes, with no warning, because the reader already trusts the sender. (OPN-63)
- Credentials stay in the server's configuration (OPN-63, ARC-23).
- The agents decide the technical details, such as which engines back the connector core and whether DuckDB or MetricFlow adapters are part of it (OPN-63).

## Scope of the rewrite

- The rewrite builds RichView core and its servers (INT-31):
  - the Rich Markdown format and style cascade, the chart engine and the report renderer;
  - the CLI with MCP and `learn`, the VS Code extension, the browser playground and preview, and PDF export;
  - RichView Cowork, the self-hostable internal server (INT-83), and RichView Publish (INT-35), which share their server core; RichView News is one instance of Publish, not a separate product;
  - richview.ai with its docs and the flagship.
- Also in scope (INT-31): scrollytelling, as one element inside a broader report (FEA-RP-48); author-hosted assets and uploads (FEA-SF-50, TYP-41); and RichView-managed hosting for large data and media, run as a service.
- Charts render with WebGL by default; RichView goes all in on it. Only PDF export converts charts to vector output, and that export may take time. (INT-31, ARC-05)
- **Open (Daniel):** FEA-RP-43 proposes keeping PDF export with one pagination bar: no split chart, figure, metric, callout or table row, no heading last on a page and no gap over a quarter page. When the next chart or figure does not fit, the text after it flows up to fill the page and the figure floats to the top of the next page, as LaTeX floats figures; a longer gap is accepted only where the next block opens a new section and does not fit (Daniel's choices of 2026-10-27). The fixtures run in the release run on `pre-prod` and on each prod candidate (DEL-81, DEL-71), not on every `dev` merge.
- All major browsers must work: Safari, Firefox and Chrome in reasonably recent versions. Where JavaScript or WebGL is unavailable, the chart's frame shows a notice that the chart cannot be displayed. No static snapshot, image or table fallback is built. (ARC-05)
- RichView aims to be the richest chart library: at least every chart type that ggplot2, Plotly, ECharts, Highcharts and Power BI define, likely around 30 types; the exact list is compiled from those libraries (FEA-CH-01).
- Beyond charts, RichView adds a map element, a flow element (flowcharts modelled on Nova flow, done more meaningfully and fully ported to Rust) and much better tables with the best styling, both normal tables and pivot tables (FEA-CH-01).
- Authored chart controls enter with all four kinds: playback, filter, metric and variant (FEA-BR-01). Multi-tab reports enter (FEA-BR-08).
- PRs #62, #63, #65, #66, #67, #68 and #69 all enter the rewrite: their features are rebuilt on the new core in the form their own cards decide (FEA-BR-08 note; OPN-13).
- Unsplash photos with attribution enter as PR #102 builds them: a `credit` on image blocks and `title_image`, `richview image search` and `use` with matching MCP tools, photos hotlinked from `images.unsplash.com`, and the key only in `UNSPLASH_ACCESS_KEY` (FEA-RP-62, accepted). Hotlinked photos show only online; what an offline file shows for them is open for Daniel.
- RichView Publish is a full member of the family and part of the rewrite (INT-35). PR #70 is merged into `main` (2026-10-26), and its work continues from there.
- Publish and Cowork share one RichView server core: an internal component, not a product of its own (INT-35). Publish carries approvals and a live-connector path (INT-35; OPN-63 later gives every server connectors). Its brief and Git-native loop (INT-22, FEA-BR-26) remain the spec it is rebuilt against.
- Today's report host becomes Cowork, as INT-83 proposed (INT-83). A RichView server comes in exactly two forms, Cowork or Publish; there is no third, stripped-down server. Cowork focuses on people publishing reports inside their own organisation.
- Rebuild, porting proven modules (INT-49). Existing code and tests are evidence, not a base: a module is ported only when it already meets every accepted item it touches and its tests prove that; otherwise it is rewritten. Nothing survives merely because it exists.
- Existing tests and fixtures for accepted behaviour become acceptance tests; tests for rejected behaviour are deleted with it (INT-49).
- RichView is not a production application yet, so no test exists only to prove that old behaviour survived. Any feature that is not decided and tracked in the lockdown is dropped, even when today's code has it. (INT-49)
- The rewrite happens in the same repository, with its history kept (INT-50): "make sure that we have all the code versions".
- Wherever possible RichView makes the performant choice (INT-52). The language itself does not matter, but Rust is the obvious way to be both stable and very fast, so RichView stays Rust: one shared core runs natively (CLI, server, MCP) and as WebAssembly in browsers and VS Code, and WebAssembly makes charts fast in the browser.
- Handwritten JavaScript exists only where part 03 allows it (ARC-03), and no product rule exists only in JavaScript in the rewrite's target (INT-52).
- RichView Publish and News (PR #70) are merged as they are, JavaScript included, so the News work lands and continues. The rewrite carries over what Publish strives for, its behaviour and decisions, not its code; Publish's JavaScript is ported to Rust when the rewrite reaches Publish. (INT-52)
- There are always five report styles: Sleek (the dark style, id `sleek` today), Air (today's `sleek-light`, renamed: it must not be called "Sleek"), Scientific, Brutalist and Hand-drawn (STY-05, STY-01).
- Sleek is the default everywhere: a report without `styles` renders in it, `init` writes `styles: [sleek]`, and the playground, landing demo and flagship page open in it. Print and PDF use its light counterpart, Air. (STY-06)
- Each style gets its own design review and must feel very distinctive beyond its fonts and colours: styles differ in structure too, for example outlines and shading or none, where the legend sits and where chart titles sit. Scientific puts chart descriptions below the chart, sometimes with boxes around them, as scientific papers do. (STY-01)
- The brand layer is abstract: only a mark (the RichView wordmark) and a colour, brand blue, in a distinct shade per style. RichView's design is driven by the LCH colour space and a set of cleanliness principles. The website and RichView's own presentations use one simple style. The earlier warm-sand and indigo house identity is retired. (STY-01)
- The showcase's studio themes are kept as extended themes: a real feature that shows how many variants RichView can produce, but not promoted much and absent from the normal style menus, at most in a hidden or secondary style dropdown. The showcase keeps its side-by-side view of different styles and layers. (STY-05)
- **Open (Daniel):** INT-51 proposes that existing sources keep working by migration in the same merge as each format change, with no compatibility layer.
- **Open (Daniel):** INT-53 proposes that RichView keeps its own Rust chart engine and that no third-party chart library renders its charts.
- **Open (Daniel):** INT-54 proposes that the five builtin identities carry over under STY-05's names, and the studio variants as extended themes.
- **Open (Daniel):** INT-55 proposes which corpora carry over as golden inputs and fixtures: the flagship, the named examples, the home-page demo, the chart-review report, the 56 before/after cases and the benchmark reports.
- **Open (Daniel):** INT-81 proposes that agent-facing contracts change only through accepted items until launch, and only additively after it.

## Servers, hosting and business

- RichView works first as a single-user tool people download and use themselves, like Microsoft Word: local tools (CLI, MCP, VS Code extension) that create one HTML file, which can be shared and works offline (INT-27, INT-04).
- On top of that, like SharePoint on top of Word, RichView's servers (Cowork and Publish) add sharing, roles and approvals (INT-27). Everything local works without them.
- Stories are shared as the self-contained HTML file, as a PDF, and through RichView's servers (INT-36). Anyone can self-host Cowork or Publish.
- RichView runs hosted Cowork itself, typically at app.richview.ai (INT-36, INT-46 note). People share reports there, including with outside parties. It is free up to limits and paid beyond them; the limits cover how many reports and files an account has and how large they are. Hosted RichView may later have features of its own.
- RichView also sets up and runs a Publish server for a customer under the customer's own DNS records, so it lives on their website (INT-36). Nothing runs until a customer asks (ARC-22).
- RichView also runs hosting for large data and media as a paid service (INT-27, INT-31).
- These paid services are how RichView makes money (INT-36). Pricing, limits and which features are hosted-only are open, and Daniel decides them.
- Both servers have full, enterprise-grade roles and access control: a governance section where an admin manages users, roles and who can see, edit, delete and share each report (FEA-SF-19). They get it from the server core they share. The protection is the same wherever the server runs: hosted by RichView, in a customer's container, or on the customer's own server.
- The CLI, MCP server and VS Code extension stay free of accounts. The agents decide the technical details, such as the identity source, single sign-on and audit logging, to meet the enterprise bar. (FEA-SF-19)
- Reads follow sharing (OPN-54). On Cowork everything is private by default, and the author shares a report with their organization or with anyone on the web. On Publish, such as RichView News, a publishing workflow can require an editor's approval: an editor's own publish lands as private until the editor makes it public, and when an editor approves an author's submission it becomes public automatically. Report lists show only what is shared with the viewer.
- Over time Cowork may also store data-source credentials, such as SQL credentials, so its reports can read live data; not in its first cut (INT-83). OPN-63 (later) makes connectors standard on every server, Cowork included, without saying when; whether Cowork's first cut ships them is open for Daniel.
- **Open (Daniel):** INT-30 proposes that RichView's software carries no pricing, plans or paid tiers, and that money comes only from the hosted services; whether accounts and billing are built before launch is open.
- **Open (Daniel):** INT-48 proposes the hosting providers: richview.ai stays on Cloudflare; the providers for hosted Cowork, customer Publish servers and data and media hosting are open.
- **Open (Daniel):** INT-73 proposes that RichView presents itself as Daniel's personal project; who sells and operates the paid services is open.

## Public presence

- The docs and the website are always public (INT-38).
- The source repository stays private until launch (INT-38). When Daniel decides the work is done, he tells an agent to publish it all, and the source goes public then, as its own explicit step. Before it, the full history is checked for secrets and private data, knowing that the history up to 2026-06-10 was public and may already have been copied, and bundled fonts and assets are checked against their licences.
- MIT everywhere, matching `LICENSE`, every crate, the impressum and News's About page; anyone may host RichView commercially (INT-37). Daniel: "Let's keep MIT. MIT for everything."
- Launch is simple: it happens when Daniel says so, and everything goes live at once (INT-43):
  - richview.ai drops "Coming soon";
  - the try path: the playground, INT-71's install prompt and script, builds from `prod`, the extension on the VS Code Marketplace and Open VSX, and the source build;
  - the flagship and the docs;
  - the public source (INT-38).
- No date is set, and nothing else gates launch. Agents keep everything ready so his one word makes it all live. The first posts go out only after Daniel has reviewed them. (INT-43)
- All three install paths (INT-71). The install page leads with one prompt people give their agent, plus a shell script they can run; both install the CLI with its MCP server, and RichView's servers where wanted. Building from source works once the source is public. Builds from `prod` are offered. The VS Code extension is published on the VS Code Marketplace and on Open VSX. Working as agent plugins may come later, not now.
- INT-71 changes INT-41's "no releases" for these builds and listings (INT-71). INT-41 is still open on how they are versioned and published (DEL-41).
- richview.ai is RichView's home and only canonical public origin: the site, docs, playground, flagship and every "Made with RichView" link point there, and the host is defined once. No other domain appears in product defaults, docs or generated reports, and the CLI and VS Code have no default server. app.richview.ai is the hosted Cowork. (INT-46, accepted, with Daniel's note)
- INT-46 also said the `richview-teaser` workers.dev origin is switched off or redirected. ARC-22 (later) keeps that address, hidden behind Cloudflare Access, as the `dev` preview of richview.ai; ARC-22 wins. INT-46 said news.richview.ai exists only while News is kept; ARC-22 (later) keeps it published as RichView's own Publish server.
- The home page promises what the product will do, and the live page gets no hotfixes: it was never advertised, so it changes when the rewrite reaches `prod` (FEA-SF-30). The report menu with share, copy and protect, and the "private" badge, are built, with protection from the enterprise roles; the VS Code snippets line is worded to match what ships; "One shortcut to publish" becomes one-click publish, including right-click publish in the editor; the line "The coming release includes the CLI, MCP tools, report server and extension" stays, because they all ship; the open-source sentence becomes "RichView is open source and MIT licensed."; immutable versions are kept and made true; style names come from core; release updates and their consent stay; the flagship verdict quote goes; the Hacker News dots caption shows only while the page renders the dots.
- **Open (Daniel):** INT-39 proposes the open-source wording: "RichView is open source and MIT licensed." from launch, and "RichView will be open source under the MIT licence." before it.
- **Open (Daniel):** INT-42 proposes "Coming soon" as the only public status word until launch, with no "beta", "alpha", "preview" or version numbers.
- **Open (Daniel):** INT-44 proposes richview.ai's role: proof of output quality and the way in, with a source beside its rendered story in one screen.
- **Open (Daniel):** INT-45 proposes that marketing links the flagship but never restates its findings.
- **Open (Daniel):** INT-72 proposes that the update list stays private to Daniel and that nothing is mailed to it without his approval of that mailing.
- **Open (Daniel):** INT-40 and INT-47 propose the end state of the legacy repositories and hosts; the accepted OPN-25, OPN-65, OPN-04, OPN-05 and OPN-45 already decide the steps, which wait for Daniel's go ([DELIVERY.md](DELIVERY.md#outward-actions-waiting-for-daniels-go)).

## Success criteria

- The rewrite is done when every accepted product item (feature, component, type, style, rule and budget) is implemented on `prod` with its evidence, every rejected product item is absent, deferred items are not built, and nothing outside the lockdown was added (INT-67).
- One-off outward actions (archiving, redirects, shutdowns, deleting the pre-release) are tracked in part 08, and Daniel's later calls (source visibility, launch) follow their own cards; neither gates done. Parity with the old code is not a goal beyond accepted items. (INT-67)
- An agent can judge design quality (INT-67, DEL-68). A review agent on Daniel's current model judges visual and motion quality against INT-12's bar from rendered output on a phone and a desktop, and the result reaches Daniel in the before-and-after report (DEL-69) before `prod`, where he sees every change and can adjust visual and motion quality independently. Passing tests alone never establish design quality. INT-67's proposal had said only Daniel's own review decides design quality; his DEL-68 and INT-67 answers replace that.
- **Open (Daniel):** INT-68 proposes the flagship bar: on `prod`, in every style at 320, 390 and 1440 px and as a PDF, with identical data, wording and sources, no error or warning diagnostics, no clipped text and smooth motion at INT-12's standard; a review agent compares it with today's flagship, and Daniel accepts it in the before-and-after report as clearly better.
- **Open (Daniel):** INT-69 proposes the fresh-agent bar: an agent that has never seen RichView, given only `richview learn` or MCP and a dataset, produces a sourced report with prose, at least three chart types and a table that passes `richview check` by the second attempt, tested with at least two agent products. Daniel's INT-21 answer makes this a measurement: the runs use real tasks and are scored and tracked over time (FEA-SF-65, DEL-78), and this bar is the pass line.
- **Open (Daniel):** INT-70 proposes measuring the rewrite against `b4db233`: files, lines, dependencies, Wasm and report size, part 07 timings and check findings.
- **Open (Daniel):** INT-82 proposes one signal after launch: agents reach a clean check unattended and a person keeps the report. Installs, stars and page views are not targets, and no telemetry measures them; before launch, scored fresh-agent runs measure the agent half (FEA-SF-65).
