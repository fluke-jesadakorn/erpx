import { multiplyBps } from "./finance-money";

/**
 * A calculation kernel, not an activated or certified Thai country pack.
 * Ordinary-pay annualization, occasional-income differences and final-year
 * reconciliation follow the structure of Revenue Department order ป.96/2543:
 * https://www.rd.go.th/3558.html . The effective rates and limits must come
 * from independently reviewed rules for the particular payment date.
 */
export type ThaiPayrollRules = {
  countryCode: "TH";
  currency: "THB";
  version: string;
  effectiveFrom: string;
  effectiveTo: string;
  standardExpenseRateBps: number;
  standardExpenseCapMinor: number;
  taxBands: Array<{ upToMinor: number | null; rateBps: number }>;
  socialSecurity: {
    employeeRateBps: number;
    employerRateBps: number;
    minimumWageBaseMinor: number;
    maximumWageBaseMinor: number;
    roundingUnitMinor: number;
  };
};

export type ThaiPayrollCalculation = {
  payDate: string;
  regularTaxableMinor: number;
  irregularTaxableMinor: number;
  /** Actual prior occasional income in this tax year (RD ป.96/2543 example 4). */
  irregularPaidBeforeMinor?: number;
  /** Occurrences of this occasional payment; a one-off bonus has one, not twelve. */
  irregularOccurrencesInYear?: number;
  /** One-time retirement/severance income has separate treatment and is not handled by ป.96/2543. */
  terminationSeveranceMinor?: number;
  nonTaxableMinor: number;
  socialSecurityWageMinor: number;
  socialSecurityEligible: boolean;
  /** Evidence for a verified statutory exemption; unknown eligibility is never represented as false. */
  socialSecurityExemptionEvidence?: string;
  otherEmployeeDeductionsMinor: number;
  /** Verified annual allowances and deductions, excluding the standard employment expense deduction. */
  verifiedAnnualDeductionsMinor: number;
  taxablePaidBeforeMinor: number;
  taxWithheldBeforeMinor: number;
  scheduledPaysInYear: number;
  lastPayInYear: boolean;
  /** Required for final annual true-up; prior income must be complete. */
  priorIncomeComplete: boolean;
};

export class ThaiPayrollRuleError extends Error {
  constructor(readonly code: string, message: string) {
    super(message);
    this.name = "ThaiPayrollRuleError";
  }
}

function fail(code: string, message: string): never {
  throw new ThaiPayrollRuleError(code, message);
}

function money(value: number, name: string): bigint {
  if (!Number.isSafeInteger(value) || value < 0) fail("INVALID_PAYROLL_INPUT", `${name} must be a nonnegative minor-unit integer.`);
  return BigInt(value);
}

function safe(value: bigint): number {
  if (value > BigInt(Number.MAX_SAFE_INTEGER) || value < BigInt(Number.MIN_SAFE_INTEGER)) {
    fail("MONEY_OVERFLOW", "Payroll result exceeds supported precision.");
  }
  return Number(value);
}

function roundUnit(value: bigint, unit: bigint): bigint {
  return ((value + unit / BigInt(2)) / unit) * unit;
}

