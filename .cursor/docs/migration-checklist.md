# rust-server 迁移清单（jiangkerLove/immich）

> 本文档描述本 fork 将上游 TypeScript `server/` 迁移到 `rust-server/` 的进度与规划。  
> 目标：**尽可能对齐上游行为，后期自行维护一个可正常使用的版本**。  
> 集成主线：`dev-rust`（你说的 rust-dev）  
> 上游同步：`main`

最后更新：`cursor/p0-partner-access-album-ws-4063`（P0 伙伴媒体访问 + `on_album_update`，2026-09）  
Cursor 规则：根目录 `AGENTS.md`、`.cursor/rules/`

---

## 1. 总体完成度

| 维度 | 上游 TS | rust-server | 评估 |
|------|---------|-------------|------|
| HTTP API 路由 | 41 个 controller，约 191 个端点 | 路由已注册，端点基本全覆盖 | ✅ **已完成** |
| 领域服务 | ~50 个 NestJS service | ~80 个 Rust service 模块 | ✅ **约 90%** |
| 数据库访问 | 55 个 repository | `models/db/` 内联 SQL（54 模块） | ✅ **已完成**（架构不同） |
| BullMQ 任务名 | 66 个 `JobName` | 66 个均有 worker 处理 | ✅ **已完成** |
| BullMQ 队列 | 19 个 `QueueName` | 18 个有 worker；`search` 无 worker | ⚠️ **基本完成** |
| WebSocket 客户端事件 | ~15 种 | 含 `on_album_update` | ✅ **已完成** |
| 内部事件总线 | `EventRepository` ~30 种 |  mainly `ConfigUpdate` via Redis | ⚠️ **部分** |
| 数据库迁移 | Kysely TS migrations | 仍调用 Node 脚本跑 Kysely | ⚠️ **混合方案** |
| 可单机部署使用 | ✓ | ✓ | ✅ **可用** |
| 与上游完全等价 | ✓ | 仍有差距 | ⚠️ **持续对齐中** |

**结论：** 日常功能（上传、浏览、相册、人物、库、同步、搜索 API、备份、完整性检查、大部分后台任务）已可跑通；剩余工作主要是**行为细节、多进程部署、观测与边缘场景**。

---

## 2. 已完成模块（可认为迁移到位）

### 2.1 HTTP / 路由层

- **全部 41 个 controller 领域**均有对应 `routes/` + `handlers/`
- 额外实现：SPA 首页 `/`、分享页 SSR `/share/*`、`/s/*`
- 主要路由文件：`rust-server/src/routes/mod.rs` 及其子模块

| 模块 | Rust 路径 | 说明 |
|------|-----------|------|
| 认证 / OAuth / Session / API Key | `routes/auth.rs`, `oauth.rs`, `session.rs`, `api_key.rs` | 登录、PIN、OIDC |
| 资产 CRUD / 批量 / 统计 | `routes/asset.rs` | |
| 上传 / 下载 / 播放 | `routes/asset_media.rs`, `asset_file.rs` | |
| 视频 / HLS | `routes/video_stream.rs` + `service/hls.rs` | 单进程可用 |
| 相册 / 标签 / 堆栈 / 伙伴 / 共享链接 | `routes/album.rs` 等 | |
| 人物 / 人脸 | `routes/person.rs`, `routes/face.rs` | |
| 外部库 | `routes/library.rs` + `service/library_watcher.rs` | 含 fs watch |
| 搜索 | `routes/search.rs` + `service/search.rs` | API 在；SQL 边缘待验 |
| 同步 | `routes/sync.rs` + `service/sync.rs` | 体量大，实体类型齐全 |
| 工作流 | `routes/workflow.rs` | 执行见 §3.4 |
| 插件（读） | `routes/plugin.rs` | |
| 管理：用户 / 配置 / 完整性 / 备份 / 维护 | `routes/user_admin.rs` 等 | |
| 通知 / 邮件 | `routes/notification.rs` | |
| 任务 / 队列管理 | `routes/job.rs`, `queue.rs` | |

### 2.2 认证与权限

