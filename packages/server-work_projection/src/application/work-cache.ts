import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {WorkSnapshot} from "@erpx/server-work_projection/domain/work-items";
import type {CacheFence,InFlight,CachedResult} from "@erpx/server-work_projection/domain/work-cache";
import type {Clock} from "@erpx/server-shared/application/ports";
export type WorkCacheUnit={readFence():Promise<{database:string;revision:string;covered:boolean;deadline:number|null}|null>};
export type WorkCachePorts={clock:Clock;pending:Map<string,InFlight>;results:Map<string,CachedResult>;bytes(value:string):number;hash(value:string):string;configuration():{environment:string;databaseManifest:string|undefined;deploymentId:string};redisEnabled():boolean;get(key:string):Promise<string|null>;set(key:string,text:string,ttl:number):Promise<unknown>;report(status:string,reason?:string):void};
export function createWorkCache(ports:WorkCachePorts){const {pending,results}=ports;
const maxResults = 128, maxResultBytes = 8 * 1024 * 1024;
function pruneResults() {
  const now = ports.clock.now();
  for (const [key, entry] of results) if (entry.validUntil <= now) results.delete(key);
}
function remember(key: string, snapshot: WorkSnapshot, validUntil: number) {
  if (validUntil <= ports.clock.now() || snapshot.unavailableSources.length) return;
  const text = JSON.stringify({ validUntil, snapshot }), bytes = ports.bytes(text);
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
function workCacheKey(actor: WorkContext, fence: CacheFence) {
  const identity = [ports.configuration().environment, ports.configuration().databaseManifest,
    ports.configuration().deploymentId, fence.database, actor.holdingId, actor.companyId,
    actor.userId, actor.assignmentId, actor.role, actor.beta, actor.scope.kind, actor.branchId, actor.departmentId,
    actor.assignmentScope, actor.inherited, actor.modules, actor.inspection, actor.authority, actor.iam, fence.revision];
  return `erpx:dev:work:v1:${ports.hash(JSON.stringify(identity))}`;
}

async function workCacheFence(db: WorkCacheUnit): Promise<CacheFence | null> {
  const row = await db.readFence();
  if (!row?.covered) return null;
  // Scheduler health observations have their own live endpoint and are not
  // WorkSnapshot inputs. Business Legal records still invalidate the snapshot.
  const now = ports.clock.now();
  // Minute boundary also covers midnight in all current business timezones.
  return { database: row.database, revision: row.revision,
    validUntil: Math.min(now + 30_000, (Math.floor(now / 60_000) + 1) * 60_000, row.deadline ?? Infinity) };
}

async function cachedWorkSnapshot(actor: WorkContext, db: WorkCacheUnit, load: () => Promise<WorkSnapshot>) {
  // No revision read is cached in memory or Redis: reads after commit see the new fence.
  let fence: CacheFence | null;
  try { fence = await workCacheFence(db); } catch { report("bypass", "fence_read_failed"); return load(); }
  if (!fence) { report("bypass", "incomplete_coverage"); return load(); }
  pruneResults();
  for (let attempt = 0; attempt < 3; attempt++) {
    if (authorityDeadline(actor) <= ports.clock.now()) { report("bypass", "authority_expired"); return load(); }
    const current: CacheFence = fence;
    const key = workCacheKey(actor, current);
    const local = results.get(key);
    const text = local?.text ?? (ports.redisEnabled() ? await ports.get(key) : null);
    if (text) {
      try {
        const entry = JSON.parse(text) as { validUntil: number; snapshot: WorkSnapshot };
        if (entry.validUntil > ports.clock.now() && Array.isArray(entry.snapshot?.items) && entry.snapshot.unavailableSources?.length === 0) {
          const after = await workCacheFence(db).catch(() => null);
          const validUntil = Math.min(current.validUntil, after?.validUntil ?? 0, entry.validUntil, authorityDeadline(actor));
          if (after?.database === current.database && after.revision === current.revision && ports.clock.now() < validUntil) {
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
    if (!computation || computation.validUntil <= ports.clock.now()) {
      computation = { result: load(), validUntil: Math.min(current.validUntil, authorityDeadline(actor)) }; pending.set(key, computation);
    }
    try {
      const snapshot = await computation.result;
      const after = await workCacheFence(db).catch(() => null);
      if (!after) return snapshot;
      if (after.database !== current.database || after.revision !== current.revision || ports.clock.now() >= Math.min(current.validUntil, computation.validUntil, after.validUntil)) { fence = after; continue; }
      const validUntil = Math.min(current.validUntil, computation.validUntil, after.validUntil, authorityDeadline(actor));
      if (!snapshot.unavailableSources.length) {
        remember(key, snapshot, validUntil);
        await ports.set(key, JSON.stringify({ validUntil, snapshot }), validUntil - ports.clock.now());
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
  ports.report(status,reason);
}

return {workCacheKey,workCacheFence,cachedWorkSnapshot};}
