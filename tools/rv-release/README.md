# Release command (DEL-83)

One command releases one candidate. Daniel gives one go for the candidate (DEL-83, 26 Nov 2026), the command runs the parts he named in a fixed order with each part's rollback ready, every check that can run before a deploy runs first, and the parts stand alone: a failure stops and rolls back only its own part.

Safety, data truth and the `prod` gate stay binding. Nothing here weakens a check: the cost guard, the Windows ceiling, the sign-up pause and reopen, the data snapshot, the gate's receipt check and every check of release 31's kit are in the kit this command runs.

## Use

```sh
# What would run, in order, with each part's gates and rollback (changes nothing)
node tools/rv-release/release.mjs plan --release <release folder>/release.json [--parts windows,prod,site,hosted,public]

# Run it, with Daniel's own words and their date as the go (nothing is spent or changed without them)
node tools/rv-release/release.mjs run --release <release folder>/release.json --go "<Daniel's words, date>" [--parts site,hosted]

# The report of the last run, and the same command with every outward step replaced by a recorded journal
node tools/rv-release/release.mjs report --release <release folder>/release.json
node tools/rv-release/release.mjs run --release <release.json> --stub --replay <recorded steps.tsv> --scratch <folder>
```

`run` without `--go` prints the plan and exits 3. A second `run` resumes: parts that passed and steps already recorded are not run again.

The release's own values (candidate, receipt, previous candidate, tag, Windows ceiling, notes, asset list, the previous release's evidence, the build host's home folder, pins) live in the release folder's `release.json` (schema 1, see `test/fixtures/release-31.json` and `lib/descriptor.mts`). The kit reads it with `jq`, the command with `loadDescriptor`; neither has a release's value written into it.

## The parts, in their fixed order

| Part | Needs | Stages (the go-gated ones need the go) |
| --- | --- | --- |
| Windows walk (`windows`) | | prepare (go): select the route, cut, walk on Windows, record the walk |
| Merge private production (`prod`) | windows | prepare: the packet. deploy (go): private `origin/prod` moves to the candidate |
| richview.ai (`site`) | prod | checks: the dry run and the local-edge check. deploy (go): the Worker deploy and its verification |
| app.richview.ai (`hosted`) | prod, build | checks: the archive. deploy (go): preflight, sign-up pause, host update, mail switch. closing (go): reopen sign-up |
| Public source and release (`public`) | windows, prod, build | prepare (go): the Windows public build. checks: stage and kit. deploy (go): public push, GitHub release, downloads, installer and Windows walks |
| `build` (not selectable) | windows | the seed and the macOS and Linux archives, run when `hosted` or `public` is chosen |

Each part is selectable (`--parts`), so a go can exclude one, as Daniel did with the public release. A chosen part's needs must be chosen too or have passed in an earlier run. `parts.json` is the one definition of this.

## One go, bound to evidence

The go is Daniel's own words and their date, free text. Before a part's outward step, the command appends a binding the part's evidence must match: the candidate, its receipt and manifest, and the part's own digests (the packet, the dry run and the Worker it replaces, the archive digests, the seed and the notes and assets hashes). The binding is written once per part and stage; if the evidence changes after it, that part is refused and the others carry on. Steps read the sentence with `release.mjs go-text`.

## Checks before a deploy

The `checks` stage of each part runs before any deploy, for every chosen part, so a failure refuses a deploy instead of finding the problem on the live site. For `site` this includes a local-edge check (`lib/predeploy.mts`): the staged deploy folder is served by `wrangler dev --local` with no Cloudflare credentials, the candidate's own route and file checks run against it, and so do the read-only header and content probes of the live comparison. Release 31's refusal of `.html` routes (c6a1d0142) is one of the routes it covers. `hosted` verifies the staged archive in its `49-archive` step.

## Independence, rollback and the report

- A part's status is passed, held, refused, failed (nothing deployed), failed and rolled back, or failed and left; a latch `FAILED.<part>` stops only that part. Skipped parts say what they waited for.
- `site` rolls back by itself when the checks after its deploy fail: DEL-05's pre-authorized rollback to the previous deployment (DEL-48). The other parts' rollbacks need Daniel's recovery words; the plan and the report name each one's command and whether its journal keys are ready. A deploy does not start unless its rollback is ready.
- Sign-up is reopened only after the hosted update is done and, when the public part is chosen, after it has finished.
- `report.md` and `report.json` in the journal list each part's outcome, checks, rollback and the go.

## What lives where

Moved into git from `tmp/` (from release 31's kit): the whole step kit (`steps/`: `run`, `lib.sh`, the numbered steps, the recovery steps, the Windows, packet, deploy and Worker-history tools, `kit/`), made descriptor-driven. The orchestrator, parts model, go binding, local edge and tests are new. Not moved: the one-off recovery scripts of the release-31 night and `11-windows-resume.sh`.

Still outside git, read from `tmp/team/launch` unless an environment variable names another place: the cutover kit (`RV_CUTOVER_KIT`), the hosted provider (`RV_HOSTED_PROVIDER`), the operations scripts (`RV_RELEASE_OPS`), `release-27/windows-tools/prepare-source.sh` (`RV_PREPARE_SOURCE`), the previous release's journal and retained evidence, the release folders with their builds, and the board's answer ledger. The provider-section approval of release 29 (`check-privacy.mjs`, `provider-check.sh`) stays a named long-lived approval.

## Tests

`node --test tools/rv-release/test/*.test.mts` (the gate's `tool-tests` step runs them): parts and selection, the go binding, the stage runner (order, independence both ways, refusal before a deploy, rollback, resume), the local edge, and a lint of the kit's bash and node files against `parts.json`. The kit's outward effects are not run by the tests; the first real run is the next candidate's.
