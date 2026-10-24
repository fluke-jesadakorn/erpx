import {afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {env,applyMigrations} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {getProject,projectSchema,saveProject,saveTask,taskSchema,taskActionSchema,transitionTask} from "@erpx/server-operations/public/staff-tasks";
import {projectMilestoneSchema,saveProjectMilestone,transitionProjectMilestone} from "@erpx/server-projects/public/project-milestones";
import {projectGraph,projectHandoffs,projectHandoffWork,projectStructureCommand} from "@erpx/server-projects/public/project-structure";
import {listStaffNotifications} from "@erpx/server-collaboration/public/staff-notifications";
import {createProjectCostAllocation,decideProjectCostAllocation,decideProjectWorkLog,projectCostAllocationSchema,projectCostAllocationDecisionSchema,projectWorkLogSchema,projectWorkLogDecisionSchema,saveProjectWorkLog} from "@erpx/server-projects/public/project-actuals";
import {createJournal,createJournalSchema,postJournal} from '@erpx/server-finance/public/finance';
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import {packageGrantRevision} from "@erpx/server-commercial/public/package-grants";
import {withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations} from "@erpx/server-commercial/public/owner-package-obligations";
import {projectPackageObligations} from "@erpx/server-projects/public/package-obligations";
import {enableAttachmentV2ForTest,seedAttachmentSet} from "../../support/helpers/attachment-v2";
import {bindRecordAttachmentSet,attachmentContent} from "@erpx/server-files/public/attachments/service";
import {listProjects} from "@erpx/server-projects/public/staff-projects";
import {listProjectCostAllocations} from "@erpx/server-projects/public/project-actuals";
import {getTask,listTasks,taskDetail,delegateTask} from "@erpx/server-operations/public/staff-tasks";
import {listRecords,recordsPage,getRecord,searchReferences} from "@erpx/server-records/public/records";
import {publishInboxEvent,inboxPage} from "@erpx/server-collaboration/public/inbox";
import {getStaffWork} from "@erpx/server-work_projection/public/domain-work";
import {enrichFlowWork} from "@erpx/server-workflow/public/flow-work";
import type {FileStore} from '@erpx/server-runtime/public/runtime/types';
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);});
afterEach(()=>vi.unstubAllEnvs());
beforeEach(async()=>{f=await expenseFixture();for(const a of [f.owner,f.claimant,f.head,f.manager,f.reviewer])a.modules=['projects','finance'];});
async function newProject(){return saveProject(db,f.owner,null,projectSchema.parse({title:'Reviewed customer implementation',memberUserIds:[f.claimant.userId,f.head.userId,f.manager.userId,f.reviewer.userId]}),key());}
async function packageIn(projectId:string,title:string,parentId:string|null=null,depends:string[]=[],position=0){return saveProjectMilestone(db,f.owner,projectId,null,projectMilestoneSchema.parse({title,dueAt:'2026-09-20T11:00:00Z',ownerUserId:f.claimant.userId,parentMilestoneId:parentId,ordinalPosition:position,dependsOnMilestoneIds:depends}),key());}
async function command(projectId:string,input:Record<string,unknown>,actor=f.owner,receipt=key()){
 const p=await getProject(db,actor,projectId);return projectStructureCommand(actor,projectId,{expectedVersion:p.version,reason:'Verified normal owner source and retained review evidence.',...input},receipt);
}
async function complete(projectId:string,packageId:string,expectedVersion=1){return transitionProjectMilestone(db,f.owner,projectId,packageId,{action:'complete',expectedVersion,reason:'The required work and evidence have been completed.'},key());}
async function approvedExpense(){
 const journal=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-12',currency:'THB',sourceType:'manual',memo:'Approved project cost source',lines:[{accountId:f.accounts.expense,debit:'75.01',credit:'0'},{accountId:f.accounts.payable,debit:'0',credit:'75.01'}]}),key());
 await postJournal(f.manager,journal.id,key(),{expectedVersion:1});return `${journal.id}-0`;
}
async function grantProject(state:'active'|'draining'='active',endsAt:number|null=null,packageId='projects',companyId=f.company){
 const before=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId===packageId),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-provider'}),obligations:async()=>{throw new Error('PROJECT_FIXTURE_CANNOT_INACTIVATE');}});
 return store.apply({expectedVersion:before?.version??0,idempotencyKey:key(),grant:{companyId,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(before?.version??0)+1,reason:'Isolated actual Projects owner workflow',evidence:'test:project-purchase-owner'}});
}
const projectSettlementInput=(packageId='projects')=>({companyId:f.company,packageId,packageVersion:1});
function projectSettlement(packageId='projects'){return withOwnerPackageObligations(db,projectSettlementInput(packageId),tx=>readOwnerPackageObligations(db,tx,{...projectSettlementInput(packageId),now:Date.now()}));}
async function projectWithdrawal(packageId='projects'){
 const grants=await readCompanyPackageGrants(db,f.company),current=grants.find(grant=>grant.packageId===packageId)!;
 return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,
 grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual Project owner settlement reviewed',evidence:'test:projects-concrete-withdrawal'}}]};
}
function projectWithdrawalReview(request:Awaited<ReturnType<typeof projectWithdrawal>>,apply=false){
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-settlement-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});
 return withOwnerPackageObligations(db,projectSettlementInput(request.changes[0].grant.packageId),tx=>apply?store.batch.applyIn(tx,request,async()=>{}):store.batch.previewIn(tx,request));
}
it.each(['disabled','enforced'] as const)('keeps actual Projects-only member document bytes inside full native source and account checks (%s)',async mode=>{
 enableAttachmentV2ForTest();f.owner.modules=['projects'];f.head.modules=['projects'];f.tie.modules=['projects'];await grantProject();vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT',mode);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 const project=await newProject(),evidence=await seedAttachmentSet(db,f.owner,{targetType:'project',targetId:project.id,componentKind:'record',purposeKey:'project_document',expectedTargetVersion:1});await bindRecordAttachmentSet(db,f.owner,evidence.setId,1,key());
 const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_PROJECT_DOCUMENT_REQUIRED');const bytes='Retained original Project-only deliverable';await env.H001_FILES.put(objectKey,bytes);let consumed=0;
 const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),get:async(...args)=>{const result=await env.H001_FILES.get(...args);return result?{...result,arrayBuffer:async()=>{consumed++;return result.arrayBuffer();}}:null;},head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 for(const kind of ['source','membership','file','iam'] as const){
  const fault=`project_private_${key().replaceAll('-','')}`,effect=kind==='source'?`UPDATE staff_projects SET title='Late private Project source' WHERE id='${project.id}'`:kind==='membership'?`DELETE FROM staff_project_members WHERE project_id='${project.id}' AND user_id='${f.head.userId}'`:kind==='file'?`UPDATE attachments SET legal_hold=1 WHERE id='${evidence.sourceId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.head.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${evidence.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,files,f.head,evidence.linkId,'original')).rejects.toMatchObject(kind==='membership'?{status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'}:{code:'PROJECT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(evidence.linkId).first('n')).toBe(0);expect(await db.prepare('SELECT title FROM staff_projects WHERE id=?').bind(project.id).first('title')).toBe('Reviewed customer implementation');
 }
 expect(consumed).toBe(4);expect(await(await attachmentContent(db,files,f.head,evidence.linkId,'original')).object.text()).toBe(bytes);expect(consumed).toBe(5);
 await expect(db.transaction(tx=>attachmentContent(tx,files,f.head,evidence.linkId,'original'))).rejects.toMatchObject({code:'PROJECT_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(5);
 await expect(attachmentContent(db,files,f.tie,evidence.linkId,'original')).rejects.toMatchObject({status:404});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.tie.assignmentId).run();await expect(attachmentContent(db,files,f.tie,evidence.linkId,'original')).rejects.toMatchObject({status:404});
 expect(await db.prepare("SELECT count(*)::int n FROM attachment_access_events WHERE link_id=? AND actor_id=? AND outcome='denied'").bind(evidence.linkId,f.tie.userId).first('n')).toBe(2);expect(consumed).toBe(5);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);
});
it('waits for the actual native Project membership writer before private bytes and isolates two Company reads without retries',async()=>{
 const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');enableAttachmentV2ForTest();const other=await expenseFixture();for(const actor of [f.owner,f.head,other.owner,other.head])actor.modules=['projects'];
 await grantProject();await grantProject('active',null,'projects',other.company);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const rows:{fixture:typeof f;project:Awaited<ReturnType<typeof saveProject>>;evidence:Awaited<ReturnType<typeof seedAttachmentSet>>;bytes:string}[]=[];
 for(const fixture of [f,other]){const project=await saveProject(db,fixture.owner,null,projectSchema.parse({title:'Actual Company private Project source',memberUserIds:[fixture.head.userId]}),key()),evidence=await seedAttachmentSet(db,fixture.owner,{targetType:'project',targetId:project.id,componentKind:'record',purposeKey:'project_document',expectedTargetVersion:1});await bindRecordAttachmentSet(db,fixture.owner,evidence.setId,1,key());const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_PROJECT_PRIVATE_FILE_REQUIRED');const bytes=`Retained Project ${fixture.company}`;await env.H001_FILES.put(objectKey,bytes);rows.push({fixture,project,evidence,bytes});}
 const first=rows[0],writerKey=key(),fault=`project_bytes_writer_${key().replaceAll('-','')}`;await db.exec(`CREATE SEQUENCE ${fault}_fired;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fault}_fired');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${writerKey}') EXECUTE FUNCTION ${fault}()`);
 const writer=saveProject(db,f.owner,first.project.id,projectSchema.parse({title:'Actual Company private Project source',memberUserIds:[],expectedVersion:2}),writerKey);void writer.catch(()=>{});let gets=0;const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),get:async(...args)=>{gets++;return env.H001_FILES.get(...args);},head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 try{let ready=false;for(let attempt=0;attempt<150;attempt++){if(await db.prepare(`SELECT is_called FROM ${fault}_fired`).first<boolean>('is_called')){ready=true;break;}await new Promise(resolve=>setTimeout(resolve,10));}expect(ready).toBe(true);const observed=await measureReads(()=>attachmentContent(db,files,f.head,first.evidence.linkId,'original').then(value=>({value}),error=>({error})));await writer;expect(observed.result).toMatchObject({error:{status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'}});expect(gets).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await writer;await db.exec(`DROP TRIGGER ${fault} ON command_receipts;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}_fired`);}
 await saveProject(db,f.owner,first.project.id,projectSchema.parse({title:'Actual Company private Project source',memberUserIds:[f.head.userId],expectedVersion:3}),key());const parallel=`project_bytes_parallel_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${parallel}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE staff_projects SET title=title WHERE id=CASE WHEN NEW.company_id='${f.company}' THEN '${first.project.id}' ELSE '${rows[1].project.id}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${parallel} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${first.evidence.linkId}','${rows[1].evidence.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${parallel}()`);
 try{const observed=await measureReads(()=>Promise.all(rows.map(row=>attachmentContent(db,env.H001_FILES,row.fixture.head,row.evidence.linkId,'original'))));for(let index=0;index<rows.length;index++)expect(await observed.result[index].object.text()).toBe(rows[index].bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${parallel} ON attachment_access_events;DROP FUNCTION ${parallel}()`);}
});
it('withdraws Projects after its actual task, reviewed time and independent handoff settle while retaining held files and current-authority replay',async()=>{
 enableAttachmentV2ForTest();vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await grantProject();
 const p=await newProject(),pack=await packageIn(p.id,'Actual package delivery'),task=await saveTask(db,f.owner,null,taskSchema.parse({title:'Retained Project withdrawal task',assigneeUserId:f.claimant.userId,projectId:p.id}),key());
 await command(p.id,{action:'assign-task',taskId:task.id,taskVersion:1,milestoneId:pack.id,milestoneVersion:1,bindingVersion:0});
 const time=await saveProjectWorkLog(db,f.claimant,p.id,projectWorkLogSchema.parse({workDate:'2026-09-12',minutes:45}),key()),current=await getProject(db,f.owner,p.id),file=await seedAttachmentSet(db,f.owner,{targetType:'project',targetId:p.id,componentKind:'record',purposeKey:'project_document',expectedTargetVersion:current.version});
 await bindRecordAttachmentSet(db,f.owner,file.setId,current.version,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key')),body='Original Project deliverable retained under Legal hold';await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();
 await grantProject('draining');const opened=await projectSettlement();expect(opened.observations.map(row=>row.moduleId)).toEqual(['projects','people_payroll','finance']);expect(opened.total).toBeGreaterThan(0);expect(JSON.stringify(opened)).not.toMatch(/snapshot_json|owner_user_id|journal_line_id|evidence/);
 await expect(projectWithdrawalReview(await projectWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await transitionTask(db,f.claimant,task.id,taskActionSchema.parse({action:'start',expectedVersion:1}),key());await transitionTask(db,f.claimant,task.id,taskActionSchema.parse({action:'complete',expectedVersion:2,note:'Completed original retained delivery.'}),key());
 await decideProjectWorkLog(db,f.owner,p.id,time.id,projectWorkLogDecisionSchema.parse({action:'approve',expectedVersion:1,reason:'Independent review of actual delivery time.'}),key());await complete(p.id,pack.id);await command(p.id,{action:'link-actual',milestoneId:pack.id,milestoneVersion:2,sourceKind:'work_log',sourceId:time.id,sourceVersion:2});
 const issued=await command(p.id,{action:'prepare-handoff',recipientId:f.head.userId,evidence:'Original reviewed delivery and held private evidence.'}),before=await getProject(db,f.head,p.id),receipt=key(),input={action:'accept-handoff',expectedVersion:before.version,handoffId:issued.handoffId,handoffVersion:1,reason:'Independent recipient accepts original completed delivery.'};
 const accepted=await projectStructureCommand(f.head,p.id,input,receipt);expect((await projectSettlement()).total).toBe(0);
 const request=await projectWithdrawal(),preview=await projectWithdrawalReview(request),applied=await projectWithdrawalReview(request,true);expect(applied).toEqual(preview);expect(await projectWithdrawalReview(request,true)).toEqual(applied);expect(await projectStructureCommand(f.head,p.id,input,receipt)).toEqual(accepted);
 expect((await projectGraph(f.owner,p.id)).totals.approvedMinutes).toBe(45);expect(await (await attachmentContent(db,env.H001_FILES,f.owner,file.linkId,'original')).object.text()).toBe(body);expect((await readCompanyPackageGrants(db,f.company)).map(grant=>[grant.packageId,grant.state])).toEqual([['projects','inactive']]);
 await expect(newProject()).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.head.assignmentId).run();await expect(projectStructureCommand(f.head,p.id,input,receipt)).rejects.toMatchObject({code:'PROJECT_ACCESS_CHANGED'});expect((await projectSettlement()).total).toBe(0);
});
it('withdraws only the settled Project Costs addon without closing an independently active basic project or its retained posted expense',async()=>{
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');
 await grantProject();await grantProject('active',null,'projects-costs');const p=await newProject(),line=await approvedExpense(),allocation=await createProjectCostAllocation(f.owner,projectCostAllocationSchema.parse({projectId:p.id,sourceLineId:line,amount:'25.01'}),key());
 await grantProject('draining',null,'projects-costs');expect((await projectSettlement('projects-costs')).total).toBeGreaterThan(0);await expect(projectWithdrawalReview(await projectWithdrawal('projects-costs'))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const receipt=key(),input=projectCostAllocationDecisionSchema.parse({action:'approve',expectedVersion:1,reason:'Independent actual posted expense allocation review.'});await decideProjectCostAllocation(f.manager,allocation.id,input,receipt);expect((await projectSettlement('projects-costs')).total).toBe(0);
 const request=await projectWithdrawal('projects-costs');await projectWithdrawalReview(request);await projectWithdrawalReview(request,true);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');expect((await getProject(db,f.owner,p.id)).lifecycle_status).toBe('active');expect((await projectSettlement()).total).toBeGreaterThan(0);expect((await listProjectCostAllocations(f.owner,p.id)).map(row=>row.id)).toContain(allocation.id);await expect(decideProjectCostAllocation(f.manager,allocation.id,input,receipt)).resolves.toMatchObject({id:allocation.id,state:'approved'});
});
it('requires minted Project source leases and refuses legacy completed headers without an independently accepted source receipt',async()=>{
 const input={...projectSettlementInput(),moduleId:'projects',now:Date.now()};await expect(db.transaction(tx=>projectPackageObligations(db,tx,input))).rejects.toThrow('PROJECT_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...projectSettlementInput(),packageVersion:2},tx=>projectPackageObligations(db,tx,{...input,packageVersion:2}))).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');await expect(withOwnerPackageObligations(db,projectSettlementInput(),tx=>projectPackageObligations(db,tx,{...input,moduleId:'finance'}))).rejects.toThrow('PROJECT_PACKAGE_OBLIGATION_SCOPE_REQUIRED');await expect(withOwnerPackageObligations(db,projectSettlementInput(),tx=>projectPackageObligations(db,tx,{...input,companyId:'foreign-company'}))).rejects.toThrow('PROJECT_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect((await projectSettlement()).total).toBe(0);
 await grantProject();const p=await newProject();await db.prepare("UPDATE staff_projects SET lifecycle_status='completed',version=version+1 WHERE id=?").bind(p.id).run();await grantProject('draining');expect((await projectSettlement()).total).toBeGreaterThan(0);await expect(projectWithdrawalReview(await projectWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
});
it('keeps unposted and foreign original Project cost lineage as raw Finance withdrawal obligations',async()=>{
 await grantProject();await grantProject('active',null,'projects-costs');const p=await newProject(),now=Date.now(),foreign=key(),first=key(),second=key();
 const journal=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-12',currency:'THB',sourceType:'manual',memo:'Original unresolved legacy Project cost',lines:[{accountId:f.accounts.expense,debit:'20.00',credit:'0'},{accountId:f.accounts.payable,debit:'0',credit:'20.00'}]}),key());
 await db.prepare("INSERT INTO staff_projects(id,company_id,title,owner_user_id,created_at,updated_at) VALUES(?,'foreign-company','Foreign retained source',?,?,?)").bind(foreign,f.owner.userId,now,now).run();
 for(const [id,projectId] of [[first,p.id],[second,foreign]])await db.prepare("INSERT INTO staff_project_cost_allocations(id,company_id,project_id,journal_line_id,amount_minor,state,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,500,'approved',?,?,?,?)").bind(id,f.company,projectId,`${journal.id}-0`,f.owner.userId,f.manager.userId,now,now).run();
 await grantProject('draining',null,'projects-costs');const observed=await projectSettlement('projects-costs');expect(observed.observations.find(row=>row.moduleId==='finance')?.outstanding).toBe(2);expect(observed.observations.find(row=>row.moduleId==='projects')?.outstanding).toBe(2);await expect(projectWithdrawalReview(await projectWithdrawal('projects-costs'))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 expect(await db.prepare('SELECT status FROM finance_journals WHERE id=?').bind(journal.id).first('status')).toBe('draft');expect(await db.prepare('SELECT COUNT(*)::int count FROM staff_project_cost_allocations WHERE id IN (?,?)').bind(first,second).first('count')).toBe(2);
});
it('rolls Commercial Project inactivation and its audit receipt back after a final deferred real Project obligation',async()=>{
 await grantProject();const p=await newProject(),issued=await command(p.id,{action:'prepare-handoff',recipientId:f.head.userId,evidence:'Completed empty delivery reviewed independently.'});await command(p.id,{action:'accept-handoff',handoffId:issued.handoffId,handoffVersion:1},f.head);await grantProject('draining');expect((await projectSettlement()).total).toBe(0);
 const request=await projectWithdrawal(),name=`project_final_settlement_${key().replaceAll('-','')}`;await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE staff_projects SET lifecycle_status='active',version=version+1 WHERE id='${p.id}'; RETURN NEW; END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON company_package_change_sets DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.id='${request.idempotencyKey}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(projectWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');}finally{await db.prepare(`DROP TRIGGER ${name} ON company_package_change_sets`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect((await getProject(db,f.owner,p.id)).lifecycle_status).toBe('completed');expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='projects'").bind(f.company).first()).toEqual({state:'draining',version:2});expect(await db.prepare('SELECT 1 FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first()).toBeNull();expect((await projectSettlement()).total).toBe(0);await projectWithdrawalReview(request,true);
});
it('observes a committed original Project writer only after its fresh settlement source leases become available',async()=>{
 await grantProject();await grantProject('draining');const request=await projectWithdrawal(),id=`project-concurrent-${key()}`,now=Date.now();let inserted!:()=>void,release!:()=>void;const ready=new Promise<void>(resolve=>{inserted=resolve;}),released=new Promise<void>(resolve=>{release=resolve;});
 const writer=db.transaction(async tx=>{await tx.prepare('LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE').run();await tx.prepare("INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,owner_user_id,created_at,updated_at) VALUES(?,?,?,?,'Actual concurrent source',?,?,?)").bind(id,f.company,f.branch,f.department,f.owner.userId,now,now).run();inserted();await released;});await ready;const review=projectWithdrawalReview(request).then(value=>({value,error:null}),error=>({value:null,error}));
 try{let waiting=false;for(let attempt=0;attempt<100;attempt++){waiting=Boolean(await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='company_package_grants'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first());if(waiting)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(waiting).toBe(true);}finally{release();await writer;}
 expect((await review).error).toMatchObject({message:'PACKAGE_OBLIGATIONS_REMAIN'});expect((await projectSettlement()).total).toBeGreaterThan(0);expect(await db.prepare('SELECT lifecycle_status FROM staff_projects WHERE id=?').bind(id).first('lifecycle_status')).toBe('active');
});
it('runs Projects-only WBS, reviewed time and independent handoff while People is enforced, retaining draining settlement and exact history replay',async()=>{
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const receipt=key(),input=projectSchema.parse({title:'Projects-only customer delivery',memberUserIds:[f.claimant.userId,f.head.userId]});
 await expect(saveProject(db,f.owner,null,input,receipt)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await expect(listProjects(db,f.owner)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await grantProject();const p=await saveProject(db,f.owner,null,input,receipt),pack=await packageIn(p.id,'Independent delivery package');
 const taskInput=taskSchema.parse({title:'Actual Projects-only delivery task',assigneeUserId:f.claimant.userId,projectId:p.id}),taskKey=key(),task=await saveTask(db,f.owner,null,taskInput,taskKey);
 await command(p.id,{action:'assign-task',taskId:task.id,taskVersion:1,milestoneId:pack.id,milestoneVersion:1,bindingVersion:0});
 const time=await saveProjectWorkLog(db,f.claimant,p.id,projectWorkLogSchema.parse({workDate:'2026-09-12',minutes:90}),key());
 await grantProject('draining');
 expect(await saveProject(db,f.owner,null,input,receipt)).toEqual(p);
 expect(await saveTask(db,f.owner,null,taskInput,taskKey)).toEqual(task);
 await expect(saveTask(db,f.owner,null,{...taskInput,title:'New Project task after draining'},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 const delegated=await delegateTask(db,f.owner,task.id,{delegateUserId:f.head.userId,expiresAt:new Date(Date.now()+60000).toISOString(),expectedVersion:1,reason:'Retained task routing while draining.'},key());
 await transitionTask(db,f.head,task.id,taskActionSchema.parse({action:'start',expectedVersion:delegated.version}),key());
 await transitionTask(db,f.head,task.id,taskActionSchema.parse({action:'complete',expectedVersion:delegated.version+1,note:'Existing work completed without People purchase.'}),key());
 expect((await taskDetail(db,f.owner,task.id)).task.state).toBe('completed');
 await expect(newProject()).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await expect(packageIn(p.id,'New package while draining')).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await expect(saveProjectWorkLog(db,f.claimant,p.id,projectWorkLogSchema.parse({workDate:'2026-09-13',minutes:20}),key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await decideProjectWorkLog(db,f.owner,p.id,time.id,projectWorkLogDecisionSchema.parse({action:'approve',expectedVersion:1,reason:'Independent retained source review.'}),key());
 await complete(p.id,pack.id);await command(p.id,{action:'link-actual',milestoneId:pack.id,milestoneVersion:2,sourceKind:'work_log',sourceId:time.id,sourceVersion:2});
 const issued=await command(p.id,{action:'prepare-handoff',recipientId:f.head.userId,evidence:'Reviewed actual approved work and retained source evidence.'});
 await command(p.id,{action:'accept-handoff',handoffId:issued.handoffId,handoffVersion:1},f.head);
 expect((await projectGraph(f.owner,p.id)).totals).toEqual({approvedMinutes:90,costByCurrency:{}});
 expect((await projectHandoffs(f.head,p.id))[0]).toMatchObject({status:'accepted',snapshot:{canReadCosts:false,totals:{approvedMinutes:0,costByCurrency:{}}}});
 await expect(listProjectCostAllocations(f.owner,p.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const accepted=await getProject(db,f.owner,p.id);expect(accepted.lifecycle_status).toBe('completed');
 await expect(command(p.id,{action:'reopen',handoffId:issued.handoffId,handoffVersion:2},f.head)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();
 await expect(getProject(db,f.owner,p.id)).rejects.toMatchObject({code:'PROJECT_ACCESS_CHANGED'});
 await expect(saveProject(db,f.owner,null,input,receipt)).rejects.toMatchObject({code:'PROJECT_ACCESS_CHANGED'});
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);
});
it('filters actual Project task sources before the Work limit while preserving unrelated tasks and rejecting deferred source reclassification',async()=>{
 const p=await newProject(),other=await newProject(),projectTask=await saveTask(db,f.owner,null,taskSchema.parse({title:'Actual private project source',assigneeUserId:f.claimant.userId,projectId:p.id}),key()),ordinary=await saveTask(db,f.owner,null,taskSchema.parse({title:'Ordinary Work owner task',assigneeUserId:f.claimant.userId}),key());
 await db.prepare(`INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,project_id,state,created_at,updated_at)
 SELECT ?||'-'||n,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,project_id,state,created_at,updated_at+n+10000 FROM staff_tasks CROSS JOIN generate_series(1,255) n WHERE id=?`).bind(key(),projectTask.id).run();
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');
 expect((await listTasks(db,f.claimant)).map(row=>row.id)).toEqual([ordinary.id]);
 await expect(getTask(db,f.claimant,projectTask.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await expect(transitionTask(db,f.claimant,projectTask.id,taskActionSchema.parse({action:'start',expectedVersion:1}),key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const free=await saveTask(db,f.owner,null,taskSchema.parse({title:'Unrelated Work remains its own owner',assigneeUserId:f.claimant.userId}),key());expect((await getTask(db,f.claimant,free.id)).project_id).toBeNull();
 await grantProject();expect((await listTasks(db,f.claimant)).filter(row=>row.project_id===p.id)).toHaveLength(250);
 const receipt=key(),name=`project_task_source_${key().replaceAll('-','')}`,input=taskActionSchema.parse({action:'start',expectedVersion:1});
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE staff_tasks SET project_id='${other.id}' WHERE id='${projectTask.id}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${receipt}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(transitionTask(db,f.claimant,projectTask.id,input,receipt)).rejects.toMatchObject({code:'PROJECT_TASK_SOURCE_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await db.prepare('SELECT project_id,state,version FROM staff_tasks WHERE id=?').bind(projectTask.id).first()).toEqual({project_id:p.id,state:'ready',version:1});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 const started=await transitionTask(db,f.claimant,projectTask.id,input,receipt);expect(await transitionTask(db,f.claimant,projectTask.id,input,receipt)).toEqual(started);
 await grantProject('draining');expect((await taskDetail(db,f.claimant,projectTask.id)).permittedActions).toContain('complete');
});
it('applies Project purchase and exact member/actual policy before shared Records, search, notification and Work limits without deleting durable sources',async()=>{
 const p=await newProject(),foreign=await saveProject(db,f.manager,null,projectSchema.parse({title:'Private Finance-owned project'}),key()),time=await saveProjectWorkLog(db,f.head,p.id,projectWorkLogSchema.parse({workDate:'2026-09-12',minutes:13}),key()),now=Date.now();
 const record=async(id:string,title:string)=>db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,created_by,updated_by,created_at,updated_at) VALUES('projects',?,'projects',?,'','draft','[]',?,?,?,?,?)").bind(id,title,f.company,f.owner.userId,f.owner.userId,now,now).run();
 await record(p.id,'Projected accessible project');await record(foreign.id,'Projected private foreign project');await record(time.id,'Projected other employee work time');
 const issue=await command((await newProject()).id,{action:'prepare-handoff',recipientId:f.head.userId,evidence:'Complete retained empty-scope delivery.'});
 for(let index=0;index<106;index++)await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'project_handoff',?,1,'Missing source must not consume page',?)").bind(key(),f.company,f.head.userId,`missing-project-handoff-${index}`,now+1000+index).run();
 await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:'project_handoff',sourceId:String(issue.handoffId),sourceVersion:1,category:'work',preview:'Review retained project handoff',recipientIds:[f.head.userId]});
 const before=await db.prepare('SELECT (SELECT count(*) FROM staff_notifications WHERE company_id=?) staff,(SELECT count(*) FROM inbox_events WHERE company_id=?) inbox').bind(f.company,f.company).first();
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');
 expect(await listRecords(f.owner,'projects','projects')).toEqual([]);expect((await recordsPage(f.owner,'projects','projects',{})).items).toEqual([]);expect(await searchReferences(f.owner,'Projected')).toEqual([]);
 await expect(getRecord(f.owner,'projects',p.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await listStaffNotifications(db,f.head)).toEqual([]);expect((await inboxPage(db,f.head,{view:'all'})).items).toEqual([]);
 await grantProject();await grantProject('draining');
 expect((await listRecords(f.owner,'projects','projects')).map(row=>row.id).sort()).toEqual([p.id,time.id].sort());
 expect((await listRecords(f.claimant,'projects','projects')).map(row=>row.id)).toEqual([p.id]);expect(await searchReferences(f.claimant,'Projected')).toEqual([expect.objectContaining({id:p.id})]);
 await expect(getRecord(f.owner,'projects',foreign.id)).rejects.toMatchObject({code:'PROJECT_SOURCE_FORBIDDEN'});await expect(getRecord(f.claimant,'projects',time.id)).rejects.toMatchObject({code:'PROJECT_SOURCE_FORBIDDEN'});
 expect((await listStaffNotifications(db,f.head)).map(row=>row.record_id)).toEqual([String(issue.handoffId)]);expect((await inboxPage(db,f.head,{view:'all'})).items.length).toBeGreaterThan(0);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.head.assignmentId).run();
 expect(await listStaffNotifications(db,f.head)).toEqual([]);expect((await inboxPage(db,f.head,{view:'all'})).items).toEqual([]);
 expect(await db.prepare('SELECT (SELECT count(*) FROM staff_notifications WHERE company_id=?) staff,(SELECT count(*) FROM inbox_events WHERE company_id=?) inbox').bind(f.company,f.company).first()).toEqual(before);
});
it('prevents a retained Project task from regaining submit authority during Flow enrichment while preserving ordinary Work',async()=>{
 const p=await newProject(),draft=await saveTask(db,f.owner,null,taskSchema.parse({title:'Reviewed Project task draft',assigneeUserId:f.claimant.userId,projectId:p.id,dueAt:new Date(Date.now()+86400000).toISOString(),startApprovers:[f.head.userId]}),key()),free=await saveTask(db,f.owner,null,taskSchema.parse({title:'Unrelated Work stays visible',assigneeUserId:f.owner.userId}),key());
 const items=await getStaffWork(f.owner),raw=items.find(row=>row.id===draft.id)!;expect(raw.canAct).toBe(true);
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');expect((await getStaffWork(f.owner)).map(row=>row.id)).toContain(free.id);expect((await getStaffWork(f.owner)).map(row=>row.id)).not.toContain(draft.id);expect(await enrichFlowWork(f.owner,[raw])).toEqual([]);
 await grantProject();await grantProject('draining');
 const retained=(await getStaffWork(f.owner)).find(row=>row.id===draft.id)!;expect(retained.canAct).toBe(false);expect((await enrichFlowWork(f.owner,[{...retained,canAct:true}]))[0].canAct).toBe(false);
});
it('keeps Project membership policy in shadow and rejects supplied transactions or deferred source/current-authority changes without partial receipts',async()=>{
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','shadow');const p=await newProject();
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');await expect(projectGraph(f.owner,p.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await grantProject();
 await expect(db.transaction(tx=>saveProject(tx,f.owner,null,projectSchema.parse({title:'Unowned supplied unit'}),key()))).rejects.toMatchObject({code:'PROJECT_TRANSACTION_REQUIRED'});
 const receipt=key(),before=await getProject(db,f.owner,p.id),input={title:'Atomic owner brief revision',expectedVersion:before.version,memberUserIds:[f.claimant.userId,f.head.userId,f.manager.userId,f.reviewer.userId]},name=`project_purchase_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE staff_projects SET branch_id=NULL WHERE id='${p.id}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${receipt}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(saveProject(db,f.owner,p.id,projectSchema.parse(input),receipt)).rejects.toMatchObject({code:'PROJECT_SOURCE_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await getProject(db,f.owner,p.id)).toEqual(before);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 const saved=await saveProject(db,f.owner,p.id,projectSchema.parse(input),receipt);expect(saved.version).toBe(before.version+1);
 const revoked=key(),revision=projectSchema.parse({...input,title:'Final current authority test',expectedVersion:saved.version}),revokeName=`project_revoke_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${revokeName}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${revokeName} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${revoked}') EXECUTE FUNCTION ${revokeName}()`).run();
 try{await expect(saveProject(db,f.owner,p.id,revision,revoked)).rejects.toMatchObject({code:'PROJECT_ACCESS_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${revokeName} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${revokeName}()`).run();}
 expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:null});expect((await getProject(db,f.owner,p.id)).version).toBe(saved.version);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(revoked).first()).toBeNull();
 expect(await saveProject(db,f.owner,p.id,revision,revoked)).toMatchObject({version:saved.version+1});
});
it('rolls back newly prepared Project work after final purchase expiry, then retries the original key once',async()=>{
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');await grantProject('active',Date.now()+1800);
 const receipt=key(),input=projectSchema.parse({title:'Final purchase clock project'}),name=`project_clock_${key().replaceAll('-','')}`;
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(1.9); RETURN NEW; END $$`).run();await db.prepare(`CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN (NEW.command_id='${receipt}') EXECUTE FUNCTION ${name}()`).run();
 try{await expect(saveProject(db,f.owner,null,input,receipt)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${name} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 expect(await db.prepare('SELECT 1 FROM staff_projects WHERE id=?').bind(`project-${receipt}`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 await grantProject();const result=await saveProject(db,f.owner,null,input,receipt);expect(await saveProject(db,f.owner,null,input,receipt)).toEqual(result);expect(await db.prepare('SELECT count(*) n FROM audit_events WHERE command_id=?').bind(receipt).first()).toEqual({n:1});
});
it('closes hierarchical work packages with real tasks, independently reviewed time and posted costs, and preserves every issued handoff after rejection, acceptance and reopening',async()=>{
 await grantProject();await grantProject('active',null,'projects-costs');await grantProject('active',null,'finance');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const p=await newProject(),root=await packageIn(p.id,'Customer delivery'),discovery=await packageIn(p.id,'Approved discovery',root.id,[],1),build=await packageIn(p.id,'Implemented release',root.id,[discovery.id],2);
 const task=await saveTask(db,f.owner,null,taskSchema.parse({title:'Implement approved customer change',assigneeUserId:f.claimant.userId,projectId:p.id}),key());
 const bind=await command(p.id,{action:'assign-task',taskId:task.id,taskVersion:1,milestoneId:build.id,milestoneVersion:1,bindingVersion:0});
 expect(bind).toMatchObject({bindingVersion:1,status:'active'});
 await expect(complete(p.id,root.id)).rejects.toMatchObject({code:'PROJECT_PACKAGE_CHILDREN_INCOMPLETE'});
 await expect(complete(p.id,build.id)).rejects.toMatchObject({code:'PROJECT_DEPENDENCIES_INCOMPLETE'});
 await complete(p.id,discovery.id);
 await expect(complete(p.id,build.id)).rejects.toMatchObject({code:'PROJECT_PACKAGE_TASKS_INCOMPLETE'});
 await transitionTask(db,f.claimant,task.id,taskActionSchema.parse({action:'start',expectedVersion:1}),key());
 await transitionTask(db,f.claimant,task.id,taskActionSchema.parse({action:'complete',expectedVersion:2,note:'Customer release verified.'}),key());
 const timeInput=projectWorkLogSchema.parse({workDate:'2026-09-12',minutes:125}),timeKey=key(),time=await saveProjectWorkLog(db,f.claimant,p.id,timeInput,timeKey);
 expect(await saveProjectWorkLog(db,f.claimant,p.id,timeInput,timeKey)).toEqual(time);
 await decideProjectWorkLog(db,f.owner,p.id,time.id,projectWorkLogDecisionSchema.parse({action:'approve',expectedVersion:1,reason:'Independently reviewed work time.'}),key());
 const expense=await approvedExpense(),costInput=projectCostAllocationSchema.parse({projectId:p.id,sourceLineId:expense,amount:'75.01'}),costKey=key(),cost=await createProjectCostAllocation(f.owner,costInput,costKey);
 expect(await createProjectCostAllocation(f.owner,costInput,costKey)).toEqual(cost);
 await decideProjectCostAllocation(f.manager,cost.id,projectCostAllocationDecisionSchema.parse({action:'approve',expectedVersion:1,reason:'Independent posted cost allocation review.'}),key());
 await complete(p.id,build.id);await complete(p.id,root.id);
 await expect(command(p.id,{action:'prepare-handoff',recipientId:f.head.userId,evidence:'Completed customer delivery.'})).rejects.toMatchObject({code:'PROJECT_HANDOFF_INCOMPLETE'});
 await command(p.id,{action:'link-actual',milestoneId:build.id,milestoneVersion:2,sourceKind:'work_log',sourceId:time.id,sourceVersion:2});
 await command(p.id,{action:'link-actual',milestoneId:build.id,milestoneVersion:2,sourceKind:'cost_allocation',sourceId:cost.id,sourceVersion:2});
 const graph=await projectGraph(f.owner,p.id);
 expect(graph.packages.map(row=>[row.wbs,row.title,row.depth])).toEqual([['1','Customer delivery',0],['1.1','Approved discovery',1],['1.2','Implemented release',1]]);
 expect(graph.edges).toContainEqual({from:discovery.id,to:build.id,kind:'requires'});expect(graph.edges).toContainEqual({from:root.id,to:build.id,kind:'contains'});expect(graph.edges).toContainEqual({from:build.id,to:task.id,kind:'task'});
 expect(graph.packages[2]).toMatchObject({approvedMinutes:125,costByCurrency:{THB:'7501'},tasks:[{id:task.id,state:'completed',version:3}]});expect(graph.totals).toEqual({approvedMinutes:125,costByCurrency:{THB:'7501'}});
 const own=await projectGraph(f.claimant,p.id),other=await projectGraph(f.head,p.id);
 expect(own.totals).toEqual({approvedMinutes:125,costByCurrency:{}});expect(own.actuals.map(a=>a.source_kind)).toEqual(['work_log']);expect(other.totals).toEqual({approvedMinutes:0,costByCurrency:{}});
 await expect(command(p.id,{action:'prepare-handoff',recipientId:f.owner.userId,evidence:'Self review attempt.'})).rejects.toMatchObject({code:'INDEPENDENT_HANDOFF_REQUIRED'});
 const prepareInput={action:'prepare-handoff',expectedVersion:graph.project.version,recipientId:f.head.userId,evidence:'Reviewed release files and complete customer handoff.',reason:'Ready for independent receiving review.'},prepareKey=key();
 const first=await projectStructureCommand(f.owner,p.id,prepareInput,prepareKey);expect(await projectStructureCommand(f.owner,p.id,prepareInput,prepareKey)).toEqual(first);
 expect(first).toMatchObject({status:'handoff_pending'});
 const id=String(first.handoffId),raw=await db.prepare('SELECT snapshot_json::text snapshot,snapshot_sha256 FROM staff_project_handoffs WHERE id=?').bind(id).first<{snapshot:string;snapshot_sha256:string}>();
 expect(raw?.snapshot_sha256).toHaveLength(64);
 expect((await projectHandoffWork(f.head)).find(item=>item.id===id)).toMatchObject({type:'project_handoff',queue:'todo',canAct:true,source:{version:1},href:`/projects/${p.id}#project-structure`});expect((await projectHandoffWork(f.owner)).find(item=>item.id===id)).toMatchObject({queue:'waiting',canAct:false});
 expect((await listStaffNotifications(db,f.head)).find(item=>item.record_id===id)).toMatchObject({record_type:'project_handoff',href:`/projects/${p.id}#project-structure`});
 expect((await projectHandoffs(f.owner,p.id))[0].snapshot.totals).toEqual(graph.totals);expect((await projectHandoffs(f.head,p.id))[0].snapshot.totals).toEqual({approvedMinutes:0,costByCurrency:{}});
 await expect(saveProject(db,f.owner,p.id,projectSchema.parse({title:'Attempt to edit issued work',memberUserIds:[f.head.userId],expectedVersion:Number(first.version)}),key())).rejects.toMatchObject({code:'PROJECT_STRUCTURE_FROZEN'});
 await expect(saveProjectWorkLog(db,f.claimant,p.id,projectWorkLogSchema.parse({workDate:'2026-09-13',minutes:20}),key())).rejects.toMatchObject({code:'PROJECT_STRUCTURE_FROZEN'});
 await expect(command(p.id,{action:'accept-handoff',handoffId:id,handoffVersion:1})).rejects.toMatchObject({code:'PROJECT_RECIPIENT_REQUIRED'});
 await command(p.id,{action:'reject-handoff',handoffId:id,handoffVersion:1},f.head);
 const second=await command(p.id,{action:'prepare-handoff',recipientId:f.head.userId,evidence:'Reviewed revised acceptance evidence.'}),secondId=String(second.handoffId);
 await command(p.id,{action:'accept-handoff',handoffId:secondId,handoffVersion:1},f.head);
 expect((await getProject(db,f.owner,p.id)).lifecycle_status).toBe('completed');
 expect((await projectHandoffWork(f.head)).find(item=>item.id===secondId)).toMatchObject({queue:'completed',canAct:false,source:{version:2}});
 await expect(db.prepare('UPDATE staff_project_handoffs SET evidence=? WHERE id=?').bind('Overwritten issued artifact',id).run()).rejects.toMatchObject({code:'23514'});
 await expect(db.prepare('DELETE FROM staff_project_package_actuals WHERE source_id=?').bind(time.id).run()).rejects.toMatchObject({code:'23514'});
 await expect(db.prepare('UPDATE staff_project_milestones SET title=? WHERE id=?').bind('Changed accepted package',build.id).run()).rejects.toMatchObject({code:'23514'});
 await command(p.id,{action:'reopen',handoffId:secondId,handoffVersion:2},f.head);
 const current=await getProject(db,f.owner,p.id);await saveProject(db,f.owner,p.id,projectSchema.parse({title:'Reopened customer implementation',expectedVersion:current.version,memberUserIds:[f.claimant.userId,f.head.userId,f.manager.userId,f.reviewer.userId]}),key());
 expect((await projectHandoffs(f.owner,p.id)).map(h=>h.status).sort()).toEqual(['accepted','rejected']);
 expect((await db.prepare('SELECT snapshot_json::text snapshot,snapshot_sha256 FROM staff_project_handoffs WHERE id=?').bind(id).first())).toEqual(raw);
 expect((await projectGraph(f.owner,p.id)).totals).toEqual({approvedMinutes:125,costByCurrency:{THB:'7501'}});
 expect(await db.prepare('SELECT count(*) n FROM audit_events WHERE command_id=?').bind(prepareKey).first()).toEqual({n:1});
});
it('rejects hierarchy and dependency deadlocks, stale task sources, competing structure commands and revoked membership without partial WBS writes',async()=>{
 const p=await newProject(),root=await packageIn(p.id,'Package parent'),child=await packageIn(p.id,'Dependent child',root.id);
 const rootRow=(await projectGraph(f.owner,p.id)).packages.find(row=>row.id===root.id)!;
 await expect(saveProjectMilestone(db,f.owner,p.id,root.id,projectMilestoneSchema.parse({title:rootRow.title,dueAt:new Date(rootRow.due_at).toISOString(),ownerUserId:rootRow.owner_user_id,parentMilestoneId:child.id,expectedVersion:1}),key())).rejects.toMatchObject({code:'PROJECT_PACKAGE_CYCLE_OR_DEPTH'});
 await expect(saveProjectMilestone(db,f.owner,p.id,child.id,projectMilestoneSchema.parse({title:'Deadlocked child',dueAt:'2026-09-20T11:00:00Z',ownerUserId:f.claimant.userId,dependsOnMilestoneIds:[root.id],expectedVersion:1}),key())).rejects.toMatchObject({code:'PROJECT_EXECUTION_GRAPH_CYCLE'});
 expect((await projectGraph(f.owner,p.id)).packages.find(row=>row.id===child.id)).toMatchObject({version:1,depends_on_ids:[]});
 const task=await saveTask(db,f.owner,null,taskSchema.parse({title:'Concrete independently tracked work',assigneeUserId:f.claimant.userId,projectId:p.id}),key());
 await expect(command(p.id,{action:'assign-task',taskId:task.id,taskVersion:99,milestoneId:child.id,milestoneVersion:1,bindingVersion:0})).rejects.toMatchObject({code:'PROJECT_TASK_BINDING_CHANGED'});
 const version=(await getProject(db,f.owner,p.id)).version,input={action:'assign-task',expectedVersion:version,taskId:task.id,taskVersion:1,milestoneId:child.id,milestoneVersion:1,bindingVersion:0,reason:'Competing package assignment must commit only once.'},results=await Promise.allSettled([projectStructureCommand(f.owner,p.id,input,key()),projectStructureCommand(f.owner,p.id,input,key())]);
 expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);expect(await db.prepare('SELECT count(*) n FROM staff_project_task_packages WHERE project_id=?').bind(p.id).first()).toEqual({n:1});expect((await getProject(db,f.owner,p.id)).version).toBe(version+1);
 await db.prepare('DELETE FROM staff_project_members WHERE project_id=? AND user_id=?').bind(p.id,f.claimant.userId).run();await expect(projectGraph(f.claimant,p.id)).rejects.toMatchObject({status:404});
 await expect(projectGraph({...f.owner,companyId:'another-company'},p.id)).rejects.toMatchObject({status:404});
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();await expect(projectGraph(f.owner,p.id)).rejects.toMatchObject({status:404});
});
it('rolls back issued Projects evidence, Collaboration notification and command journal on late outbox failure, then retries and replays once',async()=>{
 const p=await newProject(),receipt=key(),handoffId=`project-handoff-${receipt}`,input={action:'prepare-handoff',expectedVersion:1,recipientId:f.head.userId,evidence:'Complete independently reviewed source evidence.',reason:'Receiving member must retain this snapshot.'},suffix=key().replace(/[^a-zA-Z0-9]/g,''),fault=`project_handoff_fault_${suffix}`,trigger=`project_handoff_outbox_${suffix}`;
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${receipt}' THEN RAISE EXCEPTION 'PROJECT_HANDOFF_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{
  await expect(projectStructureCommand(f.owner,p.id,input,receipt)).rejects.toThrow('PROJECT_HANDOFF_LATE_FAILURE');
  expect(await db.prepare('SELECT lifecycle_status,version FROM staff_projects WHERE id=?').bind(p.id).first()).toEqual({lifecycle_status:'active',version:1});
  expect(await db.prepare('SELECT count(*) n FROM staff_project_handoffs WHERE id=?').bind(handoffId).first()).toEqual({n:0});
  expect(await db.prepare('SELECT count(*) n FROM staff_project_package_events WHERE id=?').bind(`project-package-event-${receipt}`).first()).toEqual({n:0});
  expect(await db.prepare('SELECT count(*) n FROM staff_notifications WHERE record_id=?').bind(handoffId).first()).toEqual({n:0});
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(receipt).first()).toEqual({n:0});
 }finally{await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const result=await projectStructureCommand(f.owner,p.id,input,receipt);expect(result).toMatchObject({status:'handoff_pending',version:2,handoffId});
 expect(await projectStructureCommand(f.owner,p.id,input,receipt)).toEqual(result);
 expect(await db.prepare('SELECT count(*) n FROM staff_project_handoffs WHERE id=?').bind(handoffId).first()).toEqual({n:1});
 expect(await db.prepare('SELECT count(*) n FROM staff_notifications WHERE record_id=?').bind(handoffId).first()).toEqual({n:1});
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(receipt).first()).toEqual({n:1});
});
it('rolls back independent handoff acceptance and its owner notice after late outbox failure, retaining the issued artifact for same-key retry',async()=>{
 const p=await newProject(),issued=await command(p.id,{action:'prepare-handoff',recipientId:f.head.userId,evidence:'Frozen source evidence for receiving review.'}),handoffId=String(issued.handoffId),receipt=key(),input={action:'accept-handoff',expectedVersion:2,handoffId,handoffVersion:1,reason:'Independent member verified completed deliverables.'};
 const before=await db.prepare('SELECT snapshot_json::text snapshot,snapshot_sha256,status,version FROM staff_project_handoffs WHERE id=?').bind(handoffId).first(),suffix=key().replace(/[^a-zA-Z0-9]/g,''),fault=`project_accept_fault_${suffix}`,trigger=`project_accept_outbox_${suffix}`;
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${receipt}' THEN RAISE EXCEPTION 'PROJECT_ACCEPT_LATE_FAILURE'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE TRIGGER ${trigger} AFTER INSERT ON outbox_events FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{
  await expect(projectStructureCommand(f.head,p.id,input,receipt)).rejects.toThrow('PROJECT_ACCEPT_LATE_FAILURE');
  expect(await db.prepare('SELECT lifecycle_status,version FROM staff_projects WHERE id=?').bind(p.id).first()).toEqual({lifecycle_status:'handoff_pending',version:2});
  expect(await db.prepare('SELECT snapshot_json::text snapshot,snapshot_sha256,status,version FROM staff_project_handoffs WHERE id=?').bind(handoffId).first()).toEqual(before);
  expect(await db.prepare('SELECT count(*) n FROM staff_notifications WHERE id=?').bind(`project-handoff-decision-${receipt}`).first()).toEqual({n:0});
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(receipt).first()).toEqual({n:0});
 }finally{await db.prepare(`DROP TRIGGER ${trigger} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const result=await projectStructureCommand(f.head,p.id,input,receipt);expect(result).toMatchObject({status:'completed',version:3});expect(await projectStructureCommand(f.head,p.id,input,receipt)).toEqual(result);
 expect(await db.prepare('SELECT status,version FROM staff_project_handoffs WHERE id=?').bind(handoffId).first()).toEqual({status:'accepted',version:2});
 expect(await db.prepare('SELECT count(*) n FROM staff_notifications WHERE id=?').bind(`project-handoff-decision-${receipt}`).first()).toEqual({n:1});
 expect(await db.prepare('SELECT count(*) n FROM audit_events WHERE command_id=?').bind(receipt).first()).toEqual({n:1});
});

it.each(['immediate','deferred'] as const)('keeps the Projects-only member reference unchanged through %s native worker effects',async timing=>{
 await grantProject();vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 const receipt=key(),id=`project-${receipt}`,input=projectSchema.parse({title:'Actual included worker membership',memberUserIds:[f.claimant.userId]}),fn=`project_member_reference_${key().replaceAll('-','')}`,
  before=await db.prepare('SELECT to_jsonb(e) image FROM hr_engagements e WHERE company_id=? AND person_id=?').bind(f.company,`person-${f.claimant.userId}`).first('image');
 await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE hr_engagements SET status='suspended',version=version+1 WHERE company_id='${f.company}' AND person_id='person-${f.claimant.userId}';RETURN NEW;END $$;
 CREATE ${timing==='deferred'?'CONSTRAINT ':''}TRIGGER ${fn} AFTER INSERT ON command_receipts ${timing==='deferred'?'DEFERRABLE INITIALLY DEFERRED ':''}FOR EACH ROW WHEN(NEW.command_id='${receipt}') EXECUTE FUNCTION ${fn}()`);
 // Immediate suspension is already caught by the original active-worker
 // command guard. Preserve its existing error precedence and full rollback.
 try{await expect(saveProject(db,f.owner,null,input,receipt)).rejects.toMatchObject({code:timing==='immediate'?'CONFLICT':'PROJECT_WORKER_REFERENCE_CHANGED'});}
 finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}()`);}
 expect(await db.prepare('SELECT to_jsonb(e) image FROM hr_engagements e WHERE company_id=? AND person_id=?').bind(f.company,`person-${f.claimant.userId}`).first('image')).toEqual(before);
 expect(await db.prepare('SELECT 1 FROM staff_projects WHERE id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM staff_project_members WHERE project_id=?').bind(id).first()).toBeNull();
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 const saved=await saveProject(db,f.owner,null,input,receipt);expect(saved).toMatchObject({id,version:1});expect(await saveProject(db,f.owner,null,input,receipt)).toEqual(saved);
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);
});

it.each(['immediate-placement','deferred-placement','deferred-iam','deferred-members'] as const)('protects the actual Project worker and member-set image after %s effects',async kind=>{
 await grantProject();vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const receipt=key(),id=`project-${receipt}`,input=projectSchema.parse({title:'Original native worker and membership source',memberUserIds:[f.claimant.userId]}),fn=`project_reference_image_${key().replaceAll('-','')}`,
  before=await db.prepare('SELECT to_jsonb(e) image FROM hr_engagements e WHERE company_id=? AND person_id=?').bind(f.company,`person-${f.claimant.userId}`).first('image'),
  effect=kind.endsWith('placement')?`UPDATE hr_engagements SET department_id='${f.accountsDepartment}',version=version+1 WHERE company_id='${f.company}' AND person_id='person-${f.claimant.userId}'`:
   kind==='deferred-iam'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`:
    `INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES('${id}','${f.head.userId}','${f.owner.userId}',1)`,deferred=kind!=='immediate-placement';
 await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;
 CREATE ${deferred?'CONSTRAINT ':''}TRIGGER ${fn} AFTER INSERT ON command_receipts ${deferred?'DEFERRABLE INITIALLY DEFERRED ':''}FOR EACH ROW WHEN(NEW.command_id='${receipt}') EXECUTE FUNCTION ${fn}()`);
 try{await expect(saveProject(db,f.owner,null,input,receipt)).rejects.toMatchObject({code:'PROJECT_WORKER_REFERENCE_CHANGED'});}
 finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}()`);}
 expect(await db.prepare('SELECT to_jsonb(e) image FROM hr_engagements e WHERE company_id=? AND person_id=?').bind(f.company,`person-${f.claimant.userId}`).first('image')).toEqual(before);
 expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first('revoked_at')).toBeNull();
 expect(await db.prepare('SELECT 1 FROM staff_projects WHERE id=?').bind(id).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM staff_project_members WHERE project_id=?').bind(id).first()).toBeNull();
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(receipt).first()).toBeNull();
 expect(await saveProject(db,f.owner,null,input,receipt)).toMatchObject({id,version:1});
});

