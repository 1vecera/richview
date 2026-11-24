---
title: "Publish: walkthrough"
description: Run a local report server, publish once and keep the source with the result.
styles: [scientific]
---

Publishing gives a report a link your colleagues can open. RichView Cowork, the report server, stores each rendered report with its source and its resources, and every published version keeps a permanent address, so a link in a decision record still shows the same numbers next year.

There are two places to publish to, and the command is the same for both:

- **RichView's server.** RichView runs Cowork at `https://app.richview.ai`, with reports served from `https://reports.richview.ai`. Check whether [sign-up](https://app.richview.ai/sign-up) is open if you need an account.
- **Your own server.** The `richview` command you installed is also the server. Run it on a machine your colleagues can reach, for example a virtual machine in Azure, and the whole team publishes there: reports, source and data stay on your machine, behind your sign-in. [Run your own server](https://richview.ai/docs/self-hosting) sets one up for a team.

Prepare the starter below, then choose [the hosted server](https://richview.ai/docs/publishing#publish-to-the-hosted-server) or [a server on this computer](https://richview.ai/docs/publishing#try-your-own-server-on-this-computer).

## Prepare the report

Use the `first-report/` created in [your first report](https://richview.ai/docs/first-report). Publication requires a nonempty **title, description and version**. The starter has these. Keep the version as text shaped like `0.0.1`, and raise it when the content changes. Validation runs again during publication.

## Publish to the hosted server

[Create an account](https://app.richview.ai/sign-up) with your e-mail address. If sign-ups are paused, that page says so; meanwhile, publish to a server you run, as the next section shows. Open the sign-up link in the message, enter your name and password, and sign in. Check your junk folder if the message does not arrive.

Publish from the folder that holds `first-report/`:

```sh
richview publish first-report/report.rmd --slug first-report
```

The first time, the command asks where to publish and signs this computer in. It shows a short code and opens a page on the server; check that the page shows the same code and choose **Allow**. The command then carries on by itself. Allow a code only if you started the sign-in yourself.

A server without that page asks for a CLI token instead: open **CLI tokens**, create one and paste it. It is read without being shown.

Either way the command keeps a token for that server in a file only you can read, `credentials.json` in `~/.config/richview/` (on Windows `%APPDATA%\richview\`; under `$XDG_CONFIG_HOME/richview/` when that is set), so later commands publish without signing in, and so does your agent when you ask it to publish. `richview login` signs in without publishing. `richview logout` forgets the token and, on a server with the sign-in page, revokes it; you can also revoke it on **CLI tokens**, where it is listed as “Command line on” your computer's name. Publishing sends the source and linked files to the chosen server. The first publish creates a private report by default and prints its address and version. To publish a revision there, keep the slug and raise the version. Set `slug:` in frontmatter to keep the address when the title changes.

A script or a CI job has nobody to ask. Create a token on **CLI tokens** and give the job the server and the token from your secret store as `RICHVIEW_SERVER` and `RICHVIEW_TOKEN`; they take precedence over a kept sign-in.

A report published to the hosted server names its page in the app on a **Share** line (`share` with `--format json`). Open it, or open **Reports** and select the report, and choose one of the three settings under **Who can open it**; the choice takes effect at once. **Private** lets the owner and named people open it; organization owners and admins can administer it. Choose your organization to let every member open it, or **Public link** to let anyone with the link open it without signing in. With a public link, anyone can read the original Markdown and save a copy. Copy the link from the report page. Publishing another version without a visibility flag keeps the existing setting. The CLI can also set it with `--visibility private`, `--visibility org` or `--visibility public`. Only the report owner and organization owners and admins can change it.

To invite a colleague to the organization, open **Members**, enter their e-mail under **Invite by email**, choose a role and select **Send invitation**. They receive a link by e-mail. A member's role changes from the role beside their name, and **Remove** is in the ⋯ menu beside it; the organization's name and leaving it are under **Organization settings** on the same page. To give one person access to just this report, open the report's page, enter their e-mail under **People**, choose what they can do and select **Share**. They receive a link; someone without an account can create one from it.

To remove a report, open its page, select **Delete this report** and confirm. This deletes every version; the address cannot be reused. To request deletion of your account and its reports, write to the address in the [Impressum](https://richview.ai/impressum).

A hosted organization holds up to 50 reports and 50 MiB.

## Try your own server on this computer

The server is one command. Here it runs on loopback, where only this computer reaches it, with a throwaway token and a throwaway folder; a server for your team runs the same command on a machine of yours with real names and HTTPS, as [run your own server](https://richview.ai/docs/self-hosting) shows.

This Bash example uses `openssl` and `curl` as well as `richview`. Run it in the working folder containing `first-report/`. It creates a disposable token without printing it and stores reports only in `tmp/local-publish-store/`.

```sh
export RICHVIEW_SERVER=http://127.0.0.1:4820
export RICHVIEW_TOKEN="$(openssl rand -hex 24)"
richview server --data-dir tmp/local-publish-store \
  --port 4820 --public-url http://127.0.0.1:4820 \
  --report-port 4821 --report-origin http://localhost:4821 \
  --default-visibility public &
richview_test_pid=$!
trap 'kill "$richview_test_pid"; unset RICHVIEW_TOKEN RICHVIEW_SERVER' EXIT
curl --fail --silent --show-error --retry 10 --retry-connrefused --retry-delay 1 \
  http://127.0.0.1:4820/health && echo
richview publish first-report/report.rmd \
  --server http://127.0.0.1:4820 --slug first-report
```

The server keeps running in the background. Open the report URL from the publication result. This disposable loopback example selects **anyone with the link** so you can read it without creating a user. A server's normal default is private; use that default when setting up a real service.

The application uses `127.0.0.1` and the report uses `localhost` so they have different hostnames. Different ports on the same hostname would share cookies. Use another pair of free ports consistently if these are occupied.

## Publish a folder with data and pictures

Download the [report source](https://richview.ai/docs/gallery/geyser-waits/report.rmd), [CSV](https://richview.ai/docs/gallery/geyser-waits/data/waits.csv) and [JPEG](https://richview.ai/docs/gallery/geyser-waits/media/old-faithful.jpg). Save them with these names and folders:

```text
geyser-waits/
  report.rmd
  data/waits.csv
  media/old-faithful.jpg
```

Run this from the directory containing `geyser-waits/`:

```sh
richview publish geyser-waits --slug geyser-waits
```

Use the same server as for your first report. To use loopback, keep the example server running for this step. The command sends the report and its referenced data, local pictures and linked downloads. Local pictures can be PNG, JPEG, SVG, WebP or GIF. The chart reads `data/waits.csv`, the report shows `media/old-faithful.jpg`, and the download link serves the CSV from the published version. A reader needs only the report link. [Open the live example](https://richview.ai/docs/gallery/geyser-waits/Rendered/report.html) to see the picture and try the download. [Blocks & media](https://richview.ai/docs/blocks#use-a-local-picture) shows how to reference a picture.

The publish request can hold up to **32 MiB**, including the source and encoded files. A missing referenced file stops the publish. Fix the path or restore the file inside the report folder and run the command again.

## Publish and inspect safely

With that server still running, identical publication is safe to repeat:

```sh
richview publish first-report/report.rmd \
  --server http://127.0.0.1:4820 --slug first-report
richview report regenerate first-report 0.0.1 \
  --server http://127.0.0.1:4820
```

The identical version answers `Already published at` (`created: false` with `--format json`) and keeps its original artifact. Regeneration renders the retained source with the running engine and reports differences; it stores nothing. To revise the report, edit its source, increase `version`, validate and publish again. Different content under an existing version is refused as `publish.version-exists`.

On this computer RichView remembers which source files have published to each server. If the same file later names an unused slug, publishing stops with `publish.new-slug`. Correct the slug to update the original report. To create a separate report intentionally, pass `--new` (or `new: true` in MCP). At an existing address `--new` still applies the usual edit permission and version checks. Moving the file or using a fresh configuration starts a new local history.

Pin the version you want the short address to serve, then restore version ordering:

```sh
richview report pin first-report 0.0.1 --server http://127.0.0.1:4820
richview report unpin first-report --server http://127.0.0.1:4820
```

For this disposable example only, you can delete the stored version when finished. Its immutable address becomes a tombstone and that version number cannot be published again:

```sh
richview report delete first-report 0.0.1 --yes --server http://127.0.0.1:4820
```

Stop only this example's server when finished:

```sh
kill "$richview_test_pid"
trap - EXIT
unset RICHVIEW_TOKEN RICHVIEW_SERVER
```

## Choose the right link

| Link | What it identifies |
| --- | --- |
| `/r/first-report` | Highest release version, or the owner's pinned version. |
| `/r/first-report/v/0.0.1` | One immutable version. |
| `/raw/first-report/v/0.0.1` | Its retained Rich Markdown source, subject to access rules. |

Use a versioned address in a decision record or citation. Use the short address when readers should follow revisions. Read [publishing and access](https://richview.ai/docs/access) for permissions, pins, deletion, quotas, API routes and hosting. [HTML and PDF export](https://richview.ai/docs/export) is the simpler path when a file is enough.

## Choose where the command publishes

With no server chosen, the CLI on a terminal offers RichView Cowork at `https://app.richview.ai`, a self-hosted server, or cancel, and remembers your choice. To change that choice, unset `RICHVIEW_SERVER` and run `richview publish <report> --choose-server`. An explicit `--server` takes precedence over `RICHVIEW_SERVER`, which takes precedence over the saved choice. A saved choice is per user account on that machine.

A token given with `--token` or `RICHVIEW_TOKEN` takes precedence over the sign-in the computer keeps. A kept token is sent only to the server it was kept for, and `RICHVIEW_TOKEN` only to `RICHVIEW_SERVER`: set without `RICHVIEW_SERVER`, it is not used, and a command uses the sign-in the computer keeps for its server instead and says so in one line. A command without a terminal never asks: with no server chosen, or no token for it, it fails without uploading and names both ways to sign in.

Keep tokens out of source and shared command history. Remote bearer-token requests require HTTPS; HTTP is allowed only on loopback. A server for your team needs two HTTPS names, a disk you back up and someone who looks after it: [run your own server](https://richview.ai/docs/self-hosting) goes through each. RichView starts the server; the machine, the names and the certificates are yours.
