# 出口探针生产部署与密钥轮换

本目录部署的是独立公网观察点。它必须在公网主机上直接终止 TLS，并从 TCP peer
address 取得来源地址。不要放在会终止 TLS、SNAT 或隐藏客户端来源的 L7 反向代理、
CDN 或 Service Mesh 之后。

## 1. 构建与安装

目标机基线为支持本目录全部沙箱指令的 systemd 252 或更高版本。先在目标发行版
执行 `systemd-analyze verify deploy/probe-server/nonproxy-probe-server.service`；
任何 unknown lvalue、缺失二进制或权限告警都必须在安装前解决，不能静默忽略。

在经过审查的发布源码上构建：

```bash
source ./scripts/bootstrap/env.sh
cargo build --locked --release \
  -p nonproxy-probe-server \
  -p nonproxy-probe-admin
```

在目标 Linux 主机创建不可登录的独立账户并安装只读二进制：

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin nonproxy-probe
sudo install -d -o root -g nonproxy-probe -m 0750 /etc/nonproxy/probe-server
sudo install -o root -g root -m 0755 \
  target/release/nonproxy-probe-server \
  /usr/local/libexec/nonproxy-probe-server
sudo install -o root -g root -m 0755 \
  target/release/nonproxy-probe-admin \
  /usr/local/libexec/nonproxy-probe-admin
sudo install -o root -g root -m 0644 \
  deploy/probe-server/nonproxy-probe-server.service \
  /etc/systemd/system/nonproxy-probe-server.service
sudo install -o root -g root -m 0644 \
  deploy/probe-server/nonproxy-probe-server.env.example \
  /etc/nonproxy/probe-server.env
```

TLS 证书链可以是 `0644 root:root`；TLS 私钥和 Ed25519 签名私钥必须是普通文件、
不得为符号链接，且不得有 group/world 权限。服务账户必须能读取私钥：

```bash
sudo install -o root -g root -m 0644 fullchain.pem \
  /etc/nonproxy/probe-server/tls-chain.pem
sudo install -o nonproxy-probe -g nonproxy-probe -m 0600 privkey.pem \
  /etc/nonproxy/probe-server/tls-key.pem
sudo /usr/local/libexec/nonproxy-probe-admin \
  keygen --output /etc/nonproxy/probe-server/signing-key-v1.bin
sudo chown nonproxy-probe:nonproxy-probe \
  /etc/nonproxy/probe-server/signing-key-v1.bin
sudo chmod 0600 /etc/nonproxy/probe-server/signing-key-v1.bin
```

`keygen` 只输出 `key_id`、公钥和私钥文件路径，不输出私钥内容；目标文件已存在
或路径含符号链接时拒绝写入。把输出的 `public_key` 作为客户端发布配置，不要
复制 `signing-key-*.bin`。配置目录保持 `root` 所有且不可由服务账号创建新文件；
只有生成后的单个私钥文件移交给服务账号，运行时 systemd 再把整个目录挂为只读。

检查 `/etc/nonproxy/probe-server.env` 的域名证书路径、签名密钥版本和连接上限后：

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now nonproxy-probe-server.service
sudo systemctl status --no-pager nonproxy-probe-server.service
sudo systemd-analyze security nonproxy-probe-server.service
```

生产防火墙只开放 TCP 443。若不能让进程直接监听 443，只能使用保留原始 TCP peer
address 的同机 L4 转发；上线前必须用外部网络确认探针观察到的不是转发器地址。

## 2. 上线验收

健康检查验证 TLS、进程和当前非敏感签名 key id：

```bash
curl --fail --silent --show-error https://probe.example/health
```

在独立外部主机验证 HTTPS、随机 nonce、Ed25519 签名、公网地址和回执时间：

```bash
nonproxy-probe-admin verify \
  --endpoint https://probe.example/v1/exit \
  --public-keys '<public_key>'
```

该命令只证明探针服务本身及运行命令的普通系统路径，不证明 NonProxy 的物理
DIRECT 或指定 PROXY。最终验收仍需在桌面端分别点击“验证直连出口”和目标代理的
“验证出口”，比较两份新生成的签名回执。

## 3. 零停机签名密钥轮换

轮换必须按以下顺序执行，不能先切服务端：

1. 按第 1 节相同的 root 创建、移交所有权和 `0600` 步骤生成
   `signing-key-v2.bin`，记录新 `key_id` 和 `public_key`。
2. 发布客户端信任集合
   `NONPROXY_EXIT_PROBE_PUBLIC_KEYS=<old_public_key>,<new_public_key>`；确认目标版本
   已覆盖所需客户端，且旧签名仍能验证。
3. 把服务端环境文件的 `NONPROXY_PROBE_SIGNING_KEY` 改为 v2 的绝对路径，执行
   `systemctl restart nonproxy-probe-server`。
4. 检查 `/health` 返回新 `key_id`，再用同时包含 old/new 的管理工具和桌面端完成
   一次新回执验证。
5. 经过预定兼容窗口后，从客户端集合移除旧公钥；确认旧版本退出支持范围后，才
   离线归档或销毁 v1 私钥。

客户端最多接受 4 把不重复公钥。旧版单值
`NONPROXY_EXIT_PROBE_PUBLIC_KEY=<key>` 继续兼容，但不能与复数变量同时配置。

## 4. 回滚

- 若 v2 服务启动失败：恢复环境文件中的 v1 路径并重启；客户端此时同时信任两把
  公钥，不需要回滚客户端。
- 若 v2 签名无法被已发布客户端验证：立即恢复 v1 服务端密钥，保留双钥客户端，
  修正分发范围后重新执行第 3 步。
- 不得用覆盖文件的方式回滚私钥；每个版本使用独立、权限固定的普通文件。
- TLS 证书轮换与 Ed25519 轮换分开执行，避免一次变更同时失去 TLS 和回执证据。

## 5. 备份、日志与隐私

- 签名私钥只进入受控秘密备份，不进入 Git、镜像层、普通配置库或日志。
- 探针不记录请求、nonce 或公网地址；systemd journal 只保留启动/失败状态。
- `/health` 只公开状态和 key id；`/v1/exit` 只返回本次随机 nonce 对应的签名回执。
- 恢复演练必须在隔离主机验证私钥文件权限、服务启动、key id 和真实签名回执。
