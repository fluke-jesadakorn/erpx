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
import { availability } from "./pms-inventory";
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
export const bookingSettingsSchema = z.object({
  published: z.boolean(),
  description: z.string().trim().min(1).max(3000),
  address: z.string().trim().min(1).max(500),
  contactEmail: z.string().email().max(254),
  contactPhone: z.string().trim().min(1).max(80),
  cancellationDays: z.number().int().min(0).max(365),
  cancellationTime: z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/),
  roomContent: z
    .array(
      z.object({
        typeId: idField,
        description: z.string().trim().min(1).max(2000),
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
    return {
      ...line,
      roomName: record(s, "room_type", line.typeId).data.name,
      rateName: rate.name,
      inclusions: rate.package,
      nights,
      total: nights.reduce((n, p) => n + p.total, 0) * line.quantity,
    };
  });
  const config = s.property.config,
    settings = config.booking;
  let policy: BookingPolicy | undefined;
  if (settings) {
    const arrival = selections.map((l) => l.arrival).sort()[0];
    const deadlineTime =
      settings.cancellationDays === 0 &&
      settings.cancellationTime > config.checkIn
        ? config.checkIn
        : settings.cancellationTime;
    policy = {
      deadline: localDeadline(
        plusDays(arrival, -settings.cancellationDays),
        deadlineTime,
        config.timezone,
      ),
      timezone: config.timezone,
      text: `Free cancellation until ${plusDays(arrival, -settings.cancellationDays)} ${deadlineTime} (${config.timezone}). After this deadline, contact the hotel. Pay at the hotel.`,
      payment: "pay_at_hotel",
    };
  }
  const total = lines.reduce((n, line) => n + line.total, 0);
  need(
    Number.isSafeInteger(total),
    "PMS_MONEY_OVERFLOW",
    "Booking total exceeds supported amount",
  );
  return {
    propertyId: s.property.id,
    currency: config.currency,
    lines,
    total,
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
  if (!r.data.bookingPolicy || now >= r.data.bookingPolicy.deadline)
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
  return { allowed: true, reason: "Cancel all rooms without a fee" };
}
