import {renderToStaticMarkup} from "react-dom/server";
import { cancellationEligibility, localDeadline } from "../../../src/lib/pms/pms-booking";
import {
  createPublicReservation,
  bookingPaymentStatus,
  approveBookingRefund,
  processBookingRefund,
  publicQuote,
  cancelPublicReservation,
  manageBooking,
  bookingCatalog,
  bookingSnapshot,
  publicOffers,
  deliverBookingEmails,
  bookingRateLimit,
  recoverBooking,
  requestBookingRefund,
} from "../../../src/server/modules/pms/public/pms-booking";
import {guestBookingDocument} from "../../../src/server/modules/pms/public/pms/guest-document";
import {staffBookingChange} from "../../../src/server/modules/pms/public/pms/staff-booking-changes";
import { publicChangeQuote, confirmBookingChange } from "../../../src/server/modules/pms/public/pms-booking-changes";
import { createHmac } from "node:crypto";
import { posStripeWebhook,drainPos } from "../../../src/server/modules/pos/public/pos-stripe";
import {pmsFinanceOverview} from "../../../src/server/modules/pms/public/pms/finance";
import {createPmsStripePayment,pmsStripePaymentStatus,reconcilePmsStripePayment,requestPmsStripeRefund,approvePmsStripeRefund,processPmsStripeRefund,recoverPmsStripeRefund} from "../../../src/server/modules/pms/public/pms-stripe";
import { packageGrantStore, readCompanyPackageGrants, packageGrantRevision } from "@/server/modules/commercial/public/package-grants";
import { withOwnerPackageObligations, readOwnerPackageObligations, ownerPackageObligations } from "@/server/modules/commercial/public/owner-package-obligations";
import { publicBookingDirectory } from "@/server/modules/pms/public/public-booking-directory";
import { activeHoldingBindings } from "@/server/modules/runtime/public/runtime";
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { pmsCommand, loadPms } from "../../../src/server/modules/pms/public/pms";
import { records, record, pmsAccountKeys, type Config } from "../../../src/server/modules/pms/public/rules/pms";
import { availability } from "../../../src/lib/pms/pms-engine";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { confirmManualRate } from "../../../src/server/modules/finance/public/company-money";
import { POST as publicBookingPost } from "../../../src/app/api/public/booking/[holdingSlug]/[action]/route";

vi.mock("../../../src/server/modules/runtime/adapters/runtime", async () => {
  const { env } = await import("../../support/runtime-env");
  return { holdingResources: async (id:string) => ({ write: id==="h002"?env.H002_DB:env.H001_DB }), holdingBindingBySlug:async(slug:string)=>({id:slug==="other-tenant"?"h002":"h001",slug}), activeHoldingBindings:vi.fn(async()=>[]),runtimeEnv:async()=>({CONTROL_DB:env.CONTROL_DB,ERPX_ENVIRONMENT:"local",BETTER_AUTH_SECRET:"test-booking-secret-32-characters-long",ERPX_PUBLIC_ORIGIN:"https://hotel.example.test",RESEND_API_KEY:"test-key",RESEND_FROM:"hotel@example.test",ERPX_RESOURCE_SLOTS:JSON.stringify([{id:'h001',write:'H001_DB',read:'H001_READ_DB',files:'H001_FILES'},{id:'h002',write:'H002_DB',read:'H002_READ_DB',files:'H002_FILES',beta:true},{id:'h003',write:'H003_DB',read:'H003_READ_DB',files:'H003_FILES',internal:true}])}) };
});
const db = env.H001_DB,
  key = () => crypto.randomUUID();
