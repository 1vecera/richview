// The PDF check of every chart type (FEA-CH-75): a raster counts against a chart only where it
// covers the chart's box, and a label counts only when the chart itself drew it.
import assert from "node:assert/strict";
import { test } from "node:test";
import { drawnLabels, labelInStream, labelOnPage, locateTitles, norm, overlapShare, parseImages, parseWords } from "../../../scripts/export-types.mjs";

const box = { left: 0, top: 100, width: 500, height: 200 };

test("selectable PDF words retain grouped digits and XML characters independently of styled fragments", () => {
	const xml = '<doc><page width="595" height="842"><flow><word xMin="1" yMin="2" xMax="10" yMax="5">72</word><word xMin="11" yMin="2" xMax="25" yMax="5">1,705</word></flow></page><page width="595" height="842"><word xMin="3" yMin="4" xMax="8" yMax="9">A&amp;B&#x2212;2</word></page></doc>';
	assert.deepEqual(parseWords(xml), [[{ left: 1, top: 2, width: 9, height: 3, text: '72' }, { left: 11, top: 2, width: 14, height: 3, text: '1,705' }], [{ left: 3, top: 4, width: 5, height: 5, text: 'A&B−2' }]]);
	assert.deepEqual(parseWords('<pdf2xml><page><text top="2">7</text><text top="2">2</text></page></pdf2xml>'), [[]], 'text fragments are not a word inventory');
});

test("a raster over the chart covers it, an authored poster beside it does not", () => {
	assert.equal(overlapShare(box, { left: 0, top: 90, width: 500, height: 220 }), 1);
	assert.equal(overlapShare(box, { left: 50, top: 100, width: 400, height: 100 }), 0.4);
	assert.equal(overlapShare(box, { left: 0, top: 320, width: 500, height: 200 }), 0, "below the chart");
	assert.equal(overlapShare(box, { left: -300, top: -300, width: 1000, height: 800 }), 1, "a clipped full-page image");
});

test("image places are scaled back from pdftohtml's 1.5 factor", () => {
	const xml = '<page number="1" position="absolute" top="0" left="0" height="1262" width="900"><image top="150" left="90" width="450" height="300" src="x.png"/></page>';
	assert.deepEqual(parseImages(xml, [{ width: 600, height: 841 }]), [{ page: 1, left: 60, top: 100, width: 300, height: 200 }]);
});

test("only the chart's own text counts as a drawn label", () => {
	assert.deepEqual(drawnLabels(["Solo", "2025", "200", "Q1"], "Active workspaces, January to December 2025 Solo"), ["q1"]);
	assert.deepEqual(drawnLabels(["200", "400", "0"], ""), ["200", "400"], "numbers only: three characters or more");
});

test("a label is found in the box, not in the figure's HTML text nor above it", () => {
	const page = { blocks: [{ top: 40, bottom: 60, text: norm("Cost") }, { top: 120, bottom: 140, text: norm("Cost per seat") }, { top: 150, bottom: 170, text: norm("Europe") }] };
	assert.equal(labelOnPage(page, { top: 100, bottom: 360 }, ["europe"], ""), "europe");
	assert.equal(labelOnPage(page, { top: 100, bottom: 360 }, ["cost"], "Cost per seat"), null, "the title's words are HTML");
	assert.equal(labelOnPage(page, { top: 100, bottom: 360 }, ["cost"], ""), "cost", "a chart can draw the same word");
	assert.equal(labelOnPage({ blocks: [{ top: 40, bottom: 60, text: "europe" }] }, { top: 100, bottom: 360 }, ["europe"], ""), null, "above the box");
});

test("a repeated title takes the next occurrence and a missing one is null", () => {
	const block = (text) => ({ top: 10, bottom: 20, text: norm(text) });
	const pages = [{ blocks: [block("Same title here")] }, { blocks: [block("Other"), block("Same title here")] }];
	const places = locateTitles(pages, ["Same title here", "Same title here", "Same title here"]);
	assert.deepEqual(places.map((place) => place?.page ?? null), [1, 2, null]);
});

test("curved text split by layout analysis is found in the content stream after the title", () => {
	const raw = "Sunburst\nBudget by group\nE n g i n e e r i n g\nS a l e s\n";
	assert.equal(labelInStream(raw, "Budget by group", ["engineering"]), "engineering");
	assert.equal(labelInStream(raw, "Budget by group", ["finance"]), null);
	assert.equal(labelInStream(raw, "Another title", ["sales"]), null, "no title, no chart");
});

