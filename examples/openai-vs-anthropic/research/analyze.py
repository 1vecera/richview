"""Turn collected and curated evidence into chart-ready datasets, chart specs and the facts the prose cites.

Outputs (all regenerated on every run):
- data/charts/<chart-id>.csv: one tidy table per chart the report shows, the exact rows the chart plots;
- data/charts/index.json: one spec per chart the report shows (type, assertion title, encoding, axes, annotations,
  source ids);
- data/facts.json: every number quoted in the report text, cover and showcase captions, with its source ids, so prose
  never hand-types data. Every chart builder runs for its facts, including the ones whose chart the report dropped.
Run `uv run analyze.py` for everything or `uv run analyze.py <chart-id> ...` for a subset (facts are merged).
"""

import json
import sys
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import date
from typing import Any

import polars as pl

import common

FACTS_PATH = common.REPORT_DIR / "data" / "facts.json"
INDEX_PATH = common.CHART_DIR / "index.json"
RECENT = ("2025-10", "2026-09")  # latest twelve full months
BASELINE = ("2024-10", "2025-09")  # the twelve months before


@dataclass
class ChartSpec:
    """A chart's portable definition; `rows` are written to CSV and inlined into the .rmd."""

    id: str
    type: str
    title: str
    subtitle: str
    rows: list[dict[str, Any]]
    encoding: dict[str, str]
    axes: dict[str, dict[str, str]]
    sources: list[str]
    population: str
    window: str
    notes: str = ""
    annotations: list[dict[str, Any]] = field(default_factory=list)
    ratio: str = "16:9"

    def manifest(self) -> dict[str, Any]:
        """Return the index.json entry (everything except the rows, plus the CSV path)."""
        return {
            "id": self.id,
            "type": self.type,
            "title": self.title,
            "subtitle": self.subtitle,
            "csv": f"data/charts/{self.id}.csv",
            "encoding": self.encoding,
            "axes": self.axes,
            "annotations": self.annotations,
            "ratio": self.ratio,
            "sources": self.sources,
            "population": self.population,
            "window": self.window,
            "notes": self.notes,
            "row_count": len(self.rows),
        }


@dataclass
class Facts:
    """Collects named facts (value, display string, source ids) cited by the report prose."""

    values: dict[str, dict[str, Any]] = field(default_factory=dict)

    def add(self, key: str, value: float | int | str, display: str, sources: list[str], note: str = "") -> None:
        """Record one fact under a stable key."""
        self.values[key] = {"value": value, "display": display, "sources": sources, "note": note}


def share(anthropic: float, openai: float) -> float:
    """Anthropic's percentage share of a two-company total, rounded to one decimal."""
    return round(100 * anthropic / (anthropic + openai), 1)


def pct(value: float) -> str:
    """Format a percentage for prose with no more precision than the data supports."""
    return f"{value:.0f}%" if value >= 10 else f"{value:.1f}%"


def mean_of(series: pl.Series) -> float:
    """Mean of a numeric or boolean series as a float; an empty series raises instead of yielding None."""
    value = series.cast(pl.Float64).mean()
    if value is None:
        raise ValueError(f"empty series {series.name}")
    return float(value)  # type: ignore[arg-type]


def signed(value: float) -> str:
    """Format a signed whole number with a typographic minus, e.g. +23 or −16."""
    return f"{value:+.0f}".replace("-", "\u2212")


def month_name(month: str) -> str:
    """Turn YYYY-MM into 'July 2025'."""
    return date(int(month[:4]), int(month[5:7]), 1).strftime("%B %Y")


SERIES_RANK = {
    "Anthropic": 0,
    "OpenAI": 1,
    "Google": 2,
    "Claude": 0,
    "ChatGPT": 1,
    "Gemini": 2,
    "To Anthropic": 0,
    "To OpenAI": 1,
}


def order_rows(rows: list[dict[str, Any]], x: str, series: str) -> list[dict[str, Any]]:
    """Sort by x, then put Anthropic's series first so every renderer's default palette maps companies the same way."""
    return sorted(rows, key=lambda row: (str(row[x]), SERIES_RANK.get(str(row[series]), 9)))


def months_between(first: str, last: str) -> list[str]:
    """List YYYY-MM month labels from first to last inclusive."""
    start = date(int(first[:4]), int(first[5:7]), 1)
    end = date(int(last[:4]), int(last[5:7]), 1)
    return [month.isoformat()[:7] for month in common.month_starts(start, end)]


def hn_comment_months() -> pl.DataFrame:
    """Monthly Hacker News comment counts by side from the complete item table."""
    monthly = pl.read_csv(common.DERIVED_DIR / "hn_monthly_mentions.csv")
    return monthly.filter(pl.col("type") == "comment").with_columns(pl.col("month").str.slice(0, 7))


def trends_monthly(comparison: str) -> pl.DataFrame:
    """Monthly means of weekly Google Trends values for one comparison, wide by item."""
    timeline = pl.read_csv(common.DERIVED_DIR / "trends_timeline.csv")
    return (
        timeline.filter(pl.col("comparison") == comparison)
        .with_columns(pl.col("week_start").str.slice(0, 7).alias("month"))
        .group_by("month", "item")
        .agg(pl.col("value").mean())
        .pivot(on="item", index="month", values="value")
        .sort("month")
    )


def package_months(registry: str, anthropic_package: str, openai_package: str) -> pl.DataFrame:
    """Monthly median daily downloads of an Anthropic and an OpenAI package side by side (robust to bursts)."""
    downloads = pl.read_csv(common.DERIVED_DIR / "package_downloads_monthly.csv")
    pair = downloads.filter(
        (pl.col("registry") == registry) & pl.col("package").is_in([anthropic_package, openai_package])
    )
    return (
        pair.with_columns(
            pl.when(pl.col("package") == anthropic_package)
            .then(pl.lit("anthropic"))
            .otherwise(pl.lit("openai"))
            .alias("side")
        )
        .pivot(on="side", index="month", values="median_daily")
        .sort("month")
    )


def wikipedia_months(lang: str, anthropic_entity: str, openai_entity: str) -> pl.DataFrame:
    """Monthly Wikipedia views (redirects included) for an Anthropic and an OpenAI article in one language."""
    views = pl.read_csv(common.DERIVED_DIR / "wikipedia_monthly.csv")
    pair = views.filter((pl.col("lang") == lang) & pl.col("entity").is_in([anthropic_entity, openai_entity]))
    return (
        pair.with_columns(
            pl.when(pl.col("entity") == anthropic_entity)
            .then(pl.lit("anthropic"))
            .otherwise(pl.lit("openai"))
            .alias("side")
        )
        .pivot(on="side", index="month", values="views")
        .sort("month")
    )


def window_mean_share(frame: pl.DataFrame, anthropic: str, openai: str, window: tuple[str, str]) -> float:
    """Anthropic's share of summed values over a month window (sums, not a mean of monthly shares)."""
    inside = frame.filter((pl.col("month") >= window[0]) & (pl.col("month") <= window[1]))
    return share(float(inside[anthropic].sum()), float(inside[openai].sum()))


def chart_share_over_time(facts: Facts) -> ChartSpec:
    """Anthropic's share of attention over time in four populations measured the same way every month."""
    rows = []
    hn = hn_comment_months().filter(pl.col("month") >= "2023-01")
    for row in hn.iter_rows(named=True):
        rows.append(
            {
                "month": row["month"],
                "measure": "Hacker News comments",
                "anthropic_share_pct": share(row["anthropic_side"], row["openai_side"]),
            }
        )
    npm = package_months("npm", "@anthropic-ai/sdk", "openai").filter(pl.col("month") >= "2023-01")
    for row in npm.drop_nulls().iter_rows(named=True):
        rows.append(
            {
                "month": row["month"],
                "measure": "npm SDK downloads",
                "anthropic_share_pct": share(row["anthropic"], row["openai"]),
            }
        )
    trends = trends_monthly("ww-topics-chatgpt-claude").filter(pl.col("month") >= "2023-01")
    for row in trends.iter_rows(named=True):
        if row["Claude"] + row["ChatGPT"] > 0:
            rows.append(
                {
                    "month": row["month"],
                    "measure": "Google searches",
                    "anthropic_share_pct": share(row["Claude"], row["ChatGPT"]),
                }
            )
    wiki = wikipedia_months("en", "claude", "chatgpt").filter(pl.col("month") >= "2023-01").fill_null(0)
    for row in wiki.iter_rows(named=True):
        rows.append(
            {
                "month": row["month"],
                "measure": "Wikipedia article views",
                "anthropic_share_pct": share(row["anthropic"], row["openai"]),
            }
        )
    rows.sort(key=lambda row: (row["month"], row["measure"]))

    for measure, frame, a, o in (
        ("hn", hn, "anthropic_side", "openai_side"),
        ("npm", npm, "anthropic", "openai"),
        ("trends", trends, "Claude", "ChatGPT"),
        ("wiki", wiki, "anthropic", "openai"),
    ):
        recent = window_mean_share(frame, a, o, RECENT)
        baseline = window_mean_share(frame, a, o, BASELINE)
        source = {"hn": "D-HN", "npm": "D-NPM", "trends": "D-GT", "wiki": "D-WIKI"}[measure]
        facts.add(f"{measure}_share_recent", recent, pct(recent), [source], f"{RECENT[0]}..{RECENT[1]} pooled")
        facts.add(
            f"{measure}_share_baseline", baseline, pct(baseline), [source], f"{BASELINE[0]}..{BASELINE[1]} pooled"
        )
    peak = hn.with_columns((pl.col("anthropic_side") / (pl.col("anthropic_side") + pl.col("openai_side"))).alias("s"))
    peak_row = peak.sort("s", descending=True).row(0, named=True)
    facts.add("hn_share_peak", round(100 * peak_row["s"], 1), pct(100 * peak_row["s"]), ["D-HN"], peak_row["month"])
    facts.add("hn_share_peak_month", peak_row["month"], month_name(peak_row["month"]), ["D-HN"])
    last = peak.filter(pl.col("month") == "2026-09").row(0, named=True)
    facts.add("hn_share_2026_09", round(100 * last["s"], 1), pct(100 * last["s"]), ["D-HN"])
    first_cross = peak.filter(pl.col("s") > 0.5).sort("month").row(0, named=True)
    facts.add("hn_first_majority_month", first_cross["month"], month_name(first_cross["month"]), ["D-HN"])

    return ChartSpec(
        id="share-over-time",
        type="line",
        title="Developer talk flipped to Anthropic in mid-2025; the wider public barely moved",
        subtitle="Anthropic's share of OpenAI + Anthropic, by month and population",
        rows=rows,
        encoding={"x": "month", "y": "anthropic_share_pct", "color": "measure"},
        axes={"x": {"label": "Month"}, "y": {"label": "Anthropic share, %"}},
        sources=["D-HN", "D-NPM", "D-GT", "D-WIKI"],
        population="Hacker News commenters; npm installs; Google searchers; English Wikipedia readers",
        window="2023-01 to 2026-09",
        notes=(
            "HN: comments matching openai|chatgpt vs anthropic|claude (whole words). npm: median daily downloads of "
            "@anthropic-ai/sdk vs openai. Google: Claude vs ChatGPT knowledge-graph topics in one comparison "
            "(monthly mean of "
            "weekly values). Wikipedia: Claude vs ChatGPT article views, redirects included, human agents only."
        ),
        annotations=[
            {"kind": "band", "from": {"x": "2024-06"}, "to": {"x": "2024-07"}, "text": "Claude 3.5 Sonnet"},
            {"kind": "band", "from": {"x": "2025-05"}, "to": {"x": "2025-06"}, "text": "Claude 4, Claude Code GA"},
            {"kind": "band", "from": {"x": "2026-02"}, "to": {"x": "2026-03"}, "text": "Super Bowl ad, Pentagon split"},
        ],
    )