function validateRules(rules: ThaiPayrollRules, payDate: string) {
  if (rules.countryCode !== "TH" || rules.currency !== "THB" || !rules.version.trim() ||
      !/^\d{4}-\d{2}-\d{2}$/.test(payDate) || payDate < rules.effectiveFrom || payDate > rules.effectiveTo) {
    fail("COUNTRY_PACK_NOT_EFFECTIVE", "An effective, versioned Thai rule pack is required for the pay date.");
  }
  money(rules.standardExpenseCapMinor, "standardExpenseCapMinor");
  if (!Number.isInteger(rules.standardExpenseRateBps) || rules.standardExpenseRateBps < 0 || rules.standardExpenseRateBps > 10_000) {
    fail("COUNTRY_PACK_INVALID", "Invalid standard expense rate.");
  }
  let previous = BigInt(0);
  if (!rules.taxBands.length || rules.taxBands.at(-1)?.upToMinor !== null) fail("COUNTRY_PACK_INVALID", "Tax bands need an unbounded final band.");
  for (const [index, band] of rules.taxBands.entries()) {
    if (!Number.isInteger(band.rateBps) || band.rateBps < 0 || band.rateBps > 10_000) fail("COUNTRY_PACK_INVALID", "Invalid tax band rate.");
    if (band.upToMinor === null) {
      if (index !== rules.taxBands.length - 1) fail("COUNTRY_PACK_INVALID", "Only the final tax band may be unbounded.");
    } else {
      const upper = money(band.upToMinor, "taxBand.upToMinor");
      if (upper <= previous) fail("COUNTRY_PACK_INVALID", "Tax bands must have increasing limits.");
      previous = upper;
    }
  }
  const sso = rules.socialSecurity;
  money(sso.minimumWageBaseMinor, "minimumWageBaseMinor");
  money(sso.maximumWageBaseMinor, "maximumWageBaseMinor");
  money(sso.roundingUnitMinor, "roundingUnitMinor");
  if (sso.maximumWageBaseMinor < sso.minimumWageBaseMinor || sso.roundingUnitMinor < 1 ||
      !Number.isInteger(sso.employeeRateBps) || sso.employeeRateBps < 0 || sso.employeeRateBps > 10_000 ||
      !Number.isInteger(sso.employerRateBps) || sso.employerRateBps < 0 || sso.employerRateBps > 10_000) {
    fail("COUNTRY_PACK_INVALID", "Invalid social security rule.");
  }
}

function annualTax(income: bigint, deductions: bigint, rules: ThaiPayrollRules): bigint {
  const expense = [multiplyBps(income, rules.standardExpenseRateBps), BigInt(rules.standardExpenseCapMinor)]
    .reduce((a, b) => a < b ? a : b);
  let remaining = income - expense - deductions;
  if (remaining <= BigInt(0)) return BigInt(0);
  let lower = BigInt(0);
  let tax = BigInt(0);
  for (const band of rules.taxBands) {
    const width = band.upToMinor === null ? remaining : BigInt(band.upToMinor) - lower;
    const taxable = remaining < width ? remaining : width;
    tax += multiplyBps(taxable, band.rateBps);
    remaining -= taxable;
    if (remaining === BigInt(0)) break;
    if (band.upToMinor !== null) lower = BigInt(band.upToMinor);
  }
  return tax;
}

/**
 * Deterministic calculation for a reviewed monthly-pay input. Rates, allowances and
 * eligibility must be supplied by a separately validated country pack and HR evidence.
 * The caller must not use this kernel as permission to enable production payroll.
 */
