import { afterEach,beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "../../support/runtime-env";
import { expenseDb as db,expenseFixture,key } from "../../support/helpers/expense-fixture";
import { approvePayrollRun,calculatePayrollRun,createPaymentBatch,approvePaymentBatch,preparePaymentRetry,confirmPaymentBatch,exportPaymentBatch,getPaymentFile,createPayrollRun,decidePayrollCorrection,decidePayrollAdvance,exportPayrollReview,getPayrollReviewFile,getPayrollRun,payrollRunSchema,prepareFinalPay,finalPayPreparationSchema,requestPayrollCorrection,requestPayrollAdvance,payrollAdvanceRequestSchema,payrollAdvancePage,postPayroll,payrollPostSchema,verifyPayrollCostAssignment } from "../../../src/server/modules/people_payroll/public/people-payroll";
import { submitPayrollReview } from "../../../src/server/modules/people_payroll/public/payroll-readiness";
import { getPayrollLiabilities, remitPayrollLiability, payrollRemittanceSchema } from "../../../src/server/modules/people_payroll/public/payroll-remittance";
import { listStaffNotifications } from "../../../src/server/modules/collaboration/public/staff-notifications";
import { resolveFlow,assignFlowPrimary } from "../../../src/server/modules/workflow/public/flow-routing";
import { assertOwnedCashCannotMatchAgain } from "../../support/helpers/finance-bank-ownership";
import {attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet} from "../../support/helpers/attachment-v2";
import { packageGrantStore, readCompanyPackageGrants } from "../../../src/server/modules/commercial/public/package-grants";
import { payrollSummary, listPayrollRuns, workPayrollRows, listPeople } from "../../../src/server/modules/people_payroll/public/people-payroll";
import { payrollPurchaseControls } from "../../../src/server/modules/people_payroll/public/payroll-purchases";
import {preparePayrollProjection,assertPayrollFinanceHistory} from "../../support/helpers/payroll-finance-history";
import {createAttachmentSet,attachmentContent,bindRecordAttachmentSet} from "../../../src/server/modules/files/public/attachments/service";
import {getRecord,listRecords,recordsPage,searchReferences} from "../../../src/server/modules/records/public/records";
import {payrollSourceReadScope,payrollNotificationTarget} from "../../../src/server/modules/people_payroll/public/record-purchases";
import {publishInboxEvent,inboxPage} from "../../../src/server/modules/collaboration/public/inbox";
import {peopleReferenceRows} from "../../../src/server/modules/people_payroll/public/people-purchases";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);enableAttachmentV2ForTest();});
beforeEach(async()=>{f=await expenseFixture();});
afterEach(()=>{vi.useRealTimers();vi.unstubAllEnvs();});

async function purchasePayroll(state:"active"|"draining"="active",endsAt:number|null=null) {
  const store=packageGrantStore(db,{authorize:async()=>({userId:"isolated-payroll-purchase-review"}),obligations:async()=>{throw Error("People withdrawal requires its real owner readers");}});
  const apply=async(packageId:"people"|"people-payroll")=>{
    const prior=(await readCompanyPackageGrants(db,f.company)).find(grant=>grant.packageId===packageId);
    return store.apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:"Reviewed disposable Payroll purchase",evidence:"test:people-payroll-owner"}});
  };
  await apply("people");await apply("people-payroll");
}
async function legacyPayrollGroup() {
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  const group=`payroll-purchase-${key()}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,created_by,created_at,updated_at) VALUES(?,?,'Legacy Payroll purchase cohort','TH','THB','monthly','Asia/Bangkok',?,1,1)").bind(group,f.company,f.owner.userId).run();
  return payrollRunSchema.parse({payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-30",payDate:"2026-10-02",currency:"THB"});
}
async function payrollGenericRecord(id:string,title:string,updatedAt:number=Date.now()){
 await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES('people_payroll',?,'payroll-summaries',?,'','draft','[]',?,?,?,?,?)").bind(id,title,f.company,f.owner.userId,f.owner.userId,updatedAt,updatedAt).run();
}
it("Payroll projection filters actual native Records before search and page limits while preserving included people and contractor references",async()=>{
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
 const input=await legacyPayrollGroup(),actor={...f.owner,modules:['people_payroll']},now=Date.now(),shared=key(),contractor=key(),runs:string[]=[];
 await payrollGenericRecord(shared,'Payroll projection directory reference',now);
 await db.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,created_by,created_at,updated_at) VALUES(?,?,'contractor',?,'THB',?,1,1)").bind(contractor,f.company,key(),f.owner.userId).run();await payrollGenericRecord(contractor,'Payroll projection contractor source',now+1);
 for(let index=0;index<16;index++){const day=String(index+1).padStart(2,'0'),run=await createPayrollRun(f.owner,{...input,periodStart:`2026-09-${day}`,periodEnd:`2026-09-${day}`},key());runs.push(String(run.id));await payrollGenericRecord(String(run.id),'Payroll projection private run',now+index+2);}
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(getRecord(actor,'people_payroll',runs[0],db)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect((await listRecords(actor,'people_payroll','payroll-summaries')).map(row=>row.id).sort()).toEqual([shared,contractor].sort());
 const page=await recordsPage(actor,'people_payroll','payroll-summaries',{q:'Payroll projection'});expect(page.items.map(row=>row.id).sort()).toEqual([shared,contractor].sort());const search=await searchReferences(actor,'Payroll projection');expect(search.filter(row=>row.kind==='record').map(row=>'id' in row?row.id:undefined).sort()).toEqual([shared,contractor].sort());
 await purchasePayroll();await purchasePayroll('draining');expect((await getRecord(actor,'people_payroll',runs[0],db)).id).toBe(runs[0]);expect((await listRecords(actor,'people_payroll','payroll-summaries'))).toHaveLength(18);
 const narrowed={...actor,iam:{permissions:['records.read'],grantablePermissions:[],principalId:'fictional-narrowed-context',personId:null,roleRevision:1,identityVersion:0,freshUntil:null}};await expect(getRecord(narrowed,'people_payroll',runs[0],db)).rejects.toMatchObject({status:404,code:'PAYROLL_RECORD_NOT_FOUND'});expect((await listRecords(narrowed,'people_payroll','payroll-summaries')).map(row=>row.id).sort()).toEqual([shared,contractor].sort());
 const scope=await payrollSourceReadScope(db,actor,'records');expect((await db.prepare(`SELECT id FROM workspace_records WHERE company_id=? AND ${scope.where} LIMIT 30`).bind(f.company,...scope.args).all()).results).toHaveLength(18);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(actor.assignmentId).run();await expect(getRecord(actor,'people_payroll',runs[0],db)).rejects.toMatchObject({status:404});expect((await listRecords(actor,'people_payroll','payroll-summaries')).map(row=>row.id).sort()).toEqual([shared,contractor].sort());
});
it("Payroll projection hides unpurchased notices before limits and preserves exact employee-owned advance authority during draining",async()=>{
 const input=await legacyPayrollGroup(),advance=async(actor:typeof f.claimant)=>{const engagement=await db.prepare('SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?').bind(f.company,actor.userId).first<{id:string}>();await db.prepare('UPDATE hr_engagements SET pay_group_id=?,version=version+1 WHERE id=?').bind(input.payGroupId,engagement!.id).run();return requestPayrollAdvance(actor,payrollAdvanceRequestSchema.parse({engagementId:engagement!.id,amount:'10',installmentCount:1,firstDeductionPeriod:'2026-10-01',currency:'THB',reason:'Actual owner notification projection fixture'}),key());};
 const own=await advance(f.claimant),other=await advance(f.head),now=Date.now();
 // Both durable channels retain their rows; inaccessible newer records must
 // never consume the candidate LIMIT ahead of the older own source.
 for(let index=0;index<106;index++){const id=String(index===0?own.id:other.id),message=index===0?'Own Payroll advance source':'Other employee private advance';await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'payroll_advance',?,?,?,?)").bind(key(),f.company,f.claimant.userId,id,index+100,message,now+index).run();await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:'payroll_advance',sourceId:id,sourceVersion:index+100,category:'work',preview:message,recipientIds:[f.claimant.userId]});}
 const before=(await db.prepare('SELECT COUNT(*) n FROM inbox_events WHERE company_id=?').bind(f.company).first('n'));
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await listStaffNotifications(db,f.claimant)).toEqual([]);expect((await inboxPage(db,f.claimant,{view:'all'})).items).toEqual([]);expect(await payrollNotificationTarget(db,f.claimant,'advance',String(own.id))).toBeNull();
 await purchasePayroll();await purchasePayroll('draining');expect(await payrollNotificationTarget(db,f.claimant,'advance',String(own.id))).toBe(own.id);expect(await payrollNotificationTarget(db,f.claimant,'advance',String(other.id))).toBeNull();const notices=await listStaffNotifications(db,f.claimant);expect(notices.some(row=>row.message==='Own Payroll advance source')).toBe(true);expect(notices.some(row=>row.message==='Other employee private advance')).toBe(false);const inbox=await inboxPage(db,f.claimant,{view:'all'});expect(inbox.items.some(row=>row.message==='Own Payroll advance source')).toBe(true);expect(inbox.items.some(row=>row.message==='Other employee private advance')).toBe(false);
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.claimant.assignmentId).run();expect(await payrollNotificationTarget(db,f.claimant,'advance',String(own.id))).toBeNull();expect((await inboxPage(db,f.claimant,{view:'all'})).items).toEqual([]);expect(await db.prepare('SELECT COUNT(*) n FROM inbox_events WHERE company_id=?').bind(f.company).first('n')).toBe(before);
});

it("Payroll purchase denies legacy money and private readers without the add-on, keeps worker directory independent and shadows the original authority",async()=>{
  const directoryActor={...f.owner},input=await legacyPayrollGroup();vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  await expect(createPayrollRun(f.owner,input,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
  for(const read of [()=>payrollSummary(f.owner),()=>listPayrollRuns(f.owner),()=>getPayrollRun(f.owner,"missing"),()=>getPayrollReviewFile(f.owner,"missing",1)])await expect(read()).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
  expect(await workPayrollRows(f.owner,db)).toEqual([]);await expect(listPeople(directoryActor)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
  const references=packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-worker-foundation'}),obligations:async()=>{throw Error('No foundation withdrawal witness');}});await references.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Projects-only worker reference',evidence:'test:included-worker-reference'}});const workers=await peopleReferenceRows(directoryActor,'worker',db);expect(workers.length).toBeGreaterThan(0);expect(workers.every(row=>!('phone' in row)&&!('email' in row)&&!('base_amount_minor' in row)&&!('payroll' in row))).toBe(true);
  const base=packageGrantStore(db,{authorize:async()=>({userId:"isolated-payroll-base-review"}),obligations:async()=>{throw Error("No People withdrawal witness");}});
  await base.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:"people",packageVersion:1,state:"active",startsAt:0,endsAt:null,version:1,reason:"Base People preserves directory only",evidence:"test:payroll-addon-required"}});
  await expect(createPayrollRun(f.owner,input,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
  vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","shadow");const log=vi.spyOn(console,"info").mockImplementation(()=>{});
  try {
    const created=await createPayrollRun(f.owner,input,key());expect((await getPayrollRun(f.owner,String(created.id))).run.id).toBe(created.id);
    await expect(getPayrollRun(f.claimant,String(created.id))).rejects.toMatchObject({status:403});
    expect(log.mock.calls.some(args=>String(args[0]).includes("commercial.purchase.shadow")&&String(args[0]).includes("people.payroll"))).toBe(true);
  } finally {log.mockRestore();}
});

it("Payroll purchase rechecks late deferred personal authority and rolls the original run, receipt, audit and outbox back before exact retry",async()=>{
  const input=await legacyPayrollGroup();await purchasePayroll();vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  const command=key(),fault=`payroll_purchase_${key().replaceAll('-','')}`,tables=["hr_payroll_runs","hr_payroll_items","command_receipts","audit_events","outbox_events","flow_events","assignments"],image=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await image();
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try {await expect(createPayrollRun(f.owner,input,command)).rejects.toMatchObject({code:"PAYROLL_ACCESS_CHANGED"});expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
  const result=await createPayrollRun(f.owner,input,command);expect(await createPayrollRun(f.owner,input,command)).toEqual(result);
  expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=?").bind(command).first("n")).toBe(1);
  await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();
  await expect(createPayrollRun(f.owner,input,command)).rejects.toMatchObject({code:"PAYROLL_ACCESS_CHANGED"});
  await expect(getPayrollRun(f.owner,String(result.id))).rejects.toMatchObject({code:"PAYROLL_ACCESS_CHANGED"});
});

it("Payroll purchase samples expiry after deferred owner effects and leaves no created run or money receipt",async()=>{
  const input=await legacyPayrollGroup();await purchasePayroll("active",Date.now()+1300);vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  const command=key(),fault=`payroll_expiry_${key().replaceAll('-','')}`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.4);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try {await expect(createPayrollRun(f.owner,input,command)).rejects.toMatchObject({code:"PACKAGE_CAPABILITY_CHANGED"});expect(await db.prepare("SELECT 1 FROM hr_payroll_runs WHERE id=?").bind(`payrun-${command}`).first()).toBeNull();expect(await db.prepare("SELECT 1 FROM command_receipts WHERE command_id=?").bind(command).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
});

async function submitRun(runId:string,version:number){
  const digest=await db.prepare("SELECT input_digest FROM hr_payroll_runs WHERE id=?").bind(runId).first<string>("input_digest");
  return submitPayrollReview(f.owner,runId,{expectedVersion:version,inputDigest:digest!},key());
}

it("requires explicit HR evidence for imported cost units and repairs approved time snapshots",async()=>{
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  const engagement=await db.prepare("SELECT e.id,e.start_date FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string;start_date:string}>();
  const assignment=await db.prepare("SELECT id FROM hr_payroll_cost_assignments WHERE engagement_id=?").bind(engagement!.id).first<{id:string}>();
  await db.prepare("UPDATE hr_payroll_cost_assignments SET evidence_status='legacy_unverified',source='migration' WHERE id=?").bind(assignment!.id).run();
  const timeId=`cost-time-${key()}`;
  await db.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,status,created_at,updated_at) VALUES(?,?,?,?,1,'approved',1,1)").bind(timeId,engagement!.id,f.company,engagement!.start_date).run();
  expect(await db.prepare("SELECT cost_assignment_id FROM hr_time_entries WHERE id=?").bind(timeId).first("cost_assignment_id")).toBeNull();
  await expect(verifyPayrollCostAssignment(f.owner,assignment!.id,{expectedVersion:1,evidenceRef:"HR signed roster evidence"},key())).resolves.toMatchObject({status:"verified",version:2});
  expect(await db.prepare("SELECT cost_assignment_id FROM hr_time_entries WHERE id=?").bind(timeId).first("cost_assignment_id")).toBe(assignment!.id);
  expect(await db.prepare("SELECT evidence_ref FROM hr_payroll_cost_assignment_verifications WHERE assignment_id=?").bind(assignment!.id).first("evidence_ref")).toBe("HR signed roster evidence");
});

it("snapshots configured proration and overtime and exports an immutable review revision",async()=>{
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  const engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>(),group=`paygroup-${key()}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Core payroll','TH','THB','monthly','Asia/Bangkok',20000,'calendar_days',?,1,1)").bind(group,f.company,f.owner.userId).run();
  await db.prepare("UPDATE hr_engagements SET pay_group_id=?,start_date='2026-09-16',version=version+1 WHERE id=?").bind(group,engagement!.id).run();
  await db.prepare("UPDATE hr_people SET legal_name='=REVIEW_FORMULA()',preferred_name=NULL,version=version+1 WHERE id=(SELECT person_id FROM hr_engagements WHERE id=?)").bind(engagement!.id).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-09-16','monthly',300000,0,160,'THB','[]','approved',?,? ,1,1)").bind(`comp-${key()}`,engagement!.id,f.manager.userId,f.reviewer.userId).run();
  await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','calculate',?,?,'Core payroll fixture',1)").bind(f.company,f.owner.userId,f.owner.userId).run();
  const created=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-30",payDate:"2026-09-30",currency:"THB"}),key()),calculated=await calculatePayrollRun(f.owner,String(created.id),key());
  expect(calculated.totals).toMatchObject({gross:150000,net:150000,headcount:1});
  const snapshot=JSON.parse(String(await db.prepare("SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=?").bind(created.id).first("input_snapshot_json")));
  expect(snapshot.policy).toMatchObject({overtimeMultiplierBps:20000,prorationMethod:"calendar_days",activeDays:15,periodDays:30});
  expect(snapshot.payee).toMatchObject({id:expect.any(String),legal_name:expect.any(String),version:expect.any(Number)});
  const command=key(),exported=await exportPayrollReview(f.owner,String(created.id),command);expect(await exportPayrollReview(f.owner,String(created.id),command)).toEqual(exported);
  const file=await getPayrollReviewFile(f.owner,String(created.id),Number(exported.revision)),text=await file.object.text();expect(text).toContain("employee,worker_type");expect(text).toContain("1500.00");expect(text).toContain("\"'=REVIEW_FORMULA()\"");
  expect(file.checksum).toBe(exported.checksum);
  await db.prepare("UPDATE hr_people SET legal_name=legal_name||' changed',version=version+1 WHERE id=?").bind(snapshot.payee.id).run();
  await expect(submitRun(String(created.id),Number(calculated.version))).rejects.toMatchObject({code:"PAYROLL_NOT_READY"});
});

