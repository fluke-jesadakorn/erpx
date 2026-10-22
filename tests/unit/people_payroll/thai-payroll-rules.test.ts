import { describe, expect, it } from "vitest";
import { calculateThaiPayroll, ThaiPayrollRuleError, type ThaiPayrollRules } from "@erpx/feature-people_payroll/lib/thai-payroll-rules";
import fixture from "../../../fixtures/payroll-thai/synthetic-kernel-v1.json";

const syntheticRules: ThaiPayrollRules = {
  countryCode: "TH",
  currency: "THB",
  version: "synthetic-test-only",
  effectiveFrom: "2026-01-01",
  effectiveTo: "2026-12-31",
  standardExpenseRateBps: 0,
  standardExpenseCapMinor: 0,
  taxBands: [{ upToMinor: 10_000, rateBps: 0 }, { upToMinor: null, rateBps: 1_000 }],
  socialSecurity: { employeeRateBps: 500, employerRateBps: 500, minimumWageBaseMinor: 0, maximumWageBaseMinor: 20_000, roundingUnitMinor: 1 },
};

const base = {
  payDate: "2026-09-30",
  regularTaxableMinor: 10_000,
  irregularTaxableMinor: 1_000,
  nonTaxableMinor: 0,
  socialSecurityWageMinor: 11_000,
  socialSecurityEligible: true,
  otherEmployeeDeductionsMinor: 0,
  verifiedAnnualDeductionsMinor: 0,
  taxablePaidBeforeMinor: 80_000,
  taxWithheldBeforeMinor: 5_000,
  scheduledPaysInYear: 12,
  lastPayInYear: false,
  priorIncomeComplete: true,
};

describe("Thai payroll calculation kernel with explicit reviewed rules", () => {
  it("matches the checked-in synthetic expected-result cases without claiming certification", () => {
    expect(fixture.certified).toBe(false);
    for (const scenario of fixture.cases) {
      expect(calculateThaiPayroll(scenario.input, fixture.rules as unknown as ThaiPayrollRules), scenario.id)
        .toMatchObject(scenario.expected);
    }
  });
  it("annualizes ordinary pay, calculates irregular income separately, and splits employee/employer contributions", () => {
    const result = calculateThaiPayroll(base, syntheticRules);
    expect(result).toMatchObject({ grossMinor: 11_000, taxWithheldMinor: 1_016, employeeSsoMinor: 550, employerSsoMinor: 550, netMinor: 9_434 });
    expect(result.explanation).toMatchObject({ ruleVersion: "synthetic-test-only", ordinaryAnnualTaxMinor: 11_000, irregularAnnualTaxDifferenceMinor: 100 });
  });

  it("uses actual year-to-date income and withholding for the last pay in a year", () => {
    const result = calculateThaiPayroll({ ...base, irregularTaxableMinor: 0, socialSecurityWageMinor: 10_000, taxablePaidBeforeMinor: 110_000, taxWithheldBeforeMinor: 10_080, lastPayInYear: true }, syntheticRules);
    expect(result.taxWithheldMinor).toBe(920);
    expect(result.netMinor).toBe(8_580);
    expect(result.taxWithheldYtdMinor).toBe(11_000);
  });

  it("stops when rule date, YTD evidence, social-security exemption or net pay is invalid", () => {
    const code = (run: () => unknown) => {
      try { run(); } catch (error) { expect(error).toBeInstanceOf(ThaiPayrollRuleError); return (error as ThaiPayrollRuleError).code; }
      throw new Error("Expected ThaiPayrollRuleError");
    };
    expect(code(() => calculateThaiPayroll({ ...base, payDate: "2027-01-01" }, syntheticRules))).toBe("COUNTRY_PACK_NOT_EFFECTIVE");
    expect(code(() => calculateThaiPayroll({ ...base, priorIncomeComplete: false }, syntheticRules))).toBe("YTD_INCOMPLETE");
    expect(code(() => calculateThaiPayroll({ ...base, socialSecurityEligible: false }, syntheticRules))).toBe("SSO_ELIGIBILITY_UNVERIFIED");
    expect(code(() => calculateThaiPayroll({ ...base, otherEmployeeDeductionsMinor: 20_000 }, syntheticRules))).toBe("NET_PAY_NEGATIVE");
  });
});
