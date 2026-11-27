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

The first time, the command asks where to publish and signs this computer in. It opens your browser on the server's sign-in page; sign in there the way you always do, with GitHub, Microsoft, Google or your password, and choose **Allow**. The browser comes back to the terminal by itself, and the terminal shows which account and organization the server signed this computer in as and asks you to confirm before it keeps anything or publishes. The terminal and the page both show a short code; choose **Allow** only if they show the same one, because it is what tells your own request from a link someone else's program on the same computer sent you. The page also names your computer and the account it will publish as. Answer **y** at the terminal only if the account and organization it names are yours; anything else ends that sign-in on the server and keeps nothing. The page the browser is left on never says whose sign-in it is.

```text
$ richview login
Opening https://app.richview.ai in your browser to sign in.
If it does not open, use this address:
https://app.richview.ai/cli/authorize?redirect_uri=…
Press Allow only if the page shows the code K7QM-4TZA.
The server signed this computer in with:
  address       ada@example.com
  organization  Analytical Engines
  name          “Ada Lovelace”
Keep this sign-in? Answer y only if that is you and the organization you meant [y/N]: y
Signed in to https://app.richview.ai as Ada Lovelace (ada@example.com), for Analytical Engines. This computer stays signed in; `richview logout` ends it.
```

If the browser does not open, the command prints the address to open on this computer. Over SSH, on a computer with no desktop, with `--no-browser` or with `--device`, there is no browser on this computer for the sign-in to come back to, so the command shows a short code and a page address instead: open the page on any device, check that it shows the same code and choose **Allow**; the page names the organization it signs in to, and signs in to that one only. The command collects the sign-in by itself, then shows the account and organization and asks you to confirm it as above. With nobody at the terminal to ask (a script, a pipe, `--format json`) it keeps nothing unless you name the account the sign-in must be for, `richview login --device --expect-account ada@example.com`: a sign-in for another account is ended on the server and nothing is kept, and the command says whether the server ended it. The address is compared whole and in any case, never by what the terminal shows of it. `--expect-organization <name>` names the organization too, by its name as the server writes it, whole, and `--expect-organization-id <id>` by its id, which a rename does not change: each flag is compared with its own field only, so an organization named like another one's id is never taken for it, and both may be given. The confirmation prompt shows the whole address and the id. `login --with-token` and a token typed at the terminal are checked the same way: a token belongs to the organization it was made for, the server says which, and a token for another account or another organization, or one whose server names no organization, is not kept when it is expected to be. A server whose own address is `127.0.0.1` offers only the short code, because a browser sends that address's cookies to every port on it.

A server without that page asks for a CLI token instead: open **CLI tokens**, create one and paste it. It is read without being shown.

