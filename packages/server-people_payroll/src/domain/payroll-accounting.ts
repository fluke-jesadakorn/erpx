import {BusinessRuleError} from "@erpx/server-shared/index";
import type { JournalLineInput } from "@erpx/server-finance/public/finance";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";

export type PayrollAccounts = {
  expenseAccountId: string;
  netPayableAccountId: string;
  taxPayableAccountId?: string;
  employerContributionAccountId?: string;
  employerExpenseAccountId?: string;
  socialSecurityPayableAccountId?: string;
  advanceReceivableAccountId?: string;
  benefitPayableAccountId?: string;
  otherDeductionPayableAccountId?: string;
  serviceChargeExpenseAccountId?: string;
  overtimeExpenseAccountId?: string;
  bonusExpenseAccountId?: string;
};

export type PayrollPostingItem = {
  id: string;
  gross_minor: number;
  deductions_minor: number;
  employer_cost_minor: number;
  net_minor: number;
  input_snapshot_json: string;
  result_json: string;
};

type Component = { code: string; kind: "earning" | "deduction" | "employer"; amountMinor: number };
type Assignment = {id:string;effective_from:string;effective_to:string|null;branch_id:string|null;department_id:string|null;evidence_status:string};
type TimeEntry = {work_date:string;regular_hours:number;overtime_hours:number;cost_assignment_id:string|null;cost_branch_id:string|null;cost_department_id:string|null};
type Handoff = {work_date:string;minutes:number};
type CostSnapshot = {engagement?:{employment_type?:string;start_date?:string;end_date?:string|null};period?:{start:string;end:string};policy?:{overtimeMultiplierBps?:number};hours?:{entries?:TimeEntry[];handoffs?:Handoff[]};costAssignments?:Assignment[]};
type CostUnit = {branchId:string|null;departmentId:string|null;days:bigint;regular:bigint;overtime:bigint};

function allocateMinor(amount:number,weights:bigint[]):number[]{
  if(!Number.isSafeInteger(amount)||amount<0)throw new BusinessRuleError(409,"PAYROLL_COST_ALLOCATION_INVALID","Cost allocation amount is invalid.");
  if(!amount)return weights.map(()=>0);
  const total=weights.reduce((sum,weight)=>sum+weight,BigInt(0));
  if(total<=BigInt(0))throw new BusinessRuleError(409,"PAYROLL_COST_ALLOCATION_INCOMPLETE","Approved dated work or placement evidence is missing for a payroll amount.");
  const parts=weights.map((weight,index)=>({index,base:Number(BigInt(amount)*weight/total),remainder:BigInt(amount)*weight%total}));
  let remaining=amount-parts.reduce((sum,part)=>sum+part.base,0);
  for(const part of [...parts].sort((a,b)=>a.remainder===b.remainder?a.index-b.index:a.remainder>b.remainder?-1:1)){if(remaining<=0)break;part.base++;remaining--;}
  return parts.map(part=>part.base);
}

function costUnits(snapshot:CostSnapshot):CostUnit[]{
  const period=snapshot.period,engagement=snapshot.engagement,assignments=snapshot.costAssignments;
  if(!period||!engagement?.start_date||!Array.isArray(assignments)||!assignments.length)throw new BusinessRuleError(409,"PAYROLL_COST_ALLOCATION_INCOMPLETE","Dated cost assignments are missing from the approved payroll snapshot.");
  const start=engagement.start_date>period.start?engagement.start_date:period.start,end=engagement.end_date&&engagement.end_date<period.end?engagement.end_date:period.end;
  const units=new Map<string,CostUnit>();
  const at=(date:string)=>{const rows=assignments.filter(row=>row.effective_from<=date&&(row.effective_to===null||row.effective_to>=date));if(rows.length!==1||rows[0].evidence_status!=="verified")throw new BusinessRuleError(409,"PAYROLL_COST_ALLOCATION_INCOMPLETE","Dated branch and department evidence is incomplete or ambiguous.");return rows[0];};
  const unitFor=(assignment:Assignment)=>{const key=JSON.stringify([assignment.branch_id,assignment.department_id]);let unit=units.get(key);if(!unit){unit={branchId:assignment.branch_id,departmentId:assignment.department_id,days:BigInt(0),regular:BigInt(0),overtime:BigInt(0)};units.set(key,unit);}return unit;};
  for(let day=Date.parse(start);day<=Date.parse(end);day+=86400000)unitFor(at(new Date(day).toISOString().slice(0,10))).days++;
  for(const entry of snapshot.hours?.entries??[]){const assignment=at(entry.work_date);if(entry.cost_assignment_id!==assignment.id||entry.cost_branch_id!==assignment.branch_id||entry.cost_department_id!==assignment.department_id)throw new BusinessRuleError(409,"PAYROLL_TIME_COST_EVIDENCE_MISSING","Approved time does not match its dated cost assignment.");const unit=unitFor(assignment);unit.regular+=BigInt(Math.round(Number(entry.regular_hours)*1_000_000));unit.overtime+=BigInt(Math.round(Number(entry.overtime_hours)*1_000_000));}
  for(const handoff of snapshot.hours?.handoffs??[])unitFor(at(handoff.work_date)).overtime+=BigInt(Math.round(Number(handoff.minutes)*1_000_000/60));
  return [...units.values()].sort((a,b)=>JSON.stringify([a.branchId,a.departmentId]).localeCompare(JSON.stringify([b.branchId,b.departmentId])));
}

