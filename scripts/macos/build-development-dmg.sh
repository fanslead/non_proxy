#!/usr/bin/env bash
set -euo pipefail

usage() {
    echo "用法：$0 <version> <codesign-identity> [output-directory]" >&2
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
    usage
    exit 64
fi

version=$1
codesign_identity=$2
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${script_dir}/../.." && pwd)
output_directory=${3:-"${repo_root}/.artifacts/release/${version}"}
project="${repo_root}/apps/desktop/NonProxy.Desktop.Mac/NonProxy.Desktop.Mac.csproj"
app_bundle="${repo_root}/apps/desktop/NonProxy.Desktop.Mac/bin/Release/net10.0-macos/NonProxy.app"

if [[ $(uname -s) != Darwin ]]; then
    echo "macOS 开发预览版只能在 macOS 主机构建。" >&2
    exit 69
fi
if [[ ! ${version} =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]]; then
    echo "版本号格式无效：${version}" >&2
    exit 64
fi
available_identities=$(security find-identity -v -p codesigning)
if [[ ${available_identities} != *"\"${codesign_identity}\""* ]]; then
    echo "找不到指定的 macOS 代码签名身份：${codesign_identity}" >&2
    exit 66
fi
source "${repo_root}/scripts/bootstrap/env.sh"
dotnet restore \
    "${repo_root}/apps/desktop/NonProxy.Desktop.slnx" \
    --locked-mode \
    -p:Configuration=Release
NONPROXY_CODESIGN_IDENTITY="${codesign_identity}" \
NONPROXY_RESTRICTED_SIGNING=0 \
dotnet build "${project}" \
    --configuration Release \
    --no-restore \
    --no-incremental \
    -p:CodesignKey="${codesign_identity}"

if [[ ! -d ${app_bundle} ]]; then
    echo "Release 构建没有生成预期 App Bundle：${app_bundle}" >&2
    exit 65
fi
"${script_dir}/package-development-dmg.sh" \
    "${app_bundle}" \
    "${version}" \
    "${codesign_identity}" \
    "${output_directory}"
