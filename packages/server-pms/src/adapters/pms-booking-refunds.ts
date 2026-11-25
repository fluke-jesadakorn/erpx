import { assertPmsProviderAuthority } from "@erpx/server-pms/adapters/provider-authority";
import { PMS_WRITE_CONTEXT_SQL } from "./pms-read-tail-r17-sql";
import { PMS_BOOKING_PAYMENT_BY_ID_SQL } from "./pms-read-tail-r18-sql";
import { pmsRefundPaymentJoinHeadSql } from "./pms-read-tail-r34-sql";
import 'server-only';
import { randomUUID } from 'node:crypto';
import { type Snapshot } from "@erpx/server-pms/public/rules/pms";
import { bookingSnapshot, persist, queueMail, bookingEnv, type BookingPaymentRow } from "@erpx/server-pms/public/pms-booking";
import { effects } from "../../../../packages/server-pms/src/public/pms";
import {stripeCall} from '@erpx/server-pos/public/stripe-provider';
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
import { PMS_REFUND_NON_LOCK_HEAD_SQL } from "./pms-read-tail-r40-sql";
import { nativePmsReadAll, PMS_RD_BOOKING_ALLOCATIONS, PMS_RD_REFUND_REQUEST_ALLOCATIONS } from "./native-pms-read";
function unit(db: SqlDatabase, _whole?: TransactionalSqlDatabase, companyId = ''): PmsBookingRefundUnit {
        const write = (kind: string, values: unknown[]): any => nativePmsOwnerWrite(companyId, kind, values);
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
        }) => db.prepare(`${PMS_REFUND_NON_LOCK_HEAD_SQL}id=?`).bind(facts.requestId).first<Request>(),
        readPmsBookingAllocations: (facts: {
            paymentId: string;
        }) => nativePmsReadAll<{
            stay_id: string;
            guest_account_id: string;
            entry_id: string;
            amount_minor: number;
        }>(PMS_RD_BOOKING_ALLOCATIONS, [facts.paymentId], db),
        readPmsBookingRefundRequests2: (facts: {
            paymentId: string;
        }) => nativePmsReadAll<{
            amount_minor: number;
            allocations_json: string;
        }>(PMS_RD_REFUND_REQUEST_ALLOCATIONS, [facts.paymentId], db),
        insertPmsBookingRefundRequests: (facts: {
            requestId: `${string}-${string}-${string}-${string}-${string}`;
            paymentId: string;
            amount: number;
            fact3: string;
            reason: string;
            requestedBy: string;
            now: number;
            now2: number;
        }) => write('pms_w009', [facts.requestId, facts.paymentId, facts.amount, facts.fact3, facts.reason, facts.requestedBy, facts.now, facts.now2]),
        updatePmsBookingPayments: (facts: {
            now: number;
            now2: number;
            paymentId: string;
        }) => write('pms_w010', [facts.now, facts.now2, facts.paymentId]),
        readPmsBookingRefundRequests3: (facts: {
            id: string;
            id2: string;
            companyId: string;
        }) => db.prepare(`${pmsRefundPaymentJoinHeadSql('property')} WHERE (r.id=? OR p.id=?) AND p.company_id=? AND r.state='requested' ORDER BY r.created_at LIMIT 1`).bind(facts.id, facts.id2, facts.companyId).first<{
            id: string;
            property_id: string;
        }>(),
        readWriteContext: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
        readPmsBookingRefundRequests4: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(facts.id).first<Request>(),
        updatePmsBookingRefundRequests: (facts: {
            userId: string;
            fact1: number;
            id: string;
        }) => write('pms_w011', [facts.userId, facts.fact1, facts.id]),
        updatePmsBookingPayments2: (facts: {
            userId: string;
            fact1: number;
            payment_id: string;
        }) => write('pms_w012', [facts.userId, facts.fact1, facts.payment_id]),
        readPmsRecords: (facts: {
            companyId: string;
        }) => db.prepare(`${pmsRefundPaymentJoinHeadSql('fullReceipt')} WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100`).bind(facts.companyId).all<{
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
        readWriteContext2: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
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
        }) => write('pms_w013', [facts.succeededAt, facts.succeededAt2, facts.id, facts.now, facts.id2]),
        readPmsBookingRefundRequests5: (facts: {
            id: string;
            id2: string;
        }) => db.prepare(`${PMS_REFUND_NON_LOCK_HEAD_SQL}(id=? OR payment_id=?) AND state IN ('approved','processing','requires_action') ORDER BY created_at`).bind(facts.id, facts.id2).all<Request>(),
        readPmsBookingPayments4: (facts: {
            payment_id: string;
        }) => db.prepare(PMS_BOOKING_PAYMENT_BY_ID_SQL).bind(facts.payment_id).first<BookingPaymentRow>(),
        insertPmsBookingRefundAttempts: (facts: {
            id: string;
            generation: number;
            refundId: string;
            now: number;
        }) => write('pms_w014', [facts.id, facts.generation, facts.refundId, facts.now]),
        updatePmsBookingRefundRequests2: (facts: {
            refundId: string;
            now: number;
            id: string;
        }) => write('pms_w015', [facts.refundId, facts.now, facts.id]),
        updatePmsBookingPayments4: (facts: {
            refundId: string;
            now: number;
            id: string;
        }) => write('pms_w016', [facts.refundId, facts.now, facts.id]),
        updatePmsBookingRefundRequests3: (facts: {
            now: number;
            id: string;
        }) => write('pms_w017', [facts.now, facts.id]),
        updatePmsBookingPayments5: (facts: {
            now: number;
            id: string;
        }) => write('pms_w018', [facts.now, facts.id]),
        updatePmsBookingRefundRequests4: (facts: {
            state: "requires_action" | "processing";
            now: number;
            id: string;
        }) => write('pms_w019', [facts.state, facts.now, facts.id]),
        readWriteContext3: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
        readPmsBookingRefundRequests6: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_refund_requests WHERE id=? FOR UPDATE').bind(facts.id).first<Request>(),
        updatePmsBookingRefundRequests5: (facts: {
            now: number;
            id: string;
        }) => write('pms_w020', [facts.now, facts.id]),
        updatePmsBookingPayments6: (facts: {
            now: number;
            id: string;
        }) => write('pms_w021', [facts.now, facts.id]),
        insertPmsBookingRefundAttempts2: (facts: {
            id: string;
            generation: number;
            provider_id: string | null;
            fact3: number;
        }) => write('pms_w022', [facts.id, facts.generation, facts.provider_id, facts.fact3]),
        updatePmsBookingRefundRequests6: (facts: {
            fact0: string;
            fact1: number;
            id: string;
        }) => write('pms_w023', [facts.fact0, facts.fact1, facts.id]),
        updatePmsBookingPayments7: (facts: {
            fact0: number;
            payment_id: string;
        }) => write('pms_w024', [facts.fact0, facts.payment_id]),
        persist: (arg0: Snapshot, arg1: Map<string, string>) => persist(db, arg0, arg1),
        bookingSnapshot: (arg0: string, arg1?: undefined, arg2?: true) => bookingSnapshot(db, arg0, arg1, arg2),
        effects: (arg0: WorkContext, arg1: Snapshot, arg2: Effects) => effects(db, arg0, arg1, arg2),
        queueMail: (arg0: Snapshot, arg1: string, arg2: string, arg3: "refund", arg4: BookingEnv, arg5: number) => queueMail(db, arg0, arg1, arg2, arg3, arg4, arg5)
    };
}
function repository(db: TransactionalSqlDatabase, companyId = ''): PmsBookingRefundRepository { return { ...unit(db, db, companyId), transaction: (work, purchase) => withPmsPropertyPurchasedTransaction(db, tx => work(unit(tx, db, companyId)), purchase) }; }
const application = createPmsBookingRefund({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), liveEnabled: () => process.env.PMS_STRIPE_LIVE_ENABLED, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => { const database=(await resolveResources(actor.holdingId)).write; await requirePmsFinancialRead(database,actor); return unit(database, undefined, actor.companyId); }, stripeCall: (...args) => stripeCall(...args), verifiedStripePaymentTime: (...args) => verifiedStripePaymentTime(...args), bookingEnv: (...args) => bookingEnv(...args) });
import { createHash } from "node:crypto";
import { nativePmsOwnerWrite } from "@erpx/feature-pms/public/native-pms";
export function requestBookingRefund(tx: SqlDatabase, s: Snapshot, paymentId: string, wanted: Array<{
    stayId: string;
    amount: number;
}>, reason: string, requestedBy: string, now = Date.now(), requestId = randomUUID()) { return application.requestBookingRefund(unit(tx, undefined), s, paymentId, wanted, reason, requestedBy, now, requestId); }
export function approveBookingRefund(db: TransactionalSqlDatabase, actor: WorkContext, id: string) { return application.approveBookingRefund(repository(db, actor.companyId), actor, id); }
export function bookingRefundsForFinance(actor: WorkContext) { return application.bookingRefundsForFinance(actor); }
export function recoverBookingReceipt(db: TransactionalSqlDatabase, actor: WorkContext, requestId: string, reason: string) { return application.recoverBookingReceipt(repository(db, actor.companyId), actor, requestId, reason); }
export function processBookingRefund(db: TransactionalSqlDatabase, id: string, now = Date.now()) { return application.processBookingRefund(repository(db), id, now); }
export function retryBookingRefund(db: TransactionalSqlDatabase, actor: WorkContext, id: string, reason: string, incoming: Pick<Headers, 'get'>) { return application.retryBookingRefund(repository(db, actor.companyId), actor, id, reason, incoming); }
