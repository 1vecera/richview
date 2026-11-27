---
title: Your first report
description: From an empty folder to an interactive report, then to your own data.
styles: [scientific]
---

In a few minutes you go from an empty folder to an interactive report in your browser: a chart you can hover, the table behind it and a download of its values. You need a terminal and a browser. A coding agent writes the real reports later; try the whole path yourself first, so you know what it hands you.

## Get the command

<!-- windows-preview:start -->
[Windows PowerShell](https://richview.ai/docs/install#windows) · **macOS / Linux** instructions below.
<!-- windows-preview:end -->

{{native-install}}

## Make the smallest useful report

In a fresh working folder, run:

```sh
richview init first-report
richview validate first-report/report.rmd
richview check first-report/report.rmd
richview generate first-report/report.rmd
```

Open **`first-report/Rendered/report.html`** in your browser. It is a complete interactive HTML report in Sleek, titled **Direct visits lead our traffic**, with a 910-visit metric and fictional observations in `data/visits.csv`. Hover or focus the chart, inspect its table and try a download. Keep the source beside the result: you edit `report.rmd`, then generate again.

Try changing `Direct,420` to `Direct,520` in `first-report/data/visits.csv`. The chart reads that file, but the metric is written in `first-report/report.rmd`. Change `value: 910` to `value: 1010` there. Run `richview check first-report/report.rmd` and `richview generate first-report/report.rmd` again, then reopen the HTML. The Direct bar now shows 520 visits and **Visits this week** totals 1,010.

`validate` checks syntax, schema and local resources. `check` also renders the report and inspects each chart's geometry at a phone width and at the desktop width. An info, such as a label left out for lack of room, needs no change. Each command prints text for a person; with `--format json` it prints one object for an agent or a script, in which `status: clean` means no errors or warnings and `ok: true` can still include warnings. Read those before treating the report as finished.

## Keep a preview open

```sh
richview serve first-report/report.rmd --port 4711
```

Open `http://127.0.0.1:4711/`. Save a source or data edit and the preview reloads. Stop the command with Ctrl+C. The server binds to loopback by default; this is a preview of one local report.

## Give your agent the job

Paste this into Claude Code, Codex, Cursor or another coding agent that can read your files and run the command:

```text
{{agent-prompt}}
```

Attach or name your data files and explain the question you want the report to answer. The agent writes ordinary saved Rich Markdown and data files. You can read them, put them in Git and keep working after an interrupted session. Review the interpretation as well as the chart.

## Take a finished example apart

The [orders example](https://richview.ai/docs/example/Rendered/report) uses a different fictional dataset to demonstrate a metric, table, caveat and local styling. Download its [source](https://richview.ai/docs/example/report.rmd), [style YAML](https://richview.ai/docs/example/report.rstyle.yaml) and [CSV](https://richview.ai/docs/example/data/orders.csv) into this layout:

```text
orders/
  report.rmd
  report.rstyle.yaml
  data/orders.csv
```

Keep exactly one `.rmd` in each report folder, including its subfolders. Store source backups outside that folder or use Git. Generated files go into `Rendered/` with a capital R.

Next: [use the CLI](https://richview.ai/docs/cli), [choose a style](https://richview.ai/docs/styles), or [learn the source format](https://richview.ai/docs/authoring).
