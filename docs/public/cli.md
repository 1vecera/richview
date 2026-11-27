---
title: Command line
description: Validate, inspect, render and preview the same saved source.
styles: [scientific]
---

One command takes a report from source to result: it teaches your agent the format, checks what the agent wrote, lays out every chart to see that it fits, and renders the report. Each step answers in words an agent can act on.

Continue from [your first report](https://richview.ai/docs/first-report). Run these commands from the working folder that contains `first-report/`.

## Read the authoring guide

```sh
richview learn
richview learn grammar
richview learn chart-contract
richview learn style
richview learn workflow
richview learn anti-slop
richview learn slides
```

`learn` prints the guide your coding agent uses. The whole guide is long, so an agent reads it a section at a time: each section fits one tool output. Give the agent the relevant section when it needs to repair a particular block or choose chart fields.

## Check the source and the drawing

```sh
richview validate first-report/report.rmd
richview check first-report/report.rmd --chart-width 390
```

Validation finds source and resource errors. Checking also lays out every chart and reports geometry, such as clipped labels or collisions, at the phone and desktop widths readers get; `--chart-width` measures another width instead. An error or a warning needs a fix; an info records what the renderer did. Browser inspection adds the actual font measurements and interactions. Check again after the final source edit.

## Run every report in a folder

```sh
richview check first-report
```

To try a batch, create two starter reports in a new `reports/` folder:

```sh
richview init reports/q1
richview init reports/q2
richview check reports
```

`validate`, `check` and `generate` take a folder as well as a file. The folder that holds a report stands for that report, so the first command checks `first-report/report.rmd`. A folder of report folders, such as `reports/q1/` and `reports/q2/`, stands for every report below it: each report's findings and result line start with its own path, the last line counts the reports with errors, and the command exits 1 when any report has one. With `--format json` the one object holds each report's own result under `reports`.

`serve` and `publish` take one report. `richview publish first-report/report.rmd` creates a private report by default and prints its address. An address whose report was deleted is retired: nobody publishes there again, its owner included, and the server says to publish under a different address. An address that has an access record and no report, such as one an interrupted publication left, stays reserved for its owner and its editors: publishing there keeps the audience it had, and nobody else can publish there, with `--new` or without it. When the server answers that it is busy and says for how many seconds, the command waits and sends the request again, up to three times. Raise the version and run the same command to publish a revision. Use `--new` when you want the same file to create a second address on that server.

## Write another presentation

```sh
richview generate first-report/report.rmd --style air -o review.html
```

This writes `first-report/Rendered/review.html`. An output name changes the filename within `Rendered`, not the output root. `--style` supplies the complete style list for this call and replaces the source's list; it does not edit the source.

## Keep the operations straight

| You want to… | Use |
| --- | --- |
| Start a bounded report folder | `init` |
| Inspect syntax, fields and local resources | `validate` |
| Inspect chart geometry too | `check` |
| Write interactive HTML or a local PDF | `generate` |
| Preview one report with reload | `serve` |
| Connect an agent over stdio | `mcp` |
| Store immutable report versions | `server` and `publish` |
| Manage stored versions | `report` |
| Upload integrity-pinned files to configured storage | `asset` |
| Find a credited Unsplash photo | `image` |

A report command prints its findings and one result line as text. `--format json` prints one JSON object instead, for an agent or a script that reads the result.

The [command and flag reference](https://richview.ai/docs/tools) is generated from the actual CLI. Report commands exit 0 when no diagnostic is an error, 1 on a report error and 2 on a usage error. A draft with invalid rich blocks can still be written for repair; read the diagnostics and exit status. `serve`, `server` and `mcp` keep running until stopped. Asset and image commands have their own [result contracts](https://richview.ai/docs/tools).
