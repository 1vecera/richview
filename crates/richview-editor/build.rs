//! Chart snippets come from the documentation: each chart type's page under `docs/public/charts/`
//! carries one worked example in its `Source` section, and that example is the snippet. One source
//! per type, so a snippet cannot drift from what the page shows (FEA-SF-26).
use std::{collections::BTreeMap, env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let charts = manifest.join("../../docs/public/charts");
    println!("cargo:rerun-if-changed={}", charts.display());
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(&charts) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            println!("cargo:rerun-if-changed={}", path.display());
            let (Some(stem), Ok(text)) = (path.file_stem(), fs::read_to_string(&path)) else {
                continue;
            };
            if let Some(example) = example_of(&text) {
                found.insert(stem.to_string_lossy().into_owned(), example);
            }
        }
    }
    let mut out = String::from("pub(crate) const DOC_EXAMPLES: &[(&str, &str)] = &[\n");
    for (kind, body) in &found {
        out.push_str(&format!("    ({kind:?}, {body:?}),\n"));
    }
    out.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR").unwrap()).join("doc_examples.rs"),
        out,
    )
    .unwrap();
}

/// The first chart fence inside the page's four-backtick `rmd` source block.
fn example_of(page: &str) -> Option<String> {
    let start = page.find("````rmd\n")? + "````rmd\n".len();
    let block = &page[start..];
    let block = &block[..block.find("\n````")?];
    let from = block.find("```chart\n")?;
    let rest = &block[from..];
    let end = rest[3..].find("\n```")? + 3 + "\n```".len();
    Some(format!("{}\n", &rest[..end]))
}
