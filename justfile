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

contracts-breaking:
  source "{{env}}" && buf breaking --against '.git#ref=HEAD'

restore-desktop:
  source "{{env}}" && dotnet restore apps/desktop/NonProxy.Desktop.slnx --locked-mode

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

build-desktop:
  source "{{env}}" && dotnet build apps/desktop/NonProxy.Desktop.slnx --no-restore --no-incremental --configuration Debug

verify-macos-bundle: build-desktop
  source "{{env}}" && native_rid="$(dotnet msbuild apps/desktop/NonProxy.Desktop.Mac/NonProxy.Desktop.Mac.csproj -getProperty:NETCoreSdkRuntimeIdentifier)" && codesign --verify --deep --strict --verbose=4 "apps/desktop/NonProxy.Desktop.Mac/bin/Debug/net10.0-macos/${native_rid}/NonProxy.app"

check: check-tools contracts restore-desktop restore-node format-check lint verify-macos-bundle test

status:
  source "{{env}}" && cargo run --quiet -p xtask -- status
