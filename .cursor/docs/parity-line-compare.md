# 逻辑对照记录

官方规格是 `server/src` 里会改变接口结果、任务状态、SQL 或返回字段的实现。改的是 `rust-server/`。行为改动仍记在 `migration-checklist.md` §11。

对照的是函数体里的分支、条件、错误文案、SQL 的过滤和排序、返回字段。文件里有同名函数，不算已经对齐。

## 范围

`server/src` 去掉测试和 `index.ts` 之后，还有下面这些要读的实现，加上表结构：

| 目录 | 文件数 | 对照时看什么 |
|------|--------|----------------|
| `services/` | 55 | 业务分支、任务成功/跳过/失败、错误文案 |
| `repositories/` | 55 | SQL 的 WHERE、ORDER BY、参数编号、空更新 |
| `controllers/` | 46 | 状态码、权限、查询参数名 |
| `dtos/` | 52 | `map*` 拼出来的字段，缺字段会让网页崩 |
| `utils/` | 32 | 日期、权限过滤、模板、校验这类纯逻辑 |
| `middleware/` | 7 | 登录、上传、错误体 |
| `commands/` | 9 | CLI 的退出条件和输出 |
| `maintenance/` | 5 | 维护进程的权限和健康检查 |
| `emails/` | 9 | 邮件标题和正文里的变量 |
| `workers/`、`cores/`、`bin/` 和根目录 | 15 | 启动、存储路径、请求校验、同步列清单 |
| `schema/tables` 与 `schema/migrations` | 163 | 列名和约束。Rust 不重写迁移，查询用到某一列时再对 |
| `queries/` | 36 | 仓库 SQL 的生成结果，随仓库一起对，不单算一份规格 |

上表服务到根文件合计 **285** 个。读过一部分实现的只有几十个。没有任何一个大文件被整份证明和官方相同。

## 怎么记

| 标记 | 含义 |
|------|------|
| 逻辑未对 | 还没有读实现。Rust 里已经有同名函数，也算未对 |
| 逻辑部分对 | 下一节写了已经核对过的分支。没写到的函数仍然未对 |

读完一段实现后，改对应行：写清条件、返回值和 SQL，不要只写函数名。有差异就改 Rust，并在清单 §11 加一行。

## 故意不抄

| 位置 | 官方实现 | Rust 保持的实现 |
|------|----------|-----------------|
| 相册同步 SQL | `DISTINCT ON (album.id, album.updateId)` 的 ORDER BY 对不上，语句无效 | 按 `updateId` 排序 |
| 活动搜索 | 左连接照片后再要求照片未删除，相册级评论被滤掉 | 照片不存在时仍保留这条活动 |
| 通知详情 | 条件是 `deletedAt IS NOT NULL`，只能打开已删通知 | 只打开未删除的通知 |
| 共享链接更新 | 没传 slug 时写成 null，短链被清空 | 请求里没有这个字段就不改 |
| 改密码 | 参数 `invalidateSessions` 没有被使用，其他会话总会失效 | 同样总会失效 |
| 恢复后的健康检查 | 拉起 Node 进程等启动日志 | 做结构检查。失败时用恢复点回滚 |
| 探索城市 | `DISTINCT ON (city)` 没有 ORDER BY | `ORDER BY city` 后取 12 条 |
| 合并人物 | 主人物没有名字、对方有名字时，冲突判断为真，名字不会被抄过来 | 保持这个跳过 |
| 改相册成员角色 | 用排序后的第一个成员判断是不是 owner，编辑者可能被误伤 | 看被修改的那个用户自己的角色 |

## 已核对过的实现

只列读过函数体的文件。每一格是核对过的逻辑，不是函数名单。

