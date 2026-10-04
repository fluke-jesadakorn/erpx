import { beforeAll, beforeEach, afterEach, expect, it, vi } from 'vitest';
import { randomUUID } from 'node:crypto';
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
let actor = { holdingId: 'h001', companyId: 'cache-test', userId: 'reader', assignmentId: 'grant', role: 'employee',
  scope: { kind: 'holding' }, branchId: null, departmentId: null } as WorkContext;
const snapshot = (title = 'original'): WorkSnapshot => ({ unavailableSources: [], items: [{ id: '1', type: 'staff_task', title,
  detail: 'test', href: '/staff-tasks/1', status: 'draft', updatedAt: 1, queue: 'todo', canAct: true }] });
const insert = () => db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES('cache-test','staff_task','work','assignee','fixture','test',1)").run();
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); await applyMigrations(env.H002_DB, env.STAFF_TEST_MIGRATIONS); });
beforeEach(() => { cache.clear(); redis.available = true; actor = { ...actor, userId: randomUUID() }; });
afterEach(() => vi.useRealTimers());
it('preserves complete results, order and actions on warm reads; coalesces five fills', async () => {
  const load = vi.fn(async () => snapshot());
  const values = await Promise.all(Array.from({ length: 5 }, () => cachedWorkSnapshot(actor, db, load)));
  expect(values).toEqual(Array(5).fill(snapshot())); expect(load).toHaveBeenCalledTimes(1);
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot()); expect(load).toHaveBeenCalledTimes(1);
});
it('invalidates after direct SQL, job writes and reassignment in the committing transaction', async () => {
  redis.available = false;
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
  redis.available = false;
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
  redis.available = false;
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
  redis.available = false;
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('cache-grant','cache-reader','employee','[]','cache-test',0,9999999999999,'holding')").run();
  const load = vi.fn(async () => {
    const value = snapshot();
    value.items[0].canAct = (await db.prepare('SELECT iam_assignment_active(?,?) active').bind('cache-grant', Date.now()).first<{ active: boolean }>())!.active;
    return value;
  });
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(true);
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(true);
  expect(load).toHaveBeenCalledTimes(1);
  const before = (await workCacheFence(db))!.revision;
  await db.prepare("UPDATE assignments SET revoked_at=1 WHERE id='cache-grant'").run();
  expect((await workCacheFence(db))!.revision).not.toBe(before);
  expect((await cachedWorkSnapshot(actor, db, load)).items[0].canAct).toBe(false);
  expect(load).toHaveBeenCalledTimes(2);
  const ignored = await db.pool.query("SELECT work_cache_tracks('home_read_cursors') cursor,work_cache_tracks('outbox_events') jobs");
  expect(ignored.rows[0]).toEqual({ cursor: false, jobs: false });
});
it('bypasses persistent cache when a newly added table has no revision trigger', async () => {
  redis.available = false;
  const load = vi.fn(async () => snapshot());
  await cachedWorkSnapshot(actor, db, load);
  await db.pool.query('CREATE TABLE cache_uncovered(id text)');
  try {
    expect(await workCacheFence(db)).toBeNull();
    await cachedWorkSnapshot(actor, db, load); await cachedWorkSnapshot(actor, db, load);
    expect(load).toHaveBeenCalledTimes(3);
  } finally { await db.pool.query('DROP TABLE cache_uncovered'); }
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

it('coalesces and reuses resolved revision-fenced reads without Redis', async () => {
  redis.available = false;
  const load = vi.fn(async () => { await new Promise(resolve => setTimeout(resolve, 40)); return snapshot(); });
  expect(await Promise.all(Array.from({length:5}, () => cachedWorkSnapshot(actor,db,load)))).toEqual(Array(5).fill(snapshot()));
  expect(load).toHaveBeenCalledTimes(1);
  expect(cache.size).toBe(0);
  await cachedWorkSnapshot(actor,db,load);
  expect(load).toHaveBeenCalledTimes(1);
  expect(cache.size).toBe(0);
});

it('isolates local results by current actor authority and actual database identity', async () => {
  redis.available = false;
  const load = vi.fn(async () => snapshot('first database'));
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('first database'));
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('first database'));
  expect(load).toHaveBeenCalledTimes(1);
  const otherLoad = vi.fn(async () => snapshot('other database'));
  expect(await cachedWorkSnapshot(actor, env.H002_DB, otherLoad)).toEqual(snapshot('other database'));
  expect(await cachedWorkSnapshot(actor, env.H002_DB, otherLoad)).toEqual(snapshot('other database'));
  expect(otherLoad).toHaveBeenCalledTimes(1);
  const changed = { ...actor, iam: { roleRevision: 2 } } as WorkContext;
  expect(await cachedWorkSnapshot(changed, db, async () => snapshot('new authority'))).toEqual(snapshot('new authority'));
});

