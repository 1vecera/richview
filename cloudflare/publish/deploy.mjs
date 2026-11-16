// Host reason (ARC-03): operator runbook printer for the Cloudflare deployment; it only prints text and calls nothing.
//
// THIS SCRIPT NEVER DEPLOYS. It does not run wrangler, cargo, git or npm, opens no network connection and reads no
// credential. It prints the ordered commands for switching the News Worker from PR #70's JavaScript to the Rust
// server compiled to Wasm, and for going back. Daniel's explicit go is required before anyone runs any of them
// (AGENTS.md: deploys, rollbacks, Cloudflare, secrets).
//
// Usage: node cloudflare/publish/deploy.mjs
const CONFIG = "wrangler.publish.toml";
const WRANGLER = "node node_modules/wrangler/bin/wrangler.js";
const lines = [];
const say = (text = "") => lines.push(text);
const step = (n, title, commands = [], notes = []) => {
	say(`${n}. ${title}`);
	for (const c of commands) say(`     $ ${c}`);
	for (const note of notes) say(`     ${note}`);
	say();
};

say("RichView Publish: deploy the Rust Worker (printed only, nothing was run)");
say("=".repeat(72));
say();
say("Same Worker (richview-news), same Durable Object class `Publication` and migration tag publish-v1, same R2");
say("bucket and routes: only the code changes, so no new Durable Object migration is added. The first request");
say("after the deploy converts the stored ledger in place and keeps PR #70's record as `state-pr70`.");
say();
say("DEPLOY, from the repository root, with Daniel's go for this exact build");
say();
step(1, "Start from the reviewed commit and a clean tree", ["git status --short   # must be empty", "git rev-parse HEAD   # note it with the deployment"]);
step(2, "Install the pinned tools and build the Wasm", [
	"npm ci --ignore-scripts",
	"npm ci --ignore-scripts --prefix cloudflare/publish",
	"npm run build:wasm --prefix cloudflare/publish",
]);
step(3, "Prove parity and the adapter against the built Wasm (offline)", [
	"node --test cloudflare/publish/test/*.test.mjs",
	"node tools/rv-gate/rv-gate.mjs fast",
]);
step(4, "Record what is live now, to roll back to it (read-only)", [
	`${WRANGLER} deployments list --config ${CONFIG}`,
	`${WRANGLER} versions list --config ${CONFIG}   # note the current Version ID as PREVIOUS_VERSION`,
]);
step(5, "Check the secret and variables exist; set nothing now", [
	`${WRANGLER} secret list --config ${CONFIG}`,
], [
	"Secret (already set for PR #70; the Rust server reads the same name): GITHUB_TOKEN",
	"Variables the Worker reads (set at deployment, none in the file): PUBLICATION_HOST, ACCESS_ISSUER, ACCESS_AUD, GITHUB_REPOSITORY,",
	"  PUBLICATION_BRANCH, CONTENT_ROOT, BOOTSTRAP_SUPER_EDITOR, PUBLISHER_IDENTITY, BOOTSTRAP_COMMIT (+ optional BOOTSTRAP_GITHUB_LOGIN).",
	`If a secret must be added: ${WRANGLER} secret put GITHUB_TOKEN --config ${CONFIG}   # interactive; never put a value in a command`,
]);
step(6, "Dry run the bundle (no upload)", [`${WRANGLER} deploy --config ${CONFIG} --dry-run --outdir tmp/publish-bundle`]);
step(7, "Deploy", [`${WRANGLER} deploy --config ${CONFIG}`], [
	"Verify on news.richview.ai: / , /archive , /feed.xml , /_status (release and generation unchanged), a story, a hidden item is 404.",
]);
say("ROLLBACK, to PR #70's Worker, with Daniel's go");
say();
step("R1", "Redeploy the previous Worker version", [
	`${WRANGLER} rollback PREVIOUS_VERSION --config ${CONFIG} --message "Back to PR #70's Worker"`,
	`${WRANGLER} deployments list --config ${CONFIG}`,
]);
say("What the ledger needs after a rollback (verified against PR #70's worker.mjs and policy.mjs at e8aa9e1):");
say();
say("  Reads need nothing. PR #70 loads the stored `state` as it is and ignores fields it does not know; for a");
say("  reader it uses generation, active, paused, hidden, policies, roles and accessRevision (`|| 0` when missing),");
say("  plus `activated:<id>` and `runtime:<id>`, which the Rust server leaves in place. So every GET answers as before");
say("  and every hide, role and policy the Rust server stored is honoured.");
say();
say("  Writes are NOT safe without a restore, so leaving `state` alone is not enough for them. The conversion");
say("  renames auditSequence/accessRevision to audit_sequence/access_revision. PR #70 does `state.auditSequence++`,");
say("  which is NaN when the field is absent, so its first publish, hide, restore, role change or activation writes the");
say("  audit key `audit:0000000NaN` and corrupts the audit order. Releases the Rust server activates (and runtimes it");
say("  stores) after the conversion have no `activated:<id>` / `runtime:<id>` record, so PR #70 cannot read them as");
say("  history or roll back to them. Items an editor published privately (OPN-54) are unknown to PR #70.");
say();
say("  The exact condition for a rollback with no ledger work: the old Worker is used for reading only, until a restore.");
say("  To let it write again, either the stored `state` must still equal the converted one (revision 0, meaning the Rust");
say("  server committed nothing) and `state-pr70` is copied back over `state`, or the camelCase counters are added to");
say("  `state` from audit_sequence and access_revision and the missing activated:/runtime: records are written.");
say("  Neither is a wrangler command: Durable Object storage has no CLI. It needs a one-off, reviewed change to a");
say("  Durable Object method, which is not part of this script. Until then keep PR #70 read-only.");
say();
say("ROLL FORWARD again: step 7. The Rust server recognises a ledger without `revision` only; a restored `state-pr70`");
say("copy is converted afresh, so restore only the whole record, never part of it.");

console.log(lines.join("\n"));
