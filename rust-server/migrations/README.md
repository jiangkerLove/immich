# Database migrations (sqlx) + Kysely parity lock

Upstream Immich evolves schema with TypeScript Kysely files:

`server/src/schema/migrations/*.ts`

This fork **does not run Node**. Schema is applied with sqlx:

| Artifact | Role |
|----------|------|
| `1_baseline.sql` | **Current** fused end-state of every Kysely name in the lock |
| `baseline_lock.json` | Parity record (`fused_kysely_migrations`) |
| `2_*.sql`, `3_*.sql`, … | **Only after** baseline is locked in use and you sync *new* upstream Kysely |
| `_sqlx_migrations` | Runtime bookkeeping (version + checksum) |

There is **no** separate `schema/init.sql`. Empty DBs get schema from sqlx `1_baseline.sql`.

## Before you lock baseline

While still iterating the fused snapshot (no production DBs depending on the checksum):

1. Fold any Kysely already present under `server/` into **`1_baseline.sql`**
2. Append those names to `fused_kysely_migrations` in `baseline_lock.json`
3. Keep a **single** sqlx file (`1_baseline.sql`) — do **not** open `2_` yet

## After baseline is in use (true incremental)

When you merge `main` and upstream added *new* Kysely files beyond the lock:

1. `cargo:warning` / `immich-admin migration-status` lists names ahead of the lock
2. Add `migrations/N_*.sql` (one-to-one or merged)
3. Append names to `fused_kysely_migrations` and optionally `incremental[]` for bridge
4. **Do not** rewrite `1_baseline.sql` checksum lightly once applied in real DBs

## Runtime (automatic on every API start)

1. Bridge existing Immich schema → record sqlx v1 without re-running
2. Bridge later sqlx versions if their Kysely names are already applied (when `incremental` is set)
3. Apply pending sqlx migrations
4. Require `asset`; print status; warn if tree/DB Kysely is ahead of the lock

```bash
rust-server immich-admin run-migrations
rust-server immich-admin migration-status
rust-server immich-admin schema-check
```
