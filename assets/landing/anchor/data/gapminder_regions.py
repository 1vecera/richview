# /// script
# requires-python = ">=3.13"
# ///
"""The anchor report's data, life expectancy by world region, written from Gapminder's fasttrack dataset.

The CSV is Gapminder's own regional series (fasttrack's `lex` by `world_4region`), kept for the anchor's years and
rounded to one decimal; RichView aggregates nothing. With `--check-weighting` the script also rebuilds each region
and year from fasttrack's country series, as a population-weighted and as a plain mean, and prints how far each
lands from Gapminder's value. SOURCE.md names the fasttrack commit, the files and their SHA-256.

    uv run gapminder_regions.py FASTTRACK_DIR OUT_CSV [--check-weighting]

FASTTRACK_DIR holds fasttrack's files in its repository's layout: a checkout, or the files SOURCE.md downloads.
"""

import argparse
import csv
from collections import defaultdict
from pathlib import Path

__all__ = ["main"]

FIRST_YEAR, LAST_YEAR = 1950, 2019
REGION_NAMES = {"africa": "Africa", "americas": "Americas", "asia": "Asia", "europe": "Europe"}
REGION_LIFE = "global_regions_datapoints/ddf--datapoints--lex--by--world_4region--time.csv"
COUNTRY_LIFE = "countries_etc_datapoints/ddf--datapoints--lex--by--country--time.csv"
COUNTRY_POPULATION = "countries_etc_datapoints/ddf--datapoints--pop--by--country--time.csv"
COUNTRY_REGIONS = "ddf--entities--geo--country.csv"


def main() -> None:
    """Write the anchor's CSV from a fasttrack folder and, if asked, check Gapminder's population weighting."""
    parser = argparse.ArgumentParser(description="Write the anchor's life expectancy by region from fasttrack.")
    parser.add_argument("fasttrack", type=Path, help="fasttrack's files in its repository's layout")
    parser.add_argument("out", type=Path, help="the CSV to write")
    parser.add_argument(
        "--check-weighting", action="store_true", help="compare each region with population-weighted country means"
    )
    args = parser.parse_args()
    region_life = _read_series(args.fasttrack / REGION_LIFE, key="world_4region", value="lex")
    _write_anchor_table(region_life, args.out)
    if args.check_weighting:
        weighted, plain = _compare_with_country_means(args.fasttrack, region_life=region_life)
        print(
            f"largest gap to Gapminder's regions, {FIRST_YEAR}-{LAST_YEAR}: "
            f"population-weighted mean {weighted:.2f} years, plain mean {plain:.2f} years"
        )


def _read_series(path: Path, *, key: str, value: str) -> dict[tuple[str, int], float]:
    """Read one fasttrack datapoints file as {(entity, year): value}, for the anchor's years only."""
    with path.open(newline="") as handle:
        return {
            (row[key], int(row["time"])): float(row[value])
            for row in csv.DictReader(handle)
            if FIRST_YEAR <= int(row["time"]) <= LAST_YEAR
        }


def _write_anchor_table(region_life: dict[tuple[str, int], float], out: Path) -> None:
    """Write year, region and life rows at one decimal, sorted by region and then year, as the anchor reads them."""
    rows = sorted(
        (
            {"year": year, "region": REGION_NAMES[region], "life": round(life, 1)}
            for (region, year), life in region_life.items()
        ),
        key=lambda row: (row["region"], row["year"]),
    )
    with out.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=["year", "region", "life"])
        writer.writeheader()
        writer.writerows(rows)
    print(f"{out}: {len(rows)} rows, {len(REGION_NAMES)} regions, {FIRST_YEAR}-{LAST_YEAR}")


def _compare_with_country_means(fasttrack: Path, *, region_life: dict[tuple[str, int], float]) -> tuple[float, float]:
    """Rebuild each region and year from its countries and return the largest gaps to Gapminder's values.

    The countries' life expectancy is averaged per region and year twice, weighted by population and plain. Each
    average is compared with Gapminder's value for that region and year; the result is the largest absolute gap
    of each kind, in years.
    """
    with (fasttrack / COUNTRY_REGIONS).open(newline="") as handle:
        region_of = {row["country"]: row["world_4region"] for row in csv.DictReader(handle)}
    country_life = _read_series(fasttrack / COUNTRY_LIFE, key="country", value="lex")
    population = _read_series(fasttrack / COUNTRY_POPULATION, key="country", value="pop")
    weighted_life: defaultdict[tuple[str, int], float] = defaultdict(float)
    people: defaultdict[tuple[str, int], float] = defaultdict(float)
    lives: defaultdict[tuple[str, int], list[float]] = defaultdict(list)
    for (country, year), life in country_life.items():
        cell = (region_of[country], year)
        weighted_life[cell] += life * population[(country, year)]
        people[cell] += population[(country, year)]
        lives[cell].append(life)
    weighted = max(abs(weighted_life[cell] / people[cell] - life) for cell, life in region_life.items())
    plain = max(abs(sum(lives[cell]) / len(lives[cell]) - life) for cell, life in region_life.items())
    return weighted, plain


if __name__ == "__main__":
    main()
