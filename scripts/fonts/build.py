# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.65.0", "brotli==1.2.0"]
# ///
"""Rebuild the report font catalogs from pinned upstream releases.

    uv run scripts/fonts/build.py           # download (cached), verify and rebuild
    uv run scripts/fonts/build.py --check   # rebuild in memory; fail unless the files match

Every source file comes from a pinned upstream commit and is verified by SHA-256 before
use. Variable sources are instanced to static faces. Spaces, canonical accent compositions,
compatibility digraphs (in proportional faces) and (for Newsreader) arrows that a family
lacks are added from its own glyphs and metrics. Each face is subset to the required
character set, plus the Greek and the super- and subscript digits its family has, with its
kerning, ligature, figure and case features; Newsreader and Patrick Hand borrow the script
digits they lack from a bundled face of the same style and weight, and every family but RichView
Sans borrows its script plus and minus, centred on the family's script digits. The `fonts-*.css`
catalogs and `font-manifest.json` (sources, licences, coverage, features, payload hashes)
are written to `crates/richview-core/assets/`.
"""

from __future__ import annotations

import argparse
import base64
import copy
import hashlib
import io
import json
import re
import sys
import unicodedata
import urllib.parse
import urllib.request
from pathlib import Path

import brotli
import fontTools
from fontTools import subset
from fontTools.pens.boundsPen import BoundsPen
from fontTools.pens.recordingPen import DecomposingRecordingPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.woff2 import WOFF2FlavorData
from fontTools.varLib import instancer

ROOT = Path(__file__).resolve().parents[2]
ASSETS = ROOT / "crates/richview-core/assets"
CACHE = ROOT / "tmp/fonts-cache"
TOOLS = {"fonttools": "4.65.0", "brotli": "1.2.0"}
OFL_DESCRIPTION = (
    "This Font Software is licensed under the SIL Open Font License, Version 1.1. "
    "This license is available with a FAQ at: https://openfontlicense.org"
)

# Pinned upstream releases. `release` is the tag, or the commit that published the
# current fonts when a project has no tagged release of them.
SOURCES = {
    "stix-two-math": {
        "repository": "stipub/stixfonts",
        "release": "v2.13b171",
        "commit": "744a22a4dd626cd14d75728aef34fc8ad7c85db0",
        "license": ("OFL.txt", "0c8825913b60d858aacdb33c4ca6660a7d64b0d6464702efbb19313f5765861a"),
        "files": {
            "regular": (
                "fonts/static_ttf_woff2/STIXTwoMath-Regular.woff2",
                "5f8bdddc067dd82689d7247cc96e0be46fe4cec39974c649866aa391d8d5badd",
            ),
        },
    },
    "space-grotesk": {
        "repository": "floriankarsten/space-grotesk",
        "release": "2.0.0",
        "commit": "7220f5d04813fe83babe76d4fd23e02275021280",
        "license": ("OFL.txt", "564ce565c371c5e5bbf286006565a7c9aa55a9f56e7ca58d56e05d649dd61a72"),
        "files": {
            "roman": ("fonts/ttf/SpaceGrotesk[wght].ttf", "acad6de1fc93436f5c0f1f4137751ef04f1aea3063e7036535970ffcfbd79f72"),
        },
    },
    "geist": {
        "repository": "vercel/geist-font",
        "release": "v1.7.2",
        "commit": "a73329da8fc62afc917f796555202e4997f79b7c",
        "license": ("OFL.txt", "c683bfbcc7e087f5d37a54ef628f10387c451a83ddc459b151403a164ac46c90"),
        "files": {
            "roman": ("fonts/Geist/variable/Geist[wght].ttf", "73894e0448cae90a92b6c2f8732b7bb9acb7b94c418bff559dad4a18e1de9659"),
            "italic": ("fonts/Geist/variable/Geist-Italic[wght].ttf", "a464348abe41a6392f18d04c0ac2ad3c26619357ff5fdd76aff3059116f45e62"),
            "mono": ("fonts/GeistMono/variable/GeistMono[wght].ttf", "87c2aff9723544a9adaea19d92e42a33705c9723624801b6e0224c2206a6af0d"),
        },
    },
    "source-serif-4": {
        "repository": "adobe-fonts/source-serif",
        "release": "4.005R",
        "commit": "2823e993c53fca27c5c8749f529b56a5a7c77b6b",
        "license": ("LICENSE.md", "75784a295293a8992f5a8d99210566e0064a012e6dab6731305e3787f15896c7"),
        "files": {
            "roman": ("VAR/SourceSerif4Variable-Roman.ttf", "14d360ee1b76655da9276628b229e11671bc1f5d1083636144db6677d452cf55"),
            "italic": ("VAR/SourceSerif4Variable-Italic.ttf", "6a059a64838978d54e8fab71ed86b0d82e948c0e12b2664d0c15166326dcff82"),
        },
    },
    "source-sans-3": {
        "repository": "adobe-fonts/source-sans",
        "release": "3.052R",
        "commit": "ed1808970eb3c7301c9a523bee26473ba0bb62fa",
        "license": ("LICENSE.md", "89ad2c4f66dd29127527493e729c31e731f111cf10faf5774c3db9275ed0c22c"),
        "files": {
            "roman": ("VF/SourceSans3VF-Upright.ttf", "1147db9a3f0edd4956068de77930148acce2742dd76d57f7239b2b1c687ac63f"),
        },
    },
    "source-code-pro": {
        "repository": "adobe-fonts/source-code-pro",
        "release": "2.042R-u/1.062R-i/1.026R-vf",
        "commit": "d3f1a5962cde503f9409c21e58527611d4a19ef1",
        "license": ("LICENSE.md", "7c940e28a5388e9bba866cf0e408edda45fe0899ba98665b8f6ab31dc5e4b8ff"),
        "files": {
            "regular": ("TTF/SourceCodePro-Regular.ttf", "74bd80d3e42a08517cd7e1108ba3d86f2da29ac0f3065be95e0357956ab9db37"),
        },
    },
    "newsreader": {
        "repository": "productiontype/Newsreader",
        "release": "1.003 (no tagged release; commit of the published fonts)",
        "commit": "cfcb4f7af0e52c25e8df2a2431814c8e5fe2e155",
        "license": ("OFL.txt", "fdfad38143ec470553cae82a1e45320bdd1b9ec70415d37bd0171051d8a4ded8"),
        "files": {
            "roman": ("fonts/variable/ttf/Newsreader[opsz,wght].ttf", "8a08d13f8a6c0d51be379a60af84f945f65369a67e509ee3c3bdcc421254d7c1"),
            "italic": ("fonts/variable/ttf/Newsreader-Italic[opsz,wght].ttf", "796668611f80b64d5adf182fde3b6f29ed83b4e7cbec7b96937e84ac01364792"),
        },
    },
    "big-shoulders": {
        "repository": "xotypeco/big_shoulders",
        "release": "2.002 (newer than the last tagged release v1.100; commit of the published fonts)",
        "commit": "8ba99c9e347396d828b263b8b818269cb99eb27c",
        "license": ("OFL.txt", "338f9c050f19daeda1d597243faf79f3a3d437c338af58cb7047617d0ce08771"),
        "files": {
            "roman": ("Big-Shoulders/fonts/variable/BigShoulders[opsz,wght].ttf", "4b4b24aa6f799aa73cdcd5b6fa840cbcbbb38b81fa9fa82c25126a4530c1ba44"),
        },
    },
    "space-mono": {
        "repository": "googlefonts/spacemono",
        "release": "1.003 (newer than the last tagged release v1.0; commit of the published fonts)",
        "commit": "329858c2c4dbd3476f972a4ae00624b018cf4b81",
        "license": ("OFL.txt", "8e4ee42b2553e1e01504e61cb0d46d148cd8c9e5eacaa3622a7df2d4f2955b9f"),
        "files": {
            "regular": ("fonts/ttf/SpaceMono-Regular.ttf", "95837e182baeeada83368f7748db28357f0a1b75c6b84ff7065b5edf933c8e18"),
        },
    },
    "ibm-plex-mono": {
        "repository": "IBM/plex",
        "release": "@ibm/plex-mono@2.5.0",
        "commit": "2f9ba1b25957d958db71a849e85d72e3ecfb845a",
        "license": ("LICENSE.txt", "7e6b2818edbd8f6a01ae80641cc8f16a51080d08fb4e532be3a0b6f74adb07da"),
        "files": {
            "regular": ("packages/plex-mono/fonts/complete/ttf/IBMPlexMono-Regular.ttf", "7c6fbddca4b700be918f5f6183d9bd4464fa427fe435f0b480d77fe2bb8c5a43"),
            "semibold": ("packages/plex-mono/fonts/complete/ttf/IBMPlexMono-SemiBold.ttf", "f04d7c488ddf7d1fa99f2574efc3406ea4cbe17bb1af3a1ab960f84d0c96a172"),
        },
    },
    # Patrick Wagesreiter publishes Patrick Hand through the Google Fonts directory only.
    "patrick-hand": {
        "repository": "google/fonts",
        "release": "1.003 (no tagged release; commit of the Google Fonts directory)",
        "commit": "bd8f81ddb5c74d5c8897b36ad88b440266245103",
        "license": ("ofl/patrickhand/OFL.txt", "377f4f9c19e935228552478eb68cc2ed82910988a60ba60e2ac73b09f32d02d1"),
        "files": {
            "regular": ("ofl/patrickhand/PatrickHand-Regular.ttf", "0f173b3e6cb6d1af25babf7f0057c5ac4ee11f9992b0469bb817e967ef4ad0fc"),
        },
    },
}

