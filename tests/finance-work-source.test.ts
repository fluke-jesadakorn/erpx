import {beforeAll,expect,it,vi} from "vitest";
import {applyMigrations,env} from "./runtime-env";
import {expenseDb as db,expenseFixture,key} from "./helpers/expense-fixture";
import {createInvoice,createParty,invoiceSchema} from "../src/server/finance";
import {readWorkSnapshot} from "../src/server/work-items";
vi.mock("server-only",()=>({}));
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>({})}));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
it("keeps authorized Finance work when a page actor cannot access other source apps",async()=>{const f=await expenseFixture(),party=await createParty(f.reviewer,{kind:"supplier",code:key(),name:"Queue source supplier",currency:"THB",paymentTermsDays:30,creditLimit:"0"},key()),invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:party.id,kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-10",dueDate:"2026-10-10",currency:"THB",subtotal:"100",tax:"0",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());const snapshot=await readWorkSnapshot({...f.reviewer,pageRequest:true});expect(snapshot.unavailableSources).not.toContain("Commercial and financial workflows");expect(snapshot.items.some(item=>item.type==="invoice"&&item.id===invoice.id)).toBe(true);expect(snapshot.items.filter(item=>item.type==="order")).toEqual([]);});
