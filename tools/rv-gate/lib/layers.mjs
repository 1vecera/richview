// ARC-01: retain the gate entry point while the dependency packet owns its one graph engine.
import {readFileSync} from "node:fs";
import {join} from "node:path";
import {dependencyFindings} from "../../dependency-rules/graph.mjs";
import {checkContracts} from "../../api-contracts/check.mjs";
import {checkDependencies} from "../../dependency-rules/check.mjs";
import {indirectFindings} from "../../dependency-rules/indirect.mjs";
import {sh} from "./proc.mjs";
const rules = JSON.parse(readFileSync(new URL("../../dependency-rules/rules.json",import.meta.url),"utf8"));

/** Invalid workspace edges, cycles and pure-crate dependencies from Cargo metadata. */
export function layerProblems(packages) {
	return dependencyFindings(packages,rules).map(finding=>finding.message);
}

/** Verify this checkout's current APIs before checking its actual Cargo and indirect edges. */
export function workspaceBoundaries({cwd,base}) {
	const api = checkContracts({root:cwd,manifest:"tools/api-contracts/contracts.json",base});
	try {
		const currentRules = JSON.parse(readFileSync(join(cwd,"tools/dependency-rules/rules.json"),"utf8"));
		const findings = indirectFindings(api,currentRules);
		if (findings.length) return {api,dependencies:{schema:"interface-v1",status:"fail",findings,checked:{crates:0,indirect:0}}};
		const metadata = JSON.parse(sh("cargo",["metadata","--format-version","1","--no-deps","--offline"],{cwd}));
		return {api,dependencies:checkDependencies({metadata,rules:currentRules,apiReport:api})};
	} catch (error) {
		return {api,dependencies:{schema:"interface-v1",status:"fail",findings:[{code:"dependency.input",path:"input",message:error.message}],checked:{crates:0,indirect:0}}};
	}
}
