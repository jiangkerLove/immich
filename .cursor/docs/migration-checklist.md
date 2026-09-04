# rust-server 迁移清单（jiangkerLove/immich）

> 本文档描述本 fork 将上游 TypeScript `server/` 迁移到 `rust-server/` 的进度与规划。  
> 目标：**尽可能对齐上游行为，后期自行维护一个可正常使用的版本**（无缝切到 Rust 后端）。  
> 集成主线：`dev-rust`（你说的 rust-dev）  
> 上游同步：`main`

最后更新：2026-09 全面审计（P0–P3 代码项完成；切流阻塞改为真实冒烟 / schema 锁定）  
Cursor 规则：根目录 `AGENTS.md`、`.cursor/rules/`（**进度与计划只写本文，rules 不重复抄表**）  
交互审计面板：Cursor canvas `rust-migration-audit`

---

## 0. 无缝切换结论（先看这里）

| 判断 | 说明 |
|------|------|
| **代码面** | HTTP 全领域、66 JobName、19 队列、媒体/库/同步/搜索 API、WS、HLS、sqlx baseline、CLI — **已到位** |
| **切流路径** | **Docker 一键**：`cd rust-server && docker compose up -d --build`（`docker-compose.yml`：web+API+PG+Redis+ML）；overlay 用 `docker-compose.overlay.yml`。说明见 `rust-server/README.docker.md` |
| **真正阻塞** | 不是缺 API，而是：**真实 compose 冒烟未跑通**、**现有库 baseline 未验证锁定**、**维护模式 AppRestart 重启链路未在你的部署上确认** |
| **下一步** | **仅剩 Cutover（需本机 compose/DB）**：C2 → C1 → C3；可选 P4。代码侧可迁移项已清空。 |

---

## 1. 总体完成度

| 维度 | 上游 TS | rust-server | 评估 |
|------|---------|-------------|------|
| HTTP API 路由 | ~45 个 controller（不含 spec），端点全覆盖 | ~42 个 route 模块（多域合并）+ SPA/分享页 | ✅ **已完成** |
| 领域服务 | ~54 个 NestJS service | ~80+ Rust service（含 `media/`、`workers/`） | ✅ **约 95%+**（无孤立业务域） |
| 数据库访问 | ~55 个 repository | `models/db/` 内联 SQL | ✅ **已完成**（架构不同） |
| BullMQ 任务名 | 66 个 `JobName` | 66 个均有 worker 处理 | ✅ **已完成** |
| BullMQ 队列 | 19 个 `QueueName` | 19 个均有 worker（`search` 遗留空队列 no-op） | ✅ **已完成** |
| WebSocket 客户端事件 | ~15 种 | 含 `on_album_update` | ✅ **已完成** |
| 跨进程协调 | Socket.IO serverSideEmit 等 | Redis：`ConfigUpdate` + `AppRestart` + HLS 六路 | ✅ **已完成**（够用；非 1:1 EventRepository） |
| 数据库迁移 | 上游 Kysely TS 链 | sqlx `1_baseline` + `baseline_lock`；启动 auto init/bridge/漂移检查 | ✅ **纯 Rust** |
| 运维 CLI | `immich-admin` 全量子命令 | `service/admin.rs`（另多 `run-migrations` / `migration-status`） | ✅ **已对齐** |
| 可单机部署使用 | ✓ | ✓（默认单进程） | ✅ **可用** |
| 与上游完全等价 / 真库证明 | ✓ | 冒烟与边缘未证明 | ⚠️ **切流前必测** |

**结论：** 功能迁移主体已完成。剩余工作以 **验证与运维锁定** 为主，不是再补一批接口。

---

## 2. 已完成模块（可认为迁移到位）

### 2.1 HTTP / 路由层

- 全部业务 controller 领域均有对应 `routes/` + `handlers/`（无缺失域）
- 额外：SPA `/`、分享 SSR `/share/*`、`/s/*`（`routes/static_web.rs`）；`maintenance_worker` 路由
- 入口：`rust-server/src/routes/mod.rs`

