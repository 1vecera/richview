// S27-780: mutations exercise the Cargo fields used by the direction and purity boundary.
import assert from "node:assert/strict";
import test from "node:test";
import {readFileSync} from "node:fs";
import {dependencyFindings} from "../graph.mjs";
const policy = () => JSON.parse(readFileSync(new URL("../rules.json", import.meta.url), "utf8"));
const dep = (name, kind = null, extra = {}) => ({name, kind, ...extra});
const pkg = (name, dependencies = [], targets = [{kind:["lib"]}]) => ({name, dependencies, targets});
const codes = packages => dependencyFindings(packages, policy()).map(finding => finding.code);

test("normal, renamed, optional and target-specific lower-layer edges are allowed", () => {
	const packages = [pkg("richview-chart", [dep("serde")]), pkg("richview-core", [dep("richview-chart", null, {rename:"engine",optional:true,target:"cfg(unix)"}),dep("regex","dev")])];
	assert.deepEqual(dependencyFindings(packages, policy()), []);
});

test("every dependency kind and target retains its canonical workspace direction", () => {
	for (const kind of [null,"normal","dev","build"]) {
		const packages = [pkg("richview-chart",[dep("richview-core",kind,{rename:"innocent",target:"cfg(windows)",optional:true})]),pkg("richview-core")];
		assert.ok(codes(packages).includes("dependency.edge"));
	}
});

test("the observed graph reports cycles as well as forbidden reverse edges", () => {
	assert.ok(codes([pkg("richview-chart",[dep("richview-core")]),pkg("richview-core",[dep("richview-chart")])]).includes("dependency.cycle"));
});

test("unknown crates and malformed metadata fail closed", () => {
	for (const packages of [null,[],[pkg("unassigned")],[pkg("toString")],[pkg("richview-core"),pkg("richview-core")],[{name:"richview-core"}], [{...pkg("richview-core"),manifest_path:8}], [pkg("richview-core",[{}])], [pkg("richview-core",[dep("serde","unexpected")])]]) {
		assert.ok(dependencyFindings(packages,policy()).length, JSON.stringify(packages));
	}
});

test("pure crates reject unreviewed normal, dev and build dependencies", () => {
	for (const name of ["richview-chart","richview-core"]) {
		for (const kind of [null,"dev","build"]) {
			assert.ok(codes([pkg(name,[dep("reqwest",kind,{rename:"safe",target:"cfg(unix)",optional:true})])]).includes("dependency.purity"));
		}
	}
	assert.ok(codes([pkg("richview-core",[dep("serde","build")])]).includes("dependency.purity"));
});

test("a pure crate cannot add an executable build script or an untracked path dependency", () => {
	assert.ok(codes([pkg("richview-core",[],[{kind:["custom-build"]}])]).includes("dependency.purity"));
	assert.ok(codes([pkg("richview-core",[dep("serde",null,{path:"/outside/local-serde"})])]).includes("dependency.path"));
});

test("malformed policy cannot authorize dependency edges", () => {
	const rules=policy(); rules.allowed["richview-chart"].push("richview-core");
	assert.ok(dependencyFindings([pkg("richview-chart")],rules).some(f=>f.code === "dependency.policy.cycle"));
});
