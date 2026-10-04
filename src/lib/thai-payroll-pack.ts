import type { ThaiPayrollRules } from "./thai-payroll-rules";

/** The source bytes and statutory interpretation are recorded in docs/testing/thai-payroll-rules-2026-10-02.md. */
export const thaiPayrollAdapterId = "thai-payroll-v1";
export const thaiPayroll2026Sources = [
  { kind: "withholding_method", url: "https://www.rd.go.th/3558.html", sha256: "541cd5d79114c346ddfd5ffebd8baf3d1745e7da061e366de30297177948bb8a" },
  { kind: "employment_expense", url: "https://www.rd.go.th/5937.html", sha256: "7c4b70b52f97b39e371284cdfdcab9f75b56a44b27cadd549bdf9934221e882a" },
  { kind: "tax_bands", url: "https://www.rd.go.th/5938.html", sha256: "5ec38d0dbbb7ee5d226ad1a7422aa14ff603dcf29dfc566e47e48bd04bc0a3b4" },
  { kind: "sso_wage_base", url: "https://ratchakitcha.soc.go.th/documents/98728.pdf", sha256: "529c7de6cbf8697e4777e1489639b60833131abece6f9bb7e7778c6ad3f6b07b" },
] as const;

export const thaiPayroll2026Rules: ThaiPayrollRules = {
  countryCode: "TH", currency: "THB", version: "TH-2026-10-02-r1",
  effectiveFrom: "2026-01-01", effectiveTo: "2026-12-31",
  standardExpenseRateBps: 5000, standardExpenseCapMinor: 10_000_000,
  taxBands: [
    { upToMinor: 15_000_000, rateBps: 0 },
    { upToMinor: 30_000_000, rateBps: 500 },
    { upToMinor: 50_000_000, rateBps: 1000 },
    { upToMinor: 75_000_000, rateBps: 1500 },
    { upToMinor: 100_000_000, rateBps: 2000 },
    { upToMinor: 200_000_000, rateBps: 2500 },
    { upToMinor: 500_000_000, rateBps: 3000 },
    { upToMinor: null, rateBps: 3500 },
  ],
  socialSecurity: {
    employeeRateBps: 500, employerRateBps: 500, minimumWageBaseMinor: 165_000,
    maximumWageBaseMinor: 1_750_000, roundingUnitMinor: 100,
  },
};

/** Country-pack installation still requires the ordinary independent internal review command. */
export const thaiPayroll2026Pack = {
  adapter: thaiPayrollAdapterId, calculationScope: "monthly-section40(1)-section33",
  certified: false, rules: thaiPayroll2026Rules, sources: thaiPayroll2026Sources,
};
