import { z } from 'zod';
import type { DirectoryConfig, DirectoryProvider } from "@erpx/feature-identity_access/lib/iam";
import type { DirectoryHierarchy } from "@erpx/feature-organization/lib/hierarchy";
export type ProviderUser = {
    externalId: string;
    username: string;
    name: string;
    email: string | null;
    enabled: boolean;
    groupIds: string[];
};
export type ProviderSnapshot = {
    hierarchy?: DirectoryHierarchy;
    users: ProviderUser[];
    groups: {
        externalId: string;
        name: string;
    }[];
    complete: true;
};
export type ProviderConnection = {
    id: string;
    provider: DirectoryProvider;
    config: DirectoryConfig;
};
export const secretSchema = z.object({
    allowedHosts: z.array(z.string()).min(1),
    connectionIds: z.array(z.string()).min(1),
    bindDn: z.string().optional(),
    bindPassword: z.string().optional(),
    clientSecret: z.string().optional(),
    caPem: z.string().optional(),
    tlsServerName: z.string().optional(),
    verificationIdToken: z.string().optional(),
    udmUrl: z.string().url().optional(),
    udmUsername: z.string().optional(),
    udmPassword: z.string().optional(),
    sambaRunnerUrl: z.string().url().optional(),
    sambaRunnerToken: z.string().min(32).optional(),
    windowsRunnerUrl: z.string().url().optional(),
    windowsRunnerToken: z.string().min(32).optional(),
    identityAliases: z.record(z.string(), z.string()).default({}),
    writeOperations: z.array(z.string()).default([]),
    writeBaseDns: z.array(z.string().min(3)).default([]),
});
export type DirectorySecret = z.infer<typeof secretSchema>;
export type ProviderFailure = (status: number, code: string, message: string) => Error;
export function createProviderPolicy(error: ProviderFailure) {
    function canonicalExternalId(secret: z.infer<typeof secretSchema>, providerId: string) {
        return secret.identityAliases[providerId.toLowerCase()] ?? providerId;
    }
    function approvedUrl(value: string | undefined, hosts: string[], protocol: string) {
        let url: URL;
        try {
            url = new URL(value ?? "");
        }
        catch {
            throw error(422, "DIRECTORY_URL_INVALID", "Enter a valid directory endpoint.");
        }
        if (url.protocol !== protocol ||
            url.username ||
            url.password ||
            !hosts.includes(url.hostname.toLowerCase()))
            throw error(422, "DIRECTORY_ENDPOINT_FORBIDDEN", "The endpoint must use a server-approved hostname and secure protocol.");
        return url;
    }
    return { canonicalExternalId, approvedUrl, profile: (profiles: Record<string, unknown>, config: DirectoryConfig, connectionId: string) => { const parsed = secretSchema.safeParse(profiles[config.secretProfile]); if (!parsed.success)
            throw error(409, 'DIRECTORY_SECRET_UNAVAILABLE', 'The approved credential profile is incomplete.'); if (!parsed.data.connectionIds.includes(connectionId))
            throw error(403, 'DIRECTORY_PROFILE_SCOPE', 'This secret profile is not approved for this connection.'); return parsed.data; } };
}
export function escapeLdap(value: string) {
    return value.replace(/[\\*()\0]/g, (c) => `\\${c.charCodeAt(0).toString(16).padStart(2, "0")}`);
}
export function adGuidBytes(value: Uint8Array) { const b = new Uint8Array(value), part = (from: number, to: number, reverse = false) => [...(reverse ? b.slice(from, to).reverse() : b.slice(from, to))].map(n => n.toString(16).padStart(2, '0')).join(''); return [part(0, 4, true), part(4, 6, true), part(6, 8, true), part(8, 10), part(10, 16)].join('-'); }
export function ldapAccountEnabled(values: Record<string, string>, ad: boolean, now: number) {
    const first = (name: string) => values[name] ?? '';
    if (ad) {
        const expires = first("accountExpires");
        return (!(Number(first("userAccountControl")) & 2) &&
            (!expires ||
                expires === "0" ||
                expires === "9223372036854775807" ||
                Number(BigInt(expires) / BigInt(10000) - BigInt(11644473600000)) > now));
    }
    const shadow = Number(first("shadowExpire"));
    return (!first("pwdAccountLockedTime") &&
        !first("sambaAcctFlags").includes("D") &&
        (!shadow || shadow < 0 || shadow * 86400000 > now));
}
