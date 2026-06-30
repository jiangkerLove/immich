#!/usr/bin/env node
'use strict';

const { readdir } = require('node:fs/promises');
const { join } = require('node:path');
const { Kysely, PostgresDialect, Migrator, FileMigrationProvider } = require('kysely');
const pg = require('pg');

const serverRoot = join(__dirname, '..');
const migrationFolder =
  process.env.IMMICH_KYSELY_MIGRATIONS_DIR || join(serverRoot, 'dist/schema/migrations');

async function main() {
  const pool = new pg.Pool({
    host: process.env.DB_HOSTNAME || process.env.DB_URL,
    port: Number(process.env.DB_PORT || 5432),
    user: process.env.DB_USERNAME,
    password: process.env.DB_PASSWORD,
    database: process.env.DB_DATABASE_NAME,
  });

  const db = new Kysely({
    dialect: new PostgresDialect({ pool }),
  });

  const migrator = new Migrator({
    db,
    migrationTableName: 'kysely_migrations',
    migrationLockTableName: 'kysely_migrations_lock',
    allowUnorderedMigrations: process.env.IMMICH_ENV === 'development',
    provider: new FileMigrationProvider({
      fs: { readdir },
      path: { join },
      migrationFolder,
    }),
  });

  console.log('Running migrations');
  const { error, results } = await migrator.migrateToLatest();

  for (const result of results ?? []) {
    if (result.status === 'Success') {
      console.log(`Migration "${result.migrationName}" succeeded`);
    }
    if (result.status === 'Error') {
      console.error(`Migration "${result.migrationName}" failed`);
    }
  }

  await db.destroy();

  if (error) {
    console.error(`Migrations failed: ${error}`);
    process.exit(1);
  }

  console.log('Finished running migrations');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
