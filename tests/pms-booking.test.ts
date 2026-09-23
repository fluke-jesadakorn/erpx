import { cancellationEligibility, localDeadline } from "../src/lib/pms-booking";
import {
  createPublicReservation,
  publicQuote,
  cancelPublicReservation,
  manageBooking,
  bookingCatalog,
  bookingSnapshot,
  publicOffers,
  deliverBookingEmails,
  bookingRateLimit,
  recoverBooking,
} from "../src/server/pms-booking";
import { createHmac } from "node:crypto";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { pmsCommand, loadPms } from "../src/server/pms";
import { records, record, pmsAccountKeys, type Config } from "../src/lib/pms";
import { availability } from "../src/lib/pms-engine";
import type { WorkContext } from "../src/server/authorization";

vi.mock("../src/server/runtime", async () => {
  const { env } = await import("./runtime-env");
  return { holdingResources: async () => ({ write: env.H001_DB }) };
});
const db = env.H001_DB,
  key = () => crypto.randomUUID();
let f: Awaited<ReturnType<typeof seedExpenseCompany>>,
  owner: WorkContext,
  reviewer: WorkContext,
  propertyId: string,
  typeId: string,
  rateId: string,
  roomId: string,
  guestId: string,
  accounts: Config["accounts"];
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
async function snapshot() {
  return loadPms(db, owner, propertyId);
}
async function cmd(operation: string, input: unknown = {}, actor = owner) {
  const s = await snapshot();
  return pmsCommand(
    db,
    actor,
    { propertyId, expectedVersion: s.property.version, operation, input },
    key(),
  );
}
beforeEach(async () => {
  f = await seedExpenseCompany(db, env.H001_FILES);
  owner = f.owner;
  reviewer = await f.member("hotel-reviewer", "finance_approver");
  propertyId = (
    await pmsCommand(
      db,
      owner,
      {
        operation: "property.create",
        input: {
          name: "Test Hotel",
          branchId: f.branch,
          businessDate: "2026-09-01",
        },
      },
      key(),
    )
  ).id;
  accounts = {} as Config["accounts"];
  for (const [i, k] of pmsAccountKeys.entries()) {
    const id = key();
    accounts[k] = id;
    const category = ["revenue", "service"].includes(k)
      ? "revenue"
      : ["advance", "deposit", "tax"].includes(k)
        ? "liability"
        : ["consumption", "commission", "cashDifference"].includes(k)
          ? "expense"
          : "asset";
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)",
      )
      .bind(id, f.chart, f.company, `PMS-${i}`, k, category)
      .run();
  }
  await cmd("accounting.save", {
    chartId: f.chart,
    accounts,
    taxBps: 700,
    serviceBps: 0,
    taxInclusive: false,
    advanceTax: true,
  });
  await cmd("accounting.approve", {}, reviewer);
  typeId = (
    await cmd("room_type.save", {
      name: "Deluxe",
      capacity: 2,
      amenities: "Shower",
    })
  ).id;
  roomId = (await cmd("room.save", { number: "101", typeId, active: true })).id;
  rateId = (
    await cmd("rate.save", {
      name: "Flexible",
      typeId,
      from: "2026-09-01",
      to: "2026-12-31",
      price: 10000,
    })
  ).id;
  await cmd("property.enable");
  vi.stubEnv("RESEND_API_KEY", "booking-test-key");
  vi.stubEnv("RESEND_FROM", "Hotel <hotel@example.test>");
  await cmd("booking.save", settings());
  guestId = (await cmd("guest.save", { name: "Test Guest" })).id;
});

