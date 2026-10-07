import {identityVerifiedContextProjections as organizationFinalOwnerProjections} from "../../organization/public/identity-access-verified-context-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {assertion} from "../../workflow/public/commands";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import { createIamAuthority } from "../application/authority";
import { iamDb } from "@/server/modules/identity_access/public/iam-db";
import "server-only";
import type { PermissionActor, RoleRevision } from "@/lib/iam";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
export async function iamRole(db: SqlDatabase, companyId: string, id: string): Promise<RoleRevision | null> {
    db = iamDb(db);
    const r = await db
        .prepare(`SELECT r.*,v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',?) AND r.id=? AND r.status='active'`)
        .bind(companyId, id)
        .first<{
        id: string;
        name: string;
        company_id: string;
        published_revision: number;
        built_in: boolean;
        permissions_json: string[];
        grantable_json: string[];
        scopes_json: string[];
        status: string;
    }>();
    return r
        ? {
            id: r.id,
            name: r.name,
            companyId: r.company_id,
            revision: r.published_revision,
            builtIn: r.built_in,
            permissions: r.permissions_json,
            grantablePermissions: r.grantable_json,
            scopes: r.scopes_json,
            status: r.status,
        }
        : null;
}
/** Published definitions for a set of assignments, read afresh for this request. */
export async function iamRoles(db: SqlDatabase, companyId: string, ids: readonly string[]): Promise<Map<string, RoleRevision>> {
    const unique = [...new Set(ids)];
    if (!unique.length)
        return new Map();
    const rows = (await iamDb(db).prepare(`SELECT r.id,r.name,r.company_id,r.published_revision,r.built_in,r.status,
    v.permissions_json,v.grantable_json,v.scopes_json FROM iam_roles r
    JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision
    WHERE r.company_id IN ('*',?) AND r.id IN (${unique.map(() => "?").join(",")}) AND r.status='active'`)
        .bind(companyId, ...unique).all<{
        id: string;
        name: string;
        company_id: string;
        published_revision: number;
        built_in: boolean;
        status: string;
        permissions_json: string[];
        grantable_json: string[];
        scopes_json: string[];
    }>()).results;
    const roles = new Map<string, RoleRevision>();
    for (const r of rows)
        if (!roles.has(r.id))
            roles.set(r.id, {
                id: r.id, name: r.name, companyId: r.company_id, revision: r.published_revision, builtIn: r.built_in,
                permissions: r.permissions_json, grantablePermissions: r.grantable_json, scopes: r.scopes_json, status: r.status,
            });
    return roles;
}
export async function loadIamAuthority(db: SqlDatabase, companyId: string, userId: string, roleId: string, sessionId?: string): Promise<NonNullable<PermissionActor['iam']>> {
    db = iamDb(db);
    return createIamAuthority({ role: (companyId, id) => iamRole(db, companyId, id), member: async (companyId, userId) => db
            .prepare(`SELECT m.*,i.enabled,i.observed_at,c.status connection_status FROM iam_members m LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id WHERE m.company_id=? AND m.user_id=?`)
            .bind(companyId, userId)
            .first<{
            status: string;
            login_policy: string;
            version: number;
            person_id: string | null;
            principal_id: string;
            enabled: boolean;
            observed_at: number;
            connection_status: string;
        }>(), person: async (companyId, userId) => (await db
            .prepare(`SELECT id ${peoplepayrollSources.iam_store_hr_people_from} WHERE company_id=? AND user_id=? AND merged_into_id IS NULL`)
            .bind(companyId, userId)
            .first<{
            id: string;
        }>())?.id ?? null, aliases: async (companyId, personId) => (await db
            .prepare("SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?")
            .bind(companyId, personId)
            .all<{
            user_id: string;
        }>()).results.map(row => row.user_id), emergency: async (companyId, userId, sessionId, now) => await db
            .prepare("SELECT expires_at FROM iam_emergency_sessions WHERE session_id=? AND user_id=? AND company_id=? AND expires_at>?")
            .bind(sessionId, userId, companyId, now)
            .first<{
            expires_at: number;
        }>(), hrAppointment: async (companyId, userId, now) => await db.prepare(`SELECT s.version ${organizationFinalSources.iam_store_authority_seats_s_from}
    ${peoplepayrollSources.iam_store_hr_people_p_join} ON p.id=s.person_id AND p.company_id=s.company_id AND p.status='active' AND p.merged_into_id IS NULL
    ${peoplepayrollSources.iam_store_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active'
    WHERE s.company_id=? AND s.kind='hr' AND s.scope_id=? AND s.effective_at<=? AND iam_same_person(p.user_id,?,s.company_id)
    LIMIT 1`).bind(companyId, companyId, now, userId).first<{
            version: number;
        }>(), now: Date.now, error: (status, code, message) => new ApiError(status, code, message) })(companyId, userId, roleId, sessionId);
}
/** Rechecked in the same transaction as the business mutation. */
export function iamMutationGuards(db: SqlDatabase, actor: PermissionActor & {
    userId: string;
    assignmentId: string;
    companyId: string;
    authority?: {
        sessionId: string;
    };
}, key: string): SqlStatement[] {
    db = iamDb(db);
    const iam = actor.iam;
    return [
        assertion(db,key,"iam-current-authority",`
 NOT EXISTS(SELECT 1 FROM iam_revoked_sessions WHERE session_id=? AND company_id=? AND user_id=?)
 AND (?::integer IS NULL OR EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?))
 AND iam_assignment_active(?,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND NOT EXISTS(SELECT 1 FROM iam_members m LEFT JOIN iam_identities i ON i.id=m.identity_id LEFT JOIN iam_connections c ON c.id=m.connection_id WHERE m.company_id=? AND m.user_id=? AND (m.status!='active' OR (?::integer IS NOT NULL AND m.version!=?) OR (m.login_policy='directory' AND (i.enabled IS DISTINCT FROM true OR c.status IS DISTINCT FROM 'active' OR i.observed_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint-300000))))
 AND (?::bigint IS NULL OR ?::bigint>floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND (?::integer IS NULL OR COALESCE((SELECT version FROM iam_members WHERE company_id=? AND user_id=?),0)=?)
 AND EXISTS(SELECT 1 FROM iam_roles WHERE id=? AND company_id IN ('*',?) AND status='active' AND (?::integer IS NULL OR published_revision=?))
`,[actor.authority?.sessionId ?? null,actor.companyId,actor.userId,iam?.queuedAssignmentVersion ?? null,actor.assignmentId,iam?.queuedAssignmentVersion ?? null,actor.assignmentId,actor.companyId,actor.userId,iam?.identityVersion ?? null,iam?.identityVersion ?? null,iam?.emergencyUntil ?? null,iam?.emergencyUntil ?? null,iam?.identityVersion ?? null,actor.companyId,actor.userId,iam?.identityVersion ?? null,actor.role,actor.companyId,iam?.roleRevision ?? null,iam?.roleRevision ?? null]),
        ...(iam?.hrAppointmentVersion === undefined ? [] : [assertion(db,key,"hr-appointment-current",` EXISTS(
      SELECT 1 ${organizationFinalSources.iam_store_authority_seats_s_from} ${peoplepayrollSources.iam_store_hr_people_p_join} ON p.id=s.person_id AND p.company_id=s.company_id AND p.status='active' AND p.merged_into_id IS NULL
      ${peoplepayrollSources.iam_store_hr_engagements_e_join} ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active'
      WHERE s.company_id=? AND s.kind='hr' AND s.scope_id=? AND s.version=? AND s.effective_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint
      AND iam_same_person(p.user_id,?,s.company_id))`,[actor.companyId,actor.companyId,iam.hrAppointmentVersion,actor.userId])]),
    ];
}
