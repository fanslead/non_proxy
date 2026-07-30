set shell := ["bash", "-euo", "pipefail", "-c"]

root := justfile_directory()
env := root / "scripts/bootstrap/env.sh"

default: check

bootstrap:
  ./scripts/bootstrap/install-local-tools.sh

check-tools:
  source "{{env}}" && ./scripts/bootstrap/check-prerequisites.sh

format:
  source "{{env}}" && cargo fmt --all

format-check:
  source "{{env}}" && cargo fmt --all --check

lint:
  source "{{env}}" && cargo clippy --workspace --all-targets -- -D warnings

test:
  source "{{env}}" && cargo test --workspace

check: check-tools format-check lint test

status:
  source "{{env}}" && cargo run --quiet -p xtask -- status
