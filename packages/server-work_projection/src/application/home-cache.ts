import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { CacheFence, CachedResult } from "@erpx/server-work_projection/domain/work-cache";
import type { WorkCacheUnit } from "@erpx/server-work_projection/application/work-cache";
import type { Clock } from "@erpx/server-shared/application/ports";

export type HomeFill = { result: Promise<unknown>; validUntil: number };
export type HomeCachePorts = {
  clock: Clock; results: Map<string, CachedResult>; pending: Map<string, HomeFill>;
  hash(value: string): string; bytes(value: string): number;
  configuration(): { environment: string; databaseManifest?: string; deploymentId: string };
  fence(unit: WorkCacheUnit): Promise<CacheFence | null>;
  redisEnabled(): boolean; get(key: string): Promise<string | null>;
  set(key: string, text: string, ttl: number): Promise<unknown>;
  report(status: string, reason?: string): void;
};

/** Trusted authorized contexts only; HTTP/RSC callers resolve access before this reader. */
export function homeCacheIdentity(actor: WorkContext) {
  return [actor.holdingId, actor.companyId, actor.userId, actor.assignmentId,
    actor.assignments?.find(item => item.id === actor.assignmentId),
    actor.scope, actor.assignmentScope, actor.branchId, actor.departmentId, actor.inherited,
    actor.role, actor.beta, actor.inspection, actor.modules, actor.iam, actor.authority];
}
export function homeAuthorityDeadline(actor: WorkContext) {
  return Math.min(actor.iam?.freshUntil ?? Infinity, actor.iam?.emergencyUntil ?? Infinity,
    actor.assignments?.find(item => item.id === actor.assignmentId)?.validUntil ?? Infinity);
}
export function createHomeCache(ports: HomeCachePorts) {
  const { results, pending } = ports;
  function contextKey(actor: WorkContext) {
    return ports.hash(JSON.stringify(homeCacheIdentity(actor)));
  }
  function key(actor: WorkContext, fence: CacheFence, namespace: string, input: unknown) {
    return `erpx:home:v1:${ports.hash(JSON.stringify([ports.configuration(), fence.database,
      homeCacheIdentity(actor), fence.revision, namespace, input]))}`;
  }
  function prune() {
    for (const [key, entry] of results) if (entry.validUntil <= ports.clock.now()) results.delete(key);
  }
  function remember(key: string, text: string, validUntil: number) {
    const bytes = ports.bytes(text);
    if (bytes > 8 * 1024 * 1024 || validUntil <= ports.clock.now()) return;
    prune(); results.delete(key); results.set(key, { text, bytes, validUntil });
    let total = [...results.values()].reduce((sum, entry) => sum + entry.bytes, 0);
    for (const [oldest, entry] of results) {
      if (results.size <= 128 && total <= 8 * 1024 * 1024) break;
      results.delete(oldest); total -= entry.bytes;
    }
  }
  async function read<T>(actor: WorkContext, unit: WorkCacheUnit, namespace: string, input: unknown,
    load: () => Promise<T>, policy: { complete(value: T): boolean; deadline?(value: T): number } ) {
    const observe = () => ports.fence(unit).catch(() => null);
    let fence = await observe();
    if (!fence) { ports.report("bypass", "unverified_revision"); return load(); }
    prune();
    for (let attempt = 0; attempt < 3; attempt++) {
      const current: CacheFence = fence;
      if (homeAuthorityDeadline(actor) <= ports.clock.now()) {
        ports.report("bypass", "authority_expired"); return load();
      }
      const cacheKey = key(actor, current, namespace, input);
      const local = results.get(cacheKey);
      let text = local?.text;
      if (!text && ports.redisEnabled()) {
        try { text = await ports.get(cacheKey) ?? undefined; } catch { /* PostgreSQL fallback. */ }
      }
      if (text) {
        let entry: { value: T; validUntil: number } | undefined;
        try { entry = JSON.parse(text); } catch { /* Invalid entries are misses. */ }
        if (entry && entry.validUntil > ports.clock.now() && portsPolicyComplete(entry.value)) {
          const after = await observe();
          const until = Math.min(current.validUntil, after?.validUntil ?? 0,
            homeAuthorityDeadline(actor), entry.validUntil, policy.deadline?.(entry.value) ?? Infinity);
          if (after && after.database === current.database && after.revision === current.revision && until > ports.clock.now()) {
            remember(cacheKey, text, until); ports.report("hit", local ? "memory" : "redis");
            return entry.value;
          }
          if (!after) return load();
          fence = after; continue;
        }
      }
      let fill = pending.get(cacheKey);
      if (!fill || fill.validUntil <= ports.clock.now()) {
        fill = { result: load(), validUntil: Math.min(current.validUntil, homeAuthorityDeadline(actor)) };
        pending.set(cacheKey, fill);
      }
      try {
        const value = await fill.result as T;
        const after = await observe();
        if (!after) return value;
        const until = Math.min(current.validUntil, after.validUntil, fill.validUntil,
          homeAuthorityDeadline(actor), policy.deadline?.(value) ?? Infinity);
        if (after.database !== current.database || after.revision !== current.revision || until <= ports.clock.now()) {
          fence = after; continue;
        }
        if (policy.complete(value)) {
          const serialized = JSON.stringify({ value, validUntil: until });
          remember(cacheKey, serialized, until);
          try { await ports.set(cacheKey, serialized, until - ports.clock.now()); } catch { /* Optional cache. */ }
        }
        ports.report("miss");
        // Every concurrent caller owns its result, as it does on a serialized cache hit.
        return JSON.parse(JSON.stringify(value)) as T;
      } finally { if (pending.get(cacheKey) === fill) pending.delete(cacheKey); }
    }
    ports.report("bypass", "revision_changed"); return load();
    function portsPolicyComplete(value: T) {
      try { return policy.complete(value); } catch { return false; }
    }
  }
  return { read, key, contextKey };
}
