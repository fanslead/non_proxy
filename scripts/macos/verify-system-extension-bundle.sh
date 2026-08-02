#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "用法：$0 <NonProxy.app>" >&2
    exit 64
fi

app_bundle=${1%/}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
gateway_plist_template="${script_dir}/../../platform/macos/Packaging/com.nonproxy.gatewayd.plist"
adapter_host_plist_template="${script_dir}/../../platform/macos/Packaging/com.nonproxy.adapter-host.plist"
extensions_root="${app_bundle}/Contents/Library/SystemExtensions"
transparent_bundle="${extensions_root}/com.nonproxy.desktop.transparent-proxy.systemextension"
dns_bundle="${extensions_root}/com.nonproxy.desktop.dns-proxy.systemextension"
bridge_library="${app_bundle}/Contents/Frameworks/libNonProxyMacHostBridge.dylib"
launch_agents_root="${app_bundle}/Contents/Library/LaunchAgents"
gateway_agent_plist="${launch_agents_root}/com.nonproxy.gatewayd.plist"
gateway_binary="${app_bundle}/Contents/Resources/nonproxy-gatewayd"
adapter_host_agent_plist="${launch_agents_root}/com.nonproxy.adapter-host.plist"
adapter_host_binary="${app_bundle}/Contents/Resources/nonproxy-adapter-host"
native_messaging_host="${app_bundle}/Contents/Resources/nonproxy-native-messaging-host"
browser_extensions_source="${script_dir}/../../packages/browser-extension/dist"
browser_extensions_root="${app_bundle}/Contents/Resources/BrowserExtensions"

assert_plist_value() {
    local plist=$1
    local key=$2
    local expected=$3
    local actual
    actual=$(/usr/libexec/PlistBuddy -c "Print :${key}" "${plist}")
    if [[ "${actual}" != "${expected}" ]]; then
        echo "Info.plist 字段不匹配：${key}=${actual}，预期 ${expected}" >&2
        exit 67
    fi
}

if [[ ! -d "${app_bundle}" || ! -f "${app_bundle}/Contents/Info.plist" ]]; then
    echo "待验证的 macOS App Bundle 无效：${app_bundle}" >&2
    exit 65
fi
if [[ ! -d "${extensions_root}" || -L "${extensions_root}" ]]; then
    echo "System Extension 目录缺失或不能是符号链接" >&2
    exit 67
fi
extension_entry_count=$(find "${extensions_root}" \
    -mindepth 1 \
    -maxdepth 1 | wc -l | tr -d ' ')
if [[ "${extension_entry_count}" != 2 ]]; then
    echo "System Extension 目录只能包含两个受管 Bundle" >&2
    exit 67
fi

app_plist="${app_bundle}/Contents/Info.plist"
assert_plist_value \
    "${app_plist}" \
    CFBundleIdentifier \
    com.nonproxy.desktop
assert_plist_value \
    "${app_plist}" \
    NSLocationWhenInUseUsageDescription \
    "用于在本机识别当前 Wi-Fi 并自动应用对应的直连规则；网络名称只在内存中哈希。"
host_executable=$(/usr/libexec/PlistBuddy \
    -c "Print :CFBundleExecutable" \
    "${app_plist}")
host_binary="${app_bundle}/Contents/MacOS/${host_executable}"
if [[ ! -x "${host_binary}" || -L "${host_binary}" ]]; then
    echo "macOS App Bundle 缺少宿主可执行文件：${host_binary}" >&2
    exit 67
fi
host_architectures=$(lipo -archs "${host_binary}")
host_minimum_system_version=$(/usr/libexec/PlistBuddy \
    -c "Print :LSMinimumSystemVersion" \
    "${app_plist}")
if [[ "${host_minimum_system_version}" != 15.0 ]]; then
    echo "宿主最低系统版本必须为 macOS 15.0" >&2
    exit 67
fi
if [[ ! -f "${bridge_library}" || -L "${bridge_library}" ]]; then
    echo "macOS App 缺少原生宿主桥接动态库" >&2
    exit 67
fi
file "${bridge_library}" | grep -F "Mach-O" >/dev/null
bridge_architectures=$(lipo -archs "${bridge_library}")
if [[ "${host_architectures}" != "${bridge_architectures}" ]]; then
    echo "宿主与原生桥接动态库架构不一致" >&2
    exit 67
