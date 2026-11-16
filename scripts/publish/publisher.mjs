#!/usr/bin/env node
/** Trusted content-only CI runner. Never executes code from a submitted publication revision. */
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, writeFile, readFile, rm } from "node:fs/promises";
import { resolve, join, dirname } from "node:path";
import { createHash } from "node:crypto";
import { setTimeout as delay } from "node:timers/promises";

const configPath = process.argv[2];
const watch = process.argv.includes("--watch");
if (!configPath)
	throw new Error("Usage: publisher.mjs operator.json [--watch]");
const config = JSON.parse(await readFile(configPath, "utf8"));
for (const key of [
	"origin",
	"repository",
	"branch",
	"contentRoot",
	"builder",
	"runtime",
	"scratch",
	"builderSha256",
	"runtimeSha256",
])
	if (!config[key]) throw new Error("Missing operator setting: " + key);
if (
	!/^https:\/\/[a-z0-9.-]+$/.test(config.origin) ||
	!/^[-\w.]+\/[-\w.]+$/.test(config.repository) ||
	!/^[\w/-]+$/.test(config.contentRoot) ||
	config.contentRoot.includes("..")
)
	throw new Error("Invalid operator destination");
const github =
	process.env.PUBLISH_GITHUB_TOKEN ||
	execFileSync("gh", ["auth", "token"], {
		encoding: "utf8",
		stdio: ["ignore", "pipe", "pipe"],
	}).trim();
const clientId = process.env.CF_ACCESS_CLIENT_ID,
	clientSecret = process.env.CF_ACCESS_CLIENT_SECRET;
if (!clientId || !clientSecret)
	throw new Error(
		"Provide the scoped Cloudflare Access publishing service credentials through the environment",
	);
const scratch = resolve(config.scratch);
await mkdir(scratch, { recursive: true });
const digest = (b) => createHash("sha256").update(b).digest("hex");
if (digest(await readFile(config.builder)) !== config.builderSha256)
	throw new Error(
		"Installed builder differs from the pinned trusted executable",
	);
