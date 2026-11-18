# Proposed chart point policy — S27-888

Target: Charts — account for every authored observation when another application embeds RichView. The lead ruled on 5 November 2026 that the default draws all points up to a measured ceiling, uses disclosed representatives above it, and explicit `all` bypasses the ceiling. This proposed replacement for FEA-CH-17 remains assumed until the lockdown owner records Daniel's answer. No accepted answer is changed here.

## FEA-CH-17 proposal

When `style.points` is omitted, line, area and scatter draw every valid observation up to the chart type's declared automatic ceiling in [the engine's limit table](richview-chart/src/limits.rs), counted across the selected data state before zoom or legend hiding, including all series and facets. Above it, line/area use M4, while scatter keeps at least one representative per occupied CSS-pixel cell per series plus x/y/radius extremes. Each always emits a `decimation` info check with `perPixel`, `kept` and `total`, including when the current zoom needs no reduction. For scatter, density counts points per plot pixel area; line/area count observations per horizontal pixel. The check identifies automatic fallback and its ceiling. This replaces FEA-CH-75's error for these automatic point ceilings.

Explicit `style.points: all` draws every valid observation with no ceiling; the author or embedding host accepts the cost. Line/area also accept explicit `m4` or `band`, while scatter accepts explicit `all` only. The lead ruled on these semantics on 5 November at 16:38 CET.

`m4` retains the first, last, minimum and maximum observation in each occupied pixel column. `band` uses M4 up to six observations per horizontal plot pixel and, above that density, draws per-column means and minimum–maximum bands with the existing plain-language method note. Stacked areas use M4 instead of mean/range bands. Every actual reduction in either mode emits the same informational kept/total check; band means also remain identified as aggregation.

Readouts, Table and CSV retain every observation at the author's precision. Gaps, coincident observations, stacked boundaries, facets and zoom boundary neighbours keep their existing meaning. Interpolation is independent: `style: {points: all, curve: linear}` connects every observation with straight segments at any size. Screen and vector output consume the same geometry.

Companion scope: TYP-17 adds the validated field and default; FEA-CH-14 covers the reduction disclosure; FEA-CH-75 adds disclosed default representatives instead of an error for these point ceilings, with an unlimited explicit-all override. FEA-CH-37's precision rule is unchanged. Part 07's existing budgets are unchanged; the new workloads are measurements, not new fixture-specific budget passes.

The lead's final timing scope, 5 November at 19:07 CET: the default's pointer/overlay frames use the 8 ms budget with the complete 100,000-point source. Startup, continuous full resize and this new 100,000-point zoom workload are scaling disclosures; explicit-all timings are informational. S27-898 owns the 100,000-point zoom budget. PRF-20's existing 35,000×4 gate and all other release checks are unchanged.

The generated line-and-scatter report measured the default line’s desktop pointer frames at about 2.8 ms p95. A later run during the fast gate measured scatter pointer frames at 11.4 ms p95, above the 8 ms target. Five isolated repeats confirmed 12.1–13.0 ms p95 with 100 distinct, accurate source readings each; S27-898 also owns this pointer defect. Its line zoom, around a 10% source window, measured 23.8 ms for the synchronous render and 123.3 ms to two frames; these exceed the follow-up's 8/100 ms targets. A matched full-resize comparison at viewport 1,280 measured line/scatter p95 at 242.1/209.4 ms before the readout cache and 30.1/52.4 ms after it. At 390 px with Chromium 4× CPU and 2× DPR, the corresponding values were 652.7/840.8 ms and 151.9/271.1 ms. Actual chart widths changed in every measured resize sequence. Host load differed between the two captures, so the whole ratio cannot be attributed to the cache. These measurements disclose scaling costs; they do not certify the existing release fixtures.

## Ceiling evidence

The proposed table value is 10,000 valid observations per chart. A deterministic oscillating series was measured at 1,000, 10,000 and 100,000 observations, widths 390 and 1,280, straight and monotone paths, plus scatter. Native timings use 100 samples after five warmups on Claw Mini. They time prepared layout and mesh separately and exclude GPU upload and browser paint.

| Raw all-points workload | Layout p95 | Mesh p95 | Mesh vertex storage |
| --- | --- | --- | --- |
| 1,000, across measured cases | 0.03–0.09 ms | 0.05–0.45 ms | 1.2–3.6 MB |
| 10,000, across measured cases | 0.53–2.05 ms | 1.77–5.90 ms | 12–36 MB |
| 100,000, across measured cases | 4.72–11.92 ms | 9.50–70.57 ms | 120–360 MB |

PRF-19's grid layout ceiling is 2 ms and PRF-20's grid layout-plus-drawing ceiling is 8 ms. These are different fixtures: the table supports the conservative ceiling proposal, and does not certify those gates. Raw 100,000-point line/area/scatter browser mounts took 6.3–13.4 seconds at 390 px with Chromium 4× CPU and 2× DPR, over unthrottled loopback. This is viewport/CPU emulation, not the full PRF-07 network profile or a physical-phone result.

Adaptive cubic flattening removes length-based oversampling while retaining every command endpoint. At 10,000 observations and 1,280 px, monotone-line mesh storage fell from about 416 MB to 12 MB; the new geometric accuracy checks cover loops and collinear overshoots as well as dense near-straight segments. Reproduce with [the native measurement example](crates/richview-runtime/examples/point-benchmark.rs) and [the browser verifier](scripts/verify-point-modes.mjs). Above-ceiling cases in the final implementation measure the disclosed default representatives and the unlimited explicit-all override; raw-all numbers above were captured before the ceiling was lowered.

## FEA-CH-52 and FEA-CH-53 — contour selection proposal

Target: Charts — show the selected observation without changing data size or erasing names. The proposal replaces boxes, edge highlights, point growth and pie pop-out with a true-shape contour for every contained mark, using the style's existing ink and surface and fading over the existing 120 ms ease with no pulse. Reduced motion lands immediately. This rollout covers OHLC, histogram, treemap, sunburst, scatter, packed-bubble, rose, pie, bar, combo, dumbbell, waffle, lollipop, chord, boxplot, bullet, candlestick, interval, pareto, waterfall, gantt, funnel, violin, dotplot, qq, strip, scatter3d, heatmap, calendar, bin2d, hexbin, pivot, icicle, map, surface, bar3d, parallel, network, contour, gauge and flow; the other families remain for later. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## FEA-CH-52, FEA-CH-53, FEA-CH-40, FEA-CH-42 and FEA-CH-08 — hover that lets go slowly, the rest steps back

Target: Charts — make the hovered mark unmistakable and the release calm. Daniel accepted the 11 November interaction study's recommendations (release in 250 ms, the halo, dim the rest). Hover feedback arrives in 120 ms and releases in 250 ms; nothing moves, so the pie slice keeps its place. A hovered or selected mark, like a legend entry, dims the rest in 150 ms and restores it in 250 ms: other series to 20% on bars, 3-D bars (whose rows are their series; a bar's hairlines dim with it), combos, lollipops and waffles and to 35% on scatters and packed bubbles, other wedges to 20% on pies and roses (a sunburst is a hierarchy and keeps every ring), the other layers of a streamgraph to 20%; in Sankey and alluvial charts the unconnected ribbons go to 10% and the unconnected nodes to 20%, and labels never dim. Lines and areas at the nearest x dim nothing. Point marks, hover dots, pie and rose wedges and bars carry a halo in the mark's own colour at 25%, 4 px wide, painted as one filled band; marks whose neighbours touch them keep the ink glow.

A single mark reads on one line when it has a value and at most one derived value, with the derived value in soft ink. When two or more derived values follow, all rows remain visible, except on Sankey and alluvial marks when all following rows are derived, which show the value alone; the full values remain in the table and announcement. Rows from the author's fields are never dropped. When the pointer leaves, the readout holds 300 ms and fades in 100 ms; a dismissal hides it at once. A wedge's readout sits outside the wedge on its own side and crosses the centre only when there is no room. A chord readout keeps its natural width outside the circle on the selected arc's side, then tries the other side, so it does not cover the selected group's ribbons. Both stay inside the figure; when no place clears the selected wedge or group, FEA-CH-42 decides, and inspection never scrolls the page. Pie and donut names stand beside their slices with no leaders, and the whole chart takes the key when any name does not fit. Reduced motion lands every end state at once. These amendments remain assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## STY-38 — where keyboard focus shows on a chart

