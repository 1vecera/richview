// A small YAML reader for the scorer: block maps and lists, flow maps and lists, quoted and plain
// scalars, `|` and `>` block scalars. It reads the rich fences agents write, never untrusted
// code; anything it cannot read throws, and the caller records the block as unreadable.

export type YamlValue = string | number | boolean | null | YamlValue[] | {[key: string]: YamlValue};
type Line = {indent: number; text: string; raw: string; blank?: boolean};

function stripComment(line: string) {
	let quote: string | null = null;
	for (let i = 0; i < line.length; i++) {
		const c = line[i];
		if (quote) {
			if (c === "\\" && quote === '"') i++;
			else if (c === quote) quote = null;
		} else if (c === '"' || c === "'") {
			if (i === 0 || /[\s:\[{,-]/.test(line[i - 1])) quote = c;
		} else if (c === "#" && (i === 0 || /\s/.test(line[i - 1]))) return line.slice(0, i);
	}
	return line;
}

export function scalar(text: string): string | number | boolean | null {
	const t = text.trim();
	if (t === "" || t === "~" || /^null$/i.test(t)) return null;
	if (/^(true|false)$/i.test(t)) return t.toLowerCase() === "true";
	if (/^[-+]?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?$/.test(t) && !/^[-+]?0\d/.test(t)) return Number(t);
	if (t[0] === '"' && t.endsWith('"') && t.length > 1) {
		try {
			return JSON.parse(t) as string;
		} catch {
			return t.slice(1, -1);
		}
	}
	if (t[0] === "'" && t.endsWith("'") && t.length > 1) return t.slice(1, -1).replace(/''/g, "'");
	return t;
}

// Flow collections: `{ a: 1, b: [x, y] }`.
function flow(text: string, pos = { i: 0 }): YamlValue {
	const skip = () => {
		while (pos.i < text.length && /\s/.test(text[pos.i])) pos.i++;
	};
	const token = (stops: string[]) => {
		skip();
		const start = pos.i;
		if (text[pos.i] === '"' || text[pos.i] === "'") {
			const q = text[pos.i++];
			while (pos.i < text.length && text[pos.i] !== q) pos.i += text[pos.i] === "\\" && q === '"' ? 2 : 1;
			pos.i++;
			return scalar(text.slice(start, pos.i));
		}
		while (pos.i < text.length && !stops.includes(text[pos.i])) pos.i++;
		return text.slice(start, pos.i).trim();
	};
	const value = (): YamlValue => {
		skip();
		if (text[pos.i] === "{") return map();
		if (text[pos.i] === "[") return list();
		const raw = token([",", "}", "]"]);
		return typeof raw === "string" ? scalar(raw) : raw;
	};
	const map = (): Record<string,YamlValue> => {
		pos.i++;
		const out: Record<string,YamlValue> = {};
		for (;;) {
			skip();
			if (pos.i >= text.length) throw new Error("unterminated flow map");
			if (text[pos.i] === "}") {
				pos.i++;
				return out;
			}
			const key = token([":", ",", "}"]);
			skip();
			if (text[pos.i] === ":") {
				pos.i++;
				out[String(key)] = value();
			} else out[String(key)] = null;
			skip();
			if (text[pos.i] === ",") pos.i++;
		}
	};
	const list = (): YamlValue[] => {
		pos.i++;
		const out: YamlValue[] = [];
		for (;;) {
			skip();
			if (pos.i >= text.length) throw new Error("unterminated flow list");
			if (text[pos.i] === "]") {
				pos.i++;
				return out;
			}
			out.push(value());
			skip();
			if (text[pos.i] === ",") pos.i++;
		}
	};
	return value();
}

function value(text: string): YamlValue {
	const t = text.trim();
	if (t[0] === "{" || t[0] === "[") return flow(t);
	return scalar(t);
}

export function parseYaml(source: string): YamlValue {
	const lines: Line[] = [];
	for (const raw of source.replace(/\r\n?/g, "\n").split("\n")) {
		if (/^\s*(#.*)?$/.test(raw)) {
			lines.push({ blank: true, indent: -1, text: "", raw });
			continue;
		}
		lines.push({ indent: raw.match(/^ */)![0].length, text: stripComment(raw).trimEnd().trim(), raw });
	}
	let at = 0;
	const next = (): Line | undefined => {
		while (at < lines.length && lines[at].blank) at++;
		return lines[at];
	};
	const blockScalar = (indent: number, folded: boolean) => {
		const out: YamlValue[] = [];
		let lead: number | null = null;
		while (at < lines.length) {
			const line = lines[at];
			if (line.blank) {
				out.push("");
				at++;
				continue;
			}
			if (line.indent <= indent) break;
			lead ??= line.indent;
			out.push(line.raw.slice(lead));
			at++;
		}
		while (out.length && out.at(-1) === "") out.pop();
		return folded ? out.join(" ") : out.join("\n");
	};
	const node = (indent: number): YamlValue => {
		const line = next();
		if (!line || line.indent < indent) return null;
		return line.text.startsWith("- ") || line.text === "-" ? seq(line.indent) : mapping(line.indent);
	};
	const seq = (indent: number): YamlValue[] => {
		const out: YamlValue[] = [];
		for (let line = next(); line && line.indent === indent && (line.text.startsWith("- ") || line.text === "-"); line = next()) {
			const rest = line.text.slice(1).trim();
			if (rest === "") {
				at++;
				out.push(node(indent + 1));
			} else if (/^[^"'{\[][^:]*:(\s|$)/.test(rest) || /^["'][^"']*["']:\s/.test(rest)) {
				// `- key: value` opens a map whose later keys sit two columns in.
				line.text = rest;
				line.indent = indent + 2;
				out.push(mapping(indent + 2));
			} else {
				at++;
				out.push(value(rest));
			}
		}
		return out;
	};
	const mapping = (indent: number): Record<string,YamlValue> => {
		const out: Record<string,YamlValue> = {};
		for (let line = next(); line && line.indent === indent && !line.text.startsWith("- "); line = next()) {
			const match = line.text.match(/^("[^"]*"|'[^']*'|[^:]+?)\s*:(?:\s+(.*))?$/);
			if (!match) throw new Error(`cannot read line: ${line.text}`);
			const key = String(scalar(match[1]));
			const rest = (match[2] ?? "").trim();
			at++;
			if (/^[|>][+-]?$/.test(rest)) out[key] = blockScalar(indent, rest[0] === ">");
			else if (rest === "") {
				const following = next();
				const sameIndentSeq = following && following.indent === indent && following.text.startsWith("- ");
				out[key] = following && (following.indent > indent || sameIndentSeq) ? node(sameIndentSeq ? indent : indent + 1) : null;
			} else out[key] = value(rest);
		}
		return out;
	};
	const root = node(0);
	return root ?? {};
}
