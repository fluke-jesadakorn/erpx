import { parseGuestAccountJson } from "@erpx/feature-identity_access/lib/guest-account-compat";
import { z } from "zod";
import { bookingOffers as evaluateOffers, bookingQuote as evaluateQuote, collectionAmounts, cancellationEligibility, roomSelectionSchema, searchSchema, type BookingSearch } from "@erpx/feature-pms/lib/pms-booking-domain";
import { availability as evaluateAvailability } from "@erpx/feature-pms/lib/pms-inventory-domain";
import { record, records, type Kind, type Property, type Snapshot } from "@erpx/server-pms/domain/pms";
import { executePms, PmsError, type Effects } from "@erpx/feature-pms/lib/pms-engine";
import { publicReservationSchema, bookingPublicUrl, type BookingEnv, type BookingPaymentRow, type PaymentAllocation } from "@erpx/server-pms/domain/booking";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { Clock } from "@erpx/server-shared/application/ports";
import type { CapabilityPurpose } from "@erpx/server-commercial/public/rules/packages";
import type { PmsPropertyPurchase, PmsPropertyPurchaseRequest } from "@erpx/server-pms/domain/property-purchases";
export type PmsBookingRows<Row> = {
    results: Row[];
};
export type PmsBookingEffect = {
    run(): Promise<PmsBookingRows<Record<string, unknown>>>;
};
export type PmsBookingUnit<Effect extends PmsBookingEffect> = {
    hydrateReceiptAllocationCredits(snapshot:Snapshot):Promise<void>;
    purchaseAllowed(propertyId: string, purpose: CapabilityPurpose): Promise<boolean>;
    purchaseFlags(propertyId: string): Promise<{ newWork: boolean; settleExisting: boolean; history: boolean }>;
    requirePurchase(input: PmsPropertyPurchase): Promise<void>;
    readPmsProperties1(facts: {
        propertyId: string;
        lock: boolean;
    }): Promise<{
        id: string;
        company_id: string;
        branch_id: string;
        name: string;
        business_date: string;
        state: Property["state"];
        version: number;
        config_json: string;
    } | null>;
    writePmsProperties1(facts: {
        version: number;
        fact1: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingTokens1(facts: {
        id: `${string}-${string}-${string}-${string}-${string}`;
        propertyId: string;
        reservationId: string;
        fact3: string;
        fact4: number;
        now: number;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingEmails1(facts: {
        mailId: `${string}-${string}-${string}-${string}-${string}`;
        id: string;
        reservationId: string;
        id2: `${string}-${string}-${string}-${string}-${string}`;
        slug: string;
        kind: "access" | "refund" | "change" | "cancellation" | "confirmation";
        fact6: string;
        now: number;
        now2: number;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readPmsBookingReceipts1(facts: {
        key: string;
    }): Promise<{
        body_hash: string;
        reservation_id: string;
        response_json: string | null;
    } | null>;
    readPosConnections1(facts: {
        companyId: string;
    }): Promise<{
        account_id: string;
        livemode: boolean;
        card_enabled: boolean;
        promptpay_enabled: boolean;
    } | null>;
    writePmsBookingReceipts1(facts: {
        key: string;
        id: string;
        digest: string;
        id2: string;
        now: number;
        fact5: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readPosSettings1(facts: {
        companyId: string;
    }): Promise<{
        accounts_json: string;
    } | null>;
    writePmsBookingPayments1(facts: {
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
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingReceipts2(facts: {
        key: string;
        id: string;
        digest: string;
        id2: string;
        now: number;
        fact5: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readPmsBookingPayments1(facts: {
        id: string;
    }): Promise<BookingPaymentRow | null>;
    readPmsRecords1(facts: {
        reservation_id: string;
        property_id: string;
    }): Promise<{
        email: string;
    } | null>;
    writePmsBookingPayments2(facts: {
        id: string;
        url: string;
        fact2: number;
        id2: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readPmsBookingPayments2(facts: {
        id: string;
    }): Promise<BookingPaymentRow | null>;
    readPmsBookingReceipts2(facts: {
        reservation_id: string;
        id: string;
    }): Promise<{
        response_json: string;
    } | null>;
    readPmsBookingPayments3(facts: {
        id: string;
    }): Promise<BookingPaymentRow | null>;
    writePmsBookingPayments3(facts: {
        paymentIntentId: string;
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingPayments4(facts: {
        paymentIntentId: string | null;
        providerSucceededAt: number | null;
        fact2: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readContext1(facts: Record<string, never>): Promise<Record<string, unknown> | null>;
    readPmsBookingPayments4(facts: {
        id: string;
    }): Promise<BookingPaymentRow | null>;
    writePmsBookingPayments5(facts: {
        paymentIntentId: string | null;
        providerSucceededAt: number | null;
        fact2: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingAllocations1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        id: string;
        stayId: string;
        guestAccountId: string;
        id2: string;
        amount: number;
        now: number;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingPayments6(facts: {
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingRefundRequests1(facts: {
        fact0: `${string}-${string}-${string}-${string}-${string}`;
        id: string;
        amount_minor: number;
        now: number;
        now2: number;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingPayments7(facts: {
        fact0: "expired" | "refund_review";
        fact1: "requested" | null;
        fact2: number | null;
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingPayments8(facts: {
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingPayments9(facts: {
        fact0: string;
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingRefundRequests2(facts: {
        fact0: string;
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingPayments10(facts: {
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingPayments11(facts: {
        fact0: string;
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingRefundRequests3(facts: {
        fact0: string;
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readPmsBookingTokens1(facts: {
        fact0: string;
        now: number;
    }): Promise<{
        property_id: string;
        reservation_id: string;
    } | null>;
    readPmsBookingRefundRequests1(facts: {
        id: string;
    }): Promise<PmsBookingRows<{
        id: string;
        amount_minor: number;
        state: string;
        reason: string;
    }>>;
    readPmsDocuments1(facts: {
        id: string;
        id2: string;
        id3: string;
    }): Promise<PmsBookingRows<{
        id: string;
        kind: string;
        created_at: number;
    }>>;
    readPmsBookingPayments5(facts: {
        id: string;
    }): Promise<{
        id: string;
        expires_at: number;
    } | null>;
    readPmsBookingLimits1(facts: {
        fact0: string;
        bucket: number;
    }): Promise<{
        count: number;
    } | null>;
    readPmsProperties2(facts: {
        propertyId: string;
        companyId: string;
        fact2: Array<string>;
        actor: WorkContext;
    }): Promise<Record<string, unknown> | null>;
    writePmsBookingLimits1(facts: {
        fact0: number;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readPmsBookingEmails1(facts: {
        fact0: number;
        now: number;
        now2: number;
    }): Promise<{
        id: string;
        property_id: string;
        reservation_id: string;
        token_id: string;
        holding_slug: string;
        kind: string;
        attempts: number;
        payload_json: string;
    } | null>;
    writePmsBookingTokens2(facts: {
        fact0: number;
        token_id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingEmails2(facts: {
        now: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingEmails3(facts: {
        fact0: "pending" | "failed";
        fact1: number;
        id: string;
    }): Promise<PmsBookingRows<Record<string, unknown>>>;
    writePmsBookingEmailsBlocked(facts: { id: string; code: string; nextAt: number }): Promise<PmsBookingRows<Record<string, unknown>>>;
    readPmsRecords2(facts: {
        propertyId: string;
        fact1: Array<string>;
        search: BookingSearch | undefined;
    }): Promise<{
        id: string;
        kind: Kind;
        data_json: string;
        version: number;
        updated_at: number;
    }[]>;
    readPmsProperties3(facts: Record<string, never>): Promise<{
        id: string;
        name: string;
        config_json: string;
    }[]>;
    readAttachmentPublications1(facts: Record<string, never>): Promise<{
        id: string;
    }[]>;
    readPmsBookingPayments6(facts: {
        fact0: number;
        fact1: number;
    }): Promise<{
        id: string;
    }[]>;
    readPmsBookingPayments7(facts: Record<string, never>): Promise<{
        id: string;
    }[]>;
    readPmsRecords3(facts: {
        number: string;
        email: string;
    }): Promise<{
        id: string;
        property_id: string;
    }[]>;
    persistPmsRecords(arg0: Snapshot, arg1: Map<string, string>): Promise<void>;
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
        statement: Effect;
    }>;
    persistPmsEffects(arg0: WorkContext, arg1: Snapshot, arg2: Effects): Promise<void>;
    processBookingRefund(arg0: string, arg1: number): Promise<void>;
    settleBookingChange(s: Snapshot, payment: BookingPaymentRow, context: {
        paid: boolean;
        providerSucceededAt: number | null;
        now: number;
        env: BookingEnv;
    }): Promise<void>;
    publicChangeQuote(token: string, kind: "cancel", input: {
        items: {
            stayId: string;
        }[];
    }, env: BookingEnv, now: number): Promise<{
        token: string;
    }>;
    confirmBookingChange(slug: string, token: string, proposal: string, key: string, env: BookingEnv, now: number): Promise<unknown>;
};
export type PmsBookingRepository<Effect extends PmsBookingEffect> = PmsBookingUnit<Effect> & {
    transaction<Result>(work: (unit: PmsBookingUnit<Effect>) => Promise<Result>, purchase?: PmsPropertyPurchaseRequest): Promise<Result>;
};
export function createPmsBooking<Effect extends PmsBookingEffect>(ports: {
    clock: Clock;
    uuid(): `${string}-${string}-${string}-${string}-${string}`;
    hash(value: string): string;
    sign(secret: string, value: string): string;
    same(a: string, b: string): boolean;
    protectMail(value: string, env: BookingEnv, id: string): string;
    unprotectMail(value: string, env: BookingEnv, id: string): string;
    encode(value: string): string;
    decode(value: string): string;
    bookingEnv(): Promise<BookingEnv>;
    liveEnabled(): string | undefined;
    require(actor: WorkContext, permission: string): void;
    forActor(actor: WorkContext): Promise<PmsBookingUnit<Effect>>;
    error(status: number, code: string, message: string): Error;
    isBusinessError(error: unknown): error is Error & {
        code: string;
    };
    stripeCall: typeof import("@erpx/server-pos/public/pos-stripe").stripeCall;
    verifiedStripePaymentTime: typeof import("@erpx/server-pos/public/stripe-payment-time").verifiedStripePaymentTime;
    sendMail: typeof fetch;
    timeout(): AbortSignal;
}) {
    // Expiring group reservations use an explicitly supplied application clock.
    const bookingQuote: typeof evaluateQuote = (s, ...args) => evaluateQuote({ ...s, asOf: s.asOf ?? ports.clock.now() }, ...args);
    const bookingOffers: typeof evaluateOffers = (s, ...args) => evaluateOffers({ ...s, asOf: s.asOf ?? ports.clock.now() }, ...args);
    const availability: typeof evaluateAvailability = (s, ...args) => evaluateAvailability({ ...s, asOf: s.asOf ?? ports.clock.now() }, ...args);
    async function bookingSnapshot(db: PmsBookingUnit<Effect>, propertyId: string, search?: BookingSearch, lock = false): Promise<Snapshot> {
        const row = await db.readPmsProperties1({ propertyId: propertyId, lock: lock });
        if (!row)
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found");
        const raw = await db.readPmsRecords2({ propertyId: propertyId, fact1: (search ? [search.departure, search.arrival] : []), search: search });
        const snapshot:Snapshot = {
            property: {
                id: row.id,
                companyId: row.company_id,
                branchId: row.branch_id,
                name: row.name,
                businessDate: row.business_date,
                state: row.state,
                version: Number(row.version),
                config: parseGuestAccountJson(row.config_json),
            },
            records: raw.map((r) => ({
                id: r.id,
                kind: r.kind,
                data: parseGuestAccountJson(r.data_json),
                version: Number(r.version),
                updatedAt: Number(r.updated_at),
            })),
        };
        if(records(snapshot,"entry").some(row=>row.data.kind==="adjustment"&&row.data.total<0&&row.data.originalId))await db.hydrateReceiptAllocationCredits(snapshot);
        return snapshot;
    }
    function published(s: Snapshot) {
        if (!s.property.config.booking?.published)
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found");
    }
    async function bookingCatalog(db: PmsBookingUnit<Effect>) {
        const hotels = await db.readPmsProperties3({});
        const approved = new Set((await db.readAttachmentPublications1({})).map(r => r.id));
        return hotels.map((r) => {
            const c = parseGuestAccountJson(r.config_json) as Property["config"], b = c.booking!;
            return {
                id: r.id,
                name: r.name,
                description: b.description,
                address: b.address,
                contactEmail: b.contactEmail,
                contactPhone: b.contactPhone,
                currency: c.currency,
                checkIn: c.checkIn,
                checkOut: c.checkOut,
                timezone: c.timezone,
                images: (b.images ?? []).filter(i => approved.has(i.publicationId)),
                roomContent: b.roomContent.map(room => ({ ...room, images: (room.images ?? []).filter(i => approved.has(i.publicationId)) })),
            };
        });
    }
    async function publicOffers(db: PmsBookingRepository<Effect>, input: unknown, propertyId?: string) {
        const search = searchSchema.parse(input), catalog = await bookingCatalog(db), selected = propertyId
            ? catalog.filter((h) => h.id === propertyId)
            : catalog;
        return Promise.all(selected.map(async (hotel) => ({
            hotel,
            offers: await db.transaction(async (tx) => bookingOffers(await bookingSnapshot(tx, hotel.id, search), search, true), { propertyId: hotel.id, purpose: "new-work", readOnly: true, hideUnavailable: true }),
        })));
    }
    function publicSelections(s: Snapshot, raw: unknown, now: number) {
        published(s);
        const lines = z.array(roomSelectionSchema).min(1).max(10).parse(raw);
        if (lines.reduce((n, l) => n + l.quantity, 0) > 20)
            throw ports.error(422, "PMS_ROOM_LIMIT", "Book at most 20 rooms online; contact the hotel for groups");
        for (const line of lines)
            if (!s.property.config.booking!.rateIds.includes(line.rateId) ||
                record(s, "rate", line.rateId).data.market !== "public")
                throw ports.error(422, "PMS_RATE_UNAVAILABLE", "Choose an available public rate");
        const today = new Intl.DateTimeFormat("en-CA", {
            timeZone: s.property.config.timezone,
        }).format(new Date(now));
        if (lines.some((line) => line.arrival < today))
            throw ports.error(422, "PMS_PAST_ARRIVAL", "Choose a current or future arrival date");
        return lines;
    }
    async function publicQuote(db: PmsBookingRepository<Effect>, propertyId: string, raw: unknown, env: BookingEnv, now = ports.clock.now()) {
        return db.transaction(async (tx) => {
            const lines = z.array(roomSelectionSchema).min(1).max(10).parse(raw), search = {
                ...lines[0],
                arrival: lines.map((l) => l.arrival).sort()[0],
                departure: lines
                    .map((l) => l.departure)
                    .sort()
                    .at(-1)!,
            };
            const s = await bookingSnapshot(tx, propertyId, search), quote = onlineQuote(bookingQuote(s, publicSelections(s, lines, now)));
            const payload = ports.encode(JSON.stringify({ quote, expires: now + 15 * 60 * 1000 }));
            return { quote, token: `${payload}.${ports.sign(env.secret, payload)}` };
        }, { propertyId, purpose: "new-work", readOnly: true, hideUnavailable: true });
    }
    function onlineQuote(quote: ReturnType<typeof bookingQuote>) { return quote; }
    function readQuote(token: string, env: BookingEnv, now: number) {
        const [payload, signature, extra] = token.split(".");
        if (extra || !payload || !signature || !ports.same(signature, ports.sign(env.secret, payload)))
            throw ports.error(422, "PMS_QUOTE_INVALID", "Search again for a fresh quote");
        const data = parseGuestAccountJson(ports.decode(payload)) as {
            quote: ReturnType<typeof bookingQuote>;
            expires: number;
        };
        if (data.expires <= now)
            throw ports.error(409, "PMS_QUOTE_EXPIRED", "Your quote expired. Review a fresh quote");
        return data.quote;
    }
    async function persist(db: PmsBookingUnit<Effect>, s: Snapshot, before: Map<string, string>) {
        await db.persistPmsRecords(s, before);
        await db.writePmsProperties1({ version: s.property.version, fact1: ports.clock.now(), id: s.property.id });
    }
    function tokenValue(id: string, env: BookingEnv) {
        return `${id}.${ports.sign(env.secret, `access:${id}`)}`;
    }
    async function issueBookingToken(db: PmsBookingUnit<Effect>, propertyId: string, reservationId: string, env: BookingEnv, now: number) {
        const id = ports.uuid(), token = tokenValue(id, env);
        await db.writePmsBookingTokens1({ id: id, propertyId: propertyId, reservationId: reservationId, fact3: ports.hash(token), fact4: now + 7 * 86400000, now: now });
        return { id, token };
    }
    async function queueMail(db: PmsBookingUnit<Effect>, s: Snapshot, reservationId: string, slug: string, kind: "confirmation" | "cancellation" | "access" | "change" | "refund", env: BookingEnv, now: number) {
        const { id, token } = await issueBookingToken(db, s.property.id, reservationId, env, now);
        const b = bookingSummary(s, reservationId, now);
        const link = bookingPublicUrl(env,slug,`/manage#token=${encodeURIComponent(token)}`);
        const subject = `${b.hotel.name} · ${kind === "cancellation" ? "Cancellation" : kind === "access" ? "Booking access" : kind === "change" ? "Booking updated" : kind === "refund" ? "Refund confirmed" : "Booking confirmation"} ${b.number}`;
        const payload = {
            from: env.from,
            to: [b.guest.email],
            subject,
            text: `${subject}\n\n${b.rooms.map((r) => `${r.roomName}: ${r.arrival} to ${r.departure}`).join("\n")}\n\n${b.policy?.text ?? "Contact the hotel for booking terms."}\n\nView your booking: ${link}\nThis private link expires in seven days. Request another from the booking website.\n\n${b.hotel.contactPhone}`,
        };
        const mailId = ports.uuid();
        await db.writePmsBookingEmails1({ mailId: mailId, id: s.property.id, reservationId: reservationId, id2: id, slug: slug, kind: kind, fact6: ports.protectMail(JSON.stringify(payload), env, mailId), now: now, now2: now });
        return token;
    }
    function bookingSummary(s: Snapshot, id: string, now: number) {
        const r = record(s, "reservation", id), b = s.property.config.booking, guest = record(s, "guest", r.data.guestId).data;
        const guestAccountIds = new Set(records(s, 'guest_account').filter(f => f.data.reservationId === id).map(f => f.id)), entries = records(s, 'entry'), byId = new Map(entries.map(e => [e.id, e]));
        const postedRoomTotal = entries.filter(e => guestAccountIds.has(e.data.guestAccountId)).reduce((n, e) => { let current: typeof e | undefined = e; const seen = new Set<string>(); while (current && !seen.has(current.id)) {
            seen.add(current.id);
            if (current.data.sourceKey.startsWith('night:'))
                return n + e.data.total;
            current = byId.get(current.data.originalId);
        } return n; }, 0);
        return {
            postedRoomTotal,
            id: r.id,
            number: r.data.number,
            state: r.data.state,
            hotel: {
                id: s.property.id,
                name: s.property.name,
                address: b?.address ?? "",
                contactEmail: b?.contactEmail ?? "",
                contactPhone: b?.contactPhone ?? "",
            },
            guest: { name: guest.name, email: guest.email },
            paymentState: records(s, "entry").some(e => e.data.sourceKey?.startsWith("online-payment:") && records(s, "guest_account").some(f => f.id === e.data.guestAccountId && f.data.reservationId === id)) ? "received" as const : "pay_at_hotel" as const,
            currency: s.property.config.currency,
            version: r.version,
            history: r.data.history ?? [],
            total: records(s, "stay").filter(st => st.data.reservationId === id && !["cancelled", "no_show"].includes(st.data.state)).reduce((n, st) => n + st.data.prices.reduce((sum, p) => sum + p.total, 0), 0),
            advance: records(s, "guest_account").filter(f => f.data.reservationId === id).reduce((n, f) => n + f.data.advance, 0),
            guestAccountDue: records(s, "guest_account").filter(f => f.data.reservationId === id).reduce((n, f) => n + f.data.due, 0),
            policy: r.data.bookingPolicy,
            rooms: records(s, "stay")
                .filter((st) => st.data.reservationId === id)
                .map((st) => ({
                id: st.id,
                typeId: st.data.typeId,
                rateId: st.data.rateId,
                guestAccountId: st.data.guestAccountId,
                policy: st.data.bookingPolicy ?? r.data.bookingPolicy,
                roomName: record(s, "room_type", st.data.typeId).data.name,
                arrival: st.data.arrival,
                departure: st.data.departure,
                adults: st.data.adults,
                childAges: st.data.childAges ?? [],
                state: st.data.state,
                nights: st.data.prices,
            })),
            cancellation: cancellationEligibility(s, id, now),
        };
    }
    type BookingReceipt = (ReturnType<typeof bookingSummary> & {managementToken:string}) | {attemptId:`${string}-${string}-${string}-${string}-${string}`;managementToken:string};
    type PublicBookingResponse=Omit<ReturnType<typeof bookingSummary>,"state"> & {state:string;managementToken?:string;attemptToken?:string;checkoutUrl?:string|null;expiresAt?:number;refundState?:string|null};
    function isSummaryReceipt(receipt:BookingReceipt):receipt is ReturnType<typeof bookingSummary> & {managementToken:string}{return !('attemptId' in receipt)||!receipt.attemptId;}
    async function createPublicReservation(db: PmsBookingRepository<Effect>, slug: string, raw: unknown, key: string, env: BookingEnv, now = ports.clock.now()):Promise<PublicBookingResponse> {
        const v = publicReservationSchema.parse(raw);
        z.string().uuid().parse(key);
        const digest = ports.hash(JSON.stringify(v));
        const created = await db.transaction(async (tx) => {
            // Same Property lock as staff bookings, room moves, maintenance and night audit.
            const s = await bookingSnapshot(tx, v.propertyId, undefined, true);
            const old = await tx.readPmsBookingReceipts1({ key: key });
            if (old) {
                if (old.body_hash !== digest)
                    throw ports.error(409, "PMS_KEY_REUSED", "Use a new request key for a different booking");
                if (!old.response_json)
                    throw ports.error(409, "PMS_ALREADY_CONFIRMED", "This booking was already confirmed. Use your confirmation email to manage it");
                return parseGuestAccountJson(old.response_json) as BookingReceipt;
            }
            const quoted = readQuote(v.quoteToken, env, now);
            if (quoted.propertyId !== s.property.id)
                throw ports.error(422, "PMS_QUOTE_INVALID", "Quote belongs to another hotel");
            const lines = publicSelections(s, quoted.lines, now), fresh = onlineQuote(bookingQuote(s, lines));
            if (JSON.stringify(fresh) !== JSON.stringify(quoted))
                throw ports.error(409, "PMS_QUOTE_CHANGED", "Price or policy changed. Review a fresh quote");
            const stripeBooking = fresh.dueNow > 0;
            if (fresh.currency !== 'THB' || fresh.total <= 0)
                throw ports.error(422, 'PMS_STRIPE_CURRENCY', 'Booking requires a positive THB quote');
            const connection = stripeBooking ? await tx.readPosConnections1({ companyId: s.property.companyId }) : null;
            if (stripeBooking && (!connection?.account_id || connection.livemode && ports.liveEnabled() !== 'true' || !connection.card_enabled && !connection.promptpay_enabled))
                throw ports.error(503, 'PMS_PAYMENT_UNAVAILABLE', 'Online booking payment is unavailable for this company');
            if (stripeBooking && !s.property.config.onlineAccounting?.approvedBy)
                throw ports.error(409, 'PMS_ONLINE_ACCOUNTING_REQUIRED', 'Finance must approve refund and unallocated receipt accounts before online payment');
            const before = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
            const result = executePms(s, "reservation.create", {
                guest: v.guest,
                source: "website",
                rooms: lines,
                notes: v.notes,
                paymentPending: stripeBooking,
            }, {
                userId: "guest:website",
                now,
                uuid: ports.uuid,
                independent: () => false,
            });
            if (stripeBooking)
                record(s, 'reservation', result.id).data.bookingPolicy = fresh.policy;
            if (result.effects.postings.length ||
                result.effects.stock.length ||
                result.effects.documents.length)
                throw new Error("PUBLIC_BOOKING_FINANCIAL_EFFECT");
            await persist(tx, s, before);
            if (!stripeBooking) {
                const managementToken = await queueMail(tx, s, result.id, slug, 'confirmation', env, now);
                const response = { ...bookingSummary(s, result.id, now), managementToken };
                await tx.writePmsBookingReceipts1({ key: key, id: s.property.id, digest: digest, id2: result.id, now: now, fact5: JSON.stringify(response) });
                return response;
            }
            const attemptId = ports.uuid(), expiresAt = now + 30 * 60 * 1000;
            const allocation = records(s, 'stay').filter(st => st.data.reservationId === result.id).sort((a, b) => a.id.localeCompare(b.id)).map(st => ({ stayId: st.id, guestAccountId: st.data.guestAccountId, amount: collectionAmounts(st.data.prices.reduce((sum, n) => sum + n.total, 0), st.data.bookingPolicy?.collection).dueNow })).filter(a => a.amount > 0);
            const settings = await tx.readPosSettings1({ companyId: s.property.companyId });
            const clearing = settings ? (parseGuestAccountJson(settings.accounts_json).stripe || s.property.config.accounts.edc) : s.property.config.accounts.edc;
            const methods = [...(connection!.card_enabled ? ['card'] : []), ...(connection!.promptpay_enabled ? ['promptpay'] : [])];
            await tx.writePmsBookingPayments1({ attemptId: attemptId, id: s.property.id, id2: result.id, companyId: s.property.companyId, slug: slug, fact5: connection!.account_id, fact6: connection!.livemode, dueNow: fresh.dueNow, expiresAt: expiresAt, now: now, now2: now, fact11: JSON.stringify(allocation), fact12: JSON.stringify(methods), clearing: clearing });
            const money = await tx.snapshotMoney({ companyId: s.property.companyId }, { sourceType: 'pms_booking_payment', sourceId: attemptId, amountKind: 'requested', transactionDate: s.property.businessDate, currency: 'THB', amountUnits: String(fresh.dueNow) });
            await money.statement.run();
            const { token: managementToken } = await issueBookingToken(tx, s.property.id, result.id, env, now);
            const response = { attemptId, managementToken };
            await tx.writePmsBookingReceipts2({ key: key, id: s.property.id, digest: digest, id2: result.id, now: now, fact5: JSON.stringify(response) });
            return response;
        }, { propertyId: v.propertyId, purpose: "new-work", hideUnavailable: true, receipt: { kind: "reservation", id: key } });
        if (isSummaryReceipt(created))
            return created;
        return { ...await bookingCheckout(db, slug, created.attemptId, env, now), ...('managementToken' in created ? { managementToken: created.managementToken } : {}) };
    }
    const paymentToken = (id: string, env: BookingEnv) => `${id}.${ports.sign(env.secret, `payment:${id}`)}`;
    async function bookingCheckout(db: PmsBookingRepository<Effect>, slug: string, id: string, env: BookingEnv, now = ports.clock.now()) {
        const row = await db.readPmsBookingPayments1({ id: id });
        if (!row)
            throw ports.error(404, 'PMS_PAYMENT_NOT_FOUND', 'Booking payment not found');
        const token = paymentToken(id, env);
        if (row.status !== 'payment_pending')
            return bookingPaymentStatus(db, token, env, now);
        if (Number(row.expires_at) <= now)
            return bookingPaymentStatus(db, token, env, now);
        await db.requirePurchase({ propertyId: row.property_id, purpose: "settle-existing" });
        let url = row.checkout_url;
        if (!row.checkout_id) {
            const guest = await db.readPmsRecords1({ reservation_id: row.reservation_id, property_id: row.property_id });
            if (!guest?.email)
                throw ports.error(409, 'PMS_GUEST_EMAIL_REQUIRED', 'Guest email is required for booking payment');
            const success = bookingPublicUrl(env,slug,`/${encodeURIComponent(row.property_id)}?attempt=${encodeURIComponent(token)}`);
            const session = await ports.stripeCall(Boolean(row.livemode), 'checkout/sessions', { method: 'POST', account: row.account_id, key: `pms-booking-${id}`, body: { mode: 'payment', customer_email: guest.email, ...Object.fromEntries((parseGuestAccountJson(row.methods_json) as string[]).map((method, index) => [`payment_method_types[${index}]`, method])), 'line_items[0][price_data][currency]': 'thb', 'line_items[0][price_data][unit_amount]': String(row.amount_minor), 'line_items[0][price_data][product_data][name]': 'Hotel reservation', 'line_items[0][quantity]': '1', 'metadata[pms_booking_payment_id]': id, 'payment_intent_data[metadata][pms_booking_payment_id]': id, success_url: success, cancel_url: success, expires_at: String(Math.floor((Number(row.expires_at) + 60000) / 1000)) } });
            if (typeof session.url !== 'string' || new URL(session.url).hostname !== 'checkout.stripe.com')
                throw ports.error(502, 'PMS_CHECKOUT_URL_INVALID', 'Stripe did not return a valid checkout URL');
            await db.writePmsBookingPayments2({ id: session.id, url: session.url, fact2: ports.clock.now(), id2: id });
            url = session.url;
        }
        const s = await bookingSnapshot(db, row.property_id);
        return { ...bookingSummary(s, row.reservation_id, now), state: 'payment_pending', checkoutUrl: url, attemptToken: token, expiresAt: Number(row.expires_at) };
    }
    async function bookingPaymentStatus(db: PmsBookingRepository<Effect>, token: string, env: BookingEnv, now = ports.clock.now()) {
        const [id, signature, extra] = token.split(".");
        if (extra || !id || !signature || !ports.same(signature, ports.sign(env.secret, `payment:${id}`)))
            throw ports.error(401, 'PMS_PAYMENT_TOKEN', 'Invalid booking payment token');
        let row = await db.readPmsBookingPayments1({ id: id });
        if (!row)
            throw ports.error(404, 'PMS_PAYMENT_NOT_FOUND', 'Booking payment not found');
        await db.requirePurchase({ propertyId: row.property_id, purpose: "history" });
        await reconcileBookingPayment(db, id, env, now);
        row = (await db.readPmsBookingPayments1({ id: id }))!;
        const s = await bookingSnapshot(db, row.property_id);
        const receipt = await db.readPmsBookingReceipts2({ reservation_id: row.reservation_id, id: row.id });
        return { ...bookingSummary(s, row.reservation_id, now), state: row.status, refundState: row.refund_state, checkoutUrl: row.status === 'payment_pending' ? row.checkout_url : null, attemptToken: token, expiresAt: Number(row.expires_at), managementToken: receipt ? parseGuestAccountJson(receipt.response_json).managementToken : undefined };
    }
    async function reconcileBookingPayment(db: PmsBookingRepository<Effect>, id: string, env: BookingEnv, now = ports.clock.now()) {
        const row = await db.readPmsBookingPayments1({ id: id });
        if (!row || !['payment_pending', 'expired'].includes(row.status))
            return;
        const purchase: PmsPropertyPurchase = { propertyId: row.property_id, purpose: "settle-existing" };
        await db.requirePurchase(purchase);
        let paid = false;
        let providerSucceededAt: number | null = null;
        let paymentIntentId: string | null = null;
        if (row.checkout_id) {
            let session = await ports.stripeCall(Boolean(row.livemode), `checkout/sessions/${row.checkout_id}`, { account: row.account_id });
            if (session.payment_status !== 'paid' && now >= Number(row.expires_at) && session.status === 'open') {
                await ports.stripeCall(Boolean(row.livemode), `checkout/sessions/${row.checkout_id}/expire`, { method: 'POST', account: row.account_id, key: `pms-expire-${id}`, body: {} });
                session = await ports.stripeCall(Boolean(row.livemode), `checkout/sessions/${row.checkout_id}`, { account: row.account_id });
            }
            if (session.payment_status === 'paid') {
                const pi = await ports.stripeCall(Boolean(row.livemode), `payment_intents/${String(session.payment_intent)}`, { account: row.account_id });
                if (pi.status !== 'succeeded' || pi.amount_received !== Number(row.amount_minor) || pi.currency !== 'thb' || (pi.metadata as Record<string, string> | undefined)?.pms_booking_payment_id !== id)
                    throw ports.error(409, 'PMS_PAYMENT_MISMATCH', 'Payment needs Finance review');
                paymentIntentId = String(pi.id);
                // Retain a verified receipt observation even if event-time lookup or local accounting fails.
                await db.writePmsBookingPayments3({ paymentIntentId: paymentIntentId, now: now, id: id });
                providerSucceededAt = await ports.verifiedStripePaymentTime({ livemode: Boolean(row.livemode), accountId: row.account_id, paymentIntentId: String(pi.id), known: row.provider_succeeded_at ? Number(row.provider_succeeded_at) : null, createdAt: Number(row.created_at), now });
                paid = true;
            }
        }
        if (!paid && now < Number(row.expires_at))
            return;
        if (paid)
            await db.writePmsBookingPayments4({ paymentIntentId: paymentIntentId, providerSucceededAt: providerSucceededAt, fact2: providerSucceededAt ?? now, id: id });
        await db.transaction(async (tx) => {
            const s = await bookingSnapshot(tx, row.property_id, undefined, true);
            await tx.readContext1({});
            const current = await tx.readPmsBookingPayments4({ id: id });
            if (!current || !['payment_pending', 'expired'].includes(current.status))
                return;
            const before = new Map(s.records.map(r => [r.id, JSON.stringify(r)]));
            if (paid)
                await tx.writePmsBookingPayments5({ paymentIntentId: paymentIntentId, providerSucceededAt: providerSucceededAt, fact2: providerSucceededAt ?? now, id: id });
            if (current.purpose === 'change') {
                await tx.settleBookingChange(s, current, { paid, providerSucceededAt, now, env });
                return;
            }
            const reservation = record(s, 'reservation', current.reservation_id);
            const eligible = paid && (providerSucceededAt ?? now) < Number(current.expires_at);
            let canAllocate = eligible;
            if (canAllocate && reservation.data.state === 'cancelled' && current.status === 'expired') {
                const trial = structuredClone(s);
                for (const st of records(trial, 'stay').filter(st => st.data.reservationId === current.reservation_id))
                    st.data.state = 'reserved';
                canAllocate = records(trial, 'stay').filter(st => st.data.reservationId === current.reservation_id).every(st => st.data.prices.every(n => availability(trial, st.data.typeId, n.date).available >= 0));
                if (canAllocate)
                    for (const st of records(s, 'stay').filter(st => st.data.reservationId === current.reservation_id)) {
                        st.data.state = 'reserved';
                        st.version++;
                        st.updatedAt = now;
                    }
            }
            else
                canAllocate = canAllocate && reservation.data.state === 'payment_pending';
            const oldEdc = s.property.config.accounts.edc;
            s.property.config.accounts.edc = current.clearing_account;
            if (canAllocate) {
                canAllocate = !records(s, 'stay').filter(st => st.data.reservationId === current.reservation_id).some(st => st.data.prices.some(n => availability(s, st.data.typeId, n.date).available < 0));
            }
            if (canAllocate) {
                reservation.data.state = 'confirmed';
                reservation.data.history = [...(reservation.data.history ?? []), { at: now, actor: 'system:stripe', action: 'Online payment confirmed' }];
                reservation.version++;
                reservation.updatedAt = now;
                let allocated = 0;
                for (const allocation of parseGuestAccountJson(current.allocation_json) as PaymentAllocation[]) {
                    const outcome = executePms(s, 'payment.stripe', { guestAccountId: allocation.guestAccountId, amount: allocation.amount, reference: String(paymentIntentId) }, { userId: 'system:stripe', now, uuid: ports.uuid, independent: () => false });
                    record(s, 'entry', outcome.id).data.sourceKey = `online-payment:${current.id}`;
                    record(s, 'entry', outcome.id).data.providerPaymentOffset = allocated;
                    allocated += allocation.amount;
                    await tx.persistPmsRecords(s, before);
                    await tx.persistPmsEffects({ companyId: current.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
                    await tx.writePmsBookingAllocations1({ fact0: ports.uuid(), id: id, stayId: allocation.stayId, guestAccountId: allocation.guestAccountId, id2: outcome.id, amount: allocation.amount, now: now });
                }
                if (allocated !== Number(current.amount_minor))
                    throw ports.error(409, 'PMS_PAYMENT_ALLOCATION_CHANGED', 'Receipt allocations must equal the original payment');
                s.property.config.accounts.edc = oldEdc;
                await persist(tx, s, before);
                await tx.writePmsBookingPayments6({ now: now, id: id });
                await queueMail(tx, s, current.reservation_id, current.holding_slug, 'confirmation', env, now);
            }
            else {
                if (reservation.data.state === 'payment_pending')
                    executePms(s, 'reservation.cancel', { id: current.reservation_id, reason: paid ? 'Online payment could not be assigned' : 'Online payment period expired', fee: 0 }, { userId: 'system:stripe', now, uuid: ports.uuid, independent: () => false });
                if (paid) {
                    const first = records(s, 'stay').find(st => st.data.reservationId === current.reservation_id)!;
                    const outcome = executePms(s, 'payment.unallocated', { guestAccountId: first.data.guestAccountId, amount: Number(current.amount_minor), reference: String(paymentIntentId) }, { userId: 'system:stripe', now, uuid: ports.uuid, independent: () => false });
                    record(s, 'entry', outcome.id).data.sourceKey = `online-payment:${current.id}`;
                    await tx.persistPmsRecords(s, before);
                    await tx.persistPmsEffects({ companyId: current.company_id, userId: 'system:stripe' } as WorkContext, s, outcome.effects);
                    await tx.writePmsBookingRefundRequests1({ fact0: ports.uuid(), id: id, amount_minor: current.amount_minor, now: now, now2: now });
                }
                s.property.config.accounts.edc = oldEdc;
                await persist(tx, s, before);
                await tx.writePmsBookingPayments7({ fact0: paid ? 'refund_review' : 'expired', fact1: paid ? 'requested' : null, fact2: paid ? now : null, now: now, id: id });
            }
        }, purchase);
    }
    async function drainBookingPayments(db: PmsBookingRepository<Effect>, env: BookingEnv, now = ports.clock.now()) {
        const due = await db.readPmsBookingPayments6({ fact0: now - 300000, fact1: now - 30 * 86400000 });
        for (const item of due) {
            try {
                await reconcileBookingPayment(db, item.id, env, now);
                await db.writePmsBookingPayments8({ now: now, id: item.id });
            }
            catch (error) {
                const code = ports.isBusinessError(error) ? error.code : error instanceof Error ? error.message : 'PMS_PAYMENT_REVIEW';
                await db.writePmsBookingPayments9({ fact0: String(code).slice(0, 100), now: now, id: item.id });
                await db.writePmsBookingRefundRequests2({ fact0: String(code).slice(0, 100), now: now, id: item.id });
            }
        }
        const refunds = await db.readPmsBookingPayments7({});
        for (const item of refunds) {
            try {
                await db.processBookingRefund(item.id, now);
                await db.writePmsBookingPayments8({ now: now, id: item.id });
            }
            catch (error) {
                const code = ports.isBusinessError(error) ? error.code : error instanceof Error ? error.message : 'PMS_REFUND_REVIEW';
                await db.writePmsBookingPayments9({ fact0: String(code).slice(0, 100), now: now, id: item.id });
                await db.writePmsBookingRefundRequests2({ fact0: String(code).slice(0, 100), now: now, id: item.id });
            }
        }
    }
    async function authorizedBooking(db: PmsBookingUnit<Effect>, token: string, now: number, lock = false) {
        if (token.length > 200)
            throw ports.error(401, "PMS_LINK_EXPIRED", "Request a new booking access link");
        const t = await db.readPmsBookingTokens1({ fact0: ports.hash(token), now: now });
        if (!t)
            throw ports.error(401, "PMS_LINK_EXPIRED", "Request a new booking access link");
        const s = await bookingSnapshot(db, t.property_id, undefined, lock);
        await db.requirePurchase({ propertyId: s.property.id, purpose: "history" });
        return {
            s,
            id: t.reservation_id,
        };
    }
    async function manageBooking(db: PmsBookingRepository<Effect>, token: string, now = ports.clock.now()) {
        return db.transaction(async (tx) => {
            const { s, id } = await authorizedBooking(tx, token, now);
            const refunds = (await tx.readPmsBookingRefundRequests1({ id: id })).results;
            const documents = (await tx.readPmsDocuments1({ id: s.property.id, id2: s.property.id, id3: id })).results;
            const pending = await tx.readPmsBookingPayments5({ id: id });
            const summary = bookingSummary(s, id, now);
            const purchases = await tx.purchaseFlags(s.property.id), oldRates = new Set(summary.rooms.map(room => room.rateId));
            return { ...summary, purchases, editableRates: purchases.newWork || purchases.settleExisting ? records(s, "rate").filter(r => r.data.active && r.data.market === "public" && s.property.config.booking?.rateIds.includes(r.id) && (purchases.newWork || oldRates.has(r.id))).map(r => ({ id: r.id, typeId: r.data.typeId, name: r.data.name, roomName: record(s, "room_type", r.data.typeId).data.name })) : [], refundState: refunds[0]?.state ?? null, refunds, documents, dueAtHotel: Math.max(0, summary.total - summary.postedRoomTotal + summary.guestAccountDue - summary.advance), pendingChange: pending ? { attemptToken: paymentToken(pending.id, await ports.bookingEnv()), expiresAt: Number(pending.expires_at) } : null };
        }, async () => ({ propertyId: (await authorizedBooking(db, token, now)).s.property.id, purpose: "history", readOnly: true }));
    }
    async function cancelPublicReservation(db: PmsBookingRepository<Effect>, slug: string, token: string, env: BookingEnv, now = ports.clock.now()) {
        const current = await manageBooking(db, token, now);
        if (current.state === 'cancelled')
            return current;
        const quote = await db.publicChangeQuote(token, 'cancel', { items: current.rooms.filter(st => st.state === 'reserved').map(st => ({ stayId: st.id })) }, env, now);
        await db.confirmBookingChange(slug, token, quote.token, ports.uuid(), env, now);
        return manageBooking(db, token, now);
    }
    async function recoverBooking(db: PmsBookingRepository<Effect>, slug: string, raw: unknown, env: BookingEnv, now = ports.clock.now()) {
        const v = z
            .object({
            number: z.string().trim().min(1).max(100),
            email: z.string().email().max(254),
        })
            .parse(raw);
        await db.transaction(async (tx) => {
            const matches = await tx.readPmsRecords3({ number: v.number, email: v.email });
            if (matches[0] && await tx.purchaseAllowed(matches[0].property_id, "history")) {
                const s = await bookingSnapshot(tx, matches[0].property_id);
                await queueMail(tx, s, matches[0].id, slug, "access", env, now);
            }
        });
        return {
            message: "If these details match a booking, an access link will be emailed to you",
        };
    }
    async function bookingRateLimit(db: PmsBookingUnit<Effect>, key: string, limit = 30, now = ports.clock.now()) {
        const bucket = Math.floor(now / 60000), row = await db.readPmsBookingLimits1({ fact0: ports.hash(key), bucket: bucket });
        if (Number(row?.count) > limit)
            throw ports.error(429, "PMS_RATE_LIMIT", "Too many requests. Please wait a minute");
    }
    async function staffBookingSearch(actor: WorkContext, propertyId: string, operation: "availability" | "quote", input: unknown) {
        ports.require(actor, "pms.reservations");
        const write = await ports.forActor(actor);
        if (actor.departmentId ||
            actor.beta ||
            !(await write.readPmsProperties2({ propertyId: propertyId, companyId: actor.companyId, fact2: (actor.branchId ? [actor.branchId] : []), actor: actor })))
            throw ports.error(404, "PMS_NOT_FOUND", "Hotel not found in this workspace");
        await write.requirePurchase({ propertyId, purpose: "history", actor, permission: "pms.reservations" });
        if (operation === "availability") {
            const search = searchSchema.parse(input), s = await bookingSnapshot(write, propertyId, search);
            return bookingOffers(s, search, false, (input as {
                blockId?: string;
            }).blockId);
        }
        const v = z
            .object({
            rooms: z.array(roomSelectionSchema).min(1).max(50),
            blockId: z.string().default(""),
        })
            .parse(input);
        const search = {
            ...v.rooms[0],
            arrival: v.rooms.map((l) => l.arrival).sort()[0],
            departure: v.rooms
                .map((l) => l.departure)
                .sort()
                .at(-1)!,
        };
        return bookingQuote(await bookingSnapshot(write, propertyId, search), v.rooms, v.blockId);
    }
    function bookingError(error: unknown) {
        return error instanceof PmsError
            ? ports.error(error.status, error.code, error.message)
            : error;
    }
    async function deliverBookingEmails(db: PmsBookingRepository<Effect>, env: BookingEnv, send: typeof fetch = ports.sendMail, now = ports.clock.now()) {
        if (!env.apiKey || !env.from)
            return;
        await db.writePmsBookingLimits1({ fact0: Math.floor(now / 60000) - 60 });
        for (let i = 0; i < 10; i++) {
            const mail = await db.readPmsBookingEmails1({ fact0: now + 120000, now: now, now2: now });
            if (!mail)
                break;
            try {
                await db.requirePurchase({ propertyId: mail.property_id, purpose: "history" });
                // Keep the provider request body stable across retries, even if staff edit a guest.
                await db.writePmsBookingTokens2({ fact0: now + 7 * 86400000, token_id: mail.token_id });
                const response = await send("https://api.resend.com/emails", {
                    method: "POST",
                    headers: {
                        Authorization: `Bearer ${env.apiKey}`,
                        "Content-Type": "application/json",
                        "Idempotency-Key": `pms-booking-${mail.id}`,
                    },
                    body: ports.unprotectMail(mail.payload_json, env, mail.id),
                    signal: ports.timeout(),
                });
                if (!response.ok)
                    throw new Error("EMAIL_DELIVERY_FAILED");
                await db.writePmsBookingEmails2({ now: now, id: mail.id });
            }
            catch (error) {
                const code = ports.isBusinessError(error) ? error.code : "";
                if (["PACKAGE_REQUIRED", "PACKAGE_INACTIVE", "PACKAGE_DRAINING", "DEPENDENCY_REQUIRED", "PACKAGE_CAPABILITY_CHANGED", "PACKAGE_ENFORCEMENT_ENVIRONMENT", "PACKAGE_ENFORCEMENT_CONFIGURATION"].includes(code))
                    await db.writePmsBookingEmailsBlocked({ id: mail.id, code, nextAt: now + 60000 });
                else await db.writePmsBookingEmails3({ fact0: mail.attempts >= 8 ? "failed" : "pending", fact1: now + Math.min(3600000, 1000 * 2 ** mail.attempts), id: mail.id });
            }
        }
    }
    return { bookingSnapshot, bookingCatalog, publicOffers, publicQuote, persist, queueMail, bookingSummary, createPublicReservation, bookingCheckout, bookingPaymentStatus, reconcileBookingPayment, drainBookingPayments, authorizedBooking, manageBooking, cancelPublicReservation, recoverBooking, bookingRateLimit, staffBookingSearch, bookingError, deliverBookingEmails };
}
