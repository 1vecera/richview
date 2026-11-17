---
title: Upload assets to your own storage
description: Integrity-pinned S3 uploads, verification and resuming transfers.
styles: [scientific]
---

## Upload large data and media to your own storage

`richview asset put` uploads a dataset, image, video or audio file to a bucket you control and prints an integrity-pinned reference: the object's URL, size, SHA-256, media type and visibility. It uses the S3 API, which AWS S3, Cloudflare R2, Google Cloud Storage (the XML API with an HMAC key) and S3-compatible servers such as Garage provide. Report sources do not read these references yet: `data`, `src` and `poster` still take files inside the report folder.

Describe each store once in `stores.yaml` under `$XDG_CONFIG_HOME/richview/` (usually `~/.config/richview/`), or name another file with `--stores` or `RICHVIEW_STORES`. Profiles name places and never hold secrets:

```yaml config
stores:
  local:
    endpoint: http://127.0.0.1:3900
    region: garage
    bucket: richview-private
    public_bucket: richview-public
    public_base_url: http://richview-public.web.garage.localhost:3902/
    credentials: env
  aws:
    region: eu-central-1
    bucket: example-richview-assets
    public_base_url: https://assets.example.com/
    credentials: profile:richview-uploader
```

Without `endpoint`, the store is AWS S3 in `region`, addressed by virtual host; with one, requests use path-style URLs unless `url_style: virtual-host` says otherwise. `credentials: env` reads `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` and an optional `AWS_SESSION_TOKEN`; `profile:<name>` reads static keys from the AWS shared credentials file. For single sign-on or role profiles, export temporary keys into the environment first. Every request is signed locally; keys and signed URLs never appear in references, output or the upload journal. `http` endpoints are accepted only on loopback.

```sh
richview asset put data/readings.csv --to local
richview asset put media/launch.mp4 --to local --public
richview asset verify launch.json --hash
richview asset verify readings.json --to local --hash
richview asset abort
```

Uploads are private unless `--public` is given. Keys are `<prefix><kind>/<first 16 hex digits of the SHA-256>/<file name>`, under `private/` or `public/` by default, so uploading the same file again sends nothing and changed content always gets a new key. Public objects go to `public_bucket` when the profile names one, carry `Cache-Control: public, max-age=31536000, immutable` and are referenced through `public_base_url`, which should be a CDN, custom domain or website endpoint that serves only the public objects. Private references point at the bucket and need the store's credentials to read; a website endpoint must never serve the private bucket.

The media type comes from the file extension and must be one of `text/csv`, `application/json`, PNG, JPEG, WebP, AVIF, GIF or SVG images, MP4 or WebM video, and MPEG, MP4, Ogg or WebM audio. `--type` tells audio from video in the same container, such as `--type audio/webm` for a `.webm` recording. Files up to 16 MiB go up in one PUT; larger files use a multipart upload with equal parts of at least 8 MiB, four at a time. Each request signs the media type and the SHA-256 metadata, so a store refuses a request whose type was changed. A profile's `max_bytes` caps the size, 5 GiB by default.

`put` prints the reference as YAML:

```yaml config
url: https://assets.example.com/public/video/0c6eb955d7b052f7/launch.mp4
bytes: 20037160
sha256: 0c6eb955d7b052f79ab511fb0ab43105ed10c31dcf3ff8169b23133583d4aaf4
type: video/mp4
visibility: public
```

With `--format json`, standard output carries one JSON result with the reference under `asset`, and standard error carries one event per line: `start`, `progress`, `retry`, `restart`, `exists` and `done`.

```jsonl
{"event":"progress","parts":3,"parts_done":2,"sent":16777216,"total":20037160}
{"attempt":2,"event":"retry","part":3,"reason":"503 SlowDown","wait_ms":412}
```

Network errors, timeouts, `429` and `5xx` responses get up to five attempts per request, with exponential backoff and full jitter between them. An interrupted multipart upload keeps a journal in `$XDG_STATE_HOME/richview/uploads/`; running the same command again asks the store which parts it already has and sends only the rest. `richview asset abort` cancels journaled uploads, all of them or those whose SHA-256 starts with a given prefix. Exit codes are 0 for an uploaded or already stored object, 2 for an invalid file, type or profile, 3 for a failed transfer (rerun to resume) and 4 when the stored object differs from the reference.

`verify` reads references saved as YAML or JSON, including the output of `put`. It checks the status, `Content-Length`, `Content-Type` and, for video and audio, `Accept-Ranges: bytes`; `--hash` downloads the object and compares its SHA-256. Private references need `--to` with the store that holds them.

[Garage](https://garagehq.deuxfleurs.fr/), run as a separate process on loopback, is a convenient store for trying this locally. The repository’s `scripts/verify-assets.mjs` exercises uploads against a disposable Garage server; inspect its prerequisites before running it.
