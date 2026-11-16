// Turns a Claude Code (`--output-format stream-json`) or Codex (`exec --json`) transcript into one
// list of tool calls, and reads each `richview check` result however the agent got it: the CLI's
// JSON or pretty text, or the MCP tool's structured result or one-line summary.

const CLI = /(?:^|[\s;&|(])(?:\S*\/)?richview\s+(check|validate|generate|learn|init|serve|publish)\b/;
const MCP = /(?:^|__)richview_(check|validate|render|learn|publish|image_search|image_use)$/;

/** Parses JSON-lines text, skipping lines that are not JSON. */
export function parseJsonl(text) {
	const out = [];
	for (const line of text.split("\n")) {
		const t = line.trim();
		if (!t.startsWith("{")) continue;
		try {
			out.push(JSON.parse(t));
		} catch {
			// a partial line from a killed run
		}
	}
	return out;
}

function textOf(content) {
	if (typeof content === "string") return content;
	if (Array.isArray(content)) return content.map((c) => (typeof c === "string" ? c : (c?.text ?? ""))).join("\n");
	return content == null ? "" : JSON.stringify(content);
}

function classify(name, input) {
	const mcp = name.match(MCP);
	if (mcp) return { kind: mcp[1] === "render" ? "render" : mcp[1], via: "mcp" };
	if (name === "Bash" || name === "command_execution") {
		const cli = String(input?.command ?? input ?? "").match(CLI);
		if (cli) return { kind: cli[1] === "generate" ? "render" : cli[1], via: "cli" };
		return { kind: "shell", via: "shell" };
	}
	if (/^(Write|Edit|MultiEdit|NotebookEdit|file_change)$/.test(name)) return { kind: "edit", via: "file" };
	if (/^(Read|Glob|Grep|LS)$/.test(name)) return { kind: "read", via: "file" };
	return { kind: "other", via: "other" };
}

/** Claude Code stream-json events to calls. */
function fromClaude(events) {
	const calls = [];
	const byId = new Map();
	let result = null;
	for (const ev of events) {
		if (ev.type === "assistant") {
			for (const part of ev.message?.content ?? []) {
				if (part.type !== "tool_use") continue;
				const call = { id: part.id, tool: part.name, input: part.input ?? {}, output: "", isError: false, ...classify(part.name, part.input) };
				calls.push(call);
				byId.set(part.id, call);
			}
		} else if (ev.type === "user") {
			for (const part of ev.message?.content ?? []) {
				if (part.type !== "tool_result") continue;
				const call = byId.get(part.tool_use_id);
				if (!call) continue;
				call.output = textOf(part.content);
				call.isError = Boolean(part.is_error);
				if (ev.tool_use_result?.structuredContent) call.structured = ev.tool_use_result.structuredContent;
			}
		} else if (ev.type === "result") result = ev;
	}
	return { calls, result: result && { isError: Boolean(result.is_error), turns: result.num_turns ?? null, costUsd: result.total_cost_usd ?? null, durationMs: result.duration_ms ?? null, text: result.result ?? "" } };
}

/** Codex `exec --json` events to calls. */
function fromCodex(events) {
	const calls = [];
	let last = "";
	let usage = null;
	for (const ev of events) {
		const item = ev.item;
		if (ev.type === "turn.completed") usage = ev.usage ?? usage;
		if (ev.type !== "item.completed" || !item) continue;
		if (item.type === "agent_message") last = item.text ?? last;
		else if (item.type === "command_execution") {
			calls.push({ id: item.id, tool: "command_execution", input: { command: item.command }, output: item.aggregated_output ?? "", isError: item.exit_code != null && item.exit_code !== 0 && !CLI.test(item.command ?? ""), exitCode: item.exit_code, ...classify("command_execution", { command: item.command }) });
		} else if (item.type === "mcp_tool_call") {
			const name = `${item.server ?? "richview"}__${item.tool}`;
			const structured = item.result?.structured_content ?? item.result?.structuredContent;
			calls.push({ id: item.id, tool: name, input: item.arguments ?? {}, output: textOf(item.result?.content) || (item.error?.message ?? ""), structured, isError: item.status === "failed" || Boolean(item.error), ...classify(item.tool ?? name, {}) });
		} else if (item.type === "file_change") calls.push({ id: item.id, tool: "file_change", input: { changes: item.changes }, output: "", isError: false, kind: "edit", via: "file" });
	}
	return { calls, result: { isError: false, turns: null, costUsd: null, durationMs: null, text: last, usage } };
}

/** Normalizes a transcript; `product` is `claude` or `codex`. */
export function readTranscript(events, product) {
	return product === "codex" ? fromCodex(events) : fromClaude(events);
}

/**
 * Reads one check or validate result. Returns `{ ok, clean, diagnostics, count, visible }`:
 * `visible` is false when the agent saw only a count, so the codes cannot be judged.
 */
export function parseCheck(call) {
	const raw = call.output ?? "";
	const fromObject = (obj) => {
		const diagnostics = (obj.diagnostics ?? []).map((d) => ({ code: d.code, level: d.level, line: d.line ?? null, fix: Boolean(d.fix), message: d.message ?? "" }));
		const blocking = diagnostics.filter((d) => d.level === "error" || d.level === "warning");
		return { ok: obj.ok ?? !diagnostics.some((d) => d.level === "error"), clean: blocking.length === 0 && obj.ok !== false, diagnostics, count: diagnostics.length, visible: true };
	};
	if (call.structured?.diagnostics) return fromObject(call.structured);
	const trimmed = raw.trim();
	if (trimmed.startsWith("{")) {
		try {
			const obj = JSON.parse(trimmed);
			if (Array.isArray(obj.diagnostics)) return fromObject(obj);
		} catch {
			// fall through to the text forms
		}
	}
	const diagnostics = [];
	for (const line of raw.split("\n")) {
		const m = line.match(/^(?:\S+?:(\d+)\s+)?(error|warning|info)\s+([\w.-]+):\s*(.*)$/);
		if (m) diagnostics.push({ code: m[3], level: m[2], line: m[1] ? Number(m[1]) : null, fix: false, message: m[4] });
	}
	if (/Checked \d+ charts?:\s*(clean|issues)/.test(raw) || diagnostics.length) {
		const blocking = diagnostics.filter((d) => d.level !== "info");
		return { ok: !diagnostics.some((d) => d.level === "error"), clean: blocking.length === 0, diagnostics, count: diagnostics.length, visible: true };
	}
	const summary = raw.match(/^(?:check|validate):\s*(ok|clean|has errors)[^\d]*(\d+) diagnostics?/m);
	if (summary) {
		const count = Number(summary[2]);
		return { ok: summary[1] !== "has errors", clean: summary[1] !== "has errors" && count === 0, diagnostics: [], count, visible: false };
	}
	return { ok: false, clean: false, diagnostics: [], count: 0, visible: false, unreadable: true };
}
