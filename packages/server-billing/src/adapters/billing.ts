import {nativeCommercialOwnerRead,COMMERCIAL_RD_USAGE_METER,COMMERCIAL_RD_USAGE_SNAPSHOT} from '@erpx/server-commercial/adapters/native-owner-read';
import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import {nativeBillingOwnerQuery} from '@erpx/server-billing/adapters/native-billing-owner-write';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {ApiError} from '@erpx/server-runtime/public/http';
import {billingHoldingFacts} from '@erpx/server-runtime/public/billing-holding-facts';
import {createBillingService,type BillingRepository} from '@erpx/server-billing/application/billing';
import type {BillingActor,BillingInput,BillingReceipt,BillingAccount,BillingEnrollment,BillingPeriod,BillingPlan,BillingInvoiceRow,BillingJobRow,BillingMeterRow} from '@erpx/server-billing/domain/billing';
import type {BillingStripe} from '@erpx/server-billing/domain/stripe';
import {calculateTargetBillingPeriod} from '@erpx/server-billing/adapters/billing-target';
import {calculateTargetRolloutReadiness} from '@erpx/server-billing/adapters/billing-target-rollout';
import {BILLING_ROLLOUT_ELIGIBILITY_SQL,BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL,BILLING_MONTH_INVOICE_SQL} from './billing-read-tail-r16-sql';
import {billingProviderCostsSql} from './billing-read-tail-r19-sql';
import {BILLING_ADVISORY_XACT_LOCK_SQL} from './billing-read-tail-r25-sql';
import {billingPointReadSql} from './billing-read-tail-r29-sql';
export {billingInput} from '@erpx/server-billing/domain/billing';
export type {BillingActor,BillingInput} from '@erpx/server-billing/domain/billing';
/** r17 read tail: one SQL text per alias-parameterized read family, routed through
 * the r29 point-read head. Call sites keep their own binds and result types. */
function billingEffectiveMonthGuardSql(table: string) {
  return billingPointReadSql('1', table, 'holding_id=$1 AND effective_month<=$2');
}
function billingPlanByRevisionSql(extra = "") {
  return billingPointReadSql('*', 'billing_plans', `id=$1${extra} ORDER BY revision DESC LIMIT 1`);
}
function billingPeriodByEnrollmentSql(extra = "") {
  return billingPointReadSql('*', 'billing_periods', `enrollment_id=$1${extra}`);
}
export function nativeBillingRepository(c:PoolClient):BillingRepository{return {
recordRatingRun:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11)=>nativeBillingOwnerQuery(c, 'record_rating_run', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11]),
retainRatedPeriod:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12)=>nativeBillingOwnerQuery(c, 'retain_rated_period', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12]),
activeEnrollmentForAdvance:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE",[value0]),
latestEnrollmentPeriod:(value0)=>c.query<BillingPeriod>(billingPeriodByEnrollmentSql(" ORDER BY month DESC LIMIT 1"),[value0]),
recordScheduledChange:(value0,value1,value2,value3,value4,value5,value6)=>nativeBillingOwnerQuery(c, 'record_scheduled_change', [value0,value1,value2,value3,value4,value5,value6]),
endEnrollment:(value0)=>nativeBillingOwnerQuery(c, 'end_enrollment', [value0]),
activatePendingPlan:(value0,value1)=>nativeBillingOwnerQuery(c, 'activate_pending_plan', [value0,value1]),
effectivePlan:(value0,value1)=>c.query<BillingPlan>(billingPlanByRevisionSql(" AND effective_month<=$2"),[value0,value1]),
createBillingPeriod:(value0,value1,value2,value3,value4,value5,value6,value7,value8)=>nativeBillingOwnerQuery(c, 'create_billing_period', [value0,value1,value2,value3,value4,value5,value6,value7,value8]),
closeElapsedPeriods:(value0,value1)=>nativeBillingOwnerQuery(c, 'close_elapsed_periods', [value0,value1]),
begin:()=>c.query<Record<string,unknown>>("BEGIN",[]),
lockHolding:(value0)=>c.query<Record<string,unknown>>(BILLING_ADVISORY_XACT_LOCK_SQL,[value0]),
lockCommand:(value0)=>c.query<Record<string,unknown>>(BILLING_ADVISORY_XACT_LOCK_SQL,[value0]),
receipt:(value0)=>c.query<BillingReceipt>(billingPointReadSql('*','billing_receipts','id=$1'),[value0]),
commitReplay:()=>c.query<Record<string,unknown>>("COMMIT",[]),
holdingExists:value0=>billingHoldingFacts(c).holdingExists(value0),
ensureAccount:(value0,value1)=>nativeBillingOwnerQuery(c, 'ensure_account', [value0,value1]),
lockedAccount:(value0)=>c.query<BillingAccount>("SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE",[value0]),
activeEnrollment:(value0)=>c.query<BillingEnrollment>(billingPointReadSql('*','billing_enrollments','holding_id=$1 AND ended_at IS NULL'),[value0]),
retainCustomer:(value0,value1)=>nativeBillingOwnerQuery(c, 'retain_customer', [value0,value1]),
enroll:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'enroll', [value0,value1,value2,value3]),
scheduleEnrollmentChange:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'schedule_enrollment_change', [value0,value1,value2,value3]),
retainConsentedCard:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'retain_consented_card', [value0,value1,value2,value3]),
automaticOpenInvoices:(value0)=>c.query<BillingInvoiceRow>(billingPointReadSql('*','billing_invoices',"holding_id=$1 AND status='open' AND payment_mode='auto'"),[value0]),
queueManualInvoice:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'queue_manual_invoice', [value0,value1,value2,value3]),
withdrawConsent:(value0)=>nativeBillingOwnerQuery(c, 'withdraw_consent', [value0]),
setPaymentMode:(value0,value1)=>nativeBillingOwnerQuery(c, 'set_payment_mode', [value0,value1]),
lockPlanRevision:()=>c.query<Record<string,unknown>>("SELECT pg_advisory_xact_lock(hashtextextended('billing-plans',0))",[]),
latestPlanRevision:(value0)=>c.query<BillingPlan>(billingPlanByRevisionSql(),[value0]),
insertPlanRevision:(value0,value1,value2,value3,value4,value5)=>nativeBillingOwnerQuery(c, 'insert_plan_revision', [value0,value1,value2,value3,value4,value5]),
selectedTargetPrice:(value0,value1)=>nativeBillingOwnerQuery(c, 'selected_target_price', [value0,value1]),
currentTargetPrice:(value0)=>nativeBillingOwnerQuery(c, 'current_target_price', [value0]),
sandboxHolding:(value0)=>c.query<Record<string,unknown>>(billingPointReadSql('1','billing_target_sandbox_holdings','holding_id=$1'),[value0]),
approvedRollout:(value0,value1)=>c.query<Record<string,unknown>>(billingEffectiveMonthGuardSql("billing_target_rollout_approvals"),[value0,value1]),
assignTargetPrice:(value0,value1,value2,value3,value4,value5,value6,value7)=>nativeBillingOwnerQuery(c, 'assign_target_price', [value0,value1,value2,value3,value4,value5,value6,value7]),
successfulPlatformMeter:async(value0,value1,value2)=>c.query<BillingMeterRow>(`SELECT * FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_METER)}) billing_usage_meters WHERE holding_id=$1 AND month=$2 AND source_key=$3
           AND meter_kind='ai' AND funding_source='erpx' AND request_status='succeeded'`,[value0,value1,value2]),
