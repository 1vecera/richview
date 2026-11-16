// The motion check's decisions (FEA-CH-75, DEL-75): progress, still parts and frame equality.
import assert from "node:assert/strict";
import { test } from "node:test";
import { chartVerdict, compareFrames, equalVerdict, frameIndexes, LIMITS, progressVerdict, stillVerdict } from "../../../scripts/motion-checks.mjs";

const frame = (...pixels) => Uint8ClampedArray.from(pixels.flatMap((value) => [value, value, value, 255]));

test("progress may only shrink the share that differs from rest", () => {
  assert.equal(progressVerdict([1, 0.7, 0.3, 0.1, 0]).ok, true);
  assert.equal(progressVerdict([1, 0.7, 0.7, 0.7, 0.7]).ok, true, "holding still is not going back");
  const back = progressVerdict([1, 0.4, 0.6, 0.1, 0]);
  assert.equal(back.ok, false);
  assert.match(back.detail, /between 1 and 2/);
  assert.equal(progressVerdict([0.5, 0.51, 0.3, 0.1, 0]).ok, true, "antialiasing slack");
});

test("an entrance that shows nothing at 0 % fails", () => {
  const flat = progressVerdict([0, 0, 0, 0, 0]);
  assert.equal(flat.ok, false);
  assert.match(flat.detail, /nothing at 0 %/);
});

test("a chart without marks in its boxes has no progress to judge", () => {
  assert.equal(progressVerdict([null, null, null, null, null]).ok, true);
});

test("still parts tolerate the noise limit and no more", () => {
  assert.equal(stillVerdict([0, 0.001, 0.004, null, 0]).ok, true);
  const moved = stillVerdict([0, 0.2, 0, 0, 0]);
  assert.equal(moved.ok, false);
  assert.match(moved.detail, /20\.0 %/);
  assert.equal(stillVerdict([null]).ok, true, "a chart with no axes or titles");
});

test("the last and the reduced-motion frame equal the settled frame within 0.5 %", () => {
  assert.equal(equalVerdict(0, "the last frame").ok, true);
  assert.equal(equalVerdict(LIMITS.frame, "the last frame").ok, true);
  assert.equal(equalVerdict(0.02, "the 0 % frame under reduced motion").ok, false);
});

test("frame comparison counts pixels per mask and over the chart", () => {
  const masks = { mark: Uint8Array.from([1, 1, 0, 0]), still: Uint8Array.from([0, 0, 1, 1]) };
  const rest = frame(10, 10, 200, 200);
  const shares = compareFrames(frame(10, 90, 200, 120), rest, masks, LIMITS.channel);
  assert.equal(shares.mark, 0.5);
  assert.equal(shares.still, 0.5);
  assert.equal(shares.all, 0.5);
  assert.equal(compareFrames(frame(10, 20, 200, 200), rest, masks, LIMITS.channel).all, 0, "a difference of 10 is the same colour");
  assert.equal(compareFrames(rest, rest, { mark: new Uint8Array(4), still: new Uint8Array(4) }, LIMITS.channel).mark, null, "an empty mask has no share");
});

test("the fractions pick frames, the last being the final animated one", () => {
  assert.deepEqual(frameIndexes(40), [0, 10, 20, 30, 39]);
  assert.deepEqual(frameIndexes(2), [0, 1, 1, 1, 1]);
  assert.deepEqual(frameIndexes(0), [0, 0, 0, 0, 0], "settled at once");
});

test("a chart fails with every failed check named", () => {
  const verdict = chartVerdict({ progress: { ok: true, detail: "" }, still: { ok: false, detail: "moved" }, lastFrame: { ok: false, detail: "jumps" }, reducedMotion: undefined });
  assert.equal(verdict.ok, false);
  assert.deepEqual(verdict.failures, ["still: moved", "lastFrame: jumps"]);
  assert.equal(chartVerdict({ progress: { ok: true, detail: "" } }).ok, true);
});

test("a mark passing over a gridline is not a still part moving", () => {
  const masks = { mark: Uint8Array.from([0, 0, 0, 0]), still: Uint8Array.from([1, 1, 1, 1]) };
  const rest = frame(240, 240, 240, 240);
  const bar = [200, 100, 50];
  const onBar = Uint8ClampedArray.from([...bar, 255, 240, 240, 240, 255, 240, 240, 240, 255, 240, 240, 240, 255]);
  const halfway = Uint8ClampedArray.from([220, 170, 145, 255, 240, 240, 240, 255, 240, 240, 240, 255, 240, 240, 240, 255]);
  assert.equal(compareFrames(onBar, rest, masks, LIMITS.channel).still, 0.25, "without the marks' colours it counts");
  assert.equal(compareFrames(onBar, rest, masks, LIMITS.channel, bar).still, 0, "a pixel the mark covers");
  assert.equal(compareFrames(halfway, rest, masks, LIMITS.channel, bar).still, 0, "an antialiased edge of the mark");
  const grey = Uint8ClampedArray.from([120, 200, 240, 255, 240, 240, 240, 255, 240, 240, 240, 255, 240, 240, 240, 255]);
  assert.equal(compareFrames(grey, rest, masks, LIMITS.channel, bar).still, 0.25, "a colour the marks never have is a moved still part");
});
