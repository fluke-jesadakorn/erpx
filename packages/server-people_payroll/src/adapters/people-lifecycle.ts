import {projectWorkerNativeCommands,requireProjectWorkerNativeCommand,requireProjectWorkerUnit} from '@erpx/server-people_payroll/adapters/project-worker-startup-binding';
import {filterPeopleLifecycleWork} from "@erpx/server-people_payroll/adapters/record-purchases";
import {peoplePackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {preparePeopleIamCommand} from "@erpx/server-people_payroll/adapters/people-iam-purchases";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {withPeoplePurchasedRead,withPeoplePurchasedCommand,peoplePurchaseControls} from "@erpx/server-people_payroll/adapters/people-purchases";
import {peopleLifecycleNotificationEffects} from '@erpx/server-collaboration/public/people-lifecycle-notification-effects';
import {peopleLifecycleAuditEffects} from '@erpx/server-workflow/public/people-lifecycle-audit-effects';
import {peoplePayrollFinalOwnerProjections as identityaccessFinalOwnerProjections} from "@erpx/server-identity_access/public/people-payroll-final-owner-projections";
const identityaccessFinalSources=identityaccessFinalOwnerProjections();
import { eligibleClearanceOwner } from "@erpx/server-people_payroll/public/retirement-clearance";
import { effectiveGpoRules } from "@erpx/server-identity_access/public/gpo-rules";
import "server-only";
import { calendarCommand } from "@erpx/server-work_projection/public/calendar/commands";
import { createHash } from "node:crypto";
import type { PoolClient } from "pg";
import { z } from "zod";
import { packageInput, type AccessPackage, type AccessPackageDefinition, type LifecycleCase, type LifecycleInput, type PeopleLifecycleData, type PreparationTask } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { iamDb } from "@erpx/server-identity_access/public/iam-db";
import { iamActorFromWork, type IamActor } from "@erpx/server-identity_access/public/iam-context";
import { iamCommand, assertIamOwnerTransaction } from "@erpx/server-identity_access/public/iam-transactions";
import { requireFreshMfa } from "@erpx/server-identity_access/public/iam-mfa";
import { grantInput, proposeIamReview } from "@erpx/server-identity_access/public/iam-service";
import {validateIamGrant,insertIamGrant} from '@erpx/server-identity_access/public/iam-grants';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { organizationGuard, validateStaffOrganization } from "@erpx/server-organization/public/organization";
import type { SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { lifecycleSettingsInput, rehireInput, type LifecycleGrantFact, type LifecyclePackageRevisionFact, type LifecyclePersonFact } from '@erpx/server-people_payroll/domain/people-lifecycle';
import { createPeopleLifecycleApplication, type PeopleLifecycleRepository, type PeopleLifecyclePorts } from '@erpx/server-people_payroll/application/people-lifecycle';
import { BusinessRuleError } from '@erpx/server-shared/index';
import { peopleLifecycleOrganizationFacts } from '@erpx/server-organization/public/people-lifecycle-facts';
import { peopleLifecycleIdentityAccessFacts } from '@erpx/server-identity_access/public/people-lifecycle-facts';
import { peopleLifecycleWorkflowFacts } from '@erpx/server-workflow/public/people-lifecycle-facts';
export { lifecycleSettingsInput, rehireInput } from '@erpx/server-people_payroll/domain/people-lifecycle';
function clientDatabase(c: PoolClient): SqlDatabase {
    return {
        prepare(sql: string) {
            const make = (args: unknown[]) => ({
                bind: (...v: unknown[]) => make(v),
                all: async () => {
                    index = 0;
                    return {
                        results: (await c.query(sql.replace(/\?/g, () => `$${++index}`), args)).rows,
                    };
                },
                first: async () => {
                    index = 0;
                    return ((await c.query(sql.replace(/\?/g, () => `$${++index}`), args)).rows[0] ?? null);
                },
            });
            let index = 0;
            return make([]);
        },
    } as unknown as SqlDatabase;
}
function applicationPorts(raw: SqlDatabase, client?: PoolClient): PeopleLifecyclePorts<SqlStatement> {
    const database = iamDb(client ? clientDatabase(client) : raw);
    const repository: PeopleLifecycleRepository<SqlStatement> = { readLegacyCaseForPlacementRow(companyId: string, personId: string) {
            return database.prepare(`SELECT c.id,c.version,c.status,c.kind,c.effective_at,e.branch_id,e.department_id
    FROM people_lifecycle_cases c JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
    WHERE c.company_id=? AND c.person_id=? AND c.workflow='legacy' AND c.status IN ('waiting_it','pending_review','scheduled')`)
                .bind(companyId, personId)
                .first<{
                id: string;
                version: number;
                status: string;
                kind: LifecycleCase["kind"];
                effective_at: number;
                branch_id: string | null;
                department_id: string | null;
            }>();
        },
        readPeopleLifecycleDataPositions(...args) { return peopleLifecycleOrganizationFacts(database, client).readPeopleLifecycleDataPositions(...args); },
        readPeopleLifecycleDataAllPackages(companyId: string) {
            return database.prepare("SELECT * FROM people_access_packages WHERE company_id=? ORDER BY definition->>'name',id")
                .bind(companyId)
                .all<AccessPackage>();
        },
        readPeopleLifecycleDataCases(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readPeopleLifecycleDataCases(...args); },
        readPeopleLifecycleDataTasks(companyId: string, hrView: boolean) {
            return database.prepare(hrView
                ? "SELECT * FROM people_preparation_tasks WHERE company_id=? ORDER BY updated_at DESC,id"
                : "SELECT id,position_id,case_id,kind,title,status,assignee_id,version,updated_at,application_id,clearance_type FROM people_preparation_tasks WHERE company_id=? AND (clearance_type IS NULL OR clearance_type='directory') ORDER BY updated_at DESC,id")
                .bind(companyId)
                .all<PreparationTask>();
        },
        readPeopleLifecycleDataSettings(companyId: string) {
            return database.prepare("SELECT version,definition FROM people_lifecycle_settings WHERE company_id=?")
                .bind(companyId)
                .first<{
                version: number;
                definition: Omit<PeopleLifecycleData["settings"], "version">;
            }>();
        },
        readPeopleLifecycleDataRoster(companyId: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null) {
            return database.prepare(`SELECT p.id,p.legal_name "name",e.id "engagementId",e.position_id "positionId",e.manager_person_id "managerPersonId",e.status FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.merged_into_id IS NULL AND e.worker_type='employee' AND (e.status!='ended' OR EXISTS(SELECT 1 FROM people_lifecycle_cases lc WHERE lc.engagement_id=e.id AND lc.company_id=e.company_id)) AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) ORDER BY p.legal_name,e.id`)
                .bind(companyId, branchId, branchId2, departmentId, departmentId2)
                .all<PeopleLifecycleData["roster"][number]>();
        },
        readPeopleLifecycleDataPreparation(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readPeopleLifecycleDataPreparation(...args); },
        readPeopleLifecycleDataGpoAccounts(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readPeopleLifecycleDataGpoAccounts(...args); },
        readPeopleLifecycleDataGpoJobs(companyId: string) {
            return database.prepare(`SELECT j.position_id,j.person_id,j.engagement_id,j.account_id,j.status,j.connection_id,
      COALESCE(j.rule_id,'legacy-position:'||j.position_id) rule_id
    ${identityaccessFinalSources.people_lifecycle_position_gpo_jobs_j_from} ${identityaccessFinalSources.people_lifecycle_organization_gpo_rules_r_join} ON r.company_id=j.company_id
      AND r.id=COALESCE(j.rule_id,'legacy-position:'||j.position_id) AND r.revision=j.revision AND r.active
    WHERE j.company_id=? AND j.desired_action='add' AND j.status<>'superseded'`)
                .bind(companyId)
                .all<{
                position_id: string;
                person_id: string;
                engagement_id: string | null;
                account_id: string | null;
                status: string;
                connection_id: string;
                rule_id: string;
            }>();
        },
        readPeopleLifecycleDataManagers(companyId: string) {
            return database.prepare("SELECT DISTINCT p.id,p.legal_name name FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND e.worker_type='employee' AND e.status='active' ORDER BY p.legal_name,p.id")
                .bind(companyId)
                .all<{
                id: string;
                name: string;
            }>();
        },
        readPeopleLifecycleDataNodes(...args) { return peopleLifecycleOrganizationFacts(database, client).readPeopleLifecycleDataNodes(...args); },
        readPeopleLifecycleDataClearanceOwners(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readPeopleLifecycleDataClearanceOwners(...args); },
        readPackageImpactPositions(...args) { return peopleLifecycleOrganizationFacts(database, client).readPackageImpactPositions(...args); },
        readPackageImpactPeople(companyId: string, id: string) {
            return database.prepare("SELECT e.id,e.version,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id JOIN people_position_packages b ON b.company_id=e.company_id AND b.position_id=e.position_id WHERE b.company_id=? AND b.package_id=? AND e.status!='ended' ORDER BY e.id")
                .bind(companyId, id)
                .all<{
                id: string;
                version: number;
                legal_name: string;
            }>();
        },
        readPackageImpactSnapshots(companyId: string, id: string) {
            return database.prepare("SELECT definition FROM people_package_revisions WHERE company_id=? AND package_id=? ORDER BY revision")
                .bind(companyId, id)
                .all<{
                definition: AccessPackageDefinition;
            }>();
        },
        readPreviewPackageCurrent(companyId: string, id: string) {
            return database.prepare("SELECT version,definition FROM people_access_packages WHERE company_id=? AND id=?")
                .bind(companyId, id)
                .first();
        },
        readEngagementRow(engagementId: string, personId: string, companyId: string) {
            return database.prepare("SELECT e.*,p.legal_name,p.user_id,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.id=? AND e.person_id=? AND e.company_id=? AND p.merged_into_id IS NULL")
                .bind(engagementId, personId, companyId)
                .first<{
                id: string;
                person_id: string;
                version: number;
                branch_id: string | null;
                department_id: string | null;
                position_id: string | null;
                level_id: string | null;
                manager_person_id: string | null;
                job_title: string;
                status: string;
                legal_name: string;
                user_id: string | null;
                person_status: string;
                start_date: string;
                end_date: string | null;
            }>();
        },
        readLinkedAccountsEffect(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readLinkedAccountsEffect(...args); },
        readPreviewLifecyclePosition(...args) { return peopleLifecycleOrganizationFacts(database, client).readPreviewLifecyclePosition(...args); },
        readPreviewLifecycleRecommendedPay(...args) { return peopleLifecycleOrganizationFacts(database, client).readPreviewLifecycleRecommendedPay(...args); },
        readPreviewLifecycleRecommendedLeave(...args) { return peopleLifecycleOrganizationFacts(database, client).readPreviewLifecycleRecommendedLeave(...args); },
        readPreviewLifecycleRevision(companyId: string, package_id: string, revision: number | null) {
            return database.prepare("SELECT definition FROM people_package_revisions WHERE company_id=? AND package_id=? AND revision=?")
                .bind(companyId, package_id, revision)
                .first<{
                definition: AccessPackageDefinition;
            }>();
        },
        readPreviewLifecycleEffect(companyId: string, positionId: string | null) {
            return database.prepare("SELECT 1 FROM people_preparation_tasks WHERE company_id=? AND position_id=? AND kind='position' AND status='completed'")
                .bind(companyId, positionId)
                .first();
        },
        readPreviewLifecycleOld(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readPreviewLifecycleOld(...args); },
        readSaveLifecycleCurrent(id: string, companyId: string) {
            return database.prepare("SELECT * FROM people_lifecycle_cases WHERE id=? AND company_id=?")
                .bind(id, companyId)
                .first<LifecycleCase>();
        },
        readSaveLifecycleEffect(companyId: string, engagementId: string, value: string) {
            return database.prepare("SELECT 1 FROM people_lifecycle_cases WHERE company_id=? AND engagement_id=? AND kind IN ('onboarding','transfer') AND status IN ('scheduled','completed') AND id<>?")
                .bind(companyId, engagementId, value)
                .first();
        },
        readSaveLifecycleEffect2(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readSaveLifecycleEffect2(...args); },
        readSaveLifecycleBinding(companyId: string, positionId: string) {
            return database.prepare("SELECT package_id,revision FROM people_position_packages WHERE company_id=? AND position_id=?")
                .bind(companyId, positionId)
                .first<{
                package_id: string;
                revision: number;
            }>();
        },
        readSaveLifecyclePosition(...args) { return peopleLifecycleOrganizationFacts(database, client).readSaveLifecyclePosition(...args); },
        guard31(key: string, value: "legacy-case-version", id: string, companyId: string, personId: string, version: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM people_lifecycle_cases WHERE id=? AND company_id=? AND person_id=? AND workflow='legacy' AND kind IN ('onboarding','transfer') AND version=? AND status IN ('waiting_it','pending_review','scheduled') AND (status!='scheduled' OR effective_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))", [id, companyId, personId, version]);
        },
        stageSaveLifecycleEffect(now: number, id: string, companyId: string) {
            return database.prepare("UPDATE people_lifecycle_cases SET status='cancelled',version=version+1,updated_at=? WHERE id=? AND company_id=?")
                .bind(now, id, companyId);
        },
        stageSaveLifecycleEffect2(...args) { return peopleLifecycleIdentityAccessFacts(database, client).stageSaveLifecycleEffect2(...args); },
        stageSaveLifecycleEffect3(...args) { return peopleLifecycleIdentityAccessFacts(database, client).stageSaveLifecycleEffect3(...args); },
        stageSaveLifecycleEffect4(now: number, companyId: string, id: string) {
            return database.prepare("UPDATE people_preparation_tasks SET status='cancelled',version=version+1,updated_at=? WHERE company_id=? AND case_id=? AND status='pending'")
                .bind(now, companyId, id);
        },
        ...peopleLifecycleAuditEffects(database),
        guard37(key: string, value: "case-version", value2: (string | number)[], current: boolean) {
            return assertion(database, key, value, current
                ? "EXISTS(SELECT 1 FROM people_lifecycle_cases WHERE id=? AND version=? AND status IN ('draft','waiting_it'))"
                : "?=0", value2);
        },
        stageSaveLifecycleEffect6(id: string, companyId: string, personId: string, engagementId: string, kind: "onboarding" | "transfer" | "offboarding" | "suspend", state: "draft" | "scheduled" | "pending_review" | "waiting_it", version: number, effectiveAt: number, value: string | null, value2: number | null, value3: string, userId: string, now: number) {
            return database.prepare("INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,package_id,package_revision,definition,created_by,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET status=excluded.status,version=excluded.version,effective_at=excluded.effective_at,package_id=excluded.package_id,package_revision=excluded.package_revision,definition=excluded.definition,updated_at=excluded.updated_at")
                .bind(id, companyId, personId, engagementId, kind, state, version, effectiveAt, value, value2, value3, userId, now);
        },
        stageSaveLifecycleEffect7(value: string, assignmentId: string, value2: "legacy" | "placement", id: string) {
            return database.prepare("UPDATE people_lifecycle_cases SET review_snapshot=?,authorizing_assignment_id=?,workflow=? WHERE id=?")
                .bind(value, assignmentId, value2, id);
        },
        guard40(...args) { return peopleLifecycleIdentityAccessFacts(database, client).guard40(...args); },
        stageSaveLifecycleEffect8(companyId: string, personId: string, id: string, effectiveAt: number) {
            return database.prepare("INSERT INTO people_access_cutoffs(company_id,person_id,case_id,effective_at) VALUES(?,?,?,?)")
                .bind(companyId, personId, id, effectiveAt);
        },
        guard42(key: string, value: "prepared-package", companyId: string, value2: string, package_id: string, revision: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM people_position_packages WHERE company_id=? AND position_id=? AND package_id=? AND revision=?)", [
                companyId,
                value2,
                package_id,
                revision,
            ]);
        },
        stageSaveLifecycleEffect9(value: "retirement" | "other", id: string, companyId: string) { return database.prepare("UPDATE people_lifecycle_cases SET departure_reason=? WHERE id=? AND company_id=?").bind(value, id, companyId); },
        readSaveLifecycleSettings(companyId: string) {
            return database.prepare("SELECT definition FROM people_lifecycle_settings WHERE company_id=?")
                .bind(companyId)
                .first<{
                definition: PeopleLifecycleData["settings"];
            }>();
        },
        readSaveLifecycleManager(engagementId: string, companyId: string) { return database.prepare("SELECT p.user_id FROM hr_engagements e JOIN hr_people p ON p.id=e.manager_person_id AND p.company_id=e.company_id WHERE e.id=? AND e.company_id=? AND p.status='active' AND p.merged_into_id IS NULL").bind(engagementId, companyId).first<{
            user_id: string | null;
        }>(); },
        guard46(...args) { return peopleLifecycleIdentityAccessFacts(database, client).guard46(...args); },
        stageSaveLifecycleEffect10(taskId: string, companyId: string, id: string, value: string, owner: string | null, now: number, value2: boolean, lastWorkingDate: string | null, kind: "equipment" | "directory" | "handover" | "payroll" | "finance" | "hr", kind2: "equipment" | "directory" | "handover" | "payroll" | "finance" | "hr") { return database.prepare("INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,assignee_id,updated_at,privileged,due_date,responsibility,clearance_type) VALUES(?,?,?,'lifecycle',?,?,?,?,?,?,?)").bind(taskId, companyId, id, value, owner, now, value2, lastWorkingDate, kind, kind2); },
        ...peopleLifecycleNotificationEffects(database),
        stageSaveLifecycleEffect12(value: string, companyId: string, id: string, title: string, value2: string | null, now: number, privileged: boolean) {
            return database.prepare("INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,assignee_id,updated_at,privileged) VALUES(?,?,?,'lifecycle',?,?,?,?) ON CONFLICT(id) DO NOTHING")
                .bind(value, companyId, id, title, value2, now, privileged);
        },
        readCancelLifecycleRow(companyId: string, id: string) {
            return database.prepare("SELECT * FROM people_lifecycle_cases WHERE company_id=? AND id=?")
                .bind(companyId, id)
                .first<LifecycleCase>();
        },
        guard51(key: string, value: "case-version", id: string, companyId: string, expectedVersion: number) {
            return assertion(database, key, value, "EXISTS(SELECT 1 FROM people_lifecycle_cases WHERE id=? AND company_id=? AND version=? AND status NOT IN ('completed','cancelled') AND (status!='scheduled' OR effective_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))", [id, companyId, expectedVersion]);
        },
        stageCancelLifecycleEffect(now: number, id: string) {
            return database.prepare("UPDATE people_lifecycle_cases SET status='cancelled',version=version+1,updated_at=? WHERE id=?")
                .bind(now, id);
        },
        stageCancelLifecycleEffect2(now: number, id: string) {
            return database.prepare("UPDATE people_access_cutoffs SET cancelled_at=? WHERE case_id=?")
                .bind(now, id);
        },
        stageCancelLifecycleEffect3(...args) { return peopleLifecycleIdentityAccessFacts(database, client).stageCancelLifecycleEffect3(...args); },
        stageCancelLifecycleEffect4(...args) { return peopleLifecycleIdentityAccessFacts(database, client).stageCancelLifecycleEffect4(...args); },
        stageCancelLifecycleEffect5(id: string) {
            return database.prepare("UPDATE people_preparation_tasks SET status='cancelled',version=version+1 WHERE case_id=? AND status='pending'")
                .bind(id);
        },
        readPeopleLifecycleWorkReviews(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readPeopleLifecycleWorkReviews(...args); },
        readPeopleLifecycleWorkOperations(companyId: string) {
            return database.prepare(`SELECT id,status,definition,last_error,updated_at,created_by ${identityaccessFinalSources.people_lifecycle_people_directory_operations_from} WHERE company_id=? AND status NOT IN ('completed','cancelled','rejected') ORDER BY created_at`)
                .bind(companyId)
                .all<{
                id: string;
                status: string;
                definition: {
                    reason: string;
                    action: {
                        kind: string;
                    };
                };
                last_error: string | null;
                updated_at: number;
                created_by: string;
            }>();
        },
        readCreateRehireEngagementOld(companyId: string, engagementId: string, personId: string) {
            return database.prepare("SELECT e.*,p.status person_status FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE e.company_id=? AND e.id=? AND e.person_id=? AND p.merged_into_id IS NULL")
                .bind(companyId, engagementId, personId)
                .first<{
                version: number;
                status: string;
                person_status: string;
                branch_id: string | null;
                department_id: string | null;
                worker_type: string;
                employment_type: string;
                currency: string;
            }>();
        },
        readCreateRehireEngagementPosition(...args) { return peopleLifecycleOrganizationFacts(database, client).readCreateRehireEngagementPosition(...args); },
        guard61(key: string, value: "new-engagement", companyId: string, personId: string) {
            return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_engagements WHERE company_id=? AND person_id=? AND status!='ended')", [companyId, personId]);
        },
        stageCreateRehireEngagementEffect(id: string, personId: string, companyId: string, value: string | null, value2: string | null, positionId: string, level_id: string, worker_type: string, employment_type: string, startDate: string, title: string, currency: string, userId: string, now: number, now2: number) {
            return database.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,position_id,level_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'onboarding',?,?,?,?,?,?)")
                .bind(id, personId, companyId, value, value2, positionId, level_id, worker_type, employment_type, startDate, title, currency, userId, now, now2);
        },
        stageCreateRehireEngagementEffect2(now: number, companyId: string, personId: string) {
            return database.prepare("UPDATE hr_people SET status='active',version=version+1,updated_at=? WHERE company_id=? AND id=?")
                .bind(now, companyId, personId);
        },
        guard64(key: string, value: "settings-version", companyId: string, expectedVersion: number) {
            return assertion(database, key, value, "COALESCE((SELECT version FROM people_lifecycle_settings WHERE company_id=?),0)=?", [companyId, expectedVersion]);
        },
        guard65(...args) { return peopleLifecycleIdentityAccessFacts(database, client).guard65(...args); },
        stageSaveLifecycleSettingsEffect(companyId: string, value: number, value2: string) {
            return database.prepare("INSERT INTO people_lifecycle_settings VALUES(?,?,?) ON CONFLICT(company_id) DO UPDATE SET version=excluded.version,definition=excluded.definition")
                .bind(companyId, value, value2);
        },
        stageSaveLifecycleSettingsEffect2(itAssigneeId: string | null, companyId: string) {
            return database.prepare("UPDATE people_preparation_tasks SET assignee_id=?,version=version+1 WHERE company_id=? AND assignee_id IS NULL AND status='pending' AND clearance_type IS NULL")
                .bind(itAssigneeId, companyId);
        },
        applyValidatePackageEffect(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyValidatePackageEffect(...args); },
        readValidatePackagerole(...args) { return peopleLifecycleIdentityAccessFacts(database, client).readValidatePackagerole(...args); },
        applyValidatePackageEffect2(...args) { return peopleLifecycleOrganizationFacts(database, client).applyValidatePackageEffect2(...args); },
        applyValidatePackageEffect3(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyValidatePackageEffect3(...args); },
        applyValidatePackageEffect4(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyValidatePackageEffect4(...args); },
        readSavePackagecurrent(companyId: unknown, id: unknown) { return client!.query<AccessPackage>("SELECT * FROM people_access_packages WHERE company_id=$1 AND id=$2 FOR UPDATE", [companyId, id]); },
        applySavePackageEffect(companyId: unknown, id: unknown, version: unknown, revision: unknown, value: unknown) { return client!.query<Record<string, unknown>>("INSERT INTO people_access_packages(company_id,id,version,published_revision,definition) VALUES($1,$2,$3,$4,$5) ON CONFLICT(company_id,id) DO UPDATE SET version=excluded.version,published_revision=excluded.published_revision,definition=excluded.definition", [companyId, id, version, revision, value]); },
        applySavePackageEffect2(companyId: unknown, id: unknown, revision: unknown, value: unknown, userId: unknown, value2: unknown) { return client!.query<Record<string, unknown>>("INSERT INTO people_package_revisions VALUES($1,$2,$3,$4,$5,$6)", [companyId, id, revision, value, userId, value2]); },
        applySavePackageEffect3(companyId: unknown, id: unknown, revision: unknown) { return client!.query<Record<string, unknown>>("UPDATE people_position_packages SET revision=$3 WHERE company_id=$1 AND package_id=$2", [companyId, id, revision]); },
        applyLifecycleOutboxEffect(...args) { return peopleLifecycleWorkflowFacts(database, client).applyLifecycleOutboxEffect(...args); },
        readBindPositionPackageposition(...args) { return peopleLifecycleOrganizationFacts(database, client).readBindPositionPackageposition(...args); },
        readBindPositionPackagepkg(companyId: unknown, packageId: unknown, revision: unknown) { return client!.query<LifecyclePackageRevisionFact>("SELECT r.* FROM people_package_revisions r JOIN people_access_packages p ON p.company_id=r.company_id AND p.id=r.package_id AND p.published_revision=r.revision WHERE r.company_id=$1 AND r.package_id=$2 AND r.revision=$3", [companyId, packageId, revision]); },
        readBindPositionPackagepeople(companyId: unknown, id: unknown) { return client!.query<LifecyclePersonFact>("SELECT p.legal_name,e.id,e.version FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=$1 AND e.position_id=$2 AND e.status!='ended' ORDER BY e.id", [companyId, id]); },
        applyBindPositionPackageEffect(companyId: unknown, id: unknown, packageId: unknown, revision: unknown) { return client!.query<Record<string, unknown>>("INSERT INTO people_position_packages VALUES($1,$2,$3,$4) ON CONFLICT(company_id,position_id) DO UPDATE SET package_id=excluded.package_id,revision=excluded.revision", [companyId, id, packageId, revision]); },
        applyBindPositionPackageEffect2(...args) { return peopleLifecycleOrganizationFacts(database, client).applyBindPositionPackageEffect2(...args); },
        applyBindPositionPackageEffect3(companyId: unknown, id: unknown, value: unknown) { return client!.query<Record<string, unknown>>("UPDATE people_preparation_tasks SET status='completed',version=version+1,updated_at=$3 WHERE company_id=$1 AND position_id=$2 AND kind='position'", [companyId, id, value]); },
        readRequestLifecycleAccessrow(companyId: unknown, id: unknown) { return client!.query<LifecycleCase>("SELECT * FROM people_lifecycle_cases WHERE company_id=$1 AND id=$2 FOR UPDATE", [companyId, id]); },
        applyRequestLifecycleAccessEffect(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyRequestLifecycleAccessEffect(...args); },
        readRequestLifecycleAccessdefinition(companyId: unknown, package_id: unknown, package_revision: unknown) { return client!.query<LifecyclePackageRevisionFact>("SELECT definition FROM people_package_revisions WHERE company_id=$1 AND package_id=$2 AND revision=$3", [companyId, package_id, package_revision]); },
        applyRequestLifecycleAccessEffect2(grantId: unknown, companyId: unknown, id: unknown, userId: unknown, index: unknown, value: unknown, id2: unknown) { return client!.query<Record<string, unknown>>("INSERT INTO people_lifecycle_grants(id,company_id,case_id,user_id,grant_index,definition,review_id) VALUES($1,$2,$3,$4,$5,$6,$7)", [grantId, companyId, id, userId, index, value, id2]); },
        applyRequestLifecycleAccessEffect3(id: unknown, value: unknown) { return client!.query<Record<string, unknown>>("UPDATE people_lifecycle_cases SET version=version+1,updated_at=$2 WHERE id=$1", [id, value]); },
        readApplyLifecycleReviewrow(companyId: unknown, caseId: unknown) { return client!.query<LifecycleCase>("SELECT * FROM people_lifecycle_cases WHERE company_id=$1 AND id=$2 AND status='pending_review' FOR UPDATE", [companyId, caseId]); },
        applyApplyLifecycleReviewEffect(id: unknown, value: unknown) { return client!.query<Record<string, unknown>>("UPDATE people_lifecycle_cases SET status='blocked',last_error='ACCESS_REVIEW_REJECTED',version=version+1,updated_at=$2 WHERE id=$1", [id, value]); },
        applyApplyLifecycleReviewEffect2(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyApplyLifecycleReviewEffect2(...args); },
        applyApplyLifecycleReviewEffect3(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyApplyLifecycleReviewEffect3(...args); },
        applyApplyLifecycleReviewEffect4(companyId: unknown, person_id: unknown, userId: unknown) { return client!.query<{
            same: boolean;
            ok: boolean;
        }>("SELECT people_person_user($1,$2,$3) same", [companyId, person_id, userId]); },
        applyApplyLifecycleReviewEffect5(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyApplyLifecycleReviewEffect5(...args); },
        readApplyLifecycleReviewg(grantId: unknown, companyId: unknown, id: unknown) { return client!.query<LifecycleGrantFact>("SELECT * FROM people_lifecycle_grants WHERE id=$1 AND company_id=$2 AND case_id=$3 FOR UPDATE", [grantId, companyId, id]); },
        applyApplyLifecycleReviewEffect6(companyId: unknown, person_id: unknown, userId: unknown) { return client!.query<{
            same: boolean;
            ok: boolean;
        }>("SELECT people_person_user($1,$2,$3) ok", [companyId, person_id, userId]); },
        applyApplyLifecycleReviewEffect7(...args) { return peopleLifecycleIdentityAccessFacts(database, client).applyApplyLifecycleReviewEffect7(...args); },
        applyApplyLifecycleReviewEffect8(id: unknown, assignment: unknown) { return client!.query<Record<string, unknown>>("UPDATE people_lifecycle_grants SET assignment_id=$2 WHERE id=$1", [id, assignment]); },
        applyApplyLifecycleReviewEffect9(id: unknown) { return client!.query<Record<string, unknown>>("SELECT 1 FROM people_lifecycle_grants WHERE case_id=$1 AND assignment_id IS NULL", [id]); },
        applyApplyLifecycleReviewEffect10(id: unknown, value: unknown) { return client!.query<Record<string, unknown>>("UPDATE people_lifecycle_cases SET status='scheduled',version=version+1,updated_at=$2 WHERE id=$1", [id, value]); } };
    return { repository, clock: { now: Date.now }, ids: { newId: () => crypto.randomUUID() }, authorize: requirePermission, commands: projectWorkerNativeCommands(database), digest: value => createHash('sha256').update(JSON.stringify(value)).digest('hex'), grantInput, iamCommand: (actor, key, operation, input, permission, run) => fromNative(() => iamCommand(iamDb(raw) as PostgresDatabase, actor, key, operation, input, permission, c => run(applicationPorts(raw, c)),c=>preparePeopleIamCommand(c,actor,key,operation,input))), effectiveGpoRules: (...args) => fromNative(() => effectiveGpoRules(iamDb(raw) as PostgresDatabase, ...args)), eligibleClearanceOwner: (...args) => fromNative(() => eligibleClearanceOwner(database, ...args)), iamActorFromWork: (...args) => iamActorFromWork(...args), requireFreshMfa: (...args) => fromNative(() => requireFreshMfa(...args)), validateIamGrant: (...args) => fromNative(() => validateIamGrant(client!, ...args)), insertIamGrant: (...args) => fromNative(() => insertIamGrant(client!, ...args)), proposeIamReview: (...args) => fromNative(() => proposeIamReview(client!, ...args)), organizationGuard: (...args) => fromNative(() => organizationGuard(database, ...args)), validateStaffOrganization: (...args) => fromNative(() => validateStaffOrganization(database, ...args)), calendarCommand: (...args) => fromNative(() => calendarCommand(iamDb(raw) as PostgresDatabase, ...args)) };
}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> { try {
    return await run();
}
catch (error) {
    if (error instanceof ApiError)
        throw new BusinessRuleError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function peopleLifecycleData(raw: SqlDatabase, actor: WorkContext) {
 const data=hasPermission(actor,"people.manage")?await withPeoplePurchasedRead(actor,"people.lifecycle",(tx,current)=>invoke(()=>createPeopleLifecycleApplication(applicationPorts(tx)).peopleLifecycleData(current)),"people.manage",raw):await withPeoplePurchasedRead(actor,"foundation.identity",(tx,current)=>invoke(()=>createPeopleLifecycleApplication(applicationPorts(tx)).peopleLifecycleData(current)),undefined,raw);
 return {...data,purchases:await peoplePurchaseControls(actor,raw)};
}
export async function previewPackage(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof packageInput>) { return withPeoplePurchasedRead(actor,"foundation.identity",(tx,current)=>invoke(() => createPeopleLifecycleApplication(applicationPorts(tx)).previewPackage(current, input)),"iam.directory.manage",db); }
export async function savePackage(db: PostgresDatabase, actor: WorkContext, input: z.infer<typeof packageInput>, key: string, publish: boolean, token?: string) { return invoke(() => createPeopleLifecycleApplication(applicationPorts(db)).savePackage(actor, input, key, publish, token)); }
export async function bindPositionPackage(db: PostgresDatabase, actor: WorkContext, input: {
    positionId: string;
    packageId: string;
    revision: number;
    expectedVersion: number;
    previewToken?: string;
}, key: string, previewOnly = false) { return invoke(() => createPeopleLifecycleApplication(applicationPorts(db)).bindPositionPackage(actor, input, key, previewOnly)); }
export async function previewLifecycle(raw: SqlDatabase, actor: WorkContext, input: LifecycleInput, placementOnly = false) { return withPeoplePurchasedRead(actor,"people.lifecycle",(tx,current)=>invoke(()=>createPeopleLifecycleApplication(applicationPorts(tx)).previewLifecycle(current,input,placementOnly)),"people.manage",raw); }
export async function saveLifecycle(db: SqlDatabase, actor: WorkContext, input: LifecycleInput, key: string, submit: boolean, token?: string, placementOnly = false, supersedeLegacyCase?: {
    id: string;
    expectedVersion: number;
}) { return withPeoplePurchasedCommand(actor,{key,operation:submit?"people.lifecycle.submit":"people.lifecycle.draft",capability:"people.lifecycle",purpose:["offboarding","suspend"].includes(input.kind)?"settle-existing":"new-work",permission:"people.manage",resultKind:"lifecycle",target:{kind:"engagement",id:input.engagementId}},tx=>invoke(()=>createPeopleLifecycleApplication(applicationPorts(tx)).saveLifecycle(actor,input,key,submit,token,placementOnly,supersedeLegacyCase)),db); }
export async function cancelLifecycle(db: SqlDatabase, actor: WorkContext, id: string, expectedVersion: number, key: string) { return withPeoplePurchasedCommand(actor,{key,operation:"people.lifecycle.cancel",capability:"people.lifecycle",purpose:"settle-existing",permission:"people.manage",target:{kind:"lifecycle",id}},tx=>invoke(()=>createPeopleLifecycleApplication(applicationPorts(tx)).cancelLifecycle(actor,id,expectedVersion,key)),db); }
export async function requestLifecycleAccess(db: PostgresDatabase, actor: WorkContext, id: string, version: number, key: string) { return invoke(() => createPeopleLifecycleApplication(applicationPorts(db)).requestLifecycleAccess(actor, id, version, key)); }
export async function applyLifecycleReview(c: PoolClient, actor: IamActor, kind: string, p: Record<string, unknown>, decision: "approve" | "reject" = "approve") { if(await peoplePackageEnforcementMode()!=="disabled")assertIamOwnerTransaction(c); return invoke(() => createPeopleLifecycleApplication(applicationPorts(undefined!, c)).applyLifecycleReview(actor, kind, p, decision)); }
export async function peopleLifecycleWork(db: SqlDatabase, actor: WorkContext) { return filterPeopleLifecycleWork(db,actor,await invoke(() => createPeopleLifecycleApplication(applicationPorts(db)).peopleLifecycleWork(actor))); }
export async function createRehireEngagement(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof rehireInput>, key: string) { return withPeoplePurchasedCommand(actor,{key,operation:"people.lifecycle.rehire",capability:"people.lifecycle",purpose:"new-work",permission:"people.manage",target:{kind:"engagement",id:input.engagementId}},tx=>invoke(()=>createPeopleLifecycleApplication(applicationPorts(tx)).createRehireEngagement(actor,input,key)),db); }
export async function saveLifecycleSettings(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof lifecycleSettingsInput>, key: string) { return withPeoplePurchasedCommand(actor,{key,operation:"people.lifecycle.settings",capability:"people.lifecycle",purpose:"new-work",permission:"organization.manage"},tx=>invoke(()=>createPeopleLifecycleApplication(applicationPorts(tx)).saveLifecycleSettings(actor,input,key)),db); }
export async function completePreparationTask(db: PostgresDatabase, actor: WorkContext, id: string, version: number, key: string) { return invoke(() => createPeopleLifecycleApplication(applicationPorts(db)).completePreparationTask(actor, id, version, key)); }

/** Original HR-only placement; the caller cannot select an access recipe. */
export async function previewProjectWorkerPlacementOwned(database:SqlDatabase,actor:WorkContext,input:LifecycleInput){requireProjectWorkerUnit(database,actor);return invoke(()=>createPeopleLifecycleApplication(applicationPorts(database)).previewLifecycle(actor,input,true));}
export async function submitProjectWorkerPlacementOwned(database:SqlDatabase,actor:WorkContext,input:LifecycleInput,key:string,token:string){requireProjectWorkerNativeCommand(database,actor,key,'people.lifecycle.submit');return invoke(()=>createPeopleLifecycleApplication(applicationPorts(database)).saveLifecycle(actor,input,key,true,token,true));}
export async function cancelProjectWorkerPlacementOwned(database:SqlDatabase,actor:WorkContext,id:string,expectedVersion:number,key:string){requireProjectWorkerUnit(database,actor);return invoke(()=>createPeopleLifecycleApplication(applicationPorts(database)).cancelLifecycle(actor,id,expectedVersion,key));}
