# Content and configuration

A publication has one `site.yaml` and one dedicated folder per stable item. The folder name equals the item's `publish.id`. Existing Rich Markdown resources and optional `.rstyle.yaml` files stay beside their report.

```text
publications/news/
  site.yaml
  items/
    measurement-gap/
      report.rmd
      cover.svg
```

## Site YAML

```yaml
title: Research Journal
description: Evidence for the decisions ahead.
base_url: https://journal.example.com
layout: broadsheet
mode: external
theme: paper
accent: "#1a6dac"
sections:
  - id: research
    title: Research
    description: Findings and their limits.
navigation:
  - {label: Research, path: /sections/research}
  - {label: About, path: /about}
lead: [first-finding]
```

`layout` is `broadsheet` or `dispatch`. `theme` is `paper` or `ink`. `accent` is a six-digit hex colour, RichView cobalt `#1a6dac` by default. It marks links, the lead and focus rings. Ink uses `#5aa2db` for the default accent; any other accent is lifted 40% toward white. The front page shows one lead: the first `lead` ID the reader is allowed to see. When the lead report has a chart, its first chart appears beside the headline; otherwise the story's figure is shown large. The other lead IDs act as fallbacks for readers who cannot see the first.Pages use Newsreader, Source Serif 4, Geist and IBM Plex Mono. The gateway serves them from `/_fonts/` as the official OFL Fontsource subsets, split into Latin and Latin Extended-A, which covers Czech. A browser downloads the extended file only for pages that use those characters. The licences are in `cloudflare/publish/fonts/licenses/`. `mode: internal` requires membership across the entire publication. `mode: external` permits explicitly public items and can also contain restricted items. A site has up to 20 sections, 12 navigation links and four selected lead IDs. Navigation contains local absolute paths. Unknown site keys fail validation instead of becoming a second layout language.

The `/internal` member edition is an authenticated view of the same release. Its pages are not indexed. For a completely internal publication, use `mode: internal` on a separate deployment and a private source repository; do not rely on the member-edition navigation link as a security boundary.

## Rich Markdown metadata

```yaml
---
title: A useful finding
description: A short description for the report itself.
version: 1.0.0
styles: [scientific, report.rstyle.yaml]
publish:
  id: first-finding
  slug: useful-finding
  summary: The central finding and why it deserves attention.
  byline: Example Research Desk
  section: research
  tags: [example, methods]
  published: 2026-10-18T09:00:00Z
  updated: 2026-10-18T09:00:00Z
  state: published
  audience: public
  kind: report
  aliases: []
  corrections: []
  cover: cover.svg
  cover_alt: A description of the editorial illustration.
  figure:
    value: "84%"
    label: Completion rate in the South
    as_of: Oct 2026
---

Your report follows here.
```

`figure` is optional: one number the story stands behind. It needs a `value` of at most 16 characters, a `label` of at most 80 and an `as_of` of at most 32. Up to four of the newest figures the reader may see form the front-page figures strip, each linking to its story. The strip is omitted when no visible item has a figure. Unknown keys inside `figure`, as anywhere in `publish`, fail validation. Covers are optional.

## Covers and Unsplash photos

`cover` is either a file beside the report or an `https://images.unsplash.com/...` URL; `cover_alt` is required for either. A local cover appears only as a thumbnail in list rows. A remote Unsplash cover also becomes the large photo above the front-page lead and a figure in the story header, outside the report frame.

```yaml
  cover: https://images.unsplash.com/photo-1559538834-8b012e20a84f?ixid=M3w&w=1600&q=80&fm=jpg&fit=max
  cover_alt: A red tram passing blurred façades.
  cover_credit:
    author: Marek Rucinski
    author_url: https://unsplash.com/@marulla?utm_source=richview&utm_medium=referral
    source: Unsplash
    source_url: https://unsplash.com/?utm_source=richview&utm_medium=referral
```

A remote cover accepts that host only: lowercase `https://images.unsplash.com/...`, at most 2048 ASCII characters with no spaces, quotes, backslashes or fragment, no credentials and no port. Uppercase schemes or hosts, `http://` and every other host fail validation. Only values starting with `https://` or `http://` count as remote; anything else is a local file name.

The builder never fetches, reads or embeds the photo. The reader's browser loads the URL with `ixid` and every other parameter kept, as the Unsplash API guidelines require. The pages set the display width (`w=`) and a fixed aspect ratio, so layout does not shift while the photo loads. Because the photo is hotlinked, member and internal pages also make requests to `images.unsplash.com`; Unsplash sees those requests, so do not use a remote cover where that is unacceptable.