it('does not let a returned snapshot mutate the next local result', async () => {
  redis.available = false;
  const load = vi.fn(async () => snapshot());
  const first = await cachedWorkSnapshot(actor, db, load); first.items[0].title = 'changed by caller';
  const second = await cachedWorkSnapshot(actor, db, load);
  expect(second).toEqual(snapshot()); second.items[0].canAct = false;
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot());
  expect(load).toHaveBeenCalledTimes(1);
});

it('does not reuse failed fills and recovers into a reusable complete result', async () => {
  redis.available = false;
  const load = vi.fn<() => Promise<WorkSnapshot>>().mockRejectedValueOnce(new Error('owner unavailable')).mockResolvedValue(snapshot('recovered'));
  await expect(cachedWorkSnapshot(actor, db, load)).rejects.toThrow('owner unavailable');
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('recovered'));
  expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('recovered'));
  expect(load).toHaveBeenCalledTimes(2);
});

it('reloads rather than returning a local hit when its uncached after-fence fails', async () => {
  redis.available = false;
  const load = vi.fn(async () => snapshot('warm'));
  await cachedWorkSnapshot(actor, db, load);
  load.mockResolvedValue(snapshot('authoritative fallback'));
  const prepare = db.prepare.bind(db); let fences = 0;
  const spy = vi.spyOn(db, 'prepare').mockImplementation(sql => {
    const statement = prepare(sql);
    if (sql.includes('FROM work_cache_revisions') && ++fences === 2)
      vi.spyOn(statement, 'first').mockRejectedValueOnce(new Error('fence unavailable'));
    return statement;
  });
  try {
    expect(await cachedWorkSnapshot(actor, db, load)).toEqual(snapshot('authoritative fallback'));
    expect(fences).toBe(2); expect(load).toHaveBeenCalledTimes(2);
  } finally { spy.mockRestore(); }
});

it('expires local results at the thirty-second limit without a database write', async () => {
  redis.available = false;
  const load = vi.fn(async () => snapshot());
  await cachedWorkSnapshot(actor, db, load);
  const revision = (await workCacheFence(db))!.revision;
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(Date.now() + 31_000);
  expect((await workCacheFence(db))!.revision).toBe(revision);
  await cachedWorkSnapshot(actor, db, load);
  expect(load).toHaveBeenCalledTimes(2);
});

it('expires local results at emergency authority deadlines without a database write', async () => {
  redis.available = false;
  const deadline = Date.now() + 10_000;
  const timed = { ...actor, iam: { freshUntil: deadline + 10_000, emergencyUntil: deadline } } as WorkContext;
  const load = vi.fn(async () => snapshot());
  await cachedWorkSnapshot(timed, db, load); await cachedWorkSnapshot(timed, db, load);
  expect(load).toHaveBeenCalledTimes(1);
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(deadline);
  await cachedWorkSnapshot(timed, db, load); await cachedWorkSnapshot(timed, db, load);
  expect(load).toHaveBeenCalledTimes(3);
});

it('bounds local entries and retains the most recently read actor when evicting', async () => {
  redis.available = false;
  const actors = Array.from({ length: 129 }, (_, i) => ({ ...actor, userId: `${actor.userId}-${i}` }));
  const load = vi.fn(async () => snapshot());
  for (const reader of actors.slice(0, 128)) await cachedWorkSnapshot(reader, db, load);
  expect(load).toHaveBeenCalledTimes(128);
  await cachedWorkSnapshot(actors[0], db, load); expect(load).toHaveBeenCalledTimes(128);
  await cachedWorkSnapshot(actors[128], db, load); expect(load).toHaveBeenCalledTimes(129);
  await cachedWorkSnapshot(actors[0], db, load); expect(load).toHaveBeenCalledTimes(129);
  await cachedWorkSnapshot(actors[1], db, load); expect(load).toHaveBeenCalledTimes(130);
});

