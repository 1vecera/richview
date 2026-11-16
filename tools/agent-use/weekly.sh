#!/bin/zsh
# The weekly fresh-agent run on the pre-prod head (DEL-78, DEL-81).
#
# Fetches origin, checks the pre-prod head out in a dedicated worktree under the gate's home, and
# runs tools/agent-use/gate.mjs from there against the release binary the latest release receipt
# built. The result and the packet appendix go to the evidence store:
#   <gate home>/evidence/agent-use/weekly/<date>-<short sha>/{fresh-agent.json,appendix.md,...}
# It never builds. With no release receipt that built a binary it exits 3 and says why.
#
# Environment: RV_REPO (the main checkout, default ~/code/richview-v3), RV_GATE_HOME (default
# ~/.local/share/richview-gate), RV_RELEASE_BIN_DIR (default ~/tmp/richview-rewrite/bin-release,
# one <short sha>/richview per release build), RV_RELEASE_TARGET (the Cargo target release runs
# build in, default the shared ~/tmp/richview-rewrite/target). Extra arguments go to gate.mjs.

set -eu
export PATH="$HOME/.local/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

REPO=${RV_REPO:-$HOME/code/richview-v3}
GATE_HOME=${RV_GATE_HOME:-$HOME/.local/share/richview-gate}
BIN_DIR=${RV_RELEASE_BIN_DIR:-$HOME/tmp/richview-rewrite/bin-release}
RELEASE_TARGET=${RV_RELEASE_TARGET:-$HOME/tmp/richview-rewrite/target}
EVIDENCE=$GATE_HOME/evidence
WORKTREE=$GATE_HOME/agent-use-weekly

say() { print -u2 -- "agent-use weekly: $*"; }

git -C "$REPO" fetch --quiet origin pre-prod
HEAD_SHA=$(git -C "$REPO" rev-parse origin/pre-prod)
SHORT=${HEAD_SHA[1,10]}

# The latest release receipt whose build step passed names the commit the binary was built from.
RECEIPT_COMMIT=$(node -e '
const fs = require("fs"), path = require("path");
const dir = path.join(process.argv[1], "receipts");
const ids = fs.existsSync(dir) ? fs.readdirSync(dir).filter((f) => /^release-\d{8}T\d{6}Z-[0-9a-f]+\.json$/.test(f)).sort().reverse() : [];
for (const f of ids) {
	const r = JSON.parse(fs.readFileSync(path.join(dir, f), "utf8"));
	const build = (r.steps || []).find((s) => s.id === "build");
	if (r.kind === "release" && build && (build.status === "pass" || build.status === "reused")) { console.log(r.commit); break; }
}' "$EVIDENCE")
if [ -z "$RECEIPT_COMMIT" ]; then
	say "no release receipt in $EVIDENCE/receipts has a passing build step, so there is no release binary to test. Run a release first."
	exit 3
fi

RECEIPT_SHORT=${RECEIPT_COMMIT[1,10]}
BINARY=$BIN_DIR/$RECEIPT_SHORT/richview
if [ ! -x "$BINARY" ]; then
	# The target keeps only its latest release build, from any checkout; accept it when it is the receipt's.
	CANDIDATE=$RELEASE_TARGET/release/richview
	if [ -x "$CANDIDATE" ] && "$CANDIDATE" --version 2>/dev/null | grep -q "commit ${RECEIPT_COMMIT[1,7]}"; then
		BINARY=$CANDIDATE
	else
		say "the latest release receipt built $RECEIPT_SHORT but neither $BIN_DIR/$RECEIPT_SHORT/richview nor $CANDIDATE is that build."
		exit 3
	fi
fi
[ "$RECEIPT_COMMIT" = "$HEAD_SHA" ] || say "note: the binary is from release $RECEIPT_SHORT, the pre-prod head is $SHORT"

# A dedicated worktree, detached at the pre-prod head, kept clean between runs.
if [ -e "$WORKTREE/.git" ]; then
	git -C "$WORKTREE" checkout --quiet --detach -f "$HEAD_SHA"
	git -C "$WORKTREE" clean -qfd
else
	mkdir -p "$GATE_HOME"
	git -C "$REPO" worktree add --quiet --detach "$WORKTREE" "$HEAD_SHA"
fi

OUT=$EVIDENCE/agent-use/weekly/$(date -u +%Y%m%d)-$SHORT
mkdir -p "$OUT"
say "pre-prod $SHORT, binary $BINARY, writing $OUT"
cd "$WORKTREE"
STATUS=0
node tools/agent-use/gate.mjs --richview "$BINARY" --out "$OUT" --products claude --unrun "codex:Daniel, 1 Nov 2026: Codex is no longer used and its quota is 0" "$@" || STATUS=$?
say "finished with exit $STATUS; appendix at $OUT/appendix.md"
exit $STATUS
