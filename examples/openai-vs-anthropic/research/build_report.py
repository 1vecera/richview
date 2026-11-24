"""Render the report: template + chart specs + facts + ledger → report.rmd, content/report.json, content/citations.json.

Template tokens: {{fact:key}} inserts a computed value; {{src:ID,ID}} inserts sources by name, linked to their URLs;
{{cite:ID|words}} links the sentence's own words, such as the date it states, to that source instead of naming it;
{{chart:id}} inserts a RichView chart fence with inline data plus its source line; {{table:name}} inserts a table fence
built here. The .rmd is the canonical RichView document; report.json carries the same content as theme-neutral
sections and blocks. citations.json lists every source link in the order the report shows it, with its URL, its text
("" for a table's link cell) and the ledger ids the sentence cites, so the showcase opens each link's own entry.
"""

import csv
import datetime
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
        "Attention, use, money and capability, population by population, January 2023–October 2026."
    ),
    "author": "Daniel Vecera",
    "date": "2026-11-18",
    "version": "0.1.0",
    "slug": "openai-vs-anthropic",
    "tags": ["ai", "openai", "anthropic", "market-perception", "public-data"],
    "styles": ["scientific", "report.rstyle.yaml"],
    # The cover's text alternative and caption are written from the committed data (title_image()).
    "title_image": {
        "src": "assets/cover-two-publics.svg",
        "alt": "",
        "caption": "",
        "width": 1200,
        "height": 630,
        "fit": "contain",
    },
    # The format marker the core migration wrote into report.rmd (TYP-78).
    "format": "rmd/1",
}


def title_image(sources: dict[str, dict[str, str]]) -> dict[str, str]:
    """The cover's text alternative and caption, from the population shares and facts the cover draws."""
    with (common.CHART_DIR / "population-shares.csv").open(newline="") as handle:
        rows = sorted(csv.DictReader(handle), key=lambda row: float(row["anthropic_share_pct"]))
    by = {row["measure"]: row for row in rows}
    facts = json.loads((common.REPORT_DIR / "data" / "facts.json").read_text())
    ratings, agents = by["US iOS ratings"], by["Agent at work"]
    low, high = rows[0], rows[-1]
    claude_code, codex = int(float(agents["anthropic_value"])), int(float(agents["openai_value"]))
    return {
        "alt": (
            f"One race, {len(rows)} lanes. Everyone else: ChatGPT has {facts['ios_ratings_ratio']['display']} as many "
            f"US App Store ratings as Claude ({int(ratings['openai_value']):,} against "
            f"{int(ratings['anthropic_value']):,}). People who build with AI: {claude_code}% of professional "
            f"developers used Claude Code at work against {codex}% for OpenAI's Codex. Beside them, or below them "
            "on a phone, one lane per measure, each with a runner at Anthropic's share of the OpenAI plus "
            "Anthropic total, from "
            f"{float(low['anthropic_share_pct']):g}% for {low['measure']} to "
            f"{float(high['anthropic_share_pct']):g}% for {high['measure']}; each measure has its own population "
            "and window."
        ),
        "caption": (
            "Original RichView graphic, 23 October 2026, redrawn as a race track on 18 November 2026, MIT; "
            "embedded fonts SIL OFL 1.1. Each measure is normalized within its own OpenAI–Anthropic pair; "
            "populations and windows differ and audiences can overlap. Some inputs are rounded, OpenAI's figures "
            "most of all (its Q3 revenue is about $10bn), so a lane's last decimal is less certain than it looks. "
            "Sources: "
            f"{source_name(sources[ratings['source_id']], dated=False)} ({FRONTMATTER['date']}), "
            f"{source_name(sources[agents['source_id']], dated=False)} (mid-2026) and the "
            f"{len(rows)} population measures."
        ),
    }


class Citations:
    """The report's source links in order of appearance, each with the ledger ids it cites.

    A link is written as a placeholder while the template fills, because the order in which tokens fill is not the
    order of the page; `resolve` then replaces the placeholders in reading order and records each link as it goes.
    """

    def __init__(self) -> None:
        self.links: list[tuple[str, str, list[str]]] = []
        self.tables: list[list[tuple[str, list[str]]]] = []

    def link(self, text: str, url: str, ids: list[str]) -> str:
        """A placeholder for one Markdown link that cites `ids`."""
        self.links.append((text, url, ids))
        return f"\ue000L{len(self.links) - 1}\ue001"

    def table(self, rows: list[tuple[str, list[str]]]) -> str:
        """A placeholder for a table's link cells, each a URL and the ids of its row, in row order."""
        self.tables.append(rows)
        return f"\ue000T{len(self.tables) - 1}\ue001"

    def resolve(self, body: str) -> tuple[str, list[dict[str, Any]]]:
        """Replace the placeholders in reading order and return the body with the citations it shows."""
        found: list[dict[str, Any]] = []

        def put(match: re.Match[str]) -> str:
            index = int(match.group(2))
            if match.group(1) == "T":
                found.extend({"url": url, "text": "", "ids": ids} for url, ids in self.tables[index])
                return ""
            text, url, ids = self.links[index]
            found.append({"url": url, "text": text, "ids": ids})
            return f"[{text}]({url})"

        resolved = re.sub("\ue000([LT])(\\d+)\ue001", put, body)
        assert "\ue000" not in resolved and "\ue001" not in resolved, "an unresolved citation placeholder"
        return resolved, found


