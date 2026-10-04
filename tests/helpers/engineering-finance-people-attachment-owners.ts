import type { WorkContext } from "../../src/server/authorization";
import { expenseDb as db, expenseFixture, key } from "./expense-fixture";
import { createJournal,createJournalSchema,createLease,createLoan,createParty,createRevenueContract,importBankStatement,beginClose,closePeriod,createInvoice,invoiceSchema,partySchema,postInvoice,createPayment,approvePayment,releasePayment,confirmPayment,reconcileBankTransaction } from "../../src/server/finance";
import { createAsset,assetSchema } from "../../src/server/finance-assets";
import { createAccrualSchedule,accrualScheduleSchema } from "../../src/server/finance-accruals";
import { assignFlowPrimary,resolveFlow } from "../../src/server/flow-routing";
import { createLearningCourse,changeLearningCourse,createLearningAssignment,changeLearningAssignment } from "../../src/server/learning";
import { createBenefitPlan,createBenefitWindow,createBenefitDependent,createBenefitElection,changeBenefitRecord } from "../../src/server/benefits";
import { requestPayrollCorrection,payrollCorrectionRequestSchema,updateTimeEntry,timeEntryUpdateSchema } from "../../src/server/people-payroll";
import { createOvertime,commandOvertime } from "../../src/server/overtime";
import { overtimeDetailsSchema,overtimeCommandSchema } from "../../src/lib/overtime";
import { calendarCommand } from "../../src/server/calendar/commands";
import { createSupplierBatch,supplierBatchSchema,decideSupplierBatch,exportSupplierBatch,confirmSupplierInstruction,supplierConfirmSchema } from "../../src/server/supplier-payments";
import { approveRefund,prepareRefundInstruction,exportRefundInstructions,confirmRefundInstruction,refundInstructionSchema,refundConfirmationSchema } from "../../src/server/refunds";
import {prepareTaxIdentity,taxIdentitySchema} from "../../src/server/finance-tax-identities";
import {thaiFinanceTaxPack} from "../../src/lib/thai-finance-tax-pack";

export type AttachmentOwnerTuple={id:string;targetType:string;purposeKey:string;componentKind:"record"|"action"};
export type AttachmentOwnerFixture={fixture:Awaited<ReturnType<typeof expenseFixture>>;actor:WorkContext;targetId:string;version:number;table:string;versionColumn:string;componentRef?:string;bindingType:string;bindAction?:(setId:string,commandId:string)=>Promise<unknown>};
const reason="Independently reviewed synthetic owner evidence for the engineering File Sources intake test.";

/** Owner prerequisites are explicit isolated fixtures. File Sources rows, bytes,
 * upload intents, scans and owner bindings are always made by real services. */
