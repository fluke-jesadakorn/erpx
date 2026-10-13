import { describe, expect, it } from "vitest";
import { calculateThaiPayroll } from "../../../src/lib/people_payroll/thai-payroll-rules";
import { thaiPayroll2026Rules } from "../../../src/lib/people_payroll/thai-payroll-pack";
import { thai2026Reference } from "../../support/helpers/thai-payroll-reference";
import {thai2026AdjustmentReference} from "../../support/helpers/thai-payroll-reference";
import {calculateThaiPayrollAdjustment,type ThaiPayrollAdjustmentInput} from "../../../src/lib/people_payroll/thai-payroll-adjustment";

describe("source-dated Thai 2026 adapter against independent worksheets", () => {
  for (const scenario of [
    { name: "monthly salary", ordinaryBaht: 30_000, occasionalBaht: 0, annualDeductionsBaht: 70_500, wageBaseBaht: 30_000, pays: 12 },
    { name: "one-off bonus is counted once", ordinaryBaht: 30_000, occasionalBaht: 90_000, annualDeductionsBaht: 70_500, wageBaseBaht: 30_000, pays: 12 },
    { name: "OT consumes prior occasional YTD", ordinaryBaht: 30_000, occasionalBaht: 5_200, occasionalYtdBaht: 6_500, annualDeductionsBaht: 70_500, wageBaseBaht: 30_000, pays: 12 },
    { name: "daily wages aggregated monthly", ordinaryBaht: 12_000, occasionalBaht: 900, annualDeductionsBaht: 67_200, wageBaseBaht: 12_000, pays: 12 },
    { name: "minimum SSO rounds 82.50 to 83", ordinaryBaht: 1000, occasionalBaht: 0, annualDeductionsBaht: 60_000, wageBaseBaht: 1000, pays: 12 },
    { name: "final true-up after salary increase", ordinaryBaht: 60_000, occasionalBaht: 0, annualDeductionsBaht: 70_500, wageBaseBaht: 60_000, pays: 12, priorTaxableBaht: 390_000, priorWithheldBaht: 4_000, final: true },
  ]) it(scenario.name, () => {
    const result = calculateThaiPayroll({
      payDate: "2026-10-01", regularTaxableMinor: scenario.ordinaryBaht * 100,
      irregularTaxableMinor: scenario.occasionalBaht * 100,
      irregularPaidBeforeMinor: ("occasionalYtdBaht" in scenario ? scenario.occasionalYtdBaht ?? 0 : 0) * 100,
      nonTaxableMinor: 0, otherEmployeeDeductionsMinor: 0, socialSecurityEligible: true,
      socialSecurityWageMinor: scenario.wageBaseBaht * 100, verifiedAnnualDeductionsMinor: scenario.annualDeductionsBaht * 100,
      taxablePaidBeforeMinor: ("priorTaxableBaht" in scenario ? scenario.priorTaxableBaht ?? 0 : 0) * 100,
      taxWithheldBeforeMinor: ("priorWithheldBaht" in scenario ? scenario.priorWithheldBaht ?? 0 : 0) * 100,
      scheduledPaysInYear: scenario.pays, lastPayInYear: "final" in scenario && !!scenario.final, priorIncomeComplete: true,
    }, thaiPayroll2026Rules);
    expect(result).toMatchObject(thai2026Reference(scenario));
  });
  it("uses the 2026 ceiling without backdating it into 2025", () => {
    expect(thaiPayroll2026Rules.socialSecurity.maximumWageBaseMinor).toBe(1_750_000);
    expect(thaiPayroll2026Rules.effectiveFrom).toBe("2026-01-01");
    expect(thaiPayroll2026Rules.effectiveTo).toBe("2026-12-31");
  });
  for(const scenario of [
    {name:'incremental SSO below the ceiling',ordinary:12000,originalWage:12000,delta:1000,wageDelta:1000,originalSso:600,priorWage:0,priorSso:0},
    {name:'ceiling contribution paid only once',ordinary:30000,originalWage:30000,delta:1750,wageDelta:1000,originalSso:875,priorWage:0,priorSso:0},
    {name:'later contribution delta retains prior adjustment',ordinary:12000,originalWage:12000,delta:1000,wageDelta:1000,originalSso:600,priorWage:1000,priorSso:50},
  ])it(`compares posted correction ${scenario.name} to a separate cash worksheet`,()=>{
    const input:ThaiPayrollAdjustmentInput={payDate:'2026-10-03',regularTaxableMinor:0,ordinaryTaxableBasisMinor:scenario.ordinary*100,irregularTaxableMinor:scenario.delta*100,irregularPaidBeforeMinor:100000,nonTaxableMinor:0,otherEmployeeDeductionsMinor:0,socialSecurityEligible:true,socialSecurityWageMinor:scenario.wageDelta*100,originalSocialSecurityWageMinor:scenario.originalWage*100,originalEmployeeSsoMinor:scenario.originalSso*100,originalEmployerSsoMinor:scenario.originalSso*100,priorAdjustmentSocialSecurityWageMinor:scenario.priorWage*100,priorAdjustmentEmployeeSsoMinor:scenario.priorSso*100,priorAdjustmentEmployerSsoMinor:scenario.priorSso*100,verifiedAnnualDeductionsMinor:7050000,taxablePaidBeforeMinor:27000000,taxWithheldBeforeMinor:150000,scheduledPaysInYear:12,lastPayInYear:false,priorIncomeComplete:true};
    const reference=thai2026AdjustmentReference({ordinaryBasisBaht:scenario.ordinary,deltaBaht:scenario.delta,occasionalYtdBaht:1000,annualDeductionsBaht:70500,originalWageBaht:scenario.originalWage+scenario.priorWage,additionalWageBaht:scenario.wageDelta,originalEmployeeSsoBaht:scenario.originalSso+scenario.priorSso,originalEmployerSsoBaht:scenario.originalSso+scenario.priorSso,pays:12});
    expect(calculateThaiPayrollAdjustment(input,thaiPayroll2026Rules)).toMatchObject(reference);
  });
  it('reconciles actual December correction income without repaying ordinary salary or automatically refunding tax',()=>{
    const input:ThaiPayrollAdjustmentInput={payDate:'2026-12-03',regularTaxableMinor:0,ordinaryTaxableBasisMinor:3000000,irregularTaxableMinor:100000,irregularPaidBeforeMinor:0,nonTaxableMinor:0,otherEmployeeDeductionsMinor:0,socialSecurityEligible:true,socialSecurityWageMinor:0,originalSocialSecurityWageMinor:3000000,originalEmployeeSsoMinor:87500,originalEmployerSsoMinor:87500,priorAdjustmentSocialSecurityWageMinor:0,priorAdjustmentEmployeeSsoMinor:0,priorAdjustmentEmployerSsoMinor:0,verifiedAnnualDeductionsMinor:7050000,taxablePaidBeforeMinor:36000000,taxWithheldBeforeMinor:187499,scheduledPaysInYear:12,lastPayInYear:true,priorIncomeComplete:true};
    const reference=thai2026Reference({ordinaryBaht:0,occasionalBaht:1000,annualDeductionsBaht:70500,wageBaseBaht:0,pays:12,priorTaxableBaht:360000,priorWithheldBaht:1874.99,final:true});
    expect(calculateThaiPayrollAdjustment(input,thaiPayroll2026Rules)).toMatchObject(reference);
    expect(()=>calculateThaiPayrollAdjustment({...input,taxWithheldBeforeMinor:1000000},thaiPayroll2026Rules)).toThrow(/refund|review/iu);
  });
});