function allocatedAmounts(item:PayrollPostingItem,snapshot:CostSnapshot,components:Component[]){
  const units=costUnits(snapshot),explanation=(JSON.parse(item.result_json) as {explanation?:{hourlyRateMinor?:number;overtimeHours?:number;overtimeMinor?:number;regularHours?:number;overtimeMultiplierBps?:number}}).explanation;
  const earn=components.filter(component=>component.kind==="earning"),baseGross=Number(item.gross_minor)-earn.reduce((sum,component)=>sum+component.amountMinor,0);
  const multiplier=Number(explanation?.overtimeMultiplierBps??snapshot.policy?.overtimeMultiplierBps??15000);
  const overtime=explanation?.overtimeMinor??Math.round(Number(explanation?.hourlyRateMinor??0)*Number(explanation?.overtimeHours??0)*multiplier/10000);
  const regular=baseGross-overtime;
  const overtimeRemainder=baseGross-regular;
  if(regular<0||overtimeRemainder<0)throw new BusinessRuleError(409,"PAYROLL_COST_ALLOCATION_INVALID","Wage and overtime amounts do not reconcile.");
  const timeWeights=units.map(unit=>unit.regular*BigInt(10000)+unit.overtime*BigInt(multiplier));
  const generalWeights=engagementIsSalary(snapshot)?units.map(unit=>unit.days):timeWeights;
  const regularWeights=engagementIsSalary(snapshot)?units.map(unit=>unit.days):units.map(unit=>unit.regular);
  const overtimeWeights=units.map(unit=>unit.overtime);
  const wages=allocateMinor(regular,regularWeights),overtimeAmounts=allocateMinor(overtimeRemainder,overtimeWeights);
  const earnings=earn.map(component=>allocateMinor(component.amountMinor,category(component.code,"earning")==="overtime"?overtimeWeights:generalWeights));
  const gross=units.map((_,index)=>wages[index]+overtimeAmounts[index]+earnings.reduce((sum,parts)=>sum+parts[index],0));
  const deductions=components.filter(component=>component.kind==="deduction").map(component=>allocateMinor(component.amountMinor,gross.map(BigInt)));
  const employers=components.filter(component=>component.kind==="employer").map(component=>allocateMinor(component.amountMinor,gross.map(BigInt)));
  const net=units.map((_,index)=>gross[index]-deductions.reduce((sum,parts)=>sum+parts[index],0));
  if(net.some(amount=>amount<0)||net.reduce((sum,amount)=>sum+amount,0)!==Number(item.net_minor))throw new BusinessRuleError(409,"PAYROLL_COST_ALLOCATION_INVALID","Cost-unit payable amounts do not reconcile.");
  return {units,wages,overtimeAmounts,earnings,deductions,employers,net};
}

function engagementIsSalary(snapshot:CostSnapshot){return snapshot.engagement?.employment_type==="salaried";}