| 功能 | 文件 |
|------|------|
| 登录 / 登出 / PIN | `service/auth.rs` |
| OAuth / OIDC | `service/oauth.rs` |
| Session | `service/session.rs` |
| API Key | `service/api_key.rs` |
| 批量资产权限（伙伴相册等） | `service/access.rs` → `filter_accessible_ids` |
| 单资产媒体读权限（伙伴/相册/共享链接） | `require_asset_access` → 与批量路径一致走 `filter_accessible_ids` |
| 权限枚举 | `models/db/auth_permission.rs` |

### 2.3 资产业务与媒体流水线

| 功能 | Rust 模块 |
|------|-----------|
| 资产 CRUD / 回收站 / 时间线 | `service/asset.rs`, `timeline.rs`, `trash.rs` |
| 上传 / 下载 | `service/asset_media.rs` |
| 元数据提取 | `service/media/metadata_extract.rs` |
| Live Photo / 动图后处理 | `service/media/metadata_postprocess.rs` |
| 缩略图 / 人物缩略图 / 编辑缩略图 | `service/media/thumbnail.rs` |
| 视频转码 | `service/media/video_encode.rs` |
| Sidecar 读写在盘 | `service/media/sidecar.rs` |
| 存储模板迁移 | `service/media/storage_template.rs` |
| 文件路径迁移 | `service/media/file_migration.rs` |
| 资产编辑 | `service/media/edits.rs` |
| 可见性 | `service/media/visibility.rs` |

**近期对齐（PR #7–#18 + P0 切片）：**

- Library job 状态 `Success` / `Skipped` / `Failed`
- Library scan 含软删除库、`path_normalize`、路径 `R_OK`
- Background-task：`AssetDelete` / `VersionCheck` / `UserDelete` 状态
- Websocket：`on_new_release` 连接时推送；**`on_album_update`**（加/删相册资产、共享链接上传）
- ML `*QueueAll` 在功能关闭时 `Skipped`
- Integrity 扫描跳过隐藏文件
- `LibrarySyncAssetsQueueAll` 任务名、migration 空目录清理条件
- Sidecar `null/undefined` 变更检测、Person 迁移空路径等
- **单资产媒体访问**：`require_asset_access` 支持伙伴 / 相册成员（不再仅 owner）

### 2.4 机器学习流水线（调用 ML 服务，非算法重写）

| 任务 | Worker | Service |
|------|--------|---------|
| SmartSearch (CLIP) | `workers/smart_search.rs` | `media/smart_search.rs` |
| 人脸检测 | `workers/face_detection.rs` | `media/face_detection.rs` |
| 人脸识别 / 聚类 | `workers/facial_recognition.rs` | `media/facial_recognition.rs` |
| OCR | `workers/ocr.rs` | `media/ocr.rs` |
| 重复检测 | `workers/duplicate_detection.rs` | `media/duplicate_detection.rs` |
| ML HTTP 客户端 | — | `service/ml.rs` |

> 说明：实现的是「排队 + 调 ML 容器 + 写库」，不是重写 CLIP/OCR 模型本身。

### 2.5 外部库（Library）

| 功能 | 文件 |
|------|------|
| CRUD / 校验 / 扫描 | `service/library.rs` |
| 8 种 library 任务 | `workers/library.rs` |
| 文件监视 | `service/library_watcher.rs` |
| 定时扫描 | `service/library_scheduler.rs` |
| 读权限检查 | `utils/fs_access.rs` |

### 2.6 同步（Sync）

| 功能 | 文件 |
|------|------|
| Stream / Ack API | `service/sync.rs`（~1800 行） |
| 各实体类型常量 | 与上游 `SyncEntityType` 对齐 |
| 审计表清理任务 | `workers/background_task.rs` |
| DB 查询 | `models/db/sync_repository.rs` |

### 2.7 工作流与插件

| 功能 | 状态 | 文件 |
|------|------|------|
| 工作流 CRUD / 日志 / 分享 | ✅ | `service/workflow.rs` |
| 执行引擎 | ⚠️ 仅 AssetV1 | `service/workflow_execution.rs`, `workers/workflow.rs` |
| 触发：AssetCreate / AssetMetadataExtracted / AssetTagged | ✅ | `service/workflow_trigger.rs` |
| 执行日志写入 | ✅ | `models/db/workflow_log.rs` |
| Plugin 导入 / Extism 运行时 | ✅ | `service/plugin_import.rs`, `plugin_runtime.rs` |
| Plugin host 函数（tag / HTTP 等） | ✅ | `service/plugin_host.rs` |

