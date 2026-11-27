//! Reproducible native, WebAssembly, offline-report and editor packaging.
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

mod agents;
mod chrome;
mod compress;
mod distribution;
mod documentation;
mod flagship;
mod manifest;
mod routes;
mod runtime;
mod site_fonts;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The Cargo profiles of the Wasm the browser downloads, tuned for size (PRF-30, PRF-31): the site
/// runtime for the smallest file, the report runtime for size without a slower frame.
const SITE_PROFILE: &str = "wasm-site";
const RUNTIME_PROFILE: &str = "wasm-release";
/// The bundled font catalogs, staged beside every WebAssembly runtime.
const FONT_CATALOG: &str = "richview-fonts.css";

/// The repository to stage. `cargo run -p richview-build` names the checkout it runs from in
/// `CARGO_MANIFEST_DIR`. That path wins over the one compiled in: with one Cargo target shared by
/// several checkouts, a binary built in another checkout would otherwise stage that checkout's
/// files. A value that does not point into a RichView checkout, such as one inherited
/// from a parent `cargo` for `richview-chart`, is ignored.
fn repo_root(runtime: Option<PathBuf>, compiled: &Path) -> Result<PathBuf> {
    let candidate = runtime
        .filter(|dir| dir.join("../../crates/richview-build/Cargo.toml").is_file())
        .unwrap_or_else(|| compiled.to_path_buf());
    Ok(candidate.join("../..").canonicalize()?)
}

fn staged_install_enabled(
    enabled: bool,
    assets_only: bool,
    editor_only: bool,
    local_preview: bool,
) -> Result<bool> {
    if local_preview && (!assets_only || editor_only) {
        return Err(
            "--local-preview requires --assets-only and cannot package only the editor".into(),
        );
    }
    if enabled && assets_only && !local_preview {
        return Err("an enabled install release requires a complete native build".into());
    }
    Ok(enabled && !local_preview)
}

fn require_public_repository_for_install(
    install_enabled: bool,
    repository_public: bool,
) -> Result<()> {
    if install_enabled && !repository_public {
        return Err("an enabled install release requires --repository-public".into());
    }
    Ok(())
}

