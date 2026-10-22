import { stayGroupBlock } from "@erpx/feature-pms/lib/pms-inventory-domain";
import { parseGuestAccountJson } from "@erpx/feature-identity_access/lib/guest-account-compat";
import { z } from 'zod';
import { record, records, dates, plusDays, type Snapshot } from "@erpx/server-pms/domain/pms";
import { bookingQuote, roomSelectionSchema, type RoomSelection, type BookingQuote } from "@erpx/feature-pms/lib/pms-booking-domain";
import { availability } from "@erpx/feature-pms/lib/pms-inventory-domain";
import { availableReceiptSources } from "@erpx/feature-pms/lib/pms-receipt-funding";
import { bookingRevisionPurchasePurpose, type PmsPropertyPurchase, type PmsPropertyPurchaseRequest } from "@erpx/server-pms/domain/property-purchases";
import { executePms } from "@erpx/feature-pms/lib/pms-engine";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Effects } from "@erpx/feature-pms/lib/pms-engine";
import type { BookingPolicy, NightPrice } from "@erpx/server-pms/domain/pms-model";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { BookingPaymentRow, BookingEnv } from "@erpx/server-pms/public/pms-booking";
export type PmsProviderRows<Row> = {
    results: Row[];
};
export type PmsProviderWrite = {
    run(): Promise<PmsProviderRows<Record<string, unknown>>>;
};
export type PmsBookingChangeUnit = {
    requirePurchase(input: PmsPropertyPurchase): Promise<void>;
    readPmsRecords(facts: {
        reservationId: string;
    }): Promise<{
        property_id: string;
    } | null>;
    readPmsBookingRefundRequests(facts: {
        id: string;
    }): Promise<PmsProviderRows<{
        entry_id: string;
        amount_minor: number;
        refunded: number;
    }>>;
    readPmsBookingRefundRequests2(facts: {
        stayId: string;
    }): Promise<PmsProviderRows<{
        entry_id: string;
        payment_id: string;
        amount_minor: number;
        refunded: number;
    }>>;
    readWriteContext(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsBookingChanges(facts: {
        key: string;
        id: string;
    }): Promise<{
        request_hash: string;
        state: string;
    } | null>;
    readPmsBookingPayments(facts: {
        key: string;
    }): Promise<{
        id: string;
    } | null>;
    readPmsBookingChanges2(facts: {
        id: string;
        now: number;
    }): Promise<Record<string, unknown> | null>;
    updatePmsBookingChanges(facts: {
        id: string;
        now: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    insertPmsBookingChanges(facts: {
        key: string;
        id: string;
        id2: string;
        digest: string;
        version: number;
        fact5: string;
        fact6: "payment_pending" | "applied";
        expiresAt: number;
        now: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPosConnections(facts: {
        companyId: string;
    }): Promise<{
        account_id: string;
        livemode: boolean;
        card_enabled: boolean;
        promptpay_enabled: boolean;
    } | null>;
    readPosSettings(facts: {
        companyId: string;
    }): Promise<{
        accounts_json: string;
    } | null>;
    insertPmsBookingPayments(facts: {
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
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    readPmsBookingChanges3(facts: {
        proposal_id: string | null;
    }): Promise<{
        id: string;
        state: string;
        proposal_json: string;
        reservation_version: number;
    } | null>;
    insertPmsBookingAllocations(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        id: string;
        stayId: string;
        guestAccountId: string;
        id2: string;
        amount: number;
        now: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    insertPmsBookingRefundRequests(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        id: string;
        amount_minor: number;
        now: number;
        now2: number;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingChanges2(facts: {
        fact0: "expired" | "applied";
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    updatePmsBookingPayments(facts: {
        fact0: "confirmed" | "expired" | "refund_review";
        fact1: "requested" | null;
        now: number;
        id: string;
    }): Promise<PmsProviderRows<Record<string, unknown>>>;
    authorizedBooking(arg0: string, arg1: number, arg2: boolean): Promise<{
        s: Snapshot;
        id: string;
    }>;
    bookingSnapshot(arg0: string, arg1: undefined, arg2: boolean): Promise<Snapshot>;
    assertProductionModuleEnabled(arg0: string, arg1: "pms"): Promise<void>;
    persistPmsRecords(arg0: Snapshot, arg1: Map<string, string>): Promise<void>;
    effects(arg0: WorkContext, arg1: Snapshot, arg2: Effects): Promise<void>;
    requestBookingRefund(arg0: Snapshot, arg1: string, arg2: {
        stayId: string;
        amount: number;
    }[], arg3: string, arg4: string, arg5: number): Promise<{
        id: string;
        payment_id: string;
        amount_minor: number;
        allocations_json: string;
        reason: string;
        requested_by: string;
        approved_by: string | null;
        provider_id: string | null;
        state: string;
        generation: number;
        legacy_review?: boolean;
    } | {
        id: `${string}-${string}-${string}-${string}-${string}`;
        state: string;
        amount_minor: number;
    }>;
    queueMail(arg0: Snapshot, arg1: string, arg2: string, arg3: "change" | "cancellation", arg4: BookingEnv, arg5: number): Promise<string>;
    persist(arg0: Snapshot, arg1: Map<string, string>): Promise<void>;
    snapshotMoney(arg0: {
        companyId: string;
    }, arg1: {
        sourceType: string;
        sourceId: `${string}-${string}-${string}-${string}-${string}`;
        amountKind: string;
        transactionDate: string;
        currency: string;
        amountUnits: string;
    }): Promise<{
        amountUnits: string;
        usdUnits: string;
        currencyScale: number;
        rateId: string | null;
        usedLatestFallback: boolean;
        statement: PmsProviderWrite;
    }>;
    bookingCheckout(arg0: string, arg1: string, arg2: BookingEnv, arg3: number): Promise<{
        state: "cancelled" | "payment_pending" | "confirmed" | "expired" | "refund_review";
        refundState: string | null;
        checkoutUrl: string | null;
        attemptToken: string;
        expiresAt: number;
        managementToken: unknown;
        postedRoomTotal: number;
        id: string;
        number: string;
        hotel: {
            id: string;
            name: string;
            address: string;
            contactEmail: string;
            contactPhone: string;
        };
        guest: {
            name: string;
            email: string;
        };
        paymentState: "received" | "pay_at_hotel";
        currency: string;
        version: number;
        history: {
            at: number;
            actor: string;
            action: string;
        }[];
        total: number;
        advance: number;
        guestAccountDue: number;
        policy: BookingPolicy | undefined;
        rooms: {
            id: string;
            typeId: string;
            rateId: string;
            guestAccountId: string;
            policy: BookingPolicy | undefined;
            roomName: string;
            arrival: string;
            departure: string;
            adults: number;
            childAges: number[];
            state: "reserved" | "in_house" | "cancelled" | "no_show" | "checked_out";
            nights: NightPrice[];
        }[];
        cancellation: {
            allowed: boolean;
            reason: string;
        };
    } | {
        state: string;
        checkoutUrl: string | null;
        attemptToken: string;
        expiresAt: number;
        postedRoomTotal: number;
        id: string;
        number: string;
        hotel: {
            id: string;
            name: string;
            address: string;
            contactEmail: string;
            contactPhone: string;
        };
        guest: {
            name: string;
            email: string;
        };
        paymentState: "received" | "pay_at_hotel";
        currency: string;
        version: number;
        history: {
            at: number;
            actor: string;
            action: string;
        }[];
        total: number;
        advance: number;
        guestAccountDue: number;
        policy: BookingPolicy | undefined;
        rooms: {
            id: string;
            typeId: string;
            rateId: string;
            guestAccountId: string;
            policy: BookingPolicy | undefined;
            roomName: string;
            arrival: string;
            departure: string;
            adults: number;
            childAges: number[];
            state: "reserved" | "in_house" | "cancelled" | "no_show" | "checked_out";
            nights: NightPrice[];
        }[];
        cancellation: {
            allowed: boolean;
            reason: string;
        };
    }>;
};
export type PmsBookingChangeRepository = PmsBookingChangeUnit & {
    transaction<Result>(work: (unit: PmsBookingChangeUnit) => Promise<Result>, purchase?: PmsPropertyPurchaseRequest): Promise<Result>;
};
export type PmsBookingChangePorts = {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    liveEnabled(): string | undefined;
    require(actor: WorkContext, permission: string): void;
    error(status: number, code: string, message: string): Error;
    forActor(actor: WorkContext): Promise<PmsBookingChangeUnit>;
    bookingSummary: typeof import("@erpx/server-pms/public/pms-booking").bookingSummary;
    signed(proposal: Proposal, env: BookingEnv): string;
    read(token: string, env: BookingEnv): Proposal;
};
export type StaffAuthority = {
    actor: WorkContext;
    reservationId: string;
    reason: string;
    override: boolean;
};
export type Proposal = {
    staff?: {
        userId: string;
        reason: string;
        override: boolean;
    };
    reservationId: string;
    version: number;
    kind: 'change' | 'cancel';
    items: Array<{
        stayId: string;
        room?: RoomSelection;
        quote?: BookingQuote;
        oldTotal: number;
        newTotal: number;
        dueNow: number;
        refund: number;
        fee: number;
        dueAtHotel: number;
        manualFunding?: Array<{ entryId: string; amount: number }>;
        manualRefund?: number;
    }>;
    expiresAt: number;
    dueNow: number;
    refund: number;
    fee: number;
};
export function createPmsBookingChange(ports: PmsBookingChangePorts) {
    const actor = (now: number) => ({ userId: 'guest:self-service', now, uuid: ports.uuid, independent: () => false });
    const providerActor = (now: number) => ({ ...actor(now), userId: 'system:stripe' });
    function eligible(s: Snapshot, id: string, now: number, change: boolean, override = false) {
        const stay = record(s, 'stay', id), reservation = record(s, 'reservation', stay.data.reservationId), policy = stay.data.bookingPolicy ?? reservation.data.bookingPolicy;
        if (!override && policy && now >= policy.deadline)
            throw ports.error(409, 'PMS_CONTACT_HOTEL', 'The free-cancellation deadline has passed. Contact the hotel');
        if (s.property.state !== 'open' || stay.data.state !== 'reserved' || reservation.data.state !== 'confirmed' || !policy || !override && (now >= policy.deadline || change && policy.collection?.changesAllowed === false || !change && policy.collection?.cancellationsAllowed === false))
            throw ports.error(409, 'PMS_CONTACT_HOTEL', 'This room can no longer be changed online. Contact the hotel');
        if (records(s, 'entry').some(e => e.data.guestAccountId === stay.data.guestAccountId && (['charge', 'adjustment'].includes(e.data.kind) || e.data.kind === 'allocation' && e.data.description !== 'payment.refund_liability')))
            throw ports.error(409, 'PMS_CONTACT_HOTEL', 'This room has financial activity. Contact the hotel');
        return stay;
    }
    async function bookingAuthority(tx: PmsBookingChangeUnit, token: string, now: number, lock: boolean, staff?: StaffAuthority) {
        if (!staff)
            return tx.authorizedBooking(token, now, lock);
        ports.require(staff.actor, 'pms.reservations');
        if (staff.override)
            ports.require(staff.actor, 'pms.approve');
        if (staff.actor.beta || staff.actor.departmentId || staff.reason.trim().length < 3)
            throw ports.error(422, 'PMS_REVISION_REASON', 'Use an authorized hotel workspace and enter a change reason');
        const row = await tx.readPmsRecords({ reservationId: staff.reservationId });
        if (!row)
            throw ports.error(404, 'PMS_BOOKING_NOT_FOUND', 'Booking not found');
        const s = await tx.bookingSnapshot(row.property_id, undefined, lock);
        if (s.property.companyId !== staff.actor.companyId || staff.actor.branchId && s.property.branchId !== staff.actor.branchId)
            throw ports.error(404, 'PMS_BOOKING_NOT_FOUND', 'Booking not found in this workspace');
        await tx.assertProductionModuleEnabled(staff.actor.companyId, 'pms');
        return { s, id: staff.reservationId };
    }
    async function currentManualFunding(tx: PmsBookingChangeUnit, s: Snapshot, stayId: string) {
        const stay = record(s, 'stay', stayId);
        const onlineIds = new Set((await tx.readPmsBookingRefundRequests2({ stayId })).results.map(row => row.entry_id));
        return availableReceiptSources(s, stay.data.guestAccountId, 'advance').filter(source => !onlineIds.has(source.entryId));
    }
    async function assertManualFunding(tx: PmsBookingChangeUnit, s: Snapshot, proposal: Proposal) {
        for (const item of proposal.items) {
            if (item.manualFunding !== undefined && JSON.stringify(await currentManualFunding(tx, s, item.stayId)) !== JSON.stringify(item.manualFunding))
                throw ports.error(409, 'PMS_REFUND_FUNDING_CHANGED', 'Room advance changed. Review the payment difference again');
        }
    }
    async function publicChangeQuote(db: PmsBookingChangeRepository, token: string, kind: 'change' | 'cancel', raw: unknown, env: BookingEnv, now = ports.clock.now(), staff?: StaffAuthority) {
        const input = z.object({ items: z.array(z.object({ stayId: z.string().min(1), room: roomSelectionSchema.optional() })).min(1).max(20) }).parse(raw);
        if (new Set(input.items.map(i => i.stayId)).size !== input.items.length)
            throw ports.error(422, 'PMS_DUPLICATE_ROOM', 'Choose each room once');
        return db.transaction(async (tx) => {
            const { s, id } = await bookingAuthority(tx, token, now, false, staff);
            s.asOf = now;
            const trial = structuredClone(s);
            for (const item of input.items) {
                const st = eligible(s, item.stayId, now, kind === 'change', staff?.override);
                if (st.data.reservationId !== id)
                    throw ports.error(404, 'PMS_STAY_NOT_FOUND', 'Room does not belong to this booking');
                record(trial, 'stay', st.id).data.state = 'cancelled';
            }
            const items: Proposal['items'] = [];
            for (const item of input.items) {
                const st = record(s, 'stay', item.stayId), oldTotal = st.data.prices.reduce((n, p) => n + p.total, 0);
                const sourceRows = (await tx.readPmsBookingRefundRequests({ id: st.id })).results;
                const retained = sourceRows.reduce((n, p) => n + Number(p.amount_minor) - Number(p.refunded), 0);
                let quote: BookingQuote | undefined;
                if (kind === 'change') {
                    const earliest = staff ? s.property.businessDate : new Intl.DateTimeFormat('en-CA', { timeZone: s.property.config.timezone }).format(now);
                    if (!item.room || item.room.quantity !== 1 || item.room.arrival < earliest ||
                        !staff && (!s.property.config.booking?.rateIds.includes(item.room.rateId) || record(s, 'rate', item.room.rateId).data.market !== 'public'))
                        throw ports.error(422, 'PMS_CHANGE_INPUT', 'Choose one available room with current dates and authorized rates');
                    quote = bookingQuote(trial, [item.room], '', st.id);
                    // Add each proposed room to the trial so multi-room proposals cannot oversell.
                    Object.assign(record(trial, 'stay', st.id).data, { ...item.room, state: 'reserved', roomTypePeriods: undefined, prices: quote.lines[0].nights });
                }
                const accepted = st.data.bookingPolicy ?? record(s, 'reservation', id).data.bookingPolicy;
                const fee = kind === 'cancel' ? Number((BigInt(oldTotal) * BigInt(accepted?.collection?.cancellationFeePercent ?? 0) + BigInt(99)) / BigInt(100)) : 0;
                if (fee && !s.property.config.bookingFees?.approvedBy)
                    throw ports.error(409, 'PMS_FEE_ACCOUNT_REQUIRED', 'Contact the hotel to complete cancellation accounting setup');
                const target = quote?.dueNow ?? fee, newTotal = quote?.total ?? fee;
                const onlineIds = new Set(sourceRows.map(source => source.entry_id));
                const manualFunding = staff ? availableReceiptSources(s, st.data.guestAccountId, 'advance').filter(source => !onlineIds.has(source.entryId)) : undefined;
                const manual = manualFunding?.reduce((sum, source) => sum + source.amount, 0) ?? 0;
                if (manual && records(s, 'stay').filter(row => row.data.guestAccountId === st.data.guestAccountId && ['reserved', 'in_house'].includes(row.data.state)).length > 1)
                    throw ports.error(409, 'PMS_GUEST_ACCOUNT_ROUTING_REQUIRED', 'Review the manual advance on the shared Guest Account before changing this room');
                const manualRefund = Math.max(0, manual - newTotal), onlineTarget = Math.max(0, target - Math.min(manual, newTotal));
                if (manualRefund && retained)
                    throw ports.error(409, 'PMS_REFUND_FUNDING_CHANGED', 'Review mixed manual and online refund sources in the Guest Account first');
                items.push({ stayId: st.id, room: kind === 'change' ? item.room : undefined, quote, oldTotal, newTotal, fee,
                    dueAtHotel: kind === 'cancel' ? Math.max(0, fee - retained - manual) : Math.max(0, newTotal - Math.min(manual, newTotal) - onlineTarget),
                    dueNow: kind === 'cancel' ? 0 : Math.max(0, onlineTarget - retained), refund: Math.max(0, retained - onlineTarget) + manualRefund,
                    ...(staff ? { manualFunding, manualRefund } : {}),
                });
            }
            const proposal: Proposal = { ...(staff ? { staff: { userId: staff.actor.userId, reason: staff.reason.trim(), override: staff.override } } : {}), reservationId: id, version: record(s, 'reservation', id).version, kind, items, expiresAt: now + 15 * 60 * 1000, dueNow: items.reduce((n, p) => n + p.dueNow, 0), refund: items.reduce((n, p) => n + p.refund, 0), fee: items.reduce((n, p) => n + p.fee, 0) };
            await tx.requirePurchase({ propertyId: s.property.id, purpose: bookingRevisionPurchasePurpose(s, kind, items) });
            return { proposal, token: ports.signed(proposal, env) };
        }, async () => {
            const { s } = await bookingAuthority(db, token, now, false, staff);
            return { propertyId: s.property.id, purpose: bookingRevisionPurchasePurpose(s, kind, input.items), readOnly: true,
              ...(staff ? { actor: staff.actor, permission: staff.override ? "pms.approve" as const : "pms.reservations" as const } : {}) };
        });
    }
    function releaseHolds(s: Snapshot, id: string, now: number) { for (const b of records(s, 'block').filter(b => b.data.bookingChangeId === id && b.data.state === 'active')) {
        b.data.state = 'released';
        b.version++;
        b.updatedAt = now;
    } }
    async function apply(tx: PmsBookingChangeUnit, s: Snapshot, proposal: Proposal, slug: string, env: BookingEnv, now: number, acceptedAt = now, original = new Map(s.records.map(r => [r.id, JSON.stringify(r)]))) {
        for (const item of proposal.items)
            eligible(s, item.stayId, acceptedAt, proposal.kind === 'change', proposal.staff?.override);
        await assertManualFunding(tx, s, proposal);
        // Free every selected original before applying swaps, then persist only the final state.
        if (proposal.kind === 'change')
            for (const item of proposal.items)
                record(s, 'stay', item.stayId).data.state = 'cancelled';
        for (const item of proposal.items) {
            if (proposal.kind === 'change')
                record(s, 'stay', item.stayId).data.state = 'reserved';
            if (item.fee) {
                const before = original, result = executePms(s, 'booking.cancellation_fee', { stayId: item.stayId, amount: item.fee, reference: proposal.version.toString() }, actor(now));
                await tx.persistPmsRecords(s, before);
                await tx.effects({ companyId: s.property.companyId, userId: proposal.staff?.userId ?? 'guest:self-service' } as WorkContext, s, result.effects);
            }
            if (proposal.kind === 'cancel')
                executePms(s, 'stay.cancel', { id: item.stayId, reason: proposal.staff?.reason ?? 'Guest cancelled online within accepted terms', fee: 0 }, actor(now));
            else
                executePms(s, 'stay.replace', { id: item.stayId, room: item.room, policy: item.quote!.lines[0].policy, nightPrices: item.quote!.lines[0].nights, reason: proposal.staff?.reason ?? 'Guest changed booking online' }, actor(now));
        }
        for (const item of proposal.items) {
            let refund = item.refund - (item.manualRefund ?? 0);
            if (refund) {
                const sources = (await tx.readPmsBookingRefundRequests2({ stayId: item.stayId })).results;
                for (const source of sources) {
                    const n = Math.min(refund, Number(source.amount_minor) - Number(source.refunded), availableReceiptSources(s, record(s, 'stay', item.stayId).data.guestAccountId, 'advance').find(p => p.entryId === source.entry_id)?.amount ?? 0);
                    if (n > 0) {
                        await tx.requestBookingRefund(s, source.payment_id, [{ stayId: item.stayId, amount: n }], proposal.staff?.reason ?? 'Guest booking revision', proposal.staff?.userId ?? 'guest:self-service', now);
                        refund -= n;
                    }
                }
                if (refund)
                    throw ports.error(409, 'PMS_REFUND_FUNDING_CHANGED', 'Original refund sources changed');
            }
        }
        for (const item of proposal.items) {
            if (item.manualRefund) {
                executePms(s, 'refund.request', { guestAccountId: record(s, 'stay', item.stayId).data.guestAccountId,
                    amount: item.manualRefund, pocket: 'advance', reason: proposal.staff!.reason },
                    { ...actor(now), userId: proposal.staff!.userId });
            }
        }
        const reservation = record(s, 'reservation', proposal.reservationId);
        reservation.data.history = [...(reservation.data.history ?? []), { at: now, actor: proposal.staff?.userId ?? 'guest:self-service', action: `${proposal.staff ? proposal.staff.reason + '; ' : ''}${proposal.staff?.override ? 'Policy override; ' : ''}${proposal.kind === 'cancel' ? 'Cancelled' : 'Changed'} ${proposal.items.length} room(s); additional payment ${proposal.dueNow}; refund requested ${proposal.refund}` }];
        reservation.version++;
        reservation.updatedAt = now;
        await tx.queueMail(s, reservation.id, slug, proposal.kind === 'cancel' ? 'cancellation' : 'change', env, now);
    }
    async function confirmBookingChange(db: PmsBookingChangeRepository, slug: string, token: string, proposalToken: string, key: string, env: BookingEnv, now = ports.clock.now(), staff?: StaffAuthority, expectedKind?: 'change' | 'cancel') {
        z.string().uuid().parse(key);
        const proposal = ports.read(proposalToken, env), digest = ports.hash(proposalToken);
        if (expectedKind && proposal.kind !== expectedKind)
            throw ports.error(422, 'PMS_PROPOSAL_KIND', 'Review the selected booking action');
        if ((proposal.staff?.userId ?? null) !== (staff?.actor.userId ?? null) || proposal.staff && (!staff || proposal.staff.reason !== staff.reason.trim() || proposal.staff.override !== staff.override))
            throw ports.error(403, 'PMS_PROPOSAL_AUTHORITY', 'Review the proposal using the same authorized person');
        const result = await db.transaction(async (tx) => {
            const { s, id } = await bookingAuthority(tx, token, now, true, staff);
            s.asOf = now;
            await tx.readWriteContext({});
            if (id !== proposal.reservationId)
                throw ports.error(404, 'PMS_PROPOSAL_INVALID', 'Proposal belongs to another booking');
            const prior = await tx.readPmsBookingChanges({ key: key, id: id });
            if (prior) {
                if (prior.request_hash !== digest)
                    throw ports.error(409, 'PMS_KEY_REUSED', 'Use a new request key');
                const p = await tx.readPmsBookingPayments({ key: key });
                return { summary: ports.bookingSummary(s, id, now), attemptId: prior.state === 'payment_pending' ? p?.id : undefined };
            }
            if (proposal.expiresAt <= now || record(s, 'reservation', id).version !== proposal.version)
                throw ports.error(409, 'PMS_PROPOSAL_STALE', 'Booking or quote changed. Review a new proposal');
            if (await tx.readPmsBookingChanges2({ id: id, now: now }))
                throw ports.error(409, 'PMS_CHANGE_PENDING', 'Complete the pending change first');
            await tx.updatePmsBookingChanges({ id: id, now: now });
            for (const item of proposal.items)
                eligible(s, item.stayId, now, proposal.kind === 'change', proposal.staff?.override);
            await assertManualFunding(tx, s, proposal);
            // Revalidate availability and price under the same Property lock as staff bookings.
            const trial = structuredClone(s);
            for (const item of proposal.items)
                record(trial, 'stay', item.stayId).data.state = 'cancelled';
            for (const item of proposal.items)
                if (item.room && proposal.kind === 'change') {
                    const q = bookingQuote(trial, [item.room], '', item.stayId);
                    if (JSON.stringify(q) !== JSON.stringify(item.quote))
                        throw ports.error(409, 'PMS_QUOTE_CHANGED', 'Price or terms changed. Review again');
                    Object.assign(record(trial, 'stay', item.stayId).data, { ...item.room, state: 'reserved', roomTypePeriods: undefined, prices: q.lines[0].nights });
                }
            const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)]));
            const expiresAt = now + 30 * 60 * 1000;
            await tx.insertPmsBookingChanges({ key: key, id: s.property.id, id2: id, digest: digest, version: proposal.version, fact5: JSON.stringify(proposal), fact6: proposal.dueNow ? 'payment_pending' : 'applied', expiresAt: expiresAt, now: now });
            if (!proposal.dueNow) {
                await apply(tx, s, proposal, slug, env, now, now, before);
                await tx.persist(s, before);
                return { summary: ports.bookingSummary(s, id, now) };
            }
            const connection = await tx.readPosConnections({ companyId: s.property.companyId });
            if (!connection || !connection.card_enabled && !connection.promptpay_enabled || connection.livemode && ports.liveEnabled() !== 'true' || !s.property.config.onlineAccounting?.approvedBy)
                throw ports.error(409, 'PMS_PAYMENT_UNAVAILABLE', 'Online payment is unavailable');
            const delta = new Map<string, number>();
            for (const item of proposal.items) {
                const old = record(s, 'stay', item.stayId).data;
                for (const d of dates(old.arrival, old.departure)) {
                    const k = `${old.typeId}|${d}`;
                    delta.set(k, (delta.get(k) ?? 0) - 1);
                }
                if (item.room)
                    for (const d of dates(item.room.arrival, item.room.departure)) {
                        const k = `${item.room.typeId}|${d}`;
                        delta.set(k, (delta.get(k) ?? 0) + 1);
                    }
            }
            const claimsByNight = new Map<string, Array<{ blockId: string; stayId: string }>>();
            for (const item of proposal.items) {
                if (!item.room) continue;
                for (const date of dates(item.room.arrival, item.room.departure)) {
                    const block = stayGroupBlock(s, item.stayId, item.room.typeId, date);
                    if (!block) continue;
                    const key = `${item.room.typeId}|${date}`, claims = claimsByNight.get(key) ?? [];
                    claims.push({ blockId: block.id, stayId: item.stayId }); claimsByNight.set(key, claims);
                }
            }
            // Keep the original booking occupied until payment succeeds. A durable quota
            // claim moves its own held capacity into the pending revision, without holding twice.
            for (const k of new Set([...delta.keys(), ...claimsByNight.keys()])) {
                const quantity = Math.max(0, delta.get(k) ?? 0), claims = claimsByNight.get(k) ?? [];
                if (!quantity && !claims.length) continue;
                const [typeId, date] = k.split('|');
                s.records.push({ id: ports.uuid(), kind: 'block', version: 1, updatedAt: now, data: {
                    name: 'Pending guest change', typeId, roomId: '', from: date, to: plusDays(date, 1),
                    quantity: quantity || 1, quotaOnly: !quantity, groupPickupClaims: claims,
                    releaseDate: plusDays(s.property.businessDate, 366), kind: 'group', state: 'active',
                    pickedStayIds: [], bookingChangeId: key, expiresAt,
                } });
                if (availability(s, typeId, date).available < 0)
                    throw ports.error(409, 'PMS_SOLD_OUT', 'The additional room capacity is unavailable');
            }
            const attemptId = ports.uuid(), allocations = proposal.items.filter(i => i.dueNow > 0).map(i => ({ stayId: i.stayId, guestAccountId: record(s, 'stay', i.stayId).data.guestAccountId, amount: i.dueNow }));
            const settings = await tx.readPosSettings({ companyId: s.property.companyId });
            await tx.persist(s, before);
            await tx.insertPmsBookingPayments({ attemptId: attemptId, id: s.property.id, id2: id, companyId: s.property.companyId, slug: slug, account_id: connection.account_id, livemode: connection.livemode, dueNow: proposal.dueNow, expiresAt: expiresAt, now: now, now2: now, fact11: JSON.stringify(allocations), fact12: JSON.stringify([...(connection.card_enabled ? ['card'] : []), ...(connection.promptpay_enabled ? ['promptpay'] : [])]), fact13: settings ? (parseGuestAccountJson(settings.accounts_json).stripe || s.property.config.accounts.edc) : s.property.config.accounts.edc, key: key });
            await (await tx.snapshotMoney({ companyId: s.property.companyId }, { sourceType: 'pms_booking_payment', sourceId: attemptId, amountKind: 'requested', transactionDate: s.property.businessDate, currency: 'THB', amountUnits: String(proposal.dueNow) })).statement.run();
            return { summary: ports.bookingSummary(s, id, now), attemptId };
        }, async () => {
            const { s } = await bookingAuthority(db, token, now, false, staff);
            return { propertyId: s.property.id, purpose: bookingRevisionPurchasePurpose(s, proposal.kind, proposal.items), receipt: { kind: "change", id: key },
              ...(staff ? { actor: staff.actor, permission: staff.override ? "pms.approve" as const : "pms.reservations" as const } : {}) };
        });
        return result.attemptId ? db.bookingCheckout(slug, result.attemptId, env, now) : result.summary;
    }
    async function settleBookingChange(tx: PmsBookingChangeUnit, s: Snapshot, payment: BookingPaymentRow, result: {
        paid: boolean;
        providerSucceededAt: number | null;
        now: number;
        env: BookingEnv;
    }) {
        const { paid, providerSucceededAt, now, env } = result;
        s.asOf = now;
        const change = await tx.readPmsBookingChanges3({ proposal_id: payment.proposal_id });
        if (!change)
            throw ports.error(409, 'PMS_CHANGE_MISSING', 'Change proposal is missing');
        const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)]));
        releaseHolds(s, change.id, now);
        const proposal = parseGuestAccountJson(change.proposal_json) as Proposal;
        let valid = paid && (providerSucceededAt ?? now) < Number(payment.expires_at) && record(s, 'reservation', payment.reservation_id).version === Number(change.reservation_version);
        if (valid) {
            const trial = structuredClone(s);
            for (const item of proposal.items)
                record(trial, 'stay', item.stayId).data.state = 'cancelled';
            try {
                await assertManualFunding(tx, s, proposal);
                for (const item of proposal.items) {
                    eligible(s, item.stayId, providerSucceededAt ?? now, true, proposal.staff?.override);
                    Object.assign(record(trial, 'stay', item.stayId).data, { ...item.room, state: 'reserved', prices: item.quote!.lines[0].nights });
                }
                valid = proposal.items.every(item => item.quote!.lines[0].nights.every(n => availability(trial, item.room!.typeId, n.date).available >= 0));
            }
            catch {
                valid = false;
            }
        }
        const oldEdc = s.property.config.accounts.edc;
        s.property.config.accounts.edc = payment.clearing_account;
        if (valid) {
            let offset = 0;
            for (const part of parseGuestAccountJson(payment.allocation_json) as Array<{
                stayId: string;
                guestAccountId: string;
                amount: number;
            }>) {
                const outcome = executePms(s, 'payment.stripe', { guestAccountId: part.guestAccountId, amount: part.amount, reference: payment.payment_intent_id ?? payment.checkout_id }, providerActor(now));
                record(s, 'entry', outcome.id).data.sourceKey = `online-payment:${payment.id}`;
                record(s, 'entry', outcome.id).data.providerPaymentOffset = offset;
                offset += part.amount;
                await tx.persistPmsRecords(s, before);
                await tx.effects({ companyId: payment.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
                await tx.insertPmsBookingAllocations({ fact0: ports.uuid(), id: payment.id, stayId: part.stayId, guestAccountId: part.guestAccountId, id2: outcome.id, amount: part.amount, now: now });
            }
            await apply(tx, s, proposal, payment.holding_slug, env, now, providerSucceededAt ?? now, before);
        }
        else if (paid) {
            const st = record(s, 'stay', proposal.items[0].stayId);
            const outcome = executePms(s, 'payment.unallocated', { guestAccountId: st.data.guestAccountId, amount: Number(payment.amount_minor), reference: payment.checkout_id }, providerActor(now));
            record(s, 'entry', outcome.id).data.sourceKey = `online-payment:${payment.id}`;
            await tx.persistPmsRecords(s, before);
            await tx.effects({ companyId: payment.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
            await tx.insertPmsBookingRefundRequests({ fact0: ports.uuid(), id: payment.id, amount_minor: payment.amount_minor, now: now, now2: now });
        }
        s.property.config.accounts.edc = oldEdc;
        await tx.persist(s, before);
        await tx.updatePmsBookingChanges2({ fact0: valid ? 'applied' : 'expired', id: change.id });
        await tx.updatePmsBookingPayments({ fact0: valid ? 'confirmed' : paid ? 'refund_review' : 'expired', fact1: paid && !valid ? 'requested' : null, now: now, id: payment.id });
    }
    return { publicChangeQuote, confirmBookingChange, settleBookingChange };
}
