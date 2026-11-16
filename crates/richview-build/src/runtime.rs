//! Loads staged runtime assets while the build tool bootstraps the native binaries (ARC-07).
use base64::{Engine, engine::general_purpose::STANDARD};
use richview_app::resolver::bounded_read;
use richview_core::document::RenderOptions;
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
struct CachedRuntime {
    stamp: Vec<(u64, Option<std::time::SystemTime>)>,
    js: String,
    wasm: String,
}
static RUNTIME_CACHE: std::sync::OnceLock<std::sync::Mutex<BTreeMap<PathBuf, CachedRuntime>>> =
    std::sync::OnceLock::new();

pub fn load(public_dir: &Path, options: &mut RenderOptions) -> Result<(), String> {
    let candidates = [
        public_dir.join("richview_runtime.js"),
        public_dir.join("pkg/richview_runtime.js"),
    ];
    let Some(path) = candidates.iter().find(|p| p.is_file()) else {
        return Err(format!(
            "Report runtime is not staged at {}; run cargo run -p richview-build",
            public_dir.display()
        ));
    };
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    let wasm = path.with_file_name("richview_runtime_bg.wasm");
    let gzip = path.with_file_name("richview_runtime_bg.wasm.gz");
    if !wasm.is_file() {
        return Err("Report runtime bindings exist but Wasm is missing; run cargo run -p richview-build --release".into());
    }
    let mut stamp: Vec<_> = [&path, &wasm]
        .iter()
        .map(|p| {
            fs::metadata(p)
                .map(|m| (m.len(), m.modified().ok()))
                .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    stamp.push(
        fs::metadata(&gzip)
            .map(|m| (m.len(), m.modified().ok()))
            .unwrap_or((0, None)),
    );
    let mut cache = RUNTIME_CACHE
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "Runtime cache lock poisoned")?;
    if !cache.get(&path).is_some_and(|c| c.stamp == stamp) {
        let js = String::from_utf8(bounded_read(&path)?).map_err(|e| e.to_string())?;
        let wasm_bytes = bounded_read(&wasm)?;
        if !wasm_bytes.starts_with(b"\0asm") {
            return Err("Runtime artifact is not WebAssembly".into());
        }
        let precompressed = match (stamp[1].1, stamp[2].1) {
            (Some(wasm_time), Some(gzip_time)) if gzip_time >= wasm_time => {
                verified_gzip(&gzip, &wasm_bytes)
            }
            _ => None,
        };
        let compressed_wasm = if let Some(bytes) = precompressed {
            bytes
        } else {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(&wasm_bytes).map_err(|e| e.to_string())?;
            encoder.finish().map_err(|e| e.to_string())?
        };
        if cache.len() >= 8 {
            cache.clear();
        }
        cache.insert(
            path.clone(),
            CachedRuntime {
                stamp,
                js,
                wasm: STANDARD.encode(compressed_wasm),
            },
        );
    }
    let runtime = cache.get(&path).unwrap();
    options.runtime_js = Some(runtime.js.clone());
    options.runtime_wasm = Some(runtime.wasm.clone());
    options.runtime_wasm_codec = "gzip".into();
    Ok(())
}

fn verified_gzip(path: &Path, wasm: &[u8]) -> Option<Vec<u8>> {
    let compressed = bounded_read(path).ok()?;
    let mut decoded = Vec::with_capacity(wasm.len());
    flate2::read::MultiGzDecoder::new(compressed.as_slice())
        .take(wasm.len() as u64 + 1)
        .read_to_end(&mut decoded)
        .ok()?;
    (decoded == wasm).then_some(compressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn precompressed_runtime_is_reused_only_when_current_and_exact() {
        use base64::Engine;
        use std::io::{Read, Write};
        fs::create_dir_all("tmp").unwrap();
        let folder = tempfile::tempdir_in("tmp").unwrap();
        let public = folder.path();
        let raw = [0, 97, 115, 109, 1, 0, 0, 0];
        fs::write(public.join("richview_runtime.js"), "var wasm_bindgen = {};").unwrap();
        let wasm = public.join("richview_runtime_bg.wasm");
        let gzip = public.join("richview_runtime_bg.wasm.gz");
        fs::write(&wasm, raw).unwrap();
        fs::File::open(&wasm)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)),
            )
            .unwrap();
        let pack = |name: &str, bytes: &[u8]| {
            let mut encoder = flate2::GzBuilder::new()
                .filename(name)
                .write(Vec::new(), flate2::Compression::default());
            encoder.write_all(bytes).unwrap();
            encoder.finish().unwrap()
        };
        let prebuilt = pack("prebuilt-artifact", &raw);
        fs::write(&gzip, &prebuilt).unwrap();
        let load = || {
            let mut options = richview_core::document::RenderOptions::default();
            load(public, &mut options).unwrap();
            base64::engine::general_purpose::STANDARD
                .decode(options.runtime_wasm.unwrap())
                .unwrap()
        };
        assert_eq!(load(), prebuilt);
        fs::File::open(&gzip)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH))
            .unwrap();
        assert_ne!(
            load(),
            prebuilt,
            "a stale gzip must fall back to current raw Wasm"
        );
        for invalid in [
            pack("wrong-same-size", &[0, 97, 115, 109, 2, 0, 0, 0]),
            prebuilt[..5].to_vec(),
            [prebuilt.clone(), pack("trailing-member", b"extra")].concat(),
        ] {
            fs::write(&gzip, invalid).unwrap();
            let compressed = load();
            let mut decoded = Vec::new();
            flate2::read::MultiGzDecoder::new(compressed.as_slice())
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(
                decoded, raw,
                "corrupt, unrelated or trailing-member gzip must not be embedded"
            );
        }
        let recovered = pack("recovered-build-artifact", &raw);
        fs::write(gzip, &recovered).unwrap();
        assert_eq!(
            load(),
            recovered,
            "replacing the build asset invalidates its runtime cache entry"
        );
    }
}