export function calculateThaiPayroll(input: ThaiPayrollCalculation, rules: ThaiPayrollRules) {
  validateRules(rules, input.payDate);
  if (money(input.terminationSeveranceMinor ?? 0, "terminationSeveranceMinor") > BigInt(0)) {
    fail("TERMINATION_TAX_UNSUPPORTED", "One-time termination income requires a separately reviewed tax rule.");
  }
  if (!Number.isInteger(input.scheduledPaysInYear) || input.scheduledPaysInYear < 1 || input.scheduledPaysInYear > 12) {
    fail("UNSUPPORTED_PAY_FREQUENCY", "Thai payroll needs a reviewed monthly payment count of 1–12.");
  }
  if (!input.priorIncomeComplete) fail("YTD_INCOMPLETE", "Verified prior taxable income and withholding are required.");
  if (!input.socialSecurityEligible && !input.socialSecurityExemptionEvidence?.trim()) {
    fail("SSO_ELIGIBILITY_UNVERIFIED", "A verified reason is required to exclude social security.");
  }
  const regular = money(input.regularTaxableMinor, "regularTaxableMinor");
  const irregular = money(input.irregularTaxableMinor, "irregularTaxableMinor");
  const nonTaxable = money(input.nonTaxableMinor, "nonTaxableMinor");
  const ssoWage = money(input.socialSecurityWageMinor, "socialSecurityWageMinor");
  const otherDeductions = money(input.otherEmployeeDeductionsMinor, "otherEmployeeDeductionsMinor");
  const deductions = money(input.verifiedAnnualDeductionsMinor, "verifiedAnnualDeductionsMinor");
  const paidBefore = money(input.taxablePaidBeforeMinor, "taxablePaidBeforeMinor");
  const withheldBefore = money(input.taxWithheldBeforeMinor, "taxWithheldBeforeMinor");
  const pays = BigInt(input.scheduledPaysInYear);
  const ordinaryAnnualTax = annualTax(regular * pays, deductions, rules);
  const irregularBefore = money(input.irregularPaidBeforeMinor ?? 0, "irregularPaidBeforeMinor");
  if (!Number.isInteger(input.irregularOccurrencesInYear ?? 1) || (input.irregularOccurrencesInYear ?? 1) < 1 || (input.irregularOccurrencesInYear ?? 1) > 12) {
    fail("INVALID_PAYROLL_INPUT", "Occasional income needs a verified occurrence count of 1–12.");
  }
  const beforeIrregularAnnualTax = annualTax(regular * pays + irregularBefore, deductions, rules);
  const withIrregularAnnualTax = annualTax(regular * pays + irregularBefore + irregular * BigInt(input.irregularOccurrencesInYear ?? 1), deductions, rules);
  const regularWithholding = ordinaryAnnualTax / pays;
  const irregularWithholding = withIrregularAnnualTax - beforeIrregularAnnualTax;
  const annualActualTax = annualTax(paidBefore + regular + irregular, deductions, rules);
  const tax = input.lastPayInYear ? annualActualTax - withheldBefore : regularWithholding + irregularWithholding;
  if (tax < BigInt(0)) fail("TAX_OVERWITHHELD_REVIEW", "Prior withholding exceeds final computed tax; an expert must review the correction.");
  const sso = rules.socialSecurity;
  const ssoBase = !input.socialSecurityEligible || ssoWage === BigInt(0) ? BigInt(0)
    : ssoWage < BigInt(sso.minimumWageBaseMinor) ? BigInt(sso.minimumWageBaseMinor)
    : ssoWage > BigInt(sso.maximumWageBaseMinor) ? BigInt(sso.maximumWageBaseMinor) : ssoWage;
  const employeeSso = roundUnit(multiplyBps(ssoBase, sso.employeeRateBps), BigInt(sso.roundingUnitMinor));
  const employerSso = roundUnit(multiplyBps(ssoBase, sso.employerRateBps), BigInt(sso.roundingUnitMinor));
  const gross = regular + irregular + nonTaxable;
  const net = gross - tax - employeeSso - otherDeductions;
  if (net < BigInt(0)) fail("NET_PAY_NEGATIVE", "Deductions exceed earnings; review this employee's payroll input.");
  return {
    grossMinor: safe(gross), taxWithheldMinor: safe(tax), employeeSsoMinor: safe(employeeSso),
    employerSsoMinor: safe(employerSso), otherEmployeeDeductionsMinor: safe(otherDeductions),
    netMinor: safe(net), taxableYtdMinor: safe(paidBefore + regular + irregular),
    taxWithheldYtdMinor: safe(withheldBefore + tax),
    explanation: {
      ruleVersion: rules.version, ruleEffectiveFrom: rules.effectiveFrom, ruleEffectiveTo: rules.effectiveTo,
      ordinaryAnnualTaxMinor: safe(ordinaryAnnualTax), irregularAnnualTaxDifferenceMinor: safe(irregularWithholding),
      irregularPaidBeforeMinor: safe(irregularBefore), irregularOccurrencesInYear: input.irregularOccurrencesInYear ?? 1,
      socialSecurityBaseMinor: safe(ssoBase), standardExpenseRateBps: rules.standardExpenseRateBps,
      scheduledPaysInYear: input.scheduledPaysInYear, lastPayInYear: input.lastPayInYear,
    },
  };
}