Target: Charts — show where keyboard focus is without the ring crossing the chart's text. When a chart takes focus from the keyboard, its ring is 2 px in the style's link colour: the accent in Air, Scientific and Brutalist, ink in Hand-drawn, and in Sleek the accent lightened to read on the dark page. It sits 4 px outside the frame unless text is in the way. Each side keeps 3 px from the chart's text and from the text just above and below the frame, such as the legend and the method note. Where text is flush with the frame, that side moves inside it. Before this change, the report's rule for scrolling regions drew the ring 2 px inside the frame, across the tick labels. When an arrow key selects a mark, the frame's ring goes and the chart draws a 2 px ring 2 px past the mark's contour, with a 2 px gap in the chart's surface colour. It arrives in 120 ms. A line or area gets one ring around each hover dot; a mark of several parts, such as a candle, gets one ring that follows all its parts together, not their bounding box. A 3-D bar partly behind nearer bars gets a ring round each part that shows, which runs 2 px past a nearer bar's edge where it cuts the bar; a bar hidden completely keeps a ring round its whole outline, so the selection still shows. While a 3-D bar is selected the other rows step back and its row is drawn once more over them, so it shows in its own colour (the same for a row emphasised from the legend); its ring goes round its whole silhouette, trimmed only by bars of its own row. With one row, every nearer bar trims as today. Pointer hover never draws this ring, and Escape brings the frame's ring back. The ring may reach past the plot, over the axis labels, as far as it reaches past its mark, so a mark at the plot's edge keeps its whole ring; the halo stays cut 8 px past the plot. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## STY-41 — readout padding

Target: Charts — tighten the readout. The 11 November trial uses 8 px padding for the regular readout; the compact readout keeps 6 px by 8 px. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