availableProviderEvidence:(value0,value1)=>nativeBillingOwnerQuery(c, 'available_evidence', [value0,value1]),
approvedProviderPeriod:(value0,value1)=>nativeBillingOwnerQuery(c, 'APPROVED_RATED_PERIOD', [value0,value1]),
existingProviderCost:(value0,value1,value2)=>c.query<Record<string,unknown>>(billingProviderCostsSql(" AND source_key=$3"),[value0,value1,value2]),
recordProviderCost:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12)=>nativeBillingOwnerQuery(c, 'record_provider_cost', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12]),
availableStatementEvidence:(value0,value1)=>nativeBillingOwnerQuery(c, 'available_evidence', [value0,value1]),
approvedStatementPeriod:(value0,value1)=>nativeBillingOwnerQuery(c, 'APPROVED_RATED_PERIOD', [value0,value1]),
providerStatementTotal:(value0,value1,value2,value3,value4)=>c.query<{count:number;total:string}>(billingPointReadSql('COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total\n          ','billing_target_provider_costs','holding_id=$1 AND month=$2 AND provider=$3\n             AND statement_id=$4 AND original_currency=$5'),[value0,value1,value2,value3,value4]),
recordProviderStatement:(value0,value1,value2,value3,value4,value5,value6,value7,value8)=>nativeBillingOwnerQuery(c, 'record_provider_statement', [value0,value1,value2,value3,value4,value5,value6,value7,value8]),
availableOperatingEvidence:(value0,value1)=>nativeBillingOwnerQuery(c, 'available_evidence', [value0,value1]),
recordOperatingCost:(value0,value1,value2,value3,value4,value5,value6)=>nativeBillingOwnerQuery(c, 'record_operating_cost', [value0,value1,value2,value3,value4,value5,value6]),
availableCapacityEvidence:(value0,value1)=>nativeBillingOwnerQuery(c, 'available_evidence', [value0,value1]),
recordCapacityReview:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10)=>nativeBillingOwnerQuery(c, 'record_capacity_review', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10]),
recordRolloutApproval:(value0,value1,value2,value3,value4,value5,value6,value7)=>nativeBillingOwnerQuery(c, 'record_rollout_approval', [value0,value1,value2,value3,value4,value5,value6,value7]),
extraTargetAssignment:(value0,value1)=>c.query<Record<string,unknown>>(billingEffectiveMonthGuardSql("billing_target_assignments"),[value0,value1]),
approvedExtraPeriod:(value0,value1)=>nativeBillingOwnerQuery(c, 'APPROVED_RATED_PERIOD', [value0,value1]),
availableExtraEvidence:(value0,value1)=>nativeBillingOwnerQuery(c, 'available_evidence', [value0,value1]),
recordTargetExtra:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11)=>nativeBillingOwnerQuery(c, 'record_target_extra', [value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11]),
lockTargetSnapshot:async(value0,value1)=>c.query<Record<string,unknown>>(`SELECT snapshot_id FROM (${await nativeCommercialOwnerRead(COMMERCIAL_RD_USAGE_SNAPSHOT)}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2 FOR UPDATE`,[value0,value1]),
targetRolloutEligible:(value0,value1)=>c.query<Record<string,unknown>>(BILLING_ROLLOUT_ELIGIBILITY_SQL,[value0,value1]),
approvedTargetPeriod:(value0,value1)=>nativeBillingOwnerQuery(c, 'APPROVED_RATED_PERIOD', [value0,value1]),
legacyEnrollmentOverlap:(value0,value1,value2)=>c.query<BillingEnrollment>(BILLING_LEGACY_ENROLLMENT_OVERLAP_SQL,[value0,value1,value2]),
monthInvoiceExists:(value0,value1)=>c.query<BillingInvoiceRow>(BILLING_MONTH_INVOICE_SQL,[value0,value1]),
activeTargetEnrollment:(value0)=>nativeBillingOwnerQuery(c, 'active_target_enrollment', [value0]),
enrollTarget:(value0,value1,value2)=>nativeBillingOwnerQuery(c, 'enroll_target', [value0,value1,value2]),
retainedTargetEnrollment:(value0)=>nativeBillingOwnerQuery(c, 'active_target_enrollment', [value0]),
targetPeriod:(value0,value1)=>c.query<BillingPeriod>(billingPeriodByEnrollmentSql(" AND month=$2"),[value0,value1]),
insertTargetInvoice:(value0,value1,value2,value3,value4,value5,value6,value7)=>nativeBillingOwnerQuery(c, 'insert_target_invoice', [value0,value1,value2,value3,value4,value5,value6,value7]),
approveTargetRating:(value0,value1,value2,value3,value4,value5)=>nativeBillingOwnerQuery(c, 'approve_target_rating', [value0,value1,value2,value3,value4,value5]),
queueTargetIssue:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'queue_target_issue', [value0,value1,value2,value3]),
selectedJob:(value0,value1)=>c.query<BillingJobRow>(billingPointReadSql('*','billing_jobs','id=$1 AND holding_id=$2'),[value0,value1]),
retryLegacyEvent:(value0,value1,value2)=>nativeBillingOwnerQuery(c, 'retry_legacy_event', [value0,value1,value2]),
retryJob:(value0,value1)=>nativeBillingOwnerQuery(c, 'retry_job', [value0,value1]),
incrementAccountVersion:(value0)=>nativeBillingOwnerQuery(c, 'increment_account_version', [value0]),
recordCommandAudit:(value0,value1,value2,value3,value4,value5,value6)=>nativeBillingOwnerQuery(c, 'record_command_audit', [value0,value1,value2,value3,value4,value5,value6]),
recordCommandReceipt:(value0,value1,value2,value3)=>nativeBillingOwnerQuery(c, 'record_command_receipt', [value0,value1,value2,value3]),
commit:()=>c.query<Record<string,unknown>>("COMMIT",[]),
rollback:()=>c.query<Record<string,unknown>>("ROLLBACK",[]),release:()=>c.release(),calculateTarget:(...args)=>calculateTargetBillingPeriod(c,...args),calculateRollout:(...args)=>calculateTargetRolloutReadiness(c,...args)}}
function nativeBilling(db?:PostgresDatabase){return createBillingService({error:(...args)=>new ApiError(...args),hash:value=>createHash('sha256').update(value).digest('hex'),id:randomUUID,now:Date.now,session:async()=>nativeBillingRepository(await db!.pool.connect()),view:async()=>{throw new ApiError(503,'NATIVE_BILLING_UNAVAILABLE','The native Billing view is authoritative.');}})}
export const advanceBilling=(c:PoolClient,holdingId:string,now:number)=>nativeBilling().advanceBilling(nativeBillingRepository(c),holdingId,now);
export const billingCommand=(db:PostgresDatabase,actor:BillingActor,holdingId:string,input:BillingInput,key:string,stripe:BillingStripe,returnUrl:string,now=Date.now())=>nativeBilling(db).billingCommand(actor,holdingId,input,key,stripe,returnUrl,now);
