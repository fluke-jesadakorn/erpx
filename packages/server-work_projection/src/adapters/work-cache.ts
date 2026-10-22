import {workCacheOperationsDeadlines} from "@erpx/server-operations/public/work-cache-deadlines";
import {workCacheAgentDeadlines} from "@erpx/server-agent/public/work-cache-deadlines";
import {workCachePeoplePayrollDeadlines} from "@erpx/server-people_payroll/public/work-cache-deadlines";
import {workCacheIdentityAccessDeadlines} from "@erpx/server-identity_access/public/work-cache-deadlines";
import type {WorkSnapshot} from "@erpx/server-work_projection/domain/work-items";
import "server-only";
import { createHash } from "node:crypto";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import { cacheGet, cacheSet, redisCacheEnabled } from "@erpx/server-runtime/public/runtime/redis-cache";

import type {CacheFence,InFlight,CachedResult} from "@erpx/server-work_projection/domain/work-cache";
import {createWorkCache,type WorkCacheUnit} from "@erpx/server-work_projection/application/work-cache";
const shared = globalThis as typeof globalThis & {
  erpxWorkFillsV2?: Map<string, InFlight>; erpxWorkResultsV1?: Map<string, CachedResult>;
};
const pending = shared.erpxWorkFillsV2 ??= new Map<string, InFlight>();
const results = shared.erpxWorkResultsV1 ??= new Map<string, CachedResult>();

const service=createWorkCache({clock:{now:()=>Date.now()},pending,results,bytes:value=>Buffer.byteLength(value),hash:value=>createHash("sha256").update(value).digest("hex"),configuration:()=>({environment:process.env.ERPX_ENVIRONMENT??"local",databaseManifest:process.env.ERPX_DATABASE_MANIFEST,deploymentId:process.env.NEXT_DEPLOYMENT_ID??"standalone-dev"}),redisEnabled:redisCacheEnabled,get:cacheGet,set:cacheSet,report:(status,reason)=>{if(process.env.ERPX_PERF_LOG==="1")console.info(JSON.stringify({event:"work_cache",status,reason}));}});
export function workCacheUnit(db:SqlDatabase):WorkCacheUnit{return {readFence:()=>db.prepare(`SELECT current_database() || ':' || (SELECT oid::text FROM pg_database WHERE datname=current_database()) || ':' || coalesce(inet_server_addr()::text, 'local') || ':' || coalesce(inet_server_port()::text, 'socket') database,
    (SELECT md5(string_agg(table_name || ':' || revision::text, ',' ORDER BY table_name)) FROM work_cache_revisions WHERE table_name <> 'legal_runtime_health') revision,
    NOT EXISTS (SELECT 1 FROM pg_tables t WHERE t.schemaname='public' AND work_cache_tracks(t.tablename)
      AND (NOT EXISTS (SELECT 1 FROM work_cache_revisions r WHERE r.table_name=t.tablename)
        OR (SELECT count(*) FROM pg_trigger g WHERE g.tgrelid=to_regclass(format('public.%I',t.tablename))
          AND g.tgname IN ('work_cache_change','work_cache_truncate') AND g.tgenabled='O') <> 2)) covered,
    (SELECT min(deadline) FROM (
      ${workCacheIdentityAccessDeadlines()} UNION ALL ${workCachePeoplePayrollDeadlines()} UNION ALL ${workCacheAgentDeadlines()} UNION ALL ${workCacheOperationsDeadlines()}
    ) deadlines WHERE deadline > floor(extract(epoch FROM clock_timestamp())*1000)::bigint) deadline`).first<{
      database: string; revision: string; covered: boolean; deadline: number | null;
    }>()};}
export function workCacheKey(actor: WorkContext,fence: CacheFence){return service.workCacheKey(actor,fence);}
export function workCacheFence(db: SqlDatabase){return service.workCacheFence(workCacheUnit(db));}
export function cachedWorkSnapshot(actor: WorkContext,db: SqlDatabase,load: () => Promise<WorkSnapshot>){return service.cachedWorkSnapshot(actor,workCacheUnit(db),load);}
export type {CacheFence} from "@erpx/server-work_projection/domain/work-cache";
