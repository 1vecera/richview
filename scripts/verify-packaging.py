"""Verify the locally built VSIX, the font notices and the third-party notices.

Run after the full Rust build; --package packages this host's VSIX first:
uv run --no-project python scripts/verify-packaging.py --package
"""

import argparse
import hashlib
import platform
import subprocess
import xml.etree.ElementTree as ET
import json
import zipfile
from pathlib import Path

root = Path(__file__).resolve().parents[1]
licenses = sorted((root / "assets/showcase/fonts/licenses").glob("*.txt"))
npm_notices = root / "packages/vscode/dist/THIRD-PARTY-NOTICES.txt"
# Every shipped family has its notice and every notice names a shipped family (STY-27, STY-28);
# the count follows the fonts rather than a number kept by hand.
families = {font.name.split("-latin")[0].split("-greek")[0] for font in (root / "assets/showcase/fonts").glob("*.woff2")}
assert families, "no shipped font found under assets/showcase/fonts"
assert families == {license.stem for license in licenses}, (sorted(families ^ {license.stem for license in licenses}))
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--vsix", type=Path, help="Explicit locally built VSIX; defaults to this host and package version")
parser.add_argument("--package", action="store_true", help="Package this host's VSIX first (bundle and vsce), after the release build staged the editor")
args = parser.parse_args()
system = {"Linux": "linux", "Darwin": "darwin", "Windows": "win32"}.get(platform.system())
arch = {"x86_64": "x64", "AMD64": "x64", "aarch64": "arm64", "arm64": "arm64", "armv7l": "armhf"}.get(platform.machine())
assert system and arch, "Use a supported native host for package verification"
target = f"{system}-{arch}"
version = json.loads((root / "packages/vscode/package.json").read_text())["version"]
vsix = (args.vsix or root / f"packages/vscode/richview-vscode-{target}-{version}.vsix").resolve()
if args.package:
    subprocess.run(["npm", "--workspace", "richview-vscode", "run", "package", "--", "--target", target, "--out", str(vsix)], cwd=root, check=True)
backend = "richview-editor.exe" if system == "win32" else "richview-editor"
worker = "richview-sql-worker.exe" if system == "win32" else "richview-sql-worker"

def signature(path):
    return {"path": str(path.relative_to(root) if path.is_relative_to(root) else path), "bytes": path.stat().st_size,
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}

with zipfile.ZipFile(vsix) as archive:
    manifest = ET.fromstring(archive.read("extension.vsixmanifest"))
    identity = manifest.find("{*}Metadata/{*}Identity")
    assert identity is not None
    assert identity.attrib["TargetPlatform"] == target, "VSIX architecture must match the native build being verified"
    assert identity.attrib["Version"] == version
    for license in licenses:
        expected = license.read_bytes()
        for directory in ["target/release/font-licenses", "packages/vscode/font-licenses", "assets/showcase/fonts/licenses"]:
            assert (root / directory / license.name).read_bytes() == expected, directory
        assert archive.read(f"extension/font-licenses/{license.name}") == expected
    # The bundle carries minimatch, whose BlueOak-1.0.0 text must reach everyone who gets a copy (ARC-68).
    assert archive.read("extension/dist/THIRD-PARTY-NOTICES.txt") == npm_notices.read_bytes()
    assert (root / "node_modules/minimatch/LICENSE.md").read_text().strip() in npm_notices.read_text(), "minimatch notice"
    for packaged, source in {
        f"extension/bin/{backend}": f"target/release/{backend}",
        f"extension/bin/{worker}": f"target/release/{worker}",
        "extension/web/richview_runtime_bg.wasm": "dist/web/richview_runtime_bg.wasm",
        "extension/web/richview_runtime_bg.wasm.gz": "dist/web/richview_runtime_bg.wasm.gz",
        "extension/web/richview_runtime.js": "dist/web/richview_runtime.js",
    }.items():
        assert archive.read(packaged) == (root / source).read_bytes(), packaged
    members = archive.namelist()

evidence = {
    "status": "local package verification; nothing published",
    "font_notices": [signature(path) for path in licenses],
    "third_party_notices": signature(npm_notices),
    "vsix": {**signature(vsix), "target": target, "members": members,
             "runtime_and_native_backend_byte_identical_to_local_distribution": True},
}
destination = root / "tmp/verification/packaging.json"
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(json.dumps(evidence, indent=2) + "\n")
print(json.dumps({"font_notices": len(licenses), "vsix_members": len(members)}))
