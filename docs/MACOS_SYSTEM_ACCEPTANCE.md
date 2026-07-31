# macOS 系统组件验收手册

本文只用于真实 `SMAppService`、System Extension 和 Network Extension
生命周期验收。编译、临时签名、直接运行包内后台二进制、单元测试和跨语言
Unix Socket 冒烟都不能替代本流程。

## 1. 前置条件

- 目标电脑运行 macOS 15 或更高版本。
- App 使用同一 Team 下有效的宿主、Transparent Proxy、DNS Proxy 和
  Safari Web Extension provisioning profile 签名。
- App 已由签名安装器或 Finder 放入 `/Applications`，不能从构建目录运行。
- 测试人员可以在“系统设置 → 通用 → 登录项与扩展”完成系统审批。
- 执行安装、升级、卸载或完整生命周期测试前，先确认允许短暂改变本机网络
  接管状态。
- 证据目录的父目录必须预先创建且不能位于 `/Applications`；每次动作使用新的
  空子目录。

发布候选还必须使用 `Developer ID Application`、通过 Gatekeeper，并附有
可验证的公证票据。开发签名可以验证系统生命周期，但不能作为发布验收。

## 2. 只读查询

```bash
mkdir -p artifacts/macos-system-e2e

./scripts/macos/system-lifecycle-e2e.sh \
  /Applications/NonProxy.app \
  query \
  artifacts/macos-system-e2e/query-001
```

查询不写系统状态。证据目录必须为空，脚本不会覆盖已有验收记录。

## 3. 安装与完整生命周期

安装并复查所有组件：

```bash
NONPROXY_ALLOW_SYSTEM_MUTATION=1 \
./scripts/macos/system-lifecycle-e2e.sh \
  /Applications/NonProxy.app \
  install \
  artifacts/macos-system-e2e/install-001
```

安装、复查、卸载、再次复查：

```bash
NONPROXY_ALLOW_SYSTEM_MUTATION=1 \
./scripts/macos/system-lifecycle-e2e.sh \
  /Applications/NonProxy.app \
  lifecycle \
  artifacts/macos-system-e2e/lifecycle-001
```

首次执行可能返回等待审批。完成系统设置审批后使用新的空证据目录重跑。
若系统要求重启，脚本返回 `69` 并保留证据；重启后先执行只读查询，再继续相应
动作。需要重启的中间态不能记录为通过。

## 4. 后台服务升级

升级动作验证旧 `gatewayd` 或 `adapter-host` 被当前包识别并按安全顺序替换：

1. 安装并启用旧的正式签名版本。
2. 退出 UI，确认后台服务仍处于已登记状态。
3. 使用签名安装器把 `/Applications/NonProxy.app` 替换为新版本；验收脚本
   本身不会覆盖 App。
4. 运行：

```bash
NONPROXY_ALLOW_SYSTEM_MUTATION=1 \
./scripts/macos/system-lifecycle-e2e.sh \
  /Applications/NonProxy.app \
  upgrade \
  artifacts/macos-system-e2e/upgrade-001
```

升级前查询必须至少出现 `gatewayAgent.requiresUpgrade=true` 或
`adapterHostAgent.requiresUpgrade=true`。随后脚本验证网络偏好已由产品事务
安全撤销、两个后台服务完成登记、当前包运行身份匹配，两个扩展和
两份网络偏好重新就绪。若前置状态不是旧版本，升级验收会拒绝制造假阳性。

## 5. 发布候选验收

对 Developer ID 发布候选增加严格门禁：

```bash
NONPROXY_REQUIRE_DEVELOPER_ID=1 \
./scripts/macos/system-lifecycle-e2e.sh \
  /Applications/NonProxy.app \
  query \
  artifacts/macos-system-e2e/release-query-001
```

该模式额外执行 Gatekeeper 评估和公证票据校验。安装、升级或卸载时仍需同时
设置 `NONPROXY_ALLOW_SYSTEM_MUTATION=1`。

## 6. 证据与通过标准

每次执行产生：

- `manifest.json`：动作、Bundle 版本、TeamIdentifier、宿主及两个后台服务的 SHA-256 和时间。
- `codesign.txt`：代码签名详情。
- 每一步的 UTF-8 JSON 终态及独立标准错误输出。
- 发布候选的 `gatekeeper.txt` 和 `notarization.txt`。
- `SHA256SUMS`：本次证据文件的完整性摘要。

安装通过必须同时满足：

- 当前包指纹的 `gatewayd` 运行身份就绪。
- 当前包指纹的 `adapter-host` 运行身份就绪。
- Transparent Proxy 与 DNS Proxy 均已启用。
- 两份 Network Extension 偏好均已启用。
- 操作不要求待处理重启。

卸载通过必须同时满足两个后台项目未登记、两个扩展未安装、两份偏好未配置。

本流程仍不证明 DIRECT 流量绕过任意第三方 VPN。VPN 共存需要另行记录
测试矩阵、出口 IP/接口路径和失败策略证据。

Safari 扩展的登记、启用、普通/无痕窗口多标签页隔离与隐私验收使用
[Safari Web Extension 正式验收](SAFARI_EXTENSION_ACCEPTANCE.md)。系统组件
生命周期通过不能自动推导 Safari 已启用或允许无痕浏览。