Target: Charts — the readout reads value first and stands off the chart (chart design, 16 November, on Daniel's question how the tooltip differs between the styles). A name (series, field or measure) is set in the soft ink and its value in the full ink, at regular weight and in the style's number font; the derived value that may follow a single mark's own is set the same way instead of being dimmed whole. On one row two name and value pairs stand 12 px apart, twice the 6 px between a name and its value. A readout with no shadow takes as its border whichever of `rule` and `ink-faint` contrasts more with the panel's fill, and one with a shadow keeps the hairline: with today's tokens Sleek and Hand-drawn take the axis ink and Air, Scientific and Brutalist do not change. No style file or style vocabulary changes. Assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## STY-30 — a wider reading measure

Target: Reports — fill more of a desktop screen. Daniel said on 15 November 2026 that the report is a bit too narrow and looks odd, while the docs width is fine. Each builtin's measure grows by a fifth: Sleek and Air 42rem (672 px, was 35rem), Scientific 78ch (681 px, was 66ch), Brutalist 38rem (608 px, was 32rem) and Hand-drawn 41rem (656 px, was 34rem). Every block still sits at the measure on one left edge, and paper still sets it at 80%. STY-31's column stays 880 px with its gutters, so a table or a `wide` block breaks out by 76 to 112 px a side instead of 128 to 160, and phones do not change. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## STY-21 and STY-30 — paper uses its page

Target: Reports — a PDF that fills its page instead of leaving the right third empty. Chart design decided on 15 November 2026 that paper mirrors the wide screen. The text column, at the style's measure, sits in the middle of the printable width. Charts, tables and images, each with the source note under it, run from margin to margin, like the page's running footer. A metric is one stat line and keeps the column's left edge with the text; a row of two or more metrics side by side would take the page, and the format has none today. A PDF's charts are laid out at that width, so they print at their own type sizes. A browser's own print cannot lay a chart out again, so there a chart keeps the measure, as drawn, in the centred column. On paper these blocks are wide without `wide: true`; on screen STY-30's one left edge is unchanged. Page breaks do not change. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## FEA-RP-43 — a block that just misses its page is fitted onto it

Target: Reports — no PDF page more than a quarter empty because the next block missed it by a little. Since figures print margin to margin (STY-21 above) they are taller, and the launch full of 15 November 2026 failed its bottom-gap check: in the flagship in Scientific, Brutalist and Hand-drawn a page held only an image, because the chart after it missed the space left by 26 to 43 pt and no order of the text around them fills both pages; in the mixed-media fixture in Scientific a section's heading and picture missed page 1 by 26 pt. Where floating leaves such a page, the block that opens the next page is fitted onto it the way a typesetter saves a page: the space above that block gives up to half, and then a picture (an image or a video poster) on the short page or in that block gives up to a tenth of its height, the least that fills the page. A chart is never scaled, text and its sizes do not change, and nothing changes where this does not fill the page. The bottom-gap check and its accepted section gaps are unchanged. Chart design judges the pages. This amendment remains assumed until the lockdown owner records Daniel’s answer; no accepted answer changes.

## FEA-CH-27 — area names and stable legend fallback

Target: Charts — give the plot its width while keeping series identities readable. The lead’s 6 November ruling names areas at their first visible point on the left, sharing the y-axis margin; lines keep right-end names. Full name buttons avoid axes and one another, with a thin leader after 4 px of vertical displacement. Area displacement stays within half a y-tick spacing; if any name cannot fit within that limit, use the style’s legend.

Decide the mode from the all-shown state at the current width. An all-shown legend stays in every hidden subset. Direct names may fall back only when a hidden state really cannot fit them, with focus and restoration kept on the same series control, including when zoom puts that series’ rows outside the visible window. These placement amendments remain assumed until the lockdown owner records Daniel’s answer. FEA-CH-28’s density/highlight rules and FEA-CH-33’s last-visible guard and value-axis refit are unchanged; no accepted answer changes.

## FEA-CH-28 and FEA-CH-29 — automatic primary values

Target: Charts — useful exact values without obscuring data. The lead’s 6 November ruling enables ordinary bars, large stacked-bar parts, lines, unstacked areas and density; tiny stack parts and the matrix’s dense or ambiguous extras remain off, existing useful numeric passes stay, and bullet/hierarchy/circle additions are deferred. These amendments remain assumed until the lockdown owner records Daniel’s answer. No accepted answer changes.

Use the exact defined primary readout text at author precision. Continuous values share five labels, three below 480 px of plot width. For line and unstacked area, every shown series gets the same point kind: last, then first, then extremes. A kind is used only if every label fits and the remaining budget covers all series; if last values cannot all fit, show none. Density uses one existing recorded sampled evaluation near the peak per group, not an analytic maximum.

Own-bar interiors are allowed only for complete text with at least 4.5:1 contrast and border clearance; Hand-drawn may use a small surface backing over its own hatch. Other marks, text, gridlines, baselines and leaders remain clear, except that a bar's value that does not fit inside its bar stays in the bar's own row or column, 3 px past its value end (a horizontal bar's may slide within the bar's thickness off a grouped sibling, or sit inside the bar at that end when there is no room past it), and may cross a soft gridline, which then breaks 2 px past the value's box, as gridlines and the zero line do round a name above a horizontal bar under 480 px. Unused top-margin space may hold adjacent labels without moving data. Exact observation/series identity survives replay and emphasis; numeric text is never a series toggle. Attempted or budget-excluded values retain drop provenance. No author switch or authored non-Cartesian annotation support is added.

Daniel, 15 November 2026 about 14:30 CET, dictated: “automatic annotations on by default.” The automatic values and the default point names stay on by default, and one author switch replaces “No author switch” above: a chart’s `auto_annotations: false` (TYP-12) draws no automatic value and no default point name and records no drop, since nothing was left out against the author’s wish. Authored `annotations`, `highlight` names and series names stay. The field is accepted on every chart type and changes nothing where a type draws no automatic value or name. The lead, 16 November 2026: `false` means the same everywhere, so it also reaches the values that lollipop, pareto, waterfall, funnel and ribbon charts print themselves; a funnel then gives the value column’s room to its stages. Chart design, 16 November 2026: one meaning everywhere, the chart prints no value the author did not write. So a heatmap prints no cell values and keeps its colour key, and a pie names its slices without their shares; the readout still gives both. This amendment remains assumed until the lockdown owner records Daniel’s answer. No accepted answer changes.

## FEA-CH-28 — automatic values on charts with few rows

Target: Charts — Daniel’s “automatic annotations on by default” (15 November 2026) on the types that printed no value. Chart design ruled per type on 16 November 2026: yes for dumbbell, bullet, the box plot's median, treemap, combo and the last close of candlestick and OHLC; no for histogram and dot plot; and `auto_annotations: false` prints no value the author did not write. This replaces “bullet/hierarchy/circle additions are deferred” above for the types named here. FEA-CH-28’s limits hold: rows share the budget of five, three under 480 px of plot width, and every value is printed or none. A chart over the budget prints none and records each of its values as dropped; a value with no clear place takes the others with it, and each is recorded. The numbers a chart prints or records are therefore the same at every width (DEL-52). Values use the exact readout text, stand inside the plot 2 px clear of every mark and text, and may stand across a soft gridline, which stops short of them.

- **Dumbbell:** both end values of each row, the lesser before its dot and the greater after it. The value axis keeps room for one beyond its first and last dot when the rows are within the budget, never across zero.
- **Bullet:** the measure’s value 4 px past the end of its bar. Where the target mark or a band’s edge would stand within 4 px of the text, the ruling sets it inside the bar’s end and drops it when it does not fit; a bullet’s bar is at most 13 px thick and a line of figures is taller, so today that value has no place. Where the longest bar reaches past every target and band, the axis keeps room for its value.
- **Box plot:** each box’s median beside the box at the median’s height, all on one side: right of an upright box (under a lying one), and on the other side only where that one has no room. No quartiles or whiskers. Horizontal boxes on a narrow chart stand too close for a line of figures between them, so they print none.
- **Treemap:** the value on a second line under the name, in the name’s ink and weight, in the five largest tiles (three under 480 px) where the tile holds both lines. This one is per tile, not all or none: the five largest tiles are the candidates at every width, and one of them that prints no value (the fourth and fifth under 480 px, or a tile that cannot hold both lines) records it as dropped; smaller tiles are not tried.
- **Combo:** the bars’ values and each line’s last value, each in its own unit. The bars’ values stand in one place: all past their bars’ ends when every one of those places is clear, of the line above all; otherwise all inside the foot of their bars on one line, where a value that does not fit its bar or that the line crosses is recorded as dropped. The line’s value is placed first: past its column at the point’s height, else right of the point, above it or below it, never on a bar; lines share the budget and print their last values all or none. A bar’s value is never moved onto the line. No axis keeps extra room, so on a narrow chart, where the last point stands on its bar beside the plot’s edge, the line’s value is recorded as dropped and the bars keep theirs.
- **Candlestick, OHLC:** the last close only, at the close’s own height. Chart design ruled “right of the last candle … dropped when no room”; inside the plot that room is half a period’s gap, which no price fits from about ten periods on, so the value would never print. An upright chart therefore keeps the width of its last close beside the plot and sets the value there, 6 px right of the plot’s edge, as a price tag; `auto_annotations: false` gives the plot that room back. A horizontal chart keeps no room: the value stands under the last row where the plot has room and is otherwise recorded as dropped.

Histogram and dot plot stay without values. These amendments remain assumed until the lockdown owner records Daniel’s answer; no accepted answer changes.

## FEA-CH-07 — the bubble size key in one scale

Daniel, 15 November 2026, on the Gapminder chart’s key (`4,607 · 715 million · 1.43 billion`): “What does this weird annotation mean? 4607 … they should have the same”. The size key writes its three values one way: when the largest value takes a scale word (from 10,000) and the axis sets no `format`, values from 1,000 take one too, so the key reads `4.61 thousand · 715 million · 1.43 billion`. Values below 1,000 and an authored `format` are unchanged, and so are readouts, Table and CSV. This amendment remains assumed until the lockdown owner records Daniel’s answer; no accepted answer changes. Chart design reviewed the key on 15 November 2026 and passed it.

## FEA-CH-32 — a band’s label stays with its band

Target: Charts — a `band` or `range` annotation’s text reads as that band’s. Chart design’s decision of 15 November 2026, on the flagship chart whose three event labels ran together and stood beside other bands: the label lies over its band at the top of the plot. It ends 4 px inside the band’s right edge, or starts 4 px inside its left edge. All labels of one chart hang on the same side, so no label reaches from its own band to the band another label ends at (chart design, 16 November 2026): the side that prints more labels is kept, and on a tie the labels end at their bands. Labels in one row keep 12 px between them; a label with no place in the first row takes the second, and there is no third. A label with no place in either row is omitted and reported as a dropped label, as FEA-CH-32 already says for text without a clear position. The labels of one chart are chosen together so that the most stay. Rows keep clear of bars and data points as before, and also of a drawn line between its points. Bands, their anchors and the other annotation kinds are unchanged. This amendment remains assumed until the lockdown owner records Daniel’s answer; no accepted answer changes.

## FEA-CH-22 — necessary percentage-axis precision

Target: Charts — remove a decimal column that is always zero. The lead’s 7 November grant applies Daniel’s 4 November review by removing only the forced one-decimal minimum on percentage axes. A whole-percentage step reads `0% · 25% · 50%`; a fractional step retains shared precision, such as `0% · 0.5% · 1.0%`, with the report’s locale separators. Readouts, source precision, magnitude selection and Table/CSV retain their existing paths. This proposal amendment remains assumed until the lockdown owner records Daniel’s answer; no accepted answer changes. Design review pending with rv-build.

## PRF-05, PRF-30, PRF-31 and PRF-32 — size budgets advisory until launch

Daniel, 9 November 2026 about 13:15 CET, dictated and relayed by the chief of staff: “the size can go up. We can trim the size of the whole runtime once we're done with the launch, so relax all the size checks.” Size budgets continue to be measured and reported, but overages do not block release until launch. S27-980 tracks trimming and restoring them afterwards. Instruction, memory and timing checks remain binding. This ruling is recorded here pending its lockdown capture; no accepted answer is changed.

## DEL-33, DEL-71 and DEL-74 — affected-only, Chrome-only checks until launch

Daniel, 9 November 2026 about 14:00 CET, dictated and relayed by the chief of staff. Until RichView is live in production:

- No full runs as the default. Run only what a change affects. A map from code paths to test steps, shards and cases decides what a change affects, so every agent runs only the affected tests; unaffected tests are skipped.
- Chrome only: Firefox and WebKit are skipped until launch and come back before it.
- Tests with fewer than 5 recorded failures are paused, unless the map says the change affects them; they are re-enabled after launch.
- Retries are always partial: after a fix, rerun only what failed, never the whole suite.
- The build is cached: compile once and reuse it across worktrees and runs, with nextest-style sharding where it fits.
- 5 parallel slots instead of 2.

The goal is fast fixes reaching production. These are deliberate, temporary relaxations until launch. He also set the priority: “until we nail the visual quality, all the back end work is not that important.” S27-986 and S27-987 build the changes into the gate. Daniel's go for each prod candidate, safety, data truth and the instruction, memory and timing checks remain binding. This ruling is recorded here pending its lockdown capture; no accepted answer is changed.

## STY-71 and STY-72 — your company style

Daniel, 9 November 2026 about 14:05 CET, dictated and relayed by the chief of staff: “push the RichView lockdown agent to, as quickly as possible, show me that the number one style that people are using is 'your company style' … push the agent to develop the company style as fast as possible.” This proposal is pending its lockdown capture; no accepted answer is changed.

A style file may name `company.color`, an optional `company.secondary` and a `company.base` builtin. When the base is omitted, an explicitly named builtin stays in place; otherwise the company style starts on Air. RichView derives tinted surfaces, inks, accent and a six-colour series palette. It keeps the RichView wordmark's `tokens.brand`. If a company colour cannot meet the contrast floor, the accent and text colours move toward a legible shade and an info diagnostic names the input and result. A secondary colour that fails the palette's distinction rule is omitted with a diagnostic. Explicit tokens and palette in the same file take precedence; the final result remains subject to the existing contrast, palette and colour-vision checks. A dark base derives its printed style from its light counterpart. Achromatic inputs keep the base neutrals.

The same `company` mapping may name the company and report-relative `logo` and `logo-dark`. A logo's bytes must be a still PNG, JPEG or WebP, no more than 512 KiB and 1024 px on its longer side; animated APNG and WebP are refused. The company name supplies its alt text; without one it is decorative and a warning asks for a name. The mark appears before the report title and in exported HTML, PDFs and the link-preview thumbnail. On a dark style the dark logo is used if supplied; otherwise the logo has a light plate. The logo work is a separate implementation.

The site Wasm exposes `deriveCompanyStyle` using the same derivation as the report style cascade. A company-style guide and worked example show the one-colour route. This proposal assumes STY-15, STY-16 and STY-53; STY-14 is answered.

## FEA-CH-53 — dimming that reads on dark pages

Target: Charts — keep the rest of the chart readable while one mark is emphasised on a dark page. On a chart page whose background is dark (OKLCH lightness under 0.5, Sleek), the dimmed opacities of the 12 November amendment are multiplied by 1.6: other series and wedges go to 32% instead of 20%, other points to 56% instead of 35%, and unconnected Sankey and alluvial ribbons to 16% instead of 10%. At 20% a dark fill on a near-black page all but disappears, and the reader loses the context the emphasis is set against. Light pages keep the 12 November values. A label inside a mark (a tile's name, a bar's inside value) now dims with its mark, so dark text no longer sits on a faded fill; labels outside marks still never dim. A label belongs to a mark when its whole box lies inside the mark's shape (a ribbon's or wedge's outline, not its bounding box); the smallest such mark owns it. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

# The learn guide for a fresh agent — S27-986

Target: `richview learn` — an agent that has read it writes a correct first draft without outside examples (INT-13, INT-69). FEA-SF-47 is unanswered and is built as proposed: five sections, the same bytes on the CLI and MCP, vocabulary generated from the registry.

## FEA-CH-51 — entrances that land together

Target: Charts — make chart entrances smooth instead of snapping in and then waiting for their labels. Each style keeps its duration and stagger. Data marks enter on the style's own curve only when it starts at rest and never passes its end (Scientific's `standard`, Brutalist's `snap`); Sleek's `out`, Air's `soft` and Hand-drawn's `spring` give way to `move` for marks, so a pie has fanned 61% of the way at 40% of its time instead of 78%, and no bar grows past its value. This replaces the card's "one deceleration curve" with one that eases in briefly. Labels and annotations fade in from the moment the last mark has covered 85% of its travel until it lands, so the entrance ends with them instead of 200 to 500 ms later. A row of bars longer than the 320 ms cap allows spreads its starts evenly instead of starting the rest together. Tiles and cells without a baseline (heatmap, treemap, waffle, bin2d, hexbin and calendar, and the boxes, funnel stages, Gantt bars and icicle cells) grow from their own centre in a wave from the plot's top left instead of fading; a treemap group grows as one with the tiles inside it, and the funnel widens from its axis. Filled points grow from half their radius as they fade in, as MOTION.md already specified; hollow points fade in place. Sweeps keep a hard front on lines; filled areas, streams and flows fade in over the 48 px behind it. From 2,000 marks a chart still fades as a whole. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## FEA-CH-42 — a readout that never covers its mark

Target: Charts — the readout sits beside what it describes, keeps its natural height and moves calmly. It takes the first place that lies inside the visible chart, clears its own mark and that mark's 4 px halo, and covers no label or figure text: past the value end, 8 px away; beside the mark, 12 px to the right, then the left; at the opposite end; in the chart's top or bottom margin. A gauge's readout first tries the half of its hollow away from the needle, clear of the needle, the hub and the value and name in the middle, which count with its mark. The figure text it keeps clear is the title, the subtitle, the toolbar, the legend and the note, each measured on its own. On charts narrower than 480 px it docks below the plot's axis labels, over the chart's note if needed, or else above the plot; a shared readout whose rows name every legend entry beside its swatch stands in for the legend, so either dock may cover it. A dock never covers the title, the subtitle, the toolbar, the axis labels or a legend the readout does not stand in for, and the page never moves. A key drawn in the chart, such as a gauge's bands or a colour scale, counts as a legend. A gauge's readout names its band, so a gauge gives up its key after the subtitle and before its needle. When neither dock fits, a gauge tries its hollow, then the readout takes the plot band farther from the selected mark. When no place clears everything, the readout gives things up in this order: other marks (a line by the length it hides), then chart text, the note, a legend it stands in for, and the subtitle; its own mark, the title and the toolbar go last, and when even its mark cannot be cleared it hides as little of it as possible. Ties go to the place nearest the docks on a narrow chart and nearest the mark otherwise. A readout is never shortened to dodge text; only one taller than the visible chart scrolls, with Page Up and Page Down. A pointer and a keyboard arriving at the same mark from rest put the readout in the same place; while a readout is showing and moves to a neighbour it keeps its side. The keyboard reaches every observation, including parts the chart did not draw and its note names; their readout covers no figure text. Between marks it glides in 180 ms; when the pointer leaves it holds 250 ms and fades with the 250 ms release, replacing the 300 ms hold of the 12 November amendment. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## FEA-CH-52 — readout rows that step back with their series

Target: Charts — a shared readout says which of its rows the chart is emphasising. While one series is emphasised (the streamgraph layer under the pointer or chosen with the arrow keys, or a series whose legend entry or name is hovered), the readout's rows for the other series turn to soft ink over 150 ms; the emphasised series' row and rows that name no series, such as a total, stay in ink, and every swatch keeps its colour. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## Decided here, for Daniel to overrule

- The guide prints every key a block reads, nested ones included, one per line, and each chart type with what it draws. That makes it about twice as long as before, and longer than one tool output in Claude Code (30,000 characters) and Codex. Its first lines tell an agent to read it by section or page it, and a test keeps each section under 30,000 characters.
- The guide states what a reader sees for numbers: three significant digits by default, `format: percent` and a metric's `delta` as fractions, text printed as written.
- The guide and the SQL example state that a connector query's rows are captured once and reused, and how `cache` re-queries. The default itself is unchanged.

# Hosted pages and sharing — S27-1018

## Decided here, for Daniel to overrule

- Amended by "Hosted look — S27-986" below (17 Nov): signed-out pages are now a split sheet, signed-in pages and the delete confirmation carry the app's rail, and the reports list is a table. Signed-out pages (sign in, sign up, check your mail, the mail-link forms, link expired, sign-ups paused, errors) look like the sign-in of a product people know (Daniel, 15 November: the sign-in "does not look professional"): the RichView mark centred at the top, then one card with a centred 28 px heading, a soft lede, the form and one full-width primary button, and the other way on in the card's foot; below it only Privacy, Terms and Impressum, without the site navigation or full footer. "Forgot password?" sits beside the Password label, after the field in tab order. The card is the page's one floating sheet, with STY-48's soft shadow and STY-66's panel radius. Signed-in pages and the delete confirmation keep the site chrome.
- The site header ends with a quiet "Sign in" to app.richview.ai, after the GitHub mark and a hairline, in the ink colour (Daniel, 15 November: "the main site needs a login link somewhere"). This amends FEA-SF-28; the header still has no sign-up button (FEA-SF-31).
- Notices have three looks, decided from their text so no route changes: success (green wash), information (blue wash) and error (clay wash, `role="alert"`); any text not on the success or information list reads as an error.
- Who can open a report is one badge with four looks wherever a report is named: Private (neutral), the organization (blue), Public link (the warning colour), and "Shared with you" (green) for a reader who has it by name, who never sees "Private".
- "Who can open it" is three cards, each its own submit button for the same form field; Public's button is in the warning colour and the current card says "Current setting".
- Lists of reports, people, members, versions and tokens are rows in one rounded hairline panel at every width, instead of tables that restack into cards on a phone.
- The reports list shows usage against the 50 report and 50 MiB allowance as two thin meters in the page head.
- Destructive actions sit in one bordered zone that names what goes; the delete confirmation lists it and offers "Keep it".
- Share and invitation mails get an HTML part built around the report's title and description with one button; the plain-text part keeps the same words and the one link.
- A public link's bar on the report host names no person and no organization.
- Sign-up names a new author's first organization "{name}’s organization" (cut to fit the 80-character limit a rename has), so no page or mail offers to share with a person by their own name; organizations made before keep their names, and the owner renames it on the members page. The lead approved it on 14 Nov 2026 after the end-to-end pass.
- The bar on the report host takes the report's own page, ink, rule, link and interface font, its page tinted 5% towards its ink, so it belongs to a dark report as much as a light one; the RichView mark is drawn in the report's link colour.
- Under 640px the bar stays one 44px line: the clauses after the first ("anyone with the link can open it", the count of people, the organization) give way, "Find it again under Shared with me" shortens to "Find it again", and a long name ends in an ellipsis.

## Hosted abuse limits — pending review

- Each address may make 120 API requests per minute with a burst of 30; upload and sign-up, forgot and sign-in request bodies have a 120-second total deadline, and four uploads may be in flight globally.
- Half of the daily mail budget and half of each 10,000-entry daily mail-history capacity are reserved for existing-account resets, invitations and shares.
- A person may create one organization, with the operator able to raise that limit; legacy accounts without a creation record conservatively count as one creation. Deletion removes the count, and re-registering starts a new sign-up without retaining an identity tombstone. Capacity alerts start at 50% of the account and store limits, sampled once a minute and reported in the operator summary and host log.
- Each address may make 300 report requests per minute with a burst of 60 and 32 concurrent connections. The service holds four page reads at once and admits up to 32 waiting readers for two seconds; overflow or timeout gets 503 with a one-second retry hint. Two shared download/file permits are held through response-body delivery.

# Reading aids and a shorter flagship — S27-986

Target: Reports — a long report gives its reader a map and a sense of how far they are, and the flagship sells RichView in half the length. Daniel asked on 9 November (annotation d0b7e9ae) for a table of contents on the right that follows the scroll and a small colourful progress bar at the bottom, both on by default, and for a shorter flagship; on 15 November (annotation b755dbfc) he asked again and wanted the flagship at the top of the docs' reports. The full card texts are in the stream's report. No accepted answer changes.

## FEA-RP-63 — a table of contents (new card)

A report with three or more top-level sections lists them by default; `toc: false` turns it off. From 1100 px the list sits in a rail right of the body that stays in view and marks the section being read with a dot in ink; below, it folds under the header as a closed "Contents" disclosure. Plain links, so it works without scripts. A tabbed report, a PDF and paper have none; docs pages keep their own column.

## FEA-RP-64 — a reading-progress bar (new card)

Every report carries a 3 px bar along the bottom of the window, filled with the share of the page scrolled; `progress: false` turns it off. The read part is in the style's link ink over a track in its soft hairline colour, always quieter than the read part; with contents, a 2 px gap in the page colour marks where each section starts. One colour, because in a chart report the palette means series (Chart design, 15 November). No gradient (STY-34); hidden from assistive technology; empty without scripts; absent on paper.

## INT-10 — the flagship, shortened

The flagship keeps its 2 October 2026 snapshot, wording rules and sources, and is cut from 13 sections and 19 charts to 9 and 9. It opens with a four-metric scorecard and the population chart, and Sources cited folds after eight rows.

# Less ink on highlight — S27-986

Target: Charts — a highlighted mark carries less ink. Daniel asked on 15 November for "less ink on highlight, no black outline (scatter, box plot)"; Chart design ruled it and judged it from sheets in three styles: every mark that carries a halo drops its ink outline and the page-coloured separator and keeps the halo with its gap; the tiled families, links, the line chart's hover dot, bar3d and the keyboard ring keep their treatment. No accepted answer changes.

## FEA-CH-53 — a haloed mark answers with its halo alone (replacement)

A mark that carries a halo answers hover and selection with the halo alone, with no ink outline; a hovered scatter point grows by a quarter, in its own fill over a disc of the page, in front of whatever crossed it. bar3d, tiled families and links keep their outline.

The feedback check `verify-chart-feedback` "the smallest scatter point keeps its fill colour" reads the pixel half a radius up and left of the point's centre instead of the centre (the lead's grant, 16 November). Its fixture's smallest point lies on the origin, where the axis line crosses the centre at rest and the hovered point now comes in front of that line; the assertion is unchanged and still fails when a hover recolours or covers the point's fill. Daniel may overrule either the bring-to-front or the moved sample point.