# (catalog, CSS family, style, weight, source, file, instance location, internal family).
# Families whose licences reserve a name ("Source", "Plex") use replacement internal and CSS
# family names: the faces are Modified Versions (subset and instanced).
FACES = [
    ("sleek", "Space Grotesk", "normal", 600, "space-grotesk", "roman", {"wght": 600}, "Space Grotesk"),
    ("sleek", "Geist", "normal", 400, "geist", "roman", {"wght": 400}, "Geist"),
    ("sleek", "Geist", "italic", 400, "geist", "italic", {"wght": 400}, "Geist"),
    ("sleek", "Geist", "normal", 600, "geist", "roman", {"wght": 600}, "Geist"),
    ("sleek", "Geist Mono", "normal", 400, "geist", "mono", {"wght": 400}, "Geist Mono"),
    ("scientific", "RichView Serif", "normal", 400, "source-serif-4", "roman", {"wght": 400, "opsz": 14}, "RichView Serif"),
    ("scientific", "RichView Serif", "italic", 400, "source-serif-4", "italic", {"wght": 400, "opsz": 14}, "RichView Serif"),
    ("scientific", "RichView Serif", "normal", 600, "source-serif-4", "roman", {"wght": 600, "opsz": 14}, "RichView Serif"),
    ("scientific", "RichView Sans", "normal", 400, "source-sans-3", "roman", {"wght": 400}, "RichView Sans"),
    ("scientific", "RichView Sans", "normal", 600, "source-sans-3", "roman", {"wght": 600}, "RichView Sans"),
    ("scientific", "RichView Code", "normal", 400, "source-code-pro", "regular", None, "RichView Code"),
    ("brutalist", "Big Shoulders", "normal", 800, "big-shoulders", "roman", {"wght": 800, "opsz": 14}, "Big Shoulders"),
    ("brutalist", "Space Mono", "normal", 400, "space-mono", "regular", None, "Space Mono"),
    ("editorial", "Newsreader", "normal", 400, "newsreader", "roman", {"wght": 400, "opsz": 16}, "Newsreader"),
    ("editorial", "Newsreader", "italic", 400, "newsreader", "italic", {"wght": 400, "opsz": 16}, "Newsreader"),
    ("editorial", "Newsreader", "normal", 600, "newsreader", "roman", {"wght": 600, "opsz": 16}, "Newsreader"),
    ("editorial", "RichView Mono", "normal", 400, "ibm-plex-mono", "regular", None, "RichView Mono"),
    ("editorial", "RichView Mono", "normal", 600, "ibm-plex-mono", "semibold", None, "RichView Mono"),
    ("hand-drawn", "Patrick Hand", "normal", 400, "patrick-hand", "regular", None, "Patrick Hand"),
]
# Families whose digits are made tabular in the face itself, because chart text on a canvas
# cannot request `tnum`: a `tnum` substitution is baked in where upstream has one, otherwise
# every digit gets the widest digit's advance and is centred in it.
TABULAR_FACES = {"big-shoulders", "patrick-hand"}
GREEK = (0x0370, 0x03FF)
# Superscript and subscript digits with their plus and minus, kept like Greek where the upstream
# design has them (STY-63); ¹ ² ³ are Latin-1 and required.
SCRIPT_DIGITS = [0x2070, *range(0x2074, 0x207C), *range(0x2080, 0x208C)]
# A family that lacks script digits borrows them from the face of a bundled family with the same
# style and weight (STY-63). The borrowed face has the family's CSS name and a unicode-range of
# only the borrowed characters, so browsers draw those from it and the rest from the family's
# own face. A set the family lacks any of is borrowed whole, so one number never mixes two
# designs: Newsreader has ¹ ² ³ ⁰ ⁴ at its cap height, and RichView Serif's superiors rise above it.
DIGIT_LENDERS = {"Newsreader": "RichView Serif", "Patrick Hand": "RichView Sans"}
DIGIT_SETS = [
    [0x00B9, 0x00B2, 0x00B3, 0x2070, *range(0x2074, 0x207A)],  # superior digits
    list(range(0x2080, 0x208A)),  # inferior digits
]
# The superior and inferior plus and minus are one set too, each sign with the digits of
# DIGIT_SETS it stands with. Only RichView Sans draws them, so every other family borrows them
# from it, upright at the family's weight. Each sign is moved so that its minus centres on the
# script digits the family draws, and takes the cell of a monospaced family.
SIGNS = {0x207A: 0, 0x207B: 0, 0x208A: 1, 0x208B: 1}
SIGN_LENDER = "RichView Sans"
CATALOGS = ["sleek", "scientific", "brutalist", "editorial", "hand-drawn"]
WEIGHT_NAMES = {400: "Regular", 600: "SemiBold", 700: "Bold", 800: "ExtraBold"}

