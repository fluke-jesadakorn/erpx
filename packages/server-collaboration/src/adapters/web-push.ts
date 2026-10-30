import 'server-only';
import {holdingRoutingEnabled,activeHoldingHosts} from '@erpx/feature-runtime/lib/holding-hosts';
import { createCipheriv, createDecipheriv, createECDH, createHash, randomBytes } from 'node:crypto';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { currentPushSession } from '@erpx/server-identity_access/public/collaboration-push-session';
import { sessionFromRequest } from '@erpx/server-identity_access/public/auth';
import { holdingBindingBySlug, holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { requestPublicOrigin } from '@erpx/server-manage_erpx/public/public-origin';
import { configuredProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { legacyConsoleRetired } from "@erpx/feature-runtime/lib/console-retirement";
import { createCurrentPushSessionCleanup, createWebPushApplication, type WebPushRepository } from '@erpx/server-collaboration/application/web-push';
import { pushOriginAllowed, b64, validatePushSubscription as validateSubscription, type PushConfig, Subscription, Preference, StoredSubscription, DeliveryJob, Send } from '@erpx/server-collaboration/domain/web-push';
async function exposed<T>(action: () => Promise<T>) {
    try {
        return await action();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export function pushConfig(): PushConfig | null {
    const { ERPX_PUSH_VAPID_PUBLIC_KEY: publicKey, ERPX_PUSH_VAPID_PRIVATE_KEY: privateKey, ERPX_PUSH_VAPID_SUBJECT: subject, ERPX_PUSH_DATA_KEY: sealedKey } = process.env;
    if (!publicKey || !privateKey || !subject || !sealedKey)
        return null;
    const dataKey = Buffer.from(sealedKey, "base64url");
    if (!b64.test(publicKey) || Buffer.from(publicKey, "base64url").length !== 65 ||
        !b64.test(privateKey) || Buffer.from(privateKey, "base64url").length !== 32 ||
        !b64.test(sealedKey) || dataKey.length !== 32 ||
        !(subject.startsWith("mailto:") || subject.startsWith("https://")))
        return null;
    try {
        const pair = createECDH("prime256v1");
        pair.setPrivateKey(Buffer.from(privateKey, "base64url"));
        if (pair.getPublicKey(undefined, "uncompressed").toString("base64url") !== publicKey)
            return null;
    }
    catch {
        return null;
    }
    return { publicKey, privateKey, subject, dataKey };
}
export function allowedPushOrigin(origin: string) { return pushOriginAllowed(origin, [
    ...(holdingRoutingEnabled(process.env)?activeHoldingHosts().map(host=>`https://${host.hostname}`):[]),
    ...(!legacyConsoleRetired() ? [process.env.ERPX_PUBLIC_ORIGIN] : []),
    process.env.ERPX_FRONT_PMS_ORIGIN, process.env.ERPX_FRONT_POS_ORIGIN,
    ...(["work", "pos", "pms"] as const).map(key => configuredProductOrigin(key)),
].filter((value): value is string => Boolean(value)), process.env.NODE_ENV === 'production'); }
function seal(config: PushConfig, value: Subscription) {
    const iv = randomBytes(12), cipher = createCipheriv("aes-256-gcm", config.dataKey, iv);
    const body = Buffer.concat([cipher.update(JSON.stringify(value), "utf8"), cipher.final()]);
    return Buffer.concat([iv, cipher.getAuthTag(), body]).toString("base64url");
}
function open(config: PushConfig, value: string): Subscription {
    const bytes = Buffer.from(value, "base64url");
    const decipher = createDecipheriv("aes-256-gcm", config.dataKey, bytes.subarray(0, 12));
    decipher.setAuthTag(bytes.subarray(12, 28));
    return validatePushSubscription(JSON.parse(Buffer.concat([decipher.update(bytes.subarray(28)), decipher.final()]).toString("utf8")));
}
async function sendWebPush(subscription: Subscription, payload: string, config: PushConfig) {
    const webPush = await import("web-push");
    return webPush.sendNotification(subscription, payload, { vapidDetails: { subject: config.subject,
            publicKey: config.publicKey, privateKey: config.privateKey }, TTL: 300, timeout: 8000, urgency: "normal" });
}
const hashEndpoint = (endpoint: string) => createHash('sha256').update(endpoint).digest('hex');
export function validatePushSubscription(raw: unknown) {
    try {
        return validateSubscription(raw);
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
function application(db: SqlDatabase, control?: SqlDatabase) {
    const repository: WebPushRepository = { async preferences(companyId: string, userId: string) {
            return db.prepare("SELECT * FROM push_preferences WHERE company_id=? AND user_id=?")
                .bind(companyId, userId).first<Preference>();
        }, async sessionSubscriptionCount(companyId: string, userId: string, value: unknown) {
            return db.prepare("SELECT count(*)::int n FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?")
                .bind(companyId, userId, value).first<{
                n: number;
            }>();
        }, async endpointOwner(companyId: string, endpointHash: unknown) {
            return db.prepare("SELECT id,user_id,session_id FROM push_subscriptions WHERE company_id=? AND endpoint_hash=?")
                .bind(companyId, endpointHash).first<{
                id: string;
                user_id: string;
                session_id: string;
            }>();
        }, async removePreviousJobs(id: string) { return db.prepare("DELETE FROM push_delivery_jobs WHERE subscription_id=?").bind(id).run(); }, async removePreviousSubscription(id: string) { return db.prepare("DELETE FROM push_subscriptions WHERE id=?").bind(id).run(); }, async disablePreviousOwner(now: number, companyId: string, user_id: string, companyId2: string, user_id2: string) {
            return db.prepare(`UPDATE push_preferences SET enabled=false,updated_at=?
      WHERE company_id=? AND user_id=? AND NOT EXISTS(SELECT 1 FROM push_subscriptions WHERE company_id=? AND user_id=?)`)
                .bind(now, companyId, user_id, companyId2, user_id2).run();
        }, async enablePreferences(companyId: string, userId: string, now: number) {
            return db.prepare(`INSERT INTO push_preferences(company_id,user_id,enabled,updated_at) VALUES(?,?,true,?)
    ON CONFLICT(company_id,user_id) DO UPDATE SET enabled=true,updated_at=excluded.updated_at`)
                .bind(companyId, userId, now).run();
        }, async storeSubscription(id: unknown, companyId: string, userId: string, sid: unknown, origin: string, inboxPath: string, endpointHash: unknown, value: unknown, now: number, now2: number, now3: number, now4: number, value2: "") {
            return db.prepare(`INSERT INTO push_subscriptions
    (id,company_id,user_id,session_id,origin,inbox_path,endpoint_hash,sealed_subscription,created_at,updated_at,scan_at,cursor_at,cursor_id)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,endpoint_hash) DO UPDATE SET
    user_id=excluded.user_id,session_id=excluded.session_id,origin=excluded.origin,
    inbox_path=excluded.inbox_path,sealed_subscription=excluded.sealed_subscription,
    updated_at=excluded.updated_at,
    scan_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.scan_at ELSE excluded.scan_at END,
    cursor_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.cursor_at ELSE excluded.cursor_at END,
    cursor_id=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.cursor_id ELSE '' END,
    created_at=CASE WHEN push_subscriptions.user_id=excluded.user_id AND push_subscriptions.session_id=excluded.session_id THEN push_subscriptions.created_at ELSE excluded.created_at END`)
                .bind(id, companyId, userId, sid, origin, inboxPath, endpointHash, value, now, now2, now3, now4, value2).run();
        }, async savePreferences(companyId: string, userId: string, work: boolean, mention: boolean, system: boolean, calendar: boolean, notice: boolean, quietStartMinute: number | null, quietEndMinute: number | null, now: number) {
            return db.prepare(`INSERT INTO push_preferences
    (company_id,user_id,enabled,work_enabled,mention_enabled,system_enabled,calendar_enabled,notice_enabled,quiet_start_minute,quiet_end_minute,updated_at)
    VALUES(?,?,false,?,?,?,?,?,?,?,?) ON CONFLICT(company_id,user_id) DO UPDATE SET
    work_enabled=excluded.work_enabled,mention_enabled=excluded.mention_enabled,
    system_enabled=excluded.system_enabled,calendar_enabled=excluded.calendar_enabled,
    notice_enabled=excluded.notice_enabled,quiet_start_minute=excluded.quiet_start_minute,
    quiet_end_minute=excluded.quiet_end_minute,updated_at=excluded.updated_at`)
                .bind(companyId, userId, work, mention, system, calendar, notice, quietStartMinute, quietEndMinute, now).run();
        }, async removeEndpoint(companyId: string, userId: string, sid: unknown, value: unknown) {
            return db.prepare("DELETE FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=? AND endpoint_hash=?")
                .bind(companyId, userId, sid, value).run();
        }, async removeSessionDevices(companyId: string, userId: string, sid: unknown) {
            return db.prepare("DELETE FROM push_subscriptions WHERE company_id=? AND user_id=? AND session_id=?")
                .bind(companyId, userId, sid).run();
        }, async disableOwnerWithoutDevices(now: number, companyId: string, userId: string, companyId2: string, userId2: string) {
            return db.prepare(`UPDATE push_preferences SET enabled=false,updated_at=? WHERE company_id=? AND user_id=?
    AND NOT EXISTS(SELECT 1 FROM push_subscriptions WHERE company_id=? AND user_id=?)`)
                .bind(now, companyId, userId, companyId2, userId2).run();
        }, async subscriptionTable() {
            return db.prepare("SELECT to_regclass('push_subscriptions') AS name").first<{
                name: string | null;
            }>();
        }, async revokeSessionDevices(userId: string, sid: string) {
            return db.prepare("DELETE FROM push_subscriptions WHERE user_id=? AND session_id=? RETURNING company_id")
                .bind(userId, sid).all<{
                company_id: string;
            }>();
        }, async disableRevokedSessionOwner(now: number, companyId: string, userId: string, companyId2: string, userId2: string) {
            return db.prepare(`UPDATE push_preferences SET enabled=false,updated_at=? WHERE company_id=? AND user_id=?
      AND NOT EXISTS(SELECT 1 FROM push_subscriptions WHERE company_id=? AND user_id=?)`)
                .bind(now, companyId, userId, companyId2, userId2).run();
        }, async fanoutSubscriptions() {
            return db.prepare(`SELECT s.*,p.* FROM push_subscriptions s JOIN push_preferences p
    ON p.company_id=s.company_id AND p.user_id=s.user_id WHERE p.enabled=true
    ORDER BY s.scan_at,s.id LIMIT 25`).all<StoredSubscription & Preference>();
        }, async removeInvalidSubscription(id: string) { return db.prepare("DELETE FROM push_subscriptions WHERE id=?").bind(id).run(); }, async newDeliveries(company_id: string, user_id: string, cursor_at: number, cursor_id: string) {
            return db.prepare(`SELECT d.id,d.delivered_at,e.category FROM inbox_deliveries d
      JOIN inbox_events e ON e.id=d.event_id WHERE d.company_id=? AND d.user_id=?
      AND (d.delivered_at,d.id)>(?,?) ORDER BY d.delivered_at,d.id LIMIT 50`)
                .bind(company_id, user_id, cursor_at, cursor_id)
                .all<{
                id: string;
                delivered_at: number;
                category: string;
            }>();
        }, async queueDelivery(value: unknown, company_id: string, id: string, id2: string, now: number, now2: number, id3: string, company_id2: string, user_id: string) {
            return db.prepare(`INSERT INTO push_delivery_jobs
        (id,company_id,inbox_delivery_id,subscription_id,next_attempt_at,created_at)
        SELECT ?,?,?,?, ?,? WHERE EXISTS(SELECT 1 FROM inbox_deliveries
          WHERE id=? AND company_id=? AND user_id=? AND read_at IS NULL AND archived_at IS NULL)
        ON CONFLICT(inbox_delivery_id,subscription_id) DO NOTHING`)
                .bind(value, company_id, id, id2, now, now2, id3, company_id2, user_id).run();
        }, async advanceSubscriptionCursor(now: number, value: number, value2: string, id: string, cursor_at: number, cursor_id: string) {
            return db.prepare(`UPDATE push_subscriptions SET scan_at=?,cursor_at=?,cursor_id=? WHERE id=?
      AND cursor_at=? AND cursor_id=?`)
                .bind(now, value, value2, id, cursor_at, cursor_id).run();
        }, async leasePendingDelivery(value: unknown, value2: number, now: number, now2: number) {
            return db.prepare(`UPDATE push_delivery_jobs SET lease_token=?,lease_until=?,attempts=attempts+1
      WHERE id=(SELECT j.id FROM push_delivery_jobs j WHERE j.status='pending' AND j.next_attempt_at<=?
        AND (j.lease_until IS NULL OR j.lease_until<?) ORDER BY j.next_attempt_at,j.id
        FOR UPDATE SKIP LOCKED LIMIT 1)
      RETURNING *`).bind(value, value2, now, now2).first<DeliveryJob>();
        }, async currentDelivery(id: string, lease_token: string) {
            return db.prepare(`SELECT j.*,s.user_id,s.session_id,s.origin,s.inbox_path,s.sealed_subscription,
      p.enabled,p.work_enabled,p.mention_enabled,p.system_enabled,p.calendar_enabled,p.notice_enabled,
      p.quiet_start_minute,p.quiet_end_minute,d.read_at,d.archived_at,e.category
      FROM push_delivery_jobs j JOIN push_subscriptions s ON s.id=j.subscription_id
      JOIN push_preferences p ON p.company_id=s.company_id AND p.user_id=s.user_id
      JOIN inbox_deliveries d ON d.id=j.inbox_delivery_id
      JOIN inbox_events e ON e.id=d.event_id WHERE j.id=? AND j.lease_token=?`)
                .bind(id, lease_token).first<DeliveryJob & Preference & {
                category: string;
                read_at: number | null;
                archived_at: number | null;
            }>();
        }, async deadDelivery(id: string, lease_token: string) {
            return db.prepare("UPDATE push_delivery_jobs SET status='dead',lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?")
                .bind(id, lease_token).run();
        }, async deferQuietDelivery(value: number, id: string, lease_token: string) {
            return db.prepare("UPDATE push_delivery_jobs SET next_attempt_at=?,lease_token=NULL,lease_until=NULL,attempts=attempts-1 WHERE id=? AND lease_token=?")
                .bind(value, id, lease_token).run();
        }, async completeDelivery(now: number, statusCode: number, id: string, lease_token: string) {
            return db.prepare("UPDATE push_delivery_jobs SET status='sent',sent_at=?,last_status=?,lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?")
                .bind(now, statusCode, id, lease_token).run();
        }, async removeGoneSubscription(subscription_id: string) { return db.prepare("DELETE FROM push_subscriptions WHERE id=?").bind(subscription_id).run(); }, async retryDelivery(value: "dead" | "pending", value2: number, value3: number | null, id: string, lease_token: string) {
            return db.prepare(`UPDATE push_delivery_jobs SET status=?,next_attempt_at=?,last_status=?,
          lease_token=NULL,lease_until=NULL WHERE id=? AND lease_token=?`)
                .bind(value, value2, value3, id, lease_token).run();
        } };
    return createWebPushApplication({ repository, clock: { now: () => Date.now() }, identifiers: { next: () => crypto.randomUUID() }, configuration: pushConfig, allowedOrigin: allowedPushOrigin, validate: validateSubscription, hashEndpoint, seal, open, currentSession: (s, now) => currentPushSession(control!, db, s, now), send: sendWebPush });
}
export function pushSettings(db: SqlDatabase, actor: WorkContext) { return exposed(() => application(db).pushSettings(actor)); }
export function registerPush(db: SqlDatabase, actor: WorkContext, origin: string, inboxPath: string, raw: unknown) { return exposed(() => application(db).registerPush(actor, origin, inboxPath, raw)); }
export function savePushPreferences(db: SqlDatabase, actor: WorkContext, raw: unknown) { return exposed(() => application(db).savePushPreferences(actor, raw)); }
export function removePushDevice(db: SqlDatabase, actor: WorkContext, endpoint?: string) { return exposed(() => application(db).removePushDevice(actor, endpoint)); }
export function removePushSession(db: SqlDatabase, userId: string, sid: string) { return exposed(() => application(db).removePushSession(userId, sid)); }
export function removeCurrentPushSession(request: Request, holdingSlug: string) {
    return exposed(() => createCurrentPushSessionCleanup({
        session: async () => {
            const session = await sessionFromRequest(request);
            return session ? { userId: session.user.id, sessionId: session.session.id } : null;
        },
        holdingId: async slug => (await holdingBindingBySlug(slug)).id,
        remove: async (holdingId, userId, sessionId) => removePushSession((await holdingResources(holdingId)).write, userId, sessionId),
    })({ holdingSlug, origin: request.headers.get('origin'), expectedOrigin: requestPublicOrigin(request) }));
}
export function drainWebPush(db: SqlDatabase, control: SqlDatabase, now = Date.now(), send: Send = sendWebPush) { return exposed(() => application(db, control).drainWebPush(now, send)); }
export { defaultPreferences, quietDelayMs } from '@erpx/server-collaboration/domain/web-push';
