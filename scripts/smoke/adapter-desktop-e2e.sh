#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/../bootstrap/env.sh"

state_directory="$(mktemp -d)"
adapter_pid=""

cleanup() {
  if [[ -n "$adapter_pid" ]] && kill -0 "$adapter_pid" 2>/dev/null; then
    kill "$adapter_pid" 2>/dev/null || true
    wait "$adapter_pid" 2>/dev/null || true
  fi
  rm -rf "$state_directory"
}
trap cleanup EXIT

cargo build --quiet -p nonproxy-adapter-host
NONPROXY_ADAPTER_STATE_DIR="$state_directory" \
NONPROXY_ADAPTER_BUNDLE_FINGERPRINT="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" \
  target/debug/nonproxy-adapter-host >"$state_directory/adapter.log" 2>&1 &
adapter_pid="$!"

for _attempt in {1..100}; do
  if [[ -S "$state_directory/adapter-host.sock" && \
        -f "$state_directory/adapter.capability" ]]; then
    break
  fi
  if ! kill -0 "$adapter_pid" 2>/dev/null; then
    cat "$state_directory/adapter.log" >&2
    exit 1
  fi
  sleep 0.05
done

if [[ ! -S "$state_directory/adapter-host.sock" || \
      ! -f "$state_directory/adapter.capability" ]]; then
  cat "$state_directory/adapter.log" >&2
  printf '适配器套接字或能力文件未在限定时间内创建。\n' >&2
  exit 1
fi

dotnet run \
  --project tests/adapter-desktop-smoke/NonProxy.AdapterDesktopSmoke.csproj \
  --configuration Debug \
  --no-build \
  --no-restore \
  -- "$state_directory"