fn main() -> Result<()> {
    let root = repo_root(
        std::env::var_os("CARGO_MANIFEST_DIR").map(PathBuf::from),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    run(
        &root,
        "node",
        &[
            "scripts/third-party-notices.ts",
            "--generate",
            "tmp/THIRD-PARTY-NOTICES.txt",
        ],
    )?;
    if args.first().is_some_and(|arg| arg == "--install-archive") {
        let cross = match args.len() {
            4 => None,
            // A target that is not plain text is no target the archive step knows.
            6 if args[4] == "--target" => Some(args[5].to_str().unwrap_or_default()),
            _ => {
                return Err(format!(
                    "usage: richview-build --install-archive MANIFEST NATIVE_DIR NEW_OUTPUT_DIR [--target {}]",
                    distribution::WINDOWS
                )
                .into());
            }
        };
        return distribution::existing(
            &root,
            Path::new(&args[1]),
            Path::new(&args[2]),
            Path::new(&args[3]),
            cross,
        );
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let assets_only = std::env::args().any(|arg| arg == "--assets-only");
    let editor_only = std::env::args().any(|arg| arg == "--editor-only");
    let local_preview = std::env::args().any(|arg| arg == "--local-preview");
    let wasm_from = wasm_from_arg(&args, editor_only)?;
    let wasm_from = wasm_from.as_deref();
    let binder = bindgen(&cargo);
    let manifest_binder = (!editor_only).then_some(binder.as_str());
    let build_inputs = manifest::inputs(&root, manifest_binder, wasm_from)?;
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|p| if p.is_absolute() { p } else { root.join(p) })
        .unwrap_or_else(|| root.join("target"));
    let editor = root.join("packages/vscode");
    // Packages carry the notices of the report faces and of the showcase faces.
    let font_licenses = [
        root.join("crates/richview-core/assets/font-licenses"),
        root.join("assets/showcase/fonts/licenses"),
    ];
    let install_enabled = staged_install_enabled(
        if editor_only {
            false
        } else {
            distribution::release_enabled(&root)?
        },
        assets_only,
        editor_only,
        local_preview,
    )?;
    let repository_public = args.iter().any(|arg| arg == "--repository-public");
    require_public_repository_for_install(install_enabled, repository_public)?;
    if editor_only {
        // Editor export uses the same embedded runtime as the CLI (ARC-07).
        if !assets_only {
            run(
                &root,
                &cargo,
                &[
                    "build",
                    "--release",
                    "--locked",
                    "-p",
                    "richview-app",
                    "-p",
                    "richview-editor",
                    "--bin",
                    "richview-sql-worker",
                    "--bin",
                    "richview-editor",
                    "--features",
                    "richview-app/embedded-runtime,richview-editor/embedded-runtime",
                ],
            )?;
        }
        package_native(&editor, &target, &font_licenses)?;
        manifest::write(
            &root,
            &target,
            "editor",
            manifest_binder,
            None,
            &build_inputs,
        )?;
        println!("Native editor packaged at {}", editor.display());
        return Ok(());
    }
    if assets_only && wasm_from.is_none() {
        own_wasm(&target, &root)?;
    }
    let public = root.join("dist/web");
    fs::create_dir_all(&public)?;
    // Old install artifacts must not leak into a disabled or failed build.
    for name in [
        "install.html",
        "install.sh",
        "install-release.json",
        "downloads",
    ] {
        let path = public.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)?,
            Ok(_) => fs::remove_file(path)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    // With --wasm-from the build binds the given raw Wasm instead of building its own.
    if !assets_only && wasm_from.is_none() {
        for (profile, package) in [
            (SITE_PROFILE, "richview-site"),
            (RUNTIME_PROFILE, "richview-runtime"),
        ] {
            run(
                &root,
                &cargo,
                &[
                    "build",
                    "--profile",
                    profile,
                    "--locked",
                    "--target",
                    "wasm32-unknown-unknown",
                    "-p",
                    package,
                    "--lib",
                ],
            )?;
        }
        run(
            &root,
            &cargo,
            &[
                "build",
                "--profile",
                SITE_PROFILE,
                "--locked",
                "--target",
                "wasm32-unknown-unknown",
                "-p",
                "richview-site",
                "--bin",
                "docs",
            ],
        )?;
    }
    run(
        &root,
        &binder,
        &[
            raw_wasm(&target, wasm_from, SITE_PROFILE, "docs.wasm")
                .to_str()
                .ok_or("Invalid docs Wasm path")?,
            "--target",
            "no-modules",
            "--no-modules-global",
            "richview_docs",
            "--out-dir",
            public.to_str().ok_or("Invalid output path")?,
            "--out-name",
            "richview_docs",
        ],
    )?;
    run(
        &root,
        &binder,
        &[
            raw_wasm(&target, wasm_from, SITE_PROFILE, "richview_site.wasm")
                .to_str()
                .ok_or("Invalid Wasm path")?,
            "--target",
            "no-modules",
            "--out-dir",
            public.to_str().ok_or("Invalid output path")?,
            "--out-name",
            "richview_site",
        ],
    )?;
    // The report runtime (ARC-06) under its own global, so a page that also runs the site
    // runtime keeps both. Every report embeds or loads it; nothing else does.
    run(
        &root,
        &binder,
        &[
            raw_wasm(&target, wasm_from, RUNTIME_PROFILE, "richview_runtime.wasm")
                .to_str()
                .ok_or("Invalid Wasm path")?,
            "--target",
            "no-modules",
            "--no-modules-global",
            "richview_runtime",
            "--out-dir",
            public.to_str().ok_or("Invalid output path")?,
            "--out-name",
            "richview_runtime",
        ],
    )?;
    // Offline native exports reuse these exact compressed runtimes. They are rebuilt only when
    // bindings are generated; consumers validate them against the raw module before trusting it.
    for name in ["richview_site", "richview_runtime", "richview_docs"] {
        let wasm = fs::read(public.join(format!("{name}_bg.wasm")))?;
        let gzip = if name == "richview_runtime" {
            compress::runtime(&wasm)?
        } else {
            let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            gzip.write_all(&wasm)?;
            gzip.finish()?
        };
        let compressed = public.join(format!("{name}_bg.wasm.gz.tmp"));
        fs::write(&compressed, gzip)?;
        fs::rename(compressed, public.join(format!("{name}_bg.wasm.gz")))?;
    }
    // The WebAssembly runtime does not compile the font catalogs in; each host that renders
    // reports with it loads this staged copy and provides it to the core.
    let font_catalog = richview_core::bundled_font_catalog();
    fs::write(public.join(FONT_CATALOG), &font_catalog)?;
    fs::copy(
        root.join("assets/fonts-math.css"),
        public.join("fonts-math.css"),
    )?;
    stage_assets(&root, &public, install_enabled)?;
    stage_browser_scripts(&root, &public)?;
    agents::stage(&root, &public, install_enabled)?;
    stage_reports(&root, &public)?;
    documentation::stage(
        &root,
        &public,
        documentation::BuildFlags {
            install_enabled,
            repository_public,
            feedback_enabled: args.iter().any(|arg| arg == "--feedback-enabled"),
            windows_preview: distribution::windows_preview(&root)?,
        },
    )?;
    routes::prune(&public, include_str!("../../../assets/routes.json"))?;
    if !repository_public {
        documentation::hide_private_repository_links(&public)?;
    }
    let editor_web = editor.join("web");
    if editor_web.exists() {
        fs::remove_dir_all(&editor_web)?;
    }
    fs::create_dir_all(&editor_web)?;
    for file in [
        "richview_runtime.js",
        "richview_runtime_bg.wasm",
        "richview_runtime_bg.wasm.gz",
    ] {
        fs::copy(public.join(file), editor_web.join(file))?;
    }
    stage_report_manifests(&public)?;
    // The binaries embed the runtime staged above (ARC-07), so they are built last.
    if !assets_only {
        run(
            &root,
            &cargo,
            &[
                "build",
                "--release",
                "--locked",
                "-p",
                "richview-app",
                "-p",
                "richview-editor",
                "-p",
                "richview-site",
                "--features",
                "richview-app/embedded-runtime,richview-editor/embedded-runtime",
            ],
        )?;
    }
    package_native(&editor, &target, &font_licenses)?;
    if !assets_only && distribution::platform().is_ok() {
        distribution::stage(
            &root,
            &target.join("release"),
            &target.join("release/install"),
            &build_inputs,
        )?;
        if install_enabled {
            distribution::stage_site(&root, &public)?;
        }
    } else if install_enabled {
        return Err("no install archive for this build platform".into());
    }
    manifest::write(
        &root,
        &target,
        if local_preview {
            "local-preview"
        } else if assets_only {
            "assets"
        } else {
            "full"
        },
        manifest_binder,
        wasm_from,
        &build_inputs,
    )?;
    println!("Rust native binaries: {}", target.join("release").display());
    println!("Browser + offline previews: {}", public.display());
    if local_preview {
        println!(
            "Local preview only: install routes and text are omitted; this is not a release build."
        );
    }
    Ok(())
}

/// The folder `--wasm-from DIR` names, checked by `manifest::wasm_from`, or `None` without the
/// option. The Wasm is the same on every platform, but each compiler host builds it slightly
/// differently, so a release builds it once and every platform's build binds those bytes.
fn wasm_from_arg(args: &[std::ffi::OsString], editor_only: bool) -> Result<Option<PathBuf>> {
    let Some(at) = args.iter().position(|arg| arg == "--wasm-from") else {
        return Ok(None);
    };
    let dir = args
        .get(at + 1)
        .filter(|dir| !dir.to_string_lossy().starts_with("--"))
        .ok_or("--wasm-from needs the folder of raw Wasm to bind")?;
    if editor_only {
        return Err("--wasm-from does not apply to --editor-only, which builds no Wasm".into());
    }
    manifest::wasm_from(Path::new(dir)).map(Some)
}

/// Refuses to bind Wasm that another checkout compiled. `--assets-only` binds the raw Wasm the
/// target already holds, and in a Cargo target that several checkouts share, that is whichever
/// checkout built it last: a page would then run another branch's runtime under this one's HTML.
/// The RichView `RUSTC_WRAPPER` records the checkout of the latest workspace compile in each
/// output folder as `.rv-fast/last-<triple>--<profile>`; a record that names another checkout
/// refuses. A target without records, such as a checkout's own, binds as before.
fn own_wasm(target: &Path, root: &Path) -> Result<()> {
    for (profile, packages) in [
        (SITE_PROFILE, "-p richview-site --lib --bin docs"),
        (RUNTIME_PROFILE, "-p richview-runtime --lib"),
    ] {
        let folder = format!("wasm32-unknown-unknown--{profile}");
        let Ok(owner) = fs::read_to_string(target.join(".rv-fast").join(format!("last-{folder}")))
        else {
            continue;
        };
        let owner = PathBuf::from(owner.trim());
        if owner == root || owner.canonicalize().is_ok_and(|owner| owner == root) {
            continue;
        }
        return Err(format!(
            "--assets-only would bind the Wasm in {}, which {} compiled last, not this checkout \
             ({}). Build this checkout's Wasm first (cargo build --profile {profile} --locked \
             --target wasm32-unknown-unknown {packages}, as the full build does), run the full \
             build, or pass --wasm-from.",
            target
                .join("wasm32-unknown-unknown")
                .join(profile)
                .display(),
            owner.display(),
            root.display(),
        )
        .into());
    }
    Ok(())
}

/// The raw Cargo Wasm module `name` to bind: from the `--wasm-from` folder when given, else
/// where this build's own Cargo run wrote it.
fn raw_wasm(target: &Path, wasm_from: Option<&Path>, profile: &str, name: &str) -> PathBuf {
    wasm_from.map_or_else(
        || target.join(format!("wasm32-unknown-unknown/{profile}/{name}")),
        |dir| dir.join(name),
    )
}

fn bindgen(cargo: &str) -> String {
    std::env::var("WASM_BINDGEN").unwrap_or_else(|_| {
        let sibling = Path::new(cargo)
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!("wasm-bindgen{}", std::env::consts::EXE_SUFFIX));
        if sibling.exists() {
            sibling.to_string_lossy().into_owned()
        } else {
            "wasm-bindgen".into()
        }
    })
}

