// ARC-01: inspect Cargo metadata without compiling dependencies.
import {dirname, resolve} from "node:path";
import {graphCycles, policyFindings} from "./policy.mjs";
const text = value => typeof value === "string" && value.trim() === value && value.length > 0;

/** Manifest edge, cycle and pure-crate findings; dependency aliases never supply identity. */
export function dependencyFindings(packages, rules) {
	const findings = policyFindings(rules);
	if (findings.length) return findings;
	const fail = (code,path,message) => findings.push({code,path,message});
	if (!Array.isArray(packages) || !packages.length) return [{code:"dependency.metadata",path:"packages",message:"Expected nonempty workspace package metadata"}];
	const members = new Map(), graph = new Map();
	for (const [index,pkg] of packages.entries()) {
		if (!pkg || !text(pkg.name) || members.has(pkg.name) || (pkg.manifest_path != null && !text(pkg.manifest_path)) || !Array.isArray(pkg.dependencies) || !Array.isArray(pkg.targets) || !pkg.targets.length || pkg.targets.some(target => !Array.isArray(target?.kind) || !target.kind.length || !target.kind.every(text))) {
			fail("dependency.metadata",`packages[${index}]`,"Expected unique named workspace packages with dependencies and target kinds");
			continue;
		}
		members.set(pkg.name,pkg);
		graph.set(pkg.name,[]);
	}
	for (const pkg of members.values()) {
		const allowed = Object.hasOwn(rules.allowed,pkg.name) ? rules.allowed[pkg.name] : undefined;
		if (!allowed) {
			fail("dependency.assignment",pkg.name,`${pkg.name} has no layer assignment (ARC-01)`);
			continue;
		}
		const pure = Object.hasOwn(rules.pure,pkg.name) ? rules.pure[pkg.name] : undefined;
		if (pure && pkg.targets.some(target => target.kind.includes("custom-build")))
			fail("dependency.purity",pkg.name,`${pkg.name} must not execute a build script (ARC-02)`);
		for (const [index,dependency] of pkg.dependencies.entries()) {
			const path = `${pkg.name}.dependencies[${index}]`;
			const kind = dependency?.kind ?? "normal";
			if (!text(dependency?.name) || !["normal","dev","build"].includes(kind)) {
				fail("dependency.metadata",path,"Expected a canonical dependency name and normal, dev or build kind");
				continue;
			}
			const name = dependency.name, member = members.get(name);
			if (member) {
				graph.get(pkg.name).push(name);
				if (!allowed.includes(name)) fail("dependency.edge",path,`${pkg.name} -> ${name} points outside its allowed lower layers (ARC-01)`);
				if (dependency.source != null || (dependency.path != null && (!text(dependency.path) || (member.manifest_path && resolve(dependency.path) !== dirname(resolve(member.manifest_path))))))
					fail("dependency.path",path,`${name} must name its workspace package, not a registry or replacement path`);
			} else {
				if (dependency.path != null) fail("dependency.path",path,`${pkg.name} -> ${name} is an untracked local crate; assign it a workspace layer`);
				if (Object.hasOwn(rules.allowed,name)) fail("dependency.metadata",path,`Workspace dependency ${name} is absent from the package metadata`);
				if (pure && !pure[kind].includes(name)) fail("dependency.purity",path,`${pkg.name} has unreviewed ${kind} dependency ${name} (ARC-02)`);
			}
		}
	}
	for (const cycle of graphCycles(graph)) fail("dependency.cycle","packages",`Workspace dependency cycle: ${cycle.join(" -> ")}`);
	return findings.sort((a,b) => a.path.localeCompare(b.path) || a.code.localeCompare(b.code) || a.message.localeCompare(b.message));
}
