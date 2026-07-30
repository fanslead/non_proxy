#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "用法：$0 <NonProxy.app>" >&2
    exit 64
fi

app_bundle=${1%/}
extensions_root="${app_bundle}/Contents/Library/SystemExtensions"
transparent_bundle="${extensions_root}/com.nonproxy.desktop.transparent-proxy.systemextension"
dns_bundle="${extensions_root}/com.nonproxy.desktop.dns-proxy.systemextension"
bridge_library="${app_bundle}/Contents/Frameworks/libNonProxyMacHostBridge.dylib"
launch_agents_root="${app_bundle}/Contents/Library/LaunchAgents"
gateway_agent_plist="${launch_agents_root}/com.nonproxy.gatewayd.plist"
gateway_binary="${app_bundle}/Contents/Resources/nonproxy-gatewayd"

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
for symbol in \
    _np_mac_bridge_abi_version \
    _np_mac_bridge_open_login_items_settings \
    _np_mac_bridge_probe \
    _np_mac_bridge_query \
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
if [[ "${launch_agent_count}" != 1 ||
      ! -f "${gateway_agent_plist}" ||
      -L "${gateway_agent_plist}" ]]; then
    echo "LaunchAgent 目录只能包含 NonProxy gatewayd 配置" >&2
    exit 67
fi
plutil -lint "${gateway_agent_plist}" >/dev/null
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
for forbidden_key in Program ProgramArguments; do
    if /usr/libexec/PlistBuddy \
        -c "Print :${forbidden_key}" \
        "${gateway_agent_plist}" >/dev/null 2>&1; then
        echo "LaunchAgent 必须仅使用可随 App 移动的 BundleProgram" >&2
        exit 67
    fi
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
    file "${binary}" | grep -F "Mach-O" >/dev/null
    local extension_architectures
    extension_architectures=$(lipo -archs "${binary}")
    if [[ "${host_architectures}" != "${extension_architectures}" ]]; then
        echo "宿主与 System Extension 架构不一致：${bundle}" >&2
        exit 67
    fi
    otool -L "${binary}" | grep -F \
        "/System/Library/Frameworks/NetworkExtension.framework/" >/dev/null
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
        "${dns_bundle}"; do
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
echo "macOS App 已包含签名有效的 System Extension、宿主桥接与 gatewayd LaunchAgent。"