/// Copies the native editor and SQL worker into the VS Code package and stages its assets.
fn package_native(editor: &Path, target: &Path, font_licenses: &[PathBuf; 2]) -> Result<()> {
    stage_native_binaries(editor, target)?;
    stage_snippets(editor)?;
    stage_grammar(editor)?;
    bundle_extension(editor)?;
    // The extension also ships native Rust binaries and the embedded report runtime.
    fs::copy(
        editor.join("../../tmp/THIRD-PARTY-NOTICES.txt"),
        editor.join("dist/THIRD-PARTY-NOTICES-ALL.txt"),
    )?;
    for licenses in font_licenses {
        copy_tree(licenses, &editor.join("font-licenses"))?;
        copy_tree(licenses, &target.join("release/font-licenses"))?;
    }
    Ok(())
}

/// Copies both native executables after requiring the complete editor and worker pair.
fn stage_native_binaries(editor: &Path, target: &Path) -> Result<()> {
    let suffix = std::env::consts::EXE_SUFFIX;
    let files = ["richview-editor", "richview-sql-worker"].map(|name| {
        (
            target.join(format!("release/{name}{suffix}")),
            editor.join(format!("bin/{name}{suffix}")),
        )
    });
    for (source, _) in &files {
        if !source.is_file() {
            return Err(format!(
                "required native executable is missing: {}",
                source.display()
            )
            .into());
        }
    }
    fs::create_dir_all(editor.join("bin"))?;
    for (source, destination) in files {
        fs::copy(source, destination)?;
    }
    Ok(())
}

fn stage_report_manifests(directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            stage_report_manifests(&entry.path())?;
        } else if entry
            .file_name()
            .to_str()
            .is_some_and(richview_core::document::is_source_file)
        {
            let mut folder = richview_app::resolver::report_folder(&entry.path())?;
            folder.root = directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            fs::write(
                directory.join("report-folder.json"),
                serde_json::to_string_pretty(&folder)?,
            )?;
        }
    }
    Ok(())
}

/// Keep Cargo's selected flags while removing host-specific paths from browser Wasm.
fn wasm_build_flags(root: &Path) -> Result<String> {
    let config = cargo_config2::Config::load_with_cwd(root)?;
    let flags = config
        .rustflags("wasm32-unknown-unknown")?
        .unwrap_or_default();
    let home = match std::env::var_os("CARGO_HOME") {
        Some(path) if !path.is_empty() => PathBuf::from(path),
        _ => PathBuf::from(std::env::var_os("HOME").ok_or("Cargo home is unavailable")?)
            .join(".cargo"),
    };
    let home = if home.is_absolute() {
        home
    } else {
        root.join(home)
    };
    remap_wasm_paths(flags, root, &home)
}

fn remap_wasm_paths(mut flags: cargo_config2::Flags, root: &Path, home: &Path) -> Result<String> {
    let root = root.to_str().ok_or("Wasm build path is not valid UTF-8")?;
    let home = home.to_str().ok_or("Wasm build path is not valid UTF-8")?;
    // Rustc matches prefixes as text and applies the last match.
    let mappings = if home.starts_with(root) {
        [(root, "/richview"), (home, "/cargo")]
    } else {
        [(home, "/cargo"), (root, "/richview")]
    };
    for (path, stable) in mappings {
        flags.push(format!("--remap-path-prefix={path}={stable}"));
    }
    Ok(flags.encode()?)
}

fn run(root: &Path, program: &str, args: &[&str]) -> Result<()> {
    let mut command = Command::new(program);
    command.args(args).current_dir(root);
    // `cargo run` supplies this build tool's crate identity to its process. Forwarding
    // it to Cargo changes dependency build-script fingerprints between the outer and
    // nested invocations (for example ring tracks CARGO_MANIFEST_DIR). Child Cargo
    // supplies each crate's own identity; retain the caller's configuration and toolchain.
    for (name, _) in std::env::vars_os() {
        let Some(key) = name.to_str() else { continue };
        if key.starts_with("CARGO_PKG_")
            || matches!(
                key,
                "CARGO_MANIFEST_DIR"
                    | "CARGO_MANIFEST_PATH"
                    | "CARGO_MANIFEST_LINKS"
                    | "CARGO_CRATE_NAME"
                    | "CARGO_BIN_NAME"
                    | "CARGO_PRIMARY_PACKAGE"
            )
        {
            command.env_remove(name);
        }
    }
    if args.contains(&"wasm32-unknown-unknown") {
        // Native binaries favor computation; the browser favors delivery size. Keep LTO and
        // the existing fixed payload budget instead of making it larger as features grow.
        command.env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "s");
        command.env("CARGO_ENCODED_RUSTFLAGS", wasm_build_flags(root)?);
    }
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed ({status})").into())
    }
}

/// The highlighted fence tags come from core's block tags, so a new block needs no grammar edit
/// (FEA-SF-59).
fn grammar_with_tags(grammar: &str) -> String {
    let tags = richview_core::RICH_TAGS.join("|");
    let marker = "(```)(";
    let start = grammar.find(marker).map_or(0, |at| at + marker.len());
    let end = grammar[start..]
        .find(")\\\\s*$")
        .map_or(start, |at| start + at);
    format!("{}{tags}{}", &grammar[..start], &grammar[end..])
}

fn stage_grammar(editor: &Path) -> Result<()> {
    let path = editor.join("syntaxes/rmd.tmLanguage.json");
    let grammar = fs::read_to_string(&path)?;
    let updated = grammar_with_tags(&grammar);
    if updated != grammar {
        return Err(format!(
            "{} has drifted; regenerate and commit the grammar before building (ARC-46)",
            path.display()
        )
        .into());
    }
    Ok(())
}

/// Bundles the activation module with its language client, so the package needs no
/// `node_modules` (ARC-45).
fn bundle_extension(editor: &Path) -> Result<()> {
    run(
        editor,
        "node",
        &[
            "../../node_modules/esbuild/bin/esbuild",
            "extension.cts",
            "--bundle",
            "--platform=node",
            "--format=cjs",
            "--external:vscode",
            "--outfile=dist/extension.js",
            "--metafile=dist/bundle-meta.json",
        ],
    )?;
    run(
        editor,
        "node",
        &[
            "../../scripts/vscode-notices.ts",
            "dist/bundle-meta.json",
            "dist/THIRD-PARTY-NOTICES.txt",
        ],
    )
}

