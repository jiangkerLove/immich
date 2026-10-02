# rust-server 运行与部署

本目录是 fork 自己的说明。服务器部署、本机 Docker、不用 Docker 直接看页面，都写在这里。

网页和 API 是同一个 `rust-server` 进程：构建时把 `web/` 编成静态文件，运行时由 Rust 在 **2283** 读这些文件。开发时也可以让 Vite 单独起页面，把 `/api` 代理到 2283。

## 服务器（拉镜像，一台机器）

`dev-rust` 上相关提交后，GitHub Actions 编译并上传一个镜像（里面已含网页）。仓库 Secrets：`ALIYUN_REGISTRY_USER`、`ALIYUN_REGISTRY_PASSWORD`。

镜像：`registry.cn-hangzhou.aliyuncs.com/jiangker/immich:latest`

在**仓库根目录**（不要在服务器上编译）：

```bash
cp rust-server/example.env .env   # 首次，改 DB_PASSWORD
./deploy
```

浏览器打开 `http://<服务器>:2283`。Postgres、Redis、机器学习与 API 由根目录 `docker-compose.yml` 一起启动。

```bash
docker compose exec immich-server rust-server immich-admin migration-status
docker compose down
```

## 本机 Docker

```bash
cd rust-server
cp example.env .env
docker compose up -d --build
```

打开 **http://localhost:2283**。第一次会编译 Rust 和网页，时间较长。

Postgres 必须是 Immich 的 VectorChord 镜像。Compose 会把 `DB_HOSTNAME` / `REDIS_HOSTNAME` 指到容器名 `database` / `redis`。

叠在上游 `docker/docker-compose.yml` 上时：

```bash
cd rust-server
docker compose -f ../docker/docker-compose.yml -f docker-compose.overlay.yml --env-file ../docker/.env up -d --build
```

## 本机直接运行（不用 Docker）

`cargo run` 可以在本机起 API。它**不会**自带数据库：启动时必须连上 Redis，以及带 VectorChord（或 pgvector）的 PostgreSQL 14+。普通 Postgres 会在启动时 panic。机器学习不参与启动；没有它时页面能开，人脸和智能搜索不可用。上传照片后的缩略图、EXIF 需要本机有 `ffmpeg` 和 `exiftool`。

1. 准备好本机 Postgres（VectorChord）和 Redis，并建好库 `immich`。
2. 在本目录：

```bash
cp example.env .env
```

`.env` 里改成：

```bash
DB_HOSTNAME=127.0.0.1
REDIS_HOSTNAME=127.0.0.1
IMMICH_MEDIA_LOCATION=./library
IMMICH_ENV=development
```

3. 启动 API（当前目录要在 `rust-server/`，才会读到这份 `.env`）：

```bash
cargo run
```

4. 看页面，二选一。

只想尽快看界面：另开一个终端，在仓库根目录用 Vite。它把 `/api` 代理到 `127.0.0.1:2283`，页面在 **3000**：

```bash
pnpm install
pnpm --filter immich-web dev
```

打开 **http://localhost:3000**。

想和服务器一样由 Rust 提供页面：先编静态文件，再让进程读 `web/build`。

```bash
# 仓库根目录
pnpm install
pnpm --filter @immich/sdk --filter immich-web build
```

`.env` 增加 `IMMICH_WEB_ROOT=../web/build`，然后重新 `cargo run`。打开 **http://127.0.0.1:2283**。