let f: Awaited<ReturnType<typeof seedExpenseCompany>>,
  owner: WorkContext,
  reviewer: WorkContext,
  operationalReviewer: WorkContext,
  propertyId: string,
  typeId: string,
  rateId: string,
  roomId: string,
  guestId: string,
  accounts: Config["accounts"];
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
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
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','false');
  f = await seedExpenseCompany(db, env.H001_FILES);
  await db.prepare('DELETE FROM pms_booking_payments').run();
  await db.prepare('DELETE FROM pos_connections WHERE company_id=?').bind(f.company).run();
  await db.prepare("DELETE FROM pos_connections WHERE account_id='acct_hotel'").run();
  owner = f.owner;
  reviewer = await f.member("hotel-reviewer", "finance_approver");
  operationalReviewer = await f.member("hotel-manager", "pms_manager");
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
  const liabilities=[key(),key()];
  for(const [i,id] of liabilities.entries())await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(id,f.chart,f.company,`ONLINE-${i}`,`Online liability ${i}`).run();
  await cmd("online_accounting.prepare",{refundPayable:liabilities[0],unallocated:liabilities[1]});
  await cmd("online_accounting.approve",{},reviewer);
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
afterEach(()=>{vi.useRealTimers();vi.unstubAllGlobals();vi.unstubAllEnvs();});

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
  expect(records(s, "guest_account")).toHaveLength(1);
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
  const guestAccount = records(await snapshot(), "guest_account")[0];
  await cmd("charge.add", {
    guestAccountId: guestAccount.id,
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
it("delivers a queued confirmation in a release process without enabling live payments", async () => {
  await createPublicReservation(db,'acme',await request(),key(),delivery,now);
  const send=vi.fn<typeof fetch>().mockResolvedValue(new Response('{}',{status:200}));
  vi.stubEnv('NODE_ENV','production');vi.stubEnv('PMS_STRIPE_LIVE_ENABLED','false');
  await deliverBookingEmails(db,delivery,send,now);
  expect(send).toHaveBeenCalledTimes(1);
  expect(JSON.parse(String(send.mock.calls[0][1]?.body)).subject).toContain('Booking confirmation');
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
it("cancels oversized chunked public booking bodies before rate or reservation writes", async () => {
  vi.stubEnv("PMS_STRIPE_BOOKING_ENABLED", "true");
  vi.stubEnv("ERPX_PUBLIC_ORIGIN", "https://hotel.example.test");
  const ip = `203.0.113.${Math.floor(Math.random() * 200) + 1}`;
  const context = { params: Promise.resolve({ holdingSlug: "acme", action: "reservations" }) };
  const snapshot = async () => Promise.all([
    db.prepare("SELECT count(*)::int n FROM pms_booking_limits").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_booking_receipts").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_booking_tokens").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_booking_emails").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_records WHERE property_id=? AND kind='reservation'").bind(propertyId).first<number>("n"),
  ]);
  const before = await snapshot();
  let pulls = 0, cancelled = false;
  const body = new ReadableStream<Uint8Array>({
    pull(controller) {
      pulls++;
      controller.enqueue(new Uint8Array(pulls === 1 ? 70_000 : 60_001));
    },
    cancel() { cancelled = true; },
  });
  const response = await publicBookingPost(new Request("https://hotel.example.test/api/public/booking/acme/reservations", {
    method: "POST",
    headers: { host: "hotel.example.test", origin: "https://hotel.example.test", "content-type": "application/json", "x-forwarded-for": ip },
    body,
    duplex: "half",
  } as RequestInit & { duplex: "half" }), context);

  expect(response.status).toBe(413);
  expect((await response.json()).error.code).toBe("BODY_TOO_LARGE");
  expect(cancelled).toBe(true);
  expect(pulls).toBeLessThan(4);
  expect(await snapshot()).toEqual(before);
});
it("rejects short malformed UTF-8 booking JSON before rate or reservation writes", async () => {
  vi.stubEnv("PMS_STRIPE_BOOKING_ENABLED", "true");
  vi.stubEnv("ERPX_PUBLIC_ORIGIN", "https://hotel.example.test");
  const ip = `203.0.113.${Math.floor(Math.random() * 200) + 1}`;
  const context = { params: Promise.resolve({ holdingSlug: "acme", action: "reservations" }) };
  const snapshot = async () => Promise.all([
    db.prepare("SELECT count(*)::int n FROM pms_booking_limits").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_booking_receipts").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_booking_tokens").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_booking_emails").first<number>("n"),
    db.prepare("SELECT count(*)::int n FROM pms_records WHERE property_id=? AND kind='reservation'").bind(propertyId).first<number>("n"),
  ]);
  const before = await snapshot();
  const encoded = new TextEncoder().encode(JSON.stringify({
    propertyId,
    quoteToken: "invalid-quote-fixture",
    guest: { name: "MalformedGuest", email: "guest@example.test", phone: "0123456789" },
    notes: "",
  }));
  const marker = new TextEncoder().encode('"name":"MalformedGuest"');
  const markerStart = encoded.findIndex((_value, index) => marker.every((byte, offset) => encoded[index + offset] === byte));
  expect(markerStart).toBeGreaterThanOrEqual(0);
  encoded[markerStart + new TextEncoder().encode('"name":"').length] = 0xff;
  expect(encoded.byteLength).toBeLessThan(120_000);
  expect(JSON.parse(new TextDecoder().decode(encoded))).toMatchObject({ guest: { name: expect.stringContaining("\uFFFD") } });
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(encoded);
      controller.close();
    },
  });
  const response = await publicBookingPost(new Request("https://hotel.example.test/api/public/booking/acme/reservations", {
    method: "POST",
    headers: { host: "hotel.example.test", origin: "https://hotel.example.test", "content-type": "application/json", "x-forwarded-for": ip, "Idempotency-Key": key() },
    body,
    duplex: "half",
  } as RequestInit & { duplex: "half" }), context);

  expect(response.status).toBe(422);
  expect((await response.json()).error.code).toBe("INVALID_ENCODING");
  expect(await snapshot()).toEqual(before);
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
it('holds two rooms, then confirms and allocates each guest account only after Stripe verifies the direct charge',async()=>{
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','true');vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
  await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,collection:{method:'full',depositPercent:30,changesAllowed:true}});
  await cmd('room.save',{number:'102',typeId,active:true});
  await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?, 'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
  let paid=false,attemptId='',amount=0;
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
    expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_hotel');
    if(init?.method==='POST')return new Response(JSON.stringify({id:'cs_hotel',url:'https://checkout.stripe.com/c/pay/hotel'}));
    if(String(url).includes('payment_intents/'))return new Response(JSON.stringify({id:'pi_hotel',status:'succeeded',amount_received:amount,currency:'thb',metadata:{pms_booking_payment_id:attemptId}}));
    return new Response(JSON.stringify({id:'cs_hotel',status:'complete',payment_status:paid?'paid':'unpaid',payment_intent:'pi_hotel'}));
  }));
  const input=await request({quantity:2});
  const pending=await createPublicReservation(db,'acme',input,key(),delivery,now);
  expect(pending.state).toBe('payment_pending');expect(pending.checkoutUrl).toContain('checkout.stripe.com');
  attemptId=pending.attemptToken!.split('.')[0];amount=Number((await db.prepare('SELECT amount_minor FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{amount_minor:number}>())!.amount_minor);
  expect(records(await snapshot(),'entry')).toHaveLength(0);
  const requestedMoney=await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_booking_payment' AND source_id=? AND amount_kind='requested'").bind(f.company,attemptId).first<{amount_units:string;usd_units:string;fx_rate_id:string}>();
  expect(requestedMoney!.amount_units).toBe(String(amount));
  await confirmManualRate(reviewer,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'40',note:'Delayed provider confirmation must retain the requested quote'},key());
  paid=true;
  const confirmed=await bookingPaymentStatus(db,pending.attemptToken!,delivery,now+1000);
  expect(confirmed.state).toBe('confirmed');
  const s=await snapshot();expect(records(s,'guest_account')).toHaveLength(2);expect(records(s,'entry').filter(e=>e.data.method==='stripe')).toHaveLength(2);
  const receivedMoney=(await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_entry' AND carried_source_type='pms_booking_payment' AND carried_source_id=?").bind(f.company,attemptId).all<{amount_units:string;usd_units:string;fx_rate_id:string}>()).results;
  expect(receivedMoney).toHaveLength(2);expect(receivedMoney.every(row=>row.fx_rate_id===requestedMoney!.fx_rate_id)).toBe(true);
  expect(receivedMoney.reduce((n,row)=>n+BigInt(row.amount_units),BigInt(0))).toBe(BigInt(requestedMoney!.amount_units));
  expect(receivedMoney.reduce((n,row)=>n+BigInt(row.usd_units),BigInt(0))).toBe(BigInt(requestedMoney!.usd_units));
  expect((await bookingPaymentStatus(db,pending.attemptToken!,delivery,now+1001)).state).toBe('confirmed');
  expect(await db.prepare("SELECT COUNT(*)::int n FROM money_transaction_snapshots WHERE company_id=? AND carried_source_type='pms_booking_payment' AND carried_source_id=?").bind(f.company,attemptId).first('n')).toBe(2);
  expect((await db.prepare('SELECT COUNT(*) n FROM pms_booking_emails WHERE reservation_id=?').bind(pending.id).first<{n:number}>())!.n).toBe(1);
});
it('releases an unpaid hold and sends a payment that arrives later to refund review',async()=>{
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','true');vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
  await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,collection:{method:'full',depositPercent:30,changesAllowed:true}});
  await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?, 'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
  let paid=false,attemptId='',amount=0;
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
    if(String(url).includes('payment_intents/'))return new Response(JSON.stringify({id:'pi_hotel',status:'succeeded',amount_received:amount,currency:'thb',metadata:{pms_booking_payment_id:attemptId}}));
    if(init?.method==='POST'&&String(url).includes('/expire'))return new Response(JSON.stringify({id:'cs_hotel',status:'expired'}));
    if(init?.method==='POST')return new Response(JSON.stringify({id:'cs_hotel',url:'https://checkout.stripe.com/c/pay/hotel'}));
    return new Response(JSON.stringify({id:'cs_hotel',status:paid?'complete':'expired',payment_status:paid?'paid':'unpaid',payment_intent:'pi_hotel'}));
  }));
  const pending=await createPublicReservation(db,'acme',await request(),key(),delivery,now);
  attemptId=pending.attemptToken!.split('.')[0];amount=Number((await db.prepare('SELECT amount_minor FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{amount_minor:number}>())!.amount_minor);
  const expired=await bookingPaymentStatus(db,pending.attemptToken!,delivery,now+31*60*1000);
  expect(expired.state).toBe('expired');expect(availability(await snapshot(),typeId,'2026-09-05').available).toBe(1);
  paid=true;
  const late=await bookingPaymentStatus(db,pending.attemptToken!,delivery,now+32*60*1000);
  expect(late.state).toBe('refund_review');expect(late.refundState).toBe('requested');
  expect(availability(await snapshot(),typeId,'2026-09-05').available).toBe(1);
});
it('requires Finance approval and Stripe success before a free-cancellation refund reaches each guest account',async()=>{
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','true');vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
  await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,collection:{method:'full',depositPercent:30,changesAllowed:true}});
  await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?, 'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
  let attemptId='',amount=0,refundState='pending';
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
    const path=String(url);
    if(path.includes('/refunds/'))return new Response(JSON.stringify({id:'re_hotel',amount,metadata:{pms_booking_payment_id:attemptId},status:refundState}));
    if(path.endsWith('/refunds')&&init?.method==='POST')return new Response(JSON.stringify({id:'re_hotel',status:'pending'}));
    if(path.includes('payment_intents/'))return new Response(JSON.stringify({id:'pi_hotel',status:'succeeded',amount_received:amount,currency:'thb',metadata:{pms_booking_payment_id:attemptId}}));
    if(init?.method==='POST')return new Response(JSON.stringify({id:'cs_hotel',url:'https://checkout.stripe.com/c/pay/hotel'}));
    return new Response(JSON.stringify({id:'cs_hotel',status:'complete',payment_status:'paid',payment_intent:'pi_hotel'}));
  }));
  await counterPackage();vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
  const pending=await createPublicReservation(db,'acme',await request(),key(),delivery,now);
  attemptId=pending.attemptToken!.split('.')[0];amount=Number((await db.prepare('SELECT amount_minor FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{amount_minor:number}>())!.amount_minor);
  await bookingPaymentStatus(db,pending.attemptToken!,delivery,now+1000);
  await counterPackage('draining');const cancelled=await cancelPublicReservation(db,'acme',await token(pending.id),delivery,now+2000);
  expect(cancelled.refundState).toBe('requested');
  expect((await db.prepare('SELECT refund_id FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{refund_id:string|null}>())!.refund_id).toBeNull();
  await expect(approveBookingRefund(db,reviewer,attemptId)).rejects.toMatchObject({code:"PERMISSION_REQUIRED"});
  await approveBookingRefund(db,operationalReviewer,attemptId);
  await processBookingRefund(db,attemptId,now+3000);
  expect((await db.prepare('SELECT refund_state FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{refund_state:string}>())!.refund_state).toBe('processing');
  refundState='succeeded';await processBookingRefund(db,attemptId,now+4000);
  expect((await db.prepare('SELECT refund_state FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{refund_state:string}>())!.refund_state).toBe('succeeded');
  expect(records(await snapshot(),'guest_account')[0].data.advance).toBe(0);
});
async function counterPackage(state:"active"|"draining"="active",endsAt:number|null=null) {
 const current=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId==='pms');
 return packageGrantStore(db,{authorize:async()=>({userId:'isolated-counter-package-fixture'}),obligations:async()=>{throw Error('No test withdrawal witness');}}).apply({expectedVersion:current?.version??0,idempotencyKey:key(),
  grant:{companyId:f.company,packageId:'pms',packageVersion:1,state,startsAt:0,endsAt,version:(current?.version??0)+1,reason:'Native Hotel counter settlement review',evidence:'test:pms-counter-package'}});
}
it('package Booking hides unpurchased public inventory and rejects quote and reservation without effects',async()=>{
 const input=await request(),before=await snapshot();vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 expect(await bookingCatalog(db)).toEqual([]);expect(await publicOffers(db,selection(),propertyId)).toEqual([]);
 await expect(publicQuote(db,propertyId,[selection()],delivery,now)).rejects.toMatchObject({status:404,code:'PMS_NOT_FOUND'});
 await expect(createPublicReservation(db,'acme',input,key(),delivery,now)).rejects.toMatchObject({status:404,code:'PMS_NOT_FOUND'});expect(await snapshot()).toEqual(before);
 expect(await db.prepare('SELECT count(*) n FROM pms_booking_receipts WHERE property_id=?').bind(propertyId).first()).toEqual({n:0});
 vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','shadow');const logs=vi.spyOn(console,'info').mockImplementation(()=>{});
 try{expect((await bookingCatalog(db)).map(row=>row.id)).toContain(propertyId);expect(logs.mock.calls.some(args=>String(args[0]).includes('commercial.purchase.shadow')&&String(args[0]).includes('PACKAGE_REQUIRED'))).toBe(true);}finally{logs.mockRestore();}
});
it('package Booking directory publishes only guest hotel cards in ordinary active holdings',async()=>{
 vi.mocked(activeHoldingBindings).mockResolvedValue([{id:'h001',slug:'public-stays',displayName:'Private Holding Name'},{id:'h002',slug:'beta-secret',displayName:'Beta private'},{id:'h003',slug:'internal-secret',displayName:'Internal private'},{id:'h004',slug:'unregistered-secret',displayName:'Unregistered private'}]);
 try{
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','true');vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
  expect((await publicBookingDirectory()).find(row=>row.id===propertyId)).toBeUndefined();await counterPackage();
  const directory=await publicBookingDirectory(),hotel=directory.find(row=>row.id===propertyId)!;
  expect(hotel).toMatchObject({slug:'public-stays',id:propertyId,name:'Test Hotel'});expect(Object.keys(hotel).sort()).toEqual(['address','description','id','images','name','slug']);
  expect(directory.every(row=>row.slug==='public-stays')).toBe(true);expect(JSON.stringify(directory)).not.toContain('Private Holding Name');
  await counterPackage('draining');expect((await publicBookingDirectory()).find(row=>row.id===propertyId)).toBeUndefined();
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','false');expect(await publicBookingDirectory()).toEqual([]);
 }finally{vi.mocked(activeHoldingBindings).mockResolvedValue([]);}
});
it('package Booking retains replay, a shorter stay and cancellation while draining without POS or Finance management',async()=>{
 await counterPackage();vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');const input=await request(),receipt=key(),booking=await createPublicReservation(db,'acme',input,receipt,delivery,now),access=await token(booking.id);
 const stay=records(await snapshot(),'stay').find(row=>row.data.reservationId===booking.id)!;await counterPackage('draining');
 expect(await bookingCatalog(db)).toEqual([]);expect((await manageBooking(db,access,now+1000)).purchases).toEqual({newWork:false,settleExisting:true,history:true});
 expect((await manageBooking(db,access,now+1000)).editableRates.map(row=>row.id)).toContain(rateId);
 await expect(createPublicReservation(db,'acme',input,key(),delivery,now+1000)).rejects.toMatchObject({status:404});expect(await createPublicReservation(db,'acme',input,receipt,delivery,now+1000)).toEqual(booking);
 await expect(publicChangeQuote(db,access,'change',{items:[{stayId:stay.id,room:selection({departure:'2026-09-08'})}]},delivery,now+1000)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 const reduced=await publicChangeQuote(db,access,'change',{items:[{stayId:stay.id,room:selection({departure:'2026-09-06'})}]},delivery,now+1000),changeKey=key();
 await confirmBookingChange(db,'acme',access,reduced.token,changeKey,delivery,now+1001);await confirmBookingChange(db,'acme',access,reduced.token,changeKey,delivery,now+1002);
 expect(record(await snapshot(),'stay',stay.id).data.departure).toBe('2026-09-06');expect((await cancelPublicReservation(db,'acme',access,delivery,now+2000)).state).toBe('cancelled');
 expect((await readCompanyPackageGrants(db,f.company)).map(grant=>grant.packageId)).toEqual(['pms']);expect(await db.prepare('SELECT count(*) n FROM command_assertions').first()).toEqual({n:0});
});
it('package Booking rolls back public reservation, access mail and receipt when purchase expires after deferred effects',async()=>{
 const input=await request(),before=await snapshot();await counterPackage('active',Date.now()+1500);vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 const trigger=`booking_package_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.6);RETURN NEW;END $$;
  CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON pms_booking_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.property_id='${propertyId}') EXECUTE FUNCTION ${trigger}()`);
 try{await expect(createPublicReservation(db,'acme',input,key(),delivery,now)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});expect(await snapshot()).toEqual(before);
  for(const table of ['pms_booking_receipts','pms_booking_tokens','pms_booking_emails'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE property_id=?`).bind(propertyId).first()).toEqual({n:0});}
 finally{await db.exec(`DROP TRIGGER ${trigger} ON pms_booking_receipts;DROP FUNCTION ${trigger}()`);}
});
it('package Booking retains unpaid mail and non-enumerating recovery until history is purchased',async()=>{
 const booking=await createPublicReservation(db,'acme',await request(),key(),delivery,now),access=await token(booking.id),send=vi.fn<typeof fetch>().mockImplementation(Object.assign(async()=>Response.json({id:'native-mail-only'}),{preconnect:fetch.preconnect}));
 const mail=await db.prepare('SELECT id,state,attempts FROM pms_booking_emails WHERE reservation_id=?').bind(booking.id).first<{id:string;state:string;attempts:number}>();
 vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');await expect(manageBooking(db,access,now+1000)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 expect(await recoverBooking(db,'acme',{number:booking.number,email:'guest@example.test'},delivery,now+1000)).toEqual(await recoverBooking(db,'acme',{number:'missing',email:'nobody@example.test'},delivery,now+1000));
 expect(await db.prepare('SELECT count(*) n FROM pms_booking_emails WHERE reservation_id=?').bind(booking.id).first()).toEqual({n:1});
 await db.prepare('UPDATE pms_booking_emails SET next_attempt_at=0 WHERE id=?').bind(mail!.id).run();
 const ownedSends=()=>send.mock.calls.filter(([,init])=>new Headers(init?.headers).get('Idempotency-Key')===`pms-booking-${mail!.id}`);
 await deliverBookingEmails(db,delivery,send,now+1000);expect(ownedSends()).toHaveLength(0);expect(await db.prepare('SELECT state,attempts,last_error FROM pms_booking_emails WHERE id=?').bind(mail!.id).first()).toEqual({state:'pending',attempts:mail!.attempts,last_error:'PACKAGE_REQUIRED'});
 await counterPackage();await counterPackage('draining');await db.prepare('UPDATE pms_booking_emails SET next_attempt_at=0 WHERE id=?').bind(mail!.id).run();await deliverBookingEmails(db,delivery,send,now+62000);expect(ownedSends()).toHaveLength(1);expect(await db.prepare('SELECT state FROM pms_booking_emails WHERE id=?').bind(mail!.id).first()).toEqual({state:'sent'});
});
it('package Booking keeps private documents, recovery and failed receipt mail after normal Hotel inactivation',async()=>{
 await counterPackage();vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 const booking=await createPublicReservation(db,'acme',await request(),key(),delivery,now),access=await token(booking.id);await counterPackage('draining');
 await cancelPublicReservation(db,'acme',access,delivery,now+1000);
 const bills=records(await snapshot(),'guest_account').filter(row=>row.data.reservationId===booking.id);
 for(const bill of bills)await cmd('guest_account.close',{id:bill.id});
 const documents=await manageBooking(db,access,now+2000),document=await guestBookingDocument(db,access,{id:documents.documents[0].id},now+2000);
 const scope={companyId:f.company,packageId:'pms',packageVersion:1},grants=await readCompanyPackageGrants(db,f.company),current=grants.find(grant=>grant.packageId==='pms')!;
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-booking-history-provider'}),obligations:(tx,company,packageId)=>ownerPackageObligations(db,tx,company,packageId)});
 const change={companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Hotel settlement completed; retain guest history',evidence:'test:guest-history-inactivation'}}]};
 await withOwnerPackageObligations(db,scope,async tx=>{expect((await readOwnerPackageObligations(db,tx,{...scope,now:Date.now()})).total).toBe(0);await store.batch.previewIn(tx,change);await store.batch.applyIn(tx,change,async()=>{});});
 expect((await manageBooking(db,access,now+3000)).purchases).toEqual({newWork:false,settleExisting:false,history:true});expect(await guestBookingDocument(db,access,{id:documents.documents[0].id},now+3000)).toEqual(document);
 expect(await bookingCatalog(db)).not.toContainEqual(expect.objectContaining({id:propertyId}));
 await expect(publicChangeQuote(db,access,'cancel',{items:[{stayId:documents.rooms[0].id}]},delivery,now+3000)).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});
 const fail=vi.fn<typeof fetch>().mockResolvedValue(Response.json({error:'fixture delivery failure'},{status:503}));
 for(let attempt=0;attempt<8;attempt++){await db.prepare("UPDATE pms_booking_emails SET next_attempt_at=0 WHERE reservation_id=? AND state='pending'").bind(booking.id).run();await deliverBookingEmails(db,delivery,fail,now+4000+attempt*1000);}
 const failed=(await db.prepare('SELECT id,state,attempts FROM pms_booking_emails WHERE reservation_id=?').bind(booking.id).all<{id:string;state:string;attempts:number}>()).results;
 expect(failed.length).toBeGreaterThan(0);expect(failed.every(mail=>mail.state==='failed'&&mail.attempts===8)).toBe(true);
 for(const mail of failed)await cmd('booking.retry_email',{id:mail.id});
 await recoverBooking(db,'acme',{number:booking.number,email:'guest@example.test'},delivery,now+20000);
 await db.prepare("UPDATE pms_booking_emails SET next_attempt_at=0 WHERE reservation_id=? AND state='pending'").bind(booking.id).run();
 const send=vi.fn<typeof fetch>().mockResolvedValue(Response.json({id:'retained-private-receipt'}));await deliverBookingEmails(db,delivery,send,now+21000);
 expect((await db.prepare('SELECT state FROM pms_booking_emails WHERE reservation_id=?').bind(booking.id).all<{state:string}>()).results.every(mail=>mail.state==='sent')).toBe(true);
 expect((await readCompanyPackageGrants(db,f.company)).map(grant=>grant.state)).toEqual(['inactive']);
});
it('package Hotel-only counter work blocks unpurchased requests and settles draining funds after cashier revocation',async()=>{
 vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
 await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?,'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
 const reservation=await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]}),guestAccount=records(await snapshot(),'guest_account').find(item=>item.data.reservationId===reservation.id)!;
 let paymentId='';const fetcher=vi.fn(async(_url:string|URL|Request,init?:RequestInit)=>{
  expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_hotel');
  if(init?.method==='POST'){paymentId=(init.body as URLSearchParams).get('metadata[pms_counter_payment_id]')!;return Response.json({id:'pi_package',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201'}}});}
  return Response.json({id:'pi_package',status:'succeeded',amount_received:5000,currency:'thb',metadata:{pms_counter_payment_id:paymentId}});
 });vi.stubGlobal('fetch',fetcher);vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
 const input={propertyId,guestAccountId:guestAccount.id,kind:'receipt',amountMinor:5000,payerEmail:'payer@example.test'};
 await expect(createPmsStripePayment(db,owner,input,key(),now)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(fetcher).not.toHaveBeenCalled();expect(await db.prepare('SELECT count(*) n FROM pms_stripe_payments WHERE company_id=?').bind(f.company).first()).toEqual({n:0});
 await counterPackage();const payment=await createPmsStripePayment(db,owner,input,key(),now);await counterPackage('draining');
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();
 await expect(pmsStripePaymentStatus(db,owner,payment.id,now+1000)).rejects.toMatchObject({code:'PMS_ACCESS_CHANGED'});
 await env.CONTROL_DB.prepare("INSERT INTO pos_stripe_routes(account_id,livemode,holding_id,company_id,created_at) VALUES('acct_hotel',false,'h001',?,0) ON CONFLICT(account_id,livemode) DO UPDATE SET holding_id=excluded.holding_id,company_id=excluded.company_id").bind(f.company).run();
 vi.stubEnv('POS_STRIPE_TEST_WEBHOOK_SECRET','whsec_package_counter_fixture');
 const eventId=key(),raw=JSON.stringify({id:eventId,type:'payment_intent.succeeded',account:'acct_hotel',livemode:false,created:Math.floor((now+1000)/1000),data:{object:{id:'pi_package',metadata:{pms_counter_payment_id:payment.id}}}}),timestamp=Math.floor(Date.now()/1000);
 const signature=`t=${timestamp},v1=${createHmac('sha256','whsec_package_counter_fixture').update(`${timestamp}.${raw}`).digest('hex')}`;
 await posStripeWebhook(raw,signature);await posStripeWebhook(raw,signature);await drainPos(db,'h001',8,f.company);await reconcilePmsStripePayment(db,payment.id,now+2000);
 expect(await db.prepare('SELECT count(*) n FROM pos_stripe_events WHERE id=? AND applied_at IS NOT NULL').bind(eventId).first()).toEqual({n:1});
 expect(record(await snapshot(),'guest_account',guestAccount.id).data.advance).toBe(5000);expect(await db.prepare('SELECT state FROM pms_stripe_payments WHERE id=?').bind(payment.id).first()).toEqual({state:'succeeded'});
 expect((await readCompanyPackageGrants(db,f.company)).map(grant=>grant.packageId)).toEqual(['pms']);expect(await db.prepare('SELECT count(*) n FROM command_assertions').first()).toEqual({n:0});
});
it('posts a counter PromptPay room payment and deposit only after Stripe confirms each intent',async()=>{
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
  await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?, 'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
  const reservation=await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]});
  const guestAccount=records(await snapshot(),'guest_account').find(item=>item.data.reservationId===reservation.id)!;
  let paid=false,activeId='',activeAmount=0,serial=0;
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
    expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_hotel');
    if(init?.method==='POST'){
      const body=init.body as URLSearchParams;activeId=body.get('metadata[pms_counter_payment_id]')??'';activeAmount=Number(body.get('amount'));serial++;
      return new Response(JSON.stringify({id:`pi_counter_${serial}`,status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201010212'}}}));
    }
    return new Response(JSON.stringify({id:`pi_counter_${serial}`,status:paid?'succeeded':'requires_action',amount_received:paid?activeAmount:0,currency:'thb',metadata:{pms_counter_payment_id:activeId}}));
  }));
  const receipt=await createPmsStripePayment(db,owner,{propertyId,guestAccountId:guestAccount.id,kind:'receipt',amountMinor:5000,payerEmail:''},key(),now);
  expect(receipt.state).toBe('pending');expect(receipt.qrData).toBe('000201010212');expect(records(await snapshot(),'entry')).toHaveLength(0);
  paid=true;expect((await pmsStripePaymentStatus(db,owner,receipt.id,now+1000)).state).toBe('succeeded');
  paid=false;
  const deposit=await createPmsStripePayment(db,owner,{propertyId,guestAccountId:guestAccount.id,kind:'deposit',amountMinor:2000,payerEmail:''},key(),now+2000);
  paid=true;expect((await pmsStripePaymentStatus(db,owner,deposit.id,now+3000)).state).toBe('succeeded');
  const latest=record(await snapshot(),'guest_account',guestAccount.id);
  expect(latest.data.advance).toBe(5000);expect(latest.data.deposit).toBe(2000);
});
it('requires an independent Finance approval and Stripe confirmation for a counter refund',async()=>{
  await counterPackage();vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
  await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?, 'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
  const reservation=await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]});
  const guestAccount=records(await snapshot(),'guest_account').find(item=>item.data.reservationId===reservation.id)!;
  let paymentId='',refundState='pending';
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
    const path=String(url);
    if(path.includes('/refunds/'))return new Response(JSON.stringify({id:'re_counter',amount:5000,status:refundState,metadata:{pms_counter_payment_id:paymentId}}));
    if(path.endsWith('/refunds')&&init?.method==='POST')return new Response(JSON.stringify({id:'re_counter',status:'pending'}));
    if(path.includes('payment_intents/'))return new Response(JSON.stringify({id:'pi_counter',status:'succeeded',amount_received:5000,currency:'thb',metadata:{pms_counter_payment_id:paymentId}}));
    const body=init?.body as URLSearchParams;paymentId=body.get('metadata[pms_counter_payment_id]')??'';
    return new Response(JSON.stringify({id:'pi_counter',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201010212'}}}));
  }));
  const payment=await createPmsStripePayment(db,owner,{propertyId,guestAccountId:guestAccount.id,kind:'receipt',amountMinor:5000,payerEmail:'payer@example.com'},key(),now);
  const sourceMoney=await db.prepare("SELECT fx_rate_id,usd_units::text FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_stripe_payment' AND source_id=? AND amount_kind='requested'").bind(f.company,payment.id).first<{fx_rate_id:string;usd_units:string}>();
  await confirmManualRate(reviewer,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'40',note:'Provider receipt and refund must retain requested FX'},key());
  await pmsStripePaymentStatus(db,owner,payment.id,now+1000);
  await counterPackage('draining');await requestPmsStripeRefund(db,owner,payment.id,5000,'Guest cancellation');
  await expect(approvePmsStripeRefund(db,owner,payment.id)).rejects.toThrow();
  expect((await db.prepare('SELECT refund_id FROM pms_stripe_payments WHERE id=?').bind(payment.id).first<{refund_id:string|null}>())!.refund_id).toBeNull();
  await expect(approvePmsStripeRefund(db,reviewer,payment.id)).rejects.toMatchObject({code:"PMS_APPROVAL_REQUIRED"});
  await approvePmsStripeRefund(db,operationalReviewer,payment.id);
  await processPmsStripeRefund(db,payment.id,now+2000);
  expect((await db.prepare('SELECT refund_state FROM pms_stripe_payments WHERE id=?').bind(payment.id).first<{refund_state:string}>())!.refund_state).toBe('processing');
  refundState='succeeded';await processPmsStripeRefund(db,payment.id,now+3000);
  expect((await db.prepare('SELECT refund_state FROM pms_stripe_payments WHERE id=?').bind(payment.id).first<{refund_state:string}>())!.refund_state).toBe('succeeded');
  expect(record(await snapshot(),'guest_account',guestAccount.id).data.advance).toBe(0);
  const received=await db.prepare('SELECT entry_id FROM pms_stripe_payments WHERE id=?').bind(payment.id).first<{entry_id:string}>();
  expect(await db.prepare("SELECT amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_entry' AND source_id=? AND amount_kind='total'").bind(f.company,received!.entry_id).first()).toEqual({amount_units:'5000',usd_units:sourceMoney!.usd_units,fx_rate_id:sourceMoney!.fx_rate_id});
  expect(await db.prepare("SELECT SUM(amount_units)::text amount,SUM(usd_units)::text usd FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_entry' AND (source_id=? OR original_source_id=?)").bind(f.company,received!.entry_id,received!.entry_id).first()).toEqual({amount:'0',usd:'0'});
});

async function onlinePolicy(method:'full'|'deposit'='full',percent=30){
 vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','true');vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
 await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,collection:{method,depositPercent:percent,changesAllowed:true}});
 await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?,'acct_hotel',false,true,true,true,true,'ready','daily',true) ON CONFLICT(company_id) DO NOTHING").bind(f.company).run();
 const sessions=new Map<string,{id:string;amount:number;paid:boolean}>();let serial=0;
 vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
  const path=new URL(String(url)).pathname;
  if(path.endsWith('/checkout/sessions')&&init?.method==='POST'){
   const body=new URLSearchParams(String(init.body)),id=body.get('metadata[pms_booking_payment_id]')!,cs=`cs_fixture_${++serial}`;
   expect(body.get('payment_method_types[0]')).toBe('card');expect(body.get('payment_method_types[1]')).toBe('promptpay');expect(body.has('payment_intent_data[application_fee_amount]')).toBe(false);
   sessions.set(cs,{id,amount:Number(body.get('line_items[0][price_data][unit_amount]')),paid:false});return Response.json({id:cs,url:`https://checkout.stripe.com/c/pay/${cs}`});
  }
  if(path.includes('/payment_intents/')){const cs=path.split('/').at(-1)!.replace('pi_','cs_'),s=sessions.get(cs)!;return Response.json({id:`pi_${cs.slice(3)}`,status:'succeeded',amount_received:s.amount,currency:'thb',metadata:{pms_booking_payment_id:s.id}});}
  if(path.endsWith('/expire'))return Response.json({id:path.split('/').at(-2),status:'expired'});
  const cs=path.split('/').at(-1)!,s=sessions.get(cs)!;
  return Response.json({id:cs,status:s.paid?'complete':'open',payment_status:s.paid?'paid':'unpaid',payment_intent:`pi_${cs.slice(3)}`});
 }));
 return {pay:()=>sessions.forEach(s=>s.paid=true)};
}
it('collects a 30% room advance and combines pay-at-hotel rooms without inventing income',async()=>{
 await onlinePolicy('deposit');await cmd('room.save',{number:'102',typeId,active:true});
 const hotelRate=(await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:undefined,name:'At hotel',collection:{method:'pay_at_hotel',depositPercent:30,changesAllowed:true}})).id;
 await cmd('booking.save',{...settings(),rateIds:[rateId,hotelRate]});
 const q=await publicQuote(db,propertyId,[selection(),selection({rateId:hotelRate})],delivery,now);
 expect(q.quote.total).toBe(42800);expect(q.quote.dueNow).toBe(6420);expect(q.quote.dueAtHotel).toBe(36380);
 const b=await createPublicReservation(db,'acme',{...(await request()),quoteToken:q.token},key(),delivery,now);
 const p=await db.prepare('SELECT amount_minor,allocation_json FROM pms_booking_payments WHERE id=?').bind(b.attemptToken!.split('.')[0]).first<{amount_minor:number;allocation_json:string}>();
 expect(Number(p!.amount_minor)).toBe(6420);expect(JSON.parse(p!.allocation_json)).toHaveLength(1);expect(records(await snapshot(),'entry')).toHaveLength(0);
});
it('prevents staff changes and accounting changes while a provider hold is pending',async()=>{
 await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now),stay=records(await snapshot(),'stay').find(st=>st.data.reservationId===b.id)!;
 await expect(cmd('stay.change_dates',{id:stay.id,arrival:'2026-09-06',departure:'2026-09-08',rateId,reason:'Staff edit'})).rejects.toMatchObject({code:'PMS_PAYMENT_PENDING'});
 await expect(cmd('reservation.cancel',{id:b.id,reason:'Staff cancellation',fee:0})).rejects.toMatchObject({code:'PMS_PAYMENT_PENDING'});
 await expect(db.prepare('UPDATE pms_booking_payments SET amount_minor=1 WHERE id=?').bind(b.attemptToken!.split('.')[0]).run()).rejects.toThrow(/immutable/);
});
it('changes an unpaid future stay at equal price and releases only selected cancelled rooms',async()=>{
 await cmd('room.save',{number:'102',typeId,active:true});
 const b=await createPublicReservation(db,'acme',await request({quantity:2}),key(),delivery,now),access=await token(b.id),stays=records(await snapshot(),'stay').filter(st=>st.data.reservationId===b.id);
 const change=await publicChangeQuote(db,access,'change',{items:[{stayId:stays[0].id,room:selection({arrival:'2026-09-08',departure:'2026-09-10'})}]},delivery,now+1000);
 expect(change.proposal.dueNow).toBe(0);const k=key();await confirmBookingChange(db,'acme',access,change.token,k,delivery,now+1001);await confirmBookingChange(db,'acme',access,change.token,k,delivery,now+1002);
 const cancel=await publicChangeQuote(db,access,'cancel',{items:[{stayId:stays[1].id}]},delivery,now+2000);await confirmBookingChange(db,'acme',access,cancel.token,key(),delivery,now+2001);
 const s=await snapshot();expect(record(s,'stay',stays[0].id).data.arrival).toBe('2026-09-08');expect(record(s,'stay',stays[1].id).data.state).toBe('cancelled');expect(record(s,'reservation',b.id).data.state).toBe('confirmed');expect(records(s,'entry')).toHaveLength(0);
});
it('keeps the original stay while a more expensive change expires and holds only added capacity',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);provider.pay();await bookingPaymentStatus(db,b.attemptToken!,delivery,now+1000);
 const access=await token(b.id),stay=records(await snapshot(),'stay').find(st=>st.data.reservationId===b.id)!;
 const q=await publicChangeQuote(db,access,'change',{items:[{stayId:stay.id,room:selection({departure:'2026-09-08'})}]},delivery,now+2000);expect(q.proposal.dueNow).toBe(10700);
 const pending=await confirmBookingChange(db,'acme',access,q.token,key(),delivery,now+2001);expect(pending.state).toBe('payment_pending');
 const s=await snapshot();s.asOf=now+3000;expect(record(s,'stay',stay.id).data.departure).toBe('2026-09-07');expect(availability(s,typeId,'2026-09-06').available).toBe(0);expect(availability(s,typeId,'2026-09-07').available).toBe(0);
 await bookingPaymentStatus(db,(pending as {attemptToken:string}).attemptToken,delivery,now+32*60*1000);
 const original=record(await snapshot(),'stay',stay.id);expect(original.data.departure).toBe('2026-09-07');expect(original.data.state).toBe('reserved');
});
it('applies a paid change with accepted prices even when a rate is edited during Checkout',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);provider.pay();await bookingPaymentStatus(db,b.attemptToken!,delivery,now+1000);
 const access=await token(b.id),stay=records(await snapshot(),'stay').find(st=>st.data.reservationId===b.id)!;
 const q=await publicChangeQuote(db,access,'change',{items:[{stayId:stay.id,room:selection({departure:'2026-09-08'})}]},delivery,now+2000);
 const pending=await confirmBookingChange(db,'acme',access,q.token,key(),delivery,now+2001);
 await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,price:50000});provider.pay();
 await bookingPaymentStatus(db,(pending as {attemptToken:string}).attemptToken,delivery,now+3000);
 const changed=record(await snapshot(),'stay',stay.id);expect(changed.data.departure).toBe('2026-09-08');expect(changed.data.prices.reduce((n,p)=>n+p.total,0)).toBe(32100);
 expect(Number(await db.prepare('SELECT SUM(amount_minor)::int n FROM pms_booking_allocations WHERE stay_id=?').bind(stay.id).first('n'))).toBe(32100);
});
it('reserves partial refunds cumulatively and rejects an approval by the requester',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);provider.pay();await bookingPaymentStatus(db,b.attemptToken!,delivery,now+1000);
 const stay=records(await snapshot(),'stay').find(st=>st.data.reservationId===b.id)!,paymentId=b.attemptToken!.split('.')[0];let refundId='';
 for(const amount of [5000,4000])await db.transaction(async tx=>{const s=await bookingSnapshot(tx,propertyId,undefined,true);const r=await requestBookingRefund(tx,s,paymentId,[{stayId:stay.id,amount}],'Partial refund',owner.userId,now+2000);refundId=r.id;});
 expect(record(await snapshot(),'guest_account',stay.data.guestAccountId).data.refundReserved).toBe(9000);
 await expect(approveBookingRefund(db,owner,refundId)).rejects.toMatchObject({code:'PMS_INDEPENDENT_REFUND_APPROVAL'});
 await expect(db.transaction(async tx=>requestBookingRefund(tx,await bookingSnapshot(tx,propertyId,undefined,true),paymentId,[{stayId:stay.id,amount:13000}],'Too much',owner.userId,now+2001))).rejects.toMatchObject({code:'PMS_REFUND_EXCEEDS_PAYMENT'});
 await approveBookingRefund(db,operationalReviewer,refundId);
 const guestAccount=record(await snapshot(),'guest_account',stay.data.guestAccountId);expect(guestAccount.data.advance).toBe(17400);expect(guestAccount.data.refundReserved).toBe(5000);
 expect(Number(await db.prepare("SELECT SUM(l.credit_minor-l.debit_minor)::int n FROM finance_journal_lines l WHERE l.account_id=?").bind((await snapshot()).property.config.onlineAccounting!.refundPayable).first('n'))).toBe(4000);
});
it('confirms a timely provider payment after local hold expiry only when capacity is still available',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);
 await bookingPaymentStatus(db,b.attemptToken!,delivery,now+31*60*1000);
 await db.prepare('UPDATE pms_booking_payments SET provider_succeeded_at=? WHERE id=?').bind(now+1000,b.attemptToken!.split('.')[0]).run();provider.pay();
 expect((await bookingPaymentStatus(db,b.attemptToken!,delivery,now+32*60*1000)).state).toBe('confirmed');
});
it('records unallocated money once when a timely late webhook loses the last room',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);
 await bookingPaymentStatus(db,b.attemptToken!,delivery,now+31*60*1000);
 await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]});
 await db.prepare('UPDATE pms_booking_payments SET provider_succeeded_at=? WHERE id=?').bind(now+1000,b.attemptToken!.split('.')[0]).run();provider.pay();
 expect((await bookingPaymentStatus(db,b.attemptToken!,delivery,now+32*60*1000)).state).toBe('refund_review');await bookingPaymentStatus(db,b.attemptToken!,delivery,now+33*60*1000);
 expect(records(await snapshot(),'entry').filter(e=>e.data.description==='payment.unallocated')).toHaveLength(1);
 expect(Number(await db.prepare('SELECT SUM(l.credit_minor-l.debit_minor)::int n FROM finance_journal_lines l WHERE l.account_id=?').bind((await snapshot()).property.config.onlineAccounting!.unallocated).first('n'))).toBe(21400);
});