# Chart ink and a gallery that invites its interactions — S27-986

Target: Charts — a chart carries less ink, and a visitor to the chart gallery finds and tries what a chart can do. Daniel's chart feedback of 15 November asked for thinner dense lines, a visible and discoverable drag-zoom, labels that stay the same when zoomed, less ink on highlight and a play chart that runs year by year without blinking names; the same day he said "the gallery should show off the interaction features more". Chart design judged each change from captures. No accepted answer changes.

## FEA-SF-43 — the gallery opens with charts to try (addition)

Between the flagship chart and the featured menu, `/docs/charts` has a section "Try it yourself" with two live charts at the column's width: one that plays and one that zooms. A line above each, at most 12 px from the chart's frame, in the soft ink at the small size, names the gesture, the way back and the phone equivalent; on a phone zoom needs the expanded view (FEA-CH-49). The play chart never starts by itself: it opens on its last year (FEA-BR-02) and shows the 30 most populous countries of that year, says so in its subtitle, and the link to every country stands under it. On the gallery it is 3:2, and the zoom chart prints no automatic values. The cards of the featured examples that play or zoom name their gesture. The section's charts mount on approach like every chart of the page, and its observations stay under 120 kB so the page keeps loading on a phone.

## STY-40 / FEA-CH-02 — data lines thin as they pack (addition)

