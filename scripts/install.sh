#!/bin/sh
# INT-71: installs a checksum-verified native build; no services or shell configuration are changed.
main() {
set -eu
umask 077
# Replaced only by scripts/assemble-release.mjs after both archives are checked.
release_commit='UNSTAMPED'
release_base_url='UNSTAMPED'
release_macos_sha256='UNSTAMPED'
release_linux_sha256='UNSTAMPED'
LC_ALL=C
export LC_ALL
fail() { printf '%s\n' "richview install: $*" >&2; exit 1; }
quote() { printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")"; }
prefix_hint() {
  printf '%s\n' "For an empty leftover, run: rmdir $(quote "$prefix")" "For a partial installation, save any personal files and move the directory aside before reinstalling."
}
usage() {
  cat <<'EOF'
Usage: sh install.sh [--prefix DIR] [--base-url URL] [--commit FULL_COMMIT]
       sh install.sh --uninstall [--prefix DIR]

Installs the CLI (including MCP and the Cowork server) and its SQL worker.
DIR defaults to $HOME/.richview and must not already exist; its parent must exist.
The default downloads from the release stamped into this installer.
URL serves release assets directly;
--commit optionally confirms the installer's approved build identity.
Use HTTPS, or HTTP on loopback for local verification.
Archives stop after 30 seconds below 1 KiB/s.
Default: the GitHub release stamped into this installer
Uninstall refuses changed files or additional content in its owned directory.
EOF
}
prefix=${HOME:?HOME must be set}/.richview
base=$release_base_url commit='' uninstall=false work='' installing=false
while [ "$#" -gt 0 ]; do
  case $1 in
    --prefix|--base-url|--commit)
      [ "$#" -ge 2 ] || fail "missing value for $1"
      case $1 in --prefix) prefix=$2;; --base-url) base=$2;; --commit) commit=$2;; esac
      shift 2;;
    --uninstall) uninstall=true; shift;;
    --help|-h) usage; exit 0;;
    *) fail "unknown option: $1";;
  esac
done
case $prefix in /*) ;; *) fail 'prefix must be absolute';; esac
case $prefix in */|*/.|*/..) fail 'prefix must name a dedicated directory';; esac
[ ! -L "$prefix" ] || fail 'a symlink prefix is not an owned installation'
parent=$(dirname "$prefix")
[ -d "$parent" ] || fail 'the prefix parent directory must exist'
parent=$(CDPATH='' cd -- "$parent" && pwd -P)
prefix=$parent/$(basename "$prefix")
command -v tar >/dev/null 2>&1 || fail 'tar is required'
if command -v sha256sum >/dev/null 2>&1; then
  digest() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  digest() { shasum -a 256 "$1" | awk '{print $1}'; }