it('bounds local serialized bytes and never retains an oversized snapshot', async () => {
  redis.available = false;
  const load = vi.fn(async () => snapshot('x'.repeat(8 * 1024 * 1024)));
  await cachedWorkSnapshot(actor, db, load); await cachedWorkSnapshot(actor, db, load);
  expect(load).toHaveBeenCalledTimes(2); expect(cache.size).toBe(0);
  const readers = Array.from({ length: 9 }, (_, i) => ({ ...actor, userId: `${actor.userId}-large-${i}` }));
  const largeLoad = vi.fn(async () => snapshot('x'.repeat(1024 * 1024)));
  for (const reader of readers) await cachedWorkSnapshot(reader, db, largeLoad);
  expect(largeLoad).toHaveBeenCalledTimes(9);
  await cachedWorkSnapshot(readers[8], db, largeLoad); expect(largeLoad).toHaveBeenCalledTimes(9);
  await cachedWorkSnapshot(readers[0], db, largeLoad); expect(largeLoad).toHaveBeenCalledTimes(10);
});

it('keeps Work warm through Legal scheduler health writes while health and Legal business changes stay current', async () => {
  redis.available = false;
  const { legalWork } = await import('../src/server/legal/integrations');
  const { legalRuntimeHealth } = await import('../src/server/legal/runtime-gate');
  const legalActor = { ...actor, iam: { permissions: ['legal.read', 'legal.edit', 'legal.manage'] } } as WorkContext;
  const load = vi.fn(async () => ({ items: await legalWork(db, legalActor), unavailableSources: [] }));
  expect((await cachedWorkSnapshot(legalActor, db, load)).items).toEqual([]);
  const fence = (await workCacheFence(db))!;
  const tasks = Array.from({ length: 5 }, (_, index) => `${actor.userId}-health-${index}`);
  for (const task of tasks) {
    await db.prepare('INSERT INTO legal_runtime_health(task,heartbeat_at) VALUES(?,?)').bind(task, 1).run();
    await db.prepare('UPDATE legal_runtime_health SET heartbeat_at=?,last_success_at=?,failure_count=0,last_error=NULL WHERE task=?').bind(2, 2, task).run();
  }
  expect((await workCacheFence(db))!.revision).toBe(fence.revision);
  expect((await cachedWorkSnapshot(legalActor, db, load)).items).toEqual([]);
  expect(load).toHaveBeenCalledTimes(1);
  await db.prepare('UPDATE legal_runtime_health SET heartbeat_at=?,failure_count=failure_count+1,last_error=? WHERE task=?').bind(3, 'FixtureOwnerUnavailable', tasks[0]).run();
  const health = await legalRuntimeHealth(db, legalActor);
  expect(health.tasks.find(task => task.task === tasks[0])).toMatchObject({ heartbeat_at: 3, last_success_at: 2, failure_count: 1, last_error: 'FixtureOwnerUnavailable' });
  expect((await cachedWorkSnapshot(legalActor, db, load)).items).toEqual([]);
  expect(load).toHaveBeenCalledTimes(1);
  const id = randomUUID();
  await db.prepare(`INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)
    VALUES(?,?,'request','Current Legal task','internal','TH','Thailand','en','THB','Asia/Bangkok','normal',?,?,1,1)`)
    .bind(id, actor.companyId, actor.userId, actor.userId).run();
  expect((await workCacheFence(db))!.revision).not.toBe(fence.revision);
  expect((await cachedWorkSnapshot(legalActor, db, load)).items).toMatchObject([{ id, title: 'Current Legal task', canAct: true }]);
  expect(load).toHaveBeenCalledTimes(2);
  await db.prepare("UPDATE legal_items SET state='closed',version=version+1,updated_at=2 WHERE id=?").bind(id).run();
  expect((await cachedWorkSnapshot(legalActor, db, load)).items).toEqual([]);
  expect(load).toHaveBeenCalledTimes(3);
});

it('still fails closed if an excluded health table loses revision coverage', async () => {
  redis.available = false;
  const load = vi.fn(async () => snapshot());
  await cachedWorkSnapshot(actor, db, load);
  await db.pool.query('DROP TRIGGER work_cache_change ON legal_runtime_health');
  try {
    expect(await workCacheFence(db)).toBeNull();
    await cachedWorkSnapshot(actor, db, load); await cachedWorkSnapshot(actor, db, load);
    expect(load).toHaveBeenCalledTimes(3);
  } finally {
    await db.pool.query('CREATE TRIGGER work_cache_change AFTER INSERT OR UPDATE OR DELETE ON legal_runtime_health FOR EACH ROW EXECUTE FUNCTION work_cache_changed()');
  }
});
