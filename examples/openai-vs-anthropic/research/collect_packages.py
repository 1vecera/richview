"""Collect developer-adoption signals from package registries: npm (official API) and PyPI (ClickHouse public copy).

Downloads count installs, including CI and mirrors, not developers. Bursts happen (for example @openai/codex logged
13-46 million downloads a day from 2026-04-30 to 2026-05-06 against a baseline near 1 million), so the monthly table
carries both the sum and the median daily count; the report compares medians. The "openai" client is also the
de facto client for
OpenAI-compatible endpoints (vLLM, OpenRouter, DeepSeek, local servers), so it overstates OpenAI usage; the
provider-specific adapters (@ai-sdk/*, langchain-*) are the cleaner like-for-like comparison.
"""

from datetime import date
from typing import Any
from urllib.parse import quote

import polars as pl

import common

NPM_PACKAGES = {
    "openai": "OpenAI",
    "@anthropic-ai/sdk": "Anthropic",
    "@openai/codex": "OpenAI",
    "@anthropic-ai/claude-code": "Anthropic",
    "@openai/agents": "OpenAI",
    "@anthropic-ai/claude-agent-sdk": "Anthropic",
    "@ai-sdk/openai": "OpenAI",
    "@ai-sdk/anthropic": "Anthropic",
    "@ai-sdk/google": "Google",
    "@google/genai": "Google",
    "@google/gemini-cli": "Google",
    "@modelcontextprotocol/sdk": "Anthropic-originated standard",
}
PYPI_PROJECTS = {
    "openai": "OpenAI",
    "anthropic": "Anthropic",
    "openai-agents": "OpenAI",
    "claude-agent-sdk": "Anthropic",
    "claude-code-sdk": "Anthropic",
    "langchain-openai": "OpenAI",
    "langchain-anthropic": "Anthropic",
    "langchain-google-genai": "Google",
    "google-genai": "Google",
    "mcp": "Anthropic-originated standard",
    "litellm": "Neutral router",
}


def npm_daily(fetcher: common.CachedFetcher, package: str) -> list[dict[str, Any]]:
    """Fetch daily npm downloads for one package across the study window in yearly chunks (API max is 18 months)."""
    rows = []
    for year in range(common.WINDOW_START.year, common.WINDOW_END.year + 1):
        start = max(date(year, 1, 1), common.WINDOW_START)
        end = min(date(year, 12, 31), common.WINDOW_END)
        body = fetcher.get_json(f"https://api.npmjs.org/downloads/range/{start}:{end}/{quote(package, safe='@')}")
        rows.extend({"date": day["day"], "downloads": day["downloads"]} for day in body["downloads"])
    return rows


def main() -> None:
    """Write monthly npm and PyPI download totals per package with its company tag."""
    npm_fetcher = common.CachedFetcher("npm", min_interval_s=0.5)
    daily_rows = []
    for package in NPM_PACKAGES:
        daily_rows.extend(
            {"registry": "npm", "package": package, "date": str(day["date"]), "downloads": int(day["downloads"])}
            for day in npm_daily(npm_fetcher, package)
        )

    clickhouse = common.CachedFetcher("clickhouse", timeout_s=300.0)
    pypi_rows = []
    for project in PYPI_PROJECTS:  # one query per project keeps each result under the playground's row limit
        pypi_rows.extend(
            common.clickhouse_rows(
                clickhouse,
                f"""
            SELECT project, toString(date) AS day, sum(count) AS downloads
            FROM pypi.pypi_downloads_per_day
            WHERE project = '{project}' AND date >= '{common.WINDOW_START}' AND date <= '{common.WINDOW_END}'
            GROUP BY project, day ORDER BY day
            """,
            )
        )
    daily_rows.extend(
        {
            "registry": "pypi",
            "package": str(row["project"]),
            "date": str(row["day"]),
            "downloads": int(row["downloads"]),
        }
        for row in pypi_rows
    )
    daily = pl.DataFrame(daily_rows).filter(pl.col("downloads") > 0)
    monthly_table = (
        daily.with_columns(pl.col("date").str.slice(0, 7).alias("month"))
        .group_by("registry", "package", "month")
        .agg(
            pl.col("downloads").sum().alias("downloads"),
            pl.col("downloads").median().alias("median_daily"),
            pl.len().alias("days"),
        )
        .sort("registry", "package", "month")
    )

    companies = {("npm", name): company for name, company in NPM_PACKAGES.items()} | {
        ("pypi", name): company for name, company in PYPI_PROJECTS.items()
    }
    rows = [{**row, "company": companies[(row["registry"], row["package"])]} for row in monthly_table.to_dicts()]
    common.write_rows(common.DERIVED_DIR / "package_downloads_monthly.csv", rows)
    common.upsert_collection_run(
        {
            "run_id": "package-downloads",
            "source": (
                "npm downloads API; PyPI downloads via ClickHouse public playground (pypi.pypi_downloads_per_day)"
            ),
            "collector": "research/collect_packages.py",
            "finished_at": common.utc_now(),
            "query": f"npm={','.join(NPM_PACKAGES)}; pypi={','.join(PYPI_PROJECTS)}",
            "window_start": common.WINDOW_START.isoformat(),
            "window_end": common.WINDOW_END.isoformat(),
            "items": len(rows),
            "requests": npm_fetcher.requests + clickhouse.requests,
            "cache_hits": npm_fetcher.cache_hits + clickhouse.cache_hits,
            "cost_usd": 0,
            "notes": (
                "Monthly sums and medians of daily downloads (zero-download days dropped as gaps). "
                "Includes CI and mirrors."
            ),
        }
    )
    print(f"rows={len(rows)}")


if __name__ == "__main__":
    main()
