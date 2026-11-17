---
title: Diagnostics and repair
description: Start with the saved source and one useful diagnostic.
styles: [scientific]
---

Keep the `.rmd` and its local resources as the working state. `Rendered` can be regenerated. After an interrupted session, read the source, validate it and continue from the first hard failure:

```sh
richview validate first-report/report.rmd
richview check first-report/report.rmd
richview generate first-report/report.rmd
```

## Fix a missing data file

In the visits starter created by `richview init first-report`, change `csv: data/visits.csv` to `csv: data/missing.csv`. Run `richview validate first-report/report.rmd`. It exits with status 1 and reports:

```text diagnostic
report.rmd:21 error report.asset.missing: Asset 'data/missing.csv' is absent from the report folder. Add the file there or correct its report-relative path.
Invalid, 3 rich blocks
```

With `--format json`, the diagnostic is carried unchanged in the result:

```json
{
  "blocks": 3,
  "diagnostics": [
    {
      "code": "report.asset.missing",
      "column": 9,
      "fix": "Add the missing asset to the report folder or correct its path.",
      "level": "error",
      "line": 21,
      "message": "Asset 'data/missing.csv' is absent from the report folder. Add the file there or correct its report-relative path.",
      "path": "chart.data.csv",
      "tag": "chart"
    }
  ],
  "ok": false,
  "output": null,
  "status": "issues"
}
```

Restore `csv: data/visits.csv`, then validate again. It prints `Valid, 3 rich blocks` and exits 0; with `--format json` the result has `ok: true`, `status: clean` and an empty diagnostics array. Keep the data inside its report folder; moving it into frontmatter or pointing at a remote URL does not repair the missing file.

## Diagnose by symptom

| Symptom | Next action |
| --- | --- |
| Multiple `.rmd` sources or an escaping path | Put each report in its own folder; move source backups outside it; fix paths to bounded local files. |
| YAML/schema failure | Read the diagnostic line/path, compare the block with the copyable example, repair indentation and required fields. |
| Chart renders as code | Inspect the rich-fence diagnostics; it needs a supported type, data mapping, encoding and a title. |
| Missing styling file | Restore the referenced YAML, or remove that reference to use builtin/default styling. |
| No change after a theme edit | Save the local YAML; check the ordered styles and explicit chart palette; regenerate or refresh the preview. |
| Export says runtime missing | Install the complete native release again; its report runtime is embedded. For a source build, run the documented build command before exporting. |
| A linked download fails after sharing | Include the copied files in the entire `Rendered` folder, not only its HTML. |
| A check succeeds but a label is hard to read | Inspect warnings and actual browser glyphs; shorten/reposition labels without changing values. |
| Publish returns unauthorized | Confirm the selected server and its token; the public static website is not a native publishing server. |
| Republish shows old content | Same slug/version is immutable; use a new version for the revised source. |
| VS Code preview or diagnostics are stale | Save referenced files, check workspace trust and the packaged native service, then reopen Live Preview. |

`--format json` exposes machine-readable diagnostics and nonzero exits for hard failures. Geometry diagnostics include clipping, collisions and dropped-label provenance. Native checks estimate font bounds; desktop and mobile browser inspection adds actual font geometry. A clean check is evidence about the tested output, not a universal readability guarantee.

## Keep the agent's next step small

Give the agent the report path, the failed command and the diagnostic. Ask it to preserve observations, units, source references and caveats while fixing the source, then render again. A useful handoff contains the saved source/resources, output path, remaining warnings and the next edit; it does not need a system architecture dump.

For exact grammar, run `richview learn grammar` or read the [full source contract](https://richview.ai/docs/reference). For a known good baseline, return to [your first report](https://richview.ai/docs/first-report).

## Read a diagnostic

Each record carries a level, code, message and path, with a fix, expected value, retry guidance, file, line, column, tag or details where applicable. Locations start at 1 and include frontmatter. A named `file` makes the line relative to that data or style file; otherwise it refers to the `.rmd`. The same records travel through CLI, MCP, HTTP and editor surfaces.

In the JSON result, `status: clean` means no errors or warnings. `ok: true` means no errors and can still carry warnings. An info record may explain a deliberate choice, such as missing observations or experimental styling. The output path can identify a repairable draft even when a block failed; inspect diagnostics and the command's exit status together.

## Find a code

Search documentation for an exact code, or open the [diagnostic code reference](https://richview.ai/docs/reference/diagnostic-codes). Its table comes from the schema registry.