it("posts an approved payroll run into the finance ledger exactly once",async()=>{
  const financeOwner={...f.owner};
  vi.useFakeTimers({toFake:["Date"]});vi.setSystemTime(new Date("2026-10-02T00:00:00Z"));
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  f.manager.role="payroll_approver";f.manager.assignmentId=await f.grant(f.manager,"payroll_approver");
  f.reviewer.role="finance_approver";f.reviewer.assignmentId=await f.grant(f.reviewer,"finance_approver");
  f.claimant.role="treasury";f.claimant.assignmentId=await f.grant(f.claimant,"treasury");
  await purchasePayroll();vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  const engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>(),group=`paygroup-${key()}`,employer=`${f.company}-employer-${key()}`,employerExpense=`${f.company}-employer-expense-${key()}`,taxLiability=`${f.company}-payroll-tax-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(employer,f.chart,f.company,"EMPLOYER-${key()}","Employer contributions payable"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'expense','THB',1,1)").bind(employerExpense,f.chart,f.company,"EMPLOYER-EXPENSE-${key()}","Employer contribution expense"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(taxLiability,f.chart,f.company,"PAYROLL-TAX-${key()}","Payroll tax payable"),
    db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Posting payroll','TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,1,1)").bind(group,f.company,f.owner.userId),
    db.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1 WHERE id=?").bind(group,engagement!.id),
    db.prepare("UPDATE hr_people SET legal_name='=PAYMENT_FORMULA()',preferred_name=NULL,version=version+1 WHERE id=(SELECT person_id FROM hr_engagements WHERE id=?)").bind(engagement!.id),
    db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-09-01','monthly',100000,0,160,'THB',?,'approved',?,?,1,1)").bind(`comp-${key()}`,engagement!.id,JSON.stringify([{code:"TAX",label:"Payroll deduction",kind:"deduction",amount:"10.00",taxable:false},{code:"SOCIAL",label:"Employer contribution",kind:"employer",amount:"5.00",taxable:false}]),f.manager.userId,f.reviewer.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','calculate',?,?,'Posting payroll fixture',1)").bind(f.company,f.owner.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','preview',?,?,'Posting payroll submission fixture',1)").bind(f.company,f.owner.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','in_review',?,?,'Posting payroll approval fixture',1)").bind(f.company,f.manager.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','approved',?,?,'Finance payroll posting fixture',1)").bind(f.company,f.reviewer.userId,f.owner.userId),
  ]);
  const book=await preparePayrollProjection(f,financeOwner,f.reviewer);
  const created=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-30",payDate:"2026-10-01",currency:"THB"}),key());
  await purchasePayroll("draining");expect(await payrollPurchaseControls(f.owner)).toEqual({newWork:false,settleExisting:true,history:true,revision:expect.any(String)});
  const minimalActor={userId:financeOwner.userId,assignmentId:financeOwner.assignmentId,role:financeOwner.role,companyId:financeOwner.companyId,branchId:financeOwner.branchId,departmentId:financeOwner.departmentId,authority:financeOwner.authority,iam:financeOwner.iam},flow=await resolveFlow(db,minimalActor,"payroll",String(created.id)),routeKey=key(),routeInput={stepKey:"calculate",userId:f.owner.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:"Existing Company authority retains the independent Payroll flow through its original Holding database",saveDefault:false};
  const routed=await assignFlowPrimary(db,minimalActor,"payroll",String(created.id),routeInput,routeKey);expect(await assignFlowPrimary(db,minimalActor,"payroll",String(created.id),routeInput,routeKey)).toEqual(routed);
  const calculated=await calculatePayrollRun(f.owner,String(created.id),key());
  expect(calculated.totals).toMatchObject({gross:100000,deductions:1000,employer:500,net:99000});
  const submitted=await submitRun(String(created.id),Number(calculated.version));
  const approved=await approvePayrollRun(f.manager,String(created.id),Number(submitted.version),key());
  const mappingId=`payroll-mapping-${key()}`;
  await db.prepare("INSERT INTO hr_payroll_account_mappings(id,company_id,pay_group_id,chart_id,accounts_json,status,version,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,'approved',2,?,?,1,1)")
    .bind(mappingId,f.company,group,f.chart,JSON.stringify({chartId:f.chart,expenseAccountId:f.accounts.expense,employerExpenseAccountId:employerExpense,netPayableAccountId:f.accounts.payable,taxPayableAccountId:taxLiability,employerContributionAccountId:employer}),f.owner.userId,f.reviewer.userId).run();
  const postInput=payrollPostSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-30",expenseAccountId:f.accounts.expense,employerExpenseAccountId:employerExpense,netPayableAccountId:f.accounts.payable,taxPayableAccountId:taxLiability,employerContributionAccountId:employer,expectedVersion:approved.version});
  const postKey=key(),posted=await postPayroll(f.reviewer,String(created.id),postInput,postKey);
  expect(posted).toMatchObject({status:"posted",journalId:expect.any(String),netPayableMinor:99000});
  expect(await db.prepare("SELECT status,posted_by,version FROM hr_payroll_runs WHERE id=?").bind(created.id).first()).toMatchObject({status:"posted",posted_by:f.reviewer.userId,version:Number(approved.version)+1});
  expect(await db.prepare("SELECT status FROM hr_payroll_items WHERE run_id=?").bind(created.id).first("status")).toBe("posted");
  const journal=await db.prepare("SELECT id,status,source_type FROM finance_journals WHERE id=?").bind(posted.journalId).first();expect(journal).toMatchObject({status:"posted",source_type:"payroll"});
  expect(await db.prepare("SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(posted.journalId).first()).toEqual({debit:100500,credit:100500});
  const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),peopleObservation=()=>withOwnerPackageObligations(db,{companyId:f.company,packageId:'people',packageVersion:1},tx=>readOwnerPackageObligations(db,tx,{companyId:f.company,packageId:'people',packageVersion:1,now:Date.now()}));expect((await peopleObservation()).observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(4);
  const liabilities=await getPayrollLiabilities(f.claimant,String(created.id));
  expect(liabilities.obligations).toEqual(expect.arrayContaining([
    expect.objectContaining({kind:"withholding_tax",accruedMinor:1000,outstandingMinor:1000}),
    expect.objectContaining({kind:"social_security",accruedMinor:500,outstandingMinor:500}),
  ]));
  const remittanceStatement=`statutory-statement-${key()}`,taxTransaction=`statutory-tax-${key()}`,ssoTransaction=`statutory-sso-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,0,-1500,?,?,1)").bind(remittanceStatement,f.company,f.accounts.cash,`STATUTORY-${key()}`,"2026-10-01",key(),f.owner.userId),
    db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,'2026-10-01','TAX-REMIT-001',-1000,'THB')").bind(taxTransaction,remittanceStatement,f.company),
    db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,'2026-10-01','SSO-REMIT-001',-500,'THB')").bind(ssoTransaction,remittanceStatement,f.company),
  ]);
  const taxInput=payrollRemittanceSchema.parse({kind:"withholding_tax",amountMinor:1000,expectedRunVersion:Number(posted.version),bankAccountId:f.accounts.cash,bankTransactionId:taxTransaction,bankVersion:1,bankReference:"TAX-REMIT-001"});
  const taxKey=key(),taxRemitted=await remitPayrollLiability(f.claimant,String(created.id),taxInput,taxKey);
  expect(taxRemitted).toMatchObject({amountMinor:1000,remainingMinor:0});
  expect(await remitPayrollLiability(f.claimant,String(created.id),taxInput,taxKey)).toEqual(taxRemitted);
  await expect(remitPayrollLiability(f.claimant,String(created.id),taxInput,key())).rejects.toMatchObject({code:"PAYROLL_LIABILITY_EXCEEDED"});
  const ssoRemitted=await remitPayrollLiability(f.claimant,String(created.id),payrollRemittanceSchema.parse({kind:"social_security",amountMinor:500,expectedRunVersion:Number(posted.version),bankAccountId:f.accounts.cash,bankTransactionId:ssoTransaction,bankVersion:1,bankReference:"SSO-REMIT-001"}),key());
  expect(ssoRemitted).toMatchObject({amountMinor:500,remainingMinor:0});
  expect((await getPayrollLiabilities(f.claimant,String(created.id))).obligations.map(row=>row.outstandingMinor)).toEqual([0,0]);
  expect(await db.prepare("SELECT COUNT(*) AS count FROM finance_journals WHERE source_type='payroll_remittance' AND company_id=?").bind(f.company).first()).toEqual({count:2});
  expect((await resolveFlow(db,f.reviewer,"payroll",String(created.id))).current?.key).toBe("posted");
  expect(await postPayroll(f.reviewer,String(created.id),postInput,postKey)).toEqual(posted);
  await expect(postPayroll(f.reviewer,String(created.id),postInput,`replay-${key()}`)).rejects.toMatchObject({code:"PAYROLL_NOT_APPROVED"});
  await db.prepare("UPDATE hr_people SET legal_name='Changed after posting',version=version+1 WHERE id=(SELECT person_id FROM hr_engagements WHERE id=?)").bind(engagement!.id).run();
  const batch=await createPaymentBatch(f.reviewer,{sourceType:"payroll",sourceId:String(created.id),formatCode:"generic-csv",bankAccountId:f.accounts.cash},key());
  expect(batch).toMatchObject({status:"draft",totalMinor:99000,itemCount:1});
  const loaded=await getPayrollRun(f.reviewer,String(created.id));
  expect(loaded.paymentBatch).toMatchObject({id:batch.id,status:"draft",source_type:"payroll"});
  expect(loaded.paymentItems).toHaveLength(1);
  const treasuryView=await getPayrollRun(f.claimant,String(created.id));
  expect(treasuryView.paymentBatch).toMatchObject({id:batch.id,status:"draft"});
  expect(treasuryView.items).toHaveLength(0);
  expect((await resolveFlow(db,f.reviewer,"payroll",String(created.id))).current?.key).toBe("payment_draft");
  const approvedBatch=await approvePaymentBatch(f.claimant,String(batch.id),1,key());
  expect(approvedBatch).toMatchObject({status:"approved",version:2});
  const exportedBatch=await exportPaymentBatch(f.claimant,String(batch.id),key());
  expect(exportedBatch).toMatchObject({status:"exported",itemCount:1,checksum:expect.any(String)});
  expect(await (await getPaymentFile(f.claimant,String(batch.id))).object.text()).toContain("\"'=PAYMENT_FORMULA()\"");
  const item=await db.prepare("SELECT reference,amount_minor FROM hr_payment_batch_items WHERE batch_id=?").bind(batch.id).first<{reference:string;amount_minor:number}>();
  const failed=await confirmPaymentBatch(f.claimant,String(batch.id),{rows:[{reference:item!.reference,status:"failed",confirmationRef:"SYNTHETIC-REJECTED"}]},key());
  expect(failed).toMatchObject({status:"partially_settled",confirmed:0,failed:1});
  expect(await db.prepare("SELECT COUNT(*) AS count FROM hr_payroll_payment_settlements WHERE batch_id=?").bind(batch.id).first()).toEqual({count:0});
  expect(await db.prepare("SELECT status FROM hr_payment_batch_items WHERE batch_id=?").bind(batch.id).first("status")).toBe("failed");
  const retryVersion=Number(await db.prepare("SELECT version FROM hr_payment_batches WHERE id=?").bind(batch.id).first("version"));
  const retry=await preparePaymentRetry(f.reviewer,String(batch.id),retryVersion,key());
  expect(retry).toMatchObject({status:"draft",retryItems:1});
  await expect(approvePaymentBatch(f.reviewer,String(batch.id),Number(retry.version),key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
  const approvedRetry=await approvePaymentBatch(f.claimant,String(batch.id),Number(retry.version),key());
  const retryExport=await exportPaymentBatch(f.claimant,String(batch.id),key());
  expect(retryExport).toMatchObject({status:"exported",itemCount:1,totalMinor:99000});
  expect(retryExport.fileKey).not.toBe(exportedBatch.fileKey);
  expect(await db.prepare("SELECT COUNT(*) AS count FROM hr_payment_batch_exports WHERE batch_id=?").bind(batch.id).first()).toEqual({count:2});
  expect(approvedRetry.version).toBe(Number(retry.version)+1);
  const statementId=`payroll-statement-${key()}`,transactionId=`payroll-bank-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,0,?,?,?,1)").bind(statementId,f.company,f.accounts.cash,`BANK-${key()}`,"2026-10-01",-item!.amount_minor,key(),f.owner.userId),
    db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,'2026-10-01','BANK-PAYROLL-001',?,'THB')").bind(transactionId,statementId,f.company,-item!.amount_minor),
  ]);
  const settled=await confirmPaymentBatch(f.claimant,String(batch.id),{rows:[{reference:item!.reference,status:"confirmed",confirmationRef:"BANK-PAYROLL-001",bankTransactionId:transactionId,bankVersion:1}]},key());
  expect(settled).toMatchObject({status:"settled",confirmed:1});
  expect(await db.prepare("SELECT COUNT(*) AS count FROM hr_payroll_payment_settlements WHERE batch_id=?").bind(batch.id).first()).toEqual({count:1});
  expect(await db.prepare("SELECT debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=(SELECT journal_id FROM hr_payroll_payment_settlements WHERE batch_id=?) ORDER BY ordinal").bind(batch.id).all()).toMatchObject({results:[{debit_minor:99000,credit_minor:0},{debit_minor:0,credit_minor:99000}]});
  const paymentFile=await getPaymentFile(f.claimant,String(batch.id));
  expect(await paymentFile.object.text()).toContain(item!.reference);
  expect(await db.prepare("SELECT status FROM hr_payment_batches WHERE id=?").bind(batch.id).first("status")).toBe("settled");
  const finalFlow=await resolveFlow(db,f.reviewer,"payroll",String(created.id)); expect(finalFlow.current).toBeNull(); expect(finalFlow.blocker).toBeNull();
  const ownedCash=(await db.prepare("SELECT journal_id FROM hr_payroll_statutory_remittances WHERE company_id=? UNION ALL SELECT journal_id FROM hr_payroll_payment_settlements WHERE company_id=?").bind(f.company,f.company).all<{journal_id:string}>()).results;
  expect(ownedCash).toHaveLength(3);
  await assertOwnedCashCannotMatchAgain(db,financeOwner,ownedCash.map(row=>row.journal_id),f.accounts.cash);
  await assertPayrollFinanceHistory(f,financeOwner,f.owner,String(created.id),[String(posted.journalId),...ownedCash.map(row=>row.journal_id)],book);
  // The actual net+two statutory duties are discharged. This imported salary
  // lacks its original immutable money capture and must stay conservative.
  expect((await peopleObservation()).observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(1);const workforceObservation=await withOwnerPackageObligations(db,{companyId:f.company,packageId:'people-workforce',packageVersion:1},tx=>readOwnerPackageObligations(db,tx,{companyId:f.company,packageId:'people-workforce',packageVersion:1,now:Date.now()}));expect(workforceObservation.observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(0);
});

it("requires an independent correction decision and preserves every reviewed payroll revision",async()=>{
  vi.useFakeTimers({toFake:["Date"]});vi.setSystemTime(new Date("2026-10-02T00:00:00Z"));
  const requesterWithApprovalAuthority={...f.owner};
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  f.manager.role="people_manager";f.manager.assignmentId=await f.grant(f.manager,"people_manager");
  const engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>(),group=`paygroup-${key()}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Correction payroll','TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,1,1)").bind(group,f.company,f.owner.userId).run();
  await db.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1 WHERE id=?").bind(group,engagement!.id).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-09-01','monthly',300000,0,160,'THB','[]','approved',?,?,1,1)").bind(`comp-${key()}`,engagement!.id,f.manager.userId,f.reviewer.userId).run();
  await db.batch([
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','calculate',?,?,'Correction payroll fixture',1)").bind(f.company,f.owner.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','preview',?,?,'Correction payroll fixture',1)").bind(f.company,f.owner.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','in_review',?,?,'Correction payroll fixture',1)").bind(f.company,f.manager.userId,f.owner.userId),
  ]);
  const created=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-30",payDate:"2026-10-01",currency:"THB"}),key());
  const calculated=await calculatePayrollRun(f.owner,String(created.id),key());
  const exported=await exportPayrollReview(f.owner,String(created.id),key());
  const submitted=await submitRun(String(created.id),Number(calculated.version));
  const approved=await approvePayrollRun(f.manager,String(created.id),Number(submitted.version),key());
  const correctionEvidence=await seedAttachmentSet(db,f.owner,{targetType:"payroll_run",targetId:String(created.id),componentKind:"action",componentRef:"correction-request",purposeKey:"correction_support",expectedTargetVersion:Number(approved.version)}),correctionKey=key();
  const correction=await requestPayrollCorrection(f.owner,String(created.id),{expectedVersion:Number(approved.version),reason:"Approved allowance evidence arrived after review",attachmentSetIds:[correctionEvidence.setId]},correctionKey);
  expect(await attachmentBindingState(db,correctionEvidence.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"payroll_correction_request",binding_id:String(correction.id)});

  await expect(decidePayrollCorrection(requesterWithApprovalAuthority,String(correction.id),{expectedVersion:1,action:"approve",reason:"Trying to review my own request"},key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
  const notifications=await listStaffNotifications(db,f.manager);
  expect(notifications.find(n=>n.record_id===correction.id)?.href).toBe(`/people/payroll/runs/${created.id}#correction-${correction.id}`);
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await payrollNotificationTarget(db,f.manager,'correction',String(correction.id))).toBeNull();await purchasePayroll();await purchasePayroll('draining');expect(await payrollNotificationTarget(db,f.manager,'correction',String(correction.id))).toBe(created.id);
  const selectedEmployee={...f.manager,role:'employee',assignmentId:await f.grant(f.manager,'employee')};expect(await payrollNotificationTarget(db,selectedEmployee,'correction',String(correction.id))).toBeNull();expect((await listStaffNotifications(db,selectedEmployee)).some(row=>row.record_id===correction.id)).toBe(false);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');

  const decision=await decidePayrollCorrection(f.manager,String(correction.id),{expectedVersion:1,action:"approve",reason:"Source evidence supports recalculation"},key());
  expect(decision).toMatchObject({status:"approved",runStatus:"calculating"});
  const reopened=await getPayrollRun(f.owner,String(created.id));
  expect(reopened.run).toMatchObject({status:"calculating",version:Number(approved.version)+1});
  expect(reopened.corrections[0]).toMatchObject({status:"approved",reviewed_by:f.manager.userId});

  const oldFile=await getPayrollReviewFile(f.owner,String(created.id),Number(exported.revision));
  expect(oldFile.checksum).toBe(exported.checksum);
  const recalculated=await calculatePayrollRun(f.owner,String(created.id),key());
  expect(recalculated).toMatchObject({status:"preview",version:Number(approved.version)+2});
  expect(Number((await getPayrollRun(f.owner,String(created.id))).run.revision)).toBeGreaterThan(Number(exported.revision));
  await expect(db.prepare("DELETE FROM hr_payroll_correction_events WHERE correction_id=?").bind(correction.id).run()).rejects.toThrow();
});

