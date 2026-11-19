"""Derive who leads on capability, by benchmark and over time, from open benchmark datasets.

Sources (downloaded into tmp/datasets, documented in discovery/capability_datasets.md): Epoch AI's Benchmarking Hub
and Capabilities Index (CC BY 4.0) and LMArena's leaderboard history (CC-BY-4.0). A lab "leads" a benchmark on a day
when its best score among models released by that day is the highest; margins inside measurement error still count as
a lead here, so the report shows confidence intervals for the composite index.
"""

import zipfile
from datetime import date, timedelta
from typing import Any

import httpx
import polars as pl

import common

DATASET_DIR = common.CACHE_DIR / "datasets"
DOWNLOADS = {
    "benchmark_data.zip": "https://epoch.ai/data/benchmark_data.zip",
    "lmarena-leaderboard-dataset/text/full-00000-of-00001.parquet": "https://huggingface.co/datasets/lmarena-ai/leaderboard-dataset/resolve/main/text/full-00000-of-00001.parquet",
    "lmarena-leaderboard-dataset/text_style_control/full-00000-of-00001.parquet": "https://huggingface.co/datasets/lmarena-ai/leaderboard-dataset/resolve/main/text_style_control/"
    "full-00000-of-00001.parquet",
}
BENCHMARKS = {
    "SWE-bench Verified": ("swe_bench_verified.csv", "Best score (across scorers)"),
    "Terminal-Bench": ("terminalbench_external.csv", "Accuracy mean"),
    "METR time horizon": ("metr_time_horizons_external.csv", "Time horizon"),
    "GDPval": ("gdpval_external.csv", "Win Rate (%)"),
    "OSWorld": ("os_world_external.csv", "Score"),
    "GPQA Diamond": ("gpqa_diamond.csv", "Best score (across scorers)"),
    "FrontierMath": ("frontiermath.csv", "Best score (across scorers)"),
    "Humanity's Last Exam": ("hle_external.csv", "Accuracy"),
    "ARC-AGI-2": ("arc_agi_2_external.csv", "Score"),
}
LABS = ["OpenAI", "Anthropic", "Google"]
LEAD_WINDOW = (date(2025, 11, 1), date(2026, 10, 31))
STYLE_CONTROL_DEFAULT_FROM = date(2025, 5, 16)


def download_missing() -> None:
    """Fetch each dataset once and unpack Epoch's benchmark archive."""
    for relative, url in DOWNLOADS.items():
        path = DATASET_DIR / relative
        if not path.exists():
            path.parent.mkdir(parents=True, exist_ok=True)
            with httpx.stream("GET", url, follow_redirects=True, timeout=600) as response:
                response.raise_for_status()
                with path.open("wb") as handle:
                    for chunk in response.iter_bytes():
                        handle.write(chunk)
    if not (DATASET_DIR / "benchmark_data").exists():
        with zipfile.ZipFile(DATASET_DIR / "benchmark_data.zip") as archive:
            archive.extractall(DATASET_DIR / "benchmark_data")


def lab_of(organization: pl.Expr) -> pl.Expr:
    """Map Epoch/LMArena organization strings to the three labs compared here, else Other."""
    lowered = organization.str.to_lowercase()
    return (
        pl.when(lowered.str.contains("openai"))
        .then(pl.lit("OpenAI"))
        .when(lowered.str.contains("anthropic"))
        .then(pl.lit("Anthropic"))
        .when(lowered.str.contains("google"))
        .then(pl.lit("Google"))
        .otherwise(pl.lit("Other"))
    )


def benchmark_scores(file_name: str, score_column: str) -> pl.DataFrame:
    """Load one Epoch benchmark file as (release_date, lab, model, score) rows."""
    raw = pl.read_csv(DATASET_DIR / "benchmark_data" / file_name, infer_schema_length=0)
    return raw.select(
        pl.col("Release date").str.to_date(strict=False).alias("release_date"),
        lab_of(pl.col("Organization").fill_null("")).alias("lab"),
        pl.col("Model version").alias("model"),
        pl.col(score_column).cast(pl.Float64, strict=False).alias("score"),
    ).drop_nulls(["release_date", "score"])


def lead_days(scores: pl.DataFrame, start: date, end: date) -> dict[str, int]:
    """Count days in [start, end] on which each lab's best released score was the highest of all labs."""
    counts = {lab: 0 for lab in [*LABS, "Other"]}
    day = start
    while day <= end:
        released = scores.filter(pl.col("release_date") <= day)
        if released.height:
            best = released.group_by("lab").agg(pl.col("score").max())
            top = best["score"].max()
            for lab in best.filter(pl.col("score") == top)["lab"]:
                counts[lab] += 1
        day += timedelta(days=1)
    return counts