| 文件 | 已核对的逻辑 |
|------|----------------|
| `dtos/asset-response.dto.ts` | 堆张数是成员列表长度加 1，成员列表不含主照片。人物按人物组去重。共享链接不返回人物。 |
| `dtos/sync.dto.ts` | 相册 v1 的拥有者取角色为 owner 的那个成员。 |
| `middleware/auth.guard.ts` | 时间线、搜索、资产统计在需要提升权限时返回 401，文案是 Elevated permission is required。守卫里其余分支还没读。 |
| `database.ts` | 同步用的资源列、相册资源列、伙伴资源列、EXIF 列读过，并和 Rust 的选出列对过。同文件里给接口用的其他列清单还没读。 |
| `services/workflow-execution.service.ts` | 找不到工作流当成功。步骤方法名以 noop 开头则跳过且不看 continue。`continue` 缺省为 true，false 时停住，日志是 halted 并带步骤。插件抛错记 error、带步骤，任务失败。全部走完记 completed 且不带步骤。只有 logging 为真才写日志。`config` 为 null、false、0 或空字符串时不写回，空对象会写回。写回资源时评分 null 会清空评分；纬度、经度和描述的 null 不改原值。插件加载和宿主函数还没读。 |
| `repositories/album.repository.ts` | 相册照片数和起止日期只算未删除、可见性为 archive 或 timeline 的照片。成员按角色排序；判断 owner 时看该用户自己的角色，不看排序后的第一个人。 |
| `repositories/asset.repository.ts` | 堆详情的张数等于时间线上、未删除、且不是主照片的张数再加 1。删除任务用的成员列表同样排除主照片、已删除和非时间线，并按 `fileCreatedAt` 从早到晚。其余关联查询还没读。 |
| `repositories/map.repository.ts` | 默认时间线标记的 SQL 参数从第一个占位符连续编号，未归档时不引用查看者。 |
| `repositories/person.repository.ts` | 人物列表按主键分组。找相似脸先未隐藏、再收藏、再距离，空距离在最后。统计是查看者自己的时间线照片，或查看者所在未删除共享相册里的照片，脸未删且可见。 |
| `repositories/search.repository.ts` | 建议查询的列名加引号。旧版元数据结果在拍摄时间之后用资源 id 打破平局。智能搜索在向量距离之后用资源 id 升序。标签搜索用闭包，父标签能命中子标签，条件是数量大于等于。 |
| `repositories/session.repository.ts` | 和用户表联查时，会话列都带表别名，避免 `id` 歧义。过期条件是空过期时间或晚于现在。 |
| `repositories/sync.repository.ts` | 相册、人物、回忆、资源、EXIF 的选出列和同步服务里写的一致。其余实体的选出列还没读。 |
| `services/activity.service.ts` | 点赞会先按相册、用户、资源查找已有点赞，有则 duplicate。评论直接插入。搜索把相册级评论留下来。统计 SQL 和删除 SQL 还没读。 |
| `services/album.service.ts` | 批量加入时，封面取请求顺序里第一张还不在相册中的照片。不能把别人加成 owner。最后一个 owner 不能被移出。封面资源必须已经在这个相册里。移除资源后重选封面的 SQL 还没读。 |
| `services/asset-media.service.ts` | 缩略图 `size=original` 返回 400。网页能直接显示的全尺寸图仍跳到原图。共享链接取原图和缩略图时强制走编辑后的文件。播放和上传校验还没读。 |
| `services/asset.service.ts` | 没有任何列变化时不执行 UPDATE。可见性改成 locked 会从所有相册拿掉。单条更新只在 EXIF 变了才排队写边车，批量更新总是排队。删堆栈时，主照片只要没被删就算剩余；新封面是最早的时间线成员。编辑拒绝顺序是：非图片、实况、全景、gif、svg、没有尺寸、裁剪不是第一步、裁剪越界。方向 5/6/7/8 会交换宽高。`run` 和自定义元数据键还没读。 |
| `services/auth.service.ts` | 改密码不看 `invalidateSessions`，其他会话都会失效。OAuth 在用户还没有 oauthId 时按邮箱绑定；邮箱已经绑了别的 subject 则失败；没开自动注册或资料没有邮箱都有固定文案。角色声明出现时才改管理员。配额按 GiB 乘 1024 的三次方，0 是合法值。token 交换失败的分支还没读。 |
| `services/database-backup.service.ts` | 恢复 SQL 之后如果迁移、没有管理员或结构检查失败，用恢复点重放并重建 public schema。集群转储只在正向恢复时切到 postgres 库。备份文件名和上传前缀读过。列表怎么排序还没逐行对比较器。 |
| `services/download.service.ts` | 拆包在加入当前文件后再判断是否超过 4GiB。安卓动态照片在偏好关闭时不打进包。压缩包条目用原始文件名，第二个同名是 `名+1.扩展名`。 |
| `services/duplicate.service.ts` | 还没有组 id 时，用搜索结果里第一次出现的那个。纬度和经度必须都只有一个值才写入。描述按换行和回车拆开再去重。可见性按锁定、归档、时间线、否则隐藏。机器学习关闭、没有嵌入、已堆叠或隐藏或锁定时的任务状态读过。列表 SQL 还没读。 |
| `services/hls.service.ts` | 主播放列表带宽是码率乘 1.35 后取整，编码串包含 `mp4a.40.2`，帧率三位小数。高于源分辨率或不在配置里的变体丢掉。一个变体都没有则 404。ffmpeg 命令行还没读。 |
| `services/library.service.ts` | 文件不在磁盘且已经离线则不动；已经离线且状态不是 deleted 才检查是否回到导入路径、是否仍被排除。mtime 不同才排队更新。回收站里的离线只改 `isOffline`，不清 `deletedAt`。文件监听的增删改事件还没读。 |
| `services/maintenance.service.ts` | API 进程的维护状态固定是未进行。检测旧安装时，API 要管理员，维护进程不要登录。进入维护和维修动作还没读。 |
| `services/map.service.ts` | 未要求包含归档时，标记查询不给查看者留一个空的参数位。反向地理编码返回几条、字段叫什么还没读。 |
| `services/memory.service.ts` | 更新只接受是否保存、记忆时间和看过时间。这三个都没传时不发 UPDATE。往年回忆的序列结束在去年。挑哪些照片、空回忆怎么滤掉还没读。 |
| `services/metadata.service.ts` | 没有 EXIF 日期时，用文件创建时间和 mtime 里更早的，birthtime 小于等于 0 不用，库存的 `fileModifiedAt` 不参与。GPS 正好是 0,0 不算有位置。HEIF 的 Rotation 映射到方向。探测过视频，或文件不可能是动画时，先删掉 Duration 再写探测时长。关键词优先 `TagsList`，空数组也不改用后面的层级主题或 Keywords；层级主题把 `|` 换成 `/`，数字原样转成字符串。没有关键词时清空这张照片上的标签关联，标签被锁定时不动。相机、镜头、评分 0 变空、位深、连拍 id 的取值顺序读过。动态照片怎么拆文件还没读。 |
| `services/notification.service.ts` | 列表只要未删除的，按创建时间降序。未读是 `readAt` 为空。打开单条也只找未删除的，不跟随官方那条只会命中已删记录的条件。邮件正文还没读。 |
| `services/ocr.service.ts` | 机器学习里 OCR 关闭则整批和单张都跳过。单张先看有没有预览文件，没有则失败，然后再看是不是隐藏。文字按顺序每 8 个数一组画框，搜索词用同一套分词。 |
| `services/partner.service.ts` | 创建、更新、移除的错误文案读过。搜索会去掉已删除用户。时间线开关写哪一列还没读。 |
| `services/person.service.ts` | 重分配会挪走这张照片上该人物的每一张未删脸；一张都没挪到就不换封面。合并时对方名字或生日和主人物不同就整个人跳过，主人物名字为空也算冲突。检测旧框宽或高为 0 时分母按 1。识别任务在关闭、找不到、非机器学习来源、没嵌入、已有人物时跳过或失败。非核心且未延期会再排队并跳过。已有人物组只改挂脸；没有组且是核心脸时新建组和个人、把这张脸设成封面并排队缩略图，然后再改挂。 |
| `services/search.service.ts` | 建议接口读 `lensModel`、`includeNull`。城市建议不套相机条件，镜头列名带引号。探索城市按城市名排序取 12 个。锁定搜索只含本人并要提升权限。共享链接做旧版元数据搜索时必须带相册。旧版排序是拍摄时间再加资源 id，智能搜索是距离再加 id 升序。v3 每种运算符还没读。 |
| `services/session.service.ts` | 创建接口没有会话令牌就拒绝。没有任何字段时更新返回 400。列表丢掉已过期会话，按更新时间再按创建时间降序。90 天清理和锁定的 SQL 还没读。 |
| `services/system-config.service.ts` | 只有 `IMMICH_CONFIG_FILE` 非空才拒绝在网页里改配置。空字符串不当成配置文件，设置页里的开关可以点。返回给页面的配置会补上实时转码的编码和分辨率、OAuth 账号管理地址、任务并发里的 `notifications`。库存把某一段写成 null 时，用默认值补回，避免展开那一段时页面读不到字段。保存校验和日志环境变量那几条还没逐项读完。 |
| `services/shared-link.service.ts` | 创建时 `showMetadata` 为 false 会强制关掉下载和 EXIF。更新不会因为这个字段再改下载开关。登录用库存明文和提交的密码比较。请求里没带 slug 时不清空已有短链。加入和移出资源的逐条结果还没读。 |
| `services/stack.service.ts` | 新封面必须已经在这个堆里。不能把主照片移出堆。详情上的张数在资源查询里算。搜索和批量删除还没读。 |
| `services/sync.service.ts` | 相册 v1 从成员里找 owner 填 `ownerId`，描述空值变成空字符串。资源和 EXIF 的同步列、相册资源的收藏 CASE、伙伴资源收藏恒为 false、人物列、回忆列、回忆关联的 `memoriesId` 读过。用户元数据、OCR、编辑、人脸 v2 每一列还没读。 |
| `services/tag.service.ts` | 创建前检查同名。重命名只换路径最后一段，空名字保留原值。请求里 `color` 为 null 才清空颜色，不传则保留。批量打标签的逐条结果还没读。 |
| `services/timeline.service.ts` | 带人物时，照片可以是查看者自己的，也可以在查看者所属、未删除的共享相册里。堆叠格子上的数量是该堆里全部未删时间线照片，含封面。其他筛选还没读。 |
| `services/transcoding.service.ts` | 策略 disabled、all、required、optimal、bitrate 和 remux 的选择读过。不带单位的码率按比特每秒，不乘 1000。编码参数还没读。 |
| `services/trash.service.ts` | 空 id 列表恢复数量是 0。按 id 恢复要求删除权限。清空时数量大于 0 才排队彻底删除。恢复和清空的 SQL 条件还没读。 |
| `services/user.service.ts` | 保存偏好时只写入和默认值不同的字段，读出来再合并成完整对象。用户列表按创建时间降序。当前用户找不到是 400。改邮箱和头像上传还没读。 |
| `services/view.service.ts` | 文件夹是原路径的目录段，只要时间线、未删除、创建时间不为空的照片。某一层的文件是该目录前缀且不是更深一层。 |
| `services/workflow.service.ts` | 创建时步骤列表是空的。分享出去时，没写 `enabled` 表示开启，写了 false 表示关闭。未知插件方法和触发器对不上的错误文案读过。步骤真正怎么执行还没读。 |
| `utils/asset.util.ts` | 尺寸来自 EXIF 宽高，方向是 5、6、7、8、-90、90 时对调。全景是投影 EQUIRECTANGULAR 或文件名以 .insp 结尾。 |
| `utils/mime-types.ts` | 可能是动画的扩展名包含 avif、gif、heic、heif、jxl、png、webp。HEIF 还包含 avif 和 hif。 |
| `utils/preferences.ts` | 和默认对象逐叶比较，只有不同的键会存进数据库。 |
| `utils/search-filter.ts` | 没有提升权限时，锁定可见性不能被搜到，默认再加上排除锁定。 |
| `utils/workflow.ts` | AssetCreate、AssetMetadataExtraction、AssetTagged 能接到 AssetV1。未知方法名和不兼容触发的判断读过。 |

