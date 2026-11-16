"""Build the report cover infographic from the frozen population shares and facts.

Run from research: uv run --frozen build_cover.py. The cover reads only committed snapshot
files (data/charts/population-shares.csv, data/facts.json) and the bundled scientific fonts,
so rebuilding it never fetches anything and never changes a number.
"""

import csv
import hashlib
import json
import re
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "assets/cover-two-publics.svg"
SHARES = "data/charts/population-shares.csv"
FACTS = "data/facts.json"
FONT_CSS = "crates/richview-core/assets/fonts-scientific.css"
WIDTH, HEIGHT = 1200, 630

# A deep card with light type (S27-997), so the cover reads on Sleek's dark page as well as on the
# light styles: one image serves every style, since an SVG image cannot follow the page's scheme in
# every engine (WebKit follows the system's). Every text colour is at least 6.4:1 on all three
# field colours; the paper centre is a step above Sleek's page, and the rule outlines the card.
PAPER, INK, SOFT, FAINT, RULE = "#161a1f", "#f6f3ef", "#cac7c2", "#a7a49f", "#43474e"
# The report's charts draw Anthropic in the first (blue) series colour and OpenAI in the
# second (orange), so the cover keeps the same company identities, lightened for the dark card:
# marks and hero numbers in the main colour, kickers and pole labels in the lighter label colour,
# and each company's side of the card in a deep field colour of its hue.
ANTHROPIC, ANTHROPIC_LABEL, ANTHROPIC_FIELD = "#5db5eb", "#96cff6", "#10232f"
OPENAI, OPENAI_LABEL, OPENAI_FIELD = "#eb9854", "#f2bb8a", "#2d1c10"

# Measures named on the spectrum, with their label row above the axis (1 = nearer).
ANNOTATED = {
    "US iOS ratings": ("App Store ratings", 2, "start"),
    "Weekly use": ("Weekly use, US adults", 1, "start"),
    "npm SDK": ("npm SDK downloads", 2, "middle"),
    "Firms paying": ("US firms paying", 1, "start"),
    "Agent at work": ("Coding agent at work", 2, "start"),
}


def content_digest(relative: str) -> str:
    """Hash the parsed rows or JSON, so a byte-level rewrite of the same snapshot keeps the hash."""
    path = ROOT / relative
    value = list(csv.DictReader(path.open(newline=""))) if path.suffix == ".csv" else json.loads(path.read_text())
    canonical = json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha256(canonical.encode()).hexdigest()


def fonts() -> str:
    """Return the serif and sans @font-face rules the cover uses, copied from the bundled CSS."""
    # Search upward so an isolated snapshot copy inside the repository finds the same fonts.
    css = next((p / FONT_CSS).read_text() for p in ROOT.parents if (p / FONT_CSS).is_file())
    rules = re.findall(r"@font-face \{[^}]*\}", css)
    # The sign subsets (unicode-range faces for ⁺ ⁻ ₊ ₋, STY-63) are not used on the cover.
    keep = [r for r in rules if re.search(r'"RichView (Serif|Sans)"', r) and "italic" not in r and "unicode-range" not in r]
    assert len(keep) == 4, "expected Serif 400/600 and Sans 400/600"
    return "\n".join(keep)


def text(x, y, body, *, cls, anchor="start", fill=None, extra=""):
    fill_attr = f' style="fill:{fill}"' if fill else ""
    return f'<text x="{x:.1f}" y="{y:.1f}" class="{cls}" text-anchor="{anchor}"{fill_attr}{extra}>{body}</text>'


def axis_x(share: float) -> float:
    return 96 + share / 100 * (WIDTH - 192)


def swarm(values: list[float], radius: float) -> list[tuple[float, float]]:
    """Place each dot on the axis, stacking colliding dots alternately above and below."""
    placed: list[tuple[float, float]] = []
    for value in values:
        x = axis_x(value)
        for offset in (0, -1, 1, -2, 2):
            y = offset * (radius * 2 + 2)
            if all((x - px) ** 2 + (y - py) ** 2 >= (radius * 2 + 1) ** 2 for px, py in placed):
                placed.append((x, y))
                break
    return placed


