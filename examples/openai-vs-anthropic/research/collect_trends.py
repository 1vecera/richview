"""Collect Google Trends interest (searchers, the broadest population in this study) through Trends' public JSON API.

Trends reports relative interest, 0-100 within one comparison, from a sample of Google searches; values can move by a
few points between pulls and only terms inside the same comparison share a scale. Each comparison here therefore
includes the terms it is meant to compare. Knowledge-graph topics (resolved through Trends autocomplete) separate the
Claude chatbot from people named Claude; plain search terms are kept as a cross-check.
"""

import json
import time
from collections.abc import Callable
from typing import Any

import httpx

import common

BROWSER_HEADERS = {
    "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36",
    "Accept-Language": "en-US,en;q=0.9",
}
TOPIC_LOOKUPS = {
    "ChatGPT": "ChatGPT",
    "Claude": "Claude",
    "OpenAI": "OpenAI",
    "Anthropic": "Anthropic",
    "Gemini": "Gemini",
}
LONG_WINDOW = "2022-01-01 2026-09-30"
RECENT_WINDOW = "2025-10-01 2026-09-30"
BASELINE_WINDOW = "2024-10-01 2025-09-30"


def trends_get(client: httpx.Client, path: str, params: dict[str, str]) -> Any:
    """GET a Trends API path, strip the anti-JSON-hijacking prefix and retry politely on 429."""
    for attempt in range(8):
        response = client.get(f"https://trends.google.com/trends/api/{path}", params=params)
        if response.status_code == 429:
            time.sleep(20 * (attempt + 1))
            continue
        response.raise_for_status()
        return json.loads(response.text[response.text.index("{") :])
    raise RuntimeError(f"Trends kept rate-limiting {path}")


def cached(name: str, fetch: Callable[[], Any]) -> Any:
    """Return a cached JSON payload by name, fetching and storing it on the first call."""
    path = common.CACHE_DIR / "trends" / f"{name}.json"
    if path.exists():
        return json.loads(path.read_text())
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = fetch()
    path.write_text(json.dumps({"fetched_at": common.utc_now(), "payload": payload}))
    time.sleep(4)
    return {"fetched_at": common.utc_now(), "payload": payload}


def topic_mid(client: httpx.Client, term: str) -> dict[str, str]:
    """Resolve a term to its first knowledge-graph topic through the Trends autocomplete endpoint."""
    record = cached(f"autocomplete-{term}", lambda: trends_get(client, f"autocomplete/{term}", {"hl": "en-US"}))
    topics = record["payload"]["default"]["topics"]
    return {
        "term": term,
        "mid": topics[0]["mid"],
        "title": topics[0]["title"],
        "type": topics[0]["type"],
        "alternatives": json.dumps([[topic["mid"], topic["title"], topic["type"]] for topic in topics[:6]]),
    }


def comparison(client: httpx.Client, name: str, items: list[str], window: str, geo: str) -> Any:
    """Fetch the explore widgets for one comparison, then its timeline and (when present) its per-country map."""

    def fetch() -> Any:
        request = {
            "comparisonItem": [{"keyword": item, "geo": geo, "time": window} for item in items],
            "category": 0,
            "property": "",
        }
        explore = trends_get(client, "explore", {"hl": "en-US", "tz": "0", "req": json.dumps(request)})
        widgets = {widget["id"]: widget for widget in explore["widgets"]}
        timeline = widgets["TIMESERIES"]
        result = {
            "timeline": trends_get(
                client,
                "widgetdata/multiline",
                {"hl": "en-US", "tz": "0", "req": json.dumps(timeline["request"]), "token": timeline["token"]},
            )
        }
        if "GEO_MAP" in widgets:
            geo_widget = widgets["GEO_MAP"]
            result["geo_map"] = trends_get(
                client,
                "widgetdata/comparedgeo",
                {"hl": "en-US", "tz": "0", "req": json.dumps(geo_widget["request"]), "token": geo_widget["token"]},
            )
        return result

    return cached(f"comparison-{name}", fetch)


