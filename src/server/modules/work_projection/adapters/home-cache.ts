import { measureReadPhase } from "../../runtime/public/runtime/read-metrics";
import "server-only";
import { createHash } from "node:crypto";
import type { WorkContext } from "../../identity_access/public/authorization";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { cacheGet, cacheSet, redisCacheEnabled } from "../../runtime/public/runtime/redis-cache";
import type { CachedResult } from "../domain/work-cache";
import { createHomeCache, type HomeFill } from "../application/home-cache";
import { workCacheUnit } from "./work-cache";
const shared = globalThis as typeof globalThis & {
  erpxHomeResultsV1?: Map<string, CachedResult>; erpxHomeFillsV1?: Map<string, HomeFill>;
};
const service = createHomeCache({
  clock: { now: Date.now }, results: shared.erpxHomeResultsV1 ??= new Map(), pending: shared.erpxHomeFillsV1 ??= new Map(),
  hash: text => createHash("sha256").update(text).digest("hex"), bytes: text => Buffer.byteLength(text),
  configuration: () => ({ environment: process.env.ERPX_ENVIRONMENT ?? "local",
    databaseManifest: process.env.ERPX_DATABASE_MANIFEST, deploymentId: process.env.NEXT_DEPLOYMENT_ID ?? "standalone-dev" }),
  fence: unit => measureReadPhase("home_revision", () => unit.readFence()).then(row => row?.covered ? {
    database: row.database, revision: row.revision,
    validUntil: Math.min(Date.now() + 30_000, (Math.floor(Date.now() / 60_000) + 1) * 60_000, row.deadline ?? Infinity),
  } : null),
  redisEnabled: redisCacheEnabled, get: cacheGet, set: cacheSet,
  report: (status, reason) => { if (process.env.ERPX_PERF_LOG === "1") console.info(JSON.stringify({ event: "home_cache", status, reason })); },
});
export const homeContextKey = service.contextKey;
export function cachedHomeRead<T>(actor: WorkContext, db: SqlDatabase, namespace: string, input: unknown,
  load: () => Promise<T>, policy: { complete(value: T): boolean; deadline?(value: T): number }) {
  return service.read(actor, workCacheUnit(db), namespace, input, load, policy);
}
