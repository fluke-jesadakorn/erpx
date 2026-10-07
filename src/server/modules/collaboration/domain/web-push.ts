import { z } from 'zod';
import { BusinessRuleError } from '../../shared';
export type PushConfig = {
    publicKey: string;
    privateKey: string;
    subject: string;
    dataKey: Uint8Array;
};
export type Subscription = {
    endpoint: string;
    keys: {
        p256dh: string;
        auth: string;
    };
};
export type Preference = {
    enabled: boolean;
    work_enabled: boolean;
    mention_enabled: boolean;
    system_enabled: boolean;
    calendar_enabled: boolean;
    notice_enabled: boolean;
    quiet_start_minute: number | null;
    quiet_end_minute: number | null;
};
export type StoredSubscription = {
    id: string;
    company_id: string;
    user_id: string;
    session_id: string;
    origin: string;
    inbox_path: string;
    sealed_subscription: string;
    cursor_at: number;
    cursor_id: string;
};
export type DeliveryJob = {
    id: string;
    subscription_id: string;
    inbox_delivery_id: string;
    company_id: string;
    user_id: string;
    session_id: string;
    origin: string;
    inbox_path: string;
    sealed_subscription: string;
    attempts: number;
    lease_token: string;
    quiet_start_minute: number | null;
    quiet_end_minute: number | null;
};
export type Send = (subscription: Subscription, payload: string, config: PushConfig) => Promise<{
    statusCode: number;
}>;
export const b64 = /^[A-Za-z0-9_-]+$/;
export const subscriptionInput = z.object({
    endpoint: z.string().url().max(2048),
    keys: z.object({ p256dh: z.string().regex(b64).max(160), auth: z.string().regex(b64).max(80) }),
});
export const defaultPreferences: Preference = { enabled: false, work_enabled: true, mention_enabled: true,
    system_enabled: true, calendar_enabled: false, notice_enabled: false,
    quiet_start_minute: null, quiet_end_minute: null };
export const preferenceInput = z.object({
    work: z.boolean(), mention: z.boolean(), system: z.boolean(), calendar: z.boolean(), notice: z.boolean(),
    quietStartMinute: z.number().int().min(0).max(1439).nullable(),
    quietEndMinute: z.number().int().min(0).max(1439).nullable(),
}).strict().refine(v => (v.quietStartMinute === null) === (v.quietEndMinute === null) &&
    (v.quietStartMinute === null || v.quietStartMinute !== v.quietEndMinute), "Choose different start and end times.");
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
        throw new BusinessRuleError(422, "INVALID_PUSH_SUBSCRIPTION", "This browser's push service is not supported.");
    return input;
}
export function categoryEnabled(preference: Preference, category: string) {
    return ({ work: preference.work_enabled, mention: preference.mention_enabled,
        system: preference.system_enabled, calendar: preference.calendar_enabled,
        notice: preference.notice_enabled } as Record<string, boolean>)[category] ?? false;
}
export function quietDelayMs(now: number, start: number | null, end: number | null) {
    if (start === null || end === null)
        return 0;
    const local = new Date(now + 7 * 60 * 60000), minute = local.getUTCHours() * 60 + local.getUTCMinutes();
    const inside = start < end ? minute >= start && minute < end : minute >= start || minute < end;
    if (!inside)
        return 0;
    const remaining = (end - minute + 1440) % 1440;
    return remaining * 60000 - local.getUTCSeconds() * 1000 - local.getUTCMilliseconds();
}
export function pushOriginAllowed(origin: string, origins: readonly string[], production: boolean) {
    if (origins.includes(origin))
        return true;
    return !production && /^http:\/\/(localhost|127\.0\.0\.1)(:3100)?$/.test(origin);
}
