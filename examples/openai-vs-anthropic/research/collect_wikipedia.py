"""Collect Wikipedia pageviews (human agents only, redirects included) for both companies and their products.

Wikipedia readers are a broad information-seeking population, not "the public": the series measures curiosity, not
use or approval. Views of every redirect are summed into the target so renames and alternative titles do not split
the count. Monthly views cover twelve language editions; daily views cover English for event studies. Google's
articles were dropped after the rate-limited API stalled on them; no chart needs them.
"""

import csv
from urllib.parse import quote

import common

ENTITIES = {
    "openai": ("OpenAI", "OpenAI"),
    "anthropic": ("Anthropic", "Anthropic"),
    "chatgpt": ("OpenAI", "ChatGPT"),
    "claude": ("Anthropic", "Claude (language model)"),
    "sam_altman": ("OpenAI", "Sam Altman"),
    "dario_amodei": ("Anthropic", "Dario Amodei"),
}
LANGUAGES = ["en", "de", "fr", "es", "it", "pt", "pl", "cs", "ru", "ja", "ko", "zh"]


def resolve_sitelinks(fetcher: common.CachedFetcher, en_title: str) -> dict[str, str]:
    """Map language code to the article title for the Wikidata item behind an English title."""
    pages = fetcher.get_json(
        "https://en.wikipedia.org/w/api.php",
        {"action": "query", "prop": "pageprops", "titles": en_title, "redirects": 1, "format": "json"},
    )
    page = next(iter(pages["query"]["pages"].values()))
    qid = page["pageprops"]["wikibase_item"]
    entity = fetcher.get_json(
        "https://www.wikidata.org/w/api.php",
        {"action": "wbgetentities", "ids": qid, "props": "sitelinks", "format": "json"},
    )
    sitelinks = entity["entities"][qid]["sitelinks"]
    return {lang: sitelinks[f"{lang}wiki"]["title"] for lang in LANGUAGES if f"{lang}wiki" in sitelinks}


def redirect_titles(fetcher: common.CachedFetcher, lang: str, title: str) -> list[str]:
    """Return the article title plus every main-namespace redirect pointing at it."""
    titles = [title]
    params: dict[str, str | int] = {
        "action": "query",
        "titles": title,
        "prop": "redirects",
        "rdlimit": "max",
        "rdnamespace": 0,
        "format": "json",
    }
    while True:
        result = fetcher.get_json(f"https://{lang}.wikipedia.org/w/api.php", params)
        page = next(iter(result["query"]["pages"].values()))
        titles.extend(redirect["title"] for redirect in page.get("redirects", []))
        if "continue" not in result:
            return titles
        params = {**params, **result["continue"]}


def pageviews(fetcher: common.CachedFetcher, lang: str, title: str, granularity: str) -> dict[str, int]:
    """Return timestamp → user pageviews for one exact title over the study window (missing titles → empty)."""
    start = common.WINDOW_START.strftime("%Y%m%d")
    end = common.WINDOW_END.strftime("%Y%m%d")
    article = quote(title.replace(" ", "_"), safe="")
    url = (
        f"https://wikimedia.org/api/rest_v1/metrics/pageviews/per-article/{lang}.wikipedia/all-access/user/"
        f"{article}/{granularity}/{start}00/{end}00"
    )
    body = fetcher.get_json(url, missing_ok=True)
    if body is None:
        return {}
    return {item["timestamp"][:8]: item["views"] for item in body["items"]}


def main() -> None:
    """Fetch monthly views for all languages and daily English views, then write tidy CSVs."""
    fetcher = common.CachedFetcher("wikipedia", min_interval_s=0.3)
    monthly_rows = []
    daily_rows = []
    for entity, (company, en_title) in ENTITIES.items():
        sitelinks = resolve_sitelinks(fetcher, en_title)
        for lang, title in sitelinks.items():
            titles = redirect_titles(fetcher, lang, title)
            totals: dict[str, int] = {}
            for each_title in titles:
                for stamp, views in pageviews(fetcher, lang, each_title, "monthly").items():
                    totals[stamp] = totals.get(stamp, 0) + views
            for stamp, views in sorted(totals.items()):
                monthly_rows.append(
                    {
                        "month": f"{stamp[:4]}-{stamp[4:6]}",
                        "lang": lang,
                        "entity": entity,
                        "company": company,
                        "title": title,
                        "titles_summed": len(titles),
                        "views": views,
                    }
                )
            if lang == "en":
                daily: dict[str, int] = {}
                for each_title in titles:
                    for stamp, views in pageviews(fetcher, lang, each_title, "daily").items():
                        daily[stamp] = daily.get(stamp, 0) + views
                for stamp, views in sorted(daily.items()):
                    daily_rows.append(
                        {
                            "date": f"{stamp[:4]}-{stamp[4:6]}-{stamp[6:]}",
                            "entity": entity,
                            "company": company,
                            "views": views,
                        }
                    )
        print(f"{entity}: {len(sitelinks)} languages")
    common.DERIVED_DIR.mkdir(parents=True, exist_ok=True)
    with (common.DERIVED_DIR / "wikipedia_monthly.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(monthly_rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(monthly_rows)
    with (common.DERIVED_DIR / "wikipedia_daily_en.csv").open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(daily_rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(daily_rows)
    common.upsert_collection_run(
        {
            "run_id": "wikipedia-pageviews",
            "source": "Wikimedia REST pageviews API (agent=user)",
            "collector": "research/collect_wikipedia.py",
            "finished_at": common.utc_now(),
            "query": f"entities={','.join(ENTITIES)}; languages={','.join(LANGUAGES)}; redirects summed",
            "window_start": common.WINDOW_START.isoformat(),
            "window_end": common.WINDOW_END.isoformat(),
            "items": len(monthly_rows) + len(daily_rows),
            "requests": fetcher.requests,
            "cache_hits": fetcher.cache_hits,
            "cost_usd": 0,
            "notes": "Free public API. Monthly rows per language/entity; daily rows for English only.",
        }
    )
    print(f"monthly rows={len(monthly_rows)} daily rows={len(daily_rows)} requests={fetcher.requests}")


if __name__ == "__main__":
    main()
