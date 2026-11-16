# Rules

Status: regenerated on 2026-10-27 from Daniel's answers in the lockdown app (77 of 769 cards answered, 70 of the 134 key cards among them) and the current proposals in `parts/`. DEL-81's model amendment of 2026-10-29 is applied by hand.

Rules every agent follows in the rewrite. Delivery mechanics are in [DELIVERY.md](DELIVERY.md), architecture and safety boundaries in [BOUNDARIES.md](BOUNDARIES.md), proof in [EVIDENCE.md](EVIDENCE.md).

How to read this file:

- A plain card ID marks a decision: Daniel answered that card, and it binds. An `edit` answer is restated in his words; an `accept` answer binds the proposal as it stood. Where a later answer changes an earlier one, the later one wins and both IDs are named; [RULES.md](RULES.md#authority) lists the known cases.
- **Open** marks a proposal, not a decision. It names its card and who decides it.
- **Open (agents):** a technical or implementation detail. The migration agent may decide it, consistent with every decision, and records its choice in the PR.
- **Open (Daniel):** a value, feature, design or business choice. Agents may build the proposal on `dev` as a declared feature or design change, and it reaches `prod` only after Daniel accepts it in the before-and-after report (DEL-06, DEL-69). Website changes merge into `dev` like any other change and show on the hidden preview; only going public waits for his go (DEL-06, amended 2026-10-27).
- Outward actions wait for Daniel's explicit go, even when their card is accepted. [DELIVERY.md](DELIVERY.md#outward-actions-waiting-for-daniels-go) lists them in one place.
- By default, Daniel decides open cards in parts 01, 04a to 04d and 06 and the non-technical cards of part 08; agents decide open cards in parts 02, 03, 05 and 07 and part 08's technical cross-part cards. A card that spends money, publishes something or changes what Daniel reviews stays his. The 70 cards raised to key on 2026-10-26 are next for Daniel, in the order of [review-queue.md](review-queue.md).
- `node tools/lockdown-app/cli.mjs show <ID>` prints any card in full.

## Authority

- Daniel's answers are the source of truth (README). The rewrite rebuilds RichView against the accepted lockdown, and any feature that is not decided and tracked in the lockdown is dropped, even when today's code has it (INT-49).
- Daniel leads RichView alone: he answers the lockdown, merges into `prod` and leads the Linear project (OPN-48). A second lead named in older notes was an agent persona, not a person. No message goes to anyone first, and nothing waits on anyone else. Every agent context that names another lead is corrected to Daniel (OPN-48).
- No collaborator is invited to the repository, and `prod` stays Daniel's (OPN-24, DEL-04).
- When Daniel asks for something new that the lockdown does not cover, the agent also proposes the change to the lockdown documents, and Daniel looks at that first (DEL-06).
- An agent records an answer only when Daniel gives it live and asks the agent to record it, through `cli.mjs answer`, never by editing `answers/` (README).
- Where a later answer changes an earlier one, the later one wins. The known cases:

| Later answer | Earlier answer or proposal | What holds now |
| --- | --- | --- |
| DEL-68, INT-67 | INT-67's proposal (only Daniel's review decides design quality) | A review agent on Daniel's current model judges visual and motion quality; Daniel sees every change in the before-and-after report (DEL-69) before `prod` and can adjust it. STY-68 follows. |
| ARC-22 | INT-46 (workers.dev origin switched off or redirected), DEL-40's table (`workers_dev = false`; a preview at a hostname such as preview.richview.ai) | richview-teaser.vecera.workers.dev is kept, hidden behind Cloudflare Access, as the `dev` preview of richview.ai, and redeploys from `dev` only when the website's code changes. OPN-44 follows. |
| ARC-22 | INT-46 (news.richview.ai exists only while News is kept) | news.richview.ai is RichView's own Publish server, public, and stays published. |
| FEA-SF-19 | INT-06, INT-35 (a small set of server roles), and FEA-SF-19's own proposal (a proxy until roles ship) | Full, enterprise-grade roles and access control in a governance section on both servers. |
| INT-02 (2026-10-26) | Daniel's 2026-10-23 rejection of the earlier tagline | "Let your agent present." with "Designed, reproducible, versioned." beneath. OPN-64's reel render uses it. |
| OPN-63, INT-08 | INT-24, INT-31, INT-35 (live connectors belong to Publish), INT-79 (the Publish host holds data-source credentials), INT-08's proposal (RichView never queries data) | Live connectors are standard on every RichView server, Cowork and Publish; credentials stay in that server's configuration. The core never queries data; on a server, a report can link datasets through the server's connectors (INT-08). |
| OPN-48 | OPN-25, OPN-65 (archive after a message to a second lead) | No message is sent first; each archive waits only for Daniel's go. |
| INT-71 | INT-41's proposal (no releases) | Builds from `prod` and the two extension listings are published; how they are versioned is still open (INT-41, DEL-41). |
| FEA-SF-30 | The legacy hotfix that OPN-02 and OPN-43 first proposed | No hotfix to the live page; it changes when the rewrite reaches `prod`. OPN-02 and OPN-43 were accepted in that form. |
| INT-31, ARC-05 | INT-04's proposal (static-first SVG charts) | WebGL charts by default; a frame and a notice where JavaScript or WebGL is missing. |
| INT-31, ARC-05 | INT-31's own proposal (raster or WebGL only as an opt-in acceleration; author-hosted assets and uploads out of the first cut) and ARC-05's title (static-first SVG with Wasm hydration) | WebGL by default; uploads and author-hosted assets are in scope (FEA-SF-50). |
| FEA-CH-37 | The card's title and proposal (exact source values in readouts, Table and CSV) | The author's precision, three digits by default, in every output; readers cannot retrieve more. |
| FEA-CH-01 | The card's title and proposal (exactly nine types) | At least every type of ggplot2, Plotly, ECharts, Highcharts and Power BI, likely around 30, plus a map, a flow element and pivot tables. |
| DEL-02 | INT-50's proposal (today's `main` stays frozen) | `main` is renamed `prod`; the legacy state is a commit in its history, and a legacy fix starts from a new branch (OPN-02). |
| INT-77, INT-13 (both 2026-10-27) | INT-17's order (storytelling second, a short authoring path third) | RichView does not maximize storytelling: it gives authors the tools and makes no judgment of a story's quality beyond syntax-level analysis (INT-77). Saving tokens is not a goal: RichView maximizes trust in the output (INT-13). INT-17's other slots and its guarantees stand. |
| INT-12 (2026-10-27) | OPN-42's proposal ("Mlouva", probably Mluva, is unclarified dictation) | Mluva is Daniel's own polish bar (INT-12). OPN-42 still governs the other unclarified phrases. |
| DEL-81 (2026-10-27) | DEL-02 (two branches), DEL-03 and DEL-08 (every change through the gate into `dev`), DEL-37 (a push block on `dev`), DEL-68 (process steps on every change) and DEL-71 (tiers), where they differ | Three branches, `dev`, `pre-prod` and `prod`. `dev` has the fast gate only; the release agent merges batches of up to 20 into `pre-prod` and runs the long suites and the adversarial reviews there; `prod` is cut from `pre-prod` on Daniel's go, after the full run (DEL-04, DEL-71). |
| DEL-37 (2026-10-27) | DEL-37's proposal (a gate macOS user, agent settings that deny deploy and write commands, other accounts' tokens removed) | A committed pre-push hook is the only technical block; agents have all the access. DEL-81 (later) moves the blocked branches to `prod`, `pre-prod`, `main` and `candidate/*` (AGENTS.md). |
| DEL-06 (2026-10-27 amendment) | DEL-06's 2026-10-26 answer (website and highly public-facing changes need Daniel's grant to merge into `dev`) | Website changes are always okay in `dev`; `dev` deploys to the hidden preview; going public waits for Daniel's go. |

