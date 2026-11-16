// ARC-01: command-line host for the workspace and verified API dependency policy.
import {readFileSync} from "node:fs";
import {resolve} from "node:path";
import {fileURLToPath} from "node:url";
import {sh} from "../rv-gate/lib/proc.mjs";
import {dependencyFindings} from "./graph.mjs";
import {indirectFindings} from "./indirect.mjs";
import {policyFindings} from "./policy.mjs";
const text = value => typeof value === "string" && value.trim() === value && value.length > 0;
const result = (findings,crates=0,indirect=0) => ({schema:"interface-v1",status:findings.length ? "fail" : "pass",findings,checked:{crates,indirect}});

function workspacePackages(metadata) {
	if (!metadata || metadata.version !== 1 || !Array.isArray(metadata.packages) || !Array.isArray(metadata.workspace_members) || !metadata.workspace_members.length || !metadata.workspace_members.every(text) || new Set(metadata.workspace_members).size !== metadata.workspace_members.length)
		throw new Error("Expected Cargo metadata v1 with nonempty unique workspace member IDs");
	const packages = new Map();
	for (const pkg of metadata.packages) {
		if (!text(pkg?.id) || packages.has(pkg.id)) throw new Error("Expected unique Cargo package IDs");
		packages.set(pkg.id,pkg);
	}
	return metadata.workspace_members.map(id => {
		if (!packages.has(id)) throw new Error(`Workspace member ${id} has no package metadata`);
		return packages.get(id);
	});
}

/** Check the actual workspace and a fresh API report; every finding is fatal in interface-v1. */
export function checkDependencies({metadata,rules,apiReport} = {}) {
	const policy = policyFindings(rules);
	if (policy.length) return result(policy);
	const indirect = indirectFindings(apiReport,rules);
	let packages;
	try {packages=workspacePackages(metadata);}
	catch(error) {return result([{code:"dependency.metadata",path:"metadata",message:error.message},...indirect]);}
	const findings = [...dependencyFindings(packages,rules),...indirect].sort((a,b) => a.path.localeCompare(b.path) || a.code.localeCompare(b.code) || a.message.localeCompare(b.message));
	return result(findings,packages.length,indirect.length ? 0 : apiReport.indirect.length);
}

function cli(args) {
	try {
		if (args.shift() !== "check") throw new Error("Usage: check --api-report <json> [--root <repo>] [--rules <json>]");
		const options={};
		while(args.length) {
			const key=args.shift(),value=args.shift();
			if (!["--root","--rules","--api-report"].includes(key) || Object.hasOwn(options,key) || !text(value) || value.startsWith("--")) throw new Error("Expected unique --root, --rules and --api-report path arguments");
			options[key]=value;
		}
		if (!options["--api-report"]) throw new Error("A fresh API checker report is required (--api-report)");
		const root=resolve(options["--root"] ?? process.cwd());
		const rules=JSON.parse(readFileSync(resolve(root,options["--rules"] ?? "tools/dependency-rules/rules.json"),"utf8"));
		const apiReport=JSON.parse(readFileSync(resolve(root,options["--api-report"]),"utf8"));
		const preflight=policyFindings(rules);
		if (preflight.length) return result(preflight);
		const indirect=indirectFindings(apiReport,rules);
		if (indirect.length) return result(indirect);
		const metadata=JSON.parse(sh("cargo",["metadata","--format-version","1","--no-deps","--offline"],{cwd:root}));
		return checkDependencies({metadata,rules,apiReport});
	} catch(error) {return result([{code:"dependency.input",path:"input",message:error.message}]);}
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const report=cli(process.argv.slice(2));
	process.stdout.write(`${JSON.stringify(report)}\n`);
	process.exitCode=report.status === "pass" ? 0 : 1;
}
