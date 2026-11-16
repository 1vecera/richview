# Where `life-expectancy-by-region.csv` comes from

The CSV is Gapminder's own life expectancy series for its four world regions, kept for 1950 to 2019 and rounded to one decimal. RichView aggregates nothing: every regional value is Gapminder's. [`gapminder_regions.py`](gapminder_regions.py) writes it.

| | |
| --- | --- |
| Dataset | Gapminder fasttrack, [open-numbers/ddf--gapminder--fasttrack](https://github.com/open-numbers/ddf--gapminder--fasttrack) at commit `b840e5bed1e9f68695af9cb5b037fe2dce576de0` (29 October 2026, `datapackage.json` version 2.0.0). The CSV was first written on 5 November 2026 from fasttrack's `master`, which was then at this commit. |
| Indicator | `lex`, life expectancy at birth, version 15, updated 5 April 2026 ([gapm.io/dlex](http://gapm.io/dlex)); CC BY 4.0. |
| Input | `global_regions_datapoints/ddf--datapoints--lex--by--world_4region--time.csv`, SHA-256 `7e2fe875f9dc81967906ddc2770f3d0088f6f9df6fbb545957b282d193b36996`. |
| Output | `life-expectancy-by-region.csv`, 280 rows, SHA-256 `fc4c21802abf26f80bbd95fa2097a58cd220c0831aa54ac6602286d4426e13b9`. The script writes it with Python's `csv` module, so its lines end in CRLF. |

## Rebuild it

```sh
(
set -e
dir=$(mktemp -d)
base=https://raw.githubusercontent.com/open-numbers/ddf--gapminder--fasttrack/b840e5bed1e9f68695af9cb5b037fe2dce576de0
for f in global_regions_datapoints/ddf--datapoints--lex--by--world_4region--time.csv \
         countries_etc_datapoints/ddf--datapoints--lex--by--country--time.csv \
         countries_etc_datapoints/ddf--datapoints--pop--by--country--time.csv \
         ddf--entities--geo--country.csv; do
  mkdir -p "$dir/$(dirname "$f")" && curl -fsSL "$base/$f" -o "$dir/$f"
done
(cd "$dir" && shasum -a 256 -c) <<'SUMS'
7e2fe875f9dc81967906ddc2770f3d0088f6f9df6fbb545957b282d193b36996  global_regions_datapoints/ddf--datapoints--lex--by--world_4region--time.csv
c1223d486d43908bb1a8c1a55e5108f222159614edc3f951d53eb6afefa2484b  countries_etc_datapoints/ddf--datapoints--lex--by--country--time.csv
87e96a3555f00437e1fcd85c715f8be029de64ac100d2c32085f97d25aa4f491  countries_etc_datapoints/ddf--datapoints--pop--by--country--time.csv
0b24dc8a3bc6c35991f695fb8ede4c4bb1c63af822554b7683fb1f510c1654da  ddf--entities--geo--country.csv
SUMS
uv run assets/landing/anchor/data/gapminder_regions.py "$dir" "$dir/out.csv" --check-weighting
(cd "$dir" && shasum -a 256 -c) <<'SUMS'
fc4c21802abf26f80bbd95fa2097a58cd220c0831aa54ac6602286d4426e13b9  out.csv
SUMS
echo "rebuilt $dir/out.csv"
)
```

The recipe checks the four inputs before it runs the script and the output after, and it stops at the first command that fails. It runs in a subshell, so pasted into a terminal, a failure ends only the recipe and not the shell. The CSV needs only the first input; the other three serve the check below.

## "Weighted by population"

The report's source note calls the regional values averages weighted by population. `--check-weighting` tests that against fasttrack's country series at the same commit:

- the countries' life expectancy, `countries_etc_datapoints/ddf--datapoints--lex--by--country--time.csv`, SHA-256 `c1223d486d43908bb1a8c1a55e5108f222159614edc3f951d53eb6afefa2484b`;
- their population, `countries_etc_datapoints/ddf--datapoints--pop--by--country--time.csv`, SHA-256 `87e96a3555f00437e1fcd85c715f8be029de64ac100d2c32085f97d25aa4f491`;
- the region of each country, `ddf--entities--geo--country.csv`, SHA-256 `0b24dc8a3bc6c35991f695fb8ede4c4bb1c63af822554b7683fb1f510c1654da`.

A population-weighted mean of each region's countries lands within 0.21 years of Gapminder's value in all 280 region-years. A plain mean is off by up to 13.90 years. The small remainder is Gapminder's own aggregation, which this repository does not reproduce.
