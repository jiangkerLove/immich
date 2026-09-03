# Database migrations (sqlx) + Kysely parity lock

Upstream Immich evolves schema with TypeScript Kysely files:

`server/src/schema/migrations/*.ts`

This fork **does not run Node**. History is tracked in sqlx:

| Artifact | Role |
|----------|------|
| `1_baseline.sql` | Fused end-state of all Kysely files listed in the lock |
| `baseline_lock.json` | **Parity record**: which Kysely names are already inside baseline v1 |
| `2_*.sql`, `3_*.sql`, … | After the lock snapshot: new deltas when you sync upstream |
| `_sqlx_migrations` | Runtime bookkeeping (version + checksum) |

## Runtime (automatic on every API start)

`database_migrations::run` always:

1. **Init / bridge** — empty DB → apply sqlx; existing Immich schema → record baseline v1 without re-executing.
2. **Apply pending** — any `migrations/N_*.sql` not yet in `_sqlx_migrations`.
3. **Check** — require `asset` table; print status; warn if Kysely names (DB or `server/` tree) are **ahead of** `baseline_lock.json`.

CLI:

```bash
rust-server immich-admin run-migrations
rust-server immich-admin migration-status
```

`DB_SKIP_MIGRATIONS=true` skips apply (status still useful via `migration-status`).

## Sync policy (your timing)

When you merge `main` → `dev-rust` and upstream added Kysely migrations:

1. Build will `cargo:warning` listing names not in the lock.
2. Choose **per sync batch**:
   - **One sqlx file per TS file**, or
   - **Merge several TS changes into one** `migrations/N_description.sql` (recommended when you sync infrequently).
3. Update `baseline_lock.json`:
   - Append the absorbed Kysely names to `fused_kysely_migrations`, **or**
   - Keep baseline v1 frozen and treat post-lock names as covered only by sqlx `2+` (still append them to the lock once absorbed so drift clears).
4. Keep `schema/init.sql` aligned with the latest end-state for fresh installs / docs.

**Do not** rewrite `1_baseline.sql` checksum after it has been applied in production DBs unless you deliberately re-bridge.

## No Node

`IMMICH_SERVER_PATH` / `bin/run-kysely-migrations.cjs` are unused for schema.