it('requires a signed staff revision after payment and records the authorized actor and reason',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);provider.pay();await bookingPaymentStatus(db,b.attemptToken!,delivery,now+1000);
 const stay=records(await snapshot(),'stay').find(st=>st.data.reservationId===b.id)!;
 await expect(cmd('stay.change_dates',{id:stay.id,arrival:'2026-09-08',departure:'2026-09-10',rateId,reason:'Direct edit'})).rejects.toMatchObject({code:'PMS_BOOKING_REVISION_REQUIRED'});
 const body={reservationId:b.id,reason:'Guest requested different dates',items:[{stayId:stay.id,room:selection({arrival:'2026-09-08',departure:'2026-09-10'})}]};
 const proposal=await staffBookingChange(db,owner,'acme',{...body,action:'change-quote'},key(),now+2000) as Awaited<ReturnType<typeof publicChangeQuote>>;
 await expect(confirmBookingChange(db,'acme',await token(b.id),proposal.token,key(),delivery,now+2001)).rejects.toMatchObject({code:'PMS_PROPOSAL_AUTHORITY'});
 await staffBookingChange(db,owner,'acme',{reservationId:b.id,reason:body.reason,action:'change-confirm',proposalToken:proposal.token},key(),now+2001);
 expect(record(await snapshot(),'stay',stay.id).data.arrival).toBe('2026-09-08');
 expect(record(await snapshot(),'reservation',b.id).data.history!.at(-1)).toMatchObject({actor:owner.userId});
 expect(record(await snapshot(),'reservation',b.id).data.history!.at(-1)!.action).toContain(body.reason);
});
it('uses each accepted room policy and posts cancellation fees separately from room revenue',async()=>{
 const provider=await onlinePolicy();const feeAccount=key();await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'CANCEL-FEE','Cancellation fees','revenue','THB',1,1)").bind(feeAccount,f.chart,f.company).run();
 await cmd('booking_fees.prepare',{account:feeAccount});await expect(cmd('booking_fees.approve')).rejects.toMatchObject({code:'PMS_INDEPENDENT_APPROVER'});await cmd('booking_fees.approve',{},reviewer);
 await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,collection:{method:'full',depositPercent:30,changesAllowed:true,cancellationDays:2,cancellationTime:'12:00',cancellationFeePercent:10}});
 const input=await request(),b=await createPublicReservation(db,'acme',input,key(),delivery,now);provider.pay();await bookingPaymentStatus(db,b.attemptToken!,delivery,now+1000);
 const stay=records(await snapshot(),'stay').find(st=>st.data.reservationId===b.id)!;
 expect(stay.data.bookingPolicy!.deadline).toBe(Date.parse('2026-09-03T05:00:00Z'));
 await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,collection:{method:'full',depositPercent:30,changesAllowed:true,cancellationFeePercent:90}});
 const q=await publicChangeQuote(db,await token(b.id),'cancel',{items:[{stayId:stay.id}]},delivery,now+2000);
 expect(q.proposal.fee).toBe(2140);expect(q.proposal.refund).toBe(19260);
 await confirmBookingChange(db,'acme',await token(b.id),q.token,key(),delivery,now+2001);
 const current=await snapshot();expect(record(current,'stay',stay.id).data.state).toBe('cancelled');expect(record(current,'guest_account',stay.data.guestAccountId).data.refundReserved).toBe(19260);
 const credit=async(account:string)=>Number(await db.prepare('SELECT COALESCE(SUM(credit_minor-debit_minor),0) n FROM finance_journal_lines WHERE account_id=?').bind(account).first('n'));
 expect(await credit(feeAccount)).toBe(2000);expect(await credit(accounts.revenue)).toBe(0);expect((await manageBooking(db,await token(b.id),now+3000)).dueAtHotel).toBe(0);
});
it('recovers a provider success after the local accounting transaction fails without taking another payment',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);provider.pay();
 await db.prepare("UPDATE finance_periods SET state='closed' WHERE id=?").bind(f.period).run();
 await expect(bookingPaymentStatus(db,b.attemptToken!,delivery,now+1000)).rejects.toMatchObject({code:'PMS_PERIOD_CLOSED'});
 const paymentId=b.attemptToken!.split('.')[0];expect((await db.prepare('SELECT paid_at FROM pms_booking_payments WHERE id=?').bind(paymentId).first<{paid_at:number}>())!.paid_at).toBeTruthy();
 expect(records(await snapshot(),'entry')).toHaveLength(0);expect((await snapshot()).records.filter(r=>r.kind==='reservation')[0].data).toMatchObject({state:'payment_pending'});
 await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();await bookingPaymentStatus(db,b.attemptToken!,delivery,now+2000);await bookingPaymentStatus(db,b.attemptToken!,delivery,now+3000);
 expect(Number(await db.prepare('SELECT COUNT(*) n FROM pms_booking_allocations WHERE payment_id=?').bind(paymentId).first('n'))).toBe(1);
 expect(vi.mocked(fetch).mock.calls.filter(([url,init])=>String(url).endsWith('/checkout/sessions')&&init?.method==='POST')).toHaveLength(1);
});
it('serves only guest documents belonging to the management token and excludes private accounting fields',async()=>{
 const provider=await onlinePolicy();const b=await createPublicReservation(db,'acme',await request(),key(),delivery,now);provider.pay();await bookingPaymentStatus(db,b.attemptToken!,delivery,now+1000);
 const doc=(await manageBooking(db,await token(b.id),now+2000)).documents[0];const result=await guestBookingDocument(db,await token(b.id),{id:doc.id},now+2000);
 expect(result.amount).toBe(21400);expect(JSON.stringify(result)).not.toContain(owner.userId);expect(JSON.stringify(result)).not.toContain(accounts.edc);
 await cmd('room.save',{number:'102',typeId,active:true});const another=await createPublicReservation(db,'acme',await request({arrival:'2026-09-10',departure:'2026-09-12'}),key(),delivery,now+3000);
 await expect(guestBookingDocument(db,await token(another.id),{id:doc.id},now+3001)).rejects.toMatchObject({code:'PMS_DOCUMENT_NOT_FOUND'});
});

