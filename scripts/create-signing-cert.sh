#!/usr/bin/env bash
# Create a self-signed code signing certificate for Veil App releases.
#
# macOS ties the Accessibility grant to the app's code signature. Ad-hoc
# signed builds get a new signature on every build, so users have to grant
# Accessibility again after each update. Signing every release with this one
# certificate keeps the grant. (A paid "Developer ID Application" certificate
# works the same way and also avoids the Gatekeeper warning.)
#
# Run once, keep the output safe, and add the printed values as GitHub
# repository secrets. Re-running makes a NEW certificate, and users would
# have to grant Accessibility once more.
set -euo pipefail

NAME="${1:-Veil App Code Signing}"
OUT="${OUT_DIR:-$PWD/signing}"
# macOS `security import` cannot read PKCS#12 files written by OpenSSL 3's
# defaults; the system LibreSSL writes ones it can.
OPENSSL=/usr/bin/openssl

mkdir -p "$OUT"
chmod 700 "$OUT"
PASSWORD="$("$OPENSSL" rand -hex 16)"

"$OPENSSL" req -x509 -newkey rsa:2048 -nodes -days 3650 \
  -subj "/CN=$NAME" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" \
  -addext "basicConstraints=critical,CA:false" \
  -keyout "$OUT/key.pem" -out "$OUT/cert.pem" 2>/dev/null
"$OPENSSL" pkcs12 -export -inkey "$OUT/key.pem" -in "$OUT/cert.pem" \
  -name "$NAME" -passout "pass:$PASSWORD" -out "$OUT/cert.p12"
rm "$OUT/key.pem"
base64 < "$OUT/cert.p12" | tr -d '\n' > "$OUT/cert.p12.base64"
printf '%s' "$PASSWORD" > "$OUT/password.txt"

if [ "${IMPORT:-1}" = 1 ]; then
  # Lets local `bun run tauri build` sign with it too:
  #   APPLE_SIGNING_IDENTITY="$NAME" bun run tauri build
  security import "$OUT/cert.p12" -P "$PASSWORD" -T /usr/bin/codesign
fi

cat <<MSG

Created $OUT/cert.p12 (certificate "$NAME").

Add these GitHub repository secrets (Settings → Secrets and variables → Actions):
  MACOS_CERTIFICATE           contents of $OUT/cert.p12.base64
  MACOS_CERTIFICATE_PASSWORD  contents of $OUT/password.txt
  MACOS_SIGNING_IDENTITY      $NAME

e.g. with the GitHub CLI:
  gh secret set MACOS_CERTIFICATE < "$OUT/cert.p12.base64"
  gh secret set MACOS_CERTIFICATE_PASSWORD < "$OUT/password.txt"
  gh secret set MACOS_SIGNING_IDENTITY --body "$NAME"

Keep $OUT out of git and back it up somewhere safe.
MSG
