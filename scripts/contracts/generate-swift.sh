#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/../bootstrap/env.sh"

cd "$NONPROXY_ROOT/platform/macos"
swift package \
  --allow-writing-to-package-directory \
  generate-grpc-code-from-protos \
  --output-path Sources/NonProxyProviderContracts/Generated \
  --access-level public \
  --no-servers \
  -- ../../proto