else fail 'sha256sum or shasum is required'; fi
cleanup() {
  if "$installing"; then rm -rf "$prefix"; fi
  [ -z "$work" ] || rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 1' HUP INT TERM
work=$(mktemp -d "$parent/.richview-install.XXXXXXXX" 2>/dev/null) || fail "cannot create installation scratch in $parent; choose a writable --prefix under HOME"
safe_path() {
  case $1 in ''|/*|*[!a-zA-Z0-9_./-]*) return 1;; esac
  case /$1/ in */../*|*/./*) return 1;; esac
}
verify_payload() {
  dir=$1
  [ -f "$dir/SHA256SUMS" ] || fail 'archive has no file checksums'
  : > "$work/expected-files"
  while read -r expected file extra; do
    [ -z "$extra" ] && [ "${#expected}" -eq 64 ] || fail 'invalid file checksum record'
    case $expected in *[!0-9a-f]*) fail 'invalid file checksum';; esac
    safe_path "$file" || fail 'unsafe checksum path'
    [ "$file" != SHA256SUMS ] || fail 'self-referencing checksum'
    [ -f "$dir/$file" ] && [ ! -L "$dir/$file" ] || fail "missing regular file: $file"
    if [ "$(digest "$dir/$file")" != "$expected" ]; then
      if "$uninstall"; then fail "file checksum mismatch: $file. Installation kept at $prefix; restore this file before retrying. $(prefix_hint)"; fi
      fail "file checksum mismatch: $file"
    fi
    printf '%s\n' "$file" >> "$work/expected-files"
  done < "$dir/SHA256SUMS"
  (cd "$dir" && find . -type f ! -path './SHA256SUMS' ! -path './.richview-install' ! -path './.richview-files' | sed 's|^./||' | LC_ALL=C sort) > "$work/actual-files"
  LC_ALL=C sort "$work/expected-files" > "$work/sorted-files"
  cmp -s "$work/sorted-files" "$work/actual-files" || fail 'unlisted or duplicate archive files'
}
inventory() {
  (cd "$1" && find . ! -path './.richview-install' ! -path './.richview-files' -print | LC_ALL=C sort)
}
if "$uninstall"; then
  [ -d "$prefix" ] && [ -f "$prefix/.richview-install" ] && [ ! -L "$prefix/.richview-install" ] || fail "prefix is not an owned installation: $prefix. $(prefix_hint)"
  for file in SHA256SUMS .richview-files; do
    [ -f "$prefix/$file" ] && [ ! -L "$prefix/$file" ] || fail 'installation metadata changed'
  done
  [ "$(cat "$prefix/.richview-install")" = "richview-install-v1 $(digest "$prefix/SHA256SUMS") $(digest "$prefix/.richview-files")" ] || fail 'installation ownership record changed'
  inventory "$prefix" > "$work/current-files"
  if ! cmp -s "$work/current-files" "$prefix/.richview-files"; then
    comm -13 "$prefix/.richview-files" "$work/current-files" | sed 's/^/  added: /' >&2
    comm -23 "$prefix/.richview-files" "$work/current-files" | sed 's/^/  missing: /' >&2
    fail "installation contains additional or missing content. Move or back up added paths and restore missing paths before retrying; $prefix was kept. $(prefix_hint)"
  fi
  [ -z "$(find "$prefix" ! -type d ! -type f -print)" ] || fail 'installation contains a link or special file'
  verify_payload "$prefix"
  rm -rf "$prefix"
  state_dir=${XDG_STATE_HOME:-$HOME/.local/state}/richview
  printf '%s\n' "Removed RichView from $prefix" \
    "Your reports and RichView state in $state_dir (SQL snapshots and upload journals) were kept." \
    "Remove any PATH entry you added for $prefix/bin."
  exit 0
fi
[ ! -e "$prefix" ] || fail "prefix already exists: $prefix. Use its uninstall.sh for a complete installation. $(prefix_hint)"
[ "${#release_commit}" -eq 40 ] || fail 'installer has not been assembled for a release'
case $release_commit in *[!0-9a-f]*) fail 'invalid approved release commit';; esac
[ -z "$commit" ] || [ "$commit" = "$release_commit" ] || fail 'requested commit differs from the approved release'
commit=$release_commit
case $base in
  https://*) authority=${base#https://}; authority=${authority%%/*}; case $authority in ''|*@*) fail 'invalid download authority';; esac;;
  http://*) authority=${base#http://}; authority=${authority%%/*}
    printf '%s\n' "$authority" | LC_ALL=C grep -Eq '^((127\.0\.0\.1|localhost)(:[0-9]+)?|\[::1\](:[0-9]+)?)$' || fail 'HTTP downloads must stay on loopback';;
  *) fail '--base-url requires HTTPS (or loopback HTTP)';;
esac
case $base in *\?*|*\#*) fail 'base URL cannot contain a query or fragment';; esac
case $(uname -s):$(uname -m) in
  Darwin:arm64) platform=aarch64-apple-darwin; expected=$release_macos_sha256;;
  Linux:x86_64)
    libc=$(getconf GNU_LIBC_VERSION 2>/dev/null) || fail 'Linux x64 requires glibc; this platform is not supported yet.'
    case $libc in 'glibc '*) ;; *) fail 'Linux x64 requires glibc; this platform is not supported yet.';; esac
    platform=x86_64-unknown-linux-gnu; expected=$release_linux_sha256;;
  *) fail 'This platform is not supported yet. RichView supports macOS Apple Silicon and glibc Linux x64.';;
esac
[ "${#expected}" -eq 64 ] || fail 'installer has no approved archive digest'
case $expected in *[!0-9a-f]*) fail 'invalid approved archive digest';; esac
command -v curl >/dev/null 2>&1 || fail 'curl is required'
archive=richview-$platform.tar.gz
download() {
  url=$1 destination=$2
  if [ "${base#http://}" != "$base" ]; then
    # Follow loopback redirects manually so no HTTP request can leave the original authority.
    redirects=0
    while :; do
      result=$(curl --disable --fail --silent --show-error --noproxy '*' --proto '=http' --connect-timeout 10 --speed-limit 1024 --speed-time 30 --write-out '%{http_code} %{redirect_url}' --output "$destination" "$url") \
        || fail "could not download archive on $platform from $url; installation was not started"
      status=${result%% *}; next=${result#* }
      case $status in
        2[0-9][0-9]) break;;
        3[0-9][0-9])
          [ "$redirects" -lt 5 ] || fail 'too many loopback redirects'
          case $next in http://*) next_authority=${next#http://}; next_authority=${next_authority%%/*};; *) fail 'loopback redirect must stay on the same host';; esac
          [ "$next_authority" = "$authority" ] || fail 'loopback redirect must stay on the same host'
          url=$next; redirects=$((redirects + 1));;
        *) fail "unexpected HTTP $status downloading archive";;
      esac
    done
  else
    status=$(curl --disable --fail --silent --show-error --location --max-redirs 5 --proto '=https' --proto-redir '=https' --connect-timeout 10 --speed-limit 1024 --speed-time 30 --write-out '%{http_code}' --output "$destination" "$url") \
      || fail "could not download archive on $platform from $url; installation was not started"
    case $status in 2[0-9][0-9]) ;; *) fail "unexpected HTTP $status downloading archive";; esac
  fi
}
download "${base%/}/$archive" "$work/archive"
[ "$(digest "$work/archive")" = "$expected" ] || fail 'archive checksum mismatch; nothing installed'
tar -tzf "$work/archive" > "$work/entries"
while IFS= read -r entry; do
  entry=${entry#./}; entry=${entry%/}
  [ -z "$entry" ] || safe_path "$entry" || fail 'unsafe archive path'
  case /$entry in */.richview-install|*/.richview-files) fail 'archive contains installer-owned metadata';; esac
done < "$work/entries"
tar -tvzf "$work/archive" > "$work/types"
if cut -c1 "$work/types" | LC_ALL=C grep -q '[^-d]'; then fail 'archive contains a link or special file'; fi
mkdir "$work/payload"
tar -xzf "$work/archive" --no-same-owner -C "$work/payload"
verify_payload "$work/payload"
printf 'commit=%s\nplatform=%s\n' "$commit" "$platform" > "$work/build"
cmp -s "$work/build" "$work/payload/BUILD" || fail 'archive commit/platform differs from the request'
for file in bin/richview bin/richview-sql-worker uninstall.sh; do
  [ -x "$work/payload/$file" ] || fail "archive executable missing: $file"
done
inventory "$work/payload" > "$work/payload/.richview-files"
printf 'richview-install-v1 %s %s\n' "$(digest "$work/payload/SHA256SUMS")" "$(digest "$work/payload/.richview-files")" > "$work/payload/.richview-install"
mkdir "$prefix" || fail 'prefix appeared during download; nothing installed'
installing=true
cp -R "$work/payload/." "$prefix"
installing=false
cli=$(quote "$prefix/bin/richview")
printf '%s\n' "Installed RichView build $commit in $prefix" \
  "Add $prefix/bin to PATH, or invoke $prefix/bin/richview directly." \
  "Start a report: $cli init my-report" \
  "Agent MCP command: $cli mcp" \
  "Optional Cowork server: $cli server --help" \
  'When you publish, choose RichView-hosted Cowork or your own server; no hosted service was started.' \
  "Uninstall: sh $(quote "$prefix/uninstall.sh") --uninstall --prefix $(quote "$prefix")"
}
main "$@"