## 全量清单

285 个实现文件都在这里。细节看上一节。这一节用来防止漏文件。

### （根目录）（9，已读逻辑 1）

| 文件 | 状态 |
|------|------|
| `app.common.ts` | 逻辑未对 |
| `app.module.ts` | 逻辑未对 |
| `constants.ts` | 逻辑未对 |
| `database.ts` | 逻辑部分对 |
| `decorators.ts` | 逻辑未对 |
| `enum.ts` | 逻辑未对 |
| `main.ts` | 逻辑未对 |
| `types.ts` | 逻辑未对 |
| `validation.ts` | 逻辑未对 |

### bin（2，已读逻辑 0）

| 文件 | 状态 |
|------|------|
| `sync-open-api.ts` | 逻辑未对 |
| `sync-sql.ts` | 逻辑未对 |

### commands（9，已读逻辑 0）

| 文件 | 状态 |
|------|------|
| `grant-admin.ts` | 逻辑未对 |
| `list-users.command.ts` | 逻辑未对 |
| `maintenance-mode.ts` | 逻辑未对 |
| `media-location.command.ts` | 逻辑未对 |
| `oauth-login.ts` | 逻辑未对 |
| `password-login.ts` | 逻辑未对 |
| `reset-admin-password.command.ts` | 逻辑未对 |
| `schema-check.ts` | 逻辑未对 |
| `version.command.ts` | 逻辑未对 |

