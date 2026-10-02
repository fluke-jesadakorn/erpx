import { beforeAll, beforeEach, afterEach, expect, it, vi } from 'vitest';
import { env, applyMigrations } from './runtime-env';
import type { WorkContext } from '../src/server/authorization';
import type { WorkSnapshot } from '../src/server/work-items';
import { cachedWorkSnapshot, workCacheFence, workCacheKey } from '../src/server/work-cache';
const cache = vi.hoisted(() => new Map<string, string>());
const redis = vi.hoisted(() => ({ available: true }));
vi.mock('../src/server/runtime/redis-cache', () => ({
  redisCacheEnabled: () => redis.available,
  cacheGet: async (key: string) => redis.available ? cache.get(key) ?? null : null,
  cacheSet: async (key: string, value: string, ttl: number) => { if (redis.available && ttl > 0) cache.set(key, value); },
}));
const db = env.H001_DB;
const actor = { holdingId: 'h001', companyId: 'cache-test', userId: 'reader', assignmentId: 'grant', role: 'employee',
  scope: { kind: 'holding' }, branchId: null, departmentId: null } as WorkContext;
const snapshot = (title = 'original'): WorkSnapshot => ({ unavailableSources: [], items: [{ id: '1', type: 'staff_task', title,
  detail: 'test', href: '/staff-tasks/1', status: 'draft', updatedAt: 1, queue: 'todo', canAct: true }] });