it('retains counter partial refund generations and recovers a failed provider attempt without posting debt twice',async()=>{
 vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
 await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?,'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
 const reservation=await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]}),guestAccount=records(await snapshot(),'guest_account').find(r=>r.data.reservationId===reservation.id)!;
 let paymentId='',serial=0;const refunds=new Map<string,{amount:number;status:string;metadata:Record<string,string>}>();
 vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
  const path=String(url),body=new URLSearchParams(String(init?.body??''));
  if(path.includes('/refunds/')){const id=path.split('/').at(-1)!;return Response.json({id,...refunds.get(id)});}
  if(path.endsWith('/refunds')){const id=`re_partial_${++serial}`;refunds.set(id,{amount:Number(body.get('amount')),status:serial===2?'failed':'succeeded',metadata:{pms_counter_payment_id:paymentId,pms_counter_refund_generation:body.get('metadata[pms_counter_refund_generation]')!}});return Response.json({id});}
  if(path.includes('/payment_intents/'))return Response.json({id:'pi_partial',status:'succeeded',amount_received:5000,currency:'thb',metadata:{pms_counter_payment_id:paymentId}});
  paymentId=body.get('metadata[pms_counter_payment_id]')!;return Response.json({id:'pi_partial',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201'}}});
 }));
 const payment=await createPmsStripePayment(db,owner,{propertyId,guestAccountId:guestAccount.id,kind:'receipt',amountMinor:5000,payerEmail:'payer@example.test'},key(),now);
 await pmsStripePaymentStatus(db,owner,payment.id,now+1000);
 await requestPmsStripeRefund(db,owner,payment.id,2000,'First partial refund');await approvePmsStripeRefund(db,operationalReviewer,payment.id);await processPmsStripeRefund(db,payment.id,now+2000);
 await requestPmsStripeRefund(db,owner,payment.id,3000,'Second partial refund');await approvePmsStripeRefund(db,operationalReviewer,payment.id);await processPmsStripeRefund(db,payment.id,now+3000);
 expect(record(await snapshot(),'guest_account',guestAccount.id).data.advance).toBe(0);
 const liabilities=records(await snapshot(),'entry').filter(e=>e.data.description==='payment.refund_liability');expect(liabilities).toHaveLength(2);
 const owing=(await pmsFinanceOverview(owner,propertyId)).hotels[0];expect(owing.online.refundPayable).toBe(3000);expect(owing.onlineDifferences).toEqual({refundPayable:0,unallocated:0});
 await expect(recoverPmsStripeRefund(db,owner,payment.id,'Retry failed refund')).rejects.toThrow();
 await recoverPmsStripeRefund(db,operationalReviewer,payment.id,'Retry verified failed refund');
 // A guest can leave while the approved refund is still owed. Provider settlement must still post.
 await db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();await tx.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"closed\"')::text WHERE id=?").bind(guestAccount.id).run();});
 await processPmsStripeRefund(db,payment.id,now+4000);await processPmsStripeRefund(db,payment.id,now+5000);
 expect(records(await snapshot(),'entry').filter(e=>e.data.description==='payment.refund_liability')).toHaveLength(2);
 const attempts=(await db.prepare('SELECT generation,state,amount_minor FROM pms_counter_refund_attempts WHERE payment_id=? ORDER BY generation').bind(payment.id).all()).results;
 expect((await pmsFinanceOverview(owner,propertyId)).hotels[0].online.refundPayable).toBe(0);
 expect(attempts).toEqual([{generation:0,state:'succeeded',amount_minor:2000},{generation:1,state:'failed',amount_minor:3000},{generation:2,state:'succeeded',amount_minor:3000}]);
 await expect(requestPmsStripeRefund(db,owner,payment.id,1,'Exceeds original')).rejects.toThrow();
});

