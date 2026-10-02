# rust-server 运行与部署

本目录是 fork 自己的说明。服务器部署、本机 Docker、不用 Docker 直接看页面，都写在这里。

网页和 API 是同一个 `rust-server` 进程：构建时把 `web/` 编成静态文件，运行时由 Rust 在 **2283** 读这些文件。开发时也可以让 Vite 单独起页面，把 `/api` 代理到 2283。

## 服务器：只建两个文件

不用克隆仓库，也不用在服务器上编译。网页和 API 已经打进镜像。在空目录里建下面两个文件，放在一起。

镜像都在 `registry.cn-hangzhou.aliyuncs.com/jiangker/`。`immich` 由 `dev-rust` 提交后自动编译。另外三张是转存：在 GitHub Actions 里手动运行 **Mirror runtime images**（Secrets 仍是 `ALIYUN_REGISTRY_USER`、`ALIYUN_REGISTRY_PASSWORD`）。三张镜像分开检查，摘要没变就跳过，只有上游变了才重新下载并上传。阿里云若没有自动建仓库，先建好 `immich-machine-learning`、`valkey`、`immich-postgres`。

**`.env`**

```bash
UPLOAD_LOCATION=./library
DB_DATA_LOCATION=./postgres
DB_PASSWORD=改成一串字母和数字
DB_USERNAME=postgres
DB_DATABASE_NAME=immich
```

**`docker-compose.yml`**

Postgres、Redis、机器学习、API 和网页在这一份里一起启动。文件第一行必须是 `services:`。不要把 \`\`\`yaml、`cat` 命令写进文件；镜像地址里的冒号要用引号包起来。

```yaml
services:
  immich-server:
    container_name: immich_server
    image: "registry.cn-hangzhou.aliyuncs.com/jiangker/immich:latest"
    command: ["rust-server"]
    volumes:
      - ${UPLOAD_LOCATION}:/data
      - /etc/localtime:/etc/localtime:ro
    env_file:
      - .env
    environment:
      DB_HOSTNAME: database
      REDIS_HOSTNAME: redis
      IMMICH_MEDIA_LOCATION: /data
      IMMICH_SERVER_PATH: /usr/src/app/server
      IMMICH_WEB_ROOT: /build/www
    ports:
      - "2283:2283"
    depends_on:
      - redis
      - database
    restart: always

  immich-machine-learning:
    container_name: immich_machine_learning
    image: "registry.cn-hangzhou.aliyuncs.com/jiangker/immich-machine-learning:release"
    volumes:
      - model-cache:/cache
    env_file:
      - .env
    restart: always

  redis:
    container_name: immich_redis
    image: "registry.cn-hangzhou.aliyuncs.com/jiangker/valkey:9"
    restart: always

  database:
    container_name: immich_postgres
    image: "registry.cn-hangzhou.aliyuncs.com/jiangker/immich-postgres:14-vectorchord0.4.3-pgvectors0.2.0"
    environment:
      POSTGRES_PASSWORD: ${DB_PASSWORD}
      POSTGRES_USER: ${DB_USERNAME}
      POSTGRES_DB: ${DB_DATABASE_NAME}
      POSTGRES_INITDB_ARGS: "--data-checksums"
    volumes:
      - ${DB_DATA_LOCATION}:/var/lib/postgresql/data
    shm_size: 128mb
    restart: always

volumes:
  model-cache:
```

```bash
docker login registry.cn-hangzhou.aliyuncs.com
docker compose pull
docker compose up -d
```

浏览器打开 `http://<服务器>:2283`。照片在 `./library`，数据库在 `./postgres`。

```bash
docker compose ps
docker compose logs -f immich-server
docker compose down
```

已经克隆了仓库时，在根目录 `cp rust-server/example.env .env` 后执行 `./deploy`，效果一样。

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