def load_sources() -> dict[str, dict[str, str]]:
    """Map every ledger id to its row."""
    with (common.LEDGER_DIR / "sources.csv").open(newline="") as handle:
        return {row["id"]: row for row in csv.DictReader(handle)}


# Readers see what a dataset is, never its ledger id (launch audit, 18 November 2026).
DATASET_NAMES = {
    "D-ARENA": "LMArena leaderboard",
    "D-EPOCH": "Epoch AI benchmarks",
    "D-GH": "GitHub search",
    "D-GT": "Google Trends",
    "D-HN": "Hacker News",
    "D-ITUNES": "App Store ratings",
    "D-LABELS": "this study's model labels",
    "D-NPM": "npm downloads",
    "D-PYPI": "PyPI downloads",
    "D-REDDIT": "Reddit post counts",
    "D-REVIEWS": "app reviews",
    "D-WIKI": "Wikipedia pageviews",
}


def source_name(row: dict[str, str], *, dated: bool = True) -> str:
    """Name a ledger row for readers: a dataset by what it counts, any other source by its publisher and date.

    The publisher drops its parenthetical, a `via` chain and a legal suffix, so "Anthropic, PBC (SEC EDGAR)" reads
    "Anthropic" and "LMArena (Arena) via Hugging Face …" reads "LMArena". The date is the publication day, left out
    where the text dates the source itself.
    """
    if row["id"] in DATASET_NAMES:
        return DATASET_NAMES[row["id"]]
    publisher = re.sub(r"\s*\([^)]*\)", "", row["publisher"]).split(" via ")[0]
    publisher = re.sub(r",\s*(PBC|Inc\.?|LLC)$", "", publisher.strip())
    published = row["published_date"]
    if dated and re.fullmatch(r"\d{4}-\d{2}-\d{2}", published):
        day = datetime.date.fromisoformat(published)
        return f"{publisher}, {day.day} {day:%B %Y}"
    return publisher


def published_day(row: dict[str, str]) -> datetime.date | None:
    """Return a non-dataset row's publication day, or None where it has none."""
    if row["id"] in DATASET_NAMES or not re.fullmatch(r"\d{4}-\d{2}-\d{2}", row["published_date"]):
        return None
    return datetime.date.fromisoformat(row["published_date"])


def dated_links(publisher: str, entries: list[tuple[datetime.date, str, list[str]]], cited: Citations) -> str:
    """Name one publisher once and link each of its dates to its own source, as author-date style groups them.

    Days of one month share the month and months of one year share the year, so three filings read "Anthropic,
    26 October, 10 and 17 November 2026", with "26 October", "10" and "17 November 2026" each a link.
    """
    entries = sorted(entries, key=lambda entry: entry[0])
    years = []
    for year in dict.fromkeys(day.year for day, _, _ in entries):
        in_year = [entry for entry in entries if entry[0].year == year]
        months = []
        for month in dict.fromkeys(day.month for day, _, _ in in_year):
            in_month = [entry for entry in in_year if entry[0].month == month]
            links = []
            for day, url, ids in in_month:
                text = str(day.day)
                if (day, url, ids) == in_month[-1]:
                    text += f" {day:%B}"
                if (day, url, ids) == in_year[-1]:
                    text += f" {year}"
                links.append(cited.link(text, url, ids))
            months.append(", ".join(links[:-1]) + " and " + links[-1] if len(links) > 1 else links[0])
        years.append(", ".join(months))
    return f"{publisher}, {', '.join(years)}"


def source_links(ids: list[str], sources: dict[str, dict[str, str]], cited: Citations) -> str:
    """Render sources as Markdown links named for readers, to their original URLs (a source without one stays plain).

    Rows that share both name and URL, such as four questions of one survey wave, become one link that cites them
    all. Dated links of one publisher on different days are named once, where the publisher first appears, and
    each date stays its own link (dated_links); `cited` records each link with its ids.
    """
    merged: dict[tuple[str, str], list[str]] = {}
    for source_id in ids:
        row = sources[source_id]
        merged.setdefault((source_name(row), row["url"]), []).append(source_id)
    by_publisher: dict[str, list[tuple[datetime.date, str, list[str]]]] = {}
    for (_, url), group in merged.items():
        day = published_day(sources[group[0]])
        if url and day:
            by_publisher.setdefault(source_name(sources[group[0]], dated=False), []).append((day, url, group))
    grouped = {
        publisher: entries
        for publisher, entries in by_publisher.items()
        if len(entries) > 1 and len({day for day, _, _ in entries}) == len(entries)
    }
    parts = []
    for (name, url), group in merged.items():
        publisher = source_name(sources[group[0]], dated=False)
        if url and published_day(sources[group[0]]) and publisher in grouped:
            if grouped[publisher]:
                parts.append(dated_links(publisher, grouped[publisher], cited))
                grouped[publisher] = []
            continue
        parts.append(cited.link(name, url, group) if url else name)
    return "; ".join(parts)