| 模块 | Rust 路径 | 说明 |
|------|-----------|------|
| 认证 / OAuth / Session / API Key | `routes/auth.rs`, `oauth.rs`, `session.rs`, `api_key.rs` | 含 admin unlink |
| 资产 CRUD / 批量 / 统计 | `routes/asset.rs` | |
| 上传 / 下载 / 播放 | `routes/asset_media.rs`, `asset_file.rs` | |
| 视频 / HLS | `routes/video_stream.rs` + `service/hls.rs` | 单进程 + 分进程 Redis |
| 相册 / 标签 / 堆栈 / 伙伴 / 共享链接 | `routes/album.rs` 等 | |
| 人物 / 人脸 | `routes/person.rs`, `routes/face.rs` | |
| 外部库 | `routes/library.rs` + `library_watcher` | 含 fs watch |
| 搜索 | `routes/search.rs` + `service/search.rs` | API 在；边缘见 P4 |
| 同步 | `routes/sync.rs` + `service/sync.rs` | 体量大；压测见 P4 |
| 工作流 | `routes/workflow.rs` | 执行仅 AssetV1（与上游一致） |
| 插件（读） | `routes/plugin.rs` | 管理 API 见暂缓 |
| 管理：用户 / 配置 / 完整性 / 备份 / 维护 | `routes/user_admin.rs` 等 | |
| 通知 / 邮件 | `routes/notification.rs` | |
| 任务 / 队列管理 | `routes/job.rs`, `queue.rs` | |
| Cluster groups | `routes/cluster_group.rs` | |

### 2.2 认证与权限

| 功能 | 文件 |
|------|------|
| 登录 / 登出 / PIN | `service/auth.rs` |
| OAuth / OIDC | `service/oauth.rs` |
| Session / API Key | `session.rs`, `api_key.rs` |
| 批量 + 单资产媒体权限（伙伴/相册/共享） | `access.rs` → `filter_accessible_ids` / `require_asset_access` |
| 权限枚举 | `models/db/auth_permission.rs` |

### 2.3 资产业务与媒体流水线

| 功能 | Rust 模块 |
|------|-----------|
| 资产 CRUD / 回收站 / 时间线 | `asset.rs`, `timeline.rs`, `trash.rs` |
| 上传 / 下载 | `asset_media.rs` |
| 元数据 / Live Photo / 缩略图 / 视频 / Sidecar / 模板 / 编辑 / 可见性 | `service/media/*` |

近期 parity 已含：Library job 状态、路径 `R_OK`、WS `on_album_update`、ML QueueAll 关闭时 Skipped、伙伴媒体读权限等（详见历史 PR §9）。

### 2.4 机器学习（调 ML 容器，非算法重写）

| 任务 | Worker | Service |
|------|--------|---------|
| SmartSearch (CLIP) | `workers/smart_search.rs` | `media/smart_search.rs` |
| 人脸检测 / 识别 | `face_detection`, `facial_recognition` | `media/face_*` |
| OCR / 重复检测 | `ocr`, `duplicate_detection` | `media/ocr`, `duplicate_detection` |
| ML HTTP 客户端 | — | `service/ml.rs` |

### 2.5–2.9 其他已完成域

- **Library**：CRUD、8 种任务、watch、定时扫描、Windows `fs_access`、跨平台磁盘
- **Sync**：stream/ack、实体类型对齐、审计清理
- **工作流 / 插件**：CRUD、AssetV1 执行、触发、Extism + host（`allowedHosts` 运行时校验）
- **通知 / 邮件 / 社交**：notification、email、album、activity、memory、map、download
- **运维**：system_config/metadata、version、backup、integrity（10）、maintenance + AppRestart、nightly、geodata、storage/DB bootstrap、**`immich-admin` CLI**

### 2.10 后台任务（66 JobName / 19 队列）

均有 handler。`workers/search.rs` 仅消化遗留空队列 → `skipped`（上游亦无 `@OnJob`）。

定时：`nightly`, `backup_scheduler`, `library_scheduler`, `integrity_scheduler`, `version_scheduler`。

---

## 3. 差距与待办（按切流优先级）

### Cutover — 无缝切换前必做（验证 / 运维，非缺功能）

| # | 事项 | 说明 | 怎么做 |
|---|------|------|--------|
| C1 | **真实 compose 冒烟** | 单元测试不证明全链路 | `cd rust-server && docker compose up -d --build`；`rust-server/scripts/smoke.ps1`（登录→上传→缩略图→搜索；可选库扫描/备份） |
| C2 | **现有库 schema 锁定** | Kysely 若 ahead of `baseline_lock` 会漂移 | `immich-admin migration-status` / `schema-check`；无 ahead 后再当生产 schema 源 |
| C3 | **维护模式重启链路** | CLI/UI 写 DB + Redis `AppRestart` 后 `exit(0)` | 确认 compose/k8s **restart policy** 能拉起进维护或退出维护 |

> 三项通过后，可认为「日常可无缝切到 Rust 单进程后端」。

### P0 / P1 / P3 — 代码项（已完成）

