import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
import {diagnosedHoldingBinding} from '@erpx/server-runtime/public/identity-management-binding-facts';
const organizationSources=organizationSnapshotSources();
import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { redirect } from 'next/navigation';
import { currentSession, requireWorkContext, requirePlatformContext, resolveWorkspaceScope } from '@erpx/server-identity_access/public/authorization';
import { holdingResources, runtimeEnv, type HoldingId } from '../../../../packages/server-runtime/src/public/runtime';
import { createAccessDiagnosis } from '@erpx/server-identity_access/application/access-diagnosis';
export const accessDiagnosis = createAccessDiagnosis({ session: currentSession, platform: requirePlatformContext, work: requireWorkContext, scope: resolveWorkspaceScope, now: Date.now, apiError: (error): error is ApiError => error instanceof ApiError, redirect,
    platformAssignments: async (userId) => { const env = await runtimeEnv(); return (await env.CONTROL_DB.prepare("SELECT role,valid_from,valid_until,revoked_at FROM platform_assignments WHERE user_id=?").bind(userId).all<{
        role: string;
        valid_from: number;
        valid_until: number;
        revoked_at: number | null;
    }>()).results; },
    binding: async (slug) => { const env = await runtimeEnv(); return diagnosedHoldingBinding(env.CONTROL_DB, slug); },
    member: async (holdingId, userId) => { const { write } = await holdingResources(holdingId as HoldingId); return !!await write.prepare("SELECT 1 FROM assignments WHERE user_id=? LIMIT 1").bind(userId).first(); },
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
    }; return (await write.prepare("SELECT role,company_id,branch_id,scope_kind,scope_node_id,valid_from,valid_until,revoked_at FROM assignments WHERE user_id=?").bind(userId).all<AssignmentRow>()).results; }
});
