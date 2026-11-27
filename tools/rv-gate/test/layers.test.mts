// ARC-01: upward edges fail regardless of manifest spelling or the dependency's build target.
import assert from 'node:assert/strict';
import test from 'node:test';
import { layerProblems } from '../lib/layers.mts';
import { gateEnv, sh } from '../lib/proc.mts';
import { fileURLToPath } from 'node:url';
const repo = fileURLToPath(new URL('../../..', import.meta.url));
const pkg = (name: string, dependencies: string[] = []) => ({name, targets:[{kind:['lib']}], dependencies: dependencies.map(name => ({name}))});

test('the workspace Cargo manifests keep every dependency in its allowed lower layers', () => {
	const metadata = JSON.parse(sh('cargo', ['metadata', '--format-version', '1', '--no-deps', '--offline'], {cwd: repo, env: gateEnv()}));
	assert.deepEqual(layerProblems(metadata.packages), []);
});

test('engine, core, browser hosts and native services cannot depend upward or on build tools', () => {
	for (const [from, to] of [
		['richview-chart', 'richview-core'], ['richview-core', 'richview-app'],
		['richview-runtime', 'richview-site'], ['richview-site', 'richview-app'],
		['richview-app', 'richview-editor'], ['richview-app', 'richview-build'],
		['richview-core', 'richview-bench'], ['richview-share', 'richview-publish'],
	]) {
		const dependencies = [{name: to, rename: 'alias', kind: 'build', target: 'cfg(unix)'}];
		assert.match(layerProblems([{...pkg(from), dependencies}, pkg(to)]).join('\n'), new RegExp(`${from} -> ${to}`));
	}
	assert.match(layerProblems([pkg('richview-new')]).join(), /no layer assignment/);
});
