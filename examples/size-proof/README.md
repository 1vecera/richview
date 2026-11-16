# Four report size comparisons

Four coding-agent sessions each received an analysis task and a CSV derived from the same public Gapminder data used by the landing-page anchor. Each produced a standalone HTML report with a chart, factual interpretation and complete table. Each folder also contains an independently authored RichView report on the same data and the same chart-and-table brief. Their design, prose, interactivity and precision are not identical. These examples are illustrative; they are not a representative or controlled benchmark of coding agents.

`sizes.csv` records three artifacts per report, in bytes:

- **Agent HTML**: the original `agent-report.html`, with its data, chart code and styling inline.
- **RichView source and data**: `report.rmd` and `data.csv`, each counted once.
- **RichView rendered HTML**: `Rendered/report.html`, produced by the release CLI, which embeds the runtime and data.

The first two compare authoring files. The first and third compare the HTML files delivered to readers. These comparisons answer different questions: a small RichView source does not mean a small delivered HTML file. Each raw size is the sum of file lengths. Each gzip size is the sum of independently gzipped files at level 9 with zero modification time (Node `zlib.gzipSync`); it is not the size of an archive or concatenated stream. Measurements use Node v26.5.0 and zlib 1.3.2.1-motley-3246f1b; other gzip implementations can produce different sizes. `comparison/report.rmd` and its CSVs contain the measured raw sizes in two separate charts. The script regenerates the CSVs directly from the measured files.

## Data and provenance

Source for every dataset: Gapminder, life expectancy at birth, version 15 (5 April 2026), regional averages weighted by population; [gapminder.org](https://www.gapminder.org/data/), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The source snapshot is `assets/landing/anchor/data/life-expectancy-by-region.csv`, SHA-256 `fc4c21802abf26f80bbd95fa2097a58cd220c0831aa54ac6602286d4426e13b9`. This proof uses the already-prepared regional averages from the anchor; it does not independently reconstruct their population weighting. Gapminder's Asia includes Australia and the Pacific Islands, and Europe includes all of Russia and Central Asia.

| Folder | Data derived from the source snapshot |
| --- | --- |
| `01-trends` | All 280 annual region observations, 1950–2019. |
| `02-gains` | Each region's 1950 and 2019 values and their difference. |
| `03-decades` | Each region's changes between decade endpoints; the final interval is 2010–2019. |
| `04-gap` | Europe's estimate minus Africa's for each year. |

Subtractions are calculated from the one-decimal source values and saved to one decimal. Each folder's `DATA.md`, beside its CSV, records its source, licence and transformation. `prepare-data.cjs` checks the source hash and regenerates all four extracts from the anchor. The licence above applies to the third-party data; RichView's code is MIT.

Each folder records its exact `prompt.txt` and `provenance.json` with model selection, invocation, session result and file hashes. `04-gap` also records the follow-up prompt used to resume the session after shell commands were denied; that prompt asked for the chart and full table to be generated using inline JavaScript. The agent HTML is saved unchanged from the final session output. Source links are navigation only, not assets required to open the reports; all chart code, tables, styling and data are embedded. The comparison makes no assumptions about HTML size beyond these outputs.

RichView tables keep measurements numeric so they sort by value; their formatting may omit trailing decimal zeros. The agent reports have their own rounding and interaction choices; equivalence here covers observations, chart subjects, table and analysis, not identical display precision in every view.

## Reproduce

Build the release CLI from this checkout following the repository instructions, including the staged browser runtime. Then, from the repository root:

```sh
node examples/size-proof/prepare-data.cjs
for report in 01-trends 02-gains 03-decades 04-gap; do
  target/release/richview check "examples/size-proof/$report/report.rmd" --format json
  target/release/richview generate "examples/size-proof/$report/report.rmd" --format json
done
node examples/size-proof/measure.mjs
target/release/richview check examples/size-proof/comparison/report.rmd --format json
target/release/richview generate examples/size-proof/comparison/report.rmd --format json
```

`Rendered/` is generated and ignored by Git. The delivered HTML and its byte count can change with the RichView revision or build environment; record the CLI version and rebuild before comparing. Data, prompt, model, chart implementation and browser features likewise affect the result.

The checked-in measurement was produced using `richview 3.0.0-alpha.0 (commit 81d6c20b09, runtime 5c8372d18ac4)`; the release executable's SHA-256 was `1973bf6780e340aea642aad7d60804929e65600fefccb925908a240b077d6f1a`. That executable was built by the shared full run from the base revision of this example; use the identified renderer to reproduce its delivered HTML sizes.