def spectrum(rows, *, axis_y, radius, labels: bool) -> list[str]:
    out = [
        f'<line x1="{axis_x(0):.1f}" y1="{axis_y}" x2="{axis_x(100):.1f}" y2="{axis_y}" stroke="{INK}" stroke-width="2"/>',
        f'<line x1="{axis_x(50):.1f}" y1="{axis_y - 30}" x2="{axis_x(50):.1f}" y2="{axis_y + 30}" stroke="{FAINT}" stroke-width="1.5" stroke-dasharray="4 5"/>',
    ]
    shares = [float(r["anthropic_share_pct"]) for r in rows]
    for row, (x, dy) in zip(rows, swarm(shares, radius), strict=True):
        share = float(row["anthropic_share_pct"])
        colour = ANTHROPIC if share > 50 else OPENAI
        name = f'{row["measure"]}: Anthropic {share:g}% of the pair ({row["source_id"]})'
        out.append(
            f'<circle cx="{x:.1f}" cy="{axis_y + dy:.1f}" r="{radius}" fill="{colour}" stroke="{PAPER}" stroke-width="2.5"><title>{escape(name)}</title></circle>'
        )
        if labels and row["measure"] in ANNOTATED:
            label, level, anchor = ANNOTATED[row["measure"]]
            top = axis_y + dy - radius - 4
            base = axis_y - 26 - level * 30
            out.append(f'<line x1="{x:.1f}" y1="{base + 7:.1f}" x2="{x:.1f}" y2="{top:.1f}" stroke="{FAINT}" stroke-width="1.2"/>')
            lx = x - 4 if anchor == "start" else x
            out.append(text(lx, base, f'{escape(label)} <tspan class="num" style="fill:{colour}">{share:g}%</tspan>', cls="dot", anchor=anchor))
    return out


