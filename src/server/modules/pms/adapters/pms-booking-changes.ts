import 'server-only';
import { createHmac, createHash, timingSafeEqual, randomUUID } from 'node:crypto';
import { type Snapshot } from "@/server/modules/pms/public/rules/pms";
import { authorizedBooking, bookingSummary, bookingSnapshot, persist, queueMail, bookingCheckout, type BookingEnv, type BookingPaymentRow } from "@/server/modules/pms/public/pms-booking";
import { requestBookingRefund } from "@/server/modules/pms/public/pms-booking-refunds";
import { effects, persistPmsRecords } from "@/server/modules/pms/public/pms";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase, TransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import type { Effects } from "@/lib/pms-engine";
import { createPmsBookingChange, type PmsBookingChangeUnit, type PmsBookingChangeRepository, type StaffAuthority, type Proposal } from "../application/pms-booking-changes";
import { holdingResources as resolveResources } from "../../runtime/public/runtime";
const signature = (env: BookingEnv, payload: string) => createHmac('sha256', env.secret).update(`pms-change:${payload}`).digest('base64url');
function signed(p: Proposal, env: BookingEnv) { const payload = Buffer.from(JSON.stringify(p)).toString('base64url'); return `${payload}.${signature(env, payload)}`; }
function read(token: string, env: BookingEnv): Proposal { const [body, sig, extra] = token.split('.'), expected = signature(env, body ?? ''); if (extra || !body || !sig || sig.length !== expected.length || !timingSafeEqual(Buffer.from(sig), Buffer.from(expected)))
    throw new ApiError(422, 'PMS_PROPOSAL_INVALID', 'Review a new proposal'); return JSON.parse(Buffer.from(body, 'base64url').toString()); }