it('preserves Projects-only cross-branch worker membership and an older exact receipt after a legitimate member revision',async()=>{
 const branch=`project-branch-${key()}`,department=`project-department-${key()}`;
 await db.batch([db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Other original branch',?,?,?)").bind(branch,branch,f.company,f.company,branch),
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Other original department',?,?,?)").bind(department,department,branch,f.company,branch),
  db.prepare('UPDATE hr_engagements SET branch_id=?,department_id=?,version=version+1 WHERE company_id=? AND person_id=?').bind(branch,department,f.company,`person-${f.claimant.userId}`),
  db.prepare("UPDATE assignments SET scope_kind='branch',scope_node_id=?,version=version+1 WHERE id=?").bind(f.branch,f.head.assignmentId)]);
 const actor={...f.head,branchId:f.branch,assignmentScope:'branch' as const,scope:{kind:'branch' as const,holdingId:f.head.holdingId,holdingSlug:f.head.holdingSlug,holdingName:f.head.holdingName,companyId:f.company,branchId:f.branch,branchSlug:'original-branch',branchName:'Original branch'}};
 await grantProject();vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 const receipt=key(),input=projectSchema.parse({title:'Cross branch foundation members',memberUserIds:[f.claimant.userId]}),saved=await saveProject(db,actor,null,input,receipt);
 expect(await db.prepare('SELECT branch_id,department_id FROM hr_engagements WHERE company_id=? AND person_id=?').bind(f.company,`person-${f.claimant.userId}`).first()).toEqual({branch_id:branch,department_id:department});
 expect(await db.prepare('SELECT user_id FROM staff_project_members WHERE project_id=?').bind(saved.id).first('user_id')).toBe(f.claimant.userId);
 expect(await saveProject(db,actor,String(saved.id),projectSchema.parse({...input,expectedVersion:1,memberUserIds:[f.reviewer.userId]}),key())).toMatchObject({version:2});
 await grantProject('draining');expect(await saveProject(db,actor,null,input,receipt)).toEqual(saved);
 expect(await db.prepare('SELECT user_id FROM staff_project_members WHERE project_id=?').bind(saved.id).first('user_id')).toBe(f.reviewer.userId);expect((await getProject(db,actor,String(saved.id))).version).toBe(2);
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>[row.packageId,row.state])).toEqual([['projects','draining']]);
});

