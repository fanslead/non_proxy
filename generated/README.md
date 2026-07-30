# 生成代码

本目录只保存由固定工具版本生成的跨语言契约代码，禁止人工编辑。

- `generated/csharp/`：由 `buf.gen.yaml` 和仓库内固定版本 `protoc` 生成。
- Rust、Swift 和 TypeScript 生成器会在对应消费项目建立时加入同一个 `just generate` 门禁。

修改 `proto/` 后运行：

```bash
source ./scripts/bootstrap/env.sh
just generate
just contracts
```

CI 会重新生成并检查工作区差异。生成代码中的第三方模板注释不受人工代码注释语言规则约束。
