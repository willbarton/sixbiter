#!/usr/bin/env bash
# Generate AppIcon.icns from a single PNG (ideally 1024x1024).
# Outputs to resources/AppIcon.icns.

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 path/to/icon.png" >&2
  exit 1
fi

SRC="$1"
if [[ ! -f "${SRC}" ]]; then
  echo "error: source PNG not found: ${SRC}" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${ROOT_DIR}/resources"
ICONSET_DIR="$(mktemp -d -t sixbiter-iconset)"
trap "rm -rf '${ICONSET_DIR}'" EXIT

mkdir -p "${ICONSET_DIR}.iconset"

# Generate all required sizes from the source PNG.
# Names match Apple's expected iconset format.
sizes=(
  "16:16x16"
  "32:16x16@2x"
  "32:32x32"
  "64:32x32@2x"
  "128:128x128"
  "256:128x128@2x"
  "256:256x256"
  "512:256x256@2x"
  "512:512x512"
  "1024:512x512@2x"
)

for entry in "${sizes[@]}"; do
  size="${entry%%:*}"
  name="${entry##*:}"
  sips -z "${size}" "${size}" "${SRC}" --out "${ICONSET_DIR}.iconset/icon_${name}.png" >/dev/null
done

iconutil -c icns "${ICONSET_DIR}.iconset" -o "${OUT_DIR}/AppIcon.icns"

echo "Generated: ${OUT_DIR}/AppIcon.icns"
