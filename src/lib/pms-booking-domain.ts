import { z } from "zod";
import {
  amounts,
  dates,
  dateField,
  idField,
  moneyField,
  plusDays,
  record,
  records,
  type Snapshot,
  type BookingPolicy,
} from "./pms";
import { availability } from "./pms-inventory-domain";
import { PmsError } from "./pms-error";
const need = (value: unknown, code: string, message: string) => {
  if (!value) throw new PmsError(code, message);
};
export const childBandsSchema = z
  .array(
    z.object({
      minAge: z.number().int().min(0).max(17),
      maxAge: z.number().int().min(0).max(17),
      price: moneyField,
    }),
  )
  .max(18)
  .superRefine((bands, ctx) => {
    const seen = new Set<number>();
    for (const b of bands) {
      if (b.minAge > b.maxAge)
        ctx.addIssue({
          code: "custom",
          message: "Child age range is reversed",
        });
      for (let age = b.minAge; age <= b.maxAge; age++) {
        if (seen.has(age))
          ctx.addIssue({ code: "custom", message: "Child age ranges overlap" });
        seen.add(age);
      }
    }
  });
export const collectionPolicySchema = z.object({
  method: z.enum(["full", "deposit", "pay_at_hotel"]),
  depositPercent: z.number().int().min(1).max(99).default(30),
  changesAllowed: z.boolean().default(true),
  cancellationsAllowed:z.boolean().optional(),
  cancellationDays:z.number().int().min(0).max(365).optional(),
  cancellationTime:z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/).optional(),
  cancellationFeePercent:z.number().int().min(0).max(100).optional(),
});
const imageSchema = z.object({ publicationId: idField, alt: z.string().trim().min(1).max(500) });
export const bookingSettingsSchema = z.object({
  published: z.boolean(),
  description: z.string().trim().min(1).max(3000),
  address: z.string().trim().min(1).max(500),
  contactEmail: z.string().email().max(254),
  contactPhone: z.string().trim().min(1).max(80),
  cancellationDays: z.number().int().min(0).max(365),
  cancellationTime: z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/),
  images: z.array(imageSchema).max(12).optional(),
  roomContent: z
    .array(
      z.object({
        typeId: idField,
        description: z.string().trim().min(1).max(2000),
        images: z.array(imageSchema).max(12).optional(),
        beds: z.string().trim().max(200).optional(),
        sizeSqm: z.number().positive().max(10000).optional(),
        amenities: z.string().trim().max(2000).optional(),
      }),
    )
    .max(200),
  rateIds: z.array(idField).max(200),
});
export const guestInputSchema = z.object({
  name: z.string().trim().min(1).max(200),
  email: z.string().trim().email().max(254),
  phone: z.string().trim().min(1).max(80),
});
export const roomSelectionSchema = z.object({
  typeId: idField,
  rateId: idField,
  arrival: dateField,
  departure: dateField,
  adults: z.number().int().min(1).max(50),
  childAges: z.array(z.number().int().min(0).max(17)).max(20).default([]),
  quantity: z.number().int().min(1).max(200).default(1),
});
export const searchSchema = z
  .object({
    arrival: dateField,
    departure: dateField,
    adults: z.number().int().min(1).max(50),
    childAges: z.array(z.number().int().min(0).max(17)).max(20).default([]),
    quantity: z.number().int().min(1).max(20).default(1),
  })
  .refine(
    (v) =>
      v.departure > v.arrival && dates(v.arrival, v.departure).length <= 365,
    "Choose a stay between 1 and 365 nights",
  );