def build() -> str:
    rows = list(csv.DictReader((ROOT / SHARES).open(newline="")))
    rows.sort(key=lambda r: float(r["anthropic_share_pct"]))
    by = {r["measure"]: r for r in rows}
    facts = json.loads((ROOT / FACTS).read_text())
    ratings, agents = by["US iOS ratings"], by["Agent at work"]
    ratio = facts["ios_ratings_ratio"]["display"]
    assert facts["ios_ratings_ratio"]["sources"] == [ratings["source_id"]]
    openai_ratings, anthropic_ratings = int(ratings["openai_value"]), int(ratings["anthropic_value"])
    claude_code, codex = int(float(agents["anthropic_value"])), int(float(agents["openai_value"]))
    lo, hi = rows[0], rows[-1]
    lo_share, hi_share = float(lo["anthropic_share_pct"]), float(hi["anthropic_share_pct"])

    wide = [
        # Left: everyone else.
        text(72, 92, "EVERYONE ELSE", cls="kicker", fill=OPENAI_LABEL),
        text(66, 222, escape(ratio), cls="hero", fill=OPENAI),
        text(72, 268, "as many US App Store ratings", cls="lede"),
        text(72, 302, "for ChatGPT as for Claude", cls="lede"),
        text(72, 338, f"{openai_ratings:,} vs {anthropic_ratings:,} · D-ITUNES, 2 Oct 2026", cls="src"),
        # Right: the people who build with AI.
        text(648, 92, "PEOPLE WHO BUILD WITH AI", cls="kicker", fill=ANTHROPIC_LABEL),
        text(642, 222, f'{claude_code}%<tspan class="hero-vs" style="fill:{SOFT}"> vs </tspan><tspan class="hero-vs" style="fill:{OPENAI}">{codex}%</tspan>', cls="hero", fill=ANTHROPIC),
        text(648, 268, "of professional developers used", cls="lede"),
        text(648, 302, f"Claude Code at work; {codex}% used Codex", cls="lede"),
        text(648, 338, "JetBrains Developer Ecosystem Survey, mid-2026 · S065", cls="src"),
        f'<line x1="600" y1="64" x2="600" y2="344" stroke="{RULE}" stroke-width="1.5"/>',
        # Bottom: every measure on one spectrum.
        f'<line x1="72" y1="384" x2="{WIDTH - 72}" y2="384" stroke="{RULE}" stroke-width="1.5"/>',
        text(72, 414, f"ONE RACE, {len(rows)} MEASURES", cls="kicker", fill=SOFT),
        text(WIDTH - 72, 414, "Anthropic’s share of each OpenAI + Anthropic total", cls="note", anchor="end"),
        *spectrum(rows, axis_y=536, radius=10, labels=True),
        text(axis_x(0), 584, "0%", cls="tick", anchor="middle"),
        text(axis_x(50), 584, "50% · even", cls="tick", anchor="middle"),
        text(axis_x(100), 584, "100%", cls="tick", anchor="middle"),
        text(72, 612, "← OpenAI’s publics", cls="pole", fill=OPENAI_LABEL),
        text(WIDTH - 72, 612, "Anthropic’s publics →", cls="pole", fill=ANTHROPIC_LABEL, anchor="end"),
        text(WIDTH / 2, 612, "Snapshot 2 Oct 2026 · one dot per measure, each normalised within its own pair · populations overlap", cls="src", anchor="middle"),
    ]
    compact = [
        text(60, 96, "EVERYONE ELSE", cls="kicker-l", fill=OPENAI_LABEL),
        text(52, 252, escape(ratio), cls="hero-l", fill=OPENAI),
        text(60, 312, "App Store ratings,", cls="lede-l"),
        text(60, 362, "ChatGPT vs Claude", cls="lede-l"),
        text(640, 96, "BUILDERS", cls="kicker-l", fill=ANTHROPIC_LABEL),
        text(632, 252, f"{claude_code}%", cls="hero-l", fill=ANTHROPIC),
        text(640, 312, "used Claude Code", cls="lede-l"),
        text(640, 362, f"at work; Codex {codex}%", cls="lede-l"),
        *spectrum(rows, axis_y=486, radius=15, labels=False),
        # Mid-width covers (the default reading column) keep the scale and both extremes.
        '<g class="mid">',
        text(axis_x(0), 540, "0%", cls="tick-l", anchor="middle"),
        text(axis_x(50), 540, "50% · even", cls="tick-l", anchor="middle"),
        text(axis_x(100), 540, "100%", cls="tick-l", anchor="middle"),
        f'<line x1="{axis_x(lo_share):.1f}" y1="{432}" x2="{axis_x(lo_share):.1f}" y2="{466}" stroke="{FAINT}" stroke-width="2"/>',
        text(axis_x(lo_share) - 6, 422, f'App Store <tspan class="num" style="fill:{OPENAI}">{lo_share:g}%</tspan>', cls="dot-l"),
        f'<line x1="{axis_x(hi_share):.1f}" y1="{432}" x2="{axis_x(hi_share):.1f}" y2="{466}" stroke="{FAINT}" stroke-width="2"/>',
        text(axis_x(hi_share) - 6, 422, f'Coding agents <tspan class="num" style="fill:{ANTHROPIC}">{hi_share:g}%</tspan>', cls="dot-l"),
        "</g>",
        text(60, 596, "← OpenAI leads", cls="pole-l", fill=OPENAI_LABEL),
        text(WIDTH - 60, 596, "Anthropic leads →", cls="pole-l", fill=ANTHROPIC_LABEL, anchor="end"),
    ]
    source_hash, facts_hash = content_digest(SHARES), content_digest(FACTS)
    title = "Two publics: OpenAI leads everyone else, Anthropic leads the people who build with AI"
    desc = (
        f"ChatGPT has {ratio} as many US App Store ratings as Claude ({openai_ratings:,} against {anthropic_ratings:,}). "
        f"{claude_code}% of professional developers used Claude Code at work against {codex}% for OpenAI's Codex. "
        f"Across {len(rows)} measures, Anthropic's share of the two-company total runs from "
        f"{float(lo['anthropic_share_pct']):g}% ({lo['measure']}) to {float(hi['anthropic_share_pct']):g}% ({hi['measure']})."
    )
    style = f"""
{fonts()}
text {{ fill: {INK}; font-kerning: normal; }}
.kicker, .kicker-l, .tick, .src, .dot, .note, .pole, .pole-l {{ font-family: "RichView Sans", system-ui, sans-serif; }}
.hero, .hero-l, .lede, .lede-l {{ font-family: "RichView Serif", Georgia, serif; }}
.kicker {{ font-size: 19px; font-weight: 600; letter-spacing: 3px; }}
.hero {{ font-size: 124px; font-weight: 600; letter-spacing: -3px; font-variant-numeric: lining-nums; }}
.hero-vs {{ font-size: 64px; font-weight: 400; letter-spacing: 0; }}
.lede {{ font-size: 28px; }}
.src {{ font-size: 16px; fill: {FAINT}; }}
.note {{ font-size: 18px; fill: {SOFT}; }}
.dot {{ font-size: 17px; fill: {SOFT}; }}
.num {{ font-weight: 600; font-variant-numeric: tabular-nums; }}
.tick {{ font-size: 15px; fill: {FAINT}; }}
.pole {{ font-size: 19px; font-weight: 600; }}
.kicker-l {{ font-size: 40px; font-weight: 600; letter-spacing: 4px; }}
.hero-l {{ font-size: 176px; font-weight: 600; letter-spacing: -4px; }}
.lede-l {{ font-size: 44px; }}
.pole-l {{ font-size: 42px; font-weight: 600; }}
.tick-l, .dot-l {{ font-family: "RichView Sans", system-ui, sans-serif; }}
.tick-l {{ font-size: 28px; fill: {FAINT}; }}
.dot-l {{ font-size: 32px; fill: {SOFT}; }}
.mid {{ display: none; }}
@media (min-width: 440px) {{ .mid {{ display: inline; }} }}
.compact {{ display: none; }}
@media (max-width: 620px) {{ .wide {{ display: none; }} .compact {{ display: inline; }} }}
"""
    metadata = (
        "RichView report cover, revision 2026-11-09 (dark-ready). Original artwork by Daniel Vecera, MIT "
        "(assets/licenses/richview-MIT.txt). Embedded fonts: Source Serif 4 and Source Sans 3 as "
        "RichView Serif/Sans, SIL Open Font License 1.1 (assets/licenses/source-serif-4-OFL.txt, "
        f"assets/licenses/source-sans-3-OFL.txt). Data: {SHARES} content sha256 {source_hash}; {FACTS} content sha256 "
        f"{facts_hash}. Generated by research/build_cover.py; no data value is typed by hand (source labels are fixed text)."
    )
    return "\n".join(
        [
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}" width="{WIDTH}" height="{HEIGHT}" role="img" aria-labelledby="t d">',
            f"<title id=\"t\">{escape(title)}</title>",
            f"<desc id=\"d\">{escape(desc)}</desc>",
            f"<metadata>{escape(metadata)}</metadata>",
            f"<style>{style}</style>",
            "<defs><linearGradient id=\"field\" x1=\"0\" x2=\"1\" y1=\"0\" y2=\"0\">"
            f'<stop offset="0" stop-color="{OPENAI_FIELD}"/><stop offset="0.5" stop-color="{PAPER}"/>'
            f'<stop offset="1" stop-color="{ANTHROPIC_FIELD}"/></linearGradient></defs>',
            f'<rect width="{WIDTH}" height="{HEIGHT}" rx="20" fill="url(#field)"/>',
            f'<rect x="0.75" y="0.75" width="{WIDTH - 1.5}" height="{HEIGHT - 1.5}" rx="19.5" fill="none" stroke="{RULE}" stroke-width="1.5"/>',
            '<g class="wide">',
            *wide,
            "</g>",
            '<g class="compact">',
            *compact,
            "</g>",
            "</svg>",
            "",
        ]
    )


if __name__ == "__main__":
    OUTPUT.write_text(build())
    print(OUTPUT.relative_to(ROOT), hashlib.sha256(OUTPUT.read_bytes()).hexdigest())
