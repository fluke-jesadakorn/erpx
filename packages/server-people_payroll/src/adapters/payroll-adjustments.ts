import { withPayrollPurchasedCommand, withPayrollPurchasedRead } from "@erpx/server-people_payroll/adapters/payroll-purchases";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import {snapshotMoney} from '@erpx/server-finance/public/company-money';
import { assertProductionModuleEnabled, productionFeatureGuard } from "@erpx/server-pms/public/feature-availability";
import { thaiEvidenceForEmployee, thaiEvidenceGuards, thaiPackForRun, type PayrollStatutoryRun } from "@erpx/server-people_payroll/adapters/payroll-statutory";
import { BusinessRuleError } from '@erpx/server-shared/index';
import { type Row, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from '@erpx/server-people_payroll/domain/payroll-adjustments';
import { createPayrollAdjustmentApplication, type PayrollAdjustmentRepository } from '@erpx/server-people_payroll/application/payroll-adjustments';
export { payrollAdjustmentValuesSchema, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema } from '@erpx/server-people_payroll/domain/payroll-adjustments';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PayrollAdjustmentRepository<SqlStatement> = { readOriginalSourceRun(runId: string, companyId: string) {
            return database.prepare("SELECT 1").bind().first<{
                source: string;
            }>();
        },
        readOriginalSourceItem(itemId: string, runId: string) {
            return database.prepare("SELECT 1").bind().first<{
                source: string;
            }>();
        },
        readOriginalSourceSlips(runId: string, itemId: string) { return database.prepare("SELECT 1").bind().all<Row>(); },
        readTimeSourcesEntries(engagement_id: unknown, company_id: unknown, ids: string[]) { return database.prepare(`SELECT id,version,work_date,regular_hours,overtime_hours,cost_assignment_id,cost_branch_id,cost_department_id FROM hr_time_entries WHERE engagement_id=? AND company_id=? AND status='approved' ${ids.length ? `AND id IN (${ids.map(() => "?").join(",")})` : "AND FALSE"} ORDER BY id`).bind(engagement_id, company_id, ...ids).all<Row>(); },
        stageRevisionStatementsEffect(id: string, runId: string, revision: number, taxableArrearsMinor: number, nonTaxableArrearsMinor: number, socialSecurityWageMinor: number, value: string, reason: string, evidenceRef: string, userId: string, key: string, now: number) { return database.prepare("SELECT 1").bind(); },
        stageRevisionStatementsEffect2(runId: string, revision: number, timeId: string) { return database.prepare("SELECT 1").bind(); },
        stageCreatePayrollAdjustmentEffect(value: string) { return database.prepare("SELECT 1").bind(); },
        guard8(key: string, value: "adjustment-original", originalRunId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)", [originalRunId, companyId, expectedVersion]); },
        guard9(key: string, value: string, value2: string, companyId: string, value3: string, value4: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)", [value2, companyId, value3, value4]); },
        stageCreatePayrollAdjustmentEffect2(id: string, companyId: string, pay_group_id: string, countryPackId: string, engagement_id: unknown, period_start: string, period_end: string, payDate: string, originalRunId: string, userId: string, now: number, now2: number) { return database.prepare("SELECT 1").bind(); },
        stageCreatePayrollAdjustmentEffect3(id: string, companyId: string, originalRunId: string, originalItemId: string, engagement_id: unknown, sourceRef: string, source: string, value: string, userId: string, now: number) { return database.prepare("SELECT 1").bind(); },
        readEditPayrollAdjustmentRow(runId: string, companyId: string) { return database.prepare("SELECT 1").bind().first<Row>(); },
        stageEditPayrollAdjustmentEffect(runId: string) { return database.prepare("SELECT 1").bind(); },
        guard14(key: string, value: "adjustment-edit-version", runId: string, companyId: string, expectedVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','failed','preview') AND submitted_by IS NULL)", [runId, companyId, expectedVersion]); },
        stageEditPayrollAdjustmentEffect2(runId: string) { return database.prepare("SELECT 1").bind(); },
        stageEditPayrollAdjustmentEffect3(runId: string) { return database.prepare("SELECT 1").bind(); },
        stageEditPayrollAdjustmentEffect4(now: number, runId: string) { return database.prepare("SELECT 1").bind(); },
        readPayrollAdjustmentInputsRow(id: string, company_id: string) { return database.prepare("SELECT 1").bind().first<Row>(); },
        readPayrollAdjustmentInputsEngagement(engagement_id: unknown, company_id: string) { return database.prepare("SELECT 1").bind().first<Row>(); },
        readPayrollAdjustmentInputsPayee(value: unknown, company_id: string) { return database.prepare("SELECT 1").bind().first<Row>(); },
        readPayrollAdjustmentInputsPriorAdjustments(original_item_id: unknown, id: string) { return database.prepare("SELECT 1").bind().all<Row>(); },
        stageCalculatePayrollAdjustmentRunEffect(id: string) { return database.prepare("SELECT 1").bind(); },
        guard23(key: string, value: "adjustment-run-version", id: string, companyId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND version=? AND status IN ('draft','calculating','preview','failed') AND (submitted_by IS NULL OR status='calculating' AND approved_by IS NULL))", [id, companyId, version]); },
        stageCalculatePayrollAdjustmentRunEffect2(id: string) { return database.prepare("SELECT 1").bind(); },
        stageCalculatePayrollAdjustmentRunEffect3(value: string, digest: string, now: number, id: string) { return database.prepare("SELECT 1").bind(); },
        stageCalculatePayrollAdjustmentRunEffect4(value: string, id: string, engagementId: string, value2: string, value3: string, gross: number, deductions: number, employer: number, net: number, now: number) { return database.prepare("SELECT 1").bind(); },
        readPayrollAdjustmentSourceFreshRow(id: string) {
            return database.prepare("SELECT 1").bind().first<{
                input_snapshot_json: string;
            }>();
        },
        readPayrollAdjustmentPanelRun(runId: string, companyId: string) { return database.prepare("SELECT 1").bind().first<Row>(); },
        readPayrollAdjustmentPanelAdjustments(runId: string, companyId: string) { return database.prepare("SELECT 1").bind().all<Row>(); },
        readPayrollAdjustmentPanelSource(runId: string, companyId: string) { return database.prepare("SELECT 1").bind().first<Row>(); },
        readPayrollAdjustmentPanelRevisions(runId: string) { return database.prepare("SELECT 1").bind().all<Row>(); },
        readPayrollAdjustmentPanelTimeOptions(runId: string, companyId: string, period_start: unknown, period_end: unknown) { return database.prepare("SELECT 1").bind().all<Row>(); },
        stagePayrollAdjustmentGuardsEffect(id: string) { return database.prepare("SELECT 1").bind(); },
        guard34(key: string, value: "adjustment-revision", id: string, company_id: string, sourceRevision: number, value2: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_adjustments WHERE run_id=? AND company_id=? AND source_revision=? AND source_digest=? AND active)", [id, company_id, sourceRevision, value2]); },
        guard35(key: string, value: "adjustment-current-payee", value2: string, company_id: string, value3: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_people WHERE id=? AND company_id=? AND version=? AND status='active' AND merged_into_id IS NULL)", [value2, company_id, value3]); },
        guard36(key: string, value: "adjustment-current-engagement", value2: string, company_id: string, value3: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status IN ('active','ended'))", [value2, company_id, value3]); },
        guard37(key: string, value: "adjustment-prior-count", value2: string, id: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM hr_payroll_adjustments a JOIN hr_payroll_runs r ON r.id=a.run_id WHERE a.original_item_id=? AND r.status='posted' AND r.id<>?)=?", [value2, id, length]); },
        stagePayrollAdjustmentGuardsEffect2(id: unknown) { return database.prepare("SELECT 1").bind(); },
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
