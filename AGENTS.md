# AGENTS.md — jiangkerLove/immich Rust 迁移工作规范

本文件供 Cursor Agent / Cloud Agent 在新会话中读取，描述本 fork 的分支策略、PR 流程与开发约定。
用户口中的 **rust-dev** 对应仓库分支 **`dev-rust`**。

---

## 仓库目的

- 本仓库是 Immich 的 fork，正在将 TypeScript `server/` 迁移到 `rust-server/`。
- **不要**向上游 Immich 提 PR；所有 rust 迁移工作在本 fork 内完成。

---

## 分支模型（仅两条长期分支）

| 分支 | 用途 |
|------|------|
| **`main`** | 同步上游 Immich 源头项目（upstream） |
| **`dev-rust`** | Rust 版本开发主线；**所有** rust-server 迁移改动最终都合并到这里 |

### 规则

1. **所有功能/迁移 PR 的 base 分支必须是 `dev-rust`**，不要合到 `main`，也不要再创建长期存在的中间集成分支（例如已废弃的 `cursor/rust-server-sync-main-e063`）。
2. 从 `dev-rust` 拉临时功能分支开发，合并回 `dev-rust` 后**立即删除**该远程分支。
3. `main` 仅用于同步上游；将上游变更合入 rust 工作流时，应先合到 `dev-rust`（必要时通过 `main` → `dev-rust` 的同步 PR），而不是直接在 `main` 上做 rust 迁移。

### 临时功能分支命名

```
cursor/<kebab-case-description>-4063
```

- 必须以 `cursor/` 开头
- 必须以 `-4063` 结尾
- 使用小写字母和连字符

示例：`cursor/library-job-status-4063`

---

## PR 流程

1. `git fetch origin dev-rust && git checkout -b cursor/<name>-4063 origin/dev-rust`
2. 实现一小块 TS ↔ rust-server 行为对齐（保持 PR 小而聚焦）
3. 测试通过后 `git push -u origin cursor/<name>-4063`
4. 使用 **ManagePullRequest** 工具创建 PR（**不要**用 `gh` / `origin` CLI 创建或更新 PR）
   - `base_branch`: `dev-rust`
   - `branch_name`: 功能分支名
5. PR 合并到 `dev-rust` 后，删除远程功能分支：
   ```bash
   git push origin --delete cursor/<name>-4063
   ```

### PR 描述模板

遵循 `.github/pull_request_template.md`，至少包含：

- `## Description` — 做了什么、为何需要、对齐了哪段 TypeScript 行为
- `## How Has This Been Tested?` — 列出实际运行的测试命令
- `## Checklist:` — 勾选适用项
- LLM 使用说明（模板最后一节）

### PR 标签

- 新功能 / 迁移对齐：`changelog:feature`
- 明确是 bug 修复：`changelog:fix`（或仓库实际使用的 fix 标签）

---

## 开发与测试约定

### 测试

在 `rust-server/` 目录运行：

```bash
cargo +stable test --offline --lib
```

### 格式化

**只格式化本次编辑过的文件**，不要对整个 crate 跑 `cargo fmt`（会污染无关文件）：

```bash
rustfmt +stable --edition 2024 <edited-files...>
```

### 不要提交

- **`Cargo.lock`** — 不要纳入 PR

### 代码风格

- 最小化改动范围，不做无关重构
- 匹配周围代码的命名、类型、抽象层级
- 注释只解释非显而易见的业务/技术细节
- 仅在请求或确有行为价值时添加测试

---

## 迁移范围参考

### 已完成方向（示例，合入 `dev-rust` 后勿重复）

- Workflow 执行日志、AssetTagged 触发
- Plugin host functions（tags、HTTP）
- Library job 状态（Failed / Skipped、`wrap_status_job`）
- Background-task AssetDelete / VersionCheck 对齐
- 路径读权限检查（`R_OK`）、library validate、SidecarCheck
- Library scan queue（含 soft-deleted libraries）、path normalize

详见根目录 **`RUST_MIGRATION.md`**（完整迁移清单、差距与路线图）。

### 暂缓 / 不要塞进普通 PR 的大块工作

- ML / OCR / face / duplicate worker 大规模重写
- Search v3、sync protocol
- Public plugin `allowedHosts` API
- EXIF tags → AssetTagged（若未明确要求）
- AssetV1 write-null for description/lat/lon
- `PersonRecognized`（TS 中已注释）

优先做**小而可验证**的 TS ↔ rust 行为对齐切片。

### PR 合并节奏

- 可在同一 `cursor/*` 分支上**连续积累多批**迁移改动，不必每做一小点就合并
- 合并前确保 `cargo +stable test --offline --lib` 通过
- 合并到 `dev-rust` 后删除该功能分支

---

## 上游同步（main → dev-rust）

当需要把上游 `main` 的更新带入 rust 工作时：

1. 更新 `main`（从 upstream 同步）
2. 开 PR：`main` → `dev-rust`（或先把 upstream 变更合到 `main` 再合到 `dev-rust`）
3. 解决 `rust-server/` 与上游 `server/` 的冲突
4. 合并后继续在 `dev-rust` 上开新的 `cursor/*` 功能分支

---

## CI 说明

- Fork PR 上 **Docs Destroy** 等基础设施检查可能因 GHCR rate limit / 缺少 Cloudflare+Postgres 配置而失败，**通常与 rust 代码无关**，不作为合并阻塞项（除非该检查被设为 required 且持续失败）。
- 合并前以 `cargo test` 与相关功能 CI 为准。

---

## 快速检查清单（新会话启动时）

```bash
git fetch origin --prune
git branch -r   # 应主要为 origin/main、origin/dev-rust
git log --oneline origin/dev-rust -5
```

- [ ] 当前工作基于 `dev-rust`
- [ ] 新分支名符合 `cursor/<name>-4063`
- [ ] PR target 为 `dev-rust`
- [ ] 合并后删除远程 `cursor/*` 分支
- [ ] 不提交 `Cargo.lock`
- [ ] 只 rustfmt 编辑过的文件

---

## 术语对照

| 用户说法 | 仓库分支 |
|----------|----------|
| rust-dev / rust dev | `dev-rust` |
| 源头 / 上游 | `main`（同步 upstream） |
