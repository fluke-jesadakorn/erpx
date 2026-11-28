import { ppRd21IdCompanyStatusVersionGuard } from './pp-read-tail-r21-sources';
import { ppRd26RunIdGuard } from './pp-read-tail-r26-sources';
import { readExportBytes, readIssuableItems, readIssuableRun, readOwnSlips, readPayment, readSlip } from './native-lane-8/client';
import { withPayrollPurchasedCommand, withPayrollPurchasedRead } from "@erpx/people-payroll-server/adapters/payroll-purchases";
import { nativePeopleBatchA } from '@erpx/collaboration-server/adapters/native-people-batch-a';
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/workflow-server/public/commands";
import { sha256 } from "@erpx/platform-runtime-server/public/crypto";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { BusinessRuleError } from '@erpx/platform-shared-server/index';
import { companySource } from '@erpx/organization-server/public/payroll-payslip-company';
import { activeGrantSource } from '@erpx/identity-access-server/public/payroll-payslip-grant';
import { postingSource } from '@erpx/finance-server/public/payroll-payslip-posting';
import { issuePayslipSchema } from '@erpx/people-payroll-server/domain/payroll-payslips';
import { createPayrollPayslipApplication, type PayrollPayslipRepository } from '@erpx/people-payroll-server/application/payroll-payslips';
export { issuePayslipSchema } from '@erpx/people-payroll-server/domain/payroll-payslips';
export type { PayslipSnapshot } from '@erpx/people-payroll-server/domain/payroll-payslips';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PayrollPayslipRepository<SqlStatement> = { issuableRun(runId: string, companyId: string) { return readIssuableRun(database, runId, companyId); },
        issuableItems(companyId: string, runId: string) { return readIssuableItems(database, companyId, runId); },
        company(...args) { return companySource(database).company(...args); },
        guard4(commandId: string, value: string, id: string, runId: string, companyId: string, person_id: string, recipient_user_id: string | null, input_snapshot_json: string, result_json: string, net_minor: number) { return assertion(database, commandId, value, "EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_engagements e ON e.id=i.engagement_id JOIN hr_people p ON p.id=e.person_id WHERE i.id=? AND i.run_id=? AND e.company_id=? AND e.person_id=? AND p.user_id IS NOT DISTINCT FROM ? AND i.input_snapshot_json=? AND i.result_json=? AND i.net_minor=?)", [id, runId, companyId, person_id, recipient_user_id, input_snapshot_json, result_json, net_minor]); },
        stageIssuedSlip(id: string, companyId: string, runId: string, revision: unknown, version: number, id2: string, engagement_id: string, person_id: string, recipient_user_id: string | null, pay_date: unknown, currency: unknown, gross_minor: number, deductions_minor: number, net_minor: number, sourceDigest: string, value: string, html: string, checksum: string, userId: string, commandId: string, now: number) { return nativePeopleBatchA(database).payslip({companyId,values:[id, companyId, runId, revision, version, id2, engagement_id, person_id, recipient_user_id, pay_date, currency, gross_minor, deductions_minor, net_minor, sourceDigest, value, html, checksum, userId, commandId, now]}); },
        activeGrant(...args) { return activeGrantSource(database).activeGrant(...args); },
        ownSlips(companyId: string, _value: string[], value2: string[], runId: string | undefined, broad: boolean) { return readOwnSlips(database, companyId, runId, broad ? undefined : value2[0]); },
        slip(id: string, companyId: string, value: string[], broad: boolean) { return readSlip(database, id, companyId, broad ? undefined : value[0]); },
        payment(companyId: string, run_id: string, person_id: string) { return readPayment(database, companyId, run_id, person_id); },
        exportBytes(id: string, companyId: string) { return readExportBytes(database, id, companyId); },
        posting(...args) { return postingSource(database).posting(...args); },
        guard12(commandId: string, value: "payslip-source", runId: string, companyId: string, expectedVersion: number) { return assertion(database, commandId, value, ppRd21IdCompanyStatusVersionGuard('hr_payroll_runs', 'posted'), [runId, companyId, expectedVersion]); },
        guard13(commandId: string, value: "payslips-not-issued", runId: string) { return assertion(database, commandId, value, ppRd26RunIdGuard('hr_issued_payslips'), [runId]); } };
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
