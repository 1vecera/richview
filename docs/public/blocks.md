---
title: Blocks and media
description: Working examples of every rich block, with source and resources you can reuse.
styles: [scientific]
---

Open the [live block tour](https://richview.ai/docs/gallery/blocks/) to try every block in one report. It includes local image, audio, video, captions and transcript files. The [format reference](https://richview.ai/docs/reference#rich-fences) supplies the exact fields and types; this page explains when to reach for each block.

## Evidence in charts, tables and metrics

A chart maps source columns to marks; choose a form in the [chart gallery](https://richview.ai/docs/charts). A table shows the observations and supports sorting, row limits and a total. A metric states one authored value: it does not calculate itself from a chart. Keep that value and its evidence consistent.

````rmd
```metric
value: 450
label: Total orders
```

```table
title: Three fictional months
data:
  inline:
    - { month: Jul, orders: 120 }
    - { month: Aug, orders: 150 }
    - { month: Sep, orders: 180 }
columns:
  - { key: month, label: Month }
  - { key: orders, label: Orders, format: number, align: right }
total: true
```

*Source: fictional tutorial observations; 120 + 150 + 180 = 450.*
````

Axis, table-column and metric formatting share a vocabulary. Percent formatting expects a fraction; a unit labels the value without converting it. Display rounding preserves the source observations. Expanded table-column entries let you give a column a label, format, unit and alignment.

## Keep the caveat near the claim

````rmd
```callout
kind: hold
title: A small sample
body: These observations are fictional. Replace the data and the authored total together.
```
````

Callout bodies support Markdown. Use a direction that matches the message and keep the evidence in the surrounding report.

## Images and header pictures

An image block needs a source and alt text. A frontmatter `title_image` opens the report with the same image vocabulary. Local image files are embedded by native export. Dimensions reserve space; captions explain what the picture contributes.

````rmd context=examples/elements
```image
src: media/nightingale.jpg
alt: Nightingale's original area diagrams of mortality in the Army in the East
caption: Florence Nightingale, Diagram of the Causes of Mortality in the Army in the East, 1858. David Rumsey Map Collection; public domain.
```
````

Credited Unsplash photos are the supported remote-image path for image blocks and header pictures. They remain hotlinks with visible credit. The reader needs the network for the photo; its box, alt text and credit remain offline. Other remote images must be saved inside the report folder. A Markdown image remains a local resource even when its URL points to Unsplash.

With `UNSPLASH_ACCESS_KEY` in the environment, `richview image search "calm sea" --orientation landscape --format json` finds candidates. `richview image use` takes the chosen photo ID and `--as title-image` or `--as block`; it fetches metadata, registers a download and prints ready-to-paste YAML. The MCP photo tools do the same. `RICHVIEW_UNSPLASH_APP` selects the attribution campaign, and `RICHVIEW_UNSPLASH_API` can select a signing proxy. The [command reference](https://richview.ai/docs/tools) gives the accepted flags. Never put the key in source. Rendering does not register another download.

## Audio, video and transcripts

Players use native browser controls and need a title as their accessible name. Add transcripts; add a WebVTT captions file to video. The block tour uses a NASA mission recording and a silent archival video excerpt, with transcripts and captions.

````rmd context=examples/elements
```audio
src: media/apollo-step.mp3
title: Neil Armstrong's first words on the Moon
caption: Apollo 11, 21 July 1969 UTC. NASA Historical Sounds; public-domain mission recording.
transcript: media/transcript.md
```

```video
src: media/apollo-step.mp4
title: Buzz Aldrin comes down the lunar module ladder
caption: A 12-second silent excerpt from the Apollo 11 television video that NASA partially restored in 2009. NASA/Goddard Space Flight Center; public domain.
poster: media/apollo-poster.jpg
transcript: media/video-transcript.md
captions: media/captions.vtt
```
````

Local files and safe supported data URIs work. Remote audio, video and posters are not report-local resources. PDF output uses a poster or text card for video and links for audio. [The live tour](https://richview.ai/docs/gallery/blocks/) includes a transcript and captions so you can inspect the complete bundle.

## Embed an external page

````rmd
```embed
url: https://www.youtube-nocookie.com/embed/aqz-KE-bpKQ
height: 360
caption: An external player; its destination needs the network.
```
````

An embed needs HTTPS and an allowed host. The [embed reference](https://richview.ai/docs/reference#callouts-and-media) describes the host allowlist and sandbox rules. An authored `allow` list replaces the default. Embeds retain their external availability and privacy properties; they are not offline copies.

## Explain a figure as the reader scrolls

A scrolly block keeps one image or chart beside ordered narrative steps. Image steps set a view and an optional highlighted region. Chart steps set authored chart control states. Reduced motion keeps the narrative usable; the steps remain content rather than executable code.

````rmd context=examples/elements
```scrolly
title: Read Nightingale's original diagram
image:
  src: media/nightingale.jpg
  alt: Nightingale's original mortality diagrams, with the first winter on the right
steps:
  - title: The whole picture
    text: Compare both periods before looking closer.
    view: { x: 0, y: 0, zoom: 1 }
  - title: The first winter
    text: The area assigned to preventable disease dominates the first winter.
    view: { x: 0.35, y: 0.05, zoom: 1.6 }
```
````

The [scrolly fields](https://richview.ai/docs/reference#callouts-and-media) define both figure forms. Open the [block tour](https://richview.ai/docs/gallery/blocks/) for a running example with all its local files.
