# AGENTS.md

本仓库是 Immich fork，正在将 `server/`（TypeScript）迁移到 `rust-server/`（Rust）。

**Cursor 会自动加载以下上下文（无需手动 @ 文件）：**

| 类型 | 路径 | 作用 |
|------|------|------|
| 项目规则（始终生效） | `.cursor/rules/project-context.mdc` | 分支模型、代码地图 |
| 工作流规则（始终生效） | `.cursor/rules/rust-migration-workflow.mdc` | PR 流程、测试约定 |
| rust-server 规则 | `.cursor/rules/rust-server.mdc` | 编辑 `rust-server/**` 时自动附加 |
| TS 参考规则 | `.cursor/rules/typescript-server-reference.mdc` | 编辑 `server/**` 时自动附加 |
| **进度 / 计划 / 切流（唯一）** | `.cursor/docs/migration-checklist.md` | 已完成 / Cutover / P4 / 暂缓 / 路线图 |

## 快速参考

- **开发主线**：`dev-rust`（rust-dev）
- **上游同步**：`main`
- **临时分支**：`cursor/<name>-4063` → PR 到 `dev-rust` → 合并后删除
- **测试**：`cd rust-server && cargo +stable test --offline --lib`
- **不要提交**：`Cargo.lock`
- **进度与计划**：只改 / 只读 `migration-checklist.md`，不要在 rules 里重复维护

## 新会话建议

1. 做迁移或规划 → 读 `.cursor/docs/migration-checklist.md`
2. 做具体 parity 切片 → 对照 `server/src/services/` 与 `rust-server/src/service/`
3. 可在同一分支上连续积累多批改动再合并

## 术语

| 说法 | 分支 |
|------|------|
| rust-dev | `dev-rust` |
| 上游 / 源头 | `main` |