export function payrollNetCostShares(item:PayrollPostingItem):Array<{amountMinor:number;dimensions:Record<string,string>}>{
  const snapshot=JSON.parse(item.input_snapshot_json) as CostSnapshot;
  const components=(JSON.parse(item.result_json) as {explanation?:{components?:Component[]}}).explanation?.components;
  if(!Array.isArray(components))throw new BusinessRuleError(409,"PAYROLL_COMPONENTS_MISSING","Recalculate payroll to retain item components before settlement.");
  const allocation=allocatedAmounts(item,snapshot,components);
  return allocation.units.flatMap((unit,index)=>{
    const amountMinor=allocation.net[index];if(!amountMinor)return [];
    const dimensions:Record<string,string>={};
    if(unit.branchId)dimensions.branchId=unit.branchId;
    if(unit.departmentId)dimensions.departmentId=unit.departmentId;
    return [{amountMinor,dimensions}];
  });
}

function account(id: string | undefined, code: string): string {
  if (!id) throw new BusinessRuleError(422, "PAYROLL_ACCOUNT_MAPPING_REQUIRED", `Map a ledger account for ${code} before posting payroll.`);
  return id;
}

function category(code: string, kind: Component["kind"]): string {
  const normalized = code.toUpperCase().replaceAll(/[^A-Z0-9]+/g, "_");
  if (kind === "earning") {
    if (/SERVICE_CHARGE/.test(normalized)) return "service_charge";
    if (/BONUS|INCENTIVE/.test(normalized)) return "bonus";
    if (/OVERTIME|(^|_)OT(_|$)/.test(normalized)) return "overtime";
    return "wages";
  }
  if (/^PAYROLL_ADVANCE_/.test(normalized)) return "advance";
  if (/^BENEFIT_/.test(normalized)) return "benefit";
  if (/SOCIAL|SSO/.test(normalized)) return "social_security";
  if (/TAX|WITHHOLD|(^|_)PIT(_|$)|PND1/.test(normalized)) return "tax";
  if(code.toLowerCase().startsWith("thai:"))throw new BusinessRuleError(422,"PAYROLL_COMPONENT_UNMAPPED",`No accounting classification exists for statutory component ${code}.`);
  return kind === "employer" ? "employer_other" : "deduction_other";
}

