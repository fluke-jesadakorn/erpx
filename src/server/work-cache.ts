import "server-only";
import { createHash } from "node:crypto";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { WorkSnapshot } from "./work-items";
import { cacheGet, cacheSet, redisCacheEnabled } from "./runtime/redis-cache";

export type CacheFence = { revision: string; database: string; validUntil: number };
// Route handlers, RSC and HMR share the same in-flight computation in this process.
type InFlight = { result: Promise<WorkSnapshot>; validUntil: number };
type CachedResult = { text: string; validUntil: number; bytes: number };
const shared = globalThis as typeof globalThis & {
  erpxWorkFillsV2?: Map<string, InFlight>; erpxWorkResultsV1?: Map<string, CachedResult>;
};
const pending = shared.erpxWorkFillsV2 ??= new Map<string, InFlight>();
const results = shared.erpxWorkResultsV1 ??= new Map<string, CachedResult>();
const maxResults = 128, maxResultBytes = 8 * 1024 * 1024;
function pruneResults() {
  const now = Date.now();
  for (const [key, entry] of results) if (entry.validUntil <= now) results.delete(key);
}
function remember(key: string, snapshot: WorkSnapshot, validUntil: number) {
  if (validUntil <= Date.now() || snapshot.unavailableSources.length) return;
  const text = JSON.stringify({ validUntil, snapshot }), bytes = Buffer.byteLength(text);
  if (bytes > maxResultBytes) return;
  pruneResults(); results.delete(key); results.set(key, { text, validUntil, bytes });
  let totalBytes = 0;
  for (const entry of results.values()) totalBytes += entry.bytes;
  for (const [oldest, entry] of results) {
    if (results.size <= maxResults && totalBytes <= maxResultBytes) break;
    results.delete(oldest); totalBytes -= entry.bytes;
  }
}
function authorityDeadline(actor: WorkContext) {
  return Math.min(actor.iam?.freshUntil ?? Infinity, actor.iam?.emergencyUntil ?? Infinity);
}
export function workCacheKey(actor: WorkContext, fence: CacheFence) {
  const identity = [process.env.ERPX_ENVIRONMENT ?? "local", process.env.ERPX_DATABASE_MANIFEST,
    process.env.NEXT_DEPLOYMENT_ID ?? "standalone-dev", fence.database, actor.holdingId, actor.companyId,
    actor.userId, actor.assignmentId, actor.role, actor.beta, actor.scope.kind, actor.branchId, actor.departmentId,
    actor.assignmentScope, actor.inherited, actor.modules, actor.inspection, actor.authority, actor.iam, fence.revision];
  return `erpx:dev:work:v1:${createHash("sha256").update(JSON.stringify(identity)).digest("hex")}`;
}

export async function workCacheFence(db: SqlDatabase): Promise<CacheFence | null> {
  const row = await db.prepare(`SELECT current_database() || ':' || (SELECT oid::text FROM pg_database WHERE datname=current_database()) || ':' || coalesce(inet_server_addr()::text, 'local') || ':' || coalesce(inet_server_port()::text, 'socket') database,
    (SELECT md5(string_agg(table_name || ':' || revision::text, ',' ORDER BY table_name)) FROM work_cache_revisions WHERE table_name <> 'legal_runtime_health') revision,
    NOT EXISTS (SELECT 1 FROM pg_tables t WHERE t.schemaname='public' AND work_cache_tracks(t.tablename)
      AND (NOT EXISTS (SELECT 1 FROM work_cache_revisions r WHERE r.table_name=t.tablename)
        OR (SELECT count(*) FROM pg_trigger g WHERE g.tgrelid=to_regclass(format('public.%I',t.tablename))
          AND g.tgname IN ('work_cache_change','work_cache_truncate') AND g.tgenabled='O') <> 2)) covered,
    (SELECT min(deadline) FROM (
      SELECT valid_from deadline FROM assignments UNION ALL SELECT valid_until FROM assignments
      UNION ALL SELECT expires_at FROM iam_emergency_sessions
      UNION ALL SELECT observed_at+300000 FROM iam_identities
      UNION ALL SELECT effective_at FROM people_access_cutoffs WHERE cancelled_at IS NULL
      UNION ALL SELECT restored_at FROM people_access_cutoffs WHERE cancelled_at IS NULL
      UNION ALL SELECT effective_at FROM people_lifecycle_cases WHERE status IN ('scheduled','completed')
      UNION ALL SELECT valid_from FROM people_applied_packages WHERE ended_at IS NULL
      UNION ALL SELECT valid_until FROM people_applied_packages WHERE ended_at IS NULL
      UNION ALL SELECT (definition->>'validUntil')::bigint FROM um_candidates
      UNION ALL SELECT starts_at FROM overtime_requests UNION ALL SELECT ends_at FROM overtime_requests
      UNION ALL SELECT starts_at FROM staff_task_delegations WHERE status='active'
      UNION ALL SELECT expires_at FROM staff_task_delegations WHERE status='active'
    ) deadlines WHERE deadline > floor(extract(epoch FROM clock_timestamp())*1000)::bigint) deadline`).first<{
      database: string; revision: string; covered: boolean; deadline: number | null;
    }>();
  if (!row?.covered) return null;
  // Scheduler health observations have their own live endpoint and are not
  // WorkSnapshot inputs. Business Legal records still invalidate the snapshot.
  const now = Date.now();
  // Minute boundary also covers midnight in all current business timezones.
  return { database: row.database, revision: row.revision,
    validUntil: Math.min(now + 30_000, (Math.floor(now / 60_000) + 1) * 60_000, row.deadline ?? Infinity) };
}

