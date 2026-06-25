#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
cargo build --release --bin jj
mkdir -p dist
cp target/release/jj dist/jj
