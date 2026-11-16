"""Snapshot consumer reach and satisfaction in the Apple App Store across countries, and gather recent review samples.

Rating counts are cumulative and depend on how often each app prompts users to rate, so their ratio is a rough proxy
for relative installed base, not a user count. ChatGPT launched on iOS in May 2023, Claude in May 2024. Review samples
(newest-first, via Apify) reflect the most recent days or weeks only and over-represent people with strong views.
"""

import json
import os
from typing import Any

import httpx

import common

IOS_APPS = {
    "6448311069": ("ChatGPT", "OpenAI"),
    "6473753684": ("Claude", "Anthropic"),
    "6477489729": ("Google Gemini", "Google"),
    "6670324846": ("Grok", "xAI"),
    "1668000334": ("Perplexity", "Perplexity"),
}
COUNTRIES = [
    "us",
    "gb",
    "ca",
    "au",
    "nz",
    "ie",
    "de",
    "fr",
    "es",
    "it",
    "nl",
    "be",
    "at",
    "ch",
    "se",
    "no",
    "dk",
    "fi",
    "pl",
    "cz",
    "pt",
    "gr",
    "ro",
    "hu",
    "ua",
    "tr",
    "il",
    "ae",
    "sa",
    "eg",
    "za",
    "ng",
    "ke",
    "in",
    "pk",
    "id",
    "ph",
    "th",
    "vn",
    "my",
    "sg",
    "jp",
    "kr",
    "tw",
    "hk",
    "br",
    "mx",
    "ar",
    "cl",
    "co",
]
APIFY_REVIEW_RUNS = {
    "google_play_chatgpt": "oe7SQ6cAuyybR2CT8",
    "google_play_claude": "znkx53nP9jg0kgazb",
    "app_store_chatgpt": "J1LY8vWVz2tyrQpae",
    "app_store_claude": "6o7oQPS0OyaDUEETi",
}


def download_apify_dataset(client: httpx.Client, token: str, run_id: str) -> list[dict[str, Any]]:
    """Download every item of an Apify run's default dataset (cached on disk by run id)."""
    path = common.CACHE_DIR / "apify" / f"{run_id}.json"
    if path.exists():
        return json.loads(path.read_text())
    run = client.get(f"https://api.apify.com/v2/actor-runs/{run_id}", headers={"Authorization": f"Bearer {token}"})
    dataset_id = run.json()["data"]["defaultDatasetId"]
    items = client.get(
        f"https://api.apify.com/v2/datasets/{dataset_id}/items",
        params={"clean": "true", "format": "json"},
        headers={"Authorization": f"Bearer {token}"},
    ).json()
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(items))
    return items


def main() -> None:
    """Write the per-country rating snapshot and normalised review samples."""
    fetcher = common.CachedFetcher("itunes", min_interval_s=1.0)
    snapshot = []
    for country in COUNTRIES:
        body = fetcher.get_json("https://itunes.apple.com/lookup", {"id": ",".join(IOS_APPS), "country": country})
        for result in body["results"]:
            app, company = IOS_APPS[str(result["trackId"])]
            snapshot.append(
                {
                    "country": country,
                    "app": app,
                    "company": company,
                    "rating_count": result["userRatingCount"],
                    "average_rating": round(result["averageUserRating"], 3),
                    "snapshot_date": common.utc_now()[:10],
                }
            )
    common.write_rows(common.DERIVED_DIR / "app_store_ratings_by_country.csv", snapshot)

    token = os.environ["APIFY_TOKEN"]
    client = httpx.Client(timeout=120)
    reviews = []
    for sample, run_id in APIFY_REVIEW_RUNS.items():
        store, app = sample.rsplit("_", 1)
        for item in download_apify_dataset(client, token, run_id):
            if store == "google_play":
                reviews.append(
                    {
                        "store": store,
                        "app": app,
                        "review_id": item["id"],
                        "date": item["date"],
                        "rating": item["score"],
                        "app_version": item["version"],
                        "text": item["text"],
                    }
                )
            else:
                reviews.append(
                    {
                        "store": store,
                        "app": app,
                        "review_id": item["id"],
                        "date": item["date"],
                        "rating": item["score"],
                        "app_version": item["version"],
                        "text": f"{item['title']}. {item['text']}",
                    }
                )
    (common.CACHE_DIR / "app_reviews.jsonl").write_text("\n".join(json.dumps(row) for row in reviews))
    common.write_rows(
        common.DERIVED_DIR / "app_reviews_index.csv",
        [{key: row[key] for key in ("store", "app", "review_id", "date", "rating", "app_version")} for row in reviews],
    )
    common.upsert_collection_run(
        {
            "run_id": "app-stores",
            "source": "iTunes lookup API (ratings by country); Apify thewolves Google Play and App Store review actors",
            "collector": "research/collect_app_stores.py",
            "finished_at": common.utc_now(),
            "query": f"iOS apps={','.join(name for name, _ in IOS_APPS.values())}; countries={len(COUNTRIES)}; "
            f"review runs={','.join(f'{sample}:{run}' for sample, run in APIFY_REVIEW_RUNS.items())}",
            "window_start": "snapshot",
            "window_end": common.utc_now()[:10],
            "items": len(snapshot) + len(reviews),
            "requests": fetcher.requests,
            "cache_hits": fetcher.cache_hits,
            "cost_usd": round(len(reviews) * 0.0001, 2),
            "notes": "Review samples are newest-first (US, English); text is cached locally and not committed.",
        }
    )
    print(f"snapshot={len(snapshot)} reviews={len(reviews)}")


if __name__ == "__main__":
    main()
