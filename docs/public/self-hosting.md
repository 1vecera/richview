---
title: Run your own server
description: Start a RichView server, sign in as its first admin, and publish a report to it.
styles: [scientific]
---

Start RichView Cowork on your computer and publish a report there. The server keeps the reports, their source and the people who use it in one folder. To give colleagues access, follow the same steps on a machine they can reach and put HTTPS in front.

## What you need

You need [RichView](https://richview.ai/docs/install), Bash, a browser and a writable directory. This walkthrough uses two local addresses, `http://127.0.0.1:4832` for the app and `http://localhost:4833` for reports. The separate hostnames keep the app's sign-in cookie away from report pages.

## Start the server

Run this from a directory where you want to keep your reports. Enter a password of at least 12 characters when asked.

```sh
IFS= read -r -s -p 'Initial admin password: ' RICHVIEW_ADMIN_PASSWORD
printf '\n'
RICHVIEW_ADMIN_PASSWORD="$RICHVIEW_ADMIN_PASSWORD" richview server \
  --admin-email admin@example.test --data-dir ./richview-data --port 4832 \
  --public-url http://127.0.0.1:4832 --report-origin http://localhost:4833
```

Leave that terminal running. The first start creates `admin@example.test` and saves it in `./richview-data`. Later starts use the users saved in that directory.

## Sign in and get a publishing token

Open `http://127.0.0.1:4832` and sign in with `admin@example.test` and the password you entered. Follow **People, roles and audit trail**. Enter a name such as `First report` in **Token name** beside your account and select **New token**. Copy the token when it appears. It is shown once.

## Publish a report

Open a second Bash terminal. Paste the token when asked.

```sh
export RICHVIEW_SERVER=http://127.0.0.1:4832
IFS= read -r -s -p 'CLI token: ' RICHVIEW_TOKEN
printf '\n'
export RICHVIEW_TOKEN
richview init first-report
richview publish first-report --slug first-report
```

Open the report link printed by the command. The app uses your sign-in to open the report on the reader address. To publish a revision, change the source and raise its `version`. Then run `richview publish first-report --slug first-report` again. The short link follows the newest version.

## Update the server

Stop the server with Ctrl+C. [Install the newer release](https://richview.ai/docs/install), then start the server again from the same directory with the same `--data-dir` and origins. Keep a backup of that directory. It contains the users, their roles, the reports and all their saved versions.

## Let colleagues connect

Use a machine that stays on, and point two HTTPS names at a reverse proxy there. Set the first admin's email to your own. In the start command, replace the two local origins with your app and report origins, such as `https://richview.example.org` and `https://reports.example.org`. Point the proxy's app name at `127.0.0.1:4832` and the report name at `127.0.0.1:4833`. Use the app's HTTPS address in `RICHVIEW_SERVER`.

A service manager can start RichView after a reboot. Set `--quota-bytes` to limit how much disk the reports can use. Keep the data directory on a disk you back up.

Use **People and roles** to add colleagues. Readers open shared reports, authors publish and share their own, and admins manage reports and people. Create CLI tokens for the people who publish. [Publishing and access](https://richview.ai/docs/access) covers sharing, versions and permissions.
