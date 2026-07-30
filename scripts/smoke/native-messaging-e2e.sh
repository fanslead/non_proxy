#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(CDPATH= cd -- "$script_dir/../.." && pwd)"
source "$script_dir/../bootstrap/env.sh"

state_directory="$(mktemp -d)"
gateway_pid=""

cleanup() {
  exit_code="$?"
  if [[ "$exit_code" -ne 0 ]] && [[ -f "$state_directory/gateway.log" ]]; then
    sed -n '1,240p' "$state_directory/gateway.log" >&2
  fi
  if [[ -n "$gateway_pid" ]] && kill -0 "$gateway_pid" 2>/dev/null; then
    kill "$gateway_pid" 2>/dev/null || true
    wait "$gateway_pid" 2>/dev/null || true
  fi
  rm -rf "$state_directory"
  exit "$exit_code"
}
trap cleanup EXIT

cargo build --quiet -p nonproxy-gatewayd
swift build \
  --package-path "$repo_root/platform/macos" \
  --disable-sandbox \
  --product NonProxyNativeMessagingHost
host_bin_path="$(
  swift build \
    --package-path "$repo_root/platform/macos" \
    --disable-sandbox \
    --show-bin-path
)"

NONPROXY_STATE_DIR="$state_directory" \
  "$repo_root/target/debug/nonproxy-gatewayd" \
  >"$state_directory/gateway.log" 2>&1 &
gateway_pid="$!"

for _attempt in {1..100}; do
  if [[ -S "$state_directory/gatewayd.sock" ]] &&
    [[ -f "$state_directory/session.capability" ]]; then
    break
  fi
  if ! kill -0 "$gateway_pid" 2>/dev/null; then
    sed -n '1,240p' "$state_directory/gateway.log" >&2
    exit 1
  fi
  sleep 0.05
done

if [[ ! -S "$state_directory/gatewayd.sock" ]] ||
  [[ ! -f "$state_directory/session.capability" ]]; then
  printf 'Native Messaging 联调等待 gatewayd 超时。\n' >&2
  exit 1
fi

NONPROXY_STATE_DIR="$state_directory" \
  node "$repo_root/tests/native-messaging-smoke/client.mjs" \
    "$host_bin_path/NonProxyNativeMessagingHost"
