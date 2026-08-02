# 开发预览版发布

本文说明 NonProxy `0.0.x` 开发预览版的构建、签名和发布边界。这里的开发签名只用于让
维护者和测试人员识别产物来源，不替代 Apple Developer ID、公证或 Microsoft 内核签名。

## 1. 产物类型

| 平台 | 产物 | 签名 | 可验证范围 |
|---|---|---|---|
| macOS | Universal DMG | 本机 Apple Development | App、嵌套二进制和 DMG 完整性 |
| Windows x64 | ZIP 安装目录 | 每次构建生成的自签名开发证书 | 普通二进制、PowerShell、Driver Catalog 成员关系 |
| Windows ARM64 | ZIP 安装目录 | 每次构建生成的自签名开发证书 | 普通二进制、PowerShell、Driver Catalog 成员关系 |

开发预览版必须作为 GitHub **Pre-release** 发布，资产名必须包含 `development`。发布说明和
包内说明都要保留平台限制，不能把开发签名产物描述为面向普通用户的正式安装包。

## 2. macOS 开发签名 DMG

构建机必须已经配置有效的 Apple Development 代码签名身份。运行：

```bash
./scripts/macos/build-development-dmg.sh \
  0.0.1 \
  "Apple Development: account@example.com (TEAMID)"
```

脚本会执行 Release Universal 构建、签名嵌套 System Extension、Safari Extension、后台服务
和宿主，验证 App Bundle 后生成并签名 DMG，同时写出 `.sha256` 文件。

如果没有 NonProxy 对应的 Provisioning Profile，此产物只能验证 UI、Bundle 结构和签名来源。
Transparent Proxy、DNS Proxy、Safari Web Extension 等受限权限不会因此获得可激活资格；
也没有 Developer ID、公证或 Gatekeeper 面向普通用户的分发保证。

## 3. Windows 开发签名包

Windows 包由手动 GitHub Actions 工作流
`.github/workflows/development-release.yml` 构建。工作流分别构建 x64 和 ARM64，并为每个架构
创建 90 天、私钥不可导出的临时自签名证书。公开证书会随包分发，私钥只存在于当次临时
Runner 的用户证书库，不得导出或上传。

本地 Windows 构建命令：

```powershell
.\scripts\bootstrap\install-protoc-windows.ps1
.\scripts\windows\build-development-release.ps1 `
  -Version 0.0.1 `
  -Architecture x64
```

包内 `development/README.txt` 是安装前必读说明。测试人员必须在隔离测试机或虚拟机中，
以管理员 PowerShell 显式执行：

```powershell
.\development\Install-Development-Certificate.ps1 `
  -ConfirmDevelopmentCertificateTrust `
  -EnableTestSigning `
  -ConfirmTestSigning
```

然后重启 Windows。启用了 Secure Boot 的机器通常会拒绝开启 Test Signing。开发预览版驱动
没有 Microsoft Hardware Dev Center 签名，因此不能用于生产环境，也不能通过 `/kp` 生产
内核策略验签。

测试结束后可移除开发证书：

```powershell
.\development\Install-Development-Certificate.ps1 `
  -Action Remove `
  -ConfirmDevelopmentCertificateTrust
```

脚本不会自动关闭 Test Signing。确认机器没有其他测试驱动依赖后，再手动执行
`bcdedit.exe /set testsigning off` 并重启。

## 4. CI 环境

- macOS CI 固定使用 Runner 已安装的 Xcode 26.6；若镜像中的 Xcode Toolchain 缺少
  `install_name_tool` 入口，则链接同镜像 Command Line Tools 提供的系统工具后再构建。
- Windows Driver CI 使用 `windows-2025-vs2026` 的 Driver Kit 构建组件，以及固定版本
  WDK/SDK NuGet 包中的头文件、库和主机构建工具。
- Windows Rust 构建使用仓库固定版本 Protobuf Compiler，并校验官方压缩包 SHA-256。
- WDK、Protobuf、Rust、.NET 版本都由仓库文件固定，不从未版本化的系统默认值推断。

## 5. 发布检查

创建 GitHub Pre-release 前至少确认：

1. `just check` 通过。
2. GitHub `持续集成` 全部 Job 通过。
3. Windows 两个开发签名包工作流通过，并下载复核 ZIP 与 `.sha256`。
4. macOS DMG 通过 `codesign --verify` 和 `hdiutil verify`。
5. 发布资产 SHA-256 与发布说明一致。
6. 发布说明明确列出未公证、无正式 Provisioning Profile、Windows Test Signing 等限制。

生产版本仍必须回到平台验收文档规定的正式签名链路：

- [macOS 系统组件验收](MACOS_SYSTEM_ACCEPTANCE.md)
- [Windows 系统组件与真实网络路径验收](WINDOWS_SYSTEM_ACCEPTANCE.md)
