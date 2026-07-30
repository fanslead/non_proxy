#!/usr/bin/env bash
set -euo pipefail
umask 077
export LC_ALL=C

if [[ $# -ne 3 ]]; then
    echo "用法：$0 <已安装的 NonProxy.app> <query|install|upgrade|uninstall|lifecycle> <证据目录>" >&2
    exit 64
fi

app_bundle=${1%/}
action=$2
evidence_directory=${3%/}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

case "${action}" in
    query | install | upgrade | uninstall | lifecycle) ;;
    *)
        echo "不支持的系统验收动作：${action}" >&2
        exit 64
        ;;
esac

if [[ ! -d "${app_bundle}" || -L "${app_bundle}" ]]; then
    echo "系统验收目标必须是普通 App Bundle 目录" >&2
    exit 65
fi
app_parent=$(cd "$(dirname "${app_bundle}")" && pwd -P)
app_bundle="${app_parent}/$(basename "${app_bundle}")"
case "${app_bundle}" in
    /Applications/*.app) ;;
    *)
        echo "真实系统验收只接受 /Applications 中的 App Bundle" >&2
        exit 65
        ;;
esac

if [[ "${action}" != query &&
      "${NONPROXY_ALLOW_SYSTEM_MUTATION:-0}" != 1 ]]; then
    echo "安装、卸载或生命周期验收前必须设置 NONPROXY_ALLOW_SYSTEM_MUTATION=1" >&2
    exit 64
fi

evidence_parent_input=$(dirname "${evidence_directory}")
evidence_name=$(basename "${evidence_directory}")
if [[ ! -d "${evidence_parent_input}" ||
      -L "${evidence_parent_input}" ||
      "${evidence_name}" == . ||
      "${evidence_name}" == .. ]]; then
    echo "证据目录的普通父目录必须预先存在" >&2
    exit 65
fi
evidence_parent=$(cd "${evidence_parent_input}" && pwd -P)
evidence_directory="${evidence_parent}/${evidence_name}"
case "${evidence_directory}" in
    /Applications | /Applications/*)
        echo "证据目录不能位于 /Applications 或待验收 App 内" >&2
        exit 65
        ;;
esac
if [[ -e "${evidence_directory}" ]]; then
    if [[ -L "${evidence_directory}" ||
          ! -d "${evidence_directory}" ]]; then
        echo "证据路径必须是普通目录且不能是符号链接" >&2
        exit 65
    fi
    if [[ -n "$(find "${evidence_directory}" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
        echo "证据目录必须为空，避免覆盖既有验收记录" >&2
        exit 65
    fi
fi
mkdir "${evidence_directory}"

NONPROXY_RESTRICTED_SIGNING=1 \
    "${script_dir}/verify-system-extension-bundle.sh" "${app_bundle}"

app_plist="${app_bundle}/Contents/Info.plist"
host_name=$(
    /usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "${app_plist}"
)
host_binary="${app_bundle}/Contents/MacOS/${host_name}"
signature_details="${evidence_directory}/codesign.txt"
codesign -d --verbose=4 "${app_bundle}" >"${signature_details}" 2>&1
team_identifier=$(
    awk -F= '/^TeamIdentifier=/{print $2}' "${signature_details}"
)
if [[ -z "${team_identifier}" ||
      "${team_identifier}" == "not set" ]]; then
    echo "真实系统验收拒绝临时签名或缺少 TeamIdentifier 的 App" >&2
    exit 66
fi
if ! grep -F "Authority=" "${signature_details}" >/dev/null; then
    echo "真实系统验收缺少可验证的签名证书链" >&2
    exit 66
fi

profile_directory=$(mktemp -d)
cleanup() {
    rm -r "${profile_directory}"
}
trap cleanup EXIT

validate_profile() {
    local bundle=$1
    local expected_bundle_identifier=$2
    local evidence_name=$3
    local profile="${bundle}/Contents/embedded.provisionprofile"
    local decoded="${profile_directory}/${evidence_name}.plist"
    local signing_details="${profile_directory}/${evidence_name}.codesign.txt"

    security cms -D -i "${profile}" >"${decoded}"
    codesign -d --verbose=4 "${bundle}" >"${signing_details}" 2>&1
    local signed_team
    signed_team=$(awk -F= '/^TeamIdentifier=/{print $2}' "${signing_details}")
    local profile_team
    profile_team=$(
        /usr/libexec/PlistBuddy -c "Print :TeamIdentifier:0" "${decoded}"
    )
    local application_identifier
    application_identifier=$(
        /usr/libexec/PlistBuddy \
            -c "Print :Entitlements:application-identifier" \
            "${decoded}"
    )
    local expiration
    expiration=$(plutil -extract ExpirationDate raw -o - "${decoded}")
    local now
    now=$(date -u '+%Y-%m-%dT%H:%M:%SZ')

    local expected_application_identifier="${team_identifier}.${expected_bundle_identifier}"
    if [[ "${signed_team}" != "${team_identifier}" ||
          "${profile_team}" != "${team_identifier}" ||
          "${application_identifier}" != "${expected_application_identifier}" ]]; then
        echo "签名 Team、provisioning profile 与 Bundle ID 不一致：${evidence_name}" >&2
        exit 66
    fi
    if [[ "${expiration}" < "${now}" ]]; then
        echo "provisioning profile 已过期：${evidence_name}" >&2
        exit 66
    fi
}

extensions_root="${app_bundle}/Contents/Library/SystemExtensions"
validate_profile "${app_bundle}" com.nonproxy.desktop host
validate_profile \
    "${extensions_root}/com.nonproxy.desktop.transparent-proxy.systemextension" \
    com.nonproxy.desktop.transparent-proxy \
    transparent-proxy
validate_profile \
    "${extensions_root}/com.nonproxy.desktop.dns-proxy.systemextension" \
    com.nonproxy.desktop.dns-proxy \
    dns-proxy

if [[ "${NONPROXY_REQUIRE_DEVELOPER_ID:-0}" == 1 ]]; then
    if ! grep -F "Authority=Developer ID Application:" \
        "${signature_details}" >/dev/null; then
        echo "发布验收要求 Developer ID Application 签名" >&2
        exit 66
    fi
    spctl --assess --type execute --verbose=4 "${app_bundle}" \
        >"${evidence_directory}/gatekeeper.txt" 2>&1
    xcrun stapler validate "${app_bundle}" \
        >"${evidence_directory}/notarization.txt" 2>&1
fi

manifest="${evidence_directory}/manifest.json"
plutil -create xml1 "${manifest}"
plutil -insert schemaVersion -integer 1 "${manifest}"
plutil -insert action -string "${action}" "${manifest}"
plutil -insert appBundle -string "${app_bundle}" "${manifest}"
plutil -insert bundleIdentifier -string \
    "$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "${app_plist}")" \
    "${manifest}"
plutil -insert bundleVersion -string \
    "$(/usr/libexec/PlistBuddy -c "Print :CFBundleVersion" "${app_plist}")" \
    "${manifest}"
plutil -insert teamIdentifier -string "${team_identifier}" "${manifest}"
plutil -insert hostSha256 -string \
    "$(shasum -a 256 "${host_binary}" | awk '{print $1}')" \
    "${manifest}"
plutil -insert startedAtUtc -string \
    "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    "${manifest}"

run_action() {
    local current_action=$1
    local evidence_name=$2
    local stdout_path="${evidence_directory}/${evidence_name}.json"
    local stderr_path="${evidence_directory}/${evidence_name}.stderr.txt"
    local argument="--system-components-${current_action}"

    if NONPROXY_ALLOW_SYSTEM_MUTATION="${NONPROXY_ALLOW_SYSTEM_MUTATION:-0}" \
        "${host_binary}" "${argument}" \
        >"${stdout_path}" 2>"${stderr_path}"; then
        if [[ ! -s "${stdout_path}" ]]; then
            echo "系统组件 ${current_action} 未返回 JSON 证据" >&2
            return 68
        fi
        plutil -lint "${stdout_path}" >/dev/null
        return 0
    fi
    cat "${stderr_path}" >&2
    return 68
}

json_value() {
    local path=$1
    local key=$2
    plutil -extract "${key}" raw -o - "${path}"
}

require_no_reboot() {
    local path=$1
    if [[ "$(json_value "${path}" requiresReboot)" == true ]]; then
        echo "系统已接受操作但要求重启；保留证据并在重启后继续验收" >&2
        return 69
    fi
}

assert_installed_state() {
    local path=$1
    for key in \
        state.gatewayAgent.ready \
        state.transparentExtension.enabled \
        state.dnsExtension.enabled \
        state.transparentPreference.enabled \
        state.dnsPreference.enabled; do
        if [[ "$(json_value "${path}" "${key}")" != true ]]; then
            echo "安装后系统状态未完全就绪：${key}" >&2
            return 68
        fi
    done
}

assert_uninstalled_state() {
    local path=$1
    for key in \
        state.gatewayAgent.registered \
        state.transparentExtension.installed \
        state.dnsExtension.installed \
        state.transparentPreference.configured \
        state.dnsPreference.configured; do
        if [[ "$(json_value "${path}" "${key}")" != false ]]; then
            echo "卸载后仍有系统组件残留：${key}" >&2
            return 68
        fi
    done
}

assert_upgrade_precondition() {
    local path=$1
    for key in \
        state.gatewayAgent.enabled \
        state.gatewayAgent.requiresUpgrade \
        state.transparentExtension.enabled \
        state.dnsExtension.enabled \
        state.transparentPreference.enabled \
        state.dnsPreference.enabled; do
        if [[ "$(json_value "${path}" "${key}")" != true ]]; then
            echo "升级验收前置状态不完整：${key}" >&2
            return 68
        fi
    done
}

case "${action}" in
    query)
        run_action query query
        ;;
    install)
        run_action install install
        require_no_reboot "${evidence_directory}/install.json"
        run_action query installed-state
        assert_installed_state "${evidence_directory}/installed-state.json"
        ;;
    upgrade)
        run_action query 01-before-upgrade
        assert_upgrade_precondition \
            "${evidence_directory}/01-before-upgrade.json"
        run_action install 02-upgrade
        require_no_reboot "${evidence_directory}/02-upgrade.json"
        run_action query 03-upgraded-state
        assert_installed_state \
            "${evidence_directory}/03-upgraded-state.json"
        ;;
    uninstall)
        run_action uninstall uninstall
        require_no_reboot "${evidence_directory}/uninstall.json"
        run_action query uninstalled-state
        assert_uninstalled_state \
            "${evidence_directory}/uninstalled-state.json"
        ;;
    lifecycle)
        run_action install 01-install
        require_no_reboot "${evidence_directory}/01-install.json"
        run_action query 02-installed-state
        assert_installed_state \
            "${evidence_directory}/02-installed-state.json"
        run_action uninstall 03-uninstall
        require_no_reboot "${evidence_directory}/03-uninstall.json"
        run_action query 04-uninstalled-state
        assert_uninstalled_state \
            "${evidence_directory}/04-uninstalled-state.json"
        ;;
esac

plutil -insert completedAtUtc -string \
    "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
    "${manifest}"
plutil -convert json "${manifest}"
(
    cd "${evidence_directory}"
    find . -type f ! -name SHA256SUMS -print |
        LC_ALL=C sort |
        while IFS= read -r evidence_file; do
            shasum -a 256 "${evidence_file}"
        done
) >"${evidence_directory}/SHA256SUMS"
echo "macOS 系统组件 ${action} 验收通过，证据位于：${evidence_directory}"
