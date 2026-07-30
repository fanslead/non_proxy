#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/../bootstrap/env.sh"

state_directory="$(mktemp -d)"
gateway_pid=""

cleanup() {
  if [[ -n "$gateway_pid" ]] && kill -0 "$gateway_pid" 2>/dev/null; then
    kill "$gateway_pid" 2>/dev/null || true
    wait "$gateway_pid" 2>/dev/null || true
  fi
  rm -rf "$state_directory"
}
trap cleanup EXIT

cargo build --quiet -p nonproxy-gatewayd
NONPROXY_STATE_DIR="$state_directory" \
  target/debug/nonproxy-gatewayd >"$state_directory/gateway.log" 2>&1 &
gateway_pid="$!"

for _attempt in {1..100}; do
  if [[ -S "$state_directory/gatewayd.sock" ]]; then
    break
  fi
  if ! kill -0 "$gateway_pid" 2>/dev/null; then
    cat "$state_directory/gateway.log" >&2
    exit 1
  fi
  sleep 0.05
done

if [[ ! -S "$state_directory/gatewayd.sock" ]]; then
  cat "$state_directory/gateway.log" >&2
  printf '控制套接字未在限定时间内创建。\n' >&2
  exit 1
fi

NONPROXY_STATE_DIR="$state_directory" \
  dotnet run \
    --project tests/control-smoke/NonProxy.ControlSmoke.csproj \
    --configuration Debug \
    --no-build \
    --no-restore
