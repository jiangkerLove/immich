# AGENTS.md

本仓库是 Immich fork，正在将 `server/`（TypeScript）迁移到 `rust-server/`（Rust）。

**Cursor 会自动加载以下上下文（无需手动 @ 文件）：**

| 类型 | 路径 | 作用 |
|------|------|------|
| 项目规则（始终生效） | `.cursor/rules/project-context.mdc` | 分支模型、代码地图、迁移摘要 |
| 工作流规则（始终生效） | `.cursor/rules/rust-migration-workflow.mdc` | PR 流程、测试、暂缓项 |
| rust-server 规则 | `.cursor/rules/rust-server.mdc` | 编辑 `rust-server/**` 时自动附加 |
| TS 参考规则 | `.cursor/rules/typescript-server-reference.mdc` | 编辑 `server/**` 时自动附加 |
| **完整迁移清单** | `.cursor/docs/migration-checklist.md` | 已完成 / 待办 / 路线图（规划时阅读） |

## 快速参考

- **开发主线**：`dev-rust`（rust-dev）
- **上游同步**：`main`
- **临时分支**：`cursor/<name>-4063` → PR 到 `dev-rust` → 合并后删除
- **测试**：`cd rust-server && cargo +stable test --offline --lib`
- **不要提交**：`Cargo.lock`

## 新会话建议

1. 做迁移或规划 → 读 `.cursor/docs/migration-checklist.md`（先看 §0 切流结论、§3 Cutover）
2. 做具体 parity 切片 → 对照 `server/src/services/` 与 `rust-server/src/service/`
3. 当前优先是切流验证（冒烟 / schema / 维护重启），不是再补 HTTP/Job 面
4. 可在同一分支上连续积累多批改动再合并

## 术语

| 说法 | 分支 |
|------|------|
| rust-dev | `dev-rust` |
| 上游 / 源头 | `main` |
