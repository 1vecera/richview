---
title: Flow chart
description: Flow chart for how a process runs step by step, with cards in the order their edges give, decisions with labelled answers, groups for what runs as one, and right-angled lines that never cross a card.
styles: [scientific]
---

Use a flow chart to show what happens in which order, where the process decides, and what runs as one job. Write the cards and the edges between them; RichView measures every card, places them and routes the lines.

```chart
type: flow
title: Nothing is written until every check passes
subtitle: A red line is a failure; the dashed line waits and asks again.
nodes:
  - {title: Nightly run starts, kind: start}
  - {id: fresh, title: "New files since the last run?", kind: decision}
  - {id: wait, title: Wait and look again, lines: "every 15 minutes, four times at most", tone: muted}
  - {id: load, title: Load the new files, lines: "parse CSV and JSON; reject rows without an id"}
  - {id: checks, title: Run the checks, lines: "row counts, ranges, duplicates"}
  - {id: failed, title: "Any check failed?", kind: decision}
  - {id: write, title: Write the tables, lines: "replace today's partition"}
  - {id: stop, title: Run fails, lines: "nothing written; the alert names the check", tone: alert}
  - {id: done, title: Run ends, kind: end}
groups:
  - {label: One job, nodes: [checks, failed, write, stop]}
edges:
  - Nightly run starts -> fresh
  - "fresh -> load: yes"
  - "fresh -> wait: no"
  - "wait ..> fresh: look again"
  - load -> checks
  - checks -> failed
  - "failed -> write: no"
  - {edge: "failed -> stop: yes", tone: alert}
  - write -> done
```

## Source

````rmd
```chart
type: flow
title: Nothing is written until every check passes
subtitle: A red line is a failure; the dashed line waits and asks again.
nodes:
  - {title: Nightly run starts, kind: start}
  - {id: fresh, title: "New files since the last run?", kind: decision}
  - {id: wait, title: Wait and look again, lines: "every 15 minutes, four times at most", tone: muted}
  - {id: load, title: Load the new files, lines: "parse CSV and JSON; reject rows without an id"}
  - {id: checks, title: Run the checks, lines: "row counts, ranges, duplicates"}
  - {id: failed, title: "Any check failed?", kind: decision}
  - {id: write, title: Write the tables, lines: "replace today's partition"}
  - {id: stop, title: Run fails, lines: "nothing written; the alert names the check", tone: alert}
  - {id: done, title: Run ends, kind: end}
groups:
  - {label: One job, nodes: [checks, failed, write, stop]}
edges:
  - Nightly run starts -> fresh
  - "fresh -> load: yes"
  - "fresh -> wait: no"
  - "wait ..> fresh: look again"
  - load -> checks
  - checks -> failed
  - "failed -> write: no"
  - {edge: "failed -> stop: yes", tone: alert}
  - write -> done
```
````

## Fields that matter

- `nodes`: the cards, in reading order. `title` is required: a verb and an object for a step, a short question for a decision. `lines` holds the two or three facts under it, one per line. `id` is what edges and groups call the card; without one it is the title in lower case with hyphens.
- `kind` sets the outline, so a reader tells cards apart without a key: `step` (the default, a rounded card), `decision` (a diamond), `start` and `end` (pills), `store` (a cylinder, for data at rest) and `object` (a sheet with a folded corner, for a file or a model a step makes).
- `edges`: one line each, `from -> to`; `from ..> to` draws it dashed, for context, feedback or a wait; `: label` names it (`yes`, `no`, `new rows`). Quote an edge or a title that holds `?`, `:` or `,` inside `{ … }`.
- `groups`: cards that run as one, in a box with its label set into the top border. A card sits in one group, and no other card lands inside the box.
- `tone`: `alert` takes the style's colour for bad news, for a failure and the path to it; `muted` greys a card outside the subject. Groups take the style's palette in order.
- `direction: right` runs the flow across the page; where it does not fit, it turns down.
- With `nodes` the chart takes no `data` or `encoding`; its Table view and CSV list the cards, then the edges.
- A flow can also come from data rows: `encoding.label` names steps, `source` and `target` connections, `lane` lanes drawn as bands and `y` the connection labels. A connection that closes a cycle there is drawn dashed and reported.
- An edge to no card, a repeated id, a card in two groups or a line that is not `from -> to` is reported at its path, and the rest of the flow draws. A flow takes at most 150 steps and 600 connections.

## How it is drawn

- Cards sit in ranks along the flow: each one after every card that leads to it. A loop back runs round the outside of the drawing.
- Lines are right-angled, never cross a card, meet each card at their own point and run straight between cards that line up. A decision answers by its corners.
- A label sits on free paper set into or beside its line; when there is none the gap grows. It never covers a card, another label or another line.
- Cards wrap their text narrower before the drawing shrinks; a flow too wide for a phone is scaled down whole. The same block gives the same layout.

## Interactions

- Hover or focus a card to read what it is, its group, its lines and the cards before and after it.
- Hover or focus a line to read the cards it joins and its label.
- Move through the cards with the keyboard in reading order, then the lines.
- Open the Table view, Download PNG or CSV, or Expand the chart.
