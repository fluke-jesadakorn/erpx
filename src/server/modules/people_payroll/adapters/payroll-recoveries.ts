import { withPayrollPurchasedCommand, withPayrollPurchasedRead } from "./payroll-purchases";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { snapshotMoney } from "@/server/modules/finance/public/company-money";
import { sha256 } from "@/server/modules/runtime/public/crypto";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { assertProductionModuleEnabled, productionFeatureGuard } from "@/server/modules/pms/public/hospitality-rollout";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { originalSource } from "@/server/modules/people_payroll/adapters/payroll-adjustments";
import { BusinessRuleError } from '../../shared';
import { payrollRecoveryGrant } from '../../identity_access/public/payroll-recovery-grant';
import { payrollRecoveryChart } from '../../finance/public/payroll-recovery-chart';
import { type Row, payrollRecoveryRequestSchema, payrollRecoveryConsentSchema, payrollRecoveryReviewSchema, payrollRecoveryRecognizeSchema } from '../domain/payroll-recoveries';
import { createPayrollRecoveryApplication, type PayrollRecoveryRepository } from '../application/payroll-recoveries';
export { payrollRecoveryPolicy, payrollRecoveryRequestSchema, payrollRecoveryConsentSchema, payrollRecoveryReviewSchema, payrollRecoveryRecognizeSchema } from '../domain/payroll-recoveries';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PayrollRecoveryRepository<SqlStatement> = { readCurrentPermissionEffect(...args) { return payrollRecoveryGrant(database).readCurrentPermissionEffect(...args); },
        guard2(...args) { return payrollRecoveryGrant(database).guard2(...args); },
        stageEventEffect(value: string, id: string, companyId: string, action: string, userId: string, version: number, value2: string, key: string, now: number) { return database.prepare('INSERT INTO hr_payroll_recovery_events(id,recovery_id,company_id,action,actor_id,recovery_version,evidence_json,command_id,created_at) VALUES(?,?,?,?,?,?,?,?,?)').bind(value, id, companyId, action, userId, version, value2, key, now); },
        readGetRowRow(id: string, companyId: string) { return database.prepare('SELECT * FROM hr_payroll_recoveries WHERE id=? AND company_id=?').bind(id, companyId).first<Row>(); },
        readRequestPayrollRecoveryEmployee(engagement_id: unknown, companyId: string) {
            return database.prepare("SELECT e.person_id,p.user_id,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.id=? AND e.company_id=? AND e.status='active' AND p.status='active' AND p.user_id IS NOT NULL").bind(engagement_id, companyId).first<{
                person_id: string;
                user_id: string;
                legal_name: string;
            }>();
        },
        readRequestPayrollRecoveryPaid(companyId: string, runId: string, value: string) { return database.prepare("SELECT b.id batch_id,s.id settlement_id,s.journal_id,i.reference,i.amount_minor,i.settled_at FROM hr_payment_batches b JOIN hr_payment_batch_items i ON i.batch_id=b.id JOIN hr_payroll_payment_settlements s ON s.batch_id=b.id AND s.batch_item_id=i.id WHERE b.company_id=? AND b.source_type='payroll' AND b.source_id=? AND b.status='settled' AND i.person_id=? AND i.status='settled' ORDER BY s.id").bind(companyId, runId, value).all<Row>(); },
        stageRequestPayrollRecoveryEffect(value: string) { return database.prepare('SELECT pg_advisory_xact_lock(hashtextextended(?,0))').bind(value); },
        guard8(key: string, value: "recovery-source-bound", amountMinor: number, originalItemId: string, originalItemId2: string) { return assertion(database, key, value, "?+(SELECT COALESCE(SUM(amount_minor),0) FROM hr_payroll_recoveries WHERE original_item_id=? AND status NOT IN ('declined','rejected','cancelled'))<=(SELECT net_minor FROM hr_payroll_items WHERE id=? AND status='posted')", [amountMinor, originalItemId, originalItemId2]); },
        guard9(key: string, value: "recovery-first-period", companyId: string, value2: string, firstDeductionPeriod: string, firstDeductionPeriod2: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND adjustment_of_run_id IS NULL AND status='posted' AND period_start<=? AND period_end>=?)", [companyId, value2, firstDeductionPeriod, firstDeductionPeriod2]); },
        stageRequestPayrollRecoveryEffect2(id: string, companyId: string, runId: string, originalItemId: string, engagement_id: unknown, person_id: string, user_id: string, sourceRef: string, source: string, value: string, policy: string, value2: string, amountMinor: number, installmentCount: number, firstDeductionPeriod: string, reason: string, evidenceRef: string, userId: string, now: number, now2: number) { return database.prepare('INSERT INTO hr_payroll_recoveries(id,company_id,original_run_id,original_item_id,engagement_id,person_id,employee_user_id,source_ref,source_snapshot_json,source_digest,policy_json,policy_digest,amount_minor,currency,installment_count,first_deduction_period,reason,evidence_ref,requested_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,\'THB\',?,?,?,?,?,?,?)').bind(id, companyId, runId, originalItemId, engagement_id, person_id, user_id, sourceRef, source, value, policy, value2, amountMinor, installmentCount, firstDeductionPeriod, reason, evidenceRef, userId, now, now2); },
        readConsentPayrollRecoveryName(person_id: unknown, companyId: string, userId: string) { return database.prepare("SELECT legal_name FROM hr_people WHERE id=? AND company_id=? AND user_id=? AND status='active' AND merged_into_id IS NULL").bind(person_id, companyId, userId).first<string>('legal_name'); },
        guard12(key: string, value: "recovery-own-consent", id: string, companyId: string, expectedVersion: number, userId: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_recoveries WHERE id=? AND company_id=? AND version=? AND status='consent_pending' AND employee_user_id=?)", [id, companyId, expectedVersion, userId]); },
        stageConsentPayrollRecoveryEffect(status: "review_pending" | "declined", value: string | null, value2: number | null, html: string | null, checksum: string | null, now: number, id: string) { return database.prepare('UPDATE hr_payroll_recoveries SET status=?,version=version+1,consent_by=?,consent_at=?,consent_html=?,consent_sha256=?,updated_at=? WHERE id=?').bind(status, value, value2, html, checksum, now, id); },
        guard14(key: string, value: "recovery-review-version", id: string, companyId: string, expectedVersion: number, value2: string) { return assertion(database, key, value, 'EXISTS(SELECT 1 FROM hr_payroll_recoveries WHERE id=? AND company_id=? AND version=? AND status=?)', [id, companyId, expectedVersion, value2]); },
        stageReviewPayrollRecoveryEffect(status: "approved" | "rejected" | "cancelled", value: string | null, value2: string | null, now: number, id: string) { return database.prepare('UPDATE hr_payroll_recoveries SET status=?,version=version+1,reviewed_by=?,legal_basis_ref=?,updated_at=? WHERE id=?').bind(status, value, value2, now, id); },
        readRecognizePayrollRecoveryMapping(companyId: string, pay_group_id: unknown) {
            return database.prepare("SELECT id,version,chart_id,accounts_json FROM hr_payroll_account_mappings WHERE company_id=? AND pay_group_id=? AND status='approved' ORDER BY created_at DESC LIMIT 1").bind(companyId, pay_group_id).first<{
                id: string;
                version: number;
                chart_id: string;
                accounts_json: string;
            }>();
        },
        readRecognizePayrollRecoveryChart(...args) { return payrollRecoveryChart(database).readRecognizePayrollRecoveryChart(...args); },
        stageRecognizePayrollRecoverySteps(value: string) { return database.prepare('SELECT pg_advisory_xact_lock(hashtextextended(?,0))').bind(value); },
        stageRecognizePayrollRecoverySteps2(id: string) { return database.prepare('SELECT id FROM hr_payroll_recoveries WHERE id=? FOR UPDATE').bind(id); },
        guard20(key: string, value: "recovery-recognition-version", id: string, companyId: string, expectedVersion: number, value2: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_recoveries WHERE id=? AND company_id=? AND version=? AND status='approved' AND consent_sha256=?)", [id, companyId, expectedVersion, value2]); },
        guard21(key: string, value: "recovery-current-mapping", id: string, companyId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_account_mappings WHERE id=? AND company_id=? AND version=? AND status='approved')", [id, companyId, version]); },
        guard22(key: string, value: "recovery-current-schedule", companyId: string, value2: string, value3: string, value4: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_payroll_runs WHERE company_id=? AND pay_group_id=? AND adjustment_of_run_id IS NULL AND status='posted' AND period_start<=? AND period_end>=?)", [companyId, value2, value3, value4]); },
        stageRecognizePayrollRecoverySteps3(advanceId: string, companyId: string, engagement_id: unknown, person_id: unknown, amount: number, count: number, first_deduction_period: unknown, value: string, employee_user_id: unknown, reviewed_by: unknown, legal_basis_ref: unknown, now: number, now2: number, now3: number, id: string) { return database.prepare("INSERT INTO hr_payroll_advances(id,company_id,engagement_id,person_id,status,version,amount_minor,installment_count,first_deduction_period,currency,reason,requested_by,approved_by,review_reason,created_at,reviewed_at,updated_at,recovery_id) VALUES(?,?,?,?,'approved',2,?,?,?,'THB',?,?,?,?,?,?,?,?)").bind(advanceId, companyId, engagement_id, person_id, amount, count, first_deduction_period, value, employee_user_id, reviewed_by, legal_basis_ref, now, now2, now3, id); },
        stageRecognizePayrollRecoverySteps4(id: string, advanceId: string, userId: string, now: number, id2: string) { return database.prepare("UPDATE hr_payroll_recoveries SET status='recognized',version=version+1,journal_id=?,advance_id=?,recognized_by=?,updated_at=? WHERE id=?").bind(id, advanceId, userId, now, id2); },
        stageRecognizePayrollRecoveryEffect(value: string, advanceId: string, companyId: string, value2: number, value3: string, value4: string, value5: number, now: number) { return database.prepare('INSERT INTO hr_payroll_advance_repayments(id,advance_id,company_id,sequence,due_period_start,due_period_end,amount_minor,created_at) VALUES(?,?,?,?,?,?,?,?)').bind(value, advanceId, companyId, value2, value3, value4, value5, now); },
        readPayrollRecoveryPageItems(companyId: string, value: string[], broad: boolean) { return database.prepare(`SELECT r.*,p.legal_name FROM hr_payroll_recoveries r JOIN hr_people p ON p.id=r.person_id AND p.company_id=r.company_id WHERE r.company_id=? ${broad ? '' : 'AND r.employee_user_id=? AND p.user_id=? AND p.merged_into_id IS NULL'} ORDER BY r.created_at DESC,r.id`).bind(companyId, ...value).all<Row>(); },
        readPayrollRecoveryPageEvents(companyId: string, value: unknown[], items: Row[]) { return database.prepare(`SELECT * FROM hr_payroll_recovery_events WHERE company_id=? AND recovery_id IN (${items.map(() => '?').join(',')}) ORDER BY created_at,id`).bind(companyId, ...value).all<Row>(); },
        readAssertPayrollRecoveryDeductionLimitsRows(value: string[], company_id: string, repayments: {
            id: string;
            amount_minor: number;
        }[]) { return database.prepare(`SELECT r.id,r.amount_minor,c.policy_json,c.consent_sha256,c.status,c.source_ref FROM hr_payroll_advance_repayments r JOIN hr_payroll_advances a ON a.id=r.advance_id JOIN hr_payroll_recoveries c ON c.id=a.recovery_id WHERE r.id IN (${repayments.map(() => '?').join(',')}) AND c.company_id=?`).bind(...value, company_id).all<Row>(); } };
    return createPayrollRecoveryApplication({ repository, clock: { now: Date.now }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { database = provided ?? (await holdingResources(holding)).write; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) }, money: (actor, input) => snapshotMoney(database, actor, input), original: (...args) => originalSource(database, ...args), journal: (...args) => prepareJournal(database, ...args), moduleEnabled: (company, module) => assertProductionModuleEnabled(database, company, module), featureGuards: (...args) => productionFeatureGuard(database, ...args) });
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
export async function requestPayrollRecovery(actor: WorkContext, runId: string, input: z.infer<typeof payrollRecoveryRequestSchema>, key: string) { return withPayrollPurchasedCommand(actor, {key,operation:"people.payroll-recovery.request",purpose:"settle-existing"}, tx => invoke(() => application(tx).requestPayrollRecovery(actor, runId, input, key))); }
export async function consentPayrollRecovery(actor: WorkContext, id: string, input: z.infer<typeof payrollRecoveryConsentSchema>, key: string) { return withPayrollPurchasedCommand(actor, {key,operation:"people.payroll-recovery.consent",purpose:"settle-existing"}, tx => invoke(() => application(tx).consentPayrollRecovery(actor, id, input, key))); }
export async function reviewPayrollRecovery(actor: WorkContext, id: string, input: z.infer<typeof payrollRecoveryReviewSchema>, key: string) { return withPayrollPurchasedCommand(actor, {key,operation:"people.payroll-recovery.review",purpose:"settle-existing"}, tx => invoke(() => application(tx).reviewPayrollRecovery(actor, id, input, key))); }
export async function recognizePayrollRecovery(actor: WorkContext, id: string, input: z.infer<typeof payrollRecoveryRecognizeSchema>, key: string) { return withPayrollPurchasedCommand(actor, {key,operation:"people.payroll-recovery.recognize",purpose:"settle-existing"}, tx => invoke(() => application(tx).recognizePayrollRecovery(actor, id, input, key))); }
export async function payrollRecoveryPage(actor: WorkContext) { return withPayrollPurchasedRead(actor, tx => invoke(() => application(tx).payrollRecoveryPage(actor))); }
export async function payrollRecoveryAgreement(actor: WorkContext, id: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => application(tx).payrollRecoveryAgreement(actor, id))); }
export async function assertPayrollRecoveryDeductionLimits(db: SqlDatabase, run: {
    id: string;
    company_id: string;
    period_start: string;
    period_end: string;
    pay_date?: string;
}, repayments: Array<{
    id: string;
    amount_minor: number;
}>, grossMinor: number, otherDeductionMinor: number) { return invoke(() => application(db).assertPayrollRecoveryDeductionLimits(run, repayments, grossMinor, otherDeductionMinor)); }
