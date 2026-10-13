import { expect,it } from "vitest";
import { payrollAccountingLines, payrollNetCostShares } from "../../../src/server/modules/people_payroll/public/payroll-accounting";

const item={
  id:"item-1",gross_minor:120000,deductions_minor:14000,employer_cost_minor:5000,net_minor:106000,
  input_snapshot_json:JSON.stringify({engagement:{employment_type:"salaried",start_date:"2026-09-01",end_date:null},period:{start:"2026-09-01",end:"2026-09-30"},hours:{entries:[],handoffs:[]},costAssignments:[{id:"assignment-1",effective_from:"2026-09-01",effective_to:null,branch_id:"branch-1",department_id:"department-1",evidence_status:"verified"}]}),
  result_json:JSON.stringify({explanation:{hourlyRateMinor:0,regularHours:0,overtimeHours:0,components:[
    {code:"BONUS",kind:"earning",amountMinor:20000},
    {code:"thai:withholding",kind:"deduction",amountMinor:7000},
    {code:"thai:sso-employee",kind:"deduction",amountMinor:5000},
    {code:"payroll-advance:repayment-1",kind:"deduction",amountMinor:2000},
    {code:"thai:sso-employer",kind:"employer",amountMinor:5000},
  ]}}),
};
const accounts={expenseAccountId:"wages",bonusExpenseAccountId:"bonus",employerExpenseAccountId:"sso-expense",netPayableAccountId:"net",taxPayableAccountId:"tax",socialSecurityPayableAccountId:"sso",advanceReceivableAccountId:"advances"};

it("separates wage, bonus, employer cost, statutory liabilities and advance repayment by cost unit",()=>{
  const accrual=payrollAccountingLines([item],accounts);
  expect(accrual.totals).toMatchObject({grossMinor:120000,deductionsMinor:14000,employerMinor:5000,netMinor:106000,debitMinor:125000,creditMinor:125000});
  expect(accrual.lines.map(line=>[line.accountId,line.debit,line.credit])).toEqual([
    ["wages","1000.00","0.00"],["bonus","200.00","0.00"],["tax","0.00","70.00"],
    ["sso","0.00","100.00"],["advances","0.00","20.00"],["sso-expense","50.00","0.00"],["net","0.00","1060.00"],
  ]);
  expect(accrual.lines.every(line=>line.dimensions?.branchId==="branch-1"&&line.dimensions.departmentId==="department-1")).toBe(true);
});

it("requires separate employer expense and a mapped account for unknown deductions",()=>{
  expect(()=>payrollAccountingLines([item],{...accounts,employerExpenseAccountId:undefined})).toThrow("Map a ledger account");
  const unknown={...item,result_json:JSON.stringify({explanation:{components:[{code:"UNMAPPED",kind:"deduction",amountMinor:14000},{code:"thai:sso-employer",kind:"employer",amountMinor:5000},{code:"BONUS",kind:"earning",amountMinor:20000}]}})};
  expect(()=>payrollAccountingLines([unknown],accounts)).toThrow("Map a ledger account");
});

it("allocates salary to each effective dated placement and settles the same payable dimensions",()=>{
  const split={...item,input_snapshot_json:JSON.stringify({engagement:{employment_type:"salaried",start_date:"2026-09-01",end_date:null},period:{start:"2026-09-01",end:"2026-09-30"},hours:{entries:[],handoffs:[]},costAssignments:[
    {id:"assignment-1",effective_from:"2026-09-01",effective_to:"2026-09-15",branch_id:"branch-1",department_id:"department-1",evidence_status:"verified"},
    {id:"assignment-2",effective_from:"2026-09-16",effective_to:null,branch_id:"branch-2",department_id:"department-2",evidence_status:"verified"},
  ]})};
  const accrual=payrollAccountingLines([split],accounts);
  expect(accrual.totals).toMatchObject({debitMinor:125000,creditMinor:125000});
  expect(payrollNetCostShares(split)).toEqual([
    {amountMinor:53000,dimensions:{branchId:"branch-1",departmentId:"department-1"}},
    {amountMinor:53000,dimensions:{branchId:"branch-2",departmentId:"department-2"}},
  ]);
});

it("allocates hourly pay by approved time unit and requires its dated snapshot",()=>{
  const hourly={...item,gross_minor:100000,deductions_minor:0,employer_cost_minor:0,net_minor:100000,
    input_snapshot_json:JSON.stringify({engagement:{employment_type:"hourly",start_date:"2026-09-01",end_date:null},period:{start:"2026-09-01",end:"2026-09-30"},hours:{entries:[
      {work_date:"2026-09-05",regular_hours:5,overtime_hours:0,cost_assignment_id:"assignment-1",cost_branch_id:"branch-1",cost_department_id:"department-1"},
      {work_date:"2026-09-20",regular_hours:5,overtime_hours:0,cost_assignment_id:"assignment-2",cost_branch_id:"branch-2",cost_department_id:"department-2"},
    ],handoffs:[]},costAssignments:[
      {id:"assignment-1",effective_from:"2026-09-01",effective_to:"2026-09-15",branch_id:"branch-1",department_id:"department-1",evidence_status:"verified"},
      {id:"assignment-2",effective_from:"2026-09-16",effective_to:null,branch_id:"branch-2",department_id:"department-2",evidence_status:"verified"},
    ]}),result_json:JSON.stringify({explanation:{hourlyRateMinor:10000,regularHours:10,overtimeHours:0,components:[]}})};
  expect(payrollNetCostShares(hourly)).toEqual([
    {amountMinor:50000,dimensions:{branchId:"branch-1",departmentId:"department-1"}},
    {amountMinor:50000,dimensions:{branchId:"branch-2",departmentId:"department-2"}},
  ]);
  const invalid={...hourly,input_snapshot_json:hourly.input_snapshot_json.replace('"cost_assignment_id":"assignment-1"','"cost_assignment_id":null')};
  expect(()=>payrollAccountingLines([invalid],accounts)).toThrow("Approved time does not match");
});
