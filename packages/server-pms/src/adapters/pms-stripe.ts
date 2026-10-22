import { assertPmsProviderAuthority } from "@erpx/server-pms/adapters/provider-authority";
import 'server-only';
import { createHash, randomUUID } from 'node:crypto';
import { type Snapshot } from "@erpx/server-pms/public/rules/pms";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { snapshotMoney } from "@erpx/server-finance/public/company-money";
import { bookingSnapshot, persist } from "@erpx/server-pms/public/pms-booking";
import { effects as persistEffects, persistPmsRecords } from "../../../../packages/server-pms/src/public/pms";
import { verifiedStripePaymentTime } from "@erpx/server-pos/public/stripe-payment-time";
import { stripeCall } from "@erpx/server-pos/public/pos-stripe";
import type { TransactionalSqlDatabase, SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Effects } from "@erpx/feature-pms/lib/pms-engine";
import { createPmsCounterPayment, type PmsCounterPaymentUnit, type PmsCounterPaymentRepository, type Payment } from "@erpx/server-pms/application/pms-stripe";
import { holdingResources as resolveResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { requirePmsCounterPurchase, withPmsCounterPurchasedTransaction } from "@erpx/server-pms/adapters/counter-purchase-boundary";
import { requirePmsFinancialRead } from "@erpx/server-pms/adapters/property-purchase-boundary";
function unit(db: SqlDatabase, _whole?: TransactionalSqlDatabase): PmsCounterPaymentUnit {
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
        }) => db.prepare("INSERT INTO pms_stripe_payments(id,request_hash,property_id,guest_account_id,company_id,account_id,livemode,kind,amount_minor,payer_email,state,expires_at,created_by,created_at,updated_at,clearing_account) VALUES(?,?,?,?,?,?,?,?,?,?,'requested',?,?,?,?,?)").bind(facts.key, facts.digest, facts.propertyId, facts.guestAccountId, facts.companyId, facts.account_id, facts.livemode, facts.kind, facts.amountMinor, facts.fact9, facts.fact10, facts.userId, facts.now, facts.now2, facts.clearing).run(),
        readPmsStripePayments2: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(facts.id).first<Payment>(),
        updatePmsStripePayments: (facts: {
            id: string;
            qr: string;
            now: number;
            id2: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET provider_id=?,qr_data=?,state='pending',updated_at=? WHERE id=? AND state='requested'").bind(facts.id, facts.qr, facts.now, facts.id2).run(),
        readPmsStripePayments3: (facts: {
            id: string;
            companyId: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=?').bind(facts.id, facts.companyId).first<Payment>(),
        readPmsStripePayments4: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(facts.id).first<Payment>(),
        readPmsStripePayments5: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(facts.id).first<Payment>(),
        updatePmsStripePayments2: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET state='cancelled',updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        updatePmsStripePayments3: (facts: {
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET received_at=COALESCE(received_at,?) WHERE id=?').bind(facts.now, facts.id).run(),
        updatePmsStripePayments4: (facts: {
            succeededAt: number;
            succeededAt2: number;
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET received_at=COALESCE(received_at,?),provider_succeeded_at=COALESCE(provider_succeeded_at,?) WHERE id=?').bind(facts.succeededAt, facts.succeededAt2, facts.id).run(),
        readWriteContext: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsStripePayments6: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(facts.id).first<Payment>(),
        updatePmsStripePayments5: (facts: {
            id: string;
            now: number;
            amount_minor: number;
            now2: number;
            id2: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET state='refund_review',entry_id=?,received_at=?,refund_state='requested',refund_amount_minor=?,refund_reason='Payment received after the QR hold expired',refund_requested_by='system:stripe',updated_at=? WHERE id=?").bind(facts.id, facts.now, facts.amount_minor, facts.now2, facts.id2).run(),
        updatePmsProperties: (facts: {
            version: number;
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_properties SET version=?,updated_at=? WHERE id=?').bind(facts.version, facts.now, facts.id).run(),
        updatePmsStripePayments6: (facts: {
            id: string;
            now: number;
            now2: number;
            id2: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET state='succeeded',entry_id=?,received_at=?,updated_at=? WHERE id=?").bind(facts.id, facts.now, facts.now2, facts.id2).run(),
        updatePmsStripePayments7: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET state='cancelled',updated_at=? WHERE id=? AND state IN ('requested','pending')").bind(facts.now, facts.id).run(),
        readPmsStripePayments7: (facts: {
            fact0: number;
            fact1: number;
        }) => db.prepare("SELECT id FROM pms_stripe_payments WHERE state IN ('requested','pending') OR (state='cancelled' AND provider_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20").bind(facts.fact0, facts.fact1).all<{
            id: string;
        }>(),
        updatePmsStripePayments8: (facts: {
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET last_error=NULL WHERE id=?').bind(facts.id).run(),
        updatePmsStripePayments9: (facts: {
            fact0: string;
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET last_error=? WHERE id=?').bind(facts.fact0, facts.id).run(),
        readPmsStripePayments8: () => db.prepare("SELECT id FROM pms_stripe_payments WHERE refund_state IN ('approved','processing','requires_action') ORDER BY updated_at LIMIT 20").bind().all<{
            id: string;
        }>(),
        updatePmsStripePayments10: (facts: {
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET last_error=NULL WHERE id=?').bind(facts.id).run(),
        updatePmsStripePayments11: (facts: {
            fact0: string;
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET last_error=? WHERE id=?').bind(facts.fact0, facts.id).run(),
        readPmsStripePayments9: (facts: {
            paymentId: string;
            companyId: string;
        }) => db.prepare('SELECT property_id FROM pms_stripe_payments WHERE id=? AND company_id=?').bind(facts.paymentId, facts.companyId).first<{
            property_id: string;
        }>(),
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
        }) => db.prepare("UPDATE pms_stripe_payments SET refund_generation=refund_generation+CASE WHEN refund_state='succeeded' THEN 1 ELSE 0 END,refund_id=NULL,refund_approved_by=NULL,refund_liability_posted=false,refund_state='requested',refund_reserved=true,refund_amount_minor=?,refund_reason=?,refund_requested_by=?,updated_at=? WHERE id=?").bind(facts.amountMinor, facts.fact1, facts.userId, facts.fact3, facts.paymentId).run(),
        readPmsStripePayments11: (facts: {
            paymentId: string;
            companyId: string;
        }) => db.prepare('SELECT property_id FROM pms_stripe_payments WHERE id=? AND company_id=?').bind(facts.paymentId, facts.companyId).first<{
            property_id: string;
        }>(),
        readWriteContext2: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsStripePayments12: (facts: {
            paymentId: string;
            companyId: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=? FOR UPDATE').bind(facts.paymentId, facts.companyId).first<Payment>(),
        updatePmsStripePayments13: (facts: {
            userId: string;
            fact1: number;
            paymentId: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET refund_state='approved',refund_reserved=false,refund_liability_posted=true,refund_approved_by=?,updated_at=? WHERE id=?").bind(facts.userId, facts.fact1, facts.paymentId).run(),
        readPmsStripePayments13: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=?').bind(facts.id).first<Payment>(),
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
        }) => db.prepare("INSERT INTO pms_counter_refund_attempts(payment_id,generation,provider_id,amount_minor,reason,requested_by,approved_by,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'processing',?,?) ON CONFLICT(payment_id,generation) DO NOTHING").bind(facts.id, facts.refund_generation, facts.refundId, facts.refund_amount_minor, facts.fact4, facts.fact5, facts.fact6, facts.now, facts.now2).run(),
        updatePmsStripePayments14: (facts: {
            refundId: string;
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET refund_id=?,refund_state='processing',updated_at=? WHERE id=? AND refund_id IS NULL").bind(facts.refundId, facts.now, facts.id).run(),
        updatePmsCounterRefundAttempts: (facts: {
            now: number;
            id: string;
            refund_generation: number;
        }) => db.prepare("UPDATE pms_counter_refund_attempts SET state='failed',updated_at=? WHERE payment_id=? AND generation=?").bind(facts.now, facts.id, facts.refund_generation).run(),
        updatePmsStripePayments15: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET refund_state='failed',updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        updatePmsStripePayments16: (facts: {
            state: "requires_action" | "processing";
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET refund_state=?,updated_at=? WHERE id=?').bind(facts.state, facts.now, facts.id).run(),
        updatePmsCounterRefundAttempts2: (facts: {
            state: "requires_action" | "processing";
            now: number;
            id: string;
            refund_generation: number;
        }) => db.prepare('UPDATE pms_counter_refund_attempts SET state=?,updated_at=? WHERE payment_id=? AND generation=?').bind(facts.state, facts.now, facts.id, facts.refund_generation).run(),
        readWriteContext3: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsStripePayments14: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(facts.id).first<Payment>(),
        updatePmsProperties2: (facts: {
            version: number;
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_properties SET version=?,updated_at=? WHERE id=?').bind(facts.version, facts.now, facts.id).run(),
        updatePmsCounterRefundAttempts3: (facts: {
            now: number;
            id: string;
            refund_generation: number;
        }) => db.prepare("UPDATE pms_counter_refund_attempts SET state='succeeded',updated_at=? WHERE payment_id=? AND generation=?").bind(facts.now, facts.id, facts.refund_generation).run(),
        updatePmsStripePayments17: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET refund_state='succeeded',updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        readPmsStripePayments15: (facts: {
            companyId: string;
        }) => db.prepare("SELECT id,property_id,guest_account_id,amount_minor,refund_amount_minor,refund_state,last_error,refund_reason,payer_email FROM pms_stripe_payments WHERE company_id=? AND refund_state IS NOT NULL ORDER BY updated_at DESC LIMIT 100").bind(facts.companyId).all<{
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
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? AND company_id=?').bind(facts.id, facts.companyId).first<Payment>(),
        readPmsStripePayments17: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_stripe_payments WHERE id=? FOR UPDATE').bind(facts.id).first<Payment>(),
        updatePmsStripePayments18: (facts: {
            fact0: number;
            id: string;
        }) => db.prepare("UPDATE pms_stripe_payments SET refund_id=NULL,refund_generation=refund_generation+1,refund_state='approved',updated_at=? WHERE id=?").bind(facts.fact0, facts.id).run(),
        updatePmsStripePayments19: (facts: {
            fact0: string | null;
            fact1: string;
            fact2: number;
            id: string;
        }) => db.prepare('UPDATE pms_stripe_payments SET payer_email=COALESCE(?,payer_email),last_error=?,updated_at=? WHERE id=?').bind(facts.fact0, facts.fact1, facts.fact2, facts.id).run(),
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
function repository(db: TransactionalSqlDatabase): PmsCounterPaymentRepository { return { ...unit(db, db), requirePurchase: input => requirePmsCounterPurchase(db, input),
    transaction: (work, purchase) => purchase ? withPmsCounterPurchasedTransaction(db, purchase, tx => work(unit(tx, db))) : db.transaction(tx => work(unit(tx, db))) }; }
const application = createPmsCounterPayment({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), liveEnabled: () => process.env.PMS_STRIPE_LIVE_ENABLED, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => { const database=(await resolveResources(actor.holdingId)).write; await requirePmsFinancialRead(database,actor); return unit(database); }, stripeCall: (...args) => stripeCall(...args), verifiedStripePaymentTime: (...args) => verifiedStripePaymentTime(...args) });
export function createPmsStripePayment(db: TransactionalSqlDatabase, actor: WorkContext, raw: unknown, key: string, now = Date.now()) { return application.createPmsStripePayment(repository(db), actor, raw, key, now); }
export function pmsStripePaymentStatus(db: TransactionalSqlDatabase, actor: WorkContext, id: string, now = Date.now()) { return application.pmsStripePaymentStatus(repository(db), actor, id, now); }
export function reconcilePmsStripePayment(db: TransactionalSqlDatabase, id: string, now = Date.now()) { return application.reconcilePmsStripePayment(repository(db), id, now); }
export function drainPmsStripePayments(db: TransactionalSqlDatabase, now = Date.now()) { return application.drainPmsStripePayments(repository(db), now); }
export function requestPmsStripeRefund(db: TransactionalSqlDatabase, actor: WorkContext, paymentId: string, amountMinor: number, reason: string) { return application.requestPmsStripeRefund(repository(db), actor, paymentId, amountMinor, reason); }
export function approvePmsStripeRefund(db: TransactionalSqlDatabase, actor: WorkContext, paymentId: string) { return application.approvePmsStripeRefund(repository(db), actor, paymentId); }
export function processPmsStripeRefund(db: TransactionalSqlDatabase, id: string, now = Date.now()) { return application.processPmsStripeRefund(repository(db), id, now); }
export function pmsStripeRefundsForFinance(actor: WorkContext) { return application.pmsStripeRefundsForFinance(actor); }
export function recoverPmsStripeRefund(db: TransactionalSqlDatabase, actor: WorkContext, id: string, reason: string, payerEmail?: string) { return application.recoverPmsStripeRefund(repository(db), actor, id, reason, payerEmail); }
import { pmsCounterPaymentFacts } from "@erpx/server-pos/public/pms-counter-payment-facts";
