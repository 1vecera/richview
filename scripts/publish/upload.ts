import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { createHash } from "node:crypto";
import type { ReleaseManifest } from "./wire.ts";
const [directory, origin, mode = "preview"] = process.argv.slice(2);
if (
	!directory ||
	!/^https:\/\/[a-z0-9.-]+$/.test(origin || "") ||
	!["preview", "activate"].includes(mode)
)
	throw new Error(
		"Usage: upload.ts release-directory https://publication.example [preview|activate]",
	);
const clientId = process.env.CF_ACCESS_CLIENT_ID;
const clientSecret = process.env.CF_ACCESS_CLIENT_SECRET;
if (!clientId || !clientSecret)
	throw new Error("Scoped Access service credentials are required");
const headers = {
	"CF-Access-Client-Id": clientId,
	"CF-Access-Client-Secret": clientSecret,
};
async function api<T>(path: string, method = "GET", body?: unknown): Promise<T> {
	const r = await fetch(origin + path, {
		method,
		headers: { ...headers, "Content-Type": "application/json" },
		body: body ? JSON.stringify(body) : undefined,
		redirect: "error",
	});
	if (!r.ok) throw new Error(path + " returned " + r.status);
	return r.json();
}
const state = await api<{ generation: number }>("/_publish/state");
const release: ReleaseManifest = JSON.parse(
	await readFile(join(directory, "release.json"), "utf8"),
);
for (const [hash, object] of Object.entries(release.objects)) {
	const bytes = await readFile(join(directory, "objects", hash));
	if (
		createHash("sha256").update(bytes).digest("hex") !== hash ||
		bytes.length !== object.bytes
	)
		throw new Error("Object changed after build");
	const r = await fetch(origin + "/_publish/objects/" + hash, {
		method: "PUT",
		headers: { ...headers, "Content-Type": "application/octet-stream" },
		body: bytes,
		redirect: "error",
	});
	if (!r.ok) throw new Error("Upload returned " + r.status);
}
console.log(JSON.stringify(await api("/_publish/stage", "POST", release)));
if (mode === "activate")
	console.log(
		JSON.stringify(
			await api("/_publish/activate", "POST", {
				release: release.id,
				generation: state.generation,
			}),
		),
	);
