import "server-only";
import { createHash } from "node:crypto";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import type { WorkSnapshot } from "./work-items";
import { cacheGet, cacheSet, redisCacheEnabled } from "./runtime/redis-cache";

export type CacheFence = { revision: string; database: string; validUntil: number };
// Route handlers, RSC and HMR share the same in-flight computation in this process.
type InFlight = { result: Promise<WorkSnapshot>; validUntil: number };
const shared = globalThis as typeof globalThis & { erpxWorkFillsV2?: Map<string, InFlight> };
const pending = shared.erpxWorkFillsV2 ??= new Map<string, InFlight>();
export function workCacheKey(actor: WorkContext, fence: CacheFence) {
  const identity = [process.env.ERPX_ENVIRONMENT ?? "local", process.env.ERPX_DATABASE_MANIFEST,
    process.env.NEXT_DEPLOYMENT_ID ?? "standalone-dev", fence.database, actor.holdingId, actor.companyId,
    actor.userId, actor.assignmentId, actor.role, actor.beta, actor.scope.kind, actor.branchId, actor.departmentId,
    actor.assignmentScope, actor.inherited, actor.modules, actor.inspection, actor.authority, actor.iam, fence.revision];
  return `erpx:dev:work:v1:${createHash("sha256").update(JSON.stringify(identity)).digest("hex")}`;
}

export async function workCacheFence(db: SqlDatabase): Promise<CacheFence | null> {
  const row = await db.prepare(`SELECT current_database() || ':' || (SELECT oid::text FROM pg_database WHERE datname=current_database()) || ':' || coalesce(inet_server_addr()::text, 'local') || ':' || coalesce(inet_server_port()::text, 'socket') database,
    (SELECT md5(string_agg(table_name || ':' || revision::text, ',' ORDER BY table_name)) FROM work_cache_revisions) revision,
    NOT EXISTS (SELECT 1 FROM pg_tables t WHERE t.schemaname='public' AND work_cache_tracks(t.tablename)
      AND NOT EXISTS (SELECT 1 FROM work_cache_revisions r WHERE r.table_name=t.tablename)) covered,
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
    ) deadlines WHERE deadline > floor(extract(epoch FROM clock_timestamp())*1000)::bigint) deadline`).first<{
      database: string; revision: string; covered: boolean; deadline: number | null;
    }>();
  if (!row?.covered) return null;
  const now = Date.now();
  // Minute boundary also covers midnight in all current business timezones.
  return { database: row.database, revision: row.revision,
    validUntil: Math.min(now + 30_000, (Math.floor(now / 60_000) + 1) * 60_000, row.deadline ?? Infinity) };
}

export async function cachedWorkSnapshot(actor: WorkContext, db: SqlDatabase, load: () => Promise<WorkSnapshot>) {
  if (!redisCacheEnabled()) return load();
  // No revision read is cached in memory or Redis: reads after commit see the new fence.
  let fence: CacheFence | null;
  try { fence = await workCacheFence(db); } catch { return load(); }
  if (!fence) return load();
  for (let attempt = 0; attempt < 3; attempt++) {
    const current: CacheFence = fence;
    const key = workCacheKey(actor, current);
    const text = await cacheGet(key);
    if (text) {
      try {
        const entry = JSON.parse(text) as { validUntil: number; snapshot: WorkSnapshot };
        if (entry.validUntil > Date.now() && Array.isArray(entry.snapshot?.items) && entry.snapshot.unavailableSources?.length === 0) {
          const after = await workCacheFence(db).catch(() => null);
          if (after?.revision === current.revision && Date.now() < Math.min(current.validUntil, entry.validUntil)) {
            report("hit"); return entry.snapshot;
          }
          if (!after) return load();
          fence = after; continue;
        }
      } catch { /* Invalid cache entries are misses. */ }
    }
    let computation = pending.get(key);
    if (!computation || computation.validUntil <= Date.now()) {
      computation = { result: load(), validUntil: current.validUntil }; pending.set(key, computation);
    }
    try {
      const snapshot = await computation.result;
      const after = await workCacheFence(db).catch(() => null);
      if (!after) return snapshot;
      if (after.revision !== current.revision || Date.now() >= Math.min(current.validUntil, computation.validUntil)) { fence = after; continue; }
      const validUntil = Math.min(current.validUntil, computation.validUntil, actor.iam?.freshUntil ?? Infinity, actor.iam?.emergencyUntil ?? Infinity);
      if (!snapshot.unavailableSources.length)
        await cacheSet(key, JSON.stringify({ validUntil, snapshot }), validUntil - Date.now());
      report("miss"); return snapshot;
    } finally {
      if (pending.get(key) === computation) pending.delete(key);
    }
  }
  // Continuous writes must not cause an unbounded retry or publish a mixed cache entry.
  report("bypass"); return load();
}
function report(status: string) {
  if (process.env.ERPX_PERF_LOG === "1") console.info(JSON.stringify({ event: "work_cache", status }));
}
