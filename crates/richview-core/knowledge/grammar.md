Start a first report:

- Run `richview init my-report` to create a new report folder with an example and its data. Choose a new folder name if the starter files already exist; init never overwrites them.
- Edit `my-report/report.rmd` and the files in `my-report/data/` for your report.
- Run `richview validate my-report/report.rmd` and `richview check my-report/report.rmd`; fix the findings and run `check` again after your final edit.
- Run `richview generate my-report/report.rmd`, then open `my-report/Rendered/report.html` and inspect the result.

If your shell cannot find `richview` and you installed it in the default location, use `"$HOME/.richview/bin/richview"` in place of `richview`. For a custom install, use the executable at the chosen location.

This guide is long. `richview learn <section>` prints one section: grammar, chart-contract, style, workflow or anti-slop. If your tool cuts long output, read it section by section or page it, for example `richview learn chart-contract | sed -n '1,150p'`; every key has its own line.

### A report is Markdown with tagged blocks

Rich Markdown is CommonMark with YAML frontmatter and rich YAML fences: @TAGS@. A complete small report:

````text
---
title: Direct visits lead our traffic
description: Where this week's 910 visits came from.
version: "0.0.1"
styles: [air]
---

Direct visits brought the most traffic this week, ahead of search.

```metric
value: 910
label: Visits this week
delta: 0.12
delta_label: on last week
```

```chart
type: bar
title: Direct visits lead, search follows
data: { csv: data/visits.csv }
encoding: { x: source, y: visits }
axes:
  y: { label: Visits }
```

*Source: Fictional example data in data/visits.csv.*

```callout
kind: neutral
title: Next
body: Search is the channel to watch.
```
````

The metric prints `910` and `+12.0% on last week`: a `delta` is a fraction, never a percentage.

### Rules that catch first-time authors

