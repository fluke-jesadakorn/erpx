import {accrualRecordDetail} from "../../../src/server/modules/finance/public/finance-accruals";
import {afterEach} from "vitest";
import {reviewedFinanceGrant} from "../../support/helpers/finance-purchase-fixture";
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

afterEach(()=>vi.unstubAllEnvs());

function purchaseSchedule(){return accrualScheduleSchema.parse({description:"Purchased retained insurance",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable,currency:"THB",total:"300",startDate:"2026-09-01",endDate:"2026-09-30",frequency:"monthly"});}
it("unpurchased accrual reads and commands deny before effects and draining Finance completes the exact retained period",async()=>{
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const command=key(),input=purchaseSchedule();await expect(createAccrualSchedule(f.reviewer,input,command)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(accrualSchedulePage(f.owner)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(accrualRecordDetail(f.owner,"schedule","absent")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await reviewedFinanceGrant(db,f.company,"finance");const created=await createAccrualSchedule(f.reviewer,input,command);await reviewedFinanceGrant(db,f.company,"finance","draining");expect(await createAccrualSchedule(f.reviewer,input,command)).toEqual(created);await expect(createAccrualSchedule(f.reviewer,input,key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});
 await decideAccrualSchedule(f.manager,String(created.id),{expectedVersion:1,reason:"Reviewed existing insurance source"},"approve",key());const prepared=await prepareAccrualRun(f.reviewer,String(created.id),key()),decision={expectedVersion:1,reason:"Reviewed retained insurance month"},postKey=key(),posted=await postAccrualRun(f.manager,String(prepared.id),decision,postKey);expect(await postAccrualRun(f.manager,String(prepared.id),decision,postKey)).toEqual(posted);expect((await accrualRecordDetail(f.owner,"schedule",String(created.id))).row.status).toBe("completed");
});
it("accrual final authority is rechecked after deferred journal and outbox effects with complete rollback and one retry",async()=>{
 await reviewedFinanceGrant(db,f.company,"finance");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");const created=await createAccrualSchedule(f.reviewer,purchaseSchedule(),key());await decideAccrualSchedule(f.manager,String(created.id),{expectedVersion:1,reason:"Independent recurring source review"},"approve",key());const prepared=await prepareAccrualRun(f.reviewer,String(created.id),key()),decision={expectedVersion:1,reason:"Post reviewed one-month source"},command=key(),fault=`accrual_purchase_${key().replaceAll("-","")}`;
 const snapshot=async()=>Promise.all(["finance_accrual_schedules","finance_accrual_schedule_runs","finance_accrual_schedule_events","finance_journals","finance_journal_lines","command_receipts","audit_events","outbox_events"].map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(postAccrualRun(f.manager,String(prepared.id),decision,command)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await snapshot()).toEqual(before);}
 finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}()`);}
 const posted=await postAccrualRun(f.manager,String(prepared.id),decision,command);expect(await postAccrualRun(f.manager,String(prepared.id),decision,command)).toEqual(posted);
});