function unit(db: SqlDatabase, whole?: TransactionalSqlDatabase): PmsBookingChangeUnit {
    return {
        readPmsRecords: (facts: {
            reservationId: string;
        }) => db.prepare("SELECT property_id FROM pms_records WHERE id=? AND kind='reservation'").bind(facts.reservationId).first<{
            property_id: string;
        }>(),
        readPmsBookingRefundRequests: (facts: {
            id: string;
        }) => db.prepare('SELECT a.*,COALESCE((SELECT SUM((part->>\'amount\')::bigint) FROM pms_booking_refund_requests r CROSS JOIN LATERAL jsonb_array_elements(r.allocations_json::jsonb) part WHERE r.payment_id=a.payment_id AND part->>\'entryId\'=a.entry_id),0) refunded FROM pms_booking_allocations a WHERE a.stay_id=?').bind(facts.id).all<{
            entry_id: string;
            amount_minor: number;
            refunded: number;
        }>(),
        readPmsBookingRefundRequests2: (facts: {
            stayId: string;
        }) => db.prepare('SELECT a.*,COALESCE((SELECT SUM((part->>\'amount\')::bigint) FROM pms_booking_refund_requests r CROSS JOIN LATERAL jsonb_array_elements(r.allocations_json::jsonb) part WHERE r.payment_id=a.payment_id AND part->>\'entryId\'=a.entry_id),0) refunded FROM pms_booking_allocations a WHERE a.stay_id=? ORDER BY a.created_at,a.id').bind(facts.stayId).all<{
            entry_id: string;
            payment_id: string;
            amount_minor: number;
            refunded: number;
        }>(),
        readWriteContext: () => db.prepare("SELECT set_config('erpx.pms_write','on',true)").bind().first(),
        readPmsBookingChanges: (facts: {
            key: string;
            id: string;
        }) => db.prepare('SELECT request_hash,state FROM pms_booking_changes WHERE id=? AND reservation_id=?').bind(facts.key, facts.id).first<{
            request_hash: string;
            state: string;
        }>(),
        readPmsBookingPayments: (facts: {
            key: string;
        }) => db.prepare('SELECT id FROM pms_booking_payments WHERE proposal_id=?').bind(facts.key).first<{
            id: string;
        }>(),
        readPmsBookingChanges2: (facts: {
            id: string;
            now: number;
        }) => db.prepare("SELECT id FROM pms_booking_changes WHERE reservation_id=? AND state='payment_pending' AND expires_at>? LIMIT 1").bind(facts.id, facts.now).first(),
        updatePmsBookingChanges: (facts: {
            id: string;
            now: number;
        }) => db.prepare("UPDATE pms_booking_changes SET state='expired' WHERE reservation_id=? AND state='payment_pending' AND expires_at<=?").bind(facts.id, facts.now).run(),
        insertPmsBookingChanges: (facts: {
            key: string;
            id: string;
            id2: string;
            digest: string;
            version: number;
            fact5: string;
            fact6: "payment_pending" | "applied";
            expiresAt: number;
            now: number;
        }) => db.prepare('INSERT INTO pms_booking_changes(id,property_id,reservation_id,request_hash,reservation_version,proposal_json,state,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)').bind(facts.key, facts.id, facts.id2, facts.digest, facts.version, facts.fact5, facts.fact6, facts.expiresAt, facts.now).run(),
        readPosConnections: facts => pmsBookingChangeFacts(db).readPosConnections(facts),
        readPosSettings: facts => pmsBookingChangeFacts(db).readPosSettings(facts),
        insertPmsBookingPayments: (facts: {
            attemptId: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            id2: string;
            companyId: string;
            slug: string;
            account_id: string;
            livemode: boolean;
            dueNow: number;
            expiresAt: number;
            now: number;
            now2: number;
            fact11: string;
            fact12: string;
            fact13: unknown;
            key: string;
        }) => db.prepare("INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,status,expires_at,created_at,updated_at,allocation_json,methods_json,clearing_account,purpose,proposal_id) VALUES(?,?,?,?,?,?,?,?,'THB','payment_pending',?,?,?,?,?,?,'change',?)").bind(facts.attemptId, facts.id, facts.id2, facts.companyId, facts.slug, facts.account_id, facts.livemode, facts.dueNow, facts.expiresAt, facts.now, facts.now2, facts.fact11, facts.fact12, facts.fact13, facts.key).run(),
        readPmsBookingChanges3: (facts: {
            proposal_id: string | null;
        }) => db.prepare('SELECT * FROM pms_booking_changes WHERE id=? FOR UPDATE').bind(facts.proposal_id).first<{
            id: string;
            state: string;
            proposal_json: string;
            reservation_version: number;
        }>(),
        insertPmsBookingAllocations: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            stayId: string;
            guestAccountId: string;
            id2: string;
            amount: number;
            now: number;
        }) => db.prepare('INSERT INTO pms_booking_allocations(id,payment_id,stay_id,guest_account_id,entry_id,amount_minor,created_at) VALUES(?,?,?,?,?,?,?)').bind(facts.fact0, facts.id, facts.stayId, facts.guestAccountId, facts.id2, facts.amount, facts.now).run(),
        insertPmsBookingRefundRequests: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            amount_minor: number;
            now: number;
            now2: number;
        }) => db.prepare("INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,'[]','Change payment could not be assigned','system:stripe','requested',?,?)").bind(facts.fact0, facts.id, facts.amount_minor, facts.now, facts.now2).run(),
        updatePmsBookingChanges2: (facts: {
            fact0: "expired" | "applied";
            id: string;
        }) => db.prepare('UPDATE pms_booking_changes SET state=? WHERE id=?').bind(facts.fact0, facts.id).run(),
        updatePmsBookingPayments: (facts: {
            fact0: "confirmed" | "expired" | "refund_review";
            fact1: "requested" | null;
            now: number;
            id: string;
        }) => db.prepare('UPDATE pms_booking_payments SET status=?,refund_state=?,updated_at=? WHERE id=?').bind(facts.fact0, facts.fact1, facts.now, facts.id).run(),
        authorizedBooking: (arg0: string, arg1: number, arg2: boolean) => authorizedBooking(db, arg0, arg1, arg2),
        bookingSnapshot: (arg0: string, arg1: undefined, arg2: boolean) => bookingSnapshot(db, arg0, arg1, arg2),
        assertProductionModuleEnabled: (arg0: string, arg1: "pms") => assertProductionModuleEnabled(db, arg0, arg1),
        persistPmsRecords: (arg0: Snapshot, arg1: Map<string, string>) => persistPmsRecords(db, arg0, arg1),
        effects: (arg0: WorkContext, arg1: Snapshot, arg2: Effects) => effects(db, arg0, arg1, arg2),
        requestBookingRefund: (arg0: Snapshot, arg1: string, arg2: {
            stayId: string;
            amount: number;
        }[], arg3: string, arg4: string, arg5: number) => requestBookingRefund(db, arg0, arg1, arg2, arg3, arg4, arg5),
        queueMail: (arg0: Snapshot, arg1: string, arg2: string, arg3: "change" | "cancellation", arg4: BookingEnv, arg5: number) => queueMail(db, arg0, arg1, arg2, arg3, arg4, arg5),
        persist: (arg0: Snapshot, arg1: Map<string, string>) => persist(db, arg0, arg1),
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
        bookingCheckout: (arg0: string, arg1: string, arg2: BookingEnv, arg3: number) => bookingCheckout(requireWhole(whole), arg0, arg1, arg2, arg3)
    };
}
function repository(db: TransactionalSqlDatabase): PmsBookingChangeRepository { return { ...unit(db, db), transaction: work => db.transaction(tx => work(unit(tx, db))) }; }
const application = createPmsBookingChange({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), liveEnabled: () => process.env.PMS_STRIPE_LIVE_ENABLED, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => unit((await resolveResources(actor.holdingId)).write), bookingSummary: (...args) => bookingSummary(...args), signed, read });
export function publicChangeQuote(db: TransactionalSqlDatabase, token: string, kind: 'change' | 'cancel', raw: unknown, env: BookingEnv, now = Date.now(), staff?: StaffAuthority) { return application.publicChangeQuote(repository(db), token, kind, raw, env, now, staff); }
export function confirmBookingChange(db: TransactionalSqlDatabase, slug: string, token: string, proposalToken: string, key: string, env: BookingEnv, now = Date.now(), staff?: StaffAuthority, expectedKind?: 'change' | 'cancel') { return application.confirmBookingChange(repository(db), slug, token, proposalToken, key, env, now, staff, expectedKind); }
export function settleBookingChange(tx: SqlDatabase, s: Snapshot, payment: BookingPaymentRow, result: {
    paid: boolean;
    providerSucceededAt: number | null;
    now: number;
    env: BookingEnv;
}) { return application.settleBookingChange(unit(tx, undefined), s, payment, result); }
import { pmsBookingChangeFacts } from "../../pos/public/pms-booking-change-facts";
function requireWhole(db: TransactionalSqlDatabase | undefined): TransactionalSqlDatabase { if (!db)
    throw new Error("A booking checkout requires the owning transaction-capable connection"); return db; }
