import {nativeBillingOwnerQuery} from '@erpx/server-billing/adapters/native-billing-owner-write';
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {calculateTargetRolloutReadiness as calculate,type TargetRolloutRepository} from '@erpx/server-billing/application/target-rollout';
import type * as D from '@erpx/server-billing/domain/target-rollout';
export type {TargetMarginReport,TargetRolloutReadiness} from '@erpx/server-billing/domain/target-rollout';
export function nativeTargetRolloutRepository(c:PoolClient):TargetRolloutRepository{return {effectivePrice:async(value0)=>(await nativeBillingOwnerQuery(c,'target_rollout_effective_price',[value0])) as {rows:D.EffectivePriceRow[];rowCount:number|null},
ratedShadowPeriod:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'target_rollout_rated_shadow_period',[value0,value1])) as {rows:D.RatedShadowPeriodRow[];rowCount:number|null},
usageSnapshot:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'forecast_usage_snapshot',[value0,value1])) as {rows:D.UsageSnapshotRow[];rowCount:number|null},
availableOperatingCosts:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'target_rollout_available_operating_costs',[value0,value1])) as {rows:D.AvailableOperatingCostsRow[];rowCount:number|null},
capacityReview:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'target_rollout_capacity_review',[value0,value1])) as {rows:D.CapacityReviewRow[];rowCount:number|null}}}
import {betaLimits} from '@erpx/server-manage_erpx/public/beta-config';
export const calculateTargetRolloutReadiness=(c:PoolClient,holdingId:string,effectiveMonth:string,minimumMarginBps:number,now:number)=>calculate(nativeTargetRolloutRepository(c),holdingId,effectiveMonth,minimumMarginBps,now,betaLimits({BETA_STORAGE_BYTES:process.env.BETA_STORAGE_BYTES}).storage);
export async function readTargetRolloutReadiness(db:PostgresDatabase,holdingId:string,effectiveMonth:string,minimumMarginBps:number,now=Date.now()){const c=await db.pool.connect();try{return await calculateTargetRolloutReadiness(c,holdingId,effectiveMonth,minimumMarginBps,now)}finally{c.release()}}
