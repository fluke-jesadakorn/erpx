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
  collectionAmounts,
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
import {verifiedStripePaymentTime} from "./stripe-payment-time";
import { ApiError } from "./http";
import { snapshotMoney } from "./company-money";
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
  const approved=new Set((await rows<{id:string}>(db,"SELECT id FROM attachment_publications WHERE state='approved' AND id IN (SELECT jsonb_array_elements(config_json::jsonb->'booking'->'images')->>'publicationId' FROM pms_properties WHERE config_json::jsonb->'booking'->>'published'='true' UNION SELECT image->>'publicationId' FROM pms_properties CROSS JOIN LATERAL jsonb_array_elements(config_json::jsonb->'booking'->'roomContent') room CROSS JOIN LATERAL jsonb_array_elements(room->'images') image WHERE config_json::jsonb->'booking'->>'published'='true')")).map(r=>r.id));
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
      images: (b.images ?? []).filter(i=>approved.has(i.publicationId)),
      roomContent: b.roomContent.map(room=>({...room,images:(room.images??[]).filter(i=>approved.has(i.publicationId))})),
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
function onlineQuote(quote:ReturnType<typeof bookingQuote>){ return quote; }
function readQuote(token: string, env: BookingEnv, now: number) {
  const [payload, signature, extra] = token.split(".");
  if (extra || !payload || !signature || !same(signature, sign(env.secret, payload)))
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
export async function persist(
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
async function issueBookingToken(db:SqlDatabase,propertyId:string,reservationId:string,env:BookingEnv,now:number){
 const id=randomUUID(),token=tokenValue(id,env);
 await run(db,'INSERT INTO pms_booking_tokens(id,property_id,reservation_id,token_hash,expires_at,created_at) VALUES(?,?,?,?,?,?)',id,propertyId,reservationId,hash(token),now+7*86400000,now);
 return {id,token};
}
export async function queueMail(
  db: SqlDatabase,
  s: Snapshot,
  reservationId: string,
  slug: string,
  kind: "confirmation" | "cancellation" | "access" | "change" | "refund",
  env: BookingEnv,
  now: number,
) {
  const {id,token}=await issueBookingToken(db,s.property.id,reservationId,env,now);
  const b = bookingSummary(s, reservationId, now);
  const link = `${env.origin}/book/${encodeURIComponent(slug)}/manage#token=${encodeURIComponent(token)}`;
  const subject = `${b.hotel.name} · ${kind === "cancellation" ? "Cancellation" : kind === "access" ? "Booking access" : kind === "change" ? "Booking updated" : kind === "refund" ? "Refund confirmed" : "Booking confirmation"} ${b.number}`;
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
  return token;
}
export function bookingSummary(s: Snapshot, id: string, now: number) {
  const r = record(s, "reservation", id),
    b = s.property.config.booking,
    guest = record(s, "guest", r.data.guestId).data;
  const folioIds=new Set(records(s,'folio').filter(f=>f.data.reservationId===id).map(f=>f.id)),entries=records(s,'entry'),byId=new Map(entries.map(e=>[e.id,e]));
  const postedRoomTotal=entries.filter(e=>folioIds.has(e.data.folioId)).reduce((n,e)=>{let current:typeof e|undefined=e;const seen=new Set<string>();while(current&&!seen.has(current.id)){seen.add(current.id);if(current.data.sourceKey.startsWith('night:'))return n+e.data.total;current=byId.get(current.data.originalId);}return n;},0);
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
    paymentState: records(s,"entry").some(e=>e.data.sourceKey?.startsWith("online-payment:")&&records(s,"folio").some(f=>f.id===e.data.folioId&&f.data.reservationId===id)) ? "received" as const : "pay_at_hotel" as const,
    currency: s.property.config.currency,
    version: r.version,
    history: r.data.history ?? [],
    total: records(s,"stay").filter(st=>st.data.reservationId===id && !["cancelled","no_show"].includes(st.data.state)).reduce((n,st)=>n+st.data.prices.reduce((sum,p)=>sum+p.total,0),0),
    advance: records(s,"folio").filter(f=>f.data.reservationId===id).reduce((n,f)=>n+f.data.advance,0),
    folioDue:records(s,"folio").filter(f=>f.data.reservationId===id).reduce((n,f)=>n+f.data.due,0),
    policy: r.data.bookingPolicy,
    rooms: records(s, "stay")
      .filter((st) => st.data.reservationId === id)
      .map((st) => ({
        id: st.id,
        typeId: st.data.typeId,
        rateId: st.data.rateId,
        folioId: st.data.folioId,
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
  const digest = hash(JSON.stringify(v));

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
    const stripeBooking = fresh.dueNow > 0;
    if(fresh.currency!=='THB'||fresh.total<=0)throw new ApiError(422,'PMS_STRIPE_CURRENCY','Booking requires a positive THB quote');
    const connection = stripeBooking ? await tx.prepare("SELECT * FROM pos_connections WHERE company_id=? AND state='ready' AND charges_enabled=true AND payouts_enabled=true").bind(s.property.companyId).first<{account_id:string;livemode:boolean;card_enabled:boolean;promptpay_enabled:boolean}>() : null;
    if (stripeBooking && (!connection?.account_id || connection.livemode && process.env.PMS_STRIPE_LIVE_ENABLED !== 'true' || !connection.card_enabled && !connection.promptpay_enabled)) throw new ApiError(503,'PMS_PAYMENT_UNAVAILABLE','Online booking payment is unavailable for this company');
    if (stripeBooking && !s.property.config.onlineAccounting?.approvedBy) throw new ApiError(409,'PMS_ONLINE_ACCOUNTING_REQUIRED','Finance must approve refund and unallocated receipt accounts before online payment');
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
      const managementToken=await queueMail(tx,s,result.id,slug,'confirmation',env,now);
      const response={...bookingSummary(s,result.id,now),managementToken};
      await run(tx,"INSERT INTO pms_booking_receipts(id,property_id,body_hash,reservation_id,created_at,response_json) VALUES(?,?,?,?,?,?)",key,s.property.id,digest,result.id,now,JSON.stringify(response));
      return response;
    }
    const attemptId=randomUUID(),expiresAt=now+30*60*1000;
    const allocation = records(s,'stay').filter(st=>st.data.reservationId===result.id).sort((a,b)=>a.id.localeCompare(b.id)).map(st=>({stayId:st.id,folioId:st.data.folioId,amount:collectionAmounts(st.data.prices.reduce((sum,n)=>sum+n.total,0),st.data.bookingPolicy?.collection).dueNow})).filter(a=>a.amount>0);
    const settings=await tx.prepare('SELECT accounts_json FROM pos_settings WHERE company_id=?').bind(s.property.companyId).first<{accounts_json:string}>();
    const clearing=settings?(JSON.parse(settings.accounts_json).stripe||s.property.config.accounts.edc):s.property.config.accounts.edc;
    const methods = [...(connection!.card_enabled ? ['card'] : []), ...(connection!.promptpay_enabled ? ['promptpay'] : [])];
    await run(tx,"INSERT INTO pms_booking_payments(id,property_id,reservation_id,company_id,holding_slug,account_id,livemode,amount_minor,currency,status,expires_at,created_at,updated_at,allocation_json,methods_json,clearing_account) VALUES(?,?,?,?,?,?,?,?,'THB','payment_pending',?,?,?,?,?,?)",attemptId,s.property.id,result.id,s.property.companyId,slug,connection!.account_id,connection!.livemode,fresh.dueNow,expiresAt,now,now,JSON.stringify(allocation),JSON.stringify(methods),clearing);
    const money=await snapshotMoney(tx,{companyId:s.property.companyId},{sourceType:'pms_booking_payment',sourceId:attemptId,amountKind:'requested',transactionDate:s.property.businessDate,currency:'THB',amountUnits:String(fresh.dueNow)});
    await money.statement.run();
    const {token:managementToken}=await issueBookingToken(tx,s.property.id,result.id,env,now);
    const response = {attemptId,managementToken};
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
  return {...await bookingCheckout(db,slug,created.attemptId,env,now),...('managementToken' in created?{managementToken:created.managementToken}:{})};
}
export type BookingPaymentRow={
 id:string;property_id:string;reservation_id:string;company_id:string;holding_slug:string;account_id:string;
 livemode:boolean;amount_minor:number;currency:string;checkout_id:string|null;checkout_url:string|null;
 created_at:number;status:'payment_pending'|'confirmed'|'expired'|'refund_review'|'cancelled';expires_at:number;
 refund_state:string|null;allocation_json:string;methods_json:string;clearing_account:string;payment_intent_id:string|null;provider_succeeded_at:number|null;purpose:"reservation"|"change";proposal_id:string|null;
};
const paymentToken=(id:string,env:BookingEnv)=>`${id}.${sign(env.secret,`payment:${id}`)}`;
export async function bookingCheckout(db:TransactionalSqlDatabase,slug:string,id:string,env:BookingEnv,now=Date.now()){
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
  const session=await stripeCall(Boolean(row.livemode),'checkout/sessions',{method:'POST',account:row.account_id,key:`pms-booking-${id}`,body:{mode:'payment',customer_email:guest.email,...Object.fromEntries((JSON.parse(row.methods_json) as string[]).map((method,index)=>[`payment_method_types[${index}]`,method])),'line_items[0][price_data][currency]':'thb','line_items[0][price_data][unit_amount]':String(row.amount_minor),'line_items[0][price_data][product_data][name]':'Hotel reservation','line_items[0][quantity]':'1','metadata[pms_booking_payment_id]':id,'payment_intent_data[metadata][pms_booking_payment_id]':id,success_url:success,cancel_url:success,expires_at:String(Math.floor((Number(row.expires_at)+60000)/1000))}});
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
 const receipt=await db.prepare("SELECT response_json FROM pms_booking_receipts WHERE reservation_id=? AND response_json::jsonb->>'attemptId'=? LIMIT 1").bind(row.reservation_id,row.id).first<{response_json:string}>();
 return {...bookingSummary(s,row.reservation_id,now),state:row.status,refundState:row.refund_state,checkoutUrl:row.status==='payment_pending'?row.checkout_url:null,attemptToken:token,expiresAt:Number(row.expires_at),managementToken:receipt?JSON.parse(receipt.response_json).managementToken:undefined};
}
export async function reconcileBookingPayment(db:TransactionalSqlDatabase,id:string,env:BookingEnv,now=Date.now()){
 const row=await db.prepare('SELECT * FROM pms_booking_payments WHERE id=?').bind(id).first<BookingPaymentRow>();
 if(!row||!['payment_pending','expired'].includes(row.status))return;
 let paid=false; let providerSucceededAt:number|null=null; let paymentIntentId:string|null=null;
 if(row.checkout_id){
  let session=await stripeCall(Boolean(row.livemode),`checkout/sessions/${row.checkout_id}`,{account:row.account_id});
  if(session.payment_status!=='paid'&&now>=Number(row.expires_at)&&session.status==='open'){
   await stripeCall(Boolean(row.livemode),`checkout/sessions/${row.checkout_id}/expire`,{method:'POST',account:row.account_id,key:`pms-expire-${id}`,body:{}});
   session=await stripeCall(Boolean(row.livemode),`checkout/sessions/${row.checkout_id}`,{account:row.account_id});
  }
  if(session.payment_status==='paid'){
   const pi=await stripeCall(Boolean(row.livemode),`payment_intents/${String(session.payment_intent)}`,{account:row.account_id});
   if(pi.status!=='succeeded'||pi.amount_received!==Number(row.amount_minor)||pi.currency!=='thb'||(pi.metadata as Record<string,string>|undefined)?.pms_booking_payment_id!==id)throw new ApiError(409,'PMS_PAYMENT_MISMATCH','Payment needs Finance review');
   paymentIntentId=String(pi.id);
   // Retain a verified receipt observation even if event-time lookup or local accounting fails.
   await run(db,'UPDATE pms_booking_payments SET payment_intent_id=?,paid_at=COALESCE(paid_at,?) WHERE id=?',paymentIntentId,now,id);
   providerSucceededAt=await verifiedStripePaymentTime({livemode:Boolean(row.livemode),accountId:row.account_id,paymentIntentId:String(pi.id),known:row.provider_succeeded_at?Number(row.provider_succeeded_at):null,createdAt:Number(row.created_at),now});
   paid=true;
  }
 }
 if(!paid&&now<Number(row.expires_at))return;
 if(paid)await run(db,'UPDATE pms_booking_payments SET payment_intent_id=?,provider_succeeded_at=COALESCE(provider_succeeded_at,?),paid_at=? WHERE id=?',paymentIntentId,providerSucceededAt,providerSucceededAt??now,id);
 await db.transaction(async tx=>{
  const s=await bookingSnapshot(tx,row.property_id,undefined,true);
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const current=await tx.prepare('SELECT * FROM pms_booking_payments WHERE id=? FOR UPDATE').bind(id).first<BookingPaymentRow>();
  if(!current||!['payment_pending','expired'].includes(current.status))return;
  const before=new Map(s.records.map(r=>[r.id,JSON.stringify(r)]));
  if (paid) await run(tx,'UPDATE pms_booking_payments SET payment_intent_id=?,provider_succeeded_at=?,paid_at=? WHERE id=?',paymentIntentId,providerSucceededAt,providerSucceededAt??now,id);
  if(current.purpose==='change') {
    await (await import('./pms-booking-changes')).settleBookingChange(tx,s,current,{paid,providerSucceededAt,now,env});
    return;
  }
  const reservation=record(s,'reservation',current.reservation_id);
  const eligible=paid && (providerSucceededAt??now)<Number(current.expires_at);
  let canAllocate=eligible;
  if (canAllocate && reservation.data.state==='cancelled' && current.status==='expired') {
    const trial=structuredClone(s);
    for (const st of records(trial,'stay').filter(st=>st.data.reservationId===current.reservation_id)) st.data.state='reserved';
    const {availability}=await import('@/lib/pms-inventory');
    canAllocate=records(trial,'stay').filter(st=>st.data.reservationId===current.reservation_id).every(st=>st.data.prices.every(n=>availability(trial,st.data.typeId,n.date).available>=0));
    if(canAllocate) for(const st of records(s,'stay').filter(st=>st.data.reservationId===current.reservation_id)){st.data.state='reserved';st.version++;st.updatedAt=now;}
  } else canAllocate=canAllocate && reservation.data.state==='payment_pending';
  const oldEdc=s.property.config.accounts.edc; s.property.config.accounts.edc=current.clearing_account;
  if(canAllocate){
   const {availability}=await import('@/lib/pms-inventory');
   canAllocate=!records(s,'stay').filter(st=>st.data.reservationId===current.reservation_id).some(st=>st.data.prices.some(n=>availability(s,st.data.typeId,n.date).available<0));
  }
  if(canAllocate){
   reservation.data.state='confirmed';reservation.data.history=[...(reservation.data.history??[]),{at:now,actor:'system:stripe',action:'Online payment confirmed'}];reservation.version++;reservation.updatedAt=now;
   let allocated=0;
   for(const allocation of JSON.parse(current.allocation_json) as PaymentAllocation[]){
    const outcome=executePms(s,'payment.stripe',{folioId:allocation.folioId,amount:allocation.amount,reference:String(paymentIntentId)},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
    record(s,'entry',outcome.id).data.sourceKey=`online-payment:${current.id}`;
    record(s,'entry',outcome.id).data.providerPaymentOffset=allocated; allocated+=allocation.amount;
    await persistPmsRecords(tx,s,before); await persistPmsEffects(tx,{companyId:current.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);
    await run(tx,'INSERT INTO pms_booking_allocations(id,payment_id,stay_id,folio_id,entry_id,amount_minor,created_at) VALUES(?,?,?,?,?,?,?)',randomUUID(),id,allocation.stayId,allocation.folioId,outcome.id,allocation.amount,now);
   }
   if(allocated!==Number(current.amount_minor))throw new ApiError(409,'PMS_PAYMENT_ALLOCATION_CHANGED','Receipt allocations must equal the original payment');
   s.property.config.accounts.edc=oldEdc;await persist(tx,s,before);
   await run(tx,"UPDATE pms_booking_payments SET status='confirmed',updated_at=? WHERE id=?",now,id);
   await queueMail(tx,s,current.reservation_id,current.holding_slug,'confirmation',env,now);
  }else{
   if(reservation.data.state==='payment_pending')executePms(s,'reservation.cancel',{id:current.reservation_id,reason:paid?'Online payment could not be assigned':'Online payment period expired',fee:0},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
   if(paid){
    const first=records(s,'stay').find(st=>st.data.reservationId===current.reservation_id)!;
    const outcome=executePms(s,'payment.unallocated',{folioId:first.data.folioId,amount:Number(current.amount_minor),reference:String(paymentIntentId)},{userId:'system:stripe',now,uuid:randomUUID,independent:()=>false});
    record(s,'entry',outcome.id).data.sourceKey=`online-payment:${current.id}`;
    await persistPmsRecords(tx,s,before);await persistPmsEffects(tx,{companyId:current.company_id,userId:'system:stripe'} as WorkContext,s,outcome.effects);
    await run(tx,"INSERT INTO pms_booking_refund_requests(id,payment_id,amount_minor,allocations_json,reason,requested_by,state,created_at,updated_at) VALUES(?,?,?,'[]','Payment cannot be assigned','system:stripe','requested',?,?)",randomUUID(),id,current.amount_minor,now,now);
   }
   s.property.config.accounts.edc=oldEdc;await persist(tx,s,before);
   await run(tx,"UPDATE pms_booking_payments SET status=?,refund_state=?,refund_requested_at=?,updated_at=? WHERE id=?",paid?'refund_review':'expired',paid?'requested':null,paid?now:null,now,id);
  }
 });
}
export async function drainBookingPayments(db:TransactionalSqlDatabase,env:BookingEnv,now=Date.now()){
 const due=await rows<{id:string}>(db,"SELECT id FROM pms_booking_payments WHERE status='payment_pending' OR (status='expired' AND checkout_id IS NOT NULL AND updated_at<? AND expires_at>?) ORDER BY updated_at LIMIT 20",now-300000,now-30*86400000);
 for(const item of due){
  try{await reconcileBookingPayment(db,item.id,env,now);await run(db,'UPDATE pms_booking_payments SET last_error=NULL,updated_at=? WHERE id=?',now,item.id);}
  catch(error){const code=error instanceof ApiError?error.code:error instanceof Error?error.message:'PMS_PAYMENT_REVIEW';await run(db,'UPDATE pms_booking_payments SET last_error=?,updated_at=? WHERE id=?',String(code).slice(0,100),now,item.id);await run(db,"UPDATE pms_booking_refund_requests SET last_error=?,updated_at=? WHERE payment_id=? AND state IN ('approved','processing','requires_action')",String(code).slice(0,100),now,item.id);}
 }
 const refunds=await rows<{id:string}>(db,"SELECT id FROM pms_booking_payments WHERE id IN (SELECT payment_id FROM pms_booking_refund_requests WHERE state IN ('approved','processing','requires_action')) ORDER BY updated_at LIMIT 20");
 for(const item of refunds){
  try{await processBookingRefund(db,item.id,now);await run(db,'UPDATE pms_booking_payments SET last_error=NULL,updated_at=? WHERE id=?',now,item.id);}
  catch(error){const code=error instanceof ApiError?error.code:error instanceof Error?error.message:'PMS_REFUND_REVIEW';await run(db,'UPDATE pms_booking_payments SET last_error=?,updated_at=? WHERE id=?',String(code).slice(0,100),now,item.id);await run(db,"UPDATE pms_booking_refund_requests SET last_error=?,updated_at=? WHERE payment_id=? AND state IN ('approved','processing','requires_action')",String(code).slice(0,100),now,item.id);}
 }
}
export { approveBookingRefund, bookingRefundsForFinance, processBookingRefund, requestBookingRefund } from './pms-booking-refunds';
import { processBookingRefund } from './pms-booking-refunds';
export type PaymentAllocation = {stayId:string;folioId:string;amount:number};
export async function authorizedBooking(
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
    const refunds=(await tx.prepare('SELECT r.id,r.amount_minor,r.state,r.reason FROM pms_booking_refund_requests r JOIN pms_booking_payments p ON p.id=r.payment_id WHERE p.reservation_id=? ORDER BY r.created_at DESC').bind(id).all<{id:string;amount_minor:number;state:string;reason:string}>()).results;
    const documents=(await tx.prepare("SELECT id,kind,created_at FROM pms_documents WHERE property_id=? AND folio_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='folio' AND data_json::jsonb->>'reservationId'=?) ORDER BY created_at DESC LIMIT 100").bind(s.property.id,s.property.id,id).all<{id:string;kind:string;created_at:number}>()).results;
    const pending=await tx.prepare("SELECT id,expires_at FROM pms_booking_payments WHERE reservation_id=? AND purpose='change' AND status='payment_pending' ORDER BY created_at DESC LIMIT 1").bind(id).first<{id:string;expires_at:number}>();
    const summary=bookingSummary(s,id,now);
    return {...summary,editableRates:records(s,"rate").filter(r=>r.data.active&&r.data.market==="public"&&s.property.config.booking?.rateIds.includes(r.id)).map(r=>({id:r.id,typeId:r.data.typeId,name:r.data.name,roomName:record(s,"room_type",r.data.typeId).data.name})),refundState:refunds[0]?.state??null,refunds,documents,dueAtHotel:Math.max(0,summary.total-summary.postedRoomTotal+summary.folioDue-summary.advance),pendingChange:pending?{attemptToken:paymentToken(pending.id,await bookingEnv()),expiresAt:Number(pending.expires_at)}:null};
  });
}
export async function cancelPublicReservation(db:TransactionalSqlDatabase,slug:string,token:string,env:BookingEnv,now=Date.now()){
 const current=await manageBooking(db,token,now);
 if(current.state==='cancelled')return current;
 const changes=await import('./pms-booking-changes');
 const quote=await changes.publicChangeQuote(db,token,'cancel',{items:current.rooms.filter(st=>st.state==='reserved').map(st=>({stayId:st.id}))},env,now);
 await changes.confirmBookingChange(db,slug,token,quote.token,randomUUID(),env,now);
 return manageBooking(db,token,now);
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
