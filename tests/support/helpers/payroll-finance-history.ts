import {expect,vi} from "vitest";
import type {WorkContext,AccountAccessContext} from "../../../src/server/modules/identity_access/public/authorization";
import {workspaceGrantsForAccount} from "../../../src/server/modules/identity_access/public/authorization";
import {accountWorkModuleAccess} from "../../../src/server/modules/identity_access/public/work-module-access";
import {createProductWorkspace} from "../../../src/server/modules/identity_access/application/product-workspace";
import {productWorkspaceTarget} from "../../../src/server/modules/identity_access/domain/product-workspace";
import {initializeWorkSelection,readWorkSelection,selectWorkRole} from "../../../src/server/modules/work_projection/public/work-role-store";
import {readCompanyPurchaseFacts} from "../../../src/server/modules/commercial/public/package-grants";
import {financeProductDocumentEntry} from "../../../src/server/modules/finance/public/document-purchases";
import {financeDocumentFlow,financeDocumentPresentationAccess,financeSourceDocumentBackdrop,journalRecordWorkspace} from "../../../src/server/modules/finance/public/record-presentation";
import {bookRuleSchema,prepareBookRule,decideBookRule} from "../../../src/server/modules/finance/public/finance-books";
import {createJournal,createJournalSchema,importBankStatement} from "../../../src/server/modules/finance/public/finance";
import {journalDetail,ledgerWorkspace} from "../../../src/server/modules/finance/public/ledger";
import {ApiError} from "../../../src/server/modules/runtime/public/http";
import {attachmentContent,bindRecordAttachmentSet} from "../../../src/server/modules/files/public/attachments/service";
import {env} from "../runtime-env";
import {expenseDb as db,key,type expenseFixture} from "./expense-fixture";
import {attachmentBindingState,seedAttachmentSet} from "./attachment-v2";

type Fixture=Awaited<ReturnType<typeof expenseFixture>>;
/** A pre-existing, independently reviewed secondary book created through its
 * owner service while legacy Finance compatibility is still disabled. */
export async function preparePayrollProjection(f:Fixture,preparer:WorkContext,approver:WorkContext){
 await importBankStatement(preparer,{bankAccount:f.accounts.cash,statementNumber:key(),statementDate:"2026-09-01",opening:"0",closing:"0",sourceHash:key(),rows:[]},key());
 const chart=key();await db.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,?,'THB',?,1,1)").bind(chart,f.company,"Existing Payroll secondary book",preparer.userId).run();
 const mappings=[];for(const source of (await db.prepare("SELECT id FROM finance_accounts WHERE chart_id=? ORDER BY id").bind(f.chart).all<{id:string}>()).results){
  const target=key();await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?").bind(target,chart,source.id).run();
  mappings.push({primaryAccountId:source.id,secondaryAccountId:target});
 }
 const rule=await prepareBookRule(preparer,bookRuleSchema.parse({primaryChartId:f.chart,secondaryChartId:chart,effectiveDate:"2026-09-01",openingPeriodId:f.period,reason:"Independently reviewed retained Payroll book",mappings}),key());
 await decideBookRule(approver,String(rule.id),{expectedVersion:1,reason:"Independent opening and source account review"},"approve",key());
 return {chart,openingId:`book-opening-${rule.id}`};
}

/** Exercise original owner readers, normal versioned role selection and the
 * original Files transaction using real Payroll-produced ledger identities. */
