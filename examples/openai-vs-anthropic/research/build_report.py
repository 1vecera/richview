"""Render the report: content/report.template.md + chart specs + facts + ledger → report.rmd and content/report.json.

Template tokens: {{fact:key}} inserts a computed value; {{src:ID,ID}} inserts linked source ids; {{chart:id}} inserts a
RichView chart fence with inline data plus its source line; {{table:name}} inserts a table fence built here. The
.rmd is the canonical RichView document; report.json carries the same content as theme-neutral sections and blocks.
"""

import csv
import json
import re
from typing import Any

import yaml

import common

TEMPLATE = common.REPORT_DIR / "content" / "report.template.md"
FRONTMATTER = {
    # Daniel chose the title on 10 November 2026 (asks Q14); the deck carries the verdict.
    "title": "The Most Expensive Race Ever Run",
    "description": (
        "Anthropic is winning the people who build with AI; OpenAI still has everyone else. "
        "Attention, use, money and capability, population by population, January 2023–September 2026."
    ),
    "author": "Daniel Vecera",
    "date": "2026-10-02",
    "version": "0.1.0",
    "slug": "openai-vs-anthropic",
    "tags": ["ai", "openai", "anthropic", "market-perception", "public-data"],
    "styles": ["scientific", "report.rstyle.yaml"],
    # The image revisions are separate from the fixed 2 October research snapshot.
    "title_image": {
        "src": "assets/cover-two-publics.svg",
        "alt": (
            "Two publics. Everyone else: ChatGPT has 40× as many US App Store ratings as Claude "
            "(10,883,936 against 271,740). People who build with AI: 39% of professional developers used "
            "Claude Code at work against 16% for OpenAI's Codex. Below, sixteen measures on one line of "
            "Anthropic's share of the OpenAI plus Anthropic total, from 2.4% for US App Store ratings to "
            "70.9% for coding agents at work; each measure has its own population and window."
        ),
        "caption": (
            "Original RichView graphic, 23 October 2026, MIT; embedded fonts SIL OFL 1.1. "
            "Each measure is normalized within its own OpenAI–Anthropic pair; populations and windows "
            "differ and audiences can overlap. Sources: D-ITUNES (2 October), S065 (mid-2026) and the "
            "sixteen population measures."
        ),
        "width": 1200,
        "height": 630,
        "fit": "contain",
    },
    # The format marker the core migration wrote into report.rmd (TYP-78).
    "format": "rmd/1",
}


def load_sources() -> dict[str, dict[str, str]]:
    """Map every ledger id to its row."""
    with (common.LEDGER_DIR / "sources.csv").open(newline="") as handle:
        return {row["id"]: row for row in csv.DictReader(handle)}


def source_links(ids: list[str], sources: dict[str, dict[str, str]]) -> str:
    """Render source ids as Markdown links to their original URLs (ids without a URL stay plain)."""
    links = []
    for source_id in ids:
        url = sources[source_id]["url"]
        links.append(f"[{source_id}]({url})" if url else source_id)
    return ", ".join(links)


def read_chart_rows(spec: dict[str, Any]) -> list[dict[str, Any]]:
    """Read a chart CSV back with numbers restored, so inline YAML carries numbers, not strings."""
    with (common.REPORT_DIR / str(spec["csv"])).open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    numeric = {str(value) for value in dict(spec["encoding"]).values()}  # type: ignore[arg-type]
    for row in rows:
        for key, value in row.items():
            if key in numeric and value not in ("", None):
                try:
                    row[key] = float(value) if "." in value or "e" in value.lower() else int(value)
                except ValueError:
                    pass
    return rows


def chart_block(spec: dict[str, Any], sources: dict[str, dict[str, str]]) -> str:
    """Build the chart fence (only the keys the RichView chart contract defines) and its source line."""
    encoding = dict(spec["encoding"])  # type: ignore[arg-type]
    keep = {value for value in encoding.values()}
    rows = [{key: value for key, value in row.items() if key in keep} for row in read_chart_rows(spec)]
    body: dict[str, Any] = {
        "type": spec["type"],
        "title": spec["title"],
        "subtitle": spec["subtitle"],
        "data": {"inline": rows},
        "encoding": encoding,
        "axes": spec["axes"],
    }
    if spec["annotations"]:
        body["annotations"] = spec["annotations"]
    body["style"] = {"ratio": spec["ratio"]}
    fence = yaml.safe_dump(body, sort_keys=False, allow_unicode=True, width=1000, default_flow_style=None)
    note = f" {spec['notes']}" if spec["notes"] else ""
    caption = (
        f"*Sources: {source_links(list(spec['sources']), sources)}. Population: {spec['population']}. "  # type: ignore[arg-type]
        f"Window: {spec['window']}. Data: [`{spec['csv']}`]({spec['csv']}).{note}*"
    )
    return f"```chart\n{fence}```\n\n{caption}"


