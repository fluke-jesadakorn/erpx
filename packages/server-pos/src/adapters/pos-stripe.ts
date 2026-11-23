import { nativePosOne, nativePosRun } from './native-owner-write';
import { nativePosPostingJournalStatement, nativePosSaleByIdStatement } from './native-pos-core-statement';
import{secret,stripeRequest}from '@erpx/server-pos/adapters/stripe-provider';
import {posStripeBindingFacts} from "@erpx/server-runtime/public/pos-stripe-binding-facts";
import 'server-only';
import {holdingRoutingEnabled,holdingOrigin,holdingUiUrl} from '@erpx/feature-runtime/lib/holding-hosts';
import { createHmac, timingSafeEqual } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
const nativeExec = promisify(execFile);
import { type PosAccounts } from "@erpx/server-pos/public/rules/pos";
import { resourceSlots } from "@erpx/feature-runtime/lib/holding-registry";
import { runtimeEnv, holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { loadIamAuthority } from "@erpx/server-identity_access/public/iam-store";
import {id,fail,posting,audit,enqueue,receivePayment,completeSale,completeRefundPayment,releaseUnpaidReservations,type PosRow} from '@erpx/server-pos/public/pos-core';
import {one,run} from '@erpx/server-runtime/public/runtime/sql';
import { createPosStripe, type PosStripeUnit, type PosStripeRepository } from "@erpx/server-pos/application/pos-stripe";
import { PosStripeApiError, type StripeOptions } from "@erpx/server-pos/domain/stripe";
import { ApiError } from "@erpx/server-runtime/public/http";
import { reconcilePmsStripePayment, processPmsStripeRefund } from "@erpx/server-pms/public/pms-stripe";
import { reconcileBookingPayment, processBookingRefund, bookingEnv } from "@erpx/server-pms/public/pms-booking";
import { requirePosProviderPurchase, withPosProviderPurchasedTransaction } from "@erpx/server-pos/adapters/purchase-boundary";
import { adminCompanyHandoff, applicationOrigin } from "@erpx/feature-runtime/lib/application-origins";
export { PosStripeApiError };

function origin(holdingSlug?:string) {
    const value = holdingRoutingEnabled(process.env)?holdingSlug&&holdingOrigin(holdingSlug):applicationOrigin("pos", process.env.ERPX_PUBLIC_ORIGIN);
    if (!value)
        fail('POS_ORIGIN_REQUIRED', 'Configure the public origin before connecting Stripe');
    const u = new URL(value);
    if (u.protocol !== 'https:' && u.hostname !== 'localhost')
        fail('POS_HTTPS_REQUIRED', 'Stripe requires HTTPS');
    return u.origin;
}

import { posStripeIdentityAccessFacts } from "@erpx/server-identity_access/public/pos-stripe-facts";
import { posStripePmsFacts } from "@erpx/server-pms/public/pos-stripe-facts";
function unit(db: SqlDatabase): PosStripeUnit {
    return {
        writePosStripeEvents1: (facts: {
            id: string;
            account: string;
            livemode: boolean;
            company_id: string | number | boolean | null;
            type: string;
            fact5: string;
            fact6: number;
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'stripe_writeposstripeevents1_49', [facts.id, facts.account, facts.livemode, facts.company_id, facts.type, facts.fact5, facts.fact6]),
        writePosStripeEvents2: (facts: {
            fact0: number;
            id: string;
        }) => nativePosRun(db, '', 'stripe_writeposstripeevents2_53', [facts.fact0, facts.id]),
        writePosConnectionHistory1: (facts: {
            companyId: string;
            accountId: string;
            live: boolean;
        }) => nativePosRun(db, facts.companyId, 'stripe_writeposconnectionhistory1_58', [facts.companyId, facts.accountId, facts.live]),
        writePosConnections1: (facts: {
            companyId: string;
            accountId: string;
            live: boolean;
            fact3: boolean;
            fact4: boolean;
            fact5: boolean;
            fact6: boolean;
            fact7: string;
            fact8: "ready" | "onboarding";
            fact9: string | null;
            thbBankReady: boolean;
            fact11: number;
        }) => nativePosRun(db, facts.companyId, 'stripe_writeposconnections1_72', [facts.companyId, facts.accountId, facts.live, facts.fact3, facts.fact4, facts.fact5, facts.fact6, facts.fact7, facts.fact8, facts.fact9, facts.thbBankReady, facts.fact11]),
        readPosOauthStates1: (facts: {
            stateId: string;
            companyId: string;
            userId: string;
            fact3: number;
        }) => one(db, 'SELECT * FROM pos_oauth_states WHERE id=? AND company_id=? AND user_id=? AND used_at IS NULL AND expires_at>? FOR UPDATE', facts.stateId, facts.companyId, facts.userId, facts.fact3),
        writePosOauthStates1: (facts: {
            fact0: number;
            stateId: string;
        }) => nativePosRun(db, '', 'stripe_writeposoauthstates1_82', [facts.fact0, facts.stateId]),
        writePosPayments1: (facts: {
            fact0: number;
            id: string;
        }) => nativePosRun(db, '', 'stripe_writepospayments1_86', [facts.fact0, facts.id]),
        readPosPayments1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        readPosPayments2: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        writePosPayments2: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writepospayments2_96', [facts.fact0, facts.id]),
        writePosPayments3: (facts: {
            fact0: number;
            id: string;
        }) => nativePosRun(db, '', 'stripe_writepospayments3_100', [facts.fact0, facts.id]),
        readPosPayments3: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        readPosPayments4: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        writePosPayments4: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writepospayments4_110', [facts.fact0, facts.id]),
        writePosRefundPayments1: (facts: {
            id: string;
        }) => nativePosRun(db, '', 'stripe_writeposrefundpayments1_113', [facts.id]),
        readPosStripeEvents1: (facts: {
            record_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_stripe_events WHERE id=?', facts.record_id),
        readPosConnections1: (facts: {
            company_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_connections WHERE company_id=?', facts.company_id),
        writePosConnections2: (facts: {
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'stripe_writeposconnections2_124', [facts.company_id, facts.account_id, facts.livemode]),
        readPosPayments5: (facts: {
            pid: string;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?', facts.pid, facts.company_id, facts.account_id, facts.livemode),
        writePosStripeEvents3: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writeposstripeevents3_134', [facts.fact0, facts.id]),
        writePosStripeEvents4: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writeposstripeevents4_138', [facts.fact0, facts.id]),
        readPosRefundPayments1: (facts: {
            rpid: string;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
        }) => one(db, 'SELECT rp.* FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?', facts.rpid, facts.company_id, facts.account_id, facts.livemode),
        readPosPayments6: (facts: {
            payment_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=?', facts.payment_id),
        writePosStripeEvents5: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writeposstripeevents5_151', [facts.fact0, facts.id]),
        readPosSettlements1: (facts: {
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            btId: string;
        }) => one(db, "SELECT state FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='balance' AND provider_id=?", facts.company_id, facts.account_id, facts.livemode, facts.btId),
        readPosPayments7: (facts: {
            paymentId: string;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
        }) => one(db, 'SELECT id FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?', facts.paymentId, facts.company_id, facts.account_id, facts.livemode),
        readPosRefundPayments2: (facts: {
            source: string;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
        }) => one(db, 'SELECT rp.id FROM pos_refund_payments rp JOIN pos_payments p ON p.id=rp.payment_id WHERE rp.provider_id=? AND p.company_id=? AND p.stripe_account=? AND p.livemode=?', facts.source, facts.company_id, facts.account_id, facts.livemode),
        writePosSettlements1: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            btId: string;
            amount: number;
            fee: number;
            fact7: string;
            fact8: number;
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'stripe_writepossettlements1_180', [facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.btId, facts.amount, facts.fee, facts.fact7, facts.fact8]),
        writePosSettlements2: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            btId: string;
            amount: number;
            fee: number;
            fact7: string;
            journal: string | null;
            fact9: number;
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'stripe_writepossettlements2_192', [facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.btId, facts.amount, facts.fee, facts.fact7, facts.journal, facts.fact9]),
        readPosSettings1: (facts: {
            companyId: string;
        }) => one(db, 'SELECT currency,accounts_json FROM pos_settings WHERE company_id=?', facts.companyId),
        writePosSettlements3: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            id: string;
            amount: number;
            fact6: string;
            fact7: number;
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'stripe_writepossettlements3_205', [facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.id, facts.amount, facts.fact6, facts.fact7]),
        readPosSettlements2: (facts: {
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            fact2: string;
        }) => one(db, "SELECT state FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?", facts.account_id, facts.livemode, facts.fact2),
        readPosPostings1: (facts: {
            fact0: string;
        }) => nativePosPostingJournalStatement(db, facts.fact0),
        readPosSettlements3: (facts: {
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            id: string;
        }) => one(db, "SELECT journal_id FROM pos_settlements WHERE company_id=? AND account_id=? AND livemode=? AND kind='payout' AND provider_id=? AND journal_id IS NOT NULL", facts.company_id, facts.account_id, facts.livemode, facts.id),
        readPosSettlements4: (facts: {
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            id: string;
        }) => one(db, "SELECT * FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?", facts.account_id, facts.livemode, facts.id),
        writePosSettlements4: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            fact4: "payout" | "dispute";
            id: string;
            amount: number;
            fact7: string;
            fact8: string;
            journal: string | null;
            fact10: number;
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'stripe_writepossettlements4_237', [facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.fact4, facts.id, facts.amount, facts.fact7, facts.fact8, facts.journal, facts.fact10]),
        readAssignments1: facts => posStripeIdentityAccessFacts(db).readAssignments1(facts),
        readPosConnections2: (facts: {
            company_id: string | number | boolean | null;
            accountId: string | undefined;
            fact2: boolean;
        }) => one(db, "SELECT company_id FROM pos_connections WHERE company_id=? AND account_id=? AND livemode=? AND state='ready'", facts.company_id, facts.accountId, facts.fact2),
        readAssignments2: facts => posStripeIdentityAccessFacts(db).readAssignments2(facts),
        readPosConnections3: (facts: {
            company_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_connections WHERE company_id=?', facts.company_id),
        readPosConnections4: (facts: {
            company_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_connections WHERE company_id=?', facts.company_id),
        writePosJobs1: (facts: {
            fact0: string;
            id: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writeposjobs1_254', [facts.fact0, facts.id]),
        readPosPayments8: (facts: {
            record_id: string | number | boolean | null;
            company_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? AND company_id=?', facts.record_id, facts.company_id),
        readPosSales1: (facts: {
            sale_id: string | number | boolean | null;
        }) => one(db, 'SELECT number FROM pos_sales WHERE id=?', facts.sale_id),
        writePosPayments5: (facts: {
            id: string;
            qr: string;
            fact2: number;
            fact3: number;
            id2: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writepospayments5_268', [facts.id, facts.qr, facts.fact2, facts.fact3, facts.id2]),
        readPosSales2: (facts: {
            sale_id: string | number | boolean | null;
        }) => nativePosSaleByIdStatement(db, facts.sale_id),
        writePosPayments6: (facts: {
            id: string;
            url: unknown;
            fact2: number;
            id2: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writepospayments6_277', [facts.id, facts.url, facts.fact2, facts.id2]),
        readPosPayments9: (facts: {
            record_id: string | number | boolean | null;
            company_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? AND company_id=?', facts.record_id, facts.company_id),
        readPosSales3: (facts: {
            record_id: string | number | boolean | null;
            company_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=? AND company_id=? FOR UPDATE', facts.record_id, facts.company_id),
        readPosPayments10: (facts: {
            id: string | number | boolean | null;
        }) => one(db, "SELECT COALESCE(SUM(amount_minor-refunded_minor),0) n FROM pos_payments WHERE sale_id=? AND state='succeeded'", facts.id),
        readPosRefundPayments3: (facts: {
            record_id: string | number | boolean | null;
            company_id: string | number | boolean | null;
        }) => one(db, 'SELECT rp.* FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=? AND r.company_id=?', facts.record_id, facts.company_id),
        readPosPayments11: (facts: {
            payment_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=?', facts.payment_id),
        writePosRefundPayments2: (facts: {
            id: string;
            id2: string | number | boolean | null;
        }) => nativePosRun(db, '', 'stripe_writeposrefundpayments2_299', [facts.id, facts.id2]),
        readPosJobs1: (facts: {
            token: `${string}-${string}-${string}-${string}-${string}`;
            fact1: number;
            now: number;
            now2: number;
            companyId: string | null;
            companyId2: string | null;
        }) => nativePosOne(db, facts.companyId ?? '', 'stripe_readposjobs1_307', [facts.token, facts.fact1, facts.now, facts.now2, facts.companyId, facts.companyId2]),
        writePosJobs2: (facts: {
            fact0: number;
            id: string | number | boolean | null;
            token: `${string}-${string}-${string}-${string}-${string}`;
        }) => nativePosRun(db, '', 'stripe_writeposjobs2_312', [facts.fact0, facts.id, facts.token]),
        writePosJobs3: (facts: {
            fact0: "completed" | "pending";
            fact1: number;
            id: string | number | boolean | null;
            token: `${string}-${string}-${string}-${string}-${string}`;
        }) => nativePosRun(db, '', 'stripe_writeposjobs3_318', [facts.fact0, facts.fact1, facts.id, facts.token]),
        writePosJobs4: (facts: {
            fact0: "pending" | "exception";
            fact1: number;
            code: string;
            id: string | number | boolean | null;
            token: `${string}-${string}-${string}-${string}-${string}`;
        }) => nativePosRun(db, '', 'stripe_writeposjobs4_325', [facts.fact0, facts.fact1, facts.code, facts.id, facts.token]),
        readContext1: () => one(db, "SELECT to_regclass('pos_jobs') name"),
        enqueue: (arg0: string, arg1: "event" | "complete_sale" | "check_payment", arg2: string) => enqueue(db, arg0, arg1, arg2),
        audit: (arg0: string, arg1: null, arg2: string | "system:stripe", arg3: "connection.connected" | "job.exception", arg4: string, arg5: {
            livemode: boolean;
        } | {
            jobId: string | number | boolean | null;
            code: string;
        }) => audit(db, arg0, arg1, arg2, arg3, arg4, arg5),
        receivePayment: (arg0: PosRow, arg1: "system:stripe", arg2: string | undefined) => receivePayment(db, arg0, arg1, arg2),
        releaseUnpaidReservations: (arg0: string) => releaseUnpaidReservations(db, arg0),
        completeRefundPayment: (arg0: string, arg1: "system:stripe", arg2: string | undefined) => completeRefundPayment(db, arg0, arg1, arg2),
        posting: (arg0: string, arg1: null, arg2: PosAccounts, arg3: string, arg4: string | "THB", arg5: string, arg6: string, arg7: "system:stripe", arg8: {
            account: string;
            debit: number;
            credit: number;
        }[]) => posting(db, arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8),
        loadIamAuthority: (arg0: string, arg1: string, arg2: string) => loadIamAuthority(db, arg0, arg1, arg2),
        drainNativeCompleteSales: async (holdingId: string) => {
            const binary=process.env.ERPX_NATIVE_SERVER_BIN,manifest=process.env.ERPX_DATABASE_MANIFEST;
            if(!binary||!manifest)throw Object.assign(new Error('The native POS Stripe worker is not configured.'),{code:'NATIVE_POS_STRIPE_WORKER_REQUIRED'});
            try{const result=await nativeExec(binary,['pos-stripe-worker','--once','--database-manifest',manifest,'--holding',holdingId],{timeout:120_000,maxBuffer:2*1024*1024,env:process.env,windowsHide:true});console.error('NATIVE_WORKER_OUT',result.stdout,result.stderr);}
            catch(error){console.error('NATIVE_WORKER_ERR',error);throw Object.assign(new Error('The native POS Stripe worker did not complete.'),{code:'NATIVE_POS_STRIPE_WORKER_FAILED'});}
        },
        completeSale: (arg0: PosRow, arg1: "system:stripe", arg2: string | undefined) => completeSale(db, arg0, arg1, arg2),
        controlOne1: (facts: {
            arg0: string;
            arg1: boolean;
        }) => one(db, 'SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?', facts.arg0, facts.arg1),
        controlOne2: (facts: {
            arg0: string;
            arg1: boolean;
        }) => one(db, 'SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?', facts.arg0, facts.arg1),
        controlRun1: (facts: {
            arg0: string;
            arg1: boolean;
            arg2: string;
            arg3: string;
            arg4: number;
        }) => nativePosRun(db, '', 'stripe_controlrun1_364', [facts.arg0, facts.arg1, facts.arg2, facts.arg3, facts.arg4]),
        controlOne3: (facts: {
            arg0: string;
            arg1: boolean;
        }) => one(db, 'SELECT * FROM pos_stripe_routes WHERE account_id=? AND livemode=?', facts.arg0, facts.arg1),
        controlOne4: (facts: {
            arg0: string;
        }) => posStripeBindingFacts(db).slug(facts.arg0),
        reconcileCounter: paymentId => reconcilePmsStripePayment(db as PostgresDatabase, paymentId),
        reconcileBooking: async (paymentId) => reconcileBookingPayment(db as PostgresDatabase, paymentId, await bookingEnv()),
        refundCounter: paymentId => processPmsStripeRefund(db as PostgresDatabase, paymentId),
        refundBooking: paymentId => processBookingRefund(db as PostgresDatabase, paymentId),
        readPmsStripePayments1: facts => posStripePmsFacts(db).readPmsStripePayments1(facts),
        writePmsStripePayments1: facts => posStripePmsFacts(db).writePmsStripePayments1(facts),
        readPmsBookingPayments1: facts => posStripePmsFacts(db).readPmsBookingPayments1(facts),
        writePmsBookingPayments1: facts => posStripePmsFacts(db).writePmsBookingPayments1(facts),
        readPmsStripePayments2: facts => posStripePmsFacts(db).readPmsStripePayments2(facts),
        readPmsBookingPayments2: facts => posStripePmsFacts(db).readPmsBookingPayments2(facts),
        readPmsBookingPayments3: facts => posStripePmsFacts(db).readPmsBookingPayments3(facts),
        readPmsStripePayments3: facts => posStripePmsFacts(db).readPmsStripePayments3(facts),
        readPmsBookingRefundRequests1: facts => posStripePmsFacts(db).readPmsBookingRefundRequests1(facts),
        readPmsStripePayments4: facts => posStripePmsFacts(db).readPmsStripePayments4(facts),
        readPmsProperties1: facts => posStripePmsFacts(db).readPmsProperties1(facts),
        readPmsProperties2: facts => posStripePmsFacts(db).readPmsProperties2(facts)
    };
}
function repository(db: PostgresDatabase): PosStripeRepository { return { ...unit(db), requirePurchase: input => requirePosProviderPurchase(db, input),
    transaction: (work, purchase) => purchase ? withPosProviderPurchasedTransaction(db, purchase, tx => work(repository(tx as PostgresDatabase)))
        : db.transaction(tx => work(repository(tx as PostgresDatabase))) }; }
const application = createPosStripe({ clock: { now: () => Date.now() }, uuid: id, environment: key => process.env[key], error: (status, code, message) => new ApiError(status, code, message), resources: async () => { const env = await runtimeEnv(); return { control: repository(env.CONTROL_DB as PostgresDatabase), holdings: resourceSlots(env) }; }, forHolding: async (holdingId) => repository((await holdingResources(holdingId)).write as PostgresDatabase), provider: { request: stripeRequest, secret, origin, callbackUrl:slug=>`${origin(slug)}${holdingRoutingEnabled(process.env)?'/__erpx/pos':''}/api/v1/holdings/${encodeURIComponent(slug)}/pos/connect/callback`, saleReturn:(slug,companyId,saleId)=>holdingRoutingEnabled(process.env)?holdingUiUrl(`/holdings/${encodeURIComponent(slug)}/pos?sale=${encodeURIComponent(saleId)}`,companyId)!:`${origin(slug)}/holdings/${encodeURIComponent(slug)}/pos?sale=${encodeURIComponent(saleId)}`, settingsReturn: (slug, companyId) => adminCompanyHandoff(companyId, `/holdings/${encodeURIComponent(slug)}/settings/stripe`, process.env.ERPX_PUBLIC_ORIGIN), fetch: (url, { timeoutMs, ...init }) => fetch(url, { ...init, signal: AbortSignal.timeout(timeoutMs) }) }, signing: { equal: (secret, payload, signature) => timingSafeEqual(createHmac("sha256", secret).update(payload).digest(), Buffer.from(signature, "hex")) }, leases: { repeat: setInterval, cancel: token => clearInterval(token as ReturnType<typeof setInterval>) }, report: value => console.error(value) });
export function stripeCall(live: boolean, path: string, options: StripeOptions = {}) { return application.stripeCall(live, path, options); }
export function stripeListCall(live: boolean, path: string, options: StripeOptions = {}) { return application.stripeListCall(live, path, options); }
export function posStripeWebhook(raw: string, signature: string, expectedHoldingId?:string) { return application.posStripeWebhook(raw, signature,expectedHoldingId); }
export function oauthUrl(state: string, holdingSlug: string, live: boolean) { return application.oauthUrl(state, holdingSlug, live); }
export function finishPosOAuth(actor: WorkContext, stateId: string, code: string) { return application.finishPosOAuth(actor, stateId, code); }
export function drainPos(db: PostgresDatabase, holdingId: string, limit = 8, companyId: string | null = null) { return application.drainPos(repository(db), holdingId, limit, companyId); }
export function drainAllPos() { return application.drainAllPos(); }
export function verifyPosStripeSignature(raw: string, signature: string, signingSecrets: string[], now = Date.now()) { return application.verifyPosStripeSignature(raw, signature, signingSecrets, now); }
