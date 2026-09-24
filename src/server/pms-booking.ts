import "server-only";
import {
  createHash,
  createHmac,
  randomUUID,
  randomBytes,
  createCipheriv,
  createDecipheriv,
  timingSafeEqual,
} from "node:crypto";
import { z } from "zod";
import {
  bookingOffers,
  bookingQuote,
  cancellationEligibility,
  guestInputSchema,
  roomSelectionSchema,
  searchSchema,
  type BookingSearch,
} from "@/lib/pms-booking";
import {
  record,
  records,
  type Kind,
  type Property,
  type Snapshot,
} from "@/lib/pms";
import { executePms, PmsError } from "@/lib/pms-engine";
import { effects as persistPmsEffects, persistPmsRecords } from "./pms";
import { stripeCall } from "./pos-stripe";
import { ApiError } from "./http";
import type { WorkContext } from "./authorization";
import { requirePermission } from "./iam-permissions";
import { holdingBindingBySlug, holdingResources, runtimeEnv } from "./runtime";
import {
  isTransactionalSqlDatabase,
  type SqlDatabase,
  type TransactionalSqlDatabase,
} from "./runtime/types";

export type BookingEnv = {
  secret: string;
  origin: string;
  apiKey?: string;
  from?: string;
};
export async function bookingEnv(): Promise<BookingEnv> {
  const env = await runtimeEnv();
  return {
    secret: env.BETTER_AUTH_SECRET,
    origin: env.ERPX_PUBLIC_ORIGIN,
    apiKey: env.RESEND_API_KEY,
    from: env.RESEND_FROM,
  };
}
export async function bookingDatabase(slug: string) {
  const binding = await holdingBindingBySlug(slug),
    { write } = await holdingResources(binding.id);
  if (!isTransactionalSqlDatabase(write))
    throw new Error("POSTGRES_REQUIRED");
  return write;
}
const hash = (v: string) => createHash("sha256").update(v).digest("hex");
const sign = (secret: string, v: string) =>
  createHmac("sha256", secret).update(`pms-booking:${v}`).digest("base64url");
function protectMail(value: string, env: BookingEnv, id: string) {
  const iv = randomBytes(12),
    cipher = createCipheriv(
      "aes-256-gcm",
      createHash("sha256").update(`pms-mail:${env.secret}`).digest(),
      iv,
    );
  cipher.setAAD(Buffer.from(id));
  const body = Buffer.concat([cipher.update(value, "utf8"), cipher.final()]);
  return JSON.stringify({
    iv: iv.toString("base64"),
    body: body.toString("base64"),
    tag: cipher.getAuthTag().toString("base64"),
  });
}
function unprotectMail(value: string, env: BookingEnv, id: string) {
  const v = JSON.parse(value),
    decipher = createDecipheriv(
      "aes-256-gcm",
      createHash("sha256").update(`pms-mail:${env.secret}`).digest(),
      Buffer.from(v.iv, "base64"),
    );
  decipher.setAAD(Buffer.from(id));
  decipher.setAuthTag(Buffer.from(v.tag, "base64"));
  return Buffer.concat([
    decipher.update(Buffer.from(v.body, "base64")),
    decipher.final(),
  ]).toString("utf8");
}
const same = (a: string, b: string) => {
  const x = Buffer.from(a),
    y = Buffer.from(b);
  return x.length === y.length && timingSafeEqual(x, y);
};
const run = (db: SqlDatabase, sql: string, ...v: unknown[]) =>
  db
    .prepare(sql)
    .bind(...v)
    .run();
const rows = async <T>(db: SqlDatabase, sql: string, ...v: unknown[]) =>
  (
    await db
      .prepare(sql)
      .bind(...v)
      .all<T>()
  ).results;
