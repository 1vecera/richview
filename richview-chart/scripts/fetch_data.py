"""Download the time-series datasets from their publishers and write tidy CSVs to benchmarks/fixtures/time-series/.

Run with `uv run --no-project richview-chart/scripts/fetch_data.py`. Standard library only. Values are
copied as published (same decimals); the only derived column is German wind = onshore + offshore.
"""

import csv
import io
import json
import sys
import time
import urllib.error
import urllib.request
from datetime import UTC, date, datetime
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "benchmarks" / "fixtures" / "time-series"
RETRIEVED = datetime.now(UTC).strftime("%Y-%m-%d")
USER_AGENT = "richview-chart-showcase/0.1 (+https://github.com/richview-universe/richview-v3)"


def get(url: str) -> bytes:
    """Fetch a URL with a descriptive user agent, backing off politely when rate limited."""
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    for attempt in range(8):
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                return response.read()
        except urllib.error.HTTPError as error:
            if error.code != 429 or attempt == 7:
                raise
            wait = int(error.headers.get("Retry-After") or 0) or 15 * (attempt + 1)
            print(f"  rate limited, retrying in {wait}s", file=sys.stderr)
            time.sleep(wait)
    raise AssertionError("unreachable")


def write_csv(name: str, comments: list[str], header: list[str], rows: list[list[str]]) -> None:
    """Write a CSV whose leading `#` lines record provenance, then report its size."""
    path = OUT / name
    with path.open("w", newline="", encoding="utf-8") as handle:
        for line in comments:
            handle.write(f"# {line}\n")
        writer = csv.writer(handle, lineterminator="\n")
        writer.writerow(header)
        writer.writerows(rows)
    print(f"{name}: {len(rows)} rows, {path.stat().st_size / 1024:.0f} KiB", file=sys.stderr)


def co2_mauna_loa() -> None:
    """NOAA GML monthly mean CO2 at Mauna Loa, with per-month provenance notes from the file's own flags."""
    url = "https://gml.noaa.gov/webdata/ccgg/trends/co2/co2_mm_mlo.csv"
    lines = [line for line in get(url).decode().splitlines() if line and not line.startswith("#")]
    rows = []
    for record in csv.DictReader(lines):
        year, month = int(record["year"]), int(record["month"])
        if (year, month) <= (1974, 4):
            note = "Scripps (SIO) record"
        elif (2022, 12) <= (year, month) <= (2023, 6):
            note = "Measured at Maunakea while Mauna Loa was shut by the eruption"
        elif (year, month) == (2023, 7):
            note = "Maunakea until 4 July, then Mauna Loa"
        elif float(record["sdev"]) < 0:
            note = "Month interpolated by NOAA"
        else:
            note = ""
        days = record["ndays"] if int(record["ndays"]) >= 0 else ""
        rows.append([f"{year:04d}-{month:02d}", record["average"], record["deseasonalized"], days, note])
    write_csv(
        "co2-mauna-loa-monthly.csv",
        [
            "Monthly mean CO2 mole fraction (ppm, dry air) at Mauna Loa Observatory, Hawaii.",
            f"Source: NOAA Global Monitoring Laboratory, {url}, retrieved {RETRIEVED}.",
            "Cite: Lan, X., Tans, P. and K.W. Thoning, NOAA GML; Keeling, R., Scripps Institution of Oceanography.",
            "seasonally_adjusted is NOAA's deseasonalized column. note is derived from the file's own flags.",
        ],
        ["month", "monthly_mean", "seasonally_adjusted", "days_measured", "note"],
        rows,
    )


CPI_SERIES = {
    "all_items": "CPIAUCNS",
    "core": "CPILFENS",
    "food": "CPIUFDNS",
    "energy": "CPIENGNS",
    "shelter": "CUUR0000SAH1",
}


