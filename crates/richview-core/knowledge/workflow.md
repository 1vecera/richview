### Before writing

- List the brief's quantitative questions and the population, period and units for each.
- Calculate requested comparisons and derived values from the supplied data, and show the results explicitly with clear labels in a metric, table or chart; a plotted source series alone does not state a requested calculation.
- If the brief requests several chart types, count distinct chart type fields rather than repeated charts of one type.

### The loop

- Build by editing .rmd, data and styles.
- Put an immediate emphasized *Source: ...* paragraph after every chart and data table, identifying its dataset and provenance.
- Run `richview validate report.rmd`, which checks syntax and schema, then `richview check report.rmd`, which also lays out every chart and measures its geometry, including clipping, collisions and omitted-label provenance.
- Fix every error and warning, revise the source, and preview again. An info records what the renderer did and needs no change.
- After the final source or data edit, run check again on that final version; an earlier passing check does not validate a later revision.
- Preview the actual rendered report: `richview generate report.rmd` writes `Rendered/report.html`, and `richview serve report.rmd` previews it in a browser and reloads on every save.
- Compare the final rendered report with the brief, including every requested quantity, population, period, unit and distinct chart type.

### What the commands return

- **Levels.** An error means a block or the report did not render as written. A warning means it rendered but something is probably wrong. An info is a record. `ok` is true when nothing is an error; `status` is `issues` when anything is an error or a warning, and `clean` otherwise, so a clean report can still list infos.
- **JSON.** With --format json, and from the MCP render, validate, check and publish tools, a command returns one object: ok is true when nothing is an error, status is clean or issues, output is the file written or null, and diagnostics lists {level, code, message, fix?, expected?, retry?, path, file?, line?, column?, tag?, details?}. Lines and columns count from 1, in the data or style file that file names when it is set, else in the .rmd. `check` adds `charts`, one entry per chart with its type, measured size and a one-sentence summary of what it shows.
- **A folder.** validate, check and generate also take a folder. The folder that holds a report stands for that report. A folder of report folders stands for every report below it: the one object then has `reports`, each report's own object with the `input` it read, `ok` and `status` speak for all of them, and the top-level `diagnostics` stays empty. serve and publish take one report.
- **For people.** Without --format json a command prints its findings on stderr, one per line, as `file:line level code: message`; an error is followed by its fix on an indented `fix:` line.
- **Exit codes.** The CLI exits 0 when ok, 1 on any error (a draft with block errors is still written) and 2 on a usage error. learn prints text and serve, server and mcp run until stopped, so they return no such object; asset and image print their own JSON and exit 2 on invalid input or configuration, 3 when a transfer or Unsplash fails and, for asset, 4 on a verification mismatch.

### Options worth knowing

- `--format json` on init, validate, check, generate and publish.
- `--style <name or file>` on check and generate: render with another style without editing the source.
- `-o <file>` on generate: the output's file name inside `Rendered/`; a name ending in .pdf prints a PDF.
- `--to pdf` on generate: a PDF printed by a local Chromium, Chrome or Edge.
- `--chart-width <px>` on check: measure charts at that width instead of the phone and desktop layouts; repeatable.
- `richview <command> --help` lists the rest.

### SQL and other connector queries

A chart or table can read a database when it renders. The report names a connector, and a `report.connectors.yaml` beside the `.rmd` declares it:

```yaml
connectors:
  shop:
    kind: duckdb
    database: data/shop.duckdb
```

```chart
type: bar
title: The app sells the most orders
data:
  sql:
    connector: shop
    query: queries/orders-by-channel.sql
encoding: { x: channel, y: orders }
```

- **The query** is a file in the report folder (`query: queries/name.sql`) or the SQL itself (`inline: "select channel, count(*) as orders from orders group by channel"`). Give one of the two. The result's column names are the names `encoding` and `columns` use.
- **The database** is a file inside the report folder, such as `data/shop.duckdb`. A path outside the folder, an absolute path or a symlink out of it is refused; copy or hard-link the file in. The reader opens databases in DuckDB's 1.5 storage format.
- **Results are captured once and reused.** The first render runs the query and stores the rows; later renders reuse them until the query text changes, so a report rendered again after the data changed shows the captured rows. To change that, add `cache` under `sql`:
  - `cache: { refresh: true }` runs the query on every render.
  - `cache: { ttl: 1h }` runs it again once the capture is older than that (`30s`, `5m`, `1h`).
  - `cache: { freshness: "2026-12-01T00:00:00Z" }` declares the capture stale from that time on: renders then warn `data.cache.stale`.
  - The top-level `data.cache` is something else: a reference to a stored result by its key.
- **What the reader sees.** Each connector chart carries a Data source note with the connector, the query fingerprint, when the rows were captured and how many there are. `check` reports the same as the info `data.provenance`.
- **Other hosts.** `kind: metricflow` runs MetricFlow queries (`data.metricflow`) through a runtime the host binds in a trusted `connectors.toml`, passed with `--connectors` or `RICHVIEW_CONNECTORS`. A local DuckDB file needs no such file.
- A metric's `value` is written in the source; it does not read a query.

### Other surfaces

- VS Code offers inline chart/table insertion, live semantic diagnostics, safe optional formatting, schema/value/data-field completion and preview; the web surface is a simple source/style playground.
- CLI mirrors MCP tools richview_render, richview_validate, richview_check, richview_publish and richview_learn; richview image search|use mirrors richview_image_search and richview_image_use (UNSPLASH_ACCESS_KEY from the environment).
- A server's /health answers {ok: true}, and its governance routes return their own records.

### PDF

Local PDF export uses richview generate --to pdf (or MCP richview_render with output: pdf), an installed Chromium, Google Chrome or Microsoft Edge on Linux, macOS or Windows (Edge comes with Windows), A4 or Letter, settled charts and embedded local assets. Sleek prints as air; all table rows remain present and media prints as posters or link cards. Browser requests outside the staged report, embedded data and the credited images.unsplash.com photos the report names are blocked.

### Publishing

Publication remains separate from local preparation.

- When asked to publish, run `richview publish <report>` or call `richview_publish` with `inputPath`. A first publish creates the report, private by default, and returns its address. Keep the slug and raise the version for a revision. A file that already published from this computer to that server needs `--new` (MCP `new: true`) only to create another address.
- Publishing stores immutable slug/version artifacts, source and resolved styles/data for regeneration.
- Publishing the same slug and version again with identical source, styles and data succeeds without storing anything; changed content is publish.version-exists, so raise the version.
- A server quota refuses a publication as publish.quota.
- The person signs in once with richview login; the CLI and richview_publish then use that sign-in, which goes only to its own server. Run from your shell, richview login --server <url> prints a short code and a page: give both to the person and wait while they confirm in their browser. An unattended host sets RICHVIEW_SERVER and RICHVIEW_TOKEN instead. Never ask for, print or store a token yourself.
- The report page serves the highest release version, not the latest publication, unless the owner pins one.