def chart_share_by_population(facts: Facts) -> ChartSpec:
    """One question, many populations: Anthropic's share of the pair on the latest comparable measure of each."""
    hn = hn_comment_months()
    npm = package_months("npm", "@anthropic-ai/sdk", "openai")
    pypi = package_months("pypi", "anthropic", "openai")
    trends = trends_monthly("ww-topics-chatgpt-claude")
    company_trends = trends_monthly("ww-topics-openai-anthropic")
    wiki = wikipedia_months("en", "claude", "chatgpt").fill_null(0)
    company_wiki = wikipedia_months("en", "anthropic", "openai").fill_null(0)
    leaders_wiki = wikipedia_months("en", "dario_amodei", "sam_altman").fill_null(0)
    recent = (pl.col("month") >= RECENT[0]) & (pl.col("month") <= RECENT[1])
    ratings = pl.read_csv(common.DERIVED_DIR / "app_store_ratings_by_country.csv").filter(pl.col("country") == "us")
    claude_ratings = int(ratings.filter(pl.col("app") == "Claude")["rating_count"][0])
    chatgpt_ratings = int(ratings.filter(pl.col("app") == "ChatGPT")["rating_count"][0])
    measures = [
        (
            "US iOS ratings",
            "Consumers",
            "cumulative App Store ratings, US, 2026-10-02",
            claude_ratings,
            chatgpt_ratings,
            "D-ITUNES",
        ),
        ("App users", "Consumers", "mobile monthly active users, Q2 2026 (Sensor Tower)", 56, 1000, "C186"),
        ("Web visits", "Consumers", "share of gen-AI web visits, Aug 2026 (Similarweb)", 9.3, 55.5, "C195"),
        ("Weekly use", "US adults", "used in past week, May 2026 (SSRS probability panel)", 9, 43, "S005"),
        (
            "Company searches",
            "Searchers",
            f"Google topic interest, Anthropic vs OpenAI, {RECENT[0]}..{RECENT[1]}",
            float(company_trends.filter(recent)["Anthropic"].sum()),
            float(company_trends.filter(recent)["OpenAI"].sum()),
            "D-GT",
        ),
        (
            "Company pages",
            "Readers",
            f"English Wikipedia views, Anthropic vs OpenAI articles, {RECENT[0]}..{RECENT[1]}",
            float(company_wiki.filter(recent)["anthropic"].sum()),
            float(company_wiki.filter(recent)["openai"].sum()),
            "D-WIKI",
        ),
        (
            "Product searches",
            "Searchers",
            f"Google topic interest, Claude vs ChatGPT, {RECENT[0]}..{RECENT[1]}",
            float(trends.filter(pl.col("month").is_between(pl.lit(RECENT[0]), pl.lit(RECENT[1])))["Claude"].sum()),
            float(trends.filter(pl.col("month").is_between(pl.lit(RECENT[0]), pl.lit(RECENT[1])))["ChatGPT"].sum()),
            "D-GT",
        ),
        (
            "Product pages",
            "Readers",
            f"English Wikipedia views, Claude vs ChatGPT articles, {RECENT[0]}..{RECENT[1]}",
            float(wiki.filter(pl.col("month").is_between(pl.lit(RECENT[0]), pl.lit(RECENT[1])))["anthropic"].sum()),
            float(wiki.filter(pl.col("month").is_between(pl.lit(RECENT[0]), pl.lit(RECENT[1])))["openai"].sum()),
            "D-WIKI",
        ),
        ("Dev usage", "Developers", "used for development, 2025 (Stack Overflow survey)", 42.8, 81.4, "S054"),
        (
            "PyPI SDK",
            "Developers",
            "Python SDK median daily downloads, Sep 2026",
            float(pypi.filter(pl.col("month") == "2026-09")["anthropic"][0]),
            float(pypi.filter(pl.col("month") == "2026-09")["openai"][0]),
            "D-PYPI",
        ),
        (
            "npm SDK",
            "Developers",
            "JavaScript SDK median daily downloads, Sep 2026",
            float(npm.filter(pl.col("month") == "2026-09")["anthropic"][0]),
            float(npm.filter(pl.col("month") == "2026-09")["openai"][0]),
            "D-NPM",
        ),
        ("Firms paying", "Businesses", "US businesses paying each vendor, Aug 2026 (Ramp)", 43.8, 39.8, "C196"),
        (
            "API spend",
            "Enterprises",
            "enterprise LLM API spend, end-2025 (Menlo, an Anthropic investor)",
            40,
            27,
            "C174",
        ),
        ("Q2 revenue", "Investors", "recognised revenue, Q2 2026 (reported documents)", 11.5, 6.7, "C110"),
        (
            "HN comments",
            "Developers",
            f"Hacker News comments, {RECENT[0]}..{RECENT[1]}",
            float(hn.filter(pl.col("month").is_between(pl.lit(RECENT[0]), pl.lit(RECENT[1])))["anthropic_side"].sum()),
            float(hn.filter(pl.col("month").is_between(pl.lit(RECENT[0]), pl.lit(RECENT[1])))["openai_side"].sum()),
            "D-HN",
        ),
        ("Agent at work", "Developers", "coding agent used at work, mid-2026 (JetBrains survey)", 39, 16, "S065"),
    ]
    rows = [
        {
            "measure": label,
            "population": population,
            "definition": definition,
            "anthropic_value": a,
            "openai_value": o,
            "anthropic_share_pct": share(a, o),
            "source_id": source,
        }
        for label, population, definition, a, o, source in measures
    ]
    rows.sort(key=lambda row: row["anthropic_share_pct"])
    common.write_rows(common.CHART_DIR / "population-shares.csv", rows)  # all measures, for the table
    low, high = rows[0], rows[-1]
    by_measure = {row["measure"]: row["anthropic_share_pct"] for row in rows}
    for measure, key, source in (
        ("Company pages", "wiki_company_share_recent", "D-WIKI"),
        ("Company searches", "trends_company_share_recent", "D-GT"),
        ("Product pages", "wiki_product_share_recent", "D-WIKI"),
        ("Product searches", "trends_product_share_recent", "D-GT"),
    ):
        facts.add(key, by_measure[measure], pct(float(by_measure[measure])), [source], f"{RECENT[0]}..{RECENT[1]}")
    leaders = share(
        float(leaders_wiki.filter(recent)["anthropic"].sum()), float(leaders_wiki.filter(recent)["openai"].sum())
    )
    facts.add("wiki_amodei_share_recent", leaders, pct(leaders), ["D-WIKI"], "Dario Amodei vs Sam Altman, English")
    product_ratio = (100 - float(by_measure["Product searches"])) / float(by_measure["Product searches"])
    facts.add(
        "trends_product_ratio",
        round(product_ratio, 1),
        f"{product_ratio:.0f}",
        ["D-GT"],
        "ChatGPT searches per Claude search, latest twelve months",
    )
    # Short labels: the engine thins a category axis as soon as one label is too wide for its slot.
    in_chart = {
        "US iOS ratings": "Ratings",
        "Product searches": "Search",
        "Weekly use": "Weekly",
        "PyPI SDK": "PyPI",
        "npm SDK": "npm",
        "Firms paying": "Firms",
        "HN comments": "HN",
        "Agent at work": "Agents",
    }
    chart_rows = [
        {
            "measure": in_chart[str(row["measure"])],
            "anthropic_share_pct": row["anthropic_share_pct"],
            "population": row["population"],
            "source_id": row["source_id"],
        }
        for row in rows
        if row["measure"] in in_chart
    ]
    facts.add(
        "population_share_low",
        low["anthropic_share_pct"],
        pct(low["anthropic_share_pct"]),
        [str(low["source_id"])],
        str(low["measure"]),
    )
    facts.add(
        "population_share_high",
        high["anthropic_share_pct"],
        pct(high["anthropic_share_pct"]),
        [str(high["source_id"])],
        str(high["measure"]),
    )
    facts.add(
        "ios_ratings_ratio",
        round(chatgpt_ratings / claude_ratings, 0),
        f"{chatgpt_ratings / claude_ratings:.0f}×",
        ["D-ITUNES"],
        f"ChatGPT {chatgpt_ratings:,} vs Claude {claude_ratings:,} US ratings",
    )
    return ChartSpec(
        id="share-by-population",
        type="bar",
        title=(
            f"Anthropic's share of the race runs from {pct(low['anthropic_share_pct'])} to "
            f"{pct(high['anthropic_share_pct'])}, depending on who is asked"
        ),
        subtitle="Anthropic's share of OpenAI + Anthropic on the latest comparable measure for eight populations, %",
        rows=chart_rows,
        encoding={"x": "measure", "y": "anthropic_share_pct"},
        axes={"x": {"label": ""}, "y": {"label": "Anthropic share, %"}},
        sources=sorted({str(row["source_id"]) for row in chart_rows}),
        population=(
            "Eight of the sixteen measures in population-shares.csv, from US iPhone owners to professional developers"
        ),
        window="Latest available period per measure (2025 to 2026-10-02)",
        notes="Share = Anthropic / (Anthropic + OpenAI). Measures use different units; compare shares, not levels.",
        ratio="16:9",
    )


