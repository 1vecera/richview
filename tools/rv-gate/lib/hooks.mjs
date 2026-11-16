// The pre-push hook for every checkout of the repository (DEL-37 as amended by DEL-81): agents
// push to `dev` freely; `pre-prod`, `prod`, `main` and candidate branches are refused. The gate
// pushes `pre-prod` from `rv-gate release`, candidate branches itself, and `prod` only through
// `rv-gate merge-prod` on Daniel's recorded go; each marks its push.
//
// Until the gate runs as its own macOS user, an agent could set the same marker, so the audit
// (DEL-38) stays the backstop. That user is a setup step that waits for Daniel's go.

import { chmodSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { sh } from "./proc.mjs";
import { PATCH_ID_SHOW, STACK_DIFF } from "./fastpass.mjs";
import { gateHome } from "./store.mjs";

export const PRE_PUSH = `#!/bin/sh
# RichView pre-push hook (DEL-37), installed by \`rv-gate install-hooks\`.
# Refuses pushes to prod, pre-prod, main and candidate branches from agent checkouts (DEL-81),
# and a push to dev that a passing 'rv-gate fast' run did not cover (review 1k, 1l): every commit
# it carries needs a record, and its tip must be the tip of a stack a run tested (lib/fastpass.mjs).
status=0
zero=0000000000000000000000000000000000000000
stamps="\${RV_GATE_HOME:-\$HOME/.local/share/richview-gate}/fast-pass"
# The patch id of what the command "$@" prints, as lib/fastpass.mjs computes it: empty for no
# change, a failure status when git fails.
pid() {
  d=$(mktemp) || return 1
  if ! "$@" > "$d"; then rm -f "$d"; return 1; fi
  out=$(git patch-id --verbatim < "$d"); r=$?
  rm -f "$d"
  [ $r -eq 0 ] || return 1
  echo "\${out%% *}"
}
while read -r local_ref local_sha remote_ref remote_sha; do
  case "$remote_ref" in
    refs/heads/dev)
      [ "$local_sha" = "$zero" ] && continue
      # A completed hotfix must become an ancestor of dev. The gate may attach exactly the
      # current prod as a second parent without a new run only when dev's tree is unchanged.
      # The marker alone is insufficient: compare both parents and the actual tree to the
      # remote tip advertised for this push. This does not admit any new source bytes.
      if [ "$RV_GATE_PUSH" = "hotfix-backmerge" ] && [ "$remote_sha" != "$zero" ]; then
        parents=$(git rev-list --parents -n 1 "$local_sha" 2>/dev/null)
        prod=$(git rev-parse --verify refs/remotes/origin/prod 2>/dev/null)
        old_tree=$(git rev-parse "$remote_sha^{tree}" 2>/dev/null)
        new_tree=$(git rev-parse "$local_sha^{tree}" 2>/dev/null)
        if [ -n "$prod" ] && [ -n "$old_tree" ] && [ "$old_tree" = "$new_tree" ] && [ "$parents" = "$local_sha $remote_sha $prod" ]; then
          continue
        fi
      fi
      if [ "$remote_sha" = "$zero" ]; then
        range="$local_sha --not --remotes=origin"; base=$(git merge-base "$local_sha" refs/remotes/origin/dev 2>/dev/null)
      else
        range="$remote_sha..$local_sha"; base=$(git merge-base "$local_sha" "$remote_sha" 2>/dev/null)
      fi
      # Fail closed: a remote sha this repository lacks, or any git failure, refuses the push.
      if ! commits=$(git rev-list $range); then
        echo "pre-push: cannot list the commits of $range (is the remote sha fetched? run 'git fetch origin dev'); refusing the push to dev." >&2
        status=1; continue
      fi
      [ -n "$commits" ] || continue
      for c in $commits; do
        # Every commit needs a record, merges included, keyed on the patch id of its diff against its
        # first parent. A merge that carries lines of its own, or a conflict resolution, has no
        # record until a fast run covered it.
        if ! id=$(pid ${PATCH_ID_SHOW} "$c"); then
          echo "pre-push: cannot compute the patch id of $(git log -1 --format='%h %s' "$c" 2>/dev/null | cut -c1-90); refusing the push to dev." >&2
          status=1; continue
        fi
        [ -z "$id" ] || [ -e "$stamps/$id" ] && continue
        echo "pre-push: $(git log -1 --format='%h %s' "$c" | cut -c1-90) is in no stack a passing 'rv-gate fast' run tested; run 'node tools/rv-gate/rv-gate.mjs fast' at the commit you push, then push (DEL-81)." >&2
        status=1
      done
      # The tip: the very tree a run tested, or the whole diff of a stack a run tested at its tip
      # (a rebase that changes no line keeps it; a prefix of the stack or a dropped commit does not).
      tree=$(git rev-parse "$local_sha^{tree}" 2>/dev/null) && [ -e "$stamps/tree-$tree" ] && continue
      if [ -z "$base" ] || ! sid=$(pid ${STACK_DIFF} "$base" "$local_sha"); then
        echo "pre-push: cannot compute the merge base or the stack's patch id of $(git log -1 --format='%h %s' "$local_sha" 2>/dev/null | cut -c1-90); refusing the push to dev." >&2
        status=1; continue
      fi
      [ -z "$sid" ] || [ -e "$stamps/tip-$sid" ] && continue
      echo "pre-push: $(git log -1 --format='%h %s' "$local_sha" | cut -c1-90) is not the tip of a stack a passing 'rv-gate fast' run tested (a prefix, or a stack changed after the run); run 'node tools/rv-gate/rv-gate.mjs fast' at it, then push (DEL-81)." >&2
      status=1 ;;
    refs/heads/candidate/*)
      [ "$RV_GATE_PUSH" = "candidate" ] && continue
      echo "pre-push: only the gate writes candidate branches (DEL-63)." >&2; status=1 ;;
    refs/heads/prod)
      [ "$RV_GATE_PUSH" = "prod" ] && continue
      echo "pre-push: prod moves only through 'rv-gate merge-prod' on Daniel's go (DEL-04)." >&2; status=1 ;;
    refs/heads/pre-prod)
      [ "$RV_GATE_PUSH" = "pre-prod" ] && continue
      echo "pre-push: pre-prod moves only through 'rv-gate release' (DEL-81)." >&2; status=1 ;;
    refs/heads/main)
      echo "pre-push: main was renamed prod (DEL-02); push a task branch instead." >&2; status=1 ;;
  esac
done
exit $status
`;

export function installHooks({ repoDir = process.cwd() } = {}) {
	const dir = join(gateHome(), "hooks");
	mkdirSync(dir, { recursive: true });
	const path = join(dir, "pre-push");
	writeFileSync(path, PRE_PUSH);
	chmodSync(path, 0o755);
	const common = sh("git", ["rev-parse", "--path-format=absolute", "--git-common-dir"], { cwd: repoDir });
	sh("git", ["--git-dir", common, "config", "core.hooksPath", dir]);
	console.log(`Installed ${path} as core.hooksPath for ${common}.`);
}
