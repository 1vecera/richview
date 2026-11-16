// Regenerate measurements from the four saved report pairs after running RichView generate.
import { readFileSync, writeFileSync } from 'node:fs';
import { gzipSync } from 'node:zlib';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const reports = [
  ['01-trends', 'Regional trends'],
  ['02-gains', 'Regional gains'],
  ['03-decades', 'Changes by interval'],
  ['04-gap', 'Europe–Africa gap'],
];
const size = (...paths) => {
  const files = paths.map((path) => readFileSync(join(root, path)));
  return {
    raw: files.reduce((sum, bytes) => sum + bytes.length, 0),
    gzip: files.reduce((sum, bytes) => sum + gzipSync(bytes, { level: 9, mtime: 0 }).length, 0),
  };
};
const rows = [];
for (const [id, label] of reports) {
  rows.push({ id, label, artifact: 'Agent HTML', ...size(`${id}/agent-report.html`) });
  rows.push({ id, label, artifact: 'RichView source and data', ...size(`${id}/report.rmd`, `${id}/data.csv`) });
  rows.push({ id, label, artifact: 'RichView rendered HTML', ...size(`${id}/Rendered/report.html`) });
}
const csv = (values) => values.map((row) => row.map((cell) => `"${String(cell).replaceAll('"', '""')}"`).join(',')).join('\n') + '\n';
writeFileSync(join(root, 'sizes.csv'), csv([
  ['report', 'artifact', 'raw_bytes', 'gzip_bytes'],
  ...rows.map(({ id, artifact, raw, gzip }) => [id, artifact, raw, gzip]),
]));
for (const [name, artifacts] of [
  ['authoring', ['Agent HTML', 'RichView source and data']],
  ['delivery', ['Agent HTML', 'RichView rendered HTML']],
]) {
  const selected = rows.filter((row) => artifacts.includes(row.artifact));
  writeFileSync(join(root, 'comparison', `${name}.csv`), csv([
    ['report', 'artifact', 'bytes'],
    ...selected.map(({ label, artifact, raw }) => [label, artifact, raw]),
  ]));
}