it('recovers a partial booking refund after the provider accepts it but its response is lost',async()=>{
 const provider=await onlinePolicy();await cmd('room.save',{number:'102',typeId,active:true});
 const booking=await createPublicReservation(db,'acme',await request({quantity:2}),key(),delivery,now);provider.pay();await bookingPaymentStatus(db,booking.attemptToken!,delivery,now+1000);
 const stays=records(await snapshot(),'stay').filter(st=>st.data.reservationId===booking.id),paymentId=booking.attemptToken!.split('.')[0];
 const access=await token(booking.id),quote=await publicChangeQuote(db,access,'cancel',{items:[{stayId:stays[0].id}]},delivery,now+2000);
 expect(quote.proposal.refund).toBe(21400);await confirmBookingChange(db,'acme',access,quote.token,key(),delivery,now+2001);
 const refund=(await db.prepare('SELECT id FROM pms_booking_refund_requests WHERE payment_id=?').bind(paymentId).first<{id:string}>())!;
 expect(await db.prepare('SELECT requested_by FROM pms_booking_refund_requests WHERE id=?').bind(refund.id).first('requested_by')).toBe('guest:self-service');
 await approveBookingRefund(db,operationalReviewer,refund.id);
 const existingFetch=fetch,accepted=new Map<string,{id:string;amount:number;currency:string;status:string;metadata:Record<string,string>}>(),keys:string[]=[];let loseResponse=true;
 vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
  const path=new URL(String(url)).pathname;
  if(path.endsWith('/refunds')&&init?.method==='POST'){
   expect(new Headers(init.headers).get('Stripe-Account')).toBe('acct_hotel');const idempotency=new Headers(init.headers).get('Idempotency-Key')!;keys.push(idempotency);
   const body=new URLSearchParams(String(init.body));accepted.set(idempotency,accepted.get(idempotency)??{id:'re_lost_booking',amount:Number(body.get('amount')),currency:'thb',status:'succeeded',metadata:{pms_booking_payment_id:body.get('metadata[pms_booking_payment_id]')!,pms_booking_refund_id:body.get('metadata[pms_booking_refund_id]')!}});
   if(loseResponse){loseResponse=false;throw new TypeError('Fictional provider accepted the refund; response connection lost');}
   return Response.json(accepted.get(idempotency));
  }
  if(path.endsWith('/refunds/re_lost_booking'))return Response.json([...accepted.values()][0]);
  return existingFetch(url,init);
 }));
 await expect(processBookingRefund(db,refund.id,now+3000)).rejects.toThrow('response connection lost');
 expect(await db.prepare('SELECT state,provider_id,generation FROM pms_booking_refund_requests WHERE id=?').bind(refund.id).first()).toEqual({state:'approved',provider_id:null,generation:0});
 expect((await pmsFinanceOverview(owner,propertyId)).hotels[0].online.refundPayable).toBe(21400);
 // Fresh processor invocation reads only durable request/provider facts after the failure.
 await processBookingRefund(db,refund.id,now+4000);await processBookingRefund(db,refund.id,now+5000);
 expect(keys).toEqual([`pms-refund-${refund.id}-0`,`pms-refund-${refund.id}-0`]);expect(accepted.size).toBe(1);
 expect(await db.prepare('SELECT state,provider_id,generation FROM pms_booking_refund_requests WHERE id=?').bind(refund.id).first()).toEqual({state:'succeeded',provider_id:'re_lost_booking',generation:0});
 expect(await db.prepare('SELECT COUNT(*)::int n FROM pms_booking_refund_attempts WHERE request_id=?').bind(refund.id).first('n')).toBe(1);
 const after=await snapshot();expect(record(after,'stay',stays[1].id)).toEqual(stays[1]);expect(record(after,'stay',stays[0].id).data.state).toBe('cancelled');
 const finance=(await pmsFinanceOverview(owner,propertyId)).hotels[0];expect(finance.online.refundPayable).toBe(0);expect(finance.onlineDifferences).toEqual({refundPayable:0,unallocated:0});
 expect(records(after,'entry').filter(e=>e.data.description==='payment.refund_settle')).toHaveLength(1);
});