def us_cpi() -> None:
    """US CPI-U indexes (not seasonally adjusted, 1982-84=100) for headline and four components."""
    columns: dict[str, dict[str, str]] = {}
    for name, series_id in CPI_SERIES.items():
        text = get(f"https://fred.stlouisfed.org/graph/fredgraph.csv?id={series_id}").decode()
        reader = csv.DictReader(io.StringIO(text))
        columns[name] = {row["observation_date"][:7]: row[series_id] for row in reader}
    months = sorted(m for m in columns["core"] if m >= "1957-01")
    rows = [[m, *[columns[name].get(m, "") for name in CPI_SERIES]] for m in months]
    write_csv(
        "us-cpi-components.csv",
        [
            "US consumer price index for all urban consumers (CPI-U), not seasonally adjusted, 1982-84 = 100.",
            f"Source: U.S. Bureau of Labor Statistics via FRED, series {', '.join(CPI_SERIES.values())}; retrieved {RETRIEVED}.",
            "Empty cells are months BLS did not publish (October 2025: no data collection during the federal shutdown).",
        ],
        ["month", *CPI_SERIES],
        rows,
    )


def energy_charts(path: str) -> dict:
    """GET an Energy-Charts API endpoint as JSON."""
    return json.loads(get(f"https://api.energy-charts.info/{path}"))


def germany_power(start: date, end: date) -> None:
    """German load, solar, wind and day-ahead price in 15-minute steps (UTC), month by month."""
    by_time: dict[int, dict[str, float | None]] = {}
    cursor = start
    while cursor < end:
        nxt = min(date(cursor.year + cursor.month // 12, cursor.month % 12 + 1, 1), end)
        span = f"start={cursor.isoformat()}T00:00Z&end={nxt.isoformat()}T00:00Z"
        power = energy_charts(f"public_power?country=de&{span}")
        types = {p["name"]: p["data"] for p in power["production_types"]}
        for i, ts in enumerate(power["unix_seconds"]):
            row = by_time.setdefault(ts, {})
            row["load"] = types["Load"][i]
            row["solar"] = types["Solar"][i]
            row["wind_on"] = types["Wind onshore"][i]
            row["wind_off"] = types["Wind offshore"][i]
        time.sleep(2)
        price = energy_charts(f"price?bzn=DE-LU&{span}")
        for ts, value in zip(price["unix_seconds"], price["price"], strict=True):
            by_time.setdefault(ts, {})["price"] = value
        print(f"  {cursor:%Y-%m}: {len(power['unix_seconds'])} power, {len(price['unix_seconds'])} price", file=sys.stderr)
        cursor = nxt
        time.sleep(2)
    first = int(datetime(start.year, start.month, start.day, tzinfo=UTC).timestamp())
    last = int(datetime(end.year, end.month, end.day, tzinfo=UTC).timestamp())

    def cell(value: float | None) -> str:
        return "" if value is None else repr(value)

    rows = []
    for ts in sorted(t for t in by_time if first <= t < last):
        r = by_time[ts]
        on, off = r.get("wind_on"), r.get("wind_off")
        wind = None if on is None or off is None else round(on + off, 1)
        stamp = datetime.fromtimestamp(ts, UTC).strftime("%Y-%m-%dT%H:%MZ")
        rows.append([stamp, cell(r.get("load")), cell(r.get("solar")), cell(wind), cell(r.get("price"))])
    write_csv(
        "de-power-15min.csv",
        [
            "Germany: load, solar and wind generation (MW) and DE-LU day-ahead price (EUR/MWh), 15-minute steps, UTC.",
            f"Source: Energy-Charts API by Fraunhofer ISE (api.energy-charts.info); prices from Bundesnetzagentur | SMARD.de, CC BY 4.0. Retrieved {RETRIEVED}.",
            "wind_mw is the sum of the published onshore and offshore values. Times are interval starts in UTC.",
        ],
        ["time", "load_mw", "solar_mw", "wind_mw", "price_eur_mwh"],
        rows,
    )


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    co2_mauna_loa()
    us_cpi()
    germany_power(date(2025, 10, 1), date(2026, 10, 1))
