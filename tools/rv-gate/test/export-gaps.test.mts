// The PDF check's gap rule (FEA-RP-43): a page may end more than a quarter empty when the next
// page opens a section, or a run of figures that ends its section, and its first unit does not fit.
import assert from "node:assert/strict";
import { test } from "node:test";
import * as gaps from "../../../scripts/export-gaps.ts";
import { parseBlocks } from "../../../scripts/export-types.ts";

const { gapAccepted, norm, openingOf } = gaps;

const PAGE = 842;
const margins = [20 / 297, 22 / 297];
const textHeight = PAGE * (1 - margins[0] - margins[1]);
const sectionStarts = ["Money: Anthropic crossover", "Next section"];
const figures = [
	{ title: "First chart title that is long", source: "Sources: first note text here" },
	{ title: "Second chart title that is long", source: "Sources: second note text here" },
];
type TextBlock = Parameters<typeof openingOf>[0][number];
// Historical PDF text-only evidence has vertical coordinates and no horizontal bounds.
const block = (top: number, bottom: number, text: string) => ({ top, bottom, text: norm(text) }) as TextBlock;
// Untagged captions retain the historical serialized null for a missing source note.
const absentSource = null as unknown as Parameters<typeof openingOf>[1]["figures"][number]["source"];
type Opening = ReturnType<typeof openingOf>;
const open = (blocks: TextBlock[]) => openingOf(blocks, { pageHeight: PAGE, margins, sectionStarts, figures });
const first = [block(60, 330, "First chart title that is long"), block(335, 360, "Sources: first note text here")];
const second = [block(365, 600, "Second chart title that is long"), block(605, 630, "Sources: second note text here")];

test("two figures in a row followed by a section opening end that section", () => {
	const page = open([...first, ...second, block(660, 700, "Next section")]);
	assert.ok(page);
	assert.equal(page.lead, true);
	assert.equal(page.lead ? page.ends_section : undefined, true);
	assert.ok(gapAccepted({ gap: 0.4, textHeight, next: page, readPage: () => assert.fail("no read needed") }));
});

test("two figures in a row followed by text do not end their section", () => {
	const page = open([...first, ...second, block(660, 700, "The text after both charts")]);
	assert.ok(page);
	assert.equal(page.lead ? page.ends_section : undefined, false);
	assert.equal(gapAccepted({ gap: 0.4, textHeight, next: page, readPage: () => null }), false);
});

test("a run that goes on over the page break is read on the next page", () => {
	const cut = open([...first, block(365, 700, "Second chart title that is long")]);
	assert.ok(cut);
	assert.equal(cut.lead ? cut.ends_section : undefined, null);
	const ends = (page: Opening) => gapAccepted({ gap: 0.4, textHeight, next: cut, readPage: () => page });
	assert.equal(ends(open([block(60, 100, "Next section"), block(105, 300, "Body")])), true, "a section opens next");
	assert.equal(ends(null), true, "the report ends");
	assert.equal(ends(open([block(60, 100, "Body text goes on")])), false, "text follows the run");
});

test("a gap is not accepted when the first unit fits in it", () => {
	const page = open([...first, block(400, 440, "Next section")]);
	assert.equal(gapAccepted({ gap: 0.5, textHeight, next: page, readPage: () => null }), false);
});

test("a section opening is accepted when its heading and next block do not fit", () => {
	const page = open([block(60, 90, "Next section"), block(100, 450, "A tall paragraph")]);
	assert.ok(page);
	assert.equal(page.opens_section, true);
	assert.ok(gapAccepted({ gap: 0.45, textHeight, next: page, readPage: () => null }));
});

test("a source note that breaks into several blocks ends with its last piece", () => {
	const note = "Sources: first note text here and it goes on and on over several lines of the page";
	const chart = [{ title: "First chart title that is long", source: note }];
	const blocks = [block(60, 330, "First chart title that is long"), block(335, 350, "Sources: first note text here and it goes on"), block(351, 365, "and on over several lines"), block(366, 380, "of the page"), block(400, 440, "Next section")];
	const page = openingOf(blocks, { pageHeight: PAGE, margins, sectionStarts, figures: chart });
	assert.ok(page);
	assert.equal(page.lead ? page.ends_section : undefined, true);
	assert.equal(page.height, 320);
});

