Refs S27-
decision: none
cause:
items:
assumed:
Docs and demo:

## What changed and why

## Retired tests

<!-- One line per deleted or skipped test: - `path::name` — reason (INT-49, replaced by …) -->

## Checklist

- [ ] One Linear issue, one worktree from fresh `origin/dev`, one branch (DEL-42)
- [ ] `rv-gate fast` and `rv-gate pre-pr` pass locally
- [ ] Tests added or updated for every behaviour this PR changes; obsolete tests listed above with reasons
- [ ] `decision` is right: a capture, token, typography, motion or copy change is `design`; added, changed or removed scope is `feature` (DEL-32)
- [ ] Every unanswered card this PR relies on is under `assumed:` (the organizer records it in `ASSUMED.md` on the private `team` branch; never add that file here)
- [ ] Docs, the guide and `docs/surface/` updated in this PR when behaviour or public surfaces change (DEL-45, DEL-53)
- [ ] No website or highly public-facing change without Daniel's grant (DEL-06)
