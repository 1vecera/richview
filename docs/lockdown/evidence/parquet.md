# Parquet data files: measurements

Evidence for TYP-102 (T-DATA-PARQUET) and TYP-103 (D-TYP-STATISTIC-PAYLOAD), measured on 18 and 19 November 2026. Sizes are the whole self-contained HTML file unless a row says otherwise.

## What a chart over every row costs today

Measured with the release CLI of `dev` at `b40724c8` on Claw Mini (Apple M-series, macOS), on one synthetic CSV of 100,000 orders in 16 columns, 10,496,341 B. Each report holds the charts named and nothing else.

| Report | HTML | Chart payload | Render |
| --- | --- | --- | --- |
| No chart | 1,774,495 B | none | 0.06 s |
| One density | 2,615,401 B | 0.84 MB | 1.17 s |
| One histogram, 30 bins | 3,085,594 B | 1.31 MB | 1.04 s |
| One hexbin | 3,273,678 B | 1.50 MB | 0.86 s |
| One box plot by region | 4,856,207 B | 3.07 MB | 1.19 s |
| One scatter | 24,236,751 B | 22.46 MB | 1.47 s |
| Eight charts: two histograms, scatter, box plot, violin, density, ECDF, hexbin | 34,240,035 B | 32.5 MB | 6.70 s |

Where a payload's bytes go, from the histogram, box plot and scatter payloads:

| Part | Histogram | Box plot | Scatter |
| --- | --- | --- | --- |
| `rows`: each observation's grid position | 0.71 MB | 1.51 MB | 1.20 MB |
| `shown`: each observation's value as shown | 0.59 MB | 0.47 MB | 1.04 MB |
| `text`: readouts | none per row | 1.08 MB for 7,478 marks | 20.21 MB, about 200 B a point |

- A chart embeds every observation it draws, about 13 B a row for a histogram, although the histogram's picture is its 30 bins.
- Report size follows the rows each chart draws. The data file's format changes none of these numbers: unreferenced columns already stay out of the HTML (TYP-36).
- The scatter's readout text is the readout-numbers payload work, not TYP-102 or TYP-103.

## The example report: one million rows, twelve charts and a table

`examples/parquet-report` reads `data/orders.parquet`: 1,000,000 synthetic orders in 16 columns, written by `data/seed.sql` with the bundled DuckDB 1.5.6 in about 2 s.

| The same million rows as | Size |
| --- | --- |
| Parquet, Zstandard, as the seed writes it | 6,206,305 B |
| CSV | 96,867,460 B, over the 16 MiB dataset limit, so no block can read it |

Measured on 19 November 2026 on Linux x86_64 (Lenovo, 20 cores, under the night's build load) with the release build of this branch at `720ba346`, `richview 3.0.0-alpha.3 (commit 720ba346c8, runtime 29091219ae76)`, on the example as committed here, in the Sleek style. The commits after that build change which files the reader accepts and the example's data, not how a report is rendered. Each row is one `richview generate`; times are wall time of the whole command.

| Report | HTML | Render | Result |
| --- | --- | --- | --- |
| Before: the same 13 charts and table from 14 CSV files an author exports by hand, one per chart | 1,904,256 B | 0.04 to 0.05 s | clean |
| After: 12 charts and the table by SQL over the one Parquet file, one chart from `data.parquet`, first render | 1,908,346 B | 1.03 s | clean |
| After, rendered again: the queries' rows come from the cache | 1,908,346 B | 0.07 to 0.08 s | clean |
| One histogram that reads all 1,000,000 values of one column through `data.parquet` | 16,337,313 B | 5.9 to 7.5 s | clean |

- The report over the Parquet file is the size of the report over hand-made CSV exports, 4,090 B apart: both embed each chart's few aggregated rows. What changes is the folder: one 6.2 MB file and thirteen queries in place of fourteen exports that someone keeps in step.
- The first render runs thirteen queries, each in its own confined process, in about a second. A file written again is queried again, because its size and write time are part of what the cache is keyed by.
- One all-rows statistic chart over the million-row file is 16.3 MB of HTML, about 14 MB of it the chart's payload. That is TYP-36 at work, and it is the case TYP-103 proposes to change.
- In the five builtin styles the example is between 1,895,042 B (Air) and 2,000,522 B (Hand-drawn).

## The worker's size

`richview-sql-worker` links DuckDB; the `parquet` Cargo feature adds DuckDB's own Parquet reader and its compression libraries.

| Linux x86_64 release build | `richview-sql-worker` | `richview` |
| --- | --- | --- |
| Without the feature, four other branches built the same night | 41,674,976 B | 23,126,704 B |
| With the feature, this branch at `720ba346` | 44,843,640 B | 23,176,232 B |
| Growth | 3,168,664 B, 7.6 % | 49,528 B, 0.2 % |

The macOS and Windows workers were not measured: no release build of this branch ran on either. The third-party notices do not change: `scripts/third-party-notices.mjs` already lists the licences of DuckDB's `parquet`, `thrift`, `snappy`, `zstd`, `lz4`, `brotli` and `miniz` sources, and `Cargo.lock` gains no crate.
