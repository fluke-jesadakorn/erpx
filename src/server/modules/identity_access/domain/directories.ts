import { z } from 'zod';
import type { DirectoryConfig, DirectoryProvider } from "@/lib/identity_access/iam";
export const connectionInput = z
    .object({
    id: z.string().optional(),
    expectedVersion: z.number().int().nonnegative(),
    name: z.string().trim().min(2).max(100),
    provider: z.enum(["active_directory", "entra_id", "univention"]),
    config: z
        .object({
        mode: z.enum(["ldaps", "oidc"]),
        secretProfile: z.string().regex(/^[a-zA-Z0-9_-]{1,80}$/),
        authority: z.string().trim().min(3).max(200),
        ldapUrl: z.string().url().optional(),
        baseDn: z.string().min(1).optional(),
        groupBaseDn: z.string().optional(),
        userFilter: z.string().optional(),
        issuer: z.string().url().optional(),
        tenantId: z.string().uuid().optional(),
        clientId: z.string().optional(),
    })
        .strict(),
})
    .strict();
export type ConnectionRow = {
    id: string;
    company_id: string;
    name: string;
    provider: DirectoryProvider;
    config: DirectoryConfig;
    status: string;
    version: number;
    tested_version: number | null;
    last_sync_at: number | null;
};
export const mappingInput = z.object({
    expectedRoleRevision: z.number().int().positive().optional(),
    connectionId: z.string(),
    groupId: z.string(),
    roleId: z.string(),
    scopeKind: z.enum(["holding", "branch", "department", "platform"]),
    scopeNodeId: z.string().nullable().default(null),
    validUntil: z.number().int(),
    reason: z.string().min(3).max(1000),
});
export type ConnectionInput = z.infer<typeof connectionInput>;
export type MappingInput = z.infer<typeof mappingInput>;
export type DirectoryMappingFact = {
    id: string;
    user_id: string;
    person_id: string | null;
    parent_id: string;
    valid_until: number;
    role_id: string;
    scope_kind: 'holding' | 'branch' | 'department' | 'platform';
    scope_node_id: string | null;
};
