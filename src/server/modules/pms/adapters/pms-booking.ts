import {hydratePmsReceiptAllocationCredits} from "./pms";
import {bookingEmailDeliveryPayload} from "./booking-email-delivery";
import "server-only";
import {publicSurfaceOrigin} from '../../identity_access/public/public-surface-entry';
import { createHash, createHmac, randomUUID, randomBytes, createCipheriv, createDecipheriv, timingSafeEqual } from "node:crypto";
import { type BookingSearch } from "@/lib/pms/pms-booking";
import { type Kind, type Property, type Snapshot } from "@/server/modules/pms/public/rules/pms";
import { effects as persistPmsEffects, persistPmsRecords } from "@/server/modules/pms/public/pms";
import { stripeCall } from "@/server/modules/pos/public/pos-stripe";
import { verifiedStripePaymentTime } from "@/server/modules/pos/public/stripe-payment-time";
import { ApiError } from "@/server/modules/runtime/public/http";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { holdingBindingBySlug, holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { isTransactionalSqlDatabase, type SqlDatabase, type TransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { SqlStatement } from "../../runtime/public/runtime/types";
import type { Effects } from "@/lib/pms/pms-engine";
import { createPmsBooking, type PmsBookingUnit, type PmsBookingRepository } from "../application/pms-booking";
import { publicReservationSchema, type BookingEnv, type BookingPaymentRow, type PaymentAllocation } from "../domain/booking";
import { processBookingRefund } from "../public/pms-booking-refunds";
import { settleBookingChange, publicChangeQuote, confirmBookingChange } from "../public/pms-booking-changes";
import { pmsBookingPublications } from "../../files/public/pms-booking-publications";
import { pmsBookingPosFacts } from "../../pos/public/pms-booking-facts";
import { pmsPropertyPurchaseAllowed, pmsPropertyPurchaseFlags, requirePmsPropertyPurchase, withPmsPropertyPurchasedTransaction } from "./property-purchase-boundary";
import { pmsPackageEnforcementMode } from "../../commercial/public/purchase-command-boundary";
import { purchasedCapabilityPredicate, packageClockSql } from "../../commercial/public/purchased-capability-sql";
export async function bookingEnv(): Promise<BookingEnv> {
    const env = await runtimeEnv();
    return {
        secret: env.BETTER_AUTH_SECRET,
        origin: publicSurfaceOrigin('booking',env.ERPX_PUBLIC_ORIGIN),
        apiKey: env.RESEND_API_KEY,
        from: env.RESEND_FROM,
    };
}
export async function bookingDatabase(slug: string) {
    const binding = await holdingBindingBySlug(slug), { write } = await holdingResources(binding.id);
    if (!isTransactionalSqlDatabase(write))
        throw new Error("POSTGRES_REQUIRED");
    return write;
}
const hash = (v: string) => createHash("sha256").update(v).digest("hex");
const sign = (secret: string, v: string) => createHmac("sha256", secret).update(`pms-booking:${v}`).digest("base64url");
function protectMail(value: string, env: BookingEnv, id: string) {
    const iv = randomBytes(12), cipher = createCipheriv("aes-256-gcm", createHash("sha256").update(`pms-mail:${env.secret}`).digest(), iv);
    cipher.setAAD(Buffer.from(id));
    const body = Buffer.concat([cipher.update(value, "utf8"), cipher.final()]);
    return JSON.stringify({
        iv: iv.toString("base64"),
        body: body.toString("base64"),
        tag: cipher.getAuthTag().toString("base64"),
    });
}
function unprotectMail(value: string, env: BookingEnv, id: string) {
    const v = JSON.parse(value), decipher = createDecipheriv("aes-256-gcm", createHash("sha256").update(`pms-mail:${env.secret}`).digest(), Buffer.from(v.iv, "base64"));
    decipher.setAAD(Buffer.from(id));
    decipher.setAuthTag(Buffer.from(v.tag, "base64"));
    return bookingEmailDeliveryPayload(Buffer.concat([
        decipher.update(Buffer.from(v.body, "base64")),
        decipher.final(),
    ]).toString("utf8"),env);
}
const same = (a: string, b: string) => {
    const x = Buffer.from(a), y = Buffer.from(b);
    return x.length === y.length && timingSafeEqual(x, y);
};
const rows = async <T>(db: SqlDatabase, sql: string, ...v: unknown[]) => (await db
    .prepare(sql)
    .bind(...v)
    .all<T>()).results;
function unit(db: SqlDatabase, whole?: TransactionalSqlDatabase): PmsBookingUnit<SqlStatement> {
    return {
        hydrateReceiptAllocationCredits: snapshot => hydratePmsReceiptAllocationCredits(db,snapshot),
        purchaseAllowed: (propertyId, purpose) => pmsPropertyPurchaseAllowed(db, propertyId, purpose),
        purchaseFlags: propertyId => pmsPropertyPurchaseFlags(db, propertyId),
        requirePurchase: input => requirePmsPropertyPurchase(db, input),
        readPmsProperties1: (facts: {
            propertyId: string;
            lock: boolean;
        }) => db.prepare(`SELECT * FROM pms_properties WHERE id=?${facts.lock ? " FOR UPDATE" : ""}`).bind(facts.propertyId).first<{
            id: string;
            company_id: string;
            branch_id: string;
            name: string;
            business_date: string;
            state: Property["state"];
            version: number;
            config_json: string;
        }>(),
        writePmsProperties1: (facts: {
            version: number;
            fact1: number;
            id: string;
        }) => db.prepare("UPDATE pms_properties SET version=?,updated_at=? WHERE id=?").bind(facts.version, facts.fact1, facts.id).run(),
        writePmsBookingTokens1: (facts: {
            id: `${string}-${string}-${string}-${string}-${string}`;
            propertyId: string;
            reservationId: string;
            fact3: string;
            fact4: number;
            now: number;
        }) => db.prepare('INSERT INTO pms_booking_tokens(id,property_id,reservation_id,token_hash,expires_at,created_at) VALUES(?,?,?,?,?,?)').bind(facts.id, facts.propertyId, facts.reservationId, facts.fact3, facts.fact4, facts.now).run(),
        writePmsBookingEmails1: (facts: {
            mailId: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            reservationId: string;
            id2: `${string}-${string}-${string}-${string}-${string}`;
            slug: string;
            kind: "access" | "refund" | "change" | "cancellation" | "confirmation";
            fact6: string;
            now: number;
            now2: number;
        }) => db.prepare("INSERT INTO pms_booking_emails(id,property_id,reservation_id,token_id,holding_slug,kind,payload_json,next_attempt_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(facts.mailId, facts.id, facts.reservationId, facts.id2, facts.slug, facts.kind, facts.fact6, facts.now, facts.now2).run(),
        readPmsBookingReceipts1: (facts: {
            key: string;
        }) => db.prepare("SELECT * FROM pms_booking_receipts WHERE id=?").bind(facts.key).first<{
            body_hash: string;
            reservation_id: string;
            response_json: string | null;
        }>(),
        readPosConnections1: facts => pmsBookingPosFacts(db).readPosConnections1(facts),
        writePmsBookingReceipts1: (facts: {
            key: string;
            id: string;
            digest: string;
            id2: string;
            now: number;
            fact5: string;
        }) => db.prepare("INSERT INTO pms_booking_receipts(id,property_id,body_hash,reservation_id,created_at,response_json) VALUES(?,?,?,?,?,?)").bind(facts.key, facts.id, facts.digest, facts.id2, facts.now, facts.fact5).run(),
        readPosSettings1: facts => pmsBookingPosFacts(db).readPosSettings1(facts),
        writePmsBookingPayments1: (facts: {
            attemptId: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            id2: string;
            companyId: string;
            slug: string;
            fact5: string;
            fact6: boolean;
            dueNow: number;
            expiresAt: number;
            now: number;
            now2: number;
            fact11: string;
            fact12: string;
            clearing: unknown;
        }) => db.prepare("INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,status,expires_at,created_at,updated_at,allocation_json,methods_json,clearing_account) VALUES(?,?,?,?,?,?,?,?,'THB','payment_pending',?,?,?,?,?,?)").bind(facts.attemptId, facts.id, facts.id2, facts.companyId, facts.slug, facts.fact5, facts.fact6, facts.dueNow, facts.expiresAt, facts.now, facts.now2, facts.fact11, facts.fact12, facts.clearing).run(),
        writePmsBookingReceipts2: (facts: {
            key: string;
            id: string;
            digest: string;
            id2: string;
            now: number;
            fact5: string;
        }) => db.prepare("INSERT INTO pms_booking_receipts(id,property_id,body_hash,reservation_id,created_at,response_json) VALUES(?,?,?,?,?,?)").bind(facts.key, facts.id, facts.digest, facts.id2, facts.now, facts.fact5).run(),
        readPmsBookingPayments1: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(facts.id).first<BookingPaymentRow>(),
        readPmsRecords1: (facts: {
            reservation_id: string;
            property_id: string;
        }) => db.prepare("SELECT g.data_json::jsonb->>'email' email FROM pms_records r JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' WHERE r.id=? AND r.property_id=?").bind(facts.reservation_id, facts.property_id).first<{
            email: string;
        }>(),
        writePmsBookingPayments2: (facts: {
            id: string;
            url: string;
            fact2: number;
            id2: string;
        }) => db.prepare("UPDATE pms_booking_payments SET checkout_id=?,checkout_url=?,updated_at=? WHERE id=? AND checkout_id IS NULL AND status='payment_pending'").bind(facts.id, facts.url, facts.fact2, facts.id2).run(),
        readPmsBookingPayments2: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(facts.id).first<BookingPaymentRow>(),
        readPmsBookingReceipts2: (facts: {
            reservation_id: string;
            id: string;
        }) => db.prepare("SELECT response_json FROM pms_booking_receipts WHERE reservation_id=? AND response_json::jsonb->>'attemptId'=? LIMIT 1").bind(facts.reservation_id, facts.id).first<{
            response_json: string;
        }>(),
        readPmsBookingPayments3: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(facts.id).first<BookingPaymentRow>(),
        writePmsBookingPayments3: (facts: {
            paymentIntentId: string;
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET payment_intent_id=?,paid_at=COALESCE(paid_at,?) WHERE id=?').bind(facts.paymentIntentId, facts.now, facts.id).run(),
        writePmsBookingPayments4: (facts: {
            paymentIntentId: string | null;
            providerSucceededAt: number | null;
            fact2: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET payment_intent_id=?,provider_succeeded_at=COALESCE(provider_succeeded_at,?),paid_at=? WHERE id=?').bind(facts.paymentIntentId, facts.providerSucceededAt, facts.fact2, facts.id).run(),
        readContext1: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsBookingPayments4: (facts: {
            id: string;
        }) => db.prepare('SELECT * FROM pms_booking_payments WHERE id=? FOR UPDATE').bind(facts.id).first<BookingPaymentRow>(),
        writePmsBookingPayments5: (facts: {
            paymentIntentId: string | null;
            providerSucceededAt: number | null;
            fact2: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET payment_intent_id=?,provider_succeeded_at=?,paid_at=? WHERE id=?').bind(facts.paymentIntentId, facts.providerSucceededAt, facts.fact2, facts.id).run(),
        writePmsBookingAllocations1: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            stayId: string;
            guestAccountId: string;
            id2: string;
            amount: number;
            now: number;
        }) => db.prepare('INSERT INTO pms_booking_allocations(id,payment_id,stay_id,guest_account_id,entry_id,amount_minor,created_at) VALUES(?,?,?,?,?,?,?)').bind(facts.fact0, facts.id, facts.stayId, facts.guestAccountId, facts.id2, facts.amount, facts.now).run(),
        writePmsBookingPayments6: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_payments SET status='confirmed',updated_at=? WHERE id=?").bind(facts.now, facts.id).run(),
        writePmsBookingRefundRequests1: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            amount_minor: number;
            now: number;
            now2: number;
        }) => db.prepare("INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,'[]','Payment cannot be assigned','system:stripe','requested',?,?)").bind(facts.fact0, facts.id, facts.amount_minor, facts.now, facts.now2).run(),
        writePmsBookingPayments7: (facts: {
            fact0: "expired" | "refund_review";
            fact1: "requested" | null;
            fact2: number | null;
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_payments SET status=?,refund_state=?,refund_requested_at=?,updated_at=? WHERE id=?").bind(facts.fact0, facts.fact1, facts.fact2, facts.now, facts.id).run(),
        writePmsBookingPayments8: (facts: {
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET last_error=NULL,updated_at=? WHERE id=?').bind(facts.now, facts.id).run(),
        writePmsBookingPayments9: (facts: {
            fact0: string;
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET last_error=?,updated_at=? WHERE id=?').bind(facts.fact0, facts.now, facts.id).run(),
        writePmsBookingRefundRequests2: (facts: {
            fact0: string;
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_refund_requests SET last_error=?,updated_at=? WHERE payment_id=? AND state IN ('approved','processing','requires_action')").bind(facts.fact0, facts.now, facts.id).run(),
        writePmsBookingPayments10: (facts: {
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET last_error=NULL,updated_at=? WHERE id=?').bind(facts.now, facts.id).run(),
        writePmsBookingPayments11: (facts: {
            fact0: string;
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET last_error=?,updated_at=? WHERE id=?').bind(facts.fact0, facts.now, facts.id).run(),
        writePmsBookingRefundRequests3: (facts: {
            fact0: string;
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_refund_requests SET last_error=?,updated_at=? WHERE payment_id=? AND state IN ('approved','processing','requires_action')").bind(facts.fact0, facts.now, facts.id).run(),
        readPmsBookingTokens1: (facts: {
            fact0: string;
            now: number;
        }) => db.prepare("SELECT property_id,reservation_id FROM pms_booking_tokens WHERE token_hash=? AND expires_at>?").bind(facts.fact0, facts.now).first<{
            property_id: string;
            reservation_id: string;
        }>(),
        readPmsBookingRefundRequests1: (facts: {
            id: string;
        }) => db.prepare('SELECT r.id,r.amount_minor,r.state,r.reason FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE p.reservation_id=? ORDER BY r.created_at DESC').bind(facts.id).all<{
            id: string;
            amount_minor: number;
            state: string;
            reason: string;
        }>(),
        readPmsDocuments1: (facts: {
            id: string;
            id2: string;
            id3: string;
        }) => db.prepare("SELECT id,kind,created_at FROM pms_documents WHERE property_id=? AND guest_account_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='guest_account' AND data_json::jsonb->>'reservationId'=?) ORDER BY created_at DESC LIMIT 100").bind(facts.id, facts.id2, facts.id3).all<{
            id: string;
            kind: string;
            created_at: number;
        }>(),
        readPmsBookingPayments5: (facts: {
            id: string;
        }) => db.prepare("SELECT id,expires_at FROM pms_booking_payments WHERE reservation_id=? AND purpose='change' AND status='payment_pending' ORDER BY created_at DESC LIMIT 1").bind(facts.id).first<{
            id: string;
            expires_at: number;
        }>(),
        readPmsBookingLimits1: (facts: {
            fact0: string;
            bucket: number;
        }) => db.prepare("INSERT INTO pms_booking_limits(key,bucket,count) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET count=CASE WHEN pms_booking_limits.bucket=excluded.bucket THEN pms_booking_limits.count+1 ELSE 1 END,bucket=excluded.bucket RETURNING count").bind(facts.fact0, facts.bucket).first<{
            count: number;
        }>(),
        readPmsProperties2: (facts: {
            propertyId: string;
            companyId: string;
            fact2: Array<string>;
            actor: WorkContext;
        }) => db.prepare(`SELECT id FROM pms_properties WHERE id=? AND company_id=?${facts.actor.branchId ? " AND branch_id=?" : ""}`).bind(facts.propertyId, facts.companyId, ...facts.fact2).first(),
        writePmsBookingLimits1: (facts: {
            fact0: number;
        }) => db.prepare("DELETE FROM pms_booking_limits WHERE bucket<?").bind(facts.fact0).run(),
        readPmsBookingEmails1: (facts: {
            fact0: number;
            now: number;
            now2: number;
        }) => db.prepare("UPDATE pms_booking_emails SET state='sending',attempts=attempts+1,lease_until=? WHERE id=(SELECT id FROM pms_booking_emails WHERE (state='pending' OR (state='sending' AND lease_until<?)) AND next_attempt_at<=? ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING *").bind(facts.fact0, facts.now, facts.now2).first<{
            id: string;
            property_id: string;
            reservation_id: string;
            token_id: string;
            holding_slug: string;
            kind: string;
            attempts: number;
            payload_json: string;
        }>(),
        writePmsBookingTokens2: (facts: {
            fact0: number;
            token_id: string;
        }) => db.prepare("UPDATE pms_booking_tokens SET expires_at=? WHERE id=?").bind(facts.fact0, facts.token_id).run(),
        writePmsBookingEmailsBlocked: facts => db.prepare("UPDATE pms_booking_emails SET state='pending',attempts=GREATEST(0,attempts-1),lease_until=NULL,next_attempt_at=?,last_error=? WHERE id=?")
          .bind(facts.nextAt, facts.code, facts.id).run(),
        writePmsBookingEmails2: (facts: {
            now: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_emails SET state='sent',sent_at=?,lease_until=NULL,last_error=NULL WHERE id=?").bind(facts.now, facts.id).run(),
        writePmsBookingEmails3: (facts: {
            fact0: "pending" | "failed";
            fact1: number;
            id: string;
        }) => db.prepare("UPDATE pms_booking_emails SET state=?,next_attempt_at=?,lease_until=NULL,last_error='EMAIL_DELIVERY_FAILED' WHERE id=?").bind(facts.fact0, facts.fact1, facts.id).run(),
        readPmsRecords2: (facts: {
            propertyId: string;
            fact1: Array<string>;
            search: BookingSearch | undefined;
        }) => rows<{
            id: string;
            kind: Kind;
            data_json: string;
            version: number;
            updated_at: number;
        }>(db, `SELECT * FROM pms_records WHERE property_id=?${facts.search ? " AND (kind IN ('room_type','room','rate') OR (kind='block' AND data_json::jsonb->>'state'='active') OR (kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') AND data_json::jsonb->>'arrival' < ? AND data_json::jsonb->>'departure' > ?))" : ""} ORDER BY id`, facts.propertyId, ...facts.fact1),
        readPmsProperties3: async () => {
            const mode = await pmsPackageEnforcementMode(), enforced = mode === "enforced";
            const clock = enforced ? `WITH purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} observed_at) ` : "";
            const join = enforced ? " CROSS JOIN purchase_clock" : "";
            const purchase = enforced ? purchasedCapabilityPredicate("p.company_id", "pms.reservations", "new-work", "purchase_clock.observed_at") : "TRUE";
            const hotels = await rows<{ id: string; name: string; config_json: string }>(db, `${clock}SELECT p.id,p.name,p.config_json FROM pms_properties p${join}
              WHERE p.state IN ('open','closing') AND p.config_json::jsonb->'booking'->>'published'='true' AND ${purchase} ORDER BY p.name LIMIT 200`);
            if (mode === "shadow") for (const hotel of hotels) await pmsPropertyPurchaseAllowed(db, hotel.id, "new-work");
            return hotels;
        },
        readAttachmentPublications1: () => pmsBookingPublications(db, "SELECT jsonb_array_elements(config_json::jsonb->'booking'->'images')->>'publicationId' FROM pms_properties WHERE config_json::jsonb->'booking'->>'published'='true' UNION SELECT image->>'publicationId' FROM pms_properties CROSS JOIN LATERAL jsonb_array_elements(config_json::jsonb->'booking'->'roomContent') room CROSS JOIN LATERAL jsonb_array_elements(room->'images') image WHERE config_json::jsonb->'booking'->>'published'='true'").approved(),
        readPmsBookingPayments6: (facts: {
            fact0: number;
            fact1: number;
        }) => rows<{
            id: string;
        }>(db, "SELECT id FROM pms_booking_payments WHERE status='payment_pending' OR (status='expired' AND checkout_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20", facts.fact0, facts.fact1),
        readPmsBookingPayments7: () => rows<{
            id: string;
        }>(db, "SELECT id FROM pms_booking_payments WHERE id IN (SELECT payment_id FROM pms_booking_refund_requests WHERE state IN ('approved','processing','requires_action')) ORDER BY updated_at LIMIT 20"),
        readPmsRecords3: (facts: {
            number: string;
            email: string;
        }) => rows<{
            id: string;
            property_id: string;
        }>(db, "SELECT r.id,r.property_id FROM pms_records r JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' AND g.property_id=r.property_id AND g.kind='guest' WHERE r.kind='reservation' AND r.data_json::jsonb->>'number'=? AND lower(g.data_json::jsonb->>'email')=lower(?) LIMIT 1", facts.number, facts.email),
        persistPmsRecords: (arg0: Snapshot, arg1: Map<string, string>) => persistPmsRecords(db, arg0, arg1),
        snapshotMoney: (arg0: {
            companyId: string;
        }, arg1: {
            sourceType: string;
            sourceId: `${string}-${string}-${string}-${string}-${string}`;
            amountKind: string;
            transactionDate: string;
            currency: string;
            amountUnits: string;
        }) => snapshotMoney(db, arg0, arg1),
        persistPmsEffects: (arg0: WorkContext, arg1: Snapshot, arg2: Effects) => persistPmsEffects(db, arg0, arg1, arg2),
        processBookingRefund: (arg0: string, arg1: number) => processBookingRefund(requireWhole(whole), arg0, arg1),
        settleBookingChange: (...args) => settleBookingChange(db, ...args),
        publicChangeQuote: (...args) => publicChangeQuote(requireWhole(whole), ...args),
        confirmBookingChange: (...args) => confirmBookingChange(requireWhole(whole), ...args)
    };
}
function repository(db: TransactionalSqlDatabase): PmsBookingRepository<SqlStatement> { return { ...unit(db, db), transaction: (work, purchase) => withPmsPropertyPurchasedTransaction(db, tx => work(unit(tx, db)), purchase) }; }
function requireWhole(db: TransactionalSqlDatabase | undefined): TransactionalSqlDatabase { if (!db)
    throw new Error("A booking command requires the original transaction-capable connection"); return db; }
const application = createPmsBooking<SqlStatement>({ clock: { now: () => Date.now() }, uuid: randomUUID, hash, sign, same, protectMail, unprotectMail, encode: value => Buffer.from(value).toString("base64url"), decode: value => Buffer.from(value, "base64url").toString(), bookingEnv: (...args) => bookingEnv(...args), liveEnabled: () => process.env.PMS_STRIPE_LIVE_ENABLED, require: requirePermission, forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write), error: (status, code, message) => new ApiError(status, code, message), isBusinessError: (error): error is ApiError => error instanceof ApiError, stripeCall: (...args) => stripeCall(...args), verifiedStripePaymentTime: (...args) => verifiedStripePaymentTime(...args), sendMail: fetch, timeout: () => AbortSignal.timeout(10000) });
export function bookingSnapshot(db: SqlDatabase, propertyId: string, search?: BookingSearch, lock = false) { return application.bookingSnapshot(unit(db), propertyId, search, lock); }
export function bookingCatalog(db: SqlDatabase) { return application.bookingCatalog(unit(db)); }
export function publicOffers(db: TransactionalSqlDatabase, input: unknown, propertyId?: string) { return application.publicOffers(repository(db), input, propertyId); }
export function publicQuote(db: TransactionalSqlDatabase, propertyId: string, raw: unknown, env: BookingEnv, now = Date.now()) { return application.publicQuote(repository(db), propertyId, raw, env, now); }
export function persist(db: SqlDatabase, s: Snapshot, before: Map<string, string>) { return application.persist(unit(db), s, before); }
export function queueMail(db: SqlDatabase, s: Snapshot, reservationId: string, slug: string, kind: "confirmation" | "cancellation" | "access" | "change" | "refund", env: BookingEnv, now: number) { return application.queueMail(unit(db), s, reservationId, slug, kind, env, now); }
export function bookingSummary(s: Snapshot, id: string, now: number) { return application.bookingSummary(s, id, now); }
export function createPublicReservation(db: TransactionalSqlDatabase, slug: string, raw: unknown, key: string, env: BookingEnv, now = Date.now()) { return application.createPublicReservation(repository(db), slug, raw, key, env, now); }
export function bookingCheckout(db: TransactionalSqlDatabase, slug: string, id: string, env: BookingEnv, now = Date.now()) { return application.bookingCheckout(repository(db), slug, id, env, now); }
export function bookingPaymentStatus(db: TransactionalSqlDatabase, token: string, env: BookingEnv, now = Date.now()) { return application.bookingPaymentStatus(repository(db), token, env, now); }
export function reconcileBookingPayment(db: TransactionalSqlDatabase, id: string, env: BookingEnv, now = Date.now()) { return application.reconcileBookingPayment(repository(db), id, env, now); }
export function drainBookingPayments(db: TransactionalSqlDatabase, env: BookingEnv, now = Date.now()) { return application.drainBookingPayments(repository(db), env, now); }
export function authorizedBooking(db: SqlDatabase, token: string, now: number, lock = false) { return application.authorizedBooking(unit(db), token, now, lock); }
export function manageBooking(db: TransactionalSqlDatabase, token: string, now = Date.now()) { return application.manageBooking(repository(db), token, now); }
export function cancelPublicReservation(db: TransactionalSqlDatabase, slug: string, token: string, env: BookingEnv, now = Date.now()) { return application.cancelPublicReservation(repository(db), slug, token, env, now); }
export function recoverBooking(db: TransactionalSqlDatabase, slug: string, raw: unknown, env: BookingEnv, now = Date.now()) { return application.recoverBooking(repository(db), slug, raw, env, now); }
export function bookingRateLimit(db: SqlDatabase, key: string, limit = 30, now = Date.now()) { return application.bookingRateLimit(unit(db), key, limit, now); }
export function staffBookingSearch(actor: WorkContext, propertyId: string, operation: "availability" | "quote", input: unknown) { return application.staffBookingSearch(actor, propertyId, operation, input); }
export function bookingError(error: unknown) { return application.bookingError(error); }
export function deliverBookingEmails(db: TransactionalSqlDatabase, env: BookingEnv, send: typeof fetch = fetch, now = Date.now()) { return application.deliverBookingEmails(repository(db), env, send, now); }
export { publicReservationSchema };
export type { BookingEnv, BookingPaymentRow, PaymentAllocation };
export { approveBookingRefund, bookingRefundsForFinance, processBookingRefund, requestBookingRefund } from "../public/pms-booking-refunds";
