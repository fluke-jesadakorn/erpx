import {beforeAll,expect,it,vi} from "vitest";
import {applyMigrations,env} from "./runtime-env";
import {expenseDb as db,expenseFixture,key} from "./helpers/expense-fixture";
import {createInvoice,createParty,invoiceSchema} from "../src/server/finance";
import {readWorkSnapshot} from "../src/server/work-items";
vi.mock("server-only",()=>({}));
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>({})}));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
it("keeps authorized Finance work when a page actor cannot access other source apps",async()=>{const f=await expenseFixture(),party=await createParty(f.reviewer,{kind:"supplier",code:key(),name:"Queue source supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key()),invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party.id,kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());const snapshot=await readWorkSnapshot({...f.reviewer,pageRequest:true});expect(snapshot.unavailableSources).not.toContain("Commercial and financial workflows");expect(snapshot.items.some(item=>item.type==="invoice"&&item.id===invoice.id)).toBe(true);expect(snapshot.items.filter(item=>item.type==="order")).toEqual([]);});

it("keeps batched workflow candidates identical to individual reads for every role and scope", async () => {
  const { workflowStaffRoleUsers } = await import("../src/server/modules/identity_access/public/workflow-role-facts");
  const { withPolicyReadSnapshot } = await import("../src/server/iam-read-snapshot");
  const f = await expenseFixture();
  const scopes = [
    { company_id: f.company, branch_id: null, department_id: null },
    { company_id: f.company, branch_id: f.branch, department_id: null },
    { company_id: f.company, branch_id: f.branch, department_id: f.department },
    { company_id: f.company, branch_id: "missing", department_id: null },
  ];
  const roles = [null, ...(await db.prepare("SELECT substring(id FROM 10) role FROM iam_permissions WHERE id LIKE 'workflow.%' ORDER BY id").all<{role:string}>()).results.map(row => row.role), "unknown"];
  for (const scope of scopes) {
    const reference = await Promise.all(roles.map(role => workflowStaffRoleUsers(db,scope,role)));
    const actual = await withPolicyReadSnapshot(() => Promise.all(roles.map(role => workflowStaffRoleUsers(db,scope,role))));
    expect(actual).toEqual(reference);
  }
});

it("keeps all commercial work and approval ownership identical to the authorized detail reader", async () => {
  const { otherFlowWork } = await import("../src/server/flow-work");
  const { withPolicyReadSnapshot } = await import("../src/server/iam-read-snapshot");
  const f = await expenseFixture();
  const party = await createParty(f.reviewer,{kind:"supplier",code:key(),name:"Batch work supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key());
  for (let index=0;index<8;index++) await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party.id,kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());
  const roleKeys = (await db.prepare("SELECT id FROM iam_roles WHERE company_id='*' AND status='active' ORDER BY id").all<{id:string}>()).results;
  const actors = [f.owner,f.reviewer,f.claimant,f.head,f.manager,f.payer,f.it];
  for (const {id} of roleKeys) actors.push(await f.member(`workflow-${id}`,id));
  for (const original of actors) for (const scope of [
    {branchId:null,departmentId:null},
    {branchId:f.branch,departmentId:null},
    {branchId:f.branch,departmentId:f.department},
  ]) {
    const common = {holdingId:original.holdingId,holdingSlug:original.holdingSlug,holdingName:original.holdingName,companyId:original.companyId};
    const resolved = scope.departmentId
      ? {...common,kind:"department" as const,branchId:scope.branchId,branchSlug:"branch",branchName:"Branch",departmentId:scope.departmentId,departmentSlug:"team",departmentName:"Team"}
      : scope.branchId ? {...common,kind:"branch" as const,branchId:scope.branchId,branchSlug:"branch",branchName:"Branch"}
        : {...common,kind:"holding" as const};
    const actor = {...original,...scope,scope:resolved,inherited:!!scope.branchId};
    const expected = await withPolicyReadSnapshot(() => otherFlowWork(actor,true));
    const actual = await withPolicyReadSnapshot(() => otherFlowWork(actor));
    expect(actual).toEqual(expected);
  }
});
