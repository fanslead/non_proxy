#!/usr/bin/env bash
set -euo pipefail
umask 077
export LC_ALL=C

if [[ $# -ne 3 ]]; then
    echo "用法：$0 <已安装的 NonProxy.app> <query|accept> <证据目录>" >&2
    exit 64
fi

app_bundle=${1%/}
action=$2
evidence_directory=${3%/}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${script_dir}/../.." && pwd)
package_root="${repo_root}/platform/macos"
extension_identifier=com.nonproxy.desktop.safari-web-extension

case "${action}" in
    query | accept) ;;
    *)
        echo "不支持的 Safari 验收动作：${action}" >&2
        exit 64
        ;;
esac
if [[ ! -d "${app_bundle}" || -L "${app_bundle}" ]]; then
    echo "Safari 验收目标必须是普通 App Bundle 目录" >&2
    exit 65
fi
app_parent=$(cd "$(dirname "${app_bundle}")" && pwd -P)
app_bundle="${app_parent}/$(basename "${app_bundle}")"
case "${app_bundle}" in
    /Applications/*.app) ;;
    *)
        echo "Safari 真实验收只接受 /Applications 中的 App Bundle" >&2
        exit 65
        ;;
esac

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
          ! -d "${evidence_directory}" ||
          -n "$(find "${evidence_directory}" -mindepth 1 -print -quit)" ]]; then
        echo "证据目录必须是空的普通目录" >&2
        exit 65
    fi
else
    mkdir "${evidence_directory}"
fi

NONPROXY_RESTRICTED_SIGNING=1 \
    "${script_dir}/verify-system-extension-bundle.sh" "${app_bundle}"

extension_bundle="${app_bundle}/Contents/PlugIns/NonProxySafariWebExtension.appex"
signature_details="${evidence_directory}/codesign.txt"
codesign -d --verbose=4 \
    "${extension_bundle}" >"${signature_details}" 2>&1
team_identifier=$(awk -F= '/^TeamIdentifier=/{print $2}' "${signature_details}")
if [[ -z "${team_identifier}" ||
      "${team_identifier}" == "not set" ]] ||
    ! grep -F "Authority=" "${signature_details}" >/dev/null; then
    echo "Safari 真实验收拒绝临时签名或缺少证书链的扩展" >&2
    exit 66
fi

profile_directory=$(mktemp -d)
cleanup() {
    rm -r "${profile_directory}"
}
trap cleanup EXIT
decoded_profile="${profile_directory}/safari-profile.plist"
security cms -D \
    -i "${extension_bundle}/Contents/embedded.provisionprofile" \
    >"${decoded_profile}"
profile_team=$(
    /usr/libexec/PlistBuddy \
        -c "Print :TeamIdentifier:0" \
        "${decoded_profile}"
)
application_identifier=$(
    /usr/libexec/PlistBuddy \
        -c "Print :Entitlements:application-identifier" \
        "${decoded_profile}"
)
profile_expiration=$(
    plutil -extract ExpirationDate raw -o - "${decoded_profile}"
)
if [[ "${profile_team}" != "${team_identifier}" ||
      "${application_identifier}" != "${team_identifier}.${extension_identifier}" ||
      "${profile_expiration}" < "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" ]]; then
    echo "Safari 签名 Team、profile、Bundle ID 或有效期不一致" >&2
    exit 66
fi

/usr/bin/pluginkit -m -A -D -v \
    -i "${extension_identifier}" \
    >"${evidence_directory}/pluginkit.txt"
swift build \
    --package-path "${package_root}" \
    --disable-sandbox \
    --configuration release \
    --product NonProxySafariStateProbe >/dev/null
probe_bin=$(
    swift build \
        --package-path "${package_root}" \
        --configuration release \
        --show-bin-path
)
"${probe_bin}/NonProxySafariStateProbe" \
    "${extension_identifier}" \
    >"${evidence_directory}/safari-state.json"
plutil -lint "${evidence_directory}/safari-state.json" >/dev/null

app_plist="${app_bundle}/Contents/Info.plist"
bundle_version=$(
    /usr/libexec/PlistBuddy -c "Print :CFBundleVersion" "${app_plist}"
)
if [[ "${action}" == accept ]]; then
    if [[ "$(plutil -extract available raw -o - \
            "${evidence_directory}/safari-state.json")" != true ||
          "$(plutil -extract enabled raw -o - \
            "${evidence_directory}/safari-state.json")" != true ||
          ! -s "${evidence_directory}/pluginkit.txt" ]]; then
        echo "Safari 尚未登记或启用 NonProxy 扩展" >&2
        exit 68
    fi
    operator_evidence=${NONPROXY_SAFARI_OPERATOR_EVIDENCE:-}
    if [[ -z "${operator_evidence}" ||
          ! -f "${operator_evidence}" ||
          -L "${operator_evidence}" ]]; then
        echo "完整验收必须提供普通的人工浏览器证据 JSON" >&2
        exit 68
    fi
    node -e '
      const fs = require("node:fs");
      const value = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
      const required = [
        value.schemaVersion === 1,
        value.extensionIdentifier === process.argv[2],
        String(value.bundleVersion) === process.argv[3],
        typeof value.operator === "string" && value.operator.length > 0,
        typeof value.testedAtUtc === "string" && value.testedAtUtc.length > 0,
        value.normalProfile?.twoTabsIsolated === true,
        value.normalProfile?.confirmationCommitted === true,
        value.privateProfile?.explicitlyEnabled === true,
        value.privateProfile?.twoTabsIsolated === true,
        value.privateProfile?.confirmationCommitted === true,
        value.privacy?.domainOnlyObserved === true,
        value.privacy?.temporaryPermissionReleased === true,
      ];
      if (!required.every(Boolean)) process.exit(1);
    ' "${operator_evidence}" \
        "${extension_identifier}" \
        "${bundle_version}" ||
        {
            echo "人工浏览器证据 JSON 缺少必需验收项" >&2
            exit 68
        }
    install -m 0600 \
        "${operator_evidence}" \
        "${evidence_directory}/operator-evidence.json"
fi

manifest="${evidence_directory}/manifest.plist"
plutil -create xml1 "${manifest}"
plutil -insert schemaVersion -integer 1 "${manifest}"
plutil -insert action -string "${action}" "${manifest}"
plutil -insert appBundle -string "${app_bundle}" "${manifest}"
plutil -insert extensionIdentifier -string \
    "${extension_identifier}" "${manifest}"
plutil -insert bundleVersion -string "${bundle_version}" "${manifest}"
plutil -insert teamIdentifier -string "${team_identifier}" "${manifest}"
plutil -insert extensionSha256 -string \
    "$(shasum -a 256 \
        "${extension_bundle}/Contents/MacOS/NonProxySafariWebExtension" |
        awk '{print $1}')" \
    "${manifest}"
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

echo "Safari 扩展 ${action} 验收证据已写入：${evidence_directory}"
