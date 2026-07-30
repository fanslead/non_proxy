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

format:
  source "{{env}}" && cargo fmt --all

format-check:
  source "{{env}}" && cargo fmt --all --check

lint:
  source "{{env}}" && cargo clippy --workspace --all-targets -- -D warnings

test:
  source "{{env}}" && cargo test --workspace

check: check-tools contracts format-check lint test

status:
  source "{{env}}" && cargo run --quiet -p xtask -- status
