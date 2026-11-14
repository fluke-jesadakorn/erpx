import type { AccessProblem } from "@erpx/feature-shared/lib/access-problem";
/** Only application pages can be used as a return destination, never external URLs or API actions. */
export function safeReturnPath(value: unknown): string | null {
    if (typeof value !== "string" || value.length > 2000 || /[\u0000-\u0020]/.test(value))
        return null;
    if (!/^\/(?:holdings\/|platform\/)/.test(value))
        return null;
    try {
        const url = new URL(value, "https://erpx.invalid");
        // Encoded slashes are ordinary filter values in queries and fragments.
        // Restrict route separators only in the path that controls navigation.
        if (value.split(/[?#]/, 1)[0].includes("\\") || /%(?:2f|5c|0[0-9a-f]|1[0-9a-f])/i.test(url.pathname))
            return null;
        return url.origin === "https://erpx.invalid" && /^\/(?:holdings\/[^/]+(?:\/.*)?|platform\/[^/]+(?:\/.*)?)$/.test(url.pathname) ? url.pathname + url.search + url.hash : null;
    }
    catch {
        return null;
    }
}
export type AccessAssignment = {
    applies: boolean;
    validRole: boolean;
    valid_from: number;
    valid_until: number;
    revoked_at: number | null;
};
/** Classify the user's own failed access; a usable alternative takes priority over historical rows. */
export function assignmentProblem(rows: AccessAssignment[], now: number): AccessProblem {
    const matching = rows.filter(r => r.applies);
    const valid = matching.filter(r => r.validRole);
    const unrevoked = valid.filter(r => r.revoked_at === null);
    if (unrevoked.some(r => r.valid_from <= now && r.valid_until > now))
        return "permission";
    if (unrevoked.some(r => r.valid_from > now && r.valid_until > r.valid_from))
        return "scheduled";
    if (unrevoked.some(r => r.valid_until <= now))
        return "expired";
    if (valid.length && valid.every(r => r.revoked_at !== null))
        return "revoked";
    if (matching.some(r => !r.validRole))
        return "role";
    if (rows.some(r => r.validRole && r.revoked_at === null && r.valid_from <= now && r.valid_until > now))
        return "scope";
    return "no_access";
}