test("painted paths are read with their transforms, and a backdrop or a clip is not drawing (a chart with no label needs vector paths)", async () => {
	const { MIN_VECTOR_PATHS, pageStreams, paintedPaths, vectorPaths } = await import("../../../scripts/export-types.mjs");
	// The page flips with a 0.24 scale as Chrome writes it; y 100 in CSS pixels is 24 points from the top.
	const stream = [
		".24 0 0 -.24 0 800 cm",
		"q 0 0 1000 3000 re f Q", // a page backdrop
		"q 100 100 50 50 re f Q", // a cell
		"q 1 0 0 1 200 100 cm 0 0 m 50 0 l 50 50 l h f Q", // a triangle, moved
		"q 300 100 10 10 re W n Q", // a clip, never painted
		"q 400 100 m 450 100 l S Q", // a rule
		"BT /F1 10 Tf 1 0 0 -1 100 100 Tm <01> Tj ET",
	].join("\n");
	const paths = paintedPaths(stream, 800);
	assert.equal(paths.length, 4);
	assert.deepEqual(paths[1], { left: 24, top: 800 - 800 + 24, width: 12, height: 12 });
	const box = { left: 0, top: 20, width: 200, height: 100 };
	assert.equal(vectorPaths(paths, box), 3, "the backdrop is dropped, the clip never counted");
	assert.ok(MIN_VECTOR_PATHS >= 2);
	assert.equal(vectorPaths(paths, { left: 0, top: 400, width: 200, height: 100 }), 0, "drawing elsewhere on the page is no drawing in the box");
	const qdf = "%% Page 1\n%% Original object ID: 26 0\n44 0 obj\n<<\n  /Contents 176 0 R\n>>\nendobj\n\n176 0 obj\n<<\n  /Length 177 0 R\n>>\nstream\n0 0 5 5 re f\nendstream\nendobj\n";
	assert.deepEqual(pageStreams(qdf), ["0 0 5 5 re f"]);
});

test("QDF objects may put pages 42, 1, 37, 2 in that order; streams stay in numeric page order", async () => {
	const { pageStreams } = await import("../../../scripts/export-types.mjs");
	const order = [42, 1, 37, ...Array.from({ length: 40 }, (_, i) => i + 2).filter(n => n !== 37)];
	const pages = order.map(n => `%% Page ${n}\n%% Original object ID: ${100+n} 0\n${100+n} 0 obj\n<< /Contents ${n === 2 ? '[1002 0 R 2002 0 R]' : (1000+n)+' 0 R'} >>\nendobj\n`).join('');
	const stream = (id, text) => `${id} 0 obj\n<<>>\nstream\n${text}\nendstream\nendobj\n`;
	const qdf = pages + order.map(n => stream(1000+n, 'PAGE_'+n)).join('') + stream(2002, 'SECOND_CONTENT');
	assert.deepEqual(pageStreams(qdf), Array.from({ length: 42 }, (_, i) => 'PAGE_'+(i+1)+(i === 1 ? '\nSECOND_CONTENT' : '')));
	assert.throws(() => pageStreams(qdf.replace('%% Page 42\n', '%% Page 41\n')), /page.*order|duplicate|missing/i);
	assert.throws(() => pageStreams(qdf.replace('%% Page 42\n', '%% Page 43\n')), /page.*order|duplicate|missing/i);
});

test("painted primitives retain transformed Bezier controls and each source affine matrix", async () => {
	const { paintedShapes } = await import("../../../scripts/export-types.mjs");
	const shapes = paintedShapes('2 0 0 -2 10 100 cm q 1 0 .5 1 3 4 cm 1 2 m 3 4 5 6 7 8 c S Q 0 0 m 1 1 l S', 100);
	assert.equal(shapes.length, 2);
	assert.deepEqual(shapes[0].commands, [{ c: 'm', p: [20, 12] }, { c: 'c', c1: [26, 16], c2: [32, 20], p: [38, 24] }]);
	assert.equal(shapes[0].op, 'S');
	assert.deepEqual(shapes[0].source[1], { op: 'c', args: [3, 4, 5, 6, 7, 8], matrix: [2, 0, 1, -2, 16, 92] });
	assert.deepEqual(shapes[1].commands, [{ c: 'm', p: [10, 0] }, { c: 'l', p: [12, 2] }], 'Q restores the affine transform');
});

test("PDF shorthand controls and close operators keep their drawing semantics", async () => {
	const { paintedShapes } = await import("../../../scripts/export-types.mjs");
	const shapes = paintedShapes('10 10 m 20 20 30 10 v 40 20 50 10 y h 60 20 70 10 v s 0 0 10 20 re b* 1 1 m 2 2 l f* 0 0 99 99 re W n', 100);
	assert.deepEqual(shapes[0].commands, [
		{ c: 'm', p: [10, 90] }, { c: 'c', c1: [10, 90], c2: [20, 80], p: [30, 90] },
		{ c: 'c', c1: [40, 80], c2: [50, 90], p: [50, 90] }, { c: 'z' },
		{ c: 'c', c1: [10, 90], c2: [60, 80], p: [70, 90] }, { c: 'z' },
	]);
	assert.equal(shapes[0].op, 's');
	assert.equal(shapes[1].op, 'b*');
	assert.deepEqual(shapes[1].commands, [{ c: 'm', p: [0, 100] }, { c: 'l', p: [10, 100] }, { c: 'l', p: [10, 80] }, { c: 'l', p: [0, 80] }, { c: 'z' }]);
	assert.equal(shapes[2].op, 'f*', 'the fill rule remains explicit, including implicit fill closure');
	assert.equal(shapes.length, 3, 'a clipping path ending with n is not painted');
});

