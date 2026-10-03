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
| **切流路径** | **服务器**：空目录里放 `docker-compose.yml` + `.env`，`docker compose up -d`（不克隆仓库）。**本机 Docker**：`cd rust-server && docker compose up -d --build`。**不用 Docker**：`cargo run` + Vite。说明只维护在 `rust-server/README.md` |
| **真正阻塞** | 不是缺 API，而是：**真实 compose 冒烟未跑通**、**现有库 baseline 未验证锁定**、**维护模式 AppRestart 重启链路未在你的部署上确认** |
| **下一步** | §11 行为差异已按表对齐。切流验证仍是 C2 → C1 → C3。 |

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

上表与当前上游 `server/` 的实现范围一致：算法在 ML 服务里，Search / Sync 的现有 SQL 已对照，`allowedHosts` 没有公开管理端点，`PersonRecognized` 在上游被注释，事件副作用是直接调用。

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

# 运行方式见 rust-server/README.md（./deploy、docker compose、cargo run）
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
| （续） | 360° 缩略图回写 `XMP-GPano`；EXIF RegionInfo 字符串小数人脸坐标 |
| （续） | 缩略图：sRGB/P3 选择、源 ICC 回写、透明通道标记、渐进 JPEG（质量 ≥ 80 用 4:4:4，jpeg-encoder 写出 SOF2） |
| （续） | RAW 全尺寸：仅在网页不能直接显示时生成；内嵌 JPEG 原样落盘并写 Orientation#/ColorSpace。抽出的预览和编辑图先套 EXIF 方向；ffmpeg 已旋转的帧和视频人物预览不再转一次 |
| （续） | 人脸识别按 cluster group 搜索，并为当前用户补同一 `personGroupId` 的人物行 |
| （续） | 插件 Extism 日志带上 `name@version` 上下文，对齐 `Plugin:${label}` |
| （续） | 人物合并：姓名或生日冲突则跳过，并按每个 owner 的人物行合并。外部库动态照片视频写入原 `libraryId` |
| （续） | GPS 仅在经纬度都是 0 时丢弃；`BitsPerSample` 的 `"16 16 16"` 按每通道位深解析 |
| （续） | 已编辑照片的宽高在元数据重提时分开更新，已有的一边不会被另一边空值带着重写 |
| （续） | 编辑资产时拒绝视频、实况、全景、GIF、SVG，并检查裁剪必须排在第一步且不超出画面 |
| （续） | 取消编辑后删除已编辑衍生图并排队删盘；写回宽高时按 EXIF 方向对调 |
| （续） | RAW 没有可用内嵌预览且要重做全尺寸时，按原图全帧解码，不再先缩成预览尺寸 |
| （续） | 照片和头像缩略图按短边盖住目标尺寸（sharp `outside`），短边已经更小则不放大 |
| （续） | EXIF 宽高、方向、ISO、光圈、焦距、评分取列表的第一个数，超出整数范围的值丢掉 |
| 其他 parity | MemoryGenerate 锁、trash/duplicate、ClusterGroup、download Content-Disposition、lockedProperties 等 |

---

## 10. 一句话总结

**现在：** API / 66 Job / 迁移 / HLS / CLI 均在 Rust。§11 服务对照项已对齐。  
**还差：** 切流仍要 C2 → C1 → C3。  
**本环境限制：** 无 Docker / 无 Immich 凭据时无法代跑 Cutover。  
**策略：** 之后按 git 增量跟上游 `server/`。

---

## 11. 2026-10-02 服务对照（未对齐则继续改）

对照范围：官方 `server/src` 里会改变结果的实现，对的是分支和 SQL，不是同名函数。下表只记已经改过、或核对后决定保留的行为。全量文件和已读逻辑记在 `.cursor/docs/parity-line-compare.md`。

