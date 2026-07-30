#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/../bootstrap/env.sh"

buf format --diff --exit-code
buf lint
buf build

generated_snapshot="$(mktemp -d)"
trap 'rm -rf "$generated_snapshot"' EXIT
cp -R generated/csharp/. "$generated_snapshot/"
buf generate

if ! diff -ru "$generated_snapshot" generated/csharp; then
  printf '生成的 C# 契约不是最新状态，请运行 buf generate 并提交结果。\n' >&2
  exit 1
fi