export function payrollAccountingLines(items: PayrollPostingItem[], accounts: PayrollAccounts): {
  lines: JournalLineInput[];
  totals: { grossMinor: number; deductionsMinor: number; employerMinor: number; netMinor: number; debitMinor: number; creditMinor: number };
  categories: Record<string, number>;
} {
  const grouped = new Map<string, { accountId: string; description: string; debit: number; credit: number; dimensions: Record<string, string> }>();
  const categories: Record<string, number> = {};
  const totals = { grossMinor: 0, deductionsMinor: 0, employerMinor: 0, netMinor: 0, debitMinor: 0, creditMinor: 0 };
  function add(accountId: string, description: string, debit: number, credit: number, dimensions: Record<string, string> = {}) {
    if (!debit && !credit) return;
    const key = JSON.stringify([accountId, description, dimensions]);
    const current = grouped.get(key) ?? { accountId, description, debit: 0, credit: 0, dimensions };
    current.debit += debit;
    current.credit += credit;
    grouped.set(key, current);
    totals.debitMinor += debit;
    totals.creditMinor += credit;
  }
  for (const item of items) {
    const snapshot = JSON.parse(item.input_snapshot_json) as CostSnapshot;
    const result = JSON.parse(item.result_json) as { explanation?: { components?: Component[] } };
    const components = result.explanation?.components;
    if (!Array.isArray(components)) throw new BusinessRuleError(409, "PAYROLL_COMPONENTS_MISSING", "Recalculate payroll to retain the item components before posting.");
    const earnings = components.filter(c => c.kind === "earning");
    const deductions = components.filter(c => c.kind === "deduction");
    const employer = components.filter(c => c.kind === "employer");
    if (components.some(c => !Number.isSafeInteger(c.amountMinor) || c.amountMinor < 0) ||
      deductions.reduce((n,c) => n+c.amountMinor,0) !== Number(item.deductions_minor) ||
      employer.reduce((n,c) => n+c.amountMinor,0) !== Number(item.employer_cost_minor))
      throw new BusinessRuleError(409, "PAYROLL_COMPONENTS_INVALID", "Payroll item components do not agree with the approved totals.");
    const wage = Number(item.gross_minor) - earnings.reduce((n,c) => n+c.amountMinor,0);
    if (wage < 0 || Number(item.net_minor) !== Number(item.gross_minor)-Number(item.deductions_minor))
      throw new BusinessRuleError(409, "PAYROLL_TOTALS_INVALID", "Payroll item totals do not balance.");
    const allocation=allocatedAmounts(item,snapshot,components);
    for(const [unitIndex,unit] of allocation.units.entries()){
    const dimensions:Record<string,string>={};if(unit.branchId)dimensions.branchId=unit.branchId;if(unit.departmentId)dimensions.departmentId=unit.departmentId;
    add(accounts.expenseAccountId, "Payroll wages", allocation.wages[unitIndex], 0, dimensions);
    add(accounts.overtimeExpenseAccountId??accounts.expenseAccountId,"Payroll overtime",allocation.overtimeAmounts[unitIndex],0,dimensions);
    for (const [componentIndex,component] of earnings.entries()) {
      const kind = category(component.code, "earning");
      const ledger = kind === "service_charge" ? account(accounts.serviceChargeExpenseAccountId,kind) : kind === "bonus" ? accounts.bonusExpenseAccountId ?? accounts.expenseAccountId : kind === "overtime" ? accounts.overtimeExpenseAccountId ?? accounts.expenseAccountId : accounts.expenseAccountId;
      add(ledger, `Payroll ${kind.replaceAll("_", " ")}`, allocation.earnings[componentIndex][unitIndex], 0, dimensions);
    }
    for (const [componentIndex,component] of deductions.entries()) {
      const kind = category(component.code, "deduction");
      const ledger = kind === "tax" ? account(accounts.taxPayableAccountId,kind) : kind === "social_security" ? account(accounts.socialSecurityPayableAccountId ?? accounts.employerContributionAccountId,kind) : kind === "advance" ? account(accounts.advanceReceivableAccountId,kind) : kind === "benefit" ? account(accounts.benefitPayableAccountId,kind) : account(accounts.otherDeductionPayableAccountId,kind);
      add(ledger, `Payroll ${kind.replaceAll("_", " ")}`, 0, allocation.deductions[componentIndex][unitIndex], dimensions);
    }
    for (const [componentIndex,component] of employer.entries()) {
      const kind = category(component.code, "employer");
      const ledger = kind === "social_security" ? account(accounts.socialSecurityPayableAccountId ?? accounts.employerContributionAccountId,kind) : kind === "benefit" ? account(accounts.benefitPayableAccountId,kind) : account(accounts.employerContributionAccountId,kind);
      const employerExpense=account(accounts.employerExpenseAccountId,`employer_${kind}_expense`);
      if(employerExpense===accounts.expenseAccountId)throw new BusinessRuleError(422,"PAYROLL_ACCOUNT_MAPPING_REQUIRED","Map employer costs to an expense account separate from wages.");
      add(employerExpense, `Payroll employer ${kind.replaceAll("_", " ")} expense`, allocation.employers[componentIndex][unitIndex], 0, dimensions);
      add(ledger, `Payroll ${kind.replaceAll("_", " ")}`, 0, allocation.employers[componentIndex][unitIndex], dimensions);
    }
    add(accounts.netPayableAccountId, "Net payroll payable", 0, allocation.net[unitIndex], dimensions);
    }
    for(const component of earnings){const kind=category(component.code,"earning");categories[kind]=(categories[kind]??0)+component.amountMinor;}
    for(const component of deductions){const kind=category(component.code,"deduction");categories[kind]=(categories[kind]??0)+component.amountMinor;}
    for(const component of employer){const kind=category(component.code,"employer");categories[`employer_${kind}`]=(categories[`employer_${kind}`]??0)+component.amountMinor;}
    totals.grossMinor += Number(item.gross_minor);
    totals.deductionsMinor += Number(item.deductions_minor);
    totals.employerMinor += Number(item.employer_cost_minor);
    totals.netMinor += Number(item.net_minor);
  }
  if (!items.length || totals.netMinor <= 0 || totals.debitMinor !== totals.creditMinor || !Number.isSafeInteger(totals.debitMinor))
    throw new BusinessRuleError(409, "PAYROLL_TOTALS_INVALID", "Payroll accrual debits and credits must balance.");
  const lines = [...grouped.values()].map(line => ({ accountId: line.accountId, description: line.description, debit: minorToDecimal(line.debit), credit: minorToDecimal(line.credit), dimensions: line.dimensions }));
  if (lines.length > 500) throw new BusinessRuleError(422, "PAYROLL_JOURNAL_TOO_LARGE", "Split this run before posting more than 500 grouped ledger lines.");
  return { lines, totals, categories };
}