# Required in every face: Basic Latin, Latin-1, Latin Extended-A, dashes, quotes, ellipsis,
# per mille, primes, the figure, thin and narrow no-break spaces, the true minus, math
# signs, arrows and the euro. Every character of the report sources is added at build time.
REQUIRED_RANGES = [(0x0020, 0x007E), (0x00A0, 0x00FF), (0x0100, 0x017F)]
REQUIRED_EXTRA = [
    0x2007, 0x2009, 0x2010, 0x2011, 0x2013, 0x2014, 0x2018, 0x2019, 0x201A, 0x201C, 0x201D,
    0x201E, 0x2026, 0x202F, 0x2030, 0x2032, 0x2033, 0x20AC, 0x2190, 0x2191, 0x2192, 0x2193,
    0x2212, 0x2248, 0x2260, 0x2264, 0x2265,
]
# Archaic letters some upstream designs omit, with no decomposition into glyphs they have.
UPSTREAM_GAPS_ALLOWED = {0x0138: "kra", 0x0166: "T with stroke", 0x0167: "t with stroke", 0x017F: "long s"}
# Compatibility digraphs a monospaced face cannot compose: two letters do not fit one cell,
# and the designs that have them draw a narrow I and J, or a dot inside the L.
MONOSPACE_GAPS_ALLOWED = {
    0x0132: "IJ", 0x0133: "ij", 0x013F: "L with middle dot", 0x0140: "l with middle dot",
    0x0149: "n preceded by apostrophe",
}
REPORT_SOURCES = [
    "examples/openai-vs-anthropic/report.rmd",
    "examples/openai-vs-anthropic/data",
    "examples/sales-report/report.rmd",
    "examples/first-report/report.rmd",
    "examples/first-report/data",
]
FEATURES_REQUIRED = ["kern", "liga", "tnum", "lnum", "pnum", "onum", "case"]
# Localized forms and the variation substitutions an instance needs. No combining mark is
# encoded, so ccmp, mark and mkmk could never apply and are not kept.
FEATURES_SUPPORT = ["locl", "rvrn"]
# Small caps cost 4-6 KB per face, and no RichView stylesheet or style token requests them.
FEATURES_OMITTED = {tag: "not requested by any stylesheet or style token" for tag in ("smcp", "c2sc")}
# Kerning that changes an advance by less than 1/100 em (0.2 px at 20 px) is dropped.
KERN_THRESHOLD_EM = 1 / 100

# Canonical accent compositions: code point -> composed letter with the same accent and
# case, whose composite places the accent; the base comes from the canonical decomposition.
ACCENTED = {0x0114: 0x0102, 0x0115: 0x0103, 0x012C: 0x0102, 0x012D: 0x0103, 0x014E: 0x0102, 0x014F: 0x0103}
DOTLESS = {0x0069: 0x0131}
# Compatibility digraphs, set as their decomposition in proportional faces.
DIGRAPHS = {
    0x0132: (0x0049, 0x004A), 0x0133: (0x0069, 0x006A), 0x0149: (0x02BC, 0x006E),
    0x013F: (0x004C, 0x00B7), 0x0140: (0x006C, 0x00B7),
}
# Code points that look like an existing glyph: hyphens; the micro sign as the Greek mu it
# decomposes to; primes as the typewriter apostrophe and quotation mark they are typed as.
ALIASES = {0x00AD: 0x002D, 0x2010: 0x002D, 0x2011: 0x002D, 0x00B5: 0x03BC, 0x2032: 0x0027, 0x2033: 0x0022}
ARROWS = [0x2190, 0x2191, 0x2192, 0x2193]


def fetch(source: dict, path: str, sha256: str) -> bytes:
    """Return a pinned upstream file, downloading it into the cache once."""
    cached = CACHE / sha256
    if cached.exists() and hashlib.sha256(cached.read_bytes()).hexdigest() == sha256:
        return cached.read_bytes()
    url = raw_url(source, path)
    with urllib.request.urlopen(url, timeout=60) as response:
        data = response.read()
    actual = hashlib.sha256(data).hexdigest()
    if actual != sha256:
        sys.exit(f"SHA-256 mismatch for {url}: expected {sha256}, got {actual}")
    CACHE.mkdir(parents=True, exist_ok=True)
    partial = cached.with_suffix(".part")
    partial.write_bytes(data)
    partial.replace(cached)
    return data


def raw_url(source: dict, path: str) -> str:
    return f"https://raw.githubusercontent.com/{source['repository']}/{source['commit']}/{urllib.parse.quote(path)}"


def reserved_names(license_text: str) -> list[str]:
    head = license_text.split("This Font Software is licensed", 1)[0]
    return re.findall(r"Reserved Font Names? [\"'‘“]([^\"'’”]+)[\"'’”]", head)


def report_codepoints() -> list[int]:
    found: set[int] = set()
    for entry in REPORT_SOURCES:
        path = ROOT / entry
        files = sorted(p for p in path.rglob("*") if p.suffix in {".csv", ".json", ".tsv"}) if path.is_dir() else [path]
        for file in files:
            found |= {ord(c) for c in file.read_text(encoding="utf-8") if ord(c) >= 0x20 and c != "\x7f"}
    return sorted(found)


def required_codepoints(reports: list[int]) -> list[int]:
    required = {cp for start, end in REQUIRED_RANGES for cp in range(start, end + 1)}
    return sorted(required | set(REQUIRED_EXTRA) | set(reports))


def unicode_cmaps(font: TTFont) -> list:
    return [table for table in font["cmap"].tables if table.isUnicode() and table.format != 14]


def add_glyph(font: TTFont, codepoint: int, glyph, advance: int) -> str:
    name = f"uni{codepoint:04X}"
    while name in font["glyf"].glyphs:
        name += ".rv"
    order = font.getGlyphOrder()
    if name not in order:
        font.setGlyphOrder(order + [name])
    glyf = font["glyf"]
    glyf[name] = glyph
    glyph.recalcBounds(glyf)
    font["hmtx"][name] = (advance, getattr(glyph, "xMin", 0) if glyph.numberOfContours else 0)
    map_codepoint(font, codepoint, name)
    return name


def map_codepoint(font: TTFont, codepoint: int, name: str) -> None:
    for table in unicode_cmaps(font):
        if codepoint > 0xFFFF and table.format == 4:
            continue
        table.cmap[codepoint] = name


def advance(font: TTFont, codepoint: int) -> int:
    return font["hmtx"][font.getBestCmap()[codepoint]][0]


def bounds(font: TTFont, glyph_name: str) -> tuple:
    pen = BoundsPen(font.getGlyphSet())
    font.getGlyphSet()[glyph_name].draw(pen)
    return pen.bounds


def single_substitution(font: TTFont, feature: str) -> dict:
    """Merge the single substitutions a GSUB feature applies."""
    mapping: dict = {}
    if "GSUB" not in font:
        return mapping
    gsub = font["GSUB"].table
    for record in gsub.FeatureList.FeatureRecord:
        if record.FeatureTag != feature:
            continue
        for index in record.Feature.LookupListIndex:
            lookup = gsub.LookupList.Lookup[index]
            for table in lookup.SubTable:
                table = getattr(table, "ExtSubTable", table)
                for source, target in getattr(table, "mapping", {}).items():
                    mapping.setdefault(source, target)
    return mapping


def mark_anchor(font: TTFont, base: str, mark: str):
    """Offset that GPOS mark-to-base attachment gives `mark` on `base`, if any."""
    if "GPOS" not in font:
        return None
    for lookup in font["GPOS"].table.LookupList.Lookup:
        for table in lookup.SubTable:
            table = getattr(table, "ExtSubTable", table)
            if not hasattr(table, "BaseCoverage") or not hasattr(table, "MarkCoverage"):
                continue
            marks, bases = table.MarkCoverage.glyphs, table.BaseCoverage.glyphs
            if mark not in marks or base not in bases:
                continue
            record = table.MarkArray.MarkRecord[marks.index(mark)]
            anchor = table.BaseArray.BaseRecord[bases.index(base)].BaseAnchor[record.Class]
            if anchor is not None:
                return (anchor.XCoordinate - record.MarkAnchor.XCoordinate, anchor.YCoordinate - record.MarkAnchor.YCoordinate)
    return None


