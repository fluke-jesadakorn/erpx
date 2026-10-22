import { lastAuditOwnerProjectionSources as commercialLastAuditProjectionSources } from "@erpx/server-commercial/public/last-audit-owner-projection-sources";
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {calculateTargetRolloutReadiness as calculate,type TargetRolloutRepository} from '@erpx/server-billing/application/target-rollout';
import type * as D from '@erpx/server-billing/domain/target-rollout';
export type {TargetMarginReport,TargetRolloutReadiness} from '@erpx/server-billing/domain/target-rollout';
export function nativeTargetRolloutRepository(c:PoolClient):TargetRolloutRepository{return {effectivePrice:(value0)=>c.query<D.EffectivePriceRow>(`SELECT id,included_original_bytes,seat_terms_status,rating_enabled
      FROM billing_target_prices WHERE effective_month<=$1
      ORDER BY effective_month DESC,id DESC LIMIT 1`,[value0]),
ratedShadowPeriod:(value0,value1)=>c.query<D.RatedShadowPeriodRow>("SELECT state,mode,total_satang,price_id,meter_snapshot_id,lines_json FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2",[value0,value1]),
usageSnapshot:(value0,value1)=>c.query<D.UsageSnapshotRow>(`SELECT snapshot_id FROM (${commercialLastAuditProjectionSources.billing_usage_snapshots}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2`,[value0,value1]),
availableOperatingCosts:(value0,value1)=>c.query<D.AvailableOperatingCostsRow>(`SELECT o.category,o.amount_satang,e.source_state
       FROM billing_target_operating_costs o
       JOIN billing_evidence e ON e.id=o.evidence_id
       WHERE o.holding_id=$1 AND o.month=$2`,[value0,value1]),
capacityReview:(value0,value1)=>c.query<D.CapacityReviewRow>(`SELECT r.*,e.source_state FROM billing_target_capacity_reviews r
     JOIN billing_evidence e ON e.id=r.evidence_id
     WHERE r.holding_id=$1 AND r.effective_month=$2`,[value0,value1])}}
import {betaLimits} from '@erpx/server-manage_erpx/public/beta-config';
export const calculateTargetRolloutReadiness=(c:PoolClient,holdingId:string,effectiveMonth:string,minimumMarginBps:number,now:number)=>calculate(nativeTargetRolloutRepository(c),holdingId,effectiveMonth,minimumMarginBps,now,betaLimits({BETA_STORAGE_BYTES:process.env.BETA_STORAGE_BYTES}).storage);
export async function readTargetRolloutReadiness(db:PostgresDatabase,holdingId:string,effectiveMonth:string,minimumMarginBps:number,now=Date.now()){const c=await db.pool.connect();try{return await calculateTargetRolloutReadiness(c,holdingId,effectiveMonth,minimumMarginBps,now)}finally{c.release()}}
