# OpenAI versus Anthropic: the flagship RichView example

A public-data investigation of how perception of OpenAI and Anthropic shifted from January 2023 to October 2026: what drove it, and who leads on attention, use, money and capability, population by population. The report is the content source for the RichView theme showcase; themes may change presentation, never the data, wording or sources.

## What is here

| Path | What it is |
|---|---|
| `report.rmd` | The canonical RichView report, generated. Charts carry inline data, so the stock CLI renders it end to end. |
| `content/report.template.md` | The prose source. `{{fact:…}}`, `{{src:…}}`, `{{chart:…}}` and `{{table:…}}` tokens are filled at build time, so no number in the text is typed by hand. |
| `content/report.json` | The same report as theme-neutral sections and typed blocks, for renderers other than RichView. |
| `data/charts/*.csv`, `data/charts/index.json` | One tidy table per chart plus its spec: assertion title, encoding, axes, annotations, population, window and source ids. |
| `data/facts.json` | Every number quoted in the prose, with its source ids. |
| `ledger/sources.csv` | The dated source ledger. Each article, survey, benchmark and event row has its URL, publisher, date and a verbatim quote; `D-*` rows describe the datasets collected here. |
| `ledger/collection-runs.csv` | Every collection run: source, query, window, item count, requests, cost. |
| `assets/`, `ledger/image-sources.csv` | Active editorial images, dated source/hash ledger, renderer inputs and full license notices. The 15 October image revision is separate from the fixed statistical snapshot. |
| `media/` | Original illustrative photos retained for snapshot history, with their credits. |
| `research/` | The reproducible pipeline (Python, `uv`). Collectors, model labelling, analysis and builders. |
| `research/discovery/` | The Exa-based evidence ledgers (commercial, capability, releases, events, surveys) with findings notes. |
| `research/curated/` | Series transcribed from ledger rows (each value keeps its source id). |
| `research/derived/` | Tidy collected data: counts, labels by id, download series, trends. No review text and no usernames. |

## Rebuild the fixed snapshot offline

The committed `derived/`, `curated/` and `discovery/` inputs are the evidence for the 18 November 2026 snapshot, with data to October 2026. The app-store review samples and their labels keep their 2 October 2026 window, because the report prints none of their figures. Regenerate and verify them without collection or paid model calls:

```sh
cd examples/openai-vs-anthropic/research
uv sync --frozen
uv run --frozen verify_snapshot.py
uv run --frozen build_report.py
```

From the repository root, build the shared Rust product and export the report into its own folder:

```sh
cargo run --release --locked -p richview-build
target/release/richview generate examples/openai-vs-anthropic/report.rmd
```

Open `examples/openai-vs-anthropic/Rendered/report.html`. The default source style is `scientific` followed by the optional local `report.rstyle.yaml`. Native export copies local CSV/ledger/document links into `Rendered`, retaining portable paths when the whole report folder is moved. The public showcase's **Bundle** download carries the source, data, styling and this interactive rendered output together. The source repository retains the research pipeline; the public asset bundle excludes it.

The separate 15 and 23 October image revisions are documented in [assets/CREDITS.md](assets/CREDITS.md) and [ledger/image-sources.csv](ledger/image-sources.csv). `verify_snapshot.py` also checks image hashes, source counts and the retained license notices offline. The cover infographic is rebuilt offline with `uv run --frozen build_cover.py` in `research/` (snapshot verification regenerates it and requires identical bytes). Rebuilding the two original chart SVGs requires the release build and `node examples/openai-vs-anthropic/research/build_images.mjs` from the repository root; review the output before updating the image ledger. Run snapshot verification and the product build sequentially, because snapshot verification briefly creates a second report inside its isolated research `tmp/` directory.

## Optional research refresh

These commands collect a new snapshot and can incur the listed costs. They are separate from building or verifying this release:

```bash
cd examples/openai-vs-anthropic/research
uv sync
uv run collect_hn.py            # Hacker News via ClickHouse public playground (free)
uv run collect_wikipedia.py     # Wikimedia pageviews (free, rate-limited; ~1 h cold)
uv run collect_trends.py        # Google Trends JSON API (free)
uv run collect_packages.py      # npm + PyPI downloads (free)
uv run collect_github.py        # GitHub search totals (free, rate-limited; ~40 min cold)
uv run collect_reddit.py        # Arctic Shift Reddit archive (free)
uv run collect_capability.py    # Epoch AI + LMArena datasets (free, ~125 MB)
uv run collect_app_stores.py    # iTunes lookup + Apify review runs (needs APIFY_TOKEN; ~$2.60)
uv run label_hn.py              # Bedrock labelling (needs Bedrock credentials; ~$3 cold, new comments only on a refresh)
uv run label_reviews.py
uv run analyze.py && uv run build_ledger.py && uv run build_report.py
```

The committed `derived/`, `curated/` and `discovery/` inputs let `analyze.py`, `build_ledger.py` and `build_report.py` rebuild this snapshot offline; they do not require collectors or paid model calls. Run `uv run verify_snapshot.py` inside `research/` to rebuild in isolated `tmp/`, forbid socket connections and compare the report/ledger bytes plus portable JSON semantics. Collectors and labelling are separate optional refresh operations and can incur the costs listed above. Raw responses are cached under `research/tmp/` (gitignored). The Exa discovery ledgers were built on 2 October 2026 and are kept as evidence; rerunning discovery would return a different web. Python is research and media tooling; rendering and product behavior use Rust. Generated JSON uses Python's deterministic formatter.

The web distribution serves this example's report, content, chart data and ledger. Its builder omits the research subtree, hidden environment files and development caches; the reproducibility pipeline remains in the source repository. Keep other private material outside the public example directories.

## Ground rules the report keeps

- **Name the population.** Every chart and claim says who it measures; scraped or opt-in samples are never called "the public".
- **Trace every number.** It comes from a computed fact or a ledger row with a URL and a quote. Unknowns stay empty; nothing is interpolated or estimated.
- **Keep model labels neutral.** No label comes from either lab's model by default: the primary judge is Qwen3-235B, Devstral 2 replicates every trend, and Claude Haiku 4.5 and gpt-oss-120b test for self-preference.
- **Keep the scope bounded.** Time window 2023-01 to 2026-10. "Recent" means November 2025 to October 2026, compared against the twelve months before.

## Known limits

Anthropic's revenue comes from its registration statement (S-1 filed 26 October 2026; figures of the 10 November amendment, repeated unchanged in the latest, of 17 November); OpenAI had published no registration statement as of 18 November 2026, so its revenue comes from figures it reported to investors, as the press reported them. Anthropic publishes no user counts. Two series have breaks: Ramp changed method in May 2026, and Claude Code moved to a native installer. Hacker News story points are frozen at ingestion in the ClickHouse copy from mid-December 2025, so only item counts are used. The committed report is a fixed research snapshot, not a claim that these historical figures remain current.