fi
otool -L "${bridge_library}" | grep -F \
    "/System/Library/Frameworks/NetworkExtension.framework/" >/dev/null
otool -L "${bridge_library}" | grep -F \
    "/System/Library/Frameworks/SystemExtensions.framework/" >/dev/null
otool -L "${bridge_library}" | grep -F \
    "/System/Library/Frameworks/ServiceManagement.framework/" >/dev/null
otool -L "${bridge_library}" | grep -F \
    "/System/Library/Frameworks/CoreLocation.framework/" >/dev/null
otool -L "${bridge_library}" | grep -F \
    "/System/Library/Frameworks/CoreWLAN.framework/" >/dev/null
otool -L "${bridge_library}" | grep -F \
    "/System/Library/Frameworks/SystemConfiguration.framework/" >/dev/null
for symbol in \
    _np_mac_bridge_abi_version \
    _np_mac_bridge_open_login_items_settings \
    _np_mac_bridge_probe \
    _np_mac_bridge_query \
    _np_mac_bridge_list_applications \
    _np_mac_bridge_choose_application \
    _np_mac_bridge_capture_current_network \
    _np_mac_bridge_install_and_enable \
    _np_mac_bridge_disable_and_uninstall; do
    if ! nm -g "${bridge_library}" | grep -F "${symbol}" >/dev/null; then
        echo "原生桥接动态库缺少导出符号：${symbol}" >&2
        exit 67
    fi
done
codesign --verify --strict --verbose=2 "${bridge_library}"
if [[ ! -d "${launch_agents_root}" || -L "${launch_agents_root}" ]]; then
    echo "macOS App 缺少有效的 LaunchAgent 目录" >&2
    exit 67
fi
launch_agent_count=$(find "${launch_agents_root}" \
    -mindepth 1 \
    -maxdepth 1 | wc -l | tr -d ' ')
if [[ "${launch_agent_count}" != 2 ||
      ! -f "${gateway_agent_plist}" ||
      -L "${gateway_agent_plist}" ||
      ! -f "${adapter_host_agent_plist}" ||
      -L "${adapter_host_agent_plist}" ]]; then
    echo "LaunchAgent 目录只能包含 NonProxy gatewayd 与 adapter-host 配置" >&2
    exit 67
fi
plutil -lint "${gateway_agent_plist}" >/dev/null
plutil -lint "${adapter_host_agent_plist}" >/dev/null
assert_plist_value \
    "${gateway_agent_plist}" \
    Label \
    com.nonproxy.gatewayd
assert_plist_value \
    "${gateway_agent_plist}" \
    BundleProgram \
    Contents/Resources/nonproxy-gatewayd
assert_plist_value "${gateway_agent_plist}" RunAtLoad true
assert_plist_value "${gateway_agent_plist}" KeepAlive true
assert_plist_value "${gateway_agent_plist}" ProcessType Background
assert_plist_value "${gateway_agent_plist}" ThrottleInterval 5
assert_plist_value \
    "${adapter_host_agent_plist}" \
    Label \
    com.nonproxy.adapter-host
assert_plist_value \
    "${adapter_host_agent_plist}" \
    BundleProgram \
    Contents/Resources/nonproxy-adapter-host
