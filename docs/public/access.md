---
title: Server, access and API
description: Identities, immutable versions, permissions and the native server's API.
styles: [scientific]
---

A RichView server gives every published version a permanent address and decides who can open it. This page is the reference for those addresses, the permissions and the API. To publish for the first time, start with the [publishing walkthrough](https://richview.ai/docs/publishing); to set up a server for your team, for example on a virtual machine in Azure, follow [run your own server](https://richview.ai/docs/self-hosting).

RichView Cowork is the report server: RichView runs it at `https://app.richview.ai` and serves reports from `https://reports.richview.ai`. Create an account there for an individual, revocable author token, or run your own server.

## Version and source behavior

| Path | Native server behavior |
| --- | --- |
| `/r/first-report` | Highest release version, unless pinned by its owner. |
| `/r/first-report/v/0.0.1` | Immutable rendered version. |
| `/raw/first-report/v/0.0.1` | Retained Rich Markdown source. |
| `/api/reports/first-report/versions` | Stored version list. |
| `/api/reports/first-report/v/0.0.1` | Retained source, resources and styling bundle. |

`POST /api/reports` creates an unused address by default. A client that intends to revise an existing report can send `"new": false` to require that address. An unused slug then returns HTTP 409 with `publish.new-slug` and stores nothing. Creation requires permission to publish, and every revision requires edit access to the report.

Publishing the same slug/version again returns `created: false` and preserves the first content. Increase the version to create a revision. The address `/r/<slug>` serves the highest release version, ranked by SemVer (a prerelease ranks below its release), unless the owner pins one; publishing a lower version does not move it. Changed content under a published version is refused as `publish.version-exists`, nothing is ever pruned, a deleted version answers 410 and can never be published again, and a server quota refuses a publication as `publish.quota`. The server retains source/resources for authenticated regeneration.

## Who can read a report

A new report is private to its owner and the people the owner names; the owner or an admin can share it with the whole organisation or with anyone who has the link (`richview server --default-visibility` changes the starting point for the server). People sign in with an email and a password, or a script uses a person's token. Admins add people and roles on the `/governance` page and see the audit trail there. A report you may not see answers exactly like one that does not exist.

Sign-in accepts email addresses up to 254 bytes and passwords up to 4,096 bytes. An existing longer password needs an administrator to reset it.

Report pages are served from an origin of their own, apart from sign-in and the API: `richview server` listens on a second port (`--report-port`, one above `--port` by default) and takes its public name from `--report-origin`; `--public-url` and `--report-origin` go together. Give the report origin a different host name from the app's, not only another port, because browsers share cookies between the ports of one host (the loopback example uses `127.0.0.1` for the app and `localhost` for the reports). The app's sign-in cookie is never sent on to report pages. Opening a report from the app hands a signed-in person over to the report origin with a one-minute, single-use ticket for that one report. "Anyone with the link" reports are not listed to anonymous callers.

`POST /api/validate` and `POST /api/check` run the `validate` and `check` operations on a report sent as text with its folder inventory, answer the same JSON as `--format json`, and store nothing. They need the right to publish.

A server for a team needs a machine of your own with HTTPS names and storage you back up. No `richview` command creates that machine for you.

## Run a server for your team

[Run your own server](https://richview.ai/docs/self-hosting) is the walkthrough: the machine, the names, HTTPS, your colleagues and their first report. The command itself, in short:

Use two HTTPS hostnames behind a TLS reverse proxy: one for the application and sign-in, one for rendered reports. This example listens only on loopback for that proxy. Replace the example domains and admin email with your own before using it as a service. The data directory needs persistent storage and backups.

In Bash, read the first admin password without echoing it or placing it in shell history:

```sh
read -r -s -p 'Initial admin password: ' RICHVIEW_ADMIN_PASSWORD
export RICHVIEW_ADMIN_PASSWORD
richview server --bind 127.0.0.1 --port 4832 --report-port 4833 \
  --public-url https://richview.example.org \
  --report-origin https://reports.example.org \
  --admin-email admin@example.org --quota-bytes 1073741824 \
  --data-dir ./team-reports
```

The first start creates the admin only when the server has no people yet. Open the application's sign-in page through your HTTPS proxy, then use `/governance` to manage people and roles. The byte quota is one GiB; choose a capacity appropriate to your retained source and exports. Leave the normal private visibility default in place.

Use `--bind 0.0.0.0` only when the listener must accept connections beyond loopback, with the network and HTTPS proxy deliberately configured for it. Different ports on one hostname do not separate cookies; keep the two hostnames distinct. This command starts the server but does not provision DNS, certificates, a proxy or a managed host.

Stop it with Ctrl+C, then clear the password from this shell with `unset RICHVIEW_ADMIN_PASSWORD`. For a disposable local publishing exercise with no DNS or proxy, use the [loopback walkthrough](https://richview.ai/docs/publishing).