const frame = { title: "A complete chart caption", subtitle: "A subtitle with meaning", source: absentSource };
const taggedFigure = { "/S": "/Figure", "/Alt": "u:A complete chart caption. Three bars.", "/Pg": "2 0 R", "/K": [32, "4 0 R"] };
type FigureTags = Parameters<typeof gaps.figureFrames>[0];
const tags: FigureTags = { pages: [{ object: "2 0 R" }], qpdf: [{}, { "obj:3 0 R": { value: taggedFigure } }] };
const content = "1 0 0 -1 0 1000 cm /Figure <</MCID 32>>BDC 10 50 400 300 re f EMC";
const b = (top: number, bottom: number, text: string) => ({ left: 20, right: 300, top, bottom, text: gaps.norm(text) });
const plot = [b(60, 70, "Value"), b(90, 110, "100"), b(360, 380, frame.title), b(390, 420, frame.subtitle)];
const next = b(500, 530, "Next section");
const inspect = (blocks: TextBlock[], document = tags, stream = content, gap = 0.32) => {
	const frames = gaps.figureFrames?.(document, [stream], 1000, [frame])?.[0] ?? [];
	const opening = gaps.openingOf(blocks, {
		pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section", "New heading"], figures: [frames[0] ?? frame],
	});
	return {
		opening,
		accepted: gaps.gapAccepted({ gap, textHeight: 1000, next: opening, readPage: () => assert.fail("The next block is already known") }),
	};
};

test("a caption-below chart uses its tagged frame and subtitle at a section boundary", () => {
	const result = inspect([...plot, next]);
	assert.equal(result.accepted, true, "The complete 370 pt chart cannot fit the 320 pt gap and ends its section");
	assert.ok(result.opening);
	assert.equal(result.opening.height, 370);
	assert.equal(result.opening.lead, true);
	assert.equal(result.opening.lead ? result.opening.ends_section : undefined, true);
});

test("a section opening counts its complete following chart instead of one axis label", () => {
	const result = inspect([b(10, 30, "New heading"), ...plot, next]);
	assert.equal(result.accepted, true);
	assert.ok(result.opening);
	assert.equal(result.opening.opens_section, true);
	assert.equal(result.opening.height, 410);
});

test("tagged figure metadata never excuses body text, fitting figures or a missing caption", () => {
	const refused = [
		inspect([b(5, 25, "Body before the chart"), ...plot, next]),
		inspect([{ ...b(60, 70, "Body beside the chart"), left: 500, right: 700 }, ...plot, next]),
		inspect([...plot, b(500, 530, "Body after the chart")]),
		inspect([...plot, next], tags, content, 0.4),
		inspect(plot.filter(block => block.text !== gaps.norm(frame.title)).concat(next)),
	];
	for (const result of refused) assert.equal(result.accepted, false);
	const unbound: FigureTags[] = [
		{ ...tags, qpdf: [{}, {}] },
		{ ...tags, qpdf: [{}, { "obj:3 0 R": { value: { ...taggedFigure, "/Alt": "u:Another chart. Three bars." } } }] },
	];
	for (const document of unbound) assert.equal(inspect([...plot, next], document).accepted, false);
	assert.equal(inspect([...plot, next], tags, content.replace("/MCID 32", "/MCID 9")).accepted, false);
});

test("caption fields are consumed once before repeated body text or a next heading", () => {
	for (const repeated of [frame.title, frame.subtitle, "A subtitle", "with meaning", "meaning"]) {
		assert.equal(inspect([...plot, b(450, 470, repeated), next]).accepted, false, `Body text: ${repeated}`);
		const opening = gaps.openingOf([b(10, 30, "New heading"), ...plot, b(450, 470, repeated)], {
			pageHeight: 1000, margins: [0, 0], sectionStarts: ["New heading", repeated],
			figures: [{ ...frame, frame: { left: 10, top: 50, right: 410, bottom: 350 } }],
		});
		assert.equal(gaps.gapAccepted({ gap: 0.43, textHeight: 1000, next: opening, readPage: () => null }), false, `Next heading: ${repeated}`);
	}
});

test("wrapped caption fragments reconstruct their complete fields before stopping", () => {
	const wrapped = [...plot.slice(0, -1), b(390, 405, "A subtitle"), b(410, 420, "with meaning")];
	const complete = inspect([...wrapped, next]);
	assert.equal(complete.accepted, true);
	assert.ok(complete.opening);
	assert.equal(complete.opening.height, 370);
	assert.equal(inspect([...wrapped, b(450, 470, "A subtitle"), next]).accepted, false);
	assert.equal(inspect([...plot.slice(0, -1), b(390, 420, "A subtitle"), next]).accepted, false, "An incomplete field provides no tagged-frame exception");
	assert.equal(inspect([...plot.slice(0, -1), { ...plot[plot.length - 1], left: 500, right: 700 }, next]).accepted, false, "Caption ownership stays inside the frame width");
});

