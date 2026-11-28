import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativeInboxFailedMetrics,type HoldingSelector,nativeWorkflowLane32Configuration} from './native-lane-32/client';
/**
 * Native owner read. The axum/sqlx owner executes the dead-letter count; this
 * adapter carries no SQL. A WorkContext/origin scope must be threaded in by the
 * caller: with no scope the native client fails closed rather than choosing a
 * Holding. The current `server-collaboration` owner3 source calls
 * `collaborationInboxSources(db)` without a WorkContext and therefore stays
 * fail-closed until composition passes one through.
 */
export function inboxFailedScope(holdingId:string):{holdingId:string;holdingSlug:string}|undefined {
  const raw=process.env.ERPX_NATIVE_WORKFLOW_LANE32_CONFIG;if(!raw)return undefined;
  const cfg=nativeWorkflowLane32Configuration(raw);
  const holding=cfg.holdings.find(item=>item.holdingId===holdingId);
  return holding?{holdingId:holding.holdingId,holdingSlug:holding.holdingSlug}:undefined;
}
export function collaborationInboxSources(db:SqlDatabase,scope?:HoldingSelector){return {
async failedMetrics(){return nativeInboxFailedMetrics(db,scope);},
};}