export async function financePeopleAttachmentOwner(tuple:AttachmentOwnerTuple):Promise<AttachmentOwnerFixture>{
 const f=await expenseFixture(),actor=f.owner;
 for(const work of [f.owner,f.tie,f.claimant,f.head,f.reviewer,f.manager,f.payer])work.modules=["finance","talent","learning","benefits","people_payroll"];
 await db.prepare("UPDATE assignments SET modules_json=? WHERE company_id=?").bind(JSON.stringify(["finance","talent","learning","benefits","people_payroll"]),f.company).run();
 const standard=(targetId:string,table:string,version=1):AttachmentOwnerFixture=>({fixture:f,actor,targetId,version,table,versionColumn:"version",bindingType:"record_document"});
 if(tuple.targetType==="hr_person")return standard(`person-${f.claimant.userId}`,"hr_people");
 if(tuple.targetType==="finance_tax_identity"){
  const prefix="099999999991",sum=[...prefix].reduce((total,digit,index)=>total+Number(digit)*(13-index),0),taxId=prefix+String((11-sum%11)%10);
  const identity=await prepareTaxIdentity(actor,taxIdentitySchema.parse({partyId:null,legalName:"Fictional Company payer",taxId,address:"99 Synthetic Road Bangkok 10000",branchCode:"00000",entityKind:"domestic_company",effectiveFrom:"2026-01-01",effectiveTo:"2026-12-31",signerName:"Synthetic authorized signer",signerAuthority:reason,rulePackVersion:thaiFinanceTaxPack.version,evidenceRef:"Fictional Company identity and scanned signature; no real taxpayer data."}),key());return standard(String(identity.id),"finance_tax_identities");
 }
 if(tuple.targetType==="finance_journal"){const created=await createJournal(actor,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-01",currency:"THB",memo:reason,lines:[{accountId:f.accounts.expense,debit:"100",credit:"0"},{accountId:f.accounts.cash,debit:"0",credit:"100"}]}),key());return standard(created.id,"finance_journals");}
 if(tuple.targetType==="finance_asset"){const created=await createAsset(actor,assetSchema.parse({assetNumber:key(),name:"Synthetic accounting asset",accountId:f.accounts.cash,currency:"THB",cost:"100",usefulLifeMonths:12,inServiceDate:"2026-09-01"}),key());return standard(String(created.id),"finance_assets");}
 if(tuple.targetType==="finance_accrual_schedule"){const created=await createAccrualSchedule(actor,accrualScheduleSchema.parse({description:reason,debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable,currency:"THB",total:"100",startDate:"2026-09-01",endDate:"2026-12-31"}),key());return standard(String(created.id),"finance_accrual_schedules");}
 if(tuple.targetType==="finance_lease"){const created=await createLease(actor,{leaseNumber:key(),counterparty:"Synthetic lessor",currency:"THB",commencementDate:"2026-09-01",endDate:"2027-09-01",payment:"100",discountRate:"0",paymentFrequency:"monthly"},key());return standard(String(created.id),"finance_leases");}
 if(tuple.targetType==="finance_loan"){const created=await createLoan(actor,{loanNumber:key(),lender:"Synthetic lender",currency:"THB",principal:"100",annualRate:"0",commencementDate:"2026-09-01",maturityDate:"2027-09-01"},key());return standard(String(created.id),"finance_loans");}
 if(tuple.targetType==="finance_revenue_contract"){const party=await createParty(actor,partySchema.parse({kind:"customer",code:key(),name:"Synthetic customer",currency:"THB"}),key()),created=await createRevenueContract(actor,{customerId:String(party.id),contractNumber:key(),currency:"THB",transactionPrice:"100"},key());return standard(String(created.id),"finance_revenue_contracts");}
 if(tuple.targetType==="finance_bank_statement"){const bank=await bankFixture(f,"100");return standard(bank.statementId,"finance_bank_statements");}
 if(tuple.targetType==="finance_period"){const started=await beginClose(f.reviewer,f.period,key());return {...standard(f.period,"finance_periods",Number(started.version)),actor:f.manager,versionColumn:"close_version",componentRef:"close-approval",bindingType:"finance_period_close",bindAction:(setId,command)=>closePeriod(f.manager,f.period,{expectedVersion:Number(started.version),proof:reason,attachmentSetIds:[setId]},command)};}
 if(tuple.targetType==="finance_payment"){
  const invoice=await payableFixture(f),payment=await createPayment(f.reviewer,{partyId:invoice.partyId,kind:"payment",paymentNumber:key(),paymentDate:"2026-09-05",currency:"THB",amount:"100",bankAccount:f.accounts.cash},key());
  await approvePayment(f.manager,String(payment.id),{expectedVersion:1,proof:reason},key());await releasePayment(f.payer,String(payment.id),{expectedVersion:2,proof:reason},key());const bank=await bankFixture(f,"-100");
  return {...standard(String(payment.id),"finance_payments",3),actor:f.payer,componentRef:"bank-confirmation",bindingType:"finance_payment_confirmation",bindAction:(setId,command)=>confirmPayment(f.payer,String(payment.id),{expectedVersion:3,periodId:f.period,bankTransactionId:bank.transactionId,bankVersion:1,bankReference:"FIXTURE-OWNER-001",proof:reason,allocations:[{invoiceId:invoice.id,amount:"100"}],attachmentSetIds:[setId]},command)};
 }
 if(tuple.targetType==="finance_bank_transaction"){
  const bank=await bankFixture(f,"100"),journal=key(),payment=key();
  await db.batch([
   db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,memo,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,'2026-09-05','THB',?,'posted',?,?,1,1,1)").bind(journal,f.company,f.chart,f.period,journal,reason,f.reviewer.userId,f.manager.userId),
   db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,10000,0,10000,0,0)").bind(key(),journal,f.accounts.cash),
   db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,0,10000,0,10000,1)").bind(key(),journal,f.accounts.revenue),
   db.prepare("INSERT INTO finance_payments(id,company_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,released_by,created_at,updated_at) VALUES(?,?,'receipt',?,'2026-09-05','THB',10000,0,?,'settled',?,?,?,?,1,1)").bind(payment,f.company,payment,f.accounts.cash,journal,f.reviewer.userId,f.manager.userId,f.payer.userId),
  ]);
  return {...standard(bank.transactionId,"finance_bank_transactions"),actor:f.manager,componentRef:"reconcile",bindingType:"finance_bank_reconciliation",bindAction:(setId,command)=>reconcileBankTransaction(f.manager,bank.transactionId,{expectedVersion:1,matchedType:"payment",matchedId:payment,proof:reason,attachmentSetIds:[setId]},command)};
 }
 if(tuple.targetType==="finance_supplier_batch"){
  const invoice=await payableFixture(f),batch=await createSupplierBatch(f.reviewer,supplierBatchSchema.parse({bankAccountId:f.accounts.cash,plannedDate:"2026-09-05",items:[{invoiceId:invoice.id,expectedVersion:2,amount:"100",beneficiaryName:"Synthetic supplier",beneficiaryAccount:"FIXTURE-001"}]}),key());
  if(tuple.componentKind==="record")return {...standard(String(batch.id),"finance_supplier_batches"),actor:f.reviewer};
  await decideSupplierBatch(f.manager,String(batch.id),{expectedVersion:1,reason},key(),"approve");await exportSupplierBatch(f.payer,String(batch.id),{expectedVersion:2},key());const instructionId=String(await db.prepare("SELECT id FROM finance_supplier_instructions WHERE batch_id=?").bind(batch.id).first("id")),bank=await bankFixture(f,"-100");
  return {...standard(String(batch.id),"finance_supplier_batches",3),actor:f.payer,componentRef:`supplier-instruction:${instructionId}`,bindingType:"finance_supplier_payment_confirmation",bindAction:(setId,command)=>confirmSupplierInstruction(f.payer,instructionId,supplierConfirmSchema.parse({expectedVersion:2,bankTransactionId:bank.transactionId,bankVersion:1,periodId:f.period,bankReference:"FIXTURE-001",evidence:reason,attachmentSetIds:[setId]}),command)};
 }
 if(tuple.targetType==="finance_refund_obligation")return refundFixture(f,tuple);
 if(tuple.targetType==="talent_candidate"||tuple.targetType==="talent_offer"){
  const candidate=key(),requisition=key(),application=key(),offer=key();await db.batch([
   db.prepare("INSERT INTO talent_requisitions(id,company_id,branch_id,department_id,job_title,openings,target_start_date,description,status,created_by,created_at,updated_at) VALUES(?,?,?,?,'Synthetic role',1,'2026-11-01',?,'open',?,1,1)").bind(requisition,f.company,f.branch,f.department,reason,actor.userId),
   db.prepare("INSERT INTO talent_candidates(id,company_id,legal_name,email,consent_evidence,consented_at,retention_until,status,created_by,created_at,updated_at) VALUES(?,?,'Synthetic consenting candidate',?,?,1,'2027-12-31','active',?,1,1)").bind(candidate,f.company,`${candidate}@example.test`,reason,actor.userId),
   db.prepare("INSERT INTO talent_applications(id,company_id,requisition_id,requisition_version,candidate_id,status,owner_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'selected',?,?,1,1)").bind(application,f.company,requisition,1,candidate,actor.userId,actor.userId),
   db.prepare("INSERT INTO talent_offers(id,company_id,application_id,application_version,currency,base_amount_minor,start_date,expires_on,terms,status,created_by,created_at,updated_at) VALUES(?,?,?,1,'THB',3000000,'2026-11-01','2026-11-15',?,'draft',?,1,1)").bind(offer,f.company,application,reason,actor.userId),
  ]);return standard(tuple.targetType==="talent_candidate"?candidate:offer,tuple.targetType==="talent_candidate"?"talent_candidates":"talent_offers");
 }
 if(tuple.targetType==="people_lifecycle_case"||tuple.targetType==="people_clearance_task"||tuple.targetType==="payroll_final_pay")return lifecycleFixture(f,tuple);
 if(tuple.targetType==="payroll_advance"){
  const advance=key();await db.prepare("INSERT INTO hr_payroll_advances(id,company_id,engagement_id,person_id,amount_minor,installment_count,first_deduction_period,currency,reason,requested_by,created_at,updated_at) VALUES(?,?,?,?,10000,1,'2026-10-01','THB',?,?,1,1)").bind(advance,f.company,`eng-${f.claimant.userId}`,`person-${f.claimant.userId}`,reason,f.claimant.userId).run();return standard(advance,"hr_payroll_advances");
 }
 if(tuple.targetType==="payroll_run"){
  const group=await payGroup(f),run=key();await db.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,status,input_digest,totals_json,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,'2026-09-01','2026-09-30','2026-10-01','THB','approved',?,'{}',?,?,1,1)").bind(run,f.company,group,key(),actor.userId,f.manager.userId).run();
  return {...standard(run,"hr_payroll_runs"),componentRef:"correction-request",bindingType:"payroll_correction_request",bindAction:(setId,command)=>requestPayrollCorrection(actor,run,payrollCorrectionRequestSchema.parse({expectedVersion:1,reason,attachmentSetIds:[setId]}),command)};
 }
 if(tuple.targetType==="learning_course"||tuple.targetType==="learning_assignment"){
  const course=await createLearningCourse(actor,{title:"Synthetic training",description:reason,provider:"Fixture academy",durationMinutes:30,requiredFor:null},key());
  if(tuple.targetType==="learning_course")return standard(String(course.id),"learning_courses");
  await changeLearningCourse(f.tie,String(course.id),{action:"publish",expectedVersion:1,reason},key());const assignment=await createLearningAssignment(actor,{courseId:String(course.id),courseVersion:2,learnerId:f.claimant.userId,dueDate:"2026-10-31",reason},key());
  if(tuple.componentKind==="record")return standard(String(assignment.id),"learning_assignments");
  await changeLearningAssignment(f.claimant,String(assignment.id),{action:"start",expectedVersion:1,reason},key());return {...standard(String(assignment.id),"learning_assignments",2),actor:f.claimant,componentRef:"completion",bindingType:"learning_completion",bindAction:(setId,command)=>changeLearningAssignment(f.claimant,String(assignment.id),{action:"complete",expectedVersion:2,attendance:"attended",scoreBps:9200,evidence:reason,reason,attachmentSetIds:[setId]},command)};
 }
 if(tuple.targetType.startsWith("benefit_"))return benefitsFixture(f,tuple);
 if(tuple.targetType==="time_entry"){
  const entry=key();await db.prepare("INSERT INTO hr_time_entries(id,engagement_id,company_id,work_date,regular_hours,status,submitted_by,created_at,updated_at) VALUES(?,?,?,'2026-09-25',8,'rejected',?,1,1)").bind(entry,`eng-${f.claimant.userId}`,f.company,f.claimant.userId).run();
  return {...standard(entry,"hr_time_entries"),actor:f.claimant,componentRef:"correction-resubmission",bindingType:"time_entry_correction",bindAction:(setId,command)=>updateTimeEntry(f.claimant,entry,timeEntryUpdateSchema.parse({expectedVersion:1,workDate:"2026-09-25",regularHours:8,overtimeHours:0,note:reason,attachmentSetIds:[setId]}),command)};
 }
 if(tuple.targetType==="overtime_request"){
  await f.grant(f.it,"hr_admin");
  const planned=tuple.purposeKey==="planned_evidence",details=overtimeDetailsSchema.parse({mode:planned?"advance":"retrospective",startLocal:planned?"2026-11-10T18:00":"2026-09-25T18:00",endLocal:planned?"2026-11-10T20:00":"2026-09-25T20:00",timezone:"Asia/Bangkok",reason,lateExplanation:planned?"":"Synthetic manager requested retrospective entry."}),request=await createOvertime(f.claimant,details,key());
  return {...standard(String(request.id),"overtime_requests"),actor:f.claimant,componentRef:"request-submission",bindingType:"overtime_submission",bindAction:(setId,command)=>commandOvertime(f.claimant,String(request.id),overtimeCommandSchema.parse({action:"submit",expectedVersion:1,attachmentSetIds:[setId]}),command)};
 }
 throw new Error(`Finance/People owner fixture missing: ${tuple.id}`);
}

