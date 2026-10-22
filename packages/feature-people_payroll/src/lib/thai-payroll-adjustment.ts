import { multiplyBps } from "@erpx/feature-finance/lib/finance-money";
import { calculateThaiPayroll, ThaiPayrollRuleError, type ThaiPayrollCalculation, type ThaiPayrollRules } from "@erpx/feature-people_payroll/lib/thai-payroll-rules";

/** A separately paid correction retains the ordinary salary basis but pays it only once. */
export type ThaiPayrollAdjustmentInput = ThaiPayrollCalculation & {
  ordinaryTaxableBasisMinor: number;
  originalSocialSecurityWageMinor: number;
  originalEmployeeSsoMinor: number;
  originalEmployerSsoMinor: number;
  priorAdjustmentSocialSecurityWageMinor: number;
  priorAdjustmentEmployeeSsoMinor: number;
  priorAdjustmentEmployerSsoMinor: number;
};

export function calculateThaiPayrollAdjustment(input: ThaiPayrollAdjustmentInput, rules: ThaiPayrollRules) {
  for (const name of ["ordinaryTaxableBasisMinor", "originalSocialSecurityWageMinor", "originalEmployeeSsoMinor", "originalEmployerSsoMinor", "priorAdjustmentSocialSecurityWageMinor", "priorAdjustmentEmployeeSsoMinor", "priorAdjustmentEmployerSsoMinor"] as const) {
    if (!Number.isSafeInteger(input[name]) || input[name] < 0) throw new ThaiPayrollRuleError("INVALID_PAYROLL_INPUT", `${name} must be a nonnegative minor-unit integer.`);
  }
  if (input.regularTaxableMinor !== 0 || input.ordinaryTaxableBasisMinor <= 0) throw new ThaiPayrollRuleError("PAYROLL_ADJUSTMENT_BASIS_REQUIRED", "Retain a reviewed positive ordinary salary basis; a correction cannot pay regular salary twice.");
  const calculated = calculateThaiPayroll({ ...input, regularTaxableMinor: input.lastPayInYear ? 0 : input.ordinaryTaxableBasisMinor, socialSecurityWageMinor: 0 }, rules);
  const tax = input.lastPayInYear ? calculated.taxWithheldMinor : calculated.explanation.irregularAnnualTaxDifferenceMinor;
  const totalWage = input.originalSocialSecurityWageMinor + input.priorAdjustmentSocialSecurityWageMinor + input.socialSecurityWageMinor;
  if (!Number.isSafeInteger(totalWage)) throw new ThaiPayrollRuleError("MONEY_OVERFLOW", "Contribution wage exceeds supported precision.");
  const sso = rules.socialSecurity;
  const base = !input.socialSecurityEligible || totalWage === 0 ? 0 : Math.min(sso.maximumWageBaseMinor, Math.max(sso.minimumWageBaseMinor, totalWage));
  const contribution = (rate: number) => Number((multiplyBps(BigInt(base), rate) + BigInt(sso.roundingUnitMinor / 2)) / BigInt(sso.roundingUnitMinor) * BigInt(sso.roundingUnitMinor));
  const employeeSso = contribution(sso.employeeRateBps) - input.originalEmployeeSsoMinor - input.priorAdjustmentEmployeeSsoMinor;
  const employerSso = contribution(sso.employerRateBps) - input.originalEmployerSsoMinor - input.priorAdjustmentEmployerSsoMinor;
  if (employeeSso < 0 || employerSso < 0) throw new ThaiPayrollRuleError("PAYROLL_CONTRIBUTION_CORRECTION_REVIEW", "A contribution refund needs a separately reviewed statutory correction.");
  const gross = input.irregularTaxableMinor + input.nonTaxableMinor;
  const net = gross - tax - employeeSso - input.otherEmployeeDeductionsMinor;
  if (!Number.isSafeInteger(gross) || !Number.isSafeInteger(net) || net < 0) throw new ThaiPayrollRuleError("NET_PAY_NEGATIVE", "Use the reviewed recovery handoff for an overpayment; do not create negative bank payroll.");
  return { ...calculated, grossMinor: gross, taxWithheldMinor: tax, employeeSsoMinor: employeeSso, employerSsoMinor: employerSso, netMinor: net,
    taxableYtdMinor: input.taxablePaidBeforeMinor + input.irregularTaxableMinor,
    taxWithheldYtdMinor: input.taxWithheldBeforeMinor + tax,
    explanation: { ...calculated.explanation, socialSecurityBaseMinor: base, adjustment: true, ordinaryTaxableBasisMinor: input.ordinaryTaxableBasisMinor, originalEmployeeSsoMinor: input.originalEmployeeSsoMinor, priorAdjustmentEmployeeSsoMinor: input.priorAdjustmentEmployeeSsoMinor } };
}
