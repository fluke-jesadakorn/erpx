import {identitySnapshotSources as peoplepayrollSnapshotSources} from '../../people_payroll/public/identity-snapshot-sources';
const peoplepayrollSources=peoplepayrollSnapshotSources();
import "server-only";
import type { PoolClient } from "pg";
import { type LifecycleCase, type LifecycleCaseStatus, type PeopleLifecycleData } from "@/lib/people-lifecycle";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion } from "@/server/modules/workflow/public/commands";
import { type LifecycleRoleFact } from '../../people_payroll/public/people-lifecycle-contract';
export function peopleLifecycleIdentityAccessFacts(database: SqlDatabase, client?: PoolClient) {
    return { readPeopleLifecycleDataCases(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, hrView: boolean) {
            const caseColumns = hrView ? "c.*" : "c.id,c.company_id,c.person_id,c.engagement_id,c.kind,c.status,c.version,c.effective_at,c.package_id,c.package_revision,c.updated_at,CASE WHEN c.last_error IS NOT NULL THEN 'Preparation needs attention' ELSE NULL END AS last_error";
            return database.prepare(`SELECT ${caseColumns},CASE WHEN NOT EXISTS(SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.company_id=c.company_id AND m.person_id=c.person_id AND m.status='active') THEN 'not_required' WHEN NOT EXISTS(SELECT 1 FROM iam_identities i JOIN iam_members m ON m.identity_id=i.id AND m.user_id=i.user_id AND m.company_id=i.company_id WHERE m.company_id=c.company_id AND m.person_id=c.person_id AND m.status='active' AND NOT EXISTS(SELECT 1 FROM people_directory_operations o WHERE o.company_id=c.company_id AND o.connection_id=i.connection_id AND o.status='completed' AND o.result->>'verified'='true' AND o.definition->'action'->>'kind'='account.disable' AND o.definition->'action'->>'caseId'=c.id AND o.definition->'action'->>'objectId'=i.external_id)) THEN 'verified' ELSE 'pending' END AS "directoryStatus" ${peoplepayrollSources.people_lifecycle_facts_people_lifecycle_cases_c_from} ${peoplepayrollSources.people_lifecycle_facts_hr_engagements_e_join} ON e.id=c.engagement_id AND e.company_id=c.company_id WHERE c.company_id=? AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY c.updated_at DESC,c.id`)
                .bind(companyId, branchId, branchId2, departmentId, departmentId2)
                .all<LifecycleCase | LifecycleCaseStatus>();
        },
        readPeopleLifecycleDataPreparation(now: number, companyId: string) { return database.prepare(`SELECT ap.engagement_id,ap.position_id,ap.package_id,ap.package_revision,people_application_active(ap.id,?) active,op.status,EXISTS(SELECT 1 ${peoplepayrollSources.people_lifecycle_facts_people_provision_grants_g_from} JOIN iam_reviews r ON r.id=g.review_id WHERE g.application_id=ap.id AND r.status='pending') OR EXISTS(SELECT 1 ${peoplepayrollSources.people_lifecycle_facts_people_preparation_tasks_t_from} WHERE t.application_id=ap.id AND t.status='pending') AS pending ${peoplepayrollSources.people_lifecycle_facts_people_applied_packages_ap_from} LEFT ${peoplepayrollSources.people_lifecycle_facts_people_provision_operations_op_join} ON op.id=ap.operation_id WHERE ap.company_id=? AND ap.ended_at IS NULL`).bind(now, companyId).all<{
            engagement_id: string;
            position_id: string | null;
            package_id: string;
            package_revision: number;
            active: boolean;
            status: string | null;
            pending: boolean;
        }>(); },
        readPeopleLifecycleDataGpoAccounts(companyId: string) {
            return database.prepare(`SELECT DISTINCT m.person_id,i.connection_id,i.external_id
    FROM iam_members m JOIN iam_identities i ON i.id=m.identity_id AND i.company_id=m.company_id AND i.user_id=m.user_id
    WHERE m.company_id=? AND m.status='active' AND m.person_id IS NOT NULL AND i.enabled=true`)
                .bind(companyId)
                .all<{
                person_id: string;
                connection_id: string;
                external_id: string;
            }>();
        },
        readPeopleLifecycleDataClearanceOwners(companyId: string, now: number) { return database.prepare(`SELECT a.user_id id,coalesce(max(p.legal_name),a.user_id) name,bool_or(iam_role_allows(a.role,a.company_id,'payroll.prepare') AND a.scope_kind='holding') payroll,bool_or(iam_role_allows(a.role,a.company_id,'finance.write') AND a.scope_kind='holding') finance,true equipment FROM assignments a LEFT ${peoplepayrollSources.people_lifecycle_facts_hr_people_p_join} ON p.user_id=a.user_id AND p.company_id=a.company_id AND p.merged_into_id IS NULL WHERE a.company_id=? AND iam_assignment_active(a.id,?) GROUP BY a.user_id ORDER BY name`).bind(companyId, now).all<NonNullable<PeopleLifecycleData["clearanceOwners"]>[number]>(); },
        readLinkedAccountsEffect(companyId: string, personId: string, companyId2: string, personId2: string, companyId3: string, activeOnly: boolean, value: "local" | "univention" | "active_directory" | null, value2: "local" | "univention" | "active_directory" | null, value3: string | null) {
            return database.prepare(`SELECT DISTINCT u.user_id FROM (SELECT user_id ${peoplepayrollSources.people_lifecycle_facts_hr_people_from} WHERE company_id=? AND id=? UNION SELECT user_id FROM iam_members WHERE company_id=? AND person_id=?) u LEFT JOIN iam_members m ON m.company_id=? AND m.user_id=u.user_id WHERE u.user_id IS NOT NULL AND (NOT ?::boolean OR COALESCE(m.status,'active')='active') AND (?::text IS NULL OR CASE WHEN ?='local' THEN COALESCE(m.login_policy,'local')!='directory' ELSE m.login_policy='directory' AND m.connection_id=? END) ORDER BY u.user_id`)
                .bind(companyId, personId, companyId2, personId2, companyId3, activeOnly, value, value2, value3)
                .all<{
                user_id: string;
            }>();
        },
        readPreviewLifecycleOld(companyId: string, companyId2: string, personId: string) {
            return database.prepare("SELECT a.id,a.role,a.source,a.version FROM assignments a WHERE a.company_id=? AND people_person_user(?, ?,a.user_id) AND a.revoked_at IS NULL ORDER BY a.id")
                .bind(companyId, companyId2, personId)
                .all<{
                id: string;
                role: string;
                source: string;
                version: number;
            }>();
        },
        readSaveLifecycleEffect2(companyId: string, companyId2: string, personId: string) {
            return database.prepare("SELECT 1 FROM assignments a LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id WHERE a.company_id=? AND people_person_user(?,?,a.user_id) AND a.revoked_at IS NULL AND (a.role='holding_owner' OR m.login_policy='emergency')")
                .bind(companyId, companyId2, personId)
                .first();
        },
        stageSaveLifecycleEffect2(now: number, id: string) {
            return database.prepare(`UPDATE assignments SET revoked_at=?,version=version+1 WHERE id IN (SELECT assignment_id ${peoplepayrollSources.people_lifecycle_facts_people_lifecycle_grants_from} WHERE case_id=?) AND revoked_at IS NULL`)
                .bind(now, id);
        },
        stageSaveLifecycleEffect3(companyId: string, id: string) {
            return database.prepare("UPDATE iam_reviews SET status='failed',version=version+1 WHERE company_id=? AND payload->>'caseId'=? AND kind IN ('people.lifecycle.grant','people.lifecycle.activate') AND status='pending'")
                .bind(companyId, id);
        },
        guard40(key: string, value: "protected-person", companyId: string, personId: string, userId: string, companyId2: string, companyId3: string, personId2: string) {
            return assertion(database, key, value, "NOT people_person_user(?,?,?) AND NOT EXISTS(SELECT 1 FROM assignments a LEFT JOIN iam_members m ON m.company_id=a.company_id AND m.user_id=a.user_id WHERE a.company_id=? AND people_person_user(?,?,a.user_id) AND a.revoked_at IS NULL AND (a.role='holding_owner' OR m.login_policy='emergency'))", [
                companyId,
                personId,
                userId,
                companyId2,
                companyId3,
                personId2,
            ]);
        },
        guard46(key: string, value: string, companyId: string, owner: string, value2: string | null, value3: string | null, kind: "equipment" | "directory" | "handover" | "payroll" | "finance" | "hr") { return assertion(database, key, value, "EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND (?::text IS NULL OR iam_role_allows(role,company_id,?)) AND (? NOT IN ('payroll','finance','directory') OR scope_kind='holding'))", [companyId, owner, value2, value3, kind]); },
        stageCancelLifecycleEffect3(now: number, id: string) {
            return database.prepare(`UPDATE assignments SET revoked_at=?,version=version+1 WHERE id IN (SELECT assignment_id ${peoplepayrollSources.people_lifecycle_facts_people_lifecycle_grants_from} WHERE case_id=?) AND revoked_at IS NULL`)
                .bind(now, id);
        },
        stageCancelLifecycleEffect4(id: string) {
            return database.prepare(`UPDATE iam_reviews SET status='failed',version=version+1 WHERE id IN (SELECT review_id ${peoplepayrollSources.people_lifecycle_facts_people_lifecycle_grants_from} WHERE case_id=?) AND status='pending'`)
                .bind(id);
        },
        readPeopleLifecycleWorkReviews(companyId: string) {
            return database.prepare("SELECT id,reason,created_at FROM iam_reviews WHERE company_id=? AND status='pending' ")
                .bind(companyId)
                .all<{
                id: string;
                reason: string;
                created_at: number;
            }>();
        },
        guard65(key: string, value: "it-assignee", itAssigneeId: string | null, companyId: string, itAssigneeId2: string | null) {
            return assertion(database, key, value, "?::text IS NULL OR EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint))", [itAssigneeId, companyId, itAssigneeId2]);
        },
        applyValidatePackageEffect(companyId: unknown, id: unknown) { return client!.query<Record<string, unknown>>("SELECT 1 FROM um_groups WHERE company_id=$1 AND id=$2 AND NOT (definition->>'archived')::boolean", [companyId, id]); },
        readValidatePackagerole(companyId: unknown, roleId: unknown) { return client!.query<LifecycleRoleFact>("SELECT r.published_revision,v.scopes_json FROM iam_roles r JOIN iam_role_revisions v ON v.company_id=r.company_id AND v.role_id=r.id AND v.revision=r.published_revision WHERE r.company_id IN ('*',$1) AND r.id=$2 AND r.status='active'", [companyId, roleId]); },
        applyValidatePackageEffect3(companyId: unknown, connectionId: unknown, account: unknown) { return client!.query<Record<string, unknown>>("SELECT 1 FROM iam_connections WHERE company_id=$1 AND id=$2 AND status='active' AND ($3='local' OR provider=$3)", [companyId, connectionId, account]); },
        applyValidatePackageEffect4(connectionId: unknown, id: unknown) { return client!.query<Record<string, unknown>>("SELECT 1 FROM iam_groups WHERE connection_id=$1 AND external_id=$2", [connectionId, id]); },
        applyRequestLifecycleAccessEffect(companyId: unknown, id: unknown) { return client!.query<Record<string, unknown>>("SELECT 1 FROM iam_reviews WHERE company_id=$1 AND payload->>'caseId'=$2 AND status IN ('pending','approved')", [companyId, id]); },
        applyApplyLifecycleReviewEffect2(id: unknown, value: unknown) { return client!.query<Record<string, unknown>>(`UPDATE assignments SET revoked_at=$2,version=version+1 WHERE id IN (SELECT assignment_id ${peoplepayrollSources.people_lifecycle_facts_people_lifecycle_grants_from} WHERE case_id=$1) AND revoked_at IS NULL`, [id, value]); },
        applyApplyLifecycleReviewEffect3(companyId: unknown, id: unknown) { return client!.query<Record<string, unknown>>("UPDATE iam_reviews SET status='failed',version=version+1 WHERE company_id=$1 AND payload->>'caseId'=$2 AND status='pending'", [companyId, id]); },
        applyApplyLifecycleReviewEffect5(userId: unknown, created_by: unknown, companyId: unknown) { return client!.query<{
            same: boolean;
        }>("SELECT iam_same_person($1,$2,$3) same", [userId, created_by, companyId]); },
        applyApplyLifecycleReviewEffect7(assignment: unknown, effective_at: unknown) { return client!.query<Record<string, unknown>>("UPDATE assignments SET valid_from=GREATEST(valid_from,$2) WHERE id=$1", [assignment, effective_at]); } };
}
