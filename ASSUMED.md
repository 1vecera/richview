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

Daniel, 22 November 2026, asked for the same width in all five styles. The five builtins use a shared 42rem measure. The 880 px report column, phone gutters and wide blocks retain their existing dimensions. An ordinary chart's outer block shares the paragraph edges. Sleek's card, Brutalist's border and Hand-drawn's paper keep their inner padding. Air and Scientific use the full measure for the title, toolbar and plot. STY-09, STY-11, STY-12 and STY-13 use the measure set by STY-30.

## STY-21 and STY-30 — paper uses its page

Target: Reports — a PDF that fills its page instead of leaving the right third empty. Chart design decided on 15 November 2026 that paper mirrors the wide screen. The text column, at the style's measure, sits in the middle of the printable width. Charts, tables and images, each with the source note under it, run from margin to margin, like the page's running footer. A metric is one stat line and keeps the column's left edge with the text; a row of two or more metrics side by side would take the page, and the format has none today. A PDF's charts are laid out at that width, so they print at their own type sizes. A browser's own print cannot lay a chart out again, so there a chart keeps the measure, as drawn, in the centred column. On paper these blocks are wide without `wide: true`; on screen STY-30's one left edge is unchanged. Page breaks do not change. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

The shared 42rem measure applies on screen. Scientific keeps its 78ch paper measure, and Hand-drawn keeps its 41rem measure before the print type scale. Paper figures still use the printable width.

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

## FEA-CH-29 / FEA-CH-32 — the author's annotations come before every label the chart adds (addition)

Daniel, 4 November 2026, on the gallery: annotations "should be on by default across a lot of charts, and they should avoid stuff". The lead read it as: an author's annotation takes its place first, and approved this text on 18 November 2026. It stays assumed until the lockdown owner records Daniel's answer; no accepted answer changes.

Proposed addition to FEA-CH-29: The priority order is the required text first, then the author's annotation text, then the author's highlight names, then the labels a chart adds by itself: its point names, its values and the series names inside the plot. An author's annotation stands where it would on a chart with none of those; a label the chart adds moves or is left out and recorded, never the author's.

Proposed addition to FEA-CH-32: A note also keeps clear of the drawn lines wherever a place clear of them exists; where the lines leave none it still stands, clear of the points, the bars and the other text. A note keeps off every other rule and ring the author drew, whichever is written first; where the other rules leave no clear place it stands across a rule, which beats leaving the author's text out, but never across a ring or other text. A horizontal rule's label sits above its rule at the plot's right edge, else below it, else at the left edge; where bars or lines take those places it stands at the nearest clear place round the rule's right end, never on the rule. The point names and values placed afterwards keep off an annotation's ring and rule as off its text. A highlight's leader starts inside its own point's ring and may cross a rule, but never another ring or an annotation's text.

Not in this addition: a note with no place inside the plot is still left out and recorded (no leader into the margin), and the name column beside the plot keeps its width.

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

Daniel amended the author settings on 23 November. A style file uses `colors.brand`, optional `colors.secondary` and `preset`. When the base is omitted, an explicitly named builtin stays in place; otherwise the company style starts on Air. RichView derives tinted surfaces, inks, accent and a six-colour series palette. It keeps the RichView wordmark's `tokens.brand`. If a company colour cannot meet the contrast floor, the accent and text colours move toward a legible shade and an info diagnostic names the input and result. A secondary colour that fails the palette's distinction rule is omitted with a diagnostic. An explicit palette in the same file takes precedence; the final result remains subject to the existing contrast, palette and colour-vision checks. A dark base derives its printed style from its light counterpart. Achromatic inputs keep the base neutrals.

The same `company` mapping may name the company and report-relative `logo` and `logo-dark`. A logo's bytes must be a still PNG, JPEG or WebP, no more than 512 KiB and 1024 px on its longer side; animated APNG and WebP are refused. The company name supplies its alt text; without one it is decorative and a warning asks for a name. The mark appears before the report title and in exported HTML, PDFs and the link-preview thumbnail. On a dark style the dark logo is used if supplied; otherwise the logo has a light plate. The logo work is a separate implementation.

The site Wasm exposes `deriveCompanyStyle` using the same derivation as the report style cascade. A company-style guide and worked example show the one-colour route. This proposal assumes STY-15, STY-16 and STY-53; STY-14 is answered.

## FEA-CH-53 — dimming that reads on dark pages

Target: Charts — keep the rest of the chart readable while one mark is emphasised on a dark page. On a chart page whose background is dark (OKLCH lightness under 0.5, Sleek), the dimmed opacities of the 12 November amendment are multiplied by 1.6: other series and wedges go to 32% instead of 20%, other points to 56% instead of 35%, and unconnected Sankey and alluvial ribbons to 16% instead of 10%. At 20% a dark fill on a near-black page all but disappears, and the reader loses the context the emphasis is set against. Light pages keep the 12 November values. A label inside a mark (a tile's name, a bar's inside value) now dims with its mark, so dark text no longer sits on a faded fill; labels outside marks still never dim. A label belongs to a mark when its whole box lies inside the mark's shape (a ribbon's or wedge's outline, not its bounding box); the smallest such mark owns it. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

# The learn guide for a fresh agent — S27-986

Target: `richview learn` — an agent that has read it writes a correct first draft without outside examples (INT-13, INT-69). FEA-SF-47 is unanswered and is built as proposed: five sections, the same bytes on the CLI and MCP, vocabulary generated from the registry.

## FEA-CH-51 — entrances that land together

Target: Charts — make chart entrances smooth instead of snapping in and then waiting for their labels. Each style keeps its duration and stagger. Data marks enter on the style's own curve only when it starts at rest and never passes its end (Scientific's `standard`, Brutalist's `snap`); Sleek's `out`, Air's `soft` and Hand-drawn's `spring` give way to `move` for marks, so a pie has fanned 61% of the way at 40% of its time instead of 78%, and no bar grows past its value. This replaces the card's "one deceleration curve" with one that eases in briefly. Labels and annotations fade in from the moment the last mark has covered 85% of its travel until it lands, so the entrance ends with them instead of 200 to 500 ms later. A row of bars longer than the 320 ms cap allows spreads its starts evenly instead of starting the rest together. Tiles and cells without a baseline (heatmap, treemap, waffle, bin2d, hexbin and calendar, and the boxes, funnel stages, Gantt bars and icicle cells) grow from their own centre in a wave from the plot's top left instead of fading; a treemap group grows as one with the tiles inside it, and the funnel widens from its axis. Filled points grow from half their radius as they fade in, as MOTION.md already specified; hollow points fade in place. Sweeps keep a hard front on lines; filled areas, streams and flows fade in over the 48 px behind it. From 2,000 marks a chart still fades as a whole. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## FEA-CH-42 — a readout that never covers its mark

