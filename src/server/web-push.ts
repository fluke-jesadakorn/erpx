import "server-only";
import { createCipheriv, createDecipheriv, createECDH, createHash, randomBytes, randomUUID } from "node:crypto";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";

type PushConfig = { publicKey: string; privateKey: string; subject: string; dataKey: Buffer };
type Subscription = { endpoint: string; keys: { p256dh: string; auth: string } };
type Preference = {
  enabled: boolean; work_enabled: boolean; mention_enabled: boolean; system_enabled: boolean;
  calendar_enabled: boolean; notice_enabled: boolean;
  quiet_start_minute: number | null; quiet_end_minute: number | null;
};
type StoredSubscription = { id: string; company_id: string; user_id: string; session_id: string;
  origin: string; inbox_path: string; sealed_subscription: string; cursor_at: number; cursor_id: string };
type DeliveryJob = { id: string; subscription_id: string; inbox_delivery_id: string;
  company_id: string; user_id: string; session_id: string; origin: string; inbox_path: string;
  sealed_subscription: string; attempts: number; lease_token: string;
  quiet_start_minute: number | null; quiet_end_minute: number | null };

const b64 = /^[A-Za-z0-9_-]+$/;
export function pushConfig(): PushConfig | null {
  const { ERPX_PUSH_VAPID_PUBLIC_KEY: publicKey, ERPX_PUSH_VAPID_PRIVATE_KEY: privateKey,
    ERPX_PUSH_VAPID_SUBJECT: subject, ERPX_PUSH_DATA_KEY: sealedKey } = process.env;
  if (!publicKey || !privateKey || !subject || !sealedKey) return null;
  const dataKey = Buffer.from(sealedKey, "base64url");
  if (!b64.test(publicKey) || Buffer.from(publicKey, "base64url").length !== 65 ||
      !b64.test(privateKey) || Buffer.from(privateKey, "base64url").length !== 32 ||
      !b64.test(sealedKey) || dataKey.length !== 32 ||
      !(subject.startsWith("mailto:") || subject.startsWith("https://"))) return null;
  try {
    const pair=createECDH("prime256v1");pair.setPrivateKey(Buffer.from(privateKey,"base64url"));
    if(pair.getPublicKey(undefined,"uncompressed").toString("base64url")!==publicKey)return null;
  }catch{return null;}
  return { publicKey, privateKey, subject, dataKey };
}

