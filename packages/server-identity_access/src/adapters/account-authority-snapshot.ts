import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { RoleRevision } from "@erpx/feature-identity_access/lib/iam";
import type { WorkspaceGrantFact } from '@erpx/server-identity_access/domain/authorization';
import type { ConnectionRow } from '@erpx/server-identity_access/public/iam-directories';
import { identitySnapshotSources } from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=identitySnapshotSources();
export type AccountAuthoritySnapshot = {
    revoked: boolean;
    stale: ConnectionRow | null;
    rows: WorkspaceGrantFact[];
    roles: Map<string,RoleRevision>;
};
/** Invocation-local authoritative gate, grants and published-role snapshot. */
export async function accountAuthoritySnapshot(write: SqlDatabase, companyId: string, userId: string, now: number, sessionId?: string): Promise<AccountAuthoritySnapshot> {
    const facts=await write.prepare(`WITH requested AS (
        SELECT ?::text company_id, ?::text user_id, ?::bigint observed_now, ?::text session_id, ?::bigint directory_cutoff
    ), revoked AS MATERIALIZED (
        SELECT EXISTS(SELECT 1 FROM requested q JOIN iam_revoked_sessions r
            ON r.company_id=q.company_id AND r.user_id=q.user_id AND r.session_id=q.session_id
            WHERE q.session_id IS NOT NULL) revoked
    ), readiness AS MATERIALIZED (
        SELECT r.revoked,(SELECT to_jsonb(c) FROM requested q JOIN iam_members m
            ON m.company_id=q.company_id AND m.user_id=q.user_id
            JOIN iam_connections c ON c.id=m.connection_id JOIN iam_identities i ON i.id=m.identity_id
            WHERE q.session_id IS NOT NULL AND NOT r.revoked AND m.status='active'
                AND m.login_policy='directory' AND c.status='active' AND i.observed_at<=q.directory_cutoff LIMIT 1) stale
        FROM revoked r
    ), grants AS MATERIALIZED (
        SELECT a.id,a.role,a.company_id,a.branch_id,a.modules_json::text modules_json,
            a.scope_kind,a.scope_node_id,a.version,a.valid_until,
            c.display_name company_name,n.display_name scope_name,b.display_name branch_name
        FROM requested q JOIN assignments a ON a.user_id=q.user_id AND a.company_id=q.company_id
        CROSS JOIN readiness ready
        ${organizationSources.authorization_organization_nodes_c_join} ON c.id=a.company_id AND c.kind='company' AND c.status='active'
        LEFT ${organizationSources.authorization_organization_nodes_n_join} ON n.id=a.scope_node_id
        LEFT ${organizationSources.authorization_organization_nodes_b_join} ON b.id=n.branch_id AND b.id!=n.id
        WHERE (q.session_id IS NULL OR (NOT ready.revoked AND ready.stale IS NULL))
            AND a.revoked_at IS NULL
            AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
            AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
            AND a.valid_from<=q.observed_now AND a.valid_until>q.observed_now
            AND (a.scope_kind='holding' OR (n.company_id=a.company_id AND n.status='active'
                AND (n.branch_id IS NULL OR EXISTS(SELECT 1 ${organizationSources.authorization_organization_nodes_b_from}
                    WHERE b.id=n.branch_id AND b.company_id=a.company_id AND b.status='active'))))
    ) SELECT gate.revoked,gate.stale,COALESCE((SELECT jsonb_agg(to_jsonb(g) ORDER BY g.role,g.id) FROM grants g),'[]'::jsonb) grants,
        COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'name',r.name,'companyId',r.company_id,
            'revision',r.published_revision,'builtIn',r.built_in,'permissions',v.permissions_json::jsonb,
            'grantablePermissions',v.grantable_json::jsonb,'scopes',v.scopes_json::jsonb,'status',r.status))
            FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id
                AND v.role_id=r.id AND v.revision=r.published_revision
            WHERE r.company_id IN ('*',q.company_id) AND r.status='active'
                AND r.id IN (SELECT role FROM grants)),'[]'::jsonb) roles FROM requested q CROSS JOIN readiness gate`)
        .bind(companyId,userId,now,sessionId??null,Date.now()-300000)
        .first<{revoked:boolean;stale:ConnectionRow|string|null;grants:WorkspaceGrantFact[]|string;roles:RoleRevision[]|string}>();
    if(!facts)throw new ApiError(500,'ACCOUNT_ACCESS_UNAVAILABLE','Unable to read current account access.');
    const rows=typeof facts.grants==='string'?JSON.parse(facts.grants) as WorkspaceGrantFact[]:facts.grants;
    const definitions=typeof facts.roles==='string'?JSON.parse(facts.roles) as RoleRevision[]:facts.roles;
    const roles=new Map<string,RoleRevision>();
    for(const role of definitions)if(!roles.has(role.id))roles.set(role.id,role);
    const stale=typeof facts.stale==='string'?JSON.parse(facts.stale) as ConnectionRow:facts.stale;
    return {revoked:facts.revoked,stale,rows,roles};
}