def chart_company_vs_product(facts: Facts) -> ChartSpec:
    """Attention to the companies versus their products, among general searchers and readers."""
    shares = {row["measure"]: row for row in pl.read_csv(common.CHART_DIR / "population-shares.csv").to_dicts()}
    leaders = float(facts.values["wiki_amodei_share_recent"]["value"])  # type: ignore[arg-type]
    rows = [
        {
            "medium": "Google searches",
            "level": "Products",
            "anthropic_share_pct": shares["Product searches"]["anthropic_share_pct"],
        },
        {
            "medium": "Google searches",
            "level": "Companies",
            "anthropic_share_pct": shares["Company searches"]["anthropic_share_pct"],
        },
        {
            "medium": "Wikipedia views",
            "level": "Products",
            "anthropic_share_pct": shares["Product pages"]["anthropic_share_pct"],
        },
        {
            "medium": "Wikipedia views",
            "level": "Companies",
            "anthropic_share_pct": shares["Company pages"]["anthropic_share_pct"],
        },
        {"medium": "Wikipedia views", "level": "Chief executives", "anthropic_share_pct": leaders},
    ]
    return ChartSpec(
        id="company-vs-product",
        type="bar",
        title="The public reads about Anthropic but uses ChatGPT",
        subtitle=(
            "Anthropic's share of attention to each pair (Claude vs ChatGPT; Anthropic vs OpenAI; Amodei vs Altman), %"
        ),
        rows=rows,
        encoding={"x": "medium", "y": "anthropic_share_pct", "color": "level"},
        axes={"x": {"label": ""}, "y": {"label": "Anthropic share, %"}},
        sources=["D-GT", "D-WIKI"],
        population="Google searchers worldwide; English Wikipedia readers",
        window="2025-10 to 2026-09",
        notes=(
            "Searches: knowledge-graph topics compared in pairs. Wikipedia: English articles, redirects included. "
            "Oct 2025 to Sep 2026."
        ),
        annotations=[{"kind": "line", "at": {"y": 50}, "text": ""}],
    )


def chart_ios_ratings(facts: Facts) -> ChartSpec:
    """ChatGPT's and Claude's cumulative US App Store ratings, the widest consumer gap, as a live chart."""
    ratings = {row["measure"]: row for row in pl.read_csv(common.CHART_DIR / "population-shares.csv").to_dicts()}[
        "US iOS ratings"
    ]
    ratio = facts.values["ios_ratings_ratio"]["display"]
    return ChartSpec(
        id="ios-ratings",
        type="bar",
        title=f"ChatGPT has {ratio} as many US App Store ratings as Claude",
        subtitle="Cumulative US App Store ratings at 2 October 2026",
        # Claude first, so each bar takes its company's series colour, as in every other chart here.
        rows=[
            {"app": "Claude", "ratings": int(ratings["anthropic_value"])},
            {"app": "ChatGPT", "ratings": int(ratings["openai_value"])},
        ],
        encoding={"x": "app", "y": "ratings", "color": "app"},
        axes={"x": {"label": ""}, "y": {"label": "Ratings"}},
        sources=[ratings["source_id"]],
        population="US App Store users who rated either app",
        window="cumulative to 2026-10-02",
        ratio="2:1",
    )


def quarter_of(month: str) -> str:
    """Name the calendar quarter of a YYYY-MM month, e.g. 2026 Q3."""
    return f"{month[:4]} Q{(int(month[5:7]) - 1) // 3 + 1}"


def chart_coding_agents(facts: Facts) -> ChartSpec:
    """Claude Code's share of the Claude Code + Codex pair across four public signals."""
    rows = []
    hn = hn_comment_months().filter(pl.col("month") >= "2025-04")
    for row in hn.iter_rows(named=True):
        rows.append(
            {
                "month": row["month"],
                "measure": "Hacker News comments",
                "claude_code_share_pct": share(row["claude_code"], row["codex"]),
            }
        )
    npm = package_months("npm", "@anthropic-ai/claude-code", "@openai/codex").filter(pl.col("month") >= "2025-05")
    for row in npm.drop_nulls().iter_rows(named=True):
        if row["anthropic"] + row["openai"] > 0:
            rows.append(
                {
                    "month": row["month"],
                    "measure": "npm CLI downloads",
                    "claude_code_share_pct": share(row["anthropic"], row["openai"]),
                }
            )
    trends = trends_monthly("ww-terms-coding-agents")
    for row in trends.iter_rows(named=True):
        rows.append(
            {
                "month": row["month"],
                "measure": "Google searches",
                "claude_code_share_pct": share(row["Claude Code"], row["Codex"]),
            }
        )
    github = (
        pl.read_csv(common.DERIVED_DIR / "github_agent_artifacts_monthly.csv")
        .pivot(on="metric", index="month", values="count")
        .filter(pl.col("month") >= "2025-05")
        .sort("month")
    )
    for row in github.iter_rows(named=True):
        # Like for like: pull requests opened by each vendor's cloud agent from its own branch prefix.
        rows.append(
            {
                "month": row["month"],
                "measure": "GitHub cloud-agent PRs",
                "claude_code_share_pct": share(row["claude_branch_prs"], row["codex_prs"]),
            }
        )
    rows.sort(key=lambda row: (row["month"], row["measure"]))
    for measure in ("Hacker News comments", "npm CLI downloads", "GitHub cloud-agent PRs"):
        series = [row for row in rows if row["measure"] == measure]
        key = measure.split()[0].lower()
        latest = series[-1]
        facts.add(
            f"coding_{key}_share_latest",
            latest["claude_code_share_pct"],
            pct(float(latest["claude_code_share_pct"])),
            ["D-HN" if key == "hacker" else "D-NPM" if key == "npm" else "D-GH"],
            latest["month"],
        )
        top = max(series, key=lambda row: float(row["claude_code_share_pct"]))
        facts.add(
            f"coding_{key}_share_peak",
            top["claude_code_share_pct"],
            pct(float(top["claude_code_share_pct"])),
            ["D-HN" if key == "hacker" else "D-NPM" if key == "npm" else "D-GH"],
            str(top["month"]),
        )
    latest = {
        measure: [row for row in rows if row["measure"] == measure][-1]["claude_code_share_pct"]
        for measure in ("Hacker News comments", "Google searches", "npm CLI downloads", "GitHub cloud-agent PRs")
    }
    codex_ahead = [
        label
        for measure, label in (
            ("Hacker News comments", "talk"),
            ("Google searches", "searches"),
            ("npm CLI downloads", "installs"),
        )
        if float(latest[measure]) < 50
    ]
    claude_ahead_github = float(latest["GitHub cloud-agent PRs"]) > 50
    title = (
        f"Codex leads in {', '.join(codex_ahead[:-1])} and {codex_ahead[-1]}"
        if len(codex_ahead) > 1
        else f"Codex leads in {codex_ahead[0]}"
    )
    if claude_ahead_github:
        title += "; Claude Code still files more PRs"
    return ChartSpec(
        id="coding-agents",
        type="line",
        title=title,
        subtitle="Claude Code's share of Claude Code + OpenAI Codex, by month and signal",
        rows=rows,
        encoding={"x": "month", "y": "claude_code_share_pct", "color": "measure"},
        axes={"x": {"label": "Month"}, "y": {"label": "Claude Code share, %"}},
        sources=["D-HN", "D-NPM", "D-GT", "D-GH"],
        population="Hacker News commenters; npm installs; Google searchers; public GitHub repositories",
        window="2025-04 to 2026-09",
        notes=(
            "npm (median daily downloads) understates Claude Code after Anthropic moved users to a native installer. "
            "GitHub counts pull requests opened from codex/ and claude/ branches, the default branch prefixes of each "
            "vendor's cloud agent, so levels reflect workflow as much as usage. Compare each line's direction."
        ),
        annotations=[{"kind": "line", "at": {"y": 50}, "text": ""}],
    )


