#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
source "$script_dir/env.sh"
source "$NONPROXY_ROOT/tools/versions.env"

if [[ "$(uname -s)" != "Darwin" ]]; then
  printf 'This bootstrap currently supports macOS. Use tools/versions.env on other platforms.\n' >&2
  exit 1
fi

case "$(uname -m)" in
  arm64)
    rust_target="aarch64-apple-darwin"
    rustup_sha="aeb4105778ca1bd3c6b0e75768f581c656633cd51368fa61289b6a71696ac7e1"
    buf_asset="buf-Darwin-arm64"
    buf_sha="5176f23a6118b9978de1340c3e3301a4ed0d48e16a669510be44b4c355170d57"
    just_asset="just-${JUST_VERSION}-aarch64-apple-darwin.tar.gz"
    just_sha="0381db216c2f97ce31d838a1562c1064dfbfa73f5a8a81581338a2cd9217df47"
    protoc_asset="protoc-${PROTOC_VERSION}-osx-aarch_64.zip"
    protoc_sha="193289af0470c6a1aada357d4fba0bbf8d78bfaac8b5e42ca30af2ef75583de2"
    dotnet_asset="dotnet-sdk-${DOTNET_VERSION}-osx-arm64.tar.gz"
    dotnet_sha="7978737378704435bcb46d1aabf4824f4bac4c72b559b0c5796fadcf15aa2ac7e18fd3e5c983c0bcb48c60793bd554bcfb0caa9972132cf899c97ccd7465d938"
    node_asset="node-v${NODE_VERSION}-darwin-arm64.tar.gz"
    node_sha="e1a97e14c99c803e96c7339403282ea05a499c32f8d83defe9ef5ec66f979ed1"
    ;;
  x86_64)
    rust_target="x86_64-apple-darwin"
    rustup_sha="33cf85df9142bc6d29cbc62fa5ca1d4c29622cddb55213a4c1a43c457fb9b2d7"
    buf_asset="buf-Darwin-x86_64"
    buf_sha="eb815a2708d4a43d31799049d5a2987ea81d0a9e98b53976d47bd1e78d154a8f"
    just_asset="just-${JUST_VERSION}-x86_64-apple-darwin.tar.gz"
    just_sha="5e6ade3698095576274b2b32cc9e5d467185e8e40b04949004c04cc3d7e962dc"
    protoc_asset="protoc-${PROTOC_VERSION}-osx-x86_64.zip"
    protoc_sha="537d73604a344ded6fc94e98e07e529d4fe3e4a0b09e59905353950fafc2a1f7"
    dotnet_asset="dotnet-sdk-${DOTNET_VERSION}-osx-x64.tar.gz"
    dotnet_sha="a0eb333dff6ed7895e6b2469d01d014bc3a74e8632b5704e4fab55247b29c1e17df0c52e857dedf55092ebe650c63da749d8b09af707bc28cc25a12796b4be12"
    node_asset="node-v${NODE_VERSION}-darwin-x64.tar.gz"
    node_sha="dfd0dbd3e721503434df7b7205e719f61b3a3a31b2bcf9729b8b91fea240f080"
    ;;
  *)
    printf 'Unsupported macOS architecture: %s\n' "$(uname -m)" >&2
    exit 1
    ;;
esac

tools_bin="$NONPROXY_TOOLS_DIR/bin"
downloads="$NONPROXY_TOOLS_DIR/downloads"
mkdir -p "$tools_bin" "$downloads" "$CARGO_HOME" "$RUSTUP_HOME"

download_verified() {
  local url="$1"
  local expected_sha="$2"
  local output="$3"

  curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
    "$url" --output "$output"
  printf '%s  %s\n' "$expected_sha" "$output" | shasum -a 256 --check --status
}

download_verified_sha512() {
  local url="$1"
  local expected_sha="$2"
  local output="$3"

  curl --fail --location --silent --show-error --proto '=https' --tlsv1.2 \
    "$url" --output "$output"
  printf '%s  %s\n' "$expected_sha" "$output" | shasum -a 512 --check --status
}