export async function bookingSnapshot(
  db: SqlDatabase,
  propertyId: string,
  search?: BookingSearch,
  lock = false,
): Promise<Snapshot> {
  const row = await db
    .prepare(
      `SELECT * FROM pms_properties WHERE id=?${lock ? " FOR UPDATE" : ""}`,
    )
    .bind(propertyId)
    .first<{
      id: string;
      company_id: string;
      branch_id: string;
      name: string;
      business_date: string;
      state: Property["state"];
      version: number;
      config_json: string;
    }>();
  if (!row) throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found");
  const raw = await rows<{
    id: string;
    kind: Kind;
    data_json: string;
    version: number;
    updated_at: number;
  }>(
    db,
    `SELECT * FROM pms_records WHERE property_id=?${search ? " AND (kind IN ('room_type','room','rate') OR (kind='block' AND data_json::jsonb->>'state'='active') OR (kind='stay' AND data_json::jsonb->>'state' IN ('reserved','in_house') AND data_json::jsonb->>'arrival' < ? AND data_json::jsonb->>'departure' > ?))" : ""} ORDER BY id`,
    propertyId,
    ...(search ? [search.departure, search.arrival] : []),
  );
  return {
    property: {
      id: row.id,
      companyId: row.company_id,
      branchId: row.branch_id,
      name: row.name,
      businessDate: row.business_date,
      state: row.state,
      version: Number(row.version),
      config: JSON.parse(row.config_json),
    },
    records: raw.map((r) => ({
      id: r.id,
      kind: r.kind,
      data: JSON.parse(r.data_json),
      version: Number(r.version),
      updatedAt: Number(r.updated_at),
    })),
  };
}
function published(s: Snapshot) {
  if (!s.property.config.booking?.published)
    throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found");
}
export async function bookingCatalog(db: SqlDatabase) {
  const hotels = await rows<{ id: string; name: string; config_json: string }>(
    db,
    "SELECT id,name,config_json FROM pms_properties WHERE state IN ('open','closing') AND config_json::jsonb->'booking'->>'published'='true' ORDER BY name LIMIT 200",
  );
  return hotels.map((r) => {
    const c = JSON.parse(r.config_json) as Property["config"],
      b = c.booking!;
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
      roomContent: b.roomContent,
    };
  });
}
export async function publicOffers(
  db: TransactionalSqlDatabase,
  input: unknown,
  propertyId?: string,
) {
  const search = searchSchema.parse(input),
    catalog = await bookingCatalog(db),
    selected = propertyId
      ? catalog.filter((h) => h.id === propertyId)
      : catalog;
  return Promise.all(
    selected.map(async (hotel) => ({
      hotel,
      offers: await db.transaction(async (tx) =>
        bookingOffers(
          await bookingSnapshot(tx, hotel.id, search),
          search,
          true,
        ),
      ),
    })),
  );
}
function publicSelections(s: Snapshot, raw: unknown, now: number) {
  published(s);
  const lines = z.array(roomSelectionSchema).min(1).max(10).parse(raw);
  if (lines.reduce((n, l) => n + l.quantity, 0) > 20)
    throw new ApiError(
      422,
      "PMS_ROOM_LIMIT",
      "Book at most 20 rooms online; contact the hotel for groups",
    );
  for (const line of lines)
    if (
      !s.property.config.booking!.rateIds.includes(line.rateId) ||
      record(s, "rate", line.rateId).data.market !== "public"
    )
      throw new ApiError(
        422,
        "PMS_RATE_UNAVAILABLE",
        "Choose an available public rate",
      );
  const today = new Intl.DateTimeFormat("en-CA", {
    timeZone: s.property.config.timezone,
  }).format(new Date(now));
  if (lines.some((line) => line.arrival < today))
    throw new ApiError(
      422,
      "PMS_PAST_ARRIVAL",
      "Choose a current or future arrival date",
    );
  return lines;
}
export async function publicQuote(
  db: TransactionalSqlDatabase,
  propertyId: string,
  raw: unknown,
  env: BookingEnv,
  now = Date.now(),
) {
  return db.transaction(async (tx) => {
    const lines = z.array(roomSelectionSchema).min(1).max(10).parse(raw),
      search = {
        ...lines[0],
        arrival: lines.map((l) => l.arrival).sort()[0],
        departure: lines
          .map((l) => l.departure)
          .sort()
          .at(-1)!,
      };
    const s = await bookingSnapshot(tx, propertyId, search),
      quote = onlineQuote(bookingQuote(s, publicSelections(s, lines, now)));
    const payload = Buffer.from(
      JSON.stringify({ quote, expires: now + 15 * 60 * 1000 }),
    ).toString("base64url");
    return { quote, token: `${payload}.${sign(env.secret, payload)}` };
  });
}
function onlineQuote(quote:ReturnType<typeof bookingQuote>){
 return {...quote,...(quote.policy?{policy:{...quote.policy,payment:'online' as const,text:quote.policy.text.replace('Pay at the hotel.','Full payment is required before confirmation.')}}:{})};
}
function readQuote(token: string, env: BookingEnv, now: number) {
  const [payload, signature] = token.split(".");
  if (!payload || !signature || !same(signature, sign(env.secret, payload)))
    throw new ApiError(
      422,
      "PMS_QUOTE_INVALID",
      "Search again for a fresh quote",
    );
  const data = JSON.parse(Buffer.from(payload, "base64url").toString()) as {
    quote: ReturnType<typeof bookingQuote>;
    expires: number;
  };
  if (data.expires <= now)
    throw new ApiError(
      409,
      "PMS_QUOTE_EXPIRED",
      "Your quote expired. Review a fresh quote",
    );
  return data.quote;
}
async function persist(
  db: SqlDatabase,
  s: Snapshot,
  before: Map<string, string>,
) {
  await persistPmsRecords(db, s, before);
  await run(
    db,
    "UPDATE pms_properties SET version=?,updated_at=? WHERE id=?",
    s.property.version,
    Date.now(),
    s.property.id,
  );
}
function tokenValue(id: string, env: BookingEnv) {
  return `${id}.${sign(env.secret, `access:${id}`)}`;
}
async function queueMail(
  db: SqlDatabase,
  s: Snapshot,
  reservationId: string,
  slug: string,
  kind: "confirmation" | "cancellation" | "access",
  env: BookingEnv,
  now: number,
) {
  const id = randomUUID(),
    token = tokenValue(id, env);
  await run(
    db,
    "INSERT INTO pms_booking_tokens(id,property_id,reservation_id,token_hash,expires_at,created_at) VALUES(?,?,?,?,?,?)",
    id,
    s.property.id,
    reservationId,
    hash(token),
    now + 7 * 86400000,
    now,
  );
  const b = bookingSummary(s, reservationId, now);
  const link = `${env.origin}/book/${encodeURIComponent(slug)}/manage#token=${encodeURIComponent(token)}`;
  const subject = `${b.hotel.name} · ${kind === "cancellation" ? "Cancellation" : kind === "access" ? "Booking access" : "Booking confirmation"} ${b.number}`;
  const payload = {
    from: env.from,
    to: [b.guest.email],
    subject,
    text: `${subject}\n\n${b.rooms.map((r) => `${r.roomName}: ${r.arrival} to ${r.departure}`).join("\n")}\n\n${b.policy?.text ?? "Contact the hotel for booking terms."}\n\nView your booking: ${link}\nThis private link expires in seven days. Request another from the booking website.\n\n${b.hotel.contactPhone}`,
  };
  const mailId = randomUUID();
  await run(
    db,
    "INSERT INTO pms_booking_emails(id,property_id,reservation_id,token_id,holding_slug,kind,payload_json,next_attempt_at,created_at) VALUES(?,?,?,?,?,?,?,?,?)",
    mailId,
    s.property.id,
    reservationId,
    id,
    slug,
    kind,
    protectMail(JSON.stringify(payload), env, mailId),
    now,
    now,
  );
}
function bookingSummary(s: Snapshot, id: string, now: number) {
  const r = record(s, "reservation", id),
    b = s.property.config.booking,
    guest = record(s, "guest", r.data.guestId).data;
  return {
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
    currency: s.property.config.currency,
    policy: r.data.bookingPolicy,
    rooms: records(s, "stay")
      .filter((st) => st.data.reservationId === id)
      .map((st) => ({
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
export const publicReservationSchema = z.object({
  propertyId: z.string().min(1).max(128),
  quoteToken: z.string().min(1).max(100000),
  guest: guestInputSchema,
  notes: z.string().trim().max(2000).default(""),
});
export async function createPublicReservation(
  db: TransactionalSqlDatabase,
  slug: string,
  raw: unknown,
  key: string,
  env: BookingEnv,
  now = Date.now(),
) {
  const v = publicReservationSchema.parse(raw);
  z.string().uuid().parse(key);
  if (!env.apiKey || !env.from)
    throw new ApiError(
      503,
      "PMS_EMAIL_UNAVAILABLE",
      "Online booking is temporarily unavailable. Contact the hotel",
    );
  const digest = hash(JSON.stringify(v));
  const stripeBooking=process.env.PMS_STRIPE_BOOKING_ENABLED!=='false';
  const created = await db.transaction(async (tx) => {
    // Same Property lock as staff bookings, room moves, maintenance and night audit.
    const s = await bookingSnapshot(tx, v.propertyId, undefined, true);
    const old = await tx
      .prepare("SELECT * FROM pms_booking_receipts WHERE id=?")
      .bind(key)
      .first<{ body_hash: string; reservation_id: string; response_json: string | null }>();
    if (old) {
      if (old.body_hash !== digest)
        throw new ApiError(
          409,
          "PMS_KEY_REUSED",
          "Use a new request key for a different booking",
        );
      if (!old.response_json)
        throw new ApiError(
          409,
          "PMS_ALREADY_CONFIRMED",
          "This booking was already confirmed. Use your confirmation email to manage it",
        );
      return JSON.parse(old.response_json) as {attemptId?:string} & ReturnType<typeof bookingSummary>;
    }
    const quoted = readQuote(v.quoteToken, env, now);
    if (quoted.propertyId !== s.property.id)
      throw new ApiError(
        422,
        "PMS_QUOTE_INVALID",
        "Quote belongs to another hotel",
      );
    const lines = publicSelections(s, quoted.lines, now),
      fresh = onlineQuote(bookingQuote(s, lines));
    if (JSON.stringify(fresh) !== JSON.stringify(quoted))
      throw new ApiError(
        409,
        "PMS_QUOTE_CHANGED",
        "Price or policy changed. Review a fresh quote",
      );
    if(fresh.currency!=='THB'||fresh.total<=0)throw new ApiError(422,'PMS_STRIPE_CURRENCY','Online payment requires a positive THB quote');
    const connection=stripeBooking?await tx.prepare("SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND promptpay_enabled=true AND charges_enabled=true AND payouts_enabled=true AND thb_bank_ready=true AND payout_interval='daily'").bind(s.property.companyId).first<{account_id:string;livemode:boolean}>():null;
    if(stripeBooking&&(!connection?.account_id||connection.livemode&&process.env.PMS_STRIPE_LIVE_ENABLED!=='true'))throw new ApiError(503,'PMS_PAYMENT_UNAVAILABLE','Online booking payment is unavailable for this company');
    const before = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
    const result = executePms(
      s,
      "reservation.create",
      {
        guest: v.guest,
        source: "website",
        rooms: lines,
        notes: v.notes,
        paymentPending: stripeBooking,
      },
      {
        userId: "guest:website",
        now,
        uuid: randomUUID,
        independent: () => false,
      },
    );
    if(stripeBooking)record(s,'reservation',result.id).data.bookingPolicy=fresh.policy;
    if (
      result.effects.postings.length ||
      result.effects.stock.length ||
      result.effects.documents.length
    )
      throw new Error("PUBLIC_BOOKING_FINANCIAL_EFFECT");
    await persist(tx, s, before);
    if(!stripeBooking){
      const response=bookingSummary(s,result.id,now);
      await run(tx,"INSERT INTO pms_booking_receipts(id,property_id,body_hash,reservation_id,created_at,response_json) VALUES(?,?,?,?,?,?)",key,s.property.id,digest,result.id,now,JSON.stringify(response));
      await queueMail(tx,s,result.id,slug,'confirmation',env,now);
      return response;
    }
    const attemptId=randomUUID(),expiresAt=now+30*60*1000;
    await run(tx,"INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,status,expires_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'THB','payment_pending',?,?,?)",attemptId,s.property.id,result.id,s.property.companyId,slug,connection!.account_id,connection!.livemode,fresh.total,expiresAt,now,now);
    const response = {attemptId};
    await run(
      tx,
      "INSERT INTO pms_booking_receipts(id,property_id,body_hash,reservation_id,created_at,response_json) VALUES(?,?,?,?,?,?)",
      key,
      s.property.id,
      digest,
      result.id,
      now,
      JSON.stringify(response),
    );
    return response;
  });
  if(!('attemptId' in created)||!created.attemptId)return created;
  return bookingCheckout(db,slug,created.attemptId,env,now);
}
type BookingPaymentRow={
 id:string;property_id:string;reservation_id:string;company_id:string;holding_slug:string;account_id:string;
 livemode:boolean;amount_minor:number;currency:string;checkout_id:string|null;checkout_url:string|null;
 status:'payment_pending'|'confirmed'|'expired'|'refund_review'|'cancelled';expires_at:number;
 refund_state:string|null;
};
const paymentToken=(id:string,env:BookingEnv)=>`${id}.${sign(env.secret,`payment:${id}`)}`;
async function bookingCheckout(db:TransactionalSqlDatabase,slug:string,id:string,env:BookingEnv,now=Date.now()){
 const row=await db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(id).first<BookingPaymentRow>();
 if(!row)throw new ApiError(404,'PMS_PAYMENT_NOT_FOUND','Booking payment not found');
 const token=paymentToken(id,env);
 if(row.status!=='payment_pending')return bookingPaymentStatus(db,token,env,now);
 if(Number(row.expires_at)<=now)return bookingPaymentStatus(db,token,env,now);
 let url=row.checkout_url;
 if(!row.checkout_id){
  const guest=await db.prepare("SELECT g.data_json::jsonb->>'email' email FROM pms_records r JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' WHERE r.id=? AND r.property_id=?").bind(row.reservation_id,row.property_id).first<{email:string}>();
  if(!guest?.email)throw new ApiError(409,'PMS_GUEST_EMAIL_REQUIRED','Guest email is required for booking payment');
  const success=`${env.origin}/book/${encodeURIComponent(slug)}/${encodeURIComponent(row.property_id)}?attempt=${encodeURIComponent(token)}`;
  const session=await stripeCall(Boolean(row.livemode),'checkout/sessions',{method:'POST',account:row.account_id,key:`pms-booking-${id}`,body:{mode:'payment',customer_email:guest.email,'payment_method_types[0]':'promptpay','line_items[0][price_data][currency]':'thb','line_items[0][price_data][unit_amount]':String(row.amount_minor),'line_items[0][price_data][product_data][name]':'Hotel reservation','line_items[0][quantity]':'1','metadata[pms_booking_payment_id]':id,'payment_intent_data[metadata][pms_booking_payment_id]':id,success_url:success,cancel_url:success,expires_at:String(Math.floor((Number(row.expires_at)+60000)/1000))}});
  if(typeof session.url!=='string'||new URL(session.url).hostname!=='checkout.stripe.com')throw new ApiError(502,'PMS_CHECKOUT_URL_INVALID','Stripe did not return a valid checkout URL');
  await run(db,"UPDATE pms_booking_payments SET checkout_id=?,checkout_url=?,updated_at=? WHERE id=? AND checkout_id IS NULL AND status='payment_pending'",session.id,session.url,Date.now(),id);
  url=session.url;
 }
 const s=await bookingSnapshot(db,row.property_id);
 return {...bookingSummary(s,row.reservation_id,now),state:'payment_pending',checkoutUrl:url,attemptToken:token,expiresAt:Number(row.expires_at)};
}
export async function bookingPaymentStatus(db:TransactionalSqlDatabase,token:string,env:BookingEnv,now=Date.now()){
 const [id,signature,extra]=token.split('.');
 if(extra||!id||!signature||!same(signature,sign(env.secret,`payment:${id}`)))throw new ApiError(401,'PMS_PAYMENT_TOKEN','Invalid booking payment token');
 await reconcileBookingPayment(db,id,env,now);
 const row=await db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(id).first<BookingPaymentRow>();
 if(!row)throw new ApiError(404,'PMS_PAYMENT_NOT_FOUND','Booking payment not found');
 const s=await bookingSnapshot(db,row.property_id);
 return {...bookingSummary(s,row.reservation_id,now),state:row.status,refundState:row.refund_state,checkoutUrl:row.status==='payment_pending'?row.checkout_url:null,attemptToken:token,expiresAt:Number(row.expires_at)};
}
export async function reconcileBookingPayment(db:TransactionalSqlDatabase,id:string,env:BookingEnv,now=Date.now()){
 const row=await db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(id).first<BookingPaymentRow>();
 if(!row||!['payment_pending','expired'].includes(row.status))return;
 let paid=false;
 if(row.checkout_id){
  let session=await stripeCall(Boolean(row.livemode),`checkout/sessions/${row.checkout_id}`,{account:row.account_id});
  if(session.payment_status!=='paid'&&now>=Number(row.expires_at)&&session.status==='open'){
   await stripeCall(Boolean(row.livemode),`checkout/sessions/${row.checkout_id}/expire`,{method:'POST',account:row.account_id,key:`pms-expire-${id}`,body:{}});
   session=await stripeCall(Boolean(row.livemode),`checkout/sessions/${row.checkout_id}`,{account:row.account_id});
  }
  if(session.payment_status==='paid'){
   const pi=await stripeCall(Boolean(row.livemode),`payment_intents/${String(session.payment_intent)}`,{account:row.account_id});
   if(pi.status!=='succeeded'||pi.amount_received!==Number(row.amount_minor)||pi.currency!=='thb'||(pi.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==id)throw new ApiError(409,'PMS_PAYMENT_MISMATCH','Payment needs Finance review');
   paid=true;
  }
 }
 if(!paid&&now<Number(row.expires_at))return;
 await db.transaction(async tx=>{
  const s=await bookingSnapshot(tx,row.property_id,undefined,true);
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const current=await tx.prepare('SELECT * FROM pms_booking_payments WHERE id=? FOR UPDATE').bind(id).first<BookingPaymentRow>();
  if(!current||!['payment_pending','expired'].includes(current.status))return;
  if(current.status==='expired'){
   if(paid)await run(tx,"UPDATE pms_booking_payments SET status='refund_review',refund_state='requested',refund_requested_at=?,updated_at=? WHERE id=?",now,now,id);
   return;
  }
  const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
  if(paid&&now<Number(current.expires_at)){
   const reservation=record(s,'reservation',current.reservation_id);
   if(reservation.data.state!=='payment_pending')throw new ApiError(409,'PMS_BOOKING_STATE','Booking state changed before payment confirmation');
   reservation.data.state='confirmed';reservation.data.history=[...(reservation.data.history??[]),{at:now,actor:'system:stripe',action:'Online payment confirmed'}];reservation.version++;reservation.updatedAt=now;
   const settings=await tx.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(current.company_id).first<{accounts_json:string}>();
   const clearing=settings?JSON.parse(settings.accounts_json).stripe as string:s.property.config.accounts.edc;
   if(!clearing)throw new ApiError(409,'PMS_STRIPE_CLEARING_REQUIRED','Configure a company Stripe clearing account');
   const oldEdc=s.property.config.accounts.edc;s.property.config.accounts.edc=clearing;
   for(const stay of records(s,'stay').filter(st=>st.data.reservationId===current.reservation_id)){
    const amount=stay.data.prices.reduce((sum,p)=>sum+p.total,0);
    const outcome=executePms(s,'payment.stripe',{folioId:stay.data.folioId,amount,reference:String(current.checkout_id)},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
    await persistPmsRecords(tx,s,before);
    await persistPmsEffects(tx,{companyId:current.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);
   }
   s.property.config.accounts.edc=oldEdc;
   await persist(tx,s,before);
   await run(tx,"UPDATE pms_booking_payments SET status='confirmed',paid_at=?,updated_at=? WHERE id=?",now,now,id);
   await queueMail(tx,s,current.reservation_id,current.holding_slug,'confirmation',env,now);
  }else{
   executePms(s,'reservation.cancel',{id:current.reservation_id,reason:paid?'Payment received after the booking hold expired':'Online payment period expired',fee:0},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
   await persist(tx,s,before);
   await run(tx,"UPDATE pms_booking_payments SET status=?,refund_state=?,refund_requested_at=?,updated_at=? WHERE id=?",paid?'refund_review':'expired',paid?'requested':null,paid?now:null,now,id);
  }
 });
}
export async function drainBookingPayments(db:TransactionalSqlDatabase,env:BookingEnv,now=Date.now()){
 const due=await rows<{id:string}>(db,"SELECT id FROM pms_booking_payments WHERE status='payment_pending' AND expires_at<=? ORDER BY expires_at LIMIT 20",now);
 for(const item of due){
  try{await reconcileBookingPayment(db,item.id,env,now);await run(db,'UPDATE pms_booking_payments SET last_error=NULL WHERE id=?',item.id);}
  catch(error){const code=error instanceof ApiError?error.code:error instanceof Error?error.message:'PMS_PAYMENT_REVIEW';await run(db,'UPDATE pms_booking_payments SET last_error=? WHERE id=?',String(code).slice(0,100),item.id);}
 }
 const refunds=await rows<{id:string}>(db,"SELECT id FROM pms_booking_payments WHERE refund_state IN ('approved','processing') ORDER BY refund_requested_at LIMIT 20");
 for(const item of refunds){
  try{await processBookingRefund(db,item.id,now);await run(db,'UPDATE pms_booking_payments SET last_error=NULL WHERE id=?',item.id);}
  catch(error){const code=error instanceof ApiError?error.code:error instanceof Error?error.message:'PMS_REFUND_REVIEW';await run(db,'UPDATE pms_booking_payments SET last_error=? WHERE id=?',String(code).slice(0,100),item.id);}
 }
}
export async function approveBookingRefund(db:TransactionalSqlDatabase,actor:WorkContext,id:string){
 requirePermission(actor,'finance.approve');
 if(actor.branchId||actor.departmentId)throw new ApiError(403,'PMS_COMPANY_FINANCE','Approve refunds from the company workspace');
 return db.transaction(async tx=>{
  const row=await tx.prepare('SELECT * FROM pms_booking_payments WHERE id=? AND company_id=? FOR UPDATE').bind(id,actor.companyId).first<BookingPaymentRow>();
  if(!row||row.refund_state!=='requested'||!['cancelled','refund_review'].includes(row.status))throw new ApiError(409,'PMS_REFUND_NOT_REQUESTED','No refund request is awaiting approval');
  await run(tx,"UPDATE pms_booking_payments SET refund_state='approved',refund_approved_by=?,updated_at=? WHERE id=?",actor.userId,Date.now(),id);
  return {id,state:'approved'};
 });
}
export async function bookingRefundsForFinance(actor:WorkContext){
 requirePermission(actor,'finance.read');
 if(actor.branchId||actor.departmentId)return [];
 const {write}=await holdingResources(actor.holdingId);
 return rows<{id:string;amount_minor:number;status:string;refund_state:string;last_error:string|null;created_at:number}>(write,"SELECT id,amount_minor,status,refund_state,last_error,created_at FROM pms_booking_payments WHERE company_id=? AND refund_state IS NOT NULL ORDER BY created_at DESC LIMIT 100",actor.companyId);
}
export async function processBookingRefund(db:TransactionalSqlDatabase,id:string,now=Date.now()){
 const row=await db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(id).first<BookingPaymentRow & {refund_id:string|null}>();
 if(!row||!['approved','processing'].includes(row.refund_state??''))return;
 if(!row.checkout_id)throw new ApiError(409,'PMS_REFUND_PROVIDER_MISSING','Checkout session is missing');
 let refundId=row.refund_id;
 if(!refundId){
  const session=await stripeCall(Boolean(row.livemode),`checkout/sessions/${row.checkout_id}`,{account:row.account_id});
  const pi=await stripeCall(Boolean(row.livemode),`payment_intents/${String(session.payment_intent)}`,{account:row.account_id});
  if(pi.status!=='succeeded'||pi.amount_received!==Number(row.amount_minor)||(pi.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==id)throw new ApiError(409,'PMS_REFUND_PAYMENT_MISMATCH','Payment needs provider review');
  const guest=await db.prepare("SELECT g.data_json::jsonb->>'email' email FROM pms_records r JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' WHERE r.id=?").bind(row.reservation_id).first<{email:string}>();
  if(!guest?.email)throw new ApiError(409,'PMS_REFUND_EMAIL_REQUIRED','Guest refund email is missing');
  const refund=await stripeCall(Boolean(row.livemode),'refunds',{method:'POST',account:row.account_id,key:`pms-refund-${id}`,body:{payment_intent:pi.id,amount:String(row.amount_minor),instructions_email:guest.email,'metadata[pms_booking_payment_id]':id}});
  refundId=refund.id;
  await run(db,"UPDATE pms_booking_payments SET refund_id=?,refund_state='processing',updated_at=? WHERE id=? AND refund_id IS NULL",refundId,now,id);
 }
 const refund=await stripeCall(Boolean(row.livemode),`refunds/${refundId}`,{account:row.account_id});
 if(refund.amount!==Number(row.amount_minor)||(refund.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==id)throw new ApiError(409,'PMS_REFUND_PROVIDER_MISMATCH','Refund amount or reference does not match');
 if(refund.status==='failed'||refund.status==='canceled'){await run(db,"UPDATE pms_booking_payments SET refund_state='failed',updated_at=? WHERE id=?",now,id);return;}
 if(refund.status!=='succeeded')return;
 await db.transaction(async tx=>{
  const s=await bookingSnapshot(tx,row.property_id,undefined,true);
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const current=await tx.prepare('SELECT * FROM pms_booking_payments WHERE id=? FOR UPDATE').bind(id).first<BookingPaymentRow>();
  if(current?.refund_state==='succeeded')return;
  if(!current||current.refund_state!=='processing')throw new ApiError(409,'PMS_REFUND_STATE','Refund state changed');
  if(current.status==='cancelled'){
   const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
   const settings=await tx.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(current.company_id).first<{accounts_json:string}>();
   const oldEdc=s.property.config.accounts.edc;
   s.property.config.accounts.edc=settings?JSON.parse(settings.accounts_json).stripe as string:oldEdc;
   for(const stay of records(s,'stay').filter(st=>st.data.reservationId===current.reservation_id)){
    const amount=stay.data.prices.reduce((sum,p)=>sum+p.total,0);
    const outcome=executePms(s,'payment.stripe_refund',{folioId:stay.data.folioId,amount,reference:String(refundId),reason:'Guest cancelled within the free-cancellation window'},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
    await persistPmsRecords(tx,s,before);
    await persistPmsEffects(tx,{companyId:current.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);
   }
   s.property.config.accounts.edc=oldEdc;
   await persist(tx,s,before);
  }
  await run(tx,"UPDATE pms_booking_payments SET refund_state='succeeded',updated_at=? WHERE id=?",now,id);
 });
}
async function authorizedBooking(
  db: SqlDatabase,
  token: string,
  now: number,
  lock = false,
) {
  if (token.length > 200)
    throw new ApiError(
      401,
      "PMS_LINK_EXPIRED",
      "Request a new booking access link",
    );
  const t = await db
    .prepare(
      "SELECT property_id,reservation_id FROM pms_booking_tokens WHERE token_hash=? AND expires_at>?",
    )
    .bind(hash(token), now)
    .first<{ property_id: string; reservation_id: string }>();
  if (!t)
    throw new ApiError(
      401,
      "PMS_LINK_EXPIRED",
      "Request a new booking access link",
    );
  return {
    s: await bookingSnapshot(db, t.property_id, undefined, lock),
    id: t.reservation_id,
  };
}
export async function manageBooking(
  db: TransactionalSqlDatabase,
  token: string,
  now = Date.now(),
) {
  return db.transaction(async (tx) => {
    const { s, id } = await authorizedBooking(tx, token, now);
    const payment=await tx.prepare('SELECT refund_state FROM pms_booking_payments WHERE reservation_id=?').bind(id).first<{refund_state:string|null}>();
    return {...bookingSummary(s,id,now),refundState:payment?.refund_state??null};
  });
}
export async function cancelPublicReservation(
  db: TransactionalSqlDatabase,
  slug: string,
  token: string,
  env: BookingEnv,
  now = Date.now(),
) {
  return db.transaction(async (tx) => {
    const { s, id } = await authorizedBooking(tx, token, now, true);
    const payment=await tx.prepare('SELECT * FROM pms_booking_payments WHERE reservation_id=? FOR UPDATE').bind(id).first<BookingPaymentRow>();
    if (record(s, "reservation", id).data.state === "cancelled")
      return {...bookingSummary(s,id,now),refundState:payment?.refund_state??null};
    const eligibility = cancellationEligibility(s, id, now);
    if (!eligibility.allowed)
      throw new ApiError(409, "PMS_CONTACT_HOTEL", eligibility.reason);
    const before = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
    executePms(
      s,
      "reservation.cancel",
      {
        id,
        reason: "Guest cancelled online before the free-cancellation deadline",
        fee: 0,
      },
      {
        userId: "guest:self-service",
        now,
        uuid: randomUUID,
        independent: () => false,
      },
    );
    await persist(tx, s, before);
    if(payment?.status==='confirmed')await run(tx,"UPDATE pms_booking_payments SET status='cancelled',refund_state='requested',refund_requested_at=?,updated_at=? WHERE id=?",now,now,payment.id);
    await queueMail(tx, s, id, slug, "cancellation", env, now);
    return {...bookingSummary(s,id,now),refundState:payment?.status==='confirmed'?'requested':null};
  });
}
export async function recoverBooking(
  db: TransactionalSqlDatabase,
  slug: string,
  raw: unknown,
  env: BookingEnv,
  now = Date.now(),
) {
  const v = z
    .object({
      number: z.string().trim().min(1).max(100),
      email: z.string().email().max(254),
    })
    .parse(raw);
  await db.transaction(async (tx) => {
    const matches = await rows<{ id: string; property_id: string }>(
      tx,
      "SELECT r.id,r.property_id FROM pms_records r JOIN pms_records g ON g.id=r.data_json::jsonb->>'guestId' AND g.property_id=r.property_id AND g.kind='guest' WHERE r.kind='reservation' AND r.data_json::jsonb->>'number'=? AND lower(g.data_json::jsonb->>'email')=lower(?) LIMIT 1",
      v.number,
      v.email,
    );
    if (matches[0]) {
      const s = await bookingSnapshot(tx, matches[0].property_id);
      await queueMail(tx, s, matches[0].id, slug, "access", env, now);
    }
  });
  return {
    message:
      "If these details match a booking, an access link will be emailed to you",
  };
}
export async function bookingRateLimit(
  db: SqlDatabase,
  key: string,
  limit = 30,
  now = Date.now(),
) {
  const bucket = Math.floor(now / 60000),
    row = await db
      .prepare(
        "INSERT INTO pms_booking_limits(key,bucket,count) VALUES(?,?,1) ON CONFLICT(key) DO UPDATE SET count=CASE WHEN pms_booking_limits.bucket=excluded.bucket THEN pms_booking_limits.count+1 ELSE 1 END,bucket=excluded.bucket RETURNING count",
      )
      .bind(hash(key), bucket)
      .first<{ count: number }>();
  if (Number(row?.count) > limit)
    throw new ApiError(
      429,
      "PMS_RATE_LIMIT",
      "Too many requests. Please wait a minute",
    );
}
export async function staffBookingSearch(
  actor: WorkContext,
  propertyId: string,
  operation: "availability" | "quote",
  input: unknown,
) {
  requirePermission(actor, "pms.reservations");
  const { write } = await holdingResources(actor.holdingId);
  if (
    actor.departmentId ||
    actor.beta ||
    !(await write
      .prepare(
        `SELECT id FROM pms_properties WHERE id=? AND company_id=?${actor.branchId ? " AND branch_id=?" : ""}`,
      )
      .bind(
        propertyId,
        actor.companyId,
        ...(actor.branchId ? [actor.branchId] : []),
      )
      .first())
  )
    throw new ApiError(
      404,
      "PMS_NOT_FOUND",
      "Hotel not found in this workspace",
    );
  if (operation === "availability") {
    const search = searchSchema.parse(input),
      s = await bookingSnapshot(write, propertyId, search);
    return bookingOffers(
      s,
      search,
      false,
      (input as { blockId?: string }).blockId,
    );
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
  return bookingQuote(
    await bookingSnapshot(write, propertyId, search),
    v.rooms,
    v.blockId,
  );
}
export function bookingError(error: unknown) {
  return error instanceof PmsError
    ? new ApiError(error.status, error.code, error.message)
    : error;
}
export async function deliverBookingEmails(
  db: TransactionalSqlDatabase,
  env: BookingEnv,
  send: typeof fetch = fetch,
  now = Date.now(),
) {
  if (process.env.NODE_ENV === 'production') return;
  if (!env.apiKey || !env.from) return;
  await run(
    db,
    "DELETE FROM pms_booking_limits WHERE bucket<?",
    Math.floor(now / 60000) - 60,
  );
  for (let i = 0; i < 10; i++) {
    const mail = await db
      .prepare(
        "UPDATE pms_booking_emails SET state='sending',attempts=attempts+1,lease_until=? WHERE id=(SELECT id FROM pms_booking_emails WHERE (state='pending' OR (state='sending' AND lease_until<?)) AND next_attempt_at<=? ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED) RETURNING *",
      )
      .bind(now + 120000, now, now)
      .first<{
        id: string;
        property_id: string;
        reservation_id: string;
        token_id: string;
        holding_slug: string;
        kind: string;
        attempts: number;
        payload_json: string;
      }>();
    if (!mail) break;
    try {
      // Keep the provider request body stable across retries, even if staff edit a guest.
      await run(
        db,
        "UPDATE pms_booking_tokens SET expires_at=? WHERE id=?",
        now + 7 * 86400000,
        mail.token_id,
      );
      const response = await send("https://api.resend.com/emails", {
        method: "POST",
        headers: {
          Authorization: `Bearer ${env.apiKey}`,
          "Content-Type": "application/json",
          "Idempotency-Key": `pms-booking-${mail.id}`,
        },
        body: unprotectMail(mail.payload_json, env, mail.id),
        signal: AbortSignal.timeout(10000),
      });
      if (!response.ok) throw new Error("EMAIL_DELIVERY_FAILED");
      await run(
        db,
        "UPDATE pms_booking_emails SET state='sent',sent_at=?,lease_until=NULL,last_error=NULL WHERE id=?",
        now,
        mail.id,
      );
    } catch {
      await run(
        db,
        "UPDATE pms_booking_emails SET state=?,next_attempt_at=?,lease_until=NULL,last_error='EMAIL_DELIVERY_FAILED' WHERE id=?",
        mail.attempts >= 8 ? "failed" : "pending",
        now + Math.min(3600000, 1000 * 2 ** mail.attempts),
        mail.id,
      );
    }
  }
}