### controllers（46，已读逻辑 0）

| 文件 | 状态 |
|------|------|
| `activity.controller.ts` | 逻辑未对 |
| `album.controller.ts` | 逻辑未对 |
| `api-key.controller.ts` | 逻辑未对 |
| `app.controller.ts` | 逻辑未对 |
| `asset-file.controller.ts` | 逻辑未对 |
| `asset-media.controller.ts` | 逻辑未对 |
| `asset.controller.ts` | 逻辑未对 |
| `auth-admin.controller.ts` | 逻辑未对 |
| `auth.controller.ts` | 逻辑未对 |
| `cluster-group.controller.ts` | 逻辑未对 |
| `config-admin.controller.ts` | 逻辑未对 |
| `config-public.controller.ts` | 逻辑未对 |
| `config-user.controller.ts` | 逻辑未对 |
| `database-backup.controller.ts` | 逻辑未对 |
| `download.controller.ts` | 逻辑未对 |
| `duplicate.controller.ts` | 逻辑未对 |
| `face.controller.ts` | 逻辑未对 |
| `integrity-admin.controller.ts` | 逻辑未对 |
| `job.controller.ts` | 逻辑未对 |
| `library.controller.ts` | 逻辑未对 |
| `maintenance.controller.ts` | 逻辑未对 |
| `map.controller.ts` | 逻辑未对 |
| `memory.controller.ts` | 逻辑未对 |
| `notification-admin.controller.ts` | 逻辑未对 |
| `notification.controller.ts` | 逻辑未对 |
| `oauth.controller.ts` | 逻辑未对 |
| `partner.controller.ts` | 逻辑未对 |
| `person.controller.ts` | 逻辑未对 |
| `plugin.controller.ts` | 逻辑未对 |
| `queue.controller.ts` | 逻辑未对 |
| `search.controller.ts` | 逻辑未对 |
| `server.controller.ts` | 逻辑未对 |
| `session.controller.ts` | 逻辑未对 |
| `shared-link.controller.ts` | 逻辑未对 |
| `stack.controller.ts` | 逻辑未对 |
| `sync.controller.ts` | 逻辑未对 |
| `system-config.controller.ts` | 逻辑未对 |
| `system-metadata.controller.ts` | 逻辑未对 |
| `tag.controller.ts` | 逻辑未对 |
| `timeline.controller.ts` | 逻辑未对 |
| `trash.controller.ts` | 逻辑未对 |
| `user-admin.controller.ts` | 逻辑未对 |
| `user.controller.ts` | 逻辑未对 |
| `video-stream.controller.ts` | 逻辑未对 |
| `view.controller.ts` | 逻辑未对 |
| `workflow.controller.ts` | 逻辑未对 |

