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

/// Records source identity and the tools actually invoked, without absolute checkout paths.
pub fn inputs(root: &Path, binder: Option<&str>) -> Result<Value> {
    let mut config = BTreeMap::new();
    for path in ["Cargo.lock", "package-lock.json", "rust-toolchain.toml"] {
        config.insert(path, hash(&fs::read(root.join(path))?));
    }
    Ok(json!({
        "commit": text(root, "git", &["rev-parse", "HEAD"] )?,
        "source_diff_sha256": hash(&output(root, "git", &["diff", "--binary", "HEAD", "--"])?),
        "dirty": !output(root, "git", &["status", "--porcelain", "--untracked-files=normal"] )?.is_empty(),
        "rustc": text(root, "rustc", &["--version"] )?,
        "wasm_bindgen": binder.map(|program| text(root, program, &["--version"])).transpose()?,
        "config": config,
    }))
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
    before: &Value,
) -> Result<()> {
    if &inputs(root, binder)? != before {
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
}