fn stage_snippets(editor: &Path) -> Result<()> {
    let path = editor.join("snippets/rmd.json");
    if fs::read_to_string(&path)? != richview_editor::vscode_snippet_file() {
        return Err(format!(
            "{} has drifted; regenerate and commit the snippets before building (ARC-46)",
            path.display()
        )
        .into());
    }
    Ok(())
}

/// Highlight `<code data-hl="lang">` examples at build time so they read well without
/// JavaScript. The authored text stays escaped HTML; only spans are added.
fn highlight_code_examples(html: &str) -> String {
    let mut out = String::with_capacity(html.len() * 2);
    let mut rest = html;
    while let Some(start) = rest.find("<code data-hl=\"") {
        let open_end = match rest[start..].find('>') {
            Some(end) => start + end + 1,
            None => break,
        };
        let language = rest[start + 15..open_end - 1].trim_end_matches('"');
        let Some(close) = rest[open_end..].find("</code>") else {
            break;
        };
        let body = &rest[open_end..open_end + close];
        let text = body
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&amp;", "&");
        out.push_str(&rest[..open_end]);
        out.push_str(&richview_core::highlight_html(
            &text,
            richview_core::HighlightLanguage::from_name(language),
        ));
        rest = &rest[open_end + close..];
    }
    out.push_str(rest);
    out
}

