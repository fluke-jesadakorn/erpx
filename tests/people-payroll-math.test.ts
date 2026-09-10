import { describe, expect, it } from "vitest";
import { calculatePayrollAmounts } from "../src/lib/payroll-math";

describe("payroll calculation boundaries", () => {
  it("includes contractual earnings before subtracting employee deductions", () => {
    const result=calculatePayrollAmounts({employmentType:"salaried",baseMinor:320000,hourlyMinor:0,regularHours:160,overtimeHours:0,components:[
      {code:"allowance",label:"Home office allowance",kind:"earning",amount:"150.00"},
      {code:"benefit",label:"Voluntary benefit",kind:"deduction",amount:"50.00"},
    ]});
    expect(result).toMatchObject({gross:335000,deductions:5000,net:330000});
  });
  it("calculates salaried pay and preserves component explanations", () => {
    const result = calculatePayrollAmounts({
      employmentType: "salaried",
      baseMinor: 300000,
      hourlyMinor: 0,
      regularHours: 0,
      overtimeHours: 0,
      components: [
        { code: "tax", label: "Tax", kind: "deduction", amount: "250.00" },
        { code: "pension", label: "Pension", kind: "employer", amount: "150.00" },
      ],
    });
    expect(result).toMatchObject({ gross: 300000, deductions: 25000, employer: 15000, net: 275000 });
    expect(result.applied[0]?.amountMinor).toBe(25000);
  });

  it("applies overtime at one and a half times for hourly workers", () => {
    const result = calculatePayrollAmounts({
      employmentType: "hourly",
      baseMinor: 0,
      hourlyMinor: 1000,
      regularHours: 40,
      overtimeHours: 4,
      components: [],
    });
    expect(result.gross).toBe(46000);
    expect(result.net).toBe(46000);
  });
});
