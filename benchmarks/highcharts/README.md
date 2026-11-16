# Local Highcharts reference

This folder holds the Highcharts reference corpus tooling and the eight synthetic fixture reports of the October 2026 benchmark. Each report has a dedicated folder; inline YAML and `dense/power.csv` belong to their chart. The fixtures are synthetic benchmark data, not claims about real districts or businesses. [REVIEW.md](REVIEW.md) records the rounds of that study.

The comparison page, capture, verification and browse scripts compared the SVG chart engine with Highcharts. They were removed with that engine and are in the Git history; a comparison with the WebGL report runtime needs new harnesses.

Highcharts **13.1.1** is pinned in a separate, local evidence installation, outside the repository and production dependencies.

## Reference corpus

```sh
export RICHVIEW_EVIDENCE=/home/vecera/tmp/s27-582-evidence
npm install --prefix "$RICHVIEW_EVIDENCE/reference" --save-exact --ignore-scripts highcharts@13.1.1
uv run benchmarks/highcharts/acquire.py "$RICHVIEW_EVIDENCE" --count 110
node benchmarks/highcharts/official.mjs "$RICHVIEW_EVIDENCE"
```

`RICHVIEW_CHROMIUM` selects a headless Chromium binary; the default is `/usr/bin/chromium`. It uses an isolated Playwright context and never uses a signed-in desktop profile. Open `$RICHVIEW_EVIDENCE/reference/index.html` for the gallery contact sheet.

## Reference corpus and rights

`acquire.py` reads the official gallery listing, downloads public chart thumbnails, verifies the official demo page, decodes the image and rejects small/empty images or duplicate decoded pixels. `reference/index.json` records official URLs, absolute local image paths, chart/state, dimensions, timestamps, download provenance and byte/pixel hashes. It counts **125 distinct verified pictures** in this run. Decorative website assets, unverified URLs and the additional ten live captures do not inflate that count.

`official.mjs` also captures five actual official sample pages at rest and hover, recording their configuration/axes/version. On this host, Chromium blocked the upstream script CDN with ORB; the script serves those script requests from the pinned local package while retaining the official HTML and data. This substitution is recorded in `reference/live/index.json`.

The package license and lockfile remain under `reference/`. This is local, nonproduction product evaluation, as described by the [official download guidance](https://www.highcharts.com/download/) and [installation guidance](https://www.highcharts.com/docs/getting-started/installation). Highsoft retains rights to its implementation and gallery material. Package code and pictures are neither committed nor redistributed with RichView. No license purchase, production integration or deployment is performed. Consult the actual retained terms before any later use outside this evaluation.

The before/after review and remaining gaps are in [REVIEW.md](REVIEW.md).
