# Immich + rust-server — Docker deploy

From **`rust-server/`**, one command builds web + API and starts Postgres / Redis / ML:

```bash
cd rust-server
cp example.env .env
docker compose up -d --build
```

Open **http://localhost:2283**

## What gets started

| Service | Role |
|---------|------|
| `immich-server` | `Dockerfile` — Rust API + workers + static web |
| `database` | Official Immich Postgres + VectorChord |
| `redis` | Valkey (queues / sessions / events) |
| `immich-machine-learning` | CLIP / faces / OCR (official image) |

## Important

- Postgres **must** be Immich's vectorchord image (plain PG → bootstrap panic).
- Compose forces `DB_HOSTNAME=database` / `REDIS_HOSTNAME=redis`. Do not set `DB_URL=localhost` in the container.
- First `--build` compiles Rust + web; expect a long wait.

## Overlay on stock Immich `docker/docker-compose.yml`

```bash
cd rust-server
docker compose -f ../docker/docker-compose.yml -f docker-compose.overlay.yml --env-file ../docker/.env up -d --build
```

## Admin / stop

```bash
docker compose exec immich-server rust-server immich-admin migration-status
docker compose down
```
