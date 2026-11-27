# Staff inspection of a hosted report

**Proposed operating guide.** This feature is off until the service has an absolute `RICHVIEW_STAFF_ACCESS_DIR` and the independent identity, permission, journal and host controls below have been provisioned. Only the loopback operator listener serves `POST /staff/inspections/{grant}`. The normal operator bearer, customer cookies and ordinary publishing tokens confer no inspection right.

## Approve one inspection

A registrar verifies the written request with the current report owner through an independently verified contact. Record the date, owner ID, staff member, exact version and artifact, purpose and permitted use in the restricted case register. For a binding order, the designated reviewer verifies its authority and scope and records any restriction on disclosing it, its review date and the notice recipient. The inspector cannot approve their own request or edit this register. Do not put report content, support correspondence, order text or personal contact details in the grant.

The inspector creates a personal credential on their managed workstation. The registrar receives only the fingerprint and maps it to a stable, individual staff ID. Use POSIX hosts for these commands. Run the following only against a synthetic setup or during a separately approved rollout.

~~~sh
umask 077
set -C
openssl rand -hex 32 > staff-token
tr -d '\n' < staff-token | shasum -a 256
~~~

On the registrar's restricted mount, enroll that fingerprint. A case record for a synthetic example follows.

~~~sh
uv run --offline --no-project python tools/hosted-staff/staff_access.py enroll staff-alex "$FINGERPRINT"
~~~

~~~json
{
  "actor": "staff-alex",
  "owner": "person-42",
  "case": "case-217",
  "evidence": "permission-217",
  "purpose": "Check the column headings the owner reported.",
  "basis": "owner-permission",
  "sealed": false,
  "org": "aaaa0001",
  "slug": "synthetic-quarter",
  "version": "1.0.0",
  "artifact": {"kind": "data", "path": "rows.csv"}
}
~~~

The service accepts `source`, `html`, `data` or `file`. A data entry returns its exact stored text representation. A linked file returns its original bytes. Data and file grants require one report-relative name. The full version is mandatory. A registrar can use `legal-order` as the basis and set `sealed` only when disclosure is restricted.

~~~sh
uv run --offline --no-project python tools/hosted-staff/staff_access.py issue case-217.json --minutes 15
~~~

Give the returned grant ID to the named inspector. They connect their individually authenticated, destination-restricted tunnel and run the following on their workstation. A successful request consumes the grant, including when its connection or local file write subsequently fails. The output is a new private file. A rendered HTML file can contain report resources and links. Keep it and any derived notes within the authorized case.

~~~sh
uv run --offline --no-project python tools/hosted-staff/staff_access.py read "$GRANT" --credential staff-token --out case-217.csv
~~~

The registrar can end the grant early, disable a credential or clear a disclosure hold after the designated reviewer records clearance. Verify the recipient and record actual delivery of the notice separately. After an uncertain issuance or redemption, check the grant and durable event before issuing anything else. A new inspection requires a fresh decision within the scope of the writing or order.

~~~sh
uv run --offline --no-project python tools/hosted-staff/staff_access.py revoke "$GRANT"
uv run --offline --no-project python tools/hosted-staff/staff_access.py disable "$FINGERPRINT"
uv run --offline --no-project python tools/hosted-staff/staff_access.py disclose "$GRANT"
~~~

## What the customer sees

On the app origin, a signed-in person opens **Account → Staff access history**. The page lists UTC time, outcome, staff ID, case, purpose, report, version and artifact. The session determines the owner. A transfer does not move earlier entries to the next owner's account. “Released” records the server's durable authorization to send the selected bytes. The service cannot establish that a person read them. Refusals with unverified ownership stay in the custodian journal. A sealed order appears only after clearance. An unavailable journal produces HTTP 503. Unknown credentials and unknown grant IDs release no bytes and generate no customer entry.

## Host controls required before enabling the service

The registrar and inspector are different principals. On a separately mounted persistent POSIX filesystem with working file and directory fsync, hard links and advisory locks, root owns the top directory. Create `identities/`, `grants/`, `revoked/`, `disclosed/` and `authority.lock` for the registrar. The hosted service can read those records but cannot change them. Use a setgid service-readable group on the four directories so newly issued mode-0640 records remain readable. Give only the service write access to `events/`, and give the independent security custodian backup access. The inspector has no filesystem access to this mount, the report store, credential store or backups. Preserve non-reused staff IDs across credential rotation.

The service stages each complete event, syncs its contents, publishes it without overwrite and syncs the event directory before returning bytes. One release filename per grant is the durable replay guard. The service rechecks the personal credential, revocation, expiry and current ownership at release. The journal is authoritative across application restarts and report-volume restores. Set both `RequiresMountsFor=/var/lib/richview-staff` and `AssertPathIsMountPoint=/var/lib/richview-staff` on the hosted systemd unit. Keep the unmounted directory empty and root-only; never initialize a replacement directory to resume inspections. The application does not provision the mount or an external archival service. Security administration must configure retention, independent backup, integrity monitoring and rollback detection, then rehearse recovery before rollout.

For the synthetic walkthrough, run the targeted Rust tests from the workspace root in an authorized build window. They use a disposable hosted store. The HTTP test performs a real request through the loopback router in process. The account test renders the signed-in owner's history and a different reader's view. The other fixtures cover data and file reads, replay, ownership, expiry, revocation, withheld orders and loss of the journal.

~~~sh
cargo test --locked -p richview-app --lib server::staff_access::tests
uv run --offline --no-project python -B -m unittest discover -v -s tools/hosted-staff
~~~

## Proposed privacy-page replacement

Keep this draft outside the published privacy page until the host restrictions, case procedure and independent audit are operating, a synthetic restore and failed log write have been witnessed, the coverage start date is known, and Daniel approves the wording.

> RichView's servers store, render and serve the reports you publish here. Before a staff member inspects a report's content, data or attached files, we require the report owner's written permission for that inspection or a binding legal order. We record who was authorized to receive which material, when and for what purpose. From [verified start date], you can see these inspections under Staff access history in your account. We notify you about access required by an order unless the law prevents us from doing so. We review any restriction on notice and tell you when we are permitted to. Anyone with a public report’s link can also open it anonymously. Those visits do not identify the reader in your staff access history.

Define the coverage explicitly if public-link browsing, restored data, email previews, logs or crash material can still be inspected outside the recorded path. Deliberate staff access before the verified start date cannot be reconstructed from this journal.

### Walk through the owner view

The account test builds the actual account responses before and after configuration and after a synthetic grant releases one artifact. Set `STAFF_ACCESS_WALK_DIR` to keep the three HTML files and open them locally. They contain only disposable fixture identities and events.

~~~sh
STAFF_ACCESS_WALK_DIR="$PWD/tmp/staff-access-walk" cargo test --locked -p richview-app --lib server::staff_access::tests::the_account_shows_the_history_link_only_once_the_feature_is_on -- --exact
~~~