def chart_run_rate(facts: Facts) -> ChartSpec:
    """Latest reported annualised revenue run-rate per quarter for each company."""
    reports = pl.read_csv(common.REPORT_DIR / "research" / "curated" / "run_rate_reports.csv")
    latest = (
        reports.with_columns(pl.col("as_of").map_elements(quarter_of, return_dtype=pl.String).alias("quarter"))
        .sort("as_of")
        .group_by("quarter", "company")
        .agg(pl.all().last())
        .sort("quarter", "company")
    )
    rows = order_rows(
        [
            {
                "quarter": row["quarter"],
                "company": row["company"],
                "run_rate_usd_bn": row["value_usd_bn"],
                "reported_month": row["as_of"],
                "qualifier": row["qualifier"] or "",
                "source_id": row["source_id"],
            }
            for row in latest.iter_rows(named=True)
        ],
        "quarter",
        "company",
    )
    anthropic_latest = reports.filter(pl.col("company") == "Anthropic").sort("as_of").row(-1, named=True)
    openai_latest = reports.filter(pl.col("company") == "OpenAI").sort("as_of").row(-1, named=True)
    facts.add(
        "run_rate_anthropic_latest",
        anthropic_latest["value_usd_bn"],
        f"${anthropic_latest['value_usd_bn']:.0f}bn",
        [anthropic_latest["source_id"]],
        f"{anthropic_latest['qualifier']} {anthropic_latest['as_of']}",
    )
    facts.add(
        "run_rate_openai_latest",
        openai_latest["value_usd_bn"],
        f"${openai_latest['value_usd_bn']:.0f}bn",
        [openai_latest["source_id"]],
        f"{openai_latest['qualifier']} {openai_latest['as_of']}",
    )
    first_ahead = reports.filter(pl.col("company") == "Anthropic").join(
        reports.filter(pl.col("company") == "OpenAI").select("as_of", pl.col("value_usd_bn").alias("openai")),
        on="as_of",
        how="left",
    )
    facts.add("run_rate_anthropic_2024_12", 1, "$1bn", ["C083"], "about, Dec 2024")
    facts.add("run_rate_openai_2024_12", 5.5, "$5.5bn", ["C001"], "about, Dec 2024")
    del first_ahead
    return ChartSpec(
        id="run-rate",
        type="bar",
        title="By September, Anthropic's reported run-rate exceeded OpenAI's",
        subtitle="Latest annualised revenue run-rate reported in each quarter, $bn (unaudited; mostly unnamed sources)",
        rows=rows,
        encoding={"x": "quarter", "y": "run_rate_usd_bn", "color": "company"},
        axes={"x": {"label": "Quarter"}, "y": {"label": "Run-rate, $bn"}},
        sources=sorted({str(row["source_id"]) for row in rows}),
        population="Company statements and press reports (see source ledger for each bar)",
        window="2024 Q4 to 2026 Q3",
        notes=(
            "Run-rate annualises a recent period and is neither audited nor defined identically by both companies; "
            "OpenAI says Anthropic counts gross cloud-marketplace revenue (C161). "
            "Missing bars = no report that quarter."
        ),
    )


def chart_quarterly_revenue(facts: Facts) -> ChartSpec:
    """Recognised quarterly revenue in 2026 from documents reported by Bloomberg and the WSJ."""
    revenue = pl.read_csv(common.REPORT_DIR / "research" / "curated" / "quarterly_revenue.csv")
    rows = order_rows(revenue.to_dicts(), "quarter", "company")
    q2 = {row["company"]: row["revenue_usd_bn"] for row in rows if row["quarter"] == "Q2 2026"}
    facts.add("q2_2026_revenue_anthropic", q2["Anthropic"], f"${q2['Anthropic']:.1f}bn", ["C110"], "preliminary")
    facts.add("q2_2026_revenue_openai", q2["OpenAI"], f"${q2['OpenAI']:.1f}bn", ["C015"])
    return ChartSpec(
        id="quarterly-revenue",
        type="bar",
        title="In recognised revenue, Anthropic overtook OpenAI in Q2 2026",
        subtitle="Quarterly revenue, $bn, from documents reported by Bloomberg (Anthropic) and the WSJ (OpenAI)",
        rows=rows,
        encoding={"x": "quarter", "y": "revenue_usd_bn", "color": "company"},
        axes={"x": {"label": "Quarter"}, "y": {"label": "Revenue, $bn"}},
        sources=["C015", "C110"],
        population=(
            "Company documents reported by the press; as of the 2 October snapshot neither company had a public "
            "S-1. Anthropic filed one on 26 October 2026 (C208) and amended it on 10 November (C209)"
        ),
        window="2026 Q1 to 2026 Q2",
        notes=(
            "Anthropic's Q2 figure is preliminary. For FY2025 OpenAI booked $13.07bn (C009) "
            "vs Anthropic ~$4.6bn (C093)."
        ),
    )


def chart_ramp(facts: Facts) -> ChartSpec:
    """Share of US businesses on Ramp paying each vendor, monthly, with the May 2026 methodology break."""
    ramp = pl.read_csv(common.REPORT_DIR / "research" / "curated" / "ramp_adoption.csv")
    rows = order_rows(
        ramp.select("month", "company", "share_pct", "methodology", "source_id").to_dicts(), "month", "company"
    )
    crossover = (
        ramp.pivot(on="company", index="month", values="share_pct")
        .drop_nulls()
        .filter(pl.col("Anthropic") > pl.col("OpenAI"))
        .sort("month")
        .row(0, named=True)
    )
    facts.add("ramp_crossover_month", crossover["month"], "April 2026", ["C185"])
    facts.add("ramp_anthropic_2025_07", 11.1, "11.1%", ["C168"])
    facts.add("ramp_anthropic_latest", 43.8, "43.8%", ["C196"], "Aug 2026")
    facts.add("ramp_openai_latest", 39.8, "39.8%", ["C196"], "Aug 2026")
    return ChartSpec(
        id="ramp-adoption",
        type="line",
        title="US businesses now pay Anthropic more often than OpenAI",
        subtitle="Share of US businesses on Ramp paying each vendor, % (card and bill-pay spend)",
        rows=rows,
        encoding={"x": "month", "y": "share_pct", "color": "company"},
        axes={"x": {"label": "Month"}, "y": {"label": "Businesses paying, %"}},
        sources=sorted(ramp["source_id"].unique().to_list()),
        population="US businesses using Ramp's corporate cards and bill pay (skews to tech-forward firms)",
        window="2025-07 to 2026-08",
        notes="Ramp added bill-pay capture in May 2026 (C187): levels before and after the break are not comparable.",
        annotations=[{"kind": "band", "from": {"x": "2026-04"}, "to": {"x": "2026-05"}, "text": "method change"}],
    )


def chart_web_share(facts: Facts) -> ChartSpec:
    """Generative-AI web traffic shares: ChatGPT lost share mostly to Gemini, Claude grew from a small base."""
    web = pl.read_csv(common.REPORT_DIR / "research" / "curated" / "web_share.csv")
    rows = order_rows(web.to_dicts(), "as_of", "product")
    facts.add("web_chatgpt_2025_08", 73.3, "73.3%", ["C170"])
    facts.add("web_chatgpt_2026_08", 55.5, "55.5%", ["C195"])
    facts.add("web_claude_2025_08", 1.9, "1.9%", ["C170"])
    facts.add("web_claude_2026_08", 9.3, "9.3%", ["C195"])
    facts.add("web_gemini_2026_08", 25.6, "25.6%", ["C195"])
    return ChartSpec(
        id="web-share",
        type="bar",
        title="ChatGPT's lost web share went mostly to Gemini, not Claude",
        subtitle="Share of worldwide generative-AI website visits, % (Similarweb panel estimates)",
        rows=rows,
        encoding={"x": "as_of", "y": "share_pct", "color": "product"},
        axes={"x": {"label": "Month"}, "y": {"label": "Share of gen-AI web visits, %"}},
        sources=["C170", "C191", "C195"],
        population="Worldwide desktop and mobile web visits to gen-AI sites (excludes apps and API use)",
        window="2025-08 to 2026-08",
        notes="Similarweb revises earlier points between releases; values are as first published in each release.",
    )


