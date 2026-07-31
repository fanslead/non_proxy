#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "用法：$0 <NonProxy.app>" >&2
    exit 64
fi

app_bundle=${1%/}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source_root="${script_dir}/../../packages/browser-extension/dist/safari"
plugins_root="${app_bundle}/Contents/PlugIns"
bundle="${plugins_root}/NonProxySafariWebExtension.appex"
plist="${bundle}/Contents/Info.plist"
binary="${bundle}/Contents/MacOS/NonProxySafariWebExtension"
resources="${bundle}/Contents/Resources"
plist_buddy=/usr/libexec/PlistBuddy

fail() {
    echo "$1" >&2
    exit 67
}

assert_plist_value() {
    local key=$1
    local expected=$2
    local actual
    actual=$("${plist_buddy}" -c "Print :${key}" "${plist}")
    if [[ "${actual}" != "${expected}" ]]; then
        fail "Safari 扩展 Info.plist 不匹配：${key}=${actual}，预期 ${expected}"
    fi
}

if [[ ! -d "${plugins_root}" || -L "${plugins_root}" ]]; then
    fail "macOS App 缺少有效的 PlugIns 目录"
fi
entry_count=$(
    find "${plugins_root}" -mindepth 1 -maxdepth 1 |
        wc -l |
        tr -d ' '
)
if [[ "${entry_count}" != 1 ]]; then
    fail "PlugIns 目录只能包含一个 NonProxy Safari 扩展"
fi
if [[ ! -d "${bundle}" ||
      -L "${bundle}" ||
      ! -f "${plist}" ||
      -L "${plist}" ||
      ! -x "${binary}" ||
      -L "${binary}" ||
      ! -d "${resources}" ||
      -L "${resources}" ]]; then
    fail "Safari Web Extension Bundle 结构不完整"
fi
if find "${bundle}" -type l -print -quit | grep -q .; then
    fail "Safari Web Extension Bundle 不得包含符号链接"
fi

plutil -lint "${plist}" >/dev/null
assert_plist_value CFBundlePackageType "XPC!"
assert_plist_value \
    CFBundleIdentifier \
    com.nonproxy.desktop.safari-web-extension
assert_plist_value CFBundleExecutable NonProxySafariWebExtension
assert_plist_value \
    NSExtension:NSExtensionPointIdentifier \
    com.apple.Safari.web-extension
assert_plist_value \
    NSExtension:NSExtensionPrincipalClass \
    NonProxySafariWebExtension.SafariWebExtensionHandler

host_plist="${app_bundle}/Contents/Info.plist"
host_version=$(
    "${plist_buddy}" -c "Print :CFBundleShortVersionString" "${host_plist}"
)
host_build=$("${plist_buddy}" -c "Print :CFBundleVersion" "${host_plist}")
host_minimum=$(
    "${plist_buddy}" -c "Print :LSMinimumSystemVersion" "${host_plist}"
)
assert_plist_value CFBundleShortVersionString "${host_version}"
assert_plist_value CFBundleVersion "${host_build}"
assert_plist_value LSMinimumSystemVersion "${host_minimum}"

host_name=$("${plist_buddy}" -c "Print :CFBundleExecutable" "${host_plist}")
host_binary="${app_bundle}/Contents/MacOS/${host_name}"
host_architectures=$(lipo -archs "${host_binary}")
extension_architectures=$(lipo -archs "${binary}")
if [[ "${host_architectures}" != "${extension_architectures}" ]]; then
    fail "宿主与 Safari Web Extension 架构不一致"
fi
file "${binary}" | grep -F "Mach-O" >/dev/null
otool -L "${binary}" | grep -F \
    "/System/Library/Frameworks/SafariServices.framework/" >/dev/null
for architecture in ${extension_architectures}; do
    if ! otool -arch "${architecture}" -ov "${binary}" |
        grep -F "SafariWebExtensionHandler" >/dev/null; then
        fail "Safari 扩展 ${architecture} 二进制缺少 Principal Class"
    fi
done

if ! diff -qr "${source_root}" "${resources}" >/dev/null; then
    fail "Safari App Extension 资源与当前浏览器构建产物不一致"
fi
node -e '
  const fs = require("node:fs");
  const manifest = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  if (
    manifest.manifest_version !== 3 ||
    manifest.background?.type !== undefined ||
    JSON.stringify(manifest.background?.scripts) !==
      JSON.stringify(["background/background.js"]) ||
    !manifest.permissions?.includes("nativeMessaging") ||
    manifest.icons?.["512"] !== "icons/nonproxy.svg" ||
    manifest.action?.default_icon?.["32"] !== "icons/nonproxy.svg"
  ) {
    process.exit(1);
  }
' "${resources}/manifest.json" ||
    fail "Safari 扩展清单不满足 App Extension 运行约束"
if [[ ! -f "${resources}/icons/nonproxy.svg" ||
      -L "${resources}/icons/nonproxy.svg" ]]; then
    fail "Safari 扩展缺少有效图标资源"
fi
if grep -E '^[[:space:]]*(import|export)[[:space:]]' \
    "${resources}/background/background.js" >/dev/null; then
    fail "Safari 后台入口不得依赖 JavaScript 模块加载"
fi

verification_dir=$(mktemp -d)
cleanup() {
    rm -r "${verification_dir}"
}
trap cleanup EXIT
entitlements="${verification_dir}/safari.entitlements.plist"
codesign -d --entitlements :- "${bundle}" >"${entitlements}" 2>/dev/null
for key in \
    com.apple.security.app-sandbox \
    com.apple.security.network.client; do
    value=$("${plist_buddy}" -c "Print :${key}" "${entitlements}")
    if [[ "${value}" != true ]]; then
        fail "Safari 扩展签名权限 ${key} 必须为 true"
    fi
done
if ! "${plist_buddy}" \
    -c "Print :com.apple.security.application-groups" \
    "${entitlements}" |
    grep -F group.com.nonproxy.shared >/dev/null; then
    fail "Safari 扩展签名权限缺少 NonProxy App Group"
fi
if [[ "${NONPROXY_RESTRICTED_SIGNING:-0}" == 1 &&
      ( ! -f "${bundle}/Contents/embedded.provisionprofile" ||
        -L "${bundle}/Contents/embedded.provisionprofile" ) ]]; then
    fail "正式签名 Safari 扩展缺少有效 provisioning profile"
fi

codesign --verify --strict --verbose=2 "${bundle}"
echo "Safari Web Extension .appex 的结构、资源、架构、权限与签名有效。"
