// ARC-01: consume verified public-resource bindings from the API checker.
import {policyFindings, resourceException} from "./policy.mjs";
const text = value => typeof value === "string" && value.trim() === value && value.length > 0;
const key = entry => JSON.stringify([entry.consumer,entry.producer,entry.contract]);

/** Authorize observed public bindings only after the API checker completely passes. */
export function indirectFindings(report, rules) {
	const findings = policyFindings(rules);
	if (findings.length) return findings;
	const fail = (code,path,message) => findings.push({code,path,message});
	if (!report || report.schema !== "interface-v1" || !["pass","fail"].includes(report.status) || !Array.isArray(report.findings) || !Array.isArray(report.indirect)) {
		return [{code:"dependency.api.schema",path:"api",message:"Expected an interface-v1 API report with pass/fail status, findings and observed indirect bindings"}];
	}
	if (report.status !== "pass" || report.findings.length) {
		return [{code:"dependency.api.failed",path:"api",message:"API verification must pass with no findings before indirect bindings can be trusted"}];
	}
	const explicit = new Map(rules.indirect.map(entry=>[key(entry),entry])), seen = new Set();
	for (const [index,entry] of report.indirect.entries()) {
		const path = `api.indirect[${index}]`;
		if (!entry || Object.keys(entry).length !== 3 || !["consumer","producer","contract"].every(field => Object.hasOwn(entry,field) && text(entry[field])) || !Object.hasOwn(rules.allowed,entry.consumer) || !Object.hasOwn(rules.allowed,entry.producer) || seen.has(key(entry))) {
			fail("dependency.api.schema",path,"Expected unique consumer/producer/public-contract triples naming known workspace crates");
			continue;
		}
		seen.add(key(entry));
		if (!explicit.has(key(entry))) fail("dependency.indirect.unlisted",path,`Unlisted indirect binding ${entry.consumer} -> ${entry.producer} (${entry.contract})`);
		if (!rules.allowed[entry.consumer].includes(entry.producer) && !resourceException(explicit.get(key(entry)))) fail("dependency.indirect.direction",path,`Indirect binding ${entry.consumer} -> ${entry.producer} points outside its allowed lower layers (${entry.contract})`);
	}
	return findings.sort((a,b) => a.path.localeCompare(b.path) || a.code.localeCompare(b.code) || a.message.localeCompare(b.message));
}
