import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { ApiError } from "@/server/modules/runtime/public/http";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { assertion, executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import { prepareJournal } from "@/server/modules/finance/public/finance";
import { assertStatutoryRunReady } from "@/server/payroll-statutory";
import type { SqlStatement } from '../../runtime/public/runtime/types';
import { BusinessRuleError } from '../../shared';
import { payrollRemittanceSources } from '../../finance/public/payroll-remittance-sources';
import { payrollRemittanceSchema } from '../domain/payroll-remittance';
import { createPayrollRemittanceApplication, type PayrollRemittanceRepository } from '../application/payroll-remittance';
export { payrollRemittanceSchema } from '../domain/payroll-remittance';
function application() {
    let database: SqlDatabase;
    const repository: PayrollRemittanceRepository<SqlStatement> = { postedRun(...args) { return payrollRemittanceSources(database).postedRun(...args); },
        credits(...args) { return payrollRemittanceSources(database).credits(...args); },
        remitted(id: string, kind: "withholding_tax" | "social_security") {
            return database.prepare(`SELECT COALESCE(SUM(amount_minor),0) AS amount_minor FROM hr_payroll_statutory_remittances
    WHERE run_id=? AND kind=?`).bind(id, kind).first<{
                amount_minor: number;
            }>();
        },
        bankTransaction(...args) { return payrollRemittanceSources(database).bankTransaction(...args); },
        bankAccount(...args) { return payrollRemittanceSources(database).bankAccount(...args); },
        periods(...args) { return payrollRemittanceSources(database).periods(...args); },
        lockRun(runId: string, companyId: string) { return database.prepare("SELECT id FROM hr_payroll_runs WHERE id=? AND company_id=? FOR UPDATE").bind(runId, companyId); },
        guard8(key: string, value: "payroll-posted", runId: string, companyId: string, expectedRunVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)", [runId, companyId, expectedRunVersion]); },
        guard9(...args) { return payrollRemittanceSources(database).guard9(...args); },
        guard10(...args) { return payrollRemittanceSources(database).guard10(...args); },
        insertRemittance(remittanceId: string, companyId: string, runId: string, kind: "withholding_tax" | "social_security", amountMinor: number, accountId: string, bankAccountId: string, bankTransactionId: string, id: string, bankReference: string, userId: string, now: number) {
            return database.prepare(`INSERT INTO hr_payroll_statutory_remittances(id,company_id,run_id,kind,amount_minor,liability_account_id,
      bank_account_id,bank_transaction_id,journal_id,bank_reference,recorded_by,recorded_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
                .bind(remittanceId, companyId, runId, kind, amountMinor, accountId, bankAccountId, bankTransactionId, id, bankReference, userId, now);
        },
        matchBank(...args) { return payrollRemittanceSources(database).matchBank(...args); } };
    const ports = { repository, clock: { now: Date.now }, production: async () => (await runtimeEnv()).ERPX_ENVIRONMENT === 'production', authorize: requirePermission, prepare: async (holding: string) => { database = (await holdingResources(holding)).write; }, commands: { replay: <Result = Record<string, unknown>>(...args: Parameters<import('../application/payroll-statutory').PayrollStatutoryCommands<SqlStatement>['replay']>) => replayCommand<Result>(database, ...args), commit: <Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: SqlStatement[]) => executeCommand(database, actor, key, operation, input, id, version, result, effects) }, statutoryReady: (id: string) => assertStatutoryRunReady(database, id), journal: (...args: Parameters<import('../application/payroll-remittance').PayrollRemittancePorts<SqlStatement>['journal']>) => prepareJournal(database, ...args) };
    return createPayrollRemittanceApplication(ports);
}
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function getPayrollLiabilities(actor: WorkContext, runId: string) { return invoke(() => application().getPayrollLiabilities(actor, runId)); }
export async function remitPayrollLiability(actor: WorkContext, runId: string, input: z.infer<typeof payrollRemittanceSchema>, key: string) { return invoke(() => application().remitPayrollLiability(actor, runId, input, key)); }
