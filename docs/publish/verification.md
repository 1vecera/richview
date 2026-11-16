# Verification record

This document distinguishes independently exercised behavior from inherited contracts and remaining deployment checks. A local build is not a delivery claim.

## Reproduce the checks

Run the workspace checks in `CLAUDE.md`, then the publication-specific checks:

```sh
cargo test --locked -p richview-publish
node --test cloudflare/publish/test/*.test.mjs
node scripts/publish/verify-platform.mjs tmp/news-release tmp/publish-evidence
```

The platform check bundles the actual gateway and runs it in local workerd with SQLite Durable Object and private R2 bindings. It verifies JWT signatures through the production authentication adapter, protected routes, item assets, discovery output, role denials, persistent hide and rollback. Its identity-provider and GitHub HTTP responses are explicit fixtures. It opens the actual publication in isolated headless Chromium at desktop and mobile widths. These local fixtures do not prove that a deployed provider or repository is configured correctly.

The gateway tests exercise direct content denial, protected previews, failed Git reconciliation, rollback access floors, stale activation, forged identity headers and cross-origin writes. Repository tests exercise revision conflicts, non-forced branch updates and independently mapped exact-head approvals.

The Rust builder tests exercise deterministic output, real renderer integration, audience and alias changes, failed-output preservation, date validation and resource containment.

## Bounded fixture build

Generate representative content outside the authored tree, build it with the same trusted binary/runtime, and record elapsed time and output size:

```sh
node scripts/publish/fixtures.mjs tmp/publication-fixtures 300
target/release/richview-publish build \
  --site tmp/publication-fixtures/site.yaml \
  --runtime dist/web --source-commit "$(git rev-parse HEAD)" \
  --output tmp/publication-fixture-release
```

Do not commit these generated items or outputs. A successful fixture run demonstrates the tested content shape on the recorded machine; it does not establish a reader concurrency or throughput guarantee.

## Deployment acceptance

Before claiming delivery, record the code commit, canonical content commit, live release ID, Worker version and public URLs. Exercise a content PR addition and correction, browser edit returning to Git, provider-backed reader and role denials, hide/restore across direct and historical routes, a failed build, stale activation and rollback. Record whether identities were operator test services or human sessions; service-identity drills do not claim independent human review.

Inspect representative desktop and mobile output, report interactions. Rebuild from the pinned source/toolchain and follow the operator instructions to publish another item. Keep restricted screenshots, identity details, private operator paths and audit exports out of a public documentation tree.

### Exercised on 18–19 October 2026

