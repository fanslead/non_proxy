#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
用法：./scripts/macos/run-development.sh [--smoke] [--state-directory <绝对路径>]

默认启动 gatewayd、adapter-host 和 macOS 桌面端。
--smoke 只验证两个用户态服务就绪后退出，不打开桌面端。
EOF
}

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${script_dir}/../.." && pwd)
state_directory="${repo_root}/.artifacts/np-dev"
mode=desktop

while [[ $# -gt 0 ]]; do
    case "$1" in
        --smoke)
            mode=smoke
            shift
            ;;
        --state-directory)
            if [[ $# -lt 2 ]]; then
                usage
                exit 64
            fi
            state_directory=${2%/}
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            usage
            exit 64
            ;;
    esac
done

if [[ $(uname -s) != Darwin ]]; then
    echo "macOS 用户态开发模式只能在 macOS 主机运行。" >&2
    exit 69
fi
if [[ "${state_directory}" != /* ]]; then
    echo "开发状态目录必须使用绝对路径：${state_directory}" >&2
    exit 64
fi
if [[ -L "${state_directory}" ||
      ( -e "${state_directory}" && ! -d "${state_directory}" ) ]]; then
    echo "开发状态目录不能是符号链接或普通文件：${state_directory}" >&2
    exit 65
fi

adapter_state_directory="${state_directory}/adapter"
if [[ -L "${adapter_state_directory}" ||
      ( -e "${adapter_state_directory}" && ! -d "${adapter_state_directory}" ) ]]; then
    echo "Adapter 开发状态目录不能是符号链接或普通文件：${adapter_state_directory}" >&2
    exit 65
fi

unix_socket_path_max_bytes=103
validate_unix_socket_path() {
    local socket_path=$1
    local socket_path_bytes
    socket_path_bytes=$(printf '%s' "${socket_path}" | LC_ALL=C wc -c | tr -d '[:space:]')
    if (( socket_path_bytes > unix_socket_path_max_bytes )); then
        echo "Unix Socket 路径超过 macOS 103 字节上限（${socket_path_bytes} 字节）：${socket_path}" >&2
        echo "请通过 --state-directory 选择更短的绝对路径。" >&2
        exit 64
    fi
}

for socket_path in \
    "${state_directory}/gatewayd.sock" \
    "${state_directory}/gatewayd-flow.sock" \
    "${adapter_state_directory}/adapter-host.sock"; do
    validate_unix_socket_path "${socket_path}"
done

source "${repo_root}/scripts/bootstrap/env.sh"
"${repo_root}/scripts/bootstrap/check-prerequisites.sh"

dotnet restore "${repo_root}/apps/desktop/NonProxy.Desktop.slnx" \
    --locked-mode \
    -p:Configuration=Release
pnpm --dir "${repo_root}" install --frozen-lockfile
dotnet build \
    "${repo_root}/apps/desktop/NonProxy.Desktop.Mac/NonProxy.Desktop.Mac.csproj" \
    --configuration Debug \
    --no-restore \
    --no-incremental

native_rid=$(dotnet msbuild \
    "${repo_root}/apps/desktop/NonProxy.Desktop.Mac/NonProxy.Desktop.Mac.csproj" \
    -getProperty:NETCoreSdkRuntimeIdentifier)
app_bundle="${repo_root}/apps/desktop/NonProxy.Desktop.Mac/bin/Debug/net10.0-macos/${native_rid}/NonProxy.app"
gateway_binary="${app_bundle}/Contents/Resources/nonproxy-gatewayd"
adapter_binary="${app_bundle}/Contents/Resources/nonproxy-adapter-host"
host_binary="${app_bundle}/Contents/MacOS/NonProxy.Desktop.Mac"
for binary in "${gateway_binary}" "${adapter_binary}" "${host_binary}"; do
    if [[ ! -x "${binary}" ]]; then
        echo "开发构建缺少可执行文件：${binary}" >&2
        exit 65
    fi
done

mkdir -p "${state_directory}"
chmod 0700 "${state_directory}"
run_logs=$(mktemp -d "${state_directory}/run-logs.XXXXXX")
gateway_log="${run_logs}/gatewayd.log"
adapter_log="${run_logs}/adapter-host.log"
gateway_pid=
adapter_pid=

cleanup() {
    local exit_code=$?
    trap - EXIT INT TERM
    for service_pid in "${adapter_pid}" "${gateway_pid}"; do
        if [[ -n "${service_pid}" ]] && kill -0 "${service_pid}" 2>/dev/null; then
            kill "${service_pid}" 2>/dev/null || true
            wait "${service_pid}" 2>/dev/null || true
        fi
    done
    if [[ ${exit_code} -ne 0 ]]; then
        echo "开发模式启动失败，日志保留在：${run_logs}" >&2
    fi
    exit "${exit_code}"
}
trap cleanup EXIT INT TERM

NONPROXY_STATE_DIR="${state_directory}" \
    "${gateway_binary}" >"${gateway_log}" 2>&1 &
gateway_pid=$!

for _attempt in {1..100}; do
    if [[ -S "${state_directory}/gatewayd.sock" &&
          -S "${state_directory}/gatewayd-flow.sock" &&
          -f "${state_directory}/session.capability" &&
          -f "${state_directory}/provider.capability" &&
          -f "${state_directory}/gateway.runtime.json" ]]; then
        break
    fi
    if ! kill -0 "${gateway_pid}" 2>/dev/null; then
        sed -n '1,240p' "${gateway_log}" >&2
        exit 1
    fi
    sleep 0.05
done

if [[ ! -S "${state_directory}/gatewayd.sock" ||
      ! -S "${state_directory}/gatewayd-flow.sock" ||
      ! -f "${state_directory}/session.capability" ||
      ! -f "${state_directory}/provider.capability" ||
      ! -f "${state_directory}/gateway.runtime.json" ]]; then
    sed -n '1,240p' "${gateway_log}" >&2
    echo "gatewayd 未在限定时间内就绪。" >&2
    exit 1
fi

mkdir -p "${adapter_state_directory}"
chmod 0700 "${adapter_state_directory}"
NONPROXY_ADAPTER_STATE_DIR="${adapter_state_directory}" \
    "${adapter_binary}" >"${adapter_log}" 2>&1 &
adapter_pid=$!

for _attempt in {1..100}; do
    if [[ -S "${adapter_state_directory}/adapter-host.sock" &&
          -f "${adapter_state_directory}/adapter.capability" &&
          -f "${adapter_state_directory}/adapter.runtime.json" ]]; then
        break
    fi
    if ! kill -0 "${adapter_pid}" 2>/dev/null; then
        sed -n '1,240p' "${adapter_log}" >&2
        exit 1
    fi
    sleep 0.05
done

if [[ ! -S "${adapter_state_directory}/adapter-host.sock" ||
      ! -f "${adapter_state_directory}/adapter.capability" ||
      ! -f "${adapter_state_directory}/adapter.runtime.json" ]]; then
    sed -n '1,240p' "${adapter_log}" >&2
    echo "adapter-host 未在限定时间内就绪。" >&2
    exit 1
fi

cat <<EOF
NonProxy 用户态开发模式已就绪：
- 控制服务：${state_directory}/gatewayd.sock
- Adapter：${adapter_state_directory}/adapter-host.sock
- 状态目录：${state_directory}
- 运行日志：${run_logs}

此模式不会登记 System Extension，也不会接管系统流量。
它用于测试桌面 UI、控制面、规则、出口、订阅和客户端协同。
EOF

if [[ "${mode}" == smoke ]]; then
    exit 0
fi

NONPROXY_STATE_DIR="${state_directory}" \
NONPROXY_ADAPTER_STATE_DIR="${adapter_state_directory}" \
    "${host_binary}"
