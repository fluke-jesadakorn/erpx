import {nativeCommercialOwnerRead,COMMERCIAL_RD_USAGE_METER,COMMERCIAL_RD_USAGE_SEAT,COMMERCIAL_RD_USAGE_SNAPSHOT} from '@erpx/commercial-server/adapters/native-owner-read';
import 'server-only';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/platform-runtime-server/public/runtime/postgres';
import {calculateTargetBillingPeriod as calculate,type TargetBillingRepository} from '@erpx/billing-server/application/target';
import {nativeBillingOwnerQuery} from '@erpx/billing-server/adapters/native-billing-owner-write';
import {BILLING_ROLLOUT_ELIGIBILITY_SQL,BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL,BILLING_MONTH_INVOICE_SQL} from './billing-read-tail-r16-sql';
import {billingProviderCostsSql,billingUsageSnapshotSql} from './billing-read-tail-r19-sql';
import {billingUsageMeterSql,billingSeatUsageSql} from './billing-read-tail-r20-sql';
import {billingPointReadSql} from './billing-read-tail-r29-sql';
import {billingTargetMonthSql} from './billing-read-tail-r31-sql';
import type * as D from '@erpx/billing-server/domain/target';
export type {TargetMeasuredEvidence,TargetRatedLine,TargetBillingPeriodView} from '@erpx/billing-server/domain/target';
export function nativeTargetBillingRepository(c:PoolClient):TargetBillingRepository{return {operatingCosts:(value0,value1)=>c.query<D.OperatingCostsRow>(billingPointReadSql('o.category,o.amount_satang,e.source_state\n     ','billing_target_operating_costs o\n      JOIN billing_evidence e ON e.id=o.evidence_id\n     ','o.holding_id=$1 AND o.month=$2'),[value0,value1]),
measuredSnapshot:async(value0,value1)=>c.query<D.MeasuredSnapshotRow>(await billingUsageSnapshotSql(`snapshot_id,refreshed_at,source_count,ai_coverage,storage_coverage,
      unreconciled_originals,seat_coverage,seat_source_count`,await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)),[value0,value1]),
currentSeats:async(value0,value1)=>c.query<D.CurrentSeatsRow>(await billingUsageSnapshotSql('active_seat_count,active_seat_observed_at',await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)),[value0,value1]),
measuredMeters:async(value0,value1)=>c.query<D.MeasuredMetersRow>(await billingUsageMeterSql(`source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until,active_at_period_end,snapshot_id,
      byte_milliseconds::text AS byte_milliseconds`,await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_METER)),[value0,value1]),
measuredSeatIntervals:async(value0,value1)=>c.query<D.MeasuredSeatIntervalsRow>(await billingSeatUsageSql('intervals_json,snapshot_id',await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SEAT),`
     WHERE holding_id=$1 AND month=$2`),[value0,value1]),
assignment:(value0,value1)=>c.query<D.AssignedOfferingRow>(billingPointReadSql('*','billing_target_assignments',`holding_id=$1
     AND effective_month<=$2 ORDER BY effective_month DESC LIMIT 1`),[value0,value1]),
assignedPrice:(value0,value1)=>nativeBillingOwnerQuery(c,'selected_target_price',[value0,value1]) as Promise<{rows:D.AssignedPriceRow[];rowCount:number|null}>,
approvedRating:(value0,value1)=>c.query<D.ApprovedRatingRow>(billingTargetMonthSql('*','billing_target_rated_periods'," AND state='approved'"),[value0,value1]),
approvedInvoice:(value0)=>c.query<D.ApprovedInvoiceRow>(billingPointReadSql('id,total_satang,status,stripe_id','billing_invoices','id=$1'),[value0]),
approvedStatements:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'target_statements',[value0,value1])) as {rows:D.ApprovedStatementsRow[];rowCount:number|null},
rolloutEligibility:(value0,value1)=>c.query<D.RolloutEligibilityRow>(BILLING_ROLLOUT_ELIGIBILITY_SQL,[value0,value1]),
legacyEnrollmentOverlap:(value0,value1,value2)=>c.query<D.LegacyEnrollmentOverlapRow>(BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL,[value0,value1,value2]),
monthInvoices:(value0,value1)=>c.query<D.MonthInvoicesRow>(BILLING_MONTH_INVOICE_SQL,[value0,value1]),
paymentConsent:(value0)=>c.query<D.PaymentConsentRow>(billingPointReadSql('payment_mode,consent_at,card_json','billing_accounts','holding_id=$1'),[value0]),
ratingSnapshot:async(value0,value1)=>c.query<D.RatingSnapshotRow>(await billingUsageSnapshotSql('*',await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)),[value0,value1]),
ratingMeters:async(value0,value1)=>c.query<D.RatingMetersRow>(await billingUsageMeterSql(`source_key,meter_kind,company_id,branch_id,funding_source,quantity,
      input_tokens,output_tokens,request_status,gateway_status,provider,model,units_json,
      cost_status,verified_cost_satang,cost_evidence_key,cost_reconciled_at,
      active_from,active_until`,await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_METER)),[value0,value1]),
providerCosts:(value0,value1)=>c.query<D.ProviderCostsRow>(billingProviderCostsSql(),[value0,value1]),
providerStatements:async(value0,value1)=>(await nativeBillingOwnerQuery(c,'target_statements',[value0,value1])) as {rows:D.ProviderStatementsRow[];rowCount:number|null},
approvedExtras:(value0,value1)=>c.query<D.ApprovedExtrasRow>(billingTargetMonthSql('id,kind,description,amount_satang,evidence_id','billing_target_extras',' ORDER BY approved_at,id'),[value0,value1]),
ratingSeatIntervals:async(value0,value1)=>c.query<D.RatingSeatIntervalsRow>(await billingSeatUsageSql('user_key,intervals_json,snapshot_id',await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SEAT),' WHERE holding_id=$1 AND month=$2 ORDER BY user_key'),[value0,value1])}}
export const calculateTargetBillingPeriod=(c:PoolClient,holdingId:string,month:string,now:number)=>calculate(nativeTargetBillingRepository(c),holdingId,month,now);
export async function readTargetBillingPeriod(db:PostgresDatabase,holdingId:string,month:string,now=Date.now()){const c=await db.pool.connect();try{return await calculateTargetBillingPeriod(c,holdingId,month,now)}finally{c.release()}}
