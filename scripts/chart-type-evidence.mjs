// Host reason (ARC-03): validate compiled chart cohorts and real browser evidence.

/** Read the first list in a uniquely named guide section, refusing truncated or malformed guides. */
function guideInventory(guide, heading, entryPattern, label) {
  const lines = guide.replace(/\r\n/g, '\n').split('\n');
  const headings = lines.flatMap((line, index) => line === heading ? [index] : []);
  if (headings.length !== 1) throw new Error(`Compiled ${label} has no unique inventory section`);
  const start = headings[0] + 1;
  const next = lines.findIndex((line, index) => index >= start && /^#{1,3} /.test(line));
  if (next < 0) throw new Error(`Compiled ${label} inventory section is truncated`);
  const blocks = lines.slice(start, next).join('\n').trim().split(/\n[ \t]*\n/);
  if (blocks.length < 3 || /^[-*] /m.test(blocks[2])) throw new Error(`Compiled ${label} has no complete inventory`);
  const entries = blocks[1].split('\n').map(line => {
    const match = line.match(entryPattern);
    if (!match) throw new Error(`Unrecognised compiled ${label} entry: ${line}`);
    return match[1];
  });
  if (!entries.length || new Set(entries).size !== entries.length) throw new Error(`Compiled ${label} inventory is empty or duplicated`);
  return entries;
}

/** Read the registry-generated chart vocabulary published by the tested binary. */
export function chartTypes(guide) {
  return guideInventory(guide, '### Chart types', /^- ([a-z][a-z0-9-]*) \([^()]*\): \S/, 'chart-contract');
}

/** Read builtin styles from the tested native guide without copying its inventory. */
export function builtinStyles(guide) {
  return guideInventory(guide, '### The builtin styles', /^- \*\*([a-z][a-z-]*)\*\*[:,] \S/, 'style guide');
}

/** Check selectable labels independently of PDF serialization, preserving multiplicities. */
export function selectableLabels(runs, fragments) {
  const mode = runs.some(run => run.angle % 90 !== 0) ? 'non-space-characters' : 'words';
  const items = texts => mode === 'words' ? texts.join(' ').split(/\s+/u).filter(Boolean) : Array.from(texts.join('').replace(/\s/gu, ''));
  const expected = items(runs.map(run => run.text)), actual = items(fragments);
  const remaining = new Map();
  for (const item of expected) remaining.set(item, (remaining.get(item) ?? 0) + 1);
  for (const item of actual) remaining.set(item, (remaining.get(item) ?? 0) - 1);
  const missing = [...remaining].filter(([, count]) => count > 0), extra = [...remaining].filter(([, count]) => count < 0);
  return { ok: !missing.length && !extra.length, mode, expected: expected.length, actual: actual.length, missing, extra };
}

/** Match the ordered intrinsic SVG papers without assuming captions precede the drawing. */
export function chartPaperRoots(shapesByPage, sources) {
  if (!sources.length || sources.some(s => !Number.isFinite(s.width) || !Number.isFinite(s.height) || s.width <= 0 || s.height <= 0)) throw new Error('Invalid intrinsic chart dimensions');
  // Chrome stores SVG scalar dimensions as Float32. This is root identification only;
  // all painted controls, transforms and clips still compare exactly as decoded.
  const same = (args, source) => args.length === 4 && [0, 0, source.width, source.height].every((value, i) => Math.fround(value) === Math.fround(args[i]));
  const roots = shapesByPage.flatMap((shapes, page) => shapes.filter(shape => shape.kind === 'path' && shape.source.length === 1 && shape.source[0].op === 're' && sources.some(source => same(shape.source[0].args, source))).map(shape => ({ page: page + 1, shape })));
  if (roots.length !== sources.length) throw new Error('Intrinsic chart paper count: expected ' + sources.length + ', got ' + roots.length);
  if (!roots.every((root, i) => same(root.shape.source[0].args, sources[i]))) throw new Error('Intrinsic chart paper order or dimensions differ');
  return roots;
}

/** Account once for every vector paint intersecting a chart frame, including page transforms. */
export function framePaints(shapesByPage, charts) {
  const claimed = new Set(charts.flatMap(chart => chart.marks));
  const pages = shapesByPage.map((shapes, i) => {
    const frames = charts.filter(chart => chart.page === i + 1).map(chart => chart.frame);
    return shapes.filter(shape => shape.kind === 'path' && frames.some(frame => shape.left <= frame.right && shape.left + shape.width >= frame.left && shape.top <= frame.bottom && shape.top + shape.height >= frame.top));
  });
  const classification = { chart_marks: 0, rectangular_container_paints: 0, additional_vector_paints: 0 };
  for (const shape of pages.flat()) {
    if (claimed.has(shape)) classification.chart_marks++;
    else if (shape.source.length === 1 && shape.source[0].op === 're' && ['f', 'f*', 'F'].includes(shape.op)) classification.rectangular_container_paints++;
    else classification.additional_vector_paints++;
  }
  return { pages, classification };
}

/** Require a nonempty result for every compiled type, with no extra or duplicate chart identities. */
export function requireTypes(expected, charts) {
  if (!expected.length || new Set(expected).size !== expected.length) throw new Error('Invalid expected type inventory');
  const types = new Set(charts.map(chart => chart.type));
  const missing = expected.filter(type => !types.has(type));
  const extra = [...types].filter(type => !expected.includes(type));
  const ids = charts.map(chart => chart.id);
  if (missing.length || extra.length || new Set(ids).size !== ids.length || ids.some(id => !id)) {
    throw new Error(`Incomplete chart cohort: missing=${missing.join(',')} extra=${extra.join(',')} duplicate/empty identities=${ids.length - new Set(ids).size || ids.some(id => !id)}`);
  }
  return { charts: charts.length, types: types.size };
}

/** The accessible reading is the selected observation, including its facet context. */
export function announcement(geometry, target) {
  const panel = geometry.panels[target?.panel];
  const observation = panel?.observations[target?.observation];
  if (!observation) throw new Error(`No observation at ${JSON.stringify(target)}`);
  return [...(panel.facet == null ? [] : [panel.facet]), observation.readout.heading,
    ...observation.readout.lines.map(line => `${line.name}: ${line.value}`)].join(', ');
}

/**
 * Match decoded paints in order, preserving controls, affine stroke scale, paint and clipping.
 * A `gs` state needs its canonical `resolved` PDF resource dictionary; a name alone fails.
 */
export function matchPainted(expected, actual) {
  const scalar = value => {
    if (!Number.isFinite(value)) throw new Error('Non-finite PDF primitive');
    return value;
  };
  const point = value => {
    if (!Array.isArray(value) || value.length !== 2) throw new Error('Incomplete PDF control point');
    return value.map(scalar);
  };
  const path = commands => commands.map(command => {
    if (command.c === 'z') return { c: 'z' };
    if (command.c === 'm' || command.c === 'l') return { c: command.c, p: point(command.p) };
    if (command.c === 'c') return { c: 'c', c1: point(command.c1), c2: point(command.c2), p: point(command.p) };
    throw new Error(`Unsupported PDF path command ${command.c}`);
  });
  const signature = shape => {
    if (shape.kind !== 'path') throw new Error(`Unsupported PDF primitive ${shape.op ?? shape.kind}`);
    const commands = path(shape.commands);
    const token = value => /^[+-]?(?:\d+\.?\d*|\.\d+)$/.test(value) ? scalar(Number(value)) : value;
    const paint = shape.paintState.map(state => {
      if (state.op !== 'gs') return { op: state.op, args: state.args.map(token) };
      if (!state.resolved || typeof state.resolved !== 'object' || Array.isArray(state.resolved)) throw new Error('Unresolved PDF ExtGState');
      return { op: state.op, resolved: state.resolved };
    });
    // Geometry is in page space; stroke widths still depend on the CTM when the path paints.
    if (!Array.isArray(shape.matrix) || shape.matrix.length !== 6) throw new Error('Missing PDF paint matrix');
    const affine = shape.matrix.slice(0, 4).map(scalar);
    const clips = shape.clips.map(clip => ({ rule: clip.rule, commands: path(clip.commands) }));
    return JSON.stringify({ op: shape.op, commands, affine, paint, clips });
  };
  const candidates = new Map(), unsupported = [];
  for (const [index, shape] of actual.entries()) {
    try {
      const key = signature(shape);
      if (!candidates.has(key)) candidates.set(key, []);
      candidates.get(key).push(index);
    } catch (error) { unsupported.push({ side: 'actual', index, reason: error.message }); }
  }
  const matches = [], missing = [];
  for (const [index, shape] of expected.entries()) {
    try {
      const key = signature(shape), found = candidates.get(key)?.shift();
      if (found === undefined) missing.push(index);
      else matches.push({ expected: index, actual: found });
    } catch (error) { unsupported.push({ side: 'expected', index, reason: error.message }); }
  }
  const extra = [...candidates.values()].flat();
  const reordered = matches.filter((match, i) => i > 0 && match.actual < matches[i - 1].actual);
  return { ok: expected.length > 0 && !missing.length && !extra.length && !unsupported.length && !reordered.length, matches, missing, extra, unsupported, reordered };
}