const now = Date.parse("2026-09-01T00:00:00Z");
const delivery = {
  secret: "test-booking-secret-32-characters-long",
  origin: "https://hotel.example.test",
  apiKey: "test-key",
  from: "hotel@example.test",
};
function settings(published = true) {
  return {
    published,
    description: "A quiet hotel",
    address: "Bangkok",
    contactEmail: "hotel@example.test",
    contactPhone: "0123456789",
    cancellationDays: 1,
    cancellationTime: "18:00",
    rateIds: [rateId],
    roomContent: [{ typeId, description: "A spacious room" }],
  };
}
function selection(extra: Record<string, unknown> = {}) {
  return {
    typeId,
    rateId,
    arrival: "2026-09-05",
    departure: "2026-09-07",
    adults: 2,
    quantity: 1,
    childAges: [],
    ...extra,
  };
}
async function request(extra: Record<string, unknown> = {}) {
  const q = await publicQuote(
    db,
    propertyId,
    [selection(extra)],
    delivery,
    now,
  );
  return {
    propertyId,
    quoteToken: q.token,
    guest: {
      name: "Online Guest",
      email: "guest@example.test",
      phone: "0123456789",
    },
    notes: "",
  };
}
async function token(id: string) {
  const row = await db
    .prepare(
      "SELECT id FROM pms_booking_tokens WHERE reservation_id=? ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id)
    .first<{ id: string }>();
  return `${row!.id}.${createHmac("sha256", delivery.secret).update(`pms-booking:access:${row!.id}`).digest("base64url")}`;
}
it("shares exact nightly prices, child occupancy and policy snapshots", async () => {
  await cmd("room_type.save", { id: typeId, name: "Deluxe", capacity: 3 });
  const rate = record(await snapshot(), "rate", rateId).data;
  await cmd("rate.save", {
    ...rate,
    id: rateId,
    childBands: [
      { minAge: 0, maxAge: 5, price: 500 },
      { minAge: 6, maxAge: 17, price: 1000 },
    ],
  });
  const q = await publicQuote(
    db,
    propertyId,
    [selection({ childAges: [4] })],
    delivery,
    now,
  );
  expect(q.quote.total).toBe(22470);
  expect(q.quote.policy?.deadline).toBe(Date.parse("2026-09-04T11:00:00Z"));
  await expect(
    publicQuote(
      db,
      propertyId,
      [selection({ childAges: [4, 9] })],
      delivery,
      now,
    ),
  ).rejects.toThrow(/occupancy/);
  const input = await request({ childAges: [4] }),
    b = await createPublicReservation(db, "acme", input, key(), delivery, now);
  await cmd("rate.save", { ...rate, id: rateId, price: 20000 });
  const booked = await manageBooking(db, await token(b.id), now);
  expect(booked.rooms[0].nights[0].total).toBe(11235);
  expect(booked.rooms[0].childAges).toEqual([4]);
});
it("atomically confirms and queues email without creating money entries", async () => {
  const b = await createPublicReservation(
      db,
      "acme",
      await request(),
      key(),
      delivery,
      now,
    ),
    s = await snapshot();
  expect(records(s, "stay")).toHaveLength(1);
  expect(records(s, "folio")).toHaveLength(1);
  expect(records(s, "entry")).toHaveLength(0);
  expect(b.number).toMatch(/^PMS-/);
  expect(availability(s, typeId, "2026-09-05").available).toBe(0);
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM pms_booking_emails WHERE reservation_id=?",
      )
      .bind(b.id)
      .first("n"),
  ).toBe(1);
});
it("replays identical submissions and rejects a reused key with new input", async () => {
  const input = await request(),
    k = key(),
    a = await createPublicReservation(db, "acme", input, k, delivery, now),
    b = await createPublicReservation(
      db,
      "acme",
      input,
      k,
      delivery,
      now + 3600000,
    );
  expect(a.id).toBe(b.id);
  await expect(
    createPublicReservation(
      db,
      "acme",
      { ...input, notes: "different" },
      k,
      delivery,
      now,
    ),
  ).rejects.toThrow(/request key/);
  expect(records(await snapshot(), "reservation")).toHaveLength(1);
});
it("serializes staff and guest bookings for the last room", async () => {
  const input = await request(),
    s = await snapshot();
  const results = await Promise.allSettled([
    createPublicReservation(db, "acme", input, key(), delivery, now),
    pmsCommand(
      db,
      owner,
      {
        propertyId,
        expectedVersion: s.property.version,
        operation: "reservation.create",
        input: { guestId, source: "phone", rooms: [selection()] },
      },
      key(),
    ),
  ]);
  expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect(records(await snapshot(), "stay")).toHaveLength(1);
});
it("rejects stale price, stale policy, forged and expired quotes", async () => {
  const input = await request(),
    rate = record(await snapshot(), "rate", rateId).data;
  await cmd("rate.save", { ...rate, id: rateId, price: 11000 });
  await expect(
    createPublicReservation(db, "acme", input, key(), delivery, now),
  ).rejects.toThrow(/changed/);
  const changed = await request();
  await cmd("booking.save", { ...settings(), cancellationDays: 2 });
  await expect(
    createPublicReservation(db, "acme", changed, key(), delivery, now),
  ).rejects.toThrow(/changed/);
  await expect(
    createPublicReservation(
      db,
      "acme",
      { ...changed, quoteToken: changed.quoteToken + "x" },
      key(),
      delivery,
      now,
    ),
  ).rejects.toThrow(/fresh quote/);
  await expect(
    createPublicReservation(
      db,
      "acme",
      await request(),
      key(),
      delivery,
      now + 16 * 60000,
    ),
  ).rejects.toThrow(/expired/);
});
it("requires explicit publication and prevents selling private rates", async () => {
  await cmd("booking.save", settings(false));
  expect((await bookingCatalog(db)).some((h) => h.id === propertyId)).toBe(
    false,
  );
  await expect(
    publicQuote(db, propertyId, [selection()], delivery, now),
  ).rejects.toThrow(/Hotel not found/);
  await cmd("booking.save", settings());
  const rate = record(await snapshot(), "rate", rateId).data;
  await cmd("rate.save", { ...rate, id: rateId, market: "company" });
  expect((await publicOffers(db, selection(), propertyId))[0].offers).toEqual(
    [],
  );
  await expect(
    publicQuote(db, propertyId, [selection()], delivery, now),
  ).rejects.toThrow(/public rate/);
});
it("never exposes another Holding's booking through a token or confirmation", async () => {
  const b = await createPublicReservation(
    db,
    "acme",
    await request(),
    key(),
    delivery,
    now,
  );
  await expect(manageBooking(db, b.number, now)).rejects.toThrow(/access link/);
  await expect(
    manageBooking(db, (await token(b.id)) + "x", now),
  ).rejects.toThrow(/access link/);
  await applyMigrations(env.H002_DB, env.STAFF_TEST_MIGRATIONS);
  await expect(
    manageBooking(env.H002_DB, await token(b.id), now),
  ).rejects.toThrow(/access link/);
});
it("cancels all rooms once and restores inventory without fees", async () => {
  const b = await createPublicReservation(
      db,
      "acme",
      await request(),
      key(),
      delivery,
      now,
    ),
    t = await token(b.id);
  expect((await manageBooking(db, t, now)).cancellation.allowed).toBe(true);
  const a = await cancelPublicReservation(db, "acme", t, delivery, now),
    again = await cancelPublicReservation(db, "acme", t, delivery, now);
  expect(a.state).toBe("cancelled");
  expect(again.state).toBe("cancelled");
  const s = await snapshot();
  expect(availability(s, typeId, "2026-09-05").available).toBe(1);
  expect(records(s, "entry")).toHaveLength(0);
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM pms_booking_emails WHERE reservation_id=? AND kind='cancellation'",
      )
      .bind(b.id)
      .first("n"),
  ).toBe(1);
});
it("rejects cancellation at the exact cutoff and after partial arrival or financial activity", async () => {
  const b = await createPublicReservation(
      db,
      "acme",
      await request(),
      key(),
      delivery,
      now,
    ),
    t = await token(b.id);
  await expect(
    cancelPublicReservation(db, "acme", t, delivery, b.policy!.deadline),
  ).rejects.toThrow(/deadline/);
  const s = await snapshot();
  record(s, "stay", records(s, "stay")[0].id).data.state = "in_house";
  expect(cancellationEligibility(s, b.id, now).allowed).toBe(false);
  const folio = records(await snapshot(), "folio")[0];
  await cmd("charge.add", {
    folioId: folio.id,
    description: "Extra service",
    price: 100,
  });
  await expect(
    cancelPublicReservation(db, "acme", t, delivery, now),
  ).rejects.toThrow(/financial activity/);
});
it("filters inventory reads and public responses away from guest and financial history", async () => {
  await createPublicReservation(
    db,
    "acme",
    await request(),
    key(),
    delivery,
    now,
  );
  const s = await bookingSnapshot(db, propertyId, {
    arrival: "2026-10-01",
    departure: "2026-10-02",
    adults: 2,
    childAges: [],
    quantity: 1,
  });
  expect(
    s.records.every((r) =>
      ["room_type", "room", "rate", "block"].includes(r.kind),
    ),
  ).toBe(true);
  const result = JSON.stringify(
    await publicOffers(db, {
      arrival: "2026-10-01",
      departure: "2026-10-02",
      adults: 2,
    }),
  );
  expect(result).not.toContain("Online Guest");
  expect(result).not.toContain("guest@example.test");
  expect(result).not.toContain("accounts");
});
it("retries email failures while keeping the booking confirmed", async () => {
  await db.prepare("UPDATE pms_booking_emails SET state='sent'").run();
  const b = await createPublicReservation(
    db,
    "acme",
    await request(),
    key(),
    delivery,
    now,
  );
  const fail = vi
    .fn<typeof fetch>()
    .mockResolvedValue(new Response("unavailable", { status: 503 }));
  await deliverBookingEmails(db, delivery, fail, now);
  expect((await manageBooking(db, await token(b.id), now)).state).toBe(
    "confirmed",
  );
  const send = vi
    .fn<typeof fetch>()
    .mockResolvedValue(new Response("{}", { status: 200 }));
  await deliverBookingEmails(db, delivery, send, now + 10000);
  expect(send).toHaveBeenCalledTimes(1);
  expect(JSON.parse(String(send.mock.calls[0][1]?.body)).text).toContain(
    "/manage#token=",
  );
  await deliverBookingEmails(db, delivery, send, now + 20000);
  expect(send).toHaveBeenCalledTimes(1);
});
it("does not deliver deferred public-booking email jobs in production", async () => {
  const send=vi.fn<typeof fetch>();
  vi.stubEnv('NODE_ENV','production');
  try {await deliverBookingEmails(db,delivery,send,now);expect(send).not.toHaveBeenCalled();}
  finally {vi.unstubAllEnvs();}
});
it("expires access links and provides non-enumerating email recovery", async () => {
  const b = await createPublicReservation(
    db,
    "acme",
    await request(),
    key(),
    delivery,
    now,
  );
  await expect(
    manageBooking(db, await token(b.id), now + 8 * 86400000),
  ).rejects.toThrow(/access link/);
  const a = await recoverBooking(
      db,
      "acme",
      { number: b.number, email: "guest@example.test" },
      delivery,
      now + 8 * 86400000,
    ),
    other = await recoverBooking(
      db,
      "acme",
      { number: b.number, email: "wrong@example.test" },
      delivery,
      now,
    );
  expect(a).toEqual(other);
  expect(
    (await manageBooking(db, await token(b.id), now + 8 * 86400000)).number,
  ).toBe(b.number);
});
it("blocks bookings during night closure and rate limits abuse", async () => {
  const input = await request();
  await db
    .prepare("UPDATE pms_properties SET state='closing' WHERE id=?")
    .bind(propertyId)
    .run();
  await expect(
    createPublicReservation(db, "acme", input, key(), delivery, now),
  ).rejects.toThrow(/temporarily/);
  await bookingRateLimit(db, "same-client", 1, now);
  await expect(bookingRateLimit(db, "same-client", 1, now)).rejects.toThrow(
    /Too many/,
  );
  await bookingRateLimit(db, "same-client", 1, now + 60000);
});
it("handles timezone cutoffs and rejects missing child pricing", async () => {
  expect(localDeadline("2026-07-01", "18:00", "America/New_York")).toBe(
    Date.parse("2026-07-01T22:00:00Z"),
  );
  expect(localDeadline("2026-01-01", "18:00", "America/New_York")).toBe(
    Date.parse("2026-01-01T23:00:00Z"),
  );
  await cmd("room_type.save", { id: typeId, name: "Deluxe", capacity: 3 });
  await expect(
    publicQuote(db, propertyId, [selection({ childAges: [5] })], delivery, now),
  ).rejects.toThrow(/child/);
});

it("withdraws inventory after maintenance and rejects a previously available quote", async () => {
  const input = await request();
  await cmd("maintenance.open", {
    roomId,
    from: "2026-09-04",
    to: "2026-09-08",
    reason: "Repair",
  });
  await expect(
    createPublicReservation(db, "acme", input, key(), delivery, now),
  ).rejects.toThrow(/Not enough rooms/);
  expect((await publicOffers(db, selection(), propertyId))[0].offers).toEqual(
    [],
  );
});

it("keeps confirmation replay immutable after cancellation", async () => {
  const input = await request(),
    k = key(),
    original = await createPublicReservation(
      db,
      "acme",
      input,
      k,
      delivery,
      now,
    );
  await cancelPublicReservation(
    db,
    "acme",
    await token(original.id),
    delivery,
    now,
  );
  expect(
    await createPublicReservation(
      db,
      "acme",
      input,
      k,
      delivery,
      now + 3600000,
    ),
  ).toEqual(original);
});