it("prepares a person-scoped final-pay preview from the retirement clearance task",async()=>{
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  const engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>(),group=`paygroup-${key()}`,caseId=`retirement-${key()}`,taskId=`${caseId}-clearance-payroll`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Final pay','TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,1,1)").bind(group,f.company,f.owner.userId).run();
  await db.prepare("UPDATE hr_engagements SET pay_group_id=?,end_date='2026-09-15',version=version+1 WHERE id=?").bind(group,engagement!.id).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,effective_to,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-09-01','2026-09-15','monthly',300000,0,160,'THB','[]','approved',?,?,1,1)").bind(`comp-${key()}`,engagement!.id,f.manager.userId,f.reviewer.userId).run();
  await db.prepare("INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,definition,created_by,updated_at) SELECT ?,?,?,?,'offboarding','scheduled',1,1,?::jsonb,?,1 FROM hr_people WHERE id=?").bind(caseId,f.company,`person-${f.claimant.userId}`,engagement!.id,JSON.stringify({lastWorkingDate:"2026-09-15",reason:"Final pay fixture"}),f.owner.userId,`person-${f.claimant.userId}`).run();
  await db.prepare("INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,assignee_id,version,updated_at,privileged,due_date,responsibility,clearance_type) VALUES(?,?,?,'lifecycle','Final payroll',?,1,1,false,'2026-09-15','payroll','payroll')").bind(taskId,f.company,caseId,f.owner.userId).run();
  await db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','calculate',?,?,'Final pay fixture',1)").bind(f.company,f.owner.userId,f.owner.userId).run();
  const prepared=await prepareFinalPay(f.owner,finalPayPreparationSchema.parse({taskId,expectedTaskVersion:1,payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-15",payDate:"2026-09-15",currency:"THB",reason:"Prepare final pay through the last working day"}),key());
  expect(prepared).toMatchObject({taskId, status:"prepared"});
  const linked=await db.prepare("SELECT r.mode,r.scope_engagement_id,r.period_end,fp.status FROM hr_payroll_runs r JOIN hr_payroll_final_pay_preparations fp ON fp.run_id=r.id WHERE fp.task_id=?").bind(taskId).first<{mode:string;scope_engagement_id:string;period_end:string;status:string}>();
  expect(linked).toMatchObject({mode:"final_pay",scope_engagement_id:engagement!.id,period_end:"2026-09-15",status:"prepared"});
  const calculated=await calculatePayrollRun(f.owner,String(prepared.runId),key());
  expect(calculated.totals).toMatchObject({headcount:1,gross:150000,net:150000});
  await expect(prepareFinalPay(f.owner,finalPayPreparationSchema.parse({taskId,expectedTaskVersion:1,payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-15",payDate:"2026-09-15",currency:"THB",reason:"Duplicate final pay"}),key())).rejects.toThrow();
  const preparation=await db.prepare("SELECT id,version FROM hr_payroll_final_pay_preparations WHERE run_id=?").bind(prepared.runId).first<{id:string;version:number}>(),fileInput={targetType:"payroll_final_pay",targetId:preparation!.id,componentKind:"record" as const,purposeKey:"final_pay_support",expectedTargetVersion:preparation!.version};
  vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");await expect(createAttachmentSet(db,f.owner,fileInput,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await purchasePayroll();await purchasePayroll("draining");expect(await createAttachmentSet(db,f.owner,fileInput,key())).toMatchObject({targetType:"payroll_final_pay",targetId:preparation!.id});
  await payrollGenericRecord(preparation!.id,'Assigned final-pay owner projection');const recordsActor={...f.owner,modules:['people_payroll']};expect((await getRecord(recordsActor,'people_payroll',preparation!.id,db)).id).toBe(preparation!.id);await expect(getRecord({...recordsActor,userId:f.head.userId,assignmentId:f.head.assignmentId,role:f.head.role},'people_payroll',preparation!.id,db)).rejects.toMatchObject({status:404,code:'PAYROLL_RECORD_NOT_FOUND'});
});

it("Payroll Files requires its actual purchase while retaining own advance evidence and rolls deferred owner and authority changes back",async()=>{
  const input=await legacyPayrollGroup(),engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>();
  await db.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1 WHERE id=?").bind(input.payGroupId,engagement!.id).run();
  const run=await createPayrollRun(f.owner,input,key()),advance=await requestPayrollAdvance(f.claimant,payrollAdvanceRequestSchema.parse({engagementId:engagement!.id,amount:"60",installmentCount:2,firstDeductionPeriod:"2026-10-01",currency:"THB",reason:"Existing employee advance with retained reviewed evidence"}),key()),id=String(advance.id),files=await seedAttachmentSet(db,f.claimant,{targetType:"payroll_advance",targetId:id,componentKind:"record",purposeKey:"advance_support",expectedTargetVersion:1}),objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(files.sourceId).first("object_key")),bytes="%PDF-1.7";
  await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(files.sourceId).run();
  vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  await expect(attachmentContent(db,env.H001_FILES,f.claimant,files.linkId,"original")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await expect(bindRecordAttachmentSet(db,f.claimant,files.setId,1,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
  const runFileInput={targetType:"payroll_run",targetId:String(run.id),componentKind:"action" as const,componentRef:"correction-request",purposeKey:"correction_support",expectedTargetVersion:1};await expect(createAttachmentSet(db,f.owner,runFileInput,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
  await purchasePayroll();await purchasePayroll("draining");expect(await createAttachmentSet(db,f.owner,runFileInput,key())).toMatchObject({targetType:"payroll_run",targetId:run.id});
  const tables=["hr_payroll_advances","assignments","attachment_sets","attachment_links","attachment_events","attachments","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results));
  for(const change of ["source","authority"]){
    const command=key(),fault=`payroll_sensitive_file_${key().replaceAll('-','')}`,before=await snapshot(),effect=change==="source"?`UPDATE hr_payroll_advances SET status='cancelled',version=version+1 WHERE id='${id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`;
    await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
    try{let failure:unknown;try{await bindRecordAttachmentSet(db,f.claimant,files.setId,1,command);}catch(error){failure=error;}expect(failure).toBeInstanceOf(Error);expect((failure as {cause?:unknown}).cause??failure).toMatchObject({code:"23514",detail:expect.stringContaining(change==="source"?"payroll-file-owner":"iam-current-authority")});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
  }
  const command=key(),bound=await bindRecordAttachmentSet(db,f.claimant,files.setId,1,command);expect(await bindRecordAttachmentSet(db,f.claimant,files.setId,1,command)).toEqual(bound);expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"bound",link_state:"bound"});expect(await db.prepare("SELECT COUNT(*) n FROM command_receipts WHERE command_id=?").bind(command).first("n")).toBe(1);expect(await (await attachmentContent(db,env.H001_FILES,f.claimant,files.linkId,"original")).object.text()).toBe(bytes);
  await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(f.claimant.assignmentId).run();await expect(attachmentContent(db,env.H001_FILES,f.claimant,files.linkId,"original")).rejects.toMatchObject({status:404});expect(await db.prepare("SELECT legal_hold,revoked_at FROM attachments WHERE id=?").bind(files.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect(await (await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);
});

it("approves payroll advances into scheduled deductions and snapshots them in payroll",async()=>{
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  f.manager.role="payroll_approver";f.manager.assignmentId=await f.grant(f.manager,"payroll_approver");
  const requesterApprover={...f.claimant,role:"payroll_approver",assignmentId:await f.grant(f.claimant,"payroll_approver")};
  const engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>(),group=`paygroup-${key()}`;
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Advance payroll','TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,1,1)").bind(group,f.company,f.owner.userId).run();
  await db.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1 WHERE id=?").bind(group,engagement!.id).run();
  await db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-01-01','monthly',300000,0,160,'THB','[]','approved',?,?,1,1)").bind(`comp-${key()}`,engagement!.id,f.manager.userId,f.reviewer.userId).run();
  const input=payrollAdvanceRequestSchema.parse({engagementId:engagement!.id,amount:"600.00",installmentCount:3,firstDeductionPeriod:"2026-10-01",currency:"THB",reason:"Emergency equipment replacement"});
  const requested=await requestPayrollAdvance(f.claimant,input,key());
  expect(requested).toMatchObject({status:"pending",amountMinor:60000});
  expect((await payrollAdvancePage(f.claimant)).items).toHaveLength(1);
  await expect(decidePayrollAdvance(requesterApprover,String(requested.id),{expectedVersion:1,action:"approve",reason:"Cannot approve my own request"},key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
  const approved=await decidePayrollAdvance(f.manager,String(requested.id),{expectedVersion:1,action:"approve",reason:"Emergency evidence reviewed"},key());
  expect(approved).toMatchObject({status:"approved",version:2});
  const schedule=await db.prepare("SELECT sequence,amount_minor,due_period_start,due_period_end,status FROM hr_payroll_advance_repayments WHERE advance_id=? ORDER BY sequence").bind(requested.id).all();
  expect(schedule.results).toHaveLength(3);expect(schedule.results.map(row=>Number(row.amount_minor))).toEqual([20000,20000,20000]);expect(schedule.results[0]).toMatchObject({due_period_start:"2026-10-01",status:"open"});
  await expect(db.prepare("DELETE FROM hr_payroll_advance_events WHERE advance_id=?").bind(requested.id).run()).rejects.toThrow();
  const run=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:group,periodStart:"2026-10-01",periodEnd:"2026-10-31",payDate:"2026-10-31",currency:"THB"}),key());
  const calculated=await calculatePayrollRun(f.owner,String(run.id),key());
  expect(calculated.totals).toMatchObject({gross:300000,deductions:20000,net:280000,headcount:1});
  const snapshot=JSON.parse(String(await db.prepare("SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=?").bind(run.id).first("input_snapshot_json")));
  expect(snapshot.advanceRepayments).toHaveLength(1);expect(snapshot.advanceRepayments[0]).toMatchObject({amount_minor:20000,due_period_start:"2026-10-01"});
  expect((await listStaffNotifications(db,f.manager)).find(n=>n.record_id===requested.id)?.href).toBe(`/people/payroll/advances#payroll-advance-${requested.id}`);
});

// Payroll addon settlement uses native Payroll duties, not base People workers,
// an unrelated Compensation draft, Expense claims or contractor batches.
async function payrollAddonGrant(packageId:'people'|'people-payroll'|'people-expenses',state:'active'|'draining'='active'){
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-payroll-addon-provider'}),obligations:async()=>{throw Error('Use the concrete owner settlement wrapper for withdrawal');}}),prior=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId===packageId);
 return store.apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Reviewed disposable addon lifecycle',evidence:'test:payroll-addon-owner'}});
}
function payrollAddonScope(){return {companyId:f.company,packageId:'people-payroll',packageVersion:1};}
async function payrollAddonObservation(){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations');return withOwnerPackageObligations(db,payrollAddonScope(),tx=>readOwnerPackageObligations(db,tx,{...payrollAddonScope(),now:Date.now()}));}
async function payrollAddonWithdrawal(){const {packageGrantRevision}=await import('../../../src/server/modules/commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==='people-payroll')!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual Payroll owner duties discharged',evidence:'test:payroll-addon-owner'}}]};}
async function payrollAddonReview(request:Awaited<ReturnType<typeof payrollAddonWithdrawal>>,apply=false){const {withOwnerPackageObligations,ownerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-payroll-addon-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});return withOwnerPackageObligations(db,payrollAddonScope(),tx=>apply?store.batch.applyIn(tx,request,async()=>{}):store.batch.previewIn(tx,request));}
async function payrollAddonImage(){return db.prepare(`SELECT
 (SELECT COALESCE(jsonb_agg(to_jsonb(g) ORDER BY package_id),'[]') FROM company_package_grants g WHERE company_id=?) grants,
 (SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY id),'[]') FROM company_package_grant_events e WHERE company_id=?) events,
 (SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY id),'[]') FROM company_package_change_sets e WHERE company_id=?) changes,
 (SELECT COALESCE(jsonb_agg(to_jsonb(g) ORDER BY id),'[]') FROM hr_pay_groups g WHERE company_id=?) groups,
 (SELECT COALESCE(jsonb_agg(to_jsonb(r) ORDER BY id),'[]') FROM hr_payroll_runs r WHERE company_id=?) runs,
 (SELECT COALESCE(jsonb_agg(to_jsonb(c) ORDER BY command_id),'[]') FROM command_receipts c WHERE authority_json::jsonb->>'companyId'=?) receipts,
 (SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY id),'[]') FROM audit_events a WHERE a.authority_json::jsonb->>'companyId'=?) audit,
 (SELECT COALESCE(jsonb_agg(to_jsonb(o) ORDER BY id),'[]') FROM outbox_events o JOIN command_receipts c ON c.command_id=o.command_id WHERE c.authority_json::jsonb->>'companyId'=?) outbox`).bind(...Array(8).fill(f.company)).first();}
it('withdraws only the Payroll addon after normal schedule disable and retained run cancellation while base People and Expenses continue',async()=>{
 const {createPayGroup,payGroupSchema,createCompensation,compensationSchema,cancelPayrollRun,payrollCancelSchema}=await import('../../../src/server/modules/people_payroll/public/people-payroll'),{savePayrollSchedule,payrollScheduleSchema}=await import('../../../src/server/modules/people_payroll/public/payroll-readiness');
 await payrollAddonGrant('people');await payrollAddonGrant('people-payroll');await payrollAddonGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const engagement=await db.prepare('SELECT id FROM hr_engagements WHERE company_id=? ORDER BY id LIMIT 1').bind(f.company).first<string>('id');expect(engagement).toBeTruthy();
 await createCompensation(f.owner,compensationSchema.parse({engagementId:engagement!,effectiveFrom:'2026-09-01',payFrequency:'monthly',baseAmount:'100',currency:'THB'}),key());
 const {saveExpense,expenseUpdateSchema}=await import('../../../src/server/modules/people_payroll/public/expenses'),expense=await saveExpense(f.claimant,null,expenseUpdateSchema.parse({expectedVersion:1,merchant:'Independent Expense work'}),key());
 expect((await payrollAddonObservation()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);
 f.owner.role='payroll_preparer';f.owner.assignmentId=await f.grant(f.owner,'payroll_preparer');
 const group=await createPayGroup(f.owner,payGroupSchema.parse({name:'Normal addon lifecycle group',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),key()),input=payrollRunSchema.parse({payGroupId:group.id,periodStart:'2026-09-01',periodEnd:'2026-09-30',payDate:'2026-10-02',currency:'THB'}),createKey=key(),run=await createPayrollRun(f.owner,input,createKey);
 const schedule=await savePayrollSchedule(f.owner,group.id,payrollScheduleSchema.parse({expectedVersion:1,enabled:true,payDelayDays:1}),key());
 await payrollAddonGrant('people-payroll','draining');const pending=await payrollAddonObservation();expect(pending.observations.find(row=>row.moduleId==='people_payroll')!.outstanding).toBe(2);expect(pending.observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(1);
 const blocked=await payrollAddonWithdrawal(),before=await payrollAddonImage();await expect(payrollAddonReview(blocked)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(payrollAddonReview(blocked,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await payrollAddonImage()).toEqual(before);
 await expect(createPayrollRun(f.owner,{...input,periodStart:'2026-10-01',periodEnd:'2026-10-31',payDate:'2026-11-02'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 expect(await createPayrollRun(f.owner,input,createKey)).toEqual(run);
 const cancelKey=key(),cancelInput=payrollCancelSchema.parse({expectedVersion:run.version,reason:'Normal Payroll addon draft cancellation'}),cancelled=await cancelPayrollRun(f.owner,String(run.id),cancelInput,cancelKey);expect(await cancelPayrollRun(f.owner,String(run.id),cancelInput,cancelKey)).toEqual(cancelled);
 await savePayrollSchedule(f.owner,group.id,payrollScheduleSchema.parse({expectedVersion:schedule.version,enabled:false,payDelayDays:1}),key());expect((await payrollAddonObservation()).total).toBe(0);
 const retained=await db.prepare(`SELECT (SELECT to_jsonb(e) FROM finance_expenses e WHERE id=?) expense,(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE e.company_id=?) compensation`).bind(expense.id,f.company).first(),request=await payrollAddonWithdrawal(),preview=await payrollAddonReview(request);expect(await payrollAddonReview(request,true)).toEqual(preview);expect(await payrollAddonReview(request,true)).toEqual(preview);
 expect((await readCompanyPackageGrants(db,f.company)).filter(row=>['people','people-expenses'].includes(row.packageId)).map(row=>row.state)).toEqual(['active','active']);expect(await cancelPayrollRun(f.owner,String(run.id),cancelInput,cancelKey)).toEqual(cancelled);
 expect(await db.prepare(`SELECT (SELECT to_jsonb(e) FROM finance_expenses e WHERE id=?) expense,(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE e.company_id=?) compensation`).bind(expense.id,f.company).first()).toEqual(retained);
});
it('leases absent Payroll addon rows and rejects a diagnostic deferred schedule duty atomically before withdrawal seal',async()=>{
 const {createPayGroup,payGroupSchema}=await import('../../../src/server/modules/people_payroll/public/people-payroll'),{withOwnerPackageObligations,ownerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),{peopleAddonPackageObligations}=await import('../../../src/server/modules/people_payroll/public/addon-package-obligations');
 await payrollAddonGrant('people');await payrollAddonGrant('people-payroll');f.owner.role='payroll_preparer';f.owner.assignmentId=await f.grant(f.owner,'payroll_preparer');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const group=await createPayGroup(f.owner,payGroupSchema.parse({name:'Actual normal group for final-source fault',countryCode:'TH',currency:'THB',frequency:'monthly'}),key());
 await expect(db.transaction(tx=>peopleAddonPackageObligations(db,tx,{...payrollAddonScope(),moduleId:'people_payroll',now:Date.now()}))).rejects.toThrow('PEOPLE_ADDON_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 await withOwnerPackageObligations(db,payrollAddonScope(),async tx=>{await expect(peopleAddonPackageObligations(db,tx,{...payrollAddonScope(),companyId:'other-company',moduleId:'people_payroll',now:Date.now()})).rejects.toThrow('PEOPLE_ADDON_PACKAGE_OBLIGATION_SOURCE_REQUIRED');for(const table of ['hr_payroll_runs','hr_payroll_deduction_consumptions','hr_payroll_statutory_remittances','finance_journal_lines'])await expect(db.transaction(async writer=>{await writer.prepare("SET LOCAL lock_timeout='50ms'").run();await writer.prepare(`LOCK TABLE ${table} IN ROW EXCLUSIVE MODE`).run();})).rejects.toMatchObject({code:'55P03'});});
 await payrollAddonGrant('people-payroll','draining');const request=await payrollAddonWithdrawal(),before=await payrollAddonImage(),name='addon_fault_'+key().replaceAll('-','');
 // Adversarial deferred owner-state mutation, not a normal schedule command or
 // Commercial withdrawal. The original normal schedule path is proved above.
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${f.company}' AND NEW.package_id='people-payroll' AND NEW.state='inactive' THEN UPDATE hr_pay_groups SET schedule_enabled=true,schedule_version=schedule_version+1 WHERE id='${group.id}'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER UPDATE ON company_package_grants DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${name}()`).run();
 try{const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-payroll-addon-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});await expect(withOwnerPackageObligations(db,payrollAddonScope(),tx=>store.batch.applyIn(tx,request,async()=>{}))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await payrollAddonImage()).toEqual(before);}finally{await db.prepare(`DROP TRIGGER ${name} ON company_package_grants`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 const preview=await payrollAddonReview(request);expect(await payrollAddonReview(request,true)).toEqual(preview);expect(await payrollAddonReview(request,true)).toEqual(preview);
});

// Declared original imported Payroll fixture and synthetic isolated bank rows:
// no claim of normal Company/account/money-capture creation or real bank data.
it("keeps the Payroll addon blocked through actual bank and statutory settlement when an imported salary lacks its immutable money capture",async()=>{
  const financeOwner={...f.owner};
  vi.useFakeTimers({toFake:["Date"]});vi.setSystemTime(new Date("2026-10-02T00:00:00Z"));
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  f.manager.role="payroll_approver";f.manager.assignmentId=await f.grant(f.manager,"payroll_approver");
  f.reviewer.role="finance_approver";f.reviewer.assignmentId=await f.grant(f.reviewer,"finance_approver");
  f.claimant.role="treasury";f.claimant.assignmentId=await f.grant(f.claimant,"treasury");
  await payrollAddonGrant("people");await payrollAddonGrant("people-payroll");vi.stubEnv("ERPX_PEOPLE_PACKAGE_ENFORCEMENT","enforced");
  const engagement=await db.prepare("SELECT e.id FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?").bind(f.company,f.claimant.userId).first<{id:string}>(),group=`paygroup-${key()}`,employer=`${f.company}-employer-${key()}`,employerExpense=`${f.company}-employer-expense-${key()}`,taxLiability=`${f.company}-payroll-tax-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(employer,f.chart,f.company,"EMPLOYER-${key()}","Employer contributions payable"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'expense','THB',1,1)").bind(employerExpense,f.chart,f.company,"EMPLOYER-EXPENSE-${key()}","Employer contribution expense"),
    db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'liability','THB',1,1)").bind(taxLiability,f.chart,f.company,"PAYROLL-TAX-${key()}","Payroll tax payable"),
    db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Posting payroll','TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,1,1)").bind(group,f.company,f.owner.userId),
    db.prepare("UPDATE hr_engagements SET pay_group_id=?,version=version+1 WHERE id=?").bind(group,engagement!.id),
    db.prepare("UPDATE hr_people SET legal_name='=PAYMENT_FORMULA()',preferred_name=NULL,version=version+1 WHERE id=(SELECT person_id FROM hr_engagements WHERE id=?)").bind(engagement!.id),
    db.prepare("INSERT INTO hr_compensation_revisions(id,engagement_id,effective_from,pay_frequency,base_amount_minor,hourly_rate_minor,expected_hours,currency,components_json,status,approved_by,created_by,created_at,updated_at) VALUES(?,?,'2026-09-01','monthly',100000,0,160,'THB',?,'approved',?,?,1,1)").bind(`comp-${key()}`,engagement!.id,JSON.stringify([{code:"TAX",label:"Payroll deduction",kind:"deduction",amount:"10.00",taxable:false},{code:"SOCIAL",label:"Employer contribution",kind:"employer",amount:"5.00",taxable:false}]),f.manager.userId,f.reviewer.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','calculate',?,?,'Posting payroll fixture',1)").bind(f.company,f.owner.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','preview',?,?,'Posting payroll submission fixture',1)").bind(f.company,f.owner.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','in_review',?,?,'Posting payroll approval fixture',1)").bind(f.company,f.manager.userId,f.owner.userId),
    db.prepare("INSERT INTO flow_defaults(company_id,kind,step_key,primary_user_id,created_by,reason,created_at) VALUES(?,'payroll','approved',?,?,'Finance payroll posting fixture',1)").bind(f.company,f.reviewer.userId,f.owner.userId),
  ]);
  const book=await preparePayrollProjection(f,financeOwner,f.reviewer);
  const created=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-30",payDate:"2026-10-01",currency:"THB"}),key());
  await payrollAddonGrant("people-payroll","draining");expect(await payrollPurchaseControls(f.owner)).toEqual({newWork:false,settleExisting:true,history:true,revision:expect.any(String)});
  const minimalActor={userId:financeOwner.userId,assignmentId:financeOwner.assignmentId,role:financeOwner.role,companyId:financeOwner.companyId,branchId:financeOwner.branchId,departmentId:financeOwner.departmentId,authority:financeOwner.authority,iam:financeOwner.iam},flow=await resolveFlow(db,minimalActor,"payroll",String(created.id)),routeKey=key(),routeInput={stepKey:"calculate",userId:f.owner.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:"Existing Company authority retains the independent Payroll flow through its original Holding database",saveDefault:false};
  const routed=await assignFlowPrimary(db,minimalActor,"payroll",String(created.id),routeInput,routeKey);expect(await assignFlowPrimary(db,minimalActor,"payroll",String(created.id),routeInput,routeKey)).toEqual(routed);
  const calculated=await calculatePayrollRun(f.owner,String(created.id),key());
  expect(calculated.totals).toMatchObject({gross:100000,deductions:1000,employer:500,net:99000});
  const submitted=await submitRun(String(created.id),Number(calculated.version));
  const approved=await approvePayrollRun(f.manager,String(created.id),Number(submitted.version),key());
  const mappingId=`payroll-mapping-${key()}`;
  await db.prepare("INSERT INTO hr_payroll_account_mappings(id,company_id,pay_group_id,chart_id,accounts_json,status,version,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,'approved',2,?,?,1,1)")
    .bind(mappingId,f.company,group,f.chart,JSON.stringify({chartId:f.chart,expenseAccountId:f.accounts.expense,employerExpenseAccountId:employerExpense,netPayableAccountId:f.accounts.payable,taxPayableAccountId:taxLiability,employerContributionAccountId:employer}),f.owner.userId,f.reviewer.userId).run();
  const postInput=payrollPostSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-30",expenseAccountId:f.accounts.expense,employerExpenseAccountId:employerExpense,netPayableAccountId:f.accounts.payable,taxPayableAccountId:taxLiability,employerContributionAccountId:employer,expectedVersion:approved.version});
  const postKey=key(),posted=await postPayroll(f.reviewer,String(created.id),postInput,postKey);
  expect(posted).toMatchObject({status:"posted",journalId:expect.any(String),netPayableMinor:99000});
  expect(await db.prepare("SELECT status,posted_by,version FROM hr_payroll_runs WHERE id=?").bind(created.id).first()).toMatchObject({status:"posted",posted_by:f.reviewer.userId,version:Number(approved.version)+1});
  expect(await db.prepare("SELECT status FROM hr_payroll_items WHERE run_id=?").bind(created.id).first("status")).toBe("posted");
  const journal=await db.prepare("SELECT id,status,source_type FROM finance_journals WHERE id=?").bind(posted.journalId).first();expect(journal).toMatchObject({status:"posted",source_type:"payroll"});
  expect(await db.prepare("SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?").bind(posted.journalId).first()).toEqual({debit:100500,credit:100500});
  const peopleObservation=payrollAddonObservation;expect((await peopleObservation()).observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(4);
  const liabilities=await getPayrollLiabilities(f.claimant,String(created.id));
  expect(liabilities.obligations).toEqual(expect.arrayContaining([
    expect.objectContaining({kind:"withholding_tax",accruedMinor:1000,outstandingMinor:1000}),
    expect.objectContaining({kind:"social_security",accruedMinor:500,outstandingMinor:500}),
  ]));
  const remittanceStatement=`statutory-statement-${key()}`,taxTransaction=`statutory-tax-${key()}`,ssoTransaction=`statutory-sso-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,0,-1500,?,?,1)").bind(remittanceStatement,f.company,f.accounts.cash,`STATUTORY-${key()}`,"2026-10-01",key(),f.owner.userId),
    db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,'2026-10-01','TAX-REMIT-001',-1000,'THB')").bind(taxTransaction,remittanceStatement,f.company),
    db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,'2026-10-01','SSO-REMIT-001',-500,'THB')").bind(ssoTransaction,remittanceStatement,f.company),
  ]);
  const taxInput=payrollRemittanceSchema.parse({kind:"withholding_tax",amountMinor:1000,expectedRunVersion:Number(posted.version),bankAccountId:f.accounts.cash,bankTransactionId:taxTransaction,bankVersion:1,bankReference:"TAX-REMIT-001"});
  const taxKey=key(),taxRemitted=await remitPayrollLiability(f.claimant,String(created.id),taxInput,taxKey);
  expect(taxRemitted).toMatchObject({amountMinor:1000,remainingMinor:0});
  expect(await remitPayrollLiability(f.claimant,String(created.id),taxInput,taxKey)).toEqual(taxRemitted);
  await expect(remitPayrollLiability(f.claimant,String(created.id),taxInput,key())).rejects.toMatchObject({code:"PAYROLL_LIABILITY_EXCEEDED"});
  const ssoRemitted=await remitPayrollLiability(f.claimant,String(created.id),payrollRemittanceSchema.parse({kind:"social_security",amountMinor:500,expectedRunVersion:Number(posted.version),bankAccountId:f.accounts.cash,bankTransactionId:ssoTransaction,bankVersion:1,bankReference:"SSO-REMIT-001"}),key());
  expect(ssoRemitted).toMatchObject({amountMinor:500,remainingMinor:0});
  expect((await getPayrollLiabilities(f.claimant,String(created.id))).obligations.map(row=>row.outstandingMinor)).toEqual([0,0]);
  expect(await db.prepare("SELECT COUNT(*) AS count FROM finance_journals WHERE source_type='payroll_remittance' AND company_id=?").bind(f.company).first()).toEqual({count:2});
  expect((await resolveFlow(db,f.reviewer,"payroll",String(created.id))).current?.key).toBe("posted");
  expect(await postPayroll(f.reviewer,String(created.id),postInput,postKey)).toEqual(posted);
  await expect(postPayroll(f.reviewer,String(created.id),postInput,`replay-${key()}`)).rejects.toMatchObject({code:"PAYROLL_NOT_APPROVED"});
  await db.prepare("UPDATE hr_people SET legal_name='Changed after posting',version=version+1 WHERE id=(SELECT person_id FROM hr_engagements WHERE id=?)").bind(engagement!.id).run();
  const batch=await createPaymentBatch(f.reviewer,{sourceType:"payroll",sourceId:String(created.id),formatCode:"generic-csv",bankAccountId:f.accounts.cash},key());
  expect(batch).toMatchObject({status:"draft",totalMinor:99000,itemCount:1});
  const loaded=await getPayrollRun(f.reviewer,String(created.id));
  expect(loaded.paymentBatch).toMatchObject({id:batch.id,status:"draft",source_type:"payroll"});
  expect(loaded.paymentItems).toHaveLength(1);
  const treasuryView=await getPayrollRun(f.claimant,String(created.id));
  expect(treasuryView.paymentBatch).toMatchObject({id:batch.id,status:"draft"});
  expect(treasuryView.items).toHaveLength(0);
  expect((await resolveFlow(db,f.reviewer,"payroll",String(created.id))).current?.key).toBe("payment_draft");
  const approvedBatch=await approvePaymentBatch(f.claimant,String(batch.id),1,key());
  expect(approvedBatch).toMatchObject({status:"approved",version:2});
  const exportedBatch=await exportPaymentBatch(f.claimant,String(batch.id),key());
  expect(exportedBatch).toMatchObject({status:"exported",itemCount:1,checksum:expect.any(String)});
  expect(await (await getPaymentFile(f.claimant,String(batch.id))).object.text()).toContain("\"'=PAYMENT_FORMULA()\"");
  const item=await db.prepare("SELECT reference,amount_minor FROM hr_payment_batch_items WHERE batch_id=?").bind(batch.id).first<{reference:string;amount_minor:number}>();
  const failed=await confirmPaymentBatch(f.claimant,String(batch.id),{rows:[{reference:item!.reference,status:"failed",confirmationRef:"SYNTHETIC-REJECTED"}]},key());
  expect(failed).toMatchObject({status:"partially_settled",confirmed:0,failed:1});
  expect(await db.prepare("SELECT COUNT(*) AS count FROM hr_payroll_payment_settlements WHERE batch_id=?").bind(batch.id).first()).toEqual({count:0});
  expect(await db.prepare("SELECT status FROM hr_payment_batch_items WHERE batch_id=?").bind(batch.id).first("status")).toBe("failed");
  const retryVersion=Number(await db.prepare("SELECT version FROM hr_payment_batches WHERE id=?").bind(batch.id).first("version"));
  const retry=await preparePaymentRetry(f.reviewer,String(batch.id),retryVersion,key());
  expect(retry).toMatchObject({status:"draft",retryItems:1});
  await expect(approvePaymentBatch(f.reviewer,String(batch.id),Number(retry.version),key())).rejects.toMatchObject({code:"SELF_REVIEW_FORBIDDEN"});
  const approvedRetry=await approvePaymentBatch(f.claimant,String(batch.id),Number(retry.version),key());
  const retryExport=await exportPaymentBatch(f.claimant,String(batch.id),key());
  expect(retryExport).toMatchObject({status:"exported",itemCount:1,totalMinor:99000});
  expect(retryExport.fileKey).not.toBe(exportedBatch.fileKey);
  expect(await db.prepare("SELECT COUNT(*) AS count FROM hr_payment_batch_exports WHERE batch_id=?").bind(batch.id).first()).toEqual({count:2});
  expect(approvedRetry.version).toBe(Number(retry.version)+1);
  const statementId=`payroll-statement-${key()}`,transactionId=`payroll-bank-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,0,?,?,?,1)").bind(statementId,f.company,f.accounts.cash,`BANK-${key()}`,"2026-10-01",-item!.amount_minor,key(),f.owner.userId),
    db.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,amount_minor,currency) VALUES(?,?,?,'2026-10-01','BANK-PAYROLL-001',?,'THB')").bind(transactionId,statementId,f.company,-item!.amount_minor),
  ]);
  const settled=await confirmPaymentBatch(f.claimant,String(batch.id),{rows:[{reference:item!.reference,status:"confirmed",confirmationRef:"BANK-PAYROLL-001",bankTransactionId:transactionId,bankVersion:1}]},key());
  expect(settled).toMatchObject({status:"settled",confirmed:1});
  expect(await db.prepare("SELECT COUNT(*) AS count FROM hr_payroll_payment_settlements WHERE batch_id=?").bind(batch.id).first()).toEqual({count:1});
  expect(await db.prepare("SELECT debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=(SELECT journal_id FROM hr_payroll_payment_settlements WHERE batch_id=?) ORDER BY ordinal").bind(batch.id).all()).toMatchObject({results:[{debit_minor:99000,credit_minor:0},{debit_minor:0,credit_minor:99000}]});
  const paymentFile=await getPaymentFile(f.claimant,String(batch.id));
  expect(await paymentFile.object.text()).toContain(item!.reference);
  expect(await db.prepare("SELECT status FROM hr_payment_batches WHERE id=?").bind(batch.id).first("status")).toBe("settled");
  const finalFlow=await resolveFlow(db,f.reviewer,"payroll",String(created.id)); expect(finalFlow.current).toBeNull(); expect(finalFlow.blocker).toBeNull();
  const ownedCash=(await db.prepare("SELECT journal_id FROM hr_payroll_statutory_remittances WHERE company_id=? UNION ALL SELECT journal_id FROM hr_payroll_payment_settlements WHERE company_id=?").bind(f.company,f.company).all<{journal_id:string}>()).results;
  expect(ownedCash).toHaveLength(3);
  await assertOwnedCashCannotMatchAgain(db,financeOwner,ownedCash.map(row=>row.journal_id),f.accounts.cash);
  await assertPayrollFinanceHistory(f,financeOwner,f.owner,String(created.id),[String(posted.journalId),...ownedCash.map(row=>row.journal_id)],book);
  // The actual net+two statutory duties are discharged. This imported salary
  // lacks its original immutable money capture and must stay conservative.
  expect((await peopleObservation()).observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(1);const request=await payrollAddonWithdrawal(),before=await payrollAddonImage();await expect(payrollAddonReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(payrollAddonReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await payrollAddonImage()).toEqual(before);expect((await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='people')!.state).toBe('active');
});


// Imported Organization/worker/chart/period/reference-rate fixture only. Every
// new account, compensation, review, mapping, run, bank observation and payment
// below uses its normal owner command. This is not a zero-reference startup or
// statutory calculation/certification proof, and no business SQL creates money.
async function payrollOtherNativeFixture(purchased=true){
 const api=await import('../../../src/server/modules/people_payroll/public/payroll-other-remittance'),people=await import('../../../src/server/modules/people_payroll/public/people-payroll'),finance=await import('../../../src/server/modules/finance/public/finance'),administrator={...f.owner};
 // Commercial retained history uses PostgreSQL wall time; keep the owner clock real.
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');
 const account=async(name:string,category:'asset'|'liability'|'expense',bank=false)=>String((await finance.createAccount(administrator,finance.createAccountSchema.parse({chartId:f.chart,code:key(),name,category,currency:'THB',controlKind:bank?'bank':'none',isControl:bank}),key())).id);
 const accounts={bank:await account('Actual Payroll bank','asset',true),benefit:await account('Benefit obligation','liability'),deduction:await account('Other deduction obligation','liability'),employer:await account('Other employer obligation','liability'),employerExpense:await account('Employer cost expense','expense'),tax:await account('Payroll tax obligation','liability'),social:await account('Social-security obligation','liability')};
 f.owner.role='payroll_preparer';f.owner.assignmentId=await f.grant(f.owner,'payroll_preparer');f.manager.role='payroll_approver';f.manager.assignmentId=await f.grant(f.manager,'payroll_approver');f.reviewer.role='finance_approver';f.reviewer.assignmentId=await f.grant(f.reviewer,'finance_approver');f.claimant.role='treasury';f.claimant.assignmentId=await f.grant(f.claimant,'treasury');f.it.role='hr_admin';f.it.assignmentId=await f.grant(f.it,'hr_admin');
 await payrollAddonGrant('people');if(purchased)await payrollAddonGrant('people-payroll');
 const group=await people.createPayGroup(f.owner,people.payGroupSchema.parse({name:'Actual source-bound other Payroll duties',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),key()),engagement=await db.prepare('SELECT e.id,e.version FROM hr_engagements e JOIN hr_people p ON p.id=e.person_id WHERE e.company_id=? AND p.user_id=?').bind(f.company,f.claimant.userId).first<{id:string;version:number}>();
 if(!engagement)throw Error('Actual imported employee engagement required');
 await people.assignPayGroup(f.it,engagement.id,people.assignPayGroupSchema.parse({payGroupId:String(group.id),expectedVersion:engagement.version}),key());
 const cost=(await db.prepare("SELECT id,version,evidence_status FROM hr_payroll_cost_assignments WHERE engagement_id=? AND status='active'").bind(engagement.id).all<{id:string;version:number;evidence_status:string}>()).results;expect(cost).not.toHaveLength(0);for(const assignment of cost)if(assignment.evidence_status!=='verified')await verifyPayrollCostAssignment(f.it,assignment.id,{expectedVersion:assignment.version,evidenceRef:'Actual reviewed native employee placement and dated roster'},key());
 const compensation=await people.createCompensation(f.it,people.compensationSchema.parse({engagementId:engagement.id,effectiveFrom:'2026-09-01',payFrequency:'monthly',baseAmount:'1000',currency:'THB',components:[{code:'BENEFIT_EMPLOYEE',label:'Reviewed employee benefit deduction',kind:'deduction',amount:'10'},{code:'OTHER_DEDUCTION',label:'Reviewed other deduction',kind:'deduction',amount:'7'},{code:'BENEFIT_EMPLOYER',label:'Reviewed employer benefit',kind:'employer',amount:'2'},{code:'OTHER_EMPLOYER',label:'Reviewed employer other amount',kind:'employer',amount:'3'},{code:'TAX',label:'Original explicit tax fixture amount',kind:'deduction',amount:'10'},{code:'SOCIAL',label:'Original explicit social fixture amount',kind:'employer',amount:'5'}]}),key());
 await people.transitionCompensation(f.manager,String(compensation.id),{status:'approved',expectedVersion:1},key());
 const run=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:String(group.id),periodStart:'2026-09-01',periodEnd:'2026-09-30',payDate:'2026-10-01',currency:'THB'}),key()),runId=String(run.id);
 const assign=async(stepKey:string,userId:string)=>{const flow=await resolveFlow(db,administrator,'payroll',runId);return assignFlowPrimary(db,administrator,'payroll',runId,{stepKey,userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:'Original Company administrator assigns the actual eligible independent Payroll owner',saveDefault:false},key());};
 for(const [step,user] of [['calculate',f.owner.userId],['preview',f.owner.userId],['in_review',f.manager.userId],['approved',f.reviewer.userId]] as const)await assign(step,user);
 const calculated=await calculatePayrollRun(f.owner,runId,key());expect(calculated.totals).toMatchObject({gross:100000,deductions:2700,employer:1000,net:97300});const review=await exportPayrollReview(f.owner,runId,key()),reviewBytes=await (await getPayrollReviewFile(f.owner,runId,Number(review.revision))).object.text(),submitted=await submitRun(runId,Number(calculated.version)),approved=await approvePayrollRun(f.manager,runId,Number(submitted.version),key());
 const mapping=await people.savePayrollAccountMapping(administrator,String(group.id),people.payrollAccountMappingSchema.parse({chartId:f.chart,expenseAccountId:f.accounts.expense,employerExpenseAccountId:accounts.employerExpense,netPayableAccountId:f.accounts.payable,taxPayableAccountId:accounts.tax,socialSecurityPayableAccountId:accounts.social,employerContributionAccountId:accounts.employer,benefitPayableAccountId:accounts.benefit,otherDeductionPayableAccountId:accounts.deduction}),key());
 await people.approvePayrollAccountMapping(f.reviewer,String(mapping.id),1,key());const postInput=payrollPostSchema.parse({accountMappingId:mapping.id,periodId:f.period,entryDate:'2026-09-30',expectedVersion:approved.version}),postKey=key(),posted=await postPayroll(f.reviewer,runId,postInput,postKey);
 expect(await postPayroll(f.reviewer,runId,postInput,postKey)).toEqual(posted);expect(await db.prepare('SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(posted.journalId).first()).toEqual({debit:101000,credit:101000});
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(purchased?['people','people-payroll']:['people']);
 return {api,finance,administrator,accounts,runId,posted,postInput,postKey,review,reviewBytes,assign};
}
async function payrollOtherNativeImage(){return db.prepare(`SELECT
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM hr_payroll_runs r WHERE r.company_id=?) runs,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM hr_payroll_statutory_remittances r WHERE r.company_id=?) remittances,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM finance_journals r WHERE r.company_id=?) journals,
 (SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=?) lines,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM finance_bank_statements r WHERE r.company_id=?) statements,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM finance_bank_transactions r WHERE r.company_id=?) banks,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.source_type,r.source_id,r.amount_kind) FROM money_transaction_snapshots r WHERE r.company_id=?) money,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM finance_charts r WHERE r.company_id=?) charts,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM finance_accounts r WHERE r.company_id=?) accounts,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM assignments r WHERE r.company_id=?) authority,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.package_id) FROM company_package_grants r WHERE r.company_id=?) grants,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.command_id) FROM command_receipts r WHERE r.authority_json::jsonb->>'companyId'=?) receipts,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM audit_events r WHERE r.authority_json::jsonb->>'companyId'=?) audit,
 (SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM outbox_events r JOIN command_receipts c ON c.command_id=r.command_id WHERE c.authority_json::jsonb->>'companyId'=?) outbox
 `).bind(...Array.from({length:14},()=>f.company)).first();}
async function payrollOtherImportBody(s:Awaited<ReturnType<typeof payrollOtherNativeFixture>>,kind:'benefit'|'deduction_other'|'employer_other',amountMinor:number,bankAccount=s.accounts.bank){const view=await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId),due=view.obligations.find(row=>row.kind===kind);if(!due||!view.contextKey)throw Error('Actual selected accrued account and opaque current context required');const amount=`-${(amountMinor/100).toFixed(2)}`;return s.api.payrollOtherBankEvidenceSchema.parse({kind,liabilityAccountId:due.accountId,expectedRunVersion:view.runVersion,contextKey:view.contextKey,reason:'Reviewed actual source-bound bank observation for this accrued Payroll account',bankAccount,statementNumber:key(),statementDate:'2026-10-01',opening:'0',closing:amount,sourceHash:key(),rows:[{externalId:key(),transactionDate:'2026-10-01',description:'Fictional actual owner bank observation, not a live bank import',amount,currency:view.currency}]});}
async function payrollOtherRemitBody(s:Awaited<ReturnType<typeof payrollOtherNativeFixture>>,bankId:string){const view=await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId),bank=view.bankEvidence.find(row=>row.id===bankId);if(!bank||!view.contextKey)throw Error('Actual retained Payroll bank debit required');return s.api.payrollOtherRemittanceSchema.parse({kind:bank.kind,liabilityAccountId:bank.liabilityAccountId,expectedRunVersion:view.runVersion,contextKey:view.contextKey,reason:'Treasury settles the exact independently approved Payroll accrued account',amountMinor:-bank.amountMinor,bankAccountId:bank.bankAccountId,bankTransactionId:bank.id,bankVersion:bank.version,bankReference:bank.reference});}

it('settles all three actual nonstatutory Payroll accounts while preserving original statutory services and closes mistaken scoped bank evidence without a fake payment',async()=>{
 const s=await payrollOtherNativeFixture();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const original=await getPayrollLiabilities(f.reviewer,s.runId);expect(original.obligations.map(row=>[row.kind,row.accruedMinor])).toEqual([['withholding_tax',1000],['social_security',500]]);
 let view=await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId);expect(view.obligations.map(row=>[row.kind,row.accruedMinor,row.outstandingMinor]).sort()).toEqual([['benefit',1200,1200],['deduction_other',700,700],['employer_other',300,300]]);expect(view.bankAccounts.map(row=>row.id)).toContain(s.accounts.bank);expect(view.bankAccounts.map(row=>row.id)).not.toContain(f.accounts.cash);
 const invalid=await payrollOtherImportBody(s,'benefit',100,f.accounts.cash),invalidKey=key(),invalidBefore=await payrollOtherNativeImage();await expect(s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,invalid,invalidKey)).rejects.toMatchObject({code:'BANK_ACCOUNT_INVALID'});expect(await payrollOtherNativeImage()).toEqual(invalidBefore);
 const mistakenInput=await payrollOtherImportBody(s,'benefit',200),mistakenKey=key(),mistaken=await s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,mistakenInput,mistakenKey);view=await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId);const mistakenRow=view.bankEvidence.find(row=>row.id===mistaken.bankTransactionId)!;expect(mistakenRow).toMatchObject({matchStatus:'unmatched',version:1,amountMinor:-200,canIgnore:true});
 await payrollAddonGrant('people-payroll','draining');const beforeIgnore=await payrollOtherNativeImage(),ignoreBody=s.api.payrollOtherBankEvidenceIgnoreSchema.parse({kind:mistakenRow.kind,liabilityAccountId:mistakenRow.liabilityAccountId,expectedRunVersion:view.runVersion,contextKey:view.contextKey,bankTransactionId:mistakenRow.id,bankVersion:mistakenRow.version,reason:'This imported observation was selected in error; no Payroll payment was made'}),ignoreKey=key(),ignored=await s.api.ignorePayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,ignoreBody,ignoreKey);expect(ignored).toEqual({id:mistakenRow.id,status:'ignored',version:2,payrollRunId:s.runId,kind:'benefit',liabilityAccountId:s.accounts.benefit});
 view=await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId);expect(view.bankEvidence.find(row=>row.id===mistakenRow.id)).toMatchObject({matchStatus:'ignored',version:2,canIgnore:false,canRemit:false});expect(view.obligations.find(row=>row.kind==='benefit')).toMatchObject({outstandingMinor:1200,remittedMinor:0});expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE company_id=? AND source_type='payroll_remittance'").bind(f.company).first('n')).toBe(0);expect(await db.prepare("SELECT COUNT(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='payroll_other_remittance'").bind(f.company).first('n')).toBe(0);expect(await payrollOtherNativeImage()).not.toEqual(beforeIgnore);const ignoredState=await payrollOtherNativeImage();expect(await s.api.ignorePayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,ignoreBody,ignoreKey)).toEqual(ignored);expect(await payrollOtherNativeImage()).toEqual(ignoredState);
 const settled=[];for(const [kind,amount] of [['benefit',500],['benefit',700],['deduction_other',700],['employer_other',300]] as const){const input=await payrollOtherImportBody(s,kind,amount),importKey=key(),imported=await s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,input,importKey);expect(await s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,input,importKey)).toEqual(imported);const body=await payrollOtherRemitBody(s,imported.bankTransactionId),command=key(),result=await s.api.remitPayrollOtherLiability(f.reviewer,s.runId,body,command);settled.push({command,result,body});expect(result).toMatchObject({id:`payroll-remit-${command}`,runId:s.runId,kind,liabilityAccountId:body.liabilityAccountId,amountMinor:amount});const stable=await payrollOtherNativeImage();expect(await s.api.remitPayrollOtherLiability(f.reviewer,s.runId,body,command)).toEqual(result);expect(await payrollOtherNativeImage()).toEqual(stable);await expect(s.api.remitPayrollOtherLiability(f.reviewer,s.runId,{...body,reason:'Changed original reason'},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});}
 expect((await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId)).obligations.map(row=>row.outstandingMinor)).toEqual([0,0,0]);expect(await db.prepare('SELECT version FROM hr_payroll_runs WHERE id=?').bind(s.runId).first('version')).toBe(s.posted.version);expect(await db.prepare("SELECT COUNT(*) n,SUM(amount_units)::text total FROM money_transaction_snapshots WHERE company_id=? AND source_type='payroll_other_remittance'").bind(f.company).first()).toEqual({n:4,total:'2200'});
 // The same original Finance poster may pay: no new fourth-person rule exists.
 expect((await db.prepare('SELECT DISTINCT recorded_by FROM hr_payroll_statutory_remittances WHERE company_id=?').bind(f.company).all()).results).toEqual([{recorded_by:f.reviewer.userId}]);
 const matched=(await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId)).bankEvidence.find(row=>row.matchStatus==='matched')!,matchedBefore=await payrollOtherNativeImage();await expect(s.api.ignorePayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,{...ignoreBody,contextKey:(await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId)).contextKey,bankTransactionId:matched.id,bankVersion:matched.version},key())).rejects.toMatchObject({code:'PAYROLL_BANK_EVIDENCE_MISMATCH'});expect(await payrollOtherNativeImage()).toEqual(matchedBefore);
 // Unchanged old tax/social API, result and two-row reader use normal original
 // bank import with the ordinary Finance boundary deliberately disabled. This
 // is legacy compatibility, not a new scoped statutory bank-import entitlement.
 const statementKey=key(),oldBank=await s.finance.importBankStatement(f.reviewer,{bankAccount:s.accounts.bank,statementNumber:key(),statementDate:'2026-10-01',opening:'0',closing:'-988',sourceHash:key(),rows:[{externalId:'tax-'+key(),transactionDate:'2026-10-01',amount:'-10',currency:'THB'},{externalId:'sso-'+key(),transactionDate:'2026-10-01',amount:'-5',currency:'THB'},{externalId:'net-'+key(),transactionDate:'2026-10-01',amount:'-973',currency:'THB'}]},statementKey);expect(oldBank).toEqual({id:`statement-${statementKey}`,status:'imported',imported:3});
 const ordinaryBefore=await payrollOtherNativeImage(),ordinaryView=await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId);await expect(s.api.ignorePayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,{...ignoreBody,contextKey:ordinaryView.contextKey,bankTransactionId:`bankrow-${statementKey}-0`,bankVersion:1},key())).rejects.toMatchObject({code:'PAYROLL_BANK_EVIDENCE_MISMATCH'});expect(await payrollOtherNativeImage()).toEqual(ordinaryBefore);
 for(const [index,kind,amount] of [[0,'withholding_tax',1000],[1,'social_security',500]] as const){const bank=await db.prepare('SELECT external_id FROM finance_bank_transactions WHERE id=?').bind(`bankrow-${statementKey}-${index}`).first<string>('external_id'),body=payrollRemittanceSchema.parse({kind,amountMinor:amount,expectedRunVersion:s.posted.version,bankAccountId:s.accounts.bank,bankTransactionId:`bankrow-${statementKey}-${index}`,bankVersion:1,bankReference:bank}),command=key(),result=await remitPayrollLiability(f.reviewer,s.runId,body,command);expect(await remitPayrollLiability(f.reviewer,s.runId,body,command)).toEqual(result);expect(result).toMatchObject({kind,remainingMinor:0,amountMinor:amount});}
 expect((await getPayrollLiabilities(f.reviewer,s.runId)).obligations.map(row=>row.outstandingMinor)).toEqual([0,0]);
 // Every supported liability is discharged; the actual 97300 net-payable duty
 // remains. We do not falsify whole-Payroll withdrawal as complete.
 expect((await payrollAddonObservation()).observations.find(row=>row.moduleId==='finance')!.outstanding).toBe(1);const request=await payrollAddonWithdrawal(),beforeWithdrawal=await payrollOtherNativeImage();await expect(payrollAddonReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await payrollOtherNativeImage()).toEqual(beforeWithdrawal);
 for(const command of [mistakenKey,ignoreKey,settled[0]!.command])for(const action of ['delete','reclassify'] as const){const before=await payrollOtherNativeImage();await expect(db.transaction(tx=>action==='delete'?tx.prepare('DELETE FROM command_receipts WHERE command_id=?').bind(command).run():tx.prepare("UPDATE command_receipts SET operation='diagnostic.other-owner' WHERE command_id=?").bind(command).run())).rejects.toMatchObject({code:'23514',message:expect.stringContaining('PAYROLL_REMITTANCE_SOURCE_CHANGED')});expect(await payrollOtherNativeImage()).toEqual(before);}
 const resetBefore=await payrollOtherNativeImage();await expect(db.transaction(tx=>tx.prepare("UPDATE finance_bank_transactions SET match_status='unmatched',version=version+1 WHERE id=?").bind(mistakenRow.id).run())).rejects.toMatchObject({code:'23514',message:expect.stringContaining('PAYROLL_BANK_PROVENANCE_REQUIRED')});expect(await payrollOtherNativeImage()).toEqual(resetBefore);
 // Complete the original net salary duty through its unchanged reviewed
 // payment batch/export/actual bank confirmation before withdrawing anything.
 for(const [step,user] of [['posted',f.reviewer.userId],['payment_draft',f.claimant.userId],['payment_approved',f.claimant.userId],['payment_exported',f.claimant.userId]] as const)await s.assign(step,user);
 const batch=await createPaymentBatch(f.reviewer,{sourceType:'payroll',sourceId:s.runId,formatCode:'generic-csv',bankAccountId:s.accounts.bank},key());expect(batch).toMatchObject({status:'draft',totalMinor:97300,itemCount:1});await approvePaymentBatch(f.claimant,String(batch.id),1,key());const exported=await exportPaymentBatch(f.claimant,String(batch.id),key()),paymentFile=await getPaymentFile(f.claimant,String(batch.id)),paymentBytes=await paymentFile.object.text(),item=await db.prepare('SELECT reference,amount_minor FROM hr_payment_batch_items WHERE batch_id=?').bind(batch.id).first<{reference:string;amount_minor:number}>(),netReference=await db.prepare('SELECT external_id FROM finance_bank_transactions WHERE id=?').bind(`bankrow-${statementKey}-2`).first<string>('external_id');if(!item||!netReference)throw Error('Actual original net salary instruction and bank observation required');expect(item.amount_minor).toBe(97300);
 const confirmation={rows:[{reference:item.reference,status:'confirmed' as const,confirmationRef:netReference,bankTransactionId:`bankrow-${statementKey}-2`,bankVersion:1}]},confirmationKey=key(),paid=await confirmPaymentBatch(f.claimant,String(batch.id),confirmation,confirmationKey);expect(paid).toMatchObject({status:'settled',confirmed:1});expect(await confirmPaymentBatch(f.claimant,String(batch.id),confirmation,confirmationKey)).toEqual(paid);expect(paymentFile.checksum).toBe(exported.checksum);
 const zero=await payrollAddonObservation();expect(zero.observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);const withdraw=await payrollAddonWithdrawal(),preview=await payrollAddonReview(withdraw);expect(await payrollAddonReview(withdraw,true)).toEqual(preview);expect(await payrollAddonReview(withdraw,true)).toEqual(preview);expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([['people','active'],['people-payroll','inactive']]);
 const history=await payrollOtherNativeImage(),retained=await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId);expect(retained).toMatchObject({canRemit:false,canImportBankEvidence:false});expect(retained.obligations.map(row=>row.outstandingMinor)).toEqual([0,0,0]);expect(retained.bankEvidence.every(row=>!row.canRemit&&!row.canIgnore)).toBe(true);for(const original of settled)expect(await s.api.remitPayrollOtherLiability(f.reviewer,s.runId,original.body,original.command)).toEqual(original.result);expect(await s.api.ignorePayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,ignoreBody,ignoreKey)).toEqual(ignored);expect(await s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,mistakenInput,mistakenKey)).toEqual(mistaken);expect(await postPayroll(f.reviewer,s.runId,s.postInput,s.postKey)).toEqual(s.posted);expect(await (await getPaymentFile(f.claimant,String(batch.id))).object.text()).toBe(paymentBytes);expect(await (await getPayrollReviewFile(f.owner,s.runId,Number(s.review.revision))).object.text()).toBe(s.reviewBytes);expect(await payrollOtherNativeImage()).toEqual(history);expect((await payrollAddonObservation()).total).toBe(0);

});

it('requires actual Payroll purchase and source authority and rolls deferred remittance source faults back before exact retry',async()=>{
 const s=await payrollOtherNativeFixture(false),noPurchase=await payrollOtherNativeImage();await expect(s.api.getPayrollOtherLiabilities(f.reviewer,s.runId)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await payrollOtherNativeImage()).toEqual(noPurchase);await payrollAddonGrant('people-payroll');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const body=await payrollOtherImportBody(s,'benefit',500),importKey=key(),imported=await s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,body,importKey),remit=await payrollOtherRemitBody(s,imported.bankTransactionId),command=key(),before=await payrollOtherNativeImage(),fault='payroll_other_fault_'+key().replaceAll('-','');
 // Adverse deferred source mutation inside the actual money command, never a
 // positive Finance source or ordinary account change.
 await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');UPDATE finance_charts SET name=name||' deferred change',version=version+1 WHERE id='${f.chart}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(s.api.remitPayrollOtherLiability(f.reviewer,s.runId,remit,command)).rejects.toMatchObject({code:'PAYROLL_REMITTANCE_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(await payrollOtherNativeImage()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
 const result=await s.api.remitPayrollOtherLiability(f.reviewer,s.runId,remit,command);expect(result).toMatchObject({amountMinor:500,remainingMinor:700});const committed=await payrollOtherNativeImage();expect(await s.api.remitPayrollOtherLiability(f.reviewer,s.runId,remit,command)).toEqual(result);expect(await payrollOtherNativeImage()).toEqual(committed);
 const foreign=await expenseFixture(),foreignBefore=await payrollOtherNativeImage();foreign.reviewer.role='finance_approver';foreign.reviewer.assignmentId=await foreign.grant(foreign.reviewer,'finance_approver');await expect(s.api.getPayrollOtherLiabilities(foreign.reviewer,s.runId)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await payrollOtherNativeImage()).toEqual(foreignBefore);
 const version=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.reviewer.assignmentId).first<number>('version');await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.reviewer.assignmentId).run();const revoked=await payrollOtherNativeImage();try{await expect(s.api.remitPayrollOtherLiability(f.reviewer,s.runId,remit,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});expect(await payrollOtherNativeImage()).toEqual(revoked);}finally{await db.prepare('UPDATE assignments SET revoked_at=NULL,version=? WHERE id=?').bind(version,f.reviewer.assignmentId).run();}
 const unusedBody=await payrollOtherImportBody(s,'benefit',300),unused=await s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,unusedBody,key()),stolenBefore=await payrollOtherNativeImage();await expect(db.transaction(tx=>tx.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id='diagnostic-foreign-owner',version=version+1 WHERE id=?").bind(unused.bankTransactionId).run())).rejects.toMatchObject({code:'23514',message:expect.stringContaining('PAYROLL_BANK_PROVENANCE_REQUIRED')});expect(await payrollOtherNativeImage()).toEqual(stolenBefore);
});

async function payrollOtherHeldWriter<Result>(writer:(command:string)=>Promise<unknown>,command:(key:string)=>Promise<Result>){
 const writerKey=key(),newKey=key(),fault='payroll_other_writer_'+key().replaceAll('-',''),{measureReads}=await import('../../../src/server/modules/runtime/public/runtime/read-metrics');
 await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${fault} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${writerKey}') EXECUTE FUNCTION ${fault}()`);
 try{const observed=await measureReads(async()=>{let done=false;const writing=writer(writerKey).finally(()=>{done=true;}),outcome=Promise.allSettled([writing]);let held=false;for(let attempt=0;attempt<100;attempt++){held=!!await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called');if(held||done)break;await new Promise(resolve=>setTimeout(resolve,10));}if(!held){const [result]=await outcome;if(result.status==='rejected')throw result.reason;throw Error('Actual legacy writer must reach its held native receipt');}expect(done).toBe(false);const [written,current]=await Promise.all([outcome,Promise.allSettled([command(newKey)])]);return {written:written[0],current:current[0]};});expect(observed.result.written.status).toBe('fulfilled');expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});return {...observed.result,newKey,writerKey};}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);}
}
it('orders actual disabled generic bank import and original statutory remittance before strict Payroll source constructors without transaction retry masking',async()=>{
 const s=await payrollOtherNativeFixture();vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
 const input=await payrollOtherImportBody(s,'benefit',500),originalInput={bankAccount:s.accounts.bank,statementNumber:key(),statementDate:'2026-10-01',opening:'0',closing:'-10',sourceHash:key(),rows:[{externalId:key(),transactionDate:'2026-10-01',amount:'-10',currency:'THB'}]},importOverlap=await payrollOtherHeldWriter(command=>s.finance.importBankStatement(f.reviewer,originalInput,command),command=>s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,input,command));expect(importOverlap.current.status).toBe('fulfilled');
 const bankId=`bankrow-${importOverlap.writerKey}-0`,tax=payrollRemittanceSchema.parse({kind:'withholding_tax',amountMinor:1000,expectedRunVersion:s.posted.version,bankAccountId:s.accounts.bank,bankTransactionId:bankId,bankVersion:1,bankReference:originalInput.rows[0]!.externalId}),next=await payrollOtherImportBody(s,'deduction_other',700),remitOverlap=await payrollOtherHeldWriter(command=>remitPayrollLiability(f.reviewer,s.runId,tax,command),command=>s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,next,command));expect(remitOverlap.current.status).toBe('rejected');if(remitOverlap.current.status!=='rejected')throw Error('The original statutory writer changes the selected all-remittance context');expect(remitOverlap.current.reason).toMatchObject({code:'PAYROLL_REMITTANCE_CONTEXT_CHANGED'});expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(remitOverlap.newKey).first()).toBeNull();
 const refreshed=await payrollOtherImportBody(s,'deduction_other',700),imported=await s.api.importPayrollOtherLiabilityBankEvidence(f.reviewer,s.runId,refreshed,remitOverlap.newKey);expect(imported).toMatchObject({payrollRunId:s.runId,kind:'deduction_other'});expect((await getPayrollLiabilities(f.reviewer,s.runId)).obligations.map(row=>row.outstandingMinor)).toEqual([0,500]);expect(await remitPayrollLiability(f.reviewer,s.runId,tax,remitOverlap.writerKey)).toMatchObject({remainingMinor:0,amountMinor:1000});expect((await s.api.getPayrollOtherLiabilities(f.reviewer,s.runId)).obligations.map(row=>row.outstandingMinor)).toEqual([1200,700,300]);
});

