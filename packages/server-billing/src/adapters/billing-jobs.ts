import "server-only";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";

import { advanceBilling } from "@erpx/server-billing/adapters/billing";
import {billingStripe,type BillingStripe} from "@erpx/server-billing/adapters/billing-stripe";
import type { BetaEnvironment } from "@erpx/server-manage_erpx/public/beta-config";
import { applyLegacyStripeEvent } from "@erpx/server-billing/adapters/beta-billing";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import { syncBillingTestFinance } from "@erpx/server-commerce/public/billing-test-finance";
import { ApiError } from "@erpx/server-runtime/public/http";
export type BillingJobEnv = BetaEnvironment & {
  CONTROL_DB: SqlDatabase;
  BETTER_AUTH_URL: string;
  ERPX_BILLING_PROMPTPAY_SANDBOX?: string;
};
import {createBillingJobs,type BillingJobRepository,type BillingInboxRow} from '@erpx/server-billing/application/jobs';
import {nativeBillingOwnerQuery,nativeBillingOwnerBind} from '@erpx/server-billing/adapters/native-billing-owner-write';
import {BILLING_INVOICE_BY_ID_SQL} from './billing-read-tail-r15-sql';
import {billingPointReadSql} from './billing-read-tail-r29-sql';
import {billingPendingInboxSql} from './billing-read-tail-r23-sql';
import {BILLING_ADVISORY_UNLOCK_SQL} from './billing-read-tail-r25-sql';
import {billingAccountProjectionSql} from './billing-read-tail-r30-sql';
import type {BillingAccount,BillingInvoiceRow,BillingJobRow} from '@erpx/server-billing/domain/billing';
import {billingTestFinanceReady} from '@erpx/server-commerce/public/billing-test-finance-ready';
/** r17 read tail: shared invoice-id read prefix; each caller appends its own predicate. */
function billingInvoiceIdSql(extra: string) {
  return `SELECT id FROM billing_invoices WHERE holding_id=$1${extra}`;
}
function nativeJobRepository(c:PoolClient):BillingJobRepository{return {invoice:(v0)=>c.query<BillingInvoiceRow>(BILLING_INVOICE_BY_ID_SQL,[v0]),
mirrorInvoiceState:(v0,v1,v2,v3,v4,v5,v6)=>nativeBillingOwnerQuery(c, 'mirror_invoice_state', [v0,v1,v2,v3,v4,v5,v6]),
releaseVoidedCosts:(v0)=>nativeBillingOwnerQuery(c, 'release_voided_costs', [v0]),
markVoidedPeriodIncomplete:(v0)=>nativeBillingOwnerQuery(c, 'mark_voided_period_incomplete', [v0]),
retainTestFinanceProjection:(v0,v1,v2,v3,v4,v5,v6,v7,v8,v9,v10)=>nativeBillingOwnerQuery(c, 'retain_test_finance_projection', [v0,v1,v2,v3,v4,v5,v6,v7,v8,v9,v10]),
jobInvoice:(v0)=>c.query<BillingInvoiceRow>(BILLING_INVOICE_BY_ID_SQL,[v0]),
jobAccount:(v0)=>c.query<BillingAccount>(billingPointReadSql('*','billing_accounts','holding_id=$1'),[v0]),
retainJobProgress:(v0,v1)=>nativeBillingOwnerQuery(c, 'retain_job_progress', [v0,v1]),
retainStripeInvoice:(v0,v1)=>nativeBillingOwnerQuery(c, 'retain_stripe_invoice', [v0,v1]),
retainInvoiceMode:(v0,v1)=>nativeBillingOwnerQuery(c, 'retain_invoice_mode', [v0,v1]),
completeJob:(v0)=>nativeBillingOwnerQuery(c, 'complete_job', [v0]),
tryHoldingLock:(v0)=>c.query<{locked:boolean}>("SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked",[v0]),
begin:()=>c.query<Record<string,unknown>>("BEGIN",[]),
commit:()=>c.query<Record<string,unknown>>("COMMIT",[]),
currentAccount:(v0)=>c.query<BillingAccount>(billingPointReadSql('*','billing_accounts','holding_id=$1'),[v0]),
retainCurrentCard:(v0,v1)=>nativeBillingOwnerQuery(c, 'retain_current_card', [v0,v1]),
pendingJobs:(v0,v1)=>c.query<BillingJobRow>("SELECT * FROM billing_jobs WHERE holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10",[v0,v1]),
rescheduleJob:(v0,v1,v2,v3)=>nativeBillingOwnerQuery(c, 'reschedule_job', [v0,v1,v2,v3]),
pendingEvents:(v0,v1)=>c.query<BillingInboxRow>(billingPendingInboxSql("payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$2) ORDER BY created_at,id LIMIT 50"),[v0,v1]),
eventInvoices:(v0,v1)=>c.query<{id:string}>(billingInvoiceIdSql(" AND stripe_id=$2"),[v0,v1]),
setupCustomer:(v0)=>c.query<{customer_id:string|null}>(billingPointReadSql('customer_id','billing_accounts','holding_id=$1'),[v0]),
retainSetupState:(v0,v1)=>nativeBillingOwnerQuery(c, 'retain_setup_state', [v0,v1]),
verifiedSetupCustomer:(v0)=>c.query<{customer_id:string|null}>(billingPointReadSql('customer_id','billing_accounts','holding_id=$1'),[v0]),
retainSetupCard:(v0,v1)=>nativeBillingOwnerQuery(c, 'retain_setup_card', [v0,v1]),
applyEvent:(v0,v1)=>nativeBillingOwnerQuery(c, 'apply_stripe_event', [v0,v1]),
rescheduleEvent:(v0,v1,v2)=>nativeBillingOwnerQuery(c, 'reschedule_stripe_event', [v0,v1,v2]),
staleInvoices:(v0,v1)=>c.query<{id:string}>(billingInvoiceIdSql(" AND status IN ('open','draft') AND stripe_id IS NOT NULL AND updated_at<$2 LIMIT 10"),[v0,v1]),
paidTargetInvoices:(v0,v1)=>c.query<{id:string}>(`SELECT i.id FROM billing_invoices i
         LEFT JOIN billing_test_finance_projection f ON f.billing_invoice_id=i.id
         WHERE i.holding_id=$1 AND i.rating_model='target' AND i.status='paid'
           AND (f.synced_at IS NULL OR f.synced_at<$2)
         ORDER BY i.updated_at DESC LIMIT 10`,[v0,v1]),
rollback:()=>c.query<Record<string,unknown>>("ROLLBACK",[]),
unlockHolding:(v0)=>c.query<Record<string,unknown>>(BILLING_ADVISORY_UNLOCK_SQL,[v0]),release:()=>c.release(),advance:(id,now)=>advanceBilling(c,id,now)}}
function nativeBillingJobs(env?:BillingJobEnv,stripe:BillingStripe=env?billingStripe(env):async()=>{throw new Error('BILLING_PROVIDER_REQUIRED')}){const db=env?.CONTROL_DB as PostgresDatabase|undefined;return createBillingJobs({accountHoldings:()=>db!.pool.query<{holding_id:string}>(billingAccountProjectionSql('holding_id'),[]),
legacyEvents:(v0)=>db!.pool.query<BillingInboxRow>(billingPendingInboxSql("NOT EXISTS(SELECT 1 FROM billing_accounts a WHERE a.customer_id=stripe_inbox.payload_json::jsonb #>> '{data,object,customer}') LIMIT 20"),[v0]),now:Date.now,stripe,promptpay:(env?.ERPX_BILLING_PROMPTPAY_SANDBOX??process.env.ERPX_BILLING_PROMPTPAY_SANDBOX)==='true',session:async()=>nativeJobRepository(await db!.pool.connect()),errorCode:error=>error instanceof ApiError?error.code:error instanceof Error?error.message:'STRIPE_UNAVAILABLE',financeReady:()=>billingTestFinanceReady(env!),syncFinance:(local,s,notes,now)=>syncBillingTestFinance(env!,local,s,notes,now),applyLegacyEvent:event=>applyLegacyStripeEvent(env!,event),retryLegacy:async(id,nextAt,code)=>nativeBillingOwnerBind(db!,'jobs_retry_legacy_event',[nextAt,code,id])})}
export const mirrorInvoice=(c:PoolClient,invoiceId:string,stripe:BillingStripe,now:number,env?:BillingJobEnv)=>nativeBillingJobs(env,stripe).mirrorInvoice(nativeJobRepository(c),invoiceId,stripe,now,Boolean(env));
export const drainBilling=(env:BillingJobEnv,now=Date.now(),stripe:BillingStripe=billingStripe(env))=>nativeBillingJobs(env,stripe).drainBilling(now,stripe);
