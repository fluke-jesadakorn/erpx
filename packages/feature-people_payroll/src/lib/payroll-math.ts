import { decimalToMinor } from "@erpx/feature-finance/lib/finance-money";

export type PayrollComponentInput = {
  code: string;
  label: string;
  kind: "earning" | "deduction" | "employer";
  amount: string;
  taxable?: boolean;
};

function ratio(amount:number,numerator:number,denominator:number){
  if(!Number.isSafeInteger(amount)||amount<0||!Number.isSafeInteger(numerator)||numerator<0||!Number.isSafeInteger(denominator)||denominator<=0)throw new Error("Payroll quantities must use nonnegative reviewed precision.");
  const product=BigInt(amount)*BigInt(numerator),result=(product+BigInt(denominator)/BigInt(2))/BigInt(denominator);
  if(result>BigInt(Number.MAX_SAFE_INTEGER))throw new Error("Payroll money exceeds supported precision.");return Number(result);
}

export function calculatePayrollAmounts(input: {
  employmentType: "salaried" | "daily" | "hourly" | "shift" | "contractor";
  baseMinor: number;
  hourlyMinor: number;
  dailyMinor?:number;
  regularDays?:number;
  regularHours: number;
  overtimeHours: number;
  overtimeMultiplierBps?: number;
  prorationNumerator?: number;
  prorationDenominator?: number;
  components: PayrollComponentInput[];
}) {
  const base=input.prorationNumerator!==undefined&&input.prorationDenominator
    ? ratio(input.baseMinor,input.prorationNumerator,input.prorationDenominator)
    : input.baseMinor;
  if (["salaried","daily"].includes(input.employmentType) && input.overtimeHours > 0 && input.hourlyMinor <= 0) {
    throw new Error("Approved hourly rate is required to pay salaried overtime.");
  }
  const overtimePay = ratio(input.hourlyMinor,Math.round(input.overtimeHours*1_000_000)*(input.overtimeMultiplierBps??15000),10_000_000_000);
  let gross = input.employmentType === "salaried"
    ? base + overtimePay
    :input.employmentType==="daily"?ratio(input.dailyMinor??0,Math.round((input.regularDays??0)*1_000_000),1_000_000)+overtimePay
    : input.employmentType === "contractor"
      ? input.baseMinor
      : ratio(input.hourlyMinor,Math.round(input.regularHours*1_000_000)*10000+Math.round(input.overtimeHours*1_000_000)*(input.overtimeMultiplierBps??15000),10_000_000_000);
  let deductions = 0;
  let employer = 0;
  const applied = input.components.map((component) => {
    const amountMinor = decimalToMinor(component.amount);
    if (component.kind === "earning") gross += amountMinor;
    if (component.kind === "deduction") deductions += amountMinor;
    if (component.kind === "employer") employer += amountMinor;
    return { ...component, amountMinor };
  });
  return { gross, deductions, employer, net: gross - deductions, overtimePay, applied };
}
