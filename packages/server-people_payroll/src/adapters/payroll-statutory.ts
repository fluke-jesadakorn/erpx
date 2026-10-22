import { withPayrollPurchasedCommand, withPayrollPurchasedRead } from "@erpx/server-people_payroll/adapters/payroll-purchases";
import {payrollStatutoryDecisionEffects} from '@erpx/server-workflow/public/payroll-statutory-decision-effects';
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { BusinessRuleError } from '@erpx/server-shared/index';
import { type PayrollStatutoryRun, type PackRow, thaiPayrollProfileSchema, thaiPayrollProfileDecisionSchema, type Profile, type PriorItem, type ThaiPayrollEvidence, type Calculated } from '@erpx/server-people_payroll/domain/payroll-statutory';
import { createPayrollStatutoryApplication, type PayrollStatutoryRepository } from '@erpx/server-people_payroll/application/payroll-statutory';
export { thaiPayrollProfileSchema, thaiPayrollProfileDecisionSchema } from '@erpx/server-people_payroll/domain/payroll-statutory';
export type { PayrollStatutoryRun, ThaiPayrollEvidence } from '@erpx/server-people_payroll/domain/payroll-statutory';
function application(provided?: SqlDatabase) {
    let database = provided!;
    const repository: PayrollStatutoryRepository<SqlStatement> = { countryPack(country_pack_id: string) { return database.prepare("SELECT id,country_code,version,status,rules_json,adapter_id,source_digest,record_version FROM hr_country_packs WHERE id=?").bind(country_pack_id).first<PackRow>(); },
        payGroup(pay_group_id: string, company_id: string) { return database.prepare("SELECT country_code,currency,frequency FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'").bind(pay_group_id, company_id).first<{
            country_code: string;
            currency: string;
            frequency: string;
        }>(); },
        guardEmployeeEngagement(key: string, value: "engagement", engagementId: string, companyId: string, expectedEngagementVersion: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND currency='THB' AND worker_type='employee')", [engagementId, companyId, expectedEngagementVersion]); },
        insertProfile(id: string, companyId: string, engagementId: string, taxYear: number, effectiveFrom: string, effectiveTo: string, verifiedAnnualDeductionsMinor: number, scheduledPaysInYear: number, socialSecurityEligible: boolean, value: string | null, openingThroughDate: string, openingTaxableMinor: number, openingIrregularMinor: number, openingWithheldMinor: number, priorIncomeComplete: boolean, evidenceRef: string, userId: string, now: number, now2: number) {
            return database.prepare(`INSERT INTO hr_thai_payroll_profiles(id,company_id,engagement_id,tax_year,effective_from,effective_to,annual_deductions_minor,scheduled_pays,social_security_eligible,social_security_exemption_evidence,opening_through_date,opening_taxable_minor,opening_irregular_minor,opening_withheld_minor,prior_income_complete,evidence_ref,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
                .bind(id, companyId, engagementId, taxYear, effectiveFrom, effectiveTo, verifiedAnnualDeductionsMinor, scheduledPaysInYear, socialSecurityEligible, value, openingThroughDate, openingTaxableMinor, openingIrregularMinor, openingWithheldMinor, priorIncomeComplete, evidenceRef, userId, now, now2);
        },
        profile(id: string, companyId: string) { return database.prepare("SELECT * FROM hr_thai_payroll_profiles WHERE id=? AND company_id=?").bind(id, companyId).first<Profile>(); },
        lockProfile(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(value); },
        guardProfileVersion(key: string, value: "profile", id: string, companyId: string, status: "draft" | "approved", version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE id=? AND company_id=? AND status=? AND version=?)", [id, companyId, status, version]); },
        guardNonOverlappingApproval(key: string, value: "overlap", engagement_id: string, companyId: string, tax_year: number, effective_to: string, effective_from: string) { return assertion(database, key, value, "NOT EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE engagement_id=? AND company_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?)", [engagement_id, companyId, tax_year, effective_to, effective_from]); },
        decideProfile(status: "approved" | "superseded", status2: "approved" | "superseded", userId: string, now: number, id: string) { return database.prepare("UPDATE hr_thai_payroll_profiles SET status=?,approved_by=CASE WHEN ?='approved' THEN ? ELSE approved_by END,version=version+1,updated_at=? WHERE id=?").bind(status, status2, userId, now, id); },
        ...payrollStatutoryDecisionEffects(()=>database),
        profiles(companyId: string, value: string | null, value2: string | null) { return database.prepare("SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND (?::text IS NULL OR engagement_id=?) ORDER BY created_at DESC,id").bind(companyId, value, value2).all(); },
        employees(companyId: string) { return database.prepare("SELECT e.id,e.version,e.status,e.worker_type,e.employment_type,p.legal_name FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND e.currency='THB' AND e.worker_type='employee' ORDER BY p.legal_name,e.id").bind(companyId).all(); },
        employeeProfiles(company_id: string, engagementId: string, year: number, pay_date: string | undefined, pay_date2: string | undefined) { return database.prepare("SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? AND engagement_id=? AND tax_year=? AND status='approved' AND effective_from<=? AND effective_to>=?").bind(company_id, engagementId, year, pay_date, pay_date2).all<Profile>(); },
        priorItems(company_id: string, engagementId: string, opening_through_date: string, pay_date: string | undefined, id: string, value: string) {
            return database.prepare(`SELECT i.id,i.run_id,r.version run_version,r.input_digest,i.result_json,r.pay_date FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE r.company_id=? AND i.engagement_id=? AND r.status='posted' AND r.currency='THB' AND r.pay_date>? AND r.pay_date<=? AND r.id<>? AND r.pay_date>=? ORDER BY r.pay_date,i.id`)
                .bind(company_id, engagementId, opening_through_date, pay_date, id, value).all<PriorItem>();
        },
        lockYtd(value: string) { return database.prepare("SELECT pg_advisory_xact_lock(hashtextextended(?,0))").bind(value); },
        guardEffectivePack(key: string, value: string, id: string, recordVersion: number, sourceDigest: string, rulesJson: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status='active' AND record_version=? AND source_digest=? AND rules_json=?)", [id, recordVersion, sourceDigest, rulesJson]); },
        guardApprovedProfile(key: string, value: string, id: string, company_id: string, engagementId: string, version: number) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE id=? AND company_id=? AND engagement_id=? AND status='approved' AND version=?)", [id, company_id, engagementId, version]); },
        guardRetainedYtdCount(key: string, value: string, company_id: string, engagementId: string, opening_through_date: string, value2: string, id: string, value3: string, length: number) { return assertion(database, key, value, "(SELECT COUNT(*) FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE r.company_id=? AND i.engagement_id=? AND r.status='posted' AND r.currency='THB' AND r.pay_date>? AND r.pay_date<=? AND r.id<>? AND r.pay_date>=?)=?", [company_id, engagementId, opening_through_date, value2, id, value3, length]); },
        guardRetainedYtdItem(key: string, value: string, id: string, run_id: string, run_version: number, input_digest: string, result_json: string) { return assertion(database, key, value, "EXISTS(SELECT 1 FROM hr_payroll_items i JOIN hr_payroll_runs r ON r.id=i.run_id WHERE i.id=? AND i.run_id=? AND r.version=? AND r.status='posted' AND r.input_digest=? AND i.result_json=?)", [id, run_id, run_version, input_digest, result_json]); },
        run(runId: string) { return database.prepare("SELECT mode,country_pack_id,currency FROM hr_payroll_runs WHERE id=?").bind(runId).first<{
            mode: string;
            country_pack_id: string | null;
            currency: string;
        }>(); },
        items(runId: string) { return database.prepare("SELECT gross_minor,deductions_minor,net_minor,result_json FROM hr_payroll_items WHERE run_id=?").bind(runId).all<{
            gross_minor: number;
            deductions_minor: number;
            net_minor: number;
            result_json: string;
        }>(); },
        retainedPack(country_pack_id: string) { return database.prepare("SELECT * FROM hr_country_packs WHERE id=? AND status IN ('active','superseded')").bind(country_pack_id).first<PackRow>(); } };
    return createPayrollStatutoryApplication({ repository, clock: { now: Date.now }, identifiers: { next: () => crypto.randomUUID() }, hash: sha256, authorize: requirePermission, prepare: async (holding) => { database = provided ?? (await holdingResources(holding)).write; }, commands: { replay: (...args) => replayCommand(database, ...args), commit: (...args) => executeCommand(database, ...args) } });
}
function sync<Result>(run: () => Result) { try {
    return run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
async function invoke<Result>(run: () => Promise<Result>) { try {
    return await run();
}
catch (error) {
    if (error instanceof BusinessRuleError)
        throw new ApiError(error.status, error.code, error.message);
    throw error;
} }
export async function recognizedThaiPack(rules: unknown, countryCode: string, version: string) { return invoke(() => application().recognizedThaiPack(rules, countryCode, version)); }
export async function thaiPackForRun(db: SqlDatabase, run: PayrollStatutoryRun) { return invoke(() => application(db).thaiPackForRun(run)); }
export async function createThaiPayrollProfile(actor: WorkContext, input: z.infer<typeof thaiPayrollProfileSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: "people.thai-profile.create", purpose: "new-work" }, tx => invoke(() => application(tx).createThaiPayrollProfile(actor, input, key))); }
export async function decideThaiPayrollProfile(actor: WorkContext, id: string, input: z.infer<typeof thaiPayrollProfileDecisionSchema>, key: string) { return withPayrollPurchasedCommand(actor, { key: key, operation: `people.thai-profile.${input.action}`, purpose: "settle-existing" }, tx => invoke(() => application(tx).decideThaiPayrollProfile(actor, id, input, key))); }
export async function thaiPayrollConfiguration(actor: WorkContext, engagementId?: string) { return withPayrollPurchasedRead(actor, tx => invoke(() => application(tx).thaiPayrollConfiguration(actor, engagementId))); }
export async function thaiEvidenceForEmployee(db: SqlDatabase, run: PayrollStatutoryRun, engagementId: string) { return invoke(() => application(db).thaiEvidenceForEmployee(run, engagementId)); }
export function applyThaiStatutory(calculated: Calculated, evidence: ThaiPayrollEvidence, run: PayrollStatutoryRun, overtimeMinor: number) { return sync(() => application().applyThaiStatutory(calculated, evidence, run, overtimeMinor)); }
export function thaiEvidenceGuards(db: SqlDatabase, key: string, evidence: ThaiPayrollEvidence, run: PayrollStatutoryRun, engagementId: string) { return sync(() => application(db).thaiEvidenceGuards(key, evidence, run, engagementId)); }
export async function assertStatutoryRunReady(db: SqlDatabase, runId: string) { return invoke(() => application(db).assertStatutoryRunReady(runId)); }