def main() -> None:
    """Resolve topics, pull every comparison and write tidy timeline and country tables."""
    client = httpx.Client(headers=BROWSER_HEADERS, follow_redirects=True, timeout=60)
    client.get("https://trends.google.com/trends/explore?q=ChatGPT")  # sets the NID cookie the API expects
    topics = [topic_mid(client, term) for term in TOPIC_LOOKUPS]
    common.write_rows(common.DERIVED_DIR / "trends_topics.csv", topics)
    mid = {topic["term"]: topic["mid"] for topic in topics}
    comparisons = {
        "ww-topics-all": (
            [mid["ChatGPT"], mid["Claude"], mid["OpenAI"], mid["Anthropic"], mid["Gemini"]],
            LONG_WINDOW,
            "",
        ),
        "ww-topics-chatgpt-claude": ([mid["ChatGPT"], mid["Claude"]], LONG_WINDOW, ""),
        "ww-topics-openai-anthropic": ([mid["OpenAI"], mid["Anthropic"]], LONG_WINDOW, ""),
        "ww-terms-all": (["ChatGPT", "Claude", "OpenAI", "Anthropic", "Gemini"], LONG_WINDOW, ""),
        "us-topics-all": (
            [mid["ChatGPT"], mid["Claude"], mid["OpenAI"], mid["Anthropic"], mid["Gemini"]],
            LONG_WINDOW,
            "US",
        ),
        "us-topics-chatgpt-claude": ([mid["ChatGPT"], mid["Claude"]], LONG_WINDOW, "US"),
        "ww-geo-recent-chatgpt-claude": ([mid["ChatGPT"], mid["Claude"]], RECENT_WINDOW, ""),
        "ww-geo-baseline-chatgpt-claude": ([mid["ChatGPT"], mid["Claude"]], BASELINE_WINDOW, ""),
        "ww-geo-recent-openai-anthropic": ([mid["OpenAI"], mid["Anthropic"]], RECENT_WINDOW, ""),
        "ww-terms-coding-agents": (["Claude Code", "Codex", "Cursor"], RECENT_WINDOW, ""),
    }
    labels = {topic["mid"]: topic["term"] for topic in topics}
    timeline_rows = []
    geo_rows = []
    for name, (items, window, geo) in comparisons.items():
        record = comparison(client, name, items, window, geo)
        payload = record["payload"]
        for point in payload["timeline"]["default"]["timelineData"]:
            for item, value in zip(items, point["value"], strict=True):
                timeline_rows.append(
                    {
                        "comparison": name,
                        "geo": geo or "WW",
                        "week_start": time.strftime("%Y-%m-%d", time.gmtime(int(point["time"]))),
                        "item": labels.get(item, item),
                        "value": value,
                        "partial": point.get("isPartial", False),
                    }
                )
        if "geo_map" in payload:
            for region in payload["geo_map"]["default"]["geoMapData"]:
                for item, value in zip(items, region["value"], strict=True):
                    geo_rows.append(
                        {
                            "comparison": name,
                            "window": window,
                            "country": region["geoCode"],
                            "country_name": region["geoName"],
                            "item": labels.get(item, item),
                            "share": value,
                            "has_data": region["hasData"][0],
                        }
                    )
        print(name, flush=True)
    common.write_rows(common.DERIVED_DIR / "trends_timeline.csv", timeline_rows)
    common.write_rows(common.DERIVED_DIR / "trends_geo.csv", geo_rows)
    common.upsert_collection_run(
        {
            "run_id": "google-trends",
            "source": "Google Trends public JSON API (explore, widgetdata/multiline, widgetdata/comparedgeo)",
            "collector": "research/collect_trends.py",
            "finished_at": common.utc_now(),
            "query": "; ".join(
                f"{name}={'|'.join(labels.get(i, i) for i in items)}@{geo or 'WW'} {window}"
                for name, (items, window, geo) in comparisons.items()
            ),
            "window_start": "2022-01-01",
            "window_end": common.WINDOW_END.isoformat(),
            "items": len(timeline_rows) + len(geo_rows),
            "requests": 0,
            "cache_hits": 0,
            "cost_usd": 0,
            "notes": "Relative 0-100 interest within each comparison; sampled by Google, small pull-to-pull variation.",
        }
    )
    print(f"timeline={len(timeline_rows)} geo={len(geo_rows)}")


if __name__ == "__main__":
    main()
