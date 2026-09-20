import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "./runtime-env";
import { expenseDb as db,expenseFixture,key } from "./helpers/expense-fixture";
import { calculatePayrollRun,createPayrollRun,exportPayrollReview,getPayrollReviewFile,payrollRunSchema } from "../src/server/people-payroll";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});

it("snapshots configured proration and overtime and exports an immutable review revision",async()=>{
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  const engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>(),group=`paygroup-${key()}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Core payroll','TH','THB','monthly','Asia/Bangkok',20000,'calendar_days',?,1,1)").bind(group,f.company,f.owner.userId).run();
  await db.prepare("UPDATE hr_engagements SET pay_group_id=?,start_date='2026-09-16',version=version+1 WHERE id=?").bind(group,engagement!.id).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-09-16','monthly',300000,0,160,'THB','[]','approved',?,? ,1,1)").bind(`comp-${key()}`,engagement!.id,f.manager.userId,f.reviewer.userId).run();
  await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','calculate',?,?,'Core payroll fixture',1)").bind(f.company,f.owner.userId,f.owner.userId).run();
  const created=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-30",payDate:"2026-09-30",currency:"THB"}),key()),calculated=await calculatePayrollRun(f.owner,String(created.id),key());
  expect(calculated.totals).toMatchObject({gross:150000,net:150000,headcount:1});
  const snapshot=JSON.parse(String(await db.prepare("SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=?").bind(created.id).first("input_snapshot_json")));
  expect(snapshot.policy).toMatchObject({overtimeMultiplierBps:20000,prorationMethod:"calendar_days",activeDays:15,periodDays:30});
  const command=key(),exported=await exportPayrollReview(f.owner,String(created.id),command);expect(await exportPayrollReview(f.owner,String(created.id),command)).toEqual(exported);
  const file=await getPayrollReviewFile(f.owner,String(created.id),Number(exported.revision)),text=await file.object.text();expect(text).toContain("employee,worker_type");expect(text).toContain("1500.00");
  expect(file.checksum).toBe(exported.checksum);
});