const insert = () => db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES('cache-test','staff_task','work','assignee','fixture','test',1)").run();
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(() => { cache.clear(); redis.available = true; });
afterEach(() => vi.useRealTimers());
it('preserves complete results, order and actions on warm reads; coalesces five fills', async () => {
  const load = vi.fn(async () => snapshot());
  const values = await Promise.all(Array.from({ length: 5 }, () => cachedWorkSnapshot(actor, db, load)));
  expect(values).toEqual(Array(5).fill(snapshot())); expect(load).toHaveBeenCalledTimes(1);
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot()); expect(load).toHaveBeenCalledTimes(1);
});
it('invalidates after direct SQL, job writes and reassignment in the committing transaction', async () => {
  await cachedWorkSnapshot(actor, db, async () => snapshot());
  const before = await workCacheFence(db); await insert();
  expect((await workCacheFence(db))!.revision).not.toBe(before!.revision);
  expect(await cachedWorkSnapshot(actor, db, async () => snapshot('assigned'))).toEqual(snapshot('assigned'));
  await db.prepare("UPDATE flow_defaults SET primary_user_id='new-assignee' WHERE company_id='cache-test'").run();
  expect(await cachedWorkSnapshot(actor, db, async () => snapshot('reassigned'))).toEqual(snapshot('reassigned'));
});
it('rolls revisions back with a transaction and does not expose uncommitted writes', async () => {
  const before = await workCacheFence(db), client = await db.pool.connect();
  try {
    await client.query('BEGIN');
    await client.query("UPDATE flow_defaults SET primary_user_id='rolled-back' WHERE company_id='cache-test'");
    expect((await workCacheFence(db))!.revision).toBe(before!.revision);
    await client.query('ROLLBACK');
    expect((await workCacheFence(db))!.revision).toBe(before!.revision);
  } finally { client.release(); }
});
it('discards a fill that crosses a commit; reads starting after commit never join the old fill', async () => {
  let release!: () => void, started!: () => void;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const entered = new Promise<void>(resolve => { started = resolve; });
  let first = true;
  const load = vi.fn(async () => { if (first) { first = false; started(); await gate; return snapshot('old'); } return snapshot('new'); });
  const old = cachedWorkSnapshot(actor, db, load); await entered; await insert();
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('new'));
  release(); expect(await old).toEqual(snapshot('new'));
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('new'));
});
it('does not persist incomplete sources', async () => {
  const load = vi.fn(async () => ({ ...snapshot(), unavailableSources: ['Leave'] }));
  await cachedWorkSnapshot(actor, db, load); await cachedWorkSnapshot(actor, db, load);
  expect(load).toHaveBeenCalledTimes(2); expect(cache.size).toBe(0);
});
it('partitions company/user/branch/department/role/assignment, authority and database', async () => {
  const fence = (await workCacheFence(db))!, baseline = workCacheKey(actor, fence);
  for (const change of [{ companyId: 'other' }, { userId: 'other' }, { branchId: 'other' }, { departmentId: 'other' },
    { role: 'other' }, { assignmentId: 'other' }, { scope: { kind: 'branch' } }, { iam: { roleRevision: 9 } }])
    expect(workCacheKey({ ...actor, ...change } as WorkContext, fence)).not.toBe(baseline);
  expect(workCacheKey(actor, { ...fence, database: 'other' })).not.toBe(baseline);
});
it('expires on authority deadlines without a data write', async () => {
  const deadline = Date.now() + 10_000;
  const timed = { ...actor, iam: { freshUntil: deadline } } as WorkContext;
  await cachedWorkSnapshot(timed, db, async () => snapshot('before'));
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(deadline + 1);
  expect(await cachedWorkSnapshot(timed, db, async () => snapshot('after'))).toEqual(snapshot('after'));
});
it('tracks IAM revocation and excludes read cursors/housekeeping', async () => {
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('cache-grant','cache-reader','employee','[]','cache-test',0,9999999999999,'holding')").run();
  const before = (await workCacheFence(db))!.revision;
  await db.prepare("UPDATE assignments SET revoked_at=1 WHERE id='cache-grant'").run();
  expect((await workCacheFence(db))!.revision).not.toBe(before);
  const ignored = await db.pool.query("SELECT work_cache_tracks('home_read_cursors') cursor,work_cache_tracks('outbox_events') jobs");
  expect(ignored.rows[0]).toEqual({ cursor: false, jobs: false });
});
it('bypasses persistent cache when a newly added table has no revision trigger', async () => {
  await db.pool.query('CREATE TABLE cache_uncovered(id text)');
  try { expect(await workCacheFence(db)).toBeNull(); } finally { await db.pool.query('DROP TABLE cache_uncovered'); }
});
it('expires before a candidate assignment expires without a new write', async () => {
  const deadline = Date.now() + 350;
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('cache-expiring','expiring','employee','[]','cache-test',0,?,'holding')").bind(deadline).run();
  expect((await workCacheFence(db))!.validUntil).toBeLessThanOrEqual(deadline);
  const load = async () => {
    const row = await db.prepare('SELECT iam_assignment_active(?,?) active').bind('cache-expiring', Date.now()).first<{ active: boolean }>();
    const value = snapshot(); value.items[0].canAct = row!.active; return value;
  };
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(true);
  await new Promise(resolve => setTimeout(resolve, Math.max(0, deadline - Date.now() + 20)));
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(false);
});
it('refreshes delegation authority at its start and expiry without a write', async () => {
  const startsAt = Date.now() + 300, expiresAt = startsAt + 400;
  await db.prepare(`INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,
    delegate_user_id,starts_at,expires_at,reason,created_at,updated_at)
    VALUES('cache-timed-delegation','cache-test','timed-task','owner','reader',?,?,'Timed handoff',1,1)`)
    .bind(startsAt, expiresAt).run();
  const load = async () => {
    const row = await db.prepare(`SELECT EXISTS(SELECT 1 FROM staff_task_delegations
      WHERE id='cache-timed-delegation' AND status='active'
      AND starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
      AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint) active`)
      .first<{ active: boolean }>();
    const value = snapshot(); value.items[0].canAct = row!.active; return value;
  };
  expect((await workCacheFence(db))!.validUntil).toBeLessThanOrEqual(startsAt);
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(false);
  const revision = (await workCacheFence(db))!.revision;
  await new Promise(resolve => setTimeout(resolve, Math.max(0, startsAt - Date.now() + 20)));
  expect((await workCacheFence(db))!.revision).toBe(revision);
  expect((await workCacheFence(db))!.validUntil).toBeLessThanOrEqual(expiresAt);
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(true);
  await new Promise(resolve => setTimeout(resolve, Math.max(0, expiresAt - Date.now() + 20)));
  expect((await workCacheFence(db))!.revision).toBe(revision);
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(false);
});
it('does not lose revisions under concurrent writers and invalidates on truncate', async () => {
  const revision = async () => Number((await db.pool.query("SELECT revision FROM work_cache_revisions WHERE table_name='flow_defaults'")).rows[0].revision);
  const before = await revision(); await Promise.all([insert(), insert()]); expect(await revision()).toBe(before + 2);
  const fence = (await workCacheFence(db))!; await db.pool.query('TRUNCATE flow_defaults');
  expect((await workCacheFence(db))!.revision).not.toBe(fence.revision);
});
it('does not join a pre-expiry fill after a time boundary without a commit', async () => {
  const deadline = Date.now() + 220;
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('cache-expiring-fill','expiring-fill','employee','[]','cache-test',0,?,'holding')").bind(deadline).run();
  let release!: () => void, started!: () => void, first = true;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const entered = new Promise<void>(resolve => { started = resolve; });
  const load = async () => { if (first) { first = false; started(); await gate; return snapshot('expired'); } return snapshot('current'); };
  const old = cachedWorkSnapshot(actor, db, load); await entered;
  await new Promise(resolve => setTimeout(resolve, Math.max(0, deadline - Date.now() + 20)));
  try { expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('current')); }
  finally { release(); }
  expect(await old).toEqual(snapshot('current'));
});
it('repairs future tables after migrations and invalidates inserts, updates and truncation', async () => {
  await db.pool.query('CREATE TABLE cache_future(id text)');
  try {
    expect(await workCacheFence(db)).toBeNull();
    await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
    let previous = (await workCacheFence(db))!.revision;
    for (const sql of ["INSERT INTO cache_future VALUES('a')", "UPDATE cache_future SET id='b'", 'TRUNCATE cache_future']) {
      await db.pool.query(sql);
      const current = (await workCacheFence(db))!.revision;
      expect(current).not.toBe(previous); previous = current;
    }
    await db.pool.query('DROP TRIGGER work_cache_change ON cache_future');
    expect(await workCacheFence(db)).toBeNull();
    await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
    expect(await workCacheFence(db)).not.toBeNull();
  } finally {
    await db.pool.query('DROP TABLE cache_future');
    await db.pool.query("DELETE FROM work_cache_revisions WHERE table_name='cache_future'");
  }
});

it('coalesces revision-fenced reads without Redis and never persists them', async () => {
  redis.available = false;
  const load = vi.fn(async () => { await new Promise(resolve => setTimeout(resolve, 40)); return snapshot(); });
  expect(await Promise.all(Array.from({length:5}, () => cachedWorkSnapshot(actor,db,load)))).toEqual(Array(5).fill(snapshot()));
  expect(load).toHaveBeenCalledTimes(1);
  expect(cache.size).toBe(0);
  await cachedWorkSnapshot(actor,db,load);
  expect(load).toHaveBeenCalledTimes(2);
});