// New final-pay closure uses real original HR/Payroll owners. Shared Company,
// worker and existing-role assignments are the original disposable fixture;
// no final-pay/run/task/group/stock/money positive is manufactured with SQL.
import {getFinalPayCancellation,cancelFinalPayPreparation,finalPayCancellationSchema} from '../../../src/server/modules/people_payroll/public/final-pay-cancellation';
async function normalFinalPayClosureSource(calculatedPreview=false){
 const {createPayGroup,payGroupSchema,assignPayGroup,assignPayGroupSchema,cancelPayrollRun}=await import('../../../src/server/modules/people_payroll/public/people-payroll'),{previewLifecycle,saveLifecycle,cancelLifecycle}=await import('../../../src/server/modules/people_payroll/public/people-lifecycle'),{lifecycleInput}=await import('../../../src/lib/people_payroll/people-lifecycle');
 await payrollAddonGrant('people');await payrollAddonGrant('people-payroll');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const hr={...f.owner},payroll={...f.owner,role:'payroll_preparer',assignmentId:await f.grant(f.owner,'payroll_preparer')},engagement=await db.prepare('SELECT id,person_id,version FROM hr_engagements WHERE company_id=? AND person_id=?').bind(f.company,`person-${f.claimant.userId}`).first<{id:string;person_id:string;version:number}>();expect(engagement).not.toBeNull();
 const group=await createPayGroup(payroll,payGroupSchema.parse({name:'Normal final-pay closure group',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),key());await assignPayGroup(hr,engagement!.id,assignPayGroupSchema.parse({expectedVersion:engagement!.version,payGroupId:String(group.id)}),key());
 const input=lifecycleInput.parse({kind:'offboarding',departureReason:'retirement',expectedVersion:0,personId:engagement!.person_id,engagementId:engagement!.id,positionId:null,lastWorkingDate:'2027-01-31',effectiveAt:Date.parse('2027-02-01T00:00:00+07:00'),reason:'Normal future retirement source before its later HR cancellation',clearanceOwners:{payroll:payroll.userId}}),preview=await previewLifecycle(db,hr,input),parent=await saveLifecycle(db,hr,input,key(),true,preview.token),task=await db.prepare("SELECT id,version,status,assignee_id FROM people_preparation_tasks WHERE case_id=? AND company_id=? AND clearance_type='payroll'").bind(parent.id,f.company).first<{id:string;version:number;status:string;assignee_id:string}>();expect(task).toMatchObject({status:'pending',assignee_id:payroll.userId});
 let compensationId:string|null=null;
 if(calculatedPreview){const people=await import('../../../src/server/modules/people_payroll/public/people-payroll'),reviewer={...f.manager,role:'payroll_approver',assignmentId:await f.grant(f.manager,'payroll_approver')},compensation=await people.createCompensation(hr,people.compensationSchema.parse({engagementId:engagement!.id,effectiveFrom:'2026-09-01',payFrequency:'monthly',baseAmount:'1000',currency:'THB',components:[]}),key());compensationId=String(compensation.id);await people.transitionCompensation(reviewer,compensationId,{status:'approved',expectedVersion:1},key());}
 const prepareKey=key(),prepareInput=finalPayPreparationSchema.parse({taskId:task!.id,expectedTaskVersion:task!.version,payGroupId:group.id,periodStart:'2026-09-01',periodEnd:'2026-09-15',payDate:'2026-09-15',currency:'THB',reason:'Prepare assigned final pay before cancellation of the original HR case'}),prepared=await prepareFinalPay(payroll,prepareInput,prepareKey),runId=String(prepared.runId);
 await expect(cancelPayrollRun(payroll,runId,{expectedVersion:1,reason:'The ordinary final-pay cancellation guard stays closed'},key())).rejects.toMatchObject({code:'PAYROLL_CANCELLATION_UNAVAILABLE'});
 if(calculatedPreview){const cost=(await db.prepare("SELECT id,version,evidence_status FROM hr_payroll_cost_assignments WHERE engagement_id=? AND status='active'").bind(engagement!.id).all<{id:string;version:number;evidence_status:string}>()).results;expect(cost).not.toHaveLength(0);for(const assignment of cost)if(assignment.evidence_status!=='verified')await verifyPayrollCostAssignment(hr,assignment.id,{expectedVersion:assignment.version,evidenceRef:'Actual dated native employee placement and roster reviewed before final-pay preview'},key());const flow=await resolveFlow(db,hr,'payroll',runId);await assignFlowPrimary(db,hr,'payroll',runId,{stepKey:'calculate',userId:payroll.userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:'Original Company administrator assigns the actual eligible Payroll preparer',saveDefault:false},key());const calculated=await calculatePayrollRun(payroll,runId,key());expect(calculated).toMatchObject({status:'preview',totals:{headcount:1,gross:50000,net:50000}});}
 const active=await getFinalPayCancellation(payroll,runId);expect(active).toMatchObject({canCancel:false,unavailableReason:'parent-active'});await expect(cancelFinalPayPreparation(payroll,runId,{expectedVersion:active.version,expectedRunVersion:active.runVersion,expectedTaskVersion:active.taskVersion,expectedCaseVersion:active.caseVersion,reason:'Active clearance cannot be silently discharged'},key())).rejects.toMatchObject({code:'FINAL_PAY_CANCELLATION_UNAVAILABLE'});
 // Original record-document binding advances the still-prepared source. The
 // later closure and its retained read must preserve this real version chain.
 const files=await seedAttachmentSet(db,payroll,{targetType:'payroll_final_pay',targetId:String(prepared.id),componentKind:'record',purposeKey:'final_pay_support',expectedTargetVersion:active.version}),objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(files.sourceId).first('object_key')),bytes='%PDF-1.7';await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:'application/pdf'}});await bindRecordAttachmentSet(db,payroll,files.setId,active.version,key());
 const cancelParentKey=key(),cancelledParent=await cancelLifecycle(db,hr,String(parent.id),Number(parent.version),cancelParentKey);expect(cancelledParent).toMatchObject({id:parent.id,status:'cancelled'});expect(await cancelLifecycle(db,hr,String(parent.id),Number(parent.version),cancelParentKey)).toEqual(cancelledParent);
 const view=await getFinalPayCancellation(payroll,runId);expect(view).toMatchObject({schemaAvailable:true,canCancel:true,version:active.version+1,runVersion:active.runVersion,taskVersion:task!.version+1,caseVersion:Number(parent.version)+1});
 return {hr,payroll,view,runId,prepared,files,objectKey,bytes,prepareKey,prepareInput,parent,compensationId};
}
async function finalPayClosureImage(runId:string,preparationId:string){return {original:await payrollAddonImage(),source:await db.prepare(`SELECT (SELECT to_jsonb(p) FROM hr_payroll_final_pay_preparations p WHERE p.id=?) preparation,(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM hr_payroll_final_pay_events event WHERE event.preparation_id=?) events,(SELECT to_jsonb(c) FROM people_lifecycle_cases c JOIN hr_payroll_final_pay_preparations p ON p.case_id=c.id WHERE p.id=?) lifecycle,(SELECT to_jsonb(t) FROM people_preparation_tasks t JOIN hr_payroll_final_pay_preparations p ON p.task_id=t.id WHERE p.id=?) task,(SELECT jsonb_agg(to_jsonb(j) ORDER BY j.id) FROM finance_journals j WHERE j.company_id=? AND j.source_type='payroll' AND j.source_id=?) journals,(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM hr_payroll_postings p WHERE p.run_id=?) postings,(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM hr_payment_batches p WHERE p.company_id=? AND p.source_type='payroll' AND p.source_id=?) payments,(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM hr_payroll_items i WHERE i.run_id=?) items`).bind(preparationId,preparationId,preparationId,preparationId,f.company,runId,runId,f.company,runId,runId).first()};}
it('closes only a normally cancelled-parent final-pay preparation atomically, preserves real File history and discharges its actual Payroll addon duty',async()=>{
 const source=await normalFinalPayClosureSource(),input=finalPayCancellationSchema.parse({expectedVersion:source.view.version,expectedRunVersion:source.view.runVersion,expectedTaskVersion:source.view.taskVersion,expectedCaseVersion:source.view.caseVersion,reason:'Close the stranded unreviewed preparation after normal HR cancellation'}),command=key(),before=await finalPayClosureImage(source.runId,String(source.prepared.id)),fault='final_pay_source_'+key().replaceAll('-','');
 expect((await payrollAddonObservation()).observations.find(row=>row.moduleId==='people_payroll')!.outstanding).toBeGreaterThan(0);await payrollAddonGrant('people-payroll','draining');
 // Adversarial late owner-image mutation, not a supported HR operation. Its
 // actual parent row keeps its lawful cancelled identity/status; finality must
 // still reject this within-unit source drift and roll every own effect back.
 const baseline=await finalPayClosureImage(source.runId,String(source.prepared.id));await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE people_lifecycle_cases SET updated_at=updated_at+1 WHERE id='${source.view.caseId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.event_type='people.payroll.final-pay.cancel' AND NEW.aggregate_id='${source.prepared.id}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(cancelFinalPayPreparation(source.payroll,source.runId,input,command)).rejects.toMatchObject({code:'FINAL_PAY_CANCELLATION_SOURCE_CHANGED'});expect(await finalPayClosureImage(source.runId,String(source.prepared.id))).toEqual(baseline);}finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}()`);}
 const result=await cancelFinalPayPreparation(source.payroll,source.runId,input,command);expect(result).toMatchObject({id:source.prepared.id,runId:source.runId,status:'cancelled',version:input.expectedVersion+1,runVersion:input.expectedRunVersion+1,taskVersion:input.expectedTaskVersion,caseVersion:input.expectedCaseVersion});expect(await cancelFinalPayPreparation(source.payroll,source.runId,input,command)).toEqual(result);expect(await getFinalPayCancellation(source.payroll,source.runId)).toMatchObject({status:'cancelled',runStatus:'cancelled',canCancel:false,unavailableReason:'already-cancelled'});
 expect((await payrollAddonObservation()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);const withdrawal=await payrollAddonWithdrawal(),review=await payrollAddonReview(withdrawal);expect(await payrollAddonReview(withdrawal,true)).toEqual(review);expect(await payrollAddonReview(withdrawal,true)).toEqual(review);expect(await cancelFinalPayPreparation(source.payroll,source.runId,input,command)).toEqual(result);expect(await(await attachmentContent(db,env.H001_FILES,source.payroll,source.files.linkId,'original')).object.text()).toBe(source.bytes);expect(await attachmentBindingState(db,source.files.setId)).toMatchObject({set_state:'bound',link_state:'bound'});
 expect(await prepareFinalPay(source.payroll,source.prepareInput,source.prepareKey)).toEqual(source.prepared);await expect(cancelFinalPayPreparation(source.payroll,source.runId,{...input,reason:'Changed request under the committed key'},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
 const after=await finalPayClosureImage(source.runId,String(source.prepared.id));for(const field of ['lifecycle','task','journals','postings','payments','items'] as const)expect(after.source?.[field]).toEqual(before.source?.[field]);
 await expect(db.prepare("UPDATE hr_payroll_final_pay_events SET evidence_json='{}' WHERE id=?").bind(result.cancellationId).run()).rejects.toThrow();await expect(db.prepare('DELETE FROM command_receipts WHERE command_id=?').bind(command).run()).rejects.toMatchObject({code:'23514'});
});
it('rolls back a final-pay closure when the actual existing-role native assignment expires during deferred effects',async()=>{
 const source=await normalFinalPayClosureSource(),assignment=key(),expires=Date.now()+8000;
 // Declared disposable authority fixture using the existing published role:
 // no new role, permission projection, installed grant or business fact.
 await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'payroll_preparer','[]',?,0,?,'holding')").bind(assignment,source.payroll.userId,f.company,expires).run();
 const actor={...source.payroll,assignmentId:assignment},input=finalPayCancellationSchema.parse({expectedVersion:source.view.version,expectedRunVersion:source.view.runVersion,expectedTaskVersion:source.view.taskVersion,expectedCaseVersion:source.view.caseVersion,reason:'A current native assignment is still required after deferred effects'}),command=key(),fault='final_pay_authority_'+key().replaceAll('-',''),before=await finalPayClosureImage(source.runId,String(source.prepared.id));
 // Sequence is an explicit test-only, nontransactional phase witness. It
 // proves that the real receipt/outbox/deferred stage was reached; it is not
 // source authority or a business row and is dropped in finally.
 await db.exec(`CREATE SEQUENCE ${fault}_hit;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}_hit'::regclass);PERFORM pg_sleep(GREATEST(0,(${expires}-floor(extract(epoch FROM clock_timestamp())*1000)::bigint)::numeric/1000)+0.1);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.event_type='people.payroll.final-pay.cancel' AND NEW.aggregate_id='${source.prepared.id}') EXECUTE FUNCTION ${fault}()`);
 try{await expect(cancelFinalPayPreparation(actor,source.runId,input,command)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}_hit`).first('is_called')).toBe(true);expect(await finalPayClosureImage(source.runId,String(source.prepared.id))).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}_hit`);}
 expect(await db.prepare('SELECT COUNT(*) n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(0);expect(await getFinalPayCancellation(source.payroll,source.runId)).toMatchObject({canCancel:true,status:'prepared'});
});

