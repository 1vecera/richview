---
title: Export HTML and PDF
description: Share an interactive report, or print a settled document locally.
styles: [scientific]
---

Continue from [your first report](https://richview.ai/docs/first-report). HTML and PDF are local exports. Neither starts a publishing server.

## Share interactive HTML

```sh
richview generate first-report/report.rmd --format json
```

Open `first-report/Rendered/report.html`. Native export embeds the runtime, styles, licensed fonts, data and local media. It also copies linked local files under `Rendered/`. Share that entire folder when the report offers document downloads. Keep the original `.rmd`, styles and inputs if someone needs to edit the report.

Credited Unsplash photos are hotlinks: the HTML carries their URL, reserved box, alt text and credit. The photograph needs a network connection. External links and embeds also depend on their destinations. A report using only local resources can travel offline.

## Generate a PDF

Install a local Chromium, Google Chrome or Microsoft Edge first. RichView does not download a browser. Then run:

```sh
richview generate first-report/report.rmd --to pdf --format json
richview generate first-report/report.rmd -o review.pdf --page letter --format json
```

The files are `first-report/Rendered/report.pdf` and `review.pdf`. A4 is the default; the second example selects Letter. A `.pdf` output filename also selects PDF output. The JSON result includes the path, byte and page counts, browser version, settled-state information and diagnostics.

Use `--browser` with an executable path, or set `RICHVIEW_BROWSER`, to choose a browser explicitly. Otherwise RichView checks the usual executable names and macOS application bundles. Chrome 131 or later supplies running titles and page numbers. PDF generation supports Linux and macOS; Windows and hosted PDF generation are not provided.

## What the printed report keeps

Charts settle, controls disappear and all table rows print, with repeated headers and totals once. Figures stay with their source notes. Sleek prints in Air's colours; the light styles retain their own treatment. Video becomes a poster or a text card; audio and embeds become links. A local document link prints as text, so keep the HTML bundle for downloads. Large figures may leave space at a page boundary. PDF tags and bookmarks do not by themselves establish accessibility.

The browser uses a temporary profile. Its requests are limited to the staged report, embedded data and the credited Unsplash images the report displays. If a photo cannot load, the PDF retains its box and credit and reports `export.photo-unavailable`. Other blocked requests report `export.network-blocked`. The default settle timeout is 30 seconds; `--settle-timeout` accepts a longer duration. A failed export does not replace an existing PDF.

Browser Print uses the report's print stylesheet. The native PDF command adds a controlled page size, media renditions and settled-state checks. Use [publishing](https://richview.ai/docs/publishing) when you want a server address with version history.
