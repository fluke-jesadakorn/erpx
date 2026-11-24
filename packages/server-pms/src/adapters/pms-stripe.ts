import { assertPmsProviderAuthority } from "@erpx/server-pms/adapters/provider-authority";
import { PMS_WRITE_CONTEXT_SQL } from "./pms-read-tail-r17-sql";
import { PMS_STRIPE_REFUND_COLUMNS_SQL } from "./pms-read-tail-r22-sql";
import 'server-only';
import { createHash, randomUUID } from 'node:crypto';
import { type Snapshot } from "@erpx/server-pms/public/rules/pms";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import { bookingSnapshot, persist } from "@erpx/server-pms/public/pms-booking";
import { effects as persistEffects, persistPmsRecords } from "../../../../packages/server-pms/src/public/pms";
import { verifiedStripePaymentTime } from "@erpx/server-pos/public/stripe-payment-time";
import {stripeCall} from '@erpx/server-pos/public/stripe-provider';
import type { TransactionalSqlDatabase, SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Effects } from "@erpx/feature-pms/lib/pms-engine";
import { createPmsCounterPayment, type PmsCounterPaymentUnit, type PmsCounterPaymentRepository, type Payment } from "@erpx/server-pms/application/pms-stripe";
import { holdingResources as resolveResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { requirePmsCounterPurchase, withPmsCounterPurchasedTransaction } from "@erpx/server-pms/adapters/counter-purchase-boundary";
import { requirePmsFinancialRead } from "@erpx/server-pms/adapters/property-purchase-boundary";
function unit(db: SqlDatabase, _whole?: TransactionalSqlDatabase, companyId = ''): PmsCounterPaymentUnit {
        const write = (kind: string, values: unknown[]): any => nativePmsOwnerWrite(companyId, kind, values);
    void _whole;
    return {
        authorize: (actor,permission)=>assertPmsProviderAuthority(db,actor,permission),
        readPmsStripePayments: (facts: {
            key: string;
        }) => db.prepare('SELECT id,request_hash FROM pms_stripe_payments WHERE id=?').bind(facts.key).first<{
            id: string;
            request_hash: string;
        }>(),
        readPosConnections: facts => pmsCounterPaymentFacts(db).readPosConnections(facts),
        readPosSettings: facts => pmsCounterPaymentFacts(db).readPosSettings(facts),
        insertPmsStripePayments: (facts: {
            key: string;
            digest: string;
            propertyId: string;
            guestAccountId: string;
            companyId: string;
            account_id: string;
            livemode: boolean;
            kind: "deposit" | "receipt";
            amountMinor: number;
            fact9: string | null;
            fact10: number;
            userId: string;
            now: number;
            now2: number;
            clearing: unknown;
        }) => write('pms_w049', [facts.key, facts.digest, facts.propertyId, facts.guestAccountId, facts.companyId, facts.account_id, facts.livemode, facts.kind, facts.amountMinor, facts.fact9, facts.fact10, facts.userId, facts.now, facts.now2, facts.clearing]),
        readPmsStripePayments2: (facts: {
            id: string;
        }) => nativePmsReadFirst<Payment>(PMS_RD_STRIPE_PAYMENT_BY_ID, [facts.id], db),
        updatePmsStripePayments: (facts: {
            id: string;
            qr: string;
            now: number;
            id2: string;
        }) => write('pms_w050', [facts.id, facts.qr, facts.now, facts.id2]),
        readPmsStripePayments3: (facts: {
            id: string;
            companyId: string;
        }) => nativePmsReadFirst<Payment>(PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY, [facts.id, facts.companyId], db),
        readPmsStripePayments4: (facts: {
            id: string;
        }) => nativePmsReadFirst<Payment>(PMS_RD_STRIPE_PAYMENT_BY_ID, [facts.id], db),
        readPmsStripePayments5: (facts: {
            id: string;
        }) => nativePmsReadFirst<Payment>(PMS_RD_STRIPE_PAYMENT_BY_ID, [facts.id], db),
        updatePmsStripePayments2: (facts: {
            now: number;
            id: string;
        }) => write('pms_w051', [facts.now, facts.id]),
        updatePmsStripePayments3: (facts: {
            now: number;
            id: string;
        }) => write('pms_w052', [facts.now, facts.id]),
        updatePmsStripePayments4: (facts: {
            succeededAt: number;
            succeededAt2: number;
            id: string;
        }) => write('pms_w053', [facts.succeededAt, facts.succeededAt2, facts.id]),
        readWriteContext: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
        readPmsStripePayments6: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(facts.id).first<Payment>(),
        updatePmsStripePayments5: (facts: {
            id: string;
            now: number;
            amount_minor: number;
            now2: number;
            id2: string;
        }) => write('pms_w054', [facts.id, facts.now, facts.amount_minor, facts.now2, facts.id2]),
        updatePmsProperties: (facts: {
            version: number;
            now: number;
            id: string;
        }) => write('pms_w055', [facts.version, facts.now, facts.id]),
        updatePmsStripePayments6: (facts: {
            id: string;
            now: number;
            now2: number;
            id2: string;
        }) => write('pms_w056', [facts.id, facts.now, facts.now2, facts.id2]),
        updatePmsStripePayments7: (facts: {
            now: number;
            id: string;
        }) => write('pms_w057', [facts.now, facts.id]),
        readPmsStripePayments7: (facts: {
            fact0: number;
            fact1: number;
        }) => db.prepare("SELECT id FROM pms_stripe_payments WHERE state IN ('requested','pending') OR (state='cancelled' AND provider_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20").bind(facts.fact0, facts.fact1).all<{
            id: string;
        }>(),
        updatePmsStripePayments8: (facts: {
            id: string;
        }) => write('pms_w058', [facts.id]),
        updatePmsStripePayments9: (facts: {
            fact0: string;
            id: string;
        }) => write('pms_w059', [facts.fact0, facts.id]),
        readPmsStripePayments8: () => db.prepare("SELECT id FROM pms_stripe_payments WHERE refund_state IN ('approved','processing','requires_action') ORDER BY updated_at LIMIT 20").bind().all<{
            id: string;
        }>(),
        updatePmsStripePayments11: (facts: {
            fact0: string;
            id: string;
        }) => write('pms_w060', [facts.fact0, facts.id]),
        readPmsStripePayments9: (facts: {
            paymentId: string;
            companyId: string;
        }) => nativePmsReadFirst<{ property_id: string }>(PMS_RD_STRIPE_PAYMENT_PROPERTY, [facts.paymentId, facts.companyId], db),
        readPmsStripePayments10: (facts: {
            paymentId: string;
            companyId: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE').bind(facts.paymentId, facts.companyId).first<Payment>(),
        updatePmsStripePayments12: (facts: {
            amountMinor: number;
            fact1: string;
            userId: string;
            fact3: number;
            paymentId: string;
        }) => write('pms_w061', [facts.amountMinor, facts.fact1, facts.userId, facts.fact3, facts.paymentId]),
        readPmsStripePayments11: (facts: {
            paymentId: string;
            companyId: string;
        }) => nativePmsReadFirst<{ property_id: string }>(PMS_RD_STRIPE_PAYMENT_PROPERTY, [facts.paymentId, facts.companyId], db),
        readWriteContext2: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
        readPmsStripePayments12: (facts: {
            paymentId: string;
            companyId: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE').bind(facts.paymentId, facts.companyId).first<Payment>(),
        updatePmsStripePayments13: (facts: {
            userId: string;
            fact1: number;
            paymentId: string;
        }) => write('pms_w062', [facts.userId, facts.fact1, facts.paymentId]),
        readPmsStripePayments13: (facts: {
            id: string;
        }) => nativePmsReadFirst<Payment>(PMS_RD_STRIPE_PAYMENT_BY_ID, [facts.id], db),
        insertPmsCounterRefundAttempts: (facts: {
            id: string;
            refund_generation: number;
            refundId: string;
            refund_amount_minor: number;
            fact4: string;
            fact5: string;
            fact6: string;
            now: number;
            now2: number;
        }) => write('pms_w063', [facts.id, facts.refund_generation, facts.refundId, facts.refund_amount_minor, facts.fact4, facts.fact5, facts.fact6, facts.now, facts.now2]),
        updatePmsStripePayments14: (facts: {
            refundId: string;
            now: number;
            id: string;
        }) => write('pms_w064', [facts.refundId, facts.now, facts.id]),
        updatePmsCounterRefundAttempts: (facts: {
            now: number;
            id: string;
            refund_generation: number;
        }) => write('pms_w065', [facts.now, facts.id, facts.refund_generation]),
        updatePmsStripePayments15: (facts: {
            now: number;
            id: string;
        }) => write('pms_w066', [facts.now, facts.id]),
        updatePmsStripePayments16: (facts: {
            state: "requires_action" | "processing";
            now: number;
            id: string;
        }) => write('pms_w067', [facts.state, facts.now, facts.id]),
        updatePmsCounterRefundAttempts2: (facts: {
            state: "requires_action" | "processing";
            now: number;
            id: string;
            refund_generation: number;
        }) => write('pms_w068', [facts.state, facts.now, facts.id, facts.refund_generation]),
        readWriteContext3: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
        readPmsStripePayments14: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(facts.id).first<Payment>(),
        updatePmsProperties2: (facts: {
            version: number;
            now: number;
            id: string;
        }) => write('pms_w069', [facts.version, facts.now, facts.id]),
        updatePmsCounterRefundAttempts3: (facts: {
            now: number;
            id: string;
            refund_generation: number;
        }) => write('pms_w070', [facts.now, facts.id, facts.refund_generation]),
        updatePmsStripePayments17: (facts: {
            now: number;
            id: string;
        }) => write('pms_w071', [facts.now, facts.id]),
        readPmsStripePayments15: (facts: {
            companyId: string;
        }) => db.prepare(`${PMS_STRIPE_REFUND_COLUMNS_SQL} WHERE company_id=? AND refund_state IS NOT NULL ORDER BY updated_at DESC LIMIT 100`).bind(facts.companyId).all<{
            id: string;
            property_id: string;
            guest_account_id: string;
            amount_minor: number;
            refund_amount_minor: number | null;
            refund_state: string;
            last_error: string | null;
            refund_reason: string | null;
            payer_email: string | null;
        }>(),
        readPmsStripePayments16: (facts: {
            id: string;
            companyId: string;
        }) => nativePmsReadFirst<Payment>(PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY, [facts.id, facts.companyId], db),
        readPmsStripePayments17: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(facts.id).first<Payment>(),
        updatePmsStripePayments18: (facts: {
            fact0: number;
            id: string;
        }) => write('pms_w072', [facts.fact0, facts.id]),
        updatePmsStripePayments19: (facts: {
            fact0: string | null;
            fact1: string;
            fact2: number;
            id: string;
        }) => write('pms_w073', [facts.fact0, facts.fact1, facts.fact2, facts.id]),
        bookingSnapshot: (arg0: string, arg1?: undefined, arg2?: true) => bookingSnapshot(db, arg0, arg1, arg2),
        snapshotMoney: (arg0: WorkContext, arg1: {
            sourceType: string;
            sourceId: string;
            amountKind: string;
            transactionDate: string;
            currency: string;
            amountUnits: string;
        }) => snapshotMoney(db, arg0, arg1),
        persistPmsRecords: (arg0: Snapshot, arg1: Map<string, string>) => persistPmsRecords(db, arg0, arg1),
        persistEffects: (arg0: WorkContext, arg1: Snapshot, arg2: Effects) => persistEffects(db, arg0, arg1, arg2),
        persist: (arg0: Snapshot, arg1: Map<string, string>) => persist(db, arg0, arg1)
    };
}
function repository(db: TransactionalSqlDatabase, companyId = ''): PmsCounterPaymentRepository { return { ...unit(db, db, companyId), requirePurchase: input => requirePmsCounterPurchase(db, input),
    transaction: (work, purchase) => purchase ? withPmsCounterPurchasedTransaction(db, purchase, tx => work(unit(tx, db, companyId))) : db.transaction(tx => work(unit(tx, db, companyId))) }; }
const application = createPmsCounterPayment({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), liveEnabled: () => process.env.PMS_STRIPE_LIVE_ENABLED, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => { const database=(await resolveResources(actor.holdingId)).write; await requirePmsFinancialRead(database,actor); return unit(database, undefined, actor.companyId); }, stripeCall: (...args) => stripeCall(...args), verifiedStripePaymentTime: (...args) => verifiedStripePaymentTime(...args) });
export function createPmsStripePayment(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string, now = Date.now()) { return application.createPmsStripePayment(repository(db, actor.companyId), actor, raw, key, now); }
export function pmsStripePaymentStatus(db: TransactionalSqlDatabase, actor: WorkContext, id: string, now = Date.now()) { return application.pmsStripePaymentStatus(repository(db), actor, id, now); }
export function reconcilePmsStripePayment(db: TransactionalSqlDatabase, id: string, now = Date.now()) { return application.reconcilePmsStripePayment(repository(db), id, now); }
export function drainPmsStripePayments(db: TransactionalSqlDatabase, now = Date.now()) { return application.drainPmsStripePayments(repository(db), now); }
export function requestPmsStripeRefund(db: TransactionalSqlDatabase, actor: WorkContext, paymentId: string, amountMinor: number, reason: string) { return application.requestPmsStripeRefund(repository(db, actor.companyId), actor, paymentId, amountMinor, reason); }
export function approvePmsStripeRefund(db: TransactionalSqlDatabase, actor: WorkContext, paymentId: string) { return application.approvePmsStripeRefund(repository(db, actor.companyId), actor, paymentId); }
export function processPmsStripeRefund(db: TransactionalSqlDatabase, id: string, now = Date.now()) { return application.processPmsStripeRefund(repository(db), id, now); }
export function pmsStripeRefundsForFinance(actor: WorkContext) { return application.pmsStripeRefundsForFinance(actor); }
export function recoverPmsStripeRefund(db: TransactionalSqlDatabase, actor: WorkContext, id: string, reason: string, payerEmail?: string) { return application.recoverPmsStripeRefund(repository(db, actor.companyId), actor, id, reason, payerEmail); }
import { pmsCounterPaymentFacts } from "@erpx/server-pos/public/pms-counter-payment-facts";
import { nativePmsOwnerWrite } from "@erpx/feature-pms/public/native-pms";
import { nativePmsReadFirst, PMS_RD_STRIPE_PAYMENT_BY_ID, PMS_RD_STRIPE_PAYMENT_BY_ID_COMPANY, PMS_RD_STRIPE_PAYMENT_PROPERTY } from './native-pms-read';