it('retains actual independently approved compensation and calculated final-pay item/money snapshots when its cancelled HR parent is closed',async()=>{
 const source=await normalFinalPayClosureSource(true),input=finalPayCancellationSchema.parse({expectedVersion:source.view.version,expectedRunVersion:source.view.runVersion,expectedTaskVersion:source.view.taskVersion,expectedCaseVersion:source.view.caseVersion,reason:'Close only the unreviewed calculated preparation of the normally cancelled HR case'}),command=key(),image=async()=>({closure:await finalPayClosureImage(source.runId,String(source.prepared.id)),compensation:await db.prepare('SELECT row_to_json(c)::text source FROM hr_compensation_revisions c WHERE c.id=?').bind(source.compensationId).first('source'),money:(await db.prepare("SELECT row_to_json(s)::text source FROM money_transaction_snapshots s WHERE s.company_id=? AND s.source_type='hr_compensation_revision' AND s.source_id=? ORDER BY s.amount_kind").bind(f.company,source.compensationId).all()).results});
 expect(source.view).toMatchObject({runStatus:'preview',canCancel:true});expect(await db.prepare('SELECT COUNT(*) n FROM hr_payroll_items WHERE run_id=?').bind(source.runId).first('n')).toBe(1);const before=await image();expect(before.money).not.toHaveLength(0);
 const result=await cancelFinalPayPreparation(source.payroll,source.runId,input,command);expect(await cancelFinalPayPreparation(source.payroll,source.runId,input,command)).toEqual(result);expect(await getFinalPayCancellation(source.payroll,source.runId)).toMatchObject({status:'cancelled',runStatus:'cancelled',canCancel:false});const after=await image();expect(after.compensation).toEqual(before.compensation);expect(after.money).toEqual(before.money);for(const field of ['lifecycle','task','journals','postings','payments','items'] as const)expect(after.closure.source?.[field]).toEqual(before.closure.source?.[field]);
 expect((await payrollAddonObservation()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);expect(await(await attachmentContent(db,env.H001_FILES,source.payroll,source.files.linkId,'original')).object.text()).toBe(source.bytes);expect(await prepareFinalPay(source.payroll,source.prepareInput,source.prepareKey)).toEqual(source.prepared);
});

// Original imported Organization/worker/assignment references, with the explicit
// zero-Finance fixture option. No Company chart/account/period/rate, mapping,
// journal, balance or Payroll economic result is seeded below. All Finance IDs
// used by commands come from their original closed constructor receipts.
async function zeroFinancePayrollPrerequisiteFixture(purchased=true){
 const otherCompanyId=f.company;f=await expenseFixture(false,false,false);
 const api=await import('../../../src/server/modules/finance/public/payroll-posting-prerequisites'),people=await import('../../../src/server/modules/people_payroll/public/people-payroll'),finance=await import('../../../src/server/modules/finance/public/finance'),administrator={...f.owner};
 const payroll={...f.owner,role:'payroll_preparer',assignmentId:await f.grant(f.owner,'payroll_preparer')},reviewer={...f.head,role:'payroll_approver',assignmentId:await f.grant(f.head,'payroll_approver')},financeReviewer={...f.manager};
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 if(purchased){await payrollAddonGrant('people');await payrollAddonGrant('people-payroll');}
 else vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');
 let group:Awaited<ReturnType<typeof people.createPayGroup>>;
 try{group=await people.createPayGroup(payroll,people.payGroupSchema.parse({name:'Actual zero-Finance Payroll source',countryCode:'TH',currency:'THB',frequency:'monthly',timezone:'Asia/Bangkok'}),key());}finally{vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');}
 const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','hr_payroll_account_mappings','finance_journals','money_transaction_snapshots'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(f.company).first<number>('n')));
 expect(await counts()).toEqual(Array(8).fill(0));
 async function current(){const view=await api.payrollPostingPrerequisites(administrator,{payGroupId:group.id});if(!view.available||!view.contextKey)throw Error('ACTUAL_PAYROLL_PREREQUISITE_CONTEXT_REQUIRED');return view;}
 async function envelope(){const view=await current();return {payGroupId:group.id,expectedVersion:0 as const,contextKey:view.contextKey!};}
 return {api,people,finance,administrator,payroll,reviewer,financeReviewer,group,counts,current,envelope,otherCompanyId};
}
async function payrollPrerequisiteNativeImage(){
 const tables=['finance_charts','finance_accounts','finance_periods','company_fx_rates','company_money_settings','company_currency_scales','commerce_settings','hr_pay_groups','hr_payroll_account_mappings','hr_payroll_runs','finance_journals','money_transaction_snapshots','assignments','company_package_grants','company_package_revisions'];
 return {owner:await payrollAddonImage(),rows:await Promise.all(tables.map(async table=>({table,rows:(await db.prepare(`SELECT to_jsonb(t)::text row FROM ${table} t WHERE company_id=? ORDER BY to_jsonb(t)::text`).bind(f.company).all()).results})))};
}
async function constructPayrollPrerequisites(s:Awaited<ReturnType<typeof zeroFinancePayrollPrerequisiteFixture>>){
 const view=await s.current();expect(view).toMatchObject({companyId:f.company,selection:{id:s.group.id,currency:'THB'},canCreateChart:true,canCreateAccount:true,canCreatePeriod:true,canConfirmRate:true,charts:[],accounts:[],periods:[],rates:[],unavailableReason:null});
 const chartKey=key(),chartInput=s.api.payrollPostingChartSchema.parse({...await s.envelope(),input:{name:'Actual closed Payroll chart',baseCurrency:'THB'}}),chart=await s.api.createPayrollPostingChart(s.administrator,chartInput,chartKey);expect(chart).toMatchObject({companyId:f.company,version:1,baseCurrency:'THB'});
 expect(await s.api.createPayrollPostingChart(s.administrator,chartInput,chartKey)).toEqual(chart);
 const account=async(code:string,name:string,category:'expense'|'liability')=>{const command=key(),input=s.api.payrollPostingAccountSchema.parse({...await s.envelope(),input:{chartId:chart.id,code,name,category,currency:'THB'}}),result=await s.api.createPayrollPostingAccount(s.administrator,input,command);expect(result).toMatchObject({companyId:f.company,chartId:chart.id,currency:'THB',version:1});return {result,input,command};};
 const expense=await account('5100','Actual Payroll expense','expense'),payable=await account('2100','Actual Payroll liability','liability'),periodKey=key(),periodInput=s.api.payrollPostingPeriodSchema.parse({...await s.envelope(),input:{name:'Actual closed Payroll period',startDate:'2026-09-01',endDate:'2026-09-30'}}),period=await s.api.createPayrollPostingPeriod(s.administrator,periodInput,periodKey);expect(period).toMatchObject({companyId:f.company,state:'open',version:0});
 const rateKey=key(),rateInput=s.api.payrollPostingRateSchema.parse({...await s.envelope(),input:{currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',note:'Actual isolated owner-confirmed Payroll reference quote'}}),rate=await s.api.confirmPayrollPostingRate(s.administrator,rateInput,rateKey);expect(rate).toMatchObject({currency:'THB',effectiveDate:'2026-09-01',currencyPerUsd:'36',confirmedAt:expect.any(Number)});expect(Object.keys(rate).sort()).toEqual(['confirmedAt','currency','currencyPerUsd','effectiveDate','id']);
 expect(await s.api.payrollPostingPrerequisiteConfirmation(s.administrator,{kind:'chart',request:chartInput},chartKey)).toEqual({kind:'chart',commandId:chartKey,payGroupId:s.group.id,id:chart.id,status:'active',version:1});
 expect(await s.api.payrollPostingPrerequisiteConfirmation(s.administrator,{kind:'period',request:periodInput},periodKey)).toEqual({kind:'period',commandId:periodKey,payGroupId:s.group.id,id:period.id,status:'open',version:0});
 expect(await s.api.payrollPostingPrerequisiteConfirmation(s.administrator,{kind:'rate',request:rateInput},rateKey)).toEqual({kind:'rate',commandId:rateKey,payGroupId:s.group.id,id:rate.id,status:'confirmed',version:null});
 return {chart,chartInput,chartKey,expense,payable,period,periodInput,periodKey,rate,rateInput,rateKey,account};
}
it('constructs Payroll prerequisites from zero Company Finance references under strict People and Payroll purchases and posts independently reviewed native payroll',async()=>{
 const s=await zeroFinancePayrollPrerequisiteFixture(),refs=await constructPayrollPrerequisites(s),engagement=await db.prepare('SELECT id,version FROM hr_engagements WHERE company_id=? AND person_id=?').bind(f.company,`person-${f.claimant.userId}`).first<{id:string;version:number}>();if(!engagement)throw Error('ORIGINAL_IMPORTED_EMPLOYEE_REFERENCE_REQUIRED');
 await s.people.assignPayGroup(s.administrator,engagement.id,s.people.assignPayGroupSchema.parse({payGroupId:s.group.id,expectedVersion:engagement.version}),key());
 const compensation=await s.people.createCompensation(s.administrator,s.people.compensationSchema.parse({engagementId:engagement.id,effectiveFrom:'2026-09-01',payFrequency:'monthly',baseAmount:'1000',currency:'THB',components:[]}),key());if(typeof compensation.id!=='string'||typeof compensation.version!=='number')throw Error('ACTUAL_COMPENSATION_RECEIPT_REQUIRED');await s.people.transitionCompensation(s.reviewer,compensation.id,{status:'approved',expectedVersion:compensation.version},key());
 const run=await s.people.createPayrollRun(s.payroll,s.people.payrollRunSchema.parse({payGroupId:s.group.id,periodStart:'2026-09-01',periodEnd:'2026-09-30',payDate:'2026-10-01',currency:'THB'}),key());
 for(const [stepKey,userId]of [['calculate',s.payroll.userId],['preview',s.payroll.userId],['in_review',s.reviewer.userId],['approved',s.financeReviewer.userId]]as const){const flow=await resolveFlow(db,s.administrator,'payroll',run.id);await assignFlowPrimary(db,s.administrator,'payroll',run.id,{stepKey,userId,expectedVersion:flow.version,recordVersion:flow.recordVersion,reason:'Actual eligible Payroll preparer, independent reviewer and original Finance owner',saveDefault:false},key());}
 const calculated=await s.people.calculatePayrollRun(s.payroll,run.id,key()),read=await s.people.getPayrollRun(s.administrator,run.id);expect(calculated).toMatchObject({status:'preview',totals:{headcount:1,gross:100000,net:100000}});if(typeof read.run.input_digest!=='string')throw Error('ACTUAL_CALCULATED_PAYROLL_DIGEST_REQUIRED');
 const submitted=await submitPayrollReview(s.payroll,run.id,{expectedVersion:calculated.version,inputDigest:read.run.input_digest},key());if(typeof submitted.version!=='number')throw Error('ACTUAL_SUBMITTED_PAYROLL_VERSION_REQUIRED');
 const approved=await s.people.approvePayrollRun(s.reviewer,run.id,submitted.version,key());if(typeof approved.version!=='number')throw Error('ACTUAL_APPROVED_PAYROLL_VERSION_REQUIRED');
 const mapping=await s.people.savePayrollAccountMapping(s.administrator,s.group.id,s.people.payrollAccountMappingSchema.parse({chartId:refs.chart.id,expenseAccountId:refs.expense.result.id,netPayableAccountId:refs.payable.result.id}),key());if(typeof mapping.id!=='string'||typeof mapping.version!=='number')throw Error('ACTUAL_PAYROLL_MAPPING_RECEIPT_REQUIRED');await s.people.approvePayrollAccountMapping(s.financeReviewer,mapping.id,mapping.version,key());
 const input=s.people.payrollPostSchema.parse({accountMappingId:mapping.id,periodId:refs.period.id,entryDate:'2026-09-30',expectedVersion:approved.version}),command=key(),posted=await s.people.postPayroll(s.financeReviewer,run.id,input,command);expect(posted).toMatchObject({id:run.id,status:'posted',netPayableMinor:100000});expect(await s.people.postPayroll(s.financeReviewer,run.id,input,command)).toEqual(posted);
 expect(await db.prepare('SELECT status,source_type,source_id,currency FROM finance_journals WHERE id=? AND company_id=?').bind(posted.journalId,f.company).first()).toEqual({status:'posted',source_type:'payroll',source_id:run.id,currency:'THB'});expect(await db.prepare('SELECT SUM(debit_minor) debit,SUM(credit_minor) credit FROM finance_journal_lines WHERE journal_id=?').bind(posted.journalId).first()).toEqual({debit:100000,credit:100000});
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['people','people-payroll']);expect(await db.prepare('SELECT count(*) n FROM commerce_settings WHERE company_id=?').bind(f.company).first<number>('n')).toBe(0);
 expect(await s.api.payrollPostingPrerequisiteConfirmation(s.administrator,{kind:'account',request:refs.expense.input},refs.expense.command)).toEqual({kind:'account',commandId:refs.expense.command,payGroupId:s.group.id,id:refs.expense.result.id,status:'active',version:1});
});
it('keeps Payroll prerequisite purchase and Company guards strict, rejects stale and foreign-currency sources and recovers exact retained reference receipts',async()=>{
 const s=await zeroFinancePayrollPrerequisiteFixture(false),missingBody=s.api.payrollPostingChartSchema.parse({payGroupId:s.group.id,expectedVersion:0,contextKey:'0'.repeat(64),input:{name:'Denied missing purchase chart',baseCurrency:'THB'}}),initial=await payrollPrerequisiteNativeImage();
 // This new closed owner remains strict even when ordinary legacy mode is disabled.
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');expect(await s.api.payrollPostingPrerequisites(s.administrator,{payGroupId:s.group.id})).toMatchObject({available:false,unavailableReason:'purchase-required',groups:[]});await expect(s.api.createPayrollPostingChart(s.administrator,missingBody,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await payrollPrerequisiteNativeImage()).toEqual(initial);
 await payrollAddonGrant('people');await payrollAddonGrant('people-payroll');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const stale=s.api.payrollPostingChartSchema.parse({...await s.envelope(),input:{name:'Real original Payroll chart',baseCurrency:'THB'}}),chart=await s.api.createPayrollPostingChart(s.administrator,stale,key()),afterChart=await payrollPrerequisiteNativeImage();await expect(s.api.createPayrollPostingChart(s.administrator,stale,key())).rejects.toMatchObject({code:'PAYROLL_POSTING_CONTEXT_CHANGED'});expect(await payrollPrerequisiteNativeImage()).toEqual(afterChart);
 for(const actor of [{...s.administrator,branchId:f.branch},{...s.administrator,departmentId:f.department}]){expect(await s.api.payrollPostingPrerequisites(actor,{payGroupId:s.group.id})).toMatchObject({available:false,unavailableReason:'authority-required'});await expect(s.api.createPayrollPostingChart(actor,{...stale,...await s.envelope()},key())).rejects.toMatchObject({code:'COMPANY_PAYROLL_SCOPE_REQUIRED'});}
 await expect(s.api.createPayrollPostingChart({...s.administrator,companyId:s.otherCompanyId,scope:{...s.administrator.scope,companyId:s.otherCompanyId}},{...stale,...await s.envelope()},key())).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});
 const beforeCurrency=await payrollPrerequisiteNativeImage();await expect(s.api.createPayrollPostingChart(s.administrator,{...await s.envelope(),input:{name:'Wrong functional currency',baseCurrency:'USD'}},key())).rejects.toMatchObject({code:'PAYROLL_CHART_CURRENCY'});await expect(s.api.createPayrollPostingAccount(s.administrator,{...await s.envelope(),input:{chartId:chart.id,code:'9991',name:'Wrong account currency',category:'expense',currency:'USD'}},key())).rejects.toMatchObject({code:'PAYROLL_ACCOUNT_INVALID'});await expect(s.api.confirmPayrollPostingRate(s.administrator,{...await s.envelope(),input:{currency:'EUR',effectiveDate:'2026-09-01',currencyPerUsd:'0.9',note:'Wrong selected Payroll currency'}},key())).rejects.toMatchObject({code:'PAYROLL_CHART_CURRENCY'});expect(await payrollPrerequisiteNativeImage()).toEqual(beforeCurrency);
 const accountInput=s.api.payrollPostingAccountSchema.parse({...await s.envelope(),input:{chartId:chart.id,code:'5199',name:'Unused original Payroll reference',category:'expense',currency:'THB'}}),accountKey=key(),account=await s.api.createPayrollPostingAccount(s.administrator,accountInput,accountKey);
 // Explicit retained-native-history robustness fixture, NOT a normal closed
 // Payroll status-transition claim. The unused real account has no economics;
 // no journal/balance/receipt/history or authority is manufactured here.
 await db.prepare("UPDATE finance_accounts SET status='inactive',version=version+1 WHERE id=? AND company_id=? AND status='active' AND version=1").bind(account.id,f.company).run();expect(await db.prepare('SELECT status,version FROM finance_accounts WHERE id=?').bind(account.id).first()).toEqual({status:'inactive',version:2});expect((await s.current()).accounts.some(row=>row.id===account.id)).toBe(false);
 await payrollAddonGrant('people-payroll','draining');const terminal=await payrollPrerequisiteNativeImage();expect(await s.api.createPayrollPostingAccount(s.administrator,accountInput,accountKey)).toEqual(account);expect(await s.api.payrollPostingPrerequisiteConfirmation(s.administrator,{kind:'account',request:accountInput},accountKey)).toEqual({kind:'account',commandId:accountKey,payGroupId:s.group.id,id:account.id,status:'inactive',version:2});expect(await payrollPrerequisiteNativeImage()).toEqual(terminal);
 await expect(s.api.payrollPostingPrerequisiteConfirmation(s.administrator,{kind:'account',request:{...accountInput,input:{...accountInput.input,name:'Changed original body'}}},accountKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});expect(await payrollPrerequisiteNativeImage()).toEqual(terminal);
});
it('rolls back every Payroll prerequisite effect after actual deferred native source, IAM and purchase changes',async()=>{
 const s=await zeroFinancePayrollPrerequisiteFixture(),request=s.api.payrollPostingChartSchema.parse({...await s.envelope(),input:{name:'Original deferred Payroll chart',baseCurrency:'THB'}});
 const expiry=async(endsAt:number|null)=>{const prior=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='people-payroll');if(!prior)throw Error('ORIGINAL_PAYROLL_GRANT_REQUIRED');const store=packageGrantStore(db,{authorize:async(_database,companyId)=>{if(companyId!==f.company)throw Error('PAYROLL_REFERENCE_EXPIRY_COMPANY');return {userId:'isolated-payroll-reference-provider'};},obligations:async()=>{throw Error('NO_PACKAGE_WITHDRAWAL_IN_EXPIRY_PROBE');}});await store.apply({expectedVersion:prior.version,idempotencyKey:key(),grant:{...prior,endsAt,version:prior.version+1,reason:'Audited real Payroll prerequisite expiry and renewal',evidence:'test:payroll-prerequisite-late-expiry'}});};
 // Expire the real addon through its original audited Commercial command. An
 // unaudited deferred grant UPDATE would only test audit reciprocity, not the
 // intended owner purchase observation after the real deferred-effect flush.
 const probes=[{kind:'source',sql:`UPDATE hr_pay_groups SET name=name||' deferred source drift' WHERE id='${s.group.id}'`,codes:['PAYROLL_POSTING_SOURCE_CHANGED']},{kind:'iam',sql:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${s.administrator.assignmentId}'`,codes:['QUEUED_AUTHORITY_CHANGED']},{kind:'purchase',sql:'PERFORM pg_sleep(2.6)',codes:['PACKAGE_DRAINING']}];
 for(const probe of probes){if(probe.kind==='purchase')await expiry(Date.now()+2500);const command=key(),fault='payroll_reference_'+probe.kind+'_'+key().replaceAll('-',''),currentRequest={...request,...await s.envelope()},before=await payrollPrerequisiteNativeImage();await db.exec(`CREATE SEQUENCE ${fault};CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}');${probe.sql};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
  try{let failure:unknown;try{await s.api.createPayrollPostingChart(s.administrator,currentRequest,command);}catch(error){failure=error;}expect(await db.prepare(`SELECT is_called FROM ${fault}`).first('is_called')).toBe(true);expect(failure).toBeInstanceOf(Error);expect(probe.codes).toContain((failure as {code?:string}).code);expect(await payrollPrerequisiteNativeImage()).toEqual(before);expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(command).first()).toBeNull();}finally{await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}`);if(probe.kind==='purchase')await expiry(null);}
 }
 const saved=await s.api.createPayrollPostingChart(s.administrator,request,key());expect(saved).toMatchObject({companyId:f.company,version:1});
});
it('orders real disabled receipt-first Finance writers before strict Payroll prerequisite constructors without hidden transaction retries',async()=>{
 const s=await zeroFinancePayrollPrerequisiteFixture(),foreign=await expenseFixture(false,false,false),request=s.api.payrollPostingChartSchema.parse({...await s.envelope(),input:{name:'Actual strict Payroll overlap chart',baseCurrency:'THB'}});
 // Original generic compatibility writer intentionally uses its disabled mode;
 // the new closed owner independently stays strict on real People+Payroll grants.
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
 const cross=await payrollOtherHeldWriter(command=>s.finance.createChart(foreign.owner,s.finance.createChartSchema.parse({name:'Actual unrelated Company legacy chart',baseCurrency:'THB'}),command),command=>s.api.createPayrollPostingChart(s.administrator,request,command));expect(cross.current.status).toBe('fulfilled');if(cross.current.status!=='fulfilled')throw cross.current.reason;const scopedChart=cross.current.value;expect(scopedChart.companyId).toBe(f.company);expect(await db.prepare('SELECT count(*) n FROM finance_charts WHERE company_id=?').bind(foreign.company).first<number>('n')).toBe(1);
 const periodInput=s.api.payrollPostingPeriodSchema.parse({...await s.envelope(),input:{name:'Stale concurrent Payroll period',startDate:'2026-09-01',endDate:'2026-09-30'}}),same=await payrollOtherHeldWriter(command=>s.finance.createAccount(s.administrator,s.finance.createAccountSchema.parse({chartId:scopedChart.id,code:'5190',name:'Actual original concurrent reference',category:'expense',currency:'THB'}),command),command=>s.api.createPayrollPostingPeriod(s.administrator,periodInput,command));expect(same.current.status).toBe('rejected');if(same.current.status==='rejected')expect(same.current.reason).toMatchObject({code:'PAYROLL_POSTING_CONTEXT_CHANGED'});expect(await db.prepare('SELECT count(*) n FROM finance_periods WHERE company_id=?').bind(f.company).first<number>('n')).toBe(0);expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(same.newKey).first()).toBeNull();
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');const saved=await s.api.createPayrollPostingPeriod(s.administrator,{...periodInput,...await s.envelope()},key());expect(saved).toMatchObject({companyId:f.company,state:'open',version:0});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['people','people-payroll']);
});
