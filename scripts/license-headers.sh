#!/usr/bin/env bash
# Copyright 2026 Thomas Santerre and Moderately AI Inc.
#
# SPDX-License-Identifier: MIT OR Apache-2.0

# Apply or verify SPDX dual-license headers via google/addlicense.
#
# Usage:
#   scripts/license-headers.sh          # apply headers
#   scripts/license-headers.sh check    # verify only (CI)
#
# Requires: addlicense on PATH
#   go install github.com/google/addlicense@v1.2.0

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! command -v addlicense >/dev/null 2>&1; then
  echo "error: addlicense not found on PATH" >&2
  echo "install with: go install github.com/google/addlicense@v1.2.0" >&2
  exit 1
fi

# Shared ignores: vendored / generated / non-source fixtures.
# (addlicense only stamps known source extensions; ignores are defensive.)
# Stamp Rust sources only. addlicense knows many languages (yml/sh/go/…);
# we deliberately keep the surface to .rs + build.rs under the tree.
common_args=(
  -f license-header.txt
  # Generated / dependency trees
  -ignore 'target/**'
  # Python binding: virtualenvs, build output, and tool caches. site-packages
  # carries third-party C sources that addlicense would otherwise try to stamp.
  -ignore '**/.venv/**'
  -ignore '**/dist/**'
  -ignore '**/__pycache__/**'
  -ignore '**/.pytest_cache/**'
  -ignore '**/.mypy_cache/**'
  -ignore '**/.ruff_cache/**'
  # Node binding: installed packages and build output.
  -ignore '**/node_modules/**'
  # Test fixtures (not project source)
  -ignore 'crates/interpretthis/tests/integration/parity_corpus/**'
  -ignore 'crates/interpretthis/tests/integration/cpython_vendored/**'
  # Non-Rust project files addlicense would otherwise touch
  -ignore '**/*.json'
  -ignore '**/*.md'
  -ignore '**/*.toml'
  -ignore '**/*.lock'
  -ignore '**/*.py'
  -ignore '**/*.yml'
  -ignore '**/*.yaml'
  -ignore '**/*.sh'
  -ignore '**/*.txt'
  -ignore '.github/**'
  -ignore 'scripts/**'
  -ignore 'tickets/**'
  -ignore '.ticketsplease/**'
  -ignore '.git/**'
)

mode="${1:-apply}"
if [[ "$mode" == "check" ]]; then
  addlicense -check "${common_args[@]}" .
elif [[ "$mode" == "apply" ]]; then
  addlicense "${common_args[@]}" .
else
  echo "usage: $0 [apply|check]" >&2
  exit 2
fi
