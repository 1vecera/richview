# RichView guide

For the current implementation, see [How RichView works](../how-it-works/README.md).

This guide describes RichView as it stood before the rewrite, `main` at `b4db233` (the merge of PR #61 on 16 October 2026) plus the live sites on 21 and 22 October 2026, and the lockdown answers and proposals that carry it into the rewrite. It answers Daniel's request for "a unified set of documents which let me understand performance, styling, feature set of RichView, with screenshots ... what is the goal of RichView, what features each component has, how it should work, how it should look, what are we maximizing, where do we want to go beyond what anyone has ever made" (Daniel, 2026-10-21 brief). The guide explains; the [lockdown parts](../lockdown/README.md) decide. By 27 October 2026 Daniel had answered 77 of the 769 cards, 70 of the 134 key cards among them. A page binds the rewrite only where it quotes his answer, and everything else it says about the rewrite is a proposal until he answers the card it cites.

## Pages

| Page | What it explains | Decided in |
| --- | --- | --- |
| [What RichView is for](purpose.md) | Definition, audience and readers, the five jobs, the reference use case and the flagship, what RichView maximizes (craft and trust in the output, with storytelling left to the author), the bets beyond existing tools including agent-native authoring, non-goals, the product family and its servers, open source, releases, launch and how success is judged | Part 01 (`INT-`) |
| [Architecture](architecture.md) | Layers, crates and their dependencies, where behaviour lives, safety boundaries, repository and deployment topology | Part 03 (`ARC-`) |
| Components: [Chart engine](components/chart.md), [Report core](components/core.md), [Native app](components/app.md), [Editor](components/editor.md), [Web](components/web.md), [Cloud and hosts](components/cloud.md), [Build](components/build.md) | One page per component: what it does, how it works, how it looks, its features, the checks that cover it, known defects and branch-only additions | Parts 03 and 04a to 04d (`ARC-`, `FEA-`) |
| [Format and types](format-and-types.md) | Rich Markdown, frontmatter, blocks, data, diagnostics and the agent-facing contracts | Part 05 (`TYP-`) |
| [Styles](styles.md) | The builtin styles, the style cascade, type, colour, layout and motion | Part 06 (`STY-`) |
| [Performance](performance.md) | Budgets, measurements and how to reproduce them | Part 07 (`PRF-`) |
| [Delivery](delivery.md) | `dev`, `pre-prod` and `prod`, the fast gate and the release run, evidence, the agents that build and review, and Daniel's go | Part 02 (`DEL-`) |
| [Screenshots](screenshots.md) | A visual tour of `main`, richview.ai and news.richview.ai on 21 October 2026 | All parts |

Part 08 (`OPN-`: open pull requests, live state, one-off actions and conflicts) is cited on every page where it applies. The lockdown files record the result: [GOALS](../lockdown/GOALS.md), [RULES](../lockdown/RULES.md), [BOUNDARIES](../lockdown/BOUNDARIES.md), [EVIDENCE](../lockdown/EVIDENCE.md) and [DELIVERY](../lockdown/DELIVERY.md), each regenerated on 27 October 2026 from Daniel's answers and the current proposals; the app lists them after these pages, with [Entity IDs](../lockdown/ids.md), the index of every entity ID and the card that defines it.

Read [What RichView is for](purpose.md) and [Delivery](delivery.md) first: they frame every later part, and the app asks for Parts 01 and 02 first. Then read [Architecture](architecture.md) and the component pages, [Format and types](format-and-types.md), [Styles](styles.md) and [Performance](performance.md), each before answering its part. The screenshots page works at any point.

## Reading the guide beside the lockdown app

The lockdown app (`node tools/lockdown-app/server.mjs`, then http://127.0.0.1:4790/) lists the parts to answer and, below them in the left rail, every page of this guide. A page opens at `#/guide/docs/guide/<file>`, for example http://127.0.0.1:4790/#/guide/docs/guide/purpose.md. Read a page there, then answer its part. Links between guide pages stay in the app; a guide page opened from a card opens in a new tab, so the review keeps its place.

Every statement that depends on a decision names its card: the item ID first, then in parentheses the entity the card defines or decides, as in INT-17 (D-MAXIMIZE). To reach a card, open its part and press `/` to search for the ID, or go to `#/part/<part>/<item>`, for example http://127.0.0.1:4790/#/part/01-intent/INT-17. Part ids are the part file names: `01-intent`, `02-delivery`, `03-architecture`, `04a-features-chart`, `04b-features-report`, `04c-features-surfaces`, `04d-features-branches`, `05-types`, `06-styles`, `07-performance` and `08-open-work`. The [lockdown README](../lockdown/README.md) lists every ID prefix and the answer format.

The rail shows each part's current status. A `ready` part is complete enough to review and a `draft` part can still change, but an agent can revise a ready part too, and a page quotes statuses only as of the day it was written. When a proposal changes after Daniel has answered it, the app marks the answer as changed and the export flags it as stale.

In the app, a code span that names a repository path, such as `crates/richview-core/src/style.rs:16-43`, opens that line on GitHub at `b4db233`; the repository `1vecera/richview` is private, so the link needs access. `PR #N` opens the pull request and `S27-N` or `DIR-N` the Linear issue. Guide pages and the lockdown README, GOALS, RULES, BOUNDARIES, EVIDENCE, DELIVERY and ids files open in the app, and images from the lockdown screenshots and this guide show inline, full size on click. Other lockdown files do not open from the app: the evidence files, part files, screenshot index and probe inputs exist only in the lockdown worktree, uncommitted, and the app sends links to them to a `lockdown/richview-20261021` branch that was not on GitHub on 22 October 2026, so such links return 404. The guide therefore names these files without linking them; open them in the worktree under docs/lockdown.

Daniel's answers of 26 and 27 October 2026 are carried into the guide: a page quotes an answer as "Daniel's answer" with its date where it discusses the answered card, and states a changed decision in its prose. Answers he gives later reach the guide only when an agent next updates it, so the app and its export (`/api/export.md`) remain the live record, and agents read answers from the export, not from these pages.

## How the guide states things

- **Today**: what exists, verified in the code at `b4db233`, in a local release build of that commit or on the live sites, with its source and date. On 21 and 22 October 2026 richview.ai served the unmerged PR #69 branch and news.richview.ai the `publish/news` branch, which shares draft PR #70's earlier history, so neither matched `main`; Daniel's OPN-01 (D-PROD-BASELINE) and OPN-13 answers merge #69 so that `main` records production, and `main` was then renamed `prod` (DEL-02).
- **Re-run**: a fact re-checked for the page on 22 October 2026, by running the release build of `b4db233` (`target/release/richview`) or by a read-only request to GitHub or a live site.
- **Stated**: written in a ticket, brief, pull request or note but not verified. Linear descriptions are the coordinator's record of Daniel's dictation; only words quoted as his are his.
- **Answered**: a decision Daniel recorded in the lockdown app. The page quotes it as "Daniel's answer" with its date, or as "accepted by Daniel as proposed" when he accepted the proposal unchanged, and the quote binds. Where a later answer changes an earlier one, the later one wins and both IDs are named.
- **Proposed**: a lockdown proposal awaiting Daniel, always with its item and entity ID, set off as a quote or in a table column marked Proposed. A page quotes the card's current text, so a proposal an agent has revised since the guide was written appears as revised.
- **Inferred**: a proposal whose card has the confidence `inferred`, the proposer's reading of several sources rather than a position Daniel stated or a fact the code shows. The app shows every card's confidence: observed, stated, inferred or unknown.
- **Dates** are the day a source was written or the day it was checked. These pages were written on 22 October 2026, when no part had a recorded answer, and updated on 27 October 2026 for the answers recorded on 26 and 27 October.

## Sources

- Mined evidence, one file per source, in the worktree's docs/lockdown/evidence folder: `product.md`, `format-types.md`, `styles-design.md`, `history-prs.md`, `linear.md`, `intent-notion-site.md` and `checks-performance.md`. Evidence can be wrong; each page says what it re-verified.
- The part files in docs/lockdown/parts, one question batch per part, with proposals, options, conflicts and sources; the app shows the same content as cards.
- The screenshot index, docs/lockdown/screenshots/INDEX.md, which gives the URL or command, viewport, commit and method of most captures. It lists every committed capture, the local-only set and, under [Gaps](../lockdown/screenshots/INDEX.md#gaps), what was not captured. The [screenshots page](screenshots.md) captions give the source of every image they show. `product/` shows `main` at `b4db233` from a local release build and `live/` the public sites; `styles-design/`, `report-rendering/`, `surfaces/`, `delivery/` and `open-work/` hold renders and captures that other lockdown stages made from the same commit or the live sites, and `branches/` holds captures of open pull request branches.
