# Review queue: the next 70 key cards

These 70 cards were raised from `important` or `detail` to `key` on 2026-10-26, after Daniel had answered all 62 earlier key cards. They hold the decisions where his answer most shapes the product: first values (purpose, what RichView maximizes, quality bars, what it refuses and whom it serves), then features (what users and agents can do, and design that changes what they see). Technical and implementation choices stay with the agents and are not in this queue.

Review them in the order below, most important first. Cards carry no rank field, so `node tools/lockdown-app/cli.mjs next --importance key` lists the same cards in part order; this file is the order for the live review. Questions marked *reworded* were rewritten in plain words; their proposals are unchanged.

## Values (30)

- **INT-12** Visual and motion craft: Craft wins every conflict (INT-17); this sets the bar it is judged against and confirms the Mluva reference.
- **INT-77** Storytelling: Second target after craft (INT-17); defines what a well-argued story is and what `learn` teaches.
- **INT-21** Agent-native authoring: "Presentation layer for your agent" (INT-01) rests on this bet: what an agent does on its own.
- **INT-13** Short authoring path: Third target in INT-17's order; decides the least an agent writes per chart and what defaults carry.
- **INT-08** Core jobs: Names the jobs (write, check, present, revise, share) every surface serves.
- **INT-07** Secondary audience: readers of the stories: Who RichView serves besides authors, and what every reader gets on phone, desktop and PDF.
- **INT-19** One sourced report, many coherent styles: Headline bet behind five distinct styles (STY-01, STY-05): one report, every style, nothing half-styled.
- **INT-20** Reference-grade charts from a few lines of YAML: Headline bet behind "richest chart library" (FEA-CH-01): the craft bar for about 30 chart types.
- **INT-18** Self-checking charts: Headline bet or supporting feature; 12 open cards cite it.
- **INT-22** Git-native editorial publishing: Whether Publish's pull request, preview and approval loop stays a RichView bet.
- **INT-33** Dashboards: Scope refusal: whether dashboards are out of RichView, and when they could return.
- **INT-32** Charts: internal engine or standalone library: "Richest chart library" may mean a library others use on its own; decides product scope.
- **INT-80** No reader tracking: A refusal that hosted Cowork accounts and paid limits now test.
- **INT-30** Monetization only through paid hosted services: Whether the software stays free with no paid tiers, money coming only from hosted services.
- **INT-69** The fresh-agent bar: Quality bar for agent-native: what an agent that never saw RichView must produce unaided.
- **INT-82** How RichView is judged after launch: The one success signal after launch, and how feedback reaches Daniel.
- **INT-10** Flagship story: Whether the OpenAI-versus-Anthropic report stays the one proof of RichView.
- **INT-68** The flagship bar: Quality bar for done: the flagship in every style, at phone and desktop widths and as PDF, clearly better than today.
- **INT-09** Reference journey: The end-to-end journey the guide and the acceptance tests follow.
- **STY-50** RichView's design rules *(reworded)*: The design principles every style, chart and page is judged by.
- **INT-56** A style reaches everything it renders: Quality bar: a style reaches every element, charts and PDF included.
- **INT-61** Motion is smooth or absent: Quality bar on the richer animation INT-17 asks for: smooth on a real phone or not shipped.
- **INT-58** No clipped, colliding or garbled labels: Quality bar for chart text: never clipped, overlapped or garbled at any width.
- **INT-59** Controls and chrome recede: Design value: fewer, lighter, labelled controls on reports, charts and the site.
- **FEA-RP-49** Accessibility standard and its check: Whether reports commit to WCAG 2.2 AA, now that charts draw in WebGL.
- **INT-65** Claims match what ships: Honesty rule; conflicts with FEA-SF-30's home page that promises what the product will do.
- **INT-23** Which ambitions become public claims: Keeps "competition-winning" internal and decides what public pages may claim.
- **INT-45** Flagship findings in marketing: Whether marketing restates the flagship's findings under Daniel's name.
- **INT-76** What RichView is chosen over: Positioning against screenshots, chart libraries, chat artifacts, BI tools and notebooks.
- **FEA-SF-61** Crawler and AI-use policy: Whether richview.ai invites search, AI answers and AI training.