### cores（1，已读逻辑 0）

| 文件 | 状态 |
|------|------|
| `storage.core.ts` | 逻辑未对 |

### dtos（52，已读逻辑 2）

| 文件 | 状态 |
|------|------|
| `activity.dto.ts` | 逻辑未对 |
| `album.dto.ts` | 逻辑未对 |
| `api-key.dto.ts` | 逻辑未对 |
| `asset-file.dto.ts` | 逻辑未对 |
| `asset-ids.response.dto.ts` | 逻辑未对 |
| `asset-media-response.dto.ts` | 逻辑未对 |
| `asset-media.dto.ts` | 逻辑未对 |
| `asset-response.dto.ts` | 逻辑部分对 |
| `asset.dto.ts` | 逻辑未对 |
| `auth.dto.ts` | 逻辑未对 |
| `bbox.dto.ts` | 逻辑未对 |
| `calendar-heatmap.dto.ts` | 逻辑未对 |
| `cluster-group.dto.ts` | 逻辑未对 |
| `config.dto.ts` | 逻辑未对 |
| `database-backup.dto.ts` | 逻辑未对 |
| `download.dto.ts` | 逻辑未对 |
| `duplicate.dto.ts` | 逻辑未对 |
| `editing.dto.ts` | 逻辑未对 |
| `env.dto.ts` | 逻辑未对 |
| `exif.dto.ts` | 逻辑未对 |
| `integrity.dto.ts` | 逻辑未对 |
| `job.dto.ts` | 逻辑未对 |
| `json-schema.dto.ts` | 逻辑未对 |
| `library.dto.ts` | 逻辑未对 |
| `license.dto.ts` | 逻辑未对 |
| `maintenance.dto.ts` | 逻辑未对 |
| `map.dto.ts` | 逻辑未对 |
| `memory.dto.ts` | 逻辑未对 |
| `notification.dto.ts` | 逻辑未对 |
| `ocr.dto.ts` | 逻辑未对 |
| `onboarding.dto.ts` | 逻辑未对 |
| `partner.dto.ts` | 逻辑未对 |
| `person.dto.ts` | 逻辑未对 |
| `plugin-manifest.dto.ts` | 逻辑未对 |
| `plugin.dto.ts` | 逻辑未对 |
| `queue-legacy.dto.ts` | 逻辑未对 |
| `queue.dto.ts` | 逻辑未对 |
| `search.dto.ts` | 逻辑未对 |
| `server.dto.ts` | 逻辑未对 |
| `session.dto.ts` | 逻辑未对 |
| `shared-link.dto.ts` | 逻辑未对 |
| `stack.dto.ts` | 逻辑未对 |
| `streaming.dto.ts` | 逻辑未对 |
| `sync.dto.ts` | 逻辑部分对 |
| `system-metadata.dto.ts` | 逻辑未对 |
| `tag.dto.ts` | 逻辑未对 |
| `time-bucket.dto.ts` | 逻辑未对 |
| `trash.dto.ts` | 逻辑未对 |
| `user-preferences.dto.ts` | 逻辑未对 |
| `user-profile.dto.ts` | 逻辑未对 |
| `user.dto.ts` | 逻辑未对 |
| `workflow.dto.ts` | 逻辑未对 |