test("same bounds cannot hide a different curve or erase repeated painted marks", async () => {
	const { paintedPaths, paintedShapes } = await import("../../../scripts/export-types.mjs");
	const a = '0 0 m 50 0 50 100 100 100 c S';
	const b = '0 0 m 0 50 100 50 100 100 c S';
	assert.deepEqual(paintedPaths(a, 100), paintedPaths(b, 100), 'the previous bounds alone cannot distinguish these');
	assert.notDeepEqual(paintedShapes(a, 100)[0].commands, paintedShapes(b, 100)[0].commands);
	assert.equal(paintedShapes(a + '\n' + a, 100).length, 2, 'identical marks remain two separate paints');
	assert.notDeepEqual(paintedShapes(a, 100), paintedShapes(a + '\n' + a, 100), 'extra mark');
	assert.notDeepEqual(paintedShapes(a, 100), paintedShapes('', 100), 'missing mark');
	assert.notDeepEqual(paintedShapes(a, 100), paintedShapes('1 0 0 1 1 0 cm ' + a, 100), 'moved mark');
});

test("unsupported drawing operations are explicit and cannot fabricate paths from inline image bytes", async () => {
	const { paintedPaths, paintedShapes } = await import("../../../scripts/export-types.mjs");
	const shapes = paintedShapes('BT /F1 12 Tf (0 0 m 100 100 l S) Tj ET q 2 0 0 3 4 5 cm /Image1 Do /Shade1 sh Q bogus BI ID 0 0 m 9 9 l S EI', 100);
	assert.deepEqual(shapes.map(shape => [shape.kind, shape.op]), [['unsupported', 'Do'], ['unsupported', 'sh'], ['unsupported', 'bogus'], ['unsupported', 'BI']]);
	assert.equal(shapes[0].args[0], '/Image1');
	assert.deepEqual(shapes[0].matrix, [2, 0, 0, 3, 4, 5]);
	assert.deepEqual(paintedPaths('0 0 m 1 1 l S /Image1 Do', 100), [{ left: 0, top: 99, width: 1, height: 1 }]);
});

test("literal PDF text and comments cannot manufacture vector marks", async () => {
	const { paintedPaths, paintedShapes } = await import("../../../scripts/export-types.mjs");
	const stream = 'BT /F1% 0 0 m 99 99 l S\n12 Tf (text (nested) 0 0 m 10 10 l S) Tj ET % 0 0 10 10 re f\n1 2 m 3 4 l S';
	assert.equal(paintedShapes(stream, 10).length, 1);
	assert.equal(JSON.stringify(paintedPaths(stream, 10)), '[{"left":1,"top":6,"width":2,"height":2}]', 'legacy field order and bounds are unchanged');
	assert.throws(() => paintedShapes('(unterminated', 10), /Unterminated/);
	assert.throws(() => paintedShapes('1 2 3 4 v S', 10), /no current point/);
	assert.throws(() => paintedShapes('/bad 0 m S', 10), /Non-finite/);
});

test("stroke weight, colour and external paint state remain attached to each primitive", async () => {
	const { paintedShapes, paintedPaths } = await import("../../../scripts/export-types.mjs");
	const stream = '1 w 0 0 0 RG q 2 w 1 0 0 RG [3 2] 1 d /G8 gs 0 0 m 10 10 l S Q 0 0 m 10 10 l S';
	const shapes = paintedShapes(stream, 100);
	assert.deepEqual(shapes[0].paintState.slice(-4), [{ op: 'w', args: ['2'] }, { op: 'RG', args: ['1','0','0'] }, { op: 'd', args: ['[','3','2',']','1'] }, { op: 'gs', args: ['/G8'] }]);
	assert.deepEqual(shapes[1].paintState, [{ op: 'w', args: ['1'] }, { op: 'RG', args: ['0','0','0'] }], 'Q restores all inherited paint settings');
	assert.notDeepEqual(shapes[0].paintState, shapes[1].paintState, 'a changed weight cannot hide behind identical path controls');
	assert.deepEqual(paintedPaths(stream, 100)[0], paintedPaths(stream, 100)[1], 'legacy bounds stay geometry-only');
});

test("clip paths follow saved graphics state and restore after the figure", async () => {
	const { paintedShapes } = await import("../../../scripts/export-types.mjs");
	const shapes = paintedShapes('q 0 0 20 20 re W n q 0 0 20 20 re f Q q 1 1 m 10 10 l S Q Q 2 2 m 3 3 l S', 100);
	assert.equal(shapes.length, 3);
	assert.equal(shapes[0].clips[0].rule, 'W');
	assert.deepEqual(shapes[0].clips[0].commands, shapes[0].commands);
	assert.deepEqual(shapes[0].clips, shapes[1].clips);
	assert.deepEqual(shapes[2].clips, []);
});
