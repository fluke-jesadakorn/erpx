import { requirePerformanceHistory, withPerformancePurchasedRead } from '@erpx/server-people_payroll/adapters/performance-purchases';
import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "@erpx/server-organization/public/people-payroll-final-owner-projections";

import "server-only";
import { type DefinitionKind } from "@erpx/feature-people_payroll/lib/people-evaluation";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

import { ApiError } from "@erpx/server-runtime/public/http";
import { captureJobAuthority } from "@erpx/server-identity_access/public/iam-jobs";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { iamMutationGuards } from "@erpx/server-identity_access/public/iam-store";

import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";

import { BusinessRuleError } from '@erpx/server-shared/index';
import { type Row, type AssessmentRow } from '@erpx/server-people_payroll/domain/people-evaluation';

import { peopleEvaluationWorkflowFacts } from '@erpx/server-workflow/public/people-evaluation-facts';

import{createPeopleEvaluationRead,type PeopleEvaluationReadPorts}from '@erpx/server-people_payroll/application/people-evaluation';
const organizationFinalSources=organizationFinalOwnerProjections();
export function nativeEvaluationReadRepository(database:SqlDatabase):PeopleEvaluationReadPorts<SqlStatement>['repository']{return{stageEvaluationAuthorityEffect(...args) { return peopleEvaluationWorkflowFacts(database).stageEvaluationAuthorityEffect(...args); },readEvaluationEmployeesRows(companyId: string, userId: string, companyId2: string, companyId3: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, role: string) {
            return database.prepare(`WITH RECURSIVE team AS (
    SELECT e.person_id FROM hr_engagements e JOIN hr_people manager ON manager.id=e.manager_person_id AND manager.company_id=e.company_id WHERE e.company_id=? AND manager.user_id=? AND e.status IN ('active','suspended','ended')
    UNION SELECT e.person_id FROM hr_engagements e JOIN team t ON t.person_id=e.manager_person_id WHERE e.company_id=? AND e.status IN ('active','suspended','ended')
  ) SELECT e.id,e.person_id,p.legal_name,p.user_id,e.position_id,e.level_id,e.job_title,e.branch_id,e.department_id,n.display_name department,e.currency,e.start_date,e.end_date,e.version
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id LEFT ${organizationFinalSources.people_evaluation_organization_nodes_n_join} ON n.id=e.department_id AND n.company_id=e.company_id
    WHERE e.company_id=? AND p.merged_into_id IS NULL AND e.worker_type='employee' AND e.status IN ('active','suspended','ended')
    AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)
    AND (?<>'people_manager' OR e.person_id IN (SELECT person_id FROM team)) ORDER BY p.legal_name,e.id`)
                .bind(companyId, userId, companyId2, companyId3, branchId, branchId2, departmentId, departmentId2, role).all<Row>();
        },readEvaluationDefinitionRow(id: string, kind: "skill" | "benchmark" | "policy" | "role" | "band" | "cycle", companyId: string, value: number[], v: number | undefined) {
            return database.prepare(v ? "SELECT id,kind,version,payload_json,created_at updated_at FROM hr_evaluation_definition_history WHERE id=? AND kind=? AND company_id=? AND version=?" : "SELECT id,kind,version,payload_json,updated_at FROM hr_evaluation_definitions WHERE id=? AND kind=? AND company_id=?")
                .bind(id, kind, companyId, ...value).first<{
                id: string;
                kind: DefinitionKind;
                version: number;
                payload_json: string;
                updated_at: number;
            }>();
        },readEvaluationAssessmentRow(assessmentId: string, companyId: string) { return database.prepare("SELECT * FROM hr_evaluation_assessments WHERE id=? AND company_id=?").bind(assessmentId, companyId).first<AssessmentRow>(); }};}
function application(database?:SqlDatabase){return createPeopleEvaluationRead({repository:nativeEvaluationReadRepository(database!),ids:{newId:()=>crypto.randomUUID()},authorize:requirePermission,batch:effects=>database!.batch(effects),purchasedHistory:actor=>fromNative(()=>requirePerformanceHistory(database!,actor)),captureAuthority:(...args)=>fromNative(()=>captureJobAuthority(database!,...args)),authorityGuards:(...args)=>iamMutationGuards(database!,...args)});}
async function fromNative<Result>(run: () => Promise<Result> | Result): Promise<Result> {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof ApiError)
            throw new BusinessRuleError(error.status, error.code, error.message);
        throw error;
    }
}
async function invoke<Result>(run: () => Promise<Result>) {
    try {
        return await run();
    }
    catch (error) {
        if (error instanceof BusinessRuleError)
            throw new ApiError(error.status, error.code, error.message);
        throw error;
    }
}
export async function evaluationAuthority(db: SqlDatabase, actor: WorkContext, permission = "people.read") { return invoke(() => application(db).evaluationAuthority(actor, permission)); }
export async function evaluationEmployees(db: SqlDatabase, actor: WorkContext) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).evaluationEmployees(actor)),db); }
export async function evaluationEmployee(db: SqlDatabase, actor: WorkContext, engagementId: string) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).evaluationEmployee(actor, engagementId)),db); }
export async function evaluationDefinition(db: SqlDatabase, actor: WorkContext, id: string, kind: DefinitionKind, v?: number) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).evaluationDefinition(actor, id, kind, v)),db); }
export async function evaluationAssessment(db: SqlDatabase, actor: WorkContext, assessmentId: string) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).evaluationAssessment(actor, assessmentId)),db); }