### emails（9，已读逻辑 0）

| 文件 | 状态 |
|------|------|
| `album-invite.email.tsx` | 逻辑未对 |
| `album-update.email.tsx` | 逻辑未对 |
| `button.component.tsx` | 逻辑未对 |
| `footer.template.tsx` | 逻辑未对 |
| `futo.layout.tsx` | 逻辑未对 |
| `immich.layout.tsx` | 逻辑未对 |
| `license.email.tsx` | 逻辑未对 |
| `test.email.tsx` | 逻辑未对 |
| `welcome.email.tsx` | 逻辑未对 |

### maintenance（5，已读逻辑 0）

| 文件 | 状态 |
|------|------|
| `maintenance-auth.guard.ts` | 逻辑未对 |
| `maintenance-health.repository.ts` | 逻辑未对 |
| `maintenance-websocket.repository.ts` | 逻辑未对 |
| `maintenance-worker.controller.ts` | 逻辑未对 |
| `maintenance-worker.service.ts` | 逻辑未对 |

### middleware（7，已读逻辑 1）

| 文件 | 状态 |
|------|------|
| `asset-upload.interceptor.ts` | 逻辑未对 |
| `auth.guard.ts` | 逻辑部分对 |
| `error.interceptor.ts` | 逻辑未对 |
| `file-upload.interceptor.ts` | 逻辑未对 |
| `global-exception.filter.ts` | 逻辑未对 |
| `logging.interceptor.ts` | 逻辑未对 |
| `websocket.adapter.ts` | 逻辑未对 |

### repositories（55，已读逻辑 7）

