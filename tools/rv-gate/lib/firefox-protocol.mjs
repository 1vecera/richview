// Host reason (ARC-03): source-safe, bounded diagnostics from the admitted Firefox command's stderr.
import { appendFileSync, writeFileSync } from "node:fs";

const lifecycle = new Set(["Runtime.executionContextCreated", "Runtime.executionContextDestroyed", "Page.frameAttached", "Page.frameDetached", "Page.navigationStarted", "Page.navigationCommitted", "Page.navigationAborted", "Page.eventFired"]);
const calls = new Set(["Runtime.evaluate", "Runtime.callFunction", "Page.describeNode"]);
const scalar = value => typeof value === "string" ? value.slice(0, 160) : typeof value === "number" ? value : undefined;
const safeURL = value => {
	if (typeof value !== "string") return undefined;
	try {
		const u = new URL(value);
		if (u.protocol === "http:" || u.protocol === "https:") return u.origin + u.pathname;
		if (u.protocol === "file:") return "file:///" + u.pathname.split("/").at(-1);
		if (u.protocol === "about:") return ["about:blank", "about:srcdoc"].includes(value) ? value : "about:";
		return u.protocol;
	} catch { return "unparseable-url"; }
};
class ProtocolFilter {
	constructor(emit) { this.emit = emit; this.requests = new Map(); this.contexts = new Map(); this.stats = { packets: 0, retained: 0, truncatedNonLifecycle: 0, malformedCritical: 0, malformedUnknown: 0, unsupportedProtocolFormat: 0 }; }
	consume(line, truncated = false) {
		const plain = line.replace(/\u001b\[[0-9;]*m/g, "").trimStart();
		const prefixed = /^\s*(?:(?:\d{4}-\S+|\[[^\]\r\n]{0,80}\])\s+)?pw:protocol\b/u.test(plain);
		const match = plain.match(/^(?:\d{4}-\d\d-\d\dT\S+\s+)?pw:protocol\s+(SEND ►|◀ RECV)\s+/u);
		if (!match) {
			if (!prefixed) return false;
			this.stats.packets++; this.stats.malformedUnknown++; this.stats.unsupportedProtocolFormat++;
			this.record({ at: Date.now(), unsupportedProtocolFormat: true });
			return true;
		}
		this.stats.packets++;
		const direction = match[1] === "SEND ►" ? "send" : "receive", raw = plain.slice(match[0].length);
		let packet;
		try {
			if (truncated) throw new Error("line limit");
			packet = JSON.parse(raw);
			if (!packet || typeof packet !== "object" || Array.isArray(packet) || !packet.method && packet.id === undefined) throw new Error("invalid packet");
		} catch {
			// Juggler debug truncates large call/result packets. Never retain their payload.
			const boundaries = ["\"params\":", "\"result\":", "\"error\":"].map(key => raw.indexOf(key)).filter(i => i >= 0);
			const header = raw.slice(0, Math.min(300, ...boundaries)), parsedMethod = header.match(/"method":"([A-Za-z.]+)"/)?.[1], id = Number(header.match(/"id":(\d+)/)?.[1]);
			const method = lifecycle.has(parsedMethod) || calls.has(parsedMethod) ? parsedMethod : undefined;
			if (method && lifecycle.has(method)) this.stats.malformedCritical++;
			else if (!method && !Number.isFinite(id)) this.stats.malformedUnknown++;
			else this.stats.truncatedNonLifecycle++;
			if (direction === "send" && method && Number.isFinite(id)) this.requests.set(String(id), { method, partial: true });
			this.record({ at: Date.now(), direction, id: Number.isFinite(id) ? id : undefined, method, truncated: true });
			return true;
		}
		const method = packet.method, p = packet.params || {}, session = scalar(packet.sessionId), id = scalar(packet.id), key = `${session || ""}:${id}`;
		if (direction === "send" && id !== undefined && calls.has(method)) this.requests.set(key, { method, context: scalar(p.executionContextId), object: scalar(p.objectId) });
		if (!method) {
			const request = this.requests.get(key) || this.requests.get(String(id));
			if (request && /^Runtime\.(evaluate|callFunction)$/.test(request.method)) this.record({ at: Date.now(), direction, id, session, method: request.method, context: request.context, completed: true, protocolError: Boolean(packet.error), exception: Boolean(packet.result?.exceptionDetails) });
			if (request?.method === "Page.describeNode") this.record({ at: Date.now(), direction, id, session, method: request.method, object: request.object, contentFrame: scalar(packet.result?.contentFrameId), ownerFrame: scalar(packet.result?.ownerFrameId), completed: true, protocolError: Boolean(packet.error) });
			this.requests.delete(key); this.requests.delete(String(id)); return true;
		}
		if (!lifecycle.has(method) && !calls.has(method)) return true;
		const context = scalar(p.executionContextId), aux = p.auxData || {}, contextKey = `${session || ""}:${context}`;
		if (method === "Runtime.executionContextCreated") this.contexts.set(contextKey, scalar(aux.frameId));
		const frame = scalar(p.frameId) ?? scalar(aux.frameId) ?? this.contexts.get(contextKey);
		this.record({ at: Date.now(), direction, id, session, method, context, object: method === "Page.describeNode" ? scalar(p.objectId) : undefined, frame, parent: scalar(p.parentFrameId), navigation: scalar(p.navigationId), url: safeURL(p.url),
			world: method === "Runtime.executionContextCreated" ? (aux.name === "" ? "main" : String(aux.name).includes("playwright") ? "utility" : "other") : undefined,
			lifecycle: method === "Page.eventFired" && ["load", "DOMContentLoaded"].includes(p.name) ? p.name : undefined });
		if (method === "Runtime.executionContextDestroyed") this.contexts.delete(contextKey);
		return true;
	}
	record(value) { this.stats.retained++; this.emit(value); }
}