for (const name of ["richview_runtime.js", "richview_runtime_bg.wasm"]) {
	if (
		digest(await readFile(join(config.runtime, name))) !==
		config.runtimeSha256[name]
	)
		throw new Error(
			"Installed browser runtime differs from the trusted release: " + name,
		);
}
const lock = join(scratch, "runner.lock");
try {
	await mkdir(lock);
} catch {
	throw new Error(
		"A publisher is already running, or an interrupted runner lock needs inspection",
	);
}
function log(event, fields = {}) {
	console.log(
		JSON.stringify({ at: new Date().toISOString(), event, ...fields }),
	);
}
async function request(path, method = "GET", body) {
	const response = await fetch(config.origin + path, {
		method,
		headers: {
			"CF-Access-Client-Id": clientId,
			"CF-Access-Client-Secret": clientSecret,
			...(body ? { "Content-Type": "application/json" } : {}),
		},
		body: body ? JSON.stringify(body) : undefined,
		redirect: "error",
	});
	if (!response.ok)
		throw new Error(
			`Publication ${path.split("?")[0]} returned ${response.status}`,
		);
	return response.json();
}
async function api(path, method = "GET", body) {
	const response = await fetch(
		`https://api.github.com/repos/${config.repository}${path}`,
		{
			method,
			headers: {
				Authorization: `Bearer ${github}`,
				"User-Agent": "RichView-Publish-CI",
				Accept: "application/vnd.github+json",
				"Content-Type": "application/json",
			},
			body: body ? JSON.stringify(body) : undefined,
			redirect: "error",
		},
	);
	if (!response.ok)
		throw new Error(`GitHub ${path.split("?")[0]} returned ${response.status}`);
	return response.json();
}
async function checkoutContent(commit, destination) {
	const tree = await api(`/git/trees/${commit}?recursive=1`);
	if (tree.truncated)
		throw new Error("Git tree is truncated; split the content repository");
	const files = tree.tree.filter(
		(f) => f.path.startsWith(config.contentRoot + "/") && f.type !== "tree",
	);
	if (!files.length || files.length > 10000)
		throw new Error("Content tree exceeds supported file limits");
	let total = 0;
	for (const file of files) {
		const relative = file.path.slice(config.contentRoot.length + 1);
		if (
			file.mode !== "100644" ||
			relative.split("/").some((p) => !p || p === ".." || p === ".") ||
			relative.includes("\\") ||
			file.size > 20 * 1024 * 1024
		)
			throw new Error("Unsupported content file: " + relative);
		total += file.size;
		if (total > 256 * 1024 * 1024)
			throw new Error("Canonical content exceeds 256 MiB");
		const blob = await api(`/git/blobs/${file.sha}`);
		if (blob.encoding !== "base64")
			throw new Error("Unsupported Git blob encoding");
		const bytes = Buffer.from(blob.content, "base64");
		const target = join(destination, relative);
		await mkdir(dirname(target), { recursive: true });
		await writeFile(target, bytes);
	}
}
async function build(commit, { preview = false, number, generation } = {}) {
	const job = join(scratch, commit + "-" + Date.now());
	await mkdir(job, { recursive: true });
	try {
		log("build.started", { commit, preview });
		await request("/_publish/building", "POST", { commit });
		const source = join(job, "source");
		await mkdir(source);
		await checkoutContent(commit, source);
		const out = join(job, "release");
		// No inherited credentials, shells, repository hooks, package installation or candidate code.
		const child = spawnSync(
			config.builder,
			[
				"build",
				"--site",
				join(source, "site.yaml"),
				"--runtime",
				resolve(config.runtime),
				"--output",
				out,
				"--source-commit",
				commit,
			],
			{
				env: { PATH: "/usr/bin:/bin" },
				encoding: "utf8",
				timeout: 600000,
				maxBuffer: 4 * 1024 * 1024,
			},
		);
		await writeFile(
			join(job, "build.log"),
			(child.stdout || "") + (child.stderr || ""),
		);
		if (child.status !== 0)
			throw new Error(
				"RichView validation/build failed; inspect the local build log",
			);
		const release = JSON.parse(
			await readFile(join(out, "release.json"), "utf8"),
		);
		const repo = await api("");
		if (
			!repo.private &&
			release.items.some(
				(i) =>
					i.audience !== "public" ||
					i.state !== "published" ||
					release.site.mode === "internal",
			)
		)
			throw new Error(
				"Private, hidden and draft content require a private canonical repository",
			);
		for (const [hash, object] of Object.entries(release.objects)) {
			const bytes = await readFile(join(out, "objects", hash));
			if (digest(bytes) !== hash || bytes.length !== object.bytes)
				throw new Error("Generated object changed after build");
			const response = await fetch(
				config.origin + "/_publish/objects/" + hash,
				{
					method: "PUT",
					headers: {
						"CF-Access-Client-Id": clientId,
						"CF-Access-Client-Secret": clientSecret,
						"Content-Type": "application/octet-stream",
					},
					body: bytes,
					redirect: "error",
				},
			);
			if (!response.ok)
				throw new Error("Object upload returned " + response.status);
		}
		await request("/_publish/stage", "POST", release);
		const target = config.origin + "/_preview/" + release.id + "/";
		await api("/statuses/" + commit, "POST", {
			state: "success",
			context: "richview-publish/validation",
			description:
				"Validated immutable publication; protected preview is ready",
			target_url: target,
		});
		if (!preview) {
			await request("/_publish/activate", "POST", {
				release: release.id,
				generation,
			});
			log("release.live", { commit, release: release.id });
		} else log("preview.ready", { commit, release: release.id, number });
		await writeFile(
			join(job, "receipt.json"),
			JSON.stringify({ commit, release: release.id, preview, target }, null, 2),
		);
		// Keep concise provenance and logs. Large generated output and fetched source are disposable.
		await rm(source, { recursive: true });
		await rm(out, { recursive: true });
	} catch (e) {
		log("build.failed", { commit, message: e.message });
		await request("/_publish/failed", "POST", {
			commit,
			reason: e.message,
		}).catch(() => {});
		await api("/statuses/" + commit, "POST", {
			state: "failure",
			context: "richview-publish/validation",
			description:
				"Publication build or activation failed; last good release retained",
		}).catch(() => {});
		throw e;
	}
}
async function cycle() {
	const queue = await request("/_publish/sync", "POST", {});
	if (
		queue.branch !== config.branch ||
		queue.content_root !== config.contentRoot
	)
		throw new Error("Runner destination differs from the gateway allowlist");
	for (const preview of queue.previews)
		await build(preview.head, { preview: true, number: preview.number }).catch(
			() => {},
		);
	const head = (await api("/git/ref/heads/" + config.branch)).object.sha;
	const live = await request("/_status");
	if (head !== live.commit && queue.ready === head)
		await build(head, { generation: queue.generation });
}
let stopping = false;
for (const signal of ["SIGINT", "SIGTERM"])
	process.on(signal, () => {
		stopping = true;
	});
try {
	do {
		try {
			await cycle();
		} catch (e) {
			log("cycle.failed", { message: e.message });
			if (!watch) throw e;
		}
		if (watch && !stopping) await delay(30000);
	} while (watch && !stopping);
} finally {
	await rm(lock, { recursive: true });
}