test("captions above a frame are reconstructed in reverse from the frame edge", () => {
	const opening = gaps.openingOf([b(5, 15, frame.title), b(20, 30, "A subtitle"), b(35, 45, "with meaning"), ...plot.slice(0, 2), next], {
		pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"],
		figures: [{ ...frame, frame: { left: 10, top: 50, right: 410, bottom: 350 } }],
	});
	assert.ok(opening);
	assert.equal(opening.height, 345);
	assert.equal(opening.lead ? opening.ends_section : undefined, true);
	assert.equal(gaps.gapAccepted({ gap: 0.32, textHeight: 1000, next: opening, readPage: () => assert.fail("No read needed") }), true);
});

test("a completed source note cannot consume repeated body text", () => {
	const source = "Sources: Anthropic's evidence.";
	for (const repeated of [source, "Anthropic's evidence.", "Sources:"]) {
		const opening = gaps.openingOf([...plot, b(430, 450, source), b(470, 490, repeated), b(520, 540, "Next section")], {
			pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"],
			figures: [{ ...frame, source, frame: { left: 10, top: 50, right: 410, bottom: 350 } }],
		});
		assert.ok(opening);
		assert.equal(opening.lead ? opening.ends_section : undefined, false, repeated);
		assert.equal(gaps.gapAccepted({ gap: 0.32, textHeight: 1000, next: opening, readPage: () => null }), false);
	}
});

test("PDF quote normalization keeps a fitting curly-quoted source unit refused", () => {
	for (const source of ["Sources: Anthropic’s report", "Sources: “quoted” report"]) {
		const xml = `<doc><page width="600" height="1000"><flow>
<block xMin="10" yMin="50" xMax="400" yMax="70"><line><word>Next section</word></line></block>
<block xMin="10" yMin="80" xMax="400" yMax="100"><line><word>Chart caption</word></line></block>
<block xMin="10" yMin="110" xMax="400" yMax="140"><line><word>${source}</word></line></block>
</flow></page></doc>`;
		const [{ blocks }] = parseBlocks(xml);
		const opening = gaps.openingOf(blocks, {
			pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"], figures: [{ title: "Chart caption", source }],
		});
		assert.equal(gaps.gapAccepted({ gap: 0.3, textHeight: 1000, next: opening, readPage: () => null }), false, "A source note with curly quotes must retain its real fitting height");
		assert.ok(opening);
		assert.equal(opening.height, 90);
	}
});

test("a tagged run needs physical ownership for each following figure", () => {
	const first = { ...frame, frame: { left: 10, top: 50, right: 410, bottom: 350 } };
	const borrowed = gaps.openingOf([...plot, b(450, 470, "A subtitle"), next], {
		pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"], figures: [first, { title: "A subtitle", source: absentSource }],
	});
	assert.equal(gaps.gapAccepted({ gap: 0.32, textHeight: 1000, next: borrowed, readPage: () => null }), false, "A later figure's title cannot turn intervening body text into a figure");
	const second = { title: "Second chart", subtitle: "Its own subtitle", source: absentSource, frame: { left: 10, top: 450, right: 410, bottom: 650 } };
	const run = gaps.openingOf([...plot, b(460, 470, "Value"), b(490, 510, "200"), b(660, 680, second.title), b(690, 710, second.subtitle), b(750, 770, "Next section")], {
		pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"], figures: [first, second],
	});
	assert.ok(run);
	assert.equal(run.height, 370, "The first unit still decides whether the run fits");
	assert.equal(run.lead ? run.ends_section : undefined, true);
	assert.equal(gaps.gapAccepted({ gap: 0.32, textHeight: 1000, next: run, readPage: () => assert.fail("No read needed") }), true);
});


test("empty normalized blocks cannot extend tagged captions or source notes", () => {
	const blank = { ...b(330, 600, " \u00ad\n"), text: gaps.norm(" \u00ad\n") };
	const figure = { title: "Revenue by region", source: absentSource, frame: { left: 10, top: 100, right: 410, bottom: 300 } };
	const opening = gaps.openingOf([b(110, 120, "Value"), b(310, 325, figure.title), blank, b(610, 630, "Next section")], {
		pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"], figures: [figure],
	});
	assert.equal(blank.text, "");
	assert.ok(opening);
	assert.equal(opening.height, 225, "Empty caption fragments must not stretch the physical unit");
	assert.equal(gaps.gapAccepted({ gap: 0.28, textHeight: 1000, next: opening, readPage: () => null }), false);
	const withSource = gaps.openingOf([...plot, b(430, 450, ""), b(460, 480, "Sources: Actual evidence"), b(520, 540, "Next section")], {
		pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"],
		figures: [{ ...frame, source: "Sources: Actual evidence", frame: { left: 10, top: 50, right: 410, bottom: 350 } }],
	});
	assert.equal(gaps.gapAccepted({ gap: 0.32, textHeight: 1000, next: withSource, readPage: () => null }), false, "An empty source fragment provides no tagged-frame exception");
});

