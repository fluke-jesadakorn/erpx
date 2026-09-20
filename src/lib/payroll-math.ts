import { decimalToMinor } from "./finance-money";

export type PayrollComponentInput = {
  code: string;
  label: string;
  kind: "earning" | "deduction" | "employer";
  amount: string;
  taxable?: boolean;
};

export function calculatePayrollAmounts(input: {
  employmentType: "salaried" | "hourly" | "shift" | "contractor";
  baseMinor: number;
  hourlyMinor: number;
  regularHours: number;
  overtimeHours: number;
  overtimeMultiplierBps?: number;
  prorationNumerator?: number;
  prorationDenominator?: number;
  components: PayrollComponentInput[];
}) {
  const base=input.prorationNumerator!==undefined&&input.prorationDenominator
    ? Math.round(input.baseMinor*input.prorationNumerator/input.prorationDenominator)
    : input.baseMinor;
  const overtimeMultiplier=(input.overtimeMultiplierBps??15000)/10000;
  let gross = input.employmentType === "salaried"
    ? base
    : input.employmentType === "contractor"
      ? input.baseMinor
      : Math.round(input.hourlyMinor * (input.regularHours + input.overtimeHours * overtimeMultiplier));
  let deductions = 0;
  let employer = 0;
  const applied = input.components.map((component) => {
    const amountMinor = decimalToMinor(component.amount);
    if (component.kind === "earning") gross += amountMinor;
    if (component.kind === "deduction") deductions += amountMinor;
    if (component.kind === "employer") employer += amountMinor;
    return { ...component, amountMinor };
  });
  return { gross, deductions, employer, net: gross - deductions, applied };
}
