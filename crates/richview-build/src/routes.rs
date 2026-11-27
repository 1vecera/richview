//! The public route map (FEA-SF-46): `dist/web` holds what `assets/routes.json` names and nothing
//! else, so an older build's leftovers or a new generated file cannot become a public route.
use std::fs;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Files inside kept folders that no page loads: generated sources, diagnostics and data.
const GENERATED: [&str; 5] = [
    "showcase/themes.json",
    "showcase/openai-vs-anthropic/source-data.json",
    "showcase/openai-vs-anthropic/diagnostics.json",
    "landing/data",
    "landing/anchor",
];

pub fn prune(public: &Path, map: &str) -> Result<()> {
    let map: serde_json::Value = serde_json::from_str(map)?;
    let keep: Vec<&str> = map["keep"]
        .as_array()
        .ok_or("routes.json has no keep list")?
        .iter()
        .filter_map(|name| name.as_str())
        .collect();
    for entry in fs::read_dir(public)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if keep.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    // The docs and playground load chart examples only; other example folders stay in the repository.
    let examples = public.join("examples");
    if examples.is_dir() {
        for entry in fs::read_dir(&examples)? {
            let entry = entry?;
            if entry.file_name() != "charts" {
                let path = entry.path();
                if path.is_dir() {
                    fs::remove_dir_all(path)?;
                } else {
                    fs::remove_file(path)?;
                }
            }
        }
    }
    for relative in GENERATED {
        let path = public.join(relative);
        if path.is_dir() {
            fs::remove_dir_all(path)?;
        } else if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prune_keeps_only_the_named_routes_and_chart_examples() {
        let root = std::env::temp_dir().join(format!("rv-routes-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for dir in [
            "docs",
            "bindings/core",
            "typography",
            "examples/charts/bar",
            "examples/hello-world",
            "landing/data",
            "landing/anchor/data",
        ] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in [
            "index.html",
            "richview_site.d.ts",
            "richview_web.js",
            "landing/demo.html",
            "landing/anchor.html",
        ] {
            fs::write(root.join(file), "x").unwrap();
        }
        prune(&root, include_str!("../../../assets/routes.json")).unwrap();
        for kept in [
            "docs",
            "index.html",
            "examples/charts/bar",
            "landing/demo.html",
            "landing/anchor.html",
        ] {
            assert!(root.join(kept).exists(), "{kept}");
        }
        for gone in [
            "bindings",
            "typography",
            "richview_site.d.ts",
            "richview_web.js",
            "examples/hello-world",
            "landing/data",
            "landing/anchor",
        ] {
            assert!(!root.join(gone).exists(), "{gone}");
        }
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn no_served_route_names_an_html_file() {
        // Cloudflare answers `/page.html` with a 307 to `/page`, so a route the live check
        // requests must name the page as the live site serves it.
        let map: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/routes.json")).unwrap();
        for list in ["serve", "install"] {
            for route in map[list].as_array().unwrap() {
                let route = route.as_str().unwrap();
                assert!(
                    !route.ends_with(".html"),
                    "{list} names {route}: the live site redirects it, name the page without .html"
                );
            }
        }
    }

    #[test]
    fn the_map_names_no_route_twice() {
        let map: serde_json::Value =
            serde_json::from_str(include_str!("../../../assets/routes.json")).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for list in ["serve", "gone"] {
            for route in map[list].as_array().unwrap() {
                assert!(seen.insert(route.as_str().unwrap().to_owned()), "{route}");
            }
        }
    }
}
