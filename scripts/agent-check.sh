#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ "${DROIDSCOPE_CHECK_FMT:-0}" == "1" ]]; then
  echo "==> cargo fmt --all -- --check"
  cargo fmt --all -- --check
fi

echo "==> cargo test --all-targets"
cargo test --all-targets

if [[ "${DROIDSCOPE_CHECK_CLIPPY:-0}" == "1" ]]; then
  echo "==> cargo clippy --all-targets -- -D warnings"
  cargo clippy --all-targets -- -D warnings
fi

if [[ "${DROIDSCOPE_CHECK_SIDECAR:-0}" == "1" ]]; then
  if ! command -v gradle >/dev/null 2>&1; then
    echo "gradle not found; cannot build sidecar" >&2
    exit 127
  fi

  echo "==> gradle -p sidecar/gradle-agent jar"
  gradle -p sidecar/gradle-agent jar
fi
