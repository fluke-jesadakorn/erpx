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
import { BusinessRuleError } from '@erpx/server-shared/index';
import { companySource } from '@erpx/server-organization/public/payroll-payslip-company';
import { activeGrantSource } from '@erpx/server-identity_access/public/payroll-payslip-grant';
import { postingSource } from '@erpx/server-finance/public/payroll-payslip-posting';
import { type Row, type PayslipItem, type Slip, issuePayslipSchema } from '@erpx/server-people_payroll/domain/payroll-payslips';
import { createPayrollPayslipApplication, type PayrollPayslipRepository } from '@erpx/server-people_payroll/application/payroll-payslips';
export { issuePayslipSchema } from '@erpx/server-people_payroll/domain/payroll-payslips';
export type { PayslipSnapshot } from '@erpx/server-people_payroll/domain/payroll-payslips';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PayrollPayslipRepository<SqlStatement> = { issuableRun(runId: string, companyId: string) { return database.prepare("SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=? AND status IN ('approved','posted')").bind(runId, companyId).first<Row>(); },
        issuableItems(companyId: string, runId: string) { return database.prepare("SELECT i.*,e.person_id,p.user_id recipient_user_id FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id AND e.company_id=? JOIN hr_people p ON p.id=e.person_id AND p.company_id=e.company_id WHERE i.run_id=? AND i.worker_type='employee' AND i.status IN ('calculated','approved','posted') ORDER BY i.id").bind(companyId, runId).all<PayslipItem>(); },
        company(...args) { return companySource(database).company(...args); },
        guard4(commandId: string, value: string, id: string, runId: string, companyId: string, person_id: string, recipient_user_id: string | null, input_snapshot_json: string, result_json: string, net_minor: number) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.id=? AND i.run_id=? AND e.company_id=? AND e.person_id=? AND p.user_id IS NOT DISTINCT FROM ? AND i.input_snapshot_json=? AND i.result_json=? AND i.net_minor=?)", [id, runId, companyId, person_id, recipient_user_id, input_snapshot_json, result_json, net_minor]); },
        stageIssuedSlip(id: string, companyId: string, runId: string, revision: unknown, version: number, id2: string, engagement_id: string, person_id: string, recipient_user_id: string | null, pay_date: unknown, currency: unknown, gross_minor: number, deductions_minor: number, net_minor: number, sourceDigest: string, value: string, html: string, checksum: string, userId: string, commandId: string, now: number) { return database.prepare("INSERT INTO hr_issued_payslips(id,company_id,run_id,run_revision,run_version,item_id,engagement_id,person_id,recipient_user_id,pay_date,currency,gross_minor,deductions_minor,net_minor,source_digest,snapshot_json,export_html,export_sha256,issued_by,command_id,issued_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, companyId, runId, revision, version, id2, engagement_id, person_id, recipient_user_id, pay_date, currency, gross_minor, deductions_minor, net_minor, sourceDigest, value, html, checksum, userId, commandId, now); },
        activeGrant(...args) { return activeGrantSource(database).activeGrant(...args); },
        ownSlips(companyId: string, value: string[], value2: string[], runId: string | undefined, broad: boolean) { return database.prepare(`SELECT s.id,s.run_id,s.run_revision,s.pay_date,s.currency,s.gross_minor,s.deductions_minor,s.net_minor,s.source_digest,s.export_sha256,s.issued_at,s.person_id,s.snapshot_json FROM hr_issued_payslips s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.company_id=? ${runId ? "AND s.run_id=?" : ""} ${broad ? "" : "AND s.recipient_user_id=? AND p.user_id=? AND p.merged_into_id IS NULL"} ORDER BY s.pay_date DESC,s.issued_at DESC,s.id`).bind(companyId, ...value, ...value2).all<Row>(); },
        slip(id: string, companyId: string, value: string[], broad: boolean) { return database.prepare(`SELECT s.* FROM hr_issued_payslips s JOIN hr_people p ON p.id=s.person_id AND p.company_id=s.company_id WHERE s.id=? AND s.company_id=? ${broad ? "" : "AND s.recipient_user_id=? AND p.user_id=? AND p.merged_into_id IS NULL"}`).bind(id, companyId, ...value).first<Slip>(); },
        payment(companyId: string, run_id: string, person_id: string) { return database.prepare("SELECT i.status,i.settled_at,b.status batch_status FROM hr_payment_batch_items i JOIN hr_payment_batches b ON b.id=i.batch_id WHERE b.company_id=? AND b.source_type='payroll' AND b.source_id=? AND i.person_id=? AND b.status!='cancelled' ORDER BY b.created_at DESC LIMIT 1").bind(companyId, run_id, person_id).first<Row>(); },
        exportBytes(id: string, companyId: string) {
            return database.prepare("SELECT export_html,export_sha256 FROM hr_issued_payslips WHERE id=? AND company_id=?").bind(id, companyId).first<{
                export_html: string;
                export_sha256: string;
            }>();
        },
        posting(...args) { return postingSource(database).posting(...args); },
        guard12(commandId: string, value: "payslip-source", runId: string, companyId: string, expectedVersion: number) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)", [runId, companyId, expectedVersion]); },
        guard13(commandId: string, value: "payslips-not-issued", runId: string) { return assertion(database, commandId, value, "NOT EXISTS(SELECT 1 FROM hr_issued_payslips WHERE run_id=?)", [runId]); } };
    return createPayrollPayslipApplication({ repository, clock: { now: Date.now }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { database = provided ?? (await holdingResources(holding)).write; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) } });
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
export async function preparePayslipIssue(db: SqlDatabase, actor: WorkContext, runId: string, journalId: string, commandId: string, postedVersion?: number) { return invoke(() => application(db).preparePayslipIssue(actor, runId, journalId, commandId, postedVersion)); }
export async function listPayslips(actor: WorkContext, { runId, ownOnly = true }: {
    runId?: string;
    ownOnly?: boolean;
} = {}) { return withPayrollPurchasedRead(actor, tx => invoke(() => application(tx).listPayslips(actor, { runId, ownOnly }))); }
export async function getPayslip(actor: WorkContext, id: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => application(tx).getPayslip(actor, id))); }
export async function getPayslipExport(actor: WorkContext, id: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => application(tx).getPayslipExport(actor, id))); }
export async function issuePostedPayslips(actor: WorkContext, runId: string, input: z.infer<typeof issuePayslipSchema>, commandId: string) { return withPayrollPurchasedCommand(actor, { key: commandId, operation: "people.payroll.payslips.issue", purpose: "history" }, tx => invoke(() => application(tx).issuePostedPayslips(actor, runId, input, commandId))); }