export async function cachedWorkSnapshot(actor: WorkContext, db: SqlDatabase, load: () => Promise<WorkSnapshot>) {
  // No revision read is cached in memory or Redis: reads after commit see the new fence.
  let fence: CacheFence | null;
  try { fence = await workCacheFence(db); } catch { report("bypass", "fence_read_failed"); return load(); }
  if (!fence) { report("bypass", "incomplete_coverage"); return load(); }
  pruneResults();
  for (let attempt = 0; attempt < 3; attempt++) {
    if (authorityDeadline(actor) <= Date.now()) { report("bypass", "authority_expired"); return load(); }
    const current: CacheFence = fence;
    const key = workCacheKey(actor, current);
    const local = results.get(key);
    const text = local?.text ?? (redisCacheEnabled() ? await cacheGet(key) : null);
    if (text) {
      try {
        const entry = JSON.parse(text) as { validUntil: number; snapshot: WorkSnapshot };
        if (entry.validUntil > Date.now() && Array.isArray(entry.snapshot?.items) && entry.snapshot.unavailableSources?.length === 0) {
          const after = await workCacheFence(db).catch(() => null);
          const validUntil = Math.min(current.validUntil, after?.validUntil ?? 0, entry.validUntil, authorityDeadline(actor));
          if (after?.database === current.database && after.revision === current.revision && Date.now() < validUntil) {
            // Serialized entries prevent a caller from changing a later reader's result.
            remember(key, entry.snapshot, validUntil);
            report("hit", local ? "memory" : "redis"); return entry.snapshot;
          }
          if (!after) return load();
          fence = after; continue;
        }
      } catch { /* Invalid cache entries are misses. */ }
    }
    let computation = pending.get(key);
    if (!computation || computation.validUntil <= Date.now()) {
      computation = { result: load(), validUntil: Math.min(current.validUntil, authorityDeadline(actor)) }; pending.set(key, computation);
    }
    try {
      const snapshot = await computation.result;
      const after = await workCacheFence(db).catch(() => null);
      if (!after) return snapshot;
      if (after.database !== current.database || after.revision !== current.revision || Date.now() >= Math.min(current.validUntil, computation.validUntil, after.validUntil)) { fence = after; continue; }
      const validUntil = Math.min(current.validUntil, computation.validUntil, after.validUntil, authorityDeadline(actor));
      if (!snapshot.unavailableSources.length) {
        remember(key, snapshot, validUntil);
        await cacheSet(key, JSON.stringify({ validUntil, snapshot }), validUntil - Date.now());
      }
      report("miss"); return snapshot;
    } finally {
      if (pending.get(key) === computation) pending.delete(key);
    }
  }
  // Continuous writes must not cause an unbounded retry or publish a mixed cache entry.
  report("bypass", "revision_changed"); return load();
}
function report(status: string, reason?: string) {
  if (process.env.ERPX_PERF_LOG === "1") console.info(JSON.stringify({ event: "work_cache", status, reason }));
}
