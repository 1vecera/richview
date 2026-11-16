---
title: "Publish: walkthrough"
description: Run a local report server, publish once and keep the source with the result.
styles: [scientific]
---

A RichView server stores rendered reports, their source and their resources. Each published version has a permanent address; the report's shorter address follows its highest release version unless the owner pins another. RichView operates a report server at `https://app.richview.ai`, with reports served from `https://reports.richview.ai`. You can also run your own server.

## Prepare the report

Use the `first-report/` created in [your first report](https://richview.ai/docs/first-report). Publication requires a nonempty **title, description and version**. The starter has these. Keep the version as text shaped like `0.0.1`, and raise it when the content changes. Validation runs again during publication.

## Publish to the hosted server

[Create an account](https://app.richview.ai/sign-up) with your e-mail address. If sign-ups are paused, that page says so; meanwhile, publish to a server you run, as the next section shows. Open the sign-up link in the message, enter your name and password, and sign in. Check your junk folder if the message does not arrive.

Open **CLI tokens**, give a token a name and create it. Copy it now: the server shows it only once. Set the server and token in the shell where you run RichView:

```sh
export RICHVIEW_SERVER=https://app.richview.ai
printf "CLI token: "
IFS= read -r -s RICHVIEW_TOKEN
printf "\n"
export RICHVIEW_TOKEN
richview publish first-report/report.rmd --slug first-report --new
```

Keep the token private and out of shared shell history; revoke it on **CLI tokens** if it is exposed. For an agent, set the same variables in the agent's MCP environment and ask it to publish the report. Publishing sends the source and linked files to the chosen server. A new report is private by default.

Open **Reports**, select the report and choose one of the three settings under **Who can open it**; the choice takes effect at once. **Private** lets the owner and named people open it; organization owners and admins can administer it. Choose your organization to let every member open it, or **Public link** to let anyone with the link open it without signing in. Copy the link from the report page. Publishing another version without a visibility flag keeps the existing setting. The CLI can also set it with `--visibility private`, `--visibility org` or `--visibility public`. Only the report owner and organization owners and admins can change it.

To invite a colleague to the organization, open **Members**, enter their e-mail under **Invite a colleague**, choose a role and select **Send invitation**. They receive a link by e-mail. To give one person access to just this report, open the report's page, enter their e-mail under **People**, choose what they can do and select **Share**. They receive a link; someone without an account can create one from it.

To remove a report, open its page, select **Delete this report** and confirm. This deletes every version; the address cannot be reused. To request deletion of your account and its reports, write to the address in the [Impressum](https://richview.ai/impressum).

A hosted organization holds up to 50 reports and 50 MiB. Each publish request, including encoded source and files, is limited to 32 MiB.

## Test a publishing server on loopback

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
  --server http://127.0.0.1:4820 --slug first-report --new --format json
```

The server keeps running in the background. Open the report URL from the publication result. This disposable loopback example selects **anyone with the link** so you can read it without creating a user. A server's normal default is private; use that default when setting up a real service. A new address needs `--new`; another version at the same address does not.

The application uses `127.0.0.1` and the report uses `localhost` so they have different hostnames. Different ports on the same hostname would share cookies. Use another pair of free ports consistently if these are occupied.

## Publish and inspect safely

With that server still running, identical publication is safe to repeat:

```sh
richview publish first-report/report.rmd \
  --server http://127.0.0.1:4820 --slug first-report --format json
richview report regenerate first-report 0.0.1 \
  --server http://127.0.0.1:4820 --format json
```

The identical version returns `created: false` and keeps its original artifact. Regeneration renders the retained source with the running engine and reports differences; it stores nothing. To revise the report, edit its source, increase `version`, validate and publish again. Different content under an existing version is refused as `publish.version-exists`.

Pin the version you want the short address to serve, then restore version ordering:

```sh
richview report pin first-report 0.0.1 --server http://127.0.0.1:4820 --format json
richview report unpin first-report --server http://127.0.0.1:4820 --format json
```

For this disposable example only, you can delete the stored version when finished. Its immutable address becomes a tombstone and that version number cannot be published again:

```sh
richview report delete first-report 0.0.1 --yes --server http://127.0.0.1:4820 --format json
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

## Use a server beyond your machine

With no server chosen, the interactive CLI offers RichView-operated Cowork at `https://app.richview.ai`, a self-hosted server, or cancel. It remembers your explicit choice without storing your token. To change that choice, unset `RICHVIEW_SERVER` and run `richview publish <report> --choose-server`. An explicit `--server` takes precedence over `RICHVIEW_SERVER`, which takes precedence over the saved choice. A noninteractive command with no choice fails without uploading. A saved choice is per user account on that machine.

Keep tokens out of source and shared command history. Remote bearer-token requests require HTTPS; HTTP is allowed only on loopback. A self-hosted production service needs HTTPS origins, persistent storage, backups and an operating process. The CLI does not deploy that infrastructure for you.
