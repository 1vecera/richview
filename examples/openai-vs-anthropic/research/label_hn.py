"""Label the Hacker News comment sample (stance, preference, topic) and the switching candidates.

Both neutral judges label everything, so every trend is replicated by a second model. A deterministic validation subset
(200 comments per side, spread over time) is also labelled by one model from each lab to test for self-preference.

A refresh keeps every label already committed under derived/ for the same judge and comment (same prompt, temperature
0) and sends only the comments without one, so a new month costs only that month's comments.
"""

import csv
import hashlib
import json
from collections.abc import Callable
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


def committed(name: str) -> dict[tuple[str, str], dict[str, str]]:
    """Read the labels a previous snapshot committed, keyed by (judge, comment id)."""
    path = common.DERIVED_DIR / name
    if not path.exists():
        return {}
    with path.open(newline="") as handle:
        return {(row["judge"], str(row["id"])): row for row in csv.DictReader(handle)}


def label_new(
    judge: str,
    task: classify.Task,
    items: list[dict[str, str]],
    known: dict[tuple[str, str], dict[str, str]],
    restore: Callable[[dict[str, str]], dict[str, Any]],
) -> tuple[dict[str, dict[str, Any]], int]:
    """Reuse the committed label of every item that has one and label only the rest; return labels and reuse count."""
    labels = {item["id"]: restore(known[(judge, item["id"])]) for item in items if (judge, item["id"]) in known}
    fresh = [item for item in items if item["id"] not in labels]
    reused = len(labels)
    labels.update(classify.label_items(judge, task, fresh) if fresh else {})
    return labels, reused


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
    known_stance = committed("hn_stance_labels.csv")
    known_switch = committed("hn_switch_labels.csv")
    reuse: dict[str, int] = {}
    sample = [{**row, "id": str(row["id"])} for row in load_jsonl("hn_comment_sample.jsonl")]
    validation = validation_subset(sample, per_side=200)
    rows: list[dict[str, Any]] = []
    for judge in classify.JUDGES:
        # Both neutral judges label everything (the second replicates every trend); lab judges label the subset.
        items = sample if judge in (classify.PRIMARY_JUDGE, "devstral-2") else validation
        labels, reused = label_new(
            judge,
            classify.STANCE_TASK,
            items,
            known_stance,
            lambda row: {
                "openai": row["openai"],
                "anthropic": row["anthropic"],
                "prefers": row["prefers"],
                "topics": row["topics"].split("|") if row["topics"] else [],
            },
        )
        reuse[f"hn_stance-{judge}"] = reused
        rows.extend(stance_rows(judge, items, labels))
        print(judge, len(labels), "of", len(items), f"({reused} committed)", flush=True)
    common.write_rows(common.DERIVED_DIR / "hn_stance_labels.csv", rows)
    common.write_rows(common.DERIVED_DIR / "hn_validation_ids.csv", [{"id": row["id"]} for row in validation])

    candidates = [
        {"id": str(row["id"]), "time": row["time"], "text": row["text"]}
        for row in load_jsonl("hn_switch_candidates.jsonl")
    ]
    switch_rows = []
    for judge in (classify.PRIMARY_JUDGE, "devstral-2"):
        labels, reused = label_new(
            judge,
            classify.SWITCH_TASK,
            candidates,
            known_switch,
            lambda row: {"direction": row["direction"], "first_person": row["first_person"] == "True"},
        )
        reuse[f"hn_switch-{judge}"] = reused
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
        print("switch", judge, len(labels), "of", len(candidates), f"({reused} committed)", flush=True)
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
                        f"billed at Bedrock on-demand prices; {reuse.get(f'{task.name}-{judge}', 0)} labels kept "
                        "from the previous snapshot's derived files",
                    }
                )


if __name__ == "__main__":
    main()
