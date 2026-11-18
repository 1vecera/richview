//! Build inputs and staged output hashes for the release gate (ARC-46).
use super::Result;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

pub const FILE: &str = "richview-build-manifest.json";

fn output(root: &Path, program: &str, args: &[&str]) -> Result<Vec<u8>> {
    let result = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(result.stdout)
}

fn text(root: &Path, program: &str, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(output(root, program, args)?)?
        .trim()
        .into())
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The raw Cargo Wasm modules a build binds: `--wasm-from` takes exactly these instead of building
/// them, so one platform's build can embed the Wasm another platform built.
pub const RAW_WASM: [&str; 3] = ["docs.wasm", "richview_runtime.wasm", "richview_site.wasm"];

/// Checks that `dir` holds exactly the raw Wasm modules a build binds, each a regular Wasm file,
/// and returns its canonical path. Any other, missing or extra file refuses the folder, so a build
/// never mixes its own Wasm with someone else's.
pub fn wasm_from(dir: &Path) -> Result<std::path::PathBuf> {
    let dir = dir
        .canonicalize()
        .map_err(|error| format!("--wasm-from {}: {error}", dir.display()))?;
    let mut names = fs::read_dir(&dir)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>>>()?;
    names.sort();
    if names != RAW_WASM {
        return Err(format!(
            "--wasm-from {} must hold exactly {}; it holds {}",
            dir.display(),
            RAW_WASM.join(", "),
            if names.is_empty() {
                "nothing".into()
            } else {
                names.join(", ")
            }
        )
        .into());
    }
    for name in RAW_WASM {
        let path = dir.join(name);
        if !fs::symlink_metadata(&path)?.is_file() || !fs::read(&path)?.starts_with(b"\0asm") {
            return Err(format!("--wasm-from {name} is not a regular Wasm module").into());
        }
    }
    Ok(dir)
}

/// The size and SHA-256 of each module in a `--wasm-from` folder, by file name.
fn wasm_record(dir: &Path) -> Result<Value> {
    let mut record = BTreeMap::new();
    for name in RAW_WASM {
        let bytes = fs::read(dir.join(name))?;
        record.insert(name, json!({"bytes": bytes.len(), "sha256": hash(&bytes)}));
    }
    Ok(json!(record))
}

/// Records source identity and the tools actually invoked, without absolute checkout paths. A
/// build that took its Wasm from another build (`--wasm-from`) also records those modules' hashes;
/// an ordinary build's record has no such entry.
pub fn inputs(root: &Path, binder: Option<&str>, wasm_from: Option<&Path>) -> Result<Value> {
    let mut config = BTreeMap::new();
    for path in ["Cargo.lock", "package-lock.json", "rust-toolchain.toml"] {
        config.insert(path, hash(&fs::read(root.join(path))?));
    }
    let mut record = json!({
        "commit": text(root, "git", &["rev-parse", "HEAD"] )?,
        "source_diff_sha256": hash(&output(root, "git", &["diff", "--binary", "HEAD", "--"])?),
        "dirty": !output(root, "git", &["status", "--porcelain", "--untracked-files=normal"] )?.is_empty(),
        "rustc": text(root, "rustc", &["--version"] )?,
        "wasm_bindgen": binder.map(|program| text(root, program, &["--version"])).transpose()?,
        "config": config,
    });
    if let Some(dir) = wasm_from {
        record["wasm_from"] = wasm_record(dir)?;
    }
    Ok(record)
}

fn collect(
    base: &Path,
    relative: &Path,
    group: &str,
    entries: &mut BTreeMap<String, Value>,
) -> Result<()> {
    let path = base.join(relative);
    let kind = fs::symlink_metadata(&path)?.file_type();
    if kind.is_dir() {
        for child in fs::read_dir(&path)? {
            collect(base, &relative.join(child?.file_name()), group, entries)?;
        }
    } else if kind.is_file() {
        let bytes = fs::read(&path)?;
        entries.insert(
            format!(
                "{group}:{}",
                relative
                    .to_str()
                    .ok_or("non-UTF-8 build output path")?
                    .replace('\\', "/")
            ),
            json!({"bytes": bytes.len(), "sha256": hash(&bytes)}),
        );
    } else {
        return Err(format!("unsupported build output: {}", path.display()).into());
    }
    Ok(())
}