assert_plist_value "${adapter_host_agent_plist}" RunAtLoad true
assert_plist_value "${adapter_host_agent_plist}" KeepAlive true
assert_plist_value "${adapter_host_agent_plist}" ProcessType Background
assert_plist_value "${adapter_host_agent_plist}" ThrottleInterval 5
gateway_bundle_fingerprint=$(
    /usr/libexec/PlistBuddy -c \
        "Print :EnvironmentVariables:NONPROXY_GATEWAY_BUNDLE_FINGERPRINT" \
        "${gateway_agent_plist}"
)
if [[ ${#gateway_bundle_fingerprint} -ne 64 ||
      "${gateway_bundle_fingerprint}" == *[!0-9a-f]* ]]; then
    echo "LaunchAgent 缺少规范的 gatewayd 包指纹" >&2
    exit 67
fi
adapter_host_bundle_fingerprint=$(
    /usr/libexec/PlistBuddy -c \
        "Print :EnvironmentVariables:NONPROXY_ADAPTER_BUNDLE_FINGERPRINT" \
        "${adapter_host_agent_plist}"
)
if [[ ${#adapter_host_bundle_fingerprint} -ne 64 ||
      "${adapter_host_bundle_fingerprint}" == *[!0-9a-f]* ]]; then
    echo "LaunchAgent 缺少规范的 adapter-host 包指纹" >&2
    exit 67
fi
configured_exit_probe_endpoint=$(
    /usr/libexec/PlistBuddy -c \
        "Print :EnvironmentVariables:NONPROXY_EXIT_PROBE_ENDPOINT" \
        "${gateway_agent_plist}" 2>/dev/null || true
)
configured_exit_probe_public_keys=$(
    /usr/libexec/PlistBuddy -c \
        "Print :EnvironmentVariables:NONPROXY_EXIT_PROBE_PUBLIC_KEYS" \
        "${gateway_agent_plist}" 2>/dev/null || true
)
legacy_exit_probe_public_key=$(
    /usr/libexec/PlistBuddy -c \
        "Print :EnvironmentVariables:NONPROXY_EXIT_PROBE_PUBLIC_KEY" \
        "${gateway_agent_plist}" 2>/dev/null || true
)
if [[ -n "${legacy_exit_probe_public_key}" ]]; then
    echo "LaunchAgent 发布包必须使用复数出口探针公钥配置" >&2
    exit 67
fi
if ! "${script_dir}/validate-exit-probe-config.sh" \
    "${configured_exit_probe_endpoint}" \
    "${configured_exit_probe_public_keys}"; then
    exit 67
fi
for plist in "${gateway_agent_plist}" "${adapter_host_agent_plist}"; do
    for forbidden_key in Program ProgramArguments; do
        if /usr/libexec/PlistBuddy \
            -c "Print :${forbidden_key}" \
            "${plist}" >/dev/null 2>&1; then
            echo "LaunchAgent 必须仅使用可随 App 移动的 BundleProgram" >&2
            exit 67
        fi
    done
done
if [[ ! -x "${gateway_binary}" || -L "${gateway_binary}" ]]; then
    echo "macOS App 缺少 gatewayd 可执行文件" >&2
    exit 67
fi
file "${gateway_binary}" | grep -F "Mach-O" >/dev/null
gateway_architectures=$(lipo -archs "${gateway_binary}")
if [[ "${host_architectures}" != "${gateway_architectures}" ]]; then
    echo "宿主与 gatewayd 架构不一致" >&2
    exit 67
fi
codesign --verify --strict --verbose=2 "${gateway_binary}"
gateway_identifier=$(
    codesign -d --verbose=4 "${gateway_binary}" 2>&1 |
        awk -F= '$1 == "Identifier" && !found { print $2; found = 1 }'
)
if [[ "${gateway_identifier}" != com.nonproxy.gatewayd ]]; then
    echo "gatewayd 代码签名标识必须固定为 com.nonproxy.gatewayd" >&2
    exit 67
fi
gateway_team_identifier=$(
    codesign -d --verbose=4 "${gateway_binary}" 2>&1 |
        awk -F= '$1 == "TeamIdentifier" && !found { print $2; found = 1 }'
)
host_team_identifier=$(
    codesign -d --verbose=4 "${host_binary}" 2>&1 |
        awk -F= '$1 == "TeamIdentifier" && !found { print $2; found = 1 }'
)
configured_gateway_team_identifier=$(
    /usr/libexec/PlistBuddy -c \
        "Print :EnvironmentVariables:NONPROXY_MAC_TEAM_IDENTIFIER" \
        "${gateway_agent_plist}" 2>/dev/null || true
)
if [[ -n "${gateway_team_identifier}" &&
      "${gateway_team_identifier}" != "not set" ]]; then
    if [[ "${host_team_identifier}" != "${gateway_team_identifier}" ]]; then
        echo "gatewayd 与宿主 App 的 TeamIdentifier 不一致" >&2
        exit 67
    fi
    if [[ "${configured_gateway_team_identifier}" != "${gateway_team_identifier}" ]]; then
        echo "LaunchAgent 的 macOS TeamIdentifier 与 gatewayd 签名不一致" >&2
        exit 67
    fi
else
    if [[ -n "${host_team_identifier}" &&
          "${host_team_identifier}" != "not set" ]]; then
        echo "宿主 App 有 TeamIdentifier 时 gatewayd 也必须由同一团队签名" >&2
        exit 67
    fi
    if [[ -n "${configured_gateway_team_identifier}" ]]; then
        echo "临时签名 gatewayd 不能声明受信任的 macOS TeamIdentifier" >&2
        exit 67
    fi
fi
if [[ "${NONPROXY_RESTRICTED_SIGNING:-0}" == 1 &&
      ( -z "${gateway_team_identifier}" ||
        "${gateway_team_identifier}" == "not set" ) ]]; then
    echo "正式受限签名的 gatewayd 缺少 TeamIdentifier" >&2
    exit 67
fi
if [[ ! -x "${adapter_host_binary}" || -L "${adapter_host_binary}" ]]; then
    echo "macOS App 缺少 adapter-host 可执行文件" >&2
    exit 67
fi
file "${adapter_host_binary}" | grep -F "Mach-O" >/dev/null
adapter_host_architectures=$(lipo -archs "${adapter_host_binary}")
if [[ "${host_architectures}" != "${adapter_host_architectures}" ]]; then
    echo "宿主与 adapter-host 架构不一致" >&2
    exit 67
fi
codesign --verify --strict --verbose=2 "${adapter_host_binary}"
adapter_host_identifier=$(
    codesign -d --verbose=4 "${adapter_host_binary}" 2>&1 |
        awk -F= '$1 == "Identifier" && !found { print $2; found = 1 }'
)
if [[ "${adapter_host_identifier}" != com.nonproxy.adapter-host ]]; then
    echo "adapter-host 代码签名标识必须固定为 com.nonproxy.adapter-host" >&2
    exit 67
fi
adapter_host_team_identifier=$(
    codesign -d --verbose=4 "${adapter_host_binary}" 2>&1 |
        awk -F= '$1 == "TeamIdentifier" && !found { print $2; found = 1 }'
)
if [[ "${adapter_host_team_identifier}" != "${gateway_team_identifier}" ]]; then
    echo "adapter-host 与 gatewayd 的 TeamIdentifier 不一致" >&2
    exit 67
fi
if [[ ! -x "${native_messaging_host}" || -L "${native_messaging_host}" ]]; then
    echo "macOS App 缺少 Native Messaging Host" >&2
    exit 67
fi
file "${native_messaging_host}" | grep -F "Mach-O" >/dev/null
native_host_architectures=$(lipo -archs "${native_messaging_host}")
if [[ "${host_architectures}" != "${native_host_architectures}" ]]; then
    echo "宿主与 Native Messaging Host 架构不一致" >&2
    exit 67
fi
codesign --verify --strict --verbose=2 "${native_messaging_host}"
if [[ ! -d "${browser_extensions_root}" ||
      -L "${browser_extensions_root}" ]]; then
    echo "macOS App 缺少有效的共享浏览器扩展资产" >&2
    exit 67
fi
browser_extension_entry_count=$(find "${browser_extensions_root}" \
    -mindepth 1 \
    -maxdepth 1 | wc -l | tr -d ' ')
if [[ "${browser_extension_entry_count}" != 3 ]]; then
    echo "浏览器扩展资产只能包含 Chromium、Safari 与构建信息" >&2
    exit 67
fi
if find "${browser_extensions_root}" -type l -print -quit |
    grep -q .; then
    echo "浏览器扩展资产不得包含符号链接" >&2
    exit 67
fi
for target in chromium safari; do
    target_root="${browser_extensions_root}/${target}"
    for relative in \
        manifest.json \
        background/background.js \
        popup/popup.html \
        popup/popup.js \
        shared/native-contract.js; do
        if [[ ! -f "${target_root}/${relative}" ]]; then
            echo "浏览器扩展资产缺少 ${target}/${relative}" >&2
            exit 67
        fi
    done
    manifest_version=$(node -e \
        'const fs = require("node:fs"); const value = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); process.stdout.write(String(value.manifest_version));' \
        "${target_root}/manifest.json")
    if [[ "${manifest_version}" != 3 ]]; then
        echo "${target} 浏览器扩展必须使用 Manifest V3" >&2
        exit 67
    fi
    if ! diff -qr \
        "${browser_extensions_source}/${target}" \
        "${target_root}" >/dev/null; then
        echo "${target} 浏览器扩展资产与当前源码构建不一致" >&2
        exit 67
    fi
done
if ! cmp -s \
    "${browser_extensions_source}/BUILD_INFO.json" \
    "${browser_extensions_root}/BUILD_INFO.json"; then
    echo "浏览器扩展构建信息与当前源码构建不一致" >&2
    exit 67
fi
"${script_dir}/verify-safari-web-extension.sh" "${app_bundle}"
gateway_binary_digest=$(shasum -a 256 "${gateway_binary}" | awk '{print $1}')
gateway_plist_digest=$(shasum -a 256 "${gateway_plist_template}" | awk '{print $1}')
expected_gateway_fingerprint=$(
    printf '%s%s' "${gateway_binary_digest}" "${gateway_plist_digest}" |
        shasum -a 256 |
        awk '{print $1}'
)
if [[ "${gateway_bundle_fingerprint}" != "${expected_gateway_fingerprint}" ]]; then
    echo "LaunchAgent 包指纹与已签名 gatewayd 或 plist 不一致" >&2
    exit 67
fi
adapter_host_binary_digest=$(shasum -a 256 "${adapter_host_binary}" | awk '{print $1}')
adapter_host_plist_digest=$(shasum -a 256 "${adapter_host_plist_template}" | awk '{print $1}')
expected_adapter_host_fingerprint=$(
    printf '%s%s' "${adapter_host_binary_digest}" "${adapter_host_plist_digest}" |
        shasum -a 256 |
        awk '{print $1}'
)
if [[ "${adapter_host_bundle_fingerprint}" != "${expected_adapter_host_fingerprint}" ]]; then
    echo "LaunchAgent 包指纹与已签名 adapter-host 或 plist 不一致" >&2
    exit 67
fi

verification_dir=$(mktemp -d)
trap 'rm -rf "${verification_dir}"' EXIT

assert_entitlement_contains() {
    local entitlements=$1
    local key=$2
    local expected=$3
    if ! /usr/libexec/PlistBuddy \
        -c "Print :${key}" \
        "${entitlements}" | grep -F "${expected}" >/dev/null; then
        echo "签名权限缺少 ${key}=${expected}" >&2
        exit 67
    fi
}

assert_entitlement_true() {
    local entitlements=$1
    local key=$2
    local actual
    actual=$(/usr/libexec/PlistBuddy -c "Print :${key}" "${entitlements}")
    if [[ "${actual}" != true ]]; then
        echo "签名权限 ${key} 必须为 true" >&2
        exit 67
    fi
}

verify_extension() {
    local bundle=$1
    local bundle_id=$2
    local executable=$3
    local extension_point=$4
    local principal_class=$5
    local entitlement=$6
    local objective_c_class=$7

    local plist="${bundle}/Contents/Info.plist"
    local binary="${bundle}/Contents/MacOS/${executable}"
    if [[ ! -d "${bundle}" || -L "${bundle}" ||
          ! -f "${plist}" || -L "${plist}" ||
          ! -x "${binary}" || -L "${binary}" ]]; then
        echo "System Extension Bundle 结构不完整：${bundle}" >&2
        exit 67
    fi
    plutil -lint "${plist}" >/dev/null
    assert_plist_value "${plist}" CFBundlePackageType SYSX
    assert_plist_value "${plist}" CFBundleIdentifier "${bundle_id}"
    assert_plist_value "${plist}" CFBundleExecutable "${executable}"
    assert_plist_value \
        "${plist}" \
        NSExtension:NSExtensionPointIdentifier \
        "${extension_point}"
    assert_plist_value \
        "${plist}" \
        NSExtension:NSExtensionPrincipalClass \
        "${principal_class}"
    assert_plist_value \
        "${plist}" \
        LSMinimumSystemVersion \
        "${host_minimum_system_version}"
    assert_plist_value \
        "${plist}" \
        NSLocationWhenInUseUsageDescription \
        "用于在本机识别当前 Wi-Fi 并自动应用对应的直连规则；网络名称只在内存中哈希。"
    file "${binary}" | grep -F "Mach-O" >/dev/null
    local extension_architectures
    extension_architectures=$(lipo -archs "${binary}")
    if [[ "${host_architectures}" != "${extension_architectures}" ]]; then
        echo "宿主与 System Extension 架构不一致：${bundle}" >&2
        exit 67
    fi
    otool -L "${binary}" | grep -F \
        "/System/Library/Frameworks/NetworkExtension.framework/" >/dev/null
    otool -L "${binary}" | grep -F \
        "/System/Library/Frameworks/CoreWLAN.framework/" >/dev/null
    otool -L "${binary}" | grep -F \
        "/System/Library/Frameworks/SystemConfiguration.framework/" >/dev/null
    nm -g "${binary}" | grep -F \
        "_OBJC_CLASS_\$_${objective_c_class}" >/dev/null
    codesign --verify --strict --verbose=2 "${bundle}"

    local entitlements
    entitlements="${verification_dir}/${executable}.entitlements.plist"
    codesign -d --entitlements :- "${bundle}" >"${entitlements}" 2>/dev/null
    assert_entitlement_contains \
        "${entitlements}" \
        com.apple.developer.networking.networkextension \
        "${entitlement}"
    assert_entitlement_true \
        "${entitlements}" \
        com.apple.security.app-sandbox
    assert_entitlement_true \
        "${entitlements}" \
        com.apple.security.personal-information.location
    assert_entitlement_contains \
        "${entitlements}" \
        com.apple.security.application-groups \
        group.com.nonproxy.shared
}

verify_extension \
    "${transparent_bundle}" \
    com.nonproxy.desktop.transparent-proxy \
    NonProxyTransparentSystemExtension \
    com.apple.networkextension.app-proxy \
    NonProxyTransparentProxy.TransparentProxyProvider \
    app-proxy-provider-systemextension \
    TransparentProxyProvider
verify_extension \
    "${dns_bundle}" \
    com.nonproxy.desktop.dns-proxy \
    NonProxyDNSSystemExtension \
    com.apple.networkextension.dns-proxy \
    NonProxyDNSProxy.DNSProxyProvider \
    dns-proxy-systemextension \
    DNSProxyProvider

if [[ "${NONPROXY_RESTRICTED_SIGNING:-0}" == 1 ]]; then
    host_entitlements="${verification_dir}/host.entitlements.plist"
    codesign -d --entitlements :- \
        "${app_bundle}" >"${host_entitlements}" 2>/dev/null
    assert_entitlement_true \
        "${host_entitlements}" \
        com.apple.developer.system-extension.install
    assert_entitlement_contains \
        "${host_entitlements}" \
        com.apple.developer.networking.networkextension \
        app-proxy-provider-systemextension
    assert_entitlement_contains \
        "${host_entitlements}" \
        com.apple.developer.networking.networkextension \
        dns-proxy-systemextension
    assert_entitlement_contains \
        "${host_entitlements}" \
        com.apple.security.application-groups \
        group.com.nonproxy.shared
    for bundle in \
        "${app_bundle}" \
        "${transparent_bundle}" \
        "${dns_bundle}" \
        "${app_bundle}/Contents/PlugIns/NonProxySafariWebExtension.appex"; do
        if [[ ! -f "${bundle}/Contents/embedded.provisionprofile" ||
              -L "${bundle}/Contents/embedded.provisionprofile" ]]; then
            echo "正式签名 Bundle 缺少有效 provisioning profile：${bundle}" >&2
            exit 67
        fi
    done
fi

assert_plist_value \
    "${app_plist}" \
    NSSystemExtensionUsageDescription \
    "NonProxy 需要安装网络系统扩展，以便按应用和网站选择直连或指定代理。"
codesign --verify --deep --strict --verbose=2 "${app_bundle}"
echo "macOS App 已包含签名有效的系统扩展、Safari .appex、浏览器资产、Native Messaging Host、gatewayd 与 adapter-host。"
