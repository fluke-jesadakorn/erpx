import { lastAuditOwnerProjectionSources as commercialLastAuditProjectionSources } from "@erpx/server-commercial/public/last-audit-owner-projection-sources";
import 'server-only';
import {createHash,randomUUID} from 'node:crypto';
import type {PoolClient} from 'pg';
import type {PostgresDatabase} from '@erpx/server-runtime/public/runtime/postgres';
import {ApiError} from '@erpx/server-runtime/public/http';
import {billingHoldingFacts} from '@erpx/server-runtime/public/billing-holding-facts';
import {createBillingService,type BillingRepository} from '@erpx/server-billing/application/billing';
import type {BillingActor,BillingInput,BillingReceipt,BillingAccount,BillingEnrollment,BillingPeriod,BillingPlan,BillingInvoiceRow,BillingCost,BillingJobRow,BillingPriceRow,BillingMeterRow,BillingViewFacts} from '@erpx/server-billing/domain/billing';
import type {BillingStripe} from '@erpx/server-billing/domain/stripe';
import {calculateTargetBillingPeriod} from '@erpx/server-billing/adapters/billing-target';
import {calculateTargetRolloutReadiness} from '@erpx/server-billing/adapters/billing-target-rollout';
export {billingInput} from '@erpx/server-billing/domain/billing';
export type {BillingActor,BillingInput} from '@erpx/server-billing/domain/billing';
export function nativeBillingRepository(c:PoolClient):BillingRepository{return {recordRatingRun:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_rating_runs
     (id,holding_id,month,price_id,meter_snapshot_id,state,missing_json,
      lines_json,total_satang,actor,reason,created_at)
     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,[value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11]),
retainRatedPeriod:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_rated_periods
     (holding_id,month,price_id,assignment_month,mode,meter_snapshot_id,
      service_start,service_end,state,missing_json,lines_json,total_satang,rated_at)
     VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
     ON CONFLICT(holding_id,month) DO UPDATE SET
       price_id=excluded.price_id, assignment_month=excluded.assignment_month,
       mode=excluded.mode, meter_snapshot_id=excluded.meter_snapshot_id,
       service_start=excluded.service_start, service_end=excluded.service_end,
       state=excluded.state, missing_json=excluded.missing_json,
       lines_json=excluded.lines_json, total_satang=excluded.total_satang,
       rated_at=excluded.rated_at
     WHERE billing_target_rated_periods.state<>'approved'`,[value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12]),
activeEnrollmentForAdvance:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL FOR UPDATE",[value0]),
latestEnrollmentPeriod:(value0)=>c.query<BillingPeriod>("SELECT * FROM billing_periods WHERE enrollment_id=$1 ORDER BY month DESC LIMIT 1",[value0]),
recordScheduledChange:(value0,value1,value2,value3,value4,value5,value6)=>c.query<Record<string,unknown>>("INSERT INTO billing_audit VALUES($1,$2,$3,$4,$5,$6,$7)",[value0,value1,value2,value3,value4,value5,value6]),
endEnrollment:(value0)=>c.query<Record<string,unknown>>("UPDATE billing_enrollments SET ended_at=effective_at,pending_end=FALSE,effective_at=NULL WHERE id=$1",[value0]),
activatePendingPlan:(value0,value1)=>c.query<Record<string,unknown>>("UPDATE billing_enrollments SET plan_id=$2,pending_plan=NULL,effective_at=NULL WHERE id=$1",[value0,value1]),
effectivePlan:(value0,value1)=>c.query<BillingPlan>("SELECT * FROM billing_plans WHERE id=$1 AND effective_month<=$2 ORDER BY revision DESC LIMIT 1",[value0,value1]),
createBillingPeriod:(value0,value1,value2,value3,value4,value5,value6,value7,value8)=>c.query<Record<string,unknown>>("INSERT INTO billing_periods(id,holding_id,enrollment_id,month,start_at,end_at,plan_id,plan_revision,support_satang) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",[value0,value1,value2,value3,value4,value5,value6,value7,value8]),
closeElapsedPeriods:(value0,value1)=>c.query<Record<string,unknown>>("UPDATE billing_periods SET closed_at=$2 WHERE holding_id=$1 AND end_at<=$2 AND closed_at IS NULL",[value0,value1]),
begin:()=>c.query<Record<string,unknown>>("BEGIN",[]),
lockHolding:(value0)=>c.query<Record<string,unknown>>("SELECT pg_advisory_xact_lock(hashtextextended($1,0))",[value0]),
lockCommand:(value0)=>c.query<Record<string,unknown>>("SELECT pg_advisory_xact_lock(hashtextextended($1,0))",[value0]),
receipt:(value0)=>c.query<BillingReceipt>("SELECT * FROM billing_receipts WHERE id=$1",[value0]),
commitReplay:()=>c.query<Record<string,unknown>>("COMMIT",[]),
holdingExists:value0=>billingHoldingFacts(c).holdingExists(value0),
ensureAccount:(value0,value1)=>c.query<Record<string,unknown>>("INSERT INTO billing_accounts(holding_id,customer_id,created_at) SELECT $1,(SELECT customer_id FROM beta_billing WHERE holding_id=$1),$2 ON CONFLICT DO NOTHING",[value0,value1]),
lockedAccount:(value0)=>c.query<BillingAccount>("SELECT * FROM billing_accounts WHERE holding_id=$1 FOR UPDATE",[value0]),
activeEnrollment:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND ended_at IS NULL",[value0]),
retainCustomer:(value0,value1)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET customer_id=$2 WHERE holding_id=$1",[value0,value1]),
selectedPeriod:(value0,value1)=>c.query<BillingPeriod>("SELECT * FROM billing_periods WHERE id=$1 AND holding_id=$2",[value0,value1]),
enroll:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES($1,$2,$3,$4)",[value0,value1,value2,value3]),
scheduleEnrollmentChange:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("UPDATE billing_enrollments SET pending_plan=$2,pending_end=$3,effective_at=$4 WHERE id=$1",[value0,value1,value2,value3]),
retainConsentedCard:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET card_json=$2,consent_at=$3,consent_actor=$4 WHERE holding_id=$1",[value0,value1,value2,value3]),
automaticOpenInvoices:(value0)=>c.query<BillingInvoiceRow>("SELECT * FROM billing_invoices WHERE holding_id=$1 AND status='open' AND payment_mode='auto'",[value0]),
queueManualInvoice:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'manual',$4,$4) ON CONFLICT DO NOTHING",[value0,value1,value2,value3]),
withdrawConsent:(value0)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET consent_at=NULL,consent_actor=NULL WHERE holding_id=$1",[value0]),
setPaymentMode:(value0,value1)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET payment_mode=$2 WHERE holding_id=$1",[value0,value1]),
originalInvoiceExists:(value0,value1,value2)=>c.query<BillingInvoiceRow>("SELECT 1 FROM billing_invoices WHERE id=$1 AND holding_id=$2 AND period_id=$3",[value0,value1,value2]),
availableCostEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
existingCost:(value0,value1,value2)=>c.query<BillingCost>("SELECT * FROM billing_costs WHERE id=$1 AND holding_id=$2 AND period_id=$3",[value0,value1,value2]),
insertCost:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11)=>c.query<Record<string,unknown>>(`INSERT INTO billing_costs(id,holding_id,period_id,category,amount_satang,service_start,service_end,note,simulated,evidence_id,original_invoice_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT(id) DO UPDATE SET category=EXCLUDED.category,amount_satang=EXCLUDED.amount_satang,service_start=EXCLUDED.service_start,service_end=EXCLUDED.service_end,note=EXCLUDED.note,simulated=EXCLUDED.simulated,evidence_id=EXCLUDED.evidence_id,original_invoice_id=EXCLUDED.original_invoice_id`,[value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11]),
markCostsIncomplete:(value0)=>c.query<Record<string,unknown>>("UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1",[value0]),
deleteUnissuedCost:(value0,value1)=>c.query<BillingCost>("DELETE FROM billing_costs WHERE id=$1 AND holding_id=$2 AND invoice_id IS NULL RETURNING period_id",[value0,value1]),
markDeletedCostPeriodIncomplete:(value0)=>c.query<Record<string,unknown>>("UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1",[value0]),
setCostCompletion:(value0,value1)=>c.query<Record<string,unknown>>("UPDATE billing_periods SET costs_complete=$2 WHERE id=$1",[value0,value1]),
currentMainInvoice:(value0)=>c.query<BillingInvoiceRow>("SELECT * FROM billing_invoices WHERE period_id=$1 AND kind='main' AND status<>'void'",[value0]),
unissuedCosts:(value0)=>c.query<BillingCost>("SELECT * FROM billing_costs WHERE period_id=$1 AND invoice_id IS NULL ORDER BY created_at,id",[value0]),
retainedPlan:(value0,value1)=>c.query<BillingPlan>("SELECT * FROM billing_plans WHERE id=$1 AND revision=$2",[value0,value1]),
insertLegacyInvoice:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9)=>c.query<Record<string,unknown>>("INSERT INTO billing_invoices(id,holding_id,period_id,kind,total_satang,lines_json,payment_mode,created_at,approved_by,original_invoice_id,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$8)",[value0,value1,value2,value3,value4,value5,value6,value7,value8,value9]),
attachCostToInvoice:(value0,value1)=>c.query<Record<string,unknown>>("UPDATE billing_costs SET invoice_id=$2 WHERE id=$1",[value0,value1]),
queueLegacyIssue:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'issue',$4,$4)",[value0,value1,value2,value3]),
voidableInvoice:(value0,value1)=>c.query<BillingInvoiceRow>("SELECT * FROM billing_invoices WHERE id=$1 AND holding_id=$2 AND status IN ('open','uncollectible')",[value0,value1]),
queueVoid:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'void',$4,$4) ON CONFLICT DO NOTHING",[value0,value1,value2,value3]),
lockPlanRevision:()=>c.query<Record<string,unknown>>("SELECT pg_advisory_xact_lock(hashtextextended('billing-plans',0))",[]),
latestPlanRevision:(value0)=>c.query<BillingPlan>("SELECT * FROM billing_plans WHERE id=$1 ORDER BY revision DESC LIMIT 1",[value0]),
insertPlanRevision:(value0,value1,value2,value3,value4,value5)=>c.query<Record<string,unknown>>("INSERT INTO billing_plans VALUES($1,$2,$3,$4,$5,$6)",[value0,value1,value2,value3,value4,value5]),
selectedTargetPrice:(value0,value1)=>c.query<BillingPriceRow>("SELECT * FROM billing_target_prices WHERE id=$1 AND effective_month<=$2",[value0,value1]),
currentTargetPrice:(value0)=>c.query<BillingPriceRow>("SELECT * FROM billing_target_prices WHERE effective_month<=$1 ORDER BY effective_month DESC,id DESC LIMIT 1",[value0]),
sandboxHolding:(value0)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1",[value0]),
approvedRollout:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rollout_approvals WHERE holding_id=$1 AND effective_month<=$2",[value0,value1]),
assignTargetPrice:(value0,value1,value2,value3,value4,value5,value6,value7)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_assignments
             (holding_id,effective_month,price_id,mode,service_started_at,assigned_by,reason,created_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING holding_id`,[value0,value1,value2,value3,value4,value5,value6,value7]),
successfulPlatformMeter:(value0,value1,value2)=>c.query<BillingMeterRow>(`SELECT * FROM (${commercialLastAuditProjectionSources.billing_usage_meters}) billing_usage_meters WHERE holding_id=$1 AND month=$2 AND source_key=$3
           AND meter_kind='ai' AND funding_source='erpx' AND request_status='succeeded'`,[value0,value1,value2]),
availableProviderEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
approvedProviderPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
existingProviderCost:(value0,value1,value2)=>c.query<Record<string,unknown>>("SELECT * FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND source_key=$3",[value0,value1,value2]),
recordProviderCost:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_provider_costs
           (holding_id,month,source_key,provider,statement_id,original_currency,original_minor,
            fx_numerator,fx_denominator,provider_cost_satang,evidence_id,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)`,[value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11,value12]),
availableStatementEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
approvedStatementPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
providerStatementTotal:(value0,value1,value2,value3,value4)=>c.query<{count:number;total:string}>(`SELECT COUNT(*)::int AS count, COALESCE(SUM(original_minor),0)::text AS total
           FROM billing_target_provider_costs WHERE holding_id=$1 AND month=$2 AND provider=$3
             AND statement_id=$4 AND original_currency=$5`,[value0,value1,value2,value3,value4]),
recordProviderStatement:(value0,value1,value2,value3,value4,value5,value6,value7,value8)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_statements
           (holding_id,month,provider,statement_id,evidence_id,provider_total_minor,
            original_currency,reconciled_total_minor,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$7,$8,$9) ON CONFLICT DO NOTHING RETURNING holding_id`,[value0,value1,value2,value3,value4,value5,value6,value7,value8]),
availableOperatingEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
recordOperatingCost:(value0,value1,value2,value3,value4,value5,value6)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_operating_costs
           (holding_id,month,category,amount_satang,evidence_id,verified_by,verified_at)
           VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING RETURNING holding_id`,[value0,value1,value2,value3,value4,value5,value6]),
availableCapacityEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
recordCapacityReview:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_capacity_reviews
           (holding_id,effective_month,projected_original_bytes,primary_available_bytes,
            backup_available_bytes,reserved_bytes,company_limit_bytes,evidence_id,
            approved_by,reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
           ON CONFLICT DO NOTHING RETURNING holding_id`,[value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10]),
recordRolloutApproval:(value0,value1,value2,value3,value4,value5,value6,value7)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_rollout_approvals
           (holding_id,effective_month,shadow_month_1,shadow_month_2,
            minimum_margin_bps,approved_by,reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING holding_id`,[value0,value1,value2,value3,value4,value5,value6,value7]),
extraTargetAssignment:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_assignments WHERE holding_id=$1 AND effective_month<=$2",[value0,value1]),
approvedExtraPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
availableExtraEvidence:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_evidence WHERE id=$1 AND holding_id=$2 AND source_state='available'",[value0,value1]),
recordTargetExtra:(value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11)=>c.query<Record<string,unknown>>(`INSERT INTO billing_target_extras
           (id,holding_id,month,kind,description,amount_satang,service_start,service_end,
            evidence_id,approved_by,approval_reason,approved_at)
           VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)`,[value0,value1,value2,value3,value4,value5,value6,value7,value8,value9,value10,value11]),
lockTargetSnapshot:(value0,value1)=>c.query<Record<string,unknown>>(`SELECT snapshot_id FROM (${commercialLastAuditProjectionSources.billing_usage_snapshots}) billing_usage_snapshots WHERE holding_id=$1 AND month=$2 FOR UPDATE`,[value0,value1]),
targetRolloutEligible:(value0,value1)=>c.query<Record<string,unknown>>(`SELECT 1 FROM billing_target_sandbox_holdings WHERE holding_id=$1
           UNION ALL SELECT 1 FROM billing_target_rollout_approvals
           WHERE holding_id=$1 AND effective_month<=$2 LIMIT 1`,[value0,value1]),
approvedTargetPeriod:(value0,value1)=>c.query<Record<string,unknown>>("SELECT 1 FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",[value0,value1]),
legacyEnrollmentOverlap:(value0,value1,value2)=>c.query<BillingEnrollment>(`SELECT 1 FROM billing_enrollments WHERE holding_id=$1 AND plan_id<>'platform_usage'
           AND started_at<$3 AND COALESCE(ended_at,effective_at,9223372036854775807)>$2`,[value0,value1,value2]),
monthInvoiceExists:(value0,value1)=>c.query<BillingInvoiceRow>(`SELECT 1 FROM billing_invoices i JOIN billing_periods p ON p.id=i.period_id
           WHERE i.holding_id=$1 AND p.month=$2 AND i.status<>'void'`,[value0,value1]),
activeTargetEnrollment:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL",[value0]),
enrollTarget:(value0,value1,value2)=>c.query<Record<string,unknown>>("INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at) VALUES($1,$2,'platform_usage',$3)",[value0,value1,value2]),
retainedTargetEnrollment:(value0)=>c.query<BillingEnrollment>("SELECT * FROM billing_enrollments WHERE holding_id=$1 AND plan_id='platform_usage' AND ended_at IS NULL",[value0]),
targetPeriod:(value0,value1)=>c.query<BillingPeriod>("SELECT * FROM billing_periods WHERE enrollment_id=$1 AND month=$2",[value0,value1]),
insertTargetInvoice:(value0,value1,value2,value3,value4,value5,value6,value7)=>c.query<Record<string,unknown>>(`INSERT INTO billing_invoices
           (id,holding_id,period_id,kind,total_satang,lines_json,payment_mode,created_at,
            approved_by,original_invoice_id,updated_at,rating_model)
           VALUES($1,$2,$3,'main',$4,$5,$6,$7,$8,NULL,$7,'target')`,[value0,value1,value2,value3,value4,value5,value6,value7]),
approveTargetRating:(value0,value1,value2,value3,value4,value5)=>c.query<Record<string,unknown>>(`UPDATE billing_target_rated_periods SET state='approved', approved_by=$3,
           approved_at=$4,approval_reason=$5,invoice_id=$6
           WHERE holding_id=$1 AND month=$2 AND state='ready'`,[value0,value1,value2,value3,value4,value5]),
queueTargetIssue:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("INSERT INTO billing_jobs(id,holding_id,invoice_id,kind,next_at,created_at) VALUES($1,$2,$3,'issue',$4,$4)",[value0,value1,value2,value3]),
selectedJob:(value0,value1)=>c.query<BillingJobRow>("SELECT * FROM billing_jobs WHERE id=$1 AND holding_id=$2",[value0,value1]),
retryLegacyEvent:(value0,value1,value2)=>c.query<BillingAccount>("UPDATE stripe_inbox SET next_at=$2 WHERE id=$1 AND state='pending' AND payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$3) RETURNING id",[value0,value1,value2]),
retryJob:(value0,value1)=>c.query<Record<string,unknown>>("UPDATE billing_jobs SET state='pending',next_at=$2 WHERE id=$1",[value0,value1]),
incrementAccountVersion:(value0)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET version=version+1 WHERE holding_id=$1",[value0]),
recordCommandAudit:(value0,value1,value2,value3,value4,value5,value6)=>c.query<Record<string,unknown>>("INSERT INTO billing_audit VALUES($1,$2,$3,$4,$5,$6,$7)",[value0,value1,value2,value3,value4,value5,value6]),
recordCommandReceipt:(value0,value1,value2,value3)=>c.query<Record<string,unknown>>("INSERT INTO billing_receipts VALUES($1,$2,$3,$4)",[value0,value1,value2,value3]),
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
