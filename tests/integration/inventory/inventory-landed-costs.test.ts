import {afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {createLandedCost,getLandedCost,landedCostWork,transitionLandedCost} from "@erpx/server-inventory/public/inventory-landed-costs";

vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
afterEach(()=>vi.unstubAllEnvs());
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();f.head.role='purchasing_manager';f.head.branchId=null;f.head.departmentId=null;f.head.assignmentId=await f.grant(f.head,'purchasing_manager');f.reviewer.role='finance_preparer';f.reviewer.branchId=null;f.reviewer.departmentId=null;f.reviewer.assignmentId=await f.grant(f.reviewer,'finance_preparer');f.manager.role='finance_approver';f.manager.branchId=null;f.manager.departmentId=null;f.manager.assignmentId=await f.grant(f.manager,'finance_approver');});

// Landed-cost create, approve, reject, cancel and post are native
// (`packages/server-inventory/src/public/landed_costs.rs` + composition). The
// actual stock/lot valuation and immutable evidence oracles live in
// `rust-stock-register-postgres.test.rs::transfer_landed_reorder_pg`; the
// attachment/package composition assertions remain with their own owners.
it('retires the TypeScript landed-cost writers in favour of the native command',async()=>{
  const input={fulfillmentId:'receipt',costDate:'2026-09-03',description:'Ocean freight and import handling invoice',allocationMethod:'quantity' as const,amount:'3',creditAccountId:f.accounts.expense};
  await expect(createLandedCost(f.reviewer,input,key())).rejects.toMatchObject({code:'NATIVE_INVENTORY_LANDED_COST_REQUIRED'});
  await expect(transitionLandedCost(f.manager,'landed-cost','approve',{expectedVersion:1,reason:'Native independent review'},key())).rejects.toMatchObject({code:'NATIVE_INVENTORY_LANDED_COST_REQUIRED'});
  for(const table of ['inventory_landed_costs','inventory_landed_cost_lines','inventory_landed_cost_lots','inventory_landed_cost_events','inventory_landed_cost_moves'])expect(await db.prepare(`SELECT count(*) n FROM ${table}`).first('n')).toBe(0);
});

it('keeps current landed-cost reads and Work projection read-only',async()=>{
  await expect(getLandedCost(f.reviewer,'missing-landed-cost')).rejects.toMatchObject({code:'LANDED_COST_NOT_FOUND'});
  expect(await landedCostWork(f.manager)).toEqual([]);
  expect(await db.prepare('SELECT count(*) n FROM inventory_landed_costs').first('n')).toBe(0);
});
