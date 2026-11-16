# Source-first authoring example

Keep `report.rmd` and optional `report.rstyle.yaml` together. From the repository root, run `target/release/richview generate examples/authoring/report.rmd`; open `examples/authoring/Rendered/report.html`. The HTML embeds runtime/fonts/styles/data and can be moved together with its `Rendered` assets. VS Code offers **RichView: Export HTML to Rendered** after saving the source.

Edit/save the local YAML to refresh the live preview; rerun generation to replace HTML. Choose `--style sleek`, `--style scientific` or `--style brutalist` to override source styling. To return, rerun without `--style`. To use defaults without a YAML file, remove the `styles` line (or keep a builtin only) and remove the optional file. A declared missing file is an error, not a silent fallback. Charts and tables retain exactly Jan=100, Feb=140 throughout.

The table's expanded rows are the supported default. The [series comparison](../../docs/TABLE-AUTHORING-CHOICE.md) remains pending Daniel's decision.
