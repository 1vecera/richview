"""Stage the bounded docs/flagship release over a verified production asset snapshot.

Run with uv run --no-project. This prepares local assets and never deploys them.
The baseline must be captured from the currently served, verified deployment.
"""

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import zipfile
from pathlib import Path


def inventory(root: Path) -> dict[str, str]:
    """Hash explicit files and reject caches, private sources and symlink escapes."""
    hashes = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root)
        if path.is_symlink() or any(
            part.startswith(".") or part in {"research", "tmp", "node_modules", "target", "__pycache__"}
            for part in relative.parts
        ):
            raise ValueError(f"Unsafe public asset: {relative}")
        if path.is_file():
            hashes[relative.as_posix()] = hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes


def stage(baseline: Path, built: Path, docs: Path, output: Path, commit: str) -> dict[str, object]:
    """Overlay only approved routes, isolate the runtime and retain all other baseline bytes."""
    before = inventory(baseline)
    inventory(built / "showcase/openai-vs-anthropic")
    inventory(built / "examples/openai-vs-anthropic")
    docs_hashes = inventory(docs)
    shutil.copytree(baseline, output)
    prefixes = ["showcase/openai-vs-anthropic/", "examples/openai-vs-anthropic/", "docs/"]
    for relative, source in [
        ("showcase/openai-vs-anthropic", built / "showcase/openai-vs-anthropic"),
        ("examples/openai-vs-anthropic", built / "examples/openai-vs-anthropic"),
        ("docs", docs),
    ]:
        shutil.copytree(source, output / relative, dirs_exist_ok=True)
    shared = ["showcase/themes.json", "showcase/fonts.css", "showcase/themes.css"]
    if inventory(output / "docs") != docs_hashes:
        raise ValueError("Staged docs differ from the reviewed producer artifact")
    for relative in shared:
        shutil.copyfile(built / relative, output / relative)
    runtime = f"releases/{commit}"
    (output / runtime).mkdir(parents=True)
    for name in ["richview_site.js", "richview_site_bg.wasm", "richview_site_bg.wasm.gz",
                 "bootstrap.js", "worker.js", "playground.html"]:
        shutil.copyfile(built / name, output / runtime / name)
    bootstrap = output / runtime / "bootstrap.js"
    bootstrap.write_text(bootstrap.read_text().replace("/richview_site_bg.wasm", f"/{runtime}/richview_site_bg.wasm"))
    for name in ["worker.js", "playground.html"]:
        path = output / runtime / name
        text = path.read_text()
        for asset in ["richview_site.js", "richview_site_bg.wasm", "bootstrap.js"]:
            text = text.replace(f"/{asset}", f"/{runtime}/{asset}")
        path.write_text(text)
    page = output / "showcase/openai-vs-anthropic/index.html"
    html = page.read_text()
    for name in ["richview_site.js", "bootstrap.js"]:
        needle = f'src="/{name}"'
        if html.count(needle) != 1:
            raise ValueError(f"Expected one showcase runtime reference: {name}")
        html = html.replace(needle, f'src="/{runtime}/{name}"')
    html, replacements = re.subn(
        r'(<a[^>]+id="source-download"[^>]*>.*?</a>)',
        r'\1<a href="report-bundle.zip" download>Bundle</a><a href="/docs/">Docs</a>',
        html,
        count=1,
    )
    if replacements != 1:
        raise ValueError("Missing showcase source toolbar link")
    page.write_text(html)
    page.write_text(page.read_text().replace(
        'href="/playground?src=', f'href="/{runtime}/playground.html?src='))
    # Retain the previously public direct report URL as an alias to this exact
    # export, with local links relative to the alias's report-folder root.
    direct = output / "showcase/openai-vs-anthropic/report.html"
    direct.write_bytes((direct.parent / "Rendered/report.html").read_bytes())
    headers = output / "_headers"
    headers.write_text(headers.read_text() + f"\n/{runtime}/*\n  Access-Control-Allow-Origin: *\n")
    report = output / "showcase/openai-vs-anthropic"
    # Native export already copied each local link into Rendered. Keep that output
    # with source/data/style, so unzip + moving the folder preserves those links.
    bundle_roots = {"report.rmd", "report.rstyle.yaml", "README.md", "content", "data", "ledger", "Rendered"}
    with zipfile.ZipFile(report / "report-bundle.zip", "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for relative in inventory(report):
            if Path(relative).parts[0] in bundle_roots:
                info = zipfile.ZipInfo(f"openai-vs-anthropic/{relative}", (2026, 10, 2, 0, 0, 0))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = 0o100644 << 16
                archive.writestr(info, (report / relative).read_bytes())
    after = inventory(output)
    changed = sorted(path for path, digest in after.items() if before.get(path) != digest)
    allowed = prefixes + [runtime + "/"]
    for relative in changed:
        if relative not in shared + ["_headers"] and not relative.startswith(tuple(allowed)):
            raise ValueError(f"Unapproved route change: {relative}")
    if before.keys() - after.keys():
        raise ValueError("The release removed baseline assets")
    preserved = sorted(path for path in before if path not in changed)
    return {"source_commit": commit, "runtime_prefix": runtime, "changed": changed, "preserved": preserved,
            "baseline_assets": before, "assets": after}


def main() -> None:
    """Prepare a new directory and keep release evidence outside the served assets."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--built", type=Path, default=Path("dist/web"))
    parser.add_argument("--docs", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    if subprocess.check_output(["git", "status", "--porcelain"], text=True).strip():
        raise ValueError("Commit the reviewed source changes before staging a release")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    result = stage(args.baseline, args.built, args.docs, args.output, commit)
    args.manifest.write_text(json.dumps(result, indent=2) + "\n")
    print(f"Staged {len(result['changed'])} approved assets; preserved {len(result['preserved'])} baseline files")


if __name__ == "__main__":
    main()
