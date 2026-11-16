//! Embeds the report runtime into the binary (ARC-07) and records the build commit.
//!
//! With the `embedded-runtime` feature, the staged runtime (`dist/web`, or the folder named by
//! `RICHVIEW_RUNTIME_DIR`) is compiled in. A build with the feature but without its files fails:
//! a binary must never ship reports whose charts show only their frame. Bootstrap builds omit
//! the feature; exporting without an embedded runtime fails.
use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rerun-if-env-changed=RICHVIEW_RUNTIME_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    let commit = Command::new("git")
        .args(["rev-parse", "--short=10", "HEAD"])
        .current_dir(&manifest)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=RICHVIEW_BUILD_COMMIT={commit}");
    if let Some(head) = Command::new("git")
        .args(["rev-parse", "--git-path", "HEAD"])
        .current_dir(&manifest)
        .output()
        .ok()
        .filter(|o| o.status.success())
    {
        let head = String::from_utf8_lossy(&head.stdout).trim().to_owned();
        let head = manifest.join(head);
        if head.exists() {
            println!("cargo:rerun-if-changed={}", head.display());
        }
    }

    let embed = env::var_os("CARGO_FEATURE_EMBEDDED_RUNTIME").is_some();
    let dir = env::var_os("RICHVIEW_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("../../dist/web"));
    let js = dir.join("richview_runtime.js");
    let wasm = dir.join("richview_runtime_bg.wasm.gz");
    let code = if embed && js.is_file() && wasm.is_file() {
        for file in [&js, &wasm] {
            println!("cargo:rerun-if-changed={}", file.display());
        }
        format!(
            "pub const RUNTIME_JS: Option<&[u8]> = Some(include_bytes!({js:?}));\npub const RUNTIME_WASM_GZ: Option<&[u8]> = Some(include_bytes!({wasm:?}));\n",
            js = js.canonicalize().unwrap(),
            wasm = wasm.canonicalize().unwrap()
        )
    } else {
        if embed {
            for file in [&js, &wasm] {
                println!("cargo:rerun-if-changed={}", file.display());
            }

            panic!(
                "the report runtime is not staged at {}; run `cargo run --release -p richview-build` first (ARC-07)",
                dir.display()
            );
        }
        "pub const RUNTIME_JS: Option<&[u8]> = None;\npub const RUNTIME_WASM_GZ: Option<&[u8]> = None;\n".into()
    };
    fs::write(out.join("runtime_bytes.rs"), code).unwrap();

    // Hosted app pages reuse the site's generated docs runtime. Single-server builds do not need
    // it; hosted startup fails closed if a custom runtime directory omitted these assets.
    let docs = [
        ("DOCS_JS", "richview_docs.js"),
        ("DOCS_WASM", "richview_docs_bg.wasm"),
        ("DOCS_BOOTSTRAP", "bootstrap.js"),
    ];
    let mut docs_code = String::new();
    for (constant, name) in docs {
        let path = dir.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        if embed && path.is_file() {
            docs_code.push_str(&format!(
                "pub const {constant}: Option<&[u8]> = Some(include_bytes!({:?}));\n",
                path.canonicalize().unwrap()
            ));
        } else {
            docs_code.push_str(&format!("pub const {constant}: Option<&[u8]> = None;\n"));
        }
    }
    fs::write(out.join("docs_bytes.rs"), docs_code).unwrap();
}