/// Hashes the output files and atomically writes the manifest after confirming inputs stayed fixed.
pub fn write(
    root: &Path,
    target: &Path,
    mode: &str,
    binder: Option<&str>,
    wasm_from: Option<&Path>,
    before: &Value,
) -> Result<()> {
    if &inputs(root, binder, wasm_from)? != before {
        return Err("build inputs changed while staging; rerun the build (ARC-46)".into());
    }
    if mode == "full" {
        let binary = target.join(format!("release/richview{}", std::env::consts::EXE_SUFFIX));
        let version = text(
            root,
            binary.to_str().ok_or("non-UTF-8 binary path")?,
            &["--version"],
        )?;
        let commit = before["commit"].as_str().ok_or("missing build commit")?;
        let mut runtime = Sha256::new();
        runtime.update(fs::read(root.join("dist/web/richview_runtime.js"))?);
        runtime.update(fs::read(root.join("dist/web/richview_runtime_bg.wasm.gz"))?);
        let runtime = format!("{:x}", runtime.finalize());
        if !version.contains(&format!(
            "(commit {}, runtime {})",
            &commit[..10],
            &runtime[..12]
        )) {
            return Err(format!("native binary provenance differs from this checkout's commit/runtime: {version} (ARC-46)").into());
        }
    }
    let mut outputs = BTreeMap::new();
    let suffix = std::env::consts::EXE_SUFFIX;
    for binary in if mode == "editor" {
        vec!["richview-editor", "richview-sql-worker"]
    } else {
        vec!["richview", "richview-editor", "richview-sql-worker"]
    } {
        collect(
            target,
            Path::new(&format!("release/{binary}{suffix}")),
            "target",
            &mut outputs,
        )?;
    }
    if mode != "editor" {
        collect(root, Path::new("dist/web"), "checkout", &mut outputs)?;
        collect(
            root,
            Path::new("packages/vscode/web"),
            "checkout",
            &mut outputs,
        )?;
    }
    for path in [
        "packages/vscode/bin",
        "packages/vscode/dist",
        "packages/vscode/font-licenses",
    ] {
        collect(root, Path::new(path), "checkout", &mut outputs)?;
    }
    if mode == "full" {
        let commit = before["commit"].as_str().ok_or("missing build commit")?;
        let install = format!("release/install/{commit}");
        if target.join(&install).is_dir() {
            collect(target, Path::new(&install), "target", &mut outputs)?;
            collect(
                target,
                Path::new("release/install/install.sh"),
                "target",
                &mut outputs,
            )?;
        }
    }
    let manifest = json!({"schema": 1, "mode": mode, "inputs": before, "outputs": outputs});
    let path = target.join("release").join(FILE);
    let temporary = path.with_extension("json.tmp");
    fs::write(
        &temporary,
        format!("{}\n", serde_json::to_string_pretty(&manifest)?),
    )?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_hashes_are_sha256_of_the_staged_bytes_with_relative_paths() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../tmp/build-manifest-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("artifact"), b"abc").unwrap();
        let mut entries = BTreeMap::new();
        collect(&folder, Path::new("artifact"), "checkout", &mut entries).unwrap();
        assert_eq!(
            entries["checkout:artifact"],
            json!({"bytes": 3, "sha256": "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"})
        );
        fs::remove_dir_all(folder).unwrap();
    }

    const MODULE: &[u8] = b"\0asm\x01\0\0\0";

    /// A test folder's files: name and bytes.
    type Files<'a> = Vec<(&'a str, &'a [u8])>;

    /// A fresh folder under the checkout's tmp/ holding `files`.
    fn wasm_folder(name: &str, files: &[(&str, &[u8])]) -> std::path::PathBuf {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../tmp/wasm-from-test-{name}-{}",
            std::process::id()
        ));
        if folder.exists() {
            fs::remove_dir_all(&folder).unwrap();
        }
        fs::create_dir_all(&folder).unwrap();
        for (file, bytes) in files {
            fs::write(folder.join(file), bytes).unwrap();
        }
        folder
    }

    #[test]
    fn wasm_from_takes_only_a_folder_of_exactly_the_three_raw_modules() {
        let three = [
            ("docs.wasm", MODULE),
            ("richview_runtime.wasm", MODULE),
            ("richview_site.wasm", MODULE),
        ];
        let folder = wasm_folder("exact", &three);
        assert_eq!(wasm_from(&folder).unwrap(), folder.canonicalize().unwrap());
        fs::remove_dir_all(folder).unwrap();

        let refused: [(&str, Files, &str); 5] = [
            (
                "missing",
                three[..2].to_vec(),
                "it holds docs.wasm, richview_runtime.wasm",
            ),
            (
                "extra",
                [&three[..], &[("richview_runtime_bg.wasm", MODULE)]].concat(),
                "richview_runtime_bg.wasm",
            ),
            (
                "renamed",
                vec![three[0], three[1], ("richview_site_bg.wasm", MODULE)],
                "it holds docs.wasm, richview_runtime.wasm, richview_site_bg.wasm",
            ),
            ("empty", vec![], "it holds nothing"),
            (
                "not-wasm",
                vec![
                    three[0],
                    ("richview_runtime.wasm", &b"\x1f\x8b\x08"[..]),
                    three[2],
                ],
                "richview_runtime.wasm is not a regular Wasm module",
            ),
        ];
        for (name, files, message) in refused {
            let folder = wasm_folder(name, &files);
            let error = wasm_from(&folder).unwrap_err().to_string();
            assert!(error.contains(message), "{name}: {error}");
            fs::remove_dir_all(folder).unwrap();
        }
        let error = wasm_from(Path::new("tmp/wasm-from-test-absent")).unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("--wasm-from tmp/wasm-from-test-absent: ")
        );
    }

    #[test]
    fn build_inputs_record_the_wasm_from_hashes_and_only_when_given() {
        let folder = wasm_folder(
            "record",
            &[
                ("docs.wasm", MODULE),
                ("richview_runtime.wasm", &b"\0asm\x01\0\0\0runtime"[..]),
                ("richview_site.wasm", MODULE),
            ],
        );
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let module = json!({"bytes": 8, "sha256": "93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476"});
        assert_eq!(
            inputs(&root, None, Some(&folder)).unwrap()["wasm_from"],
            json!({
                "docs.wasm": module,
                "richview_runtime.wasm": {"bytes": 15, "sha256": "f952c231e01c8e682b08f85d85870c3315470a644f4dc9a2aba58e02a61b50be"},
                "richview_site.wasm": module,
            })
        );
        assert!(
            inputs(&root, None, None)
                .unwrap()
                .get("wasm_from")
                .is_none()
        );
        fs::remove_dir_all(folder).unwrap();
    }
}
