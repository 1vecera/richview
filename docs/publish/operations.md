# Source-to-live operations

The repository contains the native builder, the Cloudflare gateway, the browser desk, content examples and a small trusted publication worker. Use an existing Cloudflare account with Workers, a private R2 bucket and SQLite Durable Objects available. Account plan changes and new spending are outside the setup procedure.

## Build locally

Use the repository's pinned Rust toolchain and `Cargo.lock`; `wasm-bindgen-cli` must match 0.2.129. Node development dependencies are pinned in the root lockfile; the adapter's small runtime dependencies have their own lockfile. The explicit build below uses the release profile at optimization level 3, then the existing RichView packaging step. Record this profile with the runtime hashes: the general contributor build selects a size-oriented Wasm profile and therefore produces different runtime bytes.

```sh
npm ci --ignore-scripts
npm ci --prefix cloudflare/publish --ignore-scripts
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo build --release --locked -p richview-app -p richview-editor \
  -p richview-site -p richview-build -p richview-publish
CARGO_PROFILE_RELEASE_OPT_LEVEL=3 cargo build --release --locked \
  --target wasm32-unknown-unknown -p richview-site --lib
target/release/richview-build --assets-only
cargo test --locked -p richview-publish
node --test cloudflare/publish/test/*.test.mjs
cargo fmt --all --check
```

Build into a new output directory. The explicit commit identifies canonical source; use a clean, inspected checkout for a release.

```sh
richview_source_commit=$(git rev-parse HEAD)
target/release/richview-publish build \
  --site publications/news/site.yaml \
  --runtime dist/web \
  --source-commit "$richview_source_commit" \
  --output tmp/news-release
```

The output contains `release.json` and `objects/<sha256>`. It is a private staging artifact, even when most items are public. Do not point a static web server or a public bucket at this directory. A render error, invalid metadata, resource escape or exceeded size bound fails the build. Reusing an output directory is rejected rather than overwriting an earlier artifact.

## Provision a gateway

Copy `wrangler.publish.toml` for your publication and change its Worker name, private bucket name and custom domain. Keep the Durable Object class and migration. Do not reuse the RichView marketing Worker's bindings. Create the R2 bucket with public access disabled, then deploy the committed gateway with Wrangler. All routes are handled by the Worker; there is no asset-first bypass.

After choosing your own bucket and domain in the copied configuration, the corresponding CLI steps are:

```sh
node node_modules/wrangler/bin/wrangler.js r2 bucket create your-publication-private
node node_modules/wrangler/bin/wrangler.js deploy --config wrangler.publish.toml
node node_modules/wrangler/bin/wrangler.js secret put GITHUB_TOKEN --config wrangler.publish.toml
```

The secret command reads the credential interactively; a managed deployment can instead pass JSON to `wrangler secret bulk` through stdin. Keep public bucket domains disabled. Verify the actual account before running these commands.

Configure these deployment values through the environment-specific Wrangler configuration:

| Value | Purpose |
| --- | --- |
| `PUBLICATION_HOST` | Exact reader hostname, without scheme or path |
| `ACCESS_ISSUER`, `ACCESS_AUD` | This publication's identity provider organization and application audience |
| `GITHUB_REPOSITORY` | Fixed `owner/repository` allowlist |
| `PUBLICATION_BRANCH` | Canonical branch, for example `publish/news` |
| `CONTENT_ROOT` | Fixed content subtree, for example `publications/news` |
| `BOOTSTRAP_SUPER_EDITOR` | Verified initial administrator principal |
| `BOOTSTRAP_GITHUB_LOGIN` | Administrator's separately verified GitHub login, for imported PR review |
| `PUBLISHER_IDENTITY` | Verified dedicated Access service principal |
| `BOOTSTRAP_COMMIT` | Exact reviewed source SHA allowed only for the first activation |

Store `GITHUB_TOKEN` as a Worker secret, through stdin or a secret manager. Use a GitHub App installation token or a fine-grained credential restricted to the intended repository, with the contents and pull-request permissions required by the bridge. The bridge itself also enforces repository, path and action scope. Keep short-lived credentials refreshed by the operator integration; browser sessions never receive them.

Set up Access as described in [access.md](access.md). Verify the reader hostname, identity audience, private bucket and Worker-origin denial before uploading content. A missing binding, invalid token or provider verification error denies access.

