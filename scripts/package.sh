#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
version="${1:?version required}"
platform="${2:?platform required}"
out_dir="${3:-release-assets}"
mkdir -p "$out_dir"
base="jj-noumena-${version}-${platform}"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/$base"
cp dist/jj "$stage/$base/jj"
cat > "$stage/$base/manifest.json" <<MANIFEST
{"name":"jj","version":"$version","platform":"$platform","binary":"jj"}
MANIFEST
tar -C "$stage" -czf "$out_dir/$base.tar.gz" "$base"
shasum -a 256 "$out_dir/$base.tar.gz" > "$out_dir/$base.tar.gz.sha256"
cp "$stage/$base/manifest.json" "$out_dir/$base.manifest.json"