const CAP_BYTES = 4 * 1024 * 1024;
const LINE_BYTES = 8 * 1024 * 1024;

/**
 * Filter official pw:protocol stderr without changing ordinary bytes or owning a process.
 * Writes are synchronous and bounded; end() flushes trailing bytes and returns loss metadata.
 * File failures disable diagnostic persistence while source-safe filtering continues.
 */
export function createFirefoxProtocolSink({ logPath, forward }) {
	const metadataPath = logPath + ".meta.json";
	const meta = { version: 1, cap_bytes: CAP_BYTES, bytes: 0, capReached: false, lineLimit: false, writeLost: false, metadataWriteLost: false, ended: false, errors: [] };
	let writable = true, fragments = [], length = 0, dropping = false, passing = false;
	const lost = (operation, error) => {
		meta.errors.push({ operation, code: typeof error?.code === "string" ? error.code : "UNKNOWN" });
	};
	try { writeFileSync(logPath, "", { flag: "wx" }); }
	catch (error) { writable = false; meta.writeLost = true; lost("open", error); }
	const filter = new ProtocolFilter(record => {
		const line = JSON.stringify(record) + "\n", bytes = Buffer.byteLength(line);
		if (meta.bytes + bytes > CAP_BYTES) { meta.capReached = true; return; }
		if (!writable || meta.capReached) return;
		try { appendFileSync(logPath, line); meta.bytes += bytes; }
		catch (error) { writable = false; meta.writeLost = true; lost("write", error); }
	});
	const status = () => ({
		...meta, errors: meta.errors.map(error => ({ ...error })), protocol: { ...filter.stats },
		critical_complete: meta.ended && !meta.capReached && !meta.lineLimit && !meta.writeLost && !meta.metadataWriteLost
			&& !filter.stats.malformedCritical && !filter.stats.malformedUnknown && !filter.stats.truncatedNonLifecycle,
		metadataPath,
	});
	const flush = newline => {
		const raw = fragments.length === 1 ? fragments[0] : Buffer.concat(fragments, length);
		fragments = []; length = 0;
		if (!filter.consume(raw.toString("utf8"))) {
			forward(raw);
			if (newline) forward(Buffer.from("\n"));
		}
	};
	return {
		write(chunk) {
			if (meta.ended) return;
			const bytes = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
			let start = 0;
			while (start < bytes.length) {
				const newline = bytes.indexOf(10, start), end = newline < 0 ? bytes.length : newline;
				const part = bytes.subarray(start, end);
				if (passing) forward(part);
				else if (!dropping) {
					fragments.push(part); length += part.length;
					if (length > LINE_BYTES) {
						const raw = Buffer.concat(fragments, length);
						// A too-large diagnostic is consumed once, then discarded up to its newline.
						if (filter.consume(raw.toString("utf8"), true)) {
							meta.lineLimit = true; dropping = true;
						} else { forward(raw); passing = !/^\s*$/.test(raw.toString("utf8")); }
						fragments = []; length = 0;
					}
				}
				if (newline >= 0) {
					if (passing) forward(Buffer.from("\n"));
					else if (!dropping) flush(true);
					dropping = false; passing = false;
				}
				start = end + 1;
			}
		},
		end() {
			if (meta.ended) return status();
			if (length) flush(false);
			meta.ended = true;
			try { writeFileSync(metadataPath, JSON.stringify(status(), null, 2) + "\n", { flag: "wx" }); }
			catch (error) { meta.metadataWriteLost = true; lost("metadata", error); }
			return status();
		},
		status,
	};
}
