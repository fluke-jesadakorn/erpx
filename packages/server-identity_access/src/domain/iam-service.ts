import { z } from 'zod';
const identifier = z.string().min(1).max(180);
export const roleInput = z.object({
    id: identifier.optional(),
    name: z.string().trim().min(2).max(100),
    expectedVersion: z.number().int().nonnegative(),
    permissions: z.array(identifier).max(1000),
    grantablePermissions: z.array(identifier).max(1000).default([]),
    scopes: z
        .array(z.enum(["holding", "branch", "department", "platform"]))
        .min(1),
    reason: z.string().trim().min(3).max(1000),
});
export const grantInput = z.object({
    userId: identifier,
    roleId: identifier,
    expectedRoleRevision: z.number().int().positive().optional(),
    scopeKind: z.enum(["holding", "branch", "department", "platform"]),
    scopeNodeId: identifier.nullable().default(null),
    validUntil: z.number().int().positive().default(0),
    reason: z.string().trim().min(3).max(1000),
});
export const reviewInput = z.object({
    expectedVersion: z.number().int().positive(),
    decision: z.enum(["approve", "reject"]),
    reason: z.string().trim().min(3).max(1000),
});
export const accessReviewCampaignInput = z.object({
    name: z.string().trim().min(3).max(120),
    dueAt: z.number().int().positive(),
    reason: z.string().trim().min(3).max(1000),
});
export const accessReviewDecisionInput = z.object({
    expectedCampaignVersion: z.number().int().positive(),
    decision: z.enum(["keep", "revoke"]),
    reason: z.string().trim().min(3).max(1000),
});
export type IamAssignmentFact = {
    id: string;
    user_id: string;
    role: string;
    company_id: string;
    scope_kind: 'holding' | 'branch' | 'department' | 'platform';
    scope_node_id: string | null;
    branch_id: string | null;
    valid_until: number;
    valid_from: number;
    version: number;
    revoked_at: number | null;
    source: string;
};
export type IamRoleFact = {
    id: string;
    role: string;
    company_id: string;
    name: string;
    published_revision: number;
    built_in: boolean;
    grantable_json: string[];
    permissions_json: string[];
    scopes_json: string[];
};
export type IamReviewPayload = Record<string, unknown> & {
    id: string;
    name: string;
    revision: number;
    expectedVersion: number;
    permissions: string[];
    grantablePermissions: string[];
    scopeKind: string;
    scopeNodeId: string | null;
    authorizingAssignmentId: string;
    parentId: string;
    version: number;
    reason: string;
};
export type IamReviewFact = {
    id: string;
    status: string;
    version: number;
    created_by: string;
    created_person: string;
    kind: string;
    payload: IamReviewPayload;
};
export type IamReviewItemFact = IamAssignmentFact & {
    assignment_id: string;
    assignment_version: number;
    campaign_id: string;
    campaign_status: string;
    campaign_version: number;
    created_by: string;
    decision: string;
};
export type IamCampaignFact = {
    id: string;
    status: string;
    version: number;
    created_by: string;
};
export type IamScopeNodeFact = {
    id: string;
    kind: string;
    branch_id: string | null;
};
export type IamMemberFact = {
    status: string;
    login_policy: string;
};
export type RoleInput = z.infer<typeof roleInput>;
export type GrantInput = z.infer<typeof grantInput>;
export type ReviewInput = z.infer<typeof reviewInput>;
export type CampaignInput = z.infer<typeof accessReviewCampaignInput>;
export type CampaignDecisionInput = z.infer<typeof accessReviewDecisionInput>;