Target: Charts — the readout sits beside what it describes, keeps its natural height and moves calmly. It takes the first place that lies inside the visible chart, clears its own mark and that mark's 4 px halo, and covers no label or figure text: past the value end, 8 px away; beside the mark, 12 px to the right, then the left; at the opposite end; in the chart's top or bottom margin. On a chart at least 480 px wide it also keeps 6 px from a chart label it does not cover, so the two never read as one phrase. A bar's readout (bar, combo, pareto, waterfall, histogram, lollipop, dumbbell, bullet, gantt, interval, funnel and 3D bar) takes such a place only where it lies on no other mark. When none does, it takes the nearest free place that stays with the bar, in the bar's own column, or row for a bar lying down, and within 120 px of it; failing that, the first of its usual places that lies only on marks that stepped back, because free paper farther away reads as another bar's. On a phone the docks come first, and a bar that cannot dock stays with its bar instead of taking a band inside the plot, which lies over other rows: a free place as above, else the nearest place within 120 px of the bar that lies only on marks that stepped back, wherever along the bar it is clear of the labels. The thumb hides what is below the bar, so a free place level with the bar or above it goes before one below, and over marks that stepped back a place below counts as 40 px farther away. A bar with no such place is searched for like any other mark, nearest its bar. In that search a place inside the plot farther than 120 px from the bar no longer counts, since it would read as another bar's, while some place that does count covers no text; where every such place would lie on a label or the figure's text, the far places stay in and the place that gives up least wins, as before the rule (the lead, 19 November 2026; whether the near place should win there too is an open question for Daniel). Tiles, points and shared lines keep their readout beside them. A gauge's readout first tries the half of its hollow away from the needle, clear of the needle, the hub and the value and name in the middle, which count with its mark. The figure text it keeps clear is the title, the subtitle, the toolbar, the legend and the note, each measured on its own. On charts narrower than 480 px it docks below the plot's axis labels, over the chart's note if needed, or else above the plot; a shared readout whose rows name every legend entry beside its swatch stands in for the legend, so either dock may cover it, each entry wholly or not at all: the readout first slides a little to an entry's edge, where that brings it onto no axis label, and otherwise its surface widens to the chart's width, with its rows at their natural width at the plot's left edge and its top and bottom edges between lines of text. Any other place on a phone is searched for, and there a cut entry counts like a cut number. Where the place found still cuts an entry, a legend between the subtitle and the plot with no room for a dock, a second search counts an entry cut only across its width as whole and widens the surface over it in the same way, while that covers no more of the plot; where the widened surface would run into the title or the toolbar it slides to where it fits and covers whole whatever labels it then lies on. The second place stands only if it cuts no entry and no more numbers than the first. A figure can leave no place that keeps every entry whole, as Hand-drawn's does on a phone for an observation in the middle of the plot, its legend lying between the axis labels and the title row: the first place then stands, and the placement check lists the cut as unavoidable, as it does a number's. A widened surface may stand flush beside its own mark but never on it. A dock never covers the title, the subtitle, the toolbar, the axis labels or a legend the readout does not stand in for, and the page never moves. A key drawn in the chart, such as a gauge's bands or a colour scale, counts as a legend. A gauge's readout names its band, so a gauge gives up its key after the subtitle and before its needle. When neither dock fits, a gauge tries its hollow, then the readout takes the plot band farther from the selected mark. When no place clears everything, the readout gives things up in this order: the gap to a label, which hides nothing, then other marks (a line by the length it hides; beside a bar, those that stepped back before those at full strength), then chart text, the note, a legend it stands in for, and the subtitle; its own mark, the title and the toolbar go last, and when even its mark cannot be cleared it hides as little of it as possible. Right after its mark it keeps every number whole: a text with a digit, in the chart or the figure, is left clear or covered wholly, never cut while some place allows it, because the part that shows reads as another value. A phone dock over the note is the exception: a note is a paragraph, and the dock lies over part of it whether or not a digit stands in it. A label it touches counts whole, so half a label never looks cheaper than all of it. Ties go to the place nearest the docks on a narrow chart and nearest the mark otherwise; for a wedge, nearest the middle of its outer arc, so the readout stays on its wedge's side. A readout is never shortened to dodge text; only one taller than the visible chart scrolls, with Page Up and Page Down. A pointer and a keyboard arriving at the same mark from rest put the readout in the same place; while a readout is showing and moves to a neighbour it keeps its side. The keyboard reaches every observation, including parts the chart did not draw and its note names; their readout covers no figure text. Between marks it glides in 180 ms; when the pointer leaves it holds 250 ms and fades with the 250 ms release, replacing the 300 ms hold of the 12 November amendment. This amendment remains assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## PRF-25 — hover timing checked for freezes

Target: Charts — the placement check times each hover it makes and fails on a freeze. Per chart, style and width, the main-thread time from the pointer move to the end of the next frame, without the wait for that frame, has a median of at most 16.7 ms, a 95th percentile of at most 50 ms and a maximum of at most 250 ms, and the first frame shows the new observation in at least 95% of moves. The median is judged from five moves up: a chart of one observation is hovered once, and its one searched place of 20 to 30 ms on a phone is a slow frame that the other two limits bound, not a freeze (assumed on 19 November 2026 until the lead confirms). The limits are always judged, beside a full run's other suites too, so they are loose: they catch a freeze such as the 240 to 350 ms per move on dense lines before S27-986, not a slow frame. A chart that misses a limit while the 1-minute load is over 8, the perf step's threshold, is re-timed once over the same moves against the same limits; both results are recorded and the re-time decides (the lead, 18 November 2026). Chart design set them on 16 November 2026 and the lead accepted them; Daniel can overrule them. No accepted answer changes.

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

The flagship is refreshed to a snapshot of 18 November 2026 with data to October 2026 (Daniel, 17 November 22:50: "Yes, all recommendations"), keeps its wording rules and adds the newer sources, and is cut from 13 sections and 19 charts to 9 and 9. It opens with a four-metric scorecard and the population chart, and Sources cited folds after eight rows.

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

## FEA-CH-28 — a line's last value may stand off on a leader (addition)

On a single line the last value is placed first and, with no clear place beside its point, may stand off on a leader, as the lowest and highest values may; the first value keeps to its point. When the last value and an extreme cannot both stand, the last value stays at every width. This replaces the rule of 16 November that gave the last value a leader only where an extreme lay close to the line's end: A warmer world dropped its last value, 1.18, at 1144 px, because its peak lay just outside that reach (Chart design's gallery ruling, 16 November).

## FEA-CH-28 — values at a line's end keep the order of their points (addition)

Two values that stand one above the other keep the order of their points, the higher value above, and no value's leader crosses another's: crossed, each reads as the other's value (Chart design, 16 November: "Of two values at a line's end the higher belongs above").

## FEA-CH-27 — a single line draws no markers (addition)

A style that marks its lines with shapes, as Hand-drawn does, draws the markers only when the chart has two or more series. A single line has nothing to be told from, and its markers would read as marked observations (Chart design, 16 November: "A single series has nothing to tell apart").

## FEA-CH-28 — an area's values stand off its own fill (addition)

An unstacked area's value stands on the far side of its own line from its fill, on the page or on another series' tint, at 4.5:1 against what it stands on, and never inside its own fill, where its name stands. With no room there the group is left out and each value is recorded as dropped (Chart design's ruling, 16 November). Such a value may stand across soft gridlines, which stop short of it as they do round a bar's value. On a phone-width chart, where no value has room between its line and the plot's top, an area prints its names only and records its values as dropped (lead's verdict on the sheet, 19 November).

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

# The fourth released build is 3.0.0-alpha.3 — S27-986

Target: Delivery — the release after `v3.0.0-alpha.2` (18 Nov 2026) can be published beside it. Lead, 18 Nov 2026, for Daniel to change.

## INT-41 — each released build carries its own version (third step)

`v3.0.0-alpha.2`'s tag is fixed like the earlier ones, so the next release's build, candidate/22 with Windows at parity, calls itself `3.0.0-alpha.3`, in the same places as alpha.2: `Cargo.toml` (every workspace crate), `Cargo.lock`, `package.json`, `package-lock.json`, the VS Code extension (`packages/vscode/package.json`, its lock entry and the version its packaging check expects) and the INT-41 judgement in `tools/rv-gate/coverage.json`. The chart engine keeps `0.1.0`, and records of what earlier builds printed keep their numbers. How versions advance after this one is still not decided here.

# The 22 November build was named 3.0.0-alpha.4 — S27-986

Target: Delivery — the release after `v3.0.0-alpha.3` (19 Nov 2026) can be published beside it. Lead, 22 Nov 2026, for Daniel to change.

## INT-41 — each released build carries its own version (fourth step)

