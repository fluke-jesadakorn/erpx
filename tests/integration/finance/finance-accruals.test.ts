import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { accrualScheduleSchema, accrualSchedulePage, createAccrualSchedule, decideAccrualSchedule, postAccrualRun, prepareAccrualRun, accrualRunPostSchema } from "../../../src/server/modules/finance/public/finance-accruals";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});

it("requires independent approval and posts each recurring accrual period once",async()=>{
  f.owner.role="finance_preparer";f.owner.assignmentId=await f.grant(f.owner,"finance_preparer");
  f.manager.role="finance_approver";f.manager.assignmentId=await f.grant(f.manager,"finance_approver");
  const selfApprover={...f.owner,role:"finance_approver",assignmentId:await f.grant(f.owner,"finance_approver")};
  const input=accrualScheduleSchema.parse({description:"Annual insurance expense",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable,currency:"THB",total:"300.00",startDate:"2026-09-01",endDate:"2026-11-30",frequency:"monthly"});
  const created=await createAccrualSchedule(f.owner,input,key());expect(created).toMatchObject({status:"draft",periodCount:3,total:"300.00"});
  expect((await accrualSchedulePage(f.owner)).schedules).toHaveLength(1);
  await expect(decideAccrualSchedule(selfApprover,String(created.id),{expectedVersion:1,reason:"I cannot approve my own schedule"},"approve",key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
  const approved=await decideAccrualSchedule(f.manager,String(created.id),{expectedVersion:1,reason:"Review accounts and supporting contract"},"approve",key());expect(approved).toMatchObject({status:"active",version:2});
  const prepared=await prepareAccrualRun(f.owner,String(created.id),key());expect(prepared).toMatchObject({status:"prepared",amountMinor:10000,periodStart:"2026-09-01",periodEnd:"2026-09-30"});
  await expect(prepareAccrualRun(f.owner,String(created.id),key())).rejects.toMatchObject({code:"ACCRUAL_PERIOD_ALREADY_PREPARED"});
  const posted=await postAccrualRun(f.manager,String(prepared.id),accrualRunPostSchema.parse({expectedVersion:1,reason:"Post reviewed September accrual"}),key());expect(posted).toMatchObject({status:"posted",version:2});
  const journal=await db.prepare("SELECT j.status,j.source_type,j.period_id,SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.id=? GROUP BY j.id").bind(prepared.journalId).first();expect(journal).toMatchObject({status:"posted",source_type:"accrual_schedule",debit:10000,credit:10000});
  const schedule=await db.prepare("SELECT status,next_run_date,version FROM finance_accrual_schedules WHERE id=?").bind(created.id).first();expect(schedule).toMatchObject({status:"active",next_run_date:"2026-10-01",version:3});
  const next=await prepareAccrualRun(f.owner,String(created.id),key());expect(next).toMatchObject({amountMinor:10000,periodStart:"2026-10-01"});
  await expect(db.prepare("DELETE FROM finance_accrual_schedule_events WHERE schedule_id=?").bind(created.id).run()).rejects.toThrow();
});
