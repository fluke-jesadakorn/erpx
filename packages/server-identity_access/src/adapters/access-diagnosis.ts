import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
import {diagnosedHoldingBinding} from '@erpx/server-runtime/public/identity-management-binding-facts';
const organizationSources=organizationSnapshotSources();
import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { redirect } from 'next/navigation';
import { currentSession, requireWorkContext, requirePlatformContext, resolveWorkspaceScope } from '@erpx/server-identity_access/public/authorization';
import { holdingResources, runtimeEnv, type HoldingId } from '../../../../packages/server-runtime/src/public/runtime';
import { createAccessDiagnosis } from '@erpx/server-identity_access/application/access-diagnosis';
import { planIdentityControlRead, PC_DIAGNOSIS_ASSIGNMENTS, PC_DIAGNOSIS_MEMBER_PROBE, PC_DIAGNOSIS_PLATFORM_ASSIGNMENTS } from './native-control-read-plan';
export const accessDiagnosis = createAccessDiagnosis({ session: currentSession, platform: requirePlatformContext, work: requireWorkContext, scope: resolveWorkspaceScope, now: Date.now, apiError: (error): error is ApiError => error instanceof ApiError, redirect,
    platformAssignments: async (userId) => { const env = await runtimeEnv(); const plan = await planIdentityControlRead(PC_DIAGNOSIS_PLATFORM_ASSIGNMENTS, [userId]); return (await env.CONTROL_DB.prepare(plan.sql).bind(...plan.values).all<{
        role: string;
        valid_from: number;
        valid_until: number;
        revoked_at: number | null;
    }>()).results; },
    binding: async (slug) => { const env = await runtimeEnv(); return diagnosedHoldingBinding(env.CONTROL_DB, slug); },
    member: async (holdingId, userId) => { const { write } = await holdingResources(holdingId as HoldingId); const plan = await planIdentityControlRead(PC_DIAGNOSIS_MEMBER_PROBE, [userId]); return !!await write.prepare(plan.sql).bind(...plan.values).first(); },
    scopedNode: async (holdingId, kind, slug, branchSlug, userId) => {
        const { write } = await holdingResources(holdingId as HoldingId);
        type Node = {
            id: string;
            parent_id: string | null;
            company_id: string;
            status: string;
        };
        return await write.prepare(`SELECT n.id,n.parent_id,n.company_id,n.status ${organizationSources.access_diagnosis_organization_nodes_n_from} WHERE n.kind=? AND n.slug=?
            AND (?::text IS NULL OR n.kind='branch' OR EXISTS(SELECT 1 ${organizationSources.access_diagnosis_organization_nodes_b_from} WHERE b.id=n.branch_id AND b.slug=?))
            AND EXISTS(SELECT 1 FROM assignments a WHERE a.user_id=? AND a.company_id=n.company_id AND (a.scope_kind='holding' OR a.scope_node_id=n.id OR a.scope_node_id=n.branch_id OR (n.kind='branch' AND a.branch_id=n.id))) LIMIT 1`)
            .bind(kind, slug, branchSlug, branchSlug, userId).first<Node>();
    },
    parent: async (holdingId, id, companyId) => { const { write } = await holdingResources(holdingId as HoldingId); return write.prepare(`SELECT id,parent_id,company_id,status ${organizationSources.access_diagnosis_organization_nodes_from} WHERE id=? AND company_id=?`).bind(id, companyId).first<{
        id: string;
        parent_id: string | null;
        company_id: string;
        status: string;
    }>(); },
    assignments: async (scope, userId) => { const { write } = await holdingResources(scope.holdingId); type AssignmentRow = {
        role: string;
        company_id: string;
        branch_id: string | null;
        scope_kind: 'holding' | 'branch' | 'department';
        scope_node_id: string | null;
        valid_from: number;
        valid_until: number;
        revoked_at: number | null;
    }; const plan = await planIdentityControlRead(PC_DIAGNOSIS_ASSIGNMENTS, [userId]); return (await write.prepare(plan.sql).bind(...plan.values).all<AssignmentRow>()).results; }
});
