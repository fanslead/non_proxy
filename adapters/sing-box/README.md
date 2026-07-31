# sing-box 适配器

该模块生成兼容 sing-box 1.11+ 的 source rule-set version 3。主配置应把文件登记为
本地 `rule_set`，再将该规则集路由到实际的 direct outbound tag。

应用路径转换为转义后的 `process_path_regex` 前缀；输出不包含 route action，避免猜测
用户 direct outbound 的 tag。当前模块不会启动 GPLv3 的 sing-box，也不会修改用户
配置；`adapter-host` 使用用户已经安装的公开 CLI 执行 `sing-box rule-set compile`，并
要求成功生成非空、有界的二进制规则集后才允许准备变更。
