# ADR-0003：首发最低系统版本设为 macOS 15

- 状态：已接受
- 日期：2026-07-30

## 背景

首发需要长期维护的异步 UDS gRPC 客户端和 macOS 15 的 `Network.framework` flow endpoint API。当前采用的 gRPC Swift 2 生成接口从 macOS 15 开始可用：

- [gRPC Swift 2](https://github.com/grpc/grpc-swift-2)
- [Network Extension](https://developer.apple.com/documentation/networkextension)

继续支持 macOS 14 需要停留在维护模式的 gRPC Swift 1，或同时维护一套旧 endpoint 适配层。两者都会扩大高权限数据面的兼容矩阵。

## 决策

首发最低版本设为 Apple Silicon + macOS 15。macOS 26 是当前主要开发和验收目标；macOS 15 必须有独立真实设备回归。

不得仅修改 Swift Package 的 deployment target 来宣称兼容。将来恢复 macOS 14 支持时，必须新增 ADR，并补齐 Provider、安装、升级、DNS、睡眠唤醒和真实路径矩阵。

## 结果

- 可以使用维护中的 gRPC Swift 2 和非弃用的 flow endpoint API。
- 减少首发 System Extension 的双 API 分支。
- macOS 14 用户不在首发支持范围内，安装器和产品页面必须明确拒绝而不是安装后失败。
