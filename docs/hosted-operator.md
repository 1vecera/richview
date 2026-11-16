# Hosted Cowork operator

The operator listener is separate from the public listener and binds to `127.0.0.1`. Reach it through a secure connection to the host. Its bearer token is an operator credential: keep it out of shell history, logs and support requests. The operator can administer metadata and reports; the API does not return private report contents.

Set `OPERATOR_URL` to the loopback listener, for example `http://127.0.0.1:8091`, and load `RICHVIEW_OPERATOR_TOKEN` into the environment using the host's approved secret mechanism. Use a randomly generated token of at least 32 ASCII letters, digits, hyphens or underscores. This helper supplies the authorization header over standard input rather than placing the token in command arguments:

```sh
operator() {
  printf 'header = "Authorization: Bearer %s"\n' "$RICHVIEW_OPERATOR_TOKEN" |
    curl --silent --show-error --fail-with-body --config - "$@"
}
```

- List organizations and usage: `operator "$OPERATOR_URL/operator/organizations"`
- Suspend or resume an organization: `operator -X POST "$OPERATOR_URL/operator/organizations/ORG/suspend"` or `operator -X POST "$OPERATOR_URL/operator/organizations/ORG/resume"`
- Remove a report and its versions: `operator -X DELETE "$OPERATOR_URL/operator/reports/ORG/SLUG"`
- Disable a person: `operator -X POST "$OPERATOR_URL/operator/people/PERSON/disable"`
- Permanently remove a hosted account: `operator -X DELETE "$OPERATOR_URL/operator/people/PERSON"`
- Pause or reopen sign-ups: `operator -X POST "$OPERATOR_URL/operator/signups/pause"` or `operator -X POST "$OPERATOR_URL/operator/signups/open"`
- Inspect pending links: `operator "$OPERATOR_URL/operator/links"`; treat its output as sensitive.
- Inspect the daily summary: `operator "$OPERATOR_URL/operator/summary"`

Use the IDs returned by the organization listing; report addresses contain the organization code and slug. Suspending an organization also makes its public reports unavailable. Removal is permanent. Every operation is audited; an audit failure before the action prevents it. If the response says the outcome is uncertain, inspect the current state and audit trail before attempting the action again.

For account deletion, verify the person ID in the private access state first. When the person is the only owner of an organization with other members, name a current member as successor for each such organization in the JSON request; their role becomes owner, and the deleted person's reports remain with that organization. A disabled successor remains disabled, so choose an active member if the organization needs immediate access. Use verified IDs, for example: `operator -X DELETE -H 'Content-Type: application/json' -d '{"successors":{"ORG":"MEMBER"}}' "$OPERATOR_URL/operator/people/PERSON"`. Organizations of which the person is the sole member are deleted with their reports. The operation removes credentials, links, invitations they issued and matching records in the configured outbox, redacts their email from local audit history, and writes one operator audit entry per attempt using the person ID. A partial failure leaves the account disabled; retry the same person ID and successor mapping. External backups and any old outbox not configured on this server need separate cleanup. A later sign-in attempt can create a new refusal audit entry for the address.

## Hosted startup and mail

Hosted mode requires separate HTTPS app and report origins behind a trusted proxy. The proxy must overwrite `X-Real-IP` with the actual client IP; do not trust a value supplied by the client. Exclude `/link/`, `/ui/link/` and report handoff queries from access and application logs. Keep the operator listener on loopback and its token in the host's secret mechanism.

Choose exactly one mail transport: `--mail-outbox` for manual delivery, or SMTP with `RICHVIEW_SMTP_PASSWORD`. For SMTP, set `RICHVIEW_MAIL_FROM` to `RichView <address@example.com>` using the real sender address so the display name is RichView. The default limits are 500 signup accounts, 1,000 mail messages per UTC day, 10 link requests per IP per hour and 60 publish attempts per organization per hour. Manual requests and links have per-address limits of three per hour and ten per day; manual requests also have a 10,000-per-day cap. Half the daily mail budget and half of the 10,000-entry daily request/link history are reserved for existing-account resets, invitations and shares; decoys and requests over the address cap use no mail unit. A person may create one organization, counted even after an ownership transfer; raise `--org-creation-limit` or `RICHVIEW_ORG_CREATION_LIMIT` and restart to change the limit. Existing accounts without a creation record count as one; deletion removes the count, so re-registration starts a new sign-up.

The operator summary reports `accountsHalfFull` and `storageHalfFull` at 50% of the configured account and store limits. The server checks both once a minute and logs one alert on each threshold crossing. Arrange delivery of those alerts to the operator separately. Check the configured `--signup-account-limit` and `--mail-daily-limit` before operating; a check-mail response alone does not confirm that a message was queued.

SMTP uses implicit TLS on port 465 and mandatory STARTTLS on port 587; for another port set `--smtp-tls implicit` or `--smtp-tls starttls`. Cloudflare Email Sending uses `smtp.mx.cloudflare.net` on port 465 with user `api_token` and its API token in `RICHVIEW_SMTP_PASSWORD`.

## Manual outbox

The manual outbox stores pending requests without live links. Requests expire after 24 hours. Immediately before sending, list the pending requests through the operator listener and dispatch the intended request. Dispatch creates a single-use link that expires one hour later and returns the recipient, subject and message text. Send the returned message promptly with sender display name RichView; never send a previously generated link after it has expired.

Work privately as the server user. Pending requests contain addresses; the dispatch response contains a live credential. Set the destination paths below to private files outside web-served and shared folders, use `umask 077`, and keep the response, links and addresses out of logs, shell history and support requests:

```sh
umask 077
operator "$OPERATOR_URL/operator/links" > "$PRIVATE_PENDING_FILE"
operator -X POST "$OPERATOR_URL/operator/mail/$REQUEST_ID/dispatch" > "$PRIVATE_MAIL_FILE"
```

Select `REQUEST_ID` from `manualRequests` after verifying the recipient and kind. Dispatch only the intended request once. If the response reports an uncertain outcome, inspect the pending state and audit before any retry. A failed response, expired request, exhausted budget or rate limit does not mean a message was sent: do not use a partial response or invent a link. Handle private files and delivery records so a message is not sent twice.