def chart_capability(facts: Facts) -> ChartSpec:
    """Best Epoch Capabilities Index score each lab had released by the end of every month."""
    frontier = pl.read_csv(common.DERIVED_DIR / "capability_eci_frontier.csv")
    rows = []
    for month in months_between("2024-01", "2026-09"):
        month_end = common.next_month(date(int(month[:4]), int(month[5:7]), 1)).isoformat()
        for lab in ("OpenAI", "Anthropic", "Google"):
            released = frontier.filter((pl.col("lab") == lab) & (pl.col("date") < month_end))
            if released.height:
                best = released.sort("eci").row(-1, named=True)
                rows.append(
                    {
                        "month": month,
                        "lab": lab,
                        "best_eci": best["eci"],
                        "model": best["model"],
                        "ci_low": best["ci_low"],
                        "ci_high": best["ci_high"],
                    }
                )
    latest = {row["lab"]: row for row in rows if row["month"] == "2026-09"}
    facts.add(
        "eci_anthropic_latest",
        latest["Anthropic"]["best_eci"],
        f"{latest['Anthropic']['best_eci']:.1f}",
        ["D-EPOCH"],
        str(latest["Anthropic"]["model"]),
    )
    facts.add(
        "eci_openai_latest",
        latest["OpenAI"]["best_eci"],
        f"{latest['OpenAI']['best_eci']:.1f}",
        ["D-EPOCH"],
        str(latest["OpenAI"]["model"]),
    )
    facts.add(
        "eci_google_latest",
        latest["Google"]["best_eci"],
        f"{latest['Google']['best_eci']:.1f}",
        ["D-EPOCH"],
        str(latest["Google"]["model"]),
    )
    rows = order_rows(rows, "month", "lab")
    return ChartSpec(
        id="capability-frontier",
        type="line",
        title="On Epoch's composite index, OpenAI and Anthropic are in a dead heat",
        subtitle="Best Epoch Capabilities Index score among models each lab had released by month end",
        rows=rows,
        encoding={"x": "month", "y": "best_eci", "color": "lab"},
        axes={"x": {"label": "Month"}, "y": {"label": "Epoch Capabilities Index"}},
        sources=["D-EPOCH"],
        population="Frontier models with enough benchmark coverage for Epoch's index",
        window="2024-01 to 2026-09",
        notes="Epoch's confidence intervals overlap: Claude Opus 5.5 164-172, GPT-6 Astra 163-171.",
    )


def chart_capability_leads(facts: Facts) -> ChartSpec:
    """Days each lab held the best released score on nine benchmarks over the latest twelve months."""
    leads = pl.read_csv(common.DERIVED_DIR / "capability_lead_days.csv").filter(pl.col("lab") != "Other")
    rows = [
        {
            "benchmark": row["benchmark"],
            "lab": row["lab"],
            "lead_share_pct": round(100 * row["lead_days"] / row["window_days"], 1),
            "best_model_by_2026_09_30": row["best_model"] or "",
        }
        for row in leads.iter_rows(named=True)
    ]
    for lab in ("OpenAI", "Anthropic", "Google"):
        led = sorted(
            {str(row["benchmark"]) for row in rows if row["lab"] == lab and float(row["lead_share_pct"]) >= 50}
        )
        facts.add(
            f"benchmarks_led_{lab.lower()}",
            len(led),
            ", ".join(led) or "none",
            ["D-EPOCH"],
            "benchmarks led on at least half the days, 2025-10..2026-09",
        )
    return ChartSpec(
        id="capability-leads",
        type="heatmap",
        title="Anthropic leads coding and long tasks; OpenAI leads maths and reasoning",
        subtitle="Share of days from Oct 2025 to Sep 2026 on which each lab held the best released score, %",
        rows=rows,
        encoding={"x": "lab", "y": "benchmark", "color": "lead_share_pct"},
        axes={"x": {"label": "Lab"}, "y": {"label": "Benchmark"}},
        sources=["D-EPOCH"],
        population="Models with published scores in Epoch AI's Benchmarking Hub",
        window="2025-10-01 to 2026-09-30",
        notes=(
            "Leads inside measurement error still count. METR horizons stop at April 2026; "
            "OSWorld lacks recent OpenAI runs."
        ),
        ratio="4:3",
    )


def chart_surveys(facts: Facts) -> ChartSpec:
    """Probability and large professional surveys: the consumer gap and the developer gap side by side."""
    surveys = pl.read_csv(common.REPORT_DIR / "research" / "curated" / "surveys_core.csv").filter(
        pl.col("source_id") != "S065"
    )  # Codex vs Claude Code is a different product pair; shown in share-by-population
    labels = {
        "S002": "Ever used",
        "S004": "Week, Feb",
        "S005": "Week, May",
        "S006": "Aware, Feb",
        "S007": "Aware, May",
        "S015": "6 nations",
        "S054": "Dev use",
    }
    rows: list[dict[str, Any]] = []
    for row in surveys.iter_rows(named=True):
        for product, column in (("Claude", "claude_pct"), ("ChatGPT", "chatgpt_pct")):
            rows.append(
                {
                    "measure": labels[row["source_id"]],
                    "product": product,
                    "pct": row[column],
                    "population": row["population"],
                    "fieldwork": row["fieldwork"],
                    "method": row["method"],
                    "source_id": row["source_id"],
                }
            )
    facts.add("pew_ever_chatgpt", 44, "44%", ["S002"])
    facts.add("pew_ever_claude", 6, "6%", ["S002"])
    facts.add("ssrs_week_claude_feb", 4, "4%", ["S004"])
    facts.add("ssrs_week_claude_may", 9, "9%", ["S005"])
    facts.add("ssrs_week_chatgpt_may", 43, "43%", ["S005"])
    facts.add("ssrs_aware_claude_feb", 21, "21%", ["S006"])
    facts.add("ssrs_aware_claude_may", 42, "42%", ["S007"])
    ratios = {row["source_id"]: row["chatgpt_pct"] / row["claude_pct"] for row in surveys.iter_rows(named=True)}
    public = [ratios[source] for source in ("S002", "S004", "S005", "S015")]
    facts.add("survey_reach_ratio_min", round(min(public), 1), f"{min(public):.0f}", ["S002", "S004", "S005", "S015"])
    facts.add("survey_reach_ratio_max", round(max(public), 1), f"{max(public):.0f}", ["S002", "S004", "S005", "S015"])
    facts.add("survey_dev_ratio", round(ratios["S054"], 1), f"{ratios['S054']:.1f}", ["S054"])
    return ChartSpec(
        id="surveys",
        type="bar",
        title=(
            f"ChatGPT reaches {min(public):.0f} to {max(public):.0f} times as many people as Claude; "
            f"developers, {ratios['S054']:.0f}x"
        ),
        subtitle="Share of each surveyed population using or aware of each assistant, %",
        rows=rows,
        encoding={"x": "measure", "y": "pct", "color": "product"},
        axes={"x": {"label": ""}, "y": {"label": "% of respondents"}},
        sources=sorted(surveys["source_id"].to_list()),
        population=(
            "US adults (Pew, SSRS), online adults in six countries (Reuters Institute), developers (Stack Overflow)"
        ),
        window="2025-06 to 2026-07 fieldwork",
        notes="Developer surveys are opt-in professional samples, not probability samples.",
    )


def chart_trends_geo(facts: Facts) -> ChartSpec:
    """Claude's share of Claude + ChatGPT search interest by country over the latest twelve months."""
    geo = pl.read_csv(common.DERIVED_DIR / "trends_geo.csv").filter(
        (pl.col("comparison") == "ww-geo-recent-chatgpt-claude") & pl.col("has_data")
    )
    wide = geo.pivot(on="item", index=["country", "country_name"], values="share")
    markets = {
        "CN": "China",
        "KR": "S. Korea",
        "IL": "Israel",
        "TW": "Taiwan",
        "US": "US",
        "JP": "Japan",
        "GB": "UK",
        "IN": "India",
        "BR": "Brazil",
    }
    rows = [
        {"country": markets[row["country"]], "claude_share_pct": float(row["Claude"]), "code": row["country"]}
        for row in wide.filter(pl.col("country").is_in(list(markets)))
        .sort("Claude", descending=True)
        .iter_rows(named=True)
    ]
    facts.add(
        "trends_geo_top",
        rows[0]["claude_share_pct"],
        f"{rows[0]['country']} {pct(rows[0]['claude_share_pct'])}",
        ["D-GT"],
    )
    facts.add(
        "trends_geo_bottom",
        rows[-1]["claude_share_pct"],
        f"{rows[-1]['country']} {pct(rows[-1]['claude_share_pct'])}",
        ["D-GT"],
    )
    return ChartSpec(
        id="trends-geo",
        type="bar",
        title="Claude's searchers concentrate in East Asian and other tech-heavy markets",
        subtitle="Claude's share of Claude + ChatGPT Google search interest, Oct 2025 to Sep 2026, nine markets, %",
        rows=rows,
        encoding={"x": "country", "y": "claude_share_pct"},
        axes={"x": {"label": ""}, "y": {"label": "Claude share of searches, %"}},
        sources=["D-GT"],
        population="Google searchers per country (China reflects VPN and Google-accessible users only)",
        window="2025-10-01 to 2026-09-30",
        notes="Google's compared-breakdown shares split each country's combined interest between the two topics.",
    )