type Fixture=Awaited<ReturnType<typeof expenseFixture>>;
async function bankFixture(f:Fixture,amount:string){await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='bank' WHERE id=?").bind(f.accounts.cash).run();const imported=await importBankStatement(f.reviewer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:"2026-09-05",opening:"0",closing:amount,sourceHash:key(),rows:[{externalId:key(),transactionDate:"2026-09-05",amount,currency:"THB"}]},key());return {statementId:String(imported.id),transactionId:String(await db.prepare("SELECT id FROM finance_bank_transactions WHERE statement_id=?").bind(imported.id).first("id"))};}
async function payableFixture(f:Fixture){
 await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='payable' WHERE id=?").bind(f.accounts.payable).run();await db.prepare("UPDATE finance_accounts SET is_control=1,control_kind='bank' WHERE id=?").bind(f.accounts.cash).run();
 const party=await createParty(f.reviewer,partySchema.parse({kind:"supplier",code:key(),name:"Synthetic supplier",currency:"THB"}),key()),invoice=await createInvoice(f.reviewer,invoiceSchema.parse({partyId:String(party.id),kind:"payable",invoiceNumber:key(),invoiceDate:"2026-09-01",dueDate:"2026-09-05",currency:"THB",subtotal:"100",debitAccountId:f.accounts.expense,creditAccountId:f.accounts.payable}),key());
 const flow=await resolveFlow(db,f.owner,"invoice",String(invoice.id));await assignFlowPrimary(db,f.owner,"invoice",String(invoice.id),{stepKey:"draft",userId:f.manager.userId,reason,saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion},key());await postInvoice(f.manager,String(invoice.id),{expectedVersion:1,periodId:f.period,entryDate:"2026-09-01"},key());return {id:String(invoice.id),partyId:String(party.id)};
}
async function payGroup(f:Fixture){const id=key();await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,timezone,overtime_multiplier_bps,proration_method,created_by,created_at,updated_at) VALUES(?,?,'Synthetic payroll intake','TH','THB','monthly','Asia/Bangkok',15000,'calendar_days',?,1,1)").bind(id,f.company,f.owner.userId).run();return id;}
async function lifecycleFixture(f:Fixture,tuple:AttachmentOwnerTuple):Promise<AttachmentOwnerFixture>{
 const lifecycle=key(),task=key(),kind=tuple.purposeKey==="offer_document"?"onboarding":tuple.purposeKey==="transfer_document"?"transfer":"offboarding",payroll=tuple.targetType==="payroll_final_pay",assignee=payroll?f.owner:f.claimant;
 await db.batch([
  db.prepare("INSERT INTO people_lifecycle_cases(id,company_id,person_id,engagement_id,kind,status,version,effective_at,definition,created_by,updated_at) VALUES(?,?,?,?,?,'scheduled',1,1,'{}'::jsonb,?,1)").bind(lifecycle,f.company,`person-${f.claimant.userId}`,`eng-${f.claimant.userId}`,kind,f.owner.userId),
  db.prepare("INSERT INTO people_preparation_tasks(id,company_id,case_id,kind,title,status,assignee_id,updated_at,responsibility,clearance_type) VALUES(?,?,?,'lifecycle','Synthetic clearance','pending',?,1,?,?)").bind(task,f.company,lifecycle,assignee.userId,payroll?"payroll":"equipment",payroll?"payroll":"equipment"),
 ]);
 const base={fixture:f,actor:f.owner,targetId:lifecycle,version:1,table:"people_lifecycle_cases",versionColumn:"version",bindingType:"record_document"};if(tuple.targetType==="people_lifecycle_case")return base;
 if(tuple.targetType==="people_clearance_task")return {...base,actor:f.claimant,targetId:task,table:"people_preparation_tasks",componentRef:"equipment",bindingType:"retirement_clearance_completion",bindAction:(setId,command)=>calendarCommand(db,f.claimant,{action:"task.complete",id:task,expectedVersion:1,status:"completed",reason,references:[],attachmentSetIds:[setId]},command)};
 const group=await payGroup(f),run=key(),preparation=key();await db.batch([
  db.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,scope_engagement_id,created_by,created_at,updated_at) VALUES(?,?,?,'2026-09-01','2026-09-30','2026-10-01','THB','final_pay',?,?,1,1)").bind(run,f.company,group,`eng-${f.claimant.userId}`,f.owner.userId),
  db.prepare("INSERT INTO hr_payroll_final_pay_preparations(id,company_id,task_id,case_id,engagement_id,run_id,reason,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,1,1)").bind(preparation,f.company,task,lifecycle,`eng-${f.claimant.userId}`,run,reason,f.owner.userId),
 ]);return {...base,targetId:preparation,table:"hr_payroll_final_pay_preparations"};
}
async function benefitsFixture(f:Fixture,tuple:AttachmentOwnerTuple):Promise<AttachmentOwnerFixture>{
 const plan=await createBenefitPlan(f.owner,{name:"Synthetic health",provider:"Declared fixture provider",coverageType:"employee",currency:"THB",employeeDeduction:"100",employerCost:"200",effectiveFrom:"2026-09-01",effectiveTo:null,requiredEligibilityCode:null},key()),base={fixture:f,actor:f.owner,targetId:String(plan.id),version:1,table:"benefit_plans",versionColumn:"version",bindingType:"record_document"};
 if(tuple.targetType==="benefit_plan")return base;
 if(tuple.targetType==="benefit_dependent"){const dependent=await createBenefitDependent(f.claimant,{name:"Synthetic dependent",relationship:"child",birthDate:"2020-01-01"},key());if(tuple.componentKind==="record")return {...base,targetId:String(dependent.id),table:"benefit_dependents"};return {...base,targetId:String(dependent.id),table:"benefit_dependents",componentRef:"verification",bindingType:"benefits_dependent_verify",bindAction:(setId,command)=>changeBenefitRecord(f.owner,"dependent",String(dependent.id),{action:"verify",expectedVersion:1,reason,attachmentSetIds:[setId]},command)};}
 await changeBenefitRecord(f.tie,"plan",String(plan.id),{action:"publish",expectedVersion:1,reason},key());const window=await createBenefitWindow(f.owner,{name:"Synthetic enrollment",opensOn:"2026-09-01",closesOn:"2026-12-31",effectiveOn:"2026-09-01"},key());await changeBenefitRecord(f.tie,"window",String(window.id),{action:"open",expectedVersion:1,reason},key());const election=await createBenefitElection(f.claimant,{planId:String(plan.id),planVersion:2,windowId:String(window.id),windowVersion:2,dependentIds:[]},key()),electionBase={...base,targetId:String(election.id),table:"benefit_elections"};
 if(tuple.componentKind==="record")return electionBase;
 if(tuple.purposeKey==="election_document")return {...electionBase,actor:f.claimant,componentRef:"employee-submit",bindingType:"benefits_election_submit",bindAction:(setId,command)=>changeBenefitRecord(f.claimant,"election",String(election.id),{action:"submit",expectedVersion:1,reason,attachmentSetIds:[setId]},command)};
 await changeBenefitRecord(f.claimant,"election",String(election.id),{action:"submit",expectedVersion:1,reason},key());return {...electionBase,version:2,componentRef:"provider-acknowledgement",bindingType:"benefits_election_acknowledge",bindAction:(setId,command)=>changeBenefitRecord(f.owner,"election",String(election.id),{action:"acknowledge",expectedVersion:2,reason,providerReference:"FIXTURE-PROVIDER-001",attachmentSetIds:[setId]},command)};
}
async function refundFixture(f:Fixture,tuple:AttachmentOwnerTuple):Promise<AttachmentOwnerFixture>{
 const invoice=await payableFixture(f),warehouse=key(),order=key(),fulfillment=key(),returned=key(),obligation=key();
 // Retained Commerce handoff fixture: the Commerce lane independently proves
 // physical returns. This test exercises the real Finance cash confirmation.
 await db.batch([
  db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?, 'Synthetic refund intake warehouse')").bind(warehouse,f.company,f.branch,warehouse),
  db.prepare("INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,created_at,updated_at) VALUES(?,?,?,'purchase',?,?,?,'THB','2026-09-01','2026-09-05','fulfilled',10000,0,?,1,1)").bind(order,f.company,f.branch,order,invoice.partyId,warehouse,f.owner.userId),
  db.prepare("INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,invoice_id) VALUES(?,?,?,'2026-09-01',?,1,?)").bind(fulfillment,order,fulfillment,f.owner.userId,invoice.id),
  db.prepare("INSERT INTO commerce_returns(id,company_id,branch_id,order_id,fulfillment_id,invoice_id,kind,number,return_date,reason,status,subtotal_minor,total_minor,credited_minor,refund_minor,base_credit_minor,base_credited_minor,base_refund_minor,requested_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'supplier_return',?,'2026-09-03',?,'posted',10000,10000,0,10000,10000,0,10000,?,?,1,1)").bind(returned,f.company,f.branch,order,fulfillment,invoice.id,returned,reason,f.owner.userId,f.manager.userId),
  db.prepare("INSERT INTO finance_refund_obligations(id,company_id,branch_id,return_id,invoice_id,party_id,direction,currency,chart_id,balance_account_id,amount_minor,outstanding_minor,base_amount_minor,base_outstanding_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,'incoming','THB',?,?,10000,10000,10000,10000,?,1,1)").bind(obligation,f.company,f.branch,returned,invoice.id,invoice.partyId,f.chart,f.accounts.payable,f.reviewer.userId),
 ]);
 const base={fixture:f,actor:f.payer,targetId:obligation,version:1,table:"finance_refund_obligations",versionColumn:"version",bindingType:"record_document"};if(tuple.componentKind==="record")return base;
 await approveRefund(f.manager,obligation,{expectedVersion:1,reason,beneficiaryName:"Synthetic supplier",beneficiaryAccount:"FIXTURE-001"},key());const instruction=await prepareRefundInstruction(f.payer,obligation,refundInstructionSchema.parse({expectedVersion:2,amount:"100",plannedDate:"2026-09-05",bankAccount:f.accounts.cash,cashAccountId:f.accounts.cash}),key());await exportRefundInstructions(f.payer,{instructions:[{id:String(instruction.id),expectedVersion:1}]},key());const ownerVersion=Number(await db.prepare("SELECT version FROM finance_refund_obligations WHERE id=?").bind(obligation).first("version")),bank=await bankFixture(f,"100");
 return {...base,version:ownerVersion,componentRef:`refund-instruction:${instruction.id}`,bindingType:"finance_refund_confirmation",bindAction:(setId,command)=>confirmRefundInstruction(f.payer,String(instruction.id),refundConfirmationSchema.parse({expectedVersion:2,bankTransactionId:bank.transactionId,bankVersion:1,periodId:f.period,bankReference:"FIXTURE-001",evidence:reason,attachmentSetIds:[setId]}),command)};
}