`v3.0.0-alpha.3`'s tag is fixed like the earlier ones, so the next release's build, with sign-in through GitHub, Microsoft and Google, calls itself `3.0.0-alpha.4`, in the same places as alpha.3: `Cargo.toml` (every workspace crate), `Cargo.lock`, `package.json`, `package-lock.json`, the VS Code extension (`packages/vscode/package.json`, its lock entry and the version its packaging check expects) and the INT-41 judgement in `tools/rv-gate/coverage.json`. The chart engine keeps `0.1.0`, and records of what earlier builds printed keep their numbers. How versions advance after this one is still not decided here.

# The public release is 3.0.0 — S27-1072

Daniel chose `v3.0.0` on 23 November 2026 (INT-41). The workspace, CLI and VS Code extension carry `3.0.0`. The chart engine keeps its independent crate version.

# Windows at parity with macOS and Linux — S27-986

Daniel, 16 November, on the open question of a Windows binary: "make windows happen using aws windows machine"; 3.0.0-alpha.2 shipped it as a preview without PDF output or SQL data sources. Daniel, 18 November: "yes, i wanna windows to work e2e, it can be tested on some aws machine, make sure it will be done e2e". ARC-67 and ARC-30 are unanswered, so this is built as their rewritten proposals and stays assumed until the lockdown owner records Daniel's answer. No accepted answer changes.

## ARC-67 — Windows x64 at parity (replacement of "Windows stays unsupported" and of the preview)

RichView supports macOS on arm64, Linux on x64, and Windows 10 and 11 on x64 (`x86_64-pc-windows-msvc`) with the same features: the CLI with its local preview, the MCP server, the editor service for the extension, PDF output and SQL data sources. On Windows the SQL worker runs under a restricted token whose only restricting SID is new for each launch, with no privileges, at Untrusted integrity, in a one-process job, and without a console. As on Linux and macOS, DuckDB opens the declared source only inside that confinement. Once per render the host copies the source's bytes, unparsed, into a new folder in the temporary folder that only the running launch's SID may read. It refuses a source over 4 GiB, one that would leave less than 1 GiB free there, and a temporary folder on a network share or named by volume GUID, and the next render sweeps the copies a killed render left. On every platform the worker may read the database file alone, so a database whose last changes are still in its `.wal` file is refused until a writer has written them into it. The restricted token can open nothing by path, so before it drops the token it started with (which keeps only SeChangeNotify of the user's privileges) the worker takes a handle to that folder, and its opens of files in that folder, and only those, go through the handle; every other path reaches Windows unchanged and is refused, and the restricted token decides every open. Its probe, run on Windows Server 2025, shows no file, network (a TCP endpoint opened on the network driver as Winsock does included) or process reach beyond the Linux sandbox's, for author SQL and for DuckDB's own extension loading. Windows is proven for each candidate by the recorded end-to-end walk on a fresh AWS Windows Server 2025 machine (`tools/windows-e2e`); Windows 10 and 11 share its kernel and are expected to behave the same, but stay untested because AWS offers no Windows 11 image. The probe's record and the record walk of this change's final commit are in pull request #191. `merge-prod` refuses a candidate that ships Windows without a passing walk on its exact commit, and it refuses one whose `release.json` is missing or unreadable. The full reports its `windows` step `unrun` with that reason. The programs stay unsigned until a certificate is bought.

## ARC-30 — the browser host service runs on Windows (addition)

PDF export finds Edge or Chrome on Windows as it finds Chrome elsewhere. The browser's DevTools pipe ends travel as C runtime descriptors 3 and 4, and only those handles are inherited. A kill-on-close job ends the browser's whole process tree when printing is done, as the process group does on Unix. Its temporary profile folder has a DACL for the user alone and inherits nothing from the temporary folder, as mode 0700 gives it on Unix.

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

The signed-in pages share one rail, top bar and content width. On desktop the layout reserves the width of a classic scrollbar across short and long pages. The narrow navigation band gives each tab the same label weight in every state so selecting a page leaves the other tabs in place. On the phone the invitation explanation reserves room before the embedded font loads.

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

The elements page's "Type and fonts" shows one live typography sample with a row of the builtin styles that swaps it, as a chart page's style row does, instead of five live samples side by side, and an "Open the sample" link follows the choice. No docs page starts more than three RichView runtimes when it loads, its own and one per live frame of the site, and the site build fails one that would: iOS gives a page's process three fast Wasm memories (the lead, 18 November, after Daniel's iPhone stopped loading the gallery and charts).

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

## TYP-39 — a macOS sidecar is not a file of the report (addition)

Proposed addition to TYP-39's text, built as assumed; no accepted answer changes. A file named `._<name>` is an AppleDouble sidecar: macOS writes it beside `<name>` wherever the volume or archive cannot keep the file's attributes (tar, zip, exFAT, an SMB share). It is not a file of the report folder. No host counts it as the folder's `.rmd`, lists it in the inventory, reads it for a reference (`report.asset.missing`, saying what the file is) or publishes it; named as the source itself it is `report.root.invalid`. Only this name is left out: no card decides hidden files in general, so `.DS_Store` and every other dotfile stay files of the folder. Cause: a report folder sent from a Mac to Windows or Linux was refused as two reports (`report.root.shared`, "multiple .rmd sources: ._report.rmd, report.rmd"), found on 19 November 2026.

## INT-73 — the impressum names Daniel as a sole trader (amendment)

RichView stays Daniel's personal project. Its impressum now says that he builds and operates it as a sole trader registered in the Czech Republic (IČO 00923192, read from the public ARES register on 18 November 2026) instead of calling it a personal hobby project, and its meta description follows. Startup-credit applications screen the site for its founder and operator, and the hobby line blocked them. The proposal text is amended in part 01; the card stays open for Daniel's answer, and the line reaches richview.ai only with his go for candidate/22.

# Expands, code blocks and one long format reference — S27-986

Target: Reports — an author folds long detail and whole sections away from the reader's first pass, and shows code as a designed block, in every style, on screen, on a phone and on paper. Daniel's review of candidate/17 on 16 November asked for expands and code blocks as elements of the format, an elements gallery that lists every element and a format reference on one long page; on 18 November he called expands a huge priority. The full card texts are in `docs/lockdown/parts/`. No accepted answer changes.

## FEA-RP-66 — Expands (new card)

An `expand` block (`title`, Markdown `body`, `open`) renders as a native disclosure: it reads without scripts, find-in-page opens it, its title row is at least 44 px tall with a line chevron, and on paper every expand is open under a small heading. One level only: a rich block inside it is a `block.nested` error with the not-rendered notice, as in a callout. `expand` is a rich fence tag from now on.

## FEA-RP-67 — Collapsible sections (new card)

`collapsed_sections` in the frontmatter names top-level heading anchors whose sections, charts and tables included, start closed under their heading, with the expand's chevron at the heading's size. A section ends at the next heading of its level or above, at a tab boundary or before the report's Notes, and prints open; a PDF is laid out as the unfolded report. A missing anchor (`section.anchor-missing`) or a heading with a link (`section.heading-interactive`) warns and stays open.

## FEA-RP-17 — Code blocks (amended)

A fence renders in the style's mono face. The label, the title, line numbers that are never copied, marked lines (a rule plus a tint; `code.mark` warns past the last line) and a report-folder file of at most 400 lines as the source belong to the `code` block. Daniel asked for value highlighting on 23 November 2026. SQL adds keywords, comments and literals, and Rich Markdown, YAML and shell show typed values. The style's inks distinguish structure, comments, strings and other literals on screen and in print. Bold and italic come from faces the style embeds. This builds the open card's "Highlight known languages" option. `code` and `expand` are now rich fence tags, so a plain fence written as ```` ```code ```` or ```` ```expand ```` is read as the block.

## FEA-SF-41 — the sidebar and the references (amended)