it('sees the committed worker before Project preparation and isolates two Companies without hidden worker-source retries',async()=>{
 const {committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics');
 await grantProject();vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const receipt=key(),input=projectSchema.parse({title:'Actual committed worker source',memberUserIds:[f.claimant.userId]}),observed=await committedSourceVisibility(db,async tx=>{await tx.prepare("UPDATE hr_engagements SET status='suspended',version=version+1 WHERE company_id=? AND person_id=?").bind(f.company,`person-${f.claimant.userId}`).run();},()=>measureReads(async()=>{
  try{return {value:await saveProject(db,f.owner,null,input,receipt),error:null};}catch(error){return {value:null,error};}
 }));
 expect(observed.result.error).toMatchObject({code:'STAFF_LINK_REQUIRED'});expect(observed.result.value).toBeNull();for(const field of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(observed.metrics[field]??0).toBe(0);
 expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(receipt).first()).toBeNull();
 await db.prepare("UPDATE hr_engagements SET status='active',version=version+1 WHERE company_id=? AND person_id=?").bind(f.company,`person-${f.claimant.userId}`).run();
 const other=await expenseFixture();await grantProject('active',null,'projects',other.company);const commands=[key(),key()],fn=`project_workers_parallel_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE hr_engagements SET job_title=job_title WHERE person_id=CASE WHEN NEW.command_id='${commands[0]}' THEN 'person-${f.claimant.userId}' ELSE 'person-${other.claimant.userId}' END;RETURN NEW;END $$;
 CREATE TRIGGER ${fn} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id IN ('${commands[0]}','${commands[1]}')) EXECUTE FUNCTION ${fn}()`);
 try{const parallel=await measureReads(()=>Promise.all([saveProject(db,f.owner,null,input,commands[0]),saveProject(db,other.owner,null,projectSchema.parse({...input,memberUserIds:[other.claimant.userId]}),commands[1])]));expect(parallel.result).toHaveLength(2);for(const field of ['deadlockFailures','serializationFailures','transactionRetries'] as const)expect(parallel.metrics[field]??0).toBe(0);
  for(const [index,fixture] of [f,other].entries())expect(await db.prepare('SELECT company_id,owner_user_id FROM staff_projects WHERE id=?').bind(parallel.result[index].id).first()).toEqual({company_id:fixture.company,owner_user_id:fixture.owner.userId});
 }finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}()`);}
});

// Bounded worker-reference extension: actual existing Company accounts only.
// The suite's old expense fixture remains in its original beforeEach, but none
// of its seeded workers/financial facts belongs to, or is used by, this Company.
type ProjectWorkerActor=import("@erpx/server-identity_access/public/authorization").WorkContext;
async function zeroWorkerProjectCompany(purchased=true){
 const company=`projects-workers-${key()}`,branch=`${company}-branch`,department=`${company}-team`;
 await db.batch([
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero-worker Projects Company',?)").bind(company,company,company),
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Real Projects Branch',?,?,?)").bind(branch,branch,company,company,branch),
  db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Real Projects team',?,?,?)").bind(department,department,branch,company,branch),
 ]);
 const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store'),{selectWorkRole}=await import('@erpx/server-work_projection/public/work-role-store');
 async function account(name:string,role='employee'):Promise<ProjectWorkerActor>{
  const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`,email=`${userId}@example.test`,modules=['projects',...(role==='holding_owner'?['people_payroll']:[])];
  // Explicit original identity/account fixture, before any worker exists. The
  // feature under test creates neither account, assignment, access nor role.
  await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES(?,?,?,1,1,1)').bind(userId,name,email).run();
  await db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,?,?,0,9999999999999,'holding')").bind(assignmentId,userId,role,JSON.stringify(modules),company).run();
  const sessionId=key(),expiresAt=Date.now()+86400000;await selectWorkRole(db,{sessionId,userId,companyId:company,assignmentId,assignmentVersion:1,revision:0,expiresAt});
  const actor:ProjectWorkerActor={userId,assignmentId,role,companyId:company,branchId:null,departmentId:null,holdingId:'h001',holdingSlug:'test',holdingName:'Test',name,email,assignmentScope:'holding',inherited:false,modules,assignments:[],workspaces:[],scope:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company},authority:{sessionId,revision:1,assignmentVersion:1}};
  actor.iam=await loadIamAuthority(db,company,userId,role,sessionId);return actor;
 }
 const owner=await account('company-hr-owner','holding_owner'),contributor=await account('project-contributor');
 const {installStaffLevels,savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');await installStaffLevels(db,owner,key());
 const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Actual first Company leader',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Normal Organization prepares the first worker position'}),key())).id);
 const worker=String((await savePosition(db,owner,null,positionSchema.parse({title:'Actual Project contributor',levelId:`org-level-${company}-6`,branchId:branch,departmentId:department,isCeo:false,reportsToPositionId:ceo,expectedVersion:0,reason:'Normal Organization prepares the explicit Project worker position'}),key())).id);
 const store=packageGrantStore(db,{authorize:async(_tx,id)=>{if(id!==company)throw Error('PROJECT_WORKER_FIXTURE_COMPANY');return {userId:'isolated-project-worker-provider'};},obligations:async()=>{throw Error('PROJECT_WORKER_FIXTURE_CANNOT_INACTIVATE');}});
 async function purchase(state:'active'|'draining'='active'){const prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId==='projects');return store.apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId:'projects',packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Actual Projects-only worker-reference foundation',evidence:'test:projects-zero-worker-native'}});}
 if(purchased)await purchase();
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 const api=await import('@erpx/server-people_payroll/public/project-worker-startup'),jobs=await import('@erpx/server-people_payroll/public/people-lifecycle-jobs');
 const counts=()=>Promise.all(['hr_people','hr_engagements','employees','people_lifecycle_cases','people_project_worker_starters','people_project_worker_placements','staff_projects','finance_charts','finance_accounts','finance_journals','finance_payments','hr_pay_groups','hr_payroll_runs'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
 expect(await counts()).toEqual(Array(13).fill(0));
 const accountImage=async()=>({assignments:(await db.prepare('SELECT to_jsonb(a)::text source FROM assignments a WHERE a.company_id=? ORDER BY a.id').bind(company).all()).results,members:(await db.prepare('SELECT to_jsonb(m)::text source FROM iam_members m WHERE m.company_id=? ORDER BY m.user_id').bind(company).all()).results,accounts:(await env.CONTROL_DB.prepare('SELECT to_jsonb(u)::text source FROM "user" u WHERE id LIKE ? ORDER BY id').bind(`${company}-%`).all()).results});
 const nonWorkerImage=async()=>({accounts:await accountImage(),compensation:(await db.prepare('SELECT to_jsonb(c)::text source FROM hr_compensation_revisions c JOIN hr_engagements e ON e.id=c.engagement_id WHERE e.company_id=? ORDER BY c.id').bind(company).all()).results,leave:(await db.prepare('SELECT to_jsonb(l)::text source FROM leave_policy_assignments l WHERE l.company_id=? ORDER BY l.id').bind(company).all()).results,money:await Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','finance_journals','finance_payments','hr_pay_groups','hr_payroll_runs'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')))});
 async function prepare(who:ProjectWorkerActor,positionId:string,managerPersonId:string|null,profileName=who.name){const view=await api.projectWorkerStartupWorkspace(owner,{accountId:who.userId});expect(view.canPrepare).toBe(true);if(!view.contextKey)throw Error('Native worker context required');const command=key(),input=api.projectWorkerPrepareSchema.parse({mode:'create',accountId:who.userId,expectedVersion:0,contextKey:view.contextKey,input:{legalName:profileName,email:who.email,countryCode:'TH',branchId:branch,departmentId:department,jobTitle:positionId===ceo?'Company leader':'Project contributor',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId,levelId:`org-level-${company}-${positionId===ceo?1:6}`,positionId,acceptRecommendedPay:false,acceptRecommendedLeave:false,reason:'Explicitly prepare this selected existing Company account'}});const result=await api.prepareProjectWorkerStartup(owner,input,command);expect(result).toEqual({id:`person-${command}`,version:1});return {result,input,command};}
 async function submit(personId:string,positionId:string,managerPersonId:string|null,effectiveAt=Date.now()-1000){const view=await api.projectWorkerStartupWorkspace(owner,{personId});expect(view.canSubmitPlacement).toBe(true);if(!view.contextKey)throw Error('Native placement context required');const input=api.projectWorkerPlacementSchema.parse({positionId,managerPersonId,effectiveAt,timezone:'Asia/Bangkok',expectedVersion:0,contextKey:view.contextKey,reason:'Reviewed original HR-only Project worker placement'}),preview=await api.previewProjectWorkerPlacement(owner,personId,input);expect(preview.ready).toBe(true);expect(preview.blockers).toEqual([]);const body={...input,token:preview.token},command=key(),result=await api.submitProjectWorkerPlacement(owner,personId,body,command);expect(result).toEqual({id:`lifecycle-${command}`,version:1,status:'scheduled'});return {result,body,command};}
 async function settle(){return withOwnerPackageObligations(db,{companyId:company,packageId:'projects',packageVersion:1},tx=>readOwnerPackageObligations(db,tx,{companyId:company,packageId:'projects',packageVersion:1,now:Date.now()}));}
 async function pendingPeople(){return (await settle()).observations.find(row=>row.moduleId==='people_payroll')?.outstanding??0;}
 return {company,branch,department,owner,contributor,ceo,worker,account,api,jobs,counts,accountImage,nonWorkerImage,prepare,submit,purchase,pendingPeople};
}

