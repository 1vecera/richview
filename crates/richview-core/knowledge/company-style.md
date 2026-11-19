## Make your company style

Put a `.rstyle.yaml` file next to the report and name it in the report's `styles` list. Start with one brand colour and a preset:

```yaml
preset: air
colors:
  brand: "#0B6E69"
```

The colour above produces `#0b6e69`, `#c9690c`, `#7f62a0`, `#50a06d`, `#334182`, `#9d3434` for the six series. Change only `colors.brand` to `#F2C200` and the palette becomes `#b18d00`, `#174bd7`, `#338c77`, `#7f62a0`, `#005474`, `#b55239`: the first yellow is darkened for contrast. A second brand colour may be given as `colors.secondary`, and the page as `colors.background`. An explicit `palette` overrides the derived series; `fonts` can name bundled faces or report-relative WOFF2 files. Run `richview check report.rmd`, fix its findings, then `richview generate report.rmd` and inspect the result. If the exact brand colour cannot meet the mark contrast floor, RichView adjusts the accent and first series colour and explains the substitution.

`company.name`, `company.logo` and `company.logo-dark` add the company's mark before the title. The RichView repository's `examples/company-style/` holds a worked report.