install_dotnet() {
  if [[ -x "$DOTNET_ROOT/dotnet" ]] &&
    [[ "$("$DOTNET_ROOT/dotnet" --version)" == "$DOTNET_VERSION" ]]; then
    return
  fi

  local archive="$downloads/$dotnet_asset"
  download_verified_sha512 \
    "https://builds.dotnet.microsoft.com/dotnet/Sdk/${DOTNET_VERSION}/${dotnet_asset}" \
    "$dotnet_sha" \
    "$archive"
  rm -rf "$DOTNET_ROOT"
  mkdir -p "$DOTNET_ROOT"
  tar -xzf "$archive" -C "$DOTNET_ROOT"
}

install_node() {
  local install_dir="$NONPROXY_TOOLS_DIR/node-v$NODE_VERSION"

  if [[ ! -x "$install_dir/bin/node" ]] ||
    [[ "$("$install_dir/bin/node" --version)" != "v$NODE_VERSION" ]]; then
    local archive="$downloads/$node_asset"
    download_verified \
      "https://nodejs.org/dist/v${NODE_VERSION}/${node_asset}" \
      "$node_sha" \
      "$archive"
    rm -rf "$install_dir"
    mkdir -p "$install_dir"
    tar -xzf "$archive" -C "$install_dir" --strip-components=1
  fi

  ln -sfn "$install_dir" "$NONPROXY_NODE_ROOT"
  "$install_dir/bin/corepack" enable pnpm
  "$install_dir/bin/corepack" install --global "pnpm@$PNPM_VERSION"
}

install_rust() {
  local rustup_init="$downloads/rustup-init-${rust_target}"

  if [[ ! -x "$CARGO_HOME/bin/rustup" ]]; then
    download_verified \
      "https://static.rust-lang.org/rustup/dist/${rust_target}/rustup-init" \
      "$rustup_sha" \
      "$rustup_init"
    chmod 0755 "$rustup_init"
    "$rustup_init" -y --no-modify-path --profile minimal --default-toolchain "$RUST_VERSION"
  fi

  "$CARGO_HOME/bin/rustup" toolchain install "$RUST_VERSION" --profile minimal \
    --component clippy --component rustfmt
  "$CARGO_HOME/bin/rustup" default "$RUST_VERSION"
}

install_buf() {
  if [[ -x "$tools_bin/buf" ]] && [[ "$("$tools_bin/buf" --version)" == "$BUF_VERSION" ]]; then
    return
  fi

  download_verified \
    "https://github.com/bufbuild/buf/releases/download/v${BUF_VERSION}/${buf_asset}" \
    "$buf_sha" \
    "$tools_bin/buf"
  chmod 0755 "$tools_bin/buf"
}

install_just() {
  if [[ -x "$tools_bin/just" ]] && [[ "$("$tools_bin/just" --version)" == "just ${JUST_VERSION}" ]]; then
    return
  fi

  local archive="$downloads/$just_asset"
  download_verified \
    "https://github.com/casey/just/releases/download/${JUST_VERSION}/${just_asset}" \
    "$just_sha" \
    "$archive"
  tar -xzf "$archive" -C "$tools_bin" just
  chmod 0755 "$tools_bin/just"
}

install_protoc() {
  if [[ -x "$tools_bin/protoc" ]] && [[ "$("$tools_bin/protoc" --version)" == "libprotoc ${PROTOC_VERSION}" ]]; then
    return
  fi

  local archive="$downloads/$protoc_asset"
  local install_dir="$NONPROXY_TOOLS_DIR/protoc-$PROTOC_VERSION"
  download_verified \
    "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/${protoc_asset}" \
    "$protoc_sha" \
    "$archive"
  rm -rf "$install_dir"
  mkdir -p "$install_dir"
  unzip -q "$archive" -d "$install_dir"
  ln -sfn "$install_dir/bin/protoc" "$tools_bin/protoc"
}

install_dotnet
install_node
install_rust
install_buf
install_just
install_protoc

"$DOTNET_ROOT/dotnet" workload restore \
  "$NONPROXY_ROOT/apps/desktop/NonProxy.Desktop.Mac/NonProxy.Desktop.Mac.csproj" \
  --version "$DOTNET_WORKLOAD_VERSION" \
  --skip-manifest-update
"$script_dir/check-prerequisites.sh"
