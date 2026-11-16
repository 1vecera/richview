// Host reason (ARC-03): verify publication through an isolated local Worker and a real browser.
import assert from "node:assert/strict";
import { readFile, writeFile, mkdir, mkdtemp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { execFileSync } from "node:child_process";
import { Miniflare, convertV4MiniflareOptions } from "miniflare";
import { bundleModules } from "./bundle-modules.mjs";
import {
	generateKeyPair,
	exportJWK,
	SignJWT,
} from "../../cloudflare/publish/node_modules/jose/dist/webapi/index.js";
import { chromium, errors } from "playwright";
import { launchBrowser } from "../../tools/rv-gate/lib/browser-launch.mjs";

const input = process.argv[2];
if (!input)
	throw new Error(
		"Usage: verify-platform.mjs release-directory [evidence-directory]",
	);
await mkdir("tmp", { recursive: true });
const work = await mkdtemp(resolve("tmp/publish-platform-"));
const evidence = resolve(process.argv[3] || join(work, "evidence"));
await mkdir(evidence, { recursive: true });
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
		env: {
			PATH: process.env.PATH,
			HOME: process.env.HOME,
			WRANGLER_SEND_METRICS: "false",
		},
	},
);
const release = JSON.parse(await readFile(join(input, "release.json"), "utf8"));
const { privateKey, publicKey } = await generateKeyPair("RS256");
const publicJwk = {
	...(await exportJWK(publicKey)),
	kid: "local-test-key",
	use: "sig",
	alg: "RS256",
};
const issuer = "https://test.cloudflareaccess.com",
	audience = "local-publish-acceptance";
async function token(email, aud = audience) {
	return new SignJWT({ email })
		.setProtectedHeader({ alg: "RS256", kid: publicJwk.kid })
		.setIssuer(issuer)
		.setAudience(aud)
		.setSubject(email)
		.setIssuedAt()
		.setExpirationTime("1h")
		.sign(privateKey);
}
const credentials = {
	super: await token("super@example.test"),
	publisher: await token("publisher@example.test"),
	reader: await token("reader@example.test"),
	editor: await token("editor@example.test"),
	wrong: await token("super@example.test", "wrong-audience"),
};
let gitAvailable = true;
let gitHead = release.commit;
const mf = new Miniflare(
	convertV4MiniflareOptions({
		...(await bundleModules(join(work, "bundle"))),
		compatibilityDate: "2026-10-18",
		r2Buckets: ["RELEASES"],
		durableObjects: {
			PUBLICATION: { className: "Publication", useSQLite: true },
		},
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
			if (
				url.hostname === "test.cloudflareaccess.com" &&
				url.pathname === "/cdn-cgi/access/certs"
			)
				return Response.json({ keys: [publicJwk] });
			if (!gitAvailable)
				return new Response("Offline fixture", { status: 503 });
			if (
				url.hostname === "api.github.com" &&
				url.pathname.endsWith("/git/ref/heads/publish/news")
			)
				return Response.json({ object: { sha: gitHead } });
			if (
				url.hostname === "api.github.com" &&
				url.pathname === "/repos/example/publication"
			)
				return Response.json({ private: true });
			return new Response("Not part of this local fixture", { status: 404 });
		},
	}),
);
let browser;
const checks = [];
const request = (path, actor, method = "GET", body) =>
	mf.dispatchFetch("https://news.richview.ai" + path, {
		method,
		headers: {
			...(actor ? { Cookie: "CF_Authorization=" + credentials[actor] } : {}),
			...(body ? { "Content-Type": "application/json" } : {}),
		},
		body: body ? JSON.stringify(body) : undefined,
	});
