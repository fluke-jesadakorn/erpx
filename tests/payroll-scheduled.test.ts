import { afterEach,beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "./runtime-env";
import { expenseDb as db,expenseFixture,key } from "./helpers/expense-fixture";
import { drainPayrollSchedules,drainPayrollScheduledPreviews,submitPayrollReview } from "../src/server/payroll-readiness";
import { approvePayrollRun } from "../src/server/people-payroll";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let fixture:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{fixture=await expenseFixture();});
afterEach(()=>vi.useRealTimers());

it("creates one scheduled monthly draft and calculates only after the period closes",async()=>{
  const group=`scheduled-group-${key()}`;
  const employee=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(fixture.company,fixture.claimant.userId).first<{id:string}>();
  await db.batch([
    db.prepare(`INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,
      overtime_multiplier_bps,proration_method,schedule_enabled,pay_delay_days,created_by,created_at,updated_at)
      VALUES(?,?,'Scheduled monthly','TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',TRUE,2,?,1,1)`)
      .bind(group,fixture.company,fixture.owner.userId),
    db.prepare("UPDATE hr_engagements SET pay_group_id=?,start_date='2026-09-01',version=version+1 WHERE id=?").bind(group,employee!.id),
    db.prepare(`INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,
      hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at)
      VALUES(?,?,'2026-09-01','monthly',100000,0,160,'THB','[]','approved',?,?,1,1)`)
      .bind(`scheduled-comp-${key()}`,employee!.id,fixture.manager.userId,fixture.reviewer.userId),
  ]);
  expect(await drainPayrollSchedules(db,new Date("2026-09-01T02:00:00Z"))).toBe(1);
  expect(await drainPayrollSchedules(db,new Date("2026-09-01T02:00:00Z"))).toBe(0);
  const draft=await db.prepare("SELECT id,status,period_start,period_end,pay_date,created_by FROM hr_payroll_runs WHERE pay_group_id=?").bind(group).first<{id:string;status:string;period_start:string;period_end:string;pay_date:string;created_by:string}>();
  expect(draft).toMatchObject({status:"draft",period_start:"2026-09-01",period_end:"2026-09-30",pay_date:"2026-10-02",created_by:"system:payroll-schedule"});
  expect(await drainPayrollScheduledPreviews(db,new Date("2026-09-30T15:00:00Z"))).toBe(0);
  expect(await drainPayrollScheduledPreviews(db,new Date("2026-10-01T02:00:00Z"))).toBe(1);
  const preview=await db.prepare("SELECT status,input_digest,submitted_by,approved_by FROM hr_payroll_runs WHERE id=?").bind(draft!.id).first<{status:string;input_digest:string;submitted_by:string|null;approved_by:string|null}>();
  expect(preview?.status).toBe("preview");
  expect(preview?.input_digest).toMatch(/^[a-f0-9]{64}$/);
  expect(preview?.submitted_by).toBeNull();
  expect(preview?.approved_by).toBeNull();
  expect(await drainPayrollScheduledPreviews(db,new Date("2026-10-01T02:00:00Z"))).toBe(0);
  const items=await db.prepare("SELECT COUNT(*) AS count FROM hr_payroll_items WHERE run_id=?").bind(draft!.id).first<{count:number}>();
  expect(Number(items?.count)).toBe(1);

  fixture.owner.role="payroll_preparer";
  fixture.owner.assignmentId=await fixture.grant(fixture.owner,"payroll_preparer");
  fixture.manager.role="payroll_approver";
  fixture.manager.assignmentId=await fixture.grant(fixture.manager,"payroll_approver");
  const samePersonApprover={...fixture.owner,role:"payroll_approver",assignmentId:await fixture.grant(fixture.owner,"payroll_approver")};
  await db.batch([
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','preview',?,?,'Scheduled payroll preparer',1)").bind(fixture.company,fixture.owner.userId,fixture.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','in_review',?,?,'Independent payroll reviewer',1)").bind(fixture.company,fixture.manager.userId,fixture.owner.userId),
  ]);
  vi.useFakeTimers({toFake:["Date"]});
  vi.setSystemTime(new Date("2026-10-02T02:00:00Z"));
  const detail=await db.prepare("SELECT version,input_digest FROM hr_payroll_runs WHERE id=?").bind(draft!.id).first<{version:number;input_digest:string}>();
  const reviewKey=key(),input={expectedVersion:detail!.version,inputDigest:detail!.input_digest};
  const submitted=await submitPayrollReview(fixture.owner,draft!.id,input,reviewKey);
  expect(submitted).toMatchObject({status:"in_review",submittedBy:fixture.owner.userId});
  expect(await submitPayrollReview(fixture.owner,draft!.id,input,reviewKey)).toEqual(submitted);
  await expect(approvePayrollRun(samePersonApprover,draft!.id,submitted.version,key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
  await db.prepare("UPDATE hr_compensation_revisions SET base_amount_minor=base_amount_minor+1,version=version+1 WHERE engagement_id=?").bind(employee!.id).run();
  await expect(approvePayrollRun(fixture.manager,draft!.id,submitted.version,key())).rejects.toMatchObject({code:"PAYROLL_SOURCE_CHANGED"});
});
