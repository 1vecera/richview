// ARC-01: validate the explicit dependency policy before it can authorize workspace edges.
export type Finding = { code: string; path: string; message: string };
export type Kind = "normal" | "dev" | "build";
export type Binding = { consumer: string; producer: string; contract: string; exception?: string };
export type Rules = { schema: string; allowed: Record<string, string[]>; pure: Record<string, Record<Kind, string[]>>; indirect: Binding[] };

const object = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === "object" && !Array.isArray(value);
const text = (value: unknown): value is string => typeof value === "string" && value.trim() === value && value.length > 0;
const strings = (value: unknown): value is string[] => Array.isArray(value) && value.every(text) && new Set(value).size === value.length;
const exactKeys = (value: unknown, keys: string[]): value is Record<string, unknown> => object(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));

/** ARC-07 embeds these runtime artifacts in the native app; it grants no runtime crate API. */
export function resourceException(entry: Partial<Binding> | undefined) {
	return entry?.exception === "ARC-07" && entry.consumer === "richview-app" && entry.producer === "richview-runtime" && entry.contract === "runtime.embed-files";
}

/** Deterministic back-edge cycles in a directed graph; unknown targets belong to validation. */
export function graphCycles(edges: Map<string, string[]>) {
	const done = new Set<string>(), active = new Map<string, number>(), stack: string[] = [], cycles: string[][] = [];
	const visit = (name: string) => {
		if (active.has(name)) {
			cycles.push([...stack.slice(active.get(name)), name]);
			return;
		}
		if (done.has(name)) return;
		active.set(name, stack.length);
		stack.push(name);
		for (const target of [...new Set(edges.get(name))].sort()) if (edges.has(target)) visit(target);
		stack.pop();
		active.delete(name);
		done.add(name);
	};
	for (const name of [...edges.keys()].sort()) visit(name);
	return cycles;
}

/** Malformed or cyclic policy entries, including missing engine/core purity declarations. */
export function policyFindings(rules: unknown) {
	const findings: Finding[] = [];
	const fail = (path: string, message: string, code = "dependency.policy.schema") => findings.push({code, path, message});
	if (!exactKeys(rules, ["schema", "allowed", "pure", "indirect"]) || rules.schema !== "interface-v1") {
		fail("rules", "Expected interface-v1 dependency policy with allowed, pure and indirect rules");
		return findings;
	}
	if (!object(rules.allowed) || !Object.keys(rules.allowed).length) {
		fail("rules.allowed", "Expected a nonempty map of workspace crates to allowed dependency names");
		return findings;
	}
	const allowed = rules.allowed;
	const known = (name: string) => Object.hasOwn(allowed, name);
	const graph = new Map<string, string[]>();
	for (const [name, targets] of Object.entries(rules.allowed)) {
		if (!text(name) || !strings(targets)) {
			fail(`rules.allowed.${name}`, "Expected unique nonempty dependency names");
			continue;
		}
		graph.set(name, targets);
		for (const target of targets) if (!known(target)) fail(`rules.allowed.${name}`, `Unknown workspace crate ${target}`);
	}
	for (const cycle of graphCycles(graph)) fail("rules.allowed", `Allowed edges contain a cycle: ${cycle.join(" -> ")}`, "dependency.policy.cycle");
	if (!object(rules.pure)) fail("rules.pure", "Expected explicit dependency kinds for pure crates");
	else {
		for (const name of ["richview-chart", "richview-core"]) if (!Object.hasOwn(rules.pure, name)) fail(`rules.pure.${name}`, "Engine and core must retain their purity declarations");
		for (const [name, kinds] of Object.entries(rules.pure)) {
			if (!known(name) || !exactKeys(kinds, ["normal", "dev", "build"]) || !Object.values(kinds).every(strings))
				fail(`rules.pure.${name}`, "Expected a known pure crate with unique normal, dev and build dependency names");
		}
	}
	if (!Array.isArray(rules.indirect)) fail("rules.indirect", "Expected explicit indirect public-contract bindings");
	else {
		const seen = new Set<string>();
		// Schema validation below handles malformed JSON fields as well as valid bindings.
		for (const [index, entry] of (rules.indirect as (Partial<Binding> | null)[]).entries()) {
			const key = JSON.stringify(entry && [entry.consumer, entry.producer, entry.contract]);
			const keys = entry && Object.hasOwn(entry,"exception") ? ["consumer","producer","contract","exception"] : ["consumer","producer","contract"];
			if (!exactKeys(entry, keys) || !Object.values(entry).every(text) || !text(entry.consumer) || !text(entry.producer) || !known(entry.consumer) || !known(entry.producer) || seen.has(key) || (Object.hasOwn(entry,"exception") && !resourceException(entry)))
				fail(`rules.indirect[${index}]`, "Expected unique consumer/producer/public-contract triples naming known crates");
			seen.add(key);
		}
	}
	return findings.sort((a,b) => a.path.localeCompare(b.path) || a.code.localeCompare(b.code) || a.message.localeCompare(b.message));
}
