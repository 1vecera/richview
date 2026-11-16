"""Label what drives app-store ratings for ChatGPT and Claude over the same recent window.

Window: 2026-09-01 to 2026-10-01, the period covered by every sample (ChatGPT's newest-first samples span days, Claude's
weeks). Up to 1,500 reviews per store and app are drawn by hash order; the primary judge labels all of them and the
second neutral judge labels a 300-review subset for agreement.
"""

import hashlib
import json

import classify
import common

WINDOW = ("2026-09-01", "2026-10-02")
PER_SAMPLE = 1500


def main() -> None:
    """Select the shared-window sample, label it, and write one row per review and judge."""
    reviews = [json.loads(line) for line in (common.CACHE_DIR / "app_reviews.jsonl").read_text().splitlines()]
    in_window = [row for row in reviews if WINDOW[0] <= str(row["date"])[:10] < WINDOW[1] and str(row["text"]).strip()]
    sample = []
    for store in ("google_play", "app_store"):
        for app in ("chatgpt", "claude"):
            rows = sorted(
                (row for row in in_window if row["store"] == store and row["app"] == app),
                key=lambda row: hashlib.sha256(str(row["review_id"]).encode()).hexdigest(),
            )
            sample.extend(rows[:PER_SAMPLE])
    items = [{"id": str(row["review_id"]), "text": f"[{row['rating']} stars] {row['text']}"} for row in sample]
    by_id = {str(row["review_id"]): row for row in sample}
    output = []
    for judge, judged in ((classify.PRIMARY_JUDGE, items), ("devstral-2", items[::5][:300])):
        labels = classify.label_items(judge, classify.REVIEW_TASK, judged)
        for item in judged:
            label = labels.get(item["id"])
            if label is None:
                continue
            row = by_id[item["id"]]
            output.append(
                {
                    "review_id": item["id"],
                    "store": row["store"],
                    "app": row["app"],
                    "date": str(row["date"])[:10],
                    "rating": row["rating"],
                    "judge": judge,
                    "topics": "|".join(label["topics"]),
                }
            )  # type: ignore[arg-type]
        print(judge, len(labels), "of", len(judged), flush=True)
    common.write_rows(common.DERIVED_DIR / "app_review_labels.csv", output)
    for judge in (classify.PRIMARY_JUDGE, "devstral-2"):
        usage = classify.usage_totals(judge, classify.REVIEW_TASK)
        common.upsert_collection_run(
            {
                "run_id": f"labels-app_review-{judge}",
                "source": f"Amazon Bedrock eu-central-1, {classify.JUDGES[judge]}",
                "collector": "research/label_reviews.py",
                "finished_at": common.utc_now(),
                "query": f"task=app_review; window={WINDOW[0]}..{WINDOW[1]}; up to {PER_SAMPLE} per store and app",
                "window_start": WINDOW[0],
                "window_end": WINDOW[1],
                "items": usage["calls"] * classify.BATCH_SIZE,
                "requests": usage["calls"],
                "cache_hits": 0,
                "cost_usd": "",
                "notes": f"input tokens={usage['inputTokens']}; output tokens={usage['outputTokens']}",
            }
        )


if __name__ == "__main__":
    main()