遥测里任务和队列仍用 `immich.jobs` / `immich.queues`。HTTP、数据库、Redis、主机指标用 OpenTelemetry 语义名（`http.server.request.duration`、`db.client.operation.duration`、`db.client.connection.count`、`system.cpu.utilization`、`system.memory.usage`、`system.filesystem.usage`）。配置校验失败带 Zod 的 `code`、`expected`、`input`、`values`、范围字段；cron 失败说明与 `cron` 4.4 相同。`schema-check` 的漂移行用 sql-tools `asHuman` 句式，向量列对比维度，CHECK 对比表达式。期望结构仍是 `1_baseline.sql`。

| 状态 | 项 | 官方 | Rust |
|------|----|------|------|
| 已改 | OCR 文字框 | 按编辑和方向变换 | 已按编辑变换 |
| 已改 | 旧版元数据搜索 | 相册权限；共享链接必须带相册 | 已补上 |
| 已改 | 改密码 | 总会踢掉其他会话 | 已总是踢掉其他会话 |
| 已改 | 手机 OAuth | 用配置里的回调地址换 token | 已替换 `app.immich:/oauth-callback` |
| 已改 | 共享链接列表 | 带回全部资源 | 已带回全部 |
| 已改 | 合并重复项 | 检查相册写入和分享权限，失败带 `errorMessage` | 已过滤相册和可分享资源，并带上说明 |
| 已改 | 删除校验和不匹配报告 | 类型 `checksum_mismatch` | 已改为该类型 |
| 已改 | 缩略图、通知、转码、备份、元数据、头像 | 合并后的配置 | 这些路径已读合并配置 |
| 已改 | 关闭的工作流 | 资源触发时仍会跑 | 已不再按 `enabled` 过滤 |
| 已改 | 提升权限 | 401 `Elevated permission is required` | 时间线、搜索、资产统计已改为 401 |
| 已改 | 找不到 | 404 | 许可证、头像、原图、播放、磁盘上缺失的文件已改为 404 |
| 已改 | 维护状态 | API 进程固定未维护 | API 固定未维护；维护进程才返回进行中 |
| 已改 | 检测旧安装 | API 要管理员；维护进程不要求登录 | 与官方相同 |
| 已改 | 新手引导权限 | `UserOnboardingRead` / `Update` | 已改用这两项 |
| 已改 | 管理员锁定统计 | 不要求 PIN 解锁 | 已不再检查提升权限 |
| 已改 | 管理员日历热力图 | `AdminUserRead` | 已改用该项 |
| 已改 | 相册改成员 | 成员角色为 owner 时拒绝 | 已按相册成员角色判断 |
| 已改 | 记忆空更新、标签重名预检、邮件模板预览回退、API Key 默认名、登录邮箱 | 空更新成功；重命名交给数据库；相册更新预览回退邀请模板；空名称用 `API Key`；登录按原样邮箱 | 已按官方对齐 |
| 已改 | 重复项解决、人物批量更新、工作流搜索、工作流缺失、通知删除 | 解决前校验重复组归属；批量更新失败原因一律 `unknown`；搜索不按 `logging` 过滤；工作流不存在时任务成功；删除通知不因已删再报错 | 已按官方对齐 |
| 已改 | 用户偏好默认值 | 含 `recentlyAdded.sidebarWeb`、`memories.sidebarWeb`；标签默认关闭 | 已补上。缺 `recentlyAdded` 时侧边栏渲染抛错，登录后页面停在加载动画。已保存的空值也会补回这些字段 |
| 已改 | 当前用户响应 | `clusterGroupId`；头像色为空时按邮箱计算 | 分享设置读取 `clusterGroupId`。旧库没有人物分组表时该字段为 null |
| 已改 | 机器学习地址 | 空的 `machineLearning.urls` 用默认地址 | 保存系统设置会提交整份配置。地址列表为空时改回 `IMMICH_MACHINE_LEARNING_URL` 或 `http://immich-machine-learning:3003`，不再挡住其它设置 |
| 已改 | 人物列表 | `GROUP BY` 人物主键；最少人脸数读偏好；统计含共享相册；人名搜索阈值 0.5 | `/api/people` 不再因分组缺列返回 500。统计和按名字搜索与官方 SQL 一致 |
| 已改 | 人脸归属 | 按当前查看者的 `ownerId` 取人物；回忆排除隐藏人物时同时匹配照片主人 | 同一人物组里不会串到别人的名字或隐藏状态 |
| 已改 | 会话列表 | 查询列带 `session` 表别名 | `/api/sessions` 联表 `user` 时 `id` 不再歧义，用户设置里的设备列表可以打开 |
| 已改 | 同步相册增量 | 每个用户在相册里只有一条成员记录，按 `updateId` 排序 | 去掉与排序冲突的 `DISTINCT ON`，手机同步相册不再因 `SELECT DISTINCT ON expressions must match initial ORDER BY expressions` 失败 |
| 已改 | 相册描述 | 空描述返回 `''`，不因数据库 NULL 失败 | 列表和详情用 `COALESCE(description, '')`。从官方库迁过来的空描述相册可以打开 |
| 已改 | 边车写回 | 描述为空时不写这条 EXIF | `asset_exif.description` 为 NULL 时任务不再解码失败 |
| 已改 | 头像色 | `UserAvatarColor` 顺序，按邮箱码点求和 | 相册成员、活动、合作伙伴与用户接口用同一套颜色 |
| 已改 | 相册数量和日期 | 只统计未删除且可见性为 archive/timeline 的照片；起止日按 UTC 日期 | 没有可见照片时数量是 0，不再把已删除或隐藏的关联算进去。日期与官方 `getMetadataForIds` 同一天 |
| 已改 | 资源更新 | 只更新本次提交的列 | 只改描述、日期或评分时不再发出空的 `UPDATE asset SET`。旧版搜索分页按 `fileCreatedAt` 再按 `id` 排序 |
| 已改 | 地图标记 | 默认时间线查询的参数从 `$1` 连续编号 | 未勾选「包含归档」时不再引用不存在的 `$1`，地图页可以加载 |
| 已改 | 旧版按标签搜索 | 父标签通过 `tag_closure` 包含子标签 | 搜索父标签时能看到打在子标签上的照片 |
| 已改 | 人物时间线 | 本人照片，或查看者所在未删除共享相册里的照片 | 别人分享到相册里、且属于这个人物的照片会出现在人物页 |
| 已改 | 缩略图尺寸 | `size=original` 拒绝 | 缩略图接口返回 400 `May not request original file`。网页能直接显示的全尺寸图仍跳到原图 |
| 已改 | 用户偏好保存 | 只写入和默认值不同的字段 | 保存设置后，没改过的项仍跟随后续默认值，不会整份固化 |
| 已改 | 用户搜索排序 | 未删除用户按 `createdAt` 降序 | 公开用户列表和官方 `getList` 顺序一致 |
| 已改 | 视频码率上限 | 不带单位的数字按比特/秒；`k`/`M` 再换算 | `maxBitrate` 为 `5000000` 时不再被放大 1000 倍 |
| 已改 | 标签颜色 | 请求里的 `color: null` 清空颜色 | 不传 `color` 时保留原颜色 |
| 已改 | 无 EXIF 日期 | 取文件创建时间、非零的 birthtime 和 mtime 里更早的一个 | 不再把库里的 `fileModifiedAt` 算进去，birthtime 为 0 时忽略，避免照片落到 1970 或错误的一天 |
| 已改 | 重复组编号 | 资产还没有 `duplicateId` 时，用搜索结果里第一次出现的组 id | 合并进已有重复组，而不是随机挑一个 |
| 已改 | 合并重复项坐标 | 纬度和经度都各自唯一时才写入 | 只有一边能确定时不改坐标 |
| 已改 | 批量加入相册 | 新封面按请求里的资源顺序取第一张还不在相册里的照片 | 空相册的封面不再随集合迭代顺序变化 |
| 已改 | 数据库恢复失败 | 迁移、没有管理员或启动检查失败时，用恢复点把库回滚 | 恢复 SQL 成功后如果后续检查失败，会重新灌入 `restore-point` 备份，不再停在半恢复的库上 |
| 已改 | 动画图和 HEIF | 可能动画的扩展名含 png/jxl/heic/heif；HEIF 含 avif | 这些格式保留时长，avif 用 Rotation 换算方向，不再把 CR3 一类的假时长留下来 |
| 已改 | 探索页城市 | `DISTINCT ON (city)` 带 `ORDER BY city` | 打开探索页不再因排序表达式不匹配失败 |
| 已改 | 相似人物排序 | 先按未隐藏、收藏，再按人脸距离 | 从某张脸找相似人物时，隐藏人物仍排在后面 |
| 已改 | 搜索建议 | `lensModel`、`includeNull` 用驼峰；镜头列名加引号；每种建议只套自己的筛选 | 选了镜头再查厂商能筛到；查城市不会被相机条件收窄 |
| 已改 | 删除堆栈照片 | 主照片只要没被删就算剩余成员；新封面按 `fileCreatedAt` 取最早的时间线照片 | 主照片被隐藏时，删掉一张成员不会把整个堆栈拆掉。回收站里的照片不再被当成还在堆栈里 |
| 已改 | OCR 任务状态 | 没有预览图先 Failed，隐藏照片再 Skipped | 还没生成预览的隐藏照片不再被记成跳过 |
| 已改 | 下载压缩包文件名 | 用原始文件名，重名加 `+1` | 编辑后的照片不再被改成磁盘上的扩展名 |
| 已改 | 重分配人脸 | 同一张照片上这个人的每张脸都改挂到目标人物；只有真的挪走了脸才重选封面 | 多张脸不再只挪一张。没匹配到脸时不再随便换封面 |
| 已改 | 人脸检测缩放 | 旧脸框宽或高为 0 时按 1 来缩放 | 宽高为 0 的旧框不再把新检测全部当成新脸 |
| 已改 | 堆栈张数 | 时间线上除封面以外的张数，再加 1 | 封面在时间线上时不再被数两次 |
| 已改 | 工作流写回 | `continue` 缺省继续；`config` 为 null、false、0、空字符串时不保存；评分 null 清空评分 | 插件返回空配置不再把步骤配置写成 null。把评分设成 null 会清掉原来的星级 |
| 已改 | 元数据关键词 | `TagsList` 即使是空数组也不回退到层级主题；没有关键词时清空照片上的标签 | 一张照片后来去掉关键词，相册标签页不再留着旧标签。空的 `TagsList` 不会再误用层级主题 |
| 已改 | 系统设置可点 | 空的 `IMMICH_CONFIG_FILE` 不锁设置页；配置补上实时转码列表、账号管理地址和 `notifications` 并发 | 设置里的开关不再因为空环境变量整页禁掉。展开实时转码时不再因为缺编码和分辨率而报错 |
| 已改 | 资产统计、边车、回收站原图、搜索范围、改密码、堆栈/标签权限、批量打标签、存储标签、OAuth 资料、API Key、反向地理编码、相册分享、认证状态 | 统计权限是 `asset.statistics`；边车任务只带资源 id；回收站仍可取原图/缩略图/视频；旧版统计按可见性收用户；智能搜索保留空白查询；改密码只更新密码；堆栈和标签错误带权限名；批量打标签只处理有权限的子集；创建用户时清洗存储标签；ID Token 无邮箱时用 userinfo；创建/轮换 API Key 带嵌套 `apiKey`；国家名用英文官方名；相册分享可带 assetIds；PIN 用户缺失时 401 文案为 `Unauthorized` | 已按官方对齐 |
