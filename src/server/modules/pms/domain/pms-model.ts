import { APP_CURRENCY, APP_LOCALE } from "@/lib/localization";
export const pmsSections = [
    ["dashboard", "Overview"],
    ["reservations", "Reservations"],
    ["front-desk", "Front Office"],
    ["rooms", "Rooms"],
    ["housekeeping", "Housekeeping"],
    ["folios", "Folios & payments"],
    ["cashier", "Cashier shifts"],
    ["inventory", "Minibar & linen"],
    ["guests", "Guests"],
    ["reports", "Reports"],
    ["night-audit", "Night Audit"],
    ["settings", "Settings"],
] as const;
/** Keep the workspace tabs and the side menu on the same readable sections. */
export function visiblePmsSections(can: (permission: string) => boolean) {
    return pmsSections.filter(([key]) => {
        if (key === "folios" || key === "cashier")
            return can("pms.cashier") || can("pms.reports");
        if (key === "reports")
            return can("pms.reports");
        if (key === "night-audit")
            return can("pms.audit") || can("pms.reports");
        return true;
    });
}
export const pmsAccountKeys = [
    "cash",
    "bank",
    "edc",
    "receivable",
    "advance",
    "deposit",
    "revenue",
    "service",
    "tax",
    "prepaidTax",
    "inventory",
    "consumption",
    "commission",
    "cashDifference",
] as const;
export type AccountKey = (typeof pmsAccountKeys)[number];
export const accountLabels: Record<AccountKey, string> = {
    cash: "Cash",
    bank: "Bank",
    edc: "EDC clearing",
    receivable: "PMS receivables",
    advance: "Room advances",
    deposit: "Damage deposits",
    revenue: "Room / service revenue",
    service: "Service charge revenue",
    tax: "Sales tax",
    prepaidTax: "Unallocated advance tax",
    inventory: "Inventory",
    consumption: "Cost / supplies consumed",
    commission: "Commissions & fees",
    cashDifference: "Cash variance",
};
export type BookingImage = {
    publicationId: string;
    alt: string;
};
export type CollectionPolicy = {
    method: "full" | "deposit" | "pay_at_hotel";
    depositPercent: number;
    changesAllowed: boolean;
    cancellationsAllowed?: boolean;
    cancellationDays?: number;
    cancellationTime?: string;
    cancellationFeePercent?: number;
};
export type BookingSettings = {
    published: boolean;
    description: string;
    address: string;
    contactEmail: string;
    contactPhone: string;
    cancellationDays: number;
    cancellationTime: string;
    images?: BookingImage[];
    roomContent: Array<{
        typeId: string;
        description: string;
        images?: BookingImage[];
        beds?: string;
        sizeSqm?: number;
        amenities?: string;
    }>;
    rateIds: string[];
};
export type BookingPolicy = {
    deadline: number;
    timezone: string;
    text: string;
    payment: "pay_at_hotel" | "online";
    collection?: CollectionPolicy;
    feeTaxBps?: number;
};
export type Config = {
    booking?: BookingSettings;
    country: string;
    currency: string;
    minorUnit: number;
    timezone: string;
    checkIn: string;
    checkOut: string;
    cancellationPolicy: string;
    chartId: string;
    accounts: Record<AccountKey, string>;
    taxBps: number;
    serviceBps: number;
    taxInclusive: boolean;
    advanceTax: boolean;
    preparedBy: string;
    approvedBy: string | null;
    revision: number;
    bookingFees?: {
        account: string;
        preparedBy: string;
        approvedBy: string | null;
    };
    onlineAccounting?: {
        refundPayable: string;
        unallocated: string;
        preparedBy: string;
        approvedBy: string | null;
    };
};
export type Property = {
    id: string;
    companyId: string;
    branchId: string;
    name: string;
    businessDate: string;
    state: "setup" | "open" | "closing";
    version: number;
    config: Config;
};
export type Kind = "room_type" | "room" | "rate" | "guest" | "reservation" | "stay" | "folio" | "entry" | "task" | "block" | "shift" | "refund" | "item" | "movement" | "lost" | "credit" | "night";
export type RoomType = {
    name: string;
    capacity: number;
    amenities: string;
};
export type Room = {
    number: string;
    typeId: string;
    building: string;
    floor: string;
    active: boolean;
    cleanliness: "dirty" | "clean" | "inspected";
    dnd: boolean;
    observedOccupied: boolean | null;
    cleaningCycle?: number;
};
export type Rate = {
    name: string;
    typeId: string;
    from: string;
    to: string;
    price: number;
    extraAdult: number;
    includedGuests: number;
    minNights: number;
    maxNights: number;
    stopSell: boolean;
    closedArrival: boolean;
    closedDeparture: boolean;
    market: "public" | "company" | "agent";
    partyId: string;
    package: string;
    active: boolean;
    childBands?: Array<{
        minAge: number;
        maxAge: number;
        price: number;
    }>;
    calendar?: Array<{
        date: string;
        price: number;
        stopSell: boolean;
        closedArrival: boolean;
        closedDeparture: boolean;
    }>;
    collection?: CollectionPolicy;
};
export type Guest = {
    name: string;
    email: string;
    phone: string;
    nationality: string;
    documentRef: string;
    notes: string;
};
export type Reservation = {
    number: string;
    guestId: string;
    source: string;
    campaignCode?: string;
    externalRef: string;
    groupName: string;
    notes: string;
    state: "payment_pending" | "confirmed" | "cancelled" | "no_show" | "completed";
    createdBy: string;
    bookingPolicy?: BookingPolicy;
    history?: Array<{
        at: number;
        actor: string;
        action: string;
    }>;
};
export type NightPrice = {
    date: string;
    net: number;
    service: number;
    tax: number;
    total: number;
};
export type Stay = {
    reservationId: string;
    guestId: string;
    typeId: string;
    rateId: string;
    roomId: string;
    arrival: string;
    departure: string;
    adults: number;
    childAges?: number[];
    state: "reserved" | "in_house" | "checked_out" | "cancelled" | "no_show";
    folioId: string;
    prices: NightPrice[];
    segments: Array<{
        roomId: string;
        from: string;
        to: string;
    }>;
    checkedInAt?: number;
    checkedOutAt?: number;
    bookingPolicy?: BookingPolicy;
};
export type Folio = {
    name: string;
    guestId: string;
    reservationId: string;
    partyId: string;
    state: "open" | "closed" | "credit";
    due: number;
    advance: number;
    advanceTax: number;
    deposit: number;
    refundReserved?: number;
    depositRefundReserved?: number;
    refundHolds?: Array<{
        entryId: string;
        amount: number;
        pocket: "advance" | "deposit";
    }>;
    invoiceId?: string;
    creditId?: string;
};
export type Entry = {
    cancellationFee?: boolean;
    folioId: string;
    stayId: string;
    date: string;
    kind: "charge" | "payment" | "allocation" | "refund" | "adjustment" | "transfer" | "deposit";
    description: string;
    net: number;
    service: number;
    tax: number;
    total: number;
    method: string;
    reference: string;
    sourceKey: string;
    originalId: string;
    shiftId: string;
    actorId: string;
    at: number;
    funding?: Array<{
        entryId: string;
        amount: number;
    }>;
    refundPocket?: "advance" | "deposit";
    /** Server-owned position in one provider payment's immutable minor units. */
    providerPaymentOffset?: number;
};
export type Task = {
    roomId: string;
    stayId: string;
    type: "departure" | "stayover" | "arrival" | "maintenance" | "discrepancy";
    date: string;
    assigneeId: string;
    priority: "normal" | "urgent";
    state: "pending" | "working" | "clean" | "done" | "superseded";
    checklist: string[];
    checked: string[];
    notes: string;
    completedBy: string;
    inspectedBy: string;
    roomCycle?: number;
};
export type Block = {
    expiresAt?: number;
    bookingChangeId?: string;
    name: string;
    typeId: string;
    roomId: string;
    from: string;
    to: string;
    quantity: number;
    releaseDate: string;
    state: "active" | "released";
    kind: "group" | "maintenance";
    pickedStayIds: string[];
};
export type Shift = {
    name: string;
    userId: string;
    date: string;
    state: "open" | "closed" | "reviewed";
    opening: number;
    cash: number;
    counted: number;
    variance: number;
    handover: string;
    reviewedBy: string;
    movements: Array<{
        amount: number;
        reason: string;
        at: number;
    }>;
};
export type Refund = {
    folioId: string;
    amount: number;
    pocket: "advance" | "deposit";
    reason: string;
    state: "requested" | "approved" | "rejected" | "paid";
    requestedBy: string;
    approvedBy: string;
};
export type Item = {
    name: string;
    kind: "minibar" | "supply" | "linen";
    productId: string;
    warehouseId: string;
    price: number;
    clean: number;
    issued: number;
    laundry: number;
    lost: number;
};
export type Movement = {
    itemId: string;
    roomId: string;
    kind: string;
    quantity: number;
    reason: string;
    date: string;
    at: number;
};
export type Lost = {
    description: string;
    roomId: string;
    guestId: string;
    location: string;
    state: "stored" | "returned" | "disposed";
    notes: string;
    foundDate: string;
    releasedBy: string;
};
export type Credit = {
    partyId: string;
    limit: number;
    terms: number;
    state: "requested" | "approved" | "rejected";
    requestedBy: string;
    approvedBy: string;
};
export type Night = {
    date: string;
    state: "pending" | "blocked" | "completed";
    initiatedBy: string;
    message: string;
    completedAt?: number;
    roomMetrics?: {
        capacity: number;
        sold: number;
    };
};
export type DataMap = {
    room_type: RoomType;
    room: Room;
    rate: Rate;
    guest: Guest;
    reservation: Reservation;
    stay: Stay;
    folio: Folio;
    entry: Entry;
    task: Task;
    block: Block;
    shift: Shift;
    refund: Refund;
    item: Item;
    movement: Movement;
    lost: Lost;
    credit: Credit;
    night: Night;
};
export type PmsRecord<K extends Kind = Kind> = {
    id: string;
    kind: K;
    version: number;
    updatedAt: number;
    data: DataMap[K];
};
export type Snapshot = {
    property: Property;
    records: PmsRecord[];
    asOf?: number;
};
export function records<K extends Kind>(s: Snapshot, kind: K): PmsRecord<K>[] {
    return s.records.filter((r) => r.kind === kind) as PmsRecord<K>[];
}
export function record<K extends Kind>(s: Snapshot, kind: K, id: string): PmsRecord<K> {
    const r = records(s, kind).find((r) => r.id === id);
    if (!r)
        throw new Error("PMS_RECORD_NOT_FOUND");
    return r;
}
export function plusDays(date: string, n: number) {
    const d = new Date(`${date}T12:00:00Z`);
    d.setUTCDate(d.getUTCDate() + n);
    return d.toISOString().slice(0, 10);
}
export function dates(from: string, to: string) {
    const out: string[] = [];
    for (let d = from; d < to; d = plusDays(d, 1)) {
        if (out.length >= 730)
            throw new Error("PMS_STAY_TOO_LONG");
        out.push(d);
    }
    return out;
}
export function amounts(price: number, config: Pick<Config, "taxBps" | "serviceBps" | "taxInclusive">): Omit<NightPrice, "date"> {
    const round = (n: bigint, d: bigint) => Number((n + d / BigInt(2)) / d);
    const net = config.taxInclusive
        ? round(BigInt(price) * BigInt(100000000), BigInt(10000 + config.serviceBps) * BigInt(10000 + config.taxBps))
        : price;
    const service = round(BigInt(net) * BigInt(config.serviceBps), BigInt(10000));
    const tax = config.taxInclusive
        ? price - net - service
        : round(BigInt(net + service) * BigInt(config.taxBps), BigInt(10000));
    return { net, service, tax, total: net + service + tax };
}
export function money(n: number, currency = "THB") {
    return new Intl.NumberFormat(APP_LOCALE, {
        style: "currency",
        currency: currency || APP_CURRENCY,
        currencyDisplay: "code",
    }).format(n / 100);
}
export const defaultConfig: Config = {
    country: "TH",
    currency: "THB",
    minorUnit: 2,
    timezone: "Asia/Bangkok",
    checkIn: "14:00",
    checkOut: "12:00",
    cancellationPolicy: "",
    chartId: "",
    accounts: Object.fromEntries(pmsAccountKeys.map((k) => [k, ""])) as Record<AccountKey, string>,
    taxBps: 0,
    serviceBps: 0,
    taxInclusive: false,
    advanceTax: true,
    preparedBy: "",
    approvedBy: null,
    revision: 1,
};
export const operationPermission = (op: string) => op === "booking_fees.prepare" ? "pms.accounting.prepare" :
    op === "booking_fees.approve" ? "pms.accounting.approve" :
        op === "online_accounting.prepare" ? "pms.accounting.prepare" :
            op === "online_accounting.approve" ? "pms.accounting.approve" :
                op === "payment.verify_transfer" ? "finance.approve" :
                    op === "charge.correction.request" ? "pms.cashier" :
                        op === "charge.correction.review" || op === "charge.reverse" ? "pms.approve" :
                            op === "property.create" ||
                                op === "property.save" ||
                                op === "booking.save" ||
                                op === "booking.retry_email" ||
                                op === "property.enable" ||
                                op === "room_type.save" ||
                                op === "room.save" ||
                                op.startsWith("rate.") ||
                                op === "item.save"
                                ? "pms.configure"
                                : op === "accounting.save" || op === "accounting.bootstrap"
                                    ? "pms.accounting.prepare"
                                    : op === "accounting.approve"
                                        ? "pms.accounting.approve"
                                        : op === "refund.approve" ||
                                            op === "refund.reject" ||
                                            op === "shift.review" ||
                                            op === "credit.approve" ||
                                            op === "credit.reject"
                                            ? "pms.approve"
                                            : op.startsWith("night.")
                                                ? "pms.audit"
                                                : op === "task.inspect" || op === "task.assign"
                                                    ? "pms.inspect"
                                                    : op === "maintenance.open" || op === "maintenance.close"
                                                        ? "pms.maintenance"
                                                        : op.startsWith("task.") ||
                                                            op.startsWith("room.observe") ||
                                                            op === "room.dnd" ||
                                                            op.startsWith("linen.") ||
                                                            op.startsWith("lost.") ||
                                                            op === "supply.consume"
                                                            ? "pms.housekeeping"
                                                            : op.startsWith("payment.") ||
                                                                op.startsWith("refund.") ||
                                                                op.startsWith("shift.") ||
                                                                op.startsWith("cash.") ||
                                                                op.startsWith("folio.") ||
                                                                op === "charge.add" ||
                                                                op === "charge.reverse" ||
                                                                op === "minibar.charge" ||
                                                                op.startsWith("ota.") ||
                                                                op.startsWith("credit.")
                                                                ? "pms.cashier"
                                                                : op.startsWith("stay.")
                                                                    ? "pms.frontdesk"
                                                                    : "pms.reservations";
/** Round an integer proportion without floating-point multiplication. */
export function proportion(value: number, numerator: number, denominator: number) {
    if (!Number.isSafeInteger(value) ||
        !Number.isSafeInteger(numerator) ||
        !Number.isSafeInteger(denominator) ||
        denominator <= 0)
        throw new Error("PMS_INVALID_PROPORTION");
    return Number((BigInt(value) * BigInt(numerator) + BigInt(denominator) / BigInt(2)) /
        BigInt(denominator));
}
