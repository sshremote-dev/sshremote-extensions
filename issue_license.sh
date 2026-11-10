#!/bin/sh
# Issues a premium license: registry/issue_license.sh EMAIL MONTHS [PLAN] > license.txt
# Signed with the registry's key; the program checks it offline.
set -eu
EMAIL=$1
MONTHS=${2:-12}
PLAN=${3:-premium}
KEY=${SIGNING_KEY:-$HOME/.ssh/sshremote_registry_ed25519}
case $EMAIL in *[\"\]*) echo "bad email" >&2; exit 1 ;; esac
EXPIRES=$(python -c "import datetime,sys;d=datetime.date.today();m=d.month-1+int(sys.argv[1]);print(datetime.date(d.year+m//12,m%12+1,min(d.day,28)).isoformat())" "$MONTHS")
ID="L-$(date -u +%Y%m%d)-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')"
WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT
printf '{"id":"%s","email":"%s","plan":"%s","expires":"%s"}' "$ID" "$EMAIL" "$PLAN" "$EXPIRES" > "$WORK/record"
ssh-keygen -q -Y sign -f "$KEY" -n sshremote-license "$WORK/record" < /dev/null
cat "$WORK/record"
printf '\n-----SIGNATURE-----\n'
cat "$WORK/record.sig"