Data lines are 2.5 px, thinning to 1.25 px as a series packs from a quarter to one observation per pixel.

## FEA-CH-49 — the drag shows its window (addition)

While dragging, the selected window is shaded on every zoomable chart; a quiet "Drag to zoom" hint shows on hover and adds "Shift-drag to pan" when zoomed.

## FEA-CH-28 — a line names its own values (replacement of the label rule for one series)

A line's values name the series' own last, first, lowest and highest observations wherever they are in view, never a zoom window's extremes. On a single line a kind with no room no longer cancels the others; a crowded lowest or highest looks further away, and a value placed beyond 4 px of its point takes a leader. Two or more series keep the earlier rule.

## FEA-BR-02 / FEA-BR-31 — playback glides and keeps its names (addition)

While playing, marks glide linearly from frame to frame, and names in both frames travel with their marks. The names placed in the opening frame stay on every frame, with a leader when their spot is taken.

## FEA-CH-28 — a line's values keep clear of each other (addition)

On a single line, a value's leader never crosses another value, a value never sits on another value's leader, and two values side by side keep 8 px between them; a value with no such place looks further away or is left out (Chart design's gallery ruling, 16 November).

## FEA-CH-28 / FEA-BR-31 — a point's name never stands behind another point (addition)

A scatter point's name takes no place with another point on the straight way between the point and the name, so the name cannot read as the neighbour's; a point that covers the named point itself does not count. On a leader such a place comes first; in a crowd that leaves none, the leader may pass a neighbour, so a playback's kept name still shows. A name's leader ends at the side of the name that faces its point and never runs through the name; it crosses no name already placed, and a later name keeps off it (Chart design's gallery ruling, 16 November).

# The command line reads a folder of reports — S27-986

Daniel's review of 16 November: "Point the CLI at a folder, not only at one report." Lead ruling of 16 November; FEA-SF-01, TYP-68 and TYP-69 are unanswered.

## FEA-SF-01 / TYP-68 / TYP-69 — a folder of report folders stands for every report below it (addition)

`validate`, `check` and `generate` take a folder of report folders and run every report below it in path order (a folder holding a `.rmd` is a report folder and is not entered further; hidden and symlinked folders are skipped). The result is one object: `ok` and `status` for all, `output` null, `diagnostics` empty and `reports`, each report's own object plus the `input` it read; the text form leads each finding and result line with the report's path and ends with the count. Exit 1 when any report has an error, 2 for a wrong call. MCP `inputPath` reads a folder the same way on `richview_validate`, `richview_check` and `richview_render`. `serve`, `publish` and `richview_publish` take one report and name the reports a folder holds.

## FEA-CH-04 — an area over categories runs edge to edge (addition)

An area chart whose x axis is a list of categories puts its first category on the plot's left edge and its last on the right edge, a step apart, as an area over dates already does; it no longer keeps half a band empty on each side. Bars, and every other chart that shares a bar's category bands, are unchanged. A category name at an edge leans inward to stay inside the figure (Daniel, 16 November: "weird that it doesn't go all the way to the right on the area chart").

## FEA-CH-28 — a line's last value stays when the end is contested (addition)

On a single line whose lowest or highest point lies so close to its last point that the two values, each set over its own point, would run into each other, the last value may stand off on a leader, as an extreme may, and is placed first; when the last value and that extreme cannot both stand, the last value stays at every width. Where the extremes lie away from the line's end nothing changes: a last value with no place beside its point is left out and the extremes show (Chart design's gallery ruling, 16 November).

## FEA-CH-34 — the count key of hexbin and 2-D bin charts (extension)

Daniel, 16 November 2026, on the hexbin and 2-D bin charts of the docs: “it would make sense to have values and drop the C bar … just have values inside and drop the C bar. The C bar is nice for like offline charts, but I would mostly drop it”. FEA-CH-34's answer for heatmaps, a key only when some values cannot be printed, extends to the two charts of counts: each cell prints its count in the style's ink or its surface, whichever reads better on the cell, and a chart that prints every count draws no key. This one is all or none: when one cell cannot hold its count, none is printed, so the fullest cells are never the blank ones; the key comes back and each count is recorded as dropped, so every width carries the same numbers (DEL-52). `auto_annotations: false` asks for the key instead of the counts and records nothing. A hexbin's plot grows to hold every drawn hexagon whole; hexagons at the edge of the data are no longer cut at the plot, because a cut one could not carry its count. This amendment remains assumed until the lockdown owner records Daniel’s answer; no accepted answer changes.

# The second released build is 3.0.0-alpha.1 — S27-986

Target: Delivery — a released build can be told from the one before it by its version, so new archives can be published beside the old ones. Lead, 16 Nov 2026, for Daniel to change.

## INT-41 — each released build carries its own version (addition)

INT-41 publishes builds from `prod` and names no version for them. The first public release, of 16 Nov 2026, is `v3.0.0-alpha.0` and its tag is fixed by the public repository's ruleset, so a later build with the same version cannot be released beside it. The workspace version therefore moves to `3.0.0-alpha.1` for the next release, in `Cargo.toml` (every workspace crate takes it), `Cargo.lock`, `package.json` and `package-lock.json`. `richview --version`, the generator tag of a rendered report and the server's version read it from the build. The VS Code extension keeps its own version (`packages/vscode/package.json`, already `3.0.0-alpha.1`), and the chart engine its own (`0.1.0`). Statements in the guide and the evidence files that record what an earlier build printed keep the number they saw. How versions advance after this one is not decided here.

# The third released build is 3.0.0-alpha.2 — S27-986

Target: Delivery — the release after `v3.0.0-alpha.1` (17 Nov 2026) can be published beside it. Lead, 18 Nov 2026, for Daniel to change.

## INT-41 — each released build carries its own version (second step)

`v3.0.0-alpha.1`'s tag is fixed like alpha.0's, so the next release's build calls itself `3.0.0-alpha.2`, in the same places as before: `Cargo.toml` (every workspace crate), `Cargo.lock`, `package.json` and `package-lock.json`. The VS Code extension, which was already `3.0.0-alpha.1` when the workspace caught up with it, now moves with the workspace to `3.0.0-alpha.2` (`packages/vscode/package.json`, its lock entry and the version its packaging check expects), so one release names one version. The chart engine keeps `0.1.0`, and records of what earlier builds printed keep their numbers. How versions advance after this one is still not decided here.

# A Windows preview of the command line — S27-986

Daniel, 16 November, on the open question of a Windows binary: "make windows happen using aws windows machine". ARC-67 is unanswered and its proposal says "Windows stays unsupported" and "a platform is added only together with a gate that runs on it". No Windows gate host exists, so this is built as a weaker class, a preview, and stays assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## ARC-67 — Windows x64 as a preview (replacement of "Windows stays unsupported")

RichView supports macOS on arm64 and Linux on x64. Windows 10 and 11 on x64 (`x86_64-pc-windows-msvc`) is a preview: the CLI with its MCP server and local preview, and the editor service for the extension. On Windows, PDF output and SQL data sources are not available yet and each says so in one diagnostic; the report server for hosting is not offered there. The binaries are unsigned. Windows is proven for each release by a recorded smoke run of the archive on a Windows x64 machine: install with the PowerShell script, `richview --version`, generate and check the flagship report, start the MCP server, uninstall. A supported platform is still added only together with a gate that runs on it; a preview platform needs the recorded smoke run.

## INT-71 — a PowerShell installer beside the shell script (addition)

The Windows archive is `richview-x86_64-pc-windows-msvc.tar.gz`, unpacked by the `tar.exe` Windows ships, with `bin/richview.exe`, `bin/richview-sql-worker.exe` and `uninstall.ps1`. `install.ps1` installs and uninstalls as `install.sh` does: a stamped archive digest, checked archive paths and file checksums, an ownership record, a new folder of its own (`.richview` in the user's home folder) and no change to PATH, services or the registry. A release is assembled and verified with or without the Windows archive. The install page names Windows only from the release that carries the archive.

## ARC-68 — the dependency gate covers the Windows target (addition)

The licence, advisory and source checks and the third-party notices cover `x86_64-pc-windows-msvc` beside the other targets, so one notice file is the same in every native archive.

# Every chart that can zoom does, and says so — S27-986

Target: Charts — a reader finds the same zoom, by the same gestures and behind the same hint, on every chart where a closer look helps. Daniel, 16 November, in the chart gallery on pre-prod: "this cool thing which says drag to zoom, I cannot see it on other charts where I can drag to zoom"; "the area chart, I would expect I can also drag to zoom"; "zooming in a map chart would be quite important"; "on scatter plot chart, you want to be able to zoom on both X and Y axis, but this is not launch needed". FEA-CH-49 and FEA-CH-39 are unanswered, so these additions extend their proposals; Chart design judged each from captures. No accepted answer changes.

## FEA-CH-49 / FEA-CH-39 — a map zooms (addition)

A map zooms along both axes by the gestures of every other chart: a drag across the plot draws a rectangle and zooms to it, Shift-drag pans in any direction, Ctrl/⌘ + wheel and + and − zoom around the pointer, and double-click, Escape or **Reset zoom** restores the whole map; on touch, pinch and drag work in the expanded view only. The rectangle is fitted whole into the plot, so the map keeps its shape; a reader zooms in at most 24 times, where the shipped boundaries still read as outlines, and the window stops at the map's edge. Countries and points a zoom moves out of the plot are neither drawn nor offered by readouts and the keyboard; the table and the CSV keep every row. FEA-CH-39's table gets the row "map: region or point; legend toggles; zoom along x and y".

## FEA-CH-49 / FEA-CH-39 — a line or an area over names zooms (addition)

A line or an area chart whose x axis holds three names or more (weekdays, months written as words, stages) zooms along x like one over numbers or dates, by the same gestures; over two names it does not zoom and shows no hint. Each end of a settled window stands on a name at the plot's edge, so no name's band is cut: when a drag ends, both ends move outward to the nearest names and the glide ends there; the drag band follows the pointer freely while dragging; pan, + and −, and Ctrl/⌘ + wheel step by whole names. At least two names stay in view, and zooming out stops at the whole axis. Bars, lollipops and every other chart over names keep every name in view (Chart design, 16 November, 23:33). This replaces the proposal's "a numeric or calendar x" for line and area, which `scripts/verify-browser.mjs` asserted as "a nominal x axis never zooms"; that check now asserts this rule both ways, with the old assertion kept for two names and for bars and lollipops (the lead's ruling, 16 November, 23:32). Proposed card text for FEA-CH-49: "Line, area and scatter charts with a numeric or calendar x, and line and area charts over three names or more, zoom along x only; over names each end of a settled window stands on a name and at least two stay in view." FEA-CH-39's table: line, area zoom "x, when x is numeric or calendar, or holds three names or more". A zoomed area therefore still runs from edge to edge of its plot, as it does unzoomed. A chart with too few names for a narrower window (two for a line, three for an area) does not zoom. Bars keep every category in view. The overview strip (FEA-CH-50) stays with numbers and dates. FEA-CH-39's table reads "x" for line and area without the condition.

## FEA-CH-49 — the hint shows on every chart that zooms (replacement of "hidden wherever chart text sits in its place")

The quiet "Drag to zoom" line showed only on a chart with a y axis header, whose row above the plot it shares; every other zooming chart said nothing. It now takes the first of three places that is free (Chart design, 16 November): the right end of the row above the plot, as before; else the title row, against the action icons on their centre line, when the title's text ends at least 12 px before it; else a corner inside the plot, 6 px in, on the chart's own paper, the first of top right, top left, bottom right and bottom left whose paper touches no mark and no chart text of the chart at rest (grid lines may pass under it). A chart with none of these says nothing rather than cover data.

## FEA-CH-49 / FEA-CH-39 — the charts over a continuous x axis zoom (addition)

Band, density, ECDF, gantt, Q–Q, streamgraph and vector charts zoom along x by the gestures of line, area and scatter charts; their pages already said so (all but Q–Q's), their layouts drew a window, and the browser refused it. Histogram, 2D bins and hexbin stay whole, because their bins are the data. A contour and a dot plot stay whole until a zoom along both axes exists: they draw x and y to one scale, so a window along x changes the chart's shape; the contour page no longer promises a zoom. FEA-CH-39's table gets "x" for these seven types.

## FEA-CH-49 / FEA-CH-43 — nothing moves when a zoom starts (addition)

A chart a mouse can zoom keeps **Reset zoom**'s place in the action row from the start, hidden until the chart is zoomed; the static page reserves the same place, by the same rule from the engine, so the page is the same with and without the runtime (ARC-05), so the title beside the icons does not wrap anew and the plot stays under the pointer; a chart that cannot zoom, and a touch screen, keep no place. The hint takes the place that holds its longer line, "Drag to zoom · Shift-drag to pan", and keeps it at rest and zoomed; inside a plot it may change corner when the chart's own ink moves into it, as on a map (Chart design, 16 November).

## FEA-CH-27 — series are named inside the plot (replacement of where the names stand)

Target: Charts — the plot takes the full width, and a name reads as its own line's. Daniel asked on 16 November 2026 for series names inside the chart or above their lines, with the streamgraph's names as the model, and said of an area chart's names in the value axis' margin ("free", "paid") that they do not read as names. Chart design ruled the placement the same day. This replaces the 6 November placement above; its rules for the all-shown mode and the legend fallback stay.

With two or more series, each line is named inside the plot, 5 px above its own line, and below only where above has no place; each area is named inside its own fill, below its top line. A name takes the place furthest right that is free, so the names read as a loose column near the line ends. A place is free when the name is clear of every other line by at least 4 px, nearer to its own line than to any other, on a stretch where its line is not steep, 8 px beside and clear of every value and annotation, and its 24 px button target is clear of every other name's. A gridline stops short of a name. All or nothing: when any name has no such place, no name stands inside. Names inside never cost a value: values and annotations are placed first.

When the names cannot all stand inside, lines are named in a column at their right ends as before, unless that column would be more than a quarter as wide as the plot beside it or the names cannot fit it; then, and for areas always, the names go to the legend. No name stands in the value axis' margin.

A hidden series keeps its name inside the plot, dimmed, where its data is: right of its line's end when the refitted value axis still shows that end, and against the plot's edge on its data's side, at the right, when it does not; never on another series' line. While a series is hidden a name may stand on a steep stretch of its line, so hiding a series does not move the names out of the plot.

Each frame of a zoom would place the names afresh, so they would jump. The names inside the plot fade out over 100 ms when a zoom glide starts (at once when the window jumps), stay away while the window moves, and fade in over 150 ms on the settled view once it has rested 120 ms. With reduced motion they move with the view at once (chart design, 16 November).

# Hosted look — S27-986

Target: Hosted app — sign-in and the pages after it read as production self-service BI software. Daniel, 17 November 2026: "the server login needs to look like a true ssbi production software using extreme modern design language, now the page after looks fairly cheap. Imagine datbaricks/pwoerbi/tawblue --> this is what we strive for". The lead chose direction B (the library) from the three on the stream's sheet on 17 November; Daniel's pick of the sheet is pending through the chief of staff. No accepted answer changes; the lockdown does not yet decide the app's look, so this proposes a card for part 06 (DEL-06).

## Proposed card: the app's look (part 06)

The app's pages (the signed-out pages, reports, one report, members, CLI tokens, account and the delete confirmation; hosted and self-hosted Cowork alike) are an application surface, not site pages, so STY-46's Newsreader headlines and STY-47's light-only rule govern the site and not them. They use the site's paper, ink and brand blue and Geist alone, on a fixed scale (12, 13, 14, 16 and 24 px; tabular figures), with:

- a warm-ink navigation rail (the site's ink, `#1f1c19`) holding the mark in white (STY-03), the organization with its initial tile, role and switcher, the pages with their icons and Docs, and the person with Sign out at its foot; under 960 px it becomes a band with the pages as tabs;
- a top bar with where the page is (organization / page) and a search of report titles;
- functional stroke icons only (16 px, 1.5 px stroke, drawn for RichView) in navigation, actions and who-can-open labels, never as ornament (STY-50 unchanged);
- 4 px controls, 12 px panels and pills for labels (STY-66); a shadow only on what floats, the organization menu (STY-37);
- no site header or footer once signed in.

The reports list is one panel: the filters as a segmented control in its head, then a table with fixed columns (name with the version's share-card thumbnail and description, who can open it, owner, updated with version and size, actions), newest first. Under 1,240 px the owner column goes; under 960 px each report is one card of its own, whose title opens the report and covers the card, with Copy link and Share behind one '⋯' menu at its top right (a details element, so it opens without script). A thumbnail is the version's kept share card (ARC-69) shrunk on the server to 120 × 63 and carried in the page as a `data:` image, so the app's CSP is unchanged; the list shows only reports the person may open, so it adds no access surface. A version without a card shows the report icon.

Signed-out pages are a split sheet: on the ink side the mark, the flagship report's share card as RichView drew it, with the line "A report’s share card, drawn by RichView in the report’s own style.", three facts (private until you share them; owner, admin, author and reader roles; every version kept at its own address) and the legal links; on the paper side the form, flat. A phone stacks the mark's band, the form, the card, the facts and the legal links. A signed-out page without a form (an error, a mail to check) keeps only the mark's ink band above its message, which sits in one panel like the delete confirmation's, and the legal links below it.

Who can open a report is one list of three rows (private, the organization, a public link), each with its label, what it means and its button, and the current one marked by a rule in its colour; forms and secondary buttons use the app's control height and hairline borders.

An invitation's link opens the same split sheet, but its ink side holds the invitation instead of the flagship's card and the facts: a paper card with the sender's initials and name, what they did ("shared a report with you", "invited you to join"), and what it opens (the report's title and its organization, or the organization's name). It shows only what the page's own sentence already says. A phone shows the card above the form.

The bar above a hosted report shows whom the report comes from as a face after the mark: the owner's initials in a circle for a report shared with you, the organization's initial in a square for one you open as a member. Its ways on (Find it again, All reports, Sign in) are buttons with the app's 4 px corners, and the author's Share is the filled one. Its words, its links and its one 44 px line on a phone are unchanged.

A report's page and its delete confirmation end the top bar's crumb with the report's title (organization / Reports / title), cut off with an ellipsis when long.

Usage meters are drawn as the mark's bars: a square start and a rounded end.

## Decided here, for Daniel to overrule

- The dark rail. Most reference products use a light grey rail (Databricks 2023 and Linear use dark ones); the rail's colour is one token, so a light rail is a one-line change.
- The split-screen sign-in; every reference product signs in on one centred panel. Direction A on the sheet is that panel.
- The reports list's count line ("9 reports you can open") and "Newest first" are new words; the list's order is unchanged.
- An invitation shows the report's title, not its share card: the card would show the report's content to whoever holds the link before they have an account.
- On a phone a report's title opens the report itself, while on a desktop it opens its details and the row's Open button opens the report; the lead asked for the phone card on 17 November.

# Link cards and a gallery of pictures — S27-986

Target: Sharing — a shared report unfurls as a card that looks like the report, and the docs gallery shows its charts at once. Daniel asked on 16 November that "when they share a link, I want the preview to look as cool as this does", and for the gallery's cards without live frames. These proposals stay assumed until the lockdown owner records his answers. No accepted answer is changed here.

## STY-70 / FEA-SF-55 — a link card is the top of the report (proposal)

A report's 1200 × 630 link card shows the report as it opens: its title, 58 px on at most two lines or else 48 px and never cut, over its first chart drawn whole, then the byline and a small RichView mark, on the report's own paper, inks and accent. A faceted chart shows its first row of panels, in its controls' first state. A report with no chart, or with a title too long to leave the chart room, keeps the title card. A company style's mark sits on its white plate above the title. The same drawing serves a hosted report's card, the app's local preview and each docs chart and report page's own og:image (`/docs/charts/<name>.share.png`, `/docs/gallery/<name>/share.png`); chart pages are drawn in Scientific, and the home page keeps its image.

## ARC-69 — the latest card is drawn in today's design (amendment)

A version's card is still drawn and kept beside it when it is stored, and its versioned address keeps serving that kept card, as promised. The reports list's small cards and the latest address `/r/<slug>/share.png` instead draw the card from the stored report in today's design on request, held in the server's memory and keyed by report, version and card design, so one list never mixes two looks and nothing new is written into the store (the lead, 17 November). The list waits at most 200 ms for cards it has not drawn; a card not ready by then shows the placeholder, never an older design, and is drawn for the next view.

## FEA-SF-43 / STY-45 — the gallery shows pictures (amendment)

The docs gallery's report cards, the Featured charts, the elements overview and the Reports page's larger previews show a picture the site build draws from the same source in the card's style, instead of a live frame; a click opens the live report. The elements overview's Type card, which is not a chart, stays a scaled live frame. A picture's alt text is the title of the chart it shows. The live frames' loading glyph and its motion are gone.

# Printed values keep the label's digits, and scale words follow the locale — S27-986

Okurka's charts set `digits: 3` and `readout_digits: 6`, and their printed values showed six digits, because a value on its mark printed its readout's text. Lead's approval of 17 November, 03:22, and ruling of 03:30: printed values at the label's digits, readouts at `readout_digits`, and one formatter whose scale words follow the locale for printed values, readouts and the Table view. No card decides the scale words; this proposal stands until the lockdown owner records Daniel's answer.

## FEA-CH-37 — a printed value never shows the readout's extra digits (clarification)

An automatic value printed on its mark (a bar's, a line's or area's, a combo's, a treemap tile's, a bullet's measure, a dumbbell's ends, a financial chart's last close, a box plot's median, a density's peak, a gauge's centre) shows the label's digits: the axis's `digits`, else the chart's, three by default, with whole numbers below 10,000 printed whole (FEA-CH-22). Its readout shows `readout_digits`. A chart with one precision prints what its readout says, as before.

## FEA-CH-37 / TYP-08 — scale words follow the report's locale (addition)

A compact value (from 10,000, or from 1,000 under `format: number_compact`) takes the scale words of its axis's locale, else the report's, in printed values, readouts and the Table view alike, so one chart never shows two forms: English `thousand`, `million`, `billion`, `trillion`; Czech `tis.`, `mil.`, `mld.`, `bil.` (`280,5 mil. Kč`); German `Tsd.`, `Mio.`, `Mrd.`, `Bio.` (`280,5 Mio.`); Japanese keeps the English words for now. Beyond the last word a value turns scientific, as before. Axis ticks keep K and M in every locale (FEA-CH-22). For 10^9 and 10^12, Czech ticks write `mld.` and `bil.` and German ticks `Mrd.` and `Bio.` (`3 mld. Kč`), because the Czech bilion and the German Billion are 10^12 and `3B Kč` invites a thousandfold misreading; other locales keep B and T. Magnitude units that scale themselves (`K`, `M`, `KBD`) keep their ladder. CSV keeps plain numbers (Chart design's proposal, lead's acceptance, 17 November, 05:03).

Proposal for Daniel, not built: Japanese values in myriads, `万`, `億` and `兆`, steps of 10,000 written after the number without a space (`2.81億`), with values below a myriad printed whole. It changes the scale steps, not only the words, and no reader uses ja-JP yet (lead, 17 November, 03:39).

# Members and smooth publishing — S27-986

Target: the members area at the Hosted look's production bar, its phone design awesome rather than merely fitting, and publishing smooth from an author's first `richview publish` to a colleague opening the link (Daniel, 17 November, through the chief of staff). No accepted answer changes. This extends the proposed card "the app's look" under "Hosted look — S27-986" above (DEL-06).

## Proposed addition to the app's look (part 06)

The members page is one panel in the reports list's language. Owners and admins invite from a bar in its head: the address, the role as segments (reader first) whose meaning shows beneath, and one primary button. A table follows with each person's initials, name and address, their role, how many of the organization's reports they own (owners and admins only), the day they joined and a ⋯ menu; open invitations follow under "Invited" in the same table, with who invited them and until when, and Send again and Cancel in the row. A role is a chip whose menu lists the roles the person looking may give, each with one line on what it can do and the current one ticked; one click on another saves it. Removing a member waits in ⋯ behind a confirmation that says what happens: access ends at once, their reports pass to the person removing them and their CLI tokens for the organization stop working. "Organization settings" in the page head leads to the organization's section on the same page: its name (owners rename it in place), its usage (for those who publish, as on the reports list) and leaving it, whose confirmation names who takes the person's reports, the longest-standing other owner, or says that an only owner cannot leave. On a phone each person is one row with 44 px targets, and the role and ⋯ menus are sheets at the bottom of the screen over a dimmed page that a tap outside closes. Every menu is a details element and every choice a submit button, so the page works without script.

A person in no organization who has reports shared with them reads them first: starting an organization of their own waits behind one line under the list, and the line goes while an invitation to join waits.

The command line's publish result names the report's page in the app for a hosted address, where people are named and who can open it is chosen: a `Share:` line under Visibility, and `share` in the JSON result (additive; the MCP tool's description names it). On a terminal it says "Publishing <report> to <server>…" before the upload. A version that exists with other content is answered with the next patch to write (`version: 0.0.2`).

Someone who makes an account from a share mail lands on the report itself: the report's page in the app, where the form's redirect has to stay under `form-action 'self'`, sends the browser on with an HTTP refresh to `/r/<key>`, whose handoff to the report origin already exists. No route or CSP change. The first Reports page tells an owner whose organization still carries its sign-up name ("Jana Novak’s organization") where to rename it.

## Decided here, for Daniel to overrule

- One page for members, invitations and the organization's settings, with the settings one click away by an anchor, rather than a Settings page of its own (a route) or tabs.
- Removal and leaving confirm in place, inside the menu or the settings row, not on a page of their own (a route).
- The report counts, the join day and usage on the members page are new facts there; nothing new is shown to someone who could not see it before (counts only to owners and admins, usage only to those who publish).
- The invitation's hint gives the link's 24 hours, as its mail does; it said an hour.
- The lead's GO of 18 November built the forward to the report, `share` in the publish JSON and the rename line; asking for the organization's name at sign-up waits for Daniel's open questions on the sign-up flow.