const subscriptionInput = z.object({
  endpoint: z.string().url().max(2048),
  keys: z.object({ p256dh: z.string().regex(b64).max(160), auth: z.string().regex(b64).max(80) }),
});
export function validatePushSubscription(raw: unknown): Subscription {
  const input = subscriptionInput.parse(raw);
  const url = new URL(input.endpoint);
  const host = url.hostname.toLowerCase();
  const knownService = host === "fcm.googleapis.com" ||
    host === "updates.push.services.mozilla.com" ||
    host === "web.push.apple.com" ||
    host === "wns2-pn1p.notify.windows.com" ||
    host.endsWith(".notify.windows.com");
  if (url.protocol !== "https:" || url.username || url.password || url.port || !knownService ||
      Buffer.from(input.keys.p256dh, "base64url").length !== 65 ||
      Buffer.from(input.keys.auth, "base64url").length !== 16)
    throw new ApiError(422, "INVALID_PUSH_SUBSCRIPTION", "This browser's push service is not supported.");
  return input;
}
const hashEndpoint = (endpoint: string) => createHash("sha256").update(endpoint).digest("hex");
function seal(config: PushConfig, value: Subscription) {
  const iv = randomBytes(12), cipher = createCipheriv("aes-256-gcm", config.dataKey, iv);
  const body = Buffer.concat([cipher.update(JSON.stringify(value), "utf8"), cipher.final()]);
  return Buffer.concat([iv, cipher.getAuthTag(), body]).toString("base64url");
}
function open(config: PushConfig, value: string): Subscription {
  const bytes = Buffer.from(value, "base64url");
  const decipher = createDecipheriv("aes-256-gcm", config.dataKey, bytes.subarray(0,12));
  decipher.setAuthTag(bytes.subarray(12,28));
  return validatePushSubscription(JSON.parse(Buffer.concat([decipher.update(bytes.subarray(28)),decipher.final()]).toString("utf8")));
}
function sessionId(actor: WorkContext) {
  if (!actor.authority?.sessionId) throw new ApiError(403,"PUSH_CONTEXT_REQUIRED","Choose a work role before enabling push.");
  return actor.authority.sessionId;
}
export function allowedPushOrigin(origin: string) {
  const configured = [process.env.ERPX_PUBLIC_ORIGIN, process.env.ERPX_FRONT_PMS_ORIGIN,
    process.env.ERPX_FRONT_POS_ORIGIN].filter(Boolean);
  if (configured.includes(origin)) return true;
  if (process.env.NODE_ENV !== "production") return /^http:\/\/(localhost|127\.0\.0\.1)(:3100)?$/.test(origin);
  return false;
}
export async function pushSettings(db: SqlDatabase, actor: WorkContext) {
  const config = pushConfig();
  if (!config) return { available: false, publicKey: null, preferences: null, deviceCount: 0 };
  const preferences = await db.prepare("SELECT * FROM push_preferences WHERE company_id=? AND user_id=?")
    .bind(actor.companyId,actor.userId).first<Preference>();
  const count = await db.prepare("SELECT count(*)::int n FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?")
    .bind(actor.companyId,actor.userId,sessionId(actor)).first<{n:number}>();
  return { available: true, publicKey: config.publicKey, preferences: preferences ?? defaultPreferences,
    deviceCount: count?.n ?? 0 };
}
export const defaultPreferences: Preference = { enabled:false, work_enabled:true, mention_enabled:true,
  system_enabled:true, calendar_enabled:false, notice_enabled:false,
  quiet_start_minute:null, quiet_end_minute:null };

export async function registerPush(db: SqlDatabase, actor: WorkContext, origin: string,
  inboxPath: string, raw: unknown) {
  const config = pushConfig();
  if (!config) throw new ApiError(503,"PUSH_DISABLED","Push is not configured. Inbox remains available.");
  if (!allowedPushOrigin(origin)) throw new ApiError(403,"PUSH_ORIGIN_FORBIDDEN","Use this workspace's own origin.");
  const input = validatePushSubscription(raw), now=Date.now(), id=randomUUID();
  const endpointHash=hashEndpoint(input.endpoint);
  const sid=sessionId(actor);
  // A browser's single endpoint cannot keep queued work for a previous owner.
  const previous=await db.prepare("SELECT id,user_id,session_id FROM push_subscriptions WHERE company_id=? AND endpoint_hash=?")
    .bind(actor.companyId,endpointHash).first<{id:string;user_id:string;session_id:string}>();
  if(previous&&(previous.user_id!==actor.userId||previous.session_id!==sid)){
    await db.prepare("DELETE FROM push_delivery_jobs WHERE subscription_id=?").bind(previous.id).run();
    await db.prepare("DELETE FROM push_subscriptions WHERE id=?").bind(previous.id).run();
    if(previous.user_id!==actor.userId)await db.prepare(`UPDATE push_preferences SET enabled=false,updated_at=?
      WHERE company_id=? AND user_id=? AND NOT EXISTS(SELECT 1 FROM push_subscriptions WHERE company_id=? AND user_id=?)`)
      .bind(now,actor.companyId,previous.user_id,actor.companyId,previous.user_id).run();
  }
  await db.prepare(`INSERT INTO push_preferences(company_id,user_id,enabled,updated_at) VALUES(?,?,true,?)
    ON CONFLICT(company_id,user_id) DO UPDATE SET enabled=true,updated_at=excluded.updated_at`)
    .bind(actor.companyId,actor.userId,now).run();
  await db.prepare(`INSERT INTO push_subscriptions
    (id,company_id,user_id,session_id,origin,inbox_path,endpoint_hash,sealed_subscription,created_at,updated_at,scan_at,cursor_at,cursor_id)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,endpoint_hash) DO UPDATE SET
    user_id=excluded.user_id,session_id=excluded.session_id,origin=excluded.origin,
    inbox_path=excluded.inbox_path,sealed_subscription=excluded.sealed_subscription,
    updated_at=excluded.updated_at,
    scan_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.scan_at ELSE excluded.scan_at END,
    cursor_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.cursor_at ELSE excluded.cursor_at END,
    cursor_id=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.cursor_id ELSE '' END,
    created_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.created_at ELSE excluded.created_at END`)
    .bind(id,actor.companyId,actor.userId,sid,origin,inboxPath,endpointHash,
      seal(config,input),now,now,now,now,"").run();
  return { enabled:true };
}

