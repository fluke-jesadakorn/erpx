import 'server-only';
import { PMS_WRITE_CONTEXT_SQL } from "./pms-read-tail-r17-sql";
import { createHmac, createHash, timingSafeEqual, randomUUID } from 'node:crypto';
import { type Snapshot } from "@erpx/server-pms/public/rules/pms";
import { authorizedBooking, bookingSummary, bookingSnapshot, persist, queueMail, bookingCheckout, type BookingEnv, type BookingPaymentRow } from "@erpx/server-pms/public/pms-booking";
import { requestBookingRefund } from "@erpx/server-pms/public/pms-booking-refunds";
import { effects, persistPmsRecords } from "../../../../packages/server-pms/src/public/pms";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, TransactionalSqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { Effects } from "@erpx/feature-pms/lib/pms-engine";
import { createPmsBookingChange, type PmsBookingChangeUnit, type PmsBookingChangeRepository, type StaffAuthority, type Proposal } from "@erpx/server-pms/application/pms-booking-changes";
import { holdingResources as resolveResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePmsPropertyPurchase, withPmsPropertyPurchasedTransaction } from "@erpx/server-pms/adapters/property-purchase-boundary";
import { nativePmsReadFirst, PMS_RD_BOOKING_CHANGE, PMS_RD_BOOKING_CHANGE_PENDING, PMS_RD_BOOKING_PAYMENT } from "./native-pms-read";
const signature = (env: BookingEnv, payload: string) => createHmac('sha256', env.secret).update(`pms-change:${payload}`).digest('base64url');
function signed(p: Proposal, env: BookingEnv) { const payload = Buffer.from(JSON.stringify(p)).toString('base64url'); return `${payload}.${signature(env, payload)}`; }
function read(token: string, env: BookingEnv): Proposal { const [body, sig, extra] = token.split("."), expected = signature(env, body ?? ''); if (extra || !body || !sig || sig.length !== expected.length || !timingSafeEqual(Buffer.from(sig), Buffer.from(expected)))
    throw new ApiError(422, 'PMS_PROPOSAL_INVALID', 'Review a new proposal'); return JSON.parse(Buffer.from(body, 'base64url').toString()); }
function unit(db: SqlDatabase, whole?: TransactionalSqlDatabase, companyId = ''): PmsBookingChangeUnit {
        const write = (kind: string, values: unknown[]): any => nativePmsOwnerWrite(companyId, kind, values);
    return {
        requirePurchase: input => requirePmsPropertyPurchase(db, input),
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
        readWriteContext: () => db.prepare(PMS_WRITE_CONTEXT_SQL).bind().first(),
        readPmsBookingChanges: (facts: {
            key: string;
            id: string;
        }) => nativePmsReadFirst<{
            request_hash: string;
            state: string;
        }>(PMS_RD_BOOKING_CHANGE, [facts.key, facts.id], db),
        readPmsBookingPayments: (facts: {
            key: string;
        }) => nativePmsReadFirst<{
            id: string;
        }>(PMS_RD_BOOKING_PAYMENT, [facts.key], db),
        readPmsBookingChanges2: (facts: {
            id: string;
            now: number;
        }) => nativePmsReadFirst(PMS_RD_BOOKING_CHANGE_PENDING, [facts.id, facts.now], db),
        updatePmsBookingChanges: (facts: {
            id: string;
            now: number;
        }) => write('pms_w002', [facts.id, facts.now]),
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
        }) => write('pms_w003', [facts.key, facts.id, facts.id2, facts.digest, facts.version, facts.fact5, facts.fact6, facts.expiresAt, facts.now]),
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
        }) => write('pms_w004', [facts.attemptId, facts.id, facts.id2, facts.companyId, facts.slug, facts.account_id, facts.livemode, facts.dueNow, facts.expiresAt, facts.now, facts.now2, facts.fact11, facts.fact12, facts.fact13, facts.key]),
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
        }) => write('pms_w005', [facts.fact0, facts.id, facts.stayId, facts.guestAccountId, facts.id2, facts.amount, facts.now]),
        insertPmsBookingRefundRequests: (facts: {
            fact0: `${string}-${string}-${string}-${string}-${string}`;
            id: string;
            amount_minor: number;
            now: number;
            now2: number;
        }) => write('pms_w006', [facts.fact0, facts.id, facts.amount_minor, facts.now, facts.now2]),
        updatePmsBookingChanges2: (facts: {
            fact0: "expired" | "applied";
            id: string;
        }) => write('pms_w007', [facts.fact0, facts.id]),
        updatePmsBookingPayments: (facts: {
            fact0: "confirmed" | "expired" | "refund_review";
            fact1: "requested" | null;
            now: number;
            id: string;
        }) => write('pms_w008', [facts.fact0, facts.fact1, facts.now, facts.id]),
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
function repository(db: TransactionalSqlDatabase, companyId = ''): PmsBookingChangeRepository { return { ...unit(db, db, companyId), transaction: (work, purchase) => withPmsPropertyPurchasedTransaction(db, tx => work(unit(tx, db, companyId)), purchase) }; }
const application = createPmsBookingChange({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), liveEnabled: () => process.env.PMS_STRIPE_LIVE_ENABLED, require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => unit((await resolveResources(actor.holdingId)).write), bookingSummary: (...args) => bookingSummary(...args), signed, read });
export function publicChangeQuote(db: TransactionalSqlDatabase, token: string, kind: 'change' | 'cancel', raw: unknown, env: BookingEnv, now = Date.now(), staff?: StaffAuthority) { return application.publicChangeQuote(repository(db, staff?.actor.companyId ?? ''), token, kind, raw, env, now, staff); }
export function confirmBookingChange(db: TransactionalSqlDatabase, slug: string, token: string, proposalToken: string, key: string, env: BookingEnv, now = Date.now(), staff?: StaffAuthority, expectedKind?: 'change' | 'cancel') { return application.confirmBookingChange(repository(db, staff?.actor.companyId ?? ''), slug, token, proposalToken, key, env, now, staff, expectedKind); }
export function settleBookingChange(tx: SqlDatabase, s: Snapshot, payment: BookingPaymentRow, result: {
    paid: boolean;
    providerSucceededAt: number | null;
    now: number;
    env: BookingEnv;
}) { return application.settleBookingChange(unit(tx, undefined), s, payment, result); }
import { pmsBookingChangeFacts } from "@erpx/server-pos/public/pms-booking-change-facts";
import { nativePmsOwnerWrite } from "@erpx/feature-pms/public/native-pms";
function requireWhole(db: TransactionalSqlDatabase | undefined): TransactionalSqlDatabase { if (!db)
    throw new Error("A booking checkout requires the owning transaction-capable connection"); return db; }
