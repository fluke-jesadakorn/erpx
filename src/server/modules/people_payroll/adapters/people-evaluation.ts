import {requirePerformanceHistory,withPerformancePurchasedRead,performanceDefinitionPurpose,withPerformancePurchasedCommand,performancePurchaseControls} from './performance-purchases';
import {peoplePayrollFinalOwnerProjections as organizationFinalOwnerProjections} from "../../organization/public/people-payroll-final-owner-projections";
const organizationFinalSources=organizationFinalOwnerProjections();
import {peopleEvaluationStaffWorkFacts} from '../../operations/public/people-evaluation-work-facts';
import "server-only";
import { type Compensation, type DefinitionKind } from "@/lib/people_payroll/people-evaluation";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { captureJobAuthority } from "@/server/modules/identity_access/public/iam-jobs";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { iamMutationGuards } from "@/server/modules/identity_access/public/iam-store";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import { attachmentLinkStatus } from '../../files/public/attachments/service';
import { BusinessRuleError } from '../../shared';
import { type Row, type AssessmentRow, type EvaluationEmployee, createEvaluationQuerySchema } from '../domain/people-evaluation';
import { createPeopleEvaluationApplication, type PeopleEvaluationRepository } from '../application/people-evaluation';
export type { EvaluationQuery, EvaluationEmployee, EvaluationRow } from '../domain/people-evaluation';
import type {EvaluationWorkspace as NativeEvaluationWorkspace} from '../application/people-evaluation';
export type EvaluationWorkspace=NativeEvaluationWorkspace&{purchase?:{newWork:boolean;settleExisting:boolean;history:boolean;revision:string}};
import { peopleEvaluationWorkflowFacts } from '../../workflow/public/people-evaluation-facts';
import { peopleEvaluationOrganizationFacts } from '../../organization/public/people-evaluation-facts';
import { peopleEvaluationOperationsFacts } from '../../operations/public/people-evaluation-facts';
import { peopleEvaluationIdentityAccessFacts } from '../../identity_access/public/people-evaluation-facts';
import { peopleEvaluationFilesFacts } from '../../files/public/people-evaluation-facts';
import { peopleEvaluationProjectsFacts } from '../../projects/public/people-evaluation-facts';
import { peopleEvaluationLearningFacts } from '../../learning/public/people-evaluation-facts';
export const evaluationQuerySchema = createEvaluationQuerySchema(new Date().getUTCFullYear());
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PeopleEvaluationRepository<SqlStatement> = { stageEvaluationAuthorityEffect(...args) { return peopleEvaluationWorkflowFacts(database).stageEvaluationAuthorityEffect(...args); },
        readEvaluationEmployeesRows(companyId: string, userId: string, companyId2: string, companyId3: string, branchId: string | null, branchId2: string | null, departmentId: string | null, departmentId2: string | null, role: string) {
            return database.prepare(`WITH RECURSIVE team AS (
    SELECT e.person_id FROM hr_engagements e JOIN hr_people manager ON manager.id=e.manager_person_id AND manager.company_id=e.company_id WHERE e.company_id=? AND manager.user_id=? AND e.status IN ('active','suspended','ended')
    UNION SELECT e.person_id FROM hr_engagements e JOIN team t ON t.person_id=e.manager_person_id WHERE e.company_id=? AND e.status IN ('active','suspended','ended')
  ) SELECT e.id,e.person_id,p.legal_name,p.user_id,e.position_id,e.level_id,e.job_title,e.branch_id,e.department_id,n.display_name department,e.currency,e.start_date,e.end_date,e.version
    FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id LEFT ${organizationFinalSources.people_evaluation_organization_nodes_n_join} ON n.id=e.department_id AND n.company_id=e.company_id
    WHERE e.company_id=? AND p.merged_into_id IS NULL AND e.worker_type='employee' AND e.status IN ('active','suspended','ended')
    AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)
    AND (?<>'people_manager' OR e.person_id IN (SELECT person_id FROM team)) ORDER BY p.legal_name,e.id`)
                .bind(companyId, userId, companyId2, companyId3, branchId, branchId2, departmentId, departmentId2, role).all<Row>();
        },
        readEvaluationDefinitionRow(id: string, kind: "skill" | "benchmark" | "policy" | "role" | "band" | "cycle", companyId: string, value: number[], v: number | undefined) {
            return database.prepare(v ? "SELECT id,kind,version,payload_json,created_at updated_at FROM hr_evaluation_definition_history WHERE id=? AND kind=? AND company_id=? AND version=?" : "SELECT id,kind,version,payload_json,updated_at FROM hr_evaluation_definitions WHERE id=? AND kind=? AND company_id=?")
                .bind(id, kind, companyId, ...value).first<{
                id: string;
                kind: DefinitionKind;
                version: number;
                payload_json: string;
                updated_at: number;
            }>();
        },
        readEvaluationTodayGroup(engagementId: string) {
            return database.prepare("SELECT g.timezone FROM hr_engagements e JOIN hr_pay_groups g ON g.id=e.pay_group_id AND g.company_id=e.company_id WHERE e.id=?").bind(engagementId).first<{
                timezone: string;
            }>();
        },
        readSaveEvaluationDefinitionPosition(...args) { return peopleEvaluationOrganizationFacts(database).readSaveEvaluationDefinitionPosition(...args); },
        readSaveEvaluationDefinitionEffect(...args) { return peopleEvaluationOrganizationFacts(database).readSaveEvaluationDefinitionEffect(...args); },
        guard7(key: string, value: "unique-cycle", companyId: string, id: string, year: number) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE company_id=? AND kind='cycle' AND id<>? AND (payload_json::jsonb->>'year')::integer=?)", [companyId, id, year]); },
        stageSaveEvaluationDefinitionEffect(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(value); },
        guard9(key: string, value: "criteria-version", id: string, companyId: string, kind: "skill" | "benchmark" | "policy" | "role" | "band" | "cycle", expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND company_id=? AND kind=? AND version=?)", [id, companyId, kind, expectedVersion]); },
        stageSaveEvaluationDefinitionEffect2(value: string, userId: string, reason: string, now: number, id: string) { return database.prepare("UPDATE hr_evaluation_definitions SET payload_json=?,version=version+1,updated_by=?,reason=?,updated_at=? WHERE id=?").bind(value, userId, reason, now, id); },
        stageSaveEvaluationDefinitionEffect3(id: string, companyId: string, kind: "skill" | "benchmark" | "policy" | "role" | "band" | "cycle", value: string, userId: string, userId2: string, reason: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_evaluation_definitions(id,company_id,kind,payload_json,created_by,updated_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(id, companyId, kind, value, userId, userId2, reason, now, now2); },
        readEvaluationWorkspaceDefinitions(companyId: string) {
            return database.prepare("SELECT id,kind,version,payload_json,updated_at FROM hr_evaluation_definitions WHERE company_id=? ORDER BY kind,id").bind(companyId).all<{
                id: string;
                kind: DefinitionKind;
                version: number;
                payload_json: string;
                updated_at: number;
            }>();
        },
        readEvaluationWorkspaceAssessments(companyId: string, id: string, date: string) { return database.prepare("SELECT * FROM hr_evaluation_assessments WHERE company_id=? AND cycle_id=? AND period_start<=? ORDER BY period_start,id").bind(companyId, id, date).all<AssessmentRow>(); },
        readEvaluationWorkspaceWages(id: string, date: string, date2: string) { return database.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC").bind(id, date, date2).all<Compensation>(); },
        readEvaluationWorkspaceBasisRow(id: string, companyId: string, date: string) {
            return database.prepare("SELECT version,payload_json FROM hr_evaluation_pay_basis WHERE engagement_id=? AND company_id=? AND effective_from<=? ORDER BY effective_from DESC LIMIT 1").bind(id, companyId, date).first<{
                version: number;
                payload_json: string;
            }>();
        },
        readEvaluationWorkspacePositions(...args) { return peopleEvaluationOrganizationFacts(database).readEvaluationWorkspacePositions(...args); },
        readEvaluationWorkspaceLevels(...args) { return peopleEvaluationOrganizationFacts(database).readEvaluationWorkspaceLevels(...args); },
        readImportEvaluationMarketLevels(...args) { return peopleEvaluationOrganizationFacts(database).readImportEvaluationMarketLevels(...args); },
        stageImportEvaluationMarketEffect(value: string, companyId: string, value2: string, userId: string, userId2: string, reason: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_evaluation_definitions(id,company_id,kind,payload_json,created_by,updated_by,reason,created_at,updated_at) VALUES(?,?,'benchmark',?,?,?,?,?,?)").bind(value, companyId, value2, userId, userId2, reason, now, now2); },
        readEvaluationAssessmentRow(assessmentId: string, companyId: string) { return database.prepare("SELECT * FROM hr_evaluation_assessments WHERE id=? AND company_id=?").bind(assessmentId, companyId).first<AssessmentRow>(); },
        readValidateEvaluationRolePeriodHistory(...args) { return peopleEvaluationOperationsFacts(database).readValidateEvaluationRolePeriodHistory(...args); },
        readValidateEvaluationRolePeriodZone(id: string) {
            return database.prepare("SELECT g.timezone FROM hr_engagements e JOIN hr_pay_groups g ON g.id=e.pay_group_id AND g.company_id=e.company_id WHERE e.id=?").bind(id).first<{
                timezone: string;
            }>();
        },
        stageCreateEvaluationAssessmentEffect(id: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(id); },
        guard24(key: string, value: "engagement-version", id: string, companyId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)", [id, companyId, version]); },
        stageCreateEvaluationAssessmentEffect2(id: string) { return database.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(id); },
        guard26(key: string, value: "cycle-open", cycleId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [cycleId, version]); },
        guard27(key: string, value: "period-overlap", id: string, cycleId: string, periodEnd: string, periodStart: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE engagement_id=? AND cycle_id=? AND period_start<=? AND period_end>=?)", [id, cycleId, periodEnd, periodStart]); },
        stageCreateEvaluationAssessmentEffect3(id: string, companyId: string, id2: string, cycleId: string, id3: string, version: number, periodStart: string, periodEnd: string, value: string, userId: string, reason: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_evaluation_assessments(id,company_id,engagement_id,cycle_id,role_id,role_version,period_start,period_end,content_json,created_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, companyId, id2, cycleId, id3, version, periodStart, periodEnd, value, userId, reason, now, now2); },
        readSaveEvaluationAssessmentEvidence(assessmentId: string, companyId: string) {
            return database.prepare("SELECT id,observed_on FROM hr_evaluation_evidence WHERE assessment_id=? AND company_id=?").bind(assessmentId, companyId).all<{
                id: string;
                observed_on: string;
            }>();
        },
        stageSaveEvaluationAssessmentGuards(id: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(id); },
        guard31(key: string, value: "engagement-version", id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [id, version]); },
        guard32(key: string, value: "period-overlap", id: string, cycle_id: string, assessmentId: string, periodEnd: string, periodStart: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE engagement_id=? AND cycle_id=? AND id<>? AND period_start<=? AND period_end>=?)", [id, cycle_id, assessmentId, periodEnd, periodStart]); },
        stageSaveEvaluationAssessmentGuards2(assessmentId: string) { return database.prepare("SELECT id FROM hr_evaluation_assessments WHERE id=? FOR UPDATE").bind(assessmentId); },
        stageSaveEvaluationAssessmentGuards3(id: string) { return database.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(id); },
        guard35(key: string, value: "cycle-open", cycle_id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [cycle_id, version]); },
        guard36(key: string, value: "assessment-version", assessmentId: string, expectedVersion: number, status: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)", [assessmentId, expectedVersion, status]); },
        stageSaveEvaluationAssessmentEffect(periodStart: string, periodEnd: string, value: string, userId: string, userId2: string, reason: string, now: number, assessmentId: string) { return database.prepare("UPDATE hr_evaluation_assessments SET period_start=?,period_end=?,content_json=?,reviewed_by=?,updated_by=?,reason=?,version=version+1,updated_at=? WHERE id=?").bind(periodStart, periodEnd, value, userId, userId2, reason, now, assessmentId); },
        readTransitionEvaluationAssessmentEffect(...args) { return peopleEvaluationIdentityAccessFacts(database).readTransitionEvaluationAssessmentEffect(...args); },
        readTransitionEvaluationAssessmentEffect2(...args) { return peopleEvaluationIdentityAccessFacts(database).readTransitionEvaluationAssessmentEffect2(...args); },
        stageTransitionEvaluationAssessmentEffect(id: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(id); },
        guard41(key: string, value: "engagement-version", id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [id, version]); },
        stageTransitionEvaluationAssessmentEffect2(assessmentId: string) { return database.prepare("SELECT id FROM hr_evaluation_assessments WHERE id=? FOR UPDATE").bind(assessmentId); },
        guard43(key: string, value: "assessment-version", assessmentId: string, expectedVersion: number, status: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)", [assessmentId, expectedVersion, status]); },
        stageTransitionEvaluationAssessmentEffect3(id: string) { return database.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(id); },
        guard45(key: string, value: "cycle-open", cycle_id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [cycle_id, version]); },
        guard46(key: string, value: "no-pay-draft", assessmentId: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE assessment_id=? AND status='draft')", [assessmentId]); },
        stageTransitionEvaluationAssessmentEffect4(value: string, userId: string, value2: string | null, value3: string, reason: string, now: number, assessmentId: string) { return database.prepare("UPDATE hr_evaluation_assessments SET status=?,updated_by=?,confirmed_by=?,reviewed_by=CASE WHEN ?='draft' THEN NULL ELSE reviewed_by END,reason=?,version=version+1,updated_at=? WHERE id=?").bind(value, userId, value2, value3, reason, now, assessmentId); },
        readEvaluationDetailEvidence(assessmentId: string, companyId: string) { return database.prepare("SELECT id,source_type,source_id,description,observed_on,created_by,created_at FROM hr_evaluation_evidence WHERE assessment_id=? AND company_id=? ORDER BY created_at,id").bind(assessmentId, companyId).all<Row>(); },
        readEvaluationDetailHistory(assessmentId: string, companyId: string) { return database.prepare("SELECT version,status,content_json,actor_id,reason,created_at,period_start,period_end,role_id,role_version FROM hr_evaluation_assessment_history WHERE id=? AND company_id=? ORDER BY version DESC").bind(assessmentId, companyId).all<Row>(); },
        readEvaluationDetailSalary(id: string) { return database.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status IN ('approved','superseded') ORDER BY effective_from DESC").bind(id).all<Compensation>(); },
        readEvaluationDetailSalary2(assessmentId: string, companyId: string) { return database.prepare("SELECT d.id,d.compensation_id,d.version,d.status,d.reason,d.review_reason,d.created_by,d.approved_by,d.previous_json,d.proposed_json,c.effective_from,c.base_amount_minor,c.hourly_rate_minor,c.daily_rate_minor FROM hr_evaluation_compensation_drafts d JOIN hr_compensation_revisions c ON c.id=d.compensation_id WHERE d.assessment_id=? AND d.company_id=? ORDER BY d.created_at DESC").bind(assessmentId, companyId).all<Row>(); },
        readEvaluationDetailFiles(...args) { return peopleEvaluationFilesFacts(database).readEvaluationDetailFiles(...args); },
        readEvaluationEvidenceSourcesTasksprojectslearning(...args) { return peopleEvaluationStaffWorkFacts(database).readEvaluationEvidenceSourcesTasksprojectslearning(...args); },
        readEvaluationEvidenceSourcesTasksprojectslearning2(...args) { return peopleEvaluationProjectsFacts(database).readEvaluationEvidenceSourcesTasksprojectslearning2(...args); },
        readEvaluationEvidenceSourcesTasksprojectslearning3(...args) { return peopleEvaluationLearningFacts(database).readEvaluationEvidenceSourcesTasksprojectslearning3(...args); },
        readAddEvaluationEvidenceLink(...args) { return peopleEvaluationFilesFacts(database).readAddEvaluationEvidenceLink(...args); },
        stageAddEvaluationEvidenceEffect(id: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(id); },
        guard58(key: string, value: "engagement-version", id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [id, version]); },
        stageAddEvaluationEvidenceEffect2(assessmentId: string) { return database.prepare("SELECT id FROM hr_evaluation_assessments WHERE id=? FOR UPDATE").bind(assessmentId); },
        guard60(key: string, value: "assessment-version", assessmentId: string, expectedVersion: number, status: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)", [assessmentId, expectedVersion, status]); },
        stageAddEvaluationEvidenceEffect3(id: string) { return database.prepare("SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE").bind(id); },
        guard62(key: string, value: "cycle-open", cycle_id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')", [cycle_id, version]); },
        stageAddEvaluationEvidenceEffect4(id: string, companyId: string, assessmentId: string, sourceType: "file" | "kpi" | "task" | "project" | "learning", sourceId: string | null, description: string, observedOn: string, value: string, userId: string, now: number) { return database.prepare("INSERT INTO hr_evaluation_evidence(id,company_id,assessment_id,source_type,source_id,description,observed_on,source_json,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(id, companyId, assessmentId, sourceType, sourceId, description, observedOn, value, userId, now); },
        stageAddEvaluationEvidenceEffect5(userId: string, userId2: string, now: number, assessmentId: string) { return database.prepare("UPDATE hr_evaluation_assessments SET version=version+1,reviewed_by=?,updated_by=?,reason='Evidence added',updated_at=? WHERE id=?").bind(userId, userId2, now, assessmentId); },
        readReadEvaluationEvidenceEvidence(evidenceId: string, companyId: string) { return database.prepare("SELECT * FROM hr_evaluation_evidence WHERE id=? AND company_id=?").bind(evidenceId, companyId).first<Row>(); },
        stageConfirmEvaluationPayBasisEffect(engagementId: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(engagementId); },
        guard67(key: string, value: "engagement-version", engagementId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [engagementId, version]); },
        guard68(key: string, value: "basis-version", engagementId: string, expectedVersion: number) { return assertion(database, key, value, "COALESCE((SELECT max(version) FROM hr_evaluation_pay_basis WHERE engagement_id=?),0)=?", [engagementId, expectedVersion]); },
        stageConfirmEvaluationPayBasisEffect2(engagementId: string, companyId: string, value: number, effectiveFrom: string, value2: string, userId: string, now: number) { return database.prepare("INSERT INTO hr_evaluation_pay_basis(engagement_id,company_id,version,effective_from,payload_json,confirmed_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(engagementId, companyId, value, effectiveFrom, value2, userId, now); },
        readCreateEvaluationCompensationDraftE(id: string, companyId: string) {
            return database.prepare("SELECT pay_group_id,status FROM hr_engagements WHERE id=? AND company_id=?").bind(id, companyId).first<{
                pay_group_id: string | null;
                status: string;
            }>();
        },
        readCreateEvaluationCompensationDraftLastRun(companyId: string, pay_group_id: string) {
            return database.prepare("SELECT period_end FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND status IN ('approved','posted') ORDER BY period_end DESC LIMIT 1").bind(companyId, pay_group_id).first<{
                period_end: string;
            }>();
        },
        readCreateEvaluationCompensationDraftWages(id: string, effectiveFrom: string, effectiveFrom2: string) {
            return database.prepare("SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved' AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?)").bind(id, effectiveFrom, effectiveFrom2).all<Compensation & {
                components_json: string;
            }>();
        },
        readCreateEvaluationCompensationDraftBasis(id: string, companyId: string, effectiveFrom: string) {
            return database.prepare("SELECT version,payload_json FROM hr_evaluation_pay_basis WHERE engagement_id=? AND company_id=? AND effective_from<=? ORDER BY effective_from DESC LIMIT 1").bind(id, companyId, effectiveFrom).first<{
                version: number;
                payload_json: string;
            }>();
        },
        stageCreateEvaluationCompensationDraftEffect(id: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(id); },
        guard75(key: string, value: "assessment-confirmed", assessmentId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status='confirmed')", [assessmentId, expectedVersion]); },
        guard76(key: string, value: "wage-version", id: string, value2: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status='approved')", [id, value2]); },
        guard77(key: string, value: "engagement-version", id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')", [id, version]); },
        stageCreateEvaluationCompensationDraftEffect2(compId: string, id: string, effectiveFrom: string, effective_to: string | null, pay_frequency: string, base: number, hourly_rate: number, daily_rate: number, standard_daily_hours: number, expected_hours: number, currency: string, components_json: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,daily_rate_minor,standard_daily_hours,expected_hours,currency,components_json,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'draft',?,?,?)").bind(compId, id, effectiveFrom, effective_to, pay_frequency, base, hourly_rate, daily_rate, standard_daily_hours, expected_hours, currency, components_json, userId, now, now2); },
        stageCreateEvaluationCompensationDraftEffect3(id: string, companyId: string, assessmentId: string, expectedVersion: number, compId: string, id2: string, version: number, version2: number, id3: string, version3: number, version4: number, reason: string, userId: string, now: number, now2: number, value: string, value2: string) { return database.prepare("INSERT INTO hr_evaluation_compensation_drafts(id,company_id,assessment_id,assessment_version,compensation_id,previous_id,previous_version,engagement_version,policy_id,policy_version,basis_version,reason,created_by,created_at,updated_at,previous_json,proposed_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, companyId, assessmentId, expectedVersion, compId, id2, version, version2, id3, version3, version4, reason, userId, now, now2, value, value2); },
        readApproveEvaluationCompensationDraft(draftId: string, companyId: string) { return database.prepare("SELECT d.*,c.engagement_id,c.effective_from,c.version compensation_version FROM hr_evaluation_compensation_drafts d JOIN hr_compensation_revisions c ON c.id=d.compensation_id WHERE d.id=? AND d.company_id=?").bind(draftId, companyId).first<Row>(); },
        readApproveEvaluationCompensationEffect(...args) { return peopleEvaluationIdentityAccessFacts(database).readApproveEvaluationCompensationEffect(...args); },
        stageApproveEvaluationCompensationEffect(id: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(id); },
        stageApproveEvaluationCompensationEffect2(id: string) { return database.prepare("SELECT id FROM hr_compensation_revisions WHERE engagement_id=? FOR UPDATE").bind(id); },
        guard84(key: string, value: "pay-draft-version", draftId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE id=? AND version=? AND status='draft')", [draftId, expectedVersion]); },
        guard85(key: string, value: "assessment-version", id: string, value2: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status='confirmed')", [id, value2]); },
        guard86(key: string, value: "engagement-version", id: string, value2: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')", [id, value2]); },
        guard87(key: string, value: "basis-version", id: string, effective: string, value2: number) { return assertion(database, key, value, "(SELECT version FROM hr_evaluation_pay_basis WHERE engagement_id=? AND effective_from<=? ORDER BY effective_from DESC LIMIT 1)=?", [id, effective, value2]); },
        guard88(key: string, value: "policy-version", value2: string, companyId: string, value3: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND company_id=? AND version=? AND payload_json::jsonb->>'confirmed'='true')", [value2, companyId, value3]); },
        guard89(key: string, value: "previous-version", value2: string, value3: number, effective: string, effective2: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status='approved' AND effective_from<? AND (effective_to IS NULL OR effective_to>=?))", [value2, value3, effective, effective2]); },
        guard90(key: string, value: "new-wage-version", value2: string, value3: 1) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND version=? AND status='draft')", [value2, value3]); },
        guard91(key: string, value: "proposed-pay-snapshot", draftId: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions c JOIN hr_evaluation_compensation_drafts d ON d.compensation_id=c.id WHERE d.id=? AND c.base_amount_minor=(d.proposed_json::jsonb->>'base_amount_minor')::bigint AND c.hourly_rate_minor=(d.proposed_json::jsonb->>'hourly_rate_minor')::bigint AND c.daily_rate_minor=(d.proposed_json::jsonb->>'daily_rate_minor')::bigint AND c.standard_daily_hours=(d.proposed_json::jsonb->>'standard_daily_hours')::numeric AND c.expected_hours=(d.proposed_json::jsonb->>'expected_hours')::numeric AND c.effective_to IS NOT DISTINCT FROM (d.proposed_json::jsonb->>'effective_to') AND c.components_json=d.proposed_json::jsonb->>'components_json' AND c.currency=d.proposed_json::jsonb->>'currency' AND c.pay_frequency=d.proposed_json::jsonb->>'pay_frequency' AND c.effective_from=d.proposed_json::jsonb->>'effective_from')", [draftId]); },
        guard92(key: string, value: "no-published-payroll", id: string, effective: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE i.engagement_id=? AND r.period_end>=? AND r.status IN ('approved','posted'))", [id, effective]); },
        guard93(key: string, value: "no-other-wage", id: string, value2: string, value3: string, effective: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND id NOT IN (?,?) AND status='approved' AND (effective_to IS NULL OR effective_to>=?))", [id, value2, value3, effective]); },
        stageApproveEvaluationCompensationEffect3(value: string, now: number, previous_id: unknown) { return database.prepare("UPDATE hr_compensation_revisions SET effective_to=?,version=version+1,updated_at=? WHERE id=?").bind(value, now, previous_id); },
        stageApproveEvaluationCompensationEffect4(userId: string, now: number, compensation_id: unknown) { return database.prepare("UPDATE hr_compensation_revisions SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=?").bind(userId, now, compensation_id); },
        stageApproveEvaluationCompensationEffect5(userId: string, reason: string, now: number, draftId: string) { return database.prepare("UPDATE hr_evaluation_compensation_drafts SET status='approved',approved_by=?,review_reason=?,version=version+1,updated_at=? WHERE id=?").bind(userId, reason, now, draftId); },
        stageApproveEvaluationCompensationEffect6(...args) { return peopleEvaluationWorkflowFacts(database).stageApproveEvaluationCompensationEffect6(...args); },
        readCancelEvaluationCompensationDraft(draftId: string, companyId: string) { return database.prepare("SELECT * FROM hr_evaluation_compensation_drafts WHERE id=? AND company_id=?").bind(draftId, companyId).first<Row>(); },
        stageCancelEvaluationCompensationEffect(id: string) { return database.prepare("SELECT id FROM hr_engagements WHERE id=? FOR UPDATE").bind(id); },
        guard100(key: string, value: "engagement-version", id: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)", [id, version]); },
        stageCancelEvaluationCompensationEffect2(draftId: string) { return database.prepare("SELECT id FROM hr_evaluation_compensation_drafts WHERE id=? FOR UPDATE").bind(draftId); },
        guard102(key: string, value: "pay-draft-version", draftId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_evaluation_compensation_drafts WHERE id=? AND version=? AND status='draft')", [draftId, expectedVersion]); },
        guard103(key: string, value: "unapproved-pay", value2: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE id=? AND status='draft')", [value2]); },
        stageCancelEvaluationCompensationEffect3(now: number, compensation_id: unknown) { return database.prepare("UPDATE hr_compensation_revisions SET status='superseded',version=version+1,updated_at=? WHERE id=?").bind(now, compensation_id); },
        stageCancelEvaluationCompensationEffect4(reason: string, now: number, draftId: string) { return database.prepare("UPDATE hr_evaluation_compensation_drafts SET status='cancelled',review_reason=?,version=version+1,updated_at=? WHERE id=?").bind(reason, now, draftId); } };
    return createPeopleEvaluationApplication({ repository, clock: { now: Date.now }, ids: { newId: () => crypto.randomUUID() }, authorize: requirePermission, prepare: async (holding) => {
            if (!provided)
                database = (await holdingResources(holding)).write;
        }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, querySchema: evaluationQuerySchema, batch: effects => database.batch(effects), purchasedHistory: actor=>fromNative(()=>requirePerformanceHistory(database,actor)), captureAuthority: (...args) => fromNative(() => captureJobAuthority(database, ...args)), authorityGuards: (...args) => iamMutationGuards(database, ...args), snapshotMoney: (...args) => fromNative(() => snapshotMoney(database, ...args)), attachmentLinkStatus: (...args) => fromNative(() => attachmentLinkStatus(database, ...args)) });
}
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
export async function saveEvaluationDefinition(actor: WorkContext, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.criteria.save',purpose:database=>performanceDefinitionPurpose(database,actor,raw)},(database,current)=>invoke(()=>application(database).saveEvaluationDefinition(current, raw, key))); }
export async function evaluationWorkspace(actor: WorkContext, raw: unknown = {},provided?:SqlDatabase):Promise<EvaluationWorkspace> { return withPerformancePurchasedRead(actor,async database=>{const workspace=await invoke(()=>application(database).evaluationWorkspace(actor, raw)),purchase=await performancePurchaseControls(actor,database);return purchase?{...workspace,purchase}:workspace;},provided); }
export async function importEvaluationMarket(actor: WorkContext, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.market.import',purpose:'new-work'},(database,current)=>invoke(()=>application(database).importEvaluationMarket(current, raw, key))); }
export async function evaluationAssessment(db: SqlDatabase, actor: WorkContext, assessmentId: string) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).evaluationAssessment(actor, assessmentId)),db); }
export async function createEvaluationAssessment(actor: WorkContext, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.assessment.create',purpose:'new-work'},(database,current)=>invoke(()=>application(database).createEvaluationAssessment(current, raw, key))); }
export async function saveEvaluationAssessment(actor: WorkContext, assessmentId: string, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.assessment.save',purpose:'new-work'},(database,current)=>invoke(()=>application(database).saveEvaluationAssessment(current, assessmentId, raw, key))); }
export async function transitionEvaluationAssessment(actor: WorkContext, assessmentId: string, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.assessment.transition',purpose:raw&&typeof raw==='object'&&'action' in raw&&raw.action==='reopen'?'new-work':'settle-existing'},(database,current)=>invoke(()=>application(database).transitionEvaluationAssessment(current, assessmentId, raw, key))); }
export async function evaluationDetail(actor: WorkContext, assessmentId: string,provided?:SqlDatabase) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).evaluationDetail(actor, assessmentId)),provided); }
export async function evaluationEvidenceSources(db: SqlDatabase, actor: WorkContext, employee: EvaluationEmployee, sourceId: string | null = null) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).evaluationEvidenceSources(actor, employee, sourceId)),db); }
export async function addEvaluationEvidence(actor: WorkContext, assessmentId: string, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.evidence.add',purpose:'settle-existing'},(database,current)=>invoke(()=>application(database).addEvaluationEvidence(current, assessmentId, raw, key))); }
export async function readEvaluationEvidence(actor: WorkContext, evidenceId: string,provided?:SqlDatabase) { return withPerformancePurchasedRead(actor,database=>invoke(()=>application(database).readEvaluationEvidence(actor, evidenceId)),provided); }
export async function confirmEvaluationPayBasis(actor: WorkContext, engagementId: string, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.basis.confirm',purpose:'new-work'},(database,current)=>invoke(()=>application(database).confirmEvaluationPayBasis(current, engagementId, raw, key))); }
export async function createEvaluationCompensationDraft(actor: WorkContext, assessmentId: string, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.compensation.create',purpose:'new-work'},(database,current)=>invoke(()=>application(database).createEvaluationCompensationDraft(current, assessmentId, raw, key))); }
export async function approveEvaluationCompensation(actor: WorkContext, draftId: string, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.compensation.approve',purpose:'settle-existing'},(database,current)=>invoke(()=>application(database).approveEvaluationCompensation(current, draftId, raw, key))); }
export async function cancelEvaluationCompensation(actor: WorkContext, draftId: string, raw: unknown, key: string) { return withPerformancePurchasedCommand(actor,{key,operation:'people.evaluation.compensation.cancel',purpose:'settle-existing'},(database,current)=>invoke(()=>application(database).cancelEvaluationCompensation(current, draftId, raw, key))); }
