import { assertPmsProviderAuthority } from "@erpx/server-pms/adapters/provider-authority";
import 'server-only';
import { randomUUID } from 'node:crypto';
import { type Snapshot } from "@erpx/server-pms/public/rules/pms";
import { bookingSnapshot, persist, queueMail, bookingEnv, type BookingPaymentRow } from "@erpx/server-pms/public/pms-booking";
import { effects } from "../../../../packages/server-pms/src/public/pms";
import { stripeCall } from "@erpx/server-pos/public/pos-stripe";
import { verifiedStripePaymentTime } from "@erpx/server-pos/public/stripe-payment-time";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, TransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Effects } from "@erpx/feature-pms/lib/pms-engine";
import type { BookingEnv } from "@erpx/server-pms/public/pms-booking";
import { createPmsBookingRefund, type PmsBookingRefundUnit, type PmsBookingRefundRepository, type Request } from "@erpx/server-pms/application/pms-booking-refunds";
import { holdingResources as resolveResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePmsPropertyPurchase, requirePmsFinancialRead, withPmsPropertyPurchasedTransaction } from "@erpx/server-pms/adapters/property-purchase-boundary";
function unit(db: SqlDatabase, _whole?: TransactionalSqlDatabase): PmsBookingRefundUnit {
    void _whole;
    return {
        requirePurchase: input => requirePmsPropertyPurchase(db, input),
        authorize: (actor,permission)=>assertPmsProviderAuthority(db,actor,permission),
        readPmsBookingPayments: (facts: {
            paymentId: string;
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_payments WHERE id=? AND property_id=? FOR UPDATE').bind(facts.paymentId, facts.id).first<BookingPaymentRow>(),
        readPmsBookingRefundRequests: (facts: {
            requestId: `${string}-${string}-${string}-${string}-${string}`;
        }) => db.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=?').bind(facts.requestId).first<Request>(),
        readPmsBookingAllocations: (facts: {
            paymentId: string;
        }) => db.prepare('SELECT stay_id,guest_account_id,entry_id,amount_minor FROM pms_booking_allocations WHERE payment_id=? ORDER BY stay_id').bind(facts.paymentId).all<{
            stay_id: string;
            guest_account_id: string;
            entry_id: string;
            amount_minor: number;
        }>(),
        readPmsBookingRefundRequests2: (facts: {
            paymentId: string;
        }) => db.prepare('SELECT amount_minor,allocations_json FROM pms_booking_refund_requests WHERE payment_id=?').bind(facts.paymentId).all<{
            amount_minor: number;
            allocations_json: string;
        }>(),
        insertPmsBookingRefundRequests: (facts: {
            requestId: `${string}-${string}-${string}-${string}-${string}`;
            paymentId: string;
            amount: number;
            fact3: string;
            reason: string;
            requestedBy: string;
            now: number;
            now2: number;
        }) => db.prepare("INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,?,?,?,'requested',?,?)").bind(facts.requestId, facts.paymentId, facts.amount, facts.fact3, facts.reason, facts.requestedBy, facts.now, facts.now2).run(),
        updatePmsBookingPayments: (facts: {
            now: number;
            now2: number;
            paymentId: string;
        }) => db.prepare("UPDATE pms_booking_payments SET refund_state='requested',refund_requested_at=?,updated_at=? WHERE id=?").bind(facts.now, facts.now2, facts.paymentId).run(),
        readPmsBookingRefundRequests3: (facts: {
            id: string;
            id2: string;
            companyId: string;
        }) => db.prepare("SELECT r.id,p.property_id FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE (r.id=? OR p.id=?) AND p.company_id=? AND r.state='requested' ORDER BY r.created_at LIMIT 1").bind(facts.id, facts.id2, facts.companyId).first<{
            id: string;
            property_id: string;
        }>(),
        readWriteContext: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsBookingRefundRequests4: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(facts.id).first<Request>(),
        updatePmsBookingRefundRequests: (facts: {
            userId: string;
            fact1: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_refund_requests SET state='approved',legacy_review=false,approved_by=?,updated_at=? WHERE id=?").bind(facts.userId, facts.fact1, facts.id).run(),
        updatePmsBookingPayments2: (facts: {
            userId: string;
            fact1: number;
            payment_id: string;
        }) => db.prepare("UPDATE pms_booking_payments SET refund_state='approved',refund_approved_by=?,updated_at=? WHERE id=?").bind(facts.userId, facts.fact1, facts.payment_id).run(),
        readPmsRecords: (facts: {
            companyId: string;
        }) => db.prepare(`SELECT r.id,p.id payment_id,p.property_id,r.amount_minor,p.status,r.state refund_state,r.last_error,r.reason,r.created_at, (p.status='refund_review' AND r.state='requested' AND r.allocations_json::jsonb='[]'::jsonb AND NOT EXISTS(SELECT 1 FROM pms_records e WHERE e.property_id=p.property_id AND e.kind='entry' AND e.data_json::jsonb->>'sourceKey'='online-payment:'||p.id)) needs_receipt_recovery FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100`).bind(facts.companyId).all<{
            id: string;
            payment_id: string;
            property_id: string;
            amount_minor: number;
            status: string;
            refund_state: string;
            last_error: string | null;
            reason: string;
            created_at: number;
            needs_receipt_recovery: boolean;
        }>(),
        readPmsBookingPayments2: (facts: {
            requestId: string;
            companyId: string;
        }) => db.prepare(`SELECT p.* FROM pms_booking_payments p JOIN pms_booking_refund_requests r ON r.payment_id=p.id WHERE r.id=? AND p.company_id=? AND p.status='refund_review' AND r.state='requested'`).bind(facts.requestId, facts.companyId).first<BookingPaymentRow>(),
        readWriteContext2: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsBookingPayments3: (facts: {
            id: string;
        }) => db.prepare("SELECT status FROM pms_booking_payments WHERE id=? FOR UPDATE").bind(facts.id).first<{
            status: string;
        }>(),
        updatePmsBookingPayments3: (facts: {
            succeededAt: number;
            succeededAt2: number;
            id: string;
            now: number;
            id2: string;
        }) => db.prepare('UPDATE pms_booking_payments SET paid_at=?,provider_succeeded_at=?,payment_intent_id=?,last_error=NULL,updated_at=? WHERE id=?').bind(facts.succeededAt, facts.succeededAt2, facts.id, facts.now, facts.id2).run(),
        readPmsBookingRefundRequests5: (facts: {
            id: string;
            id2: string;
        }) => db.prepare("SELECT * FROM pms_booking_refund_requests WHERE (id=? OR payment_id=?) AND state IN ('approved','processing','requires_action') ORDER BY created_at").bind(facts.id, facts.id2).all<Request>(),
        readPmsBookingPayments4: (facts: {
            payment_id: string;
        }) => db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(facts.payment_id).first<BookingPaymentRow>(),
        insertPmsBookingRefundAttempts: (facts: {
            id: string;
            generation: number;
            refundId: string;
            now: number;
        }) => db.prepare('INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING').bind(facts.id, facts.generation, facts.refundId, facts.now).run(),
        updatePmsBookingRefundRequests2: (facts: {
            refundId: string;
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_refund_requests SET provider_id=?,state='processing',updated_at=? WHERE id=? AND provider_id IS NULL").bind(facts.refundId, facts.now, facts.id).run(),
        updatePmsBookingPayments4: (facts: {
            refundId: string;
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_payments SET refund_id=?,refund_state='processing',updated_at=? WHERE id=?").bind(facts.refundId, facts.now, facts.id).run(),
        updatePmsBookingRefundRequests3: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_refund_requests SET state='failed',last_error='Provider refund failed; liability remains outstanding',updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        updatePmsBookingPayments5: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_payments SET refund_state='failed',updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        updatePmsBookingRefundRequests4: (facts: {
            state: "requires_action" | "processing";
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_refund_requests SET state=?,updated_at=? WHERE id=?').bind(facts.state, facts.now, facts.id).run(),
        readWriteContext3: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsBookingRefundRequests6: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(facts.id).first<Request>(),
        updatePmsBookingRefundRequests5: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_refund_requests SET state='succeeded',last_error=NULL,updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        updatePmsBookingPayments6: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_payments SET refund_state='succeeded',updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        readPmsBookingRefundRequests7: (facts: {
            id: string;
            companyId: string;
        }) => db.prepare("SELECT r.*,p.property_id,p.account_id,p.livemode FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE r.id=? AND p.company_id=?").bind(facts.id, facts.companyId).first<Request & {
            property_id: string;
            account_id: string;
            livemode: boolean;
        }>(),
        readPmsBookingRefundRequests8: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(facts.id).first<Request>(),
        insertPmsBookingRefundAttempts2: (facts: {
            id: string;
            generation: number;
            provider_id: string | null;
            fact3: number;
        }) => db.prepare('INSERT INTO pms_booking_refund_attempts(request_id,generation,provider_id,created_at) VALUES(?,?,?,?) ON CONFLICT DO NOTHING').bind(facts.id, facts.generation, facts.provider_id, facts.fact3).run(),
        updatePmsBookingRefundRequests6: (facts: {
            fact0: string;
            fact1: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_refund_requests SET generation=generation+1,provider_id=NULL,state='approved',last_error=?,updated_at=? WHERE id=?").bind(facts.fact0, facts.fact1, facts.id).run(),
        updatePmsBookingPayments7: (facts: {
            fact0: number;
            payment_id: string;
        }) => db.prepare("UPDATE pms_booking_payments SET refund_state='approved',updated_at=? WHERE id=?").bind(facts.fact0, facts.payment_id).run(),
        persist: (arg0: Snapshot, arg1: Map<string, string>) => persist(db, arg0, arg1),
        bookingSnapshot: (arg0: string, arg1?: undefined, arg2?: true) => bookingSnapshot(db, arg0, arg1, arg2),
        effects: (arg0: WorkContext, arg1: Snapshot, arg2: Effects) => effects(db, arg0, arg1, arg2),
        queueMail: (arg0: Snapshot, arg1: string, arg2: string, arg3: "refund", arg4: BookingEnv, arg5: number) => queueMail(db, arg0, arg1, arg2, arg3, arg4, arg5)
    };
}
function repository(db: TransactionalSqlDatabase): PmsBookingRefundRepository { return { ...unit(db, db), transaction: (work, purchase) => withPmsPropertyPurchasedTransaction(db, tx => work(unit(tx, db)), purchase) }; }
const application = createPmsBookingRefund({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), liveEnabled: () => process.env.PMS_STRIPE_LIVE_ENABLED, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => { const database=(await resolveResources(actor.holdingId)).write; await requirePmsFinancialRead(database,actor); return unit(database); }, stripeCall: (...args) => stripeCall(...args), verifiedStripePaymentTime: (...args) => verifiedStripePaymentTime(...args), bookingEnv: (...args) => bookingEnv(...args) });
import { createHash } from "node:crypto";
export function requestBookingRefund(tx: SqlDatabase, s: Snapshot, paymentId: string, wanted: Array<{
    stayId: string;
    amount: number;
}>, reason: string, requestedBy: string, now = Date.now(), requestId = randomUUID()) { return application.requestBookingRefund(unit(tx, undefined), s, paymentId, wanted, reason, requestedBy, now, requestId); }
export function approveBookingRefund(db: TransactionalSqlDatabase, actor: WorkContext, id: string) { return application.approveBookingRefund(repository(db), actor, id); }
export function bookingRefundsForFinance(actor: WorkContext) { return application.bookingRefundsForFinance(actor); }
export function recoverBookingReceipt(db: TransactionalSqlDatabase, actor: WorkContext, requestId: string, reason: string) { return application.recoverBookingReceipt(repository(db), actor, requestId, reason); }
export function processBookingRefund(db: TransactionalSqlDatabase, id: string, now = Date.now()) { return application.processBookingRefund(repository(db), id, now); }
export function retryBookingRefund(db: TransactionalSqlDatabase, actor: WorkContext, id: string, reason: string) { return application.retryBookingRefund(repository(db), actor, id, reason); }
