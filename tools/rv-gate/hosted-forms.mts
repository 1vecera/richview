// Host reason (ARC-03): exercise hosted forms in a real browser against an isolated local server.
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
import { chromium } from "playwright";

const app = new URL(process.env.HOSTED_TEST_APP!);
if (app.protocol !== "https:" || !["localhost", "127.0.0.1", "[::1]"].includes(app.hostname)) throw new Error("A loopback HTTPS test server is required");
const outbox = process.env.HOSTED_TEST_OUTBOX;
if (!outbox) throw new Error("A disposable local outbox is required");
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ ignoreHTTPSErrors: true });
try {
	const page = await context.newPage();
	const posts: { path: string; headers: { [key: string]: string; }; }[] = [];
	page.on("request", async request => {
		if (request.method() === "POST") posts.push({ path: new URL(request.url()).pathname, headers: await request.allHeaders() });
	});
	await page.goto(new URL("/", app).href);
	await page.getByLabel("Email").fill("absent@example.test");
	await page.getByLabel("Password").fill("not-the-password");
	await page.getByRole("button", { name: "Sign in" }).click();
	await page.getByText("Could not sign in. Check your details or try again later.").waitFor();
	await page.goto(new URL("/sign-up", app).href);
	await page.getByLabel("Email").fill("browser@example.test");
	await page.getByRole("button", { name: "Send sign-up link" }).click();
	await page.getByRole("heading", { name: "Check your mail" }).waitFor();
	const operator = new URL(process.env.HOSTED_TEST_OPERATOR!);
	if (operator.protocol !== "http:" || operator.hostname !== "127.0.0.1" || !process.env.HOSTED_TEST_OPERATOR_TOKEN) throw new Error("A loopback operator listener and test token are required");
	const authorization = `Bearer ${process.env.HOSTED_TEST_OPERATOR_TOKEN}`;
	let request: {id: string; to: string} | undefined;
	for (let n = 0; n < 100 && !request; n++) {
		const pending = await fetch(new URL("/operator/links", operator), { headers: { authorization } });
		assert.equal(pending.status, 200);
		request = (await pending.json() as {manualRequests: {id: string; to: string}[]}).manualRequests.find(item => item.to === "browser@example.test");
		if (!request) await new Promise(resolve => setTimeout(resolve, 100));
	}
	assert.ok(request, "signup request reached the disposable outbox");
	for (const name of await readdir(outbox)) assert.doesNotMatch(await readFile(join(outbox, name), "utf8"), /\/link\//);
	const dispatched = await fetch(new URL(`/operator/mail/${request.id}/dispatch`, operator), { method: "POST", headers: { authorization } });
	assert.equal(dispatched.status, 200);
	const mail = await dispatched.json() as {body: string};
	const link = mail.body.match(/https:\/\/[^\s]+\/link\/[A-Za-z0-9_-]+/)?.[0];
	assert.ok(link, "dispatch returned a fresh signup link");
	assert.equal(new URL(link).origin, app.origin);
	const landing = await page.goto(link);
	assert.ok(landing);
	assert.equal(landing.headers()["referrer-policy"], "no-referrer");
	assert.match(landing.headers()["cache-control"], /no-store/);
	await page.getByLabel("Name", { exact: true }).fill("Browser test");
	await page.getByLabel("Password", { exact: true }).fill("browser-test-password");
	await page.getByLabel("Confirm password").fill("browser-test-password");
	const completion = page.waitForResponse(response => response.request().method() === "POST" && new URL(response.url()).pathname.startsWith("/ui/link/"));
	await page.getByRole("button", { name: "Create account" }).click();
	const completed = await completion;
	assert.equal(completed.status(), 303);
	assert.equal(completed.headers()["referrer-policy"], "no-referrer");
	assert.match(completed.headers()["cache-control"], /no-store/);
	await page.getByRole("heading", { name: "Reports", exact: true }).waitFor();
	for (const path of ["/ui/sign-in", "/ui/sign-up", "/ui/link/"]) {
		const post = posts.find(item => item.path.startsWith(path));
		assert.equal(post?.headers.origin, "null");
		assert.equal(post?.headers["sec-fetch-site"], "same-origin");
	}
	const forged = await context.request.post(new URL("/ui/sign-up", app).href, { headers: { origin: "https://attacker.test", "sec-fetch-site": "same-origin", "content-type": "application/x-www-form-urlencoded" }, data: "email=forged%40example.test" });
	assert.equal(forged.status(), 403);
	// Members, with script on and off: every control there is a form, the menus details elements.
	// The new account owns the organization it got at sign-up. Desktop width shows the
	// invitations' row buttons; the role menu opens from its chip.
	const signedIn = await context.storageState();
	for (const script of [true, false]) {
		const members = script ? page : await (await browser.newContext({ ignoreHTTPSErrors: true, javaScriptEnabled: false, storageState: signedIn })).newPage();
		const address = `colleague-${script ? "script" : "no-script"}@example.test`;
		await members.goto(new URL("/members", app).href);
		await members.getByLabel("Invite by email").fill(address);
		await members.locator("label[for='invite-role-author']").click();
		await members.getByRole("button", { name: "Send invitation" }).click();
		await members.getByText(`Invited ${address}.`).waitFor();
		const row = members.locator("tr", { hasText: address });
		assert.equal(await row.locator(".p-role-text").textContent(), "Author");
		await row.getByRole("button", { name: `Send the invitation to ${address} again` }).click();
		await members.getByText(`Invited ${address} again.`).waitFor();
		await members.locator("tr", { hasText: address }).getByRole("button", { name: `Cancel the invitation to ${address}` }).click();
		await members.getByText("Invitation cancelled.").waitFor();
		assert.equal(await members.locator("tr", { hasText: address }).count(), 0);
		await members.locator("tr.is-me details.pick:not(.pick--more) > summary").click();
		const owner = members.getByRole("button", { name: /^Owner/ });
		assert.equal(await owner.getAttribute("aria-pressed"), "true");
		await owner.click();
		await members.getByText("Role saved.").waitFor();
		await members.getByText("You’re its only owner.").waitFor();
	}
	console.log("PASS: Chromium sign-in, sign-up, check-mail and link completion; foreign origin refused; members' invite, send again, cancel and role menu with script on and off");
} finally {
	await context.close();
	await browser.close();
}