def eci_frontier() -> list[dict[str, Any]]:
    """Return each model that raised its lab's record on Epoch's Capabilities Index, with its confidence interval."""
    eci = pl.read_csv(DATASET_DIR / "benchmark_data" / "epoch_capabilities_index" / "eci_scores.csv")
    eci = eci.with_columns(lab_of(pl.col("Organization")).alias("lab"), pl.col("date").str.to_date()).sort("date")
    rows = []
    for lab in LABS:
        record = float("-inf")
        for model in eci.filter(pl.col("lab") == lab).iter_rows(named=True):
            if model["eci"] > record:
                record = model["eci"]
                rows.append(
                    {
                        "date": model["date"].isoformat(),
                        "lab": lab,
                        "model": model["Display name"],
                        "eci": model["eci"],
                        "ci_low": model["eci_ci_low"],
                        "ci_high": model["eci_ci_high"],
                    }
                )
    return rows


def lmarena_leaders() -> list[dict[str, Any]]:
    """Return, per leaderboard snapshot, each lab's best overall text rating and the leading lab.

    The plain text board is used before style control became the default view (2025-05-16) and the style-controlled
    board after it; ratings are only compared within a snapshot.
    """
    frames = []
    for subset, keep in (
        ("text", pl.col("snapshot") < STYLE_CONTROL_DEFAULT_FROM),
        ("text_style_control", pl.col("snapshot") >= STYLE_CONTROL_DEFAULT_FROM),
    ):
        board = pl.read_parquet(DATASET_DIR / "lmarena-leaderboard-dataset" / subset / "full-00000-of-00001.parquet")
        board = board.filter(pl.col("category") == "overall").with_columns(
            pl.col("leaderboard_publish_date").cast(pl.Date).alias("snapshot"),
            lab_of(pl.col("organization")).alias("lab"),
        )
        frames.append(board.filter(keep))
    board = pl.concat(frames, how="diagonal_relaxed").filter(pl.col("snapshot") >= date(2023, 12, 1))
    best = board.group_by("snapshot", "lab").agg(
        pl.col("rating").max().alias("best_rating"),
        pl.col("model_name").sort_by("rating", descending=True).first().alias("best_model"),
    )
    top = best.group_by("snapshot").agg(pl.col("best_rating").max().alias("top_rating"))
    best = best.join(top, on="snapshot").with_columns(
        (pl.col("best_rating") - pl.col("top_rating")).alias("gap_to_top")
    )
    return best.sort("snapshot", "lab").with_columns(pl.col("snapshot").cast(pl.String)).to_dicts()


def main() -> None:
    """Download inputs and write the ECI frontier, benchmark lead days and LMArena leader tables."""
    download_missing()
    frontier = eci_frontier()
    eci_end = max(str(row["date"]) for row in frontier)
    common.write_rows(common.DERIVED_DIR / "capability_eci_frontier.csv", frontier)
    rows = []
    for benchmark, (file_name, score_column) in BENCHMARKS.items():
        scores = benchmark_scores(file_name, score_column)
        days = lead_days(scores, *LEAD_WINDOW)
        as_of = scores.filter(pl.col("release_date") <= LEAD_WINDOW[1])
        for lab in [*LABS, "Other"]:
            lab_scores = as_of.filter(pl.col("lab") == lab).sort("score", descending=True)
            rows.append(
                {
                    "benchmark": benchmark,
                    "lab": lab,
                    "lead_days": days[lab],
                    "window_days": (LEAD_WINDOW[1] - LEAD_WINDOW[0]).days + 1,
                    "best_score": lab_scores["score"][0] if lab_scores.height else None,
                    "best_model": lab_scores["model"][0] if lab_scores.height else None,
                }
            )
        print(benchmark, days, flush=True)
    common.write_rows(common.DERIVED_DIR / "capability_lead_days.csv", rows)
    leaders = lmarena_leaders()
    arena_end = max(str(row["snapshot"]) for row in leaders)
    common.write_rows(common.DERIVED_DIR / "lmarena_lab_best.csv", leaders)
    common.upsert_collection_run(
        {
            "run_id": "capability-datasets",
            "source": (
                "Epoch AI Benchmarking Hub + Capabilities Index (CC BY 4.0); LMArena leaderboard dataset (CC-BY-4.0)"
            ),
            "collector": "research/collect_capability.py",
            "finished_at": common.utc_now(),
            "query": f"benchmarks={','.join(BENCHMARKS)}; lead window {LEAD_WINDOW[0]}..{LEAD_WINDOW[1]}",
            "window_start": "2023-12-01",
            "window_end": LEAD_WINDOW[1].isoformat(),
            "items": len(rows),
            "requests": len(DOWNLOADS),
            "cache_hits": 0,
            "cost_usd": 0,
            "notes": f"Downloaded {date.today().isoformat()}; Epoch Capabilities Index to {eci_end}, LMArena to "
            f"{arena_end}; the report reads both to {LEAD_WINDOW[1].isoformat()}.",
        }
    )


if __name__ == "__main__":
    main()