it('deduplicates signed booking payment callbacks through durable jobs and rejects tampered payloads',async()=>{
 vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(now+1000);
 const provider=await onlinePolicy(),booking=await createPublicReservation(db,'acme',await request(),key(),delivery,now);provider.pay();
 const paymentId=booking.attemptToken!.split('.')[0];vi.stubEnv('POS_STRIPE_TEST_WEBHOOK_SECRET','whsec_booking_fixture');
 await env.CONTROL_DB.prepare("INSERT INTO pos_stripe_routes(account_id,livemode,holding_id,company_id,created_at) VALUES('acct_hotel',false,'h001',?,?) ON CONFLICT(account_id,livemode) DO UPDATE SET holding_id=EXCLUDED.holding_id,company_id=EXCLUDED.company_id").bind(f.company,now).run();
 const eventId=`evt_booking_${key()}`,raw=JSON.stringify({id:eventId,type:'payment_intent.succeeded',account:'acct_hotel',livemode:false,created:Math.floor((now+1000)/1000),data:{object:{id:'pi_fixture_1',metadata:{pms_booking_payment_id:paymentId}}}}),timestamp=Math.floor(Date.now()/1000),signature=`t=${timestamp},v1=${createHmac('sha256','whsec_booking_fixture').update(`${timestamp}.${raw}`).digest('hex')}`;
 await expect(posStripeWebhook(raw+' ',signature)).rejects.toMatchObject({code:'POS_WEBHOOK_SIGNATURE'});
 await posStripeWebhook(raw,signature);await posStripeWebhook(raw,signature);
 expect(records(await snapshot(),'entry')).toHaveLength(0);
 // Inject the durable lease left by an interrupted worker. An active lease
 // must not be stolen; a subsequent worker can reclaim it after expiry.
 await db.prepare("UPDATE pos_jobs SET state='running',lease_token='fictional-interrupted-worker',lease_until=? WHERE kind='event' AND record_id=?").bind(now+121000,eventId).run();
 await drainPos(db,'h001',8,f.company);expect(records(await snapshot(),'entry')).toHaveLength(0);
 vi.setSystemTime(now+121001);
 await drainPos(db,'h001',8,f.company);await posStripeWebhook(raw,signature);await drainPos(db,'h001',8,f.company);
 expect(await db.prepare('SELECT COUNT(*)::int n FROM pos_stripe_events WHERE id=? AND applied_at IS NOT NULL').bind(eventId).first('n')).toBe(1);
 expect(await db.prepare("SELECT COUNT(*)::int n FROM pos_jobs WHERE kind='event' AND record_id=? AND state='completed'").bind(eventId).first('n')).toBe(1);
 expect((await manageBooking(db,await token(booking.id),now+2000)).state).toBe('confirmed');
 const s=await snapshot();expect(records(s,'entry').filter(e=>e.data.sourceKey===`online-payment:${paymentId}`)).toHaveLength(1);
 expect(await db.prepare('SELECT COUNT(*)::int n FROM pms_booking_allocations WHERE payment_id=?').bind(paymentId).first('n')).toBe(1);
});

