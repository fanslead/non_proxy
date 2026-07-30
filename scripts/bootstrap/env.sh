#!/usr/bin/env bash

if nonproxy_git_root="$(git rev-parse --show-toplevel 2>/dev/null)" &&
  [[ -f "$nonproxy_git_root/tools/versions.env" ]]; then
  export NONPROXY_ROOT="$nonproxy_git_root"
elif [[ -n "${BASH_SOURCE[0]:-}" ]]; then
  nonproxy_env_source="${BASH_SOURCE[0]}"
  nonproxy_script_dir="$(CDPATH= cd -- "$(dirname -- "$nonproxy_env_source")" && pwd)"
  export NONPROXY_ROOT="$(CDPATH= cd -- "$nonproxy_script_dir/../.." && pwd)"
elif [[ -n "${ZSH_VERSION:-}" ]]; then
  nonproxy_env_source="${(%):-%x}"
  nonproxy_script_dir="$(CDPATH= cd -- "$(dirname -- "$nonproxy_env_source")" && pwd)"
  export NONPROXY_ROOT="$(CDPATH= cd -- "$nonproxy_script_dir/../.." && pwd)"
else
  printf 'Unable to resolve the NonProxy repository root.\n' >&2
  return 1 2>/dev/null || exit 1
fi

export NONPROXY_TOOLS_DIR="$NONPROXY_ROOT/.tools"
export DOTNET_ROOT="$NONPROXY_TOOLS_DIR/dotnet"
export NONPROXY_NODE_ROOT="$NONPROXY_TOOLS_DIR/node"
export CARGO_HOME="$NONPROXY_TOOLS_DIR/cargo"
export RUSTUP_HOME="$NONPROXY_TOOLS_DIR/rustup"
export DOTNET_CLI_HOME="$NONPROXY_TOOLS_DIR/dotnet-home"
export NUGET_PACKAGES="$NONPROXY_TOOLS_DIR/nuget/packages"
export DOTNET_CLI_TELEMETRY_OPTOUT=1
export DOTNET_NOLOGO=1
export PATH="$DOTNET_ROOT:$NONPROXY_NODE_ROOT/bin:$NONPROXY_TOOLS_DIR/bin:$CARGO_HOME/bin:$PATH"

if [[ -d /Applications/Xcode.app/Contents/Developer ]]; then
  export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode.app/Contents/Developer}"
fi

unset nonproxy_env_source
unset nonproxy_script_dir
unset nonproxy_git_root