The source repository must stay private whenever its history contains restricted, draft or hidden content. Making source public is a separate decision; website access controls cannot protect a public Git tree. Preserve existing branch protection rules. Where repository-plan limits prevent mandatory GitHub review rules, the gateway still requires exact-head independent approval before activation; document the repository's actual protection state.

## Run the trusted publisher

Copy [`operator.example.json`](../../scripts/publish/operator.example.json) into private operator configuration. Point it at the installed, immutable `richview-publish` executable and its matching runtime, and record the executable and two runtime files' SHA-256 hashes. Keep those trusted tools outside the candidate content directory. The runner verifies these files before use and never builds or executes candidate repository code.

Provide `CF_ACCESS_CLIENT_ID` and `CF_ACCESS_CLIENT_SECRET` from the dedicated Access service token, and optionally `PUBLISH_GITHUB_TOKEN`, through your secret manager. Without the latter, the runner uses the signed-in `gh` account's token in memory. No credential belongs in the operator JSON, source files, command arguments or logs.

```sh
node scripts/publish/publisher.mjs /absolute/path/to/operator.json --watch
```

Where the existing managed credential store is read-only, the optional `access-runner.py` launcher can use an already authorized Cloudflare bootstrap credential to rotate the dedicated service token before starting the publisher. It invokes the provider's [service-token rotation API](https://developers.cloudflare.com/api/resources/zero_trust/subresources/access/subresources/service_tokens/methods/rotate/), keeps the new client secret only in memory and replaces itself with the publishing process. The child does not inherit bootstrap credentials. Keep the token UUID (not its secret) in private operator configuration, run only one instance and renew its provider expiration before it expires.

```sh
uv run --no-project scripts/publish/access-runner.py \
  --account YOUR_ACCOUNT_ID --service-token YOUR_DEDICATED_TOKEN_UUID -- \
  node scripts/publish/publisher.mjs /absolute/path/to/operator.json --watch
```

Run this command under your normal service manager. The watch loop polls every 30 seconds and writes structured progress to stdout. The runner must be online to build new previews and publications; an outage leaves the last good hosted release available and pending Git changes queued. Its local lock prevents duplicate runners against the same scratch directory. After a crash, verify that the old process has stopped before removing only that runner's lock directory. Keep private build logs and receipts; clean old failed-job source/output directories according to your retention policy.

The worker fetches only the configured content subtree through Git blobs at an exact commit. Symlinks, submodules, excessive file counts and excessive bytes are rejected. The Rust child receives a minimal environment without inherited credentials. No shell, repository hook, package installer or candidate code runs during a content build. Production upload credentials stay in the trusted parent process.

This is the continuous integration/deployment mechanism for the content publication. Hosted GitHub Actions is manual-only and calls the same `rv-gate pre-pr` command as the local gate. The publishing loop does not wait for an Actions check or require paid hosted runners. The manual job has read-only repository access and no production credentials. The trusted publisher independently rebuilds content before upload and provides an authenticated preview URL through the commit status.

## Author, preview, approve, publish

Add an item folder or edit an existing report and open a PR against the configured publication branch. The browser desk does the same operation for quick edits. The trusted publisher observes in-scope PRs, runs validation and stages an immutable preview. The preview is available only to permitted editorial identities; its assets and draft bytes have the same checks.

Review the preview and the source diff. Approve the exact head through a different super editor in the desk, or use a different mapped super editor's GitHub approval when merging through GitHub. A changed head needs a new build and approval. Only content-subtree PRs can be merged through the desk. Normal platform-source PRs remain subject to the repository's ordinary review process.

After merge, the publisher builds that publication-branch commit and uploads content-addressed objects. Staging verifies every object before registering the release. Activation checks the current Git branch head, independent approval and the expected release generation. An older or superseded job cannot replace the active release. The reader sees one coherent release, including listings, links, feeds and search.

Merge one publication change at a time and wait for it to become live before merging the next. The merge's first parent must be the approved live commit; GitHub merge commits and squash merges support this rule. A review of a PR head cannot authorize an unreviewed change that appeared on its base branch. The desk checks the base before merging and inspects the resulting commit afterward. Imported GitHub approvals receive the same base check before activation. A previously authorized commit remains eligible for recovery.

