# Reader access and the editorial desk

Identity is delegated to Cloudflare Access. The gateway verifies the signed Access JWT with `jose`, checking its issuer, audience, algorithm and expiry. It never trusts a caller-supplied email or role header. Passwords, account recovery and sessions belong to the provider. The authentication adapter is isolated in `cloudflare/publish/auth.mjs`; another standards-based provider can replace that adapter while preserving the verified-principal contract.

Cloudflare Access admission establishes membership. It does not grant editorial writes. The Durable Object stores explicit role/scope mappings as an authorization exception to Git-canonical content. Provider secrets and repository write credentials are separate from reader sessions.

| Role | Reader access | Editorial actions |
| --- | --- | --- |
| Reader | Public items and explicitly entitled audiences | None |
| Editor | Entitled items in permitted sections for previews | New drafts, source/metadata edits, submit Git PRs, urgently hide scoped items |
| Super editor | Configured entitlements and section scopes | Independent approval/publish, site curation, restore, audited role administration |
| Publishing service | Deployment APIs only | Upload verified immutable objects, stage builds, activate approved commits |

The publishing service is an infrastructure identity, not an editorial user. Its reserved identity cannot receive an editorial role, approve a content change or lift an urgent hide. Only that trusted content-only service can upload, stage or activate rendered artifacts; super editors publish through the Git review workflow and can select a previously live release for rollback. Scope and audience checks apply to source reads and edits as well as rendered previews.

## Provider setup

Create a self-hosted Access application for the publication's `/_auth/*`, `/_editor/*`, `/_preview/*`, `/_publish/*` and `/internal` paths. Use explicit person or group allow rules. Add a Service Auth policy containing only the dedicated publishing service token; do not reuse an unrelated application's token or an everyone rule. Existing identity providers, including a managed email PIN provider, can be selected in Access. Do not change an account-wide setting to make this application work.

Set `ACCESS_ISSUER` to the Access organization HTTPS origin and `ACCESS_AUD` to this application's audience. The gateway checks the assertion header and, for ordinary item requests outside the Access path matcher, the signed `CF_Authorization` cookie. A valid reader session can therefore open a permitted item at its canonical story URL. An unauthenticated restricted request gets a generic unavailable response without item metadata. `/_auth/login` is the provider-protected sign-in entry and returns the reader to `/internal`.

The deployment host allowlist also rejects an alternative Worker origin. Keep `workers_dev = false` and `preview_urls = false`; do not add a public R2 domain. Generic Wasm/JavaScript runtime bytes contain no report data. All content objects, images, downloads, search results, preview pages and historical-release paths are checked by the gateway.

## Roles and scopes

`BOOTSTRAP_SUPER_EDITOR` is a verified principal key such as `email:owner@example.com`. It establishes the initial administrator. A super editor can then use the desk's Roles & scopes form to assign another identity a role, section list and audience list. `*` grants the whole corresponding scope. Use an empty audience list to remove member entitlements; revoke the identity in Access when it should no longer authenticate. A role assignment is audited and immediately affects subsequent requests.

Principal keys for service authentication use the verified JWT's `common_name`, prefixed by `service:`. `PUBLISHER_IDENTITY` names the single dedicated deployment service. Never infer this key from a display label. Verify it at the protected `/_auth/session` endpoint during setup.

Retire temporary verification identities by assigning `reader` with empty section and audience lists before revoking their provider credentials. This also denies still-valid signed sessions. A super editor may disable its own entitlement this way only after another full-scope super editor is assigned; self-promotion and self-granted scopes are rejected. Keep the explicit empty entitlement record so an older bootstrap configuration cannot restore that identity's permissions.

Imported GitHub pull requests need a separately verified `github_login` mapping for approval through the desk. Set it in Roles & scopes after confirming the person's repository identity. Browser-originated changes already retain their authenticated editorial actor independently of the technical Git credential. GitHub-originated publication merges may also be approved through GitHub reviews: the gateway requires an approval of the exact head from a mapped super editor other than the PR author, with no truncated review history. Changing a role invalidates that identity's outstanding desk approvals.

Map every known login alias of one person to the same verified GitHub login. Those aliases count as the same actor for approval, publication and restoration, including case differences in the GitHub login. Provider authentication alone cannot discover that two unrelated accounts belong to one person; role administration must preserve this verified mapping.

## Source editing and conflicts

The browser desk requires a private canonical repository and rejects a public destination before writing any Git object. Protected website previews cannot conceal source committed to a public repository. The desk opens a file at an explicit Git commit and shows that revision. Submit for review creates a new branch, commit and PR. The commit records the authenticated actor and inspected revision; the bridge's technical credential never becomes a reader credential. The bridge has a fixed repository, publication branch and content subtree. The browser can edit `site.yaml` (super editor only) or `items/<id>/report.rmd`; it cannot select another repository or write arbitrary code paths.

If the publication branch changes after a file was opened, submission returns a conflict. The browser preserves unsaved text. Reload the current Git source, reconcile the change, then submit again. Git's own PR merge conflict checks remain in force. A failed Git request is reported rather than quietly creating a second content store.

A new draft begins with `state: draft`. Change its metadata and text in the source window, then submit. A protected preview must build successfully before approval. Approval is bound to the exact head SHA. A different super editor reviews the result; the author cannot approve or publish their own restricted publication action. The publish action merges only the intended content PR through the GitHub API and respects any existing repository protections. Source-code PRs are outside this bridge.

The desk displays draft, pending review, approved, building, live, hidden and failed states from Git operations, build receipts, the active release and the hide ledger. A successful build is not automatically a live release.

## Urgent hide and restore

Urgently hide requires an editor with access to the item's section and audience, plus a reason. The gateway records and audits the denial **before** trying to reconcile `state: hidden` back to Git. If Git is unavailable or its revision changes, the hide remains active and the desk reports pending reconciliation. Retry the hide action after the repository is reachable to create the reconciliation PR.

After the hide acknowledgement, new requests through canonical URLs, stable-ID aliases, former slugs, item assets, downloads, protected previews and old hosted release routes are denied. Discovery pages, search, RSS and sitemap omit the item. Responses use `private, no-store`; this service does not place content in a public CDN cache or a service worker. A policy change detected during storage reads prevents the response from being served under the old decision. Requests already delivered, in-flight network bytes and copies a reader previously downloaded cannot be recalled.

A later merge or rollback cannot clear the runtime hide. A different super editor must explicitly restore it, with a reason. The canonical Git source and active release must already say `published`. If they say `hidden`, first approve and activate the Git restoration while the runtime hide remains in force. If the urgent reconciliation PR is still open and the canonical source was never hidden, restoration closes that stale reconciliation PR before lifting the denial. Every restoration is audited.

Rollback preserves current access floors: a current deletion or more restrictive audience still denies access to older versions. A legitimate future publication may deliberately change the audience through the approval workflow. Runtime hides remain an independent veto until explicitly restored.