export type RoomSelection = z.infer<typeof roomSelectionSchema>;
export type BookingSearch = z.infer<typeof searchSchema>;
export function collectionAmounts(total: number, collection?: import("./pms-model").CollectionPolicy) {
  const policy = collection ?? { method: "pay_at_hotel", depositPercent: 30, changesAllowed: true };
  const dueNow = policy.method === "full" ? total : policy.method === "deposit"
    ? Number((BigInt(total) * BigInt(policy.depositPercent) + BigInt(99)) / BigInt(100)) : 0;
  return { dueNow, dueAtHotel: total - dueNow, collection: policy };
}
export function nightlyPrices(s: Snapshot, line: RoomSelection) {
  const rate = record(s, "rate", line.rateId).data,
    rt = record(s, "room_type", line.typeId).data;
  need(
    line.departure > line.arrival,
    "PMS_INVALID_DATES",
    "Departure must follow arrival",
  );
  const nights = dates(line.arrival, line.departure);
  need(
    rate.active &&
      rate.typeId === line.typeId &&
      !rate.stopSell &&
      !rate.closedArrival &&
      !rate.closedDeparture &&
      line.arrival >= rate.from &&
      line.departure <= plusDays(rate.to, 1),
    "PMS_RATE_UNAVAILABLE",
    "Rate is unavailable for these dates",
  );
  need(
    nights.length >= rate.minNights &&
      nights.length <= rate.maxNights &&
      line.adults + line.childAges.length <= rt.capacity,
    "PMS_RATE_RESTRICTION",
    "Stay length or occupancy exceeds this room's limits",
  );
  need(
    !rate.calendar?.find((n) => n.date === line.arrival)?.closedArrival &&
      !rate.calendar?.find((n) => n.date === line.departure)?.closedDeparture,
    "PMS_RATE_RESTRICTION",
    "Arrival or departure is closed",
  );
  let children = 0;
  for (const age of line.childAges) {
    const band = rate.childBands?.find(
      (b) => b.minAge <= age && age <= b.maxAge,
    );
    need(
      band,
      "PMS_CHILD_RATE_REQUIRED",
      "This rate has no configured price for a child's age",
    );
    children += band!.price;
  }
  return nights.map((date) => {
    const override = rate.calendar?.find((n) => n.date === date);
    need(!override?.stopSell, "PMS_RATE_UNAVAILABLE", `Stop-sell on ${date}`);
    return {
      date,
      ...amounts(
        (override?.price ?? rate.price) +
          Math.max(0, line.adults - rate.includedGuests) * rate.extraAdult +
          children,
        s.property.config,
      ),
    };
  });
}
export function localDeadline(date: string, time: string, timezone: string) {
  const desired = Date.parse(`${date}T${time}:00Z`);
  const format = new Intl.DateTimeFormat("en-GB", {
    timeZone: timezone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hourCycle: "h23",
  });
  const local = (n: number) => {
    const parts = Object.fromEntries(
      format.formatToParts(n).map((p) => [p.type, p.value]),
    );
    return Date.parse(
      `${parts.year}-${parts.month}-${parts.day}T${parts.hour}:${parts.minute}:${parts.second}Z`,
    );
  };
  let result = desired;
  for (let i = 0; i < 4; i++) result += desired - local(result);
  need(
    local(result) === desired,
    "PMS_POLICY_TIME",
    "Cancellation time does not exist on this date; contact the hotel",
  );
  // On the autumn DST overlap use the earlier deadline.
  if (local(result - 3600000) === desired) result -= 3600000;
  return result;
}
function roomPolicy(s:Snapshot,arrival:string,collection:import('./pms-model').CollectionPolicy|undefined):BookingPolicy|undefined{
 const c=s.property.config,b=c.booking;if(!b)return undefined;
 const days=collection?.cancellationDays??b.cancellationDays,time=collection?.cancellationTime??b.cancellationTime,deadlineTime=days===0&&time>c.checkIn?c.checkIn:time,date=plusDays(arrival,-days),fee=collection?.cancellationFeePercent??0;
 return {deadline:localDeadline(date,deadlineTime,c.timezone),timezone:c.timezone,payment:collection?.method&&collection.method!=='pay_at_hotel'?'online':'pay_at_hotel',collection,feeTaxBps:c.taxBps,text:`${collection?.cancellationsAllowed===false?'Contact the hotel for cancellation.':`${fee?`Cancellation fee ${fee}% of the accepted stay total`:'Free cancellation'} until ${date} ${deadlineTime} (${c.timezone}). After this deadline, contact the hotel.`} ${collection?.method&&collection.method!=='pay_at_hotel'?'Pay the amount due now online; the remaining balance is due at the hotel. Room advances are separate from damage deposits.':'Pay at the hotel.'}`};
}
export function bookingQuote(s: Snapshot, raw: unknown, blockId = "") {
  const selections = z.array(roomSelectionSchema).min(1).max(50).parse(raw);
  need(
    s.property.state === "open",
    "PMS_NOT_OPEN",
    "This hotel is temporarily unavailable for bookings",
  );
  const consumed = new Map<string, number>();
  const lines = selections.map((line) => {
    need(
      line.arrival >= s.property.businessDate,
      "PMS_INVALID_DATES",
      "Arrival cannot precede the hotel's business date",
    );
    const nights = nightlyPrices(s, line);
    for (const night of nights) {
      const k = `${line.typeId}:${night.date}`,
        used = consumed.get(k) ?? 0;
      need(
        availability(s, line.typeId, night.date, "", blockId).available >=
          used + line.quantity,
        "PMS_SOLD_OUT",
        `Not enough rooms on ${night.date}`,
      );
      consumed.set(k, used + line.quantity);
    }
    const rate = record(s, "rate", line.rateId).data;
    const total = nights.reduce((n, p) => n + p.total, 0) * line.quantity;
    return {
      ...line,
      roomName: record(s, "room_type", line.typeId).data.name,
      rateName: rate.name,
      inclusions: rate.package,
      policy:roomPolicy(s,line.arrival,rate.collection),
      nights,
      total,
      ...collectionAmounts(nights.reduce((n, p) => n + p.total, 0), rate.collection),
      dueNow: collectionAmounts(nights.reduce((n, p) => n + p.total, 0), rate.collection).dueNow * line.quantity,
      dueAtHotel: collectionAmounts(nights.reduce((n, p) => n + p.total, 0), rate.collection).dueAtHotel * line.quantity,
    };
  });
  const config=s.property.config;
  const policy=lines.map(line=>line.policy).filter((p):p is BookingPolicy=>Boolean(p)).sort((a,b)=>a.deadline-b.deadline)[0];
  const total = lines.reduce((n, line) => n + line.total, 0);
  need(
    Number.isSafeInteger(total),
    "PMS_MONEY_OVERFLOW",
    "Booking total exceeds supported amount",
  );
  const dueNow = lines.reduce((n, line) => n + line.dueNow, 0);
  if (policy) {
    policy.payment = dueNow ? "online" : "pay_at_hotel";
    policy.text = policy.text.replace("Pay at the hotel.", dueNow ? "Pay the amount due now online; the remaining balance is due at the hotel. Room advances are separate from damage deposits." : "Pay at the hotel.");
  }
  return {
    propertyId: s.property.id,
    currency: config.currency,
    lines,
    total,
    dueNow,
    dueAtHotel: total - dueNow,
    ...(policy ? { policy } : {}),
  };
}
export type BookingQuote = ReturnType<typeof bookingQuote>;
export function bookingOffers(
  s: Snapshot,
  raw: unknown,
  publicOnly = false,
  blockId = "",
) {
  const search = searchSchema.parse(raw),
    out: BookingQuote[] = [];
  for (const r of records(s, "rate")) {
    if (
      publicOnly &&
      (r.data.market !== "public" ||
        !s.property.config.booking?.rateIds.includes(r.id))
    )
      continue;
    try {
      out.push(
        bookingQuote(
          s,
          [{ ...search, typeId: r.data.typeId, rateId: r.id }],
          blockId,
        ),
      );
    } catch (error) {
      if (!(error instanceof PmsError)) throw error;
    }
  }
  return out.sort((a, b) => a.total - b.total);
}
export function cancellationEligibility(
  s: Snapshot,
  reservationId: string,
  now: number,
) {
  const r = record(s, "reservation", reservationId),
    stays = records(s, "stay").filter((st) => st.data.reservationId === r.id);
  if (r.data.state === "cancelled")
    return { allowed: false, reason: "This booking is already cancelled" };
  if (s.property.state !== "open")
    return {
      allowed: false,
      reason: "Hotel day close is in progress. Please try again shortly",
    };
  if (!r.data.bookingPolicy || stays.some(st=>!st.data.bookingPolicy && now >= r.data.bookingPolicy!.deadline || st.data.bookingPolicy && (now >= st.data.bookingPolicy.deadline || st.data.bookingPolicy.collection?.cancellationsAllowed === false)))
    return {
      allowed: false,
      reason: "The free-cancellation deadline has passed. Contact the hotel",
    };
  if (
    r.data.state !== "confirmed" ||
    !stays.length ||
    stays.some((st) => st.data.state !== "reserved")
  )
    return {
      allowed: false,
      reason: "Contact the hotel to change this booking",
    };
  const folios = new Set(
    records(s, "folio")
      .filter((f) => f.data.reservationId === r.id)
      .map((f) => f.id),
  );
  if (
    records(s, "entry").some((e) => folios.has(e.data.folioId) && !(e.data.kind==='payment'&&e.data.method==='stripe')) ||
    records(s, "refund").some((e) => folios.has(e.data.folioId))
  )
    return {
      allowed: false,
      reason:
        "This booking has financial activity. Contact the hotel for cancellation",
    };
  return { allowed: true, reason: stays.some(st=>(st.data.bookingPolicy?.collection?.cancellationFeePercent??0)>0) ? "Review cancellation fees and refund before confirming" : "Cancel all rooms without a fee" };
}