- **One folder per report.** A folder holds exactly one `.rmd`, with its data and style files inside it; paths are relative to the `.rmd`, and local files must stay inside the folder. A folder that contains another report folder is refused (`report.root.shared`), so keep reports side by side, never nested.
- **Quote YAML text that YAML would read as something else.** A value that contains a colon followed by a space, or starts with a symbol such as `%`, `*`, `&`, `@`, `[` or `{`, needs quotes: `label: "Gross margin (Q2: 67.5%)"`, `unit: "%"`. Unquoted, it is a `block.yaml` error, or `frontmatter.yaml` in the frontmatter. Quote text that looks like a number too: `version: "1.0.0"`, `tags: ["2025"]`.
- **State the source.** Put an emphasised paragraph such as `*Source: Orders database, May 2025 to October 2026.*` directly after every chart and every `table` block. Without it `check` warns `block.source`.
- **One row per observation.** A chart reads one column per channel. Two series are two sets of rows told apart by a `color` column, not two value columns: reshape `month, prague, brno` into `month, store, revenue` first.
- **One value per block.** Write one `metric` fence per figure. Consecutive metrics stack one under another, each on its own row at every width, so keep to the three or four figures that matter and put the rest in a table.
- **The title is the page's heading.** The frontmatter `title` is drawn at the top of the report. Start the body with a paragraph or a `##` section, not with the title again.
- **Readers on phones.** `check` lays out every chart twice: for a phone (the 358 px column of a 390 px screen, less the style's frame) and for the style's desktop text column. It measures charts only, not tables, metrics or prose, so keep a phone-first table to a few short columns.
- **Look at the result.** The report draws its charts in the browser, with JavaScript and WebGL, so the text of the HTML file holds a placeholder where each chart is. To see the report, open the HTML in a browser, run `richview serve my-report/report.rmd`, or print it with `richview generate my-report/report.rmd --to pdf`, which uses a local Chromium, Chrome or Edge.

### Numbers: what readers see

Readers never get more precision than the author chose, and the default is three significant digits. This applies to metrics, table columns, axes and chart labels. Say a figure the same way in prose and in a block: a metric that prints `732,000` above a sentence that says 731,585 reads as two different numbers.

- **No `format`:** large values use scale words. `13341.31` prints `13.3 thousand`; `5630000` prints `5.63 million`.
- **`format: number`** groups thousands. `13341.31` prints `13,300`, and `13,341.3` with `digits: 6`. `16425000` prints `16,400,000`, and `16,425,000` with `digits: 6`.
- **`format: percent`** takes a fraction. `0.368` prints `36.8%`; `36.8` would print `3680%`. For a value already in percent, write `value: 36.8` with `unit: "%"`.
- **`format: year`** prints `2019` without a separator.
- **`digits`** counts significant digits from 2 to 6, not decimal places. At `digits: 5`, `1872.95` prints `1,873` and `1908.79` prints `1,908.8`. There is no fixed number of decimals.
- **Exact or longer figures:** six significant digits is the most a number prints with, so `13341.31` is `13,341.3` at best. Text prints as written: write `value: "13,341.31 EUR"`, or keep a table column as text in the data. Text cells are not added up by `total`.
- **`unit`** follows the value: `13.3 thousand EUR`. `unit: "%"` attaches without a space.
- **A metric's `delta`** is a fraction of change and prints as a signed percentage: `delta: 0.12` prints `+12.0%`, `delta: -0.054` prints `−5.4%`, and `delta: 12` prints `+1200%`. For a change in points or in absolute units, leave `delta` out and say it in `label`.
- **A metric's `trend`** chooses the delta's colour: `up` is the good-news colour, `down` the bad-news colour, `flat` neither. Left out, the sign of the delta decides. A fall that is good news is `delta: -0.054` with `trend: up`.

### Data

data takes exactly one source: data.inline with expanded row mappings for small inputs, or data.csv / data.json holding the path of a file in the report folder (data: {csv: data/sales.csv}; never the CSV text itself) for larger ones, or a connector query (data.sql, below). The accepted data.url field is not implemented by current native or browser loaders; save remote data locally for now. data.id and headers are errors.

Data typing is one contract for inline, CSV and JSON rows:

- The key decides the parser: csv: content is CSV, json: content is JSON, and content that is not the declared format is a data.csv or data.json error whose file names the data file, with the line and column inside it.
- CSV is UTF-8 (a BOM is dropped) with RFC 4180 quoting and a comma delimiter only; convert semicolon or tab files first, and save an Excel export as CSV UTF-8, since a file in another encoding fails its block at the first byte that is not UTF-8.
- Cells and headers are trimmed; headers must be nonempty and unique (data.csv.header); a row with the wrong number of fields is data.csv.
- In CSV an empty cell is null, true and false are booleans, plain numbers are numbers (10.50 reads as 10.5), a leading-zero token such as 00123 is an identifier that stays text, and an integer past 2^53 or a decimal that binary64 cannot hold stays text with its digits.
- A JSON file is an array of flat row objects whose values are strings, finite numbers, booleans or null; an outer object such as {"inline": [...]} or a nested object or array is a data.json error naming the row and key. Inline rows follow the same value rule (data.inline).
- Missing values are null, an empty CSV cell, or . NA N/A NaN null none in any case after trimming. Charts draw a gap and count it (chart.missing-observations); tables print the cell as written, sort it last and leave it out of totals.
- Any other text in a numeric chart channel, such as 1,234 or 12 kg, is a chart.non-numeric error naming the row and value: write plain numbers.
- Dates are text, plain in a CSV or JSON cell and quoted in YAML (`"2026-07"`): 2020, 2020-03, 2020-03-31, 2020-03-31T14:05, with :SS and a fraction of 1 to 9 digits (milliseconds kept), optionally ending in Z or an offset such as +02:00; without an offset the time is UTC. A line or area axis is a calendar only when every x qualifies.
- The year format is applied automatically to integer x columns named year or *_year in 1000-2999.
- Limits: a .rmd over 16 MiB is source.limit, one data file over 16 MiB is data.limit, and report HTML over 64 MiB is render.output-limit.

Statistics and layout use the exact source observations; readouts, Table and CSV carry only the precision the author chose. Readers never get more precision than the author chose.

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

### Frontmatter

Local drafts need no metadata. Publishing requires a title, a description (the lede) and a version; first drafts start at 0.0.1. The fields (* required):

@FRONTMATTER@

- **version** is MAJOR.MINOR.PATCH with an optional -prerelease, no leading zeros and no +build. Quoting is always safe. An unquoted three-part version such as the starter's 0.0.1 is text too, but an unquoted 1.0 is a number and is dropped.
- **slug** is the --slug option, else frontmatter slug (1-80 characters of a-z, 0-9 and hyphen, never rewritten), else derived from the title with accents transliterated (Průzkum koně becomes pruzkum-kone); a bad slug is publish.slug, not a fallback.
- **date** is shown under the description.
- **format** `rmd/1` marks the format version; a newer major is an error.
- **tabs** `tabs.panels` lists {id, label, from} views that split the body at top-level `## Heading {#from}` anchors, with optional tabs.default and tabs.export: all|active for printed and PDF output. An explicit heading id is kept only when a panel names it; other ids warn and the heading keeps its generated anchor. A generated anchor alone does not start a panel.

An unknown key in a fence or in the frontmatter warns and names the nearest known key (`block.unknown-key`); a retired key or tag fails with its replacement. Unknown fences remain code.

### Blocks

Every block but the chart, with each key it reads (* required). The chart has its own section, chart-contract. A table's `data` takes the same sources as a chart's, listed there under Chart keys.

@BLOCKFIELDS@

Callout kinds: `up` for good news, `down` for bad news, `hold` for a caveat or something to wait for, and `neutral` for a plain note.

### Columns

Two to four blocks side by side: two charts, a chart beside the sentence that explains it, a row of metrics, an image beside text. `::: columns` opens them, each `::: column` holds one column's prose and rich fences written as at the top level, and a line of `:::` closes each column and then the columns:

```
::: columns {widths: 2-1}
::: column
(a chart fence and its *Source:* note)
:::
::: column
The sentence that explains it.
:::
:::
```

Options after `::: columns`, as a YAML flow mapping:

@COLUMNFIELDS@

- A column holds prose, headings of level 3 and below and rich blocks; not other columns, scrolly, a level 1 or 2 heading or a tab's start.
- Columns stack on a phone, and wherever a column would be narrower than a phone column for a chart, table, embed, video or code (11rem otherwise). Put a chart in the wider share (`2-1`) and its explanation in the narrower one; three or four metrics fit side by side, two charts fit only at full width.
- Malformed columns (one column or more than four, an empty column, text between columns, a missing `:::`) warn with a `columns.*` code and render in one column.

### Markdown

- Markdown supports tables, strikethrough, tasks, footnotes, definition lists and native MathML from $inline$ / $$display$$ LaTeX.
- Prose gets curly quotes, dashes and ellipses; code, math and URLs keep their characters.
- A wholly emphasised paragraph starting with Source:, Sources:, Note: or Notes: directly after a rich block becomes that block's source note.
- No JavaScript/JSX is evaluated. Raw HTML never renders: write Markdown or a rich block instead. Each HTML tag is removed with a security.html warning at its line that names the Markdown alternative (**bold**, [text](url), ![alt](path), a table, a list), the text inside it stays, and HTML comments are dropped without a warning; entities such as &nbsp; still work, and sub- and superscripts use Unicode (m², CO₂).
- A link may target http, https, mailto, tel, a # anchor or a file in the report folder; any other target (javascript:, data:, vbscript:, //host) loses the link, keeps its text and warns security.link.
- A heading {#id} or footnote label that starts with rv, richview, tab- or panel- belongs to the report runtime and is dropped or renamed with a security.id warning, and heading classes and attributes are dropped with security.class.

### Images

- A Markdown image is a report file or a base64 data:image URI; a remote one is refused.
- The only remote image is a credited images.unsplash.com photo, which is hotlinked: an image or title_image src on that exact https host needs credit {author, author_url, source, source_url}, which adds a visible "Photo by <author> on <source>" line; author_url is required and, like source_url, must be an https://unsplash.com/ address, and source must be Unsplash.
- Without a credit image.credit-missing is an error, and a credit that breaks these rules is an image.credit-invalid error; either way the block stays source.
- Every other remote image, audio or video address is a report.asset.remote error that fails only that element.
- Optional width, height and color (#rrggbb, the photo's dominant colour) reserve its box and fill it when the photo cannot load, so offline the text and credit stay readable.
- Find and place Unsplash photos with richview image search|use (MCP richview_image_search, richview_image_use): search, show the user the photographers, use the chosen id, paste the returned YAML unchanged and never download or rehost the image.