def cited_words(words: str, ids: list[str], sources: dict[str, dict[str, str]], cited: Citations) -> str:
    """Link the sentence's own words to one source, where the sentence already states the date a citation would repeat.

    The ids must share one URL, as the rows of one survey report do; `cited` records the link with them all.
    """
    urls = {sources[source_id]["url"] for source_id in ids}
    assert len(urls) == 1 and "" not in urls, f"{{{{cite:}}}} needs one URL for {ids}, got {sorted(urls)}"
    return cited.link(words, urls.pop(), ids)


def named_ids(text: str, sources: dict[str, dict[str, str]], cited: Citations) -> str:
    """Replace each parenthesised ledger id in a chart note, such as "(C208)", with its named source link."""
    return re.sub(
        r"\(([A-Z]-?[A-Z0-9-]*[A-Z0-9])\)",
        lambda match: (
            f"({source_links([match.group(1)], sources, cited)})" if match.group(1) in sources else match.group(0)
        ),
        text,
    )


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


def chart_block(spec: dict[str, Any], sources: dict[str, dict[str, str]], cited: Citations) -> str:
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
    note = f" {named_ids(str(spec['notes']), sources, cited)}" if spec["notes"] else ""
    population = named_ids(str(spec["population"]), sources, cited)
    caption = (
        f"*Sources: {source_links(list(spec['sources']), sources, cited)}. Population: {population}. "  # type: ignore[arg-type]
        f"Window: {spec['window']}. Data: [`{spec['csv']}`]({spec['csv']}).{note}*"
    )
    return f"```chart\n{fence}```\n\n{caption}"


def table_block(name: str, sources: dict[str, dict[str, str]], cited: Citations) -> str:
    """Build a RichView table fence for one of the report's tables (inline rows from the chart and ledger data)."""
    if name == "cited-sources":
        named = sorted(
            re.findall(r"\b(?:[CKMES]\d{3}|D-[A-Z]+)\b", TEMPLATE.read_text())
            + re.findall(
                r"\b(?:[CKMES]\d{3}|D-[A-Z]+)\b", json.dumps(json.loads((common.CHART_DIR / "index.json").read_text()))
            )
        )
        unique = sorted(set(named), key=lambda source_id: (source_id[0], source_id))
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
    # RichView links a cell that is one web address; each such cell cites its own row.
    links = [(str(row["link"]), [str(row["id"])]) for row in rows if str(row.get("link", "")).startswith("http")]
    return f"{cited.table(links)}```table\n{fence}```"


def render(
    template: str,
    specs: dict[str, dict[str, Any]],
    facts: dict[str, dict[str, Any]],
    sources: dict[str, dict[str, str]],
) -> tuple[str, list[dict[str, Any]]]:
    """Replace every template token and return the body with its citations in reading order.

    Unknown keys raise, so a typo can never ship a blank number.
    """
    cited = Citations()
    template = re.sub(r"\{\{fact:([a-z0-9_]+)\}\}", lambda match: str(facts[match.group(1)]["display"]), template)
    template = re.sub(
        r"\{\{cite:([A-Z0-9,\- ]+)\|([^}]+)\}\}",
        lambda match: cited_words(match.group(2), [part.strip() for part in match.group(1).split(",")], sources, cited),
        template,
    )
    template = re.sub(
        r"\{\{src:([A-Z0-9,\- ]+)\}\}",
        lambda match: source_links([part.strip() for part in match.group(1).split(",")], sources, cited),
        template,
    )
    template = re.sub(
        r"\{\{chart:([a-z0-9\-]+)\}\}", lambda match: chart_block(specs[match.group(1)], sources, cited), template
    )
    template = re.sub(
        r"\{\{table:([a-z0-9\-]+)\}\}", lambda match: table_block(match.group(1), sources, cited), template
    )
    return cited.resolve(template)


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
    body, citations = render(TEMPLATE.read_text(), specs, facts, sources)
    FRONTMATTER["title_image"].update(title_image(sources))  # type: ignore[union-attr]
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
    (common.REPORT_DIR / "content" / "citations.json").write_text(
        json.dumps(citations, indent="\t", ensure_ascii=False) + "\n"
    )
    print(  # type: ignore[arg-type]
        f"report.rmd, report.json and citations.json written; sections={len(content['sections'])}, "
        f"citations={len(citations)}"
    )


if __name__ == "__main__":
    main()
