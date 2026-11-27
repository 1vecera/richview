// ARC-46: the gate verifies the builder's inputs and SHA-256 output hashes before copying a build.
import { createHash } from "node:crypto";
import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { isAbsolute, join, relative, sep } from "node:path";

export const BUILD_MANIFEST = "release/richview-build-manifest.json";
const hash = (bytes: string | Buffer) => createHash("sha256").update(bytes).digest("hex");
const inside = (base: string, path: string) => {
	const rel = relative(realpathSync(base), realpathSync(path));
	return rel !== ".." && !rel.startsWith(`..${sep}`) && !isAbsolute(rel);
};

interface BuildManifest {
	schema: unknown;
	mode: unknown;
	inputs?: {
		commit?: unknown;
		dirty?: unknown;
		source_diff_sha256?: unknown;
		rustc?: string;
		wasm_bindgen?: unknown;
		config?: Record<string, unknown>;
	};
	outputs?: Record<string, { bytes?: unknown; sha256?: unknown } | null>;
}

/** Returns provenance failures without trusting paths or hashes supplied by a malformed manifest. */
export function buildManifestProblems({ cwd, target, head, rust, wasmBindgen }: {
	cwd: string; target: string; head: string; rust: string; wasmBindgen: string;
}) {
	try {
		// The builder writes this schema. The checks below reject malformed or stale evidence.
		const manifest: BuildManifest = JSON.parse(readFileSync(join(target, BUILD_MANIFEST), "utf8"));
		const problems = [];
		if (manifest.schema !== 1 || manifest.mode !== "full") problems.push("expected a full schema-1 build manifest");
		const input = manifest.inputs;
		if (input?.commit !== head) problems.push(`manifest commit ${input?.commit} differs from ${head}`);
		if (input?.dirty !== false || input?.source_diff_sha256 !== hash("")) problems.push("manifest was built from dirty sources");
		if (!input?.rustc?.startsWith(`rustc ${rust} `)) problems.push(`manifest rustc differs from ${rust}`);
		if (input?.wasm_bindgen !== `wasm-bindgen ${wasmBindgen}`) problems.push(`manifest wasm-bindgen differs from ${wasmBindgen}`);
		for (const path of ["Cargo.lock", "package-lock.json", "rust-toolchain.toml"]) {
			if (input?.config?.[path] !== hash(readFileSync(join(cwd, path)))) problems.push(`manifest input changed: ${path}`);
		}
		const outputs = manifest.outputs;
		if (!outputs || Array.isArray(outputs) || typeof outputs !== "object") return [...problems, "manifest outputs are missing"];
		for (const key of [
			"target:release/richview", "target:release/richview-editor",
			"target:release/richview-sql-worker", "checkout:packages/vscode/bin/richview-sql-worker",
			"checkout:dist/web/richview_runtime.js", "checkout:dist/web/richview_runtime_bg.wasm",
			"checkout:dist/web/richview_site.js", "checkout:dist/web/richview_site_bg.wasm",
		]) if (!Object.hasOwn(outputs, key)) problems.push(`manifest omits ${key}`);
		const worker = outputs["target:release/richview-sql-worker"];
		const packagedWorker = outputs["checkout:packages/vscode/bin/richview-sql-worker"];
		if (worker && packagedWorker && (worker.bytes !== packagedWorker.bytes || worker.sha256 !== packagedWorker.sha256)) problems.push("packaged SQL worker differs from its native sibling");
		for (const [key, record] of Object.entries(outputs)) {
			const colon = key.indexOf(":");
			const group = key.slice(0, colon);
			const path = key.slice(colon + 1);
			const base = group === "checkout" ? cwd : group === "target" ? target : undefined;
			if (!base || !path || isAbsolute(path) || path.includes("\\") || path.split("/").some((part) => part === ".." || part === ".")) {
				problems.push(`invalid manifest output path: ${key}`);
				continue;
			}
			const file = join(base, path);
			if (!inside(base, file) || !lstatSync(file).isFile()) {
				problems.push(`manifest output is not a regular file within ${group}: ${path}`);
				continue;
			}
			const bytes = readFileSync(file);
			if (record?.bytes !== bytes.length || record?.sha256 !== hash(bytes)) problems.push(`manifest output changed: ${key}`);
		}
		return problems;
	} catch (error) {
		return [`cannot verify build manifest: ${(error as Error).message}`];
	}
}
