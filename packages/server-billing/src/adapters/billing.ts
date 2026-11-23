import {nativeCommercialOwnerRead,COMMERCIAL_RD_USAGE_METER,COMMERCIAL_RD_USAGE_SNAPSHOT} from '@erpx/server-commercial/adapters/native-owner-read';
import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {nativeBillingOwnerQuery} from '@erpx/server-billing/adapters/native-billing-owner-write';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {ApiError} from '@erpx/server-runtime/public/http';
import {billingHoldingFacts} from '@erpx/server-runtime/public/billing-holding-facts';
import {createBillingService,type BillingRepository} from '@erpx/server-billing/application/billing';
import type {BillingActor,BillingInput,BillingReceipt,BillingAccount,BillingEnrollment,BillingPeriod,BillingPlan,BillingInvoiceRow,BillingJobRow,BillingPriceRow,BillingMeterRow,BillingViewFacts} from '@erpx/server-billing/domain/billing';
import type {BillingStripe} from '@erpx/server-billing/domain/stripe';
import {calculateTargetBillingPeriod} from '@erpx/server-billing/adapters/billing-target';
import {calculateTargetRolloutReadiness} from '@erpx/server-billing/adapters/billing-target-rollout';
export {billingInput} from '@erpx/server-billing/domain/billing';
export type {BillingActor,BillingInput} from '@erpx/server-billing/domain/billing';
export function nativeBillingRepository(c:PoolClient):BillingRepository{return {
recordRatingRun:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11)=>nativeBillingOwnerQuery(c, 'record_rating_run', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11]),
retainRatedPeriod:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12)=>nativeBillingOwnerQuery(c, 'retain_rated_period', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12]),
activeEnrollmentForAdvance:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE",[value0]),
latestEnrollmentPeriod:(value0)=>c.query<BillingPeriod>("SELECT * FROM billing_periods WHERE enrollment_id=$1 ORDER BY month DESC LIMIT 1",[value0]),
recordScheduledChange:(value0,value1,value2,value3,value4,value5,value6)=>nativeBillingOwnerQuery(c, 'record_scheduled_change', [value0,value1,value2,value3,value4,value5,value6]),
endEnrollment:(value0)=>nativeBillingOwnerQuery(c, 'end_enrollment', [value0]),
activatePendingPlan:(value0,value1)=>nativeBillingOwnerQuery(c, 'activate_pending_plan', [value0,value1]),
effectivePlan:(value0,value1)=>c.query<BillingPlan>("SELECT * FROM billing_plans WHERE id=$1 AND effective_month<=$2 ORDER BY revision DESC LIMIT 1",[value0,value1]),
createBillingPeriod:(value0,value1,value2,value3,value4,value5,value6,value7,value8)=>nativeBillingOwnerQuery(c, 'create_billing_period', [value0,value1,value2,value3,value4,value5,value6,value7,value8]),
closeElapsedPeriods:(value0,value1)=>nativeBillingOwnerQuery(c, 'close_elapsed_periods', [value0,value1]),
begin:()=>c.query<Record<string,unknown>>("BEGIN",[]),
lockHolding:(value0)=>c.query<Record<string,unknown>>("SELECT pg_advisory_xact_lock(hashtextextended($1,0))",[value0]),
lockCommand:(value0)=>c.query<Record<string,unknown>>("SELECT pg_advisory_xact_lock(hashtextextended($1,0))",[value0]),
receipt:(value0)=>c.query<BillingReceipt>("SELECT * FROM billing_receipts WHERE id=$1",[value0]),
commitReplay:()=>c.query<Record<string,unknown>>("COMMIT",[]),
holdingExists:value0=>billingHoldingFacts(c).holdingExists(value0),
ensureAccount:(value0,value1)=>nativeBillingOwnerQuery(c, 'ensure_account', [value0,value1]),
lockedAccount:(value0)=>c.query<BillingAccount>("SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE",[value0]),
activeEnrollment:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL",[value0]),
retainCustomer:(value0,value1)=>nativeBillingOwnerQuery(c, 'retain_customer', [value0,value1]),
enroll:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'enroll', [value0,value1,value2,value3]),
scheduleEnrollmentChange:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'schedule_enrollment_change', [value0,value1,value2,value3]),
retainConsentedCard:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'retain_consented_card', [value0,value1,value2,value3]),
automaticOpenInvoices:(value0)=>c.query<BillingInvoiceRow>("SELECT * FROM billing_invoices WHERE holding_id=$1 AND status='open' AND payment_mode='auto'",[value0]),
queueManualInvoice:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'queue_manual_invoice', [value0,value1,value2,value3]),
withdrawConsent:(value0)=>nativeBillingOwnerQuery(c, 'withdraw_consent', [value0]),
setPaymentMode:(value0,value1)=>nativeBillingOwnerQuery(c, 'set_payment_mode', [value0,value1]),
lockPlanRevision:()=>c.query<Record<string,unknown>>("SELECT pg_advisory_xact_lock(hashtextextended('billing-plans',0))",[]),
latestPlanRevision:(value0)=>c.query<BillingPlan>("SELECT * FROM billing_plans WHERE id=$1 ORDER BY revision DESC LIMIT 1",[value0]),
insertPlanRevision:(value0,value1,value2,value3,value4,value5)=>nativeBillingOwnerQuery(c, 'insert_plan_revision', [value0,value1,value2,value3,value4,value5]),
selectedTargetPrice:(value0,value1)=>c.query<BillingPriceRow>("SELECT * FROM billing_target_prices WHERE id=$1 AND effective_month<=$2",[value0,value1]),
currentTargetPrice:(value0)=>c.query<BillingPriceRow>("SELECT * FROM billing_target_prices WHERE effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1",[value0]),
sandboxHolding:(value0)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1",[value0]),
approvedRollout:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rollout_approvals WHERE holding_id=$1 AND effective_month<=$2",[value0,value1]),
assignTargetPrice:(value0,value1,value2,value3,value4,value5,value6,value7)=>nativeBillingOwnerQuery(c, 'assign_target_price', [value0,value1,value2,value3,value4,value5,value6,value7]),
successfulPlatformMeter:async(value0,value1,value2)=>c.query<BillingMeterRow>(`SELECT * FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_METER)}) billing_usage_meters WHERE holding_id=$1 AND month=$2 AND source_key=$3
           AND meter_kind='ai' AND funding_source='erpx' AND request_status='succeeded'`,[value0,value1,value2]),
availableProviderEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
approvedProviderPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
existingProviderCost:(value0,value1,value2)=>c.query<Record<string,unknown>>("SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND source_key=$3",[value0,value1,value2]),
recordProviderCost:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12)=>nativeBillingOwnerQuery(c, 'record_provider_cost', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12]),
availableStatementEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
approvedStatementPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
providerStatementTotal:(value0,value1,value2,value3,value4)=>c.query<{count:number;total:string}>(`SELECT COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total
           FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND provider=$3
             AND statement_id=$4 AND original_currency=$5`,[value0,value1,value2,value3,value4]),
recordProviderStatement:(value0,value1,value2,value3,value4,value5,value6,value7,value8)=>nativeBillingOwnerQuery(c, 'record_provider_statement', [value0,value1,value2,value3,value4,value5,value6,value7,value8]),
availableOperatingEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
recordOperatingCost:(value0,value1,value2,value3,value4,value5,value6)=>nativeBillingOwnerQuery(c, 'record_operating_cost', [value0,value1,value2,value3,value4,value5,value6]),
availableCapacityEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
recordCapacityReview:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10)=>nativeBillingOwnerQuery(c, 'record_capacity_review', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10]),
recordRolloutApproval:(value0,value1,value2,value3,value4,value5,value6,value7)=>nativeBillingOwnerQuery(c, 'record_rollout_approval', [value0,value1,value2,value3,value4,value5,value6,value7]),
extraTargetAssignment:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_assignments WHERE holding_id=$1 AND effective_month<=$2",[value0,value1]),
approvedExtraPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
availableExtraEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
recordTargetExtra:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11)=>nativeBillingOwnerQuery(c, 'record_target_extra', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11]),
lockTargetSnapshot:async(value0,value1)=>c.query<Record<string,unknown>>(`SELECT snapshot_id FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2 FOR UPDATE`,[value0,value1]),
targetRolloutEligible:(value0,value1)=>c.query<Record<string,unknown>>(`SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1
           UNION ALL SELECT 1 FROM billing_target_rollout_approvals
           WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1`,[value0,value1]),
approvedTargetPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
legacyEnrollmentOverlap:(value0,value1,value2)=>c.query<BillingEnrollment>(`SELECT 1 FROM billing_enrollments WHERE holding_id=$1 AND plan_id<>'platform_usage'
           AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2`,[value0,value1,value2]),
monthInvoiceExists:(value0,value1)=>c.query<BillingInvoiceRow>(`SELECT 1 FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id
           WHERE i.holding_id=$1 AND p.month=$2 AND i.status<>'void'`,[value0,value1]),
