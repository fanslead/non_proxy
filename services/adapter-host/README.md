# nonproxy-adapter-host

`adapter-host` 是第三方客户端适配器的独立低权限进程。它只接受用户明确登记的客户端
路径，使用私有本地 RPC 和独立会话能力文件，负责版本检测、能力降级和可恢复双文件事务。
安装项同时绑定主配置、受管 sidecar 和可选直连出口。候选在写入事务状态前必须通过客户端
原生工具校验：Surge 对完整候选使用随包 `surge-cli -c`，Mihomo 使用 `-t`，sing-box 同时
执行 `rule-set compile` 和 `check -c`。

macOS 包把它作为独立签名二进制和 LaunchAgent plist 嵌入，并用绑定二进制与模板的
包指纹生成私有运行身份。桌面桥把它作为第二个独立后台项目登记、检查、升级和卸载；
包级冒烟覆盖启动、权限和优雅退出，真实系统审批仍由系统生命周期门禁验收。

prepare 不通过 RPC 返回主配置正文；apply 使用受哈希保护的协调事务同时写入 sidecar 与
主配置，并在检测到外部修改时失败关闭。当前服务尚未执行客户端公开重载或真实路径验证，
因此 `reloaded` 与 `path_verified` 仍为 false，桌面端不得显示“已接管”。