### 2.8 通知、邮件、社交

| 功能 | 文件 |
|------|------|
| 用户通知 | `service/notification.rs` |
| 邮件模板 / 发送 | `service/email.rs` |
| 通知任务 | `workers/notifications.rs` |
| 相册 / 活动 / 记忆 / 地图 / 下载 | 各 `service/*.rs` |

### 2.9 系统 / 运维

| 功能 | 文件 |
|------|------|
| 系统配置读写 | `service/system_config.rs` |
| 系统元数据 | `service/system_metadata.rs` |
| 版本检查 + 历史 + websocket | `service/version_check.rs`, `version_scheduler.rs` |
| 数据库备份 / 恢复 | `service/database_backup.rs`, `database_backup_runner.rs` |
| 完整性检查（10 种任务） | `service/integrity.rs`, `workers/integrity.rs` |
| 维护模式 + maintenance worker | `service/maintenance.rs`, `maintenance_worker.rs` |
| 夜间任务编排 | `service/nightly.rs`, `job.rs::queue_nightly_jobs` |
| 地理数据导入 | `service/geodata_import.rs` |
| 存储 / DB 引导 | `storage_bootstrap.rs`, `database_bootstrap.rs` |
| `immich-admin` 子集 | `service/admin.rs` |

### 2.10 后台任务（全部 66 个 JobName）

每个上游 `JobName` 在 Rust 中都有对应 handler（按 worker 分文件）：

| Worker 文件 | 负责的任务（摘要） |
|-------------|-------------------|
| `background_task.rs` | 清理类、VersionCheck、UserDelete、AssetDelete、MemoryGenerate 等 |
| `thumbnail_generation.rs` | 缩略图 QueueAll / 单资产 / 人物 |
| `metadata_extraction.rs` | 元数据提取 |
| `video_conversion.rs` | 视频编码 |
| `editor.rs` | 编辑缩略图 |
| `sidecar.rs` | SidecarCheck / Write / QueueAll |
| `face_detection.rs` | 人脸检测 |
| `facial_recognition.rs` | 人脸识别 QueueAll |
| `smart_search.rs` | CLIP 编码 |
| `duplicate_detection.rs` | 重复检测 |
| `ocr.rs` | OCR |
| `library.rs` | 库扫描 / 同步 / 删除 |
| `migration.rs` | 存储迁移 |
| `storage_template_migration.rs` | 模板迁移 |
| `backup_database.rs` | 数据库备份 |
| `integrity.rs` | 完整性 10 任务 |
| `workflow.rs` | WorkflowAssetTrigger |
| `notifications.rs` | 通知 / 邮件 |

定时调度：`nightly.rs`, `backup_scheduler.rs`, `library_scheduler.rs`, `integrity_scheduler.rs`, `version_scheduler.rs`

---

## 3. 部分完成 / 已知差距（建议优先处理）

按**对你「能正常使用」的影响**排序。

### P0 — 用户可感知的功能缺口

| # | 问题 | 上游行为 | 当前 rust | 建议改动 |
|---|------|----------|-----------|----------|
| ~~1~~ | ~~伙伴资产媒体访问~~ | ~~伙伴可通过媒体端点访问共享资产~~ | ✅ `require_asset_access` → `filter_accessible_ids` | 已完成 |
| ~~2~~ | ~~缺少 `on_album_update` websocket~~ | ~~相册变更后推送~~ | ✅ `AlbumService::notify_album_update` + `emit_album_update` | 已完成 |

> P0 两项已在本切片落地。下一优先见 P1。

### P1 — 多实例 / 拆分部署

| # | 问题 | 说明 | 文件 |
|---|------|------|------|
| 3 | **HLS 跨进程协调** | TS 用 Socket.IO server events（`HlsSegmentRequest` 等）；Rust 用进程内 `PendingEvents` | API 与转码分进程时会坏 | `service/transcoding.rs`, `service/hls.rs` |
| 4 | **内部事件总线不完整** | TS `@OnEvent` 驱动多处副作用；Rust  mainly `ConfigUpdate` Redis 广播 | 某些边界行为可能缺触发 | `service/server_events.rs` |

