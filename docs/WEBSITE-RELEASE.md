# Releasing the website

Moved from the README so the front page stays short. It describes the Cloudflare release of the public site at `https://richview.ai`; deploying needs explicit authorization from the release owner.

The teaser, browser playground, theme showcase and fixed flagship report are served at `https://richview.ai` by the `richview-teaser` Worker using Static Assets. The source repository retains its existing visibility. Local builds and task branches do not publish the site; deployment requires explicit authorization for the affected routes.

For a bounded docs/flagship release, start from an asset snapshot verified against the current production deployment. Build and test the exact committed revision; the Rust builder generates the practical docs alongside the runtime. Then stage only those routes, using the ready producer's verified docs directory for coordinated publication:

```sh
cargo run --release --locked -p richview-build -- --repository-public
uv run --no-project scripts/stage-docs-flagship.py \
  --baseline tmp/production-baseline --docs dist/web/docs \
  --output tmp/docs-flagship-release --manifest tmp/docs-flagship-assets.json
npx --yes wrangler@4.151.0 deploy --assets tmp/docs-flagship-release --dry-run
```

The staging tool preserves every unrelated baseline file, isolates the showcase runtime under the source commit, and packages a portable native report bundle. Its manifest lists every changed and preserved asset with SHA-256 hashes. Verify the staged directory through local Wrangler and browser checks. After the bounded routes are authorized, deploy with `wrangler deploy --assets <staged-directory> --keep-vars`, using the same staged directory; verify exact asset hashes, docs/report interactions and private-path rejection on both `richview.ai` and `richview-teaser.vecera.workers.dev`. Retain the previous deployment/version ID and its asset snapshot for rollback. A full `dist/web` deployment also changes homepage and other routes, and needs authorization for that wider scope. Check the current official Wrangler version/commands before each release.

`wrangler.toml` binds the generated `dist/web/` tree and the custom domain. Cloudflare manages the domain's DNS and TLS certificate. Supply Cloudflare credentials through managed environment variables; do not put them in configuration or command arguments. The builder stages `assets/_headers` at the distribution root, preserving the native server's public-runtime CORS headers for sandboxed previews. The static site does not expose the native host's report publication/storage API; the playground renders and edits reports in the browser. Unknown routes return 404. Deployments preserve the bundled font notices and research snapshot.
