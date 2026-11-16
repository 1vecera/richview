// ARC-50: hosted checks are manual, read-only and delegate to the local gate.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { PINS } from '../lib/checks.mjs';
const repo = fileURLToPath(new URL('../../..', import.meta.url));
const read = path => readFileSync(join(repo, path), 'utf8');

test('the sole workflow is manual, read-only and calls rv-gate pre-pr for every check', () => {
	const files = readdirSync(join(repo, '.github/workflows')).filter(name => /\.ya?ml$/.test(name));
	assert.deepEqual(files, ['ci.yml']);
	const source = read('.github/workflows/ci.yml').replace(/^\s*#.*$/gm, '');
	assert.match(source, /^on:\s*\n  workflow_dispatch:\s*\n/m);
	assert.doesNotMatch(source, /^\s*(push|pull_request|pull_request_target|schedule|workflow_run):/m);
	assert.match(source, /^permissions:\s*\n  contents: read\s*\n/m);
	assert.doesNotMatch(source, /secrets\.|permissions:.*write|\b(deploy|wrangler|publish.yml)\b/);
	assert.deepEqual([...source.matchAll(/^\s*(?:-\s+)?run:\s*([^\n]+)$/gm)].map(match=>match[1].trim()), ['node tools/rv-gate/rv-gate.mjs pre-pr']);
	assert.match(source, /runs-on: ubuntu-latest/);
	assert.match(source, /fetch-depth: 0/);
	assert.match(source, /uses: \.\/\.github\/actions\/setup-gate/);
});

test('the setup action prepares the repository pins and installed Playwright without running checks', () => {
	const source = read('.github/actions/setup-gate/action.yml');
	assert.match(source, /node-version-file: \.node-version/);
	assert.match(source, /rustup show active-toolchain/);
	assert.ok(source.includes(`cargo install wasm-bindgen-cli --version ${PINS.wasmBindgen} --locked`));
	assert.ok(source.includes(`cargo install cargo-deny --version ${PINS.cargoDeny} --locked`));
	assert.match(source, /npm ci --ignore-scripts/);
	assert.match(source, /npx --no-install playwright install --with-deps chromium firefox webkit/);
	assert.doesNotMatch(source, /cargo (test|clippy|run)|scripts\/verify-|wrangler|secrets\./);
});