test("a section heading that repeats a chart title is not the chart's caption", () => {
	const figure = { title: "Revenue by region", source: absentSource, frame: { left: 10, top: 100, right: 410, bottom: 300 } };
	const open = (blocks: TextBlock[], sectionStarts: string[]) => gaps.openingOf(blocks, { pageHeight: 1000, margins: [0, 0], sectionStarts, figures: [figure] });
	const accepted = (next: Opening) => gaps.gapAccepted({ gap: 0.28, textHeight: 1000, next, readPage: () => null });
	// The heading and its paragraph (70 pt) fit the 280 pt gap; the chart's own caption sits below its empty frame.
	const heading = open([b(10, 30, figure.title), b(40, 80, "Some paragraph text here"), b(310, 325, figure.title), b(400, 420, "Next section")], ["Next section", figure.title]);
	assert.ok(heading);
	assert.equal(heading.opens_section, true);
	assert.equal(heading.lead, undefined, "The heading is not a figure");
	assert.equal(heading.height, 70);
	assert.equal(accepted(heading), false, "A fitting heading and paragraph leave a refused gap");
	// Without a section opening, a caption above the frame is still the chart's, and body text after it refuses the gap.
	const above = open([b(10, 30, figure.title), b(320, 340, "Body text continues")], ["Next section"]);
	assert.ok(above);
	assert.equal(above.lead ? above.ends_section : undefined, false);
	assert.equal(accepted(above), false);
	// The caption nearest the frame wins whichever side it is on.
	const below = open([b(10, 30, figure.title), b(40, 80, "Some paragraph text here"), b(310, 325, figure.title), b(400, 420, "Next section")], ["Next section"]);
	assert.ok(below);
	assert.equal(below.lead, undefined);
	assert.equal(below.height, 70);
});

test("a printed legend between a chart's frame and its caption belongs to the figure (FEA-CH-67)", () => {
	const keyed = { ...frame, legend: ["Enterprise", "Mid-market"], frame: { left: 10, top: 50, right: 410, bottom: 350 } };
	const open = (blocks: TextBlock[], figure: Omit<typeof keyed, "legend"> & { legend?: string[] } = keyed) => gaps.openingOf(blocks, { pageHeight: 1000, margins: [0, 0], sectionStarts: ["Next section"], figures: [figure] });
	const accepted = (opening: Opening) => gaps.gapAccepted({ gap: 0.32, textHeight: 1000, next: opening, readPage: () => null });
	const inside = plot.slice(0, 2), title = b(370, 385, frame.title), subtitle = b(390, 420, frame.subtitle);
	// Hand-drawn prints the legend under the plot and the caption under the legend (sales-report, 5 Nov).
	const below = open([...inside, b(355, 365, "Enterprise Mid-market"), title, subtitle, next]);
	assert.ok(below);
	assert.equal(below.lead, true);
	assert.equal(below.height, 370, "The unit runs from the frame through the legend and the caption");
	assert.equal(accepted(below), true);
	// Each entry may be its own block, and the subtitle may start with a series' name: the legend comes first.
	const starts = { ...keyed, subtitle: "Enterprise contributed most of the rise" };
	const split = open([...inside, b(355, 365, "Enterprise"), { ...b(355, 365, "Mid-market"), left: 120 }, title, b(390, 420, starts.subtitle), next], starts);
	assert.ok(split);
	assert.equal(split.height, 370);
	assert.equal(accepted(split), true);
	// A style that puts the caption and legend above the frame is read back from the frame's edge.
	const above = open([b(5, 15, frame.title), b(20, 30, frame.subtitle), b(35, 45, "Enterprise Mid-market"), ...inside, next]);
	assert.ok(above);
	assert.equal(above.height, 345);
	assert.equal(accepted(above), true);
	// A chart whose plot names its series prints no legend; its caption is still its own.
	assert.equal(accepted(open([...inside, title, subtitle, next])), true);
	// Read, the legend must be whole: a part of it gives no exception.
	assert.equal(accepted(open([...inside, b(355, 365, "Enterprise"), title, subtitle, next])), false);
	// Without the legend's names, the caption under it is not reached, as before this rule.
	const { legend: _, ...unkeyed } = keyed;
	assert.equal(accepted(open([...inside, b(355, 365, "Enterprise Mid-market"), title, subtitle, next], unkeyed)), false);
});