The public deployment is [RichView News](https://news.richview.ai). Its [status endpoint](https://news.richview.ai/_status) identifies the active content commit and immutable release. Implementation review is [PR70](https://github.com/richview-universe/richview-v3/pull/70); that platform PR remains a draft, separate from the content publication branch.

The deployed gateway source is `439c5cbb38b0fadd5f835b7197b83d3aeea11455`. After the account-alias guard and retirement of verification identities, the inspected Cloudflare Worker version is `e54ea01f-e1d6-4626-b47c-a9b968135db7`, deployment `008f867e-bef3-4d69-9261-eebdd76c0681`. Provider inspection confirmed private R2 storage with no public bucket domain, and disabled `workers.dev` and Worker preview URLs.

The installed trusted publisher revision is `b4b45a0678033f704a6abd29d4ee79c54a4dd648`; its builder, runtime and publisher source are unchanged through the deployed gateway revision. Rust 1.99.0, wasm-bindgen 0.2.129 and the explicit optimization profile in [operations](operations.md) produced these retained inputs:

| Trusted input | SHA-256 |
| --- | --- |
| Native publication builder | `88acc6f0220fed5a19c2f05d5a28bf286b61dd96ef4b7ab34670c27442af3e60` |
| `richview_runtime.js` | `b5b41f6721c5d6a1454ed29d3db7c23843fb0ecfb8ca1181a141bf4ee16d73e4` |
| `richview_runtime_bg.wasm` | `8b717d8ed58158c3d87b5d78de267711e9727bb135c7320d0764c26a6c3c5624` |

| Published step | Canonical commit | Immutable release |
| --- | --- | --- |
| Initial example publication | `459454867ae05a7fe5f4f3c824cdc5c635d63b6a` | `73c6f08cbd295454bf96c14a0977c056c8c7ec02b4fa613032c937c4b3d656fa` |
| Browser-created story, [PR71](https://github.com/richview-universe/richview-v3/pull/71) | `1075c673c76a7fae1634ef29c9a42db4dcf12c81` | `edb1d59a42779217351c77786c78f51444df30686b7b746575279ce750396b9a` |
| Correction and former-slug alias, [PR72](https://github.com/richview-universe/richview-v3/pull/72) | `7e7d9f8015e4ffff9d3a9abde4349d2a62ca7217` | `33843a62a65ce8c6344c7b3e9ed9e6a5bc4344a4cc804c7ec198d52cb04c0158` |

The installed publisher fetched the initial canonical Git tree independently and reproduced the locally verified release identity. Both content PRs passed protected previews before a distinct provider identity approved their exact heads. The [corrected story](https://news.richview.ai/stories/returning-to-git) is live, the [former URL](https://news.richview.ai/stories/publishing-loop) returns a 308 redirect, and the dated correction is visible. Section listing, search, RSS and sitemap were checked after publication. The browser editor's stale revision was rejected with 409, and a contributor's self-approval was rejected with 403.

A deliberately invalid chart in [PR73](https://github.com/richview-universe/richview-v3/pull/73) failed validation while the corrected release stayed live. Repair produced a successful protected preview; the test PR was closed without merging. An urgent hide created reconciliation [PR74](https://github.com/richview-universe/richview-v3/pull/74) and immediately denied canonical, stable-ID, direct content/asset, preview and historical-release routes. Discovery omitted the item and responses retained `no-store`. Rollback preserved the hide and remained paused across a publisher cycle. A stale rollback generation was rejected with 409. A different super editor restored the item, which closed the unmerged hide PR, then returned to the corrected release and explicitly resumed publication.

The restore drill exposed an overly broad reader guard: unrelated preview staging could cause a reload response. The gateway now tracks access/release revisions separately from audit activity. A regression test proves that unrelated builds preserve reader delivery while a concurrent withdrawal still prevents content delivery. Activation and its rollback-history marker are stored in the same transaction. The final access review also added recognition of known account aliases through verified GitHub mappings: the same person cannot use a mapped alias to approve, publish or restore their own action. A regression test checks denial and a distinct reviewer’s successful approval/restoration; the actual workerd policy suite passed afterward. A final live withdrawal/restoration using [PR75](https://github.com/richview-universe/richview-v3/pull/75) exercised the private-repository guard and completed 32 successful reader requests while a preview was building; the reconciliation PR was closed without merging.

The local workerd/Chromium run exercised Broadsheet/Paper and Dispatch/Ink at 1440 and 390 pixels. It opened the actual report Table view, verified SVG and CSV download contents, decoded all six dashboard charts, switched a captured selection and expanded its query receipt. A policy-only workerd rerun passed after the reader guard fix. The gateway/repository suite has 25 passing tests.

The final deployed browser run repeated report Table interaction and verified SVG/CSV downloads, six decoded dashboard charts, captured selection and query-receipt expansion at 1440 and 390 pixels. It checked keyboard skip navigation and horizontal containment, reported no page errors and retained screenshots that were visually inspected. The live authenticated internal edition and member report also loaded successfully. The real provider issued an HttpOnly, SameSite=None session cookie; the member report and its embedded content remained accessible after the service-authentication headers were removed.

The representative fixture run generated 300 items across sections, public/member audiences and draft/published states. Its bounded full build produced 609 objects totaling 76,972,608 bytes, release `861bcafb562e8bb04a5865a748a457c2ea2ee1ba9bd350cae3ddc54f5680984e`. The observed build took 8.942 seconds on a busy operator machine; this is one fixture observation, not a throughput or concurrency benchmark. Generated fixtures and outputs are kept outside the authored source tree.

### Evidence boundaries

The complete live editorial/provider journey and browser captures ran before the final account-alias guard, at gateway revision `566897c`. The final `439c5cb` revision passed all 25 gateway tests, the actual workerd policy rerun and live HTTP checks. It changes reviewer identity comparison; the native builder, browser runtime, content commit, release and captured reading experience are unchanged. The alias-specific denial and distinct-reviewer success were exercised locally, not through a new human login.

Access drills used distinct, temporary Cloudflare Access service identities with real provider-issued assertions and a provider session cookie. They prove deployed authentication and role enforcement, not independent human review. A human email/GitHub sign-in was not exercised because the signed-in browser connector was unavailable. The anonymous sign-in entry was verified to redirect to the real Access form with email and GitHub options; no human session was created. Provider setup and owner role mappings were inspected separately. The internal demonstration uses safe synthetic member content; no private business data was published.

After verification, all three temporary identities received explicit empty entitlements. Their still-valid authentication was rejected for editorial and internal access, then their provider tokens and admission rules were removed. Bootstrap ownership now names the verified owner identity. The separate publishing service remains enabled; owner access does not depend on a temporary test identity.

Dashboard snapshot pages were retired from Publish by FEA-BR-30; the dashboard evidence above describes the earlier PR #70 deployment.

Operator-retained evidence includes release/build receipts, provider and infrastructure checks, audit exports, failed-attempt logs and representative desktop/mobile screenshots. Restricted screenshots, identity identifiers, credentials and private host paths are deliberately outside this public documentation tree. Workspace formatting, native/Wasm Clippy, all workspace test packages and the source-owned native/Wasm build passed. The complete report/browser, native, editor, compatibility, browser API, motion, showcase, reel, documentation, local Cloudflare adapter and TypeScript declaration checks passed.

A separate native-host marketing-homepage run failed at unchanged `scripts/verify-homepage.mjs:46`: its transient `pending` assertion observed `live` after typing. The full homepage check subsequently passed through the local Cloudflare adapter, including mobile/no-JavaScript, local signup and offline export. The earlier timing failure is retained; the marketing implementation is unchanged by Publish. Hosted GitHub Actions remains billing-blocked, with no spending requested. Local checks and the installed trusted publisher provide the recorded validation.

## Local preview

`node scripts/publish/serve-local.mjs <release-dir> [port]` serves a built release on `127.0.0.1` through the real gateway bundle, with local workerd, private R2, the SQLite Durable Object and locally signed Access tokens. It uploads, stages and activates the release, then serves the public reader until stopped.Reports load the shared runtime from a root-relative `/_runtime/` path, so one release works on the live host, the preview host and this local server. The table, CSV and SVG controls work offline.
