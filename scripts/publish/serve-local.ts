// Serve one built release as the public reader on 127.0.0.1, through the real gateway bundle.
// Usage: node scripts/publish/serve-local.ts <release-dir> [port]
// Same local platform as verify-platform.ts: workerd, private R2, the SQLite Durable Object and
// locally signed Access JWTs. Nothing leaves this machine; stop it with Ctrl-C.
import { readFile, mkdir, mkdtemp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { execFileSync } from "node:child_process";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { bundleModules } from "./bundle-modules.ts";
import {
	generateKeyPair,
	exportJWK,
	SignJWT,
} from "../../cloudflare/publish/node_modules/jose/dist/webapi/index.js";
import type { ReleaseManifest } from "./wire.ts";

const input = process.argv[2];
const port = Number(process.argv[3] || 8787);
if (!input || !Number.isInteger(port))
	throw new Error("Usage: serve-local.ts release-directory [port]");
await mkdir("tmp", { recursive: true });
const work = await mkdtemp(resolve("tmp/publish-serve-"));
execFileSync(
	process.execPath,
	[
		"node_modules/wrangler/bin/wrangler.js",
		"deploy",
		"--dry-run",
		"--config",
		"wrangler.publish.toml",
		"--outdir",
		join(work, "bundle"),
	],
	{
		stdio: "pipe",
		env: { PATH: process.env.PATH, HOME: process.env.HOME, WRANGLER_SEND_METRICS: "false" },
	},
);
const release: ReleaseManifest = JSON.parse(await readFile(join(input, "release.json"), "utf8"));
const { privateKey, publicKey } = await generateKeyPair("RS256");
const publicJwk = { ...(await exportJWK(publicKey)), kid: "local-serve-key", use: "sig", alg: "RS256" };
const issuer = "https://test.cloudflareaccess.com";
const audience = "local-publish-serve";
const publisher = await new SignJWT({ email: "publisher@example.test" })
	.setProtectedHeader({ alg: "RS256", kid: publicJwk.kid })
	.setIssuer(issuer)
	.setAudience(audience)
	.setSubject("publisher@example.test")
	.setIssuedAt()
	.setExpirationTime("12h")
	.sign(privateKey);
const mf = new Miniflare(
	convertV4MiniflareOptions({
		...(await bundleModules(join(work, "bundle"))),
		compatibilityDate: "2026-10-18",
		host: "127.0.0.1",
		port,
		r2Buckets: ["RELEASES"],
		durableObjects: { PUBLICATION: { className: "Publication", useSQLite: true } },
		bindings: {
			PUBLICATION_HOST: "news.richview.ai",
			ACCESS_ISSUER: issuer,
			ACCESS_AUD: audience,
			BOOTSTRAP_SUPER_EDITOR: "email:super@example.test",
			PUBLISHER_IDENTITY: "email:publisher@example.test",
			GITHUB_REPOSITORY: "example/publication",
			GITHUB_TOKEN: "local-fixture-only",
			CONTENT_ROOT: "publications/news",
			PUBLICATION_BRANCH: "publish/news",
			BOOTSTRAP_COMMIT: release.commit,
		},
		outboundService: async (request) => {
			const url = new URL(request.url);
			if (url.hostname === "test.cloudflareaccess.com")
				return Response.json({ keys: [publicJwk] });
			if (url.hostname === "api.github.com" && url.pathname.endsWith("/git/ref/heads/publish/news"))
				return Response.json({ object: { sha: release.commit } });
			if (url.hostname === "api.github.com" && url.pathname === "/repos/example/publication")
				return Response.json({ private: true });
			return new Response("Not part of this local server", { status: 404 });
		},
	}),
);
const origin = await mf.ready;
const call = (path: string, method: string, body?: string | Buffer) =>
	mf.dispatchFetch(new URL(path, origin), {
		method,
		headers: { Cookie: "CF_Authorization=" + publisher },
		body,
	});
for (const hash of Object.keys(release.objects)) {
	const r = await call("/_publish/objects/" + hash, "PUT", await readFile(join(input, "objects", hash)));
	if (r.status !== 200) throw new Error(`Upload ${hash}: ${await r.text()}`);
}
for (const [path, body] of [
	["/_publish/stage", release],
	["/_publish/activate", { release: release.id, generation: 0 }],
] as const) {
	const r = await call(path, "POST", JSON.stringify(body));
	if (r.status !== 200) throw new Error(`${path}: ${await r.text()}`);
}
console.log(`Serving release ${release.id.slice(0, 12)} at ${origin.href} (Ctrl-C to stop)`);
const stop = async () => {
	await mf.dispose();
	process.exit(0);
};
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