| 优先级 | 项 | 状态 |
|--------|-----|------|
| P0 | 伙伴媒体访问、`on_album_update` | ✅ |
| P1 | HLS Redis 六路、ConfigUpdate + AppRestart | ✅ |
| P3 | search no-op、sqlx baseline、telemetry、tracing、immich-admin CLI | ✅ |

### P2 — 工作流 / 插件（与上游一致或可暂缓）

| # | 问题 | 说明 | 处理 |
|---|------|------|------|
| 5 | 工作流仅 AssetV1 | 上游亦仅 AssetV1 | **等上游**；非缺口 |
| ~~6~~ | AssetV1 写 null | 上游亦未实现 | 无需改 |
| 7 | PersonRecognized 触发 | TS 已注释 | 暂缓 |
| 8 | Plugin `allowedHosts` 管理 API | 运行时有校验，公开管理 API 无 | 暂缓 |
| — | ~~Plugin host 边界测试~~ | ✅ 单元测试：stubs / parse_args / allowedHosts deny | 已完成 |

### P4 — 代码已有，parity 未用真库证明

| 领域 | 风险点 | 建议验证 |
|------|--------|----------|
| Search v3 | 筛选 / 游标 / 智能搜索边缘 | 对比同库 TS 结果 |
| Sync | 全实体 backfill / ack / 多端 | 手机 + web 同步一轮 |
| ML 流水线 | CLIP / 人脸 / OCR / 重复 QueueAll | live ML 容器跑全量 |
| Integrity | 大库 checksum / untracked | 万级文件扫一次 |
| 伙伴 / 共享媒体 | 权限已修，需 E2E | 伙伴账号打开共享图 |
| 分进程 | `INCLUDE=api` + `microservices` + HLS Redis | **仅在需要拆分时测**；默认单进程可跳过 |

### 工程小项（不挡切流）

| 项 | 说明 |
|----|------|
| ~~启动 / worker / media `println!` → `tracing`~~ | ✅ 服务热路径已换；保留 `admin` / `schema_check` / `database_migrations` / `logging` 的 CLI/早期输出 |
| ~~Plugin host 边界测试~~ | ✅ `plugin_host.rs` 单元测试 |
| ~~缩略图 / profile JPEG quality~~ | ✅ `profile_image` + `media/thumbnail` fallback `write_resized` |

---

## 4. 明确暂缓（不要塞进普通 PR）

| 项 | 原因 |
|----|------|
| ML/OCR/face/duplicate **算法**重写 | 继续调上游 ML 服务 |
| Search v3 **大规模** SQL / Sync **协议级**重写 | API 已有；优先修实测 bug |
| Public plugin `allowedHosts` API | 非核心；TS 亦无独立管理端点 |
| EXIF 自动打标 → AssetTagged | 手动 tag API 已可用；上游亦弱 |
| PersonRecognized / AssetPersonV1 | 上游关闭 |
| Nest `EventRepository` 全量复刻 | 直调 + Redis 少量频道已够 |

---

## 5. WebSocket / 跨进程事件对照

| 事件 | TS | Rust | 状态 |
|------|----|------|------|
| `on_upload_success` 及资产删/废/更/隐/恢 | ✓ | ✓ | ✅ |
| `on_asset_stack_update` / `on_user_delete` / `on_session_delete` | ✓ | ✓ | ✅ |
| `on_notification` / `on_person_thumbnail` / `on_config_update` | ✓ | ✓ | ✅ |
| `on_server_version` / `on_new_release` | ✓ | ✓ | ✅ |
| `AssetUploadReadyV2` / `AssetEditReadyV2` / `AppRestartV1` | ✓ | ✓ | ✅ |
| **`on_album_update`** | ✓ | ✓ | ✅ |
| HLS server events（跨进程） | ✓ | ✓ Redis 六路；单进程本地 `PendingEvents` | ✅ |

---

## 6. 推荐路线图（无缝切流）

### 阶段 A — 切流证明（当前最高优先）

1. ~~P0 伙伴媒体 + `on_album_update`~~ ✅  
2. **C2** 目标库：`migration-status` / `schema-check`，确认无 `kysely_ahead_of_lock`  
3. **C1** `cd rust-server && docker compose up -d --build`，跑 `smoke.ps1`  
4. **C3** 验证维护模式进入/退出后进程自动重启  

### 阶段 B — 部署模型

5. **单进程（推荐默认）** ✅ — API + workers + HLS 同进程  
6. 可选：`IMMICH_WORKERS_INCLUDE=api` + `microservices`（HLS Redis 已就绪，需实测）  