def chart_pentagon_week(facts: Facts) -> ChartSpec:
    """Weekly US search interest around the Super Bowl ad and the Pentagon split."""
    timeline = pl.read_csv(common.DERIVED_DIR / "trends_timeline.csv").filter(
        (pl.col("comparison") == "us-topics-chatgpt-claude") & (pl.col("week_start") >= "2025-11-01")
    )
    rows = order_rows(
        [
            {"week": row["week_start"], "topic": row["item"], "interest": row["value"]}
            for row in timeline.iter_rows(named=True)
        ],
        "week",
        "topic",
    )
    claude = [row for row in rows if row["topic"] == "Claude"]
    before = [row["interest"] for row in claude if "2026-01-01" <= row["week"] < "2026-02-01"]
    peak = max(claude, key=lambda row: row["interest"])
    after = [row["interest"] for row in claude if "2026-08-01" <= row["week"] < "2026-09-30"]
    january = sum(before) / len(before)
    late = sum(after) / len(after)
    held = 100 * (late - january) / (peak["interest"] - january)
    facts.add("us_trends_claude_jan", round(january, 1), f"{january:.0f}", ["D-GT"])
    facts.add("us_trends_claude_peak", peak["interest"], f"{peak['interest']}", ["D-GT"], peak["week"])
    facts.add("us_trends_claude_aug_sep", round(late, 1), f"{late:.0f}", ["D-GT"])
    facts.add(
        "us_trends_claude_gain_held",
        round(held, 0),
        f"{held:.0f}%",
        ["D-GT"],
        "share of Jan-to-peak gain still present Aug-Sep",
    )
    multiple = peak["interest"] / january
    return ChartSpec(
        id="pentagon-week",
        type="line",
        title=f"Claude's US search interest rose {multiple:.1f}x around the Pentagon split; {held:.0f}% held",
        subtitle="Weekly US Google search interest in the Claude and ChatGPT topics (0-100, same comparison)",
        rows=rows,
        encoding={"x": "week", "y": "interest", "color": "topic"},
        axes={"x": {"label": "Week starting"}, "y": {"label": "Search interest (0-100)"}},
        sources=["D-GT", "E078", "E088"],
        population="US Google searchers",
        window="2025-11 to 2026-09",
        notes=(
            "Super Bowl 'no ads' campaign 2026-02-08 (E078); Pentagon supply-chain-risk designation 2026-02-27 (E088)."
        ),
        annotations=[
            {
                "kind": "band",
                "from": {"x": "2026-02-08"},
                "to": {"x": "2026-03-01"},
                "text": "Super Bowl ad to Pentagon split",
            }
        ],
    )


def chart_reddit(facts: Facts) -> ChartSpec:
    """Monthly posts in the product communities on Reddit (Arctic Shift archive); months it missed stay empty."""
    posts = pl.read_csv(common.DERIVED_DIR / "reddit_monthly_posts.csv", infer_schema_length=0).filter(
        pl.col("subreddit").is_in(["ChatGPT", "ClaudeAI", "ClaudeCode", "codex"]) & (pl.col("month") >= "2024-07")
    )
    rows = [
        {"month": row["month"], "community": f"r/{row['subreddit']}", "posts": int(row["posts"])}
        for row in posts.filter(pl.col("posts") != "").sort("month", "subreddit").iter_rows(named=True)
    ]
    by = {(row["community"], row["month"]): row["posts"] for row in rows}
    claude_peak = max((row for row in rows if row["community"] == "r/ClaudeAI"), key=lambda row: row["posts"])
    claude_latest = by[("r/ClaudeAI", "2026-09")]
    drop = 100 * (1 - claude_latest / claude_peak["posts"])
    facts.add(
        "reddit_claudeai_peak", claude_peak["posts"], f"{claude_peak['posts']:,}", ["D-REDDIT"], claude_peak["month"]
    )
    facts.add("reddit_claudeai_latest", claude_latest, f"{claude_latest:,}", ["D-REDDIT"], "2026-09")
    facts.add("reddit_claudeai_drop", round(drop), f"{drop:.0f}%", ["D-REDDIT"], "fall from peak to Sep 2026")
    facts.add("reddit_codex_latest", by[("r/codex", "2026-09")], f"{by[('r/codex', '2026-09')]:,}", ["D-REDDIT"])
    facts.add(
        "reddit_claudecode_latest",
        by[("r/ClaudeCode", "2026-09")],
        f"{by[('r/ClaudeCode', '2026-09')]:,}",
        ["D-REDDIT"],
    )
    facts.add("reddit_codex_2026_01", by[("r/codex", "2026-01")], f"{by[('r/codex', '2026-01')]:,}", ["D-REDDIT"])
    chatgpt = [row for row in rows if row["community"] == "r/ChatGPT"]
    chatgpt_peak = max(chatgpt, key=lambda row: row["posts"])
    chatgpt_last = chatgpt[-1]
    facts.add(
        "reddit_chatgpt_peak", chatgpt_peak["posts"], f"{chatgpt_peak['posts']:,}", ["D-REDDIT"], chatgpt_peak["month"]
    )
    facts.add(
        "reddit_chatgpt_last", chatgpt_last["posts"], f"{chatgpt_last['posts']:,}", ["D-REDDIT"], chatgpt_last["month"]
    )
    return ChartSpec(
        id="reddit-communities",
        type="line",
        title=(f"r/codex now out-posts r/ClaudeCode; r/ClaudeAI is down {drop:.0f}% from its peak"),
        subtitle="Posts per month in each product's home community on Reddit",
        rows=rows,
        encoding={"x": "month", "y": "posts", "color": "community"},
        axes={"x": {"label": "Month"}, "y": {"label": "Posts per month"}},
        sources=["D-REDDIT"],
        population="Reddit users posting in r/ChatGPT, r/ClaudeAI, r/ClaudeCode and r/codex",
        window="2024-07 to 2026-09",
        notes="Gaps are months the archive could not count (timeouts on the largest subreddit), not zero.",
    )


STANCE_SCORE = {"positive": 1, "negative": -1, "mixed": 0, "neutral": 0}
TOPIC_LABELS = {
    "model_quality": "Quality",
    "coding_tools": "Coding",
    "pricing_limits": "Pricing",
    "reliability_outages": "Reliability",
    "safety_ethics": "Safety",
    "business_finance": "Business",
    "legal_copyright": "Legal",
    "government_military": "Government",
    "privacy_data": "Privacy",
    "leadership_governance": "Leaders",
    "ads_monetization": "Ads",
    "open_source": "Open source",
    "hype_skepticism": "Hype",
    "product_ux": "Product",
    "other": "Other",
}
REVIEW_LABELS = {
    "usage_limits": "Limits",
    "pricing_subscription": "Pricing",
    "ads": "Ads",
    "refusals_censorship": "Refusals",
    "accuracy_hallucination": "Accuracy",
    "bugs_crashes": "Bugs",
    "login_account": "Login",
    "speed": "Speed",
    "memory_context": "Memory",
    "voice": "Voice",
    "images_files": "Images, files",
    "personality_tone": "Tone",
    "privacy": "Privacy",
    "helpfulness_quality": "Quality",
    "other": "Other",
}


def stance_labels(judge: str) -> pl.DataFrame:
    """HN stance labels for one judge with a calendar-quarter column."""
    labels = pl.read_csv(common.DERIVED_DIR / "hn_stance_labels.csv", infer_schema_length=0).filter(
        pl.col("judge") == judge
    )
    return labels.with_columns(pl.col("month").map_elements(quarter_of, return_dtype=pl.String).alias("quarter"))


def net_stance(labels: pl.DataFrame, company: str, by: str) -> pl.DataFrame:
    """Net stance (positive share minus negative share, in points) toward one company, with a 95% interval."""
    mentioned = labels.filter(pl.col(company) != "absent")
    return (
        mentioned.group_by(by)
        .agg(
            pl.len().alias("n"),
            (pl.col(company) == "positive").mean().alias("positive"),
            (pl.col(company) == "negative").mean().alias("negative"),
        )
        .with_columns(
            ((pl.col("positive") - pl.col("negative")) * 100).alias("net"),
            (
                1.96
                * 100
                * (
                    (pl.col("positive") + pl.col("negative") - (pl.col("positive") - pl.col("negative")) ** 2)
                    / pl.col("n")
                ).sqrt()
            ).alias("ci95"),
        )
        .sort(by)
    )