export async function assertPayrollFinanceHistory(f:Fixture,actor:WorkContext,payrollActor:WorkContext,runId:string,primaryIds:string[],book:{chart:string;openingId:string}){
 const forged=await createJournal(actor,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:"2026-09-30",currency:"THB",sourceType:"payroll",sourceId:runId,memo:"A caller label cannot manufacture Payroll ledger ownership",lines:[{accountId:f.accounts.expense,debit:"1",credit:"0"},{accountId:f.accounts.cash,debit:"0",credit:"1"}]}),key());
 vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 expect((await readCompanyPurchaseFacts(db,f.company)).grants.map(grant=>grant.packageId).sort()).toEqual(["people","people-payroll"]);
 const projections=(await db.prepare(`SELECT secondary_journal_id id FROM finance_book_postings WHERE primary_journal_id IN (${primaryIds.map(()=>"?").join(",")}) ORDER BY id`).bind(...primaryIds).all<{id:string}>()).results;
 expect(projections).toHaveLength(primaryIds.length);
 const assignment=await db.prepare("SELECT version FROM assignments WHERE id=?").bind(actor.assignmentId).first<{version:number}>(),scope={companyId:f.company,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:assignment!.version,role:actor.role};
 for(const id of [...primaryIds,...projections.map(row=>row.id)]){
  expect((await journalDetail(actor,id,db)).journal.id).toBe(id);
  expect(await financeProductDocumentEntry(db,scope,{kind:"journal",id})).toEqual({capability:"people.payroll",purchasePurpose:"history"});
  expect(await financeDocumentPresentationAccess(actor,"journal",id)).toEqual({sourceOnly:true,historyOnly:true});
  expect(await financeDocumentFlow(actor,"journal",id)).toMatchObject({canAct:false,canManage:false});
 }
 const id=primaryIds[0],workspace=await journalRecordWorkspace(actor,id);
 expect(workspace.journal.id).toBe(id);expect(workspace).not.toHaveProperty("journals");expect(workspace).not.toHaveProperty("report");
 expect((await journalRecordWorkspace(actor,projections[0].id)).chart.id).toBe(book.chart);
 for(const forbidden of [String(forged.id),book.openingId]){
  await expect(journalDetail(actor,forbidden,db)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
  expect(await financeProductDocumentEntry(db,scope,{kind:"journal",id:forbidden})).toBeNull();
 }
 // This owner has no generic Finance invoice/payment relation, even when a
 // caller passes a real Payroll journal ID as an alternate document kind.
 for(const kind of ["invoice","payment"] as const)expect(await financeProductDocumentEntry(db,scope,{kind,id})).toBeNull();
 await expect(ledgerWorkspace(actor,f.chart)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await expect(journalDetail(payrollActor,id,db)).rejects.toMatchObject({status:403});
 expect(await financeProductDocumentEntry(db,{...scope,assignmentVersion:scope.assignmentVersion+1},{kind:"journal",id})).toBeNull();
 // Existing Payroll role remains selected until the user chooses an existing
 // Finance-authorized assignment through the ordinary revisioned service.
 const sessionId=key(),account=async():Promise<AccountAccessContext>=>{const {rows,roles}=await workspaceGrantsForAccount(db,f.company,actor.userId);return {scope:actor.scope,sessionId,userId:actor.userId,name:actor.name,email:actor.email,expiresAt:Date.now()+600000,requestHeaders:new Headers(),assignments:rows.filter(row=>roles.get(row.role)?.scopes.includes(row.scope_kind)).map(row=>({id:row.id,role:row.role,companyId:row.company_id,branchId:row.branch_id,scopeKind:row.scope_kind,scopeNodeId:row.scope_node_id,modules:JSON.parse(row.modules_json),version:row.version,validUntil:row.valid_until}))};};
 const service=createProductWorkspace({access:account,modules:accountWorkModuleAccess,purchaseFacts:access=>readCompanyPurchaseFacts(db,access.scope.companyId),ready:async()=>true,financeDocumentEntry:(access,option,target)=>financeProductDocumentEntry(db,{...scope,userId:access.userId,assignmentId:option.id,assignmentVersion:option.version!,role:option.role},target),selection:(access,applicable)=>initializeWorkSelection(db,{sessionId,userId:access.userId,companyId:f.company,expiresAt:access.expiresAt},access.assignments,applicable),select:(access,option,revision)=>selectWorkRole(db,{sessionId,userId:access.userId,companyId:f.company,expiresAt:access.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision}),now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 await selectWorkRole(db,{sessionId,userId:actor.userId,companyId:f.company,expiresAt:Date.now()+600000,assignmentId:payrollActor.assignmentId,assignmentVersion:1,revision:0});
 const image=async()=>(await db.prepare("SELECT row_to_json(a)::text row FROM assignments a WHERE company_id=? ORDER BY id").bind(f.company).all()).results,beforeRoles=await image(),input=productWorkspaceTarget("finance",f.company,`/holdings/test/finance/accounting/journals/${id}/overview?chart=retained#history`)!;
 const state=await service.read("finance",input);expect(state.target).toBeNull();expect(state.options.map(option=>option.assignmentId)).not.toContain(payrollActor.assignmentId);expect((await readWorkSelection(db,sessionId,actor.userId,f.company))?.assignment_id).toBe(payrollActor.assignmentId);
 await service.choose("finance",input,actor.assignmentId,state.selection!.revision,scope.assignmentVersion);expect(await service.read("finance",input)).toMatchObject({target:input.target,sourceRecord:true});expect(await image()).toEqual(beforeRoles);
 expect(await financeSourceDocumentBackdrop(actor,"journal",id)).toEqual({kind:"journal",id,number:expect.any(String),date:"2026-09-30",status:"posted",version:expect.any(Number)});
 await expect(service.read("finance",productWorkspaceTarget("finance",f.company,"/holdings/test/finance/accounting/journals")!)).rejects.toMatchObject({code:"PRODUCT_PACKAGE_REQUIRED"});
 // The final Files source fence must see deferred source changes after all
 // effects, and roll both the original owner relationship and file write back.
 const version=Number(await db.prepare("SELECT version FROM finance_journals WHERE id=?").bind(id).first("version")),files=await seedAttachmentSet(db,actor,{targetType:"finance_journal",targetId:id,componentKind:"record",purposeKey:"journal_support",expectedTargetVersion:version}),bytes="%PDF-1.7",objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(files.sourceId).first("object_key"));
 await env.H001_FILES.put(objectKey,bytes,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(files.sourceId).run();
 const command=key(),fault=`payroll_file_source_${key().replaceAll('-','')}`,tables=["hr_payroll_postings","finance_journals","attachment_sets","attachment_links","attachment_events","attachments","command_receipts","audit_events","outbox_events"],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(t)::text row FROM ${table} t ORDER BY row_to_json(t)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN DELETE FROM hr_payroll_postings WHERE run_id='${runId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`);
 try{let failure:unknown;try{await bindRecordAttachmentSet(db,actor,files.setId,version,command);}catch(error){failure=error;}expect(failure).toBeInstanceOf(Error);const cause=(failure as {cause?:unknown}).cause??failure;expect(cause).toMatchObject({code:"23514",detail:expect.stringContaining("finance-document-owner")});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON audit_events;DROP FUNCTION ${fault}()`);}
 await bindRecordAttachmentSet(db,actor,files.setId,version,command);expect(await attachmentBindingState(db,files.setId)).toMatchObject({set_state:"bound",link_state:"bound"});expect(await db.prepare("SELECT COUNT(*) n FROM command_receipts WHERE command_id=?").bind(command).first("n")).toBe(1);
 expect(await (await attachmentContent(db,env.H001_FILES,actor,files.linkId,"original")).object.text()).toBe(bytes);expect(await db.prepare("SELECT legal_hold FROM attachments WHERE id=?").bind(files.sourceId).first("legal_hold")).toBe(1);
 await db.prepare("UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?").bind(actor.assignmentId).run();
 await expect(journalDetail(actor,id,db)).rejects.toMatchObject({code:"FINANCE_ACCESS_CHANGED"});expect(await financeProductDocumentEntry(db,scope,{kind:"journal",id})).toBeNull();await expect(attachmentContent(db,env.H001_FILES,actor,files.linkId,"original")).rejects.toMatchObject({status:404});
 expect(await db.prepare("SELECT legal_hold,revoked_at FROM attachments WHERE id=?").bind(files.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});expect(await (await env.H001_FILES.get(objectKey))!.text()).toBe(bytes);
}