### 阶段 C — 工作流 / 插件

7. ~~AssetV1 null / 扩展类型~~ — 上游无缺口  
8. ~~Plugin host 边界测试~~ ✅  
9. ~~启动路径 tracing + profile quality~~ ✅  

### 阶段 D — 长期维护

10. ~~P3 工程项（search no-op / baseline / telemetry / logging / CLI）~~ ✅  
11. P4 真库验证（Search / Sync / ML / Integrity）  
12. 定期 `main` → `dev-rust`；baseline 锁定后有 Kysely 增量再写 `migrations/2+`  
13. ~~其余 worker/`media` 内 `println!` → `tracing`~~ ✅（CLI 面保留 stdout）

---

## 7. 日常维护检查清单

```bash
# 同步分支
git fetch origin main dev-rust

# 单元测试
cd rust-server && cargo +stable test --offline --lib

# 切流前（按你的 compose）
# cd rust-server && docker compose up -d --build
# # or overlay: docker compose -f ../docker/docker-compose.yml -f docker-compose.overlay.yml up -d --build
# rust-server immich-admin migration-status
# rust-server immich-admin schema-check
# $env:IMMICH_URL="http://127.0.0.1:2283"
# $env:IMMICH_EMAIL="..."; $env:IMMICH_PASSWORD="..."
# .\rust-server\scripts\smoke.ps1
```

| 操作 | 分支 |
|------|------|
| 合并上游 Immich | `main`，再 PR → `dev-rust` |
| rust 功能开发 | 从 `dev-rust` 拉 `cursor/<name>-4063` |
| 合并 rust 功能 | PR → `dev-rust`，删 `cursor/*` |

详见根目录 **`AGENTS.md`**。

---

## 8. 关键文件索引

| 关注点 | TypeScript | Rust |
|--------|------------|------|
| 入口 / Worker | `server/src/main.ts`, `workers/*.ts` | `main.rs`, `service/bootstrap.rs` |
| 路由 | `server/src/controllers/` | `rust-server/src/routes/` |
| 任务枚举 / Worker | `enum.ts` / `@OnJob` | `service/job.rs` / `workers/mod.rs` |
| 媒体流水线 | `media.service.ts`, `metadata.service.ts` | `service/media/` |
| 同步 / 搜索 | `sync.service.ts`, `search.service.ts` | `sync.rs`, `search.rs` |
| 工作流 | `workflow-execution.service.ts` | `workflow_execution.rs` |
| WebSocket / 权限 | `websocket.repository.ts`, `access.repository.ts` | `websocket.rs`, `access.rs` |
| 跨进程事件 | Socket.IO / EventRepository | `server_events.rs`, `hls_events.rs` |
| CLI | `commands/*`, `cli.service.ts` | `service/admin.rs` |
| 切流 compose | — | `rust-server/docker-compose.yml`（全栈） |

---

## 9. 已合并 PR / 切片摘要（本 fork）

| 批次 | 内容 |
|------|------|
| #7–#18 | 早期 parity、sync-main、workflow/plugin、library/WS/ML/sidecar/integrity 等 |
| P0 | 伙伴/相册媒体权限；`on_album_update` |
| sqlx | 单一 `1_baseline` + `baseline_lock`；去掉 `init.sql`；`migration-status` |
| 维护 | Redis `AppRestart`；JWT `/maintenance?token=` |
| P1 | HLS Redis；`INCLUDE=api` 不再误开 microservices |
| 运维 | telemetry `repo`/`io`；tracing；头像缩略图；`smoke.ps1` |
| P3 | `immich-admin` CLI 行为对齐（list-users / reset / grant / externalDomain / ConfigUpdate） |
| （续） | Plugin host 边界单测；bootstrap/workers/media 等热路径 `println!`→`tracing`；profile/thumbnail JPEG quality |
| 其他 parity | MemoryGenerate 锁、trash/duplicate、ClusterGroup、download Content-Disposition、lockedProperties 等 |

---

## 10. 一句话总结

**现在：** API / 66 Job / 迁移 / HLS / CLI / 日志指标 / plugin 边界测试均在 Rust；服务热路径已用 tracing。  
**还差（切流，需本机 compose/DB）：** C2 `migration-status` → C1 `smoke.ps1` → C3 维护重启 →（可选）P4。  
**本环境限制：** 无 Docker / 无 Immich 凭据时无法代跑 Cutover；代码侧可迁移项已清空。  
**策略：** 单进程 + ML sidecar 先切；合上游后开 `2+`；大块算法/协议重写继续暂缓。