| 文件 | 状态 |
|------|------|
| `access.repository.ts` | 逻辑未对 |
| `activity.repository.ts` | 逻辑未对 |
| `album-user.repository.ts` | 逻辑未对 |
| `album.repository.ts` | 逻辑部分对 |
| `api-key.repository.ts` | 逻辑未对 |
| `app.repository.ts` | 逻辑未对 |
| `asset-edit.repository.ts` | 逻辑未对 |
| `asset-file.repository.ts` | 逻辑未对 |
| `asset-job.repository.ts` | 逻辑未对 |
| `asset.repository.ts` | 逻辑部分对 |
| `cluster-group.repository.ts` | 逻辑未对 |
| `config.repository.ts` | 逻辑未对 |
| `cron.repository.ts` | 逻辑未对 |
| `crypto.repository.ts` | 逻辑未对 |
| `database.repository.ts` | 逻辑未对 |
| `download.repository.ts` | 逻辑未对 |
| `duplicate.repository.ts` | 逻辑未对 |
| `email.repository.ts` | 逻辑未对 |
| `event.repository.ts` | 逻辑未对 |
| `integrity.repository.ts` | 逻辑未对 |
| `job.repository.ts` | 逻辑未对 |
| `library.repository.ts` | 逻辑未对 |
| `logging.repository.ts` | 逻辑未对 |
| `machine-learning.repository.ts` | 逻辑未对 |
| `map.repository.ts` | 逻辑部分对 |
| `media.repository.ts` | 逻辑未对 |
| `memory.repository.ts` | 逻辑未对 |
| `metadata.repository.ts` | 逻辑未对 |
| `move.repository.ts` | 逻辑未对 |
| `notification.repository.ts` | 逻辑未对 |
| `oauth.repository.ts` | 逻辑未对 |
| `ocr.repository.ts` | 逻辑未对 |
| `partner.repository.ts` | 逻辑未对 |
| `person.repository.ts` | 逻辑部分对 |
| `plugin.repository.ts` | 逻辑未对 |
| `process.repository.ts` | 逻辑未对 |
| `search.repository.ts` | 逻辑部分对 |
| `server-info.repository.ts` | 逻辑未对 |
| `session.repository.ts` | 逻辑部分对 |
| `shared-link-asset.repository.ts` | 逻辑未对 |
| `shared-link.repository.ts` | 逻辑未对 |
| `stack.repository.ts` | 逻辑未对 |
| `storage.repository.ts` | 逻辑未对 |
| `sync-checkpoint.repository.ts` | 逻辑未对 |
| `sync.repository.ts` | 逻辑部分对 |
| `system-metadata.repository.ts` | 逻辑未对 |
| `tag.repository.ts` | 逻辑未对 |
| `telemetry.repository.ts` | 逻辑未对 |
| `trash.repository.ts` | 逻辑未对 |
| `user.repository.ts` | 逻辑未对 |
| `version-history.repository.ts` | 逻辑未对 |
| `video-stream.repository.ts` | 逻辑未对 |
| `view-repository.ts` | 逻辑未对 |
| `websocket.repository.ts` | 逻辑未对 |
| `workflow.repository.ts` | 逻辑未对 |

### services（55，已读逻辑 32）

