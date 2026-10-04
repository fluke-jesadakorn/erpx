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
} from "../src/server/pms-booking";
import { createHmac } from "node:crypto";
import {createPmsStripePayment,pmsStripePaymentStatus,requestPmsStripeRefund,approvePmsStripeRefund,processPmsStripeRefund} from '../src/server/pms-stripe';
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
it('holds two rooms, then confirms and allocates each folio only after Stripe verifies the direct charge',async()=>{
  vi.stubEnv('PMS_STRIPE_BOOKING_ENABLED','true');vi.stubEnv('POS_STRIPE_TEST_SECRET_KEY','sk_test_booking_fixture');
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
