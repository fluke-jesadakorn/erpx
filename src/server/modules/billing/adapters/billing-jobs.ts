import "server-only";
import type { PoolClient } from "pg";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";

import { advanceBilling } from "@/server/modules/billing/adapters/billing";
import {billingStripe,type BillingStripe} from "@/server/modules/billing/adapters/billing-stripe";
import type { BetaEnvironment } from "@/server/modules/manage_erpx/public/beta-config";
import { applyLegacyStripeEvent } from "@/server/modules/billing/adapters/beta-billing";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";

import { syncBillingTestFinance } from "@/server/modules/commerce/public/billing-test-finance";
import { ApiError } from "@/server/modules/runtime/public/http";
export type BillingJobEnv = BetaEnvironment & {
  CONTROL_DB: SqlDatabase;
  BETTER_AUTH_URL: string;
  ERPX_BILLING_PROMPTPAY_SANDBOX?: string;
};
import {createBillingJobs,type BillingJobRepository,type BillingInboxRow} from '../application/jobs';
import type {BillingAccount,BillingInvoiceRow,BillingJobRow} from '../domain/billing';
import {billingTestFinanceReady} from '@/server/modules/commerce/public/billing-test-finance-ready';
function nativeJobRepository(c:PoolClient):BillingJobRepository{return {invoice:(v0)=>c.query<BillingInvoiceRow>("SELECT * FROM billing_invoices WHERE id=$1",[v0]),
mirrorInvoiceState:(v0,v1,v2,v3,v4,v5,v6)=>c.query<Record<string,unknown>>("UPDATE billing_invoices SET status=$2,hosted_url=$3,pdf_url=$4,due_at=$5,payment_state=$6,updated_at=$7 WHERE id=$1",[v0,v1,v2,v3,v4,v5,v6]),
releaseVoidedCosts:(v0)=>c.query<Record<string,unknown>>("UPDATE billing_costs SET invoice_id=NULL WHERE invoice_id=$1",[v0]),
markVoidedPeriodIncomplete:(v0)=>c.query<Record<string,unknown>>("UPDATE billing_periods SET costs_complete=FALSE WHERE id=$1",[v0]),
retainTestFinanceProjection:(v0,v1,v2,v3,v4,v5,v6,v7,v8,v9,v10)=>c.query<Record<string,unknown>>(`INSERT INTO billing_test_finance_projection
      (billing_invoice_id,stripe_invoice_id,customer_holding_id,company_id,invoice_satang,
       receivable_satang,received_satang,reversed_satang,credited_satang,stripe_status,synced_at)
      VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
      ON CONFLICT(billing_invoice_id) DO UPDATE SET
        receivable_satang=EXCLUDED.receivable_satang,
        received_satang=EXCLUDED.received_satang,
        reversed_satang=EXCLUDED.reversed_satang,
        credited_satang=EXCLUDED.credited_satang,
        stripe_status=EXCLUDED.stripe_status,synced_at=EXCLUDED.synced_at`,[v0,v1,v2,v3,v4,v5,v6,v7,v8,v9,v10]),
jobInvoice:(v0)=>c.query<BillingInvoiceRow>("SELECT * FROM billing_invoices WHERE id=$1",[v0]),
jobAccount:(v0)=>c.query<BillingAccount>("SELECT * FROM billing_accounts WHERE holding_id=$1",[v0]),
retainJobProgress:(v0,v1)=>c.query<Record<string,unknown>>("UPDATE billing_jobs SET progress_json=$2 WHERE id=$1",[v0,v1]),
retainStripeInvoice:(v0,v1)=>c.query<Record<string,unknown>>("UPDATE billing_invoices SET stripe_id=$2,status='draft' WHERE id=$1",[v0,v1]),
retainInvoiceMode:(v0,v1)=>c.query<Record<string,unknown>>("UPDATE billing_invoices SET payment_mode=$2 WHERE id=$1",[v0,v1]),
completeJob:(v0)=>c.query<Record<string,unknown>>("UPDATE billing_jobs SET state='done',last_error=NULL WHERE id=$1",[v0]),
tryHoldingLock:(v0)=>c.query<{locked:boolean}>("SELECT pg_try_advisory_lock(hashtextextended($1,0)) AS locked",[v0]),
begin:()=>c.query<Record<string,unknown>>("BEGIN",[]),
commit:()=>c.query<Record<string,unknown>>("COMMIT",[]),
currentAccount:(v0)=>c.query<BillingAccount>("SELECT * FROM billing_accounts WHERE holding_id=$1",[v0]),
retainCurrentCard:(v0,v1)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET card_json=$2 WHERE holding_id=$1",[v0,v1]),
pendingJobs:(v0,v1)=>c.query<BillingJobRow>("SELECT * FROM billing_jobs WHERE holding_id=$1 AND state='pending' AND next_at<=$2 ORDER BY created_at,id LIMIT 10",[v0,v1]),
rescheduleJob:(v0,v1,v2,v3)=>c.query<Record<string,unknown>>("UPDATE billing_jobs SET attempts=attempts+1,state=$2,next_at=$3,last_error=$4 WHERE id=$1",[v0,v1,v2,v3]),
pendingEvents:(v0,v1)=>c.query<BillingInboxRow>("SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND payload_json::jsonb #>> '{data,object,customer}'=(SELECT customer_id FROM billing_accounts WHERE holding_id=$2) ORDER BY created_at,id LIMIT 50",[v0,v1]),
eventInvoices:(v0,v1)=>c.query<{id:string}>("SELECT id FROM billing_invoices WHERE holding_id=$1 AND stripe_id=$2",[v0,v1]),
setupCustomer:(v0)=>c.query<{customer_id:string|null}>("SELECT customer_id FROM billing_accounts WHERE holding_id=$1",[v0]),
retainSetupState:(v0,v1)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET setup_state=$2 WHERE holding_id=$1",[v0,v1]),
verifiedSetupCustomer:(v0)=>c.query<{customer_id:string|null}>("SELECT customer_id FROM billing_accounts WHERE holding_id=$1",[v0]),
retainSetupCard:(v0,v1)=>c.query<Record<string,unknown>>("UPDATE billing_accounts SET card_json=$2 WHERE holding_id=$1",[v0,v1]),
applyEvent:(v0,v1)=>c.query<Record<string,unknown>>("UPDATE stripe_inbox SET state='applied',applied_at=$2,last_error=NULL WHERE id=$1",[v0,v1]),
rescheduleEvent:(v0,v1,v2)=>c.query<Record<string,unknown>>("UPDATE stripe_inbox SET attempts=attempts+1,next_at=$2,last_error=$3 WHERE id=$1",[v0,v1,v2]),
staleInvoices:(v0,v1)=>c.query<{id:string}>("SELECT id FROM billing_invoices WHERE holding_id=$1 AND status IN ('open','draft') AND stripe_id IS NOT NULL AND updated_at<$2 LIMIT 10",[v0,v1]),
paidTargetInvoices:(v0,v1)=>c.query<{id:string}>(`SELECT i.id FROM billing_invoices i
         LEFT JOIN billing_test_finance_projection f ON f.billing_invoice_id=i.id
         WHERE i.holding_id=$1 AND i.rating_model='target' AND i.status='paid'
           AND (f.synced_at IS NULL OR f.synced_at<$2)
         ORDER BY i.updated_at DESC LIMIT 10`,[v0,v1]),
rollback:()=>c.query<Record<string,unknown>>("ROLLBACK",[]),
unlockHolding:(v0)=>c.query<Record<string,unknown>>("SELECT pg_advisory_unlock(hashtextextended($1,0))",[v0]),release:()=>c.release(),advance:(id,now)=>advanceBilling(c,id,now)}}
function nativeBillingJobs(env?:BillingJobEnv,stripe:BillingStripe=env?billingStripe(env):async()=>{throw new Error('BILLING_PROVIDER_REQUIRED')}){const db=env?.CONTROL_DB as PostgresDatabase|undefined;return createBillingJobs({accountHoldings:()=>db!.pool.query<{holding_id:string}>("SELECT holding_id FROM billing_accounts",[]),
legacyEvents:(v0)=>db!.pool.query<BillingInboxRow>("SELECT * FROM stripe_inbox WHERE state='pending' AND next_at<=$1 AND NOT EXISTS(SELECT 1 FROM billing_accounts a WHERE a.customer_id=stripe_inbox.payload_json::jsonb #>> '{data,object,customer}') LIMIT 20",[v0]),now:Date.now,stripe,promptpay:(env?.ERPX_BILLING_PROMPTPAY_SANDBOX??process.env.ERPX_BILLING_PROMPTPAY_SANDBOX)==='true',session:async()=>nativeJobRepository(await db!.pool.connect()),errorCode:error=>error instanceof ApiError?error.code:error instanceof Error?error.message:'STRIPE_UNAVAILABLE',financeReady:()=>billingTestFinanceReady(env!),syncFinance:(local,s,notes,now)=>syncBillingTestFinance(env!,local,s,notes,now),applyLegacyEvent:event=>applyLegacyStripeEvent(env!,event),retryLegacy:(id,nextAt,code)=>db!.prepare('UPDATE stripe_inbox SET attempts=attempts+1,next_at=?,last_error=? WHERE id=?').bind(nextAt,code,id).run()})}
export const mirrorInvoice=(c:PoolClient,invoiceId:string,stripe:BillingStripe,now:number,env?:BillingJobEnv)=>nativeBillingJobs(env,stripe).mirrorInvoice(nativeJobRepository(c),invoiceId,stripe,now,Boolean(env));
export const drainBilling=(env:BillingJobEnv,now=Date.now(),stripe:BillingStripe=billingStripe(env))=>nativeBillingJobs(env,stripe).drainBilling(now,stripe);
