#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/../bootstrap/env.sh"

state_directory="$(mktemp -d)"
gateway_pid=""

cleanup() {
  exit_code="$?"
  if [[ "$exit_code" -ne 0 ]] && [[ -f "$state_directory/gateway.log" ]]; then
    cat "$state_directory/gateway.log" >&2
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
NONPROXY_STATE_DIR="$state_directory" \
  target/debug/nonproxy-gatewayd >"$state_directory/gateway.log" 2>&1 &
gateway_pid="$!"

for _attempt in {1..100}; do
  if [[ -S "$state_directory/gatewayd.sock" ]] &&
    [[ -f "$state_directory/session.capability" ]] &&
    [[ -f "$state_directory/provider.capability" ]]; then
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
  printf 'Provider 联调套接字未在限定时间内创建。\n' >&2
  exit 1
fi

NONPROXY_STATE_DIR="$state_directory" \
  dotnet run \
    --project tests/control-smoke/NonProxy.ControlSmoke.csproj \
    --configuration Debug \
    --no-build \
    --no-restore

swift run \
  --package-path platform/macos \
  NonProxyProviderSmoke \
  "$state_directory/gatewayd.sock" \
  "$state_directory/provider.capability" \
  "$state_directory/provider-cache" \
  transparent-proxy \
  pending

swift run \
  --package-path platform/macos \
  NonProxyProviderSmoke \
  "$state_directory/gatewayd.sock" \
  "$state_directory/provider.capability" \
  "$state_directory/provider-cache" \
  dns-proxy \
  active

swift run \
  --package-path platform/macos \
  NonProxyProviderSmoke \
  "$state_directory/gatewayd.sock" \
  "$state_directory/provider.capability" \
  "$state_directory/provider-cache" \
  transparent-proxy \
  active
