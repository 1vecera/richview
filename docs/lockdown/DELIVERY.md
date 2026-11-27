# Delivery

Status: regenerated on 2026-10-27 from Daniel's answers in the lockdown app (77 of 769 cards answered, 70 of the 134 key cards among them) and the current proposals in `parts/`. DEL-81's model amendment of 2026-10-29 is applied by hand.

How a change reaches `dev`, `pre-prod`, `prod` and the live sites, and which outward actions wait for Daniel.

How to read this file:

- A plain card ID marks a decision: Daniel answered that card, and it binds. An `edit` answer is restated in his words; an `accept` answer binds the proposal as it stood. Where a later answer changes an earlier one, the later one wins and both IDs are named; [RULES.md](RULES.md#authority) lists the known cases.
- **Open** marks a proposal, not a decision. It names its card and who decides it.
- **Open (agents):** a technical or implementation detail. The migration agent may decide it, consistent with every decision, and records its choice in the PR.
- **Open (Daniel):** a value, feature, design or business choice. Agents may build the proposal on `dev` as a declared feature or design change, and it reaches `prod` only after Daniel accepts it in the before-and-after report (DEL-06, DEL-69). Website changes merge into `dev` like any other change and show on the hidden preview; only going public waits for his go (DEL-06, amended 2026-10-27).
- Outward actions wait for Daniel's explicit go, even when their card is accepted. [Outward actions waiting for Daniel's go](#outward-actions-waiting-for-daniels-go) lists them in one place.
- By default, Daniel decides open cards in parts 01, 04a to 04d and 06 and the non-technical cards of part 08; agents decide open cards in parts 02, 03, 05 and 07 and part 08's technical cross-part cards. A card that spends money, publishes something or changes what Daniel reviews stays his. The 70 cards raised to key on 2026-10-26 are next for Daniel, in the order of [review-queue.md](review-queue.md).
- `node tools/lockdown-app/cli.mjs show <ID>` prints any card in full.

## Branches and authority

- Three long-lived branches (DEL-81, which supersedes DEL-02's two-branch model where they differ):
  - `dev` is the default branch. Development agents merge into it freely.
  - `pre-prod` is written by the release agent, which merges `dev` into it after a batch and runs the long suites on the combined state.
  - `prod` moves only on Daniel's go. It is cut from `pre-prod` after the full run (DEL-04, DEL-71 apply to that step).
- Today's `dev` was renamed `pre-prod`, and a new `dev` was created from it (DEL-81). `main` was renamed `prod` (DEL-02). The legacy state is only a commit in `prod`'s history, and a legacy fix needs a new branch (DEL-02, OPN-02).
- `prod` starts from `main` after PR #69 is merged into it as it stands, as the record of production (OPN-01, OPN-13). That `main` already holds PR #70, merged as it is on 2026-10-26, so `prod`, `pre-prod` and `dev` start with News's code (OPN-01, INT-52). Merging #69 records production; it does not approve its design.
- Development agents run in parallel, as many as there are fully independent pieces of work, each in its own worktree, and merge into `dev` freely. No commit, pre-push or stop hook slows them down. Models: the final reviewer, the checker and the adversarial reviews run on Claude Opus at xhigh; design development (anything judged by eye) runs on Claude Sonnet at xhigh; everything else runs on Codex (`gpt-6.1-sol`, reasoning xhigh) in the Codex TUI; at most two Opus agents run at once across the build. (DEL-81, amended 2026-10-27 and 2026-10-29)
- Testing in `dev` is at the agent's discretion. The fast gate is unit tests only, ideally under five seconds; agents run browser, visual or end-to-end checks when they judge them useful, for example to see a new chart. Dependencies between crates, modules and suites are traced automatically, so the fast gate and the release run only run the tests a change can affect. (DEL-81)
- The release agent merges `dev` into `pre-prod` after a batch of changes: three, five or ten by their size and significance, and up to 20 (DEL-81, amended 2026-10-27). It runs the long suites (end-to-end, browsers, visual, data integrity, PDF) on the combined state, for what the batch affects, and fixes forward until everything works together.
- The checker and the adversarial reviews run on Opus, and at most two Opus agents run at once across the build (DEL-81, amended 2026-10-27).
- The lockdown session leads: it splits the work into independent streams, briefs the agents and relays Daniel (DEL-81).
- Anything that slows development can change: steps, tiers, hooks, reviews and thresholds. Broad end-to-end coverage at release is what lets small changes move fast. (DEL-81, amended 2026-10-27) How this fits DEL-68's rule that no agent skips a step of the process is open for Daniel (see [Trust and review before prod](#trust-and-review-before-prod)).
- DEL-81 is later than DEL-02, DEL-03, DEL-08, DEL-37, DEL-68 and DEL-71 and, as its card records, supersedes them where they differ: DEL-02's two branches, DEL-03's and DEL-08's gate on every change into `dev`, DEL-37's hook on `dev`, DEL-68's process steps on every change (reviews and long suites now run per `pre-prod` batch) and DEL-71's tiers (fast in `dev`, full in `pre-prod`).
- DEL-03 chose squash merges and that the agent completes every item of the PR checklist before merging. A change now reaches `dev` after the fast gate alone, by a rebase and a push or by merging its PR, and every commit or PR into `dev` names its issue with `Refs S27-<n>` (DEL-81, AGENTS.md). How commits reference work once the source is public is still to be decided, because the Linear board is private and `Refs S27-<n>` then points nowhere for outside readers (DEL-03, DEL-44; open, agents).
- An agent merges into `prod` after Daniel's go (DEL-04). A second, automatic set of tests runs on the prod candidate, the frozen `candidate/<n>` cut from `pre-prod` (DEL-63, DEL-33); when it passes, Daniel tells an agent to merge, and the agent merges into `prod` with a single command, `rv-gate merge-prod candidate/<n> --go "<Daniel's words, date>"`. Only Daniel gives that go for now; named main collaborators may later. No agent moves `prod` on its own.
- `dev` evolves on its own against the lockdown's principles, documents and ideas (DEL-06). Feature and design changes merge into `dev` through the fast gate without asking or pinging Daniel, whether he asked for them or an autonomous loop generated and implemented the idea. Other agents may revert or remove them on `dev`. Gates always run: the fast gate before each push to `dev`, and the release run on `pre-prod`.
- Website changes are always okay in `dev`: they merge through the fast gate without a grant, and `dev` deploys to the hidden preview (DEL-40). Nothing becomes public before `prod`, and going public stays on Daniel's go (DEL-06, amended 2026-10-27).
- Design changes and feature changes are kept clearly separate. `dev` keeps a feature and design log, so Daniel can review every design change and every feature change one by one before anything goes to `prod`. (DEL-06)
- When Daniel asks for something new that the lockdown does not cover, the agent also proposes the change to the lockdown documents, and Daniel looks at that first (DEL-06).
- The trail is complete: every PR is part of the documentation, every change is tracked so any of them can be reverted, and one report shows before and after, both together and step by step, with what caused each change (DEL-06).
- Agents may, without asking (DEL-05, as proposed with Daniel's additions): create worktrees and task branches, push them, and open, update and close their PRs into `dev`; merge into `dev` and revert `dev` merges through the fast gate; run local builds, checks, benchmarks and loopback previews; create and update RichView Linear issues; assemble review packets from frozen candidates; delete branches, their own and others; set up automations themselves, such as workflows and scheduled jobs, within the other limits of DEL-05; and talk to other agents and collaborate on tasks, including message boards and pings through Linear and local helpers such as Herdr. Writing `pre-prod` is the release agent's work through its release run (DEL-81).
- Everything else needs Daniel's explicit authorization, and his go covers exactly the action it names (DEL-05): any change to `prod`, including a merge, push, revert, reset or legacy fix; deploys and deployment rollbacks, except DEL-48's pre-authorized rollback; Cloudflare, D1, R2, Access, DNS, Fly and secrets; origin settings such as the default branch, renames, new long-lived branches, tags or releases, protection, auto-merge, runners, webhooks, collaborators, machine accounts and visibility; users and tokens on the gate host; spending, such as the GitHub plan, Actions minutes or paid services; publication, such as tags, releases, package uploads and posts; weakening a gate; and changing an accepted lockdown answer.
- The setup steps are the agents' call: they pick the best order and method, following DEL-02, DEL-05 and DEL-81. Nothing is in production, so it is fine if things are down for a while during setup. (DEL-07, whose list of setup steps is superseded where DEL-37 and DEL-81 differ)

## Outward actions waiting for Daniel's go

Each action below changes something outside the repository's `dev` flow. None may run until Daniel gives his explicit go for that action, even where its card is accepted. Accepting a card decides what happens; the go decides when.

Status on 2026-10-27: Daniel gave his go for the whole cleanup batch ("yes go all").
- **Done and verified:**
  - OPN-05 and OPN-45: full backups first, in `~/archive/richview-legacy/2026-10-27/` on Claw Mini, including the `richview-api` database. Then the Fly apps `richview-v3` and `richview-api` were destroyed.
  - OPN-04 and OPN-45: richview.uk, app.richview.uk and docs.richview.uk return 301 to richview.ai through the Worker `richview-uk-redirect`. Their old Pages projects are deleted.
  - OPN-25 and OPN-65: README pointer added and repository archived.
  - OPN-15: the worktree is saved as a `wip/` tag for the site round and removed.
  - OPN-47: the supervisor and its launch agent are deleted, and the delegations removed.
  - `main` was renamed `prod`, and `dev` is the default branch.
  - DEL-81: today's `dev` was renamed `pre-prod`, and a new `dev`, the default, was created from it. `origin/pre-prod` exists.
  - OPN-45: the `api.richview.uk` DNS record is deleted (with a restore record in the ticket). It no longer resolves, and the zone keeps only the apex, `app` and `docs` redirect records, CAA, DMARC and SPF.
  - OPN-44: Cloudflare Access application "RichView teaser dev preview" covers `richview-teaser.vecera.workers.dev` and admits only Daniel's account email. The address now answers with a redirect to the Access sign-in; richview.ai still serves publicly.
  - Rechecked live on 2026-10-27 at 22:40: `api.richview.uk` has no record, the three richview.uk hosts return 301 to `https://richview.ai/`, the teaser returns 302 to `cloudflareaccess.com`, richview.ai returns 200, and both Fly apps no longer answer.
- **Still open:**
  - Daniel's own first sign-in through Access is not yet exercised.
  - Deploying PR #69 to richview.ai is not part of this batch.

The batch as accepted (Daniel accepted the cleanup batch on 2026-10-26: "Yes, all is good"):

- **OPN-04:** richview.uk redirects (301) to richview.ai now, as part of the cleanup, not at launch. `www.richview.uk` does not resolve. therichview.com is allowed to lapse, and the server code that redirects it goes.
- **OPN-05:** the May Fly app's one report (`v3-launch`, a 2026-05-21 sample) is exported for the archive, then the Fly app `richview-v3` and its volume are destroyed. `packages/server/fly.toml` goes with the orphan TypeScript folders.
- **OPN-45:** the seven reports of the Fly app `richview-api` are exported for the archive; app.richview.uk and docs.richview.uk redirect to richview.ai with the apex; api.richview.uk leaves DNS; `richview-api` and its database are destroyed; and whatever origin serves the apex, app. and docs. is retired once the redirects are in place. Daniel confirmed he owns and pays for `richview-api`, so no one else needs to agree.
- **OPN-25:** `richview-universe/richview-charts` and `richview-universe/richview-docs` each get a short README note pointing to richview.ai, linking no private repository, and are then archived, now rather than at launch. The card's earlier precondition, a message to a second lead, no longer applies (OPN-48).
- **OPN-65:** `1vecera/RichTemplates` is archived the same way, on the same go.
- **OPN-47:** the October automation is retired before the rewrite starts: the Codex automation `richview-supervisor-30m` and the launch agent `dev.daniel.richview-supervisor-recovery` are deleted, keeping their state and logs as evidence; the supervisor's scope file and the October briefs get a note that the lockdown supersedes them and grants no authority; the app delegations on the nine October issues are removed, the site and chart-polish issues move to To Review, and the galleries, site and RichView Publish issues get a comment that their deploy authority ended on 2026-10-20; the Lenovo oversight timer is checked and removed. From then on only DEL-40 authorizes deploys.
- **Deploying PR #69 to richview.ai.** richview.ai already serves the Worker version built from #69's branch (`7f3bef0d-72d8-4f20-9ef4-478091be7ee6`, 7b23a22, deployed 2026-10-19), which stays the rollback target (OPN-01). Any new deploy of it waits for his go.

Decided steps that DEL-05 keeps behind his go:

- Merging PR #69 into `main` (OPN-13), renaming `main` to `prod`, creating `dev` and making it the default branch (DEL-02), and renaming `dev` to `pre-prod` with a new default `dev` (DEL-81). These are changes to `main` and origin settings; the renames and the default branch are done (see above).
- Putting the Cloudflare Access policy in front of richview-teaser.vecera.workers.dev during setup, before `dev` first deploys there (OPN-44, ARC-22).
- Disabling the registered workflows, so setup pushes start no hosted runs, if that needs an Actions setting rather than a workflow change (DEL-05, DEL-39).
- Every deploy, including hosted Cowork, customer Publish servers, the data and media hosting, News and the `dev` preview's Access setup (DEL-40, ARC-22).
- Every merge into `prod` (DEL-04).
- Every post, including any that uses the reel's MP4 and poster; rendering them is local work that needs no go (INT-43, FEA-SF-45, OPN-64).
- Making the source public and launch: his word alone (INT-38, INT-43). The first posts go out only after he has reviewed them (INT-43).

Open cards whose outward steps will also need his go once he answers them:

- News deployments, its publishing credential and publisher job (OPN-03, OPN-46).
- Closing PRs #62 to #68 unmerged (OPN-06 to OPN-12) and pushing the `wip/*` tags that preserve unpushed work (OPN-15).
- Deleting the October prerelease, keeping its tag (OPN-28).
- Publishing builds from `prod` and the extension listings (INT-71 decides that they exist; DEL-41 and INT-41 are open on how).
- Mailing the update list (INT-72).
- Updating RichView guidance outside this repository: the agent context file and two Notion pages (OPN-49).
- Spending on hosted CI (DEL-39).

Agents may do these without a go: deleting merged and closed branches (OPN-29, DEL-05), reconciling Linear (OPN-41, DEL-05), and local work such as installing Firefox, qpdf and Poppler on Claw Mini (DEL-07).

## Trust and review before prod

- Trust is set per kind of change (DEL-68, "Gates decide none; you see design and feature one by one"):

| Change | Evidence that is enough | Where Daniel sees it |
| --- | --- | --- |
| `none`: fixes, refactors, accepted items implemented with no capture change | The fast gate when it merges into `dev`, then the release receipt of the batch that holds it (DEL-81, DEL-21) | Packet appendix only, plus a sample a fresh review agent reads |
| `design`: a capture, motion, token, typography or copy change | Release receipt, its before-and-after entry and a review agent's verdict | One by one in the design log before `prod` |
| `feature`: scope added, changed or removed | Release receipt, public-surface diff and its before-and-after entry | One by one in the feature log before `prod` |
| Website change | Merges into `dev` like any other change and shows on the hidden preview; it becomes public only with `prod` (DEL-06, amended 2026-10-27) | His go for the `prod` merge and the deploy |
| `prod` merge | The prod tests (DEL-71) and his log review | His go |
| Visual and motion quality | A review agent on the model Daniel uses at the time (Opus, Codex or another), judging against INT-12's bar | One large document with every change before `prod`; he can adjust visual and motion quality independently |

- Each packet, a fresh review agent reads a sample of `none` merges end to end. A defect the gates missed adds a check, and that kind of change moves to one-by-one review until the check exists and has caught a deliberate mutation of the defect. (DEL-68)
- Tests are not sacred; the process is (DEL-68). Agents may delete tests freely when they are no longer needed: unit tests at will, and end-to-end tests whose feature no longer makes sense. A test audit, run by an agent or on Daniel's request, removes tests that are only formal, test the obvious, test the same thing several times or do not test real functionality. No agent may delete or skip a step of the process: the gates themselves and the adversarial reviews.
- **Open (Daniel):** DEL-81 (later) lets steps, tiers, hooks, reviews and thresholds change when they slow development, while DEL-68 says no agent removes or skips a step of the process. How the two fit is not decided; until Daniel decides, a change to a step is recorded with its reason in the PR that makes it (DEL-26, DEL-35, DEL-36, DEL-38). DEL-81 also moves the reviews and the long suites from every change to every `pre-prod` batch (DEL-35).
- The review agent judges visual and motion quality against INT-12: charts that withstand side-by-side comparison with Highcharts and ECharts, typography and layout against FT-grade and Wall Street Journal editorial design, and animation that is delightful and smooth, "the whimsical quality of Studio Ghibli meets the polish of Apple's website animations". Those are benchmarks, never code or styling to copy. Daniel's own polish bar is Mluva. (INT-12, INT-67, DEL-68)
- The before-and-after report (DEL-69, accepted) is generated from `dev`'s change log for a frozen candidate against `prod`, and on request for any range of `dev`. DEL-70 proposes that the release run writes the log as it takes each batch into `pre-prod`. Design and feature changes sit in separate sections and never share an entry:
  - **Together:** one contact sheet of every changed capture, grouped by style and width, and the net size and timing deltas.
  - **Step by step:** one entry per change, in merge order, with its cause (Daniel's request with its quote, an autonomous-loop idea with the principle it pursued, or a bug or regression issue), the lockdown items it touches, its PRs and receipts, and a one-command revert.
  - **Design entries:** `prod`, candidate and diff crops at 390 and 1440 px in every affected style (all five for a style-wide change); for motion, the frame strip at fixed fractions and a short clip at real speed, before and after side by side; for interaction, a short recording.
  - **Feature entries:** the public-surface diff (CLI, MCP, schema, `learn`), a demo source with its render, and the docs diff.
  - **Reverted and superseded changes** stay listed, linked to what undid them.
- The report opens in the lockdown app with the packet and on the `dev` preview, so it works on a phone (DEL-69, ARC-22).
- The prod gate is not a special suite (DEL-71). It re-runs the same tests that already run before it, all of them, on the frozen candidate, to prove everything still works after many merges have combined. DEL-81 places those tests in the release run and the full run on `pre-prod`, and the candidate is cut from `pre-prod`.
- Tests come in three tiers (DEL-81, DEL-24, which replace DEL-71's four): the fast gate in `dev`, unit tests only; the release run on `pre-prod` per batch, with the long suites for what the batch affects; and the full run before a candidate is cut, repeated on the frozen candidate before `prod`. The tiers still grow as work moves toward `prod` (DEL-71).
- Every row of DEL-71's table belongs to that full set and must pass: the browser matrix, motion, every chart type in every style, journeys and roles, connectors, PDF, install paths, budgets, accessibility, a fresh-agent run and the merge audit (DEL-71). A row that cannot run fails the candidate unless this file names it as a standing gap (see [Records kept here](#records-kept-here)).
- Adversarial reviews run per `pre-prod` batch, on Opus (DEL-81, DEL-35), and visual polish happens before the candidate is cut (DEL-71); the prod step only confirms that everything runs and that each review was recorded. The go card shows the receipt, and the merge command accepts only the named candidate. After each deploy, smoke checks run, and a failure triggers DEL-48's rollback. (DEL-71, DEL-04)
- Each style gets one review sheet generated from the candidate (STY-68): the typography specimen, the every-block fixture, a representative majority of chart types, the map, flow, a normal table and the pivot table, and the flagship, at phone and desktop widths and as PDF, beside the same content in the other four styles; the style's declared structure verified against the geometry model; entrance clips; and STY-51's three answers. The agents choose the selection. A review agent judges the sheet first, distinctiveness included. Daniel reviews a sheet when a style is created and when its tokens or structure change; after that, changes reach him one by one in the before-and-after report.

## Deployment and rollback

- Only `prod` deploys richview.ai, on Daniel's go; a deploy is its own decision, the deploy card in a packet (DEL-40, "Deploy on a separate go in the packet").
- **One go for a candidate (DEL-83, Daniel 26 Nov 2026; supersedes only the per-deploy go of the line above and DEL-40/DEL-05's separate deploy go):** one go from Daniel covers every part of a candidate's release: the Windows walk, the `prod` merge, richview.ai, app.richview.ai, and the public source and release. One command (`tools/rv-release/release.mjs`) runs the parts he named, in that fixed order, each with its rollback ready, and a go can still exclude a part. Every check that can run before a deploy, such as the live route check against the staged site and the staged hosted build, runs first and refuses the deploy when it fails. The parts are independent: a failure stops and rolls back only its own part, and the later parts that do not depend on it still run, so a website failure does not block the app update or the other way round. Safety, data truth and the `prod` gate stay binding, and no check is weakened.
- Hosts (ARC-22, which later answers on the `dev` preview and News; DEL-40's table holds for the rest):

| Host | Decision |
| --- | --- |
| richview.ai | Static site plus signup, deployed only from `prod`, on Daniel's go. Until the first rewrite deploy it keeps serving today's site; it gets no hotfix (FEA-SF-30). |
| richview-teaser.vecera.workers.dev | Kept and hidden behind Cloudflare Access, a sign-in Daniel holds. It is the `dev` preview of richview.ai and the pre-production check (DEL-40's phrase; it is not the `pre-prod` branch). It redeploys from `dev` only when the website's code changes, not on every `dev` change. |
| Hosted RichView Cowork | At app.richview.ai, free up to limits and paid beyond. Deploys from `prod` on Daniel's go. |
| Publish servers for customers | Set up on request only, under the customer's DNS. Nothing runs until a customer asks. |
| news.richview.ai | RichView's own Publish server, public. It stays published. Each deployment is on Daniel's go. |
| Managed hosting for large data and media | From `prod` on Daniel's go (DEL-40's table, ARC-22). |
| richview-v3.fly.dev | Shut down after export (OPN-05). |
| richview.uk | 301 to richview.ai (OPN-04). |
| therichview.com | The domain lapses; its redirect code is removed. |

- Every production deployment goes through the `prod` gate on Daniel's go, and Fly configuration is deleted (ARC-22).
- One deployment owner per deploy, named in the card, deploys the exact tree, records previous and new version IDs and asset hashes in a deploy receipt, and runs the route check against the live routes (DEL-40, as proposed).
- DEL-40 said `dev` deploys to a preview at a separate hostname such as preview.richview.ai; ARC-22 (later) makes the hidden workers.dev address that preview. DEL-40's table also named a separate private phone-preview Worker; ARC-22's later host list has none, and OPN-66 proposes using the `dev` preview instead (open, agents).
- Which branch News deploys from now that PR #70's branch is fully merged into `main` is open (OPN-01, INT-74, OPN-03; agents propose, each deploy is Daniel's go).
- DEL-05 pre-authorizes one rollback: the deployment owner may roll back, without asking, only to the previous `prod` deployment and only when post-deploy checks fail, and reports to Daniel at once. **Open (agents):** DEL-48 proposes the details: nobody reverts a merge commit on `prod`; Daniel fixes forward through `dev` (the default) or resets `prod` himself.
- **Open (agents):** DEL-47 proposes that any agent reverts a bad `dev` merge with an ordinary revert that passes the fast gate, that `dev` is never rewritten and is not stopped, and that when a release run fails the release agent fixes forward, with a fix or a revert, until `pre-prod` works together (DEL-81).
- **Open (agents):** DEL-77 proposes smoke checks per hosted target after every deploy and on a schedule between deploys, including that a shared story's link preview returns its thumbnail and a private story's preview reveals no title or image (INT-07).

## The gate commands: fast, release and full

All of this section is open, and the agents decide it within DEL-81's three tiers, DEL-71's full set for the candidate and DEL-68's rule that no gate step is skipped (DEL-81's amendment and that rule are in tension, which is open for Daniel; see [Trust and review before prod](#trust-and-review-before-prod)). DEL-81 supersedes the earlier proposal that every change passes one gate into `dev`.

- **Open (agents):** DEL-08 proposes `rv-gate` with four commands, as AGENTS.md has them: `rv-gate fast` before every push to `dev`; `rv-gate release`, which the release agent runs per batch of up to 20 merges, merging `dev` into `pre-prod` and running the long suites for what the batch affects; `rv-gate full` on `pre-prod` before a candidate; and `rv-gate prod candidate/<n>` on each frozen candidate (DEL-33, DEL-71). `rv-gate candidate` cuts `candidate/<n>` from `origin/pre-prod` (DEL-63); the tool has it and AGENTS.md does not name it. `rv-gate steps` lists every step with its tier and cards. A step not built yet is reported as unrun with its reason, and blocks once it exists.
- **Open (agents):** the fast gate is unit tests only, ideally under five seconds (DEL-81): the unit tests of the changed crates and of the crates that `include_str!` a changed file, `cargo clippy -D warnings` of them and their dependents, tests included, the node tests of the tools whose folder changed, and rustfmt on the changed files. Impact tracing selects what a change can affect (DEL-08, DEL-24).
- **Open (agents):** the release run runs the rest of the registry below on `pre-prod`'s combined state, for what the batch affects, from the gate code of the previous release so that a batch cannot loosen the check that judges it (DEL-36). The release agent fixes forward until it passes (DEL-47). Only release and candidate runs leave receipts; a fast-gate run leaves only its output, which is not evidence (DEL-20, DEL-21).
- **Open (agents):** on Claw Mini every development stream and subagent builds in one shared Cargo target, `~/tmp/richview-rewrite/target`, one build at a time, with a wrapper that stamps which checkout built last so the target never builds the wrong checkout and a disk floor of 10 GiB (Daniel, 2026-10-29; it was 40) free; the release run builds in its own folder, `target-release`, so a long release build never holds the streams' lock (DEL-22, DEL-13, AGENTS.md). The release and candidate runs happen only on Claw Mini, under Daniel's normal user (DEL-22, DEL-37).
- **Open (agents):** DEL-72 to DEL-80 add the WebGL-era checks: the geometry model as the chart test oracle with a draw-fidelity check, deterministic WebGL captures with per-engine baselines, the Chrome, Firefox and Safari matrix (a three-engine smoke in every release run, full suites and real Safari at the candidate), motion tests on stepped frames, end-to-end journeys (author and agent journeys in every release run, all at the candidate), hosted smoke checks, the fresh-agent run, traceability from each accepted item to its checks, and a precision check that no output shows more digits than the author chose.
- **Open (agents):** DEL-78 proposes the fresh-agent run as the gate side of FEA-SF-65's measurement of how well agents use RichView (Daniel's INT-21 answer): Claude Code and Codex, each in an empty sandbox outside any checkout with only the candidate's installed CLI and MCP server, run real tasks against INT-69's bar, three runs per product, at each candidate and weekly on the `pre-prod` head, where DEL-81 runs the long suites; whether a run on an Opus model counts toward DEL-81's limit of two Opus agents at once is open. Each run is scored, the scores are kept as a series so the trend shows, and each confusion caused by docs or diagnostics becomes an issue, so that RichView is changed until agents understand it as easily as possible. FEA-SF-65 itself, with INT-69, is open for Daniel.

The proposed registry, with where each step runs and what was measured on Claw Mini on 2026-10-21 and 2026-10-22 at `b4db233` (a record, not a decision):

| Step | Card | Runs in | Today on Claw Mini |
| --- | --- | --- | --- |
| Host and toolchain pins: Rust 1.99.0, wasm-bindgen 0.2.129, Playwright 1.65.0 with its browsers, one Node version, the macOS build | DEL-22, DEL-49 | release, full | Not built; Node pins disagree today (20.11.0, 22, 26.5.0) |
| Tier registry, decision class, public-surface snapshots, weakening detection, issue reference | DEL-23, DEL-32, DEL-53, DEL-36, DEL-44 | release | Not built |
| Format | DEL-09 | fast (changed files), release | 0.8 s |
| Native lint | DEL-10 | fast (changed crates and dependents), release | 0.1–0.6 s warm |
| Wasm lint | DEL-11 | release; fast when a change reaches the browser crate | 0.1–2.5 s warm |
| Architecture boundaries; secrets and private data | DEL-54, DEL-55 | fast (source-only checks and the diff scan), release | Not built |
| All Rust tests, slow tier included | DEL-12 | fast (the unit tests a change can affect), release (everything the batch can affect) | 438 pass; 44.2–47.0 s warm for the whole suite, 83.3 s after relinking |
| Release build once, artifacts identified by hash | DEL-13 | release | 2.1 s no-op, 105.9 s after a chart edit, 153.9 s cold |
| Host checks: editor, native CLI, MCP and server, facades, declarations | DEL-14 | release | 6.5 s, pass |
| 21 Chromium behaviour suites and the docs tutorial | DEL-15 | release, for the suites a batch can affect | 295.7 s, pass |
| Data integrity at the author's precision; cross-style values | DEL-16, DEL-52, DEL-80 | release | 79.1 s, pass; not built |
| Theme-switch proof over every builtin | DEL-17 | release | Fails after 1.7 s; covers 3 of 5 builtins |
| Docs match the code; golden inputs, snippets and getting-started commands | DEL-18, DEL-60 | release | Not built |
| No-JS reading with the chart notice; hostile input; accessibility; site routes and layout | DEL-57, DEL-56, DEL-58, DEL-59 | release | Not built |
| Visual matrix; new captures; exact pixel diff against the previous `pre-prod` head | DEL-19, DEL-27, DEL-28 | release | 136.4 s, pass; not built; not built |
| Geometry oracle, WebGL smoke in three engines, motion tests, author and agent journeys | DEL-72, DEL-74, DEL-75, DEL-76 | release | Not built |
| PDF pagination fixtures | FEA-RP-43, OPN-52 | release, when a batch touches PDF export; candidate | Not built |
| Performance: part 07's release-run set; the full capture before and on each candidate | DEL-51, PRF-04, OPN-50 | release; full | 157.2 s for today's 25-budget harness, which is not the release-run set |
| Receipt and change-log entry | DEL-20, DEL-70 | release, full, candidate | Not built |

- No release run has run end to end. The measured steps add up to about 15 minutes for a batch that changes code, and that cost is paid once per batch, not per merge; the steps not built yet come on top, so the release run stays within the limit PRF-51 proposes (an upper bound of 45 minutes, set by the first release receipt) only if the waste fixes (one integration-test binary per crate, one package selection, no per-run site copy, parallel browser suites) save more than the new steps add. The fast gate's target is five seconds (DEL-81, PRF-51). (DEL-08, PRF-51)
- A flaky step is fixed at its cause, never retried until green (DEL-15).

## Prod-candidate gate and hosted CI

- The prod step re-runs the full set on the frozen candidate, as DEL-71 decides, and the candidate is cut from `pre-prod` (DEL-81, DEL-63). **Open (agents):** DEL-33 proposes `rv-gate prod candidate/<n>`: the release run plus the browser matrix (the WebKit loop, 23 suites in 356.5 s today, and a Firefox loop), site-wide motion filmstrips, every chart type, map, flow and pivot table in every style, journeys and the role matrix, connectors when the batch touches them, PDF export with qpdf and Poppler, asset upload, install paths, VSIX packaging and the VS Code extension check on Lenovo's offscreen Linux, the full performance capture, accessibility with a screen-reader pass, a fresh-agent run and the merge audit. Each also runs on `pre-prod`, in the release run for what a batch affects and at the latest in the full run before the candidate is cut, so none runs for the first time at `prod`. Adversarial reviews run per `pre-prod` batch (DEL-81); this step runs no review and only checks that each was recorded (DEL-35).
- No candidate gate has run. About 21 minutes is the sum of the release run and WebKit only; PRF-51 proposes a 45-minute limit with the full capture (open, agents); the other steps are not measured.
- **Open (agents):** OPN-51 proposes that `prod` merges only with a passing full capture of the frozen candidate's final tree, and that the full capture also runs once a day on a changed `pre-prod` head, where a failure is bisected and fixed forward by the release agent before the candidate is frozen. DEL-05 lets agents set up such a schedule themselves.
- **Open (Daniel, because it can spend money):** DEL-39 proposes that hosted CI is never a merge gate, and that every workflow that can trigger stays disabled until Daniel confirms an Actions budget that stops at $0; after that it is advisory and runs only on manual dispatch and on `pre-prod` and `prod`, because merges into `dev` take only the fast gate (DEL-39, ARC-50, DEL-81). Agents never spend, raise limits or retry billing-blocked runs.

## Batches and the review packet

All open; the agents decide the mechanics within DEL-06, DEL-68, DEL-69, DEL-71 and DEL-81.

- **Open (agents):** DEL-63 proposes two kinds of batch. A release batch is the release agent's: up to 20 merges taken from `dev` into `pre-prod` and run through the release run (DEL-81). A packet batch is a Linear milestone, closed by the chief of staff when its issues are merged or deferred and no regression is open against `pre-prod`, or when Daniel asks; closing runs the full run on `pre-prod` and cuts a frozen `candidate/<n>` from the `pre-prod` head, and `dev` and `pre-prod` keep moving.
- **Open (agents):** DEL-34 proposes the packet as a part file, `docs/lockdown/parts/pkt-<n>-<yyyy-mm-dd>.json`, with one card per design or feature change in separate sections, one deploy card, the before-and-after report up front and an appendix that needs no answer. Agents revert rejected and deferred changes on the candidate, on `pre-prod` and on `dev`, rerun the prod tests, and Daniel gives the go once every card left is accepted.
- **Open (agents):** DEL-70 proposes that the release run, never the agent that made the change, writes the feature and design log as it takes each batch into `pre-prod`, each entry with its cause (`request`, `loop:<run>` or `issue`), and that an autonomous loop is a recorded run with a principle, a limit and one Linear issue per idea.
- **Open (agents):** DEL-66 proposes that answers reach `dev` through an `answers/<date>` PR whose every answer matches `log.jsonl` and its `proposedHash`; the release run checks it.
- **Open (agents):** DEL-44 proposes that every commit and PR into `dev` names one RichView issue with `Refs S27-<n>`, that the release run's tool moves the issue to Done (`decision: none`) or To Review (a design or feature decision) when it takes the merge into `pre-prod`, and that Daniel's packet answer then sets Done on accept, Que on edit, Waiting on defer and Nah on reject after the revert.

## Enforcement

- **Decided (DEL-37, as amended by DEL-81):** a pre-push hook committed in the repository is the only technical block. It refuses pushes to `prod`, `pre-prod`, `main` and `candidate/*` from agent checkouts (AGENTS.md); DEL-37's own text named `prod` and `dev`, and DEL-81 (later) lets agents push to `dev`. Only the release agent's `rv-gate release` writes `pre-prod`, and `prod` moves only through `rv-gate merge-prod` on Daniel's go. There is no separate gate user, no machine account and no paid plan, and no GitHub login leaves the agents' environment. Agents have all the access: `.claude/settings.json` carries no deny list. Daniel accepts the occasional mistake; the audit (DEL-38) catches it after the fact. Outward actions still follow the go rules (DEL-04, DEL-40).
- **Open (agents):** DEL-38 proposes an audit, which any agent may run, before every packet and every deploy: every `pre-prod` merge since the last `prod` move has a release receipt with a matching tree, every `dev` commit names its issue and is in a released batch or waits for the next one, `prod` points only at frozen candidates with a candidate receipt and Daniel's recorded go, and live deployments match the latest deploy receipt. A fresh review agent also reads a sample of `none` merges each packet (DEL-68).
- **Open (agents):** DEL-20 and DEL-61 propose that every release run and every candidate run writes a receipt, and a fast-gate run leaves none (DEL-21); receipts are never edited, kept for the repository's life beside content-addressed captures in an evidence store on Claw Mini, with a daily off-host copy; a destination that costs money needs Daniel's go. Per Daniel's request of 27 Oct, the gate deletes its temp folders after every run, and the gate and the rewrite worktrees share one Rust target folder through `CARGO_TARGET_DIR` (DEL-22), and run folders and logs are kept only for each open PR's latest run plus the latest full and prod runs; receipts stay.

## One-time setup

The order and method are the agents' call (DEL-07, superseded where DEL-37 and DEL-81 differ). The steps below are the proposals; the ones that touch `main`, origin settings, Cloudflare or tokens wait for Daniel's go, as listed under [Outward actions waiting for Daniel's go](#outward-actions-waiting-for-daniels-go).

- Retire the October automation (OPN-47) and disable the registered workflows (DEL-39).
- Merge PR #69 into `main` (OPN-13), rename `main` to `prod`, create `dev` and make it the default (DEL-02), then rename `dev` to `pre-prod` and create the new default `dev` from it (DEL-81). The renames are done (see above).
- Preserve unpushed work in task worktrees, remove the worktrees, and fast-forward the primary checkout and keep it on `dev` (OPN-15).
- Make the lockdown, the guide, the lockdown app and the new `AGENTS.md` the first change on `dev`, after the private-data pass; remove the stale agent docs and agent definitions there (DEL-45, OPN-30, OPN-31, OPN-33).
- Create the evidence store and install Firefox, qpdf and Poppler on Claw Mini (DEL-61, DEL-33).
- Land the gate commands (`rv-gate fast`, `release`, `full`, `candidate`, `prod` and `merge-prod`), their registry, receipts, tiers, visual baselines, the pre-push hook, the instruction files (`AGENTS.md`, with `CLAUDE.md` importing it) and the app's evidence and packet views; agent settings carry no deny list (DEL-08, DEL-31, DEL-37, DEL-46). Retire `rv-gate dev <pr>` and `rv-gate pre-pr`, the per-PR gate that DEL-81 replaced, rename the registry's `pre-pr` and `integration` tiers to the fast, release and full tiers, and change the `captures` step's baseline from the `dev` head to the previous `pre-prod` head (DEL-81, DEL-28).
- Put Cloudflare Access in front of the workers.dev address before `dev` first deploys there (OPN-44).
- Re-measure the fast gate after the waste fixes on Claw Mini and Lenovo, against DEL-81's five seconds and PRF-51's 30-second maximum (DEL-24, DEL-50), reconcile Linear (OPN-41), then start the rewrite.

## Records kept here

Fast-tier measurements per host, taken when the tiers are first set and after every tier change (DEL-50, open). The figures are for the whole-workspace run with slow tests ignored; the fast gate's own run with impact tracing, the release run and the full run have not been measured. [EVIDENCE.md](EVIDENCE.md) holds the full tier table.

| Host | Date | Warm | After a one-file edit | Cold | 1-minute load |
| --- | --- | --- | --- | --- | --- |
| Claw Mini, Apple M4, macOS 26.3 | 2026-10-22, `b4db233`, before the waste fixes | 10.6 s median of 3 | 57.7 s, after a `touch` that left the content unchanged, so a lower bound | 74.6 s | 2.6–3.8 |
| Lenovo | — | not measured | not measured | not measured | — |

Standing gaps, listed in every packet appendix; any other step that cannot run fails the candidate (DEL-71):

- The VoiceOver pass over the every-block fixture, until an agent can drive VoiceOver reliably (DEL-58, open).

Mutation exceptions, for behaviours only a slower tier catches: none is named yet (DEL-26, open).