- Four tensions between answers stay open, and all wait for Daniel:
  - DEL-81 (later) lets steps, tiers, hooks, reviews and thresholds change when they slow development, while DEL-68 says no agent removes or skips a step of the process. How the two fit is open; until Daniel decides, a change to a step is recorded with its reason in the PR that makes it.
  - INT-83 says Cowork stores data-source credentials only over time, not in its first cut, while OPN-63 (later) makes connectors standard on every server without a date. Whether Cowork's first cut ships connectors is open.
  - INT-04 and INT-27 make the exported file self-contained and working offline, while FEA-RP-62 (accepted later) hotlinks Unsplash photos from `images.unsplash.com`, so those photos show only online. What an offline file shows for them is open. OPN-63's URL data source is a decided exception of the same kind.
  - INT-03 has the rendered page travel with its source, while FEA-CH-37 says readers cannot retrieve more precision than the author chose. `/raw/...` and the stored record carry the source's full digits. DEL-80 holds the choice: serve the source only to readers who may edit the story, or serve it to every reader and scan it as part of the disclosure check.
- **Open (agents):** DEL-45 proposes that accepted answers are normative, that code or docs contradicting them are bugs, and that only the lockdown app writes `answers/`. DEL-66 proposes how answers reach `dev`.
- **Open (Daniel):** INT-62 proposes "build what was decided, nothing more": no unrequested effects or options, and only the final dictated correction counts.
- **Open (Daniel):** INT-63 proposes one recorded answer per preference; older tickets, briefs and PRs are history. OPN-35 to OPN-40 sort the June decision records the same way.
- **Open (Daniel):** OPN-42 proposes that unclarified dictation is not a requirement: "Escapism", "Where to Render", the "Big view" tagline, the "red nose" example, the remembered Rust alternative to Remotion, a second design resource and the dbt Charts reference. Mluva left the list: INT-12 names it as Daniel's own polish bar.
- **Open (agents):** OPN-67 proposes that part 05's types follow Daniel's accepted feature answers; OPN-57 and OPN-58 propose one diagnostic record and one code catalogue.

