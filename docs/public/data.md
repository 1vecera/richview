---
title: Data and connectors
description: Local observations, native queries, cached results and integrity-pinned storage.
styles: [scientific]
---

Keep small observations in the block. Keep larger inputs in files beside the report. The same file can feed several charts and tables; paths are relative to the `.rmd`, never to the shell's working directory.

## Choose one source per block

This is the CSV variant of the tutorial chart:

```yaml chart-data context=examples/hello-world
data:
  csv: data/visits.csv
```

Use `data.json` for a JSON array of flat row objects. Inline, CSV and JSON cells may be text, finite numbers, booleans or null. Nested objects and arrays are not table cells. The [data contract](https://richview.ai/docs/reference#report-folders-local-data-and-output) defines encoding, quoting, trimming, date precision and limits in full.

CSV uses UTF-8, commas and a header row with unique, nonempty names. Empty cells and recognized missing tokens remain missing observations. Leading-zero identifiers remain text. Long decimals and integers that cannot round-trip through binary64 remain source text; chart coordinates still use binary64. Tables and source downloads retain their digits. JSON cannot recover precision its producer already lost.

A source or one resolved dataset is limited to 16 MiB; complete generated HTML is limited to 64 MiB, including runtime, fonts and media. Reduce the input outside RichView when you need a smaller analytical result. Preserve what you filtered or aggregated in the source note.

The accepted `data.url` field is not loaded by the native or browser hosts. Download remote data into the report folder. Paths and symlinks cannot escape that folder.

## Query through a native connector

The native host also resolves SQL, MetricFlow and cached results. A report names its connector in `report.connectors.yaml`; a trusted host configuration binds that name to a runtime and data source. Credentials belong to that configuration, never to the report.

On Linux, reports with SQL data sources need kernel 6.2 or newer with Landlock ABI 3 enabled. Debian 12’s default 6.1 kernel and Ubuntu 22.04’s original 5.15 kernel are below this requirement; use a newer kernel for SQL. Reports without SQL need glibc 2.35 or newer and do not have this additional kernel requirement.

### Work through a local DuckDB query

Download these files into a dedicated `sql-report/` folder, preserving their paths: [report.rmd](https://richview.ai/docs/examples/sql/report.rmd), [report.connectors.yaml](https://richview.ai/docs/examples/sql/report.connectors.yaml), [data/seed.sql](https://richview.ai/docs/examples/sql/data/seed.sql) and [queries/orders-by-month.sql](https://richview.ai/docs/examples/sql/queries/orders-by-month.sql). The seed contains fictional monthly orders.

The report-local declaration is:

```yaml config
connectors:
  local-duckdb:
    kind: duckdb
    database: data/warehouse.duckdb
```

The chart names that connector and the query file:

```yaml config
data:
  sql:
    connector: local-duckdb
    query: queries/orders-by-month.sql
```

Install [DuckDB 1.5.6](https://duckdb.org/docs/installation/) for this example. The bundled reader uses the 1.5 storage format; a database created by DuckDB 2.0 is rejected. Create the database before rendering:

```sh
cd sql-report
duckdb data/warehouse.duckdb < data/seed.sql
richview generate report.rmd --format json
```

A successful render reports `data.provenance` with three rows from `local-duckdb` via SQL. The HTML carries the resolved rows, not the database or a live connection.

The rows are captured at the first render and reused. A later render shows the same rows until the query text changes or the capture is older than its `cache.ttl`, one hour in this example. To query on every render, set `cache: { refresh: true }` under `sql`. A `cache.freshness` timestamp declares the capture stale from that time on, and renders then warn `data.cache.stale`. The reader sees when the rows were captured, and how many there are, in the chart's Data source note.

A missing database currently appears as an unregistered connector; check the declared database path first.

### Connect an existing MetricFlow project

MetricFlow requires your own configured semantic project, warehouse access and compatible `mf` launcher. This is configuration guidance, not a standalone runnable demo. The report only declares a runtime handle:

```yaml config
connectors:
  analytics:
    kind: metricflow
    runtime: analytics-local
```

Keep the trusted host configuration outside the report folder, for example as `connectors.toml`:

```text config
[metricflow.analytics-local]
launcher = ["mf"]
project_dir = "/absolute/path/to/your/semantic-project"
```

A report's `data.metricflow` names `connector: analytics` and the metrics defined by that project. Run it with `richview generate your-report/report.rmd --connectors /absolute/path/to/connectors.toml`, or set `RICHVIEW_CONNECTORS` to that configuration file. Keep credentials in the launch environment. Browser-only drafts do not run warehouse queries.

The [generated chart fields](https://richview.ai/docs/reference#charts) define the connector and cache shapes. A missing connector declaration is a resource-resolution failure. A successful query is still data to review: RichView does not establish that the query answers your business question.


For integrity-pinned uploads, read [asset storage](https://richview.ai/docs/storage). Reports still read their chart data and media from local files.
