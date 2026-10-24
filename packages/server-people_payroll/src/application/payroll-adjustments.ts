import { z } from 'zod';
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { calculateThaiPayrollAdjustment, type ThaiPayrollAdjustmentInput } from "@erpx/feature-people_payroll/lib/thai-payroll-adjustment";
import { thaiPayroll2026Pack, thaiPayrollAdapterId } from "@erpx/feature-people_payroll/lib/thai-payroll-pack";
import { ThaiPayrollRuleError, type ThaiPayrollCalculation } from "@erpx/feature-people_payroll/lib/thai-payroll-rules";
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type {snapshotMoney} from '@erpx/server-finance/public/company-money';
import type { thaiEvidenceForEmployee, thaiPackForRun, PayrollStatutoryRun, ThaiPayrollEvidence } from '@erpx/server-people_payroll/public/payroll-statutory';
import { BusinessRuleError, type Clock } from '@erpx/server-shared/index';
import type { PayrollStatutoryCommands } from '@erpx/server-people_payroll/application/payroll-statutory';
import { type Row, payrollAdjustmentCreateSchema, payrollAdjustmentEditSchema, type Values, type Origin, equal } from '@erpx/server-people_payroll/domain/payroll-adjustments';
export interface PayrollAdjustmentRepository<Effect> {
    readOriginalSourceRun(runId: string, companyId: string): Promise<{
        source: string;
    } | null>;
    readOriginalSourceItem(itemId: string, runId: string): Promise<{
        source: string;
    } | null>;
    readOriginalSourceSlips(runId: string, itemId: string): Promise<{
        results: (Row)[];
    }>;
    readTimeSourcesEntries(engagement_id: unknown, company_id: unknown, ids: string[]): Promise<{
        results: (Row)[];
    }>;
    stageRevisionStatementsEffect(id: string, runId: string, revision: number, taxableArrearsMinor: number, nonTaxableArrearsMinor: number, socialSecurityWageMinor: number, value: string, reason: string, evidenceRef: string, userId: string, key: string, now: number): Effect;
    stageRevisionStatementsEffect2(runId: string, revision: number, timeId: string): Effect;
    stageCreatePayrollAdjustmentEffect(value: string): Effect;
    guard8(key: string, value: "adjustment-original", originalRunId: string, companyId: string, expectedVersion: number): Effect;
    guard9(key: string, value: string, value2: string, companyId: string, value3: string, value4: number): Effect;
    stageCreatePayrollAdjustmentEffect2(id: string, companyId: string, pay_group_id: string, countryPackId: string, engagement_id: unknown, period_start: string, period_end: string, payDate: string, originalRunId: string, userId: string, now: number, now2: number): Effect;
    stageCreatePayrollAdjustmentEffect3(id: string, companyId: string, originalRunId: string, originalItemId: string, engagement_id: unknown, sourceRef: string, source: string, value: string, userId: string, now: number): Effect;
    readEditPayrollAdjustmentRow(runId: string, companyId: string): Promise<Row | null>;
    stageEditPayrollAdjustmentEffect(runId: string): Effect;
    guard14(key: string, value: "adjustment-edit-version", runId: string, companyId: string, expectedVersion: number): Effect;
    stageEditPayrollAdjustmentEffect2(runId: string): Effect;
    stageEditPayrollAdjustmentEffect3(runId: string): Effect;
    stageEditPayrollAdjustmentEffect4(now: number, runId: string): Effect;
    readPayrollAdjustmentInputsRow(id: string, company_id: string): Promise<Row | null>;
    readPayrollAdjustmentInputsEngagement(engagement_id: unknown, company_id: string): Promise<Row | null>;
    readPayrollAdjustmentInputsPayee(value: unknown, company_id: string): Promise<Row | null>;
    readPayrollAdjustmentInputsPriorAdjustments(original_item_id: unknown, id: string): Promise<{
        results: (Row)[];
    }>;
    stageCalculatePayrollAdjustmentRunEffect(id: string): Effect;
    guard23(key: string, value: "adjustment-run-version", id: string, companyId: string, version: number): Effect;
    stageCalculatePayrollAdjustmentRunEffect2(id: string): Effect;
    stageCalculatePayrollAdjustmentRunEffect3(value: string, digest: string, now: number, id: string): Effect;
    stageCalculatePayrollAdjustmentRunEffect4(value: string, id: string, engagementId: string, value2: string, value3: string, gross: number, deductions: number, employer: number, net: number, now: number): Effect;
    readPayrollAdjustmentSourceFreshRow(id: string): Promise<{
        input_snapshot_json: string;
    } | null>;
    readPayrollAdjustmentPanelRun(runId: string, companyId: string): Promise<Row | null>;
    readPayrollAdjustmentPanelAdjustments(runId: string, companyId: string): Promise<{
        results: (Row)[];
    }>;
    readPayrollAdjustmentPanelSource(runId: string, companyId: string): Promise<Row | null>;
    readPayrollAdjustmentPanelRevisions(runId: string): Promise<{
        results: (Row)[];
    }>;
    readPayrollAdjustmentPanelTimeOptions(runId: string, companyId: string, period_start: unknown, period_end: unknown): Promise<{
        results: (Row)[];
    }>;
    stagePayrollAdjustmentGuardsEffect(id: string): Effect;
    guard34(key: string, value: "adjustment-revision", id: string, company_id: string, sourceRevision: number, value2: string): Effect;
    guard35(key: string, value: "adjustment-current-payee", value2: string, company_id: string, value3: number): Effect;
    guard36(key: string, value: "adjustment-current-engagement", value2: string, company_id: string, value3: number): Effect;
    guard37(key: string, value: "adjustment-prior-count", value2: string, id: string, length: number): Effect;
    stagePayrollAdjustmentGuardsEffect2(id: unknown): Effect;
    guard39(key: string, value: string, value2: string, value3: number, id: string, sourceRevision: number): Effect;
}
export interface PayrollAdjustmentPorts<Effect> {
    repository: PayrollAdjustmentRepository<Effect>;
    clock: Clock;
    hash(value: string): Promise<string>;
    authorize(actor: WorkContext, permission: string): WorkContext;
    prepare(holdingId: string): Promise<void>;
    commands: PayrollStatutoryCommands<Effect>;
    money(actor: WorkContext, input: Parameters<typeof snapshotMoney>[2]): Promise<{
        statement: Effect;
    }>;
    pack(run: PayrollStatutoryRun): ReturnType<typeof thaiPackForRun>;
    evidence(run: PayrollStatutoryRun, engagementId: string): ReturnType<typeof thaiEvidenceForEmployee>;
    evidenceGuards(key: string, evidence: ThaiPayrollEvidence, run: PayrollStatutoryRun, engagementId: string): Effect[];
    moduleEnabled(companyId: string, module: string): Promise<void>;
    featureGuards(key: string, companyId: string, module: string): Effect[];
}
export function createPayrollAdjustmentApplication<Effect>(ports: PayrollAdjustmentPorts<Effect>) {
    const repository = ports.repository;
    function companyScope(actor: WorkContext) { if (actor.branchId || actor.departmentId)
        throw new BusinessRuleError(403, "COMPANY_PAYROLL_SCOPE_REQUIRED", "Use a Company Payroll assignment for a posted-payroll adjustment."); }
    async function originalSource(companyId: string, runId: string, itemId: string): Promise<Origin> {
        const run = await repository.readOriginalSourceRun(runId, companyId);
        const item = await repository.readOriginalSourceItem(itemId, runId);
        if (!run || !item)
            throw new BusinessRuleError(404, "PAYROLL_ADJUSTMENT_SOURCE_NOT_FOUND", "Select a posted statutory employee item in this Company.");
        const slips = (await repository.readOriginalSourceSlips(runId, itemId)).results;
        if (slips.length !== 1)
            throw new BusinessRuleError(409, "PAYROLL_ADJUSTMENT_SLIP_REQUIRED", "The original posted employee payslip must be retained before correction.");
        return { run: JSON.parse(run.source), item: JSON.parse(item.source), slips };
    }
    async function timeSources(origin: Origin, values: Values) {
        const ids = [...new Set(values.timeEntryIds)].sort();
        if (ids.length !== values.timeEntryIds.length)
            throw new BusinessRuleError(422, "PAYROLL_ADJUSTMENT_TIME_DUPLICATE", "Select each late time source once.");
        const original = JSON.parse(String(origin.item.input_snapshot_json)) as {
            hours: {
                entries: Array<{
                    id: string;
                }>;
            };
            costAssignments: Array<{
                id: string;
                effective_from: string;
                effective_to: string | null;
                branch_id: string | null;
                department_id: string | null;
                evidence_status: string;
            }>;
        };
        const entries = (await repository.readTimeSourcesEntries(origin.item.engagement_id, origin.run.company_id, ids)).results;
        if (entries.length !== ids.length || entries.some(entry => String(entry.work_date) < String(origin.run.period_start) || String(entry.work_date) > String(origin.run.period_end) || Number(entry.regular_hours) !== 0 || Number(entry.overtime_hours) <= 0 || original.hours.entries.some(old => old.id === entry.id)))
            throw new BusinessRuleError(422, "PAYROLL_ADJUSTMENT_TIME_INVALID", "Use independently approved late overtime, within the original work period, with zero regular hours and absent from its posted inputs.");
        for (const entry of entries) {
            const costs = original.costAssignments.filter(cost => cost.effective_from <= String(entry.work_date) && (cost.effective_to === null || cost.effective_to >= String(entry.work_date)));
            if (costs.length !== 1 || costs[0].evidence_status !== "verified" || entry.cost_assignment_id !== costs[0].id || entry.cost_branch_id !== costs[0].branch_id || entry.cost_department_id !== costs[0].department_id)
                throw new BusinessRuleError(422, "PAYROLL_TIME_COST_EVIDENCE_MISSING", "Late time must retain the verified original work placement.");
        }
        if (values.taxableArrearsMinor + values.nonTaxableArrearsMinor === 0 && !entries.length)
            throw new BusinessRuleError(422, "PAYROLL_ADJUSTMENT_EMPTY", "Record an evidenced arrear or an approved late overtime source.");
        return entries;
    }
    async function revisionStatements(actor: WorkContext, runId: string, revision: number, payDate: string, values: Values, key: string) {
        const id = `payroll-adjustment-revision-${key}`, now = ports.clock.now(), money = await Promise.all([
            ports.money(actor, { sourceType: "payroll_adjustment_revision", sourceId: id, amountKind: "taxable_arrears", transactionDate: payDate, currency: "THB", currencyScale: 2, amountUnits: String(values.taxableArrearsMinor) }),
            ports.money(actor, { sourceType: "payroll_adjustment_revision", sourceId: id, amountKind: "non_taxable_arrears", transactionDate: payDate, currency: "THB", currencyScale: 2, amountUnits: String(values.nonTaxableArrearsMinor) }),
        ]);
        return [repository.stageRevisionStatementsEffect(id, runId, revision, values.taxableArrearsMinor, values.nonTaxableArrearsMinor, values.socialSecurityWageMinor, JSON.stringify([...values.timeEntryIds].sort()), values.reason, values.evidenceRef, actor.userId, key, now), ...values.timeEntryIds.map(timeId => repository.stageRevisionStatementsEffect2(runId, revision, timeId)), ...money.map(item => item.statement)];
    }
    async function createPayrollAdjustment(actor: WorkContext, originalRunId: string, input: z.infer<typeof payrollAdjustmentCreateSchema>, key: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        companyScope(actor);
        await ports.prepare(actor.holdingId);
        const id = `payrun-adjustment-${key}`, previous = await ports.commands.replay(actor, key, "people.payroll.adjustment.create", id, { originalRunId, ...input });
        if (previous)
            return previous;
        await ports.moduleEnabled(actor.companyId, 'people_payroll');
        const origin = await originalSource(actor.companyId, originalRunId, input.originalItemId);
        if (Number(origin.run.version) !== input.expectedVersion)
            throw new BusinessRuleError(409, "PAYROLL_ADJUSTMENT_SOURCE_CHANGED", "Reload the original posted payroll before preparing its correction.");
        if (input.payDate <= String(origin.run.pay_date) || input.payDate.slice(0, 4) !== String(origin.run.pay_date).slice(0, 4))
            throw new BusinessRuleError(422, "PAYROLL_ADJUSTMENT_PAY_DATE", "Use a later payment in the same verified tax year; cross-year income requires a new reviewed source adapter.");
        const run = { id, company_id: actor.companyId, pay_group_id: String(origin.run.pay_group_id), country_pack_id: input.countryPackId, period_start: String(origin.run.period_start), period_end: String(origin.run.period_end), pay_date: input.payDate, currency: "THB", mode: "production", adjustment_of_run_id: originalRunId };
        const pack = await ports.pack(run), originResult = JSON.parse(String(origin.item.result_json)) as {
            statutory: {
                sourceDigest: string;
                input: ThaiPayrollCalculation;
            };
        };
        if (originResult.statutory.sourceDigest !== pack.sourceDigest || originResult.statutory.input.regularTaxableMinor <= 0)
            throw new BusinessRuleError(409, "PAYROLL_ADJUSTMENT_BASIS_REQUIRED", "The current reviewed pack must match the original statutory salary basis.");
        const entries = await timeSources(origin, input.values), now = ports.clock.now(), source = JSON.stringify(origin);
        return ports.commands.commit(actor, key, "people.payroll.adjustment.create", { originalRunId, ...input }, id, 1, { id, status: "calculating", version: 1, originalRunId }, [
            ...ports.featureGuards(key, actor.companyId, 'people_payroll'),
            repository.stageCreatePayrollAdjustmentEffect(`payroll-adjustment:${actor.companyId}:${origin.item.engagement_id}`),
            repository.guard8(key, "adjustment-original", originalRunId, actor.companyId, input.expectedVersion),
            ...entries.map(entry => repository.guard9(key, `adjustment-time-${entry.id}`, String(entry.id), actor.companyId, String(origin.item.engagement_id), Number(entry.version))),
            repository.stageCreatePayrollAdjustmentEffect2(id, actor.companyId, run.pay_group_id, input.countryPackId, origin.item.engagement_id, run.period_start, run.period_end, input.payDate, originalRunId, actor.userId, now, now),
            repository.stageCreatePayrollAdjustmentEffect3(id, actor.companyId, originalRunId, input.originalItemId, origin.item.engagement_id, input.sourceRef, source, await ports.hash(source), actor.userId, now),
            ...await revisionStatements(actor, id, 1, input.payDate, input.values, key),
        ]);
    }
    async function editPayrollAdjustment(actor: WorkContext, runId: string, input: z.infer<typeof payrollAdjustmentEditSchema>, key: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        companyScope(actor);
        await ports.prepare(actor.holdingId);
        const previous = await ports.commands.replay(actor, key, "people.payroll.adjustment.edit", runId, input);
        if (previous)
            return previous;
        const row = await repository.readEditPayrollAdjustmentRow(runId, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "PAYROLL_ADJUSTMENT_NOT_FOUND", "Payroll adjustment not found.");
        await ports.moduleEnabled(actor.companyId, 'people_payroll');
        if (Number(row.version) !== input.expectedVersion || !['draft', 'calculating', 'failed', 'preview'].includes(String(row.status)) || row.submitted_by)
            throw new BusinessRuleError(409, "PAYROLL_ADJUSTMENT_LOCKED", "Only the current unsubmitted correction can be edited.");
        if (!samePerson(actor, String(row.preparer)))
            throw new BusinessRuleError(403, "PAYROLL_ADJUSTMENT_OWNER_REQUIRED", "The correction preparer must edit its source.");
        await timeSources(JSON.parse(String(row.source_snapshot_json)), input.values);
        const revision = Number(row.source_revision) + 1, now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.payroll.adjustment.edit", input, runId, input.expectedVersion + 1, { id: runId, status: "calculating", version: input.expectedVersion + 1, sourceRevision: revision }, [
            ...ports.featureGuards(key, actor.companyId, 'people_payroll'),
            repository.stageEditPayrollAdjustmentEffect(runId),
            repository.guard14(key, "adjustment-edit-version", runId, actor.companyId, input.expectedVersion),
            repository.stageEditPayrollAdjustmentEffect2(runId),
            repository.stageEditPayrollAdjustmentEffect3(runId),
            ...await revisionStatements(actor, runId, revision, String(row.pay_date), input.values, key),
            repository.stageEditPayrollAdjustmentEffect4(now, runId),
        ]);
    }
    async function payrollAdjustmentInputs(run: PayrollStatutoryRun) {
        const row = await repository.readPayrollAdjustmentInputsRow(run.id, run.company_id);
        if (!row)
            throw new BusinessRuleError(409, "PAYROLL_ADJUSTMENT_SOURCE_CHANGED", "The active correction source is missing.");
        const origin = await originalSource(run.company_id, String(row.original_run_id), String(row.original_item_id));
        if (!equal(origin, JSON.parse(String(row.source_snapshot_json))))
            throw new BusinessRuleError(409, "PAYROLL_ADJUSTMENT_SOURCE_CHANGED", "The original posted source does not match its retained correction evidence.");
        const values = { taxableArrearsMinor: Number(row.taxable_arrears_minor), nonTaxableArrearsMinor: Number(row.non_taxable_arrears_minor), socialSecurityWageMinor: Number(row.social_security_wage_minor), timeEntryIds: JSON.parse(String(row.time_entries_json)) as string[], reason: String(row.reason), evidenceRef: String(row.evidence_ref) }, entries = await timeSources(origin, values);
        const original = JSON.parse(String(origin.item.input_snapshot_json)) as Row, engagement = await repository.readPayrollAdjustmentInputsEngagement(row.engagement_id, run.company_id);
        const payee = await repository.readPayrollAdjustmentInputsPayee(engagement?.person_id ?? "", run.company_id);
        if (!engagement || !payee)
            throw new BusinessRuleError(422, "PAYROLL_PAYEE_INVALID", "Verify the original employee identity before correcting payroll.");
        const priorAdjustments = (await repository.readPayrollAdjustmentInputsPriorAdjustments(row.original_item_id, run.id)).results;
        return { ...original, compensation: original.compensation, policy: original.policy, costAssignments: original.costAssignments, engagement, payee, hours: { regular: 0, overtime: entries.reduce((sum, entry) => sum + Number(entry.overtime_hours), 0), entries, handoffs: [] }, benefitInputs: [], advanceRepayments: [], period: { start: run.period_start, end: run.period_end }, thaiStatutory: await ports.evidence(run, String(row.engagement_id)), adjustment: { runId: run.id, originalRunId: row.original_run_id, originalItemId: row.original_item_id, sourceRef: row.source_ref, sourceDigest: row.source_digest, sourceRevision: Number(row.source_revision), revisionId: row.revision_id, values, originalStatutory: JSON.parse(String(origin.item.result_json)).statutory, priorAdjustments } };
    }
    async function calculatePayrollAdjustmentItem(run: PayrollStatutoryRun) {
        const inputSnapshot = await payrollAdjustmentInputs(run), source = inputSnapshot.adjustment, original = source.originalStatutory as {
            input: ThaiPayrollCalculation;
            calculation: {
                employeeSsoMinor: number;
                employerSsoMinor: number;
            };
        }, compensation = inputSnapshot.compensation as Row, policy = inputSnapshot.policy as {
            overtimeMultiplierBps: number;
        };
        const overtime = Math.round(Number(compensation.hourly_rate_minor ?? 0) * inputSnapshot.hours.overtime * Number(policy.overtimeMultiplierBps) / 10000);
        if (inputSnapshot.hours.overtime > 0 && Number(compensation.hourly_rate_minor ?? 0) <= 0)
            throw new BusinessRuleError(422, "OVERTIME_RATE_MISSING", "The original approved overtime rate is required.");
        const evidence = inputSnapshot.thaiStatutory, profile = evidence.profile, prior = evidence.priorItems.map(item => JSON.parse(item.result_json).statutory), previous = source.priorAdjustments.map(item => JSON.parse(String(item.result_json)).statutory);
        const input: ThaiPayrollAdjustmentInput = { payDate: run.pay_date!, regularTaxableMinor: 0, irregularTaxableMinor: source.values.taxableArrearsMinor + overtime, nonTaxableMinor: source.values.nonTaxableArrearsMinor, ordinaryTaxableBasisMinor: original.input.regularTaxableMinor,
            irregularPaidBeforeMinor: profile.opening_irregular_minor + prior.reduce((sum, item) => sum + item.input.irregularTaxableMinor, 0), socialSecurityWageMinor: source.values.socialSecurityWageMinor, socialSecurityEligible: profile.social_security_eligible, socialSecurityExemptionEvidence: profile.social_security_exemption_evidence ?? undefined,
            originalSocialSecurityWageMinor: original.input.socialSecurityWageMinor, originalEmployeeSsoMinor: original.calculation.employeeSsoMinor, originalEmployerSsoMinor: original.calculation.employerSsoMinor,
            priorAdjustmentSocialSecurityWageMinor: previous.reduce((sum, item) => sum + item.input.socialSecurityWageMinor, 0), priorAdjustmentEmployeeSsoMinor: previous.reduce((sum, item) => sum + item.calculation.employeeSsoMinor, 0), priorAdjustmentEmployerSsoMinor: previous.reduce((sum, item) => sum + item.calculation.employerSsoMinor, 0),
            otherEmployeeDeductionsMinor: 0, verifiedAnnualDeductionsMinor: profile.annual_deductions_minor, taxablePaidBeforeMinor: profile.opening_taxable_minor + prior.reduce((sum, item) => sum + item.input.regularTaxableMinor + item.input.irregularTaxableMinor, 0), taxWithheldBeforeMinor: profile.opening_withheld_minor + prior.reduce((sum, item) => sum + item.calculation.taxWithheldMinor, 0), scheduledPaysInYear: profile.scheduled_pays, lastPayInYear: run.pay_date!.slice(5, 7) === '12', priorIncomeComplete: profile.prior_income_complete };
        let calculation: ReturnType<typeof calculateThaiPayrollAdjustment>;
        try {
            calculation = calculateThaiPayrollAdjustment(input, thaiPayroll2026Pack.rules);
        }
        catch (error) {
            if (error instanceof ThaiPayrollRuleError)
                throw new BusinessRuleError(422, error.code, error.message);
            throw error;
        }
        const components = [{ code: "ARREARS", label: "Reviewed arrears", kind: "earning", amountMinor: source.values.taxableArrearsMinor + source.values.nonTaxableArrearsMinor }, { code: "STATUTORY:TAX", label: "Thai correction withholding tax", kind: "deduction", amountMinor: calculation.taxWithheldMinor }, { code: "STATUTORY:SSO", label: "Section 33 employee contribution difference", kind: "deduction", amountMinor: calculation.employeeSsoMinor }, { code: "STATUTORY:SSO:EMPLOYER", label: "Section 33 employer contribution difference", kind: "employer", amountMinor: calculation.employerSsoMinor }].map(item => ({ ...item, amount: minorToDecimal(item.amountMinor) }));
        const result = { grossMinor: calculation.grossMinor, deductionsMinor: calculation.grossMinor - calculation.netMinor, employerCostMinor: calculation.employerSsoMinor, netMinor: calculation.netMinor, statutory: { adapterId: thaiPayrollAdapterId, calculationKind: "adjustment", sourceDigest: evidence.pack.sourceDigest, certified: false, input, calculation }, explanation: { baseMinor: 0, hourlyRateMinor: Number(compensation.hourly_rate_minor ?? 0), regularHours: 0, overtimeHours: inputSnapshot.hours.overtime, overtimeMinor: overtime, overtimeMultiplierBps: Number(policy.overtimeMultiplierBps), components, adjustmentOfRunId: source.originalRunId, originalItemId: source.originalItemId, sourceRef: source.sourceRef } };
        return { engagementId: String(inputSnapshot.engagement.id), personId: String(inputSnapshot.engagement.person_id), workerType: "employee", inputSnapshot, result, gross: calculation.grossMinor, deductions: result.deductionsMinor, employer: calculation.employerSsoMinor, net: calculation.netMinor };
    }
    async function calculatePayrollAdjustmentRun(actor: WorkContext, run: PayrollStatutoryRun & {
        version: number;
    }, key: string) {
        const item = await calculatePayrollAdjustmentItem(run), totals = { gross: item.gross, deductions: item.deductions, employer: item.employer, net: item.net, headcount: 1 }, digest = await ports.hash(JSON.stringify([item.inputSnapshot])), now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.payroll.calculate", {}, run.id, run.version + 1, { id: run.id, status: "preview", version: run.version + 1, totals }, [
            repository.stageCalculatePayrollAdjustmentRunEffect(run.id),
            repository.guard23(key, "adjustment-run-version", run.id, actor.companyId, run.version),
            ...await payrollAdjustmentGuards(key, run, item.inputSnapshot),
            repository.stageCalculatePayrollAdjustmentRunEffect2(run.id),
            repository.stageCalculatePayrollAdjustmentRunEffect3(JSON.stringify(totals), digest, now, run.id),
            repository.stageCalculatePayrollAdjustmentRunEffect4(`payitem-${key}-adjustment`, run.id, item.engagementId, JSON.stringify(item.inputSnapshot), JSON.stringify(item.result), item.gross, item.deductions, item.employer, item.net, now),
        ]);
    }
    async function payrollAdjustmentSourceFresh(run: PayrollStatutoryRun) {
        const row = await repository.readPayrollAdjustmentSourceFreshRow(run.id);
        if (!row)
            throw new BusinessRuleError(409, "PREVIEW_REQUIRED", "Calculate the correction before reviewing it.");
        if (!equal(JSON.parse(row.input_snapshot_json), await payrollAdjustmentInputs(run)))
            throw new BusinessRuleError(409, "PAYROLL_ADJUSTMENT_SOURCE_CHANGED", "Correction inputs or actual prior payroll changed; recalculate before review.");
    }
    async function payrollAdjustmentPanel(actor: WorkContext, runId: string) {
        ports.authorize(actor, 'payroll.read');
        companyScope(actor);
        await ports.prepare(actor.holdingId);
        const run = await repository.readPayrollAdjustmentPanelRun(runId, actor.companyId);
        if (!run)
            throw new BusinessRuleError(404, 'PAYROLL_RUN_NOT_FOUND', 'Payroll run not found.');
        const adjustments = (await repository.readPayrollAdjustmentPanelAdjustments(runId, actor.companyId)).results;
        const source = await repository.readPayrollAdjustmentPanelSource(runId, actor.companyId);
        const revisions = source ? (await repository.readPayrollAdjustmentPanelRevisions(runId)).results : [];
        const timeOptions = run.status === 'posted' && run.mode === 'production' && !run.adjustment_of_run_id ? (await repository.readPayrollAdjustmentPanelTimeOptions(runId, actor.companyId, run.period_start, run.period_end)).results : [];
        return { adjustments, source, revisions, timeOptions };
    }
    async function payrollAdjustmentGuards(key: string, run: PayrollStatutoryRun, input: Awaited<ReturnType<typeof payrollAdjustmentInputs>>): Promise<Effect[]> {
        const source = input.adjustment, guards = ports.evidenceGuards(key, input.thaiStatutory, run, String(input.engagement.id));
        guards.push(repository.stagePayrollAdjustmentGuardsEffect(run.id), repository.guard34(key, "adjustment-revision", run.id, run.company_id, source.sourceRevision, String(source.sourceDigest)), repository.guard35(key, "adjustment-current-payee", String(input.payee.id), run.company_id, Number(input.payee.version)), repository.guard36(key, "adjustment-current-engagement", String(input.engagement.id), run.company_id, Number(input.engagement.version)), repository.guard37(key, "adjustment-prior-count", String(source.originalItemId), run.id, source.priorAdjustments.length));
        for (const entry of input.hours.entries)
            guards.push(repository.stagePayrollAdjustmentGuardsEffect2(entry.id), repository.guard39(key, `adjustment-current-time-${entry.id}`, String(entry.id), Number(entry.version), run.id, source.sourceRevision));
        return guards;
    }
    return { originalSource, createPayrollAdjustment, editPayrollAdjustment, payrollAdjustmentInputs, calculatePayrollAdjustmentItem, calculatePayrollAdjustmentRun, payrollAdjustmentSourceFresh, payrollAdjustmentPanel, payrollAdjustmentGuards };
}
