# RichView lockdown

This folder holds the lockdown for the RichView rewrite: what RichView is for, what it maximizes, every feature, component, type and style it keeps, how agents deliver changes, and the evidence that proves each outcome. Daniel reviews it through the local lockdown app (`tools/lockdown-app/`), part by part. His answers are the source of truth; everything else here is evidence or proposal until he accepts it.

## Layout

| Path | Content |
| --- | --- |
| `evidence/<source>.md` | Mined evidence, one file per source (code, format, styles, history/PRs, Linear, Notion/site, checks/performance). Each claim cites its source. |
| `parts/NN-<slug>.json` | One question batch per part, with proposed answers and IDs prefilled from evidence. The app lists every file here. |
| `answers/NN-<slug>.json` | Daniel's answers, written only by the app when Daniel answers. |
| `answers/log.jsonl` | Append-only log of every answer change, written only by the app. |
| `screenshots/` | Current screenshots referenced by parts and the guide. |
| `GOALS.md`, `RULES.md`, `BOUNDARIES.md`, `EVIDENCE.md`, `DELIVERY.md` | The lockdown files, produced from accepted answers. |
| `ids.md` | Index of every entity ID (`C-`, `F-`, `T-`, `S-`, `P-`, `G-`, `D-`) with its title and part. |
| `../guide/` | The unified RichView documentation set (purpose, architecture, components, types, styles, performance, delivery, screenshots). |

The repository is `1vecera/richview` (private; transferred from `richview-universe/richview-v3` on 21 Oct 2026). Evidence was mined at `origin/main` `b4db233`; link repo sources as `https://github.com/1vecera/richview/blob/b4db233/<path>`.

## Part file format (`parts/NN-<slug>.json`)

```json
{
  "id": "01-intent",
  "order": 1,
  "title": "Purpose and what RichView maximizes",
  "status": "draft | ready",
  "summary": "Markdown: what this part settles and how to answer it.",
  "items": [
    {
      "id": "INT-01",
      "kind": "question | feature | component | type | style | rule | gate | performance | decision",
      "importance": "key | important | detail",
      "refId": "Optional entity ID this item defines or decides, e.g. F-CHART-LINE",
      "component": "Optional C- ID of the part 03 component card that owns this item, e.g. C-RICHVIEW-CHART",
      "group": "Optional heading used to group cards, e.g. a component name",
      "title": "Short title",
      "question": "Markdown: the decision Daniel makes.",
      "why": "Markdown: what the answer changes.",
      "proposed": "Markdown: the prefilled answer, written as the final statement if accepted.",
      "options": [{ "label": "Short choice", "detail": "Tradeoff" }],
      "recommendation": "Optional: the recommended option label and why.",
      "confidence": "observed | stated | inferred | unknown",
      "sources": [{ "label": "What it shows", "ref": "repo path[:line] | URL | S27-600 | PR #62 | Notion page title", "date": "YYYY-MM-DD" }],
      "conflicts": "Optional Markdown: contradicting evidence and the choice it forces.",
      "images": ["docs/lockdown/screenshots/<file>.png"]
    }
  ]
}
```

`confidence`: `observed` = verified in code or a running build; `stated` = written in a doc, PR, ticket or note but not verified; `inferred` = the proposer's reading of several sources; `unknown` = no evidence, Daniel must decide. `importance` sets the review order: `key` = shapes everything else, is hard to undo or needs Daniel's go for an outward action; `important` = a real choice with visible but local effect; `detail` = follows from accepted key items or is fine-grained. Only `ready` parts are complete enough for review; `draft` parts may still change.

`component` names the part 03 component card that owns the item, by its `C-` ID: the most specific card that implements the behaviour, such as the CLI or the live chart chrome rather than the crate that contains them. Every item in parts 04a to 04d carries one, so each component's features can be listed from the parts or the export. A branch-only feature names its PR's branch-only component card when part 03 has one, otherwise the component it would extend. The app shows the ID as a chip beside the entity ID that filters the part to that component, and the export prints `Component: <ID> <title>` with the title of the part 03 card.

### ID prefixes

IDs are stable and unique across all parts. Item IDs use the part prefix; entity IDs (`refId`) use the prefix of what they define: `C-` component, `F-` feature, `T-` type, `S-` style, `P-` performance budget, `G-` gate or rule, `D-` decision. The table lists the entity prefixes each part's items use, most frequent first.

| Part | Item prefix | Entity prefixes |
| --- | --- | --- |
| 01 Purpose and maximization | `INT-` | `D-`, `G-` |
| 02 Delivery model and gates | `DEL-` | `G-` |
| 03 Architecture and components | `ARC-` | `C-`, `D-` |
| 04a Features: charts | `FEA-CH-` | `F-`, `D-` |
| 04b Features: report rendering | `FEA-RP-` | `F-` |
| 04c Features: CLI, server, MCP, web, editor, cloud | `FEA-SF-` | `F-`, `D-` |
| 04d Features only on open PR branches | `FEA-BR-` | `F-`, `D-`, `G-`, `S-`, `P-` |
| 05 Format and types | `TYP-` | `T-`, `D-` |
| 06 Styles and design | `STY-` | `S-`, `D-` |
| 07 Performance | `PRF-` | `P-` |
| 08 Open work and conflicts | `OPN-` | `D-` |

## Answer file format (`answers/NN-<slug>.json`)

```json
{
  "partId": "01-intent",
  "answers": {
    "INT-01": {
      "decision": "accept | edit | reject | defer",
      "text": "Final text when decision is edit (otherwise the accepted proposal is implied).",
      "note": "Optional comment for the agent.",
      "updatedAt": "ISO timestamp"
    }
  }
}
```

`accept` keeps the proposal as written. `edit` replaces it with `text`. `reject` drops it from the rewrite (with `note` explaining why, when given). `defer` leaves it open. Every write also appends `{ "at", "partId", "itemId", "decision", "text", "note" }` to `answers/log.jsonl`.

Only Daniel answers. An agent records an answer only when Daniel gives it live and asks the agent to record it (allowed since 2026-10-26): it writes through the app's answer API (`tools/lockdown-app/cli.mjs answer`), never edits `answers/` files directly, never replaces an existing answer unless Daniel asks, and marks the note as dictated and recorded by the agent. Otherwise agents change proposals in `parts/` only. Commits of `answers/` record the files as the app wrote them, without edits.