| 文件 | 状态 |
|------|------|
| `activity.service.ts` | 逻辑部分对 |
| `album.service.ts` | 逻辑部分对 |
| `api-key.service.ts` | 逻辑未对 |
| `api.service.ts` | 逻辑未对 |
| `asset-file.service.ts` | 逻辑未对 |
| `asset-media.service.ts` | 逻辑部分对 |
| `asset.service.ts` | 逻辑部分对 |
| `auth-admin.service.ts` | 逻辑未对 |
| `auth.service.ts` | 逻辑部分对 |
| `base.service.ts` | 逻辑未对 |
| `cli.service.ts` | 逻辑未对 |
| `cluster-group.service.ts` | 逻辑未对 |
| `database-backup.service.ts` | 逻辑部分对 |
| `database.service.ts` | 逻辑未对 |
| `download.service.ts` | 逻辑部分对 |
| `duplicate.service.ts` | 逻辑部分对 |
| `hls.service.ts` | 逻辑部分对 |
| `integrity.service.ts` | 逻辑未对 |
| `job.service.ts` | 逻辑未对 |
| `library.service.ts` | 逻辑部分对 |
| `maintenance.service.ts` | 逻辑部分对 |
| `map.service.ts` | 逻辑部分对 |
| `media.service.ts` | 逻辑未对 |
| `memory.service.ts` | 逻辑部分对 |
| `metadata.service.ts` | 逻辑部分对 |
| `notification-admin.service.ts` | 逻辑未对 |
| `notification.service.ts` | 逻辑部分对 |
| `ocr.service.ts` | 逻辑部分对 |
| `partner.service.ts` | 逻辑部分对 |
| `person.service.ts` | 逻辑部分对 |
| `plugin.service.ts` | 逻辑未对 |
| `queue.service.ts` | 逻辑未对 |
| `search.service.ts` | 逻辑部分对 |
| `server.service.ts` | 逻辑未对 |
| `session.service.ts` | 逻辑部分对 |
| `user-methods.ts` | 逻辑未对 |
| `shared-link.service.ts` | 逻辑部分对 |
| `smart-info.service.ts` | 逻辑未对 |
| `stack.service.ts` | 逻辑部分对 |
| `storage-template.service.ts` | 逻辑未对 |
| `storage.service.ts` | 逻辑未对 |
| `sync.service.ts` | 逻辑部分对 |
| `system-config.service.ts` | 逻辑部分对 |
| `system-metadata.service.ts` | 逻辑未对 |
| `tag.service.ts` | 逻辑部分对 |
| `telemetry.service.ts` | 逻辑未对 |
| `timeline.service.ts` | 逻辑部分对 |
| `transcoding.service.ts` | 逻辑部分对 |
| `trash.service.ts` | 逻辑部分对 |
| `user-admin.service.ts` | 逻辑未对 |
| `user.service.ts` | 逻辑部分对 |
| `version.service.ts` | 逻辑未对 |
| `view.service.ts` | 逻辑部分对 |
| `workflow-execution.service.ts` | 逻辑部分对 |
| `workflow.service.ts` | 逻辑部分对 |

### utils（32，已读逻辑 5）

| 文件 | 状态 |
|------|------|
| `access.ts` | 逻辑未对 |
| `asset.util.ts` | 逻辑部分对 |
| `bytes.ts` | 逻辑未对 |
| `config.ts` | 逻辑未对 |
| `database-backups.ts` | 逻辑未对 |
| `database.ts` | 逻辑未对 |
| `date.ts` | 逻辑未对 |
| `duplicate.ts` | 逻辑未对 |
| `editor.ts` | 逻辑未对 |
| `event.ts` | 逻辑未对 |
| `fetch.ts` | 逻辑未对 |
| `file.ts` | 逻辑未对 |
| `logger.ts` | 逻辑未对 |
| `maintenance.ts` | 逻辑未对 |
| `media.ts` | 逻辑未对 |
| `mime-types.ts` | 逻辑部分对 |
| `misc.ts` | 逻辑未对 |
| `object.ts` | 逻辑未对 |
| `pagination.ts` | 逻辑未对 |
| `preferences.ts` | 逻辑部分对 |
| `profile-image.ts` | 逻辑未对 |
| `replace-template-tags.ts` | 逻辑未对 |
| `request.ts` | 逻辑未对 |
| `response.ts` | 逻辑未对 |
| `search-cursor.ts` | 逻辑未对 |
| `search-filter.ts` | 逻辑部分对 |
| `set.ts` | 逻辑未对 |
| `sync.ts` | 逻辑未对 |
| `tag.ts` | 逻辑未对 |
| `tasks.ts` | 逻辑未对 |
| `transform.ts` | 逻辑未对 |
| `workflow.ts` | 逻辑部分对 |

### workers（3，已读逻辑 0）

| 文件 | 状态 |
|------|------|
| `api.ts` | 逻辑未对 |
| `maintenance.ts` | 逻辑未对 |
| `microservices.ts` | 逻辑未对 |

## 下次从这里继续

按文件读函数体，不要只核对有没有这个函数。建议顺序：

1. `services/metadata.service.ts` 里动态照片怎么拆出内嵌视频
2. `repositories/sync.repository.ts` 里用户元数据、OCR、编辑、人脸的选出列
3. `services/shared-link.service.ts` 的加入资源和移出资源，每条成功、重复、无权限怎么返回
4. `repositories/trash.repository.ts` 的恢复和清空条件
5. `emails/` 里邀请、更新、欢迎信的变量
6. 服务逻辑对过之后，再读对应的 `controllers/` 状态码