def accent_offset(font: TTFont, base: str, model_base: str, accent: str, combining: str, model_offset: tuple) -> tuple:
    """Place the model's accent on a new base the way the family's anchors place it."""
    for mark in (accent, combining):
        new, old = mark_anchor(font, base, mark), mark_anchor(font, model_base, mark)
        if new and old:
            return (round(model_offset[0] + new[0] - old[0]), round(model_offset[1] + new[1] - old[1]))
    new, old = bounds(font, base), bounds(font, model_base)
    return (round(model_offset[0] + (new[0] + new[2] - old[0] - old[2]) / 2), round(model_offset[1]))


def feature_inputs(font: TTFont, tag: str) -> set[str]:
    """Glyphs a feature's lookups can act on."""
    glyphs: set[str] = set()
    for table_tag in ("GSUB", "GPOS"):
        if table_tag not in font:
            continue
        table = font[table_tag].table
        for record in table.FeatureList.FeatureRecord:
            if record.FeatureTag != tag:
                continue
            for index in record.Feature.LookupListIndex:
                for st in table.LookupList.Lookup[index].SubTable:
                    st = getattr(st, "ExtSubTable", st)
                    for attr in ("mapping", "alternates", "ligatures"):
                        glyphs |= set(getattr(st, attr, None) or {})
                    for attr in ("Coverage", "MarkCoverage", "BaseCoverage"):
                        if getattr(st, attr, None) is not None:
                            glyphs |= set(getattr(st, attr).glyphs)
                    for coverage in getattr(st, "InputCoverage", None) or []:
                        glyphs |= set(coverage.glyphs)
    return glyphs


def pair_value(record) -> int:
    if record is None:
        return 0
    return getattr(record, "XAdvance", 0) or 0


def plain_kern(value) -> bool:
    return value is None or set(vars(value)) <= {"XAdvance"}


def later_kerning(tables: list, left: str, right: str) -> int:
    """The adjustment the remaining subtables of a lookup give a pair (the first that applies)."""
    for table in tables:
        if left not in table.Coverage.glyphs:
            continue
        if hasattr(table, "PairSet"):
            pairs = table.PairSet[table.Coverage.glyphs.index(left)].PairValueRecord
            match = [pair for pair in pairs if pair.SecondGlyph == right]
            if match:
                return pair_value(match[0].Value1)
        else:
            first = table.ClassDef1.classDefs.get(left, 0)
            second = table.ClassDef2.classDefs.get(right, 0)
            return pair_value(table.Class1Record[first].Class2Record[second].Value1)
    return 0


def prune_kerning(font: TTFont) -> int:
    """Drop kerning below the threshold without changing any effective value by more."""
    if "GPOS" not in font:
        return 0
    threshold = font["head"].unitsPerEm * KERN_THRESHOLD_EM
    dropped = 0
    for lookup in font["GPOS"].table.LookupList.Lookup:
        tables = [getattr(table, "ExtSubTable", table) for table in lookup.SubTable]
        if not tables or not all(hasattr(t, "PairSet") or hasattr(t, "Class1Record") for t in tables):
            continue
        for table in tables:
            if getattr(table, "Class1Record", None) is None:
                continue
            for first in table.Class1Record:
                for second in first.Class2Record:
                    if plain_kern(second.Value1) and second.Value1 is not None and abs(pair_value(second.Value1)) < threshold:
                        second.Value1.XAdvance = 0
        for index, table in enumerate(tables):
            if not hasattr(table, "PairSet"):
                continue
            for left, pairs in zip(table.Coverage.glyphs, table.PairSet, strict=True):
                kept = [
                    pair for pair in pairs.PairValueRecord
                    if not (plain_kern(pair.Value1) and getattr(pair, "Value2", None) is None
                            and abs(pair_value(pair.Value1) - later_kerning(tables[index + 1:], left, pair.SecondGlyph)) < threshold)
                ]
                dropped += len(pairs.PairValueRecord) - len(kept)
                pairs.PairValueRecord, pairs.PairValueCount = kept, len(kept)
    return dropped


def tabular_figures(font: TTFont) -> list[str]:
    """Figures under tnum (the defaults when the family has no tnum)."""
    cmap = font.getBestCmap()
    tnum = single_substitution(font, "tnum")
    return [tnum.get(cmap[cp], cmap[cp]) for cp in range(0x30, 0x3A)]


def equalize_tabular(font: TTFont) -> list[str]:
    """Give a lone tabular figure with a different advance the common advance, centred.

    Two upstream italics draw one tabular seven wider than the other figures."""
    figures = tabular_figures(font)
    hmtx, glyf = font["hmtx"], font["glyf"]
    widths = [hmtx[name][0] for name in figures]
    common = max(set(widths), key=widths.count)
    outliers = {name for name, width in zip(figures, widths, strict=True) if width != common}
    fixed = []
    if len(outliers) != 1:
        return fixed
    name = outliers.pop()
    referenced = any(c.glyphName == name for other in font.getGlyphOrder() for c in getattr(glyf[other], "components", None) or [])
    glyph = glyf[name]
    glyph.recalcBounds(glyf)
    if referenced or glyph.xMax - glyph.xMin > common:
        return fixed
    width = hmtx[name][0]
    shift = round((common - width) / 2)
    if glyph.isComposite():
        for component in glyph.components:
            component.x += shift
    else:
        glyph.coordinates.translate((shift, 0))
    glyph.recalcBounds(glyf)
    hmtx[name] = (common, glyph.xMin)
    fixed.append(f"{name} {width}->{common}")
    return fixed


def zero_value(value) -> None:
    for name in list(vars(value)):
        setattr(value, name, 0)


def silence_digit_kerning(font: TTFont, digits: set[str]) -> int:
    """Remove every kerning pair that involves a digit; class kerning moves digits into zero classes."""
    if "GPOS" not in font:
        return 0
    removed = 0
    for lookup in font["GPOS"].table.LookupList.Lookup:
        for table in (getattr(t, "ExtSubTable", t) for t in lookup.SubTable):
            if hasattr(table, "PairSet"):
                for left, pairs in zip(table.Coverage.glyphs, table.PairSet, strict=True):
                    kept = [p for p in pairs.PairValueRecord if left not in digits and p.SecondGlyph not in digits]
                    removed += len(pairs.PairValueRecord) - len(kept)
                    pairs.PairValueRecord, pairs.PairValueCount = kept, len(kept)
            elif hasattr(table, "Class1Record"):
                removed += silence_class_kerning(table, digits)
    return removed


def silence_class_kerning(table, digits: set[str]) -> int:
    """Give a class-kerning subtable's digits their own classes whose values are all zero."""
    changed = 0
    first = [g for g in table.Coverage.glyphs if g in digits]
    if first:
        new = len(table.Class1Record)
        template = copy.deepcopy(table.Class1Record[0])
        for record in template.Class2Record:
            for value in (record.Value1, record.Value2):
                if value is not None:
                    zero_value(value)
        table.Class1Record.append(template)
        table.Class1Count = len(table.Class1Record)
        for glyph in first:
            if table.ClassDef1.classDefs.get(glyph, 0) != new:
                table.ClassDef1.classDefs[glyph] = new
                changed += 1
    new = table.Class2Count
    for row in table.Class1Record:
        extra = copy.deepcopy(row.Class2Record[0])
        for value in (extra.Value1, extra.Value2):
            if value is not None:
                zero_value(value)
        row.Class2Record.append(extra)
    table.Class2Count = new + 1
    for glyph in digits:
        table.ClassDef2.classDefs[glyph] = new
        changed += 1
    return changed


