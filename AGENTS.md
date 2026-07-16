# AGENTS.md

## Cursor Cloud specific instructions

Immich is a monorepo (pnpm workspaces + [`mise`](https://mise.jdx.dev) task runner). The standard dev
workflow runs everything in Docker Compose. General setup/run/test commands are documented in
`docs/docs/developer/setup.md`, `docs/docs/developer/testing.md`, and `mise.toml` (list tasks with
`mise tasks ls --all`). The notes below only cover non-obvious, environment-specific gotchas for this VM.

### Services

- `immich-server` — NestJS API + background workers. API at `http://localhost:2283` (health: `/api/server/ping`). Runs in Docker (dev image `server/Dockerfile.dev`) with hot reload.
- `immich-web` — SvelteKit/Vite web UI at `http://localhost:3000`. Runs in Docker with HMR.
- `immich-machine-learning` — FastAPI ML service at `http://localhost:3003` (health: `/ping`). Optional for most flows; runs in Docker.
- `database` — PostgreSQL 14 on `:5432`, using Immich's **custom pgvector/VectorChord image** (a stock Postgres will NOT work).
- `redis` — Valkey (Redis-compatible) for BullMQ job queues / caching.

### Starting the dev stack (non-obvious startup caveats)

1. **The Docker daemon is NOT started automatically** (this VM has no systemd). Start it once per VM boot before anything Docker-related, e.g. in a tmux session:
   `sudo dockerd` (logs to a file; leave it running in the background).
2. The `ubuntu` user is in the `docker` group, but a shell created *before* that group change won't have it. Use a fresh login shell, or prefix docker commands with `sg docker -c "..."`.
3. Requires `docker/.env` (gitignored). If missing, run `cp docker/example.env docker/.env`.
4. Start the full dev stack with `mise dev` (builds plugins first, then `docker compose -f docker/docker-compose.dev.yml up`). First run builds/pulls images and the init container runs `mise install` inside the container (takes a few minutes). Tear down with `mise dev-down`.
5. During container startup you'll see `sharp`/`node-gyp` build warnings — these are **non-fatal**; the server still starts (look for `Immich Server is listening on http://[::1]:2283`).
6. First run is uninitialized (`/api/server/config` shows `isInitialized:false`); create the admin account at `http://localhost:3000`.

### Lint / test (host commands, non-obvious notes)

- Server lint/tests run on the host: `mise //server:lint`, `mise //server:test` (fast, ~2200 unit tests).
- **`mise //web:lint` OOMs in this VM.** Its script uses `eslint . --concurrency 6`, which exceeds available memory (~15 GB, shared with the running stack). Run web lint with reduced concurrency and a larger synckit worker timeout instead:
  `cd web && SYNCKIT_TIMEOUT=60000 npx eslint . --max-warnings 0 --concurrency 1`
  (Without `SYNCKIT_TIMEOUT`, the `better-tailwindcss` plugin can fail with `Atomics.wait() timed-out`.)
- E2E tests need their own Docker stack (`mise e2e`) plus one-time `mise //e2e:ci-setup` and `mise //:open-api` — see `docs/docs/developer/testing.md`.

### Tooling

- `mise` and `uv` are installed at `~/.local/bin` and activated via `~/.bashrc` (node 24.15.0, pnpm 11.6.0 come from `mise`, which takes precedence over the system nvm).
- `mise run //:plugins` builds the SDK + WASM plugins on the host; `mise dev` does this automatically as a dependency, but run it manually if host-side server code/tests need a fresh `@immich/sdk`.