## Corrections never to repeat

Each of these needed correcting between July and October 2026 ([history evidence](evidence/history-prs.md), "Repeated corrections"). The rule cards are open and Daniel decides them; until he answers, each is the working default, because each records a correction he made. A repeat is a defect, not a matter of taste.

| What kept going wrong | Proposed rule | Card |
| --- | --- | --- |
| Styles did not reach charts, in seven rounds | A style applies to everything a report renders, PDF included; chart code holds no hard-coded colour or font | INT-56, open (Daniel) |
| Unquoted font families and silent fallbacks | Every declared font is bundled, named correctly and verified as loaded; a silent fallback is a defect a check reports | INT-57, open (Daniel) |
| Clipped, cropped and colliding labels, years printed as `1.70K` | No chart text is clipped, overlapped or garbled at any width; labels drop by priority and every drop is reported | INT-58, open (Daniel) |
| Button bars, unclear glyphs, too many links | Every control serves a named reader task; the default is fewer | INT-59, open (Daniel) |
| Charts and tables wider than the prose | One reading measure and one left edge for every block; wider artwork is an explicit author choice | INT-60, open (Daniel) |
| Ticking entrances, distorted bars, a scroll story that flickered on a phone | Motion ships only when smooth, which is the floor under INT-12's aim of delightful motion; a review agent judges it from frame strips and real-speed clips; reduced motion shows the settled state | INT-61, open (Daniel) |
| Requests misread or extended, abandoned options implemented | Build what was decided, nothing more | INT-62, open (Daniel) |
| Preferences that changed between rounds with no single recorded answer | One recorded answer per preference | INT-63, open (Daniel) |
| Code that returned quietly when a DOM anchor, runtime or asset was missing | Missing pieces produce a diagnostic and fail visibly | INT-64, open (Daniel) |
| Claims ahead of evidence | Every statement describes only what its build does, verified by running it | INT-65, open (Daniel) |
| Agent docs listing six chart types and three styles while the code shipped more | Agent-facing docs are generated from or checked against the code by a gate, so the drift cannot come back (INT-21 requires it) | INT-66, open (Daniel); INT-21 |
| Work started from a checkout 354 commits behind | Every task starts from fresh `origin/dev` in its own worktree | DEL-42, OPN-15, open (agents) |
| A shared Cargo target ran another worktree's builder | Every stream builds in one shared Cargo target whose wrapper records which checkout compiled last and rebuilds from this checkout; the release run builds in its own target and tests only the artifacts it hashed | DEL-13, DEL-22, DEL-42, open (agents) |
| Production deployed from unmerged branches under per-task authority | Only `prod` deploys richview.ai, on Daniel's go; tasks hold no deploy authority of their own | DEL-40 (decided), OPN-47 (accepted, waiting for go) |

## Engineering rules

