import { cancellationEligibility, localDeadline } from "../src/lib/pms-booking";
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
} from "../src/server/pms-booking";
import {guestBookingDocument} from "../src/server/pms/guest-document";
import {staffBookingChange} from "../src/server/pms/staff-booking-changes";
import { publicChangeQuote, confirmBookingChange } from "../src/server/pms-booking-changes";
import { createHmac } from "node:crypto";
import {pmsFinanceOverview} from '../src/server/pms/finance';
import {createPmsStripePayment,pmsStripePaymentStatus,requestPmsStripeRefund,approvePmsStripeRefund,processPmsStripeRefund,recoverPmsStripeRefund} from '../src/server/pms-stripe';
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { pmsCommand, loadPms } from "../src/server/pms";
import { records, record, pmsAccountKeys, type Config } from "../src/lib/pms";
import { availability } from "../src/lib/pms-engine";
import type { WorkContext } from "../src/server/authorization";
import { confirmManualRate } from "../src/server/company-money";

vi.mock("../src/server/runtime", async () => {
  const { env } = await import("./runtime-env");
  return { holdingResources: async () => ({ write: env.H001_DB }), runtimeEnv:async()=>({BETTER_AUTH_SECRET:"test-booking-secret-32-characters-long",ERPX_PUBLIC_ORIGIN:"https://hotel.example.test",RESEND_API_KEY:"test-key",RESEND_FROM:"hotel@example.test"}) };
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
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','false');
  f = await seedExpenseCompany(db, env.H001_FILES);
  await db.prepare('DELETE FROM pms_booking_payments').run();
  await db.prepare('DELETE FROM pos_connections WHERE company_id=?').bind(f.company).run();
  await db.prepare("DELETE FROM pos_connections WHERE account_id='acct_hotel'").run();
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
afterEach(()=>{vi.unstubAllGlobals();vi.unstubAllEnvs();});

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
it('holds two rooms, then confirms and allocates each folio only after Stripe verifies the direct charge',async()=>{
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
  const s=await snapshot();expect(records(s,'folio')).toHaveLength(2);expect(records(s,'entry').filter(e=>e.data.method==='stripe')).toHaveLength(2);
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
it('requires Finance approval and Stripe success before a free-cancellation refund reaches each folio',async()=>{
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
  const pending=await createPublicReservation(db,'acme',await request(),key(),delivery,now);
  attemptId=pending.attemptToken!.split('.')[0];amount=Number((await db.prepare('SELECT amount_minor FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{amount_minor:number}>())!.amount_minor);
  await bookingPaymentStatus(db,pending.attemptToken!,delivery,now+1000);
  const cancelled=await cancelPublicReservation(db,'acme',await token(pending.id),delivery,now+2000);
  expect(cancelled.refundState).toBe('requested');
  expect((await db.prepare('SELECT refund_id FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{refund_id:string|null}>())!.refund_id).toBeNull();
  await approveBookingRefund(db,reviewer,attemptId);
  await processBookingRefund(db,attemptId,now+3000);
  expect((await db.prepare('SELECT refund_state FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{refund_state:string}>())!.refund_state).toBe('processing');
  refundState='succeeded';await processBookingRefund(db,attemptId,now+4000);
  expect((await db.prepare('SELECT refund_state FROM pms_booking_payments WHERE id=?').bind(attemptId).first<{refund_state:string}>())!.refund_state).toBe('succeeded');
  expect(records(await snapshot(),'folio')[0].data.advance).toBe(0);
});
it('posts a counter PromptPay room payment and deposit only after Stripe confirms each intent',async()=>{
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
  await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?, 'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
  const reservation=await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]});
  const folio=records(await snapshot(),'folio').find(item=>item.data.reservationId===reservation.id)!;
  let paid=false,activeId='',activeAmount=0,serial=0;
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
    expect(new Headers(init?.headers).get('Stripe-Account')).toBe('acct_hotel');
    if(init?.method==='POST'){
      const body=init.body as URLSearchParams;activeId=body.get('metadata[pms_counter_payment_id]')??'';activeAmount=Number(body.get('amount'));serial++;
      return new Response(JSON.stringify({id:`pi_counter_${serial}`,status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201010212'}}}));
    }
    return new Response(JSON.stringify({id:`pi_counter_${serial}`,status:paid?'succeeded':'requires_action',amount_received:paid?activeAmount:0,currency:'thb',metadata:{pms_counter_payment_id:activeId}}));
  }));
  const receipt=await createPmsStripePayment(db,owner,{propertyId,folioId:folio.id,kind:'receipt',amountMinor:5000,payerEmail:''},key(),now);
  expect(receipt.state).toBe('pending');expect(receipt.qrData).toBe('000201010212');expect(records(await snapshot(),'entry')).toHaveLength(0);
  paid=true;expect((await pmsStripePaymentStatus(db,owner,receipt.id,now+1000)).state).toBe('succeeded');
  paid=false;
  const deposit=await createPmsStripePayment(db,owner,{propertyId,folioId:folio.id,kind:'deposit',amountMinor:2000,payerEmail:''},key(),now+2000);
  paid=true;expect((await pmsStripePaymentStatus(db,owner,deposit.id,now+3000)).state).toBe('succeeded');
  const latest=record(await snapshot(),'folio',folio.id);
  expect(latest.data.advance).toBe(5000);expect(latest.data.deposit).toBe(2000);
});
it('requires an independent Finance approval and Stripe confirmation for a counter refund',async()=>{
  vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
  await db.prepare("INSERT INTO pos_connections(company_id,account_id,livemode,charges_enabled,payouts_enabled,card_enabled,promptpay_enabled,state,payout_interval,thb_bank_ready) VALUES(?, 'acct_hotel',false,true,true,true,true,'ready','daily',true)").bind(f.company).run();
  const reservation=await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]});
  const folio=records(await snapshot(),'folio').find(item=>item.data.reservationId===reservation.id)!;
  let paymentId='',refundState='pending';
  vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
    const path=String(url);
    if(path.includes('/refunds/'))return new Response(JSON.stringify({id:'re_counter',amount:5000,status:refundState,metadata:{pms_counter_payment_id:paymentId}}));
    if(path.endsWith('/refunds')&&init?.method==='POST')return new Response(JSON.stringify({id:'re_counter',status:'pending'}));
    if(path.includes('payment_intents/'))return new Response(JSON.stringify({id:'pi_counter',status:'succeeded',amount_received:5000,currency:'thb',metadata:{pms_counter_payment_id:paymentId}}));
    const body=init?.body as URLSearchParams;paymentId=body.get('metadata[pms_counter_payment_id]')??'';
    return new Response(JSON.stringify({id:'pi_counter',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201010212'}}}));
  }));
  const payment=await createPmsStripePayment(db,owner,{propertyId,folioId:folio.id,kind:'receipt',amountMinor:5000,payerEmail:'payer@example.com'},key(),now);
  const sourceMoney=await db.prepare("SELECT fx_rate_id,usd_units::text FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_stripe_payment' AND source_id=? AND amount_kind='requested'").bind(f.company,payment.id).first<{fx_rate_id:string;usd_units:string}>();
  await confirmManualRate(reviewer,{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'40',note:'Provider receipt and refund must retain requested FX'},key());
  await pmsStripePaymentStatus(db,owner,payment.id,now+1000);
  await requestPmsStripeRefund(db,owner,payment.id,5000,'Guest cancellation');
  await expect(approvePmsStripeRefund(db,owner,payment.id)).rejects.toThrow();
  expect((await db.prepare('SELECT refund_id FROM pms_stripe_payments WHERE id=?').bind(payment.id).first<{refund_id:string|null}>())!.refund_id).toBeNull();
  await approvePmsStripeRefund(db,reviewer,payment.id);
  await processPmsStripeRefund(db,payment.id,now+2000);
  expect((await db.prepare('SELECT refund_state FROM pms_stripe_payments WHERE id=?').bind(payment.id).first<{refund_state:string}>())!.refund_state).toBe('processing');
  refundState='succeeded';await processPmsStripeRefund(db,payment.id,now+3000);
  expect((await db.prepare('SELECT refund_state FROM pms_stripe_payments WHERE id=?').bind(payment.id).first<{refund_state:string}>())!.refund_state).toBe('succeeded');
  expect(record(await snapshot(),'folio',folio.id).data.advance).toBe(0);
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
 expect(record(await snapshot(),'folio',stay.data.folioId).data.refundReserved).toBe(9000);
 await expect(approveBookingRefund(db,owner,refundId)).rejects.toMatchObject({code:'PMS_INDEPENDENT_REFUND_APPROVAL'});
 await expect(db.transaction(async tx=>requestBookingRefund(tx,await bookingSnapshot(tx,propertyId,undefined,true),paymentId,[{stayId:stay.id,amount:13000}],'Too much',owner.userId,now+2001))).rejects.toMatchObject({code:'PMS_REFUND_EXCEEDS_PAYMENT'});
 await approveBookingRefund(db,reviewer,refundId);
 const folio=record(await snapshot(),'folio',stay.data.folioId);expect(folio.data.advance).toBe(17400);expect(folio.data.refundReserved).toBe(5000);
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
 const current=await snapshot();expect(record(current,'stay',stay.id).data.state).toBe('cancelled');expect(record(current,'folio',stay.data.folioId).data.refundReserved).toBe(19260);
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
 const reservation=await cmd('reservation.create',{guestId,source:'direct',rooms:[selection()]}),folio=records(await snapshot(),'folio').find(r=>r.data.reservationId===reservation.id)!;
 let paymentId='',serial=0;const refunds=new Map<string,{amount:number;status:string;metadata:Record<string,string>}>();
 vi.stubGlobal('fetch',vi.fn(async(url:string|URL|Request,init?:RequestInit)=>{
  const path=String(url),body=new URLSearchParams(String(init?.body??''));
  if(path.includes('/refunds/')){const id=path.split('/').at(-1)!;return Response.json({id,...refunds.get(id)});}
  if(path.endsWith('/refunds')){const id=`re_partial_${++serial}`;refunds.set(id,{amount:Number(body.get('amount')),status:serial===2?'failed':'succeeded',metadata:{pms_counter_payment_id:paymentId,pms_counter_refund_generation:body.get('metadata[pms_counter_refund_generation]')!}});return Response.json({id});}
  if(path.includes('/payment_intents/'))return Response.json({id:'pi_partial',status:'succeeded',amount_received:5000,currency:'thb',metadata:{pms_counter_payment_id:paymentId}});
  paymentId=body.get('metadata[pms_counter_payment_id]')!;return Response.json({id:'pi_partial',status:'requires_action',next_action:{promptpay_display_qr_code:{data:'000201'}}});
 }));
 const payment=await createPmsStripePayment(db,owner,{propertyId,folioId:folio.id,kind:'receipt',amountMinor:5000,payerEmail:'payer@example.test'},key(),now);
 await pmsStripePaymentStatus(db,owner,payment.id,now+1000);
 await requestPmsStripeRefund(db,owner,payment.id,2000,'First partial refund');await approvePmsStripeRefund(db,reviewer,payment.id);await processPmsStripeRefund(db,payment.id,now+2000);
 await requestPmsStripeRefund(db,owner,payment.id,3000,'Second partial refund');await approvePmsStripeRefund(db,reviewer,payment.id);await processPmsStripeRefund(db,payment.id,now+3000);
 expect(record(await snapshot(),'folio',folio.id).data.advance).toBe(0);
 const liabilities=records(await snapshot(),'entry').filter(e=>e.data.description==='payment.refund_liability');expect(liabilities).toHaveLength(2);
 const owing=(await pmsFinanceOverview(owner,propertyId)).hotels[0];expect(owing.online.refundPayable).toBe(3000);expect(owing.onlineDifferences).toEqual({refundPayable:0,unallocated:0});
 await expect(recoverPmsStripeRefund(db,owner,payment.id,'Retry failed refund')).rejects.toThrow();
 await recoverPmsStripeRefund(db,reviewer,payment.id,'Retry verified failed refund');
 // A guest can leave while the approved refund is still owed. Provider settlement must still post.
 await db.transaction(async tx=>{await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();await tx.prepare("UPDATE pms_records SET data_json=jsonb_set(data_json::jsonb,'{state}','\"closed\"')::text WHERE id=?").bind(folio.id).run();});
 await processPmsStripeRefund(db,payment.id,now+4000);await processPmsStripeRefund(db,payment.id,now+5000);
 expect(records(await snapshot(),'entry').filter(e=>e.data.description==='payment.refund_liability')).toHaveLength(2);
 const attempts=(await db.prepare('SELECT generation,state,amount_minor FROM pms_counter_refund_attempts WHERE payment_id=? ORDER BY generation').bind(payment.id).all()).results;
 expect((await pmsFinanceOverview(owner,propertyId)).hotels[0].online.refundPayable).toBe(0);
 expect(attempts).toEqual([{generation:0,state:'succeeded',amount_minor:2000},{generation:1,state:'failed',amount_minor:3000},{generation:2,state:'succeeded',amount_minor:3000}]);
 await expect(requestPmsStripeRefund(db,owner,payment.id,1,'Exceeds original')).rejects.toThrow();
});
