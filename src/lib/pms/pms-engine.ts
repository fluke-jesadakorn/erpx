import {availableReceiptSources,captureReceiptAllocationCredit} from "@/lib/pms/pms-receipt-funding";
import { dailyRoomReport, readiness, nightBlockers } from "@/lib/pms/pms-read";
export { dailyRoomReport, readiness, nightBlockers } from "@/lib/pms/pms-read";
import { PmsError } from "@/lib/pms/pms-error";
export { PmsError } from "@/lib/pms/pms-error";
import { availability, groupPickupCount, stayGroupAllowance } from "@/lib/pms/pms-inventory-domain";
export { availability } from "@/lib/pms/pms-inventory-domain";
import {
  nightlyPrices,
  bookingQuote,
  bookingSettingsSchema,
  childBandsSchema,
  collectionPolicySchema,
  guestInputSchema,
  roomSelectionSchema,
} from "@/lib/pms/pms-booking-domain";
import { z } from "zod";
import {
  amounts,
  dates,
  dateField,
  defaultConfig,
  idField,
  moneyField,
  pmsAccountKeys,
  plusDays,
  proportion,
  record,
  records,
  textField,
  type AccountKey,
  type DataMap,
  type Entry,
  type Kind,
  type PmsRecord,
  type Snapshot,
} from "@/server/modules/pms/public/rules/pms";

