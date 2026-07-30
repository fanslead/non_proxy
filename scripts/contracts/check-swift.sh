#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/../bootstrap/env.sh"

generated_dir="$NONPROXY_ROOT/platform/macos/Sources/NonProxyProviderContracts/Generated"
generated_snapshot="$(mktemp -d)"
trap 'rm -rf "$generated_snapshot"' EXIT
cp -R "$generated_dir/." "$generated_snapshot/"

"$script_dir/generate-swift.sh"

if ! diff -ru "$generated_snapshot" "$generated_dir"; then
  printf '生成的 Swift 契约不是最新状态，请运行 scripts/contracts/generate-swift.sh 并提交结果。\n' >&2
  exit 1
fi