The sidebar's Reference group holds the elements gallery and the format reference, each one page with its parts as anchors. The format reference is one long page, and a topic's old address forwards to its part, kept out of search. The elements gallery has a part for each element that has one and a compact card for each of the others, linking to the guide section that shows it live.

# Columns — S27-986

Target: Reports — an author puts two to four blocks side by side, such as two charts, a chart beside its explanation, a row of metrics or an image beside text, and the page looks designed in every style, on a phone and on paper. Daniel called blocks and columns a huge priority on 18 November; the lead approved the proposal's option A the same day. The full card text is in `docs/lockdown/parts/`. No accepted answer changes.

## FEA-RP-68 — Columns (new card)

`::: columns`, one `::: column` per column and `:::` to close each, the fenced-div shape of Pandoc, Quarto and MyST, around ordinary Markdown and rich fences. Options in a YAML flow mapping of closed vocabulary: `widths` (`equal` or a ratio such as `2-1`), `gap`, `align` and `wide`, which defaults to `true`. Reading order is source order; the columns stack, all of them, when the narrowest would fall under its floor (a phone's column for a chart, table, frame, video or code; 11rem otherwise), on screen and on paper alike, and always on a phone. Malformed columns warn with a `columns.*` code and render exactly as without columns.

## Decided here, for Daniel to overrule

- `wide: true` is the default, unlike single blocks: on the text measure two columns are a phone's width each.
- A wrong option value (`widths: 70%`) is a `block.schema` error as in a block, and the markers stay text; the proposal said it would fall back to the default with a warning.
- A heading inside a column never starts or ends a collapsed section (FEA-RP-67), so a fold always holds whole columns; an anchor that names such a heading warns `section.anchor-missing`.
- A `:::` line after a paragraph was a definition (`: `) before columns existed; any `:::` line that is not part of well-formed columns still renders that way.

## STY-74 and STY-50 — backgrounds behind a report

Daniel, 18 November 2026 about 14:05 CET, relayed by the lead: "actual background styles are also priority to make it much more personal." The lead approved the proposal at 14:39; the look sheet went to Daniel as Q33. This proposal is pending its lockdown capture; no accepted answer is changed.

A style file may set `background`: a `texture` (`grain`, `dots`, `grid`, `lines`), a `gradient` from the author's colours (`soft`, `glow`) or a report-relative `picture` (a still PNG, JPEG or WebP, at most 400 KiB and 2400 px), at a `place` (`page`, `margins`, `header`) and a five-step `strength`. Every ink keeps its STY-25 floor over the background's worst pixel: inks the author did not set move in lightness first, and the background weakens only when they cannot. A picture under text is washed by the least page-coloured layer that holds over a black or a white pixel; in the margins the report sits on a solid sheet, and behind the header a picture puts the title on a solid panel. Charts, tables, code and embeds keep solid surfaces. Nothing moves; paper, high contrast and forced colours show the flat page.

STY-50, unanswered, is amended with it: "no decorative gradients … in RichView's chrome; an author's `background` may use the derived gradients and textures of STY-74." STY-62's "styles do not tint images" stays true for image blocks. Behind a chosen section (`::: band`) waits for the Columns stream's fenced divs.

# Glow on highlight — S27-986

Target: Charts — a highlighted mark glows the way Daniel picked from the glow sheet on 17 November ("Medium, Far for scatter points"). No accepted answer changes.

## FEA-CH-53 — the glow (replaces the 4 px halo band outside Hand-drawn)

A highlighted mark glows in its own colour: many thin bands of equal strength, so the glow fades evenly outwards, with the page showing between mark and glow. Every highlighted mark in Sleek, Air, Scientific and Brutalist, and the hover dot on a line, glows Medium: 10 px, 34% at the mark. A single point is small, so it carries the larger glow. Far is for small single dots that stand apart: the points of scatter, strip, Q-Q and 3-D scatter charts glow Far, 18 px and 36% at the mark, and the point grows by 60% instead of 25%. A dot plot's dots are large and touch, so they glow Medium and do not grow (the lead, 18 November, from the glow sheet). Packed bubbles and a scatter or 3-D scatter sized by a field draw bubbles, which keep Medium; of these only the sized scatter's bubble grows, by a quarter, as before. Hand-drawn keeps its flat 4 px band at 25%, because a soft glow on paper reads as a drop shadow, which a pen drawing does not have. That is the style's own setting, an internal `chart.glow: flat` leaf that only Hand-drawn and the presets built on it carry, so an author's `chart.decals` switch changes no glow. A line inside its mark's own box, as a boxplot's median, gets no glow. This replaces the 25%, 4 px halo of the FEA-CH-52 and FEA-CH-53 amendment above for every style but Hand-drawn. Assumed: the lead read "Far for scatter points" as every small single dot that stands apart (18 November); Daniel may narrow it to the scatter chart alone.

# Sign-in with GitHub, Microsoft and Google — S27-986

Target: Hosted app — a stranger signs up and signs in with an account they already have. Daniel, 16 November 2026: "I want to support some social sign-ins, maybe using some AWS gate or some other gate"; about 17:10: "Yeah, A is fine", GitHub and Microsoft first, Google after, "you should be able to add social to your already existing account". 18 November, 11:52: "main prio for launch: are windows fully working, oauths ready". Daniel answered the two open questions on 18 November about 18:05, through the chief of staff (below). The same evening, 18:15 through the lead: GitHub, Microsoft and Google sign-in are a launch blocker, so Google is built now on the same pattern.

## Proposed card: sign-in with GitHub, Microsoft or Google (part 04c, decision, key)

Proposal for the lockdown app, the card text of `card-proposal.md` (16 Nov), as amended by Daniel's answers of 18 Nov. A RichView server may let people sign in with an account at GitHub, Microsoft or Google; the server runs each provider's sign-in itself, with no external gate. An identity is the provider plus the provider's id for the person (GitHub's numeric id; Microsoft's tenant id and object id), never the address. An address counts as vouched for only when the provider says it checked it: GitHub's primary address marked verified; Microsoft's `email` with `xms_edov` true (Microsoft's `email` alone proves nothing); Google's `email` with `email_verified` true, on Gmail or a Workspace domain (`hd`). A vouched address that an account has joins that account at once; a vouched address no account has makes a new one after a confirmation; without a vouched address nothing is created or joined, and the person is told to sign in the usual way and connect the provider on the Account page. Passwords stay, nobody can remove their last way in, a join is told by mail, the provider's token is used once and kept nowhere, the buttons load nothing from a provider (INT-80), and while a sign-in runs a second cookie lives at most ten minutes. Client ids and secrets are server configuration, a secret only from the environment (DEL-55).

## The gate: built into the server (Daniel's answer, 18 Nov)

Daniel, Q29: "Let's build it on our end. I don't want to have it externally through AWS Cognito or anything." The server runs the standard sign-in with each provider itself: GitHub's OAuth and Microsoft's OpenID Connect, each with PKCE (S256), a one-time state bound to the browser by the ten-minute cookie, and for Microsoft a nonce. No further processor, no paid service (INT-15), no second user directory.

## On a vouched address that matches an account: join at once (Daniel's answer, 18 Nov)

Daniel, Q30: "take them straight away". The identity joins the account that has the vouched address at once and signs it in, and that address gets the notice mail every connection sends. The safety rule stays: only an address the provider vouches for joins anything (above). A disabled account is not joined. The mailed one-hour link that was built first is removed.

## The joining rule as built