it('reviews and applies a staff group pickup date change within its held allotment without extra payment', async () => {
  const blockId = (await cmd('block.create', { name: 'Staff review group', typeId, from: '2026-09-03', to: '2026-09-08', releaseDate: '2026-09-02', quantity: 1 })).id;
  const booking = (await cmd('reservation.create', { guestId, source: 'phone', blockId, rooms: [selection({ arrival: '2026-09-03', departure: '2026-09-04' })] })).id;
  const stay = records(await snapshot(), 'stay').find(row => row.data.reservationId === booking)!;
  const body = { reservationId: booking, reason: 'Group moved its pickup dates', items: [{ stayId: stay.id, room: selection({ arrival: '2026-09-04', departure: '2026-09-06' }) }] };
  const proposal = await staffBookingChange(db, owner, 'acme', { ...body, action: 'change-quote' }, key(), now + 1000) as Awaited<ReturnType<typeof publicChangeQuote>>;
  expect(proposal.proposal.dueNow).toBe(0);
  const confirm = { reservationId: booking, reason: body.reason, action: 'change-confirm', proposalToken: proposal.token }, replayKey = key();
  const result = await staffBookingChange(db, owner, 'acme', confirm, replayKey, now + 1001);
  expect(await staffBookingChange(db, owner, 'acme', confirm, replayKey, now + 1002)).toEqual(result);
  const final = await snapshot();
  expect(record(final, 'stay', stay.id).data).toMatchObject({ arrival: '2026-09-04', departure: '2026-09-06' });
  expect(record(final, 'block', blockId).data.pickedStayIds).toEqual([stay.id]);
  expect(availability(final, typeId, '2026-09-03')).toMatchObject({ occupied: 0, held: 1 });
  expect(availability(final, typeId, '2026-09-04')).toMatchObject({ occupied: 1, held: 0 });
  expect(records(final, 'entry')).toEqual([]);
});

it('allows authorized staff to review an active private company rate while public offers exclude it', async () => {
  const privateRate = (await cmd('rate.save', { name: 'Company contract', typeId, from: '2026-09-01', to: '2026-12-31', price: 9000, market: 'company' })).id;
  const booking = (await cmd('reservation.create', { guestId, source: 'phone', rooms: [selection()] })).id;
  const stay = records(await snapshot(), 'stay').find(row => row.data.reservationId === booking)!;
  const proposal = await staffBookingChange(db, owner, 'acme', { action: 'change-quote', reservationId: booking, reason: 'Use approved company contract', items: [{ stayId: stay.id, room: selection({ rateId: privateRate }) }] }, key(), now + 1000) as Awaited<ReturnType<typeof publicChangeQuote>>;
  expect(proposal.proposal.items[0].quote?.lines[0].rateId).toBe(privateRate);
  await expect(publicQuote(db, propertyId, [selection({ rateId: privateRate })], delivery, now)).rejects.toMatchObject({ code: 'PMS_RATE_UNAVAILABLE' });
});

it('protects own group quota while an additional-payment date change is pending and applies it once after payment', async () => {
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(now + 1500);
  const provider = await onlinePolicy();
  await cmd('guest.save', { id: guestId, name: 'Group Guest', email: 'group@example.test' });
  await cmd('room.save', { number: '102', typeId, active: true });
  const blockId = (await cmd('block.create', { name: 'Paid revision group', typeId, from: '2026-09-03', to: '2026-09-08', releaseDate: '2026-09-02', quantity: 1 })).id;
  const booking = (await cmd('reservation.create', { guestId, source: 'phone', blockId, rooms: [selection({ arrival: '2026-09-03', departure: '2026-09-04' })] })).id;
  const stay = records(await snapshot(), 'stay').find(row => row.data.reservationId === booking)!;
  const body = { action: 'change-quote', reservationId: booking, reason: 'Group requests two different nights', items: [{ stayId: stay.id, room: selection({ arrival: '2026-09-04', departure: '2026-09-06' }) }] };
  const proposal = await staffBookingChange(db, owner, 'acme', body, key(), now + 1000) as Awaited<ReturnType<typeof publicChangeQuote>>;
  expect(proposal.proposal.dueNow).toBe(21400);
  const pending = await staffBookingChange(db, owner, 'acme', { action: 'change-confirm', reservationId: booking, reason: body.reason, proposalToken: proposal.token }, key(), now + 1001) as { attemptToken: string };
  expect(pending.attemptToken).toBeTruthy();
  const held = await snapshot();
  expect(record(held, 'stay', stay.id).data.arrival).toBe('2026-09-03');
  expect(availability(held, typeId, '2026-09-04').available).toBe(1);
  await expect(cmd('reservation.create', { guestId, source: 'phone', blockId, rooms: [selection({ arrival: '2026-09-04', departure: '2026-09-06' })] })).rejects.toMatchObject({ code: 'PMS_BLOCK_UNAVAILABLE' });
  provider.pay();
  await bookingPaymentStatus(db, pending.attemptToken, delivery, now + 2000);
  await bookingPaymentStatus(db, pending.attemptToken, delivery, now + 2001);
  const final = await snapshot();
  expect(record(final, 'stay', stay.id).data).toMatchObject({ arrival: '2026-09-04', departure: '2026-09-06' });
  expect(record(final, 'block', blockId).data.pickedStayIds).toEqual([stay.id]);
  expect(records(final, 'entry').filter(row => row.data.kind === 'payment')).toHaveLength(1);
  expect(availability(final, typeId, '2026-09-04')).toMatchObject({ occupied: 1, held: 0, available: 1 });
});

it('deducts an existing manual room advance from additional collection during a staff booking review', async () => {
  await onlinePolicy();
  const booking = (await cmd('reservation.create', { guestId, source: 'phone', rooms: [selection()] })).id;
  const stay = records(await snapshot(), 'stay').find(row => row.data.reservationId === booking)!;
  const shiftId = (await cmd('shift.open', { name: 'Manual advance desk', opening: 0 })).id;
  await cmd('payment.receive', { guestAccountId: stay.data.guestAccountId, shiftId, amount: 1000, method: 'cash', reference: 'Existing room advance' });
  const proposal = await staffBookingChange(db, owner, 'acme', { action: 'change-quote', reservationId: booking, reason: 'Review with existing advance', items: [{ stayId: stay.id, room: selection() }] }, key(), now + 1000) as Awaited<ReturnType<typeof publicChangeQuote>>;
  expect(proposal.proposal.dueNow).toBe(20400);
  expect(proposal.proposal.refund).toBe(0);
  expect(records(await snapshot(), 'entry').filter(row => row.data.kind === 'payment')).toHaveLength(1);
});

it('retains a manual advance on pay-at-hotel review and rejects stale funding before confirmation', async () => {
  const booking = (await cmd('reservation.create', { guestId, source: 'phone', rooms: [selection()] })).id;
  const stay = records(await snapshot(), 'stay').find(row => row.data.reservationId === booking)!;
  const shiftId = (await cmd('shift.open', { name: 'Advance review desk', opening: 0 })).id;
  await cmd('payment.receive', { guestAccountId: stay.data.guestAccountId, shiftId, amount: 1000, method: 'cash', reference: 'Existing advance' });
  const body = { action: 'change-quote', reservationId: booking, reason: 'Review hotel collection with advance', items: [{ stayId: stay.id, room: selection() }] };
  const proposal = await staffBookingChange(db, owner, 'acme', body, key(), now + 1000) as Awaited<ReturnType<typeof publicChangeQuote>>;
  expect(proposal.proposal.dueNow).toBe(0);
  expect(proposal.proposal.refund).toBe(0);
  expect(proposal.proposal.items[0].dueAtHotel).toBe(20400);
  await cmd('payment.receive', { guestAccountId: stay.data.guestAccountId, shiftId, amount: 500, method: 'cash', reference: 'Later advance' });
  await expect(staffBookingChange(db, owner, 'acme', { action: 'change-confirm', reservationId: booking, reason: body.reason, proposalToken: proposal.token }, key(), now + 1001)).rejects.toMatchObject({ code: 'PMS_REFUND_FUNDING_CHANGED' });
  expect(record(await snapshot(), 'guest_account', stay.data.guestAccountId).data.advance).toBe(1500);
  expect(records(await snapshot(), 'refund')).toEqual([]);
});