- Wherever possible RichView makes the performant choice. RichView stays Rust, one shared core runs natively and as WebAssembly, and no product rule exists only in JavaScript in the rewrite's target. (INT-52)
- Charts render with WebGL by default. Where JavaScript or WebGL is unavailable, the chart's frame shows a notice that the chart cannot be displayed; no static snapshot, image or table fallback is built. Safari, Firefox and Chrome in reasonably recent versions must work. (ARC-05, INT-31)
- Only PDF export converts charts to vector output, and it may take time (INT-31).
- Authors customize only through declared, validated vocabulary; no raw CSS, JavaScript, HTML, engine specs or raw SVG (INT-28).
- Style files are rich (STY-14). Everything that differs between the built-in styles can be changed in a style file: fonts, colours, spacing, chart details and structural choices such as outlines, shading and where legends and titles sit. Anything about how a report feels can change; anything that fixes underlying mechanics, such as phone width or page width, cannot. Every value still passes the closed, validated vocabulary.
- Agents get clear guidance on which style settings are safe to play with and which are experimental; for example, centring a title may break the layout, so it is experimental (STY-14). OPN-59 settles the cross-part conflict this way: every value is validated and measured at render, so a weak result shows as a warning rather than being impossible.
- Visual and motion craft wins when targets conflict; the bar is side-by-side comparison with Highcharts and ECharts and FT-grade and Wall Street Journal design, and animation is delightful and smooth, the whimsical quality of Studio Ghibli meets the polish of Apple's website animations, with INT-61 as the smoothness floor (INT-12, INT-17). The references are benchmarks, never code or styling to copy.
- Saving tokens is not a goal: RichView maximizes trust in the output, so the same source renders consistently and to a high standard every time; good defaults and styles carry quality, and agents may write as much as they need (INT-13).
- RichView makes no judgment of a story's quality beyond syntax-level analysis (structure, missing sources, unlabelled charts) and does not promise that a report argues well; storytelling is the author's, and RichView gives the tools (INT-77).
- Agents learn, write, render, check and publish RichView on their own: `richview learn`, the skill and the MCP tool descriptions are generated from, or tested against, the same code as the renderer, and every error names the line and the fix (INT-21, INT-66).
- The core never queries data; only a RichView server can load linked datasets, through its connectors (INT-08, OPN-63).
- Readers never get more precision than the author chose (FEA-CH-37); see [GOALS.md](GOALS.md#what-richview-maximizes).
- Use tabular figures wherever the font can do it; proportional figures appear only where alignment is not needed and the font has no tabular set (OPN-62). RichView creates tabular-figure variants of Big Shoulders (Brutalist) and Patrick Hand (Hand-drawn); both are under the SIL Open Font License, which allows modified versions, and any Reserved Font Name rule decides what the variants are called.
- Data-source credentials stay in the server's configuration, never in `.rmd` files, report folders, rendered output, MCP arguments or agent context (OPN-63).
- Tests are not sacred; the process is (DEL-68). Agents may delete tests freely when they are no longer needed: unit tests at will, and end-to-end tests whose feature no longer makes sense. A test audit, run by an agent or on Daniel's request, removes tests that are only formal, test the obvious, repeat each other or test no real functionality. What an agent can never delete or skip is a step of the process: the gates themselves and the adversarial reviews. Elements inside a step may change; the steps stay. DEL-81 (later) lets steps, tiers, hooks, reviews and thresholds change when they slow development; how the two fit is open (see Authority).
- **Open (agents):** ARC-02 proposes that core and engine do no filesystem, network, process, environment or clock access.
- **Open (agents):** ARC-03 proposes where handwritten JavaScript is allowed: the VS Code adapter, Worker adapters, the embedded source editor on server pages, generated glue and test drivers.
- **Open (agents):** ARC-08 proposes one operation layer behind every surface, with identical diagnostics and byte-identical output from native and Wasm builds.
- **Open (agents):** ARC-09 proposes one schema registry from which every enumeration is generated.
- **Open (agents):** ARC-18 proposes that only `richview-chart` computes chart geometry, as one geometry model that the WebGL renderer draws and every check measures.
- **Open (Daniel):** ARC-69 proposes that the server core renders each story version's link-preview thumbnail natively from that geometry model, with no browser or GPU, stores it with the immutable version and serves it under the story's own access rule (INT-07).
- **Open (agents):** ARC-68 proposes a dependency gate with a licence allowlist compatible with MIT.
- **Open (agents):** TYP-53 and TYP-54 propose the diagnostic record and its levels; OPN-57 makes TYP-53's record the only one.
- **Open (Daniel):** FEA-SF-65 proposes measuring how well agents use RichView as a feature, with scored fresh-agent runs on real tasks tracked over time (INT-21, INT-69, DEL-78).
- **Open (Daniel):** FEA-RP-01 proposes byte-identical renders from the same source, inputs, styles and build, which INT-02's "Reproducible" property requires in some form.
- **Open (Daniel):** FEA-RP-30 proposes that a missing input fails only its element and the draft is still written.
- **Open (Daniel):** FEA-RP-39, FEA-RP-36, FEA-RP-49 and FEA-RP-59 propose the reading floor: text and tables readable without scripts, a reduced-motion floor that the WebGL renderer also honours, WCAG 2.2 AA in every style, and Baseline browser support with WebGL2 as the one exception.
- **Open (Daniel):** STY-50 and STY-51 propose RichView's design rules and the three review questions; OPN-32 proposes them as the one design-rule source. STY-70 proposes that a story's link-preview thumbnail follows its style and joins each style's review sheet.

## Working rules

- Agents may, without asking (DEL-05): create worktrees and task branches, push them, and open, update and close their PRs into `dev`; merge into `dev` after the fast gate and revert `dev` merges; run local builds, checks, benchmarks and loopback previews; create and update RichView Linear issues; assemble review packets; delete branches, their own and others; set up automations themselves, such as workflows and scheduled jobs, within the other limits of DEL-05; and talk to other agents and collaborate on tasks, including message boards and pings through Linear and local helpers such as Herdr.
- Everything else needs Daniel's explicit authorization, as DEL-05 proposed and his answer kept: any change to `prod`; deploys and deployment rollbacks (except DEL-48's pre-authorized rollback); Cloudflare, D1, R2, Access, DNS, Fly and secrets; origin settings (default branch, renames, new long-lived branches, tags or releases, protection, auto-merge, runners, webhooks, collaborators, machine accounts, visibility); users and tokens on the gate host; spending (GitHub plan, Actions minutes, paid services); publication (tags, releases, package uploads, posts); weakening a gate; changing an accepted answer. His go covers exactly the action it names. The list of actions waiting for his go is in [DELIVERY.md](DELIVERY.md#outward-actions-waiting-for-daniels-go).
- There are three branches (DEL-81, which supersedes DEL-02's two-branch model where they differ). `prod` is the old `main`, renamed (DEL-02); today's `dev` was renamed `pre-prod`; a new `dev` was created from it and is the default branch. `prod` moves only on Daniel's go, cut from `pre-prod` after the full run (DEL-04, DEL-71). Agents do adventurous work on `dev`.
- Development agents run in parallel, as many as there are fully independent pieces of work, each in its own worktree, and merge into `dev` freely. No commit, pre-push or stop hooks slow them down. The lockdown session leads: it splits the work into independent streams, briefs the agents and relays Daniel. (DEL-81)
- Testing in `dev` is at the agent's discretion. The fast gate is unit tests only, ideally under five seconds; agents run browser, visual or end-to-end checks when they judge it useful. Impact tracing runs only the tests a change can affect, in the fast gate and the release run alike. (DEL-81)
- The release agent merges `dev` into `pre-prod` after a batch of up to 20 merges, runs the long-running suites (end-to-end, browsers, visual, data integrity, PDF) on the combined state for what the batch affects, and fixes forward until everything works together. The adversarial reviews run per batch. (DEL-81)
- The final reviewer, the checker and the adversarial reviews run on Claude Opus at xhigh; design development (anything judged by eye) runs on Claude Sonnet at xhigh; everything else runs on Codex (`gpt-6.1-sol`, reasoning xhigh) in the Codex TUI; at most two Opus agents run at once across the build. (DEL-81, amended 2026-10-29)
- Anything that slows development can change: steps, tiers, hooks, reviews and thresholds. Broad end-to-end coverage at release is what lets small changes move fast. (DEL-81; see Authority for its tension with DEL-68.)
- Squash merges (DEL-03). Before merging, the agent completes every item of the PR checklist, including its set of tests; the gate a change passes into `dev` is the fast gate, because DEL-81 moves the long suites to the release run.
- Feature and design changes merge into `dev` without asking or pinging Daniel, whether he asked for them or an autonomous loop generated them; other agents may revert or remove them on `dev`. Gates always run. (DEL-06)
- A committed pre-push hook is the only technical block, and no GitHub login leaves the agents' environment: there is no gate macOS user, no machine account and no paid plan, and agents have all the access, `.claude/settings.json` carrying no deny list. Daniel accepts the occasional slip; the audit (DEL-38) catches it after the fact. Outward actions still follow the go rules (DEL-04, DEL-40). (DEL-37, as amended by DEL-81)
- Website changes are always okay in `dev`: they merge through the gates without a grant, and `dev` deploys to the hidden preview (DEL-06, DEL-40). Nothing becomes public before `prod`, and going public stays on Daniel's go (DEL-06).
- Design changes and feature changes are kept clearly separate, and `dev` keeps a feature and design log so Daniel can review every change one by one before anything goes to `prod` (DEL-06).
- The trail is complete: every PR is part of the documentation, every change can be reverted, and one report shows before and after, together and step by step, with what caused each change (DEL-06, DEL-69).
- Setup steps are the agents' call: they pick the best order and method, following DEL-02, DEL-05 and DEL-81. Nothing is in production, so it is fine if things are down for a while during setup. (DEL-07)
- **Open (agents):** DEL-42 proposes one task as one Linear issue, one worktree under `.claude/worktrees/<slug>` from fresh `origin/dev`, one branch and one PR; on Claw Mini every stream builds in the one shared Cargo target (DEL-22).
- **Open (agents):** DEL-22, DEL-20 and DEL-61 propose that development streams and the gate share one Rust target folder through `CARGO_TARGET_DIR`, with a wrapper that stamps which checkout built last, a disk floor of 10 GiB (Daniel, 2026-10-29; it was 40) and a separate release target; that the gate runs under Daniel's normal user, with no gate user; and that only release and candidate runs write receipts, kept for the life of the repository while run folders are pruned.
- **Open (agents):** DEL-44 proposes `Refs S27-<n>` in every PR. DEL-03's answer flags that the Linear board is private, so how commits reference work once the source is public is still to be decided.
- **Open (agents):** DEL-21 proposes honest status: every status names what it rests on and says what passed, failed and did not run; a merge into `dev` is reported as merged, not verified, and only release and candidate receipts are evidence.
- **Open (agents):** DEL-35 proposes that review agents are required steps whose verdicts advise; a finding counts once reproduced. An adversarial review by an Opus agent reads each release batch on `pre-prod` before its candidate is frozen (DEL-81). Whether a negative verdict also holds a batch back from `pre-prod`, or a candidate from `prod`, is open.
- **Open (agents):** DEL-43 proposes first come, first served merges into `dev` with no merge lock, rebase conflicts resolved by the owner, and force-pushes only on one's own task branch, with lease.
- **Open (agents):** DEL-46 and DEL-67 propose `AGENTS.md` as the one instruction text, `CLAUDE.md` importing it, and no in-repo multi-agent flow; OPN-30 and OPN-33 propose the first `dev` change that applies them.
- **Open (agents):** DEL-62 proposes no cap on the number of development agents, since the independence of the work sets it (DEL-81); the limits are the shared Cargo target's lock, its 10 GiB disk floor (Daniel, 2026-10-29; it was 40) and the host's memory, plus DEL-81's cap of two Opus agents at once.

## Public copy and privacy

- The tagline and its use are INT-02's (see [GOALS.md](GOALS.md#definition)).
- Every story with an address unfurls as a rich link preview with a designed thumbnail in its own style; a story shared only with signed-in readers shows its title and image only to them (INT-07, FEA-SF-55).
- The docs and the website are always public; the source stays private until launch (INT-38).
- The open-source sentence is "RichView is open source and MIT licensed." (FEA-SF-30, INT-37). When it goes live relative to the public source is open (INT-39, INT-65, Daniel).
- The live page gets no hotfixes; it changes when the rewrite reaches `prod` (FEA-SF-30).
- The first posts go out only after Daniel has reviewed them (INT-43).
- **Open (Daniel):** INT-42 proposes "Coming soon" as the only public status word until launch.
- **Open (Daniel):** INT-65 proposes that public surfaces describe what `prod` does, and that counts come from the code.
- **Open (Daniel):** INT-45 proposes that marketing never restates the flagship's findings.
- **Open (Daniel):** INT-80 proposes that nothing tracks readers or authors.
- **Open (Daniel):** OPN-27 proposes that no public page links a private repository.
- **Open (agents):** DEL-55 proposes a secret and private-data scan that fails a merge on credentials, personal addresses, private Notion or vault links, home paths or subscriber data.
