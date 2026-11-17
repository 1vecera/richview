"""Assemble the dated source ledger (ledger/sources.csv) from the discovery ledgers and the collected datasets.

Every id cited in the report resolves here: C (commercial), K (capability), M (model releases), E (events) and
S (surveys) rows come from research/discovery/*.csv, each with its URL, publisher, date and a verbatim quote; D rows
describe the datasets this study collected itself.
"""

import csv

import common

ACCESSED = "2026-10-02"
FIELDS = ["id", "kind", "title", "publisher", "published_date", "accessed_date", "url", "quote", "notes"]
DATASETS = [
    (
        "D-HN",
        "Hacker News: every story and comment, 2022-01 to 2026-09",
        "Y Combinator via ClickHouse public playground",
        "https://sql-clickhouse.clickhouse.com/",
        "Table hackernews.hackernews (live copy of the official HN API). Story "
        "points are frozen at ingestion from mid-December 2025, so only item counts are used.",
    ),
    (
        "D-WIKI",
        "Wikipedia pageviews, human agents, redirects summed, 12 languages",
        "Wikimedia Foundation",
        "https://wikimedia.org/api/rest_v1/",
        "per-article monthly and daily endpoints; Wikidata sitelinks map titles.",
    ),
    (
        "D-GT",
        "Google Trends interest over time and by country (knowledge-graph topics and terms)",
        "Google",
        "https://trends.google.com/trends/explore",
        "Relative 0-100 interest within each comparison; sampled by Google.",
    ),
    (
        "D-NPM",
        "npm package downloads, daily",
        "npm, Inc.",
        "https://api.npmjs.org/downloads/",
        "Counts installs including CI; monthly medians of daily downloads used for shares.",
    ),
    (
        "D-PYPI",
        "PyPI package downloads, daily",
        "Python Software Foundation via ClickHouse public playground",
        "https://clickpy.clickhouse.com/",
        "Table pypi.pypi_downloads_per_day.",
    ),
    (
        "D-GH",
        "GitHub search totals for agent-signed pull requests and commits, monthly",
        "GitHub",
        "https://docs.github.com/en/rest/search/search",
        "Query set extends PRarena (github.com/aavetis/PRarena).",
    ),
    (
        "D-REDDIT",
        "Reddit monthly post counts by subreddit and title term",
        "Arctic Shift archive",
        "https://arctic-shift.photon-reddit.com/",
        "Months bucketed in CET/CEST by the archive; timeouts left empty.",
    ),
    (
        "D-ITUNES",
        "App Store ratings count and average by country (snapshot)",
        "Apple iTunes Lookup API",
        "https://itunes.apple.com/lookup",
        "Snapshot taken 2026-10-02 for 50 storefronts.",
    ),
    (
        "D-REVIEWS",
        "Newest app reviews, US: Google Play (8,000 per app) and App Store (5,000 per app)",
        "Apify actors thewolves/google-play-reviews-scraper and thewolves/appstore-reviews-scraper",
        "https://apify.com/thewolves",
        "Run ids in ledger/collection-runs.csv; review text kept out of the repository.",
    ),
    (
        "D-EPOCH",
        "Epoch AI Benchmarking Hub and Epoch Capabilities Index",
        "Epoch AI",
        "https://epoch.ai/benchmarks",
        "CC BY 4.0; archive contents to 2026-09-29.",
    ),
    (
        "D-ARENA",
        "LMArena leaderboard history (text, text with style control)",
        "LMArena",
        "https://huggingface.co/datasets/lmarena-ai/leaderboard-dataset",
        "CC-BY-4.0; snapshots to 2026-09-30.",
    ),
    (
        "D-LABELS",
        "Model labels for the Hacker News sample and app reviews (this study)",
        "This study (Amazon Bedrock)",
        "",
        "Primary judge Qwen3-235B; validation by Devstral 2, Claude Haiku 4.5 and gpt-oss-120b. "
        "Prompts in research/classify.py.",
    ),
]
DISCOVERY = {
    "commercial": ("press_or_primary", "metric"),
    "capability": ("benchmark", "benchmark"),
    "releases": ("release", "model"),
    "events": ("event", "event"),
    "surveys": ("survey", "metric"),
}


def main() -> None:
    """Write ledger/sources.csv with dataset rows first, then every discovery row in id order, then later sources."""
    rows = [
        {
            "id": dataset_id,
            "kind": "dataset",
            "title": title,
            "publisher": publisher,
            "published_date": "",
            "accessed_date": ACCESSED,
            "url": url,
            "quote": "",
            "notes": notes,
        }
        for dataset_id, title, publisher, url, notes in DATASETS
    ]
    for name, (kind, title_field) in DISCOVERY.items():
        with (common.RESEARCH_DIR / "discovery" / f"{name}.csv").open(newline="") as handle:
            for record in csv.DictReader(handle):
                title = record[title_field]
                if name == "surveys":
                    title = f"{record['survey']}: {record['metric']}"
                if name == "capability":
                    title = f"{record['benchmark']}: {record['model']} ({record['lab']})"
                if name == "releases":
                    title = f"{record['model']} ({record['lab']}) released"
                rows.append(
                    {
                        "id": record["id"],
                        "kind": kind,
                        "title": title,
                        "publisher": record.get("publisher", ""),
                        "published_date": record.get("published_date", record.get("date", "")),
                        "accessed_date": ACCESSED,
                        "url": record["url"],
                        "quote": record["quote"],
                        "notes": record.get("notes", ""),
                    }
                )
    discovered = len(rows) - len(DATASETS)
    # Sources read after the 2 October snapshot keep their own access date and stay out of the discovery run.
    with (common.RESEARCH_DIR / "discovery" / "after_snapshot.csv").open(newline="") as handle:
        rows.extend({field: record[field] for field in FIELDS} for record in csv.DictReader(handle))
    common.LEDGER_DIR.mkdir(parents=True, exist_ok=True)
    with (common.LEDGER_DIR / "sources.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=FIELDS, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
    common.upsert_collection_run(
        {
            "run_id": "exa-discovery",
            "source": "Exa search and fetch (Brave as fallback) by four research agents",
            "collector": "research/discovery/*.csv (agent-built ledgers, kept as evidence)",
            "finished_at": "2026-10-02T13:45:00+00:00",
            "query": "commercial position; capability and releases; perception events; surveys and polls",
            "window_start": "2023-01-01",
            "window_end": "2026-10-02",
            "items": discovered,
            "requests": "",
            "cache_hits": "",
            "cost_usd": "",
            "notes": "Every row carries URL, publisher, date and a verbatim quote of at most 300 characters.",
        }
    )
    common.upsert_collection_run(
        {
            "run_id": "apify-aborted-runs",
            "source": "Apify apify/google-trends-scraper run 53oEG8dfepPB0d9tJ; review runs gqUl15nxdSrsWLN3O, "
            "SdzTH3QAJraonfd7q",
            "collector": "manual (aborted)",
            "finished_at": "2026-10-02T13:02:30+00:00",
            "query": (
                "Trends actor could not load Google Trends pages; first review runs combined two apps under one cap"
            ),
            "window_start": "",
            "window_end": "",
            "items": 0,
            "requests": "",
            "cache_hits": "",
            "cost_usd": 0,
            "notes": "Replaced by collect_trends.py (direct JSON API) and one review run per app.",
        }
    )
    print(f"sources={len(rows)}")


if __name__ == "__main__":
    main()
