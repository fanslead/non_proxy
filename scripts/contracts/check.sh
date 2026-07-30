#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/../bootstrap/env.sh"

buf format --diff --exit-code
buf lint
buf build
buf generate

if ! git diff --exit-code -- generated/csharp; then
  printf '生成的 C# 契约不是最新状态，请运行 buf generate 并提交结果。\n' >&2
  exit 1
fi