activeTargetEnrollment:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL",[value0]),
enrollTarget:(value0,value1,value2)=>nativeBillingOwnerQuery(c, 'enroll_target', [value0,value1,value2]),
retainedTargetEnrollment:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL",[value0]),
targetPeriod:(value0,value1)=>c.query<BillingPeriod>("SELECT * FROM billing_periods WHERE enrollment_id=$1 AND month=$2",[value0,value1]),
insertTargetInvoice:(value0,value1,value2,value3,value4,value5,value6,value7)=>nativeBillingOwnerQuery(c, 'insert_target_invoice', [value0,value1,value2,value3,value4,value5,value6,value7]),
approveTargetRating:(value0,value1,value2,value3,value4,value5)=>nativeBillingOwnerQuery(c, 'approve_target_rating', [value0,value1,value2,value3,value4,value5]),
queueTargetIssue:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'queue_target_issue', [value0,value1,value2,value3]),
selectedJob:(value0,value1)=>c.query<BillingJobRow>("SELECT * FROM billing_jobs WHERE id=$1 AND holding_id=$2",[value0,value1]),
retryLegacyEvent:(value0,value1,value2)=>nativeBillingOwnerQuery(c, 'retry_legacy_event', [value0,value1,value2]),
retryJob:(value0,value1)=>nativeBillingOwnerQuery(c, 'retry_job', [value0,value1]),
incrementAccountVersion:(value0)=>nativeBillingOwnerQuery(c, 'increment_account_version', [value0]),
recordCommandAudit:(value0,value1,value2,value3,value4,value5,value6)=>nativeBillingOwnerQuery(c, 'record_command_audit', [value0,value1,value2,value3,value4,value5,value6]),
recordCommandReceipt:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'record_command_receipt', [value0,value1,value2,value3]),
commit:()=>c.query<Record<string,unknown>>("COMMIT",[]),
rollback:()=>c.query<Record<string,unknown>>("ROLLBACK",[]),release:()=>c.release(),calculateTarget:(...args)=>calculateTargetBillingPeriod(c,...args),calculateRollout:(...args)=>calculateTargetRolloutReadiness(c,...args)}}


async function nativeBillingView(
  db: PostgresDatabase,
  holdingId: string,
  ): Promise<BillingViewFacts> {
  const query = async (table: string, order = "") =>
    (
      await db.pool.query(
        `SELECT * FROM ${table} WHERE holding_id=$1 ${order}`,
        [holdingId],
      )
    ).rows;
  return {
        account: (await query("billing_accounts"))[0] ?? null,
    enrollment:
      (
        await query("billing_enrollments", "ORDER BY started_at DESC LIMIT 1")
      )[0] ?? null,
    plans: (
      await db.pool.query(
        "SELECT * FROM billing_plans WHERE id IN ('infrastructure','standard','priority') ORDER BY CASE id WHEN 'infrastructure' THEN 0 WHEN 'standard' THEN 1 ELSE 2 END,revision DESC",
      )
    ).rows,
    periods: await query("billing_periods", "ORDER BY month DESC"),
    costs: await query("billing_costs", "ORDER BY created_at DESC"),
    invoices: await query("billing_invoices", "ORDER BY created_at DESC"),
    audit: await query("billing_audit", "ORDER BY created_at DESC LIMIT 100"),
    jobs: [
      ...(await query("billing_jobs", "AND state<>'done' ORDER BY created_at")),
      ...(
        await db.pool.query(
          "SELECT i.id,'attention' AS state,i.last_error FROM stripe_inbox i JOIN billing_accounts a ON a.customer_id=i.payload_json::jsonb #>> '{data,object,customer}' WHERE a.holding_id=$1 AND i.state='pending' AND i.last_error IS NOT NULL ORDER BY i.created_at LIMIT 50",
          [holdingId],
        )
      ).rows,
    ],
    legacy: (await query("beta_billing"))[0] ?? null,
  };
}
function nativeBilling(db?:PostgresDatabase){return createBillingService({error:(...args)=>new ApiError(...args),hash:value=>createHash('sha256').update(value).digest('hex'),id:randomUUID,now:Date.now,session:async()=>nativeBillingRepository(await db!.pool.connect()),view:holdingId=>nativeBillingView(db!,holdingId)})}
export const advanceBilling=(c:PoolClient,holdingId:string,now:number)=>nativeBilling().advanceBilling(nativeBillingRepository(c),holdingId,now);
export const billingView=(db:PostgresDatabase,holdingId:string,now=Date.now())=>nativeBilling(db).billingView(holdingId,now);
export const billingCommand=(db:PostgresDatabase,actor:BillingActor,holdingId:string,input:BillingInput,key:string,stripe:BillingStripe,returnUrl:string,now=Date.now())=>nativeBilling(db).billingCommand(actor,holdingId,input,key,stripe,returnUrl,now);
