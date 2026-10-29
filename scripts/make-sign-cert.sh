#!/usr/bin/env bash
# Make the self-signed certificate the macOS app is signed with, once:
#   scripts/make-sign-cert.sh <out-dir>
# It writes Mtrx-sign.p12 and its password to <out-dir>. Keep both safe (a
# password manager): a new certificate makes every Mac ask for the keychain
# password once more. Give them to the release workflow with
#   base64 -i <out-dir>/Mtrx-sign.p12 | gh secret set MTRX_MACOS_CERT
#   gh secret set MTRX_MACOS_CERT_PASSWORD < <out-dir>/password
set -euo pipefail
out="${1:?usage: $0 <out-dir>}"
mkdir -p "$out"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cat > "$work/cert.cnf" <<'CNF'
[req]
distinguished_name = dn
prompt = no
x509_extensions = ext
[dn]
CN = Mtrx
[ext]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
CNF
openssl req -x509 -newkey rsa:3072 -nodes -days 7300 \
  -keyout "$work/key.pem" -out "$work/cert.pem" -config "$work/cert.cnf"
password="$(openssl rand -base64 24)"
# The legacy algorithms macOS's `security import` reads.
openssl pkcs12 -export -legacy -name Mtrx -inkey "$work/key.pem" -in "$work/cert.pem" \
  -out "$out/Mtrx-sign.p12" -passout "pass:$password"
printf '%s' "$password" > "$out/password"
chmod 600 "$out/Mtrx-sign.p12" "$out/password"
echo "Wrote $out/Mtrx-sign.p12 and $out/password"
