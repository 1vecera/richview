// Host reason (ARC-03): turn terminal signals into one drainable gate cancellation.
import {homedir} from "node:os";
import {join} from "node:path";
import {sh} from "./proc.mjs";

/** Notify the release lead once that a live gate is retaining its holds for operator attention. */
export function alertUnresolvedWorkers(receiptId, records) {
	const message = `Gate ${receiptId} is operator-blocked and retains its holds: ${records.map(record => `${record.reason} (${record.file})`).join("; ")}`;
	console.error(message);
	sh(join(homedir(), "tmp/richview-rewrite/bin/rv-mail"), ["send", "richview-lockdown", message, "--from", "rv-gate"]);
}

export async function withRunCancellation(run) {
	const controller = new AbortController();
	let received;
	const onInt = () => { received ??= "SIGINT"; controller.abort(); };
	const onTerm = () => { received ??= "SIGTERM"; controller.abort(); };
	const cancel = () => { received ??= "worker-drain-unknown"; controller.abort(); };
	process.on("SIGINT", onInt);
	process.on("SIGTERM", onTerm);
	try { return await run(controller.signal, () => received, cancel); }
	finally { process.off("SIGINT", onInt); process.off("SIGTERM", onTerm); }
}