A remote cover needs `cover_credit`, which has the same shape as an image `credit` in Rich Markdown. `author` is required. `author_url` is required for a remote cover and, like `source_url` when present, must be an `https://unsplash.com/...` address. `source` may be left out or set to `Unsplash` (any case); any other source fails. Unknown keys fail. A local cover may also carry a credit, with any http(s) addresses. The credit renders as "Photo by <author> on <source>" under the story and lead photo and as a compact "Photo: <author> / <source>" line in list rows. Both names are links that open in a new tab, so write the profile and Unsplash URLs with `?utm_source=richview&utm_medium=referral`.

The gateway allows `https://images.unsplash.com` as an image source for pages, which carry covers. A report frame gets it only when that report's own HTML has an element whose `src` is `https://images.unsplash.com/...` (a credited photo in the report; mentioning the address in text or a link does not count); every other report frame keeps `img-src 'self' data:`. Report frames also permit popups, so credit links inside a report can open in a new tab.

IDs, slugs, sections and audience names use lowercase letters, numbers and hyphens. IDs never change. Dates are RFC3339 timestamps, and update time must not precede publication. Summaries are at most 600 characters; bylines are at most 160. Covers require alternative text. The renderer enforces the existing published-report title/version contract in addition to publication metadata.

`kind` is `report` or `page`; any other value, `dashboard` included, fails validation. Reports use `/stories/<slug>`; static pages use `/<slug>`, such as `/about`. Reserved platform routes cannot be taken by an item. `/r/<id>` resolves the stable identity to its current route. Add a former slug to `aliases` when renaming; do not invent a new ID to update a story.

States are `draft`, `published` and `hidden`. Drafts can be read in a permitted editorial preview; hidden items are unavailable. A runtime urgent hide is an additional persistent denial and cannot be cleared by changing YAML or rolling back a release.

Audiences are `public`, `members`, or an explicit entitlement such as `research-team`. Role scopes and entitlements are separate: an editor must have access to the audience and the item's section. Draft, hidden and restricted source files require a private canonical repository. Access control on the website cannot make a public Git history private.

A correction is an entry with `date` and `note` under `publish.corrections`. Keep the ID and relevant aliases, update the date, and explain the material correction. Deleting an item removes it from discovery and denies its older hosted routes after activation. Changing an audience also applies a current access restriction to old releases.

## Resources and reproducibility

The builder performs no network fetches. It reuses RichView's bounded source-folder resolver; absolute paths, parent escapes and symlink escapes fail. Local media and data are embedded in the rendered report. Linked downloads are stored as separate per-item objects and pass through the same access checks. Arbitrary embedded services are not part of the offline publication contract; link to a sourced external page instead.

Generated HTML, search/feed output and release objects belong outside canonical source folders. The release JSON names content-addressed objects, the source commit, the pinned toolchain and runtime hashes. Uploads verify hashes before staging. A failed build leaves any previous output directory and hosted release untouched.

## Charts as images

SVGs are served as images with a script-free sandbox policy rather than interpolated into the publication DOM. An image cannot use the page's CSS, so the lead thumbnail, `thumbnail.svg`, carries its own chart styles. Reports run the shared RichView browser runtime in a sandboxed frame. Those frames cannot access the editorial desk's authenticated origin.

## Link previews

Every published story (not a page) gets a 1200 × 630 PNG link preview, `og.png`, rendered by the builder with `richview-share` when the release is built: the story's title and byline in IBM Plex Mono over its first chart, or over its local cover when it has no chart, or alone when it has neither. A remote Unsplash cover is hotlinked and never fetched, so it does not appear on the card. The image is a release object served at `/_content/<id>/og.png` through the same checks as the story, so it is a 404 to anyone who cannot read the story. The story page carries `og:title`, `og:description`, `og:type article`, `og:image` (absolute, from `base_url`), `og:image:alt` and `twitter:card summary_large_image` only when an anonymous reader may read that story now: published, `public` in the release and in the ledger floor, not private and not hidden, in an external publication. The stored article holds only a slot; the server fills it on each request, so a story that stops being public stops advertising its image at once. Feeds, search and the sitemap do not list the image. A file the author linked from the story as `og.png` is used as it is, in place of the generated card, and is advertised under the same rules. Robots.txt allows `/_content/*/og.png` ahead of its `Disallow: /_`, so chat and social crawlers can fetch the image of a public story.