### P2 — 工作流 / 插件细节

| # | 问题 | 说明 | 文件 |
|---|------|------|------|
| 5 | **工作流仅执行 AssetV1** | 其他 workflow 类型返回 `Skipped` | `service/workflow_execution.rs` |
| 6 | **AssetV1 字段清空** | TS 对 description/lat/lon 显式 `null` 会清空；Rust 忽略 null | `workflow_execution.rs` `apply_asset_v1_changes` |
| 7 | **PersonRecognized 触发** | TS 已注释，双方均未启用 | 可暂缓 |
| 8 | **Plugin `allowedHosts` 管理 API** | 运行时校验有，公开管理 API 无 | 可暂缓 |

### P3 — 运维与工程化

| # | 问题 | 说明 | 文件 |
|---|------|------|------|
| 9 | **`search` 队列无 worker** | 队列列表里有，无消费者；若有遗留入队会积压 | `service/queue.rs`, `workers/mod.rs` |
| 10 | **数据库迁移依赖 Node** | 通过 `bin/run-kysely-migrations.cjs` 跑 Kysely | `service/database_migrations.rs`；需 `IMMICH_SERVER_PATH` 或构建 `server/` |
| 11 | **Telemetry Io/Repo 指标** | TS 有 DB/IO prometheus；Rust 未接全 | `utils/telemetry.rs` |
| 12 | **结构化日志** | TS `LoggingRepository`；Rust 多为 `println!` | 全库 |
| 13 | **`immich-admin` CLI** | 只移植了常用子命令 | `service/admin.rs` |

### P4 — 需实测验证（代码已有，parity 未证明）

| 领域 | 建议验证方式 |
|------|--------------|
| Search v3 筛选 / 游标 | 对比 `search.service.ts` 与真实库查询结果 |
| Sync 全实体 backfill | 多端同步压测 |
| ML 流水线 | 接 live `machine-learning` 容器跑 QueueAll |
| Integrity 大库 | 万级文件 checksum / untracked |
| 拆分 worker | `IMMICH_WORKER_INCLUDE=api` + `microservices` 分开跑 |

---

## 4. 明确暂缓（非阻塞「能用」）

以下在 `AGENTS.md` 中标记为**不要塞进普通 PR** 的大块工作：

| 项 | 原因 |
|----|------|
| ML/OCR/face/duplicate **算法**重写 | 仍调用上游 ML 服务即可 |
| Search v3 **大规模** SQL 重写 | API 已有，边缘对齐即可 |
| Sync **协议级**重写 | `sync.rs` 已很大，优先修 bug |
| Public plugin `allowedHosts` API | 管理面向，非核心路径 |
| EXIF 自动打标 → AssetTagged | 手动 tag API 已可用 |
| PersonRecognized workflow | 上游也注释掉了 |

---

## 5. WebSocket 事件对照

| 事件 | TS | Rust | 状态 |
|------|----|------|------|
| `on_upload_success` | ✓ | ✓ | ✅ |
| `on_asset_delete/trash/update/hidden/restore` | ✓ | ✓ | ✅ |
| `on_asset_stack_update` | ✓ | ✓ | ✅ |
| `on_user_delete` | ✓ | ✓ | ✅ |
| `on_session_delete` | ✓ | ✓（500ms 延迟） | ✅ |
| `on_notification` | ✓ | ✓ | ✅ |
| `on_person_thumbnail` | ✓ | ✓ | ✅ |
| `on_config_update` | ✓ | ✓ | ✅ |
| `on_server_version` / `on_new_release` | ✓ | ✓ | ✅ |
| `AssetUploadReadyV2` / `AssetEditReadyV2` | ✓ | ✓ | ✅ |
| `AppRestartV1` | ✓ | ✓ | ✅ |
| **`on_album_update`** | ✓ | ✓ | ✅ |
| HLS server events（跨进程） | ✓ | 进程内 only | ⚠️ |

