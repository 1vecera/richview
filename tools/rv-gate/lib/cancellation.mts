// Host reason (ARC-03): turn terminal signals into one drainable gate cancellation.
import {homedir} from "node:os";
import {join} from "node:path";
import {sh} from "./proc.mts";

/** Notify the release lead once that a live gate is retaining its holds for operator attention. */
export function alertUnresolvedWorkers(receiptId: string, records: readonly { reason: string; file: string }[]) {
	const message = `Gate ${receiptId} is operator-blocked and retains its holds: ${records.map(record => `${record.reason} (${record.file})`).join("; ")}`;
	console.error(message);
	sh(join(homedir(), "tmp/richview-rewrite/bin/rv-mail"), ["send", "richview-lockdown", message, "--from", "rv-gate"]);
}

type Cancellation = "SIGINT" | "SIGTERM" | "worker-drain-unknown";

export async function withRunCancellation<T>(run: (signal: AbortSignal, reason: () => Cancellation | undefined, cancel: () => void) => T) {
	const controller = new AbortController();
	let received: Cancellation | undefined;
	const onInt = () => { received ??= "SIGINT"; controller.abort(); };
	const onTerm = () => { received ??= "SIGTERM"; controller.abort(); };
	const cancel = () => { received ??= "worker-drain-unknown"; controller.abort(); };
	process.on("SIGINT", onInt);
	process.on("SIGTERM", onTerm);
	try { return await run(controller.signal, () => received, cancel); }
	finally { process.off("SIGINT", onInt); process.off("SIGTERM", onTerm); }
}