const preferenceInput = z.object({
  work: z.boolean(), mention: z.boolean(), system: z.boolean(), calendar: z.boolean(), notice: z.boolean(),
  quietStartMinute: z.number().int().min(0).max(1439).nullable(),
  quietEndMinute: z.number().int().min(0).max(1439).nullable(),
}).strict().refine(v => (v.quietStartMinute === null) === (v.quietEndMinute === null) &&
  (v.quietStartMinute === null || v.quietStartMinute !== v.quietEndMinute),"Choose different start and end times.");
export async function savePushPreferences(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const v=preferenceInput.parse(raw),now=Date.now();
  await db.prepare(`INSERT INTO push_preferences
    (company_id,user_id,enabled,work_enabled,mention_enabled,system_enabled,calendar_enabled,notice_enabled,quiet_start_minute,quiet_end_minute,updated_at)
    VALUES(?,?,false,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET
    work_enabled=excluded.work_enabled,mention_enabled=excluded.mention_enabled,
    system_enabled=excluded.system_enabled,calendar_enabled=excluded.calendar_enabled,
    notice_enabled=excluded.notice_enabled,quiet_start_minute=excluded.quiet_start_minute,
    quiet_end_minute=excluded.quiet_end_minute,updated_at=excluded.updated_at`)
    .bind(actor.companyId,actor.userId,v.work,v.mention,v.system,v.calendar,v.notice,
      v.quietStartMinute,v.quietEndMinute,now).run();
  return { saved:true };
}
export async function removePushDevice(db: SqlDatabase, actor: WorkContext, endpoint?: string) {
  if(!pushConfig())return {removed:true};
  const sid=sessionId(actor);
  if (endpoint) await db.prepare("DELETE FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=? AND endpoint_hash=?")
    .bind(actor.companyId,actor.userId,sid,hashEndpoint(endpoint)).run();
  else await db.prepare("DELETE FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?")
    .bind(actor.companyId,actor.userId,sid).run();
  await db.prepare(`UPDATE push_preferences SET enabled=false,updated_at=? WHERE company_id=? AND user_id=?
    AND NOT EXISTS(SELECT 1 FROM push_subscriptions WHERE company_id=? AND user_id=?)`)
    .bind(Date.now(),actor.companyId,actor.userId,actor.companyId,actor.userId).run();
  return { removed:true };
}

/** Account-level workspace switches can revoke their own session without a selected work role. */
export async function removePushSession(db:SqlDatabase,userId:string,sid:string) {
  const table=await db.prepare("SELECT to_regclass('push_subscriptions') AS name").first<{name:string|null}>();
  if(!table?.name)return {removed:true};
  const removed=(await db.prepare("DELETE FROM push_subscriptions WHERE user_id=? AND session_id=? RETURNING company_id")
    .bind(userId,sid).all<{company_id:string}>()).results;
  for(const companyId of new Set(removed.map(row=>row.company_id)))
    await db.prepare(`UPDATE push_preferences SET enabled=false,updated_at=? WHERE company_id=? AND user_id=?
      AND NOT EXISTS(SELECT 1 FROM push_subscriptions WHERE company_id=? AND user_id=?)`)
      .bind(Date.now(),companyId,userId,companyId,userId).run();
  return {removed:true};
}

