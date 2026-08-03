#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "用法：$0 <NonProxy.app> <version> <codesign-identity> [output-directory]" >&2
}

if [[ $# -lt 3 || $# -gt 4 ]]; then
    usage
    exit 64
fi

app_bundle=${1%/}
version=$2
codesign_identity=$3
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${script_dir}/../.." && pwd)
output_directory=${4:-"${repo_root}/.artifacts/release/${version}"}
asset_name="NonProxy-${version}-macos-universal-development.dmg"
output="${output_directory}/${asset_name}"

if [[ $(uname -s) != Darwin ]]; then
    echo "macOS 开发预览版只能在 macOS 主机打包。" >&2
    exit 69
fi
if [[ ! -d ${app_bundle} || ! -f ${app_bundle}/Contents/Info.plist ]]; then
    echo "待打包 App Bundle 无效：${app_bundle}" >&2
    exit 65
fi
if [[ -e ${output} ]]; then
    echo "DMG 已存在，拒绝覆盖：${output}" >&2
    exit 73
fi

actual_version=$(/usr/libexec/PlistBuddy \
    -c "Print :CFBundleShortVersionString" \
    "${app_bundle}/Contents/Info.plist")
if [[ ${actual_version} != "${version}" ]]; then
    echo "App Bundle 版本 ${actual_version} 与发布版本 ${version} 不一致。" >&2
    exit 65
fi
codesign --verify --deep --strict --verbose=2 "${app_bundle}"
"${script_dir}/verify-system-extension-bundle.sh" "${app_bundle}"
"${script_dir}/native-bridge-smoke.sh" "${app_bundle}"

staging=$(mktemp -d)
cleanup() {
    rm -rf "${staging}"
}
trap cleanup EXIT
cp -R "${app_bundle}" "${staging}/NonProxy.app"
ln -s /Applications "${staging}/Applications"
cat >"${staging}/开发预览版说明.txt" <<EOF
NonProxy ${version} macOS 开发预览版

此 DMG 使用 Apple Development 身份签名，不是 Developer ID 签名，也未经过 Apple 公证。
当前构建没有 NonProxy 专用 Provisioning Profile，因此应用界面可以用于源码与交互验证，
但 Transparent Proxy、DNS Proxy 和 Safari Web Extension 的受限系统权限不能作为可激活状态验收。

请勿把此包用于生产环境，也不要据此声明已完成真实 VPN 直连路径验收。
EOF

mkdir -p "${output_directory}"
hdiutil create \
    -volname "NonProxy ${version} Development" \
    -srcfolder "${staging}" \
    -format UDZO \
    -ov \
    "${output}"
codesign \
    --force \
    --sign "${codesign_identity}" \
    --timestamp \
    "${output}"
codesign --verify --strict --verbose=2 "${output}"
hdiutil verify "${output}"

sha256=$(shasum -a 256 "${output}" | awk '{print $1}')
printf '%s  %s\n' "${sha256}" "${asset_name}" >"${output}.sha256"
printf 'macOS 开发预览版已生成：%s\n' "${output}"
printf 'SHA-256：%s\n' "${sha256}"
