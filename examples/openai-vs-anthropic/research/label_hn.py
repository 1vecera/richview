"""Label the Hacker News comment sample (stance, preference, topic) and the switching candidates.

Both neutral judges label everything, so every trend is replicated by a second model. A deterministic validation subset
(200 comments per side, spread over time) is also labelled by one model from each lab to test for self-preference.
"""

import hashlib
import json
from typing import Any

import classify
import common


def load_jsonl(name: str) -> list[dict[str, str]]:
    """Read a cached JSONL file of collected items."""
    return [json.loads(line) for line in (common.CACHE_DIR / name).read_text().splitlines()]


def validation_subset(sample: list[dict[str, str]], per_side: int) -> list[dict[str, str]]:
    """Pick a fixed, hash-ordered subset with the same number of comments per side."""
    chosen = []
    for side in ("openai", "anthropic", "both"):
        rows = sorted(
            (row for row in sample if row["side"] == side),
            key=lambda row: hashlib.sha256(str(row["id"]).encode()).hexdigest(),
        )
        chosen.extend(rows[:per_side])
    return chosen


def stance_rows(judge: str, items: list[dict[str, str]], labels: dict[str, dict[str, Any]]) -> list[dict[str, Any]]:
    """Flatten stance labels into CSV rows keyed by comment id and judge."""
    rows = []
    for item in items:
        label = labels.get(str(item["id"]))
        if label is None:
            continue
        rows.append(
            {
                "id": item["id"],
                "month": str(item["time"])[:7],
                "side": item["side"],
                "judge": judge,
                "openai": label["openai"],
                "anthropic": label["anthropic"],
                "prefers": label["prefers"],
                "topics": "|".join(label["topics"]),
            }
        )  # type: ignore[arg-type]
    return rows


def main() -> None:
    """Run every judge on the validation subset, the primary judge on everything, and the switching task."""
    sample = [{**row, "id": str(row["id"])} for row in load_jsonl("hn_comment_sample.jsonl")]
    validation = validation_subset(sample, per_side=200)
    rows: list[dict[str, Any]] = []
    for judge in classify.JUDGES:
        # Both neutral judges label everything (the second replicates every trend); lab judges label the subset.
        items = sample if judge in (classify.PRIMARY_JUDGE, "devstral-2") else validation
        labels = classify.label_items(judge, classify.STANCE_TASK, items)
        rows.extend(stance_rows(judge, items, labels))
        print(judge, len(labels), "of", len(items), flush=True)
    common.write_rows(common.DERIVED_DIR / "hn_stance_labels.csv", rows)
    common.write_rows(common.DERIVED_DIR / "hn_validation_ids.csv", [{"id": row["id"]} for row in validation])

    candidates = [
        {"id": str(row["id"]), "time": row["time"], "text": row["text"]}
        for row in load_jsonl("hn_switch_candidates.jsonl")
    ]
    switch_rows = []
    for judge in (classify.PRIMARY_JUDGE, "devstral-2"):
        labels = classify.label_items(judge, classify.SWITCH_TASK, candidates)
        for item in candidates:
            label = labels.get(item["id"])
            if label is not None:
                switch_rows.append(
                    {
                        "id": item["id"],
                        "month": str(item["time"])[:7],
                        "judge": judge,
                        "direction": label["direction"],
                        "first_person": label["first_person"],
                    }
                )
        print("switch", judge, len(labels), "of", len(candidates), flush=True)
    common.write_rows(common.DERIVED_DIR / "hn_switch_labels.csv", switch_rows)

    for judge in classify.JUDGES:
        for task in (classify.STANCE_TASK, classify.SWITCH_TASK):
            usage = classify.usage_totals(judge, task)
            if usage["calls"]:
                common.upsert_collection_run(
                    {
                        "run_id": f"labels-{task.name}-{judge}",
                        "source": f"Amazon Bedrock eu-central-1, {classify.JUDGES[judge]}",
                        "collector": "research/label_hn.py",
                        "finished_at": common.utc_now(),
                        "query": f"task={task.name}; batch={classify.BATCH_SIZE}; temperature=0",
                        "window_start": "2023-01-01",
                        "window_end": common.WINDOW_END.isoformat(),
                        "items": usage["calls"] * classify.BATCH_SIZE,
                        "requests": usage["calls"],
                        "cache_hits": 0,
                        "cost_usd": "",
                        "notes": f"input tokens={usage['inputTokens']}; output tokens={usage['outputTokens']}; "
                        "billed at Bedrock on-demand prices",
                    }
                )


if __name__ == "__main__":
    main()