function categoryEnabled(preference: Preference, category: string) {
  return ({work:preference.work_enabled,mention:preference.mention_enabled,
    system:preference.system_enabled,calendar:preference.calendar_enabled,
    notice:preference.notice_enabled} as Record<string,boolean>)[category] ?? false;
}
/** Bangkok has no DST; a value of zero means no quiet period is configured. */
export function quietDelayMs(now:number,start:number|null,end:number|null) {
  if(start===null||end===null)return 0;
  const local=new Date(now+7*60*60_000),minute=local.getUTCHours()*60+local.getUTCMinutes();
  const inside=start<end?minute>=start&&minute<end:minute>=start||minute<end;
  if(!inside)return 0;
  const remaining=(end-minute+1440)%1440;
  return remaining*60_000-local.getUTCSeconds()*1000-local.getUTCMilliseconds();
}
async function currentSession(control:SqlDatabase,db:SqlDatabase,
  s:Pick<StoredSubscription,"session_id"|"user_id"|"company_id">,now:number) {
  const session=await control.prepare('SELECT 1 FROM session WHERE id=? AND "userId"=? AND "expiresAt">?')
    .bind(s.session_id,s.user_id,new Date(now).toISOString()).first();
  if(!session)return false;
  return Boolean(await db.prepare(`SELECT 1 FROM active_work_contexts c JOIN assignments a
    ON a.id=c.assignment_id AND a.company_id=c.company_id AND a.user_id=c.user_id
    WHERE c.session_id=? AND c.company_id=? AND c.user_id=? AND c.expires_at>?
    AND a.version=c.assignment_version AND a.revoked_at IS NULL
    AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
    AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.session_id=c.session_id
      AND r.company_id=c.company_id AND r.user_id=c.user_id) LIMIT 1`)
    .bind(s.session_id,s.company_id,s.user_id,now,now,now).first());
}
async function fanout(db:SqlDatabase,control:SqlDatabase,now:number) {
  let queued=0;
  const subscriptions=(await db.prepare(`SELECT s.*,p.* FROM push_subscriptions s JOIN push_preferences p
    ON p.company_id=s.company_id AND p.user_id=s.user_id WHERE p.enabled=true
    ORDER BY s.scan_at,s.id LIMIT 25`).all<StoredSubscription & Preference>()).results;
  for(const s of subscriptions) {
    if(!allowedPushOrigin(s.origin)||!await currentSession(control,db,s,now)) {
      await db.prepare("DELETE FROM push_subscriptions WHERE id=?").bind(s.id).run();
      continue;
    }
    const rows=(await db.prepare(`SELECT d.id,d.delivered_at,e.category FROM inbox_deliveries d
      JOIN inbox_events e ON e.id=d.event_id WHERE d.company_id=? AND d.user_id=?
      AND (d.delivered_at,d.id)>(?,?) ORDER BY d.delivered_at,d.id LIMIT 50`)
      .bind(s.company_id,s.user_id,s.cursor_at,s.cursor_id)
      .all<{id:string;delivered_at:number;category:string}>()).results;
    for(const row of rows)if(categoryEnabled(s,row.category))
      queued+=(await db.prepare(`INSERT INTO push_delivery_jobs
        (id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at)
        SELECT ?,?,?,?, ?,? WHERE EXISTS(SELECT 1 FROM inbox_deliveries
          WHERE id=? AND company_id=? AND user_id=? AND read_at IS NULL AND archived_at IS NULL)
        ON CONFLICT(inbox_delivery_id,subscription_id) DO NOTHING`)
        .bind(randomUUID(),s.company_id,row.id,s.id,now,now,row.id,s.company_id,s.user_id).run()).meta.changes;
    const last=rows.at(-1);
    await db.prepare(`UPDATE push_subscriptions SET scan_at=?,cursor_at=?,cursor_id=? WHERE id=?
      AND cursor_at=? AND cursor_id=?`)
      .bind(now,last?.delivered_at??s.cursor_at,last?.id??s.cursor_id,s.id,s.cursor_at,s.cursor_id).run();
  }
  return queued;
}
type Send = (subscription:Subscription,payload:string,config:PushConfig)=>Promise<{statusCode:number}>;
async function sendWebPush(subscription:Subscription,payload:string,config:PushConfig) {
  const webPush=await import("web-push");
  return webPush.sendNotification(subscription,payload,{vapidDetails:{subject:config.subject,
    publicKey:config.publicKey,privateKey:config.privateKey},TTL:300,timeout:8000,urgency:"normal"});
}
export async function drainWebPush(db:SqlDatabase,control:SqlDatabase,now=Date.now(),send:Send=sendWebPush) {
  const config=pushConfig();
  if(!config)return {disabled:true,queued:0,sent:0};
  const queued=await fanout(db,control,now);
  let sent=0;
  for(let i=0;i<25;i++) {
    const job=await db.prepare(`UPDATE push_delivery_jobs SET lease_token=?,lease_until=?,attempts=attempts+1
      WHERE id=(SELECT j.id FROM push_delivery_jobs j WHERE j.status='pending' AND j.next_attempt_at<=?
        AND (j.lease_until IS NULL OR j.lease_until<?) ORDER BY j.next_attempt_at,j.id
        FOR UPDATE SKIP LOCKED LIMIT 1)
      RETURNING *`).bind(randomUUID(),now+30_000,now,now).first<DeliveryJob>();
    if(!job)break;
    const row=await db.prepare(`SELECT j.*,s.user_id,s.session_id,s.origin,s.inbox_path,s.sealed_subscription,
      p.enabled,p.work_enabled,p.mention_enabled,p.system_enabled,p.calendar_enabled,p.notice_enabled,
      p.quiet_start_minute,p.quiet_end_minute,d.read_at,d.archived_at,e.category
      FROM push_delivery_jobs j JOIN push_subscriptions s ON s.id=j.subscription_id
      JOIN push_preferences p ON p.company_id=s.company_id AND p.user_id=s.user_id
      JOIN inbox_deliveries d ON d.id=j.inbox_delivery_id
      JOIN inbox_events e ON e.id=d.event_id WHERE j.id=? AND j.lease_token=?`)
      .bind(job.id,job.lease_token).first<DeliveryJob & Preference & {category:string;read_at:number|null;archived_at:number|null}>();
    if(!row)continue;
    const active=row.enabled && categoryEnabled(row,row.category) && row.read_at===null && row.archived_at===null &&
      allowedPushOrigin(row.origin) && await currentSession(control,db,row,now);
    if(!active){
      await db.prepare("UPDATE push_delivery_jobs SET status='dead',lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?")
        .bind(job.id,job.lease_token).run();
      continue;
    }
    const quiet=quietDelayMs(now,row.quiet_start_minute,row.quiet_end_minute);
    if(quiet>0){
      await db.prepare("UPDATE push_delivery_jobs SET next_attempt_at=?,lease_token=NULL,lease_until=NULL,attempts=attempts-1 WHERE id=? AND lease_token=?")
        .bind(now+quiet,job.id,job.lease_token).run();
      continue;
    }
    try {
      const payload=JSON.stringify({tag:job.id,path:row.inbox_path});
      const result=await send(open(config,row.sealed_subscription),payload,config);
      await db.prepare("UPDATE push_delivery_jobs SET status='sent',sent_at=?,last_status=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?")
        .bind(now,result.statusCode,job.id,job.lease_token).run();
      sent++;
    } catch(error) {
      const status=Number((error as {statusCode?:number}).statusCode)||0;
      if(status===404||status===410) {
        await db.prepare("DELETE FROM push_subscriptions WHERE id=?").bind(job.subscription_id).run();
      } else {
        const dead=job.attempts>=5||status>=400&&status<500&&status!==429;
        await db.prepare(`UPDATE push_delivery_jobs SET status=?,next_attempt_at=?,last_status=?,
          lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?`)
          .bind(dead?"dead":"pending",now+Math.min(3_600_000,1000*2**Math.min(job.attempts,10)),status||null,job.id,job.lease_token).run();
      }
    }
  }
  return {disabled:false,queued,sent};
}