fn stage_assets(root: &Path, public: &Path, install_enabled: bool) -> Result<()> {
    fs::copy(
        root.join("tmp/THIRD-PARTY-NOTICES.txt"),
        public.join("third-party-notices.txt"),
    )?;
    // Only the files a page loads are public; the assets folder is not mirrored (FEA-SF-46).
    for retired in ["assets", "reel"] {
        let _ = fs::remove_dir_all(public.join(retired));
    }
    // `_redirects` keeps retired routes pointing somewhere useful: `/charts/` went to the docs
    // gallery (ARC-40, FEA-SF-44).
    for file in [
        "_headers",
        "_redirects",
        "robots.txt",
        "site.css",
        "legal.css",
        "playground.css",
    ] {
        fs::copy(root.join("assets").join(file), public.join(file))?;
    }
    pin_inline_preview_runtime(public)?;
    // `404.html` is what the host serves for an unknown path, at any depth: its links are rooted.
    for file in [
        "about.html",
        "impressum.html",
        "privacy.html",
        "terms.html",
        "404.html",
    ] {
        let page = chrome::apply(&fs::read_to_string(root.join("assets").join(file))?);
        fs::write(public.join(file), page)?;
    }
    // A checkout built before the page was retired still holds its staged copy, which would
    // shadow the redirect.
    for retired in ["charts", "playground", "app"] {
        match fs::remove_dir_all(public.join(retired)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
    }
    let _ = fs::remove_file(public.join("app.html"));
    site_fonts::stage(root, public)?;
    let _ = fs::remove_file(public.join("landing/demo.rmd"));
    let _ = fs::remove_file(public.join("assets/landing/demo.rmd"));
    // Staged afresh, so a file the source no longer has does not outlive it in `dist/web`.
    let _ = fs::remove_dir_all(public.join("landing"));
    copy_tree(&root.join("assets/landing"), &public.join("landing"))?;
    copy_tree(&root.join("assets/share"), &public.join("share"))?;
    stage_home_share(root, public, install_enabled)?;
    // The live example's tabs and its pre-rendered page come from the hero's report folder, so
    // the page shows one report from the hero to the live example.
    let escape = |text: String| {
        text.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let demo_source = escape(fs::read_to_string(
        root.join("assets/landing/anchor/report.rmd"),
    )?);
    let demo_style = escape(fs::read_to_string(
        root.join("assets/landing/anchor/report.rstyle.yaml"),
    )?);
    let landing = signup_copy(
        &fs::read_to_string(root.join("assets/landing/index.html"))?,
        install_enabled,
    )?;
    let homepage = highlight_code_examples(&chrome::apply(&chrome::apply_styles(
        &landing
            .replace("<!-- richview-demo-source -->", &demo_source)
            .replace("<!-- richview-demo-style -->", &demo_style)
            .replace("<!-- rv-demo-check -->", &demo_check_json(root, public)?)
            .replace("<!-- rv-source-count -->", &flagship_source_count(root)?)
            // The registry's number of chart types; the page never types one (DEL-18).
            .replace(
                "<!-- rv-chart-count -->",
                &documentation::chart_count(root)?.to_string(),
            ),
    )))
    .replace(chrome::AGENT_PROMPT, chrome::agent_prompt(install_enabled));
    // The landing ships static and complete until Daniel accepts its scroll story (STY-44): without
    // the canvas and the stage the page never starts the story, and shows the same copy. The story
    // is the opt-in: `RICHVIEW_LANDING=story` builds it for work on it.
    let homepage = if std::env::var("RICHVIEW_LANDING").is_ok_and(|mode| mode == "story") {
        homepage
    } else {
        static_landing(&homepage)
    };
    fs::write(public.join("index.html"), &homepage)?;
    // The playground answers at `/playground`; `/app` redirects to it (FEA-SF-33, `_redirects`).
    let playground = chrome::apply(&chrome::apply_styles(&fs::read_to_string(
        root.join("assets/playground.html"),
    )?));
    fs::write(public.join("playground.html"), &playground)?;
    stage_public_examples(&root.join("examples"), &public.join("examples"))?;
    Ok(())
}

/// Compile each handwritten host script into its existing classic browser entrypoint.
fn stage_browser_scripts(root: &Path, public: &Path) -> Result<()> {
    let font_catalog_url = format!("--define:FONT_CATALOG_URL=\"/{FONT_CATALOG}\"");
    for (source, destination) in [
        ("assets/browser/landing.ts", "landing/bootstrap.js"),
        ("assets/browser/bootstrap.ts", "bootstrap.js"),
        ("assets/browser/report-runtime.ts", "report-runtime.js"),
        ("assets/browser/worker.ts", "worker.js"),
    ] {
        let output = public.join(destination);
        let output = output
            .to_str()
            .ok_or("Invalid browser script output path")?;
        run(
            root,
            "node",
            &[
                "node_modules/esbuild/bin/esbuild",
                source,
                "--format=iife",
                "--target=es2020",
                "--minify-whitespace",
                "--log-level=warning",
                &font_catalog_url,
                &format!("--outfile={output}"),
            ],
        )?;
    }
    Ok(())
}

fn signup_copy(page: &str, install_enabled: bool) -> Result<String> {
    let current =
        "<label class=\"updates__label\" for=\"interest-email\">Get release updates</label>";
    if page.matches(current).count() != 1 {
        return Err("landing signup label is missing or repeated".into());
    }
    let text = if install_enabled {
        "Get release updates."
    } else {
        "Coming soon."
    };
    Ok(page.replacen(
        current,
        &format!("<label class=\"updates__label\" for=\"interest-email\">{text}</label>"),
        1,
    ))
}

fn stage_home_share(root: &Path, public: &Path, install_enabled: bool) -> Result<()> {
    let source = if install_enabled {
        "assets/site-home-launched.png"
    } else {
        "assets/share/home.png"
    };
    fs::copy(root.join(source), public.join("share/home.png"))?;
    Ok(())
}

/// Add the exact embedded preview module to each parent policy inherited by a srcdoc frame.
fn pin_inline_preview_runtime(public: &Path) -> Result<()> {
    use base64::Engine as _;
    let js = fs::read_to_string(public.join("richview_runtime.js"))?;
    let wasm = fs::read(public.join("richview_runtime_bg.wasm"))?;
    if js.is_empty() || !wasm.starts_with(b"\0asm") {
        return Err("Preview runtime bindings or Wasm are invalid".into());
    }
    let options = richview_core::document::RenderOptions {
        runtime_js: Some(js),
        runtime_wasm: Some(base64::engine::general_purpose::STANDARD.encode(wasm)),
        report_folder: Some(richview_core::document::ReportFolder {
            root: "preview-policy".into(),
            source: "report.rmd".into(),
            files: vec!["report.rmd".into()],
        }),
        ..Default::default()
    };
    let report = richview_core::document::render("# Preview policy", &options)
        .map_err(|errors| format!("Cannot render preview policy: {errors:?}"))?;
    let module = report
        .html
        .split_once("<script type=\"module\">")
        .and_then(|(_, rest)| rest.split_once("</script>"))
        .map(|(body, _)| body)
        .ok_or("Preview runtime module is missing")?;
    let hash = format!(
        "'sha256-{}'",
        base64::engine::general_purpose::STANDARD.encode(Sha256::digest(module.as_bytes()))
    );
    let path = public.join("_headers");
    let headers = fs::read_to_string(&path)?;
    let mut output = String::with_capacity(headers.len() + 3 * hash.len());
    let mut target = 0;
    let mut updated = 0;
    for line in headers.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('/') && !line.starts_with([' ', '\t']) {
            target = match trimmed {
                "/" => 1,
                "/playground" => 2,
                "/docs/*" => 4,
                _ => 0,
            };
        }
        if target != 0 && trimmed.starts_with("Content-Security-Policy:") {
            if updated & target != 0 {
                return Err("Duplicate preview CSP".into());
            }
            let start = line
                .find("script-src ")
                .ok_or("Preview CSP has no script directive")?;
            let end = line[start..]
                .find(';')
                .map(|i| start + i)
                .ok_or("Preview CSP script directive is unterminated")?;
            if line[start..end].contains(&hash) {
                return Err("Preview CSP script directive is invalid".into());
            }
            output.push_str(&line[..end]);
            output.push(' ');
            output.push_str(&hash);
            output.push_str(&line[end..]);
            updated |= target;
        } else {
            output.push_str(line);
        }
    }
    if updated != 7 {
        return Err("Missing preview CSP".into());
    }
    fs::write(path, output)?;
    Ok(())
}

/// The number of dated sources in the flagship's ledger, one row each under the header (FEA-SF-62).
fn flagship_source_count(root: &Path) -> Result<String> {
    let ledger = fs::read_to_string(root.join("examples/openai-vs-anthropic/ledger/sources.csv"))?;
    Ok(ledger
        .lines()
        .skip(1)
        .filter(|row| !row.trim().is_empty())
        .count()
        .to_string())
}

/// The landing without its scroll story: no canvas, no stage, and a marker the stylesheet reads to
/// keep the static mark visible.
fn static_landing(page: &str) -> String {
    let mut page = page.replacen("<html ", "<html data-landing=\"static\" ", 1);
    for (open, close) in [
        ("<canvas id=\"rv-field\"", "</canvas>"),
        ("<svg id=\"rv-stage\"", "</svg>"),
    ] {
        if let Some(start) = page.find(open)
            && let Some(end) = page[start..].find(close)
        {
            page.replace_range(start..start + end + close.len(), "");
        }
    }
    page
}

/// The output of `check` on the live example as shipped, the hero's report, written into the
/// home page so the diagnostics disclosure opens without a request or JavaScript (FEA-SF-57).
fn demo_check_json(root: &Path, public: &Path) -> Result<String> {
    let demo_file = root.join("assets/landing/anchor/report.rmd");
    let demo_source = fs::read_to_string(&demo_file)?;
    let (document, options, _) = resolve_document_options(&demo_source, &demo_file, &[], public)?;
    let checked = richview_core::document::check_document(document, &options)
        .map_err(|errors| format!("Demo check failed: {errors:?}"))?;
    let json = serde_json::to_string_pretty(&checked)?;
    Ok(json
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;"))
}

fn stage_reports(root: &Path, public: &Path) -> Result<()> {
    flagship::stage(root, public)?;
    // The anchor report (INT-01) in the default style is `landing/anchor.html`, the page the live
    // example's scene writes, and the voices scene shows the same report in each other style,
    // `landing/anchor-<style>.html`; all link one hashed copy of the runtime. The live example
    // shows the same report, rendered from the source in its tabs, until the visitor edits it
    // (FEA-SF-29), and keeps its runtime inside: its frame is sandboxed, and WebKit does not match
    // the report's connect-src 'self' from the frame's opaque origin, so it could not fetch a
    // linked runtime's Wasm.
    let runtime = stage_hashed_runtime(public)?;
    let styles = chrome::site_styles();
    for (index, style) in styles.iter().enumerate() {
        let page = if index == 0 {
            "anchor".to_owned()
        } else {
            format!("anchor-{style}")
        };
        stage_landing_report(root, public, "anchor", style, &page, Some(&runtime))?;
    }
    stage_landing_report(root, public, "anchor", styles[0], "demo", None)?;
    // The home page's best-elements report sits in a sandboxed frame like the live example's, so it
    // keeps its runtime inside as well.
    stage_landing_report(root, public, "elements", styles[0], "elements", None)?;
    let landing = public.join("landing");
    fs::write(landing.join("voices.css"), chrome::voices_css())?;
    // The raw page template is an input, not a route (FEA-SF-46).
    let _ = fs::remove_file(landing.join("index.html"));
    let _ = fs::remove_dir_all(landing.join("generated"));
    Ok(())
}

/// Stages the report runtime once more for the landing reports, in a folder named by its content:
/// `landing/runtime/<hash>/richview_runtime.js` and its `_bg.wasm`. A host serves the runtime from
/// a content-hashed URL cached for ever (PRF-31, as proposed; the site's headers mark the folder
/// immutable), so a new build never pairs a cached copy with a new report. Returns the URL base the
/// reports link.
fn stage_hashed_runtime(public: &Path) -> Result<String> {
    let js = fs::read(public.join("richview_runtime.js"))?;
    let wasm = fs::read(public.join("richview_runtime_bg.wasm"))?;
    let mut hash = Sha256::new();
    hash.update(&js);
    hash.update(&wasm);
    let hash = format!("{:x}", hash.finalize());
    let folder = public.join("landing/runtime").join(&hash[..16]);
    fs::create_dir_all(&folder)?;
    fs::write(folder.join("richview_runtime.js"), &js)?;
    fs::write(folder.join("richview_runtime_bg.wasm"), &wasm)?;
    Ok(format!("/landing/runtime/{}/richview_runtime", &hash[..16]))
}

/// Renders `assets/landing/<name>/report.rmd` in `style` to `landing/<page>.html`, and removes the
/// copied report folder, an input rather than a route (FEA-SF-46). Given a `runtime`, the page links
/// that runtime instead of embedding it: it is the build's own report, served beside that runtime,
/// while an exported report keeps its runtime inside (INT-04: self-contained by default, linked on
/// a server). Without one it embeds the runtime.
fn stage_landing_report(
    root: &Path,
    public: &Path,
    name: &str,
    style: &str,
    page: &str,
    runtime: Option<&str>,
) -> Result<()> {
    let file = root.join("assets/landing").join(name).join("report.rmd");
    let source = fs::read_to_string(&file)?;
    let variant = richview_core::with_primary_style(&source, style)
        .ok_or_else(|| format!("The {name} report needs frontmatter"))?;
    let mut options = resolve_document_options(&variant, &file, &[], public)?.1;
    if let Some(runtime) = runtime {
        options.runtime_url = Some(runtime.into());
        options.runtime_wasm = None;
    }
    let report = richview_core::document::render(&variant, &options)
        .map_err(|errors| format!("The {name} report failed: {errors:?}"))?;
    let landing = public.join("landing");
    fs::create_dir_all(&landing)?;
    fs::write(landing.join(format!("{page}.html")), report.html)?;
    let _ = fs::remove_dir_all(landing.join(name));
    Ok(())
}

// Research collectors and their local environments/caches belong in the source
// repository, never in the served product. Recreate this subtree so older builds
// cannot leave private scratch files behind after the policy changes.
fn stage_public_examples(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    copy_public_examples(source, destination)
}

fn copy_public_examples(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.')
            || matches!(
                name.as_ref(),
                "research" | "tmp" | "__pycache__" | "node_modules" | "target"
            )
            || name.ends_with(".pyc")
            || name.ends_with(".pyo")
        {
            continue;
        }
        // Check exclusions before inspecting symlinks: an excluded .venv/cache
        // may contain symlinks, while a genuine product asset must stay explicit.
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!(
                "Refusing public example asset symlink {}",
                entry.path().display()
            )
            .into());
        }
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_public_examples(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(format!("Refusing asset symlink {}", entry.path().display()).into());
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        } else if kind.is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Resolves build inputs using the runtime this build just staged.
fn resolve_document_options(
    source: &str,
    path: &Path,
    styles: &[String],
    public: &Path,
) -> Result<(
    richview_core::document::Document,
    richview_core::document::RenderOptions,
    richview_app::input_limits::InputSizes,
)> {
    let (document, mut options, inputs) =
        richview_app::resolver::resolve_version(source, path, styles, false, false, None)?;
    runtime::load(public, &mut options)?;
    Ok((document, options, inputs))
}

#[cfg(test)]
mod tests {
    #[test]
    fn assets_only_refuses_wasm_another_checkout_compiled_last() {
        use std::fs;
        use std::path::Path;
        let base = std::env::temp_dir().join(format!("rv-own-wasm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (target, spelled, other) = (
            base.join("target"),
            base.join("checkout"),
            base.join("other"),
        );
        for dir in [&target.join(".rv-fast"), &spelled, &other] {
            fs::create_dir_all(dir).unwrap();
        }
        let checkout = spelled.canonicalize().unwrap();
        let record = |profile: &str, owner: &Path| {
            let name = format!("last-wasm32-unknown-unknown--{profile}");
            fs::write(
                target.join(".rv-fast").join(name),
                format!("{}\n", owner.display()),
            )
            .unwrap();
        };
        // A target without records, such as a checkout's own, binds as before.
        super::own_wasm(&target, &checkout).unwrap();
        // The wrapper writes the shell's spelling of the checkout, which may run through a link.
        record("wasm-site", &spelled);
        record("wasm-release", &checkout);
        super::own_wasm(&target, &checkout).unwrap();
        for (profile, package) in [
            ("wasm-site", "-p richview-site --lib --bin docs"),
            ("wasm-release", "-p richview-runtime --lib"),
        ] {
            record(profile, &other);
            let error = super::own_wasm(&target, &checkout).unwrap_err().to_string();
            assert!(
                error.contains(&format!("wasm32-unknown-unknown/{profile}"))
                    && error.contains(package)
                    && error.contains(&other.display().to_string())
                    && error.contains("--wasm-from"),
                "{error}"
            );
            record(profile, &checkout);
        }
        super::own_wasm(&target, &checkout).unwrap();
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn wasm_from_binds_the_given_folder_and_refuses_a_missing_folder_or_the_editor_build() {
        use std::path::Path;
        let target = Path::new("/target");
        assert_eq!(
            super::raw_wasm(target, None, "wasm-release", "richview_runtime.wasm"),
            Path::new("/target/wasm32-unknown-unknown/wasm-release/richview_runtime.wasm")
        );
        assert_eq!(
            super::raw_wasm(
                target,
                Some(Path::new("/from")),
                "wasm-release",
                "richview_runtime.wasm"
            ),
            Path::new("/from/richview_runtime.wasm")
        );
        let args = |list: &[&str]| list.iter().map(Into::into).collect::<Vec<_>>();
        assert!(
            super::wasm_from_arg(&args(&["--repository-public"]), false)
                .unwrap()
                .is_none()
        );
        for (list, editor_only, message) in [
            (&["--wasm-from"][..], false, "needs the folder"),
            (
                &["--wasm-from", "--repository-public"][..],
                false,
                "needs the folder",
            ),
            (&["--wasm-from", "tmp"][..], true, "--editor-only"),
        ] {
            let error = super::wasm_from_arg(&args(list), editor_only).unwrap_err();
            assert!(error.to_string().contains(message), "{list:?}: {error}");
        }
    }

    #[test]
    fn wasm_build_nested_cargo_home_is_the_last_matching_remap() {
        let flags = super::remap_wasm_paths(
            ["--cfg=example_flag"].into(),
            std::path::Path::new("/checkout"),
            std::path::Path::new("/checkout/.cargo"),
        )
        .unwrap();
        assert_eq!(
            flags.split('\x1f').collect::<Vec<_>>(),
            [
                "--cfg=example_flag",
                "--remap-path-prefix=/checkout=/richview",
                "--remap-path-prefix=/checkout/.cargo=/cargo",
            ]
        );
        let flags = super::remap_wasm_paths(
            Default::default(),
            std::path::Path::new("/checkout"),
            std::path::Path::new("/checkout-other/.cargo"),
        )
        .unwrap();
        assert!(flags.ends_with("--remap-path-prefix=/checkout-other/.cargo=/cargo"));
    }

    #[cfg(unix)]
    #[test]
    fn wasm_build_keeps_cargo_config_flags_and_normalizes_host_paths() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
        std::fs::create_dir_all(&root).unwrap();
        for section in ["build", "target.wasm32-unknown-unknown"] {
            let fixture = tempfile::tempdir_in(&root).unwrap();
            let checkout = fixture.path().join("checkout with spaces");
            std::fs::create_dir_all(checkout.join(".cargo")).unwrap();
            std::fs::write(
                checkout.join(".cargo/config.toml"),
                format!("[{section}]\nrustflags = ['--cfg=example_flag']\n"),
            )
            .unwrap();
            super::run(
                &checkout,
                "sh",
                &[
                    "-c",
                    "printf %s \"${CARGO_ENCODED_RUSTFLAGS-}\" > flags",
                    "probe",
                    "wasm32-unknown-unknown",
                ],
            )
            .unwrap();
            let flags = std::fs::read_to_string(checkout.join("flags")).unwrap();
            assert!(flags.split('\x1f').any(|flag| flag == "--cfg=example_flag"));
            assert!(flags.split('\x1f').any(|flag| flag.ends_with("=/cargo")));
            let expected = format!("--remap-path-prefix={}=/richview", checkout.display());
            assert!(flags.split('\x1f').any(|flag| flag == expected.as_str()));
        }
    }

    #[test]
    fn home_copy_and_share_follow_both_install_states() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let page = std::fs::read_to_string(root.join("assets/landing/index.html")).unwrap();
        std::fs::create_dir_all(root.join("tmp")).unwrap();
        let output_dir = tempfile::tempdir_in(root.join("tmp")).unwrap();
        let output = output_dir.path();
        std::fs::create_dir_all(output.join("share")).unwrap();
        for (enabled, expected, image) in [
            (false, "Coming soon.", "assets/share/home.png"),
            (
                true,
                "Get release updates.",
                "assets/site-home-launched.png",
            ),
        ] {
            let html = super::signup_copy(&page, enabled).unwrap();
            assert!(html.contains(&format!("for=\"interest-email\">{expected}</label>")));
            super::stage_home_share(&root, output, enabled).unwrap();
            assert_eq!(
                std::fs::read(output.join("share/home.png")).unwrap(),
                std::fs::read(root.join(image)).unwrap()
            );
        }
    }
    #[test]
    fn local_preview_omits_install_without_weakening_release_modes() {
        for enabled in [false, true] {
            assert!(!super::staged_install_enabled(enabled, true, false, true).unwrap());
            assert_eq!(
                super::staged_install_enabled(enabled, false, false, false).unwrap(),
                enabled
            );
        }
        assert!(super::staged_install_enabled(true, true, false, false).is_err());
        assert!(!super::staged_install_enabled(false, true, false, false).unwrap());
        assert!(super::staged_install_enabled(true, false, false, true).is_err());
        assert!(super::staged_install_enabled(false, true, true, true).is_err());
    }

    #[test]
    fn installing_requires_a_public_repository() {
        for (install, public, ok) in [
            (false, false, true),
            (false, true, true),
            (true, false, false),
            (true, true, true),
        ] {
            let result = super::require_public_repository_for_install(install, public);
            assert_eq!(result.is_ok(), ok);
            if !ok {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("--repository-public")
                );
            }
        }
    }

    #[test]
    fn staged_site_policies_pin_the_exact_inline_preview_module() {
        use base64::Engine as _;
        let dir = tempfile::tempdir().unwrap();
        let public = dir.path();
        let headers = include_str!("../../../assets/_headers");
        let js = "const richview_runtime = async()=>{};";
        let wasm = b"\0asm\x01\0\0\0";
        fs::write(public.join("_headers"), headers).unwrap();
        fs::write(public.join("richview_runtime.js"), js).unwrap();
        fs::write(public.join("richview_runtime_bg.wasm"), wasm).unwrap();
        super::pin_inline_preview_runtime(public).unwrap();
        let options = richview_core::document::RenderOptions {
            runtime_js: Some(js.into()),
            runtime_wasm: Some(base64::engine::general_purpose::STANDARD.encode(wasm)),
            report_folder: Some(richview_core::document::ReportFolder {
                root: "test".into(),
                source: "report.rmd".into(),
                files: vec!["report.rmd".into()],
            }),
            ..Default::default()
        };
        let report = richview_core::document::render("# Test", &options).unwrap();
        let module = report
            .html
            .split_once("<script type=\"module\">")
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0;
        let hash = format!(
            "'sha256-{}'",
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(module.as_bytes()))
        );
        let staged = fs::read_to_string(public.join("_headers")).unwrap();
        for page in ["\n/\n", "\n/playground\n", "\n/docs/*\n"] {
            let block = staged
                .split(page)
                .nth(1)
                .unwrap()
                .split("\n\n")
                .next()
                .unwrap();
            assert!(block.contains(&hash), "{page}: {block}");
        }
        assert_eq!(staged.replace(&format!(" {hash}"), ""), headers);
        fs::write(public.join("_headers"), headers).unwrap();
        fs::write(public.join("richview_runtime.js"), format!("{js} ")).unwrap();
        super::pin_inline_preview_runtime(public).unwrap();
        assert!(
            !fs::read_to_string(public.join("_headers"))
                .unwrap()
                .contains(&hash)
        );
        fs::write(
            public.join("_headers"),
            headers.replace("\n/playground\n", "\n/playground-missing\n"),
        )
        .unwrap();
        assert!(super::pin_inline_preview_runtime(public).is_err());
        fs::write(public.join("richview_runtime_bg.wasm"), b"not wasm").unwrap();
        assert!(super::pin_inline_preview_runtime(public).is_err());
    }

    #[test]
    fn the_landing_runtime_copy_is_named_by_its_content() {
        let public = std::env::temp_dir().join(format!("rv-hashed-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&public);
        fs::create_dir_all(&public).unwrap();
        let stage = |js: &str| {
            fs::write(public.join("richview_runtime.js"), js).unwrap();
            fs::write(public.join("richview_runtime_bg.wasm"), b"\0asm").unwrap();
            super::stage_hashed_runtime(&public).unwrap()
        };
        let first = stage("one");
        let folder = first
            .strip_prefix("/landing/runtime/")
            .and_then(|rest| rest.strip_suffix("/richview_runtime"))
            .unwrap();
        assert_eq!(folder.len(), 16);
        assert!(folder.chars().all(|c| c.is_ascii_hexdigit()), "{first}");
        let copy = public.join("landing/runtime").join(folder);
        assert_eq!(fs::read(copy.join("richview_runtime.js")).unwrap(), b"one");
        assert_eq!(
            fs::read(copy.join("richview_runtime_bg.wasm")).unwrap(),
            b"\0asm"
        );
        // The same runtime keeps its name; a changed one gets a new name.
        assert_eq!(stage("one"), first);
        assert_ne!(stage("two"), first);
        fs::remove_dir_all(&public).unwrap();
    }

    #[test]
    fn the_static_landing_has_no_canvas_or_stage_and_says_so() {
        let page = "<html lang=\"en\"><body><canvas id=\"rv-field\" class=\"rv-field\"></canvas>\n<svg id=\"rv-stage\" class=\"rv-stage\"></svg><main>Copy</main></body></html>";
        let out = super::static_landing(page);
        assert!(out.starts_with("<html data-landing=\"static\" lang"));
        assert!(!out.contains("rv-field") && !out.contains("rv-stage"));
        assert!(out.contains("<main>Copy</main>"));
    }

    #[test]
    fn the_committed_grammar_names_every_block_tag_core_parses() {
        let grammar = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../packages/vscode/syntaxes/rmd.tmLanguage.json"),
        )
        .unwrap();
        assert_eq!(grammar_with_tags(&grammar), grammar);
        for tag in richview_core::RICH_TAGS {
            assert!(grammar.contains(tag), "{tag}");
        }
    }

    use super::*;

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn the_editor_package_carries_the_worker_and_refuses_an_incomplete_pair() {
        let fixture = Fixture(Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../tmp/native-editor-pair-{}",
            std::process::id()
        )));
        let target = fixture.0.join("target");
        let editor = fixture.0.join("editor");
        fs::create_dir_all(target.join("release")).unwrap();
        let suffix = std::env::consts::EXE_SUFFIX;
        let backend = format!("richview-editor{suffix}");
        let worker = format!("richview-sql-worker{suffix}");
        fs::write(target.join("release").join(&backend), b"editor bytes").unwrap();
        fs::write(target.join("release").join(&worker), b"worker bytes").unwrap();
        stage_native_binaries(&editor, &target).unwrap();
        assert_eq!(
            fs::read(editor.join("bin").join(&backend)).unwrap(),
            b"editor bytes"
        );
        assert_eq!(
            fs::read(editor.join("bin").join(&worker)).unwrap(),
            b"worker bytes"
        );
        fs::remove_file(target.join("release").join(&worker)).unwrap();
        fs::write(target.join("release").join(&backend), b"a different editor").unwrap();
        assert!(
            stage_native_binaries(&editor, &target)
                .unwrap_err()
                .to_string()
                .contains(&worker)
        );
        assert_eq!(
            fs::read(editor.join("bin").join(&backend)).unwrap(),
            b"editor bytes"
        );
    }
    #[test]
    fn generated_snippet_and_grammar_drift_fails_without_rewriting_tracked_files() {
        let fixture = Fixture(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../tmp/generated-drift-{}", std::process::id())),
        );
        for directory in ["snippets", "syntaxes"] {
            fs::create_dir_all(fixture.0.join(directory)).unwrap();
        }
        let snippets = fixture.0.join("snippets/rmd.json");
        let grammar = fixture.0.join("syntaxes/rmd.tmLanguage.json");
        fs::write(&snippets, "stale snippets").unwrap();
        fs::write(&grammar, "stale grammar").unwrap();
        assert!(stage_snippets(&fixture.0).is_err());
        assert!(stage_grammar(&fixture.0).is_err());
        assert_eq!(fs::read_to_string(snippets).unwrap(), "stale snippets");
        assert_eq!(fs::read_to_string(grammar).unwrap(), "stale grammar");
    }
    #[test]
    fn the_checkout_cargo_runs_in_wins_over_the_compiled_path_unless_it_points_elsewhere() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
        fs::create_dir_all(&root).unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fixture = Fixture(root.join(format!("build-root-{}-{nonce}", std::process::id())));
        let checkout = |name: &str| {
            let dir = fixture.0.join(name);
            fs::create_dir_all(dir.join("crates/richview-build")).unwrap();
            fs::create_dir_all(dir.join("richview-chart")).unwrap();
            fs::write(dir.join("crates/richview-build/Cargo.toml"), "").unwrap();
            dir.canonicalize().unwrap()
        };
        let (here, compiled) = (checkout("here"), checkout("compiled"));
        let compiled_manifest = compiled.join("crates/richview-build");
        assert_eq!(
            repo_root(Some(here.join("crates/richview-build")), &compiled_manifest).unwrap(),
            here,
            "cargo run's checkout wins"
        );
        assert_eq!(
            repo_root(None, &compiled_manifest).unwrap(),
            compiled,
            "run directly, the compiled path stays"
        );
        assert_eq!(
            repo_root(Some(here.join("richview-chart")), &compiled_manifest).unwrap(),
            compiled,
            "a parent cargo's variable for another package is ignored"
        );
    }

    #[test]
    fn example_publication_keeps_report_data_and_excludes_private_research_scratch() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tmp");
        fs::create_dir_all(&root).unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fixture = Fixture(root.join(format!("public-examples-{}-{nonce}", std::process::id())));
        let source = fixture.0.join("source");
        let destination = fixture.0.join("public");
        for relative in [
            "flagship/data",
            "flagship/research/.venv",
            "flagship/tmp",
            "flagship/__pycache__",
            "flagship/node_modules",
            "flagship/target",
            "flagship/.venv",
            "flagship/content",
            "public-old",
        ] {
            fs::create_dir_all(source.join(relative)).unwrap();
        }
        let report = b"---\r\ntitle: Exact source\r\n---\r\n";
        let csv = b"name,value\n\"Unicode \xc4\x8d\",1.250\n";
        fs::write(source.join("flagship/report.rmd"), report).unwrap();
        fs::write(source.join("flagship/data/observations.csv"), csv).unwrap();
        fs::write(
            source.join("flagship/content/report.template.md"),
            b"{{chart:traceable}}",
        )
        .unwrap();
        for relative in [
            "flagship/.env",
            "flagship/.env.local",
            "flagship/research/collector.py",
            "flagship/research/.venv/token",
            "flagship/tmp/raw-review.json",
            "flagship/__pycache__/labels.pyc",
            "flagship/node_modules/private",
            "flagship/target/build",
            "flagship/cache.pyc",
            "flagship/cache.pyo",
        ] {
            fs::write(source.join(relative), b"private research material").unwrap();
        }
        fs::create_dir_all(destination.join("flagship/research")).unwrap();
        fs::write(destination.join("flagship/research/stale-secret"), b"stale").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            "missing-private-environment",
            source.join("flagship/.venv/python"),
        )
        .unwrap();
        stage_public_examples(&source, &destination).unwrap();
        assert_eq!(
            fs::read(destination.join("flagship/report.rmd")).unwrap(),
            report
        );
        assert_eq!(
            fs::read(destination.join("flagship/data/observations.csv")).unwrap(),
            csv
        );
        assert!(
            destination
                .join("flagship/content/report.template.md")
                .exists()
        );
        for relative in [
            "flagship/research",
            "flagship/.env",
            "flagship/.env.local",
            "flagship/.venv",
            "flagship/tmp",
            "flagship/__pycache__",
            "flagship/node_modules",
            "flagship/target",
            "flagship/cache.pyc",
            "flagship/cache.pyo",
        ] {
            assert!(
                !destination.join(relative).exists(),
                "{relative} must never be served"
            );
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                "observations.csv",
                source.join("flagship/data/asset-link.csv"),
            )
            .unwrap();
            let error = stage_public_examples(&source, &destination)
                .unwrap_err()
                .to_string();
            assert!(error.contains("asset symlink"));
        }
    }
}