---

## 6. 推荐路线图（自行维护 fork 时）

### 阶段 A —「日常能用」加固（1–2 个 PR 批次）

1. ~~修复伙伴/共享链接的**单资产媒体访问**（P0-1）~~ ✅
2. ~~补上 **`on_album_update`**（P0-2）~~ ✅
3. 用真实数据跑一遍：上传 → 元数据 → 缩略图 → 搜索 → 同步

### 阶段 B — 部署模型清晰化

4. 若只跑**单进程**（API+workers 一起）：文档写明即可，HLS 可暂不改  
5. 若要 **API / worker 分离**：优先做 HLS Redis/Socket 协调（P1-3）

### 阶段 C — 工作流与插件

6. AssetV1 null 清空（P2-6）  
7. 按需扩展 workflow 类型（P2-5）  
8. Plugin host 边界测试

### 阶段 D — 工程与长期维护

9. `search` 队列：实现 no-op worker 或从 `ALL_QUEUES` 移除（P3-9）  
10. 补全 telemetry / 日志（P3-11/12）  
11. 评估是否将 Kysely 迁移纯 Rust 化（P3-10，工作量大）  
12. 定期 `main` → `dev-rust` 合并上游，解决 `rust-server` 冲突

---

## 7. 日常维护检查清单

```bash
# 同步分支
git fetch origin main dev-rust

# 测试
cd rust-server && cargo +stable test --offline --lib

# 发布前冒烟（按你的 compose 调整）
# - 登录 / 上传 / 缩略图 / 搜索 / 外部库扫描 / 备份 / 完整性
```

| 操作 | 分支 |
|------|------|
| 合并上游 Immich | `main`，再 PR → `dev-rust` |
| rust 功能开发 | 从 `dev-rust` 拉 `cursor/<name>-4063` |
| 合并 rust 功能 | PR → `dev-rust`，删 `cursor/*` |

详见根目录 **`AGENTS.md`**（Agent 工作流 + 批量合并节奏）。

---

## 8. 关键文件索引

| 关注点 | TypeScript | Rust |
|--------|------------|------|
| 入口 / Worker | `server/src/main.ts`, `workers/*.ts` | `rust-server/src/main.rs`, `service/bootstrap.rs` |
| 路由 | `server/src/controllers/` | `rust-server/src/routes/` |
| 任务枚举 | `server/src/enum.ts` | `rust-server/src/service/job.rs` |
| Worker 注册 | `@OnJob` | `rust-server/src/service/workers/mod.rs` |
| 媒体流水线 | `services/media.service.ts`, `metadata.service.ts` | `rust-server/src/service/media/` |
| 同步 | `services/sync.service.ts` | `rust-server/src/service/sync.rs` |
| 搜索 | `services/search.service.ts` | `rust-server/src/service/search.rs` |
| 工作流执行 | `services/workflow-execution.service.ts` | `rust-server/src/service/workflow_execution.rs` |
| WebSocket | `repositories/websocket.repository.ts` | `rust-server/src/service/websocket.rs` |
| 权限 | `repositories/access.repository.ts` | `rust-server/src/service/access.rs` |

---

## 9. 已合并 PR 摘要（本 fork）

| PR | 内容 |
|----|------|
| #7–#9 | 早期 server parity、sync-main → dev-rust |
| #11–#16 | Workflow 日志、plugin host、library/background 任务状态、R_OK、scan queue |
| #17 | sync-main 全部合入 `dev-rust` |
| #18 | Websocket 版本、UserDelete、ML QueueAll、Sidecar、integrity、dedup 等批量 parity |
| （本切片） | P0：`require_asset_access` 伙伴/相册读权限；`on_album_update` websocket + 共享链接上传通知 |

---

## 10. 一句话总结

**现在：** API 和后台任务主体已在 Rust，`dev-rust` 可支撑单机 Immich 使用；P0 伙伴媒体访问与相册 websocket 已对齐。  
**还差：** 多进程 HLS、工作流细节、运维可观测性、与上游持续同步、真实数据冒烟。  
**策略：** 按本文 §6 分阶段做小 PR，合并到 `dev-rust`，不必每个小点单独发版。