export function need(ok: unknown, code: string, message: string): asserts ok {
  if (!ok) throw new PmsError(code, message);
}
const fail = (message: string) => new PmsError("PMS_INVALID_STATE", message);
const ref = z.object({ id: idField });
const reason = textField.min(1);
const amount = moneyField.refine(
  (n) => n > 0,
  "Enter an amount greater than zero",
);
const method = z.enum(["cash", "transfer", "edc"]);
export type Posting = {
  source: string;
  recordId: string;
  date: string;
  lines: Array<{ account: AccountKey | "refundPayable" | "unallocated" | "cancellationFee"; debit: number; credit: number }>;
};
export type StockEffect = {
  id: string;
  itemId: string;
  productId: string;
  warehouseId: string;
  quantityMilli: number;
  date: string;
  description: string;
};
export type Effects = {
  postings: Posting[];
  stock: StockEffect[];
  documents: Array<{
    id: string;
    kind: "invoice" | "receipt" | "refund" | "credit_note";
    guestAccountId: string;
    entryId?: string;
    amount: number;
  }>;
  sources: Array<{ key: string; id: string }>;
  nightJob?: string;
};
export type EngineActor = {
  userId: string;
  personId?: string;
  now: number;
  uuid: () => string;
  independent: (other: string) => boolean;
  verifiedBank?: boolean;
};
export function executePms(
  s: Snapshot,
  op: string,
  input: unknown,
  actor: EngineActor,
): { id: string; effects: Effects } {
  const effects: Effects = {
    postings: [],
    stock: [],
    documents: [],
    sources: [],
  };
  const p = s.property,
    c = p.config;
  s.asOf = actor.now;
  const uuid = actor.uuid,
    now = actor.now,
    user = actor.userId;
  function add<K extends Kind>(
    kind: K,
    data: DataMap[K],
    id = uuid(),
  ): PmsRecord<K> {
    const r = { id, kind, data, version: 1, updatedAt: now };
    s.records.push(r);
    return r;
  }
  function get<K extends Kind>(kind: K, id: string) {
    try {
      return record(s, kind, id);
    } catch {
      throw new PmsError(
        "PMS_RECORD_NOT_FOUND",
        "Record not found in this hotel",
        404,
      );
    }
  }
  function touch(r: PmsRecord) {
    r.version++;
    r.updatedAt = now;
  }
  function post(id: string, suffix: string, lines: Posting["lines"]) {
    const nonzero = lines.filter((l) => l.debit || l.credit);
    if (!nonzero.length) return;
    need(
      c.approvedBy,
      "PMS_SETUP_REQUIRED",
      "Have Finance approve the chart of accounts first",
    );
    need(
      nonzero.every(
        (l) =>
          Number.isSafeInteger(l.debit) &&
          Number.isSafeInteger(l.credit) &&
          l.debit >= 0 &&
          l.credit >= 0 &&
          !!l.debit !== !!l.credit,
      ) &&
        nonzero.reduce(
          (n, l) => n + BigInt(l.debit) - BigInt(l.credit),
          BigInt(0),
        ) === BigInt(0),
      "PMS_UNBALANCED",
      "Journal is unbalanced",
    );
    effects.postings.push({
      source: `${id}:${suffix}`,
      recordId: id,
      date: p.businessDate,
      lines: nonzero,
    });
  }
  const pair = (debit: AccountKey, credit: AccountKey, n: number) => [
    { account: debit, debit: n, credit: 0 },
    { account: credit, debit: 0, credit: n },
  ];
  function entry(guestAccountId: string, values: Partial<Entry>) {
    return add("entry", {
      guestAccountId,
      stayId: "",
      date: p.businessDate,
      kind: "charge",
      description: "",
      net: 0,
      service: 0,
      tax: 0,
      total: 0,
      method: "",
      reference: "",
      sourceKey: "",
      originalId: "",
      shiftId: "",
      actorId: user,
      at: now,
      ...values,
    });
  }
  function openGuestAccount(id: string) {
    const f = get("guest_account", id);
    need(
      f.data.state === "open" || f.data.state === "credit",
      "PMS_GUEST_ACCOUNT_CLOSED",
      "The bill is closed",
    );
    return f;
  }
  function noRefund(guestAccountId: string) {
    need(
      !(get("guest_account", guestAccountId).data.refundReserved ?? 0) && !(get("guest_account", guestAccountId).data.depositRefundReserved ?? 0) &&
      !records(s, "refund").some(
        (r) =>
          r.data.guestAccountId === guestAccountId &&
          ["requested", "approved"].includes(r.data.state),
      ),
      "PMS_REFUND_PENDING",
      "Complete pending refunds first",
    );
  }
  function funding(guestAccountId: string, pocket: "advance" | "deposit", n: number) {
    const result: Array<{ entryId: string; amount: number }> = [];
    for(const source of availableReceiptSources(s,guestAccountId,pocket)){
      const take=Math.min(n,source.amount);if(take>0){result.push({entryId:source.entryId,amount:take});n-=take;}if(!n)break;
    }
    need(
      n === 0,
      "PMS_RECEIPT_TRACE_REQUIRED",
      "Original payment receipts do not cover the full amount",
    );
    return result;
  }
  function shift(id: string) {
    const r = get("shift", id);
    need(
      r.data.state === "open" &&
        r.data.userId === user &&
        r.data.date === p.businessDate,
      "PMS_SHIFT_REQUIRED",
      "Open your shift for the current business date first",
    );
    return r;
  }
  function allocate(f: PmsRecord<"guest_account">) {
    const a = Math.min(f.data.due, Math.max(0, f.data.advance - (f.data.refundReserved ?? 0)));
    if (!a) return;
    const tax =
      a === f.data.advance
        ? f.data.advanceTax
        : proportion(f.data.advanceTax, a, f.data.advance);
    const e = entry(f.id, {
      kind: "allocation",
      description: "Allocate advance payment",
      funding: funding(f.id,"advance",a),
      total: a,
      tax,
    });
    post(e.id, "allocation", [
      ...pair("advance", "receivable", a),
      ...pair("tax", "prepaidTax", tax),
    ]);
    f.data.advance -= a;
    f.data.advanceTax -= tax;
    f.data.due -= a;
    touch(f);
  }
  function charge(
    guestAccountId: string,
    values: {
      description: string;
      net: number;
      service: number;
      tax: number;
      total: number;
      stayId?: string;
      sourceKey?: string;
      cancellationFee?:boolean;
    },
  ) {
    const f = openGuestAccount(guestAccountId);
    noRefund(guestAccountId);
    const prior = values.sourceKey
      ? records(s, "entry").find((r) => r.data.sourceKey === values.sourceKey)
      : null;
    if (prior) return prior;
    need(
      !f.data.invoiceId,
      "PMS_INVOICE_ISSUED",
      "An invoice has been issued. Add new items to a new bill",
    );
    need(
      values.total > 0,
      "PMS_ZERO_CHARGE",
      "The charge must be greater than zero",
    );
    const e = entry(guestAccountId, { ...values, date: p.businessDate });
    post(e.id, "charge", [
      { account: "receivable", debit: values.total, credit: 0 },
      { account: values.cancellationFee?"cancellationFee":"revenue", debit: 0, credit: values.net },
      { account: "service", debit: 0, credit: values.service },
      { account: "tax", debit: 0, credit: values.tax },
    ]);
    f.data.due += values.total;
    touch(f);
    allocate(f);
    if (values.sourceKey)
      effects.sources.push({ key: values.sourceKey, id: e.id });
    return e;
  }
  function guestAccount(guestId: string, reservationId: string, name: string) {
    return add("guest_account", {
      guestId,
      reservationId,
      name,
      partyId: "",
      state: "open",
      due: 0,
      advance: 0,
      advanceTax: 0,
      deposit: 0,
    });
  }
  function task(
    roomId: string,
    type: DataMap["task"]["type"],
    stayId = "",
    date = p.businessDate,
  ) {
    const old = records(s, "task").find(
      (r) =>
        r.data.roomId === roomId &&
        r.data.type === type &&
        r.data.date === date &&
        r.data.stayId === stayId &&
        (r.data.roomCycle ?? 0) ===
          (get("room", roomId).data.cleaningCycle ?? 0),
    );
    if (old) return old;
    return add("task", {
      roomCycle: get("room", roomId).data.cleaningCycle ?? 0,
      roomId,
      type,
      stayId,
      date,
      assigneeId: "",
      priority: type === "departure" ? "urgent" : "normal",
      state: "pending",
      checklist:
        type === "maintenance"
          ? ["Repair complete", "Safety test"]
          : [
              "Bathroom",
              "Bed and linen",
              "Floor and dust",
              "Minibar",
              "Equipment and safety",
            ],
      checked: [],
      notes: "",
      completedBy: "",
      inspectedBy: "",
    });
  }
  function dirty(room: PmsRecord<"room">) {
    room.data.cleanliness = "dirty";
    room.data.cleaningCycle = (room.data.cleaningCycle ?? 0) + 1;
    room.data.dnd = false;
    touch(room);
    for (const t of records(s, "task").filter(
      (t) =>
        t.data.roomId === room.id &&
        !["done", "superseded"].includes(t.data.state) &&
        t.data.type !== "maintenance",
    )) {
      t.data.state = "superseded";
      touch(t);
    }
  }
  function prices(
    rateId: string,
    typeId: string,
    from: string,
    to: string,
    adults: number,
    childAges: number[] = [],
  ) {
    return nightlyPrices(s, {
      rateId,
      typeId,
      arrival: from,
      departure: to,
      adults,
      childAges,
      quantity: 1,
    });
  }
  function checkCapacity(
    typeId: string,
    from: string,
    to: string,
    excludeStay = "",
    groupId = "",
  ) {
    for (const date of dates(from, to)) {
      const allowance = excludeStay && !groupId ? stayGroupAllowance(s, excludeStay, typeId, date) : null;
      need(allowance !== 0, "PMS_BLOCK_UNAVAILABLE", `Group quota is full on ${date}`);
      need(
        availability(s, typeId, date, excludeStay, groupId).available + (allowance ?? 0) > 0,
        "PMS_SOLD_OUT",
        `Not enough rooms on ${date}`,
      );
    }
  }
  function checkRoom(
    id: string,
    stayId: string,
    typeId: string,
    from: string,
    to: string,
    checkClean = false,
  ) {
    const r = get("room", id);
    need(
      r.data.active && r.data.typeId === typeId,
      "PMS_ROOM_TYPE",
      "Room type does not match or the room is inactive",
    );
    need(
      !records(s, "block").some(
        (b) =>
          b.data.kind === "maintenance" &&
          b.data.state === "active" &&
          b.data.roomId === id &&
          b.data.from < to,
      ),
      "PMS_ROOM_BLOCKED",
      "Room is under maintenance",
    );
    need(
      !records(s, "stay").some(
        (x) =>
          x.id !== stayId &&
          ["reserved", "in_house"].includes(x.data.state) &&
          x.data.segments.some(
            (seg) => seg.roomId === id && seg.from < to && seg.to > from,
          ),
      ),
      "PMS_ROOM_OVERLAP",
      "This room is assigned to another stay",
    );
    if (checkClean)
      need(
        r.data.cleanliness === "inspected" &&
          !records(s, "stay").some(
            (x) =>
              x.id !== stayId &&
              x.data.state === "in_house" &&
              x.data.roomId === id,
          ),
        "PMS_ROOM_NOT_READY",
        "Room has not passed inspection or has a guest",
      );
    return r;
  }
  function postNights(stay: PmsRecord<"stay">, before: string) {
    for (const price of stay.data.prices.filter((n) => n.date < before)) {
      charge(stay.data.guestAccountId, {
        ...price,
        description: `Room charge ${price.date}`,
        stayId: stay.id,
        sourceKey: `night:${stay.id}:${price.date}`,
      });
    }
  }
  function reconcileCapacity() {
    for (const type of records(s, "room_type")) {
      const stays = records(s, "stay").filter(
        (r) =>
          ["reserved", "in_house"].includes(r.data.state),
      );
      const blocks = records(s, "block").filter(
        (r) =>
          r.data.state === "active" &&
          (r.data.typeId === type.id || r.data.roomId),
      );
      const days = new Set([
        ...stays.flatMap((r) => dates(r.data.arrival, r.data.departure)),
        ...blocks.flatMap((r) => dates(r.data.from, r.data.to)),
      ]);
      for (const date of days)
        need(
          availability(s, type.id, date).available >= 0,
          "PMS_SOLD_OUT",
          `Not enough rooms on ${date}`,
        );
    }
  }
  if (p.state === "closing" && !["night.step", "night.resume"].includes(op))
    throw new PmsError(
      "PMS_DAY_CLOSING",
      "Day close is in progress. Please wait for it to finish",
    );
  if (
    !op.endsWith(".save") &&
    !["property.enable", "accounting.approve", "online_accounting.prepare", "online_accounting.approve", "booking_fees.prepare", "booking_fees.approve"].includes(op)
  )
    need(
      p.state !== "setup",
      "PMS_SETUP_REQUIRED",
      "Review readiness and open the hotel first",
    );
  let resultId = p.id;
  switch (op) {
    case "booking.retry_email":
      ref.parse(input);
      break;
    case "booking.save": {
      const v = bookingSettingsSchema.parse(input);
      for (const id of v.rateIds)
        need(
          get("rate", id).data.market === "public",
          "PMS_PRIVATE_RATE",
          "Only public rates can be published",
        );
      for (const room of v.roomContent) get("room_type", room.typeId);
      if (v.published) {
        need(
          p.state === "open" && !readiness(s).length,
          "PMS_SETUP_REQUIRED",
          "Open the hotel before publishing",
        );
        need(
          v.rateIds.length > 0 && v.roomContent.length > 0,
          "PMS_CONTENT_REQUIRED",
          "Select public rates and describe rooms",
        );
        for (const id of v.rateIds)
          need(
            v.roomContent.some((x) => x.typeId === get("rate", id).data.typeId),
            "PMS_CONTENT_REQUIRED",
            "Describe every published room type",
          );
      }
      c.booking = v;
      break;
    }
    case "property.save": {
      const v = z
        .object({
          name: textField.min(1),
          country: z.string().length(2),
          timezone: textField.min(1),
          checkIn: z.string().regex(/^\d{2}:\d{2}$/),
          checkOut: z.string().regex(/^\d{2}:\d{2}$/),
          cancellationPolicy: textField,
        })
        .parse(input);
      try {
        new Intl.DateTimeFormat("en", { timeZone: v.timezone });
      } catch {
        throw fail("Invalid timezone");
      }
      p.name = v.name;
      Object.assign(c, {
        country: v.country,
        timezone: v.timezone,
        checkIn: v.checkIn,
        checkOut: v.checkOut,
        cancellationPolicy: v.cancellationPolicy,
      });
      break;
    }
    case "booking_fees.prepare": {
      const v=z.object({account:idField}).parse(input);
      need(!c.bookingFees?.approvedBy&&!Object.values(c.accounts).includes(v.account),"PMS_FEE_ACCOUNT_INVALID","Choose a separate cancellation revenue account before approval");
      c.bookingFees={...v,preparedBy:user,approvedBy:null};break;
    }
    case "booking_fees.approve": {
      need(c.bookingFees&&actor.independent(c.bookingFees.preparedBy),"PMS_INDEPENDENT_APPROVER","A different Finance user must approve cancellation accounting");
      c.bookingFees.approvedBy=user;break;
    }
    case "online_accounting.prepare": {
      const v = z.object({ refundPayable: idField, unallocated: idField }).parse(input);
      need(v.refundPayable !== v.unallocated && !Object.values(c.accounts).includes(v.refundPayable) && !Object.values(c.accounts).includes(v.unallocated), "PMS_ACCOUNTS_DISTINCT", "Use distinct liability accounts for refunds and unallocated receipts");
      need(!c.onlineAccounting?.approvedBy, "PMS_ACCOUNTING_IN_USE", "Approved online mappings are immutable");
      c.onlineAccounting = { ...v, preparedBy: user, approvedBy: null };
      break;
    }
    case "online_accounting.approve": {
      need(c.onlineAccounting && actor.independent(c.onlineAccounting.preparedBy), "PMS_INDEPENDENT_APPROVER", "A different Finance user must approve the online liability accounts");
      c.onlineAccounting.approvedBy = user;
      break;
    }
    case "accounting.save": {
      const v = z
        .object({
          chartId: idField,
          accounts: z.object(
            Object.fromEntries(
              pmsAccountKeys.map((k) => [k, idField]),
            ) as Record<AccountKey, typeof idField>,
          ),
          taxBps: z.number().int().min(0).max(10000),
          serviceBps: z.number().int().min(0).max(10000),
          taxInclusive: z.boolean(),
          advanceTax: z.boolean(),
        })
        .parse(input);
      need(
        !records(s, "entry").length,
        "PMS_ACCOUNTING_IN_USE",
        "The account is already active. Use the same hotel and approved chart",
      );
      Object.assign(c, v, {
        preparedBy: user,
        approvedBy: null,
        revision: c.revision + 1,
      });
      p.state = "setup";
      break;
    }
    case "accounting.approve":
      need(
        c.preparedBy && actor.independent(c.preparedBy),
        "PMS_INDEPENDENT_APPROVER",
        "The approver must be different from the account configurator",
      );
      need(
        c.chartId && pmsAccountKeys.every((k) => c.accounts[k]),
        "PMS_SETUP_REQUIRED",
        "Complete the account setup",
      );
      c.approvedBy = user;
      break;
    case "property.enable": {
      const issues = readiness(s);
      need(!issues.length, "PMS_SETUP_REQUIRED", issues.join(" · "));
      p.state = "open";
      break;
    }
    case "room_type.save": {
      const v = z
        .object({
          id: idField.optional(),
          name: textField.min(1),
          capacity: z.number().int().min(1).max(50),
          amenities: textField.default(""),
        })
        .parse(input);
      if (v.id) {
        const r = get("room_type", v.id);
        need(
          !records(s, "stay").some(
            (x) =>
              x.data.typeId === r.id &&
              ["reserved", "in_house"].includes(x.data.state) &&
              x.data.adults + (x.data.childAges?.length ?? 0) > v.capacity,
          ),
          "PMS_CAPACITY_IN_USE",
          "Capacity is lower than the number of booked guests",
        );
        r.data = v;
        touch(r);
        resultId = r.id;
      } else resultId = add("room_type", v).id;
      break;
    }
    case "room.save": {
      const v = z
        .object({
          id: idField.optional(),
          number: textField.min(1),
          typeId: idField,
          building: textField.default(""),
          floor: textField.default(""),
          active: z.boolean().default(true),
        })
        .parse(input);
      get("room_type", v.typeId);
      need(
        !records(s, "room").some(
          (r) => r.id !== v.id && r.data.number === v.number,
        ),
        "PMS_ROOM_NUMBER_EXISTS",
        "Duplicate room number",
      );
      if (v.id) {
        const r = get("room", v.id);
        need(
          !records(s, "stay").some(
            (x) =>
              ["reserved", "in_house"].includes(x.data.state) &&
              x.data.segments.some((seg) => seg.roomId === r.id),
          ) ||
            (v.active && v.typeId === r.data.typeId),
          "PMS_ROOM_IN_USE",
          "Room is assigned to a stay",
        );
        Object.assign(r.data, v);
        touch(r);
        resultId = r.id;
      } else
        resultId = add("room", {
          ...v,
          cleanliness: "dirty",
          dnd: false,
          observedOccupied: null,
        }).id;
      reconcileCapacity();
      break;
    }
    case "rate.save": {
      const v = z
        .object({
          id: idField.optional(),
          name: textField.min(1),
          typeId: idField,
          from: dateField,
          to: dateField,
          price: amount,
          extraAdult: moneyField.default(0),
          childBands: childBandsSchema.optional(),
          collection: collectionPolicySchema.optional(),
          includedGuests: z.number().int().min(1).max(50).default(2),
          minNights: z.number().int().min(1).max(730).default(1),
          maxNights: z.number().int().min(1).max(730).default(365),
          stopSell: z.boolean().default(false),
          closedArrival: z.boolean().default(false),
          closedDeparture: z.boolean().default(false),
          market: z.enum(["public", "company", "agent"]).default("public"),
          partyId: textField.default(""),
          package: textField.default(""),
          active: z.boolean().default(true),
        })
        .parse(input);
      get("room_type", v.typeId);
      need(
        v.to >= v.from && v.maxNights >= v.minNights,
        "PMS_INVALID_DATES",
        "Invalid rate period or number of nights",
      );
      if (v.id) {
        const r = get("rate", v.id);
        r.data = {
          ...v,
          childBands: v.childBands ?? r.data.childBands,
          calendar: r.data.calendar,
          collection: v.collection ?? r.data.collection,
        };
        touch(r);
        resultId = r.id;
      } else resultId = add("rate", v).id;
      break;
    }
    case "rate.calendar": {
      const v = z
          .object({
            id: idField,
            from: dateField,
            to: dateField,
            price: amount,
            stopSell: z.boolean(),
            closedArrival: z.boolean(),
            closedDeparture: z.boolean(),
          })
          .parse(input),
        r = get("rate", v.id);
      need(v.to >= v.from, "PMS_INVALID_DATES", "Invalid rate period");
      const days = dates(v.from, plusDays(v.to, 1));
      r.data.calendar = [
        ...(r.data.calendar ?? []).filter((n) => !days.includes(n.date)),
        ...days.map((date) => ({
          date,
          price: v.price,
          stopSell: v.stopSell,
          closedArrival: v.closedArrival,
          closedDeparture: v.closedDeparture,
        })),
      ];
      touch(r);
      resultId = r.id;
      break;
    }
    case "guest.save": {
      const v = z
        .object({
          id: idField.optional(),
          name: textField.min(1),
          email: z.union([z.email(), z.literal("")]).default(""),
          phone: textField.default(""),
          nationality: textField.default(""),
          documentRef: textField.default(""),
          notes: textField.default(""),
          preferredLanguage: z.string().trim().max(40).optional(),
          roomPreference: z.string().trim().max(200).optional(),
        })
        .parse(input);
      if (v.id) {
        const r = get("guest", v.id);
        r.data = {...r.data,...v};
        touch(r);
        resultId = r.id;
      } else resultId = add("guest", v).id;
      break;
    }
    case "guest.service_request": {
      const v = z.object({ guestId: idField, requestId: idField.optional(), text: z.string().trim().min(1).max(500), state: z.enum(["open", "completed", "cancelled"]).default("open") }).parse(input);
      const guest = get("guest", v.guestId);
      const requests = guest.data.serviceRequests ?? [];
      if (v.requestId) {
        const request = requests.find(row => row.id === v.requestId);
        need(request, "PMS_REQUEST_NOT_FOUND", "Service request not found for this guest");
        need(request.state === "open", "PMS_REQUEST_CLOSED", "Closed requests cannot be rewritten");
        Object.assign(request, { text: v.text, state: v.state, updatedAt: now, updatedBy: user });
        resultId = request.id;
      } else {
        need(v.state === "open", "PMS_REQUEST_INVALID_STATE", "New service requests start open");
        need(requests.length < 100, "PMS_REQUEST_LIMIT", "Export and review this guest's requests before adding more");
        const request = { id: uuid(), text: v.text, state: v.state, createdAt: now, createdBy: user, updatedAt: now, updatedBy: user };
        requests.push(request); resultId = request.id;
      }
      guest.data.serviceRequests = requests; touch(guest);
      break;
    }
    case "reservation.create": {
      const v = z
        .object({
          guestId: idField.optional(),
          guest: guestInputSchema.optional(),
          source: textField.min(1),
          campaignCode: z.string().trim().max(60).default(''),
          externalRef: textField.default(""),
          groupName: textField.default(""),
          notes: textField.default(""),
          blockId: textField.default(""),
          rooms: z.array(roomSelectionSchema).min(1).max(50),
          quote: z.string().max(100000).optional(),
          paymentPending: z.boolean().default(false),
        })
        .parse(input);
      const quoted = bookingQuote(s, v.rooms, v.blockId);
      if (v.quote)
        need(
          v.quote === JSON.stringify(quoted),
          "PMS_QUOTE_CHANGED",
          "Price or policy changed. Review a fresh quote before confirming",
        );
      if (v.guestId) get("guest", v.guestId);
      else {
        need(v.guest, "PMS_GUEST_REQUIRED", "Enter guest details");
        v.guestId = add("guest", {
          ...v.guest,
          nationality: "",
          documentRef: "",
          notes: "",
        }).id;
      }
      need(
        !v.externalRef ||
          !records(s, "reservation").some(
            (r) =>
              r.data.source === v.source &&
              r.data.externalRef === v.externalRef,
          ),
        "PMS_EXTERNAL_REF_EXISTS",
        "A booking reference for this channel already exists",
      );
      const res = add("reservation", {
        number: `PMS-${p.businessDate.replaceAll("-", "")}-${uuid().slice(0, 8)}`,
        guestId: v.guestId,
        source: v.source,
        campaignCode: v.campaignCode,
        externalRef: v.externalRef,
        groupName: v.groupName,
        notes: v.notes,
        state: v.paymentPending ? "payment_pending" : "confirmed",
        createdBy: user,
        bookingPolicy: quoted.policy,
        history: [{ at: now, actor: user, action: v.paymentPending ? "Awaiting online payment" : "Reservation confirmed" }],
      });
      resultId = res.id;
      const block = v.blockId ? get("block", v.blockId) : null;
      for (const line of v.rooms) {
        need(
          line.arrival >= p.businessDate && line.departure > line.arrival,
          "PMS_INVALID_DATES",
          "Stay date cannot be in the past and departure must be after arrival",
        );
        const nightly = prices(
          line.rateId,
          line.typeId,
          line.arrival,
          line.departure,
          line.adults,
          line.childAges,
        );
        for (let i = 0; i < line.quantity; i++) {
          if (block)
            need(
              block.data.kind === "group" &&
                !block.data.bookingChangeId &&
                block.data.state === "active" &&
                block.data.releaseDate > p.businessDate &&
                block.data.typeId === line.typeId &&
                line.arrival >= block.data.from &&
                line.departure <= block.data.to &&
                dates(line.arrival, line.departure).every(date =>
                  groupPickupCount(s, block.id, date) < block.data.quantity),
              "PMS_BLOCK_UNAVAILABLE",
              "Group quota is unavailable or full",
            );
          checkCapacity(
            line.typeId,
            line.arrival,
            line.departure,
            "",
            v.blockId,
          );
          const f = guestAccount(v.guestId, res.id, `${res.data.number} · ${i + 1}`);
          const stay = add("stay", {
            reservationId: res.id,
            guestId: v.guestId,
            typeId: line.typeId,
            rateId: line.rateId,
            roomId: "",
            arrival: line.arrival,
            departure: line.departure,
            adults: line.adults,
            childAges: line.childAges,
            state: "reserved",
            guestAccountId: f.id,
            prices: nightly,
            segments: [],
            bookingPolicy: quoted.lines.find(x => x.rateId === line.rateId&&x.arrival===line.arrival&&x.departure===line.departure)!.policy,
          });
          if (block) {
            block.data.pickedStayIds.push(stay.id);
            touch(block);
          }
        }
      }
      reconcileCapacity();
      if (v.externalRef)
        effects.sources.push({
          key: `reservation:${v.source}:${v.externalRef}`,
          id: res.id,
        });
      break;
    }
    case "block.create": {
      const v = z
        .object({
          name: textField.min(1),
          typeId: idField,
          from: dateField,
          to: dateField,
          releaseDate: dateField,
          quantity: z.number().int().min(1).max(1000),
        })
        .parse(input);
      get("room_type", v.typeId);
      need(
        v.from >= p.businessDate &&
          v.to > v.from &&
          v.releaseDate > p.businessDate &&
          v.releaseDate <= v.from,
        "PMS_INVALID_DATES",
        "Quota return date cannot be after arrival or before the business date",
      );
      resultId = add("block", {
        ...v,
        roomId: "",
        state: "active",
        kind: "group",
        pickedStayIds: [],
      }).id;
      reconcileCapacity();
      break;
    }
    case "block.release": {
      const r = get("block", ref.parse(input).id);
      need(
        r.data.kind === "group" && !r.data.bookingChangeId,
        "PMS_BLOCK_TYPE",
        "Only ordinary group allotments can be released here",
      );
      r.data.state = "released";
      touch(r);
      resultId = r.id;
      break;
    }
    case "reservation.cancel":
    case "reservation.no_show": {
      const v = z
          .object({ id: idField, reason, fee: moneyField.default(0) })
          .parse(input),
        r = get("reservation", v.id);
      need(
        r.data.state === "confirmed" || (op === "reservation.cancel" && r.data.state === "payment_pending" && user === "system:stripe"),
        "PMS_RESERVATION_CLOSED",
        "Booking is not confirmed",
      );
      const stays = records(s, "stay").filter(
        (x) => x.data.reservationId === r.id,
      );
      need(
        stays.every((x) => x.data.state === "reserved"),
        "PMS_ALREADY_CHECKED_IN",
        "Handle the checked-in stay first",
      );
      if (op.endsWith("no_show"))
        need(
          stays.every((x) => x.data.arrival <= p.businessDate),
          "PMS_TOO_EARLY",
          "The stay date has not arrived",
        );
      if (v.fee) {
        need(c.bookingFees?.approvedBy,"PMS_FEE_ACCOUNT_REQUIRED","Finance must approve separate cancellation and no-show fee accounting");
        charge(stays[0].data.guestAccountId, {
          cancellationFee:true,
          ...amounts(v.fee, c),
          description: `Fee ${v.reason}`,
        });
      }
      for (const x of stays) {
        x.data.state = op.endsWith("no_show") ? "no_show" : "cancelled";
        touch(x);
      }
      r.data.state = op.endsWith("no_show") ? "no_show" : "cancelled";
      r.data.notes += `\n${v.reason}`;
      touch(r);
      resultId = r.id;
      break;
    }
    case "stay.cancel":
    case "stay.no_show": {
      const v = z
          .object({ id: idField, reason, fee: moneyField.default(0) })
          .parse(input),
        r = get("stay", v.id);
      need(
        r.data.state === "reserved",
        "PMS_ALREADY_CHECKED_IN",
        "Only rooms that have not checked in can be cancelled",
      );
      if (op === "stay.no_show")
        need(
          r.data.arrival <= p.businessDate,
          "PMS_TOO_EARLY",
          "The stay date has not arrived",
        );
      if (v.fee) {
        need(c.bookingFees?.approvedBy,"PMS_FEE_ACCOUNT_REQUIRED","Finance must approve separate cancellation and no-show fee accounting");
        charge(r.data.guestAccountId, { ...amounts(v.fee, c), cancellationFee:true, description: v.reason });
      }
      r.data.state = op === "stay.no_show" ? "no_show" : "cancelled";
      touch(r);
      const res = get("reservation", r.data.reservationId),
        others = records(s, "stay").filter(
          (st) => st.data.reservationId === res.id,
        );
      if (
        others.every((st) => !["reserved", "in_house"].includes(st.data.state))
      ) {
        res.data.state = others.some((st) => st.data.state === "checked_out")
          ? "completed"
          : r.data.state;
        touch(res);
      }
      resultId = r.id;
      break;
    }
    case "stay.assign":
    case "stay.move": {
      const v = z
          .object({
            id: idField,
            roomId: idField,
            reason: textField.default(""),
          })
          .parse(input),
        r = get("stay", v.id);
      need(
        ["reserved", "in_house"].includes(r.data.state),
        "PMS_STAY_CLOSED",
        "The stay is closed",
      );
      const from =
        r.data.state === "in_house" ? p.businessDate : r.data.arrival;
      need(
        r.data.departure > from,
        "PMS_DEPARTURE_DUE",
        "Post the night before moving rooms",
      );
      const room = checkRoom(
        v.roomId,
        r.id,
        r.data.typeId,
        from,
        r.data.departure,
        r.data.state === "in_house",
      );
      if (
        r.data.roomId &&
        r.data.roomId !== v.roomId &&
        r.data.state === "in_house"
      ) {
        const old = get("room", r.data.roomId);
        dirty(old);
        task(old.id, "departure", r.id);
        r.data.segments = r.data.segments
          .map((seg) => ({ ...seg, to: seg.to > from ? from : seg.to }))
          .filter((seg) => seg.to > seg.from);
      } else if (r.data.state === "reserved") r.data.segments = [];
      if (r.data.roomId !== v.roomId || !r.data.segments.length)
        r.data.segments.push({ roomId: room.id, from, to: r.data.departure });
      r.data.roomId = room.id;
      touch(r);
      resultId = r.id;
      break;
    }
    case "stay.upgrade": {
      const v = z.object({ id: idField, roomId: idField, rateId: idField, reason }).parse(input);
      const r = get("stay", v.id), from = p.businessDate;
      need(r.data.state === "in_house", "PMS_ALREADY_CHECKED_IN", "Upgrade an in-house stay");
      need(r.data.departure > from, "PMS_DEPARTURE_DUE", "Use checkout for a departure today");
      const account = openGuestAccount(r.data.guestAccountId);
      need(!account.data.invoiceId, "PMS_GUEST_ACCOUNT_CLOSED", "Route future charges to an unissued bill before upgrading");
      need(!records(s, "entry").some(entry => entry.data.stayId === r.id &&
        entry.data.sourceKey.startsWith(`night:${r.id}:`) &&
        entry.data.sourceKey.slice(`night:${r.id}:`.length) >= from),
        "PMS_ROOM_CHARGE_POSTED", "Review posted room charges before repricing these nights");
      const target = get("room", v.roomId);
      need(target.data.typeId !== r.data.typeId, "PMS_ROOM_TYPE", "Use Move room within the same room type");
      checkRoom(target.id, r.id, target.data.typeId, from, r.data.departure, true);
      checkCapacity(target.data.typeId, from, r.data.departure, r.id);
      const replacement = prices(v.rateId, target.data.typeId, from, r.data.departure, r.data.adults, r.data.childAges);
      const previous = { typeId: r.data.typeId, rateId: r.data.rateId, roomId: r.data.roomId };
      const periods = r.data.roomTypePeriods ?? [{ typeId: previous.typeId, from: r.data.arrival, to: r.data.departure }];
      r.data.roomTypePeriods = periods.filter(period => period.from < from)
        .map(period => ({ ...period, to: period.to > from ? from : period.to }));
      r.data.roomTypePeriods.push({ typeId: target.data.typeId, from, to: r.data.departure });
      r.data.prices = [...r.data.prices.filter(night => night.date < from), ...replacement];
      r.data.segments = r.data.segments.filter(segment => segment.from < from)
        .map(segment => ({ ...segment, to: segment.to > from ? from : segment.to }));
      r.data.segments.push({ roomId: target.id, from, to: r.data.departure });
      (r.data.upgrades ??= []).push({ effectiveDate: from, at: now, actorId: user, reason: v.reason,
        fromRoomId: previous.roomId, toRoomId: target.id, fromTypeId: previous.typeId,
        toTypeId: target.data.typeId, fromRateId: previous.rateId, toRateId: v.rateId });
      r.data.typeId = target.data.typeId; r.data.rateId = v.rateId; r.data.roomId = target.id;
      if (previous.roomId) { dirty(get("room", previous.roomId)); task(previous.roomId, "departure", r.id); }
      touch(r); reconcileCapacity(); resultId = r.id;
      break;
    }
    case "stay.guest": {
      const v = z.object({ id: idField, guestId: idField }).parse(input),
        r = get("stay", v.id);
      get("guest", v.guestId);
      need(
        ["reserved", "in_house"].includes(r.data.state),
        "PMS_STAY_CLOSED",
        "The stay is closed",
      );
      r.data.guestId = v.guestId;
      touch(r);
      resultId = r.id;
      break;
    }
    case "stay.route_guest_account": {
      const v = z.object({ id: idField, guestAccountId: idField }).parse(input),
        r = get("stay", v.id),
        f = openGuestAccount(v.guestAccountId);
      need(
        ["reserved", "in_house"].includes(r.data.state) &&
          r.data.reservationId === f.data.reservationId &&
          !f.data.invoiceId,
        "PMS_ROUTING_INVALID",
        "Select an open bill in the same booking",
      );
      r.data.guestAccountId = f.id;
      touch(r);
      resultId = r.id;
      break;
    }
    case "stay.check_in": {
      const r = get("stay", ref.parse(input).id);
      need(get("reservation",r.data.reservationId).data.state === "confirmed", "PMS_PAYMENT_PENDING", "Confirm the reservation payment before check-in");
      need(
        r.data.state === "reserved" && r.data.arrival === p.businessDate,
        "PMS_ARRIVAL_DATE",
        "Check-in is allowed on the stay date according to the business date",
      );
      checkRoom(
        r.data.roomId,
        r.id,
        r.data.typeId,
        r.data.arrival,
        r.data.departure,
        true,
      );
      r.data.state = "in_house";
      r.data.checkedInAt = now;
      touch(r);
      resultId = r.id;
      break;
    }
    case "stay.change_dates": {
      const v = z
          .object({
            id: idField,
            arrival: dateField,
            departure: dateField,
            rateId: idField,
            reason,
          })
          .parse(input),
        r = get("stay", v.id);
      need(
        ["reserved", "in_house"].includes(r.data.state),
        "PMS_STAY_CLOSED",
        "The stay is closed",
      );
      need(
        v.departure > v.arrival &&
          (r.data.state === "reserved"
            ? v.arrival >= p.businessDate
            : v.arrival === r.data.arrival && v.departure >= p.businessDate),
        "PMS_INVALID_DATES",
        "Invalid arrival or departure date",
      );
      const from = r.data.state === "in_house" ? p.businessDate : v.arrival;
      need(
        v.departure > from,
        "PMS_USE_CHECKOUT",
        "Use early checkout for a departure today",
      );
      checkCapacity(r.data.typeId, from, v.departure, r.id);
      if (r.data.roomId)
        checkRoom(r.data.roomId, r.id, r.data.typeId, from, v.departure);
      const replacement = prices(
        v.rateId,
        r.data.typeId,
        from,
        v.departure,
        r.data.adults,
        r.data.childAges,
      );
      r.data.prices = [
        ...(r.data.state === "in_house"
          ? r.data.prices.filter((n) => n.date < from)
          : []),
        ...replacement,
      ];
      if (r.data.roomTypePeriods) {
        r.data.roomTypePeriods = r.data.state === "in_house"
          ? r.data.roomTypePeriods.filter(period => period.from < from)
            .map(period => ({ ...period, to: period.to > from ? from : period.to })) : [];
        r.data.roomTypePeriods.push({ typeId: r.data.typeId, from, to: v.departure });
      }
      r.data.arrival = v.arrival;
      r.data.departure = v.departure;
      r.data.rateId = v.rateId;
      r.data.segments = r.data.segments
        .filter((seg) => seg.from < from)
        .map((seg) => ({ ...seg, to: seg.to > from ? from : seg.to }));
      if (r.data.roomId)
        r.data.segments.push({ roomId: r.data.roomId, from, to: v.departure });
      touch(r);
      reconcileCapacity();
      resultId = r.id;
      break;
    }
    case "stay.replace": {
      need(user === "guest:self-service" || user === "system:stripe", "PMS_BOOKING_SERVICE_REQUIRED", "Use the signed booking change service");
      const v = z.object({ id: idField, room: roomSelectionSchema, policy: z.unknown(), nightPrices: z.array(z.object({date:dateField,net:moneyField,service:moneyField,tax:moneyField,total:moneyField})), reason }).parse(input);
      const r = get("stay", v.id);
      need(r.data.state === "reserved", "PMS_ALREADY_CHECKED_IN", "Only future stays can be changed online");
      const replacement = v.nightPrices;
      need(JSON.stringify(replacement.map(n=>n.date)) === JSON.stringify(dates(v.room.arrival,v.room.departure)) && replacement.every(n=>n.net+n.service+n.tax===n.total), "PMS_PRICE_SNAPSHOT_REQUIRED", "Retain the accepted nightly quote");
      checkCapacity(v.room.typeId, v.room.arrival, v.room.departure, r.id);
      Object.assign(r.data, { ...v.room, prices: replacement, roomId: "", segments: [], roomTypePeriods: undefined, bookingPolicy: v.policy });
      touch(r); reconcileCapacity(); resultId = r.id;
      break;
    }
    case "booking.cancellation_fee": {
      need(user==="guest:self-service"||user==="system:stripe","PMS_PROVIDER_AUTHORITY","Booking revision service required");
      const v=z.object({stayId:idField,amount,reference:textField}).parse(input),st=get("stay",v.stayId),policy=st.data.bookingPolicy??get("reservation",st.data.reservationId).data.bookingPolicy;
      need(c.bookingFees?.approvedBy&&st.data.state==="reserved","PMS_FEE_ACCOUNT_REQUIRED","Finance must approve separate cancellation fee accounting");
      const e=charge(st.data.guestAccountId,{...amounts(v.amount,{taxInclusive:true,serviceBps:0,taxBps:policy?.feeTaxBps??c.taxBps}),description:"Cancellation fee",cancellationFee:true,sourceKey:`cancellation-fee:${st.id}:${v.reference}`});resultId=e.id;break;
    }
    case "payment.refund_reserve": {
      need(user === "system:stripe", "PMS_PROVIDER_AUTHORITY", "Provider refund service required");
      const v = z.object({ guestAccountId: idField, amount, pocket:z.enum(["advance","deposit"]).default("advance"), sourceEntryId:textField.default(""), release: z.boolean().default(false) }).parse(input), f = openGuestAccount(v.guestAccountId);
      const key=v.pocket==="advance"?"refundReserved":"depositRefundReserved",reserved = f.data[key] ?? 0;
      need(v.release ? reserved >= v.amount : f.data[v.pocket] - reserved >= v.amount, "PMS_REFUND_EXCEEDS_BALANCE", "Refund exceeds available room advance");
      if(v.sourceEntryId){
        if(!v.release)need((availableReceiptSources(s,f.id,v.pocket).find(p=>p.entryId===v.sourceEntryId)?.amount??0)>=v.amount,"PMS_REFUND_FUNDING_CHANGED","Original receipt funds are unavailable");
        const held=f.data.refundHolds??=[];let part=held.find(p=>p.entryId===v.sourceEntryId&&p.pocket===v.pocket);if(!part){part={entryId:v.sourceEntryId,amount:0,pocket:v.pocket};held.push(part);}need(!v.release||part.amount>=v.amount,"PMS_REFUND_FUNDING_CHANGED","Original hold changed");part.amount+=v.release?-v.amount:v.amount;
      }
      f.data[key] = reserved + (v.release ? -v.amount : v.amount); touch(f); resultId = f.id;
      break;
    }
    case "payment.refund_liability":
    case "payment.refund_settle":
    case "payment.unallocated": {
      need(user === "system:stripe" && c.onlineAccounting?.approvedBy, "PMS_ONLINE_ACCOUNTING_REQUIRED", "Finance must approve online liability mappings");
      const v = z.object({ guestAccountId: idField, amount, reference: textField, sourceEntryId: textField.default(""), pocket:z.enum(["advance","deposit"]).default("advance") }).parse(input), f = op === "payment.refund_liability" ? openGuestAccount(v.guestAccountId) : get("guest_account",v.guestAccountId);
      const e = entry(f.id, { kind: op === "payment.refund_settle" ? "refund" : "allocation", description: op, total: op === "payment.refund_settle" ? -v.amount : v.amount, method: "stripe", reference: v.reference, originalId: op === "payment.refund_settle" ? v.sourceEntryId : "", refundPocket: v.pocket, funding: v.sourceEntryId ? [{ entryId: v.sourceEntryId, amount: v.amount }] : [] });
      if (op === "payment.refund_liability") {
        const key=v.pocket==="advance"?"refundReserved":"depositRefundReserved";
        need((f.data[key] ?? 0) >= v.amount && f.data[v.pocket] >= v.amount, "PMS_REFUND_EXCEEDS_BALANCE", "Reserved advance is unavailable");
        const tax = v.pocket==="deposit"?0:v.amount === f.data.advance ? f.data.advanceTax : proportion(f.data.advanceTax, v.amount, f.data.advance);
        f.data[v.pocket] -= v.amount; f.data.advanceTax -= tax; f.data[key]! -= v.amount;
        const held=f.data.refundHolds?.find(p=>p.entryId===v.sourceEntryId&&p.pocket===v.pocket);if(held){need(held.amount>=v.amount,"PMS_REFUND_FUNDING_CHANGED","Original hold changed");held.amount-=v.amount;}
        post(e.id, "refund-liability", [{ account: v.pocket, debit: v.amount, credit: 0 }, { account: "refundPayable", debit: 0, credit: v.amount }]);
        post(e.id, "refund-tax", pair("tax", "prepaidTax", tax));
      } else if (op === "payment.unallocated") {
        post(e.id, "unallocated", [{ account: "edc", debit: v.amount, credit: 0 }, { account: "unallocated", debit: 0, credit: v.amount }]);
      } else {
        post(e.id, "refund", [{ account: v.sourceEntryId && get("entry",v.sourceEntryId).data.description !== "payment.unallocated" ? "refundPayable" : "unallocated", debit: v.amount, credit: 0 }, { account: "edc", debit: 0, credit: v.amount }]);
        effects.documents.push({ id: uuid(), kind: "refund", guestAccountId: f.id, entryId: e.id, amount: v.amount });
      }
      touch(f); resultId = e.id; break;
    }
    case "stay.check_out": {
      const v = z
          .object({
            id: idField,
            lateFee: moneyField.default(0),
            reason: textField.default(""),
          })
          .parse(input),
        r = get("stay", v.id);
      need(r.data.state === "in_house", "PMS_NOT_IN_HOUSE", "Check in first");
      const until =
        p.businessDate === r.data.arrival
          ? plusDays(p.businessDate, 1)
          : p.businessDate;
      postNights(r, until);
      if (v.lateFee)
        charge(r.data.guestAccountId, {
          ...amounts(v.lateFee, c),
          description: `Late checkout ${v.reason}`,
        });
      const f = openGuestAccount(r.data.guestAccountId);
      need(
        f.data.due === 0 || f.data.state === "credit",
        "PMS_BALANCE_DUE",
        "Settle the balance or approve credit before checkout",
      );
      if (f.data.state === "credit") {
        const credit = get("credit", f.data.creditId!);
        const exposure = records(s, "guest_account")
          .filter(
            (x) =>
              x.data.partyId === f.data.partyId && x.data.state === "credit",
          )
          .reduce((n, x) => n + x.data.due, 0);
        need(
          credit.data.state === "approved" && exposure <= credit.data.limit,
          "PMS_CREDIT_LIMIT",
          "Receivable exceeds approved credit",
        );
      }
      need(
        f.data.advance === 0 && f.data.deposit === 0,
        "PMS_DEPOSIT_REMAINS",
        "Refund or allocate the remaining balance before checkout",
      );
      r.data.state = "checked_out";
      r.data.departure = until;
      r.data.checkedOutAt = now;
      r.data.segments = r.data.segments.map((seg) => ({
        ...seg,
        to: seg.to > until ? until : seg.to,
      }));
      touch(r);
      const room = get("room", r.data.roomId);
      dirty(room);
      task(room.id, "departure", r.id);
      const res = get("reservation", r.data.reservationId);
      if (
        records(s, "stay")
          .filter((x) => x.data.reservationId === res.id)
          .every((x) => !["reserved", "in_house"].includes(x.data.state))
      ) {
        res.data.state = "completed";
        touch(res);
      }
      resultId = r.id;
      break;
    }
    case "stay.preview_checkout": {
      const r = get("stay", ref.parse(input).id);
      need(r.data.state === "in_house", "PMS_NOT_IN_HOUSE", "Check in first");
      postNights(
        r,
        p.businessDate === r.data.arrival
          ? plusDays(p.businessDate, 1)
          : p.businessDate,
      );
      resultId = r.id;
      break;
    }
    case "guest_account.create": {
      const v = z
        .object({ reservationId: idField, name: textField.min(1) })
        .parse(input);
      const r = get("reservation", v.reservationId);
      resultId = guestAccount(r.data.guestId, r.id, v.name).id;
      break;
    }
    case "charge.add": {
      const v = z
        .object({ guestAccountId: idField, price: amount, description: reason })
        .parse(input);
      resultId = charge(v.guestAccountId, {
        ...amounts(v.price, c),
        description: v.description,
      }).id;
      break;
    }
    case "charge.import_pos": {
      const v = z.object({
        guestAccountId: idField,
        stayId: idField,
        sourceKey: textField.min(1),
        description: reason,
        net: moneyField,
        service: moneyField,
        tax: moneyField,
        total: amount,
      }).parse(input);
      need(v.net + v.service + v.tax === v.total,
        "PMS_CHARGE_TOTAL", "The service total does not match its components.");
      resultId = charge(v.guestAccountId, v).id;
      break;
    }
    case "charge.reverse": {
      const v = z.object({ id: idField, reason }).parse(input),
        original = get("entry", v.id);
      need(
        original.data.method!=="opening" && ["charge", "transfer"].includes(original.data.kind) &&
          original.data.total > 0 &&
          !records(s, "entry").some((e) => e.data.originalId === original.id),
        "PMS_ALREADY_REVERSED",
        "This record cannot be reversed or has already been reversed",
      );
      const f = get("guest_account", original.data.guestAccountId);
      if (f.data.state === "closed") f.data.state = "open";
      noRefund(f.id);
      const e = entry(f.id, {
        kind: "adjustment",
        description: v.reason,
        originalId: original.id,
        net: -original.data.net,
        service: -original.data.service,
        tax: -original.data.tax,
        total: -original.data.total,
      });
      post(e.id, "reverse", [
        { account: original.data.cancellationFee?"cancellationFee":"revenue", debit: original.data.net, credit: 0 },
        { account: "service", debit: original.data.service, credit: 0 },
        { account: "tax", debit: original.data.tax, credit: 0 },
        { account: "receivable", debit: 0, credit: original.data.total },
      ]);
      f.data.due -= original.data.total;
      if (f.data.due < 0) {
        post(
          e.id,
          "credit-balance",
          pair("receivable", "advance", -f.data.due),
        );
        captureReceiptAllocationCredit(s, e.id, -f.data.due);
        f.data.advance -= f.data.due;
        f.data.due = 0;
      }
      touch(f);
      effects.documents.push({
        id: uuid(),
        kind: "credit_note",
        guestAccountId: f.id,
        entryId: e.id,
        amount: original.data.total,
      });
      resultId = e.id;
      break;
    }
    case "guest_account.transfer": {
      const v = z
          .object({ entryId: idField, toGuestAccountId: idField, reason })
          .parse(input),
        e = get("entry", v.entryId),
        from = openGuestAccount(e.data.guestAccountId),
        to = openGuestAccount(v.toGuestAccountId);
      need(
        e.data.method!=="opening" && ["charge", "transfer"].includes(e.data.kind) &&
          e.data.total > 0 &&
          from.id !== to.id &&
          !from.data.invoiceId &&
          !to.data.invoiceId,
        "PMS_TRANSFER_INVALID",
        "Select an expense item and a different destination bill",
      );
      noRefund(from.id);
      noRefund(to.id);
      need(
        !records(s, "entry").some((x) => x.data.originalId === e.id),
        "PMS_ENTRY_REFERENCED",
        "This record has already been changed",
      );
      need(
        from.data.due >= e.data.total,
        "PMS_PAID_CHARGE",
        "Only unpaid expense amounts can be transferred",
      );
      const a = entry(from.id, {
        kind: "transfer",
        description: v.reason,
        total: -e.data.total,
        net: -e.data.net,
        service: -e.data.service,
        tax: -e.data.tax,
        originalId: e.id,
      });
      entry(to.id, {
        kind: "transfer",
        description: v.reason,
        total: e.data.total,
        net: e.data.net,
        service: e.data.service,
        tax: e.data.tax,
        originalId: a.id,
      });
      from.data.due -= e.data.total;
      to.data.due += e.data.total;
      touch(from);
      touch(to);
      allocate(to);
      resultId = a.id;
      break;
    }
    case "guest_account.merge": {
      const v = z.object({ id: idField, toGuestAccountId: idField }).parse(input),
        from = openGuestAccount(v.id),
        to = openGuestAccount(v.toGuestAccountId);
      need(
        from.id !== to.id &&
          from.data.reservationId === to.data.reservationId &&
          !from.data.invoiceId &&
          !to.data.invoiceId,
        "PMS_MERGE_INVALID",
        "Only different bills in the same booking can be merged",
      );
      noRefund(from.id);
      noRefund(to.id);
      for (const e of records(s, "entry").filter(
        (x) => x.data.guestAccountId === from.id,
      )) {
        e.data.guestAccountId = to.id;
        touch(e);
      }
      for (const stay of records(s, "stay").filter(
        (x) => x.data.guestAccountId === from.id,
      )) {
        stay.data.guestAccountId = to.id;
        touch(stay);
      }
      to.data.due += from.data.due;
      to.data.advance += from.data.advance;
      to.data.advanceTax += from.data.advanceTax;
      to.data.deposit += from.data.deposit;
      Object.assign(from.data, {
        due: 0,
        advance: 0,
        advanceTax: 0,
        deposit: 0,
        state: "closed",
      });
      touch(from);
      touch(to);
      allocate(to);
      resultId = to.id;
      break;
    }
    case "payment.stripe":
    case "payment.stripe_deposit": {
      need(actor.userId === "system:stripe", "PMS_PROVIDER_AUTHORITY", "Only verified provider payments can use this operation");
      const v = z.object({guestAccountId:idField,amount,reference:textField.min(1)}).parse(input);
      const f=openGuestAccount(v.guestAccountId);
      noRefund(f.id);
      const security=op==='payment.stripe_deposit';
      const e=entry(f.id,{kind:security?"deposit":"payment",description:security?"PromptPay damage deposit":"PromptPay room payment",total:v.amount,method:"stripe",reference:v.reference});
      post(e.id,"receipt",pair("edc",security?"deposit":"advance",v.amount));
      if(security)f.data.deposit+=v.amount;
      else{
       const excess=Math.max(0,v.amount-f.data.due);
       const prepaidTax=c.advanceTax?proportion(excess,c.taxBps,10000+c.taxBps):0;
       f.data.advance+=v.amount;
       f.data.advanceTax+=prepaidTax;
       post(e.id,"advance-tax",pair("prepaidTax","tax",prepaidTax));
       allocate(f);
      }
      touch(f);
      effects.documents.push({id:uuid(),kind:"receipt",guestAccountId:f.id,entryId:e.id,amount:v.amount});
      resultId=e.id;
      break;
    }
    case "payment.stripe_refund":
    case "payment.stripe_deposit_refund": {
      need(actor.userId === "system:stripe", "PMS_PROVIDER_AUTHORITY", "Only verified provider refunds can use this operation");
      const v=z.object({guestAccountId:idField,amount,reference:textField.min(1),reason}).parse(input);
      const f=openGuestAccount(v.guestAccountId);
      const pocket=op==='payment.stripe_deposit_refund'?'deposit':'advance';
      need(v.amount<=f.data[pocket],"PMS_REFUND_EXCEEDS_BALANCE","Refund exceeds the remaining balance");
      const sources=funding(f.id,pocket,v.amount);
      const e=entry(f.id,{funding:sources,refundPocket:pocket,kind:"refund",description:v.reason,total:-v.amount,method:"stripe",reference:v.reference});
      post(e.id,"refund",pair(pocket,"edc",v.amount));
      if(pocket==='advance'){
       const tax=v.amount===f.data.advance?f.data.advanceTax:proportion(f.data.advanceTax,v.amount,f.data.advance);
       post(e.id,"refund-tax",pair("tax","prepaidTax",tax));
       f.data.advanceTax-=tax;
      }
      f.data[pocket]-=v.amount;touch(f);
      effects.documents.push({id:uuid(),kind:"refund",guestAccountId:f.id,entryId:e.id,amount:v.amount});
      resultId=e.id;
      break;
    }
    case "payment.receive":
    case "payment.deposit": {
      const v = z
        .object({
          guestAccountId: idField,
          amount,
          method,
          reference: textField.default(""),
          shiftId: idField,
        })
        .parse(input);
      need(
        v.method === "cash" || v.reference,
        "PMS_REFERENCE_REQUIRED",
        "Enter the transfer or EDC reference",
      );
      const f = openGuestAccount(v.guestAccountId),
        sh = v.method === "transfer" && actor.verifiedBank ? get("shift", v.shiftId) : shift(v.shiftId);
      noRefund(f.id);
      const security = op === "payment.deposit";
      const e = entry(f.id, {
        kind: security ? "deposit" : "payment",
        description: security ? "Damage deposit" : "Room payment",
        total: v.amount,
        method: v.method,
        reference: v.reference,
        shiftId: sh.id,
      });
      const account =
        v.method === "cash" ? "cash" : v.method === "transfer" ? "bank" : "edc";
      post(
        e.id,
        "receipt",
        pair(account, security ? "deposit" : "advance", v.amount),
      );
      if (security) f.data.deposit += v.amount;
      else {
        const excess = Math.max(0, v.amount - f.data.due);
        const prepaidTax = c.advanceTax
          ? proportion(excess, c.taxBps, 10000 + c.taxBps)
          : 0;
        f.data.advance += v.amount;
        f.data.advanceTax += prepaidTax;
        post(e.id, "advance-tax", pair("prepaidTax", "tax", prepaidTax));
        allocate(f);
      }
      touch(f);
      if (v.method === "cash") {
        sh.data.cash += v.amount;
        touch(sh);
      }
      effects.documents.push({
        id: uuid(),
        kind: "receipt",
        guestAccountId: f.id,
        entryId: e.id,
        amount: v.amount,
      });
      resultId = e.id;
      break;
    }
    case "payment.apply_deposit": {
      const v = z.object({ guestAccountId: idField, amount, reason }).parse(input),
        f = openGuestAccount(v.guestAccountId);
      noRefund(f.id);
      need(
        v.amount <= f.data.deposit - (f.data.depositRefundReserved ?? 0) && v.amount <= f.data.due,
        "PMS_DEPOSIT_AMOUNT",
        "A deposit deduction cannot exceed the deposit or balance due",
      );
      const sources = funding(f.id, "deposit", v.amount);
      const e = entry(f.id, {
        kind: "allocation",
        description: v.reason,
        total: v.amount,
        reference: "deposit-application",
        funding: sources,
      });
      post(e.id, "apply-deposit", pair("deposit", "receivable", v.amount));
      f.data.deposit -= v.amount;
      f.data.due -= v.amount;
      touch(f);
      resultId = e.id;
      break;
    }
    case "refund.request": {
      const v = z
          .object({
            guestAccountId: idField,
            amount,
            pocket: z.enum(["advance", "deposit"]),
            reason,
          })
          .parse(input),
        f = openGuestAccount(v.guestAccountId);
      noRefund(f.id);
      need(
        v.amount <= f.data[v.pocket],
        "PMS_REFUND_EXCEEDS_BALANCE",
        "Refund exceeds the remaining balance",
      );
      resultId = add("refund", {
        ...v,
        state: "requested",
        requestedBy: user,
        approvedBy: "",
      }).id;
      break;
    }
    case "refund.approve":
    case "refund.reject": {
      const r = get("refund", ref.parse(input).id);
      need(
        r.data.state === "requested" && actor.independent(r.data.requestedBy),
        "PMS_INDEPENDENT_APPROVER",
        "The approver must be different from the requester",
      );
      r.data.state = op.endsWith("approve") ? "approved" : "rejected";
      r.data.approvedBy = user;
      touch(r);
      resultId = r.id;
      break;
    }
    case "refund.pay": {
      const v = z
          .object({
            id: idField,
            method,
            reference: textField.default(""),
            shiftId: idField,
          })
          .parse(input),
        r = get("refund", v.id);
      need(
        r.data.state === "approved",
        "PMS_REFUND_NOT_APPROVED",
        "Approve the refund first",
      );
      need(
        v.method === "cash" || v.reference,
        "PMS_REFERENCE_REQUIRED",
        "Enter the refund reference",
      );
      const f = openGuestAccount(r.data.guestAccountId),
        sh = shift(v.shiftId),
        n = r.data.amount;
      need(
        n <= f.data[r.data.pocket],
        "PMS_REFUND_EXCEEDS_BALANCE",
        "Insufficient remaining balance",
      );
      const sources = funding(f.id, r.data.pocket, n);
      const e = entry(f.id, {
        funding: sources,
        kind: "refund",
        description: r.data.reason,
        total: -n,
        method: v.method,
        reference: v.reference,
        shiftId: sh.id,
        originalId: r.id,
      });
      post(
        e.id,
        "refund",
        pair(
          r.data.pocket,
          v.method === "cash"
            ? "cash"
            : v.method === "transfer"
              ? "bank"
              : "edc",
          n,
        ),
      );
      if (r.data.pocket === "advance") {
        const tax =
          n === f.data.advance
            ? f.data.advanceTax
            : proportion(f.data.advanceTax, n, f.data.advance);
        post(e.id, "refund-tax", pair("tax", "prepaidTax", tax));
        f.data.advanceTax -= tax;
      }
      f.data[r.data.pocket] -= n;
      r.data.state = "paid";
      touch(r);
      touch(f);
      if (v.method === "cash") {
        need(
          sh.data.cash >= n,
          "PMS_CASH_SHORT",
          "Insufficient shift cash for the refund",
        );
        sh.data.cash -= n;
        touch(sh);
      }
      effects.documents.push({
        id: uuid(),
        kind: "refund",
        guestAccountId: f.id,
        entryId: e.id,
        amount: n,
      });
      resultId = e.id;
      break;
    }
    case "credit.request": {
      const v = z
        .object({
          partyId: idField,
          limit: amount,
          terms: z.number().int().min(1).max(365),
        })
        .parse(input);
      resultId = add("credit", {
        ...v,
        state: "requested",
        requestedBy: user,
        approvedBy: "",
      }).id;
      break;
    }
    case "credit.approve":
    case "credit.reject": {
      const r = get("credit", ref.parse(input).id);
      need(
        r.data.state === "requested" && actor.independent(r.data.requestedBy),
        "PMS_INDEPENDENT_APPROVER",
        "The credit approver must be different",
      );
      r.data.state = op.endsWith("approve") ? "approved" : "rejected";
      r.data.approvedBy = user;
      touch(r);
      resultId = r.id;
      break;
    }
    case "guest_account.credit": {
      const v = z.object({ id: idField, creditId: idField }).parse(input),
        f = openGuestAccount(v.id),
        credit = get("credit", v.creditId);
      need(
        credit.data.state === "approved",
        "PMS_CREDIT_NOT_APPROVED",
        "Credit has not been approved",
      );
      const used = records(s, "guest_account")
        .filter(
          (x) =>
            x.id !== f.id &&
            x.data.partyId === credit.data.partyId &&
            x.data.state === "credit",
        )
        .reduce((n, x) => n + x.data.due, 0);
      need(
        used + f.data.due <= credit.data.limit,
        "PMS_CREDIT_LIMIT",
        "Credit limit exceeded",
      );
      f.data.state = "credit";
      f.data.partyId = credit.data.partyId;
      f.data.creditId = credit.id;
      touch(f);
      resultId = f.id;
      break;
    }
    case "ota.settle": {
      const v = z
          .object({
            guestAccountId: idField,
            gross: amount,
            commission: moneyField,
            fees: moneyField,
            reference: reason,
            shiftId: idField,
          })
          .parse(input),
        f = openGuestAccount(v.guestAccountId);
      shift(v.shiftId);
      need(
        f.data.state === "credit" &&
          v.gross <= f.data.due &&
          v.commission + v.fees < v.gross,
        "PMS_SETTLEMENT_INVALID",
        "Invalid remittance amount or fee",
      );
      const e = entry(f.id, {
        kind: "payment",
        description: "Receive OTA / company remittance",
        total: v.gross,
        net: v.gross - v.commission - v.fees,
        service: v.commission + v.fees,
        reference: v.reference,
        method: "transfer",
        shiftId: v.shiftId,
      });
      post(e.id, "settlement", [
        { account: "bank", debit: v.gross - v.commission - v.fees, credit: 0 },
        { account: "commission", debit: v.commission + v.fees, credit: 0 },
        { account: "receivable", debit: 0, credit: v.gross },
      ]);
      f.data.due -= v.gross;
      touch(f);
      effects.documents.push({
        id: uuid(),
        kind: "receipt",
        guestAccountId: f.id,
        entryId: e.id,
        amount: v.gross,
      });
      resultId = e.id;
      break;
    }
    case "guest_account.close": {
      const f = openGuestAccount(ref.parse(input).id);
      noRefund(f.id);
      need(
        (f.data.due === 0 || f.data.state === "credit") &&
          f.data.advance === 0 &&
          f.data.deposit === 0,
        "PMS_BALANCE_DUE",
        "Handle the balance due and remaining funds before issuing the bill",
      );
      need(
        !records(s, "stay").some(
          (r) =>
            r.data.guestAccountId === f.id &&
            ["reserved", "in_house"].includes(r.data.state),
        ),
        "PMS_STAY_OPEN",
        "Check out or cancel the stay before issuing the bill",
      );
      if (f.data.invoiceId) {
        need(
          f.data.due === 0,
          "PMS_BALANCE_DUE",
          "Receivable balance is not settled",
        );
        f.data.state = "closed";
        touch(f);
        resultId = f.data.invoiceId;
        break;
      }
      const doc = uuid();
      f.data.invoiceId = doc;
      if (f.data.state !== "credit") f.data.state = "closed";
      touch(f);
      effects.documents.push({
        id: doc,
        kind: "invoice",
        guestAccountId: f.id,
        amount: records(s, "entry")
          .filter(
            (e) =>
              e.data.guestAccountId === f.id &&
              ["charge", "adjustment", "transfer"].includes(e.data.kind),
          )
          .reduce((n, e) => n + e.data.total, 0),
      });
      resultId = doc;
      break;
    }
    case "shift.open": {
      const v = z.object({ name: reason, opening: moneyField }).parse(input);
      need(
        !records(s, "shift").some(
          (r) => r.data.userId === user && r.data.state === "open",
        ),
        "PMS_SHIFT_OPEN",
        "You already have an open shift",
      );
      resultId = add("shift", {
        ...v,
        userId: user,
        date: p.businessDate,
        state: "open",
        cash: v.opening,
        counted: 0,
        variance: 0,
        handover: "",
        reviewedBy: "",
        movements: [],
      }).id;
      break;
    }
    case "cash.move": {
      const v = z
          .object({
            shiftId: idField,
            amount: z
              .number()
              .int()
              .min(-999999999)
              .max(999999999)
              .refine((n) => n !== 0),
            reason,
          })
          .parse(input),
        sh = shift(v.shiftId);
      need(
        sh.data.cash + v.amount >= 0,
        "PMS_CASH_SHORT",
        "Insufficient shift cash",
      );
      sh.data.cash += v.amount;
      sh.data.movements.push({ amount: v.amount, reason: v.reason, at: now });
      touch(sh);
      const moveId = uuid();
      post(
        moveId,
        "cash-transfer",
        v.amount > 0
          ? pair("cash", "bank", v.amount)
          : pair("bank", "cash", -v.amount),
      );
      resultId = sh.id;
      break;
    }
    case "shift.close": {
      const v = z
          .object({ id: idField, counted: moneyField, handover: textField })
          .parse(input),
        sh = shift(v.id);
      sh.data.counted = v.counted;
      sh.data.variance = v.counted - sh.data.cash;
      sh.data.handover = v.handover;
      sh.data.state = "closed";
      touch(sh);
      resultId = sh.id;
      break;
    }
    case "shift.review": {
      const sh = get("shift", ref.parse(input).id);
      need(
        sh.data.state === "closed" && actor.independent(sh.data.userId),
        "PMS_INDEPENDENT_APPROVER",
        "A different reviewer must review the shift total",
      );
      if (sh.data.variance)
        post(
          sh.id,
          "variance",
          sh.data.variance > 0
            ? pair("cash", "cashDifference", sh.data.variance)
            : pair("cashDifference", "cash", -sh.data.variance),
        );
      sh.data.state = "reviewed";
      sh.data.reviewedBy = user;
      touch(sh);
      resultId = sh.id;
      break;
    }
    case "task.create": {
      const v = z
        .object({
          roomId: idField,
          type: z.enum(["departure", "stayover", "arrival", "discrepancy"]),
          date: dateField,
        })
        .parse(input);
      get("room", v.roomId);
      resultId = task(v.roomId, v.type, "", v.date).id;
      break;
    }
    case "task.assign": {
      const v = z
          .object({
            id: idField,
            assigneeId: idField,
            priority: z.enum(["normal", "urgent"]),
          })
          .parse(input),
        r = get("task", v.id);
      need(
        !["done", "superseded"].includes(r.data.state),
        "PMS_TASK_DONE",
        "Task has already been inspected",
      );
      r.data.assigneeId = v.assigneeId;
      r.data.priority = v.priority;
      touch(r);
      resultId = r.id;
      break;
    }
    case "task.start":
    case "task.complete": {
      const v = z
          .object({
            id: idField,
            checked: z.array(textField).default([]),
            notes: textField.default(""),
          })
          .parse(input),
        r = get("task", v.id),
        room = get("room", r.data.roomId);
      need(
        (r.data.roomCycle ?? 0) === (room.data.cleaningCycle ?? 0),
        "PMS_CLEANING_STALE",
        "The room has a newer cleaning cycle. Use the latest cycle",
      );
      need(
        r.data.state !== "done" &&
          (!r.data.assigneeId || r.data.assigneeId === user),
        "PMS_TASK_ASSIGNMENT",
        "This task is assigned to another employee or is already complete",
      );
      need(
        !room.data.dnd || r.data.type === "maintenance",
        "PMS_DND",
        "Guest requested Do Not Disturb",
      );
      if (op.endsWith("start")) {
        need(
          r.data.state === "pending",
          "PMS_TASK_STATE",
          "Task has already started",
        );
        r.data.assigneeId = user;
        r.data.state = "working";
      } else {
        need(
          ["working", "pending"].includes(r.data.state) &&
            r.data.checklist.every((x) => v.checked.includes(x)),
          "PMS_CHECKLIST_REQUIRED",
          "Complete the checklist",
        );
        r.data.state = "clean";
        r.data.completedBy = user;
        r.data.checked = v.checked;
        if (!["maintenance", "discrepancy"].includes(r.data.type)) {
          room.data.cleanliness = "clean";
          touch(room);
        }
      }
      r.data.notes = v.notes;
      touch(r);
      resultId = r.id;
      break;
    }
    case "task.inspect": {
      const r = get("task", ref.parse(input).id);
      need(
        (r.data.roomCycle ?? 0) ===
          (get("room", r.data.roomId).data.cleaningCycle ?? 0),
        "PMS_CLEANING_STALE",
        "The room has a newer cleaning cycle",
      );
      need(
        r.data.state === "clean" && actor.independent(r.data.completedBy),
        "PMS_INDEPENDENT_INSPECTOR",
        "A different supervisor must inspect after the task is complete",
      );
      r.data.state = "done";
      r.data.inspectedBy = user;
      touch(r);
      if (!["maintenance", "discrepancy"].includes(r.data.type)) {
        const room = get("room", r.data.roomId);
        room.data.cleanliness = "inspected";
        touch(room);
      }
      resultId = r.id;
      break;
    }
    case "room.dnd": {
      const v = z.object({ id: idField, dnd: z.boolean() }).parse(input),
        r = get("room", v.id);
      r.data.dnd = v.dnd;
      touch(r);
      resultId = r.id;
      break;
    }
    case "room.observe": {
      const v = z.object({ id: idField, occupied: z.boolean() }).parse(input),
        r = get("room", v.id);
      r.data.observedOccupied = v.occupied;
      touch(r);
      const expected = records(s, "stay").some(
        (st) => st.data.roomId === r.id && st.data.state === "in_house",
      );
      if (expected !== v.occupied) task(r.id, "discrepancy");
      resultId = r.id;
      break;
    }
    case "maintenance.open": {
      const v = z
          .object({ roomId: idField, from: dateField, to: dateField, reason })
          .parse(input),
        room = get("room", v.roomId);
      need(
        v.from >= p.businessDate && v.to > v.from,
        "PMS_INVALID_DATES",
        "Invalid maintenance period",
      );
      need(
        !records(s, "stay").some(
          (st) =>
            ["reserved", "in_house"].includes(st.data.state) &&
            st.data.segments.some(
              (seg) =>
                seg.roomId === room.id && seg.from < v.to && seg.to > v.from,
            ),
        ),
        "PMS_ROOM_IN_USE",
        "Move the booking/guest out of the room before maintenance",
      );
      const b = add("block", {
        roomId: room.id,
        typeId: room.data.typeId,
        from: v.from,
        to: v.to,
        name: v.reason,
        quantity: 1,
        releaseDate: v.to,
        state: "active",
        kind: "maintenance",
        pickedStayIds: [],
      });
      task(room.id, "maintenance", b.id);
      reconcileCapacity();
      resultId = b.id;
      break;
    }
    case "maintenance.close": {
      const r = get("block", ref.parse(input).id);
      need(
        r.data.kind === "maintenance" && r.data.state === "active",
        "PMS_BLOCK_TYPE",
        "This is not a maintenance room",
      );
      need(
        records(s, "task").some(
          (t) =>
            t.data.stayId === r.id &&
            t.data.type === "maintenance" &&
            t.data.state === "done",
        ),
        "PMS_INSPECTION_REQUIRED",
        "Repair and inspect before opening the room",
      );
      r.data.state = "released";
      touch(r);
      const room = get("room", r.data.roomId);
      dirty(room);
      task(room.id, "arrival");
      resultId = r.id;
      break;
    }
    case "item.save": {
      const v = z
        .object({
          id: idField.optional(),
          name: reason,
          kind: z.enum(["minibar", "supply", "linen"]),
          productId: idField,
          warehouseId: idField,
          price: moneyField.default(0),
        })
        .parse(input);
      if (v.id) {
        const r = get("item", v.id);
        need(
          r.data.productId === v.productId &&
            r.data.warehouseId === v.warehouseId &&
            r.data.kind === v.kind,
          "PMS_ITEM_IN_USE",
          "Create a new record to change the product, warehouse, or type",
        );
        Object.assign(r.data, v);
        touch(r);
        resultId = r.id;
      } else
        resultId = add("item", {
          ...v,
          clean: 0,
          issued: 0,
          laundry: 0,
          lost: 0,
        }).id;
      break;
    }
    case "minibar.charge":
    case "supply.consume":
    case "linen.receive":
    case "linen.issue":
    case "linen.return":
    case "linen.wash":
    case "linen.clean":
    case "linen.loss": {
      const v = z
          .object({
            itemId: idField,
            quantity: z.number().int().min(1).max(100000),
            roomId: textField.default(""),
            guestAccountId: textField.default(""),
            reason: textField.default(""),
          })
          .parse(input),
        r = get("item", v.itemId);
      if (v.roomId) get("room", v.roomId);
      const kind = op.split(".")[1];
      need(
        op.startsWith("linen.")
          ? r.data.kind === "linen"
          : op === "minibar.charge"
            ? r.data.kind === "minibar"
            : r.data.kind === "supply",
        "PMS_ITEM_TYPE",
        "Record type does not match",
      );
      const move = add("movement", {
        itemId: r.id,
        roomId: v.roomId,
        kind: op,
        quantity: v.quantity,
        reason: v.reason,
        date: p.businessDate,
        at: now,
      });
      resultId = move.id;
      if (op === "minibar.charge") {
        charge(v.guestAccountId, {
          ...amounts(r.data.price * v.quantity, c),
          description: `${r.data.name} × ${v.quantity}`,
          sourceKey: `minibar:${move.id}`,
        });
      }
      if (["minibar.charge", "supply.consume", "linen.receive"].includes(op)) {
        effects.stock.push({
          id: move.id,
          itemId: r.id,
          productId: r.data.productId,
          warehouseId: r.data.warehouseId,
          quantityMilli: -v.quantity * 1000,
          date: p.businessDate,
          description: op,
        });
        if (op === "linen.receive") r.data.clean += v.quantity;
      } else if (kind === "issue") {
        need(
          r.data.clean >= v.quantity,
          "PMS_LINEN_SHORT",
          "Insufficient clean linen",
        );
        r.data.clean -= v.quantity;
        r.data.issued += v.quantity;
      } else if (kind === "return" || kind === "wash") {
        need(
          r.data.issued >= v.quantity,
          "PMS_LINEN_SHORT",
          "Insufficient linen issued",
        );
        r.data.issued -= v.quantity;
        r.data.laundry += v.quantity;
      } else if (kind === "clean") {
        need(
          r.data.laundry >= v.quantity,
          "PMS_LINEN_SHORT",
          "Insufficient linen sent to laundry",
        );
        r.data.laundry -= v.quantity;
        r.data.clean += v.quantity;
      } else if (kind === "loss") {
        need(
          r.data.issued >= v.quantity && v.reason,
          "PMS_LINEN_SHORT",
          "Enter the reason and quantity issued",
        );
        r.data.issued -= v.quantity;
        r.data.lost += v.quantity;
      }
      touch(r);
      break;
    }
    case "lost.save": {
      const v = z
        .object({
          id: idField.optional(),
          description: reason,
          roomId: idField,
          guestId: textField.default(""),
          location: reason,
          notes: textField.default(""),
          foundDate: dateField,
        })
        .parse(input);
      get("room", v.roomId);
      if (v.guestId) get("guest", v.guestId);
      if (v.id) {
        const r = get("lost", v.id);
        need(
          r.data.state === "stored",
          "PMS_LOST_CLOSED",
          "This item has already been returned or disposed of",
        );
        Object.assign(r.data, v);
        touch(r);
        resultId = r.id;
      } else
        resultId = add("lost", { ...v, state: "stored", releasedBy: "" }).id;
      break;
    }
    case "lost.release": {
      const v = z
          .object({
            id: idField,
            state: z.enum(["returned", "disposed"]),
            notes: reason,
          })
          .parse(input),
        r = get("lost", v.id);
      need(
        r.data.state === "stored",
        "PMS_LOST_CLOSED",
        "This record is complete",
      );
      Object.assign(r.data, v, { releasedBy: user });
      touch(r);
      resultId = r.id;
      break;
    }
    case "night.start": {
      const blockers = nightBlockers(s);
      need(!blockers.length, "PMS_NIGHT_BLOCKED", blockers.join(" · "));
      need(
        !records(s, "night").some((r) => r.data.date === p.businessDate),
        "PMS_NIGHT_EXISTS",
        "This business date already has a day-close job",
      );
      const r = add("night", {
        date: p.businessDate,
        state: "pending",
        initiatedBy: user,
        message: "",
      });
      p.state = "closing";
      effects.nightJob = r.id;
      resultId = r.id;
      break;
    }
    case "night.resume": {
      const r = get("night", ref.parse(input).id);
      need(
        r.data.date === p.businessDate &&
          p.state === "closing" &&
          r.data.state !== "completed",
        "PMS_NIGHT_CLOSED",
        "Day-close job is complete",
      );
      r.data.state = "pending";
      r.data.message = "";
      r.data.initiatedBy = user;
      touch(r);
      effects.nightJob = r.id;
      resultId = r.id;
      break;
    }
    case "night.step": {
      const r = get("night", ref.parse(input).id);
      need(
        p.state === "closing" &&
          r.data.date === p.businessDate &&
          r.data.state !== "completed",
        "PMS_NIGHT_CLOSED",
        "Business date has changed",
      );
      const stays = records(s, "stay").filter(
        (st) =>
          st.data.state === "in_house" &&
          st.data.arrival <= p.businessDate &&
          st.data.departure > p.businessDate,
      );
      const outstanding = stays.filter(
        (st) =>
          !records(s, "entry").some(
            (e) => e.data.sourceKey === `night:${st.id}:${p.businessDate}`,
          ),
      );
      for (const st of outstanding.slice(0, 25))
        postNights(st, plusDays(p.businessDate, 1));
      if (outstanding.length <= 25) {
        const metrics = dailyRoomReport(s, p.businessDate);
        r.data.roomMetrics = { capacity: metrics.capacity, sold: metrics.sold };
        for (const st of stays)
          task(st.data.roomId, "stayover", st.id, plusDays(p.businessDate, 1));
        for (const b of records(s, "block").filter(
          (x) =>
            x.data.kind === "group" &&
            x.data.releaseDate <= plusDays(p.businessDate, 1),
        )) {
          b.data.state = "released";
          touch(b);
        }
        r.data.state = "completed";
        r.data.completedAt = now;
        touch(r);
        p.businessDate = plusDays(p.businessDate, 1);
        p.state = "open";
      }
      resultId = r.id;
      break;
    }
    default:
      throw new PmsError("PMS_UNKNOWN_OPERATION", "Command not found", 404);
  }
  if (
    op !== "reservation.create" &&
    (op.startsWith("reservation.") || op.startsWith("stay."))
  ) {
    const target =
      records(s, "reservation").find((r) => r.id === resultId) ??
      records(s, "reservation").find(
        (r) =>
          r.id ===
          records(s, "stay").find((st) => st.id === resultId)?.data
            .reservationId,
      );
    if (target && op !== "stay.preview_checkout") {
      target.data.history = [
        ...(target.data.history ?? []),
        {
          at: now,
          actor: user,
          action: op.replaceAll(".", " ").replaceAll("_", " "),
        },
      ];
      touch(target);
    }
  }
  for (const f of records(s, "guest_account"))
    need(
      [f.data.due, f.data.advance, f.data.advanceTax, f.data.deposit].every(
        (n) => Number.isSafeInteger(n) && n >= 0,
      ),
      "PMS_MONEY_OVERFLOW",
      "Amount is out of range",
    );
  p.version++;
  return { id: resultId, effects };
}

export function newProperty(
  id: string,
  companyId: string,
  branchId: string,
  name: string,
  businessDate: string,
): Snapshot {
  return {
    property: {
      id,
      companyId,
      branchId,
      name,
      businessDate,
      state: "setup",
      version: 1,
      config: structuredClone(defaultConfig),
    },
    records: [],
  };
}
