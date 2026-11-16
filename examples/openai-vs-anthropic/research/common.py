"""Shared paths, the study window and a cached HTTP fetcher used by every collector."""

import csv
import hashlib
import json
import time
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from datetime import UTC, date, datetime
from pathlib import Path
from typing import Any

import httpx

RESEARCH_DIR = Path(__file__).resolve().parent
REPORT_DIR = RESEARCH_DIR.parent
CACHE_DIR = RESEARCH_DIR / "tmp"
DERIVED_DIR = RESEARCH_DIR / "derived"
LEDGER_DIR = REPORT_DIR / "ledger"
CHART_DIR = REPORT_DIR / "data" / "charts"

WINDOW_START = date(2023, 1, 1)
WINDOW_END = date(2026, 9, 30)
USER_AGENT = "richview-research/0.1 (OpenAI-vs-Anthropic public-data study; github.com/1vecera)"

COLLECTION_RUN_FIELDS = [
    "run_id",
    "source",
    "collector",
    "finished_at",
    "query",
    "window_start",
    "window_end",
    "items",
    "requests",
    "cache_hits",
    "cost_usd",
    "notes",
]


@dataclass
class CachedFetcher:
    """GET JSON through an on-disk cache so reruns are free and every response stays inspectable."""

    namespace: str
    min_interval_s: float = 0.0
    headers: dict[str, str] = field(default_factory=dict)
    timeout_s: float = 60.0
    retry_statuses: tuple[int, ...] = (429, 500, 502, 503, 504)
    backoff_s: float = 3.0
    requests: int = 0
    cache_hits: int = 0
    _last_request_at: float = 0.0

    def __post_init__(self) -> None:
        self.cache_dir = CACHE_DIR / "http" / self.namespace
        self.cache_dir.mkdir(parents=True, exist_ok=True)
        self.client = httpx.Client(
            headers={"User-Agent": USER_AGENT, **self.headers}, timeout=self.timeout_s, follow_redirects=True
        )

    def get_json(
        self,
        url: str,
        params: dict[str, str | int] | None = None,
        retries: int = 6,
        missing_ok: bool = False,
        accept: Callable[[Any], bool] | None = None,
    ) -> Any:
        """Return the decoded JSON body, reading the cache first and pacing live requests.

        With missing_ok, a 404 is cached and returned as None (e.g. a title with no recorded views). With accept, a
        body the predicate rejects (e.g. a soft "timeout" payload with HTTP 200) is retried and never cached.
        """
        request = self.client.build_request("GET", url, params=params)
        key = hashlib.sha256(str(request.url).encode()).hexdigest()[:32]
        cache_path = self.cache_dir / f"{key}.json"
        if cache_path.exists():
            self.cache_hits += 1
            return json.loads(cache_path.read_text())["body"]
        for attempt in range(retries + 1):
            wait_s = self.min_interval_s - (time.monotonic() - self._last_request_at)
            if wait_s > 0:
                time.sleep(wait_s)
            self._last_request_at = time.monotonic()
            response = self.client.send(request)
            self.requests += 1
            if response.status_code in self.retry_statuses and attempt < retries:
                time.sleep(min(300.0, self.backoff_s * 2**attempt))
                continue
            if response.status_code == 404 and missing_ok:
                body = None
            else:
                response.raise_for_status()
                body = response.json()
                if accept is not None and not accept(body):
                    if attempt == retries:
                        raise RuntimeError(f"rejected body after {retries} retries for {request.url}")
                    time.sleep(min(120.0, 15.0 * 2**attempt))
                    continue
            envelope = {"url": str(request.url), "fetched_at": utc_now(), "status": response.status_code, "body": body}
            cache_path.write_text(json.dumps(envelope))
            return body
        raise RuntimeError(f"unreachable retry loop for {request.url}")


def clickhouse_rows(fetcher: CachedFetcher, sql: str) -> list[dict[str, Any]]:
    """Run a read-only SQL query on ClickHouse's public playground (user demo) and return rows as dicts."""
    body = fetcher.get_json("https://sql-clickhouse.clickhouse.com/", {"user": "demo", "query": f"{sql} FORMAT JSON"})
    return body["data"]


def write_rows(path: Path, rows: Sequence[Mapping[str, Any]]) -> None:
    """Write dict rows to CSV with the first row's keys as the header."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=list(rows[0]), lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)


def utc_now() -> str:
    """Return the current UTC time as an ISO-8601 string with seconds precision."""
    return datetime.now(UTC).replace(microsecond=0).isoformat()


def month_starts(start: date, end: date) -> list[date]:
    """List the first day of every month from start's month through end's month."""
    months = []
    current = date(start.year, start.month, 1)
    while current <= end:
        months.append(current)
        current = date(current.year + current.month // 12, current.month % 12 + 1, 1)
    return months


def next_month(month: date) -> date:
    """Return the first day of the month after the given month start."""
    return date(month.year + month.month // 12, month.month % 12 + 1, 1)


def upsert_collection_run(run: dict[str, Any]) -> None:
    """Insert or replace one row (keyed by run_id) in the shared collection-run ledger."""
    LEDGER_DIR.mkdir(parents=True, exist_ok=True)
    ledger_path = LEDGER_DIR / "collection-runs.csv"
    rows: list[dict[str, str]] = []
    if ledger_path.exists():
        with ledger_path.open(newline="") as handle:
            rows = [row for row in csv.DictReader(handle) if row["run_id"] != run["run_id"]]
    rows.append({name: str(run[name]) for name in COLLECTION_RUN_FIELDS})
    rows.sort(key=lambda row: row["run_id"])
    with ledger_path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLLECTION_RUN_FIELDS, lineterminator="\n")
        writer.writeheader()
        writer.writerows(rows)
