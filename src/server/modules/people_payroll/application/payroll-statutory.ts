import { z } from 'zod';
import { samePerson } from "@/lib/identity_access/iam";
import type { WorkContext } from '../../identity_access/public/authorization';
import { BusinessRuleError, type Clock, type IdentifierSource } from '../../shared';
import { thaiPayroll2026Pack, thaiPayrollAdapterId } from "@/lib/people_payroll/thai-payroll-pack";
import { calculateThaiPayroll, type ThaiPayrollCalculation } from "@/lib/people_payroll/thai-payroll-rules";
import { calculateThaiPayrollAdjustment, type ThaiPayrollAdjustmentInput } from "@/lib/people_payroll/thai-payroll-adjustment";
import { type PayrollStatutoryRun, type PackRow, thaiPayrollProfileSchema, thaiPayrollProfileDecisionSchema, type Profile, type PriorItem, type ThaiPayrollEvidence, canonical, companyScope, applyThaiStatutory } from '../domain/payroll-statutory';
export interface PayrollStatutoryCommands<Effect> {
    replay<Result = Record<string, unknown>>(actor: WorkContext, key: string, operation: string, id: string, input: unknown): Promise<Result | null>;
    commit<Result>(actor: WorkContext, key: string, operation: string, input: unknown, id: string, version: number, result: Result, effects: Effect[]): Promise<Result>;
}
export interface PayrollStatutoryRepository<Effect> {
    countryPack(country_pack_id: string): Promise<PackRow | null>;
    payGroup(pay_group_id: string, company_id: string): Promise<{
        country_code: string;
        currency: string;
        frequency: string;
    } | null>;
    guardEmployeeEngagement(key: string, value: "engagement", engagementId: string, companyId: string, expectedEngagementVersion: number): Effect;
    insertProfile(id: string, companyId: string, engagementId: string, taxYear: number, effectiveFrom: string, effectiveTo: string, verifiedAnnualDeductionsMinor: number, scheduledPaysInYear: number, socialSecurityEligible: boolean, value: string | null, openingThroughDate: string, openingTaxableMinor: number, openingIrregularMinor: number, openingWithheldMinor: number, priorIncomeComplete: boolean, evidenceRef: string, userId: string, now: number, now2: number): Effect;
    profile(id: string, companyId: string): Promise<Profile | null>;
    lockProfile(value: string): Effect;
    guardProfileVersion(key: string, value: "profile", id: string, companyId: string, status: "draft" | "approved", version: number): Effect;
    guardNonOverlappingApproval(key: string, value: "overlap", engagement_id: string, companyId: string, tax_year: number, effective_to: string, effective_from: string): Effect;
    decideProfile(status: "approved" | "superseded", status2: "approved" | "superseded", userId: string, now: number, id: string): Effect;
    decisionEvidence(value: string, key: string, id: string, value2: string, userId: string, assignmentId: string, version: number, value3: string, now: number): Effect;
    profiles(companyId: string, value: string | null, value2: string | null): Promise<{
        results: Record<string, unknown>[];
    }>;
    employees(companyId: string): Promise<{
        results: Record<string, unknown>[];
    }>;
    employeeProfiles(company_id: string, engagementId: string, year: number, pay_date: string | undefined, pay_date2: string | undefined): Promise<{
        results: Profile[];
    }>;
    priorItems(company_id: string, engagementId: string, opening_through_date: string, pay_date: string | undefined, id: string, value: string): Promise<{
        results: PriorItem[];
    }>;
    lockYtd(value: string): Effect;
    guardEffectivePack(key: string, value: string, id: string, recordVersion: number, sourceDigest: string, rulesJson: string): Effect;
    guardApprovedProfile(key: string, value: string, id: string, company_id: string, engagementId: string, version: number): Effect;
    guardRetainedYtdCount(key: string, value: string, company_id: string, engagementId: string, opening_through_date: string, value2: string, id: string, value3: string, length: number): Effect;
    guardRetainedYtdItem(key: string, value: string, id: string, run_id: string, run_version: number, input_digest: string, result_json: string): Effect;
    run(runId: string): Promise<{
        mode: string;
        country_pack_id: string | null;
        currency: string;
    } | null>;
    items(runId: string): Promise<{
        results: {
            gross_minor: number;
            deductions_minor: number;
            net_minor: number;
            result_json: string;
        }[];
    }>;
    retainedPack(country_pack_id: string): Promise<PackRow | null>;
}
export interface PayrollStatutoryPorts<Effect> {
    clock: Clock;
    identifiers: IdentifierSource;
    hash(value: string): Promise<string>;
    authorize(actor: WorkContext, permission: string): WorkContext;
    prepare(holdingId: string): Promise<void>;
    commands: PayrollStatutoryCommands<Effect>;
    repository: PayrollStatutoryRepository<Effect>;
}
export function createPayrollStatutoryApplication<Effect>(ports: PayrollStatutoryPorts<Effect>) {
    const repository = ports.repository;
    async function recognizedThaiPack(rules: unknown, countryCode: string, version: string) {
        if (countryCode !== "TH" || version !== thaiPayroll2026Pack.rules.version || canonical(rules) !== canonical(thaiPayroll2026Pack))
            throw new BusinessRuleError(409, "PAYROLL_ADAPTER_RULES_UNSUPPORTED", "Install the source-dated Thai rule revision supported by the calculation adapter before validating this pack.");
        return { adapterId: thaiPayrollAdapterId, sourceDigest: await ports.hash(canonical(thaiPayroll2026Pack)), rules: thaiPayroll2026Pack.rules };
    }
    async function thaiPackForRun(run: PayrollStatutoryRun) {
        if (!run.country_pack_id || !run.pay_date || run.currency !== "THB")
            throw new BusinessRuleError(409, "PAYROLL_COUNTRY_PACK_REQUIRED", "Select an active Thai rule pack and THB payroll currency.");
        const row = await repository.countryPack(run.country_pack_id);
        if (!row || row.status !== "active")
            throw new BusinessRuleError(409, "PAYROLL_COUNTRY_PACK_INACTIVE", "The selected payroll rule pack is not active.");
        const recognized = await recognizedThaiPack(JSON.parse(row.rules_json), row.country_code, row.version);
        if (row.adapter_id !== recognized.adapterId || row.source_digest !== recognized.sourceDigest || run.pay_date < recognized.rules.effectiveFrom || run.pay_date > recognized.rules.effectiveTo)
            throw new BusinessRuleError(409, "PAYROLL_COUNTRY_PACK_NOT_EFFECTIVE", "The adapter and immutable rule evidence must be effective on the pay date.");
        const group = await repository.payGroup(run.pay_group_id, run.company_id);
        if (!group || group.country_code !== "TH" || group.currency !== "THB" || group.frequency !== "monthly")
            throw new BusinessRuleError(409, "PAYROLL_FREQUENCY_ADAPTER_UNSUPPORTED", "This Thai adapter aggregates salaried, daily and hourly wages in a monthly Section 33 pay group.");
        return { ...recognized, row };
    }
    async function createThaiPayrollProfile(actor: WorkContext, input: z.infer<typeof thaiPayrollProfileSchema>, key: string) {
        actor = ports.authorize(actor, "payroll.prepare");
        companyScope(actor);
        await ports.prepare(actor.holdingId);
        const id = `thai-payroll-profile-${key}`, prior = await ports.commands.replay<{id:string;status:string;version:number}>(actor, key, "people.thai-profile.create", id, input);
        if (prior)
            return prior;
        const year = String(input.taxYear);
        if (input.effectiveFrom > input.effectiveTo || !input.effectiveFrom.startsWith(year) || !input.effectiveTo.startsWith(year) || input.openingThroughDate >= input.effectiveFrom || input.openingIrregularMinor > input.openingTaxableMinor)
            throw new BusinessRuleError(422, "PAYROLL_STATUTORY_DATES_INVALID", "Use an evidence interval within one tax year and opening YTD ending before it.");
        if (!input.socialSecurityEligible && !input.socialSecurityExemptionEvidence)
            throw new BusinessRuleError(422, "SSO_ELIGIBILITY_UNVERIFIED", "Record the verified Section 33 exemption evidence.");
        const now = ports.clock.now();
        return ports.commands.commit(actor, key, "people.thai-profile.create", input, id, 1, { id, status: "draft", version: 1 }, [
            repository.guardEmployeeEngagement(key, "engagement", input.engagementId, actor.companyId, input.expectedEngagementVersion),
            repository.insertProfile(id, actor.companyId, input.engagementId, input.taxYear, input.effectiveFrom, input.effectiveTo, input.verifiedAnnualDeductionsMinor, input.scheduledPaysInYear, input.socialSecurityEligible, input.socialSecurityExemptionEvidence ?? null, input.openingThroughDate, input.openingTaxableMinor, input.openingIrregularMinor, input.openingWithheldMinor, input.priorIncomeComplete, input.evidenceRef, actor.userId, now, now),
        ]);
    }
    async function decideThaiPayrollProfile(actor: WorkContext, id: string, input: z.infer<typeof thaiPayrollProfileDecisionSchema>, key: string) {
        actor = ports.authorize(actor, "people.compensation.approve");
        companyScope(actor);
        await ports.prepare(actor.holdingId);
        const prior = await ports.commands.replay(actor, key, `people.thai-profile.${input.action}`, id, input);
        if (prior)
            return prior;
        const row = await repository.profile(id, actor.companyId);
        if (!row)
            throw new BusinessRuleError(404, "PAYROLL_PROFILE_NOT_FOUND", "Statutory payroll evidence not found.");
        if (row.version !== input.expectedVersion || row.status !== (input.action === "approve" ? "draft" : "approved"))
            throw new BusinessRuleError(409, "PAYROLL_PROFILE_CHANGED", "The payroll evidence changed. Reload before deciding.");
        if (samePerson(actor, row.created_by))
            throw new BusinessRuleError(409, "SELF_REVIEW_FORBIDDEN", "A different person must review statutory payroll evidence.");
        const status = input.action === "approve" ? "approved" : "superseded", now = ports.clock.now();
        return ports.commands.commit(actor, key, `people.thai-profile.${input.action}`, input, id, row.version + 1, { id, status, version: row.version + 1 }, [
            repository.lockProfile(`thai-payroll-profile:${actor.companyId}:${row.engagement_id}:${row.tax_year}`),
            repository.guardProfileVersion(key, "profile", id, actor.companyId, row.status, row.version),
            ...(input.action === "approve" ? [repository.guardNonOverlappingApproval(key, "overlap", row.engagement_id, actor.companyId, row.tax_year, row.effective_to, row.effective_from)] : []),
            repository.decideProfile(status, status, actor.userId, now, id),
            repository.decisionEvidence(ports.identifiers.next(), key, id, `people.thai-profile.${input.action}`, actor.userId, actor.assignmentId, row.version, await ports.hash(canonical({ row, reason: input.reason })), now),
        ]);
    }
    async function thaiPayrollConfiguration(actor: WorkContext, engagementId?: string) {
        actor = ports.authorize(actor, "payroll.configuration.read");
        companyScope(actor);
        await ports.prepare(actor.holdingId);
        const profiles = (await repository.profiles(actor.companyId, engagementId ?? null, engagementId ?? null)).results;
        const employees = (await repository.employees(actor.companyId)).results;
        return { supportedPack: thaiPayroll2026Pack, profiles, employees };
    }
    async function thaiEvidenceForEmployee(run: PayrollStatutoryRun, engagementId: string): Promise<ThaiPayrollEvidence> {
        const pack = await thaiPackForRun(run), year = Number(run.pay_date!.slice(0, 4));
        const profiles = (await repository.employeeProfiles(run.company_id, engagementId, year, run.pay_date, run.pay_date)).results;
        if (profiles.length !== 1 || !profiles[0].prior_income_complete)
            throw new BusinessRuleError(422, "PAYROLL_STATUTORY_EVIDENCE_INCOMPLETE", "Verify annual deductions, eligibility, payment count and opening YTD before statutory calculation.");
        const profile = profiles[0];
        const priorItems = (await repository.priorItems(run.company_id, engagementId, profile.opening_through_date, run.pay_date, run.id, `${year}-01-01`)).results;
        if (priorItems.some(item => !(JSON.parse(item.result_json) as {
            statutory?: {
                adapterId?: string;
            };
        }).statutory?.adapterId))
            throw new BusinessRuleError(422, "PAYROLL_YTD_UNVERIFIED", "Import reviewed opening YTD through the last gross-pay preview before calculating statutory payroll.");
        return { pack: { id: pack.row.id, recordVersion: pack.row.record_version, sourceDigest: pack.sourceDigest, rulesJson: pack.row.rules_json }, profile, priorItems };
    }
    function thaiEvidenceGuards(key: string, evidence: ThaiPayrollEvidence, run: PayrollStatutoryRun, engagementId: string): Effect[] {
        return [
            repository.lockYtd(`thai-payroll-ytd:${run.company_id}:${engagementId}:${run.pay_date!.slice(0, 4)}`),
            repository.guardEffectivePack(key, `thai-pack-${engagementId}`, evidence.pack.id, evidence.pack.recordVersion, evidence.pack.sourceDigest, evidence.pack.rulesJson),
            repository.guardApprovedProfile(key, `thai-profile-${engagementId}`, evidence.profile.id, run.company_id, engagementId, evidence.profile.version),
            repository.guardRetainedYtdCount(key, `thai-ytd-count-${engagementId}`, run.company_id, engagementId, evidence.profile.opening_through_date, run.pay_date!, run.id, `${run.pay_date!.slice(0, 4)}-01-01`, evidence.priorItems.length),
            ...evidence.priorItems.map(item => repository.guardRetainedYtdItem(key, `thai-ytd-${item.id}`, item.id, item.run_id, item.run_version, item.input_digest, item.result_json)),
        ];
    }
    async function assertStatutoryRunReady(runId: string) {
        const run = await repository.run(runId);
        const items = (await repository.items(runId)).results;
        if (!run?.country_pack_id || run.currency !== "THB" || !items.length)
            throw new BusinessRuleError(409, "PAYROLL_STATUTORY_ADAPTER_REQUIRED", "A calculated, source-dated Thai statutory run is required for this payroll action.");
        const pack = await repository.retainedPack(run.country_pack_id);
        if (!pack)
            throw new BusinessRuleError(409, "PAYROLL_STATUTORY_ADAPTER_REQUIRED", "Reviewed statutory rule evidence is unavailable.");
        const recognized = await recognizedThaiPack(JSON.parse(pack.rules_json), pack.country_code, pack.version);
        for (const item of items) {
            const statutory = (JSON.parse(item.result_json) as {
                statutory?: {
                    adapterId: string;
                    calculationKind?: string;
                    sourceDigest: string;
                    input: ThaiPayrollCalculation | ThaiPayrollAdjustmentInput;
                    calculation: ReturnType<typeof calculateThaiPayroll>;
                };
            }).statutory;
            const calculation = statutory?.calculationKind === 'adjustment' ? calculateThaiPayrollAdjustment(statutory.input as ThaiPayrollAdjustmentInput, recognized.rules) : statutory ? calculateThaiPayroll(statutory.input, recognized.rules) : null;
            if (!statutory || statutory.adapterId !== thaiPayrollAdapterId || statutory.sourceDigest !== recognized.sourceDigest || canonical(calculation) !== canonical(statutory.calculation) || item.gross_minor !== statutory.calculation.grossMinor || item.net_minor !== statutory.calculation.netMinor || item.gross_minor - item.deductions_minor !== item.net_minor)
                throw new BusinessRuleError(409, "PAYROLL_STATUTORY_RESULT_INVALID", "Payroll amounts must reconcile with the retained statutory adapter calculation.");
        }
    }
    return { recognizedThaiPack, thaiPackForRun, createThaiPayrollProfile, decideThaiPayrollProfile, thaiPayrollConfiguration, thaiEvidenceForEmployee, thaiEvidenceGuards, assertStatutoryRunReady, applyThaiStatutory };
}
