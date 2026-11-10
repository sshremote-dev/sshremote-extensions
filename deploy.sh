#!/bin/sh
# Builds sshremote.dev (page, downloads, signed registry) and uploads it to the web server.
#   registry/deploy.sh [SSH-HOST] [WEB-ROOT]
# Downloads are taken from $WINDOWS_BIN and $LINUX_BIN (SSH Remote) and $TERMINAL_WINDOWS_BIN and
# $TERMINAL_LINUX_BIN (SSH Remote Terminal) when set.
# The native packs' programs come from $HELPERS (see registry/packs/web-gateway/make.py).
set -eu
HOST=${1:-ubuntu}
ROOT=${2:-/var/www/sshremote.dev}
KEY=${SIGNING_KEY:-$HOME/.ssh/sshremote_registry_ed25519}
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=$(mktemp -d)
trap 'rm -rf -- "$OUT"' EXIT

set -- "$OUT"
[ -n "${WINDOWS_BIN:-}" ] && set -- "$@" --download "sshremote-windows-x86_64.exe=$WINDOWS_BIN"
[ -n "${LINUX_BIN:-}" ] && set -- "$@" --download "sshremote-linux-x86_64=$LINUX_BIN"
[ -n "${TERMINAL_WINDOWS_BIN:-}" ] && set -- "$@" --download "sshremote-terminal-windows-x86_64.exe=$TERMINAL_WINDOWS_BIN"
[ -n "${TERMINAL_LINUX_BIN:-}" ] && set -- "$@" --download "sshremote-terminal-linux-x86_64=$TERMINAL_LINUX_BIN"
python "$HERE/build.py" "$@"

INDEX="$OUT/registry/v1/index.json"
ssh-keygen -q -Y sign -f "$KEY" -n sshremote-registry "$INDEX"
# The check the program makes, before anything goes up.
printf 'registry@sshremote.dev namespaces="sshremote-registry" %s\n' "$(cut -d' ' -f1,2 "$KEY.pub")" > "$OUT/.signers"
ssh-keygen -Y verify -f "$OUT/.signers" -I registry@sshremote.dev -n sshremote-registry -s "$INDEX.sig" < "$INDEX"
rm -f "$OUT/.signers"

# Packages and downloads first, the signed index last, so the index never names a missing file.
tar -C "$OUT" -c --exclude registry/v1/index.json --exclude registry/v1/index.json.sig . | ssh "$HOST" "tar -x -m -C '$ROOT'"
tar -C "$OUT" -c registry/v1/index.json registry/v1/index.json.sig | ssh "$HOST" "tar -x -m -C '$ROOT'"
echo "deployed to $HOST:$ROOT"
