#!/usr/bin/env bash
set -euo pipefail

artifacts="${1:-artifacts}"
VERSION="${GITHUB_REF_NAME#v}"
BASE="https://github.com/${GITHUB_REPOSITORY}/releases/download/${GITHUB_REF_NAME}"
platforms='{}'

add_platform() {
  local key="$1" sig_file="$2"
  local art_base sig
  if jq -e --arg key "$key" 'has($key)' <<<"$platforms" >/dev/null; then
    echo "::error::Multiple updater artifacts for $key" >&2
    exit 1
  fi
  if [ ! -s "${sig_file%.sig}" ]; then
    echo "::error::Missing or empty updater artifact: ${sig_file%.sig}" >&2
    exit 1
  fi
  art_base="$(basename "${sig_file%.sig}")"
  sig="$(cat "$sig_file")"
  if [[ -z "${sig//[[:space:]]/}" ]]; then
    echo "::error::Empty updater signature: $sig_file" >&2
    exit 1
  fi
  platforms="$(jq -n --argjson p "$platforms" --arg k "$key" --arg s "$sig" --arg u "${BASE}/${art_base}" \
    '$p + {($k): {signature: $s, url: $u}}')"
}

while IFS= read -r f; do add_platform 'linux-x86_64' "$f"; done < <(find "$artifacts" -name '*.AppImage.sig')
while IFS= read -r f; do add_platform 'darwin-aarch64' "$f"; done < <(find "$artifacts" -name '*.app.tar.gz.sig')
while IFS= read -r f; do add_platform 'windows-x86_64' "$f"; done < <(find "$artifacts" -name '*-setup.exe.sig')

for platform in linux-x86_64 darwin-aarch64 windows-x86_64; do
  if ! jq -e --arg key "$platform" 'has($key)' <<<"$platforms" >/dev/null; then
    echo "::error::Missing signed updater artifact for $platform" >&2
    exit 1
  fi
done

jq -n --arg v "$VERSION" --arg d "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson p "$platforms" \
  '{version: $v, pub_date: $d, platforms: $p}' > "$artifacts/latest.json"
cat "$artifacts/latest.json"