- **Connect, signed in:** from the Account page, by a form with the session's CSRF token, whatever address the provider reports. The session is the proof.
- **Identity already joined:** signs that person in.
- **New identity, vouched address, account exists:** joins at once and signs in; the notice mail goes to the account's address.
- **New identity, vouched address, no account:** a confirm page with the terms line creates the account under sign-up's rules (pause, account limit), with no password; no notice mail.
- **Nothing vouched:** creates and joins nothing; the page says to sign in with the email address and connect the provider on the Account page.
- **Invitation:** for an address without an account, accepting with a provider creates the account and joins the identity. An existing account cannot accept with a provider: a mail link never adds a way into an existing account. Accept as usual, sign in, then connect on the Account page.
- **Password reset:** the reset page lists the connected providers, unticked; saving disconnects every one left unticked.
- **Join notice:** never dropped. When it cannot be mailed (the day's budget is spent, or the queue refuses it), the account keeps it, the Account page says "<Provider> was connected on <time>. We could not mail you about it yet. If it was not you, disconnect it and change your password.", and it is mailed at the person's next visit to the Account page once the budget allows; the audit records it. Security read (S4), lead's ruling of 19 Nov; assumed.
- **Removed provider account:** one taken off by Disconnect or by a reset never joins that account again by its vouched address; the sign-in says it was disconnected, and only Connect while signed in brings it back. Otherwise the next sign-in with it would undo the reset. Found in the security read of 18 Nov night; assumed.

## Google (lead, 18 Nov 18:15)

OpenID Connect with PKCE (S256), the state and the ten-minute cookie, and a nonce, as for Microsoft; scope `openid email profile`. The identity is `sub`, Google's stable id for the account. An address counts only when the ID token's `email_verified` is the JSON value `true` and Google is authoritative for it: the address ends in `@gmail.com`, or `hd` names a Workspace domain. Any other address Google checked once, when the account was made, and its mailbox may have changed hands since (Google's "Verify the Google ID token"), so it takes the unvouched path and never joins an existing account by address. Security read of 18 Nov night (S2), lead's ruling of 19 Nov about 00:01; assumed. The ID token is read on the same grounds as Microsoft's (below): its issuer must be `https://accounts.google.com` or `accounts.google.com`, and `aud`, `exp`, `iat` and `nonce` are checked. Settings `RICHVIEW_GOOGLE_CLIENT_ID` (or `--google-client-id`) and `RICHVIEW_GOOGLE_CLIENT_SECRET`, from the environment only. The button carries Google's "G" inline, so no page loads anything from Google (INT-80).

## Microsoft's ID token and personal accounts (assumed)

The ID token comes straight from Microsoft's token endpoint over TLS with the client's secret, so its signature is not checked (OpenID Connect Core 3.1.3.7); its `iss` must name its own `tid`, and `aud`, `exp`, `nbf`, `iat` and `nonce` are checked. Personal Microsoft accounts (tenant `9188040d-6c67-4c5b-b112-36a304b66dad`) follow the same rule as work accounts: an address counts only with `email` and `xms_edov` true, which Microsoft's optional-claims reference lists for Microsoft accounts. Lead, 18 Nov 12:17.

## The stand-in for tests (assumed)

A hidden server flag points every provider at a stand-in. It starts only when the stand-in and the server's own public address are both on loopback, so it can never be on at `app.richview.ai`. Lead, 18 Nov 12:11 and 12:17.

## Plain HTTP at a loopback address, for local development (assumed)

Hosted mode still requires HTTPS app and report addresses, with one exception: plain HTTP at a loopback address (`localhost`, `127.0.0.1` or `[::1]`). This lets the sign-in be tried locally against the providers' development clients, whose callbacks are `http://localhost:8080/sign-in/<provider>/callback`; before, only the stand-in allowed it. It does not keep the internet out, since a hosted server always listens on loopback behind its proxy: such an address left in a deployed setting no longer stops the start, its links and callbacks point at `localhost` instead, and the start prints "hosted mode on plain HTTP at <address>: for local development only". A guard refusing proxied requests under the exception was not added: the stand-in tests model the proxy with `X-Real-IP` on a loopback server. Every other plain-HTTP address is refused as before. Lead, 18 Nov 23:41; wording and start line after the security read (S3), 19 Nov.

# Publishing signs in once and keeps the sign-in — S27-986

Daniel after his first sign-in to app.richview.ai, 16 November: publishing must not start from a global `RICHVIEW_TOKEN`; "I'm fine with CLI tokens as well, which are then permanent". Lead ruling of 16 November on the proposal; ARC-23, FEA-SF-01, FEA-SF-09, FEA-SF-12, FEA-SF-53, TYP-69 and TYP-70 are unanswered. FEA-SF-19's answer is amended: Daniel chose "Yes, amend FEA-SF-19 as written" on the status board (Q34, 19 November 2026, 00:35 CET), so it reads "The CLI, MCP server and VS Code extension need no account; to publish, the command line keeps a sign-in per server that you can remove with richview logout."

## ARC-23 / FEA-SF-12 / FEA-SF-53 — the command line keeps a sign-in per server (addition)

`richview login` reads a CLI token without showing it, asks the server whose it is and keeps it for that server alone in `credentials.json` in the user's RichView configuration folder (`~/.config/richview/`, the folder 0700 and the file 0600; `%APPDATA%\richview\` on Windows). A file other users can read is not used. `publish` and the `report` commands take `--token`, then `RICHVIEW_TOKEN` (still sent only to `RICHVIEW_SERVER`), then the token kept for exactly the server they send to; a kept token is never sent to another host, port or scheme. On a terminal a command without a token asks its person to sign in and carries on; without a terminal it asks nothing and fails with exit 2, naming `richview login` and the variables. `richview logout` forgets the token on the computer. `report` commands without `--server` use the remembered publish server. A token is never printed, never part of a message and never in an address.

## FEA-SF-01 — `login` and `logout` (addition)

Two commands join the list: `richview login [--server <url>] [--with-token]` and `richview logout [--server <url>]`, both with `--format`. `--with-token` reads the token from standard input, for a script. Signing in also remembers the server as the publish destination unless `RICHVIEW_SERVER` names one.

## FEA-SF-09 / FEA-SF-12 / TYP-69 — the MCP publish tool uses the kept sign-in (addition)

When the MCP host's environment names no server, `richview_publish` publishes to the server the user chose in a terminal, with the token kept for exactly that server; when it names a server without a token, the token kept for that server is used. The environment wins when it is set, `RICHVIEW_TOKEN` without `RICHVIEW_SERVER` is sent nowhere, and a `server` argument can only repeat the configured or chosen server, so a tool call still cannot choose where a token goes (INT-79).

## ARC-23 / FEA-SF-12 / TYP-70 — the command line signs in through the browser (addition)

On the hosted server `richview login`, and `publish` or a `report` command on a terminal without a token, sign in through the browser. The command line starts a sign-in (`POST /api/cli/sign-in`), shows a short code `XXXX-XXXX` and the page `/cli?code=…`, and asks every 3 seconds (`POST /api/cli/sign-in/token`) whether its person has decided. A signed-in person sees the code, the computer's own name and their organization on that page and chooses Allow or Cancel (`POST /ui/cli`, with the session's CSRF token). A yes hands the command line an ordinary CLI token once, named “Command line on” the computer's name, listed and revocable on CLI tokens. `richview logout` revokes it (`DELETE /api/cli/token`, a token revoking itself). `richview login` also works without a terminal, printing the code and the page and waiting; `publish` without a terminal never starts a sign-in.

The short code is no credential and is the only part in an address. The command line's secret and the token travel in request and response bodies only, are stored nowhere on the server (the token as a digest, like every CLI token) and are spent by the first collection after a decision. A sign-in lasts 10 minutes. One address starts at most 10 sign-ins in 10 minutes and collects at most 120 times a minute; at most 512 wait at once; a person who gets 10 codes wrong in 10 minutes is refused every code for a while. A server without these routes, older or self-hosted, makes the command line ask for a pasted token instead.

# The playground opens a full report, with the live example's tools — S27-986

Target: Playground — a first visitor sees what a whole report can hold and adds blocks without knowing the syntax. Daniel, 16 November, on candidate/17's playground: "The playground needs the plus buttons and a much more complex thing to start with." The lead approved the proposal's option A on 17 November. FEA-SF-34 is unanswered, so this extends its proposal; INT-26 holds, because every button writes visible source. No accepted answer is changed here.

## FEA-SF-34 — the playground's start and tools (amendment)

The playground opens a full report, not a sample: Florence Nightingale's mortality record of April to June 1854 (public domain, `examples/playground-start/`), in 76 lines, with a line chart, a metric, a grouped bar chart, a table, a callout and her 1858 diagram as an image, every figure inline in the source. The page carries the source in its text area, so the first chart draws without a fetch and a reader without scripts still reads it; the starter's folder is published at `/examples/playground-start/`, and the image arrives after the first draw. A `?src=` link loads its own folder instead.

Above the source sit the home page's tools, from one code path: a Chart type picker that lists only the types the core draws for the chart under the caret without a finding, and an Add row (Chart, Table, Metric, Callout, Image, Heading, Paragraph) that writes a block into the visible source after the block the caret is in, or at the end. Undo takes it back. The playground's blocks carry their data inline, so they render in any draft; Image adds the starter's own picture, and says so when the draft's folder holds none. A draft too long to show whole leaves the tools off. The home page keeps its six buttons, its blocks and its look.

# Slides, an experimental second rendition of a source — S27-986

Target: Report rendering — a source can be shown on a screen for a review meeting, with big numbers, charts and short text, one screen each. Daniel, 17 November 2026: "make richview slides a thing. [...] Tag experimental but we launch it for people as well". The lead approved the card the same day with three additions: where slides show, the progress bar, and that slides reach users under the experimental label.

## FEA-RP-65 / TYP-100 — slides (new cards)

The cards' proposed text in parts 04b and 05 is the rule this branch builds: `slides: true`, the title slide and one slide per top-level heading or `---`, the four regions, the density warnings, the 16:9 stage and the phone stack, the keys and the address, and one slide a page on paper. One sentence is not built: a publication does not yet show a slides source as the report with a note; a published story frames slides at its content's height. With expands (FEA-RP-67), `collapsed_sections` has no effect in slides, without a warning: a slide shows everything it holds. With columns (FEA-RP-68), a slide lays out no columns either: the markers drop out before the cut, and the content reads in the slide's own regions, without a warning. With backgrounds (STY-74), a picture behind the header fills the title slide's stage and the title stands on a padded solid panel; a phone keeps the report's band above the title. Textures and gradients sit behind the slides as behind a report.

# A group of capped bars stands on its category — S27-986

Target: Charts — a group of bars reads as its category's. On the first sheet of slides, 17 November, the lead found a month's two bars standing left of the month's name. Bars widen with the room up to a cap (64 px upright, 40 px lying), and a capped group began where an uncapped one would, at the start of its category's band, so it stood off its name: about 44 px in the flagship's quarterly revenue chart at 1280 px. FEA-CH-06 is unanswered, so this extends its proposal; the lead granted the engine path and asked for the change in a commit of its own, with before-and-after captures.

## FEA-CH-06 — a group of bars is centred on its category (addition)

The bars of one category stand side by side centred on its band, and so on its name, at every width: when the cap stops them widening, the group is shorter at both ends, never only at its far end. The value labels' room is measured at the same place.

# Each style's slides stand on a stage of their own — S27-986

Target: Report rendering — slides that are a joy to present in every style. Daniel, 17 November 2026, on the first sheet: "slides look basic --> wanna have the most kicks ass slides ever made - pure aesthetic joy for all of them thanks!". The research is the look section of `docs/lockdown/evidence/slides.md`.

## FEA-RP-65 — the stages and the arrivals (addition)

The card's new paragraphs "Each style has a stage" and "Slides arrive" are what this branch builds: five worlds chosen by the style, full-bleed fields for the lone number and the section in Sleek and Brutalist, Air's one quiet raised field, a chart on the stage rather than on a panel, numbered figures and tables in Scientific, and a build as a slide comes into view at the style's own pace, at rest for the first screen, without scripts, under reduced motion, with motion off and on paper. Assumed because the card is unanswered and it goes past it: the look of each stage, the fields, the builds and Brutalist's accent tag are design decisions Daniel judges on the sheets.

## FEA-RP-65 — every slide a stage moment (addition)

The card's paragraph "Every slide is a stage moment" is what this branch builds on top of the stages: a one-chart slide's claim as its headline, the first of a row of numbers leading, a change as a chip, a Markdown table's change chips, lead column and total, the callout as a pull quote, a chart's first series leading over quiet ones with two to four gridlines and its values printed, two to four numbers over one chart standing in a rail beside it at the slide's height, and a title slide of its own on each stage. Assumed because it goes past the card: that the first metric of a row is its headline figure; that a column of signed changes compares against the number column just before it; that a last row equal to the sum of those above is their total; that a chip's mark follows the sign, not whether the change is good; and the look of each, which Daniel judges on the sheets. The quiet series are the palette a stage gives its charts: tones of their own hues on a light page, slates of the page's hue on Sleek's night, and on the sketchbook the red pencil leads ahead of the greys. The few gridlines are one slide-only chart theme field, `few_gridlines`, absent from a report's payload as `wide_bars` is. A chart on a stage keeps the engine's default `auto_annotations`, so its values print as in a report (Daniel, 15 November 2026: “automatic annotations on by default”). Assumed further: on a stage a bar, line or area chart sets them half again as large as its axes and in the axis's compact form (`$420K` rather than FEA-CH-22's `$420 thousand`), at the same precision, so they read as the slide's headlines do; one slide-only chart theme field, `stage_values`, absent from a report's payload as `wide_bars` is. A chart on a stage keeps the style's data face: the engine measures only its bundled data and display faces, and the runtime clips each label to its measured box, so an interface face would be cut.

