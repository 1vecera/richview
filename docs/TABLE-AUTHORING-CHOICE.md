# Table authoring choice — pending Daniel's decision

This choice is part of the RichView feedback work. The editor work reuses the VS Code linting, formatting and auto-suggestions delivery; chart defaults are the work to default small chart data to inline YAML.

Recommendation: keep row mappings as the supported table contract, using expanded YAML rows and column settings by default. They retain record identity while inserting, deleting or moving observations. The series alternative below is a source comparison only, outside the parser/schema. Daniel's final choice remains **pending**; no reports migrate and no `series` table schema ships.

## Before and after small-chart source

The existing native/editor/browser starters already used inline data; external data remains useful for larger datasets inside this report folder. A small authored example need not create `sales.csv` containing `month,value\nJan,100\nFeb,140`:

````rmd
```chart
type: bar
title: Revenue rose in February
data: {csv: sales.csv}
encoding: {x: month, y: value}
```
````

The updated default source keeps the observations beside the chart:

````rmd
```chart
type: bar
title: Revenue rose in February
data:
  inline:
    - month: Jan
      value: 100
    - month: Feb
      value: 140
encoding: {x: month, y: value}
```
````

## Before: compact row/object table source

````rmd
```table
data:
  inline:
    - {month: Jan, value: 100}
    - {month: Feb, value: 140}
columns:
  - {key: month, label: Month}
  - {key: value, label: Value, align: right}
```
````

## After: supported structured YAML table default

````rmd
```table
data:
  inline:
    - month: Jan
      value: 100
    - month: Feb
      value: 140
columns:
  - key: month
    label: Month
  - key: value
    label: Value
    align: right
```
````

This source is also in [the installable authoring example](../examples/authoring/report.rmd). Both table sources above render the same table:

| Month | Value |
| --- | ---: |
| Jan | 100 |
| Feb | 140 |

## Tentative series/column-oriented source

```yaml
# Comparison only — unsupported proposal, not a Rich Markdown table contract.
series:
  - key: month
    label: Month
    values: [Jan, Feb]
  - key: value
    label: Value
    align: right
    values: [100, 140]
```

Pairing values by position would produce exactly the table above. It puts each column's label, formatting and data together, and saves repeated row keys. For many narrow rows it can be shorter; long arrays wrap and make one observation harder to inspect. Adding or moving a row requires synchronized changes across every series. Unequal lengths, duplicate keys and null padding need explicit rules; missing observations must remain distinct from absent positions. Diagnostics would need both a column and an observation index, and editing commands would have to keep the arrays aligned. Sorting must transpose to records first, preserving the established table behavior.

| Concern | Expanded `data.inline` rows (implemented) | Series proposal (pending) |
| --- | --- | --- |
| Read one observation | Adjacent named values in one record | Match positions across arrays |
| Add/remove/move a row | One YAML record | Every values array |
| Add/change a column | Column config and row keys | One series |
| Missing values | Existing explicit `null` or absent field | New equal-length/null policy required |
| Validation/completion | Existing data fields, column keys and sort contract | New length/index/transpose contract required |
| Compatibility | Existing row tables remain valid after required folder/data migration | Needs an optional adapter and a decision before adoption |

Confirmed changes implemented: automatic semantic diagnostics with source ranges; optional normal VS Code formatting; schema/value/data-field suggestions; a new installable extension preparation version; readable inline YAML defaults for chart/table insertion. Chart-local CSV/JSON files remain supported within each dedicated report folder. Header registries migrate as described in [the source contract](RMD-SPEC.md). Folder, output and style scope, and the separate chart visual work, are tracked in their own issues.