def make_tabular(font: TTFont) -> list[str]:
    """Make the ten digits tabular by default: bake in `tnum`, or equalize and centre them."""
    cmap = font.getBestCmap()
    tnum = single_substitution(font, "tnum")
    digits = [cmap[cp] for cp in range(0x30, 0x3A)]
    if all(name in tnum for name in digits):
        for cp, name in zip(range(0x30, 0x3A), digits, strict=True):
            map_codepoint(font, cp, tnum[name])
        return ["tnum baked into the cmap"]
    glyf, hmtx = font["glyf"], font["hmtx"]
    if any(c.glyphName in digits for g in font.getGlyphOrder() for c in getattr(glyf[g], "components", None) or []):
        sys.exit(f"{font_label(font)}: a composite uses a digit glyph")
    common = max(hmtx[name][0] for name in digits)
    notes = []
    for name in digits:
        glyph, width = glyf[name], hmtx[name][0]
        glyph.recalcBounds(glyf)
        shift = round((common - (glyph.xMax - glyph.xMin)) / 2) - glyph.xMin
        glyph.coordinates.translate((shift, 0))
        glyph.recalcBounds(glyf)
        hmtx[name] = (common, glyph.xMin)
        if width != common:
            notes.append(f"{name} {width}->{common}")
    kerning = silence_digit_kerning(font, set(digits))
    if kerning:
        notes.append(f"digit kerning removed ({kerning})")
    return notes


def verify_tabular(font: TTFont, label: str) -> None:
    """Fail unless the ten digits share one advance and sit centred in it."""
    hmtx, glyf = font["hmtx"], font["glyf"]
    names = [font.getBestCmap()[cp] for cp in range(0x30, 0x3A)]
    if len({hmtx[name][0] for name in names}) != 1:
        sys.exit(f"{label}: digit advances differ: {[hmtx[name][0] for name in names]}")
    for name in names:
        glyph = glyf[name]
        glyph.recalcBounds(glyf)
        if abs(glyph.xMin - (hmtx[name][0] - glyph.xMax)) > 1 or hmtx[name][1] != glyph.xMin:
            sys.exit(f"{label}: digit {name} is not centred in its advance")


def is_monospace(font: TTFont) -> bool:
    return advance(font, ord("i")) == advance(font, ord("m"))


def figure_width(font: TTFont) -> int:
    return font["hmtx"][tabular_figures(font)[0]][0]


def arrow_outline(font: TTFont, codepoint: int) -> tuple:
    """An open arrow drawn from the family's minus: same stroke, axis and side bearings."""
    cmap = font.getBestCmap()
    minus = cmap[0x2212]
    x_min, y_min, x_max, y_max = bounds(font, minus)
    width = font["hmtx"][minus][0]
    stroke, axis = y_max - y_min, (y_min + y_max) / 2
    total = round(width * 1.25)
    left, right = x_min, total - (width - x_max)
    half = 0.55 * font["OS/2"].sxHeight
    diagonal, inset = stroke * 2**0.5, stroke / 2**0.5
    joint = right - diagonal - stroke / 2
    # Right arrow, clockwise from the shaft's upper-left corner.
    points = [
        (left, axis + stroke / 2), (joint, axis + stroke / 2),
        (right - half - inset, axis + half - inset), (right - half, axis + half), (right, axis),
        (right - half, axis - half), (right - half - inset, axis - half + inset),
        (joint, axis - stroke / 2), (left, axis - stroke / 2),
    ]
    centre = total / 2
    if codepoint == 0x2190:
        points = [(total - x, y) for x, y in reversed(points)]
    elif codepoint == 0x2191:
        points = [(centre - (y - axis), axis + (x - centre)) for x, y in points]
    elif codepoint == 0x2193:
        points = [(centre + (y - axis), axis - (x - centre)) for x, y in points]
    pen = TTGlyphPen(None)
    pen.moveTo((round(points[0][0]), round(points[0][1])))
    for x, y in points[1:]:
        pen.lineTo((round(x), round(y)))
    pen.closePath()
    return pen.glyph(), total


def complete(font: TTFont, wanted: list[int], tabularize: bool) -> dict:
    """Add what the family lacks of the required set from its own glyphs and metrics."""
    added: dict[str, list[str]] = {"alias": [], "space": [], "accent": [], "digraph": [], "arrow": [], "tabular": []}
    if tabularize:
        added["tabular"] = make_tabular(font)
    cmap = font.getBestCmap()
    mono = is_monospace(font)
    for codepoint, target in ALIASES.items():
        if codepoint not in cmap and target in cmap:
            map_codepoint(font, codepoint, cmap[target])
            added["alias"].append(f"{codepoint:04X}")
    cmap = font.getBestCmap()
    space = advance(font, 0x20)
    if 0x2009 not in cmap:
        add_glyph(font, 0x2009, TTGlyphPen(None).glyph(), space if mono else round(space / 2))
        added["space"].append("2009")
    if 0x2007 not in cmap:
        add_glyph(font, 0x2007, TTGlyphPen(None).glyph(), figure_width(font))
        added["space"].append("2007")
    cmap = font.getBestCmap()
    if 0x202F not in cmap:
        map_codepoint(font, 0x202F, cmap[0x2009])
        added["alias"].append("202F")
    glyf = font["glyf"]
    for codepoint, model_cp in ACCENTED.items():
        cmap = font.getBestCmap()
        if codepoint in cmap or codepoint not in wanted:
            continue
        base_cp, accent_cp = (int(part, 16) for part in unicodedata.decomposition(chr(codepoint)).split())
        base_cp = DOTLESS.get(base_cp, base_cp)
        model_base_cp = int(unicodedata.decomposition(chr(model_cp)).split()[0], 16)
        model = glyf[cmap[model_cp]]
        bases = [c for c in getattr(model, "components", []) if c.glyphName == cmap[model_base_cp]]
        accents = [c for c in getattr(model, "components", []) if c.glyphName != cmap[model_base_cp]]
        if len(bases) != 1 or len(accents) != 1 or (bases[0].x, bases[0].y) != (0, 0):
            sys.exit(f"{font_label(font)}: U+{model_cp:04X} is not a base-plus-accent composite")
        accent = accents[0]
        offset = accent_offset(font, cmap[base_cp], bases[0].glyphName, accent.glyphName, cmap[accent_cp], (accent.x, accent.y))
        pen = TTGlyphPen(font.getGlyphSet())
        pen.addComponent(cmap[base_cp], (1, 0, 0, 1, 0, 0))
        pen.addComponent(accent.glyphName, (1, 0, 0, 1, *offset))
        add_glyph(font, codepoint, pen.glyph(), advance(font, base_cp))
        added["accent"].append(f"{codepoint:04X}")
    for codepoint, (first, second) in DIGRAPHS.items():
        cmap = font.getBestCmap()
        if mono or codepoint in cmap or codepoint not in wanted:
            continue
        first = first if first in cmap else 0x2019
        pen = TTGlyphPen(font.getGlyphSet())
        pen.addComponent(cmap[first], (1, 0, 0, 1, 0, 0))
        pen.addComponent(cmap[second], (1, 0, 0, 1, advance(font, first), 0))
        add_glyph(font, codepoint, pen.glyph(), advance(font, first) + advance(font, second))
        added["digraph"].append(f"{codepoint:04X}")
    for codepoint in ARROWS:
        if codepoint not in font.getBestCmap():
            glyph, width = arrow_outline(font, codepoint)
            add_glyph(font, codepoint, glyph, width)
            added["arrow"].append(f"{codepoint:04X}")
    added["tabular"] += equalize_tabular(font)
    return {kind: codes for kind, codes in added.items() if codes}


def font_label(font: TTFont) -> str:
    return font["name"].getDebugName(4) or "font"


