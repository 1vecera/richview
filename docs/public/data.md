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

## Read a Parquet file

A large or wide export can stay one file. Name it as you would a CSV:

```yaml config
data:
  parquet: data/orders.parquet
```

- **Only the columns you use are read.** RichView reads the columns the report's charts and tables name in that file, once per render, so a file with 200 columns costs what its used columns cost. A table without `columns` reads every column.
- **Every row, never a sample.** One file gives a report at most 1,000,000 rows, and the columns read stay within the 16 MiB dataset limit; a breach is a `data.limit` error.
- **Parquet's types decide each value.** Numbers stay numbers, a decimal is read from its own digits, and text stays text, so an identifier such as `00123` or `123` stored as text never becomes a number. Dates and timestamps become ISO 8601 text. NaN and infinite values are read as missing, with a `data.parquet.non-finite` warning that names the column.
- **Columns must hold single values.** A list, struct, map or binary column that a block reads is a `data.parquet` error naming the column; one no block reads is never read.
- **The file is read by the `richview` command and RichView servers**, in the same confined process that runs SQL, which can read that one file and nothing else. The playground does not read Parquet. On Linux this needs kernel 6.2 or newer, as SQL sources do.

A chart embeds every row it draws, so a chart over a million rows makes a large report. To keep a report with many charts small, give each chart an aggregate of the file with SQL, as the next section shows.

The accepted `data.url` field is not loaded by the native or browser hosts. Download remote data into the report folder. Paths and symlinks cannot escape that folder.

## Query through a native connector

The native host also resolves SQL, MetricFlow and cached results. A report names its connector in `report.connectors.yaml`; a trusted host configuration binds that name to a runtime and data source. Credentials belong to that configuration, never to the report.

On Linux, SQL queries and Parquet files need kernel 6.2 or newer with Landlock ABI 3 enabled. Debian 12’s default 6.1 kernel and Ubuntu 22.04’s original 5.15 kernel are below this requirement. Reports using neither SQL nor Parquet need glibc 2.35 or newer and have no additional kernel requirement.
<!-- windows-preview:start -->

On Windows, the SQL worker is confined as on Linux and macOS, and it reads a copy. Each render that runs a query first copies the declared database into a new folder in your temporary folder that only that render's worker may open, and deletes the copy when the render ends. A database over 4 GiB is refused, as is one whose copy would leave less than 1 GiB free in the temporary folder. Rows served from the cache need no copy.
<!-- windows-preview:end -->

The worker reads the database file alone, so a DuckDB database whose last changes are still in the `.wal` file beside it is refused: close the program writing it, or run `CHECKPOINT` in it, then render again.

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
richview generate report.rmd
```

A successful render reports `data.provenance` with three rows from `local-duckdb` via SQL. The HTML carries the resolved rows, not the database or a live connection.

The rows are captured at the first render and reused. A later render shows the same rows until the query text changes or the capture is older than its `cache.ttl`, one hour in this example. To query on every render, set `cache: { refresh: true }` under `sql`. A `cache.freshness` timestamp declares the capture stale from that time on, and renders then warn `data.cache.stale`. The reader sees when the rows were captured, and how many there are, in the chart's Data source note.

A missing database currently appears as an unregistered connector; check the declared database path first.

### Aggregate a Parquet file with SQL

A connector of kind `parquet` names a Parquet file in the report folder. Report SQL reads the file as the table named after the connector:

```yaml config
connectors:
  orders:
    kind: parquet
    file: data/orders.parquet
```

```yaml config
data:
  sql:
    connector: orders
    inline: SELECT region, sum(revenue) AS revenue FROM orders GROUP BY region
```

The query runs in the confined worker, which reads that file alone; it cannot read other files or reach the network. Each chart embeds its aggregated rows, never the file. In the repository, `examples/parquet-report` draws twelve charts and a table from one file of a million rows this way, and its `data/seed.sql` writes the file. The rows are captured and reused as for any SQL source, and a file written again is queried again.

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
