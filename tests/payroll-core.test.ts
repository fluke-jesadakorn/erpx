import { afterEach,beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "./runtime-env";
import { expenseDb as db,expenseFixture,key } from "./helpers/expense-fixture";
import { approvePayrollRun,calculatePayrollRun,createPaymentBatch,approvePaymentBatch,preparePaymentRetry,confirmPaymentBatch,exportPaymentBatch,getPaymentFile,createPayrollRun,decidePayrollCorrection,decidePayrollAdvance,exportPayrollReview,getPayrollReviewFile,getPayrollRun,payrollRunSchema,prepareFinalPay,finalPayPreparationSchema,requestPayrollCorrection,requestPayrollAdvance,payrollAdvanceRequestSchema,payrollAdvancePage,postPayroll,payrollPostSchema,verifyPayrollCostAssignment } from "../src/server/people-payroll";
import { submitPayrollReview } from "../src/server/payroll-readiness";
import { getPayrollLiabilities, remitPayrollLiability, payrollRemittanceSchema } from "../src/server/payroll-remittance";
import { listStaffNotifications } from "../src/server/staff-notifications";
import { resolveFlow } from "../src/server/flow-routing";
import {attachmentBindingState,enableAttachmentV2ForTest,seedAttachmentSet} from "./helpers/attachment-v2";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));

let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);enableAttachmentV2ForTest();});
beforeEach(async()=>{f=await expenseFixture();});
afterEach(()=>vi.useRealTimers());

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
  vi.useFakeTimers({toFake:["Date"]});vi.setSystemTime(new Date("2026-10-02T00:00:00Z"));
  f.owner.role="payroll_preparer";f.owner.assignmentId=await f.grant(f.owner,"payroll_preparer");
  f.manager.role="payroll_approver";f.manager.assignmentId=await f.grant(f.manager,"payroll_approver");
  f.reviewer.role="finance_approver";f.reviewer.assignmentId=await f.grant(f.reviewer,"finance_approver");
  f.claimant.role="treasury";f.claimant.assignmentId=await f.grant(f.claimant,"treasury");
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
  const created=await createPayrollRun(f.owner,payrollRunSchema.parse({payGroupId:group,periodStart:"2026-09-01",periodEnd:"2026-09-30",payDate:"2026-10-01",currency:"THB"}),key());
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
