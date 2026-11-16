# Hosted Cowork: author access

This runbook applies after the hosted Cowork server has been deployed and its operator has approved an access request. The server is not live merely because this file exists. The operator can see and administer all reports. Keep the service/admin credential private; never give it to an author.

## Issue access

- The visitor requests access by emailing the contact listed at `https://richview.ai/impressum`. Confirm who they are and agree a secure channel for credentials. Email is the request address, not a place to paste a token.
- Sign in to the Cowork app as an administrator over HTTPS and open `/governance`. Add the person with the **author** role, set an individual password and deliver it over the agreed secure channel. The password lets the author sign in to read, manage and share their private reports.
- Under that person's **Password and tokens**, enter a distinct token name and select **New token**. Copy the secret immediately; the server shows it once and stores only its digest. Deliver it through the agreed secure channel. Never copy it into an issue, log, terminal transcript or report.
- Tell the author to set `RICHVIEW_SERVER=https://app.richview.ai` and `RICHVIEW_TOKEN` to their token in their own shell or MCP host environment. They can use `richview publish <report> --new`. A new report is private by default; sign in to the app to share it when desired.

## Revoke access

- In `/governance`, find the person and token name and select **Revoke**. To suspend the person and all their tokens and sessions, select **Disable**. Check the result in the people list and audit trail.
- Ask the author to remove the old token from their environment. If access is still needed, issue a new token and deliver it as above. Do not delete the user merely to revoke one token: deleting a user transfers report ownership to the service account.

Hosted deployment, backups, quotas, incident response and delivery channels need their own verified plan and approval before launch.