However it signs in, the command keeps the sign-in for that server in a file only you can read, `credentials.json` in `~/.config/richview/` (on Windows `%APPDATA%\richview\`; under `$XDG_CONFIG_HOME/richview/` when that is set), so later commands publish without signing in, and so does your agent when you ask it to publish. A sign-in made through the browser or with a short code works for an hour at a time and renews itself when you publish, so you are asked again only when it was not used for 90 days, you signed out, or you revoked it; each refresh token works once, and a refresh token that is used after a newer one has replaced it, however long ago and however many renewals since, as a copy of the file would be, ends the whole sign-in and is audited: every refresh token carries the name and a secret of its sign-in, so the server needs no list of the ones it replaced and none can run out. If the answer to a renewal never reaches this computer, the sign-in ends at the next renewal too, and `richview login` signs in again. `richview login` signs in without publishing. `richview logout` forgets the sign-in and, on a server with the sign-in page, revokes it, in the tokens it holds at that moment even when another command renewed them a moment before; you can also revoke it on **CLI tokens**, where it is listed as “Command line on” your computer's name. Publishing sends the source and linked files to the chosen server. The first publish creates a private report by default and prints its address and version. An address whose report was deleted is retired: nobody publishes there again, its owner included, and the server says to publish under a different address. An address that has an access record and no report, such as one an interrupted publication left, stays reserved for its owner and its editors: publishing there keeps the audience it had, and nobody else can publish there, however they ask for a new address. A sign-in that is replaced or ended while a publish waits is never swapped for another account or organization: the publish stops. To publish a revision there, keep the slug and raise the version. On the hosted app you can also [edit the report in your browser](https://richview.ai/docs/online-editor) and save the result as a new version. Set `slug:` in frontmatter to keep the address when the title changes.

A script or a CI job has nobody to ask. Create a token on **CLI tokens** and give the job the server and the token from your secret store as `RICHVIEW_SERVER` and `RICHVIEW_TOKEN`; they take precedence over a kept sign-in.

A report published to the hosted server names its page in the app on a **Share** line (`share` with `--format json`). Open it, or open **Reports** and select the report, and choose one of the three settings under **Who can open it**; the choice takes effect at once. **Private** lets the owner and named people open it; organization owners and admins can administer it. Choose your organization to let every member open it, or **Public link** to let anyone with the link open it without signing in. With a public link, anyone can read the original Markdown and save a copy. Copy the link from the report page. Publishing another version without a visibility flag keeps the existing setting. The CLI can also set it with `--visibility private`, `--visibility org` or `--visibility public`. Only the report owner and organization owners and admins can change it.

To invite a colleague to the organization, open **Members**, enter their e-mail under **Invite by email**, choose a role and select **Send invitation**. They receive a link by e-mail. A member's role changes from the role beside their name, and **Remove** is in the ⋯ menu beside it; the organization's name and leaving it are under **Organization settings** on the same page. To give one person access to just this report, open the report's page, enter their e-mail under **People**, choose what they can do and select **Share**. They receive a link; someone without an account can create one from it.

To remove a report, open its page, select **Delete this report** and confirm. This deletes every version; the address cannot be reused. To request deletion of your account and its reports, write to the address in the [Impressum](https://richview.ai/impressum).

A hosted organization holds up to 50 reports and 50 MiB.

## Credit the organization above the report

Put the publisher's display name and optional logo paths in the style file your report uses. The hosted page shows the publisher before the access message. See the fictional company's mark and try its chart in the [live example](https://richview.ai/docs/example/Rendered/publisher).

```yaml style context=examples/company-style
company:
  name: Your Company Name Inc.
  logo: logo.png
  logo-dark: logo-dark.png
```

A name alone works. Without a name, the host shows its usual bar. Use report-relative still PNG, JPEG or WebP files up to 512 KiB and 1024 px on each side. The mark is 24 px tall (20 px on a phone) and as wide as its shape needs, up to 180 px (120 px on a phone). A mark more than 2.5 times wider than it is tall is treated as a lockup that already carries your name, so the page shows it alone and keeps the name as its alt text; a squarer mark is shown with the name beside it, on one line that ends in an ellipsis when it does not fit. A dark report uses the dark mark if supplied, or a light backing behind the standard mark. Printed and standalone copies carry the mark by the report title.

Everyone who can open a version can see the publisher you supplied, including anonymous readers on a public link. Remove private metadata from logo files before publishing. Access still follows the report's sharing settings. To change the credit, publish another version. Earlier links to a specific version keep their original credit.

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

Use the same server as for your first report. To use loopback, keep the example server running for this step. The command sends the report and its referenced data, local pictures and linked downloads. Local pictures can be PNG, JPEG, SVG, WebP or GIF. The chart reads `data/waits.csv`, the report shows `media/old-faithful.jpg`, and the download link serves the CSV from the published version. A reader needs only the report link. [Open the live example](https://richview.ai/docs/gallery/geyser-waits/Rendered/report) to see the picture and try the download. [Blocks & media](https://richview.ai/docs/blocks#use-a-local-picture) shows how to reference a picture.

Before sending, RichView checks each input against 5,000,000 bytes and their total against 50,000,000 bytes. In this example the CSV counts once. The server measures the source, decoded files and results it receives. See [report input limits](https://richview.ai/docs/data#report-input-limits) for the counting rules. Hosted storage also includes the generated HTML and earlier versions.

The publish request can hold up to **32 MiB** (33,554,432 bytes), including the source and the files. Files travel as base64, which makes them a third larger, so **about 25,000,000 bytes of linked files** is the most one report can publish. `check` and `generate` accept the full 50,000,000 bytes. A report whose request is over 32 MiB is refused before anything is sent, with `report.input-limit` and the size of the request. The same code refuses a request that lists more than 10,000 files or names a file, a source or a folder in more than 512 bytes: `check`, `validate` and `publish` answer it at once, before the server reads the report, and the answer does not repeat the name. A missing referenced file stops the publish. Fix the path or restore the file inside the report folder and run the command again.

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