it('prepares a true zero-worker Projects-only Company through original Directory and reviewed HR placement before explicit Project membership and approved time',async()=>{
 const s=await zeroWorkerProjectCompany(),unchanged=await s.nonWorkerImage(),grants=await readCompanyPackageGrants(db,s.company),{activeStaff}=await import('@erpx/server-operations/public/staff-policy');
 expect(grants.map(row=>row.packageId)).toEqual(['projects']);await expect(activeStaff(db,s.company,s.owner.userId)).rejects.toMatchObject({code:'STAFF_LINK_REQUIRED'});
 const view=await s.api.projectWorkerStartupWorkspace(s.owner,{});expect(view).toMatchObject({available:true,schemaAvailable:true,readiness:'selection-required',contextKey:null});
 const options=await s.api.projectWorkerStartupAccounts(s.owner,{query:'project-contributor'});expect(options.accounts).toEqual([{id:s.contributor.userId,name:s.contributor.name,email:s.contributor.email}]);
 const {saveDirectoryPerson,directoryUpdateSchema}=await import('@erpx/server-people_payroll/public/people-directory');
 await expect(saveDirectoryPerson(s.owner,null,directoryUpdateSchema.parse({legalName:'Generic People must remain closed',email:s.owner.email,countryCode:'TH',branchId:s.branch,departmentId:s.department,jobTitle:'Company leader',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId:null,levelId:`org-level-${s.company}-1`,positionId:s.ceo,expectedVersion:1}),key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await s.counts()).toEqual(Array(13).fill(0));
 const owner=await s.prepare(s.owner,s.ceo,null),ownerPlacement=await s.submit(owner.result.id,s.ceo,null);expect(await s.pendingPeople()).toBe(1);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);expect(await s.pendingPeople()).toBe(0);expect((await activeStaff(db,s.company,s.owner.userId)).person_id).toBe(owner.result.id);
 const contributor=await s.prepare(s.contributor,s.worker,owner.result.id),placement=await s.submit(contributor.result.id,s.worker,owner.result.id);expect(await s.pendingPeople()).toBe(1);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);expect(await s.pendingPeople()).toBe(0);
 const actual=await activeStaff(db,s.company,s.contributor.userId);expect(actual).toMatchObject({person_id:contributor.result.id,user_id:s.contributor.userId,company_id:s.company});
 const ready=await s.api.projectWorkerStartupWorkspace(s.owner,{accountId:s.contributor.userId});expect(ready).toMatchObject({readiness:'ready',canPrepare:false,canLink:false,canSubmitPlacement:false});
 expect(await s.api.prepareProjectWorkerStartup(s.owner,owner.input,owner.command)).toEqual(owner.result);expect(await s.api.submitProjectWorkerPlacement(s.owner,owner.result.id,ownerPlacement.body,ownerPlacement.command)).toEqual(ownerPlacement.result);expect(await s.api.prepareProjectWorkerStartup(s.owner,contributor.input,contributor.command)).toEqual(contributor.result);expect(await s.api.submitProjectWorkerPlacement(s.owner,contributor.result.id,placement.body,placement.command)).toEqual(placement.result);
 const noFakeBefore=await s.counts();await expect(s.api.prepareProjectWorkerStartup(s.owner,{...contributor.input,contextKey:ready.contextKey},key())).rejects.toMatchObject({code:'PROJECT_WORKER_ALREADY_READY'});expect(await s.counts()).toEqual(noFakeBefore);
 const p=await saveProject(db,s.owner,null,projectSchema.parse({title:'Normal zero-worker foundation Project',memberUserIds:[s.contributor.userId]}),key());expect((await getProject(db,s.contributor,p.id)).id).toBe(p.id);
 const timeInput=projectWorkLogSchema.parse({workDate:new Date().toISOString().slice(0,10),minutes:75}),timeKey=key(),time=await saveProjectWorkLog(db,s.contributor,p.id,timeInput,timeKey);expect(time).toMatchObject({state:'submitted',version:1});expect(await saveProjectWorkLog(db,s.contributor,p.id,timeInput,timeKey)).toEqual(time);
 const approved=await decideProjectWorkLog(db,s.owner,p.id,time.id,projectWorkLogDecisionSchema.parse({action:'approve',expectedVersion:1,reason:'The distinct actual Project owner reviewed these work minutes'}),key());expect(approved).toMatchObject({state:'approved',version:2});
 expect(await s.nonWorkerImage()).toEqual(unchanged);expect(await readCompanyPackageGrants(db,s.company)).toEqual(grants);
 expect((await db.prepare('SELECT role,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(s.company).all()).results).toHaveLength(2);
 const counts=await s.counts();expect(counts.slice(0,6)).toEqual([2,2,2,2,2,2]);expect(counts.slice(7)).toEqual(Array(6).fill(0));
});

it('links only an explicit original unlinked worker and keeps source/queued rollback plus reviewed due and cancellation duties correct while Projects drains',async()=>{
 const absent=await zeroWorkerProjectCompany(false),absentInput=absent.api.projectWorkerPrepareSchema.parse({mode:'create',accountId:absent.owner.userId,expectedVersion:0,contextKey:'0'.repeat(64),input:{legalName:absent.owner.name,email:absent.owner.email,countryCode:'TH',branchId:absent.branch,departmentId:absent.department,jobTitle:'Company leader',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId:null,levelId:`org-level-${absent.company}-1`,positionId:absent.ceo,acceptRecommendedPay:false,acceptRecommendedLeave:false,reason:'Valid normal HR input must not create purchase authority'}});
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');const absentBefore=await absent.counts();await expect(absent.api.prepareProjectWorkerStartup(absent.owner,absentInput,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await absent.counts()).toEqual(absentBefore);
 const s=await zeroWorkerProjectCompany(),{saveDirectoryPerson,directoryUpdateSchema}=await import('@erpx/server-people_payroll/public/people-directory'),{activeStaff}=await import('@erpx/server-operations/public/staff-policy');
 const noHrBefore=await s.counts();await expect(s.api.projectWorkerStartupAccounts(s.contributor,{})).rejects.toMatchObject({status:403});expect(await s.counts()).toEqual(noHrBefore);
 const owner=await s.prepare(s.owner,s.ceo,null);await s.submit(owner.result.id,s.ceo,null);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);
 // Existing unlinked legacy Person is produced by the original Directory under
 // its original disabled-mode contract, then retained in Projects-only mode.
 // No positive HR Person/engagement INSERT or private constructor receipt SQL.
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');
 const legacyKey=key(),legacy=await saveDirectoryPerson(s.owner,null,directoryUpdateSchema.parse({legalName:'Explicit existing Project contributor',email:s.contributor.email,countryCode:'TH',branchId:s.branch,departmentId:s.department,jobTitle:'Project contributor',workerType:'employee',employmentType:'salaried',startDate:new Date().toISOString().slice(0,10),currency:'THB',managerPersonId:owner.result.id,levelId:`org-level-${s.company}-6`,positionId:s.worker,expectedVersion:1}),legacyKey);
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const people=await s.api.projectWorkerStartupPeople(s.owner,{query:'Explicit existing'});expect(people.people).toEqual([{id:legacy.id,legalName:'Explicit existing Project contributor',version:1,engagementId:`eng-${legacyKey}`,engagementVersion:1}]);
 const accountOnly=await s.api.projectWorkerStartupWorkspace(s.owner,{accountId:s.contributor.userId});expect(accountOnly.canPrepare).toBe(false);expect(accountOnly.person).toBeNull();
 const selected=await s.api.projectWorkerStartupWorkspace(s.owner,{accountId:s.contributor.userId,personId:legacy.id});expect(selected.canLink).toBe(true);if(!selected.contextKey)throw Error('Explicit source link context required');
 const link={mode:'link' as const,accountId:s.contributor.userId,personId:legacy.id,expectedVersion:1,expectedEngagementVersion:1,contextKey:selected.contextKey,reason:'Company HR explicitly links the selected original Person to the selected existing Company account'},linkKey=key(),beforeLink=await s.accountImage(),linked=await s.api.prepareProjectWorkerStartup(s.owner,link,linkKey);expect(linked).toEqual({id:legacy.id,version:2});expect(await s.accountImage()).toEqual(beforeLink);
 const previewView=await s.api.projectWorkerStartupWorkspace(s.owner,{personId:legacy.id});if(!previewView.contextKey)throw Error('Reviewed source context required');
 const previewInput=s.api.projectWorkerPlacementSchema.parse({positionId:s.worker,managerPersonId:owner.result.id,effectiveAt:Date.now()-1000,timezone:'Asia/Bangkok',expectedVersion:0,contextKey:previewView.contextKey,reason:'Actual original HR placement for explicit linked employee'}),preview=await s.api.previewProjectWorkerPlacement(s.owner,legacy.id,previewInput),body={...previewInput,token:preview.token};expect(preview.ready).toBe(true);
 const submitKey=key(),fault=`project_worker_queued_${key().replaceAll('-','')}`,beforeAssignment=await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(s.owner.assignmentId).first();
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${s.owner.assignmentId}';RETURN NEW;END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.command_id='${submitKey}') EXECUTE FUNCTION ${fault}()`).run();
 try{await expect(s.api.submitProjectWorkerPlacement(s.owner,legacy.id,body,submitKey)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${fault} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(s.owner.assignmentId).first()).toEqual(beforeAssignment);expect(await db.prepare('SELECT 1 FROM people_lifecycle_cases WHERE id=?').bind(`lifecycle-${submitKey}`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(submitKey).first()).toBeNull();
 const submitted=await s.api.submitProjectWorkerPlacement(s.owner,legacy.id,body,submitKey);expect(submitted).toMatchObject({id:`lifecycle-${submitKey}`,status:'scheduled',version:1});
 const sourceFault=`project_worker_source_${key().replaceAll('-','')}`,beforePosition=await db.prepare('SELECT title,version FROM organization_positions WHERE id=?').bind(s.worker).first(),beforeEngagement=await db.prepare('SELECT to_jsonb(e)::text image FROM hr_engagements e WHERE id=?').bind(`eng-${legacyKey}`).first();
 await db.prepare(`CREATE FUNCTION ${sourceFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE organization_positions SET title='Late changed reviewed Project source' WHERE id='${s.worker}';RETURN NEW;END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${sourceFault} AFTER INSERT ON iam_audit DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.id='applied-${submitted.id}') EXECUTE FUNCTION ${sourceFault}()`).run();
 try{expect(await s.jobs.drainPeopleLifecycle(db)).toBe(0);}finally{await db.prepare(`DROP TRIGGER ${sourceFault} ON iam_audit`).run();await db.prepare(`DROP FUNCTION ${sourceFault}()`).run();}
 expect(await db.prepare('SELECT title,version FROM organization_positions WHERE id=?').bind(s.worker).first()).toEqual(beforePosition);expect(await db.prepare('SELECT to_jsonb(e)::text image FROM hr_engagements e WHERE id=?').bind(`eng-${legacyKey}`).first()).toEqual(beforeEngagement);expect(await db.prepare('SELECT status,version,attempts,last_error FROM people_lifecycle_cases WHERE id=?').bind(submitted.id).first()).toEqual({status:'scheduled',version:1,attempts:1,last_error:'PROJECT_WORKER_SOURCE_CHANGED'});expect(await db.prepare('SELECT 1 FROM iam_audit WHERE id=?').bind(`applied-${submitted.id}`).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM outbox_events WHERE id=?').bind(`event-applied-${submitted.id}`).first()).toBeNull();expect(await s.pendingPeople()).toBe(1);
 await new Promise(resolve=>setTimeout(resolve,1100));expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);expect((await activeStaff(db,s.company,s.contributor.userId)).person_id).toBe(legacy.id);expect(await s.pendingPeople()).toBe(0);
 const dueAccount=await s.account('reviewed-due'),futureAccount=await s.account('future-closure'),due=await s.prepare(dueAccount,s.worker,owner.result.id),future=await s.prepare(futureAccount,s.worker,owner.result.id),dueCase=await s.submit(due.result.id,s.worker,owner.result.id),futureCase=await s.submit(future.result.id,s.worker,owner.result.id,Date.now()+86400000);expect(await s.pendingPeople()).toBe(2);
 await s.purchase('draining');expect(await s.api.prepareProjectWorkerStartup(s.owner,due.input,due.command)).toEqual(due.result);expect(await s.api.submitProjectWorkerPlacement(s.owner,due.result.id,dueCase.body,dueCase.command)).toEqual(dueCase.result);expect(await s.jobs.drainPeopleLifecycle(db)).toBe(1);expect(await s.pendingPeople()).toBe(1);
 const beforeDenied=await s.counts();await expect(s.api.prepareProjectWorkerStartup(s.owner,{...due.input,accountId:futureAccount.userId},key())).rejects.toMatchObject({status:403});expect(await s.counts()).toEqual(beforeDenied);
 // Adverse subject revocation cannot authorize its due job, but retained HR
 // history can still cancel this future source obligation normally.
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),futureAccount.assignmentId).run();const retained=await s.api.projectWorkerStartupWorkspace(s.owner,{personId:future.result.id});expect(retained.canCancelPlacement).toBe(true);
 const cancelKey=key(),cancel=await s.api.cancelProjectWorkerPlacement(s.owner,future.result.id,futureCase.result.id,{expectedVersion:1},cancelKey);expect(cancel).toEqual({id:futureCase.result.id,status:'cancelled'});expect(Object.hasOwn(cancel,'version')).toBe(false);expect(await s.api.cancelProjectWorkerPlacement(s.owner,future.result.id,futureCase.result.id,{expectedVersion:1},cancelKey)).toEqual(cancel);expect(await s.pendingPeople()).toBe(0);
 expect((await readCompanyPackageGrants(db,s.company)).map(row=>[row.packageId,row.state])).toEqual([['projects','draining']]);
 const beforeRevoked=await s.counts();await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),s.owner.assignmentId).run();await expect(s.api.prepareProjectWorkerStartup(s.owner,link,linkKey)).rejects.toMatchObject({code:'QUEUED_AUTHORITY_CHANGED'});expect(await s.counts()).toEqual(beforeRevoked);
});


