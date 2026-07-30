# Safari Web Extension 正式验收

本手册验证正式签名的 `NonProxySafariWebExtension.appex` 已被 Safari 登记、启用，并在普通窗口与无痕窗口中保持标签页隔离和最小化数据采集。临时签名、单元测试、静态 `.appex` 校验或转换器零告警都不能替代这项真实验收。

## 前置条件

1. 使用与宿主一致的 Team 签名 `NonProxy.app`、两个 Network Extension 和 Safari Web Extension，并分别嵌入匹配 Bundle ID 的 provisioning profile。
2. 将待验收版本安装到 `/Applications/NonProxy.app` 并启动一次。
3. 在 Safari 的“设置 > 扩展”中启用“NonProxy 智能直连”。
4. 在 Safari 17 或更高版本中，为当前测试 Profile 单独确认扩展权限；无痕窗口必须由测试人员明确允许，不能由脚本绕过系统设置。
5. 启动 NonProxy 系统组件，确认本地 `gatewayd` 已就绪。

## 只读状态采集

```bash
mkdir -p "$PWD/acceptance"
./scripts/macos/safari-extension-e2e.sh \
  /Applications/NonProxy.app \
  query \
  "$PWD/acceptance/safari-query"
```

`query` 会拒绝临时签名、错误 Team、过期或不匹配的 profile，并记录 `pluginkit` 与 `SFSafariExtensionManager` 的权威状态。它不会修改 Safari 设置；`available=false` 或 `enabled=false` 会保留在证据中，不能当作通过。

## 普通窗口验收

1. 在两个标签页分别打开不同站点。
2. 仅在第一个标签页主动开始识别，确认第二个标签页不出现活动会话。
3. 停止识别，确认临时站点权限已立即回收。
4. 审核候选，主站必须保持选中；选择一个可信关联域名并确认。
5. 在桌面端确认只新增所选精确域名规则，未选域名没有写入。

## 无痕窗口验收

1. 在 Safari 设置中明确允许该扩展用于无痕浏览。
2. 新建无痕窗口，在两个标签页重复普通窗口的隔离、停止、审核与确认步骤。
3. 确认普通窗口与无痕窗口不共享正在进行的标签页会话。
4. 检查本地日志或抓取的控制面事件，只允许出现随机浏览器上下文、规范化域名、资源枚举和计数；不得出现完整 URL、查询参数、页面标题、Cookie 或正文。

## 人工证据

把下列模板保存到独立 JSON 文件，填写真实测试人员和 UTC 时间。所有布尔项都必须来自本轮实际操作，禁止预填或仅凭自动化测试推断。

```json
{
  "schemaVersion": 1,
  "extensionIdentifier": "com.nonproxy.desktop.safari-web-extension",
  "bundleVersion": "1",
  "operator": "待填写",
  "testedAtUtc": "2026-07-31T00:00:00Z",
  "normalProfile": {
    "twoTabsIsolated": true,
    "confirmationCommitted": true
  },
  "privateProfile": {
    "explicitlyEnabled": true,
    "twoTabsIsolated": true,
    "confirmationCommitted": true
  },
  "privacy": {
    "domainOnlyObserved": true,
    "temporaryPermissionReleased": true
  }
}
```

执行最终验收：

```bash
NONPROXY_SAFARI_OPERATOR_EVIDENCE="$PWD/safari-operator-evidence.json" \
  ./scripts/macos/safari-extension-e2e.sh \
  /Applications/NonProxy.app \
  accept \
  "$PWD/acceptance/safari-accept"
```

输出目录包含扩展签名、Safari 登记状态、启用状态、人工证据和绑定扩展二进制 SHA-256 的清单。只有 `accept` 成功退出才表示本版本完成签名 Safari 生命周期验收。
