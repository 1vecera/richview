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


def snapshot_day(ratings: dict[str, str]) -> str:
    """The snapshot's day as '18 Nov 2026', read from the ratings measure's definition."""
    found = re.search(r"(\d{4})-(\d{2})-(\d{2})", ratings["definition"])
    assert found, "the ratings measure names its snapshot date"
    year, month, day = (int(part) for part in found.groups())
    names = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split()
    return f"{day} {names[month - 1]} {year}"


def track(rows, *, left: float, right: float, top: float, bottom: float, radius: float, labels: bool) -> list[str]:
    """One lane per measure, OpenAI's strongest public first; a glowing runner stands at Anthropic's share."""
    lane = (bottom - top) / len(rows)

    def at(share: float) -> float:
        return left + share / 100 * (right - left)

    out = []
    for i, row in enumerate(rows):
        y = top + i * lane
        out.append(
            f'<rect x="{left:.1f}" y="{y + 1.5:.1f}" width="{right - left:.1f}" height="{lane - 3:.1f}" rx="3" '
            f'fill="{INK}" fill-opacity="{0.06 if i % 2 == 0 else 0.035}"/>'
        )
    middle = at(50)
    out.append(
        f'<line x1="{middle:.1f}" y1="{top - 6:.1f}" x2="{middle:.1f}" y2="{bottom + 6:.1f}" stroke="{SOFT}" '
        f'stroke-width="{1.5 if labels else 3}" stroke-dasharray="{"4 5" if labels else "8 10"}"/>'
    )
    for i, row in enumerate(rows):
        share = float(row["anthropic_share_pct"])
        cy = top + (i + 0.5) * lane
        x = at(share)
        anthropic = share > 50
        colour = ANTHROPIC if anthropic else OPENAI
        name = f'{row["measure"]}: Anthropic {share:g}% of the pair ({row["source_id"]})'
        out.append(
            f'<line x1="{left:.1f}" y1="{cy:.1f}" x2="{x:.1f}" y2="{cy:.1f}" stroke="{colour}" '
            f'stroke-opacity="0.4" stroke-width="{2 if labels else 4}"/>'
        )
        out.append(
            f'<circle cx="{x:.1f}" cy="{cy:.1f}" r="{radius * 2.6:.1f}" fill="url(#{"glow-a" if anthropic else "glow-o"})"/>'
        )
        out.append(
            f'<circle cx="{x:.1f}" cy="{cy:.1f}" r="{radius}" fill="{colour}" stroke="{PAPER}" stroke-width="{radius / 4:.1f}">'
            f"<title>{escape(name)}</title></circle>"
        )
        if labels:
            out.append(text(left - 14, cy + 5, escape(row["measure"]), cls="lane", anchor="end"))
            beyond = x + radius + 8 if share < 88 else x - radius - 8
            out.append(
                text(beyond, cy + 5, f"{share:.1f}%", cls="share", anchor="start" if share < 88 else "end", fill=colour)
            )
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
    day = snapshot_day(ratings)

    left, right = 262.0, WIDTH - 72.0
    wide = [
        # The two headline numbers as one line, so the card still speaks at thumbnail size.
        text(72, 96, escape(ratio), cls="top", fill=OPENAI),
        text(196, 76, "as many US App Store ratings", cls="top-lede"),
        text(196, 102, "for ChatGPT as for Claude", cls="top-lede"),
        text(612, 96, f"{claude_code}%", cls="top", fill=ANTHROPIC),
        text(736, 76, "of professional developers used Claude Code", cls="top-lede"),
        text(736, 102, f'at work; <tspan style="fill:{OPENAI}" class="top-num">{codex}%</tspan> used OpenAI’s Codex', cls="top-lede"),
        text(72, 132, f"{openai_ratings:,} vs {anthropic_ratings:,} ratings · {ratings['source_id']}, {day}", cls="src"),
        text(612, 132, f"JetBrains Developer Ecosystem survey, 2026 · {agents['source_id']}", cls="src"),
        f'<line x1="72" y1="156" x2="{WIDTH - 72}" y2="156" stroke="{RULE}" stroke-width="1.5"/>',
        # The race: one lane per measure.
        text(72, 190, f"ONE RACE, {len(rows)} LANES · {day.upper()}", cls="kicker", fill=SOFT),
        text(WIDTH - 72, 190, "Anthropic’s share of each OpenAI + Anthropic pair · populations overlap", cls="note", anchor="end"),
        text((left + right) / 2, 214, "EVEN", cls="tick", anchor="middle"),
        *track(rows, left=left, right=right, top=226, bottom=566, radius=7, labels=True),
        text(left, 600, "← OpenAI’s publics", cls="pole", fill=OPENAI_LABEL),
        text(right, 600, "Anthropic’s publics →", cls="pole", fill=ANTHROPIC_LABEL, anchor="end"),
    ]
    compact = [
        text(60, 96, "EVERYONE ELSE", cls="kicker-l", fill=OPENAI_LABEL),
        text(52, 236, escape(ratio), cls="hero-l", fill=OPENAI),
        text(60, 292, "App Store ratings,", cls="lede-l"),
        text(60, 340, "ChatGPT vs Claude", cls="lede-l"),
        text(640, 96, "BUILDERS", cls="kicker-l", fill=ANTHROPIC_LABEL),
        text(632, 236, f"{claude_code}%", cls="hero-l", fill=ANTHROPIC),
        text(640, 292, "used Claude Code", cls="lede-l"),
        text(640, 340, f"at work; Codex {codex}%", cls="lede-l"),
        # Below card size the lanes keep their marks and drop their names.
        *track(rows, left=60, right=WIDTH - 60, top=376, bottom=556, radius=6.5, labels=False),
        text(60, 600, "← OpenAI leads", cls="pole-l", fill=OPENAI_LABEL),
        text(WIDTH - 60, 600, "Anthropic leads →", cls="pole-l", fill=ANTHROPIC_LABEL, anchor="end"),
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
.kicker, .kicker-l, .tick, .src, .note, .pole, .pole-l {{ font-family: "RichView Sans", system-ui, sans-serif; }}
.top, .hero-l, .lede-l {{ font-family: "RichView Serif", Georgia, serif; }}
.top-lede, .lane, .share {{ font-family: "RichView Sans", system-ui, sans-serif; }}
.kicker {{ font-size: 17px; font-weight: 600; letter-spacing: 3px; }}
.top {{ font-size: 54px; font-weight: 600; letter-spacing: -1px; font-variant-numeric: lining-nums; }}
.top-num {{ font-weight: 600; font-variant-numeric: tabular-nums; }}
.top-lede {{ font-size: 19px; fill: {SOFT}; }}
.lane {{ font-size: 15px; fill: {SOFT}; }}
.share {{ font-size: 13px; font-weight: 600; font-variant-numeric: tabular-nums; }}
.src {{ font-size: 16px; fill: {FAINT}; }}
.note {{ font-size: 16px; fill: {SOFT}; }}
.tick {{ font-size: 13px; letter-spacing: 2px; fill: {SOFT}; }}
.pole {{ font-size: 19px; font-weight: 600; }}
.kicker-l {{ font-size: 40px; font-weight: 600; letter-spacing: 4px; }}
.hero-l {{ font-size: 150px; font-weight: 600; letter-spacing: -4px; }}
.lede-l {{ font-size: 44px; }}
.pole-l {{ font-size: 42px; font-weight: 600; }}
.compact {{ display: none; }}
@media (max-width: 620px) {{ .wide {{ display: none; }} .compact {{ display: inline; }} }}
"""
    metadata = (
        "RichView report cover, revision 2026-11-18 (the race as a track). Original artwork by Daniel Vecera, MIT "
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
            f'<stop offset="1" stop-color="{ANTHROPIC_FIELD}"/></linearGradient>'
            # A runner's glow: its own colour, fading evenly to nothing.
            f'<radialGradient id="glow-a"><stop offset="0.3" stop-color="{ANTHROPIC}" stop-opacity="0.55"/>'
            f'<stop offset="1" stop-color="{ANTHROPIC}" stop-opacity="0"/></radialGradient>'
            f'<radialGradient id="glow-o"><stop offset="0.3" stop-color="{OPENAI}" stop-opacity="0.55"/>'
            f'<stop offset="1" stop-color="{OPENAI}" stop-opacity="0"/></radialGradient></defs>',
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
