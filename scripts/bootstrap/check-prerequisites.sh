#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/env.sh"
source "$NONPROXY_ROOT/tools/versions.env"

failures=0

check_exact() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  if [[ "$actual" == "$expected" ]]; then
    printf 'ok   %-10s %s\n' "$label" "$actual"
  else
    printf 'fail %-10s expected %s, found %s\n' "$label" "$expected" "${actual:-missing}" >&2
    failures=$((failures + 1))
  fi
}

command_version() {
  local command_name="$1"
  shift

  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf ''
    return
  fi

  "$@" 2>/dev/null
}

dotnet_actual="$(command_version dotnet dotnet --version)"
rust_actual="$(command_version rustc rustc --version | awk '{print $2}')"
buf_actual="$(command_version buf buf --version)"
protoc_actual="$(command_version protoc protoc --version | awk '{print $2}')"
just_actual="$(command_version just just --version | awk '{print $2}')"
node_actual="$(command_version node node --version | sed 's/^v//')"
pnpm_actual="$(command_version pnpm pnpm --version)"

check_exact dotnet "$DOTNET_VERSION" "$dotnet_actual"
check_exact rust "$RUST_VERSION" "$rust_actual"
check_exact buf "$BUF_VERSION" "$buf_actual"
check_exact protoc "$PROTOC_VERSION" "$protoc_actual"
check_exact just "$JUST_VERSION" "$just_actual"
check_exact node "$NODE_VERSION" "$node_actual"
check_exact pnpm "$PNPM_VERSION" "$pnpm_actual"

if [[ "$failures" -ne 0 ]]; then
  printf '\nRun ./scripts/bootstrap/install-local-tools.sh for repository-local tools.\n' >&2
  exit 1
fi
