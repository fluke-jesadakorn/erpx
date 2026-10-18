import { withPayrollPurchasedCommand, withPayrollPurchasedRead } from "./payroll-purchases";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import { assertProductionModuleEnabled, productionFeatureGuard } from "@/server/modules/pms/public/hospitality-rollout";
import { thaiEvidenceForEmployee, thaiEvidenceGuards, thaiPackForRun, type PayrollStatutoryRun } from "@/server/modules/people_payroll/adapters/payroll-statutory";
import { BusinessRuleError } from '../../shared';
import { type Row, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from '../domain/payroll-adjustments';
import { createPayrollAdjustmentApplication, type PayrollAdjustmentRepository } from '../application/payroll-adjustments';
export { payrollAdjustmentValuesSchema, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from '../domain/payroll-adjustments';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PayrollAdjustmentRepository<SqlStatement> = { readOriginalSourceRun(runId: string, companyId: string) {
            return database.prepare("SELECT to_jsonb(r)::text source FROM hr_payroll_runs r WHERE id=? AND company_id=? AND status='posted' AND mode='production' AND adjustment_of_run_id IS NULL").bind(runId, companyId).first<{
                source: string;
            }>();
        },
        readOriginalSourceItem(itemId: string, runId: string) {
            return database.prepare("SELECT to_jsonb(i)::text source FROM hr_payroll_items i WHERE id=? AND run_id=? AND status='posted'").bind(itemId, runId).first<{
                source: string;
            }>();
        },
        readOriginalSourceSlips(runId: string, itemId: string) { return database.prepare("SELECT id,source_digest,export_sha256 FROM hr_issued_payslips WHERE run_id=? AND item_id=? ORDER BY id").bind(runId, itemId).all<Row>(); },
        readTimeSourcesEntries(engagement_id: unknown, company_id: unknown, ids: string[]) { return database.prepare(`SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND company_id=? AND status='approved' ${ids.length ? `AND id IN (${ids.map(() => "?").join(",")})` : "AND FALSE"} ORDER BY id`).bind(engagement_id, company_id, ...ids).all<Row>(); },
        stageRevisionStatementsEffect(id: string, runId: string, revision: number, taxableArrearsMinor: number, nonTaxableArrearsMinor: number, socialSecurityWageMinor: number, value: string, reason: string, evidenceRef: string, userId: string, key: string, now: number) { return database.prepare("INSERT INTO hr_payroll_adjustment_revisions(id,run_id,revision,taxable_arrears_minor,non_taxable_arrears_minor,social_security_wage_minor,time_entries_json,reason,evidence_ref,created_by,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, runId, revision, taxableArrearsMinor, nonTaxableArrearsMinor, socialSecurityWageMinor, value, reason, evidenceRef, userId, key, now); },
        stageRevisionStatementsEffect2(runId: string, revision: number, timeId: string) { return database.prepare("INSERT INTO hr_payroll_adjustment_time_claims(run_id,source_revision,time_entry_id) VALUES(?,?,?)").bind(runId, revision, timeId); },
        stageCreatePayrollAdjustmentEffect(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(value); },
        guard8(key: string, value: "adjustment-original", originalRunId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)", [originalRunId, companyId, expectedVersion]); },
        guard9(key: string, value: string, value2: string, companyId: string, value3: string, value4: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)", [value2, companyId, value3, value4]); },
        stageCreatePayrollAdjustmentEffect2(id: string, companyId: string, pay_group_id: string, countryPackId: string, engagement_id: unknown, period_start: string, period_end: string, payDate: string, originalRunId: string, userId: string, now: number, now2: number) { return database.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,scope_engagement_id,period_start,period_end,pay_date,currency,mode,status,adjustment_of_run_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'THB','production','calculating',?,?,?,?)").bind(id, companyId, pay_group_id, countryPackId, engagement_id, period_start, period_end, payDate, originalRunId, userId, now, now2); },
        stageCreatePayrollAdjustmentEffect3(id: string, companyId: string, originalRunId: string, originalItemId: string, engagement_id: unknown, sourceRef: string, source: string, value: string, userId: string, now: number) { return database.prepare("INSERT INTO hr_payroll_adjustments(run_id,company_id,original_run_id,original_item_id,engagement_id,source_ref,source_snapshot_json,source_digest,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(id, companyId, originalRunId, originalItemId, engagement_id, sourceRef, source, value, userId, now); },
        readEditPayrollAdjustmentRow(runId: string, companyId: string) { return database.prepare("SELECT a.*,r.status,r.submitted_by,r.created_by preparer,r.version,r.pay_date FROM hr_payroll_adjustments a JOIN hr_payroll_runs r ON r.id=a.run_id WHERE a.run_id=? AND a.company_id=?").bind(runId, companyId).first<Row>(); },
        stageEditPayrollAdjustmentEffect(runId: string) { return database.prepare("SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE").bind(runId); },
        guard14(key: string, value: "adjustment-edit-version", runId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','failed','preview') AND submitted_by IS NULL)", [runId, companyId, expectedVersion]); },
        stageEditPayrollAdjustmentEffect2(runId: string) { return database.prepare("UPDATE hr_payroll_adjustment_time_claims SET active=false WHERE run_id=? AND active").bind(runId); },
        stageEditPayrollAdjustmentEffect3(runId: string) { return database.prepare("UPDATE hr_payroll_adjustments SET source_revision=source_revision+1 WHERE run_id=?").bind(runId); },
        stageEditPayrollAdjustmentEffect4(now: number, runId: string) { return database.prepare("UPDATE hr_payroll_runs SET status='calculating',input_digest=NULL,totals_json='{}',version=version+1,updated_at=? WHERE id=?").bind(now, runId); },
        readPayrollAdjustmentInputsRow(id: string, company_id: string) { return database.prepare("SELECT a.*,v.id revision_id,v.taxable_arrears_minor,v.non_taxable_arrears_minor,v.social_security_wage_minor,v.time_entries_json,v.reason,v.evidence_ref FROM hr_payroll_adjustments a JOIN hr_payroll_adjustment_revisions v ON v.run_id=a.run_id AND v.revision=a.source_revision WHERE a.run_id=? AND a.company_id=? AND a.active").bind(id, company_id).first<Row>(); },
        readPayrollAdjustmentInputsEngagement(engagement_id: unknown, company_id: string) { return database.prepare("SELECT id,version,person_id,worker_type,employment_type,start_date,end_date,job_title,branch_id,department_id FROM hr_engagements WHERE id=? AND company_id=? AND status IN ('active','ended')").bind(engagement_id, company_id).first<Row>(); },
        readPayrollAdjustmentInputsPayee(value: unknown, company_id: string) { return database.prepare("SELECT id,version,legal_name,preferred_name FROM hr_people WHERE id=? AND company_id=? AND status='active' AND merged_into_id IS NULL").bind(value, company_id).first<Row>(); },
        readPayrollAdjustmentInputsPriorAdjustments(original_item_id: unknown, id: string) { return database.prepare("SELECT a.run_id,i.result_json,r.input_digest FROM hr_payroll_adjustments a JOIN hr_payroll_runs r ON r.id=a.run_id JOIN hr_payroll_items i ON i.run_id=r.id WHERE a.original_item_id=? AND r.status='posted' AND r.id<>? ORDER BY r.pay_date,r.id").bind(original_item_id, id).all<Row>(); },
        stageCalculatePayrollAdjustmentRunEffect(id: string) { return database.prepare("SELECT id FROM hr_payroll_runs WHERE id=? FOR UPDATE").bind(id); },
        guard23(key: string, value: "adjustment-run-version", id: string, companyId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','preview','failed') AND (submitted_by IS NULL OR status='calculating' AND approved_by IS NULL))", [id, companyId, version]); },
        stageCalculatePayrollAdjustmentRunEffect2(id: string) { return database.prepare("DELETE FROM hr_payroll_items WHERE run_id=?").bind(id); },
        stageCalculatePayrollAdjustmentRunEffect3(value: string, digest: string, now: number, id: string) { return database.prepare("UPDATE hr_payroll_runs SET status='preview',revision=revision+1,totals_json=?,input_digest=?,version=version+1,error_message=NULL,updated_at=? WHERE id=?").bind(value, digest, now, id); },
        stageCalculatePayrollAdjustmentRunEffect4(value: string, id: string, engagementId: string, value2: string, value3: string, gross: number, deductions: number, employer: number, net: number, now: number) { return database.prepare("INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,created_at) VALUES(?,?,?,'employee',?,?,?,?,?,?,'THB',?)").bind(value, id, engagementId, value2, value3, gross, deductions, employer, net, now); },
        readPayrollAdjustmentSourceFreshRow(id: string) {
            return database.prepare("SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=?").bind(id).first<{
                input_snapshot_json: string;
            }>();
        },
        readPayrollAdjustmentPanelRun(runId: string, companyId: string) { return database.prepare('SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?').bind(runId, companyId).first<Row>(); },
        readPayrollAdjustmentPanelAdjustments(runId: string, companyId: string) { return database.prepare('SELECT r.id,r.status,r.pay_date,r.version,a.source_ref FROM hr_payroll_runs r JOIN hr_payroll_adjustments a ON a.run_id=r.id WHERE a.original_run_id=? AND a.company_id=? ORDER BY r.created_at,r.id').bind(runId, companyId).all<Row>(); },
        readPayrollAdjustmentPanelSource(runId: string, companyId: string) { return database.prepare('SELECT a.*,v.taxable_arrears_minor,v.non_taxable_arrears_minor,v.social_security_wage_minor,v.time_entries_json,v.reason,v.evidence_ref FROM hr_payroll_adjustments a JOIN hr_payroll_adjustment_revisions v ON v.run_id=a.run_id AND v.revision=a.source_revision WHERE a.run_id=? AND a.company_id=?').bind(runId, companyId).first<Row>(); },
        readPayrollAdjustmentPanelRevisions(runId: string) { return database.prepare('SELECT revision,taxable_arrears_minor,non_taxable_arrears_minor,time_entries_json,reason,created_at FROM hr_payroll_adjustment_revisions WHERE run_id=? ORDER BY revision').bind(runId).all<Row>(); },
        readPayrollAdjustmentPanelTimeOptions(runId: string, companyId: string, period_start: unknown, period_end: unknown) { return database.prepare("SELECT t.id,t.engagement_id,t.work_date,t.overtime_hours,p.legal_name FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id JOIN hr_people p ON p.id=e.person_id JOIN hr_payroll_items i ON i.engagement_id=e.id AND i.run_id=? WHERE t.company_id=? AND t.status='approved' AND t.regular_hours=0 AND t.overtime_hours>0 AND t.work_date BETWEEN ? AND ? AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(i.input_snapshot_json::jsonb->'hours'->'entries') old WHERE old->>'id'=t.id) AND NOT EXISTS(SELECT 1 FROM hr_payroll_adjustment_time_claims c WHERE c.time_entry_id=t.id AND c.active) ORDER BY t.work_date,t.id").bind(runId, companyId, period_start, period_end).all<Row>(); },
        stagePayrollAdjustmentGuardsEffect(id: string) { return database.prepare("SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE").bind(id); },
        guard34(key: string, value: "adjustment-revision", id: string, company_id: string, sourceRevision: number, value2: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_adjustments WHERE run_id=? AND company_id=? AND source_revision=? AND source_digest=? AND active)", [id, company_id, sourceRevision, value2]); },
        guard35(key: string, value: "adjustment-current-payee", value2: string, company_id: string, value3: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND version=? AND status='active' AND merged_into_id IS NULL)", [value2, company_id, value3]); },
        guard36(key: string, value: "adjustment-current-engagement", value2: string, company_id: string, value3: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status IN ('active','ended'))", [value2, company_id, value3]); },
        guard37(key: string, value: "adjustment-prior-count", value2: string, id: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM hr_payroll_adjustments a JOIN hr_payroll_runs r ON r.id=a.run_id WHERE a.original_item_id=? AND r.status='posted' AND r.id<>?)=?", [value2, id, length]); },
        stagePayrollAdjustmentGuardsEffect2(id: unknown) { return database.prepare("SELECT id FROM hr_time_entries WHERE id=? FOR SHARE").bind(id); },
        guard39(key: string, value: string, value2: string, value3: number, id: string, sourceRevision: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries t JOIN hr_payroll_adjustment_time_claims c ON c.time_entry_id=t.id WHERE t.id=? AND t.version=? AND t.status='approved' AND c.run_id=? AND c.source_revision=? AND c.active)", [value2, value3, id, sourceRevision]); } };
    return createPayrollAdjustmentApplication({ repository, clock: { now: Date.now }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { database = provided ?? (await holdingResources(holding)).write; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, money: (actor, input) => snapshotMoney(database, actor, input), pack: run => thaiPackForRun(database, run), evidence: (run, id) => thaiEvidenceForEmployee(database, run, id), evidenceGuards: (...args) => thaiEvidenceGuards(database, ...args), moduleEnabled: (company, module) => assertProductionModuleEnabled(database, company, module), featureGuards: (...args) => productionFeatureGuard(database, ...args) });
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
export async function originalSource(db: SqlDatabase, companyId: string, runId: string, itemId: string) { return invoke(() => application(db).originalSource(companyId, runId, itemId)); }
export async function createPayrollAdjustment(actor: WorkContext, originalRunId: string, input: z.infer<typeof payrollAdjustmentCreateSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.adjustment.create", purpose: "settle-existing" }, tx => invoke(() => application(tx).createPayrollAdjustment(actor, originalRunId, input, key))); }
export async function editPayrollAdjustment(actor: WorkContext, runId: string, input: z.infer<typeof payrollAdjustmentEditSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.payroll.adjustment.edit", purpose: "settle-existing" }, tx => invoke(() => application(tx).editPayrollAdjustment(actor, runId, input, key))); }
export async function payrollAdjustmentInputs(db: SqlDatabase, run: PayrollStatutoryRun) { return invoke(() => application(db).payrollAdjustmentInputs(run)); }
export async function calculatePayrollAdjustmentItem(db: SqlDatabase, run: PayrollStatutoryRun) { return invoke(() => application(db).calculatePayrollAdjustmentItem(run)); }
export async function calculatePayrollAdjustmentRun(db: SqlDatabase, actor: WorkContext, run: PayrollStatutoryRun & {
    version: number;
}, key: string) { return invoke(() => application(db).calculatePayrollAdjustmentRun(actor, run, key)); }
export async function payrollAdjustmentSourceFresh(db: SqlDatabase, run: PayrollStatutoryRun) { return invoke(() => application(db).payrollAdjustmentSourceFresh(run)); }
export async function payrollAdjustmentPanel(actor: WorkContext, runId: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => application(tx).payrollAdjustmentPanel(actor, runId))); }
export async function payrollAdjustmentGuards(db: SqlDatabase, key: string, run: PayrollStatutoryRun, input: Awaited<ReturnType<typeof payrollAdjustmentInputs>>) { return invoke(() => application(db).payrollAdjustmentGuards(key, run, input)); }