it('proves the actual Projects worker entry against current native Company HR assignment, purchase and Person source',async()=>{
 const s=await zeroWorkerProjectCompany(),entry=s.api.projectWorkerStartupProductEntry;
 if(!s.owner.authority)throw Error('Original selected Company work authority required');
 const scope={companyId:s.company,branchId:null,departmentId:null,userId:s.owner.userId,assignmentId:s.owner.assignmentId,assignmentVersion:s.owner.authority.assignmentVersion,role:s.owner.role,sessionId:s.owner.authority.sessionId};
 expect(await entry(db,scope)).toMatchObject({revision:expect.stringMatching(/^[a-f0-9]{64}$/),purchasePurpose:'history'});
 for(const change of [{assignmentVersion:scope.assignmentVersion+1},{sessionId:''},{branchId:s.branch},{departmentId:s.department},{beta:true}])expect(await entry(db,{...scope,...change})).toBeNull();
 expect(await entry(db,scope,{personId:'person-foreign-unavailable'})).toBeNull();
 if(!s.contributor.authority)throw Error('Original contributor authority required');
 expect(await entry(db,{...scope,userId:s.contributor.userId,assignmentId:s.contributor.assignmentId,role:s.contributor.role,sessionId:s.contributor.authority.sessionId,assignmentVersion:s.contributor.authority.assignmentVersion})).toBeNull();
 const created=await s.prepare(s.owner,s.ceo,null);
 expect(await entry(db,scope,{personId:created.result.id})).toMatchObject({purchasePurpose:'history'});
 expect((await readCompanyPackageGrants(db,s.company)).map(row=>row.packageId)).toEqual(['projects']);
});