it('requests excess manual advance refund on a staff revision without changing its issued receipt', async () => {
  const booking = (await cmd('reservation.create', { guestId, source: 'phone', rooms: [selection()] })).id;
  const stay = records(await snapshot(), 'stay').find(row => row.data.reservationId === booking)!;
  const shiftId = (await cmd('shift.open', { name: 'Refund review desk', opening: 0 })).id;
  await cmd('payment.receive', { guestAccountId: stay.data.guestAccountId, shiftId, amount: 15000, method: 'cash', reference: 'Original advance' });
  const original = (await db.prepare('SELECT id,snapshot_json FROM pms_documents WHERE property_id=? ORDER BY id').bind(propertyId).all()).results;
  const body = { action: 'change-quote', reservationId: booking, reason: 'Shorten manual booking', items: [{ stayId: stay.id, room: selection({ departure: '2026-09-06' }) }] };
  const proposal = await staffBookingChange(db, owner, 'acme', body, key(), now + 1000) as Awaited<ReturnType<typeof publicChangeQuote>>;
  expect(proposal.proposal.dueNow).toBe(0); expect(proposal.proposal.refund).toBe(4300);
  await staffBookingChange(db, owner, 'acme', { action: 'change-confirm', reservationId: booking, reason: body.reason, proposalToken: proposal.token }, key(), now + 1001);
  const refund = records(await snapshot(), 'refund')[0];
  expect(refund.data).toMatchObject({ amount: 4300, state: 'requested', requestedBy: owner.userId });
  expect(record(await snapshot(), 'guest_account', stay.data.guestAccountId).data.advance).toBe(15000);
  expect((await db.prepare('SELECT id,snapshot_json FROM pms_documents WHERE property_id=? ORDER BY id').bind(propertyId).all()).results).toEqual(original);
  await expect(cmd('refund.approve', { id: refund.id })).rejects.toMatchObject({ code: 'PMS_INDEPENDENT_APPROVER' });
  await cmd('refund.approve', { id: refund.id }, operationalReviewer);
  expect(record(await snapshot(), 'refund', refund.id).data.state).toBe('approved');
});

async function pendingGroupHoldFixture() {
  vi.useFakeTimers({ toFake: ['Date'] }); vi.setSystemTime(now + 1500);
  await onlinePolicy();
  await cmd('guest.save', { id: guestId, name: 'Pending Group Guest', email: 'pending-group@example.test' });
  await cmd('room.save', { number: '102', typeId, active: true });
  const blockId = (await cmd('block.create', { name: 'Protected group', typeId, from: '2026-09-03', to: '2026-09-08', releaseDate: '2026-09-02', quantity: 1 })).id;
  const booking = (await cmd('reservation.create', { guestId, source: 'phone', blockId, rooms: [selection({ arrival: '2026-09-03', departure: '2026-09-04' })] })).id;
  const stay = records(await snapshot(), 'stay').find(row => row.data.reservationId === booking)!;
  const body = { action: 'change-quote', reservationId: booking, reason: 'Keep paid revision hold private to its owner', items: [{ stayId: stay.id, room: selection({ arrival: '2026-09-04', departure: '2026-09-06' }) }] };
  const proposal = await staffBookingChange(db, owner, 'acme', body, key(), now + 1000) as Awaited<ReturnType<typeof publicChangeQuote>>;
  await staffBookingChange(db, owner, 'acme', { action: 'change-confirm', reservationId: booking, reason: body.reason, proposalToken: proposal.token }, key(), now + 1001);
  const s = await snapshot(), hold = records(s, 'block').find(row => row.data.bookingChangeId && !row.data.quotaOnly)!;
  expect(hold).toBeTruthy(); return { s, hold };
}

it('refuses staff pickup of an internal provider hold without changing its capacity or booking', async () => {
  const { s, hold } = await pendingGroupHoldFixture();
  await expect(cmd('reservation.create', { guestId, source: 'phone', blockId: hold.id, rooms: [selection({ arrival: hold.data.from, departure: hold.data.to })] })).rejects.toMatchObject({ code: 'PMS_BLOCK_UNAVAILABLE' });
  expect(await snapshot()).toEqual(s);
});

it('refuses ordinary staff release of an internal provider hold without changing its capacity or booking', async () => {
  const { s, hold } = await pendingGroupHoldFixture();
  await expect(cmd('block.release', { id: hold.id })).rejects.toMatchObject({ code: 'PMS_BLOCK_TYPE' });
  expect(await snapshot()).toEqual(s);
});

it('keeps actual guest receipt recovery and Stripe checkout returns on the configured Booking host without changing payment callbacks or token authority',async()=>{
 vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');const {bookingEnv}=await import('../../../src/server/modules/pms/public/pms-booking'),configured=await bookingEnv();expect(configured.origin).toBe('https://booking-dev-erpx.jesadakorn.com');
 await db.prepare("UPDATE pms_booking_emails SET state='sent'").run();const unpaid=await createPublicReservation(db,'acme',await request(),key(),configured,now),send=vi.fn<typeof fetch>().mockResolvedValue(Response.json({id:'sandbox-provider-receipt'}));await deliverBookingEmails(db,configured,send,now);expect(send).toHaveBeenCalledTimes(1);const body=JSON.parse(String(send.mock.calls[0][1]?.body)) as {text:string},links=body.text.match(/https:\/\/booking-dev-erpx\.jesadakorn\.com\/book\/acme\/manage#token=[^\s]+/g);expect(links).toHaveLength(1);if(!links)throw Error('Actual management link missing');const link=new URL(links[0]),access=new URLSearchParams(link.hash.slice(1)).get('token');if(!access)throw Error('Original access token missing');expect((await manageBooking(db,access,now)).id).toBe(unpaid.id);await expect(manageBooking(db,access+'tampered',now)).rejects.toMatchObject({code:'PMS_LINK_EXPIRED'});await cancelPublicReservation(db,'acme',access,configured,now);
 vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','true');vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_isolated_host_return');await cmd('rate.save',{...record(await snapshot(),'rate',rateId).data,id:rateId,collection:{method:'full',depositPercent:30,changesAllowed:true}});await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?,'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();let success:string|null=null,cancel:string|null=null;vi.stubGlobal('fetch',vi.fn<typeof fetch>(Object.assign(async(url:Parameters<typeof fetch>[0],init?:Parameters<typeof fetch>[1])=>{expect(String(url)).toContain('/checkout/sessions');const form=new URLSearchParams(String(init?.body));success=form.get('success_url');cancel=form.get('cancel_url');return Response.json({id:'cs_isolated_booking_host',url:'https://checkout.stripe.com/c/pay/isolated-booking-host'});},{preconnect:vi.fn()})));const pending=await createPublicReservation(db,'acme',await request(),key(),configured,now);expect(pending.state).toBe('payment_pending');const expected='https://booking-dev-erpx.jesadakorn.com/book/acme/'+propertyId+'?'+new URLSearchParams({attempt:pending.attemptToken!});expect(success).toBe(expected);expect(cancel).toBe(expected);expect(vi.mocked(fetch)).toHaveBeenCalledTimes(1);
 // A genuine retained checkout must return through the actual Server Page even
 // after the owner withdraws the property from new public reservations.
 vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(now);
 const provider=vi.mocked(fetch);provider.mockImplementation(async(url,init)=>{expect(init?.method).not.toBe('POST');expect(String(url)).toContain('/checkout/sessions/cs_isolated_booking_host');return Response.json({id:'cs_isolated_booking_host',status:'open',payment_status:'unpaid'});});
 const {default:HotelPage}=await import('../../../src/app/book/[holdingSlug]/[propertyId]/page');
 const pageInput=(attempt:string|string[]=pending.attemptToken!,id=propertyId,holdingSlug='acme')=>({params:Promise.resolve({holdingSlug,propertyId:id}),searchParams:Promise.resolve({attempt})});
 const visible=await HotelPage(pageInput());expect(renderToStaticMarkup(visible)).toContain('Waiting for payment');
 await cmd('booking.save',settings(false));expect((await bookingCatalog(db)).some(hotel=>hotel.id===propertyId)).toBe(false);
 const page=await HotelPage(pageInput()),html=renderToStaticMarkup(page);expect(html).toContain('Waiting for payment');expect(html).toContain(pending.number);expect(html).not.toContain('Find rooms');expect(html).not.toContain('Guest details');expect(html).not.toContain('Test Hotel');expect(html).not.toContain('test@example');
 const {createElement}=await import('react'),{FrontLanguageContext}=await import('../../../src/components/front/front-language');const thai=renderToStaticMarkup(createElement(FrontLanguageContext.Provider,{value:'th'},page));expect(thai).toContain('รอการชำระเงิน');expect(thai).toContain('กันห้องไว้ให้ 30 นาที กรุณาชำระผ่าน Stripe Checkout');expect(thai).not.toContain('Waiting for payment');
 expect(provider).toHaveBeenCalledTimes(3); // checkout creation and one authoritative read per server render; SSR adds no GET
 await expect(HotelPage({params:Promise.resolve({holdingSlug:'acme',propertyId})})).rejects.toThrow('NEXT_HTTP_ERROR_FALLBACK;404');
 await expect(HotelPage(pageInput(pending.attemptToken!+'tampered'))).rejects.toThrow('NEXT_HTTP_ERROR_FALLBACK;404');
 await expect(HotelPage(pageInput([pending.attemptToken!,pending.attemptToken!]))).rejects.toThrow('NEXT_HTTP_ERROR_FALLBACK;404');
 await expect(HotelPage(pageInput(pending.attemptToken!,key()))).rejects.toThrow('NEXT_HTTP_ERROR_FALLBACK;404');
 await applyMigrations(env.H002_DB,env.STAFF_TEST_MIGRATIONS);await expect(HotelPage(pageInput(pending.attemptToken!,propertyId,'other-tenant'))).rejects.toThrow('NEXT_HTTP_ERROR_FALLBACK;404');
 provider.mockRejectedValueOnce(new Error('DECLARED_PROVIDER_OUTAGE'));await expect(HotelPage(pageInput())).rejects.toThrow('DECLARED_PROVIDER_OUTAGE');
 // Publication is restored independently; draining alone also removes new
 // inventory, while the original payment's history remains available.
 await cmd('booking.save',settings());await counterPackage();vi.stubEnv('ERPX_PMS_PACKAGE_ENFORCEMENT','enforced');await counterPackage('draining');expect((await bookingCatalog(db)).some(hotel=>hotel.id===propertyId)).toBe(false);
 const draining=await HotelPage(pageInput());expect(renderToStaticMarkup(draining)).toContain('Waiting for payment');
 const statusProps=(draining.props.children as {props:{initial:Record<string,unknown>}}).props.initial;expect(Object.keys(statusProps).sort()).toEqual(['checkoutUrl','managementToken','number','paymentState','refundState','state']);expect(JSON.stringify(statusProps)).not.toContain('Test Hotel');
 vi.setSystemTime(now+31*60*1000);provider.mockImplementation(async(url,init)=>{expect(init?.method).not.toBe('POST');expect(String(url)).toContain('/checkout/sessions/cs_isolated_booking_host');return Response.json({id:'cs_isolated_booking_host',status:'expired',payment_status:'unpaid'});});
 expect(renderToStaticMarkup(await HotelPage(pageInput()))).toContain('Payment period expired');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','disabled');expect((await bookingEnv()).origin).toBe(delivery.origin);
});