def chart_hn_stance(facts: Facts) -> ChartSpec:
    """Quarterly net stance of Hacker News comments toward each company, primary judge, with a replication."""
    rows = []
    for judge in (classify_primary(), "devstral-2"):
        labels = stance_labels(judge)
        for company, name in (("openai", "OpenAI"), ("anthropic", "Anthropic")):
            for row in net_stance(labels, company, "quarter").iter_rows(named=True):
                rows.append(
                    {
                        "quarter": row["quarter"],
                        "company": name,
                        "judge": judge,
                        "net_stance": round(row["net"], 1),
                        "ci95": round(row["ci95"], 1),
                        "n": row["n"],
                    }
                )
    primary = [row for row in rows if row["judge"] == classify_primary()]
    replica = [row for row in rows if row["judge"] == "devstral-2"]
    for company in ("OpenAI", "Anthropic"):
        series = [row for row in primary if row["company"] == company]
        latest = series[-1]
        peak = max(series, key=lambda row: float(row["net_stance"]))
        key = company.lower()
        facts.add(
            f"hn_net_{key}_latest",
            latest["net_stance"],
            signed(float(latest["net_stance"])),
            ["D-HN", "D-LABELS"],
            f"{latest['quarter']}, ±{latest['ci95']} (95%), n={latest['n']}",
        )
        facts.add(
            f"hn_net_{key}_peak",
            peak["net_stance"],
            signed(float(peak["net_stance"])),
            ["D-HN", "D-LABELS"],
            str(peak["quarter"]),
        )
        facts.add(f"hn_net_{key}_peak_quarter", peak["quarter"], str(peak["quarter"]), ["D-HN", "D-LABELS"])
        first_half_2025 = [float(row["net_stance"]) for row in series if row["quarter"] in ("2025 Q1", "2025 Q2")]
        facts.add(
            f"hn_net_{key}_2025h1",
            round(sum(first_half_2025) / 2, 1),
            signed(sum(first_half_2025) / 2),
            ["D-HN", "D-LABELS"],
        )
        early = [float(row["net_stance"]) for row in series if row["quarter"] <= "2025 Q4"]
        facts.add(
            f"hn_net_{key}_min_2023_2025", min(early), signed(min(early)), ["D-HN", "D-LABELS"], "2023 Q1..2025 Q4"
        )
        facts.add(
            f"hn_net_{key}_max_2023_2025", max(early), signed(max(early)), ["D-HN", "D-LABELS"], "2023 Q1..2025 Q4"
        )
        replica_series = [row for row in replica if row["company"] == company]
        replica_2025 = [float(row["net_stance"]) for row in replica_series if row["quarter"] in ("2025 Q1", "2025 Q2")]
        facts.add(
            f"hn_net_{key}_2025h1_replica",
            round(sum(replica_2025) / 2, 1),
            signed(sum(replica_2025) / 2),
            ["D-HN", "D-LABELS"],
            "Devstral 2 replication",
        )
        replica_latest = replica_series[-1]
        facts.add(
            f"hn_net_{key}_latest_replica",
            replica_latest["net_stance"],
            signed(float(replica_latest["net_stance"])),
            ["D-HN", "D-LABELS"],
            f"Devstral 2 replication, n={replica_latest['n']}",
        )
    comparisons = stance_labels(classify_primary()).filter(
        (pl.col("side") == "both") & pl.col("prefers").is_in(["openai", "anthropic"])
    )
    for key, window in (("2025", ("2025-01", "2025-12")), ("latest", ("2026-07", "2026-09"))):
        picked = comparisons.filter(pl.col("month").is_between(pl.lit(window[0]), pl.lit(window[1])))
        prefer = 100 * mean_of(picked["prefers"] == "anthropic")
        facts.add(
            f"pref_anthropic_{key}",
            round(prefer, 1),
            f"{pct(prefer)} (n={picked.height})",
            ["D-HN", "D-LABELS"],
            f"{window[0]}..{window[1]}, comparison comments that pick a side",
        )
    return ChartSpec(
        id="hn-stance",
        type="line",
        title="Developers' warmth toward Anthropic is gone: net stance fell below zero",
        subtitle="Net stance of Hacker News comments mentioning each company (% positive minus % negative), by quarter",
        rows=order_rows(
            [{key: value for key, value in row.items() if key != "judge"} for row in primary], "quarter", "company"
        ),
        encoding={"x": "quarter", "y": "net_stance", "color": "company"},
        axes={"x": {"label": "Quarter"}, "y": {"label": "Net stance, points"}},
        sources=["D-HN", "D-LABELS"],
        population="Hacker News commenters; 60 comments per month and side, hash-sampled from all matching comments",
        window="2023 Q1 to 2026 Q3",
        notes=(
            "Labels from Qwen3-235B (neither lab's model); Devstral 2 relabelled every comment and reproduces "
            "the trend "
            "(see method). ci95 column gives each point's 95% interval."
        ),
        annotations=[{"kind": "line", "at": {"y": 0}, "text": ""}],
    )


def classify_primary() -> str:
    """Name of the primary judge (kept here so analysis does not import the Bedrock client)."""
    return "qwen3-235b"


def chart_hn_topics(facts: Facts) -> ChartSpec:
    """What each company is criticised for: topic shares among negative HN comments in the latest twelve months."""
    labels = stance_labels(classify_primary()).filter(pl.col("month").is_between(pl.lit(RECENT[0]), pl.lit(RECENT[1])))
    rows = []
    for company, name in (("openai", "OpenAI"), ("anthropic", "Anthropic")):
        negative = labels.filter(pl.col(company) == "negative")
        topics = negative.with_columns(pl.col("topics").str.split("|")).explode("topics")
        counts = topics.group_by("topics").len()
        for row in counts.iter_rows(named=True):
            if row["topics"] and row["topics"] != "other":
                rows.append(
                    {
                        "topic": TOPIC_LABELS[row["topics"]],
                        "company": name,
                        "share_of_negative_pct": round(100 * row["len"] / negative.height, 1),
                        "negative_comments": negative.height,
                    }
                )
        facts.add(f"hn_negative_n_{company}", negative.height, f"{negative.height:,}", ["D-HN", "D-LABELS"])
    order = sorted(
        {str(row["topic"]) for row in rows},
        key=lambda topic: (-max(float(row["share_of_negative_pct"]) for row in rows if row["topic"] == topic), topic),
    )
    by = {(row["topic"], row["company"]): row["share_of_negative_pct"] for row in rows}
    for topic in ("reliability_outages", "pricing_limits", "business_finance", "hype_skepticism", "safety_ethics"):
        for company in ("OpenAI", "Anthropic"):
            value = float(by.get((TOPIC_LABELS[topic], company), 0.0))
            facts.add(
                f"hn_neg_{topic}_{company.lower()}",
                value,
                pct(value),
                ["D-HN", "D-LABELS"],
                f"{RECENT[0]}..{RECENT[1]}",
            )
    rows = [row for row in rows if order.index(str(row["topic"])) < 8]
    rows.sort(key=lambda row: (order.index(str(row["topic"])), SERIES_RANK[str(row["company"])]))
    positive = labels.filter(pl.col("anthropic") == "positive")
    coding = positive.filter(pl.col("topics").str.contains("coding_tools")).height
    facts.add(
        "hn_pos_coding_anthropic",
        round(100 * coding / positive.height, 1),
        pct(100 * coding / positive.height),
        ["D-HN", "D-LABELS"],
        "share of positive Anthropic comments about coding tools",
    )
    return ChartSpec(
        id="hn-topics",
        type="bar",
        title="Anthropic draws complaints about reliability; OpenAI about money and hype",
        subtitle=(
            "Share of negative Hacker News comments about each company that concern each topic, Oct 2025 to Sep 2026, %"
        ),
        rows=rows,
        encoding={"x": "topic", "y": "share_of_negative_pct", "color": "company"},
        axes={"x": {"label": ""}, "y": {"label": "% of negative comments"}},
        sources=["D-HN", "D-LABELS"],
        population="Hacker News commenters (negative comments in the labelled sample)",
        window="2025-10 to 2026-09",
        notes="A comment can carry up to two topics, so shares do not sum to 100.",
    )


def chart_hn_switching(facts: Facts) -> ChartSpec:
    """First-person switching statements on Hacker News, by quarter and direction."""
    labels = pl.read_csv(common.DERIVED_DIR / "hn_switch_labels.csv", infer_schema_length=0).filter(
        (pl.col("judge") == classify_primary()) & (pl.col("first_person").str.to_lowercase() == "true")
    )
    labels = labels.with_columns(pl.col("month").map_elements(quarter_of, return_dtype=pl.String).alias("quarter"))
    names = {"to_anthropic": "To Anthropic", "to_openai": "To OpenAI"}
    counts = labels.filter(pl.col("direction").is_in(list(names))).group_by("quarter", "direction").len()
    rows = [
        {
            "quarter": quarter,
            "direction": names[direction],
            "statements": int(
                counts.filter((pl.col("quarter") == quarter) & (pl.col("direction") == direction))["len"].sum()
            ),
        }
        for quarter in sorted(counts["quarter"].unique().to_list())
        if quarter >= "2024 Q1"
        for direction in names
    ]
    rows = order_rows(rows, "quarter", "direction")
    by = {(row["quarter"], row["direction"]): row["statements"] for row in rows}
    for quarter in ("2026 Q1", "2026 Q2", "2026 Q3"):
        key = quarter.replace(" ", "_").lower()
        facts.add(
            f"switch_to_anthropic_{key}",
            by[(quarter, "To Anthropic")],
            str(by[(quarter, "To Anthropic")]),
            ["D-HN", "D-LABELS"],
        )
        facts.add(
            f"switch_to_openai_{key}", by[(quarter, "To OpenAI")], str(by[(quarter, "To OpenAI")]), ["D-HN", "D-LABELS"]
        )
    return ChartSpec(
        id="hn-switching",
        type="bar",
        title="Developers now report switching to OpenAI far more often than to Anthropic",
        subtitle="First-person Hacker News comments reporting a move between the two, by quarter (count)",
        rows=rows,
        encoding={"x": "quarter", "y": "statements", "color": "direction"},
        axes={"x": {"label": "Quarter"}, "y": {"label": "Statements"}},
        sources=["D-HN", "D-LABELS"],
        population="Hacker News commenters who describe their own or their team's switch",
        window="2024 Q1 to 2026 Q3",
        notes=(
            "Candidates: every comment matching a switch or cancel phrase near a brand name (complete HN table); "
            "direction and first-person status labelled by Qwen3-235B and checked by Devstral 2."
        ),
    )


