# syntax=docker/dockerfile:1

FROM rust:1.85-bookworm AS rust-builder

WORKDIR /build/rust-server
COPY rust-server/Cargo.toml rust-server/build.rs ./
# Lockfile is optional in the git tree; generate when missing.
COPY rust-server/Cargo.lock* ./
COPY rust-server/src ./src
COPY rust-server/migrations ./migrations
COPY rust-server/schema ./schema
COPY rust-server/config ./config
COPY server/src/schema/migrations /build/server/src/schema/migrations
RUN if [ ! -f Cargo.lock ]; then cargo generate-lockfile; fi \
  && cargo build --release

FROM ghcr.io/immich-app/base-server-dev:202606161235@sha256:9f88b07acc8b7bf37a1dd3d5a19193f664443eaaab4e08e9f9341414c5e4b23f AS node-builder

ENV COREPACK_ENABLE_DOWNLOAD_PROMPT=0 \
  CI=1 \
  COREPACK_HOME=/tmp \
  PNPM_HOME=/buildcache/pnpm-store \
  PATH="/buildcache/pnpm-store:$PATH"

RUN npm install --global corepack@latest && \
  corepack enable pnpm && \
  pnpm config set store-dir "$PNPM_HOME"

FROM node-builder AS server-builder

WORKDIR /usr/src/app
COPY ./server ./server/
COPY ./packages/sdk ./packages/sdk/
COPY ./packages/plugin-sdk ./packages/plugin-sdk/
RUN --mount=type=cache,id=pnpm-server,target=/buildcache/pnpm-store \
  --mount=type=bind,source=package.json,target=package.json \
  --mount=type=bind,source=.pnpmfile.cjs,target=.pnpmfile.cjs \
  --mount=type=bind,source=pnpm-lock.yaml,target=pnpm-lock.yaml \
  --mount=type=bind,source=pnpm-workspace.yaml,target=pnpm-workspace.yaml \
  SHARP_IGNORE_GLOBAL_LIBVIPS=true pnpm --filter @immich/sdk --filter @immich/plugin-sdk --filter immich build && \
  SHARP_FORCE_GLOBAL_LIBVIPS=true pnpm --filter immich --prod --no-optional deploy /output/server-pruned

FROM node-builder AS web-builder

ARG BUILD_ID
ENV IMMICH_BUILD=${BUILD_ID}

WORKDIR /usr/src/app
COPY ./web ./web/
COPY ./i18n ./i18n/
COPY ./packages ./packages/
RUN --mount=type=cache,id=pnpm-web,target=/buildcache/pnpm-store \
  --mount=type=bind,source=package.json,target=package.json \
  --mount=type=bind,source=.pnpmfile.cjs,target=.pnpmfile.cjs \
  --mount=type=bind,source=pnpm-lock.yaml,target=pnpm-lock.yaml \
  --mount=type=bind,source=pnpm-workspace.yaml,target=pnpm-workspace.yaml \
  SHARP_IGNORE_GLOBAL_LIBVIPS=true pnpm --filter @immich/sdk --filter immich-web install --frozen-lockfile --force && \
  pnpm --filter @immich/sdk --filter immich-web build

FROM ghcr.io/immich-app/base-server-prod:202606161235@sha256:c6d59e3923f548d29a212b4dc51b6281a722cfa1da7972a009c0f3830f5762d6

WORKDIR /usr/src/app
ENV NODE_ENV=production \
  NVIDIA_DRIVER_CAPABILITIES=all \
  NVIDIA_VISIBLE_DEVICES=all \
  IMMICH_SERVER_PATH=/usr/src/app/server \
  IMMICH_WEB_ROOT=/build/www \
  DB_HOSTNAME=database \
  REDIS_HOSTNAME=redis \
  IMMICH_MEDIA_LOCATION=/data

COPY --from=server-builder /output/server-pruned ./server
COPY --from=web-builder /usr/src/app/web/build /build/www
COPY --from=rust-builder /build/rust-server/target/release/rust-server /usr/local/bin/rust-server

COPY LICENSE /licenses/LICENSE.txt
COPY LICENSE /LICENSE

ARG BUILD_ID
ARG BUILD_IMAGE
ARG BUILD_SOURCE_REF
ARG BUILD_SOURCE_COMMIT

ENV IMMICH_BUILD=${BUILD_ID}
ENV IMMICH_BUILD_URL=https://github.com/immich-app/immich/actions/runs/${BUILD_ID}
ENV IMMICH_BUILD_IMAGE=${BUILD_IMAGE}
ENV IMMICH_BUILD_IMAGE_URL=https://github.com/immich-app/immich/pkgs/container/immich-rust-server
ENV IMMICH_REPOSITORY=immich-app/immich
ENV IMMICH_REPOSITORY_URL=https://github.com/immich-app/immich
ENV IMMICH_SOURCE_REF=${BUILD_SOURCE_REF}
ENV IMMICH_SOURCE_COMMIT=${BUILD_SOURCE_COMMIT}
ENV IMMICH_SOURCE_URL=https://github.com/immich-app/immich/commit/${BUILD_SOURCE_COMMIT}

VOLUME /data
EXPOSE 2283
ENTRYPOINT ["tini", "--"]
CMD ["rust-server"]

HEALTHCHECK CMD immich-healthcheck
