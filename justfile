set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()
env := root / "scripts/bootstrap/env.sh"

default: check

bootstrap:
  ./scripts/bootstrap/install-local-tools.sh

check-tools:
  source "{{env}}" && ./scripts/bootstrap/check-prerequisites.sh

generate:
  source "{{env}}" && buf generate

contracts:
  source "{{env}}" && ./scripts/contracts/check.sh

contracts-swift:
  source "{{env}}" && ./scripts/contracts/check-swift.sh

contracts-breaking:
  source "{{env}}" && buf breaking --against '.git#ref=HEAD'

restore-desktop:
  # macOS Release 是 Universal，锁文件以 Release 的双 RID 依赖图为基线。
  source "{{env}}" && dotnet restore apps/desktop/NonProxy.Desktop.slnx --locked-mode -p:Configuration=Release

restore-node:
  source "{{env}}" && pnpm install --frozen-lockfile

format:
  source "{{env}}" && cargo fmt --all
  source "{{env}}" && dotnet format apps/desktop/NonProxy.Desktop.slnx --no-restore
  source "{{env}}" && pnpm run format

format-check:
  source "{{env}}" && cargo fmt --all --check
  source "{{env}}" && dotnet format apps/desktop/NonProxy.Desktop.slnx --no-restore --verify-no-changes
  source "{{env}}" && pnpm run format:check

lint:
  source "{{env}}" && cargo clippy --workspace --all-targets -- -D warnings
  source "{{env}}" && pnpm run lint

test:
  source "{{env}}" && cargo test --workspace
  source "{{env}}" && dotnet test apps/desktop/NonProxy.Desktop.slnx --no-restore --configuration Debug
  source "{{env}}" && pnpm run test
  source "{{env}}" && pnpm run typecheck

control-e2e:
  source "{{env}}" && ./scripts/smoke/control-plane-e2e.sh

provider-e2e:
  source "{{env}}" && ./scripts/smoke/provider-cross-language-e2e.sh

build-desktop:
  source "{{env}}" && dotnet build apps/desktop/NonProxy.Desktop.slnx --no-restore --no-incremental --configuration Debug

test-macos:
  source "{{env}}" && swift test --package-path platform/macos --disable-sandbox

verify-macos-bundle: build-desktop
  source "{{env}}" && native_rid="$(dotnet msbuild apps/desktop/NonProxy.Desktop.Mac/NonProxy.Desktop.Mac.csproj -getProperty:NETCoreSdkRuntimeIdentifier)" && ./scripts/macos/verify-system-extension-bundle.sh "apps/desktop/NonProxy.Desktop.Mac/bin/Debug/net10.0-macos/${native_rid}/NonProxy.app"

native-bridge-smoke: verify-macos-bundle
  source "{{env}}" && native_rid="$(dotnet msbuild apps/desktop/NonProxy.Desktop.Mac/NonProxy.Desktop.Mac.csproj -getProperty:NETCoreSdkRuntimeIdentifier)" && ./scripts/macos/native-bridge-smoke.sh "apps/desktop/NonProxy.Desktop.Mac/bin/Debug/net10.0-macos/${native_rid}/NonProxy.app"

check: check-tools contracts contracts-swift restore-desktop restore-node format-check lint native-bridge-smoke test test-macos control-e2e provider-e2e

status:
  source "{{env}}" && cargo run --quiet -p xtask -- status