def set_names(font: TTFont, family: str, style: str, weight: int, upstream: dict) -> tuple[str, str]:
    weight_name = WEIGHT_NAMES[weight]
    subfamily = "Italic" if style == "italic" else "Regular"
    if weight_name not in ("Regular", "Bold"):
        legacy_family = f"{family} {weight_name}"
        typographic = f"{weight_name} Italic" if style == "italic" else weight_name
    else:
        legacy_family = family
        typographic = None
        if weight_name == "Bold":
            subfamily = "Bold Italic" if style == "italic" else "Bold"
    full_style = typographic or subfamily
    postscript = f"{family.replace(' ', '')}-{full_style.replace(' ', '')}"
    version = upstream.get(5, "Version 1.000")
    number = re.search(r"\d+\.\d+", version).group(0)
    table = font["name"]
    table.names = []
    records = {
        0: upstream.get(0), 1: legacy_family, 2: subfamily, 3: f"{number};RichView;{postscript}",
        4: f"{family} {full_style}", 5: version.split(";")[0], 6: postscript,
        7: upstream.get(7), 8: upstream.get(8), 9: upstream.get(9), 11: upstream.get(11),
        12: upstream.get(12), 13: OFL_DESCRIPTION, 14: "https://openfontlicense.org",
        16: family if typographic else None, 17: typographic,
    }
    for name_id, value in records.items():
        if value:
            table.setName(value, name_id, 3, 1, 0x409)
    return postscript, records[4]


def set_style_bits(font: TTFont, style: str, weight: int) -> None:
    os2, head = font["OS/2"], font["head"]
    os2.usWeightClass = weight
    selection = os2.fsSelection & ~0b1100001  # clear ITALIC, BOLD, REGULAR
    mac = head.macStyle & ~0b11
    if style == "italic":
        selection |= 1
        mac |= 2
    if weight == 700:
        selection |= 1 << 5
        mac |= 1
    if style != "italic" and weight != 700:
        selection |= 1 << 6
    os2.fsSelection, head.macStyle = selection, mac


def open_face(face: tuple, blobs: dict) -> tuple[TTFont, dict, list[str]]:
    """The face's upstream font instanced at its location, its English names and its layout features."""
    source_key, file_key, location = face[4:7]
    path, sha256 = SOURCES[source_key]["files"][file_key]
    font = TTFont(io.BytesIO(blobs[sha256]), recalcTimestamp=False)
    upstream = {r.nameID: r.toUnicode() for r in font["name"].names if r.platformID == 3 and r.langID == 0x409}
    source_features = sorted({r.FeatureTag for tag in ("GSUB", "GPOS") if tag in font for r in font[tag].table.FeatureList.FeatureRecord})
    if location:
        font = instancer.instantiateVariableFont(font, location, inplace=True, overlap=instancer.OverlapMode.KEEP_AND_SET_FLAGS)
    return font, upstream, source_features


def subsetter(features: list[str]) -> subset.Subsetter:
    """A subsetter keeping `features`, every English name and no hinting or glyph names."""
    options = subset.Options()
    options.layout_features = features
    options.name_IDs = ["*"]
    options.name_languages = [0x409]
    options.hinting = False
    options.glyph_names = False
    options.drop_tables += ["STAT", "meta", "DSIG", "BASE"]
    return subset.Subsetter(options)


def woff2(font: TTFont) -> bytes:
    """The subset face as WOFF2; without hinting, rasterizers smooth in both directions at every size."""
    gasp = newTable("gasp")
    gasp.version, gasp.gaspRange = 1, {0xFFFF: 0x000F}
    font["gasp"] = gasp
    font.recalcTimestamp = False
    font.flavor = "woff2"
    font.flavorData = WOFF2FlavorData(transformedTables={"glyf", "loca", "hmtx"})
    buffer = io.BytesIO()
    font.save(buffer)
    return buffer.getvalue()


def build_face(face: tuple, wanted: list[int], blobs: dict) -> tuple[bytes, dict]:
    catalog, css_family, style, weight, source_key, file_key, location, family = face
    path = SOURCES[source_key]["files"][file_key][0]
    font, upstream, source_features = open_face(face, blobs)
    added = complete(font, wanted, source_key in TABULAR_FACES)
    cmap = font.getBestCmap()
    greek = [cp for cp in sorted(cmap) if GREEK[0] <= cp <= GREEK[1]]
    scripts = [cp for cp in SCRIPT_DIGITS if cp in cmap]
    inputs = {tag: feature_inputs(font, tag) for tag in FEATURES_REQUIRED if tag in source_features}
    cutter = subsetter(FEATURES_REQUIRED + FEATURES_SUPPORT)
    cutter.populate(unicodes=sorted(set(wanted) | set(greek) | set(scripts)))
    cutter.subset(font)
    reachable = set(cutter.glyphs_gsubed)  # from the cmap through substitutions, not components
    kerning_dropped = prune_kerning(font)
    postscript, full_name = set_names(font, family, style, weight, upstream)
    set_style_bits(font, style, weight)
    data = woff2(font)
    final = TTFont(io.BytesIO(data))
    cmap = final.getBestCmap()
    mono = is_monospace(final)
    missing = sorted(set(wanted) - set(cmap))
    allowed = UPSTREAM_GAPS_ALLOWED | (MONOSPACE_GAPS_ALLOWED if mono else {})
    unexpected = [cp for cp in missing if cp not in allowed]
    if unexpected:
        sys.exit(f"{css_family} {style} {weight}: missing {' '.join(f'U+{cp:04X}' for cp in unexpected)}")
    features = sorted({r.FeatureTag for tag in ("GSUB", "GPOS") if tag in final for r in final[tag].table.FeatureList.FeatureRecord})
    dropped = [tag for tag in inputs if tag not in features]
    lost = [tag for tag in dropped if inputs[tag] & reachable]
    if lost:
        sys.exit(f"{css_family} {style} {weight}: lost features {lost}")
    if source_key in TABULAR_FACES:
        verify_tabular(final, f"{css_family} {style} {weight}")
    kept_greek = [cp for cp in sorted(cmap) if GREEK[0] <= cp <= GREEK[1]]
    kept_scripts = [cp for cp in SCRIPT_DIGITS if cp in cmap]
    tabular = {final["hmtx"][name][0] for name in tabular_figures(final)}
    record = {
        "family": css_family, "style": style, "weight": weight, "catalog": f"fonts-{catalog}.css",
        "monospace": mono, "source": source_key, "file": path, "location": location or {},
        "name": full_name, "postscript": postscript, "version": upstream.get(5, "").split(";")[0],
        "glyphs": final["maxp"].numGlyphs, "woff2_bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
        "features": features, "source_features": [t for t in source_features if t in FEATURES_REQUIRED],
        "features_without_effect": dropped, "kerning_pairs_below_threshold": kerning_dropped,
        "tabular_digits": len(tabular) == 1, "cmap": ranges(sorted(cmap)), "greek": ranges(kept_greek),
        "script_digits": ranges(kept_scripts),
        "synthesized": added, "upstream_gaps": [f"{cp:04X}" for cp in missing],
    }
    return data, record


def script_centre(font: TTFont, codepoints: list[int]) -> float:
    """The centre in em between the mean bottom and the mean top of `codepoints`' outlines."""
    cmap = font.getBestCmap()
    boxes = [bounds(font, cmap[cp]) for cp in codepoints]
    return sum(box[1] + box[3] for box in boxes) / (2 * len(boxes)) / font["head"].unitsPerEm


def sign_lender(weight: int) -> tuple:
    """SIGN_LENDER's upright face at `weight`, which lends every other family the script signs."""
    face = next(f for f in FACES if f[1] == SIGN_LENDER and f[2] == "normal")
    return (*face[:3], weight, *face[4:6], {**(face[6] or {}), "wght": weight}, face[7])


