# rust-server

Rust 版 Immich API 与后台 worker。行为对齐参考同级目录外的 `server/`（TypeScript）。

- 规则：`.cursor/rules/rust-server.mdc`（编辑本目录时自动生效）
- 完整迁移清单：`.cursor/docs/migration-checklist.md`
- Docker 一键：`docker compose up -d --build`（见 `README.docker.md`）
- 仓库入口：`../AGENTS.md`
- Docker 一键：`cp example.env .env && docker compose up -d --build`（见 `README.docker.md`）
