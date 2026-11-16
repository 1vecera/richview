// Compare two chart-review captures of the same build: identical bytes, then decoded pixels.
// Usage: node compare-captures.mjs <dirA> <dirB>
// Run from the repository root after npm ci, which installs sharp from the lockfile. DEL-28 cites
// this script as it ran on Claw Mini on 2026-10-21, against two `node scripts/verify-chart-review.mjs`
// output folders of b4db233.
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, relative } from 'node:path';

const require = createRequire(join(process.cwd(), 'package.json'));
const sharp = require('sharp');
const [a, b] = process.argv.slice(2);
const walk = dir => readdirSync(dir).flatMap(name => {
	const path = join(dir, name);
	return statSync(path).isDirectory() ? walk(path) : name.endsWith('.png') ? [path] : [];
});
const hash = path => createHash('sha256').update(readFileSync(path)).digest('hex');
const filesA = walk(a).map(path => relative(a, path)).sort();
const filesB = new Set(walk(b).map(path => relative(b, path)));
const summary = { pairs: 0, missing: 0, identicalBytes: 0, identicalPixels: 0, sizeMismatch: 0, differing: [] };
for (const file of filesA) {
	if (!filesB.has(file)) { summary.missing += 1; continue; }
	summary.pairs += 1;
	const pathA = join(a, file), pathB = join(b, file);
	if (hash(pathA) === hash(pathB)) { summary.identicalBytes += 1; continue; }
	const [imgA, imgB] = await Promise.all([pathA, pathB].map(p => sharp(p).ensureAlpha().raw().toBuffer({ resolveWithObject: true })));
	if (imgA.info.width !== imgB.info.width || imgA.info.height !== imgB.info.height) {
		summary.sizeMismatch += 1;
		summary.differing.push({ file, size: [`${imgA.info.width}x${imgA.info.height}`, `${imgB.info.width}x${imgB.info.height}`] });
		continue;
	}
	let changed = 0, changedOver8 = 0, maxDelta = 0;
	for (let i = 0; i < imgA.data.length; i += 4) {
		let delta = 0;
		for (let c = 0; c < 4; c++) delta = Math.max(delta, Math.abs(imgA.data[i + c] - imgB.data[i + c]));
		if (delta > 0) changed += 1;
		if (delta > 8) changedOver8 += 1;
		maxDelta = Math.max(maxDelta, delta);
	}
	const pixels = imgA.info.width * imgA.info.height;
	if (changed === 0) { summary.identicalPixels += 1; continue; }
	summary.differing.push({ file, pixels, changed, changedOver8, share: +(changed / pixels * 100).toFixed(4), maxDelta });
}
summary.differing.sort((x, y) => (y.share ?? 100) - (x.share ?? 100));
console.log(JSON.stringify({ ...summary, differingCount: summary.differing.length, worst: summary.differing.slice(0, 12) }, null, 1));
