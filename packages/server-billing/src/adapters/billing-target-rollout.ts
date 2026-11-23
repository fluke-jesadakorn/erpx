import {nativeCommercialOwnerRead,COMMERCIAL_RD_USAGE_SNAPSHOT} from '@erpx/server-commercial/adapters/native-owner-read';
import {nativeBillingOwnerQuery} from '@erpx/server-billing/adapters/native-billing-owner-write';
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {calculateTargetRolloutReadiness as calculate,type TargetRolloutRepository} from '@erpx/server-billing/application/target-rollout';
import type * as D from '@erpx/server-billing/domain/target-rollout';
export type {TargetMarginReport,TargetRolloutReadiness} from '@erpx/server-billing/domain/target-rollout';
export function nativeTargetRolloutRepository(c:PoolClient):TargetRolloutRepository{return {effectivePrice:async(value0)=>(await nativeBillingOwnerQuery(c,'target_rollout_effective_price',[value0])) as {rows:D.EffectivePriceRow[];rowCount:number|null},
ratedShadowPeriod:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'target_rollout_rated_shadow_period',[value0,value1])) as {rows:D.RatedShadowPeriodRow[];rowCount:number|null},
usageSnapshot:async(value0,value1)=>c.query<D.UsageSnapshotRow>(`SELECT snapshot_id FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2`,[value0,value1]),
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
