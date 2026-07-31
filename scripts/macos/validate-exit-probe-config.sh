#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "用法：$0 <endpoint-or-empty> <public-keys-or-empty>" >&2
    exit 64
fi

exit_probe_endpoint=$1
exit_probe_public_keys=$2
if [[ -z "${exit_probe_endpoint}" &&
      -z "${exit_probe_public_keys}" ]]; then
    exit 0
fi
if [[ -z "${exit_probe_endpoint}" ||
      -z "${exit_probe_public_keys}" ]]; then
    echo "出口探针 endpoint 与公钥集合必须成对" >&2
    exit 65
fi

exit_probe_endpoint_pattern='^https://[A-Za-z0-9.-]+(:[0-9]{1,5})?(/[A-Za-z0-9._~/%+-]*)?$'
if ! [[ "${exit_probe_endpoint}" =~ ${exit_probe_endpoint_pattern} ]]; then
    echo "出口探针 endpoint 必须是规范的 HTTPS 域名地址" >&2
    exit 65
fi

IFS=',' read -r -a exit_probe_keys <<<"${exit_probe_public_keys}"
if [[ ${#exit_probe_keys[@]} -lt 1 ||
      ${#exit_probe_keys[@]} -gt 4 ]]; then
    echo "出口探针公钥集合必须包含 1 到 4 把公钥" >&2
    exit 65
fi

exit_probe_seen_keys=,
for exit_probe_key in "${exit_probe_keys[@]}"; do
    if [[ ${#exit_probe_key} -ne 43 ||
          "${exit_probe_key}" == *[^A-Za-z0-9_-]* ]]; then
        echo "出口探针公钥必须是 43 位 base64url" >&2
        exit 65
    fi
    if [[ "${exit_probe_seen_keys}" == *",${exit_probe_key},"* ]]; then
        echo "出口探针公钥集合不能包含重复项" >&2
        exit 65
    fi
    exit_probe_seen_keys+="${exit_probe_key},"
done