## FEA-SF-47 — `richview learn slides` (amendment)

The lead's call of 18 November, as owner: how to write slides is its own learn section, `slides`, between `style` and `workflow` on the command line and in `richview_learn`, and `grammar` keeps one line that points to it. With slides, columns and expands in, `richview learn grammar` was 31,680 characters, over the 30,000 one tool output shows; the limit stays. FEA-SF-47's proposal names the sixth section.

## TYP-48 — `components.slides.stage` (addition)

A style's `components` section takes `slides.stage`: `plain`, `keynote`, `gallery`, `poster`, `sketchbook` or `paper`, marked experimental. The builtins set one each; a style that sets none keeps the layout alone.

# Slides are no longer experimental — S27-986

Target: Report rendering — slides stand as a format of their own, on the home page and without a label. Daniel, 19 November 2026, looking at pre-prod: slides on the home page, and "don't call it experimental anymore". No accepted answer changes.

## FEA-RP-65 / TYP-100 / TYP-48 — the label goes, the syntax stays (amendment)

The info `slides.experimental` is gone, with its registry entry, its test and the words in the docs, `richview learn`, the command help, the docs navigation and the gallery. The leaf `components.slides.stage` is marked safe, so a style file that sets it gets no `style.experimental` note. The syntax stays as built: `slides: true` and `components.slides.stage` (the lead, 19 November, over `as: slides`, `format: slides` and `style:`/`look:`: `format` is already the source format version, `as` is a control's word, and `stage` is used nowhere else). The registry, `richview learn slides` and the guide open with the same three lines: `slides: true` turns the source into slides; each top-level heading or `---` starts a slide; the style's stage sets the look.

# A price chart names its last close, high and low — S27-986

Daniel, 16 November, 18:20, read the lone last close of a candlestick chart as "one random number". Chart design's ruling of 17 November, 07:17; no card decides it.

## FEA-CH-28 — a candlestick or OHLC chart prints its last close as a tag and names its high and low (replacement of the lone last close)

The last close stands as a tag on the value axis side at the close's own height, in the last period's colour with the number in whichever of the style's ink and surface reads better on it (at 4.5:1 or more; on a colour where neither reaches that, the tag is the surface outlined in the period's colour and the number stands in the ink), joined to the last period's mark by a hairline; an upright chart keeps the number's width for it beside the plot, and the tag's padding stands in the margin the plot already kept, so the plot is no narrower than it was for the bare number. The period's high is printed over the wick of the period that reached it, `High` in the soft ink and the number in the full ink, at the value label size; the low under its wick, `Low` and the number; the earliest period wins a tie. All three print at the label's digits. An upright chart whose panel is at least 480 px wide (the chart's own frame, or one facet's) keeps a line of room past the highest and the lowest wick for the two, as a dumbbell's axis does for its end values; a narrower or a horizontal chart keeps none, because the room would cost its marks half the plot, so there the high and the low print only where the axis happens to leave a place for both. The two come as a pair: where either would meet the tag, a mark or the other, or has no room, neither is printed and both are recorded as dropped, because a reader shown one extreme expects the other (lead, 19 November); a record names its value, `High 107.4` or `Low 99.5`. On an upright chart the tag always stays. A horizontal chart has no room for any of the three: the tag's place is under its last row, which is the plot's last, and the high and the low stand past the axis' ends, so it prints none and records all three, as it recorded the lone close before (lead, 19 November: it stays so for now). `auto_annotations: false` prints none of the three. Volume, moving averages and the change since the open are not printed. Candlestick and OHLC charts draw no author annotations today, so none takes an automatic value's place.

