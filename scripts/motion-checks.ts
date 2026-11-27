// The decisions of the motion check (FEA-CH-75, DEL-75, DEL-30), kept apart from the browser so
// tests can feed them numbers. The verdicts take shares of differing pixels, measured against
// the settled frame; `compareFrames` is the one function that also runs inside the page, so it
// reads nothing outside its own body.

/** What counts as a different pixel, and how much difference each check lets through. */
export const LIMITS = {
  // Largest per-channel difference that still counts as the same colour (antialiasing noise).
  channel: 16,
  // Marks may grow back by this much of their box between two fractions (spring-like curves
  // overshoot and settle back; antialiasing moves a few edge pixels).
  progressSlack: 0.02,
  // The entrance has to start away from rest by at least this share of the marks' box.
  startAway: 0.02,
  // Share of the still parts' pixels allowed to differ at any frame.
  still: 0.005,
  // DEL-30: the last and the reduced-motion frame differ from the settled frame in at most this
  // share of the chart's pixels.
  frame: 0.005,
};

/** The fractions of the entrance a filmstrip shows; the last is the final drawn frame before it settles. */
export const FRACTIONS = [0, 0.25, 0.5, 0.75, 1];

/**
 * Which of the frames 0..last (one per 16 ms step, the last the settled draw) show the fractions.
 * 100 % is the last frame still animating, because the settled draw is the settled frame by
 * construction and would prove nothing.
 */
export function frameIndexes(last: number) {
  const animated = Math.max(last - 1, 0);
  return FRACTIONS.map((f) => (f === 1 ? animated : Math.min(Math.round(f * last), animated)));
}

/**
 * Shares of pixels where frame `a` differs from `b` (RGBA bytes): inside the marks' mask, inside
 * the still parts' mask, and over the whole chart. A share is null where its mask is empty.
 * A still pixel that differs only because a mark passes over it (a floating waterfall bar over a
 * gridline, an overshoot) is not a still part moving: it counts as the same when its colour lies
 * between the rest colour and a colour of the chart's marks (`palette`, flat RGB triples).
 */
export function compareFrames(a: ArrayLike<number>, b: ArrayLike<number>, masks: { mark: ArrayLike<number>; still: ArrayLike<number> }, channel: number, palette: ArrayLike<number> = []) {
  // True when pixel `o` of `a` lies on a segment between two of: the rest colour and the marks' colours.
  const near = (o: number, p: number[], q: number[]) => {
    const d = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    const norm = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if (!norm) return false;
    const t = Math.min(1, Math.max(0, ((a[o] - p[0]) * d[0] + (a[o + 1] - p[1]) * d[1] + (a[o + 2] - p[2]) * d[2]) / norm));
    return Math.abs(a[o] - p[0] - t * d[0]) <= channel && Math.abs(a[o + 1] - p[1] - t * d[1]) <= channel && Math.abs(a[o + 2] - p[2] - t * d[2]) <= channel;
  };
  const covered = (o: number) => {
    const points = [[b[o], b[o + 1], b[o + 2]]];
    for (let k = 0; k < palette.length; k += 3) points.push([palette[k], palette[k + 1], palette[k + 2]]);
    for (let i = 0; i < points.length; i++) for (let j = i + 1; j < points.length; j++) if (near(o, points[i], points[j])) return true;
    return false;
  };
  let mark = 0, markAll = 0, still = 0, stillAll = 0, all = 0;
  const pixels = a.length / 4;
  for (let i = 0; i < pixels; i++) {
    const o = i * 4;
    const differs =
      Math.abs(a[o] - b[o]) > channel || Math.abs(a[o + 1] - b[o + 1]) > channel || Math.abs(a[o + 2] - b[o + 2]) > channel || Math.abs(a[o + 3] - b[o + 3]) > channel;
    if (masks.mark[i]) { markAll++; if (differs) mark++; }
    if (masks.still[i]) { stillAll++; if (differs && !covered(o)) still++; }
    if (differs) all++;
  }
  return { mark: markAll ? mark / markAll : null, still: stillAll ? still / stillAll : null, all: all / pixels, markPixels: markAll, stillPixels: stillAll };
}

const pct = (share: number) => `${(share * 100).toFixed(1)} %`;

/** Marks move towards their settled drawing: the share that differs never grows, and starts above the end. */
export function progressVerdict(shares: (number | null)[], limits = LIMITS) {
  const known = shares.filter((share) => share !== null);
  if (known.length < shares.length || !shares.length) return { ok: true, detail: "no marks drawn to measure" };
  for (let i = 1; i < known.length; i++) {
    if (known[i] > known[i - 1] + limits.progressSlack) {
      return { ok: false, detail: `marks move away from rest between ${i - 1} and ${i} of the fractions: ${pct(known[i - 1])} then ${pct(known[i])} differ` };
    }
  }
  if (known[0] < limits.startAway) return { ok: false, detail: `the entrance shows nothing at 0 %: only ${pct(known[0])} of the marks' box differs from rest` };
  return { ok: true, detail: `${known.map(pct).join(" > ")}` };
}

/** Axes, titles and legends do not move: no frame differs from rest in more than the allowed share of their pixels. */
export function stillVerdict(shares: (number | null)[], limits = LIMITS) {
  const known = shares.filter((share) => share !== null);
  if (!known.length) return { ok: true, detail: "no still parts to measure" };
  const worst = Math.max(...known);
  return worst > limits.still
    ? { ok: false, detail: `still parts differ from rest by up to ${pct(worst)} of their pixels during the entrance (limit ${pct(limits.still)})` }
    : { ok: true, detail: `at most ${pct(worst)}` };
}

/** A frame that must equal the settled frame (DEL-30's 0.5 % per chart). */
export function equalVerdict(share: number, what: string, limits = LIMITS) {
  return share > limits.frame
    ? { ok: false, detail: `${what} differs from the settled frame in ${pct(share)} of the chart's pixels (limit ${pct(limits.frame)})` }
    : { ok: true, detail: pct(share) };
}

/** A chart passes when every check it has passes; the reasons name what failed. */
export function chartVerdict(checks: Record<string, {ok: boolean; detail: string} | null | undefined>) {
  const failed = Object.entries(checks).flatMap(([name, check]) => check?.ok === false ? [`${name}: ${check.detail}`] : []);
  return { ok: failed.length === 0, failures: failed };
}