## Features (40)

- **STY-09** Sleek *(reworded)*: The default style everywhere (STY-06); its purpose and look come first.
- **STY-10** Air *(reworded)*: Must stop reading as "Sleek Light" and feel distinct (STY-01, STY-05) while Sleek prints as it.
- **STY-11** Scientific *(reworded)*: Purpose and look of the dense-evidence style.
- **STY-12** Brutalist *(reworded)*: Purpose and look of the heavy, opinionated style.
- **STY-13** Hand-drawn *(reworded)*: Purpose and look of the pencil style, including its red accent.
- **FEA-RP-35** What moves in a report: Whether text animates too, now that INT-17 asks for richer animation.
- **FEA-CH-51** Chart entrances: How charts animate in as the reader scrolls to them.
- **FEA-CH-53** Motion described but never built *(reworded)*: Which hover and interaction effects charts gain.
- **FEA-RP-48** Scrollytelling element: How the scrollytelling element INT-31 brings in behaves.
- **FEA-CH-39** Reader interactions per chart type *(reworded)*: What a reader can do with each chart type; every new type copies its row.
- **FEA-CH-43** Chart action row: Which actions every chart carries and whether they show labels; 13 open cards cite it.
- **FEA-CH-44** One expanded view: One full-screen chart view on every device instead of three.
- **FEA-CH-49** Zoom and pan: Where readers zoom, and whether phones keep pinch zoom inline.
- **FEA-BR-02** Playback *(reworded)*: How playable charts step and glide between frames.
- **FEA-BR-06** Selector presentation: Confirms Daniel's buttons-first, then dropdowns default for chart controls.
- **FEA-BR-03** Filter: Whether a filter offers an "all" option besides one value at a time.
- **FEA-BR-05** Chart variants *(reworded)*: Named alternative views of one chart, without composed groups.
- **FEA-BR-31** Axes during playback: Fixed or refitting axes while a chart plays.
- **FEA-RP-33** `theme: light|dark` and `--theme` *(reworded)*: Whether a report can flip light or dark, or each style is one scheme; 14 open cards cite it.
- **STY-02** Brand blue and style accents: Which blue is RichView's own and each style's accent, Hand-drawn's red included.
- **STY-53** Colours authors bring: Whether authors may bring their brand's colours.
- **STY-16** Authors' own fonts: Whether authors may use brand fonts RichView does not bundle.
- **FEA-CH-63** Stable colours for named entities: Pin a category's colour across a whole report.
- **FEA-CH-05** Stacked bars and areas: A missing chart form: opt-in stacked bars and areas.
- **FEA-CH-27** Series named at their line ends: How several line series are named: at the line end, alone or with a value.
- **FEA-CH-28** Sparse point labels and author highlights: How many points get name labels by default, and when leaders appear.
- **FEA-CH-03** Curves: monotone by default, straight and steps on request *(reworded)*: Smooth or straight lines by default; follows INT-11's note on smoothing.
- **FEA-CH-42** Readout panel: light, stable, never invented: How light the hover readout is: translucency, blur, movement.
- **FEA-CH-55** Accessible chart structure: A generated summary and named marks for every chart.
- **FEA-RP-43** PDF export *(reworded)*: What every PDF must get right on the page.
- **FEA-SF-36** Flagship report page: How the flagship page shows its styles and keeps the reading position.
- **FEA-SF-23** Live preview beside the source: The VS Code live preview, the main authoring surface (INT-26).
- **FEA-SF-35** Share a playground draft by link: Share a playground draft by link, with nothing uploaded.
- **FEA-SF-09** MCP tools *(reworded)*: What an agent can do through the MCP server.
- **STY-43** Switching style on a page: What readers see when they switch style on a page.
- **STY-44** Landing motion: Whether the home page keeps its scroll story.
- **STY-46** One look for the whole site: Which one look the site and RichView's own presentations share.
- **FEA-RP-06** “Made with RichView” colophon: Whether every report carries the "Made with RichView" link, and who can remove it.
- **FEA-RP-09** Source notes under blocks: Traceability in practice (INT-11): source notes, and a warning when a chart has none.
- **FEA-CH-46** SVG download: Left open for Daniel: keep an SVG download beside PNG.