def move_signs(font: TTFont, centres: list[float], cell: int | None) -> dict[str, float]:
    """Move the signs so each minus centres on the script digits of its kind, at `centres` em.

    The lender draws ⁺ and ⁻ as ₊ and ₋ raised, so every sign is drawn out of the lender's
    outlines before any is moved. With `cell`, each sign takes that advance and is centred in it.
    Returns how far each kind moved, in em.
    """
    cmap, glyphs, glyf, hmtx = font.getBestCmap(), font.getGlyphSet(), font["glyf"], font["hmtx"]
    units = font["head"].unitsPerEm
    outlines = {}
    for cp in SIGNS:
        pen = DecomposingRecordingPen(glyphs)
        glyphs[cmap[cp]].draw(pen)
        outlines[cp] = pen

    def box(cp: int) -> tuple:
        pen = BoundsPen(None)
        outlines[cp].replay(pen)
        return pen.bounds

    shift = {}
    for minus in (0x207B, 0x208B):
        _, low, _, high = box(minus)
        shift[SIGNS[minus]] = round(centres[SIGNS[minus]] * units - (low + high) / 2)
    for cp, kind in SIGNS.items():
        name = cmap[cp]
        x_min, _, x_max, _ = box(cp)
        width, dx = hmtx[name][0], 0
        if cell is not None:
            width, dx = cell, round((cell - (x_max - x_min)) / 2 - x_min)
        pen = TTGlyphPen(None)
        outlines[cp].replay(TransformPen(pen, (1, 0, 0, 1, dx, shift[kind])))
        glyph = pen.glyph()
        glyf[name] = glyph
        glyph.recalcBounds(glyf)
        hmtx[name] = (width, glyph.xMin)
    return {"superior": round(shift[0] / units, 3), "inferior": round(shift[1] / units, 3)}


def verify_signs(font: TTFont, centres: list[float], cell: int | None, label: str) -> None:
    """Fail unless each minus centres on its digits within a unit and each plus within 1/200 em,
    and, with `cell`, every sign has that advance with equal side bearings."""
    cmap, glyf, hmtx = font.getBestCmap(), font["glyf"], font["hmtx"]
    units = font["head"].unitsPerEm
    for cp, kind in SIGNS.items():
        _, low, _, high = bounds(font, cmap[cp])
        tolerance = 1 if cp in (0x207B, 0x208B) else units / 200
        if abs((low + high) / 2 - centres[kind] * units) > tolerance:
            sys.exit(f"{label}: U+{cp:04X} does not centre on its script digits")
        glyph = glyf[cmap[cp]]
        glyph.recalcBounds(glyf)
        if cell is not None and (hmtx[cmap[cp]][0] != cell or abs(glyph.xMin - (cell - glyph.xMax)) > 1):
            sys.exit(f"{label}: U+{cp:04X} is not centred in the {cell}-unit cell")


def build_lent_faces(host: tuple, host_data: bytes, blobs: dict) -> list[tuple[bytes, dict]]:
    """The faces lending `host`'s family the script digits and signs it lacks, one per lender.

    Each digit set the family lacks comes from DIGIT_LENDERS' face of the same style and weight,
    cut as it is. The signs come from SIGN_LENDER upright at the family's weight, moved to the
    centres of the digits the family draws, own or lent (`move_signs`). Each face has no layout
    features and is named `<lender> Digits`, or `<lender> Signs` when it lends only signs.
    """
    catalog, css_family, style, weight = host[:4]
    own_font = TTFont(io.BytesIO(host_data))
    own = set(own_font.getBestCmap())
    label = f"{css_family} {style} {weight}"
    digit_lender = None
    if css_family in DIGIT_LENDERS:
        digit_lender = next(f for f in FACES if f[1] == DIGIT_LENDERS[css_family] and f[2:4] == (style, weight))
    lender_font = open_face(digit_lender, blobs)[0] if digit_lender else None
    centres, digits = [], []
    for group in DIGIT_SETS:
        drawn_by = own_font if own.issuperset(group) else lender_font
        if drawn_by is None or not set(drawn_by.getBestCmap()).issuperset(group):
            sys.exit(f"{label}: no bundled face lends the script digits it lacks")
        centres.append(script_centre(drawn_by, group))
        if drawn_by is lender_font:
            digits += group
    lent = [(digit_lender, digits)] if digits else []
    if css_family != SIGN_LENDER and not own.issuperset(SIGNS):
        signs_from = sign_lender(weight)
        if lent and lent[0][0] == signs_from:
            lent[0] = (signs_from, digits + list(SIGNS))
        else:
            lent.append((signs_from, list(SIGNS)))
    cell = None
    if is_monospace(own_font):
        cell = advance(own_font, 0x30) / own_font["head"].unitsPerEm
    faces = []
    for lender, codepoints in lent:
        borrowed = sorted(codepoints)
        source_key, file_key, location, family = lender[4:8]
        font, upstream, _ = open_face(lender, blobs)
        cutter = subsetter([])
        cutter.populate(unicodes=borrowed)
        cutter.subset(font)
        signs = SIGNS.keys() <= set(borrowed)
        lends_digits = any(cp not in SIGNS for cp in borrowed)
        units = font["head"].unitsPerEm
        lender_cell = round(cell * units) if cell is not None else None
        moved = move_signs(font, centres, lender_cell) if signs else None
        kind = "Digits" if lends_digits else "Signs"
        postscript, full_name = set_names(font, f"{family} {kind}", style, weight, upstream)
        set_style_bits(font, style, weight)
        data = woff2(font)
        final = TTFont(io.BytesIO(data))
        if sorted(final.getBestCmap()) != borrowed:
            sys.exit(f"{label}: the borrowed face must map exactly its unicode-range")
        if signs:
            verify_signs(final, centres, lender_cell, label)
        lends = "script digits and signs" if signs and lends_digits else "script signs" if signs else "script digits"
        record = {
            "family": css_family, "style": style, "weight": weight, "catalog": f"fonts-{catalog}.css",
            "monospace": cell is not None and signs, "source": source_key,
            "file": SOURCES[source_key]["files"][file_key][0], "location": location or {}, "name": full_name,
            "postscript": postscript, "version": upstream.get(5, "").split(";")[0],
            "glyphs": final["maxp"].numGlyphs, "woff2_bytes": len(data), "sha256": hashlib.sha256(data).hexdigest(),
            "features": [], "cmap": ranges(borrowed), "script_digits": ranges([cp for cp in borrowed if cp in SCRIPT_DIGITS]),
            "borrowed_from": lender[1], "lends": lends,
            "unicode_range": ", ".join(f"U+{part}" for part in ranges(borrowed).split(",")),
        }
        if moved:
            record["signs_moved_em"] = moved
        faces.append((data, record))
    return faces


def ranges(codepoints: list[int]) -> str:
    parts, start = [], None
    for index, cp in enumerate(codepoints):
        if start is None:
            start = cp
        if index + 1 == len(codepoints) or codepoints[index + 1] != cp + 1:
            parts.append(f"{start:04X}" if start == cp else f"{start:04X}-{cp:04X}")
            start = None
    return ",".join(parts)


