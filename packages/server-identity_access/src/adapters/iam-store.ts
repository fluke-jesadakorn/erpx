import {identityVerifiedContextProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/identity-access-verified-context-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {identitySnapshotSources as peoplepayrollSnapshotSources} from '@erpx/server-people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import { createIamAuthority, type IamAuthorityMember } from "@erpx/server-identity_access/application/authority";
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import "server-only";
import type { PermissionActor, RoleRevision } from "@erpx/feature-identity_access/lib/iam";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
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
export async function readIamAuthorityFacts(db: SqlDatabase, companyId: string, userId: string, roleId: string, sessionId?: string) {
    db = iamDb(db);
    // One authoritative snapshot per invocation, never reused across requests or transactions.
    // Keep policy decisions in the application service and mutation guards in the write transaction.
    // PostgreSQL resolves every referenced relation before evaluating CASE.
    // Control has no Holding HR projections; omit those relations for Platform.
    const personFallback = companyId === 'platform' ? 'NULL::text' : `CASE WHEN q.company_id <> 'platform' THEN
            (SELECT id ${peoplepayrollSources.iam_store_hr_people_from}
             WHERE company_id=q.company_id AND user_id=q.user_id AND merged_into_id IS NULL LIMIT 1)
        END`;
    const hrAppointment = companyId === 'platform' ? 'NULL::jsonb' : `(SELECT jsonb_build_object('version',s.version)
            ${organizationFinalSources.iam_store_authority_seats_s_from}
            ${peoplepayrollSources.iam_store_hr_people_p_join}
                ON p.id=s.person_id AND p.company_id=s.company_id AND p.status='active' AND p.merged_into_id IS NULL
            ${peoplepayrollSources.iam_store_hr_engagements_e_join}
                ON e.person_id=p.id AND e.company_id=p.company_id AND e.status='active'
            WHERE q.company_id <> 'platform' AND s.company_id=q.company_id AND s.kind='hr'
                AND s.scope_id=q.company_id AND s.effective_at<=q.observed_now
                AND iam_same_person(p.user_id,q.user_id,s.company_id) LIMIT 1)`;
    const row = await db.prepare(`WITH requested AS (
        SELECT ?::text company_id, ?::text user_id, ?::text role_id, ?::text session_id, ?::bigint observed_now
    ), member AS (
        SELECT m.*,i.enabled,i.observed_at,c.status connection_status FROM requested q
        JOIN iam_members m ON m.company_id=q.company_id AND m.user_id=q.user_id
        LEFT JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id
        LEFT JOIN iam_connections c ON c.id=m.connection_id AND c.company_id=m.company_id
    ), person AS (
        SELECT COALESCE((SELECT person_id FROM member), ${personFallback}) id FROM requested q
    ) SELECT
        (SELECT jsonb_build_object('id',r.id,'name',r.name,'companyId',r.company_id,
            'revision',r.published_revision,'builtIn',r.built_in,'permissions',v.permissions_json::jsonb,
            'grantablePermissions',v.grantable_json::jsonb,'scopes',v.scopes_json::jsonb,'status',r.status)
         FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id
            AND v.role_id=r.id AND v.revision=r.published_revision
         WHERE r.company_id IN ('*',q.company_id) AND r.id=q.role_id AND r.status='active' LIMIT 1) role,
        (SELECT to_jsonb(m) FROM member m) member,
        (SELECT id FROM person) person_id,
        COALESCE((SELECT jsonb_agg(m.user_id) FROM iam_members m
            WHERE m.company_id=q.company_id AND m.person_id=(SELECT id FROM person)), '[]'::jsonb) aliases,
        (SELECT jsonb_build_object('expires_at',expires_at) FROM iam_emergency_sessions
            WHERE session_id=q.session_id AND user_id=q.user_id AND company_id=q.company_id) emergency,
        ${hrAppointment} hr_appointment
        FROM requested q`).bind(companyId, userId, roleId, sessionId ?? '', Date.now()).first<{
            role: RoleRevision | string | null;
            member: IamAuthorityMember | string | null;
            person_id: string | null;
            aliases: string[] | string;
            emergency: { expires_at: number } | string | null;
            hr_appointment: { version: number } | string | null;
        }>();
    if (!row)
        throw new ApiError(500, 'IAM_AUTHORITY_UNAVAILABLE', 'Unable to read current authority.');
    // The legacy SQL driver serializes JSON columns; decode only these declared facts.
    const decode = <T>(value: T | string): T => typeof value === 'string' ? JSON.parse(value) as T : value;
    const facts = {
        role: decode<RoleRevision | null>(row.role),
        member: decode<IamAuthorityMember | null>(row.member),
        person_id: row.person_id,
        aliases: decode<string[]>(row.aliases),
        emergency: decode<{ expires_at: number } | null>(row.emergency),
        hr_appointment: decode<{ version: number } | null>(row.hr_appointment),
    };
    return facts;
}
export async function loadIamAuthority(db: SqlDatabase, companyId: string, userId: string, roleId: string, sessionId?: string): Promise<NonNullable<PermissionActor['iam']>> {
    const facts = await readIamAuthorityFacts(db, companyId, userId, roleId, sessionId);
    return createIamAuthority({
        role: async () => facts.role,
        member: async () => facts.member,
        person: async () => facts.person_id,
        aliases: async () => facts.aliases,
        emergency: async (_companyId, _userId, _sessionId, now) =>
            facts.emergency && Number(facts.emergency.expires_at) > now ? facts.emergency : null,
        hrAppointment: async () => facts.hr_appointment,
        now: Date.now,
        error: (status, code, message) => new ApiError(status, code, message),
    })(companyId, userId, roleId, sessionId);
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
