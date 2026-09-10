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
  components: PayrollComponentInput[];
}) {
  let gross = input.employmentType === "salaried"
    ? input.baseMinor
    : input.employmentType === "contractor"
      ? input.baseMinor
      : Math.round(input.hourlyMinor * (input.regularHours + input.overtimeHours * 1.5));
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
