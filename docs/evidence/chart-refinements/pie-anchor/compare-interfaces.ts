import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { join } from 'node:path';
const { values } = parseArgs({ options: { before: { type: 'string' }, after: { type: 'string' }, 'native-before': { type: 'string' }, 'native-after': { type: 'string' }, out: { type: 'string' } } });
interface InterfaceRecord {
  id: string;
  keyboard: unknown[];
  pointer: unknown[];
  touch: unknown[];
  table: unknown[];
  geometry: { marks: unknown[] };
  staticShape: { marks: unknown[] };
  exportShape: { marks: unknown[] };
  activeAnimations: number;
  csvPath: string;
  live: { checks: unknown[] };
}
interface NativeRecord {
  id: string;
  overflow: boolean;
  charts: { svg: string; outlineRows: number[] }[];
}
interface Capture<T> {
  head: string;
  wasm_sha256: string;
  native_sha256: string;
  records: T[];
  errors: string[];
  external: string[];
}

const load = <T>(dir: string): Capture<T> => JSON.parse(readFileSync(join(dir, 'checks.json'), 'utf8'));
const before = load<InterfaceRecord>(values.before!), after = load<InterfaceRecord>(values.after!), nativeBefore = load<NativeRecord>(values['native-before']!), nativeAfter = load<NativeRecord>(values['native-after']!);
const summary = { before: { head: before.head, wasm: before.wasm_sha256, native: nativeBefore.native_sha256 }, after: { head: after.head, wasm: after.wasm_sha256, native: nativeAfter.native_sha256 }, interface_panels: 0, keyboard_rows: 0, pointer_rows: 0, touch_rows: 0, table_csv_pairs: 0, offline_native_views: 0, offline_native_panels: 0, unchanged_native_controls: 0 };
assert.equal(before.records.length, 60); assert.equal(after.records.length, 60);
assert.deepEqual(before.errors, []); assert.deepEqual(after.errors, []);
for (const a of after.records) {
  const b = before.records.find(r => r.id === a.id); assert(b);
  for (const key of ['keyboard', 'pointer', 'touch', 'table'] as const) assert.deepEqual(a[key], b[key], a.id + ' unchanged ' + key);
  for (const key of ['geometry', 'staticShape', 'exportShape'] as const) assert.deepEqual(a[key].marks, b[key].marks, a.id + ' unchanged ' + key + ' marks');
  assert.equal(a.activeAnimations, 0); assert(readFileSync(a.csvPath).equals(readFileSync(b.csvPath)), a.id + ' original CSV bytes');
  for (const finding of a.live.checks) assert(b.live.checks.some(x => JSON.stringify(x) === JSON.stringify(finding)), a.id + ' no new live finding');
  summary.interface_panels++; summary.keyboard_rows += a.keyboard.length; summary.pointer_rows += a.pointer.length; summary.touch_rows += a.touch.length; summary.table_csv_pairs++;
}
const withoutAnnotations = (s: string) => s.replace(/<path\b[^>]*stroke-width="4"[^>]*(?:\/>|>[\s\S]*?<\/path>)/g, '').replace(/<text\b[^>]*class="rv-annotation"[^>]*>[\s\S]*?<\/text>/g, '');
assert.equal(nativeBefore.records.length, 48); assert.equal(nativeAfter.records.length, 48);
for (const m of [nativeBefore, nativeAfter]) { assert.deepEqual(m.errors, []); assert.deepEqual(m.external, []); }
for (const a of nativeAfter.records) {
  const b = nativeBefore.records.find(r => r.id === a.id); assert(b); assert.equal(a.charts.length, b.charts.length); assert.equal(a.overflow, false);
  for (let i = 0; i < a.charts.length; i++) {
    assert.equal(withoutAnnotations(a.charts[i].svg), withoutAnnotations(b.charts[i].svg), a.id + ' native output changes only annotations');
    if (JSON.stringify(a.charts[i].outlineRows) === JSON.stringify(b.charts[i].outlineRows)) { assert.equal(a.charts[i].svg, b.charts[i].svg); summary.unchanged_native_controls++; }
    summary.offline_native_panels++;
  }
  summary.offline_native_views++;
}
writeFileSync(values.out!, JSON.stringify(summary, null, 2)); console.log(JSON.stringify(summary, null, 2));
