import { BusinessRuleError, type Clock, type IdentifierSource } from '@erpx/server-shared/index';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { defaultPreferences, preferenceInput, categoryEnabled, quietDelayMs, type PushConfig, Subscription, Preference, StoredSubscription, DeliveryJob, Send } from '@erpx/server-collaboration/domain/web-push';
export interface CurrentPushSessionCleanupPorts {
    session(): Promise<{ userId: string; sessionId: string } | null>;
    holdingId(slug: string): Promise<string>;
    remove(holdingId: string, userId: string, sessionId: string): Promise<{ removed: boolean }>;
}

/** Session-owned cleanup must remain available before a business workspace can be selected. */
export function createCurrentPushSessionCleanup(ports: CurrentPushSessionCleanupPorts) {
    return async (input: { holdingSlug: string; origin: string | null; expectedOrigin: string }) => {
        if (input.origin !== input.expectedOrigin)
            throw new BusinessRuleError(403, 'ORIGIN_FORBIDDEN', "Use this workspace's own origin.");
        const session = await ports.session();
        if (!session)
            throw new BusinessRuleError(401, 'UNAUTHENTICATED', 'Sign in to continue.');
        const holdingId = await ports.holdingId(input.holdingSlug);
        return ports.remove(holdingId, session.userId, session.sessionId);
    };
}

