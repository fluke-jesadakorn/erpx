import { z } from 'zod';
import { hasPermission } from "@/lib/identity_access/permissions";
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock } from '../../shared';
import type { PayrollStatutoryCommands } from './payroll-statutory';
import { type Row, type PayslipItem, type PayslipSnapshot, type Slip, issuePayslipSchema, render } from '../domain/payroll-payslips';
export interface PayrollPayslipRepository<Effect> {
    issuableRun(runId: string, companyId: string): Promise<Row | null>;
    issuableItems(companyId: string, runId: string): Promise<{
        results: PayslipItem[];
    }>;
    company(companyId: string): Promise<{
        display_name: string;
    } | null>;
    guard4(commandId: string, value: string, id: string, runId: string, companyId: string, person_id: string, recipient_user_id: string | null, input_snapshot_json: string, result_json: string, net_minor: number): Effect;
    stageIssuedSlip(id: string, companyId: string, runId: string, revision: unknown, version: number, id2: string, engagement_id: string, person_id: string, recipient_user_id: string | null, pay_date: unknown, currency: unknown, gross_minor: number, deductions_minor: number, net_minor: number, sourceDigest: string, value: string, html: string, checksum: string, userId: string, commandId: string, now: number): Effect;
    activeGrant(assignmentId: string, companyId: string, userId: string, role: string, now: number, now2: number, permission: "payroll.read" | "self.requests", branchId: string | null, departmentId: string | null): Promise<Record<string, unknown> | null>;
    ownSlips(companyId: string, value: string[], value2: string[], runId: string | undefined, broad: boolean): Promise<{
        results: Row[];
    }>;
    slip(id: string, companyId: string, value: string[], broad: boolean): Promise<Slip | null>;
    payment(companyId: string, run_id: string, person_id: string): Promise<Row | null>;
    exportBytes(id: string, companyId: string): Promise<{
        export_html: string;
        export_sha256: string;
    } | null>;
    posting(runId: string, companyId: string, expectedVersion: number): Promise<{
        journal_id: string;
        version: number;
    } | null>;
    guard12(commandId: string, value: "payslip-source", runId: string, companyId: string, expectedVersion: number): Effect;
    guard13(commandId: string, value: "payslips-not-issued", runId: string): Effect;
}
export interface PayrollPayslipPorts<Effect> {
    repository: PayrollPayslipRepository<Effect>;
    clock: Clock;
    hash(value: string): Promise<string>;
    authorize(actor: WorkContext, permission: string): WorkContext;
    prepare(holdingId: string): Promise<void>;
    commands: PayrollStatutoryCommands<Effect>;
}
export function createPayrollPayslipApplication<Effect>(ports: PayrollPayslipPorts<Effect>) {
    const repository = ports.repository;
    async function preparePayslipIssue(actor: WorkContext, runId: string, journalId: string, commandId: string, postedVersion?: number): Promise<{
        ids: string[];
        guards: Effect[];
        steps: Effect[];
    }> {
        const run = await repository.issuableRun(runId, actor.companyId);
        if (!run || !run.input_digest)
            throw new BusinessRuleError(409, "PAYSLIP_SOURCE_NOT_POSTABLE", "A retained payroll calculation and independent approval are required before issuing employee slips.");
        const items = (await repository.issuableItems(actor.companyId, runId)).results;
        const company = await repository.company(actor.companyId), now = ports.clock.now(), guards: Effect[] = [], steps: Effect[] = [], ids: string[] = [];
        if (!items.length)
            throw new BusinessRuleError(409, "PAYSLIP_SOURCE_EMPTY", "The posted run needs calculated employee items.");
        for (const item of items) {
            const input = JSON.parse(item.input_snapshot_json) as Row, result = JSON.parse(item.result_json) as Row, payee = input.payee as {
                id?: string;
                legal_name?: string;
            } | undefined, engagement = input.engagement as {
                job_title?: string;
            } | undefined;
            if (payee?.id !== item.person_id || !payee.legal_name || Number(item.gross_minor) - Number(item.deductions_minor) !== Number(item.net_minor))
                throw new BusinessRuleError(409, "PAYSLIP_SOURCE_CHANGED", "The employee identity and exact totals must match the retained payroll input.");
            const sourceDigest = await ports.hash(JSON.stringify({ runInputDigest: run.input_digest, itemId: item.id, input: item.input_snapshot_json, result: item.result_json })), id = `payslip-${runId}-${item.id}`, snapshot: PayslipSnapshot = { schema: "payroll.payslip.v1", company: { id: actor.companyId, name: company?.display_name ?? actor.companyId }, employee: { personId: item.person_id, engagementId: item.engagement_id, name: payee.legal_name, jobTitle: engagement?.job_title ?? "" }, run: { id: runId, revision: Number(run.revision), version: postedVersion ?? Number(run.version), mode: String(run.mode), periodStart: String(run.period_start), periodEnd: String(run.period_end), payDate: String(run.pay_date), currency: String(run.currency), inputDigest: String(run.input_digest), journalId, issuedAt: now }, grossMinor: Number(item.gross_minor), deductionsMinor: Number(item.deductions_minor), employerCostMinor: Number(item.employer_cost_minor), netMinor: Number(item.net_minor), input, result, statutory: Boolean(result.statutory), sourceDigest }, html = render(snapshot), checksum = await ports.hash(html);
            ids.push(id);
            guards.push(repository.guard4(commandId, `payslip-item-${item.id}`, item.id, runId, actor.companyId, item.person_id, item.recipient_user_id, item.input_snapshot_json, item.result_json, item.net_minor));
            steps.push(repository.stageIssuedSlip(id, actor.companyId, runId, run.revision, snapshot.run.version, item.id, item.engagement_id, item.person_id, item.recipient_user_id, run.pay_date, run.currency, item.gross_minor, item.deductions_minor, item.net_minor, sourceDigest, JSON.stringify(snapshot), html, checksum, actor.userId, commandId, now));
        }
        return { ids, guards, steps };
    }
    async function currentAccess(actor: WorkContext, ownOnly = false) {
        const permission = !ownOnly && hasPermission(actor, "payroll.read") && !actor.branchId && !actor.departmentId ? "payroll.read" : "self.requests";
        if (!hasPermission(actor, permission))
            throw new BusinessRuleError(404, "PAYSLIP_NOT_FOUND", "The retained employee payslip is unavailable in this work role.");
        const now = ports.clock.now(), grant = await repository.activeGrant(actor.assignmentId, actor.companyId, actor.userId, actor.role, now, now, permission, actor.branchId, actor.departmentId);
        if (!grant)
            throw new BusinessRuleError(404, "PAYSLIP_NOT_FOUND", "The retained employee payslip is unavailable under your current grant.");
        return permission === "payroll.read";
    }
    async function listPayslips(actor: WorkContext, { runId, ownOnly = true }: {
        runId?: string;
        ownOnly?: boolean;
    } = {}) {
        await ports.prepare(actor.holdingId);
        const broad = await currentAccess(actor, ownOnly);
        return (await repository.ownSlips(actor.companyId, (runId ? [runId] : []), (broad ? [] : [actor.userId, actor.userId]), runId, broad)).results.map(row => ({ id: String(row.id), runId: String(row.run_id), runRevision: Number(row.run_revision), payDate: String(row.pay_date), currency: String(row.currency), grossMinor: Number(row.gross_minor), deductionsMinor: Number(row.deductions_minor), netMinor: Number(row.net_minor), sourceDigest: String(row.source_digest), checksum: String(row.export_sha256), issuedAt: Number(row.issued_at), employeeName: (JSON.parse(String(row.snapshot_json)) as PayslipSnapshot).employee.name }));
    }
    async function getPayslip(actor: WorkContext, id: string) {
        await ports.prepare(actor.holdingId);
        const broad = await currentAccess(actor), slip = await repository.slip(id, actor.companyId, (broad ? [] : [actor.userId, actor.userId]), broad);
        if (!slip)
            throw new BusinessRuleError(404, "PAYSLIP_NOT_FOUND", "The retained employee payslip was not found.");
        const payment = await repository.payment(actor.companyId, slip.run_id, slip.person_id);
        return { id: slip.id, checksum: slip.export_sha256, snapshot: JSON.parse(slip.snapshot_json) as PayslipSnapshot, payment };
    }
    async function getPayslipExport(actor: WorkContext, id: string) {
        const detail = await getPayslip(actor, id);
        await ports.prepare(actor.holdingId);
        const row = await repository.exportBytes(id, actor.companyId);
        if (!row || await ports.hash(row.export_html) !== detail.checksum)
            throw new BusinessRuleError(409, "PAYSLIP_EXPORT_CHECKSUM_INVALID", "The retained payslip bytes failed their issued checksum.");
        return { html: row.export_html, checksum: row.export_sha256 };
    }
    async function issuePostedPayslips(actor: WorkContext, runId: string, input: z.infer<typeof issuePayslipSchema>, commandId: string) {
        actor = ports.authorize(actor, "finance.approve");
        input = issuePayslipSchema.parse(input);
        if (actor.branchId || actor.departmentId)
            throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Issue retained employee slips with the Company Payroll owner.");
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, commandId, "people.payroll.payslips.issue", runId, input);
        if (previous)
            return previous;
        await currentAccess(actor);
        const posting = await repository.posting(runId, actor.companyId, input.expectedVersion);
        if (!posting)
            throw new BusinessRuleError(409, "PAYSLIP_SOURCE_CHANGED", "Reload the independently posted payroll revision.");
        const prepared = await preparePayslipIssue(actor, runId, posting.journal_id, commandId);
        return ports.commands.commit(actor, commandId, "people.payroll.payslips.issue", input, runId, posting.version, { runId, payslipIds: prepared.ids }, [repository.guard12(commandId, "payslip-source", runId, actor.companyId, input.expectedVersion), repository.guard13(commandId, "payslips-not-issued", runId), ...prepared.guards, ...prepared.steps]);
    }
    return { preparePayslipIssue, listPayslips, getPayslip, getPayslipExport, issuePostedPayslips };
}
