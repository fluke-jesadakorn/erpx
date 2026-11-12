import {afterEach,beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {env,applyMigrations} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {getProject,projectSchema,saveTask,taskSchema} from '@erpx/server-operations/public/staff-tasks';
import {projectGraph,projectHandoffs,projectHandoffWork} from "@erpx/server-projects/public/project-structure";
import {listProjectCostAllocations} from "@erpx/server-projects/public/project-actuals";
import {approveCostAllocation,seedCostAllocation,seedMilestone,seedProjectFor,seedWorkLog} from "./project-actual-fixtures";
import {seedFinanceJournal} from '../../support/helpers/finance-journal-seed';
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
import {packageGrantRevision} from "@erpx/server-commercial/public/package-grants";
import {withOwnerPackageObligations,readOwnerPackageObligations,ownerPackageObligations} from "@erpx/server-commercial/public/owner-package-obligations";
import {projectPackageObligations} from "@erpx/server-projects/public/package-obligations";
import {listProjects} from "@erpx/server-projects/public/staff-projects";
import {getTask} from '@erpx/server-operations/public/staff-task-read';
import {listTasks,taskDetail} from '@erpx/server-operations/public/staff-tasks';
import {getStaffWork} from "@erpx/server-work_projection/public/domain-work";
import {enrichFlowWork} from "@erpx/server-workflow/public/flow-work";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);});
afterEach(()=>vi.unstubAllEnvs());
beforeEach(async()=>{f=await expenseFixture();for(const a of [f.owner,f.claimant,f.head,f.manager,f.reviewer])a.modules=['projects','finance'];});
async function newProject(){return seedProjectFor(db,f.owner,null,projectSchema.parse({title:'Reviewed customer implementation',memberUserIds:[f.claimant.userId,f.head.userId,f.manager.userId,f.reviewer.userId]}),key());}
async function approvedExpense(){
 const journal=await seedFinanceJournal(db,{companyId:f.company,chartId:f.chart,periodId:f.period,entryDate:'2026-09-12',currency:'THB',memo:'Approved project cost source',key:key(),createdBy:f.owner.userId,status:'posted',lines:[{accountId:f.accounts.expense,debit:'75.01',credit:'0'},{accountId:f.accounts.payable,debit:'0',credit:'75.01'}]});
 return `${journal.id}-0`;
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
it('withdraws only the settled Project Costs addon without closing an independently active basic project or its retained posted expense',async()=>{
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');
 await grantProject();await grantProject('active',null,'projects-costs');const p=await newProject(),line=await approvedExpense(),allocation=await seedCostAllocation(db,{id:`project-cost-${key()}`,companyId:f.company,projectId:p.id,journalLineId:line,amountMinor:2501,createdBy:f.owner.userId});
 await grantProject('draining',null,'projects-costs');expect((await projectSettlement('projects-costs')).total).toBeGreaterThan(0);await expect(projectWithdrawalReview(await projectWithdrawal('projects-costs'))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await approveCostAllocation(db,{id:allocation.id,companyId:f.company,projectId:p.id,version:1,approverId:f.manager.userId,receipt:key()});expect((await projectSettlement('projects-costs')).total).toBe(0);
 const request=await projectWithdrawal('projects-costs');await projectWithdrawalReview(request);await projectWithdrawalReview(request,true);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');expect((await getProject(db,f.owner,p.id)).lifecycle_status).toBe('active');expect((await projectSettlement()).total).toBeGreaterThan(0);expect((await listProjectCostAllocations(f.owner,p.id)).map(row=>row.id)).toContain(allocation.id);
});
it('requires minted Project source leases and refuses legacy completed headers without an independently accepted source receipt',async()=>{
 const input={...projectSettlementInput(),moduleId:'projects',now:Date.now()};await expect(db.transaction(tx=>projectPackageObligations(db,tx,input))).rejects.toThrow('PROJECT_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...projectSettlementInput(),packageVersion:2},tx=>projectPackageObligations(db,tx,{...input,packageVersion:2}))).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');await expect(withOwnerPackageObligations(db,projectSettlementInput(),tx=>projectPackageObligations(db,tx,{...input,moduleId:'finance'}))).rejects.toThrow('PROJECT_PACKAGE_OBLIGATION_SCOPE_REQUIRED');await expect(withOwnerPackageObligations(db,projectSettlementInput(),tx=>projectPackageObligations(db,tx,{...input,companyId:'foreign-company'}))).rejects.toThrow('PROJECT_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect((await projectSettlement()).total).toBe(0);
 await grantProject();const p=await newProject();await db.prepare("UPDATE staff_projects SET lifecycle_status='completed',version=version+1 WHERE id=?").bind(p.id).run();await grantProject('draining');expect((await projectSettlement()).total).toBeGreaterThan(0);await expect(projectWithdrawalReview(await projectWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
});
it('keeps unposted and foreign original Project cost lineage as raw Finance withdrawal obligations',async()=>{
 await grantProject();await grantProject('active',null,'projects-costs');const p=await newProject(),now=Date.now(),foreign=key(),first=key(),second=key();
 const journal=await seedFinanceJournal(db,{companyId:f.company,chartId:f.chart,periodId:f.period,entryDate:'2026-09-12',currency:'THB',memo:'Original unresolved legacy Project cost',key:key(),createdBy:f.owner.userId,lines:[{accountId:f.accounts.expense,debit:'20.00',credit:'0'},{accountId:f.accounts.payable,debit:'0',credit:'20.00'}]});
 await db.prepare("INSERT INTO staff_projects(id,company_id,title,owner_user_id,created_at,updated_at) VALUES(?,'foreign-company','Foreign retained source',?,?,?)").bind(foreign,f.owner.userId,now,now).run();
 for(const [id,projectId] of [[first,p.id],[second,foreign]])await db.prepare("INSERT INTO staff_project_cost_allocations(id,company_id,project_id,journal_line_id,amount_minor,state,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,500,'approved',?,?,?,?)").bind(id,f.company,projectId,`${journal.id}-0`,f.owner.userId,f.manager.userId,now,now).run();
 await grantProject('draining',null,'projects-costs');const observed=await projectSettlement('projects-costs');expect(observed.observations.find(row=>row.moduleId==='finance')?.outstanding).toBe(2);expect(observed.observations.find(row=>row.moduleId==='projects')?.outstanding).toBe(2);await expect(projectWithdrawalReview(await projectWithdrawal('projects-costs'))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 expect(await db.prepare('SELECT status FROM finance_journals WHERE id=?').bind(journal.id).first('status')).toBe('draft');expect(await db.prepare('SELECT COUNT(*)::int count FROM staff_project_cost_allocations WHERE id IN (?,?)').bind(first,second).first('count')).toBe(2);
});
it('observes a committed original Project writer only after its fresh settlement source leases become available',async()=>{
 await grantProject();await grantProject('draining');const request=await projectWithdrawal(),id=`project-concurrent-${key()}`,now=Date.now();let inserted!:()=>void,release!:()=>void;const ready=new Promise<void>(resolve=>{inserted=resolve;}),released=new Promise<void>(resolve=>{release=resolve;});
 const writer=db.transaction(async tx=>{await tx.prepare('LOCK TABLE company_package_grants IN ROW EXCLUSIVE MODE').run();await tx.prepare("INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,owner_user_id,created_at,updated_at) VALUES(?,?,?,?,'Actual concurrent source',?,?,?)").bind(id,f.company,f.branch,f.department,f.owner.userId,now,now).run();inserted();await released;});await ready;const review=projectWithdrawalReview(request).then(value=>({value,error:null}),error=>({value:null,error}));
 try{let waiting=false;for(let attempt=0;attempt<100;attempt++){waiting=Boolean(await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='company_package_grants'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first());if(waiting)break;await new Promise(resolve=>setTimeout(resolve,10));}expect(waiting).toBe(true);}finally{release();await writer;}
 expect((await review).error).toMatchObject({message:'PACKAGE_OBLIGATIONS_REMAIN'});expect((await projectSettlement()).total).toBeGreaterThan(0);expect(await db.prepare('SELECT lifecycle_status FROM staff_projects WHERE id=?').bind(id).first('lifecycle_status')).toBe('active');
});
it('filters actual Project task sources before the Work limit while preserving unrelated tasks and retained task reads',async()=>{
 const p=await newProject(),projectTask=await saveTask(db,f.owner,null,taskSchema.parse({title:'Actual private project source',assigneeUserId:f.claimant.userId,projectId:p.id}),key()),ordinary=await saveTask(db,f.owner,null,taskSchema.parse({title:'Ordinary Work owner task',assigneeUserId:f.claimant.userId}),key());
 await db.prepare(`INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,project_id,state,created_at,updated_at)
 SELECT ?||'-'||n,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,project_id,state,created_at,updated_at+n+10000 FROM staff_tasks CROSS JOIN generate_series(1,255) n WHERE id=?`).bind(key(),projectTask.id).run();
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');
 expect((await listTasks(db,f.claimant)).map(row=>row.id)).toEqual([ordinary.id]);
 await expect(getTask(db,f.claimant,projectTask.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const free=await saveTask(db,f.owner,null,taskSchema.parse({title:'Unrelated Work remains its own owner',assigneeUserId:f.claimant.userId}),key());expect((await getTask(db,f.claimant,free.id)).project_id).toBeNull();
 await grantProject();expect((await listTasks(db,f.claimant)).filter(row=>row.project_id===p.id)).toHaveLength(250);
 await db.prepare("UPDATE staff_tasks SET state='in_progress',version=version+1,updated_at=? WHERE id=?").bind(Date.now(),projectTask.id).run();
 await grantProject('draining');expect((await taskDetail(db,f.claimant,projectTask.id)).permittedActions).toContain('complete');
});
it('prevents a retained Project task from regaining submit authority during Flow enrichment while preserving ordinary Work',async()=>{
 const p=await newProject(),draft=await saveTask(db,f.owner,null,taskSchema.parse({title:'Reviewed Project task draft',assigneeUserId:f.claimant.userId,projectId:p.id,dueAt:new Date(Date.now()+86400000).toISOString(),startApprovers:[f.head.userId]}),key()),free=await saveTask(db,f.owner,null,taskSchema.parse({title:'Unrelated Work stays visible',assigneeUserId:f.owner.userId}),key());
 const items=await getStaffWork(f.owner),raw=items.find(row=>row.id===draft.id)!;expect(raw.canAct).toBe(true);
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');expect((await getStaffWork(f.owner)).map(row=>row.id)).toContain(free.id);expect((await getStaffWork(f.owner)).map(row=>row.id)).not.toContain(draft.id);expect(await enrichFlowWork(f.owner,[raw])).toEqual([]);
 await grantProject();await grantProject('draining');
 const retained=(await getStaffWork(f.owner)).find(row=>row.id===draft.id)!;expect(retained.canAct).toBe(false);expect((await enrichFlowWork(f.owner,[{...retained,canAct:true}]))[0].canAct).toBe(false);
});
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
 const {savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');const {installStaffLevels}=await import('../../support/helpers/organization-levels');await installStaffLevels(db,owner,key());
 const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Actual first Company leader',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Normal Organization prepares the first worker position'}),key())).id);
 const worker=String((await savePosition(db,owner,null,positionSchema.parse({title:'Actual Project contributor',levelId:`org-level-${company}-6`,branchId:branch,departmentId:department,isCeo:false,reportsToPositionId:ceo,expectedVersion:0,reason:'Normal Organization prepares the explicit Project worker position'}),key())).id);
 const store=packageGrantStore(db,{authorize:async(_tx,id)=>{if(id!==company)throw Error('PROJECT_WORKER_FIXTURE_COMPANY');return {userId:'isolated-project-worker-provider'};},obligations:async()=>{throw Error('PROJECT_WORKER_FIXTURE_CANNOT_INACTIVATE');}});
 async function purchase(state:'active'|'draining'='active'){const prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId==='projects');return store.apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId:'projects',packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Actual Projects-only worker-reference foundation',evidence:'test:projects-zero-worker-native'}});}
 if(purchased)await purchase();
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
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
 const p=await seedProjectFor(db,s.owner,null,projectSchema.parse({title:'Normal zero-worker foundation Project',memberUserIds:[s.contributor.userId]}),key());expect((await getProject(db,s.contributor,p.id)).id).toBe(p.id);
 const time=await seedWorkLog(db,{id:`project-time-${key()}`,companyId:s.company,projectId:p.id,userId:s.contributor.userId,workDate:new Date().toISOString().slice(0,10),minutes:75,state:'approved',version:2,reviewerId:s.owner.userId});expect(time).toMatchObject({state:'approved',version:2});
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

it('reads the retained project graph and independent handoff snapshots without a TypeScript structure writer',async()=>{
 await grantProject();await grantProject('active',null,'projects-costs');
 const now=Date.now(),p=await newProject(),pack=await seedMilestone(db,{id:`milestone-${key()}`,companyId:f.company,projectId:p.id,title:'Retained read package',dueAt:Date.parse('2026-09-20T11:00:00Z'),ownerUserId:f.claimant.userId,createdBy:f.owner.userId,parentId:null,ordinal:0,dependsOn:[]});
 const time=await seedWorkLog(db,{id:`project-time-${key()}`,companyId:f.company,projectId:p.id,userId:f.claimant.userId,workDate:'2026-09-12',minutes:45,state:'approved',version:2,reviewerId:f.owner.userId});
 const line=await approvedExpense(),cost=await seedCostAllocation(db,{id:`project-cost-${key()}`,companyId:f.company,projectId:p.id,journalLineId:line,amountMinor:7501,createdBy:f.owner.userId,state:'approved',version:2,approvedBy:f.manager.userId});
 await db.prepare('INSERT INTO staff_project_package_actuals VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(`actual-time-${key()}`,f.company,p.id,pack.id,'work_log',time.id,2,45,null,null,f.owner.userId,now).run();
 await db.prepare('INSERT INTO staff_project_package_actuals VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(`actual-cost-${key()}`,f.company,p.id,pack.id,'cost_allocation',cost.id,2,null,7501,'THB',f.owner.userId,now).run();
 const graph=await projectGraph(f.owner,p.id);
 expect(graph.packages.find(row=>row.id===pack.id)).toMatchObject({approvedMinutes:45,costByCurrency:{THB:'7501'}});
 expect(graph.totals).toEqual({approvedMinutes:45,costByCurrency:{THB:'7501'}});
 expect((await projectGraph(f.claimant,p.id)).totals).toEqual({approvedMinutes:45,costByCurrency:{}});
 const handoffId=`project-handoff-${key()}`,snapshot=JSON.stringify(graph);
 await db.prepare('INSERT INTO staff_project_handoffs(id,company_id,project_id,source_project_version,recipient_id,status,version,snapshot_json,snapshot_sha256,evidence,prepared_by,prepared_at) VALUES(?,?,?,?,?,\'pending\',1,?::jsonb,?,?,?,?)').bind(handoffId,f.company,p.id,graph.project.version,f.head.userId,snapshot,'a'.repeat(64),'Retained read evidence.',f.owner.userId,now).run();
 await db.prepare("UPDATE staff_projects SET lifecycle_status='handoff_pending',version=version+1 WHERE id=?").bind(p.id).run();
 const ownerView=(await projectHandoffs(f.owner,p.id))[0],recipientView=(await projectHandoffs(f.head,p.id))[0];
 expect(ownerView).toMatchObject({id:handoffId,status:'pending',snapshot:{canReadCosts:true,totals:{approvedMinutes:45,costByCurrency:{THB:'7501'}}}});
 expect(recipientView).toMatchObject({id:handoffId,status:'pending',snapshot:{canReadCosts:false,totals:{approvedMinutes:0,costByCurrency:{}}}});
 expect((await projectHandoffWork(f.head)).find(item=>item.id===handoffId)).toMatchObject({queue:'todo',canAct:true,href:`/projects/${p.id}#project-structure`});
 expect((await projectHandoffWork(f.owner)).find(item=>item.id===handoffId)).toMatchObject({queue:'waiting',canAct:false});
});

it('closes hierarchical work packages with real tasks, independently reviewed time and posted costs, and preserves every issued handoff after rejection, acceptance and reopening',async()=>{
 await grantProject();await grantProject('active',null,'projects-costs');
 const now=Date.now(),p=await newProject(),root=await seedMilestone(db,{id:`milestone-${key()}`,companyId:f.company,projectId:p.id,title:'Customer delivery',dueAt:Date.parse('2026-09-20T11:00:00Z'),ownerUserId:f.claimant.userId,createdBy:f.owner.userId,parentId:null,ordinal:0,dependsOn:[]}),
 discovery=await seedMilestone(db,{id:`milestone-${key()}`,companyId:f.company,projectId:p.id,title:'Approved discovery',dueAt:Date.parse('2026-09-20T11:00:00Z'),ownerUserId:f.claimant.userId,createdBy:f.owner.userId,parentId:root.id,ordinal:1,dependsOn:[]}),
 build=await seedMilestone(db,{id:`milestone-${key()}`,companyId:f.company,projectId:p.id,title:'Implemented release',dueAt:Date.parse('2026-09-20T11:00:00Z'),ownerUserId:f.claimant.userId,createdBy:f.owner.userId,parentId:root.id,ordinal:2,dependsOn:[discovery.id]});
 const task=await saveTask(db,f.owner,null,taskSchema.parse({title:'Implement approved customer change',assigneeUserId:f.claimant.userId,projectId:p.id}),key());
 await db.prepare('INSERT INTO staff_project_task_packages(task_id,company_id,project_id,milestone_id,source_task_version,version,linked_by,linked_at) VALUES(?,?,?,?,1,1,?,?)').bind(task.id,f.company,p.id,build.id,f.owner.userId,now).run();
 const time=await seedWorkLog(db,{id:`project-time-${key()}`,companyId:f.company,projectId:p.id,userId:f.claimant.userId,workDate:'2026-09-12',minutes:125,state:'approved',version:2,reviewerId:f.owner.userId});
 const line=await approvedExpense(),cost=await seedCostAllocation(db,{id:`project-cost-${key()}`,companyId:f.company,projectId:p.id,journalLineId:line,amountMinor:7501,createdBy:f.owner.userId,state:'approved',version:2,approvedBy:f.manager.userId});
 await db.prepare('INSERT INTO staff_project_package_actuals VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(`actual-time-${key()}`,f.company,p.id,build.id,'work_log',time.id,2,125,null,null,f.owner.userId,now).run();
 await db.prepare('INSERT INTO staff_project_package_actuals VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(`actual-cost-${key()}`,f.company,p.id,build.id,'cost_allocation',cost.id,2,null,7501,'THB',f.owner.userId,now).run();
 const graph=await projectGraph(f.owner,p.id);
 expect(graph.packages.map(row=>[row.wbs,row.title,row.depth])).toEqual([['1','Customer delivery',0],['1.1','Approved discovery',1],['1.2','Implemented release',1]]);
 expect(graph.edges).toContainEqual({from:discovery.id,to:build.id,kind:'requires'});expect(graph.edges).toContainEqual({from:root.id,to:build.id,kind:'contains'});expect(graph.edges).toContainEqual({from:build.id,to:task.id,kind:'task'});
 expect(graph.packages[2]).toMatchObject({approvedMinutes:125,costByCurrency:{THB:'7501'},tasks:[{id:task.id,state:'ready',version:1}]});expect(graph.totals).toEqual({approvedMinutes:125,costByCurrency:{THB:'7501'}});
 expect((await projectGraph(f.claimant,p.id)).totals).toEqual({approvedMinutes:125,costByCurrency:{}});expect((await projectGraph(f.head,p.id)).totals).toEqual({approvedMinutes:0,costByCurrency:{}});
 const snapshot=JSON.stringify(graph),digest='a'.repeat(64),first=`project-handoff-${key()}`,second=`project-handoff-${key()}`;
 await db.prepare("INSERT INTO staff_project_handoffs(id,company_id,project_id,source_project_version,recipient_id,status,version,snapshot_json,snapshot_sha256,evidence,prepared_by,prepared_at,decided_by,decided_at) VALUES(?,?,?,?,?,'rejected',2,?::jsonb,?,'Reviewed rejection evidence.',?,?,?,?)").bind(first,f.company,p.id,graph.project.version,f.head.userId,snapshot,digest,f.owner.userId,now,f.head.userId,now+1).run();
 await db.prepare("INSERT INTO staff_project_handoffs(id,company_id,project_id,source_project_version,recipient_id,status,version,snapshot_json,snapshot_sha256,evidence,prepared_by,prepared_at,decided_by,decided_at) VALUES(?,?,?,?,?,'accepted',2,?::jsonb,?,'Reviewed acceptance evidence.',?,?,?,?)").bind(second,f.company,p.id,graph.project.version,f.head.userId,snapshot,digest,f.owner.userId,now+2,f.head.userId,now+3).run();
 await db.prepare("UPDATE staff_projects SET lifecycle_status='completed',version=version+1 WHERE id=?").bind(p.id).run();
 expect((await projectHandoffs(f.owner,p.id)).map(h=>h.status).sort()).toEqual(['accepted','rejected']);
 expect((await projectHandoffs(f.head,p.id))[0].snapshot.totals).toEqual({approvedMinutes:0,costByCurrency:{}});
 expect((await projectHandoffWork(f.head)).find(item=>item.id===second)).toMatchObject({queue:'completed',canAct:false,href:`/projects/${p.id}#project-structure`});
 await expect(db.prepare('UPDATE staff_project_handoffs SET evidence=? WHERE id=?').bind('Overwritten issued artifact',second).run()).rejects.toMatchObject({code:'23514'});
 await expect(db.prepare('DELETE FROM staff_project_package_actuals WHERE source_id=?').bind(time.id).run()).rejects.toMatchObject({code:'23514'});
 await expect(db.prepare('UPDATE staff_project_milestones SET title=? WHERE id=?').bind('Changed accepted package',build.id).run()).rejects.toMatchObject({code:'23514'});
});

it('rejects hierarchy and dependency deadlocks, stale task sources, competing structure commands and revoked membership without partial WBS writes',async()=>{
 await grantProject();
 const p=await newProject(),root=await seedMilestone(db,{id:`milestone-${key()}`,companyId:f.company,projectId:p.id,title:'Package parent',dueAt:Date.parse('2026-09-20T11:00:00Z'),ownerUserId:f.claimant.userId,createdBy:f.owner.userId,parentId:null,ordinal:0,dependsOn:[]}),
 child=await seedMilestone(db,{id:`milestone-${key()}`,companyId:f.company,projectId:p.id,title:'Dependent child',dueAt:Date.parse('2026-09-20T11:00:00Z'),ownerUserId:f.claimant.userId,createdBy:f.owner.userId,parentId:root.id,ordinal:1,dependsOn:[]});
 const graph=await projectGraph(f.owner,p.id),rootRow=graph.packages.find(row=>row.id===root.id)!;
 expect(rootRow).toMatchObject({id:root.id,version:1});
 expect(graph.packages.find(row=>row.id===child.id)).toMatchObject({version:1,depends_on_ids:[]});
 expect(graph.edges).toContainEqual({from:root.id,to:child.id,kind:'contains'});
 await db.prepare('DELETE FROM staff_project_members WHERE project_id=? AND user_id=?').bind(p.id,f.claimant.userId).run();await expect(projectGraph(f.claimant,p.id)).rejects.toMatchObject({status:404});
 await expect(projectGraph({...f.owner,companyId:'another-company'},p.id)).rejects.toMatchObject({status:404});
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();await expect(projectGraph(f.owner,p.id)).rejects.toMatchObject({status:404});
});