export interface WebPushRepository {
    preferences(companyId: string, userId: string): Promise<Preference | null>;
    sessionSubscriptionCount(companyId: string, userId: string, value: string): Promise<{
        n: number;
    } | null>;
    endpointOwner(companyId: string, endpointHash: string): Promise<{
        id: string;
        user_id: string;
        session_id: string;
    } | null>;
    removePreviousJobs(id: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    removePreviousSubscription(id: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    disablePreviousOwner(now: number, companyId: string, user_id: string, companyId2: string, user_id2: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    enablePreferences(companyId: string, userId: string, now: number): Promise<{
        meta: {
            changes: number;
        };
    }>;
    storeSubscription(id: string, companyId: string, userId: string, sid: string, origin: string, inboxPath: string, endpointHash: string, value: string, now: number, now2: number, now3: number, now4: number, value2: ""): Promise<{
        meta: {
            changes: number;
        };
    }>;
    savePreferences(companyId: string, userId: string, work: boolean, mention: boolean, system: boolean, calendar: boolean, notice: boolean, quietStartMinute: number | null, quietEndMinute: number | null, now: number): Promise<{
        meta: {
            changes: number;
        };
    }>;
    removeEndpoint(companyId: string, userId: string, sid: string, value: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    removeSessionDevices(companyId: string, userId: string, sid: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    disableOwnerWithoutDevices(now: number, companyId: string, userId: string, companyId2: string, userId2: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    subscriptionTable(): Promise<{
        name: string | null;
    } | null>;
    revokeSessionDevices(userId: string, sid: string): Promise<{
        results: {
            company_id: string;
        }[];
    }>;
    disableRevokedSessionOwner(now: number, companyId: string, userId: string, companyId2: string, userId2: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    fanoutSubscriptions(): Promise<{
        results: (StoredSubscription & Preference)[];
    }>;
    removeInvalidSubscription(id: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    newDeliveries(company_id: string, user_id: string, cursor_at: number, cursor_id: string): Promise<{
        results: {
            id: string;
            delivered_at: number;
            category: string;
        }[];
    }>;
    queueDelivery(value: string, company_id: string, id: string, id2: string, now: number, now2: number, id3: string, company_id2: string, user_id: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    advanceSubscriptionCursor(now: number, value: number, value2: string, id: string, cursor_at: number, cursor_id: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    leasePendingDelivery(value: string, value2: number, now: number, now2: number): Promise<DeliveryJob | null>;
    currentDelivery(id: string, lease_token: string): Promise<DeliveryJob & Preference & {
        category: string;
        read_at: number | null;
        archived_at: number | null;
    } | null>;
    deadDelivery(id: string, lease_token: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    deferQuietDelivery(value: number, id: string, lease_token: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    completeDelivery(now: number, statusCode: number, id: string, lease_token: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    removeGoneSubscription(subscription_id: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
    retryDelivery(value: "dead" | "pending", value2: number, value3: number | null, id: string, lease_token: string): Promise<{
        meta: {
            changes: number;
        };
    }>;
}
export interface WebPushPorts {
    repository: WebPushRepository;
    clock: Clock;
    identifiers: IdentifierSource;
    configuration(): PushConfig | null;
    allowedOrigin(origin: string): boolean;
    validate(raw: unknown): Subscription;
    hashEndpoint(endpoint: string): string;
    seal(config: PushConfig, value: Subscription): string;
    open(config: PushConfig, value: string): Subscription;
    currentSession(subscription: Pick<StoredSubscription, 'session_id' | 'user_id' | 'company_id'>, now: number): Promise<boolean>;
    send: Send;
}
export function createWebPushApplication(ports: WebPushPorts) {
    const r = ports.repository;
    function sessionId(actor: WorkContext) {
        if (!actor.authority?.sessionId)
            throw new BusinessRuleError(403, 'PUSH_CONTEXT_REQUIRED', 'Choose a work role before enabling push.');
        return actor.authority.sessionId;
    }
    async function pushSettings(actor: WorkContext) {
        const config = ports.configuration();
        if (!config)
            return { available: false, publicKey: null, preferences: null, deviceCount: 0 };
        const preferences = await r.preferences(actor.companyId, actor.userId);
        const count = await r.sessionSubscriptionCount(actor.companyId, actor.userId, sessionId(actor));
        return { available: true, publicKey: config.publicKey, preferences: preferences ?? defaultPreferences,
            deviceCount: count?.n ?? 0 };
    }
    async function registerPush(actor: WorkContext, origin: string, inboxPath: string, raw: unknown) {
        const config = ports.configuration();
        if (!config)
            throw new BusinessRuleError(503, "PUSH_DISABLED", "Push is not configured. Inbox remains available.");
        if (!ports.allowedOrigin(origin))
            throw new BusinessRuleError(403, "PUSH_ORIGIN_FORBIDDEN", "Use this workspace's own origin.");
        const input = ports.validate(raw), now = ports.clock.now(), id = ports.identifiers.next();
        const endpointHash = ports.hashEndpoint(input.endpoint);
        const sid = sessionId(actor);
        // A browser's single endpoint cannot keep queued work for a previous owner.
        const previous = await r.endpointOwner(actor.companyId, endpointHash);
        if (previous && (previous.user_id !== actor.userId || previous.session_id !== sid)) {
            await r.removePreviousJobs(previous.id);
            await r.removePreviousSubscription(previous.id);
            if (previous.user_id !== actor.userId)
                await r.disablePreviousOwner(now, actor.companyId, previous.user_id, actor.companyId, previous.user_id);
        }
        await r.enablePreferences(actor.companyId, actor.userId, now);
        await r.storeSubscription(id, actor.companyId, actor.userId, sid, origin, inboxPath, endpointHash, ports.seal(config, input), now, now, now, now, "");
        return { enabled: true };
    }
    async function savePushPreferences(actor: WorkContext, raw: unknown) {
        const v = preferenceInput.parse(raw), now = ports.clock.now();
        await r.savePreferences(actor.companyId, actor.userId, v.work, v.mention, v.system, v.calendar, v.notice, v.quietStartMinute, v.quietEndMinute, now);
        return { saved: true };
    }
    async function removePushDevice(actor: WorkContext, endpoint?: string) {
        if (!ports.configuration())
            return { removed: true };
        const sid = sessionId(actor);
        if (endpoint)
            await r.removeEndpoint(actor.companyId, actor.userId, sid, ports.hashEndpoint(endpoint));
        else
            await r.removeSessionDevices(actor.companyId, actor.userId, sid);
        await r.disableOwnerWithoutDevices(ports.clock.now(), actor.companyId, actor.userId, actor.companyId, actor.userId);
        return { removed: true };
    }
    async function removePushSession(userId: string, sid: string) {
        const table = await r.subscriptionTable();
        if (!table?.name)
            return { removed: true };
        const removed = (await r.revokeSessionDevices(userId, sid)).results;
        for (const companyId of new Set(removed.map(row => row.company_id)))
            await r.disableRevokedSessionOwner(ports.clock.now(), companyId, userId, companyId, userId);
        return { removed: true };
    }
    async function fanout(now: number) {
        let queued = 0;
        const subscriptions = (await r.fanoutSubscriptions()).results;
        for (const s of subscriptions) {
            if (!ports.allowedOrigin(s.origin) || !await ports.currentSession(s, now)) {
                await r.removeInvalidSubscription(s.id);
                continue;
            }
            const rows = (await r.newDeliveries(s.company_id, s.user_id, s.cursor_at, s.cursor_id)).results;
            for (const row of rows)
                if (categoryEnabled(s, row.category))
                    queued += (await r.queueDelivery(ports.identifiers.next(), s.company_id, row.id, s.id, now, now, row.id, s.company_id, s.user_id)).meta.changes;
            const last = rows.at(-1);
            await r.advanceSubscriptionCursor(now, last?.delivered_at ?? s.cursor_at, last?.id ?? s.cursor_id, s.id, s.cursor_at, s.cursor_id);
        }
        return queued;
    }
    async function drainWebPush(now = ports.clock.now(), send: Send = ports.send) {
        const config = ports.configuration();
        if (!config)
            return { disabled: true, queued: 0, sent: 0 };
        const queued = await fanout(now);
        let sent = 0;
        for (let i = 0; i < 25; i++) {
            const job = await r.leasePendingDelivery(ports.identifiers.next(), now + 30000, now, now);
            if (!job)
                break;
            const row = await r.currentDelivery(job.id, job.lease_token);
            if (!row)
                continue;
            const active = row.enabled && categoryEnabled(row, row.category) && row.read_at === null && row.archived_at === null &&
                ports.allowedOrigin(row.origin) && await ports.currentSession(row, now);
            if (!active) {
                await r.deadDelivery(job.id, job.lease_token);
                continue;
            }
            const quiet = quietDelayMs(now, row.quiet_start_minute, row.quiet_end_minute);
            if (quiet > 0) {
                await r.deferQuietDelivery(now + quiet, job.id, job.lease_token);
                continue;
            }
            try {
                const payload = JSON.stringify({ tag: job.id, path: row.inbox_path });
                const result = await send(ports.open(config, row.sealed_subscription), payload, config);
                await r.completeDelivery(now, result.statusCode, job.id, job.lease_token);
                sent++;
            }
            catch (error) {
                const status = Number((error as {
                    statusCode?: number;
                }).statusCode) || 0;
                if (status === 404 || status === 410) {
                    await r.removeGoneSubscription(job.subscription_id);
                }
                else {
                    const dead = job.attempts >= 5 || status >= 400 && status < 500 && status !== 429;
                    await r.retryDelivery(dead ? "dead" : "pending", now + Math.min(3600000, 1000 * 2 ** Math.min(job.attempts, 10)), status || null, job.id, job.lease_token);
                }
            }
        }
        return { disabled: false, queued, sent };
    }
    return { pushSettings, registerPush, savePushPreferences, removePushDevice, removePushSession, drainWebPush };
}
