// Public-surface snapshots (DEL-53): CLI commands, flags and help text, MCP tool names and input
// schemas, and MCP resources, generated from the release binary. The committed copy lives in
// `docs/surface/`; a PR that changes it must declare `feature` or name the items behind it.

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { listFiles } from "./glob.mjs";

function run(binary, args, input) {
	const result = spawnSync(binary, args, { encoding: "utf8", input, timeout: 30_000 });
	return `${result.stdout ?? ""}${result.stderr ?? ""}`;
}

/** Subcommands listed under "Commands:" in clap help text, without `help`. */
export function subcommands(help) {
	const block = help.split(/^Commands:\s*$/m)[1];
	if (!block) return [];
	const names = [];
	for (const line of block.split("\n")) {
		if (/^\S/.test(line)) break;
		const match = line.match(/^\s{2}([a-z][\w-]*)\b/);
		if (match && match[1] !== "help") names.push(match[1]);
	}
	return names;
}

function mcpSurface(binary) {
	const messages = [
		{ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "rv-gate", version: "1" } } },
		{ jsonrpc: "2.0", method: "notifications/initialized" },
		{ jsonrpc: "2.0", id: 2, method: "tools/list" },
		{ jsonrpc: "2.0", id: 3, method: "resources/list" },
	];
	const output = run(binary, ["mcp"], `${messages.map((m) => JSON.stringify(m)).join("\n")}\n`);
	const replies = output
		.split("\n")
		.filter((line) => line.startsWith("{"))
		.map((line) => JSON.parse(line));
	const byId = Object.fromEntries(replies.map((r) => [r.id, r.result]));
	const init = byId[1] ?? {};
	return {
		protocolVersion: init.protocolVersion,
		capabilities: init.capabilities,
		instructions: init.instructions,
		tools: (byId[2]?.tools ?? []).sort((a, b) => a.name.localeCompare(b.name)),
		resources: (byId[3]?.resources ?? []).sort((a, b) => a.uri.localeCompare(b.uri)),
	};
}

/** Generate every snapshot file into `{name: content}`. */
export function generateSurface(binDir) {
	const binary = join(binDir, "richview");
	const files = {};
	const walk = (path) => {
		const help = run(binary, [...path, "--help"]).replace(/\b3\.\d+\.\d+[\w.-]*/g, "<version>");
		files[`cli/${["richview", ...path].join("-")}.txt`] = help;
		for (const sub of subcommands(help)) walk([...path, sub]);
	};
	walk([]);
	const mcp = mcpSurface(binary);
	mcp.serverVersion = "<version>";
	files["mcp/surface.json"] = `${JSON.stringify(mcp, null, 2)}\n`;
	return files;
}

export function readCommitted(dir) {
	const files = {};
	if (!existsSync(dir)) return files;
	for (const rel of listFiles(dir, "", new Set())) {
		if (rel !== "README.md") files[rel] = readFileSync(join(dir, rel), "utf8");
	}
	return files;
}

export function writeSurface(dir, files) {
	for (const [name, content] of Object.entries(files)) {
		mkdirSync(join(dir, name, ".."), { recursive: true });
		writeFileSync(join(dir, name), content);
	}
}

export function diffSurface(a, b) {
	const names = new Set([...Object.keys(a), ...Object.keys(b)]);
	return [...names].filter((name) => a[name] !== b[name]).sort();
}