def build() -> dict[Path, bytes]:
    if fontTools.version != TOOLS["fonttools"] or getattr(brotli, "__version__", None) != TOOLS["brotli"]:
        sys.exit(f"Use fonttools {TOOLS['fonttools']} and brotli {TOOLS['brotli']} (uv run scripts/fonts/build.py) for byte-identical output")
    blobs, sources = {}, {}
    for key, source in SOURCES.items():
        license_path, license_sha = source["license"]
        license_text = fetch(source, license_path, license_sha).decode("utf-8")
        reserved = reserved_names(license_text)
        copyright_line = license_text.strip().splitlines()[0].strip()
        for face in FACES:
            if face[4] == key:
                clash = [name for name in reserved if any(name.lower() in family.lower() for family in (face[1], face[7]))]
                if clash:
                    sys.exit(f"{face[7]} uses Reserved Font Name {clash} of {key}")
        files = []
        for _, (path, sha256) in sorted(source["files"].items()):
            blobs[sha256] = fetch(source, path, sha256)
            files.append({"path": path, "url": raw_url(source, path), "sha256": sha256})
        sources[key] = {
            "repository": f"https://github.com/{source['repository']}", "release": source["release"],
            "commit": source["commit"], "files": files,
            "license": {"path": license_path, "url": raw_url(source, license_path), "sha256": license_sha,
                        "copyright": copyright_line, "reserved_font_names": reserved},
        }
    reports = report_codepoints()
    wanted = required_codepoints(reports)
    outputs: dict[Path, bytes] = {}
    catalogs = {name: [] for name in CATALOGS}
    faces = []
    for face in FACES:
        data, record = build_face(face, wanted, blobs)
        reserved = sources[face[4]]["license"]["reserved_font_names"]
        record["css_family_uses_reserved_name"] = any(name.lower() in face[1].lower() for name in reserved)
        faces.append(record)
        payload = base64.b64encode(data).decode("ascii")
        catalogs[face[0]].append(
            f'@font-face {{ font-family: "{face[1]}"; font-style: {face[2]}; font-weight: {face[3]}; '
            f'font-display: block; src: url(data:font/woff2;base64,{payload}) format("woff2"); }}'
        )
        for lent_data, lent in build_lent_faces(face, data, blobs):
            # After the family's own face, so browsers try it first for its range. The comment
            # names what it lends and the lender, whose licence notice the report then carries
            # (FEA-RP-38).
            reserved = sources[lent["source"]]["license"]["reserved_font_names"]
            clash = [name for name in reserved if any(name.lower() in n.lower() for n in (face[1], lent["name"]))]
            if clash:
                sys.exit(f"{lent['name']} uses Reserved Font Name {clash} of {lent['source']}")
            lent["css_family_uses_reserved_name"] = False
            faces.append(lent)
            payload = base64.b64encode(lent_data).decode("ascii")
            catalogs[face[0]].append(
                f'@font-face {{ font-family: "{face[1]}"; font-style: {face[2]}; font-weight: {face[3]}; '
                f'font-display: block; unicode-range: {lent["unicode_range"]}; '
                f'src: url(data:font/woff2;base64,{payload}) format("woff2"); }} /* {lent["lends"]} from {lent["borrowed_from"]} */'
            )
    header = "/* Generated by scripts/fonts/build.py from pinned upstream releases; sources, licences and coverage: font-manifest.json. */"
    for name, lines in catalogs.items():
        outputs[ASSETS / f"fonts-{name}.css"] = ("\n".join([header, *lines]) + "\n").encode("utf-8")
    # Distribute the official math face unchanged: subsetting its cmap can remove the MATH
    # variants and assemblies needed by native MathML. It is embedded only when math renders.
    math_source = SOURCES["stix-two-math"]
    math_path, math_sha = math_source["files"]["regular"]
    math_payload = blobs[math_sha]
    math_font = TTFont(io.BytesIO(math_payload))
    if "MATH" not in math_font or not math_font["MATH"].table.MathVariants:
        sys.exit("STIX Two Math must retain its OpenType MATH variants and assemblies")
    math_features = sorted({
        item.FeatureTag for tag in ("GSUB", "GPOS") if tag in math_font
        for item in math_font[tag].table.FeatureList.FeatureRecord
    })
    faces.append({
        "family": "STIX Two Math", "style": "normal", "weight": 400, "catalog": "fonts-math.css",
        "monospace": False, "source": "stix-two-math", "file": math_path, "location": {},
        "name": math_font["name"].getDebugName(4), "postscript": math_font["name"].getDebugName(6),
        "version": math_font["name"].getDebugName(5), "glyphs": math_font["maxp"].numGlyphs,
        "woff2_bytes": len(math_payload), "sha256": math_sha,
        "features": math_features, "source_features": math_features, "features_without_effect": [],
        "cmap": ranges(sorted(math_font.getBestCmap())), "unmodified": True,
    })
    math_css = (
        f"/* Generated by scripts/fonts/build.py; {raw_url(math_source, math_path)}; SHA-256 {math_sha}. */\n"
        '@font-face { font-family: "STIX Two Math"; font-style: normal; font-weight: 400; '
        f'font-display: block; src: url(data:font/woff2;base64,{base64.b64encode(math_payload).decode("ascii")}) '
        'format("woff2"); }\n'
    )
    outputs[ROOT / "assets/fonts-math.css"] = math_css.encode("utf-8")
    outputs[ASSETS / "font-licenses/stix-two-math.txt"] = fetch(math_source, *math_source["license"])
    manifest = {
        "generator": "scripts/fonts/build.py", "tools": TOOLS,
        "required": {
            "ranges": [f"{a:04X}-{b:04X}" for a, b in REQUIRED_RANGES],
            "codepoints": [f"{cp:04X}" for cp in REQUIRED_EXTRA],
            "report_sources": REPORT_SOURCES,
            "report_codepoints": [f"{cp:04X}" for cp in reports if cp > 0x7E],
            "upstream_gaps_allowed": {f"{cp:04X}": name for cp, name in UPSTREAM_GAPS_ALLOWED.items()},
            "monospace_gaps_allowed": {f"{cp:04X}": name for cp, name in MONOSPACE_GAPS_ALLOWED.items()},
            "features": FEATURES_REQUIRED,
            "features_omitted": FEATURES_OMITTED,
            "kerning_threshold_em": KERN_THRESHOLD_EM,
        },
        "sources": sources,
        "faces": faces,
    }
    outputs[ASSETS / "font-coverage.txt"] = coverage_table(faces).encode("utf-8")
    outputs[ASSETS / "font-manifest.json"] = (json.dumps(manifest, indent=1, ensure_ascii=False) + "\n").encode("utf-8")
    return outputs


def coverage_table(faces: list[dict]) -> str:
    """One line per family, `<family lowercase>\t<hex ranges>`, the union over its weights (STY glyph check)."""
    covered: dict[str, set[int]] = {}
    for face in faces:
        points = covered.setdefault(face["family"].lower(), set())
        for part in face["cmap"].split(","):
            low, _, high = part.partition("-")
            points.update(range(int(low, 16), int(high or low, 16) + 1))
    lines = []
    for family, points in sorted(covered.items()):
        ordered = sorted(points)
        runs, start, prev = [], ordered[0], ordered[0]
        for cp in ordered[1:]:
            if cp != prev + 1:
                runs.append((start, prev))
                start = cp
            prev = cp
        runs.append((start, prev))
        lines.append(family + "\t" + ",".join(f"{a:04X}" if a == b else f"{a:04X}-{b:04X}" for a, b in runs) + "\n")
    return "".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="fail unless the committed files match a rebuild")
    args = parser.parse_args()
    outputs = build()
    stale = [path for path, data in outputs.items() if not path.exists() or path.read_bytes() != data]
    if args.check:
        if stale:
            sys.exit("Stale: " + ", ".join(str(path.relative_to(ROOT)) for path in stale))
        print(f"{len(outputs)} files match")
        return
    for path, data in outputs.items():
        path.write_bytes(data)
    for path in outputs:
        print(f"{path.relative_to(ROOT)} {len(outputs[path]):,} bytes")


if __name__ == "__main__":
    main()