If someone directly pushes an unreviewed base, or the base races during a merge, the last good release stays live and the new commit is held. The repository owner must preserve the held commit on a recovery branch, restore the publication branch to the recorded approved commit through the repository's permitted recovery process, and resubmit the intended changes as a PR. Do not bypass repository protections or delete the held source. Multi-commit rebase merges are outside this v1 publication contract.

`/_status` reports the live release, source commit and generation. The desk's review queue and audit distinguish pending review, approved, building, failed and live. Compare the expected commit with `/_status`; a successful local build or upload is not delivery evidence.

A transient failed preview can be queued again with **Retry preview** in the desk. A source validation failure needs a corrected PR head. GitHub-merged or closed PRs are reconciled into the desk's status by the publisher.

For an initial explicitly reviewed launch, `BOOTSTRAP_COMMIT` permits only the first activation. It does not authorize future direct pushes or bypass later review. A publication branch can be bootstrapped without merging the platform implementation into the repository's default branch.

## Manual upload, rollback and recovery

Rollback also pauses automatic activation so the polling publisher cannot immediately re-publish the release being investigated. Previews can still build. After correcting and reviewing the canonical branch, a super editor uses **Resume publication after rollback** in the desk with an audit reason. Resuming keeps the selected release live until a new approved release activates; it does not bypass approval or clear any hide.

The same service credential can upload a previously built release without rebuilding it:

```sh
node scripts/publish/upload.mjs tmp/news-release https://news.example.com preview
node scripts/publish/upload.mjs tmp/news-release https://news.example.com activate
```

The activation command captures the generation before uploading; concurrent activation fails with a conflict. An upload or staging failure leaves the previous release live. A failed build is reported through `/_publish/failed`; it changes status, not reader content. If the branch advanced during the build, rebuild the new head instead of forcing the old job through.

A super editor can post `{"release":"PREVIOUS_RELEASE_ID","generation":CURRENT_GENERATION,"reason":"Explanation"}` to `/_publish/rollback` using the authenticated session. The target must have been live previously. Rollback changes the active content pointer and increments the generation. It keeps urgent hides, deleted-item denials and current audience restrictions. Restore content through the normal approved publication workflow when access policy must intentionally change.

Keep the pinned source commit, toolchain, runtime hashes, installed-builder hash, release manifest and build receipt. Rebuild from those inputs into a fresh directory and compare the immutable release identity before replacing missing objects. Preserve the Durable Object's state; deleting it would erase release ordering, authorization, audit and urgent hides. R2 objects alone are not a complete backup. Use the provider's supported storage backup/recovery tools and rehearse recovery before relying on it for sensitive production content.

The gateway runtime and content releases have separate rollback operations. A Wrangler version rollback changes gateway code; an editorial rollback changes content. Compatible gateway versions retain the same storage bindings and schema. Never deploy an old gateway that omits the access checks. Runtime upgrades require their own code review and verification.

## RichView News deployment

The maintained News content is `publications/news` on the repository's `publish/news` branch. Platform implementation changes are reviewed separately from that content branch. The News gateway uses `wrangler.publish.toml`, Cloudflare Access, a private R2 bucket and one SQLite Durable Object. Its trusted publisher is installed under the existing operator host's service manager, with immutable builder/runtime hashes and managed credentials. It polls Git every 30 seconds, validates protected previews and publishes approved canonical commits; it does not depend on a hosted Actions deployment job.

At launch, the source repository is private and `publish/news` has no GitHub branch protection. The gateway independently requires a reviewed exact head and an approved live base; a direct unreviewed push cannot activate content. Hosted Actions was blocked by account billing, and no spending was requested. Local checks and the installed publisher provide the recorded validation. The existing repository protections and marketing deployment were preserved.

Reader login uses the existing Access email/GitHub providers. Owner roles are mapped to verified identities in the authorization ledger. The separate deployment service has no editorial role. The current bridge uses the existing authorized GitHub credential, restricted in application code to this repository and content subtree; replacing it with a repository-scoped App or fine-grained credential remains an operator hardening step. No repository credential is sent to a reader.

To publish another item, sign in at `/_editor/`, create a draft, edit its Rich Markdown and submit it for review. Wait for its protected preview, have a different authorized super editor approve the current revision, and publish. Alternatively, create a content-only PR against `publish/news` and use a separately mapped GitHub reviewer. The same technical Git account cannot approve its own imported PR. Invite and map a second editorial identity through the documented provider and role setup when working as a team. Verify the resulting source commit at `/_status`.
