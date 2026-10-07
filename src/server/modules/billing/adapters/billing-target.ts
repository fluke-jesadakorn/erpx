import { lastAuditOwnerProjectionSources as commercialLastAuditProjectionSources } from "../../commercial/public/last-audit-owner-projection-sources";
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@/server/modules/runtime/public/runtime/postgres';
import {calculateTargetBillingPeriod as calculate,type TargetBillingRepository} from '../application/target';
import type * as D from '../domain/target';
export type {TargetMeasuredEvidence,TargetRatedLine,TargetBillingPeriodView} from '../domain/target';
export function nativeTargetBillingRepository(c:PoolClient):TargetBillingRepository{return {operatingCosts:(value0,value1)=>c.query<D.OperatingCostsRow>(`SELECT o.category,o.amount_satang,e.source_state
      FROM billing_target_operating_costs o
      JOIN billing_evidence e ON e.id=o.evidence_id
      WHERE o.holding_id=$1 AND o.month=$2`,[value0,value1]),
measuredSnapshot:(value0,value1)=>c.query<D.MeasuredSnapshotRow>(`SELECT snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,
      unreconciled_originals,seat_coverage,seat_source_count
     FROM (${commercialLastAuditProjectionSources.billing_usage_snapshots}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2`,[value0,value1]),
currentSeats:(value0,value1)=>c.query<D.CurrentSeatsRow>(`SELECT active_seat_count,active_seat_observed_at FROM (${commercialLastAuditProjectionSources.billing_usage_snapshots}) billing_usage_snapshots
     WHERE holding_id=$1 AND month=$2`,[value0,value1]),
measuredMeters:(value0,value1)=>c.query<D.MeasuredMetersRow>(`SELECT source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until,active_at_period_end,snapshot_id,
      byte_milliseconds::text AS byte_milliseconds
     FROM (${commercialLastAuditProjectionSources.billing_usage_meters}) billing_usage_meters WHERE holding_id=$1 AND month=$2 ORDER BY source_key`,[value0,value1]),
measuredSeatIntervals:(value0,value1)=>c.query<D.MeasuredSeatIntervalsRow>(`SELECT intervals_json,snapshot_id FROM (${commercialLastAuditProjectionSources.billing_seat_usage}) billing_seat_usage
     WHERE holding_id=$1 AND month=$2`,[value0,value1]),
assignment:(value0,value1)=>c.query<D.AssignedOfferingRow>(`SELECT * FROM billing_target_assignments WHERE holding_id=$1
     AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1`,[value0,value1]),
assignedPrice:(value0,value1)=>c.query<D.AssignedPriceRow>("SELECT * FROM billing_target_prices WHERE id=$1 AND effective_month<=$2",[value0,value1]),
approvedRating:(value0,value1)=>c.query<D.ApprovedRatingRow>("SELECT * FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
approvedInvoice:(value0)=>c.query<D.ApprovedInvoiceRow>("SELECT id,total_satang,status,stripe_id FROM billing_invoices WHERE id=$1",[value0]),
approvedStatements:(value0,value1)=>c.query<D.ApprovedStatementsRow>("SELECT * FROM billing_target_statements WHERE holding_id=$1 AND month=$2",[value0,value1]),
rolloutEligibility:(value0,value1)=>c.query<D.RolloutEligibilityRow>(`SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1
     UNION ALL SELECT 1 FROM billing_target_rollout_approvals
     WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1`,[value0,value1]),
legacyEnrollmentOverlap:(value0,value1,value2)=>c.query<D.LegacyEnrollmentOverlapRow>(`SELECT 1 FROM billing_enrollments WHERE holding_id=$1 AND plan_id<>'platform_usage'
     AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2`,[value0,value1,value2]),
monthInvoices:(value0,value1)=>c.query<D.MonthInvoicesRow>(`SELECT 1 FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id
     WHERE i.holding_id=$1 AND p.month=$2 AND i.status<>'void'`,[value0,value1]),
paymentConsent:(value0)=>c.query<D.PaymentConsentRow>("SELECT payment_mode,consent_at,card_json FROM billing_accounts WHERE holding_id=$1",[value0]),
ratingSnapshot:(value0,value1)=>c.query<D.RatingSnapshotRow>(`SELECT * FROM (${commercialLastAuditProjectionSources.billing_usage_snapshots}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2`,[value0,value1]),
ratingMeters:(value0,value1)=>c.query<D.RatingMetersRow>(`SELECT source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until
     FROM (${commercialLastAuditProjectionSources.billing_usage_meters}) billing_usage_meters WHERE holding_id=$1 AND month=$2 ORDER BY source_key`,[value0,value1]),
providerCosts:(value0,value1)=>c.query<D.ProviderCostsRow>("SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2",[value0,value1]),
providerStatements:(value0,value1)=>c.query<D.ProviderStatementsRow>("SELECT * FROM billing_target_statements WHERE holding_id=$1 AND month=$2",[value0,value1]),
approvedExtras:(value0,value1)=>c.query<D.ApprovedExtrasRow>("SELECT id,kind,description,amount_satang,evidence_id FROM billing_target_extras WHERE holding_id=$1 AND month=$2 ORDER BY approved_at,id",[value0,value1]),
ratingSeatIntervals:(value0,value1)=>c.query<D.RatingSeatIntervalsRow>(`SELECT user_key,intervals_json,snapshot_id FROM (${commercialLastAuditProjectionSources.billing_seat_usage}) billing_seat_usage WHERE holding_id=$1 AND month=$2 ORDER BY user_key`,[value0,value1])}}
export const calculateTargetBillingPeriod=(c:PoolClient,holdingId:string,month:string,now:number)=>calculate(nativeTargetBillingRepository(c),holdingId,month,now);
export async function readTargetBillingPeriod(db:PostgresDatabase,holdingId:string,month:string,now=Date.now()){const c=await db.pool.connect();try{return await calculateTargetBillingPeriod(c,holdingId,month,now)}finally{c.release()}}
