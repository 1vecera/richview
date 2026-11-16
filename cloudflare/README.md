# RichView interest signup preparation

This branch prepares the homepage and signup for review. **No live Worker, binding, DNS or email delivery is changed.** `wrangler.toml` remains the production assets-only configuration. The release owner must exclude this branch from the bounded docs/report release.

## Isolated review

Build `dist/web` using `cargo run --release --locked -p richview-build`, then run:

```sh
npm ci --ignore-scripts
npx wrangler d1 migrations apply richview-interest-s27-590-local --local --config wrangler.interest-preview.toml --persist-to tmp/interest-state
npx wrangler dev --local --config wrangler.interest-preview.toml --persist-to tmp/interest-state --ip 127.0.0.1 --port 8790
RICHVIEW_PREVIEW_URL=http://127.0.0.1:8790 node scripts/verify-interest.mjs
```

Only the local D1 emulator is used. The config has an inert database ID, no account/domain routes and disables hosted preview URLs. Use synthetic addresses ending in `example.invalid`; no mail service is configured. `POST /api/interest` requires same-origin JSON, explicit boolean consent, a valid email and an empty honeypot. Body size is bounded even without Content-Length. Addresses are normalised and deduplicated by the database primary key. The original consent timestamp/version is retained. All success responses wait for D1 acknowledgment and have the same shape for duplicate/new addresses. Rate limiting is ten attempts per minute per network address per Cloudflare location, including invalid attempts. Subscriber rows store no IP or user agent. There is no subscriber read/export endpoint.

The homepage and its source/report demo use the Rust/Wasm engine. The small JavaScript Worker is the Cloudflare HTTP/D1 host adapter. The Cloudflare [rate-limit binding](https://developers.cloudflare.com/workers/runtime-apis/bindings/rate-limit/) and [local D1 workflow](https://developers.cloudflare.com/d1/best-practices/local-development/) define the tested integration.

## Later release, pending explicit authorization

S27-430 alone owns production `richview-teaser` deployment. After Daniel approves the final homepage/signup, the owner may provision a dedicated private D1 database, apply `cloudflare/migrations/0001_interest.sql`, allocate an unused rate-limit namespace and merge the Worker entrypoint, ASSETS binding, `run_worker_first = ["/api/*"]`, INTEREST_DB binding and rate-limit binding into the then-current live configuration. Preserve existing routes/bindings. The prepared preview config must not be deployed. No subscriber list is embedded in `dist/web` or public artifacts; state belongs in `.wrangler`/`tmp` locally and D1 remotely. Do not send confirmation or campaign emails without separate explicit authorization. The native preview server does not implement signup and correctly shows an unavailable/error state.

Review note: Daniel’s tentative “Big view” tagline was not adopted as a product name. The homepage consistently uses RichView and “Coming soon”.
