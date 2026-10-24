import{secret,stripeRequest}from '@erpx/server-pos/adapters/stripe-provider';
import {posStripeBindingFacts} from "@erpx/server-runtime/public/pos-stripe-binding-facts";
import 'server-only';
import { createHmac, timingSafeEqual } from 'node:crypto';
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

function origin() {
    const value = applicationOrigin("pos", process.env.ERPX_PUBLIC_ORIGIN);
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
        }) => run(db, 'INSERT INTO pos_stripe_events(id,account_id,livemode,company_id,event_type,payload_json,received_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO NOTHING', facts.id, facts.account, facts.livemode, facts.company_id, facts.type, facts.fact5, facts.fact6),
        writePosStripeEvents2: (facts: {
            fact0: number;
            id: string;
        }) => run(db, 'UPDATE pos_stripe_events SET provider_created_at=COALESCE(provider_created_at,?) WHERE id=?', facts.fact0, facts.id),
        writePosConnectionHistory1: (facts: {
            companyId: string;
            accountId: string;
            live: boolean;
        }) => run(db, 'INSERT INTO pos_connection_history VALUES(?,?,?) ON CONFLICT DO NOTHING', facts.companyId, facts.accountId, facts.live),
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
        }) => run(db, 'INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,requirements_json,state,payout_interval,thb_bank_ready,last_checked_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET account_id=excluded.account_id,livemode=excluded.livemode,charges_enabled=excluded.charges_enabled,payouts_enabled=excluded.payouts_enabled,card_enabled=excluded.card_enabled,promptpay_enabled=excluded.promptpay_enabled,requirements_json=excluded.requirements_json,state=excluded.state,payout_interval=excluded.payout_interval,thb_bank_ready=excluded.thb_bank_ready,last_checked_at=excluded.last_checked_at,version=pos_connections.version+1', facts.companyId, facts.accountId, facts.live, facts.fact3, facts.fact4, facts.fact5, facts.fact6, facts.fact7, facts.fact8, facts.fact9, facts.thbBankReady, facts.fact11),
        readPosOauthStates1: (facts: {
            stateId: string;
            companyId: string;
            userId: string;
            fact3: number;
        }) => one(db, 'SELECT * FROM pos_oauth_states WHERE id=? AND company_id=? AND user_id=? AND used_at IS NULL AND expires_at>? FOR UPDATE', facts.stateId, facts.companyId, facts.userId, facts.fact3),
        writePosOauthStates1: (facts: {
            fact0: number;
            stateId: string;
        }) => run(db, 'UPDATE pos_oauth_states SET used_at=? WHERE id=?', facts.fact0, facts.stateId),
        writePosPayments1: (facts: {
            fact0: number;
            id: string;
        }) => run(db, 'UPDATE pos_payments SET provider_received_at=COALESCE(provider_received_at,?) WHERE id=?', facts.fact0, facts.id),
        readPosPayments1: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        readPosPayments2: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        writePosPayments2: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?", facts.fact0, facts.id),
        writePosPayments3: (facts: {
            fact0: number;
            id: string;
        }) => run(db, 'UPDATE pos_payments SET provider_received_at=COALESCE(provider_received_at,?) WHERE id=?', facts.fact0, facts.id),
        readPosPayments3: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        readPosPayments4: (facts: {
            id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? FOR UPDATE', facts.id),
        writePosPayments4: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_payments SET state='cancelled',version=version+1,updated_at=? WHERE id=?", facts.fact0, facts.id),
        writePosRefundPayments1: (facts: {
            id: string;
        }) => run(db, "UPDATE pos_refund_payments SET state='failed' WHERE id=?", facts.id),
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
        }) => run(db, "UPDATE pos_connections SET state='disconnected',charges_enabled=false,payouts_enabled=false,version=version+1 WHERE company_id=? AND account_id=? AND livemode=?", facts.company_id, facts.account_id, facts.livemode),
        readPosPayments5: (facts: {
            pid: string;
            company_id: string | number | boolean | null;
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_payments WHERE id=? AND company_id=? AND stripe_account=? AND livemode=?', facts.pid, facts.company_id, facts.account_id, facts.livemode),
        writePosStripeEvents3: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_stripe_events SET applied_at=? WHERE id=?', facts.fact0, facts.id),
        writePosStripeEvents4: (facts: {
            fact0: number;
            id: string | number | boolean | null;
        }) => run(db, 'UPDATE pos_stripe_events SET applied_at=? WHERE id=?', facts.fact0, facts.id),
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
        }) => run(db, 'UPDATE pos_stripe_events SET applied_at=? WHERE id=?', facts.fact0, facts.id),
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
        }) => run(db, "INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,created_at) VALUES(?,?,?,?,'balance',?,?,?,'THB','unmapped',?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO NOTHING", facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.btId, facts.amount, facts.fee, facts.fact7, facts.fact8),
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
        }) => run(db, "INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,journal_id,created_at) VALUES(?,?,?,?,'balance',?,?,?,'THB','recorded',?,?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO UPDATE SET state='recorded',journal_id=excluded.journal_id,details_json=excluded.details_json WHERE pos_settlements.state='unmapped'", facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.btId, facts.amount, facts.fee, facts.fact7, facts.journal, facts.fact9),
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
        }) => run(db, "INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,fee_minor,currency,state,details_json,created_at) VALUES(?,?,?,?,'payout',?, ?,0,'THB','unmapped',?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO UPDATE SET state='unmapped',details_json=excluded.details_json WHERE pos_settlements.journal_id IS NULL", facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.id, facts.amount, facts.fact6, facts.fact7),
        readPosSettlements2: (facts: {
            account_id: string | number | boolean | null;
            livemode: string | number | boolean | null;
            fact2: string;
        }) => one(db, "SELECT state FROM pos_settlements WHERE account_id=? AND livemode=? AND kind='balance' AND provider_id=?", facts.account_id, facts.livemode, facts.fact2),
        readPosPostings1: (facts: {
            fact0: string;
        }) => one(db, 'SELECT journal_id FROM pos_postings WHERE source_key=?', facts.fact0),
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
        }) => run(db, 'INSERT INTO pos_settlements(id,company_id,account_id,livemode,kind,provider_id,amount_minor,currency,state,details_json,journal_id,created_at) VALUES(?,?,?,?,?,?,?,\'THB\',?,?,?,?) ON CONFLICT(account_id,livemode,kind,provider_id) DO UPDATE SET state=excluded.state,details_json=excluded.details_json,journal_id=COALESCE(excluded.journal_id,pos_settlements.journal_id)', facts.fact0, facts.company_id, facts.account_id, facts.livemode, facts.fact4, facts.id, facts.amount, facts.fact7, facts.fact8, facts.journal, facts.fact10),
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
        }) => run(db, 'UPDATE pos_jobs SET payload_json=? WHERE id=?', facts.fact0, facts.id),
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
        }) => run(db, "UPDATE pos_payments SET provider_id=?,qr_data=?,expires_at=?,state='pending',version=version+1,updated_at=? WHERE id=? AND state IN ('requested','exception')", facts.id, facts.qr, facts.fact2, facts.fact3, facts.id2),
        readPosSales2: (facts: {
            sale_id: string | number | boolean | null;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=?', facts.sale_id),
        writePosPayments6: (facts: {
            id: string;
            url: unknown;
            fact2: number;
            id2: string | number | boolean | null;
        }) => run(db, "UPDATE pos_payments SET provider_id=?,checkout_url=?,state='pending',version=version+1,updated_at=? WHERE id=? AND state IN ('requested','exception')", facts.id, facts.url, facts.fact2, facts.id2),
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
        }) => run(db, "UPDATE pos_refund_payments SET provider_id=?,state='pending' WHERE id=?", facts.id, facts.id2),
        readPosJobs1: (facts: {
            token: `${string}-${string}-${string}-${string}-${string}`;
            fact1: number;
            now: number;
            now2: number;
            companyId: string | null;
            companyId2: string | null;
        }) => one(db, "UPDATE pos_jobs SET state='running',lease_token=?,lease_until=?,attempts=attempts+1 WHERE id=(SELECT id FROM pos_jobs WHERE (state='pending' OR state='running' AND lease_until<?) AND next_attempt_at<=? AND (?::text IS NULL OR company_id=?) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *", facts.token, facts.fact1, facts.now, facts.now2, facts.companyId, facts.companyId2),
        writePosJobs2: (facts: {
            fact0: number;
            id: string | number | boolean | null;
            token: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, "UPDATE pos_jobs SET lease_until=? WHERE id=? AND lease_token=? AND state='running'", facts.fact0, facts.id, facts.token),
        writePosJobs3: (facts: {
            fact0: "completed" | "pending";
            fact1: number;
            id: string | number | boolean | null;
            token: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, 'UPDATE pos_jobs SET state=?,next_attempt_at=?,attempts=0,lease_until=NULL,lease_token=NULL,last_error=NULL WHERE id=? AND lease_token=?', facts.fact0, facts.fact1, facts.id, facts.token),
        writePosJobs4: (facts: {
            fact0: "pending" | "exception";
            fact1: number;
            code: string;
            id: string | number | boolean | null;
            token: `${string}-${string}-${string}-${string}-${string}`;
        }) => run(db, 'UPDATE pos_jobs SET state=?,next_attempt_at=?,last_error=?,lease_until=NULL,lease_token=NULL WHERE id=? AND lease_token=?', facts.fact0, facts.fact1, facts.code, facts.id, facts.token),
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
        }) => run(db, 'INSERT INTO pos_stripe_routes VALUES(?,?,?,?,?) ON CONFLICT(account_id,livemode) DO NOTHING', facts.arg0, facts.arg1, facts.arg2, facts.arg3, facts.arg4),
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
const application = createPosStripe({ clock: { now: () => Date.now() }, uuid: id, environment: key => process.env[key], error: (status, code, message) => new ApiError(status, code, message), resources: async () => { const env = await runtimeEnv(); return { control: repository(env.CONTROL_DB as PostgresDatabase), holdings: resourceSlots(env) }; }, forHolding: async (holdingId) => repository((await holdingResources(holdingId)).write as PostgresDatabase), provider: { request: stripeRequest, secret, origin, settingsReturn: (slug, companyId) => adminCompanyHandoff(companyId, `/holdings/${encodeURIComponent(slug)}/settings/stripe`, process.env.ERPX_PUBLIC_ORIGIN), fetch: (url, { timeoutMs, ...init }) => fetch(url, { ...init, signal: AbortSignal.timeout(timeoutMs) }) }, signing: { equal: (secret, payload, signature) => timingSafeEqual(createHmac("sha256", secret).update(payload).digest(), Buffer.from(signature, "hex")) }, leases: { repeat: setInterval, cancel: token => clearInterval(token as ReturnType<typeof setInterval>) }, report: value => console.error(value) });
export function stripeCall(live: boolean, path: string, options: StripeOptions = {}) { return application.stripeCall(live, path, options); }
export function stripeListCall(live: boolean, path: string, options: StripeOptions = {}) { return application.stripeListCall(live, path, options); }
export function posStripeWebhook(raw: string, signature: string) { return application.posStripeWebhook(raw, signature); }
export function oauthUrl(state: string, holdingSlug: string, live: boolean) { return application.oauthUrl(state, holdingSlug, live); }
export function finishPosOAuth(actor: WorkContext, stateId: string, code: string) { return application.finishPosOAuth(actor, stateId, code); }
export function drainPos(db: PostgresDatabase, holdingId: string, limit = 8, companyId: string | null = null) { return application.drainPos(repository(db), holdingId, limit, companyId); }
export function drainAllPos() { return application.drainAllPos(); }
export function verifyPosStripeSignature(raw: string, signature: string, signingSecrets: string[], now = Date.now()) { return application.verifyPosStripeSignature(raw, signature, signingSecrets, now); }