def chart_app_reviews(facts: Facts) -> ChartSpec:
    """Complaint topics in one- and two-star app reviews over the window every sample covers (September 2026)."""
    labels = pl.read_csv(common.DERIVED_DIR / "app_review_labels.csv").filter(pl.col("judge") == classify_primary())
    names = {"chatgpt": "ChatGPT", "claude": "Claude"}
    rows = []
    for app, name in names.items():
        reviews = labels.filter(pl.col("app") == app)
        low = reviews.filter(pl.col("rating") <= 2)
        facts.add(
            f"reviews_low_share_{app}",
            round(100 * low.height / reviews.height, 1),
            pct(100 * low.height / reviews.height),
            ["D-REVIEWS", "D-LABELS"],
            f"1-2 stars, n={reviews.height}, Sep 2026, US Google Play + App Store",
        )
        facts.add(
            f"reviews_mean_{app}",
            round(mean_of(reviews["rating"]), 2),
            f"{mean_of(reviews['rating']):.1f}",
            ["D-REVIEWS"],
            f"n={reviews.height}",
        )
        topics = low.with_columns(pl.col("topics").str.split("|")).explode("topics").group_by("topics").len()
        for row in topics.iter_rows(named=True):
            if row["topics"] not in ("other", None, ""):
                rows.append(
                    {
                        "topic": REVIEW_LABELS[row["topics"]],
                        "app": name,
                        "share_of_low_star_pct": round(100 * row["len"] / low.height, 1),
                        "low_star_reviews": low.height,
                    }
                )
    order = sorted(
        {str(row["topic"]) for row in rows},
        key=lambda topic: (-max(float(row["share_of_low_star_pct"]) for row in rows if row["topic"] == topic), topic),
    )
    by = {(row["topic"], row["app"]): row["share_of_low_star_pct"] for row in rows}
    for topic in ("refusals_censorship", "usage_limits", "login_account", "bugs_crashes"):
        for name in names.values():
            value = float(by.get((REVIEW_LABELS[topic], name), 0.0))
            facts.add(f"reviews_{topic}_{name.lower()}", value, pct(value), ["D-REVIEWS", "D-LABELS"])
    rows = [row for row in rows if order.index(str(row["topic"])) < 7]
    rows.sort(key=lambda row: (order.index(str(row["topic"])), SERIES_RANK[str(row["app"])]))
    return ChartSpec(
        id="app-reviews",
        type="bar",
        title="Unhappy Claude users cite refusals, limits and logins; ChatGPT users, bugs",
        subtitle=(
            "Share of one- and two-star reviews mentioning each topic, US Google Play and App Store, September 2026, %"
        ),
        rows=rows,
        encoding={"x": "topic", "y": "share_of_low_star_pct", "color": "app"},
        axes={"x": {"label": ""}, "y": {"label": "% of low-star reviews"}},
        sources=["D-REVIEWS", "D-LABELS"],
        population="US app reviewers (English); reviewers over-represent strong opinions",
        window="2026-09-01 to 2026-10-01",
        notes="Up to 1,500 reviews per store and app, hash-sampled; a review can carry up to three topics.",
    )


def landing_series(facts: Facts) -> None:
    """Write the monthly Hacker News comment series the RichView landing page uses as its data background."""
    months = hn_comment_months().filter(pl.col("month") >= "2023-01")
    rows = [
        {
            "month": row["month"],
            "openai_comments": row["openai_side"],
            "anthropic_comments": row["anthropic_side"],
            "both_comments": row["both_sides"],
            "all_comments": row["items"],
            "anthropic_share": round(row["anthropic_side"] / (row["anthropic_side"] + row["openai_side"]), 4),
        }
        for row in months.iter_rows(named=True)
    ]
    common.write_rows(common.CHART_DIR / "hn-comment-mentions-monthly.csv", rows)
    facts.add("landing_series_months", len(rows), str(len(rows)), ["D-HN"], "rows in hn-comment-mentions-monthly.csv")


def arena_facts(facts: Facts) -> None:
    """When Anthropic's 2026 run at #1 on LMArena's default text board began and when it ended."""
    board = pl.read_csv(common.DERIVED_DIR / "lmarena_lab_best.csv").sort("snapshot")
    leaders = board.filter(pl.col("gap_to_top") == 0).group_by("snapshot").agg(pl.col("lab").first()).sort("snapshot")
    in_2026 = leaders.filter(pl.col("snapshot") >= "2026-01-01").to_dicts()
    first = next(index for index, row in enumerate(in_2026) if row["lab"] == "Anthropic")
    start = str(in_2026[first]["snapshot"])
    end = next(row for row in in_2026[first:] if row["lab"] != "Anthropic")
    facts.add("arena_anthropic_first_top", start, date.fromisoformat(start).strftime("%-d %B %Y"), ["D-ARENA"])
    facts.add(
        "arena_google_retake",
        end["snapshot"],
        date.fromisoformat(end["snapshot"]).strftime("%-d %B %Y"),
        ["D-ARENA"],
        f"new leader: {end['lab']}",
    )


def validation_facts(facts: Facts) -> None:
    """Agreement between the four judges and the self-preference test on the shared validation subset."""
    labels = pl.read_csv(common.DERIVED_DIR / "hn_stance_labels.csv", infer_schema_length=0)
    validation_ids = pl.read_csv(common.DERIVED_DIR / "hn_validation_ids.csv", infer_schema_length=0)["id"]
    shared = labels.filter(pl.col("id").is_in(validation_ids.to_list()))
    judges = ["qwen3-235b", "devstral-2", "claude-haiku-4-5", "gpt-oss-120b"]
    complete = shared.group_by("id").agg(pl.col("judge").n_unique().alias("judges")).filter(pl.col("judges") == 4)["id"]
    shared = shared.filter(pl.col("id").is_in(complete.to_list()))
    facts.add("validation_n", complete.len(), f"{complete.len():,}", ["D-LABELS"])
    agreements = []
    wide = {judge: shared.filter(pl.col("judge") == judge).sort("id") for judge in judges}
    for index, first in enumerate(judges):
        for second in judges[index + 1 :]:
            for field_name in ("openai", "anthropic"):
                agreements.append(mean_of(wide[first][field_name] == wide[second][field_name]))
    facts.add("validation_agreement_min", round(100 * min(agreements)), f"{100 * min(agreements):.0f}%", ["D-LABELS"])
    facts.add("validation_agreement_max", round(100 * max(agreements)), f"{100 * max(agreements):.0f}%", ["D-LABELS"])
    mean_score = {}
    for judge in judges:
        for company in ("openai", "anthropic"):
            scores = (
                wide[judge]
                .filter(pl.col(company) != "absent")[company]
                .replace_strict(STANCE_SCORE, return_dtype=pl.Int8)
            )
            mean_score[(judge, company)] = mean_of(scores)
    neutral = {
        company: (mean_score[("qwen3-235b", company)] + mean_score[("devstral-2", company)]) / 2
        for company in ("openai", "anthropic")
    }
    for judge, own, rival in (("claude-haiku-4-5", "anthropic", "openai"), ("gpt-oss-120b", "openai", "anthropic")):
        own_gap = mean_score[(judge, own)] - neutral[own]
        rival_gap = mean_score[(judge, rival)] - neutral[rival]
        key = judge.split("-")[0]
        facts.add(
            f"bias_{key}_own_gap", round(own_gap, 3), f"{own_gap:+.2f}", ["D-LABELS"], f"{judge} vs neutral on {own}"
        )
        facts.add(
            f"bias_{key}_rival_gap",
            round(rival_gap, 3),
            f"{rival_gap:+.2f}",
            ["D-LABELS"],
            f"{judge} vs neutral on {rival}",
        )
        facts.add(
            f"bias_{key}_self_preference",
            round(own_gap - rival_gap, 3),
            f"{own_gap - rival_gap:+.2f}",
            ["D-LABELS"],
            "own-company gap minus rival gap, stance scale -1..+1",
        )


CHARTS: dict[str, Callable[[Facts], ChartSpec]] = {
    "share-over-time": chart_share_over_time,
    "share-by-population": chart_share_by_population,
    "company-vs-product": chart_company_vs_product,
    "pentagon-week": chart_pentagon_week,
    "ios-ratings": chart_ios_ratings,
    "surveys": chart_surveys,
    "trends-geo": chart_trends_geo,
    "coding-agents": chart_coding_agents,
    "run-rate": chart_run_rate,
    "quarterly-revenue": chart_quarterly_revenue,
    "ramp-adoption": chart_ramp,
    "web-share": chart_web_share,
    "capability-frontier": chart_capability,
    "capability-leads": chart_capability_leads,
    "hn-stance": chart_hn_stance,
    "hn-topics": chart_hn_topics,
    "hn-switching": chart_hn_switching,
    "app-reviews": chart_app_reviews,
    "reddit-communities": chart_reddit,
}
# The charts the shortened report shows (15 November 2026). The other builders still run, because the facts they
# compute are quoted in the prose, the cover and the showcase captions; their charts are no longer published.
REPORT_CHARTS = frozenset(
    {
        "share-by-population",
        "share-over-time",
        "reddit-communities",
        "coding-agents",
        "hn-topics",
        "pentagon-week",
        "ramp-adoption",
        "quarterly-revenue",
        "capability-frontier",
    }
)
FACT_BUILDERS: dict[str, Callable[[Facts], None]] = {
    "validation": validation_facts,
    "arena": arena_facts,
    "landing": landing_series,
}


def main(selected: list[str]) -> None:
    """Build the selected charts (all by default), write their CSVs, and merge specs and facts into the outputs."""
    facts = Facts(json.loads(FACTS_PATH.read_text()) if FACTS_PATH.exists() and selected else {})
    index = (
        {entry["id"]: entry for entry in json.loads(INDEX_PATH.read_text())} if INDEX_PATH.exists() and selected else {}
    )
    for chart_id in [chart_id for chart_id in selected or list(CHARTS) if chart_id in CHARTS]:
        spec = CHARTS[chart_id](facts)
        if spec.id not in REPORT_CHARTS:
            print(f"{spec.id}: facts only")
            continue
        common.write_rows(common.CHART_DIR / f"{spec.id}.csv", spec.rows)
        index[spec.id] = spec.manifest()
        print(f"{spec.id}: {len(spec.rows)} rows")
    for name, builder in FACT_BUILDERS.items():
        if not selected or name in selected:
            builder(facts)
    ordered = [index[chart_id] for chart_id in CHARTS if chart_id in index and chart_id in REPORT_CHARTS]
    INDEX_PATH.write_text(json.dumps(ordered, indent="\t", ensure_ascii=False) + "\n")
    FACTS_PATH.write_text(json.dumps(dict(sorted(facts.values.items())), indent="\t", ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main(sys.argv[1:])