# Site copy — S27-986

Target: Site and docs — a visitor learns from the home page what RichView is, in Daniel's own sentence, and the docs say what it gives before how it works. Daniel dictated the sentence and the asks on 16 November. No accepted answer changes.

## INT-01 — the home page's line under the headline (note proposed)

INT-01's accepted answer, "Presentation layer for your agent", stays the definition in descriptions, share texts, the README and `richview learn`. On the home page the line under "Let your agent present." is Daniel's sentence of 16 November 2026, verbatim: "RichView is a local CLI that lets you generate beautiful HTML reports from simple Git-storable text files." The problem sentence under it sets its two claims in the strong ink, as he asked the same day. The note on INT-01 that records this is proposed for the lockdown app; nothing here writes an answer.

## FEA-SF-41 — Documentation guides (amended)

FEA-SF-41, unanswered, names six task guides. The docs gain a seventh, "Run your own server": a machine, two HTTPS names, the proxy in front, people and roles, an author's first publication and backup, with its proxy examples described as a starting point. No step was run in a cloud; the guide's commands are tested on loopback. The publishing walkthrough names both places to publish to, RichView's and your own. Each how-to guide opens with what it gives the reader; the styles guide keeps the opening STY-73 gave it.

## FEA-SF-56 — Home-page sections (amended) and DEL-18 (a note)

The Charts card on the home page says that RichView has its own charting library and how many chart types it has, linking to the chart gallery, and the docs' home has a section "Why RichView" that starts with the same number. The number is the schema registry's public chart types, the flow chart and the pivot table among them (56 today), written in by the build through a slot; no page types it. The chart gallery lists two fewer, since those two sit under their own elements, so the card's link names the gallery rather than the number, and the sentence says the flow chart and the pivot table have pages of their own (Reviewer's note C2, FEA-SF-30). The lead ruled on 18 November that the count is the registry's, the number DEL-18's docs check already holds every typed count to.

## The home page's brand picture (no card)

The picture in "Your brand" is a committed capture, retaken on 19 November with `scripts/capture-brand-report.mjs`: the home page's example report without its photo, over Air with the brand file, which now names the company's logo and uses `colors` instead of the legacy `accent` and `tokens`. No card names this picture; STY-45 and STY-46, unanswered, cover the home page's look. Decided here, for Daniel to overrule: the report is shown without its header photo so the chart's brand colours are in view, and the logo is the example company's, teal beside the page's green.

# Background defaults — S27-986

Target: Styles — Daniel, 19 Nov 2026 ~11:45: "at least three of the default styles, or maybe two ... should have backgrounds", "backgrounds properly documented" and "iterate to have a super clear, super meaningful syntax". No accepted answer changes.

## STY-74 — defaults per preset and the one-word form (amended)

Hand-drawn ships `grain` at medium, Scientific `grid` at small and Sleek `glow` at medium; Air and Brutalist stay flat, and the extended presets take their base's. The lead chose them from the defaults sheet on 19 Nov 11:45. A preset's background yields to its exact inks, and Sleek's faint ink limited its glow to about a third of medium, so Sleek's `ink-faint` rises from `oklch(0.51 0.014 260)` to `oklch(0.535 0.014 260)` (STY-73, assumed; the lead, 19 Nov 13:29) and the glow ships at medium. One word stands for its mapping in a style file and in the frontmatter (`background: grain`, `glow`, `hero.jpg`, `none`). A report that names no style renders in Sleek and gets the glow. Docs guides and the reference stay flat as site chrome, set by the docs build (lead, 11:52); examples, gallery reports and the playground show their preset's background. Paper prints the flat page as before.

## TYP-101 — frontmatter `background` (new card)

The frontmatter takes STY-74's words as the cascade's last layer, so the report's word wins over every style and `--style`. It passes the same style vocabulary, and its picture the same report-relative 400 KiB resolver bound, as a style file's; a bad value is a finding at `frontmatter.background`. Chosen by the lead from the syntax page (option A), 19 Nov 11:46. TYP-100 is the Slides session's.

# The home page says what RichView is, and shows slides and backgrounds — S27-986

Target: Site — a visitor knows within one screen what RichView is, and finds slides and backgrounds among its pieces. Daniel, 19 November 2026, looking at pre-prod: the new pieces belong under "RichView is a whole ecosystem", and a "What is RichView?" explainer, "like it's an offline CLI", goes near "There's a better way: RichView.". No accepted answer changes.

## FEA-SF-56 — the explainer and seven pieces (amendment)

Under the better way and above the proof pair: "What is RichView?", the line "A command-line tool your agent runs on your own machine.", four facts and the path from the agent to the file, in the lead's copy of 19 November (fact three says "you can open anywhere", since a report with a hotlinked photo needs the network, FEA-RP-62). The ecosystem has seven pieces in two rows, Slides after the renderer and Backgrounds after the styles, each linking to its guide; Backgrounds' guide is `/docs/backgrounds`, written by the backgrounds work. Daniel sees the copy in one line through the chief of staff, and any change he asks for lands before `prod`.

## FEA-SF-56 — the five-step homepage explainer (2026-11-22)

Above “What your agent writes”, the paragraph describes the downloadable command, editable plain text beside the data, and a single HTML file to share, with slides and PDF output. The same card treatment as the ecosystem shows download, init, generate, open or share the file, and optional publishing for a link. The published homepage copy follows Daniel’s confirmation.

The strip immediately below the explainer shows a rendered slide from the docs demo deck. The image reserves its space in the static page and links beside it open the full deck and the slides guide.

# Highlight and mute — S27-986

Daniel, 19 November 2026, relayed by the lead: "add annotations - full breadth of cool chart annotations"; the annotations gallery opens with a muted line chart and one highlighted series. TYP-78, FEA-CH-27, FEA-CH-28 and STY-25 are unanswered.

## TYP-78 — `highlight` on line, area, bar and lollipop charts, and `highlight.mute` (addition)

`highlight: {by, include, priority, mute}` is accepted on line, area, bar, lollipop and scatter charts; another type, another key or a `mute` that is not `true` or `false` is a schema error. The rows `include` matches, case-insensitively, are the chart's subject. `mute` (default `true` on line, area, bar and lollipop, `false` on scatter, so existing scatters keep their look) draws every other mark in the style's quiet ink, `ink-faint`, paints it before the highlighted marks and gives it none of the names and values the chart adds by itself. A line or area recedes by series (one without a matching row); a bar, lollipop or scatter point by row. Readouts, Table and CSV keep every value. An `include` value that matches no row warns `chart.unresolved-highlight` on every type, and a highlight that matches no row mutes nothing.

