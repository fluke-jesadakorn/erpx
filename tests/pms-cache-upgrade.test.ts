import { expect, it } from 'vitest';
import { cp, mkdir, mkdtemp, readdir, rm } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { env } from './runtime-env';
import { migrateDatabase } from '../src/server/runtime/migrations';

it('preserves every original cache revision on the populated 1037 upgrade and replays 1038 once', async () => {
  const root = await mkdtemp(resolve('.runtime/tests/pms-cache-upgrade-'));
  const db = env.H001_DB;
  try {
    await mkdir(join(root, 'holding-write'));
    for (const name of (await readdir('migrations/postgres/holding-write')).filter(name => name.endsWith('.sql') && Number(name.slice(0,4)) <= 1037))
      await cp(join('migrations/postgres/holding-write',name), join(root,'holding-write',name));
    await migrateDatabase(db, 'holding-write', root);
    await db.pool.query("UPDATE work_cache_revisions SET revision=revision+17 WHERE table_name IN ('pms_records','finance_journals','assignments')");
    const revisions = async () => (await db.pool.query('SELECT table_name,revision::text revision FROM work_cache_revisions ORDER BY table_name')).rows;
    const before = await revisions();
    expect(before.length).toBeGreaterThan(100);
    await migrateDatabase(db,'holding-write');
    expect(await revisions()).toEqual(before);
    expect((await db.pool.query('SELECT count(*) n FROM work_cache_revision_scopes')).rows[0].n).toBe(0);
    expect((await db.pool.query('SELECT table_name,revision::text revision FROM work_cache_revision_baselines ORDER BY table_name')).rows).toEqual(before);
    const receipts = (await db.pool.query('SELECT name,checksum FROM erpx_migrations ORDER BY name')).rows;
    await migrateDatabase(db,'holding-write');
    expect(await revisions()).toEqual(before);
    expect((await db.pool.query('SELECT name,checksum FROM erpx_migrations ORDER BY name')).rows).toEqual(receipts);
  } finally { await rm(root,{recursive:true,force:true}); }
},90_000);
