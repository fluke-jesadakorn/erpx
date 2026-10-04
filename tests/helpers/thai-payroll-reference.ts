/** Independent decimal worksheet. It deliberately does not import the production kernel or money helpers. */
export function thai2026Reference(input: {
  ordinaryBaht: number; occasionalBaht: number; occasionalYtdBaht?: number;
  annualDeductionsBaht: number; wageBaseBaht: number; pays: number;
  priorTaxableBaht?: number; priorWithheldBaht?: number; final?: boolean;
}) {
  const annual = (income: number) => {
    const taxable = Math.max(0, income - Math.min(income / 2, 100_000) - input.annualDeductionsBaht);
    const ceilings = [150_000, 300_000, 500_000, 750_000, 1_000_000, 2_000_000, 5_000_000, Infinity];
    const rates = [0, .05, .10, .15, .20, .25, .30, .35];
    let tax = 0, lower = 0;
    for (let index = 0; index < ceilings.length; index++) {
      const slice = Math.max(0, Math.min(taxable, ceilings[index]) - lower);
      tax += Math.round(slice * rates[index] * 100) / 100;
      lower = ceilings[index];
    }
    return Math.round(tax * 100);
  };
  const ordinaryAnnual = input.ordinaryBaht * input.pays;
  const irregularPrior = input.occasionalYtdBaht ?? 0;
  const taxMinor = input.final
    ? annual((input.priorTaxableBaht ?? 0) + input.ordinaryBaht + input.occasionalBaht) - Math.round((input.priorWithheldBaht ?? 0) * 100)
    : Math.floor(annual(ordinaryAnnual) / input.pays)
      + annual(ordinaryAnnual + irregularPrior + input.occasionalBaht) - annual(ordinaryAnnual + irregularPrior);
  const ssoMinor = input.wageBaseBaht === 0 ? 0 : Math.round(Math.min(17_500, Math.max(1_650, input.wageBaseBaht)) * .05) * 100;
  const grossMinor = Math.round((input.ordinaryBaht + input.occasionalBaht) * 100);
  return { grossMinor, taxWithheldMinor: taxMinor, employeeSsoMinor: ssoMinor, employerSsoMinor: ssoMinor, netMinor: grossMinor - taxMinor - ssoMinor };
}

/** Separate decimal worksheet for an additional occasional payment; no ordinary wage is repaid. */
export function thai2026AdjustmentReference(input:{ordinaryBasisBaht:number;deltaBaht:number;occasionalYtdBaht:number;annualDeductionsBaht:number;originalWageBaht:number;additionalWageBaht:number;originalEmployeeSsoBaht:number;originalEmployerSsoBaht:number;pays:number}){
 const base=thai2026Reference({ordinaryBaht:input.ordinaryBasisBaht,occasionalBaht:0,occasionalYtdBaht:input.occasionalYtdBaht,annualDeductionsBaht:input.annualDeductionsBaht,wageBaseBaht:input.originalWageBaht,pays:input.pays});
 const withDelta=thai2026Reference({ordinaryBaht:input.ordinaryBasisBaht,occasionalBaht:input.deltaBaht,occasionalYtdBaht:input.occasionalYtdBaht,annualDeductionsBaht:input.annualDeductionsBaht,wageBaseBaht:input.originalWageBaht+input.additionalWageBaht,pays:input.pays});
 const tax=withDelta.taxWithheldMinor-base.taxWithheldMinor,gross=Math.round(input.deltaBaht*100),employee=withDelta.employeeSsoMinor-Math.round(input.originalEmployeeSsoBaht*100),employer=withDelta.employerSsoMinor-Math.round(input.originalEmployerSsoBaht*100);
 return {grossMinor:gross,taxWithheldMinor:tax,employeeSsoMinor:employee,employerSsoMinor:employer,netMinor:gross-tax-employee};
}