## FEA-CH-27 / FEA-CH-28 — muted marks are not named or valued (addition)

A muted series has no line-end or inside name and no legend entry, and takes no room in the name column; its line still keeps the other names off it. Muted bars, lollipops and series print no automatic values, and a muted scatter names its highlights only. None of these is recorded as a dropped label: the author chose it.

## STY-25 — the quiet ink (addition)

`ink-faint`'s floor, 3:1 on the page, raised and chart surfaces, now also carries the marks a highlight mutes.

# Parquet data files — S27-986

Target: Data — one large Parquet file in the report folder feeds many charts, and the report stays small. Daniel asked on 9 November for "Parquet data files for large data, and smaller reports with many charts"; the lead approved this design on 19 November. No accepted answer changes.

## TYP-102 — Parquet data files (new card, built as assumed)

A block names a Parquet file with `data.parquet`, as it names a CSV. Only native hosts read it, in the confined SQL worker with DuckDB's own Parquet reader: the worker can read that one file and nothing else. It reads only the columns the report's blocks name, every row of them, at most 1,000,000 rows and 16 MiB of those columns. Parquet's types decide each value. A larger file is aggregated by report SQL through a connector of kind `parquet`, where the file is the table named after the connector; each chart then embeds its few aggregated rows.

## Decided here, for Daniel to overrule

- **The reader.** DuckDB in the confined worker, not a pure Rust reader in the host process. A hostile file is parsed inside the sandbox. The cost: on Linux, Parquet needs kernel 6.2 or newer, as SQL sources do.
- **No sampling.** A file over the limits is a `data.limit` error whose fix names SQL. RichView never draws a sample as if it were the data.
- **No aggregation vocabulary.** Aggregation is the author's SQL. `data.parquet` adds no `group` or `sum` keys (INT-24).
- **The example's large file is not committed.** `examples/parquet-report/data/seed.sql` writes the million-row file, as the SQL example's seed writes its database; the forty-row `stores.parquet` is committed.

## TYP-35 and TYP-43 — amended proposals

TYP-35's source table gains the `data.parquet` row. TYP-43's limits gain the Parquet row: 1,000,000 rows from one file, and the 16 MiB dataset limit counted on the columns the report reads, not on the file.

## TYP-103 — statistic charts over many rows (new card, proposed only, not built)

A histogram, density, box plot or similar chart embeds every observation today, so one over 100,000 rows costs 0.8 to 3.1 MB although its picture is a few kilobytes. The card proposes embedding only the derived structure above 10,000 observations, with the Table showing that structure. It changes TYP-36's promise that the Table shows every row, so it waits for Daniel.

## TYP-104 — one number of decimals for a typed numeric column (new card, proposed only, not built)

A number that arrives as a number prints in its shortest form, so a column holding 9.26, 9.00 and 8.98 prints 9.26, 9 and 8.98, from SQL, from Parquet and from a CSV alike, and the vocabulary has significant digits but no decimal places. The card proposes one number of decimals per column, from the author or from the source's declared scale, never more than the author chose (FEA-CH-37). The example's queries return whole numbers and one-decimal averages meanwhile.

## FEA-CH-53 and STY-38 — tight chart contours and plot borders (2026-11-22)

The chart mark glow and contrast outline go. Line hover dots draw whole at axes. Scatter growth remains 60% for small points and 25% for sized points. Other bars and packed bubbles recede while the target keeps its edge and size. OHLC traces its own stroke colour in a narrow band. Flow nodes strengthen only their own border. Alluvial and Sankey keep connected marks at full strength. Pointer-hover feedback clips to its own plot. The actual mark, line or scatter, stays whole at an axis. A grown scatter mark with its centre inside the plot has an 8 px allowance beyond it. The one keyboard-focus ring stays whole and concentric with its mark, including across an axis or the plot edge. This supersedes the Medium, Far and Hand-drawn mark-glow proposal.

# Tables: heat and filtering — S27-986

Target: Reports — Daniel's "Tables get heat maps, sorting and filtering" (16 November 2026). Sorting exists (FEA-RP-22). No accepted answer changes.

## FEA-RP-69 — Heat in table cells (new card)

`columns[n].heat` shades a numeric column's cells by value: `true` or `sequential` from the lowest value to the highest, `diverging` on both sides of zero; `heat_scale: table` puts the heat columns on one scale. The shade is the style's own heat colour, capped where `ink` keeps 7:1 (STY-25), so every number stays printed in the page's ink; paper keeps it. A shade comes from the value the cell shows, in whole percent, never a finer one (FEA-CH-37). Off by default.

## FEA-RP-70 — Filtering a table (new card)

A search field above a table's rows keeps the rows whose shown text holds every typed word, and a status line says how many. The runtime adds it, so without scripts and on paper the table is the plain table. `filter: auto` (the default) shows it for more than 10 rows; `true` and `false` decide. A total row stays the total of every row, and the status line says so, also when no row matches. In a narrow table the field shrinks beside its label, so the empty box keeps to its one line. Slides (FEA-RP-65) show a table without the box, as they show a chart without the report's tools.

## Amended proposals

- TYP-24 gains `columns[n].heat`, `heat_scale`, `sortable` and `filter`.
- FEA-RP-22 gains `sortable: false`, and sorting together with a filter.
- FEA-RP-21 says the total stays the total of every row while a reader filters.

## Decided here, for Daniel to overrule

- The filter box is on by default above 10 rows, so existing reports with longer tables gain it.
- A page that runs scripts reserves the box's line under the title before the runtime adds it, so neither the title nor the rows move; without scripts there is no gap, and after 10 s without a runtime the room is given back.
- With a filter box, the table's title moves from its caption to a paragraph above the box that still names the table (`aria-labelledby`); without the box it stays a caption (FEA-RP-20).
- Ink never flips on a shade; the shade is capped instead.
- Hand-drawn's two heat colours are the same graphite, so its cells below zero are hatched, as its charts mark a negative.
- `diverging` is centred on zero only; a fixed range, bars in cells and an authored `controls` filter on tables are not built.

## FEA-RP-19 — short labels stay on one line (amended proposal)

A table cell holding a short label of several words, at most 12 characters such as "Store 40" or "New York", stays on one line on screen wherever the table then fits its box and is no taller than with its labels wrapped. A browser gives a column that can wrap less than its cells need as soon as the table is tight, so "Store 40" broke before its number while the narrower "Store 19" beside it did not. The runtime measures each table: when one line per label would make a table scroll sideways that fits with its labels wrapped, they wrap at their words as before; they also wrap when whole labels would squeeze another column onto more lines and so make the table taller. A table that scrolls either way keeps them on one line when it is no taller for it. It measures again when the box, the rows a filter or sort leaves, or the fonts change, once a frame for however many tables. Until the runtime has measured, a wide screen keeps labels whole and a phone wraps them. Longer text still wraps, and without scripts and on paper a label may still break.

## FEA-CH-53 — crisp ring on pointer hover (23 November 2026)

Daniel chose “I've definitely in between. Without receding” and “we do crisp ring”. The prototype has a 1.5 px paper edge, a 1.25 px ring in the mark colour at 80%, and a faint 4 px tail at 12%. Ring and tail use the mark’s clip. On 24 November the lead clarified that a line point’s ring crosses an axis with the point, and a scatter ring shares the grown point’s 8 px allowance. He clarified “pointer hover thanks” at 21:38 with “A: the same crisp ring as pointer hover”. STY-38 and FEA-CH-53 share the 80% ring. On 13 palette colours its contrast falls below 3:1, so only the ring lightness moves, to a target of 3.05:1 on the paper.
