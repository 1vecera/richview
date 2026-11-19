"""Collect monthly Reddit activity from the Arctic Shift archive (arctic-shift.photon-reddit.com), a research mirror.

Measures: (1) current subscribers and lifetime post counts per subreddit (archive metadata snapshot); (2) posts per
month in each company's home subreddits, a proxy for the enthusiast community that gathers around a product. The
archive buckets months in CET/CEST, so month edges shift by one or two hours. Its aggregate endpoint times out on the
largest subreddits for some months; such a cell keeps the count the previous snapshot committed, with that count's
date in `counted`, and stays empty only where no snapshot ever counted it. Title-keyword counts
in neutral subreddits were attempted and dropped: the archive timed out on nearly every such query on 2026-10-02.
"""

import csv
from datetime import date, timedelta

import httpx

import common

HOME_SUBREDDITS = {
    "ChatGPT": "OpenAI",
    "OpenAI": "OpenAI",
    "ChatGPTPro": "OpenAI",
    "codex": "OpenAI",
    "ClaudeAI": "Anthropic",
    "Anthropic": "Anthropic",
    "ClaudeCode": "Anthropic",
    "GeminiAI": "Google",
    "Bard": "Google",
}
API = "https://arctic-shift.photon-reddit.com/api"


def window_counts(
    fetcher: common.CachedFetcher, params: dict[str, str | int], after: date, before: date, retries: int
) -> dict[str, int] | None:
    """Return month → post count for one filter and window, or None when the archive keeps timing out."""
    query = {
        **params,
        "aggregate": "created_utc",
        "frequency": "month",
        "after": after.isoformat(),
        "before": before.isoformat(),
    }
    try:
        body = fetcher.get_json(
            f"{API}/posts/search/aggregate", query, retries=retries, accept=lambda payload: payload["data"] is not None
        )
    except (RuntimeError, httpx.HTTPStatusError):
        return None
    # Buckets start at local midnight (e.g. 2025-12-31T23:00Z for January); shift forward a day to name them.
    return {
        (date.fromisoformat(str(bucket["created_utc"])[:10]) + timedelta(days=1)).strftime("%Y-%m"): int(
            bucket["count"]
        )
        for bucket in body["data"]
    }


def series(fetcher: common.CachedFetcher, params: dict[str, str | int]) -> dict[str, int | None]:
    """Return month → count over the study window: one request per year, falling back to monthly requests.

    Months the archive cannot count (repeated timeouts) stay None rather than being estimated.
    """
    counts: dict[str, int | None] = {}
    for year in range(common.WINDOW_START.year, common.WINDOW_END.year + 1):
        end = date(year + 1, 1, 1) if year < common.WINDOW_END.year else common.next_month(common.WINDOW_END)
        yearly = window_counts(fetcher, params, date(year, 1, 1), end, retries=2)
        months = [month for month in common.month_starts(date(year, 1, 1), end - timedelta(days=1))]
        if yearly is not None:
            for month in months:
                counts[month.isoformat()[:7]] = yearly.get(month.isoformat()[:7], 0)
            continue
        for month in months:
            monthly = window_counts(fetcher, params, month, common.next_month(month), retries=2)
            counts[month.isoformat()[:7]] = None if monthly is None else sum(monthly.values())
    return counts


def committed_counts() -> dict[tuple[str, str], tuple[str, str]]:
    """Read the counts the previous snapshot committed, keyed by (subreddit, month), with the day each was counted."""
    path = common.DERIVED_DIR / "reddit_monthly_posts.csv"
    if not path.exists():
        return {}
    with (common.LEDGER_DIR / "collection-runs.csv").open(newline="") as handle:
        run_day = next(
            (row["finished_at"][:10] for row in csv.DictReader(handle) if row["run_id"] == "reddit-arctic-shift"), ""
        )
    with path.open(newline="") as handle:
        return {
            (row["subreddit"], row["month"]): (row["posts"], row.get("counted") or run_day)
            for row in csv.DictReader(handle)
            if row["measure"] == "home_posts" and row["posts"] != ""
        }


def keep_committed(rows: list[dict[str, object]], committed: dict[tuple[str, str], tuple[str, str]]) -> int:
    """Give each month the archive could not count this time its previously committed count; return how many."""
    kept = 0
    for row in rows:
        previous = committed.get((str(row["subreddit"]), str(row["month"])))
        if row["posts"] is None and previous is not None:
            row["posts"], row["counted"] = int(previous[0]), previous[1]
            kept += 1
    return kept


def main() -> None:
    """Write subreddit metadata, monthly home-subreddit posts and monthly title mentions in neutral subreddits."""
    committed = committed_counts()
    fetcher = common.CachedFetcher("arctic-shift", min_interval_s=8.0, timeout_s=120.0)
    metadata = []
    for subreddit in HOME_SUBREDDITS:
        body = fetcher.get_json(f"{API}/subreddits/search", {"subreddit": subreddit})
        record = body["data"][0]
        metadata.append(
            {
                "subreddit": subreddit,
                "subscribers": record["subscribers"],
                "archive_posts": record["_meta"]["num_posts"],
                "archive_comments": record["_meta"]["num_comments"],
                "created_utc": date.fromtimestamp(int(record["created_utc"])).isoformat(),
                "snapshot": common.utc_now()[:10],
            }
        )
    common.write_rows(common.DERIVED_DIR / "reddit_subreddits.csv", metadata)
    rows = []
    for subreddit, company in HOME_SUBREDDITS.items():
        for month, count in series(fetcher, {"subreddit": subreddit}).items():
            rows.append(
                {
                    "measure": "home_posts",
                    "subreddit": subreddit,
                    "term": "",
                    "company": company,
                    "month": month,
                    "posts": count,
                    "counted": common.utc_now()[:10] if count is not None else "",
                }
            )
        print(subreddit, flush=True)
    kept = keep_committed(rows, committed)
    common.write_rows(common.DERIVED_DIR / "reddit_monthly_posts.csv", rows)
    unavailable = sum(row["posts"] is None for row in rows)
    common.upsert_collection_run(
        {
            "run_id": "reddit-arctic-shift",
            "source": "Arctic Shift Reddit archive API (subreddits/search, posts/search/aggregate)",
            "collector": "research/collect_reddit.py",
            "finished_at": common.utc_now(),
            "query": f"home={','.join(HOME_SUBREDDITS)}",
            "window_start": common.WINDOW_START.isoformat(),
            "window_end": common.WINDOW_END.isoformat(),
            "items": len(rows) + len(metadata),
            "requests": fetcher.requests,
            "cache_hits": fetcher.cache_hits,
            "cost_usd": 0,
            "notes": f"Monthly post counts, CET/CEST month buckets; {kept} month-series cells the archive timed out on "
            f"keep the previous snapshot's count (dated in `counted`); {unavailable} never counted, left empty rather "
            "than estimated.",
        }
    )
    print(f"rows={len(rows)} unavailable={unavailable}")


if __name__ == "__main__":
    main()