def table_block(name: str, sources: dict[str, dict[str, str]]) -> str:
    """Build a RichView table fence for one of the report's tables (inline rows from the chart and ledger data)."""
    if name == "cited-sources":
        cited = sorted(
            re.findall(r"\b(?:[CKMES]\d{3}|D-[A-Z]+)\b", TEMPLATE.read_text())
            + re.findall(
                r"\b(?:[CKMES]\d{3}|D-[A-Z]+)\b", json.dumps(json.loads((common.CHART_DIR / "index.json").read_text()))
            )
        )
        unique = sorted(set(cited), key=lambda source_id: (source_id[0], source_id))
        rows = [
            {
                "id": source_id,
                "source": f"{sources[source_id]['publisher']}: {sources[source_id]['title']}"[:160],
                "date": sources[source_id]["published_date"] or sources[source_id]["accessed_date"],
                "link": sources[source_id]["url"],
            }
            for source_id in unique
        ]
        columns = [
            {"key": "id", "label": "Id"},
            {"key": "source", "label": "Source"},
            {"key": "date", "label": "Published"},
            {"key": "link", "label": "Link"},
        ]
    else:
        raise KeyError(name)
    # The first rows show; the rest wait in the table's own disclosure, so the ledger does not lengthen the report.
    fence = yaml.safe_dump(
        {"data": {"inline": rows}, "columns": columns, "max_rows": 8}, sort_keys=False, allow_unicode=True, width=1000
    )
    return f"```table\n{fence}```"


def render(
    template: str,
    specs: dict[str, dict[str, Any]],
    facts: dict[str, dict[str, Any]],
    sources: dict[str, dict[str, str]],
) -> str:
    """Replace every template token; unknown keys raise so a typo can never ship a blank number."""
    template = re.sub(r"\{\{fact:([a-z0-9_]+)\}\}", lambda match: str(facts[match.group(1)]["display"]), template)
    template = re.sub(
        r"\{\{src:([A-Z0-9,\- ]+)\}\}",
        lambda match: source_links([part.strip() for part in match.group(1).split(",")], sources),
        template,
    )
    template = re.sub(
        r"\{\{chart:([a-z0-9\-]+)\}\}", lambda match: chart_block(specs[match.group(1)], sources), template
    )
    return re.sub(r"\{\{table:([a-z0-9\-]+)\}\}", lambda match: table_block(match.group(1), sources), template)


def portable_content(body: str) -> dict[str, Any]:
    """Split the rendered Markdown into theme-neutral sections and typed blocks for other renderers."""
    sections = []
    for chunk in re.split(r"\n(?=## )", body.strip()):
        heading = chunk.splitlines()[0].lstrip("# ").strip()
        blocks = []
        for part in re.split(r"(```[a-z]+\n.*?```)", chunk, flags=re.DOTALL):
            if part.startswith("```"):
                tag, content = part[3:].split("\n", 1)
                payload = yaml.safe_load(content.removesuffix("```"))
                if tag == "chart":
                    blocks.append({"type": "chart", "title": payload["title"]})
                else:
                    blocks.append({"type": tag, **payload})
            elif part.strip():
                blocks.append({"type": "markdown", "text": part.strip()})
        sections.append(
            {"id": re.sub(r"[^a-z0-9]+", "-", heading.lower()).strip("-"), "heading": heading, "blocks": blocks}
        )
    return {"frontmatter": FRONTMATTER, "sections": sections}


def main() -> None:
    """Write report.rmd and content/report.json from the template and current analysis outputs."""
    specs = {spec["id"]: spec for spec in json.loads((common.CHART_DIR / "index.json").read_text())}
    facts = json.loads((common.REPORT_DIR / "data" / "facts.json").read_text())
    sources = load_sources()
    facts["ledger_rows"] = {"value": len(sources), "display": f"{len(sources):,}", "sources": [], "note": "ledger rows"}
    body = render(TEMPLATE.read_text(), specs, facts, sources)
    frontmatter = yaml.safe_dump(FRONTMATTER, sort_keys=False, allow_unicode=True, width=1000)
    (common.REPORT_DIR / "report.rmd").write_text(f"---\n{frontmatter}---\n\n{body.strip()}\n")
    content = portable_content(body)
    for section in content["sections"]:  # type: ignore[union-attr]
        for block in section["blocks"]:
            if block["type"] == "chart":
                block["chart_id"] = next(spec_id for spec_id, spec in specs.items() if spec["title"] == block["title"])
    (common.REPORT_DIR / "content" / "report.json").write_text(
        json.dumps(content, indent="\t", ensure_ascii=False) + "\n"
    )
    print(f"report.rmd and report.json written; sections={len(content['sections'])}")  # type: ignore[arg-type]


if __name__ == "__main__":
    main()