async function expect(path, status, actor) {
	const response = await request(path, actor);
	assert.equal(response.status, status, path);
	checks.push({ path, status, actor: actor || "anonymous" });
	return response;
}
try {
	await mf.ready;
	for (const [hash] of Object.entries(release.objects)) {
		const bytes = await readFile(join(input, "objects", hash));
		const r = await mf.dispatchFetch(
			"https://news.richview.ai/_publish/objects/" + hash,
			{
				method: "PUT",
				headers: { Cookie: "CF_Authorization=" + credentials.publisher },
				body: bytes,
			},
		);
		assert.equal(r.status, 200, await r.text());
	}
	let r = await request("/_publish/stage", "publisher", "POST", release);
	assert.equal(r.status, 200, await r.text());
	r = await request("/_publish/activate", "publisher", "POST", {
		release: release.id,
		generation: 0,
	});
	assert.equal(r.status, 200, await r.text());
	await expect("/", 200);
	await expect("/_editor/api/session", 403);
	await expect("/_editor/api/session", 403, "wrong");
	await expect("/_editor/api/session", 403, "reader");
	await expect("/_editor/api/session", 200, "super");
	await expect("/stories/member-notebook", 404);
	await expect("/stories/member-notebook", 200, "reader");
	await expect("/_content/member-notebook/report.html", 404);
	await expect("/internal", 401);
	await expect("/internal", 200, "reader");
	r = await request("/_editor/api/roles", "super", "POST", {
		identity: "email:editor@example.test",
		role: "editor",
		sections: ["research"],
		audiences: ["members"],
	});
	assert.equal(r.status, 200);
	await expect(
		"/_preview/" + release.id + "/stories/desk-draft",
		404,
		"editor",
	); // Different section scope.
	r = await request("/_editor/api/roles", "editor", "POST", {});
	assert.equal(r.status, 403);
	for (const path of ["/", "/search.json", "/feed.xml", "/sitemap.xml"]) {
		const text = await (await request(path)).text();
		assert(!text.includes("Inside the example research notebook"));
		assert(!text.includes("A draft that has not been published"));
	}
	browser = await launchBrowser(chromium, { headless: true }, { TimeoutError: errors.TimeoutError });
	for (const width of [1440, 390]) {
		console.log("Checking publication viewport", width);
		const page = await browser.newPage({
			viewport: { width, height: 1000 },
			reducedMotion: "reduce",
		});
		page.setDefaultTimeout(60000);
		const errors = [];
		page.on("pageerror", (e) => errors.push(e.message));
		await page.route("https://news.richview.ai/**", async (route) => {
			const original = route.request();
			const response = await mf.dispatchFetch(original.url(), {
				method: original.method(),
				headers: original.headers(),
				body: original.postDataBuffer() || undefined,
			});
			await route.fulfill({
				status: response.status,
				headers: Object.fromEntries(response.headers),
				body: Buffer.from(await response.arrayBuffer()),
			});
		});
		await page.goto("https://news.richview.ai/");
		await page.screenshot({
			path: join(evidence, `front-${width}.png`),
			fullPage: true,
		});
		assert.equal(
			await page.locator(".wordmark").textContent(),
			"RichView News",
		);
		assert(
			await page.evaluate(
				() => document.documentElement.scrollWidth <= innerWidth,
			),
		);
		await page.goto("https://news.richview.ai/stories/measurement-gap");
		const report = page.frameLocator("iframe.report");
		await report.locator(".rv-chart").first().scrollIntoViewIfNeeded();
		await report.locator(".rv-chart svg.rvc-svg:visible").first().waitFor().catch(async (error) => {
			const diagnostics = await report.locator(".rv-chart svg.rvc-svg").first().evaluate((svg) => {
				const ancestors = [];
				for (let node = svg; node; node = node.parentElement) {
					const style = getComputedStyle(node), bounds = node.getBoundingClientRect();
					ancestors.push({ tag: node.tagName, class: node.getAttribute("class"), hidden: node.hidden, display: style.display, visibility: style.visibility, opacity: style.opacity, width: bounds.width, height: bounds.height });
				}
				return { ancestors, runtime: typeof wasm_bindgen, body: document.body.innerText.slice(0, 2000) };
			});
			await writeFile(join(evidence, "report-failure.json"), JSON.stringify({ errors, ...diagnostics }, null, 2));
			await page.screenshot({ path: join(evidence, "report-failure.png"), fullPage: true });
			throw error;
		});
		await page.waitForTimeout(1000);
		await page.screenshot({
			path: join(evidence, `report-${width}.png`),
			fullPage: true,
		});
		const iframe = page
			.frames()
			.find((f) => f.url().includes("/_content/measurement-gap/"));
		assert(iframe);
		const runtime = await iframe.evaluate(
			() => typeof wasm_bindgen === "function",
		);
		assert(runtime, "Shared RichView runtime loaded");
		// Long inline code and wide tables must wrap or scroll inside their own box.
		const overflow = await iframe.evaluate(
			() => document.documentElement.scrollWidth - document.documentElement.clientWidth,
		);
		assert(overflow <= 1, `Embedded report overflows its frame by ${overflow}px at ${width}`);
		await report
			.getByRole("button", { name: "Table", exact: true })
			.first()
			.click();
		await report.locator(".rvc-table tbody tr").first().waitFor();
		assert(
			(await report.locator(".rvc-table tbody tr").count()) > 0,
			"Interactive exact-value table opened",
		);
		await report
			.getByRole("button", { name: "Table", exact: true })
			.first()
			.click();
		for (const format of ["SVG", "CSV"]) {
			const [download] = await Promise.all([
				page.waitForEvent("download"),
				report.getByRole("button", { name: "Download " + format, exact: true }).first().click(),
			]);
			const file = join(evidence, `report-${width}.${format.toLowerCase()}`);
			await download.saveAs(file);
			const exported = await readFile(file, "utf8");
			if (format === "SVG") assert(exported.includes("<svg"));
			else assert(exported.trim().split("\n").length > 2);
		}
		console.log("Report table and downloads passed", width);
		await page.close();
	}
	if (process.argv[4]) {
		const alternateDirectory = resolve(process.argv[4]);
		const alternate = JSON.parse(
			await readFile(join(alternateDirectory, "release.json"), "utf8"),
		);
		assert.notEqual(alternate.site.layout, release.site.layout);
		for (const hash of Object.keys(alternate.objects)) {
			const response = await mf.dispatchFetch(
				"https://news.richview.ai/_publish/objects/" + hash,
				{
					method: "PUT",
					headers: { Cookie: "CF_Authorization=" + credentials.publisher },
					body: await readFile(join(alternateDirectory, "objects", hash)),
				},
			);
			assert.equal(response.status, 200, await response.text());
		}
		const staged = await request("/_publish/stage", "publisher", "POST", alternate);
		assert.equal(staged.status, 200, await staged.text());
		for (const width of [1440, 390]) {
			const page = await browser.newPage({ viewport: { width, height: 1000 } });
			await page.route("https://news.richview.ai/**", async (route) => {
				const response = await mf.dispatchFetch(route.request().url(), {
					headers: { Cookie: "CF_Authorization=" + credentials.super },
				});
				await route.fulfill({
					status: response.status,
					headers: Object.fromEntries(response.headers),
					body: Buffer.from(await response.arrayBuffer()),
				});
			});
			await page.goto(`https://news.richview.ai/_preview/${alternate.id}/`);
			assert.equal(await page.locator("body").getAttribute("class"), alternate.site.layout);
			assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
			await page.screenshot({ path: join(evidence, `alternate-${width}.png`), fullPage: true });
			await page.close();
		}
		checks.push({ alternate: alternate.id, layout: alternate.site.layout, theme: alternate.site.theme });
	}
	gitHead = "b".repeat(40);
	r = await request("/_publish/activate", "publisher", "POST", {
		release: release.id,
		generation: 1,
	});
	assert.equal(r.status, 409, "A superseded source commit cannot activate");
	assert.equal((await (await request("/_status")).json()).release, release.id);
	checks.push({ supersededActivation: 409, retainedRelease: release.id });
	gitHead = release.commit;
	gitAvailable = false;
	r = await request("/_editor/api/hide", "editor", "POST", {
		id: "measurement-gap",
		reason: "Local platform hide drill",
	});
	assert.equal(r.status, 202);
	for (const path of [
		"/stories/measurement-gap",
		"/_content/measurement-gap/cover",
		"/_content/measurement-gap/report.html",
		"/_releases/" + release.id + "/stories/measurement-gap",
	])
		await expect(path, 404);
	r = await request("/_publish/rollback", "super", "POST", {
		release: release.id,
		generation: 1,
		reason: "Local rollback drill",
	});
	assert.equal(r.status, 200);
	await expect("/stories/measurement-gap", 404);
	r = await request("/_publish/rollback", "super", "POST", {
		release: release.id,
		generation: 1,
		reason: "Late rollback",
	});
	assert.equal(r.status, 409);
	const result = {
		passed: true,
		release: release.id,
		commit: release.commit,
		checks,
		notes: [
			"Real local workerd, SQLite Durable Object and R2 bindings",
			"Locally signed test JWTs through the production jose verifier; provider/GitHub HTTP responses are fixtures",
			"Headless Chromium at 1440 and 390 pixels; network isolated to this workerd instance",
		],
	};
	await writeFile(
		join(evidence, "platform.json"),
		JSON.stringify(result, null, 2),
	);
	console.log(JSON.stringify(result));
} finally {
	if (browser) await browser.close();
	await mf.dispose();
}
