# Boundaries

Status: regenerated on 2026-10-27 from Daniel's answers in the lockdown app (77 of 769 cards answered, 70 of the 134 key cards among them) and the current proposals in `parts/`. DEL-81's model amendment of 2026-10-29 and STY-73 of 2026-11-18 are applied by hand.

Where RichView's parts end, who owns what, what a source file can never do, which changes an agent makes alone and which reach Daniel.

How to read this file:

- A plain card ID marks a decision: Daniel answered that card, and it binds. An `edit` answer is restated in his words; an `accept` answer binds the proposal as it stood. Where a later answer changes an earlier one, the later one wins and both IDs are named; [RULES.md](RULES.md#authority) lists the known cases.
- **Open** marks a proposal, not a decision. It names its card and who decides it.
- **Open (agents):** a technical or implementation detail. The migration agent may decide it, consistent with every decision, and records its choice in the PR.
- **Open (Daniel):** a value, feature, design or business choice. Agents may build the proposal on `dev` as a declared feature or design change, and it reaches `prod` only after Daniel accepts it in the before-and-after report (DEL-06, DEL-69). Website changes merge into `dev` like any other change and show on the hidden preview; only going public waits for his go (DEL-06, amended 2026-10-27).
- Outward actions wait for Daniel's explicit go, even when their card is accepted. [DELIVERY.md](DELIVERY.md#outward-actions-waiting-for-daniels-go) lists them in one place.
- By default, Daniel decides open cards in parts 01, 04a to 04d and 06 and the non-technical cards of part 08; agents decide open cards in parts 02, 03, 05 and 07 and part 08's technical cross-part cards. A card that spends money, publishes something or changes what Daniel reviews stays his. The 70 cards raised to key on 2026-10-26 are next for Daniel, in the order of [review-queue.md](review-queue.md).
- `node tools/lockdown-app/cli.mjs show <ID>` prints any card in full.

## Product boundaries

- RichView is local tools first, servers on top (INT-27). The CLI, MCP server and VS Code extension create one HTML file that can be shared and works offline; RichView's servers add sharing, roles and approvals. Everything local works without the servers.
- The core holds no accounts, roles or approvals; they belong to the servers (INT-06, FEA-SF-19). The CLI, MCP server and VS Code extension need no account; to publish, the command line keeps a sign-in per server that you can remove with richview logout (FEA-SF-19, amended 19 Nov).
- A RichView server comes in exactly two forms, Cowork or Publish; there is no third server (INT-83). Both share one internal server core (INT-35). Today's report host becomes Cowork (INT-83).
- RichView does five jobs: Write, Check, Present, Revise and Share (INT-08). It is not a BI tool: it does not run the analysis, and the core never queries data. The one exception is a RichView server, where a report can link datasets through the server's connectors and the server loads the data (INT-08, OPN-63).
- RichView does not maximize storytelling and makes no judgment of a story's quality beyond syntax-level analysis (structure, missing sources, unlabelled charts); it gives authors and agents the tools, and nothing promises that a report argues well (INT-77).
- RichView does not maximize how little an author or agent writes; saving tokens is not a goal. It maximizes trust in the output: the same source renders consistently and to a high standard every time (INT-13).
- RichView is agent-native: an agent learns, writes, renders, checks and publishes RichView on its own, through MCP or CLI JSON output, and the agent-facing surfaces are generated from or tested against the code, so they cannot drift (INT-21, INT-66). Measuring how well agents use RichView, with scored fresh-agent runs tracked over time, is a feature (INT-21); FEA-SF-65 is open for Daniel.
- RichView has no chat interface and runs no agent; a server may at most expose MCP (INT-25).
- The browser never gets a full editor. On a server it offers approval and the signed-in author's quick source edit, which the server validates and renders before saving (INT-26).
- The voice-to-dashboard demo stays outside this repository (INT-34).
- Live connectors live on the servers, never in the core or in an exported file (OPN-63, INT-08). A self-contained file never queries live data, except a URL data source, which the file downloads.
- Every story with an address unfurls as a rich link preview wherever its link is shared: a designed thumbnail in the story's own style, with its title (INT-07, FEA-SF-55, FEA-RP-51). A story shared only with signed-in readers shows its title and image only to them (see Safety boundaries).

## Layers and dependency direction

All of this is open, and the agents decide it within the decisions above.

- **Open (agents):** ARC-01 proposes six layers, each depending only downward: engine (`richview-chart`); core (`richview-core`: format, validation, styles, renderer, knowledge); host services (resolver, export, store, browser host service); hosts (CLI, HTTP host, MCP, editor service, report and site runtimes); adapters (VS Code extension, Cloudflare Workers, generated JavaScript glue); content and tools, which never ship.
- **Open (agents):** ARC-02 proposes that core and engine do no filesystem, network, process, environment or clock access, and that a gate fails when they gain such a dependency (DEL-54).
- **Open (agents):** ARC-03 proposes where handwritten JavaScript is allowed. INT-52 decides the principle: no product rule exists only in JavaScript in the rewrite's target, and PR #70's JavaScript stays until the rewrite reaches Publish.
- **Open (agents):** ARC-06 proposes two Wasm modules, a report runtime (the WebGL chart renderer, interaction, the settle hook) and a site runtime (playground, landing, showcase, docs helpers); reports never load site code. OPN-53 proposes one report runtime in every report.
- **Open (agents):** ARC-18 proposes that only the engine computes chart geometry, as one model that the WebGL renderer draws on the page, the vector renderer draws for PDF, and every check measures.
- **Open (Daniel):** ARC-69 proposes that the server core, which Cowork and Publish share, renders each story version's link-preview thumbnail natively from that geometry model, with no browser and no GPU, stores it with the immutable version and serves it under the story's own access rule (INT-07).
- **Open (agents):** ARC-16 and ARC-17 propose one Cargo workspace in `1vecera/richview`, with publication content in its own content repository. INT-50 decides the repository: the same one, with its history.
- **Open (agents):** ARC-67 proposes macOS arm64 and Linux x64 as the supported platforms. ARC-05 decides the browsers: Safari, Firefox and Chrome in reasonably recent versions.

## Components

- Built in the rewrite (INT-31): the format and style cascade, the chart engine, the report renderer, the CLI with MCP and `learn`, the VS Code extension, the browser playground and preview, PDF export, Cowork, Publish and their shared server core, richview.ai with its docs and the flagship, scrollytelling as a report element, author-hosted assets and uploads, and RichView-managed hosting for large data and media.
- Charts go far beyond today's nine types (FEA-CH-01): every type of the major chart libraries, a map element, a flow element and much better normal and pivot tables.
- Authored chart controls (FEA-BR-01) and tabs (FEA-BR-08) enter. The work of PRs #62, #63, #65, #66, #67, #68 and #69 enters, each in the form its own card decides (FEA-BR-08 note).
- Unsplash photos enter as PR #102 builds them (FEA-RP-62, accepted): image blocks and `title_image` take an optional `credit` that renders as "Photo by <author> on Unsplash"; `richview image search` and `richview image use`, with the MCP tools `richview_image_search` and `richview_image_use`, return paste-ready YAML; photos are hotlinked from `images.unsplash.com`; the key comes only from `UNSPLASH_ACCESS_KEY`. In RichView News, `publish.cover` may be an Unsplash URL with a required `cover_credit`. Hotlinked photos show only online, so what an offline file shows for them is open for Daniel (INT-27, INT-04).
- Dropped: the LinkedIn carousel (INT-31, INT-77) and the voice-to-dashboard demo (INT-34).
- **Open (Daniel):** FEA-SF-65 proposes the agent-use measurement as a feature (INT-21): fresh-agent runs on real tasks, scored per agent product and RichView build and tracked over time, whose findings change `richview learn`, the skill, the MCP descriptions, diagnostics and defaults. STY-70 proposes the thumbnail's look in each style, which joins each style's review sheet.
- **Open (agents):** ARC-25 to ARC-65 propose keep, change or drop for each component; keep means the responsibility stays, and whether its code is ported follows INT-49. The current proposals keep and change the engine, core, the native host with its CLI, resolver, PDF printer, HTTP host (Cowork's first cut), report store, MCP server and asset uploader; split the browser host into report and site runtimes; keep the playground, landing, showcase and docs site, the editor service and VS Code extension, build, bench, CI, skill, `llms.txt`, examples, the site Worker, font pipeline, verification scripts, comparison labs and lockdown tooling; and change agent instructions to one `AGENTS.md` and the container image to a self-host image of Cowork.
- **Open (Daniel):** the proposed drops that touch features or public surfaces: the `/charts/` page (ARC-40), the reel as a site page (ARC-42, FEA-SF-45), the npm packages `@richview/core` and `@richview/charts` (ARC-48, ARC-49, INT-32) and standalone galleries (ARC-60).
- **Open (agents):** OPN-68 proposes removing the `richview-chart` binary, so `richview` is the only command-line tool.
- ARC-57 and ARC-59 already follow the decisions: Publish is rebuilt in Rust on the shared server core (INT-35, INT-52), and the connector core is built by RichView with PR #66's work entering it (OPN-63).

## Ownership

| Who | Owns | Card |
| --- | --- | --- |
| Daniel, alone | Lockdown and packet answers; the go for every merge into `prod`, every deploy and every outward action; launch and source visibility | OPN-48, DEL-04, DEL-05, DEL-40, INT-43, INT-38 |
| An agent, on Daniel's go | The merge into `prod`, with a single command, after the prod tests pass on the candidate (DEL-71) | DEL-04 |
| Any development agent | Merges into `dev` after the fast gate; reverting or removing changes on `dev`; deleting branches; setting up automations within DEL-05's limits; talking to other agents through Linear and local helpers such as Herdr. Models follow DEL-81 as amended on 2026-10-29: Opus at xhigh for the final reviewer, checker and adversarial reviews; Sonnet at xhigh for design; Codex `gpt-6.1-sol` at xhigh for everything else | DEL-05, DEL-06, DEL-81 |
| The release agent | Merges `dev` into `pre-prod` after a batch of up to 20 merges, runs the long suites for what the batch affects and fixes forward until everything works together | DEL-81 |
| The lockdown session | Splits the work into independent streams, briefs the agents and relays Daniel | DEL-81 |
| The deployment owner named in each deploy card | Deploying the exact tree, the deploy receipt, the live route check | DEL-40 |
| Self-hosters | Running Cowork or Publish themselves | INT-36 |
| RichView, as a service | Hosted Cowork at app.richview.ai, Publish servers for customers under their DNS, hosting for large data and media | INT-36, INT-27 |
| No one else | No collaborator is invited; `prod` stays Daniel's | OPN-24 |

- **Decided (DEL-37, as amended by DEL-81):** a committed pre-push hook is the only technical block; it refuses pushes to `prod`, `pre-prod`, `main` and `candidate/*` from agent checkouts, and DEL-81 lets agents push to `dev`. There is no separate gate user, agents have all the access, and the audit (DEL-38) catches the occasional mistake.
- **Decided (DEL-81):** the checker and the adversarial reviews run on Opus, and at most two Opus agents run at once across the build.
- **Open (agents):** DEL-63 proposes that the chief of staff closes packet batches and that the release agent takes release batches of up to 20 merges; DEL-62 proposes that development agents are not capped by count, only by the shared Cargo target's lock, its 10 GiB disk floor (Daniel, 2026-10-29; it was 40) and the host's memory; and OPN-41 that the chief of staff reconciles Linear. DEL-42 proposes one task per worktree, branch and PR.
- Named main collaborators may later give the `prod` go too; only Daniel does for now (DEL-04).

## Safety boundaries

Safety is a floor that is never traded, and a hostile-input fixture proves these boundaries before anything ships (INT-79).

- No `.rmd`, data or style file can make RichView emit executable HTML, CSS or script; embeds run sandboxed; outputs never carry credentials, tokens or files from outside the report folder (INT-79).
- Private uploads stay private, and public delivery is an explicit choice (INT-79).
- Every server endpoint that renders, publishes or regenerates requires a token and is bounded, except the loopback preview (INT-79).
- Authors customize only through declared, validated vocabulary; no raw CSS, JavaScript, HTML, engine specs or raw SVG (INT-28).
- Style files take a preset, the author's colours and five-step settings; an author file skips retired keys and reports `style.retired` at each one. Nothing changes the underlying mechanics such as phone or page width; every value passes the closed vocabulary and is measured at render (STY-73, which overrides that part of STY-14 and OPN-59).
- Data-source credentials stay in the server's configuration and never appear in `.rmd` files, report folders, rendered output, MCP arguments or agent context (OPN-63). The Unsplash key comes only from `UNSPLASH_ACCESS_KEY` and never appears in sources, output or agent context (FEA-RP-62).
- Readers never get more precision than the author chose: readouts, tooltips, the Table view, CSV and every export show the author's precision, and the exact source values cannot be retrieved (FEA-CH-37). This also keeps an author's calculation precision private.
- A link preview never reveals a private story: a story shared only with signed-in readers shows its title and thumbnail only to them, and an anonymous unfurler gets neither (INT-07, OPN-54; FEA-SF-55, ARC-69, TYP-65).
- Access follows roles and sharing (FEA-SF-19, OPN-54). Both servers have enterprise-grade roles managed in a governance section, the same wherever they run. On Cowork everything is private by default until the author shares it with the organization or the web. On Publish, an editor's own publish lands private until the editor makes it public, and an approved author submission becomes public automatically. Report lists show only what the viewer may see.
- Reports are served from a separate report origin before read protection ships (FEA-SF-19, ARC-15).
- Whether a server render API exists, and how tokens work, are technical details the agents decide (OPN-54).
- **Open (agents):** ARC-10 proposes that local hosts read report inputs only from the report folder, through the resolver, and write only `Rendered/`.
- **Open (agents):** ARC-11 proposes that no host fetches report inputs. It already allows OPN-63's URL data source, which the reader's browser downloads. TYP-40 still makes every plain remote media URL an error, which conflicts with FEA-RP-62's hotlinked Unsplash photos; FEA-RP-62 wins.
- **Open (agents):** ARC-12 and TYP-51 propose a closed grammar for every style value and one typed serializer per sink.
- **Open (agents):** ARC-13 and FEA-RP-14 propose that raw HTML never renders and that every value is escaped at its sink; the only script in a report is the report runtime.
- **Open (agents):** ARC-14 proposes HTTPS-only, allowlisted, sandboxed embeds.
- **Open (agents):** ARC-15 proposes a Content Security Policy on every RichView server and in every exported file, with `connect-src` limited to a report's URL data sources.
- **Open (agents):** ARC-23 proposes that publish and upload tokens come only from the host environment, `--token` or editor secret storage, travel only over HTTPS and are redacted everywhere.
- **Open (agents):** TYP-43 proposes size and count limits on sources, data, output and requests.
- **Open (Daniel):** INT-80 proposes that RichView never tracks readers or authors and holds no personal data beyond addresses typed into the update sign-up.
- On `b4db233` these boundaries do not hold: a style file injects a live script, `POST /api/render` and regenerate need no token, and no host sends a CSP. Legacy is not patched; each defect becomes a regression fixture of the rewrite (OPN-02, OPN-16, OPN-22).

## Change areas

| Change | Path | Card |
| --- | --- | --- |
| Fixes, refactors, accepted items implemented with no capture change | Merge into `dev` after the fast gate; the release receipt of its batch is the evidence; listed in the packet appendix and sampled by a fresh review agent | DEL-68, DEL-81 |
| A design change: a capture, motion, token, typography or copy change | Merges into `dev` after the fast gate without pinging Daniel; a review agent judges visual and motion quality against INT-12's bar; Daniel sees it one by one in the design log and the before-and-after report before `prod` | DEL-06, DEL-68, DEL-69, INT-12 |
| A feature change: scope added, changed or removed | Merges into `dev` after the fast gate without pinging Daniel; Daniel sees it one by one in the feature log before `prod` | DEL-06, DEL-68, DEL-69 |
| Something new Daniel asks for that the lockdown does not cover | The agent also proposes the change to the lockdown documents, and Daniel looks at that first | DEL-06 |
| Going public: a `prod` merge or deploy, a post, release or publication | Waits for Daniel's go; website changes in `dev` need no grant | DEL-06 |
| Deleting a test | Allowed when no longer needed: unit tests at will, end-to-end tests whose feature no longer makes sense, and tests an audit finds formal, obvious or duplicated | DEL-68 |
| Removing or skipping a gate step or an adversarial review | Never by an agent; elements inside a step may change. DEL-81 (later) lets steps, tiers, hooks, reviews and thresholds change when they slow development; how the two fit is open for Daniel | DEL-68, DEL-81 |
| A release batch | The release agent merges `dev` into `pre-prod`, runs the long suites for what the batch affects and fixes forward; adversarial reviews run per batch | DEL-81 |
| A `prod` merge | The prod tests re-run the full set that already ran on `pre-prod` on the frozen candidate cut from it; Daniel gives the go; an agent merges | DEL-71, DEL-81, DEL-04 |

- **Open (agents):** DEL-32 proposes how a PR declares `decision: none`, `design` or `feature`, and that the release run enforces the class. A PR that changes the website needs no grant (DEL-06, amended 2026-10-27).
- **Open (agents):** DEL-36 proposes the three changes that still come to Daniel as cards: raising a threshold, budget or baseline; changing the detector of removed steps; and changing `answers/` outside DEL-66. INT-17 lets a budget rise when craft needs it, with the trade recorded, and DEL-81 lets thresholds that slow development change; which still come to Daniel is open.
- **Open (agents):** DEL-64 proposes replacing each component in place, one PR series at a time, with design cards batched per component and style while `prod` still holds legacy output.

## What needs Daniel

- Every value, feature, design and business choice on an open card, before it reaches `prod` (DEL-06, DEL-69). The 70 cards in [review-queue.md](review-queue.md) are next.
- Every merge into `prod`, every deploy and every outward action, each on his explicit go (DEL-04, DEL-05, DEL-40). The full list is in [DELIVERY.md](DELIVERY.md#outward-actions-waiting-for-daniels-go).
- Launch and making the source public: his word alone (INT-43, INT-38).
