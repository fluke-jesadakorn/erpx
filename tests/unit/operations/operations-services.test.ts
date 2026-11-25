import {describe,expect,test,vi} from 'vitest';
import {createStaffPolicyApplication,type StaffPolicyPorts,type StaffPolicyRepository} from "@erpx/server-operations/application/staff-policy";
import {createStaffTasksApplication,type StaffTasksPorts,type StaffTasksRepository} from "@erpx/server-operations/application/staff-tasks";
import {createStaffTaskAlertsApplication,type StaffTaskAlertsPorts} from "@erpx/server-operations/application/staff-task-alerts";
import {createHandoverApplication,type HandoverPorts} from "@erpx/server-operations/application/handover";
import {createOperationsExceptionsApplication,type OperationsExceptionsPorts} from "@erpx/server-operations/application/operations-exceptions";
import {createGuidedIntakeApplication,type GuidedIntakePorts,type GuidedIntakeRepository} from "@erpx/server-operations/application/guided-intake";
import {reportingChain,type StaffPerson} from "@erpx/server-operations/domain/staff-policy";
import type {TaskRow} from "@erpx/server-operations/domain/staff-tasks";
import {cleanDraft,explicitPurchaseQuantity} from "@erpx/server-operations/domain/guided-intake";
import {handoverPublishSchema,peopleInScope,type HandoverView} from "@erpx/server-operations/domain/handover";
import {bangkokToday} from "@erpx/server-operations/domain/handover-time";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {WorkSnapshot} from "@erpx/server-work_projection/public/work-items";
import type {OperationsCommands} from "@erpx/server-operations/domain/commands";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
const actor:WorkContext={userId:'owner',name:'Owner',email:'owner@example.test',assignmentId:'assignment',role:'holding_owner',companyId:'company',branchId:null,departmentId:null,holdingId:'holding',holdingSlug:'holding',holdingName:'Holding',scope:{kind:'holding',holdingId:'holding',holdingSlug:'holding',holdingName:'Holding',companyId:'company'},assignmentScope:'holding',inherited:false,modules:['operations'],assignments:[],workspaces:[],authority:{sessionId:'session',revision:1,assignmentVersion:1}};
function strict<Ports extends object>(facts:Partial<Ports>):Ports{return new Proxy(facts,{get(target,name){if(name in target)return target[name as keyof Ports];return ()=>{throw Error('Unexpected port: '+String(name));};}}) as Ports;}
const commands:OperationsCommands<string>={replay:async()=>null,commit:async(_a,_key,_operation,_input,_id,_version,result)=>result};
const person=(id:string,patch:Partial<StaffPerson>={}):StaffPerson=>({person_id:id,user_id:'user-'+id,legal_name:id,engagement_id:'engagement-'+id,engagement_version:1,company_id:'company',branch_id:null,department_id:null,worker_type:'employee',start_date:'2020-01-01',end_date:null,manager_person_id:null,level_id:null,level_name:null,level_rank:null,...patch});
function staff(repository:Partial<StaffPolicyRepository<string>>={}){return createStaffPolicyApplication(strict<StaffPolicyPorts<string>>({repository:strict(repository),clock:{now:()=>42},policyNow:()=>42,policyRead:(_key,run)=>run()}));}
describe('Operations staff facts preserve scope and independent routes',()=>{
 test('missing and ambiguous active engagements never choose a convenient record',async()=>{for(const results of [[],[person('one'),person('two')]])await expect(staff({readReadActiveStaffResults:async()=>({results})}).activeStaff('company','user')).rejects.toMatchObject({code:'STAFF_LINK_REQUIRED'});});
 test('staff query validity bounds use one injected instant',async()=>{const read=vi.fn(async()=>({results:[]}));await staff({readStaffPeopleResults:read}).staffPeople('company',{departmentId:'department',levelId:'level'});expect(read).toHaveBeenCalledWith('company',42,42,'department','department','level','level','level');});
 test('an absent engagement returns a blocked approval route in the original company scope',async()=>{expect(await staff({readReadActiveStaffResults:async()=>({results:[]})}).requestRoute('company','user','leave')).toMatchObject({steps:[],scope:{company_id:'company',branch_id:null,department_id:null},blockedReason:expect.stringContaining('active employee')});});
 test('reporting cycles and duplicate manager engagements require correction',()=>{expect(()=>reportingChain([person('one',{manager_person_id:'two'}),person('two',{manager_person_id:'one'})],'one')).toThrowError(expect.objectContaining({code:'REPORTING_CYCLE'}));expect(()=>reportingChain([person('one',{manager_person_id:'two'}),person('two'),person('two')],'one')).toThrowError(expect.objectContaining({code:'MANAGER_UNAVAILABLE'}));});
});
const task=(patch:Partial<TaskRow>={}):TaskRow=>({id:'task',company_id:'company',branch_id:null,department_id:null,title:'Task',description:'Work',creator_user_id:'owner',assignee_user_id:'assignee',assignee_name:'Assignee',creator_name:'Owner',project_id:null,project_title:null,project_owner_id:null,due_at:null,completion_note:'',start_approvers_json:'[]',completion_approvers_json:'[]',state:'ready',blocked_reason:null,version:1,revision:1,updated_at:0,template_id:null,template_version:null,schedule_id:null,occurrence_at:null,active_delegation_id:null,active_delegate_user_id:null,active_delegate_name:null,active_delegate_expires_at:null,...patch});
function tasks(row:TaskRow|null,overrides:Partial<StaffTasksPorts<string>>={}){return createStaffTasksApplication(strict<StaffTasksPorts<string>>({repository:strict<StaffTasksRepository<string>>({readGetTaskRow:async()=>row}),commands,clock:{now:()=>42},...overrides}));}
describe('Task reads keep source visibility checks',()=>{
 test('private conversation task visibility is checked even after a repository returned a row',async()=>{const read=vi.fn(async()=>{throw Error('private source denied');});await expect(tasks(task({source_post_id:'private-post'}),{readablePost:read}).getTask(actor,'task')).rejects.toThrow('private source denied');expect(read).toHaveBeenCalledWith(actor,'private-post');});
});
describe('Workflow projection source native read',()=>{
 test('task projection source delegates to the native owner plan and keeps no local SQL',async()=>{
  const prepare=vi.fn(()=>({bind:vi.fn()}));
  const batch=vi.fn(async()=>[{results:[{state:'ready',version:3,company_id:'company',branch_id:null,department_id:null}],success:true,meta:{changes:0,duration:0}}]);
  const db={prepare,batch} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({data:{statements:[{sql:'SELECT state,version FROM staff_tasks WHERE id=?',values:['task']}]}}),{status:200,headers:{'content-type':'application/json'}}));
  try{
   const {taskProjectionSource}=await import('@erpx/server-operations/adapters/workflow-projection-source');
   await expect(taskProjectionSource(db,'task')).resolves.toMatchObject({state:'ready',version:3});
   const call=fetchMock.mock.calls[0]!;
   expect(String(call[0])).toContain('/operations/command-plan');
   expect(JSON.parse(String(call[1]!.body))).toMatchObject({kind:'operations_rd_task_projection_source',values:['task']});
   expect(prepare).toHaveBeenCalledWith('SELECT state,version FROM staff_tasks WHERE id=?');
   expect(String((prepare.mock.calls[0] as unknown as [string])[0])).not.toContain('department_id');
  }finally{
   fetchMock.mockRestore();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  }
 });
});
describe('App upload task state native read',()=>{
 test('app upload task state delegates to the native owner plan and keeps no local SQL',async()=>{
  const prepare=vi.fn(()=>({bind:vi.fn()}));
  const batch=vi.fn(async()=>[{results:[{state:'ready'}],success:true,meta:{changes:0,duration:0}}]);
  const db={prepare,batch} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
  const plannedSql='SELECT state FROM staff_tasks WHERE id=? AND company_id=?';
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:['task','company']}]}}),{status:200,headers:{'content-type':'application/json'}}));
  try{
   const {appUploadTaskState}=await import('@erpx/server-operations/adapters/app-upload-task-source');
   await expect(appUploadTaskState(db,'task','company')).resolves.toMatchObject({state:'ready'});
   const call=fetchMock.mock.calls[0]!;
   expect(String(call[0])).toContain('/operations/command-plan');
   expect(JSON.parse(String(call[1]!.body))).toMatchObject({kind:'operations_rd_app_upload_task_state',values:['task','company']});
   expect(prepare).toHaveBeenCalledWith(plannedSql);
  }finally{
   fetchMock.mockRestore();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  }
 });
});
describe('Collaboration home task state native read dedupe',()=>{
 test('trackingState reuses the registered task-state plan and keeps no local SQL',async()=>{
  const prepare=vi.fn(()=>({bind:vi.fn()}));
  const batch=vi.fn(async()=>[{results:[{state:'ready'}],success:true,meta:{changes:0,duration:0}}]);
  const db={prepare,batch} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
  const plannedSql='SELECT state FROM staff_tasks WHERE id=? AND company_id=?';
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:['task','company']}]}}),{status:200,headers:{'content-type':'application/json'}}));
  try{
   const {collaborationHomeSources}=await import('@erpx/server-operations/adapters/collaboration-home-sources');
   await expect(collaborationHomeSources(db).trackingState('task','company')).resolves.toBe('ready');
   expect(fetchMock).toHaveBeenCalledTimes(1);
   const call=fetchMock.mock.calls[0]!;
   expect(String(call[0])).toContain('/operations/command-plan');
   expect(JSON.parse(String(call[1]!.body))).toMatchObject({kind:'operations_rd_app_upload_task_state',values:['task','company']});
   expect(prepare).toHaveBeenCalledWith(plannedSql);
   expect(prepare).toHaveBeenCalledTimes(1);
  }finally{
   fetchMock.mockRestore();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  }
 });
});
describe('Flow routing delegation native read',()=>{
  test('delegation reads delegate to the native owner plan and keep no local SQL',async()=>{
   const prepare=vi.fn(()=>({bind:vi.fn()}));
   const batch=vi.fn(async()=>[{results:[{delegate_user_id:'delegate'}],success:true,meta:{changes:0,duration:0}}]);
   const db={prepare,batch} as unknown as SqlDatabase;
   process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
   const plannedSql="SELECT delegate_user_id FROM staff_task_delegations WHERE company_id=? AND task_id=? AND status='active' AND starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY created_at DESC,id DESC LIMIT 1";
   const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:['company','task']}]}}),{status:200,headers:{'content-type':'application/json'}}));
   try{
    const {createFlowRoutingSourceRepository}=await import('@erpx/server-operations/adapters/workflow-flow-routing-facts');
    const repo=createFlowRoutingSourceRepository(db);
    await expect(repo.readDefinitionsDelegation('company','task')).resolves.toMatchObject({delegate_user_id:'delegate'});
    const call=fetchMock.mock.calls[0]!;
    expect(String(call[0])).toContain('/operations/command-plan');
    expect(JSON.parse(String(call[1]!.body))).toMatchObject({kind:'operations_rd_flow_definitions_delegation',values:['company','task']});
    expect(prepare).toHaveBeenCalledWith(plannedSql);
   }finally{
    fetchMock.mockRestore();
    delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
   }
  });
 });
describe('Beta management task owner native reads',()=>{
 const nativeConfig=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
 const fragment='EXISTS(SELECT 1 FROM staff_tasks t WHERE t.id=a.owner_id AND t.company_id=?)';
 const plan=(sql:string,values:unknown[])=>new Response(JSON.stringify({data:{statements:[{sql,values}]}}),{status:200,headers:{'content-type':'application/json'}});
 test('task company delegates to the native owner plan and keeps no local SQL',async()=>{
  const prepare=vi.fn(()=>({bind:vi.fn()}));
  const batch=vi.fn(async()=>[{results:[{company_id:'company'}],success:true,meta:{changes:0,duration:0}}]);
  const db={prepare,batch} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=nativeConfig;
  const plannedSql='SELECT company_id FROM staff_tasks WHERE id=?';
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(plannedSql,['task']));
  try{
   const {betaManagementTaskSources}=await import('@erpx/server-operations/adapters/beta-management-sources');
   await expect(betaManagementTaskSources(db).taskCompany('task')).resolves.toMatchObject({company_id:'company'});
   const call=fetchMock.mock.calls[0]!;
   expect(String(call[0])).toContain('/operations/command-plan');
   expect(JSON.parse(String(call[1]!.body))).toMatchObject({kind:'operations_rd_beta_management_task_company',values:['task']});
   expect(prepare).toHaveBeenCalledWith(plannedSql);
  }finally{
   fetchMock.mockRestore();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  }
 });
 test('task file owner returns the owner-planned correlated fragment',async()=>{
  const db={} as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=nativeConfig;
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(fragment,[]));
  try{
   const {betaManagementTaskSources}=await import('@erpx/server-operations/adapters/beta-management-sources');
   await expect(betaManagementTaskSources(db).taskFileOwner()).resolves.toBe(fragment);
   expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({kind:'operations_rd_beta_management_task_file_owner',values:[]});
  }finally{
   fetchMock.mockRestore();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  }
 });
 test('the server-files consumer composes the planned fragment and keeps every bind',async()=>{
  const first=vi.fn(async()=>({bytes:42}));
  const bind=vi.fn(()=>({first}));
  const prepare=vi.fn(()=>({bind}));
  const db={prepare} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=nativeConfig;
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(fragment,[]));
  try{
   const {betaManagementFacts}=await import('@erpx/server-files/adapters/beta-management-facts');
   await expect(betaManagementFacts(db).files('c1','c2','c3','c4','c5','c6')).resolves.toEqual({bytes:42});
   expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({kind:'operations_rd_beta_management_task_file_owner',values:[]});
   const sql=String((prepare.mock.calls[0] as unknown as [string])[0]);
   expect(sql).toContain("a.owner_type='staff_task'");
   expect(sql).toContain(fragment);
   expect(bind).toHaveBeenCalledWith('c1','c2','c3','c4','c5','c6');
  }finally{
   fetchMock.mockRestore();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  }
 });
});
describe('Routine template schedules native read',()=>{
 const nativeConfig=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
 const plannedSql='SELECT * FROM routine_schedules WHERE template_id=? AND company_id=? ORDER BY created_at';
 test('both owners delegate to the single native owner plan and keep no local SQL',async()=>{
  const prepare=vi.fn(()=>({bind:vi.fn()}));
  const batch=vi.fn(async()=>[{results:[{id:'schedule',template_id:'tpl'}],success:true,meta:{changes:0,duration:0}}]);
  const db={prepare,batch} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=nativeConfig;
  const fetchMock=vi.spyOn(globalThis,'fetch').mockImplementation(async()=>new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values:['tpl','company']}]}}),{status:200,headers:{'content-type':'application/json'}}));
  try{
   const {routineTemplateSchedules}=await import('@erpx/server-operations/adapters/people-app-entrypoint-facts');
   const {workUiStaffFacts}=await import('@erpx/server-operations/adapters/work-ui-staff-facts');
   await expect(routineTemplateSchedules(db,'tpl','company')).resolves.toMatchObject({results:[{id:'schedule'}]});
   await expect(workUiStaffFacts(db).templateSchedules('tpl','company')).resolves.toMatchObject({results:[{id:'schedule'}]});
   expect(fetchMock).toHaveBeenCalledTimes(2);
   for(const call of fetchMock.mock.calls){
    expect(String(call[0])).toContain('/operations/command-plan');
    expect(JSON.parse(String(call[1]!.body))).toMatchObject({kind:'operations_rd_routine_template_schedules',values:['tpl','company']});
   }
   expect(prepare).toHaveBeenCalledWith(plannedSql);
   expect(prepare).toHaveBeenCalledTimes(2);
  }finally{
   fetchMock.mockRestore();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  }
 });
});
describe('Field command owner native reads',()=>{
 const nativeConfig=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
 const fieldActor={...actor,holdingId:'h1',companyId:'company'};
 const exceptionSql='SELECT source_kind FROM operations_exceptions WHERE id=? AND company_id=?';
 const policySql='SELECT source_kinds_json FROM operations_queue_policies WHERE id=? AND company_id=?';
 const plan=(sql:string,values:unknown[])=>new Response(JSON.stringify({data:{statements:[{sql,values}]}}),{status:200,headers:{'content-type':'application/json'}});
 const harness=(rows:unknown[])=>{
  const prepare=vi.fn(()=>({bind:vi.fn()}));
  const batch=vi.fn(async()=>[{results:rows,success:true,meta:{changes:0,duration:0}}]);
  const db={prepare,batch} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=nativeConfig;
  vi.resetModules();
  vi.doMock('@erpx/server-runtime/public/runtime',()=>({holdingResources:async()=>({write:db})}));
  return {prepare,batch};
 };
 const restore=()=>{vi.doUnmock('@erpx/server-runtime/public/runtime');vi.resetModules();delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;};
 test('exception routing delegates to the native owner plan and keeps no local SQL',async()=>{
  const {prepare}=harness([{source_kind:'field_service_order'}]);
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(exceptionSql,['ex','company']));
  try{
   const {nativeFieldException}=await import('../../../packages/server-operations/src/adapters/field-command-owner');
   await expect(nativeFieldException(fieldActor,'ex')).resolves.toBe(true);
   expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({kind:'operations_rd_field_exception',values:['ex','company']});
   expect(prepare).toHaveBeenCalledWith(exceptionSql);
  }finally{fetchMock.mockRestore();restore();}
 });
 test('a non-field exception returns false under the native plan',async()=>{
  harness([{source_kind:'staff_task'}]);
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(exceptionSql,['ex','company']));
  try{
   const {nativeFieldException}=await import('../../../packages/server-operations/src/adapters/field-command-owner');
   await expect(nativeFieldException(fieldActor,'ex')).resolves.toBe(false);
  }finally{fetchMock.mockRestore();restore();}
 });
 test('policy routing delegates to the native owner plan and keeps field-only semantics',async()=>{
  const {prepare}=harness([{source_kinds_json:'["field_service_order"]'}]);
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(policySql,['pol','company']));
  try{
   const {nativeFieldPolicy}=await import('../../../packages/server-operations/src/adapters/field-command-owner');
   await expect(nativeFieldPolicy(fieldActor,'pol')).resolves.toBe(true);
   expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({kind:'operations_rd_field_policy',values:['pol','company']});
   expect(prepare).toHaveBeenCalledWith(policySql);
  }finally{fetchMock.mockRestore();restore();}
 });
 test('a missing policy row returns false without a local SQL fallback',async()=>{
  harness([]);
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(policySql,['pol','company']));
  try{
   const {nativeFieldPolicy}=await import('../../../packages/server-operations/src/adapters/field-command-owner');
   await expect(nativeFieldPolicy(fieldActor,'pol')).resolves.toBe(false);
  }finally{fetchMock.mockRestore();restore();}
 });
});
describe('Legacy routine access native read',()=>{
 const nativeConfig=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
 const plannedSql='SELECT t.* FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NULL AND (t.published=1 OR t.creator_user_id=? OR ?=1) ORDER BY t.id';
 const plan=(values:unknown[])=>new Response(JSON.stringify({data:{statements:[{sql:plannedSql,values}]}}),{status:200,headers:{'content-type':'application/json'}});
 const harness=(templates:unknown[],owner:boolean)=>{
  const prepare=vi.fn(()=>({bind:vi.fn()}));
  const batch=vi.fn(async()=>[{results:templates,success:true,meta:{changes:0,duration:0}}]);
  const db={prepare,batch} as unknown as SqlDatabase;
  process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=nativeConfig;
  vi.resetModules();
  vi.doMock('@erpx/server-identity_access/public/operations-staff-policy-facts',()=>({
   StaffPolicyIdentityAccessFacts:()=>({
    readReadActiveStaffResults:async()=>({results:[{person_id:'p',engagement_id:'e',engagement_version:1,company_id:'company',branch_id:null,department_id:null,start_date:0,end_date:null}]}),
    readCompanyAuthorityEffect:async()=>owner,
   }),
  }));
  return {db,prepare};
 };
 const restore=()=>{vi.doUnmock('@erpx/server-identity_access/public/operations-staff-policy-facts');vi.resetModules();delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;};
 test('owner templates delegate to the native owner plan and keep no local SQL',async()=>{
  const {db,prepare}=harness([{id:'tpl',title:'Template'}],true);
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(['company','owner',1]));
  try{
   const {legacyRoutineAccess}=await import('../../../packages/server-operations/src/adapters/legacy-routine-access');
   const result=await legacyRoutineAccess(db,'company',{id:'assignment',userId:'owner'},0);
   expect(result).toMatchObject({source:'operations/original-routines/v1',activeStaff:true,retainedTemplates:1,revision:expect.any(String)});
   const call=fetchMock.mock.calls[0]!;
   expect(String(call[0])).toContain('/operations/command-plan');
   expect(JSON.parse(String(call[1]!.body))).toMatchObject({kind:'operations_rd_legacy_routine_templates',values:['company','owner',1]});
   expect(prepare).toHaveBeenCalledWith(plannedSql);
  }finally{fetchMock.mockRestore();restore();}
 });
 test('a non-owner keeps the original visibility bind without a local SQL fallback',async()=>{
  const {db}=harness([],false);
  const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(['company','owner',0]));
  try{
   const {legacyRoutineAccess}=await import('../../../packages/server-operations/src/adapters/legacy-routine-access');
   const result=await legacyRoutineAccess(db,'company',{id:'assignment',userId:'owner'},0);
   expect(result.retainedTemplates).toBe(0);
   expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({kind:'operations_rd_legacy_routine_templates',values:['company','owner',0]});
  }finally{fetchMock.mockRestore();restore();}
 });
});
describe('Operations exceptions native reads',()=>{
  const nativeConfig=JSON.stringify({privateBaseUrl:'http://127.0.0.1:3999',timeoutMs:1000,holdings:[{origin:'https://acme.jesadakorn.com',holdingSlug:'acme',holdingId:'h1',companyId:'company'}]});
  const policiesSql='SELECT * FROM operations_queue_policies WHERE company_id=? AND NOT (jsonb_typeof(source_kinds_json::jsonb)=\'array\' AND source_kinds_json::jsonb<>\'[]\'::jsonb AND source_kinds_json::jsonb <@ \'["field_service_order"]\'::jsonb) ORDER BY area,revision DESC';
  const eventsSql='SELECT * FROM operations_exception_events WHERE exception_id=? ORDER BY version';
  const plan=(sql:string,values:unknown[])=>new Response(JSON.stringify({data:{statements:[{sql,values}]}}),{status:200,headers:{'content-type':'application/json'}});
  const ex={id:'ex',source_kind:'staff_task',source_id:'src',status:'open',title:'T',issue:'I',updated_at:1,due_at:2,area:'a',owner_id:'owner',created_by:'owner',resolved_by:null,version:1};
  const harness=(policies:unknown[],events:unknown[])=>{
   const prepare=vi.fn((sql:unknown)=>({bind:()=>({sql,all:async()=>({results:[ex]})})}));
   const batch=vi.fn(async(stmts:{sql:unknown}[])=>{
    const sql=String(stmts[0]?.sql??'');
    return [{results:sql.includes('operations_queue_policies')?policies:events,success:true,meta:{changes:0,duration:0}}];
   });
   const db={prepare,batch} as unknown as SqlDatabase;
   process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG=nativeConfig;
   vi.resetModules();
   vi.doMock('@erpx/server-runtime/public/runtime',()=>({holdingResources:async()=>({write:db})}));
   vi.doMock('@erpx/server-identity_access/public/operations-operations-exceptions-facts',()=>({OperationsExceptionsIdentityAccessFacts:()=>({readOperationsWorkspaceCandidatespoliciespeople3:async()=>({results:[]})})}));
   vi.doMock('@erpx/server-identity_access/public/iam-permissions',()=>({requirePermission:()=>{}}));
   vi.doMock('@erpx/server-records/public/source-record-policy',()=>({visibleSourceRecord:async()=>({kind:'staff_task',id:'src'})}));
   return {db,prepare,batch};
  };
  const restore=()=>{
   vi.doUnmock('@erpx/server-runtime/public/runtime');
   vi.doUnmock('@erpx/server-identity_access/public/operations-operations-exceptions-facts');
   vi.doUnmock('@erpx/server-identity_access/public/iam-permissions');
   vi.doUnmock('@erpx/server-records/public/source-record-policy');
   vi.resetModules();
   delete process.env.ERPX_NATIVE_OPERATIONS_READ_CONFIG;
  };
  test('workspace queue policies delegate to the native owner plan without local SQL',async()=>{
   harness([{area:'a'}],[]);
   const fetchMock=vi.spyOn(globalThis,'fetch').mockResolvedValue(plan(policiesSql,['company']));
   try{
    const {operationsWorkspace}=await import('../../../packages/server-operations/src/adapters/operations-exceptions');
    const result=await operationsWorkspace(actor);
    expect(result.policies).toHaveLength(1);
    expect(String(fetchMock.mock.calls[0]![0])).toContain('/operations/command-plan');
    expect(JSON.parse(String(fetchMock.mock.calls[0]![1]!.body))).toMatchObject({kind:'operations_rd_operations_queue_policies',values:['company']});
   }finally{fetchMock.mockRestore();restore();}
  });
  test('exception detail history delegates to the native owner plan without local SQL',async()=>{
   harness([],[{version:1,action:'open'}]);
   const fetchMock=vi.spyOn(globalThis,'fetch').mockImplementation(async(_input:RequestInfo|URL,init?:RequestInit)=>{
    const kind=JSON.parse(String(init?.body)).kind;
    return plan(kind==='operations_rd_operations_exception_events'?eventsSql:policiesSql,['ex']);
   });
   try{
    const {operationsExceptionDetail}=await import('../../../packages/server-operations/src/adapters/operations-exceptions');
    const result=await operationsExceptionDetail(actor,'ex');
    expect(result.history).toHaveLength(1);
    const kinds=fetchMock.mock.calls.map(call=>JSON.parse(String(call[1]!.body)).kind);
    expect(kinds).toContain('operations_rd_operations_exception_events');
   }finally{fetchMock.mockRestore();restore();}
  });
 });
describe('Handover source and colleague constraints',()=>{
 test('Bangkok day derives entirely from the supplied instant',()=>{expect(bangkokToday(Date.UTC(2026,0,1,20))).toBe('2026-01-02');});
 test('duplicate recipients and source references fail validation',()=>{const source={id:'task',sourceVersion:1,kind:'staff_task' as const},base={date:'2026-01-02',body:'Shift report',sources:[source],recipientIds:['colleague']};expect(handoverPublishSchema.safeParse({...base,sources:[source,source]}).success).toBe(false);expect(handoverPublishSchema.safeParse({...base,recipientIds:['colleague','colleague']}).success).toBe(false);});
 test('recipient facts are deduplicated and limited to selected branch and department',()=>{const selected={...actor,branchId:'branch',departmentId:'department'},one=person('one',{branch_id:'branch',department_id:'department'});expect(peopleInScope(selected,[one,one,person('other'),person('self',{user_id:'owner',branch_id:'branch',department_id:'department'})])).toEqual([one]);});
 test('a stale handover day fails before opening any source or post port',async()=>{const app=createHandoverApplication(strict<HandoverPorts<string>>({clock:{now:()=>Date.UTC(2026,0,2,20)}}));await expect(app.publishHandover(actor,{items:[],unavailableSources:[]} satisfies WorkSnapshot,{date:'2026-01-02',body:'Shift report',sources:[{id:'task',sourceVersion:1}],recipientIds:['colleague']},'key')).rejects.toMatchObject({code:'HANDOVER_DATE_CHANGED'});});
});
describe('Guided intake creates proposals only from explicit user facts',()=>{
 test('purchase quantities are not inferred from conflicting or absent counts',()=>{expect(explicitPurchaseQuantity('two monitors','monitors')).toBe('2');expect(explicitPurchaseQuantity('two monitors, quantity: 3','monitors')).toBeNull();expect(explicitPurchaseQuantity('need monitors','monitors')).toBeNull();});
 test('model money, currency and dates absent from the message are discarded',()=>{expect(cleanDraft('expense',{amount:'9000',currency:'USD',expenseDate:'2026-10-06',paymentIntent:'company_payment'},'client meal')).toEqual({paymentIntent:'company_payment'});expect(cleanDraft('leave',{startDate:'2026-02-30'},'2026-02-30')).toEqual({});});
 test('unknown intents cannot produce any prepared fields',()=>{expect(cleanDraft('unknown',{amount:'100',body:'Publish immediately'},'100')).toEqual({});});
 test('failed fresh authority guards prevent AI provider discovery',async()=>{const batch=vi.fn(async()=>{throw Error('revoked');}),app=createGuidedIntakeApplication(strict<GuidedIntakePorts<string>>({identifiers:{next:()=> 'key'},workContextGuards:()=>[],iamMutationGuards:()=>[],repository:strict<GuidedIntakeRepository<string>>({stageCheckAuthorityEffect:async()=> 'cleanup'}),batch}));await expect(app.guidedIntakeOptions(actor,{})).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});expect(batch).toHaveBeenCalledWith(['cleanup']);});
});
test('Operations module access fails before database resource preparation',async()=>{const app=createOperationsExceptionsApplication(strict<OperationsExceptionsPorts<string>>({requirePermission:a=>a}));await expect(app.operationsWorkspace({...actor,modules:[]})).rejects.toMatchObject({code:'MODULE_FORBIDDEN'});});
test('overdue notifications deduplicate recipients, preserve explicit time and count only inserted effects',async()=>{const writes:unknown[][]=[];const app=createStaffTaskAlertsApplication(strict<StaffTaskAlertsPorts<string>>({production:true,repository:{readEscalateStaffTasksRows:async()=>({results:[{id:'task',company_id:'company',title:'Work',due_at:41,version:3,assignee_user_id:'same',creator_user_id:'same',project_owner_id:'owner',delegate_user_id:'same'}]}),readEscalateStaffTasksApprovers:async()=>({results:[{approver_user_id:'owner'}]}),stageEscalateStaffTasksResult:async(...args)=>{writes.push(args);return {success:true,meta:{changes:args[2]==='same'?1:0}};}}}));expect(await app.escalateStaffTasks(42)).toEqual({dueSoon:0,overdue:1});expect(writes).toHaveLength(2);for(const args of writes){expect(args[7]).toBe(42);expect(args[8]).toEqual(['company']);}});

import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {createRoutineCommands,validRoutineReceipt,type RoutineAttempt} from '@erpx/feature-operations/lib/routine-command';
import {routineSourcePurchase} from '@erpx/feature-operations/lib/routine-purchase-controls';
import {RoutineSchedules} from '@erpx/feature-operations/components/routine-schedule-controls';
import {RoutineCommandFeedback} from '@erpx/feature-operations/components/routine-command-feedback';
import type {useRoutineCommand} from '@erpx/feature-operations/components/use-routine-command';
import type {TemplateRow,ScheduleRow} from '@erpx/server-operations/domain/routines';
import {createStaffApiService,type StaffApiPorts} from '@erpx/server-work_projection/application/staff-api';
vi.mock('next/navigation',()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn()}),usePathname:()=>'/holdings/holding/projects-routines/routines',useSearchParams:()=>new URLSearchParams()}));
const routineBase='/holdings/holding',routineContext='opaque-original-context',routineResponse=(data:unknown,status=200)=>Response.json({data},{status});
const routineAttempt=(path:string,input:Record<string,unknown>={}):RoutineAttempt=>({path,input,body:JSON.stringify(input),key:'key',context:routineContext,existing:false});
describe('Routine exact native receipts and uncertain submitted commands',()=>{
 test.each([
  ['routine-templates',{}, {id:'routine-key',version:1}],
  ['routine-templates/source/clone',{}, {id:'routine-key',version:1}],
  ['routine-templates/source',{expectedVersion:3},{id:'source',version:4}],
  ['routine-templates/source/schedules',{}, {id:'schedule-key',version:1,state:'paused'}],
  ['routine-templates/source/schedules/schedule',{expectedVersion:3},{id:'schedule',version:4,state:'paused'}],
  ['routine-templates/source/run',{}, {id:'task-key',version:1,state:'ready'}],
  ['routine-schedules/schedule/pause',{expectedVersion:3},{id:'schedule',version:4,state:'paused'}],
  ['routine-schedules/schedule/resume',{expectedVersion:3},{id:'schedule',version:4,state:'active'}],
  ['routine-schedules/schedule/end',{expectedVersion:3},{id:'schedule',version:4,state:'ended'}],
 ])('requires the original identity version and phase for %s', (path,input,receipt)=>{const attempt=routineAttempt(path,input);expect(validRoutineReceipt(attempt,receipt)).toBe(true);expect(validRoutineReceipt(attempt,{...receipt,id:'unrelated'})).toBe(false);expect(validRoutineReceipt(attempt,{...receipt,version:99})).toBe(false);});
 test.each([{id:'schedule',version:4,state:'active'},{id:'schedule',version:3,state:'ended'},{id:'schedule',version:4},null])('retains malformed or mismatched HTTP200: %j',async value=>{const commands=createRoutineCommands(async()=>routineResponse(value),()=> 'key'),attempt=commands.prepare('routine-schedules/schedule/end',{expectedVersion:3},undefined,routineContext,true);await expect(commands.execute(routineBase,attempt,()=>routineContext)).rejects.toThrow('acknowledgement is incomplete');expect(commands.pending()).toBe(attempt);});
 test('retains original body key and source through uncertain transport later403 and newer edits',async()=>{const send=vi.fn().mockRejectedValueOnce(Error('unknown transport')).mockResolvedValueOnce(Response.json({error:{message:'Current role revoked'}},{status:403})).mockResolvedValueOnce(routineResponse({id:'routine-key',version:1})),commands=createRoutineCommands(send,()=> 'key'),draft={title:'Submitted original source'},attempt=commands.prepare('routine-templates',draft,undefined,routineContext);await expect(commands.execute(routineBase,attempt,()=>routineContext)).rejects.toThrow('unknown transport');draft.title='Newer draft';expect(attempt.input.title).toBe('Submitted original source');expect(()=>commands.prepare('routine-templates',draft,undefined,routineContext)).toThrow('Retry the submitted routine action first');await expect(commands.execute(routineBase,attempt,()=>routineContext)).rejects.toThrow('Current role revoked');expect(commands.pending()).toBe(attempt);expect(await commands.execute(routineBase,attempt,()=>routineContext)).toEqual({result:{id:'routine-key',version:1},unchanged:true});expect(send.mock.calls.map(call=>call[1].body)).toEqual([attempt.body,attempt.body,attempt.body]);expect(send.mock.calls.map(call=>call[1].headers['Idempotency-Key'])).toEqual(['key','key','key']);expect(send.mock.calls.every(call=>call[3]==='caller')).toBe(true);});
 test('keeps later form edits and disconnected old drawers after a matching durable receipt',async()=>{let edit='submitted';const form={isConnected:true,getAttribute:()=>edit} as Pick<HTMLFormElement,'isConnected'|'getAttribute'>,send=vi.fn(async()=>{edit='newer';return routineResponse({id:'routine-key',version:1});}),commands=createRoutineCommands(send,()=> 'key'),attempt=commands.prepare('routine-templates',{},form as HTMLFormElement,routineContext);expect(await commands.execute(routineBase,attempt,()=>routineContext)).toMatchObject({unchanged:false});Object.defineProperty(form,'isConnected',{value:false});const second=createRoutineCommands(async()=>routineResponse({id:'routine-key',version:1}),()=> 'key');expect(await second.execute(routineBase,second.prepare('routine-templates',{},form as HTMLFormElement,routineContext),()=>routineContext)).toMatchObject({unchanged:false});});
 test('checks context before send and after response and never treats truncated JSON as success',async()=>{let current=routineContext;const send=vi.fn(async()=>{current='different-account-company-role';return routineResponse({id:'routine-key',version:1});}),commands=createRoutineCommands(send,()=> 'key'),attempt=commands.prepare('routine-templates',{},undefined,routineContext);await expect(commands.execute(routineBase,attempt,()=>current)).rejects.toThrow('original routine context');expect(commands.pending()).toBe(attempt);await expect(commands.execute(routineBase,attempt,()=>current)).rejects.toThrow('original routine context');expect(send).toHaveBeenCalledTimes(1);const truncated=createRoutineCommands(async()=>new Response('{"data":',{status:200}),()=> 'key'),submitted=truncated.prepare('routine-templates',{},undefined,routineContext);await expect(truncated.execute(routineBase,submitted,()=>routineContext)).rejects.toThrow();expect(truncated.pending()).toBe(submitted);});
});
const routineActive={newWork:true,settleExisting:true,history:true,revision:'active'},routineDraining={...routineActive,newWork:false},routineInactive={...routineDraining,settleExisting:false},routinePurchases={work:routineDraining,projects:routineActive,context:routineContext};
const nativeTemplate:TemplateRow={id:'template',company_id:'company',branch_id:null,department_id:null,title:'Real retained routine',description:'Source history',creator_user_id:'owner',assignee_user_id:'assignee',project_id:null,start_approvers_json:'[]',completion_approvers_json:'["reviewer"]',published:1,version:2,created_at:1,updated_at:2},nativeSchedule:ScheduleRow={id:'schedule',template_id:'template',company_id:'company',frequency:'daily',timezone:'Asia/Bangkok',occurrence_time:'09:00',start_date:'2030-01-01',end_date:null,weekday:null,month_day:null,due_offset_hours:24,state:'paused',next_occurrence_at:1893463200000,paused_at:1,last_generated_at:null,last_error:null,version:3,created_at:1,updated_at:2};
describe('Routine minimal actual source and server presentation',()=>{
 test('keeps Project-only foundation and intersects the actual old source when moving or cloning',()=>{expect(routineSourcePurchase(routinePurchases,'project','project')?.newWork).toBe(true);expect(routineSourcePurchase(routinePurchases,'project',null)?.newWork).toBe(false);expect(routineSourcePurchase({...routinePurchases,projects:routineInactive},null,'project')?.settleExisting).toBe(false);expect(routineSourcePurchase(undefined,null)).toBeUndefined();});
 test('server-renders actual schedule history and original end/unpublish controls with no private authority or queued snapshot',()=>{const html=renderToStaticMarkup(createElement(RoutineSchedules,{base:routineBase,template:nativeTemplate,schedules:[nativeSchedule],canEdit:true,purchases:routinePurchases}));expect(html).toContain('daily at');expect(html).toContain('paused');expect(html).toMatch(/disabled=""[^>]*>Resume schedule/);expect(html).toContain('End schedule');expect(html).toContain('Unpublish routine');expect(html).not.toContain('Edit paused schedule');expect(html).not.toContain('Add schedule');for(const secret of ['queuedAssignmentVersion','authority_json','sessionId','source_hash','permissions'])expect(html).not.toContain(secret);});
 test('keeps inactive actual source actions disabled while retaining normal schedule facts',()=>{const html=renderToStaticMarkup(createElement(RoutineSchedules,{base:routineBase,template:nativeTemplate,schedules:[nativeSchedule],canEdit:true,purchases:{...routinePurchases,work:routineInactive}}));expect(html).toMatch(/disabled=""[^>]*>End schedule/);expect(html).toMatch(/disabled=""[^>]*>Unpublish routine/);expect(html).toContain('Next occurrence');});
 test('keeps retained retry and saved later-draft recovery in the editor feedback under the current original role',()=>{const command:ReturnType<typeof useRoutineCommand>={send:vi.fn(async()=>null),retry:vi.fn(async()=>null),retryAttempt:routineAttempt('routine-schedules/schedule/end',{expectedVersion:3}),busy:false,error:'Unknown',notice:'Newer draft remains',saved:{receipt:{id:'schedule',version:4,state:'ended'},href:'/saved'},clearSaved:vi.fn(),retryAllowed:true},html=renderToStaticMarkup(createElement(RoutineCommandFeedback,{command}));expect(html).toContain('Retry submitted routine action');expect(html).toContain('Closing this editor does not cancel that request');expect(html).toContain('Open saved routine work');expect(html).toContain('Review retained draft');expect(html).not.toContain('Idempotency-Key');expect(renderToStaticMarkup(createElement(RoutineCommandFeedback,{command:{...command,retryAllowed:false}}))).toContain('disabled=""');});
 test('does not answer any retired routine writer on the staff API after the native run mount',async()=>{const app=createStaffApiService(strict<StaffApiPorts>({requireWorkContext:async()=>actor,prepare:async()=>{},error:(status,code)=>Object.assign(new Error(code),{status,code})})),request=(path:string[],body:unknown)=>app('routine-templates',new Request('https://console-dev-erpx.jesadakorn.com/api/owner',{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':'original-key'},body:JSON.stringify(body)}),{params:Promise.resolve({holdingSlug:'holding',path})});for(const [path,body] of [[['template','run'],{dueAt:'2026-12-01T02:00:00Z'}],[[],{title:'Gone writer'}],[['template'],{expectedVersion:1}],[['template','clone'],{}],[['template','schedules'],{}]] as const)await expect(request([...path],body)).rejects.toMatchObject({status:404,code:'NOT_FOUND'});});
});

import {createHandoverReceiptCommands,handoverReceiptMatches} from '@erpx/feature-work_projection/lib/handover-acknowledgement';
import {HandoverAcknowledgement as HandoverReceiptView} from '@erpx/feature-work_projection/components/today/handover-acknowledgement';
const handoverEndpoint='/api/v1/holdings/test/handover/post/acknowledgement';
describe('Handover current seed and exact submitted receipt',()=>{
 test('renders the matching original server receipt and draining control before hydration without a status-loading placeholder',()=>{const html=renderToStaticMarkup(createElement(HandoverReceiptView,{api:'/api/v1/holdings/test',postId:'post',version:3,contextToken:'opaque',initial:{postId:'post',postVersion:3,canAcknowledge:true,acknowledged:false,recipientCount:2,acknowledgedCount:1},purchase:{newWork:false,settleExisting:true,history:true,revision:'opaque-purchase'}}));expect(html).toContain('1 of 2 recipients confirmed receipt');expect(html).toContain('Confirm receipt');expect(html).not.toContain('Loading handover receipt');expect(html).not.toContain('authority_json');});
 test('renders retained inactive receipt counts with no new confirmation action',()=>{const html=renderToStaticMarkup(createElement(HandoverReceiptView,{api:'/api/v1/holdings/test',postId:'post',version:3,initial:{postId:'post',postVersion:3,canAcknowledge:true,acknowledged:false,recipientCount:2,acknowledgedCount:1},purchase:{newWork:false,settleExisting:false,history:true,revision:'opaque'}}));expect(html).toContain('1 of 2');expect(html).toMatch(/disabled=""/);});
 test('does not treat another source/version or malformed counts as a matching initial snapshot',()=>{const seed={postId:'post',postVersion:3,canAcknowledge:true,acknowledged:false,recipientCount:2,acknowledgedCount:1};expect(handoverReceiptMatches(seed,'post',3)).toBe(true);expect(handoverReceiptMatches(seed,'other',3)).toBe(false);expect(handoverReceiptMatches(seed,'post',4)).toBe(false);expect(handoverReceiptMatches({...seed,acknowledgedCount:3},'post',3)).toBe(false);expect(handoverReceiptMatches({...seed,recipientCount:'2'},'post',3)).toBe(false);});
 test('retries the exact original key body and rendered version after an incomplete acknowledgement',async()=>{const calls:RequestInit[]=[],commands=createHandoverReceiptCommands(async(_url,init)=>{calls.push(init!);return Response.json({data:{postId:calls.length===1?'other':'post',postVersion:3,acknowledged:true}});},()=> 'original-key'),attempt=commands.prepare(handoverEndpoint,'post',3,'original');await expect(commands.execute(attempt,()=> 'original')).rejects.toThrow(/incomplete/);expect(()=>commands.prepare(handoverEndpoint,'post',4,'original')).toThrow(/another version/);expect(await commands.execute(attempt,()=> 'original')).toMatchObject({postId:'post',postVersion:3,acknowledged:true});expect(calls[0].body).toBe('{}');expect(calls[1].body).toBe(calls[0].body);expect(new Headers(calls[0].headers).get('Idempotency-Key')).toBe(new Headers(calls[1].headers).get('Idempotency-Key'));expect(new Headers(calls[1].headers).get('x-erpx-handover-version')).toBe('3');});
 test('retains a successful-looking receipt if actual selected context changes before its acknowledgement',async()=>{let context='original';const commands=createHandoverReceiptCommands(async()=>{context='other';return Response.json({data:{postId:'post',postVersion:3,acknowledged:true}});},()=> 'key'),attempt=commands.prepare(handoverEndpoint,'post',3,context);await expect(commands.execute(attempt,()=>context)).rejects.toThrow(/account or role changed/);expect(commands.pending()).toEqual(attempt);});
 test('a first definitive current denial allows review but later denial cannot erase an uncertain original result',async()=>{let count=0;const commands=createHandoverReceiptCommands(async()=>Response.json({error:{message:'Original current recipient'}},{status:++count===1?503:403}),()=> 'key'),attempt=commands.prepare(handoverEndpoint,'post',3,'original');await expect(commands.execute(attempt,()=> 'original')).rejects.toThrow('Original current recipient');await expect(commands.execute(attempt,()=> 'original')).rejects.toThrow('Original current recipient');expect(commands.pending()).toEqual(attempt);const denied=createHandoverReceiptCommands(async()=>Response.json({error:{message:'Original current recipient'}},{status:403}),()=> 'key');await expect(denied.execute(denied.prepare(handoverEndpoint,'post',3,'original'),()=> 'original')).rejects.toThrow('Original current recipient');expect(denied.pending()).toBeNull();});
});

import {createHandoverPublishCommands,handoverPublishReceiptMatches,handoverViewMatches,type HandoverInput} from '@erpx/feature-work_projection/lib/handover-command';
const handoverPublishInput:HandoverInput={date:'2026-10-16',body:'Original reviewed handover',sources:[{id:'task',sourceVersion:3,kind:'staff_task'}],recipientIds:['receiver']},handoverPublishKey='11111111-1111-4111-8111-111111111111';
const publishedHandover={id:`post-post-draft-${handoverPublishKey}`,version:1};
describe('Handover publication exact recovery and retained newer drafts',()=>{
 test.each([null,{id:'unrelated',version:1},{id:publishedHandover.id},{...publishedHandover,version:2}])('retains original submission on incomplete or foreign successful receipt %j',async value=>{const command=createHandoverPublishCommands(async()=>Response.json({data:value}),()=>handoverPublishKey),attempt=command.prepare('/handover',handoverPublishInput,'original-context',2);await expect(command.execute(attempt,()=> 'original-context',()=>2)).rejects.toThrow('acknowledgement is incomplete');expect(command.pending()).toBe(attempt);expect(handoverPublishReceiptMatches(attempt,publishedHandover)).toBe(true);});
 test('retries the exact committed body and key while later edited content remains a separate draft',async()=>{const calls:RequestInit[]=[],input=structuredClone(handoverPublishInput),command=createHandoverPublishCommands(async(_url,init)=>{calls.push(init!);if(calls.length===1)throw Error('unknown transport');return Response.json({data:publishedHandover});},()=>handoverPublishKey),attempt=command.prepare('/handover',input,'context',2);await expect(command.execute(attempt,()=> 'context',()=>2)).rejects.toThrow('unknown transport');input.body='Newer private draft';input.recipientIds.push('new-recipient');expect(()=>command.prepare('/handover',input,'context',3)).toThrow('Retry the submitted handover first');const reply=await command.execute(attempt,()=> 'context',()=>3);expect(reply).toEqual({receipt:publishedHandover,unchanged:false});expect(calls[0].body).toBe(calls[1].body);expect(JSON.parse(String(calls[1].body))).toEqual(handoverPublishInput);expect(new Headers(calls[0].headers).get('Idempotency-Key')).toBe(handoverPublishKey);expect(new Headers(calls[1].headers).get('Idempotency-Key')).toBe(handoverPublishKey);expect(command.pending()).toBeNull();});
 test('acknowledges only a captured unchanged editor generation',async()=>{const command=createHandoverPublishCommands(async()=>Response.json({data:publishedHandover}),()=>handoverPublishKey),attempt=command.prepare('/handover',handoverPublishInput,'context',2);expect(await command.execute(attempt,()=> 'context',()=>2)).toEqual({receipt:publishedHandover,unchanged:true});});
 test('never sends under another role and retains a successful-looking late response after context loss',async()=>{let calls=0,context='original';const command=createHandoverPublishCommands(async()=>{calls++;context='changed';return Response.json({data:publishedHandover});},()=>handoverPublishKey),attempt=command.prepare('/handover',handoverPublishInput,context,1);await expect(command.execute(attempt,()=> 'other',()=>1)).rejects.toThrow('original account');expect(calls).toBe(0);expect(command.pending()).toBeNull();await expect(command.execute(attempt,()=>context,()=>1)).rejects.toThrow('account or role changed');expect(command.pending()).toBe(attempt);});
 test('a first actual denial clears no unknown result while later denial preserves original recovery',async()=>{let count=0;const command=createHandoverPublishCommands(async()=>Response.json({error:{message:'Current source denied'}},{status:++count===1?503:403}),()=>handoverPublishKey),attempt=command.prepare('/handover',handoverPublishInput,'context',0);await expect(command.execute(attempt,()=> 'context',()=>0)).rejects.toThrow('Current source denied');await expect(command.execute(attempt,()=> 'context',()=>0)).rejects.toThrow('Current source denied');expect(command.pending()).toBe(attempt);const denied=createHandoverPublishCommands(async()=>Response.json({error:{message:'No current source'}},{status:403}),()=>handoverPublishKey);await expect(denied.execute(denied.prepare('/handover',handoverPublishInput,'context',0),()=> 'context',()=>0)).rejects.toThrow('No current source');expect(denied.pending()).toBeNull();});
 test('restores only the same original context endpoint and validated immutable submitted audience and source versions',async()=>{const original=createHandoverPublishCommands(async()=>{throw Error('unknown');},()=>handoverPublishKey),attempt=original.prepare('/handover',handoverPublishInput,'context',3);await expect(original.execute(attempt,()=> 'context',()=>3)).rejects.toThrow('unknown');const stored=JSON.parse(JSON.stringify(attempt)),command=createHandoverPublishCommands();expect(command.restore(stored,'/other','context')).toBeNull();expect(command.restore(stored,'/handover','other-role')).toBeNull();expect(command.restore({...stored,body:JSON.stringify({...handoverPublishInput,recipientIds:[]})},'/handover','context')).toBeNull();expect(command.restore({...stored,key:'not-native-uuid'},'/handover','context')).toBeNull();expect(command.restore(stored,'/handover','context')).toEqual({attempt,input:handoverPublishInput});expect(command.pending()).toEqual(attempt);});
 test('rejects a partial source snapshot rather than replacing the current draft or licensing missing work',()=>{const view:HandoverView={date:'2026-10-16',ownerKey:'same-owner',items:[{id:'task',title:'Task',status:'ready',href:'/staff-tasks/task',updatedAt:1,kind:'pending',sourceVersion:3,planned:false,sourceKind:'staff_task'}],defaultIds:['task'],people:[{id:'receiver',name:'Receiver'}],unavailable:false,reason:null};expect(handoverViewMatches(view)).toBe(true);expect(handoverViewMatches({...view,defaultIds:['missing']})).toBe(false);expect(handoverViewMatches({...view,items:[{...view.items[0],sourceVersion:'3'}]})).toBe(false);expect(handoverViewMatches({...view,people:[{id:'receiver'}]})).toBe(false);});
});

import {matchesOperationsReceipt} from '@erpx/feature-operations/lib/operations-work-model';
describe('original Operations acknowledgement identity',()=>{
 test('new policy and exception acknowledgements bind to their original key',()=>{
  expect(matchesOperationsReceipt('{"entity":"policy"}','original',{id:'queue-policy-original',version:1,status:'draft'})).toBe(true);
  expect(matchesOperationsReceipt('{"entity":"exception"}','original',{id:'operations-exception-original',version:1,status:'open'})).toBe(true);
  expect(matchesOperationsReceipt('{"entity":"exception"}','original',{id:'operations-exception-later',version:1,status:'open'})).toBe(false);
 });
 test('late reassignment acknowledgement uses the captured previous status',()=>{
  const body=JSON.stringify({entity:'exception',id:'original',action:'assign',expectedVersion:4});
  const receipt={id:'original',version:5,status:'in_progress'};
  expect(matchesOperationsReceipt(body,'key',receipt,'awaiting_review')).toBe(true);
  expect(matchesOperationsReceipt(body,'key',receipt,'closed')).toBe(false);
  expect(matchesOperationsReceipt(body,'key',receipt)).toBe(false);
  expect(matchesOperationsReceipt(body,'key',{...receipt,id:'another'},'awaiting_review')).toBe(false);
 });
 test('policy review and lifecycle bind their exact version and outcome',()=>{
  for(const [entity,action,status] of [['policy','publish','published'],['policy','retire','retired'],['exception','resolve','awaiting_review'],['exception','approve','resolved']]){
   const body=JSON.stringify({entity,id:'original',action,expectedVersion:2});
   expect(matchesOperationsReceipt(body,'key',{id:'original',version:3,status})).toBe(true);
   expect(matchesOperationsReceipt(body,'key',{id:'original',version:2,status})).toBe(false);
   expect(matchesOperationsReceipt(body,'key',{id:'original',version:3,status:'open'})).toBe(false);
  }
 });
});

import {fieldOnlyPolicy} from '../../../packages/server-operations/src/domain/field-ownership';
test('legacy policy partition excludes only Field-only policies and retains mixed consumers',()=>{
 expect(fieldOnlyPolicy(['field_service_order'])).toBe(true);expect(fieldOnlyPolicy(['field_service_order','field_service_order'])).toBe(true);
 for(const kinds of [[],['support_ticket'],['field_service_order','support_ticket'],['asset','field_service_order'],null,'field_service_order'])expect(fieldOnlyPolicy(kinds)).toBe(false);
});

import {OPERATIONS_STAFF_TASKS_SOURCE_SQL,identitySnapshotSources} from '../../../packages/server-operations/src/adapters/identity-snapshot-sources';
import {collaborationNotificationSnapshotSources} from '../../../packages/server-operations/src/adapters/collaboration-notification-snapshot-sources';
describe('Operations staff_tasks snapshot sources share one canonical projection',()=>{
 test('the canonical projection owns a single fixed read over staff_tasks',()=>{
  expect(OPERATIONS_STAFF_TASKS_SOURCE_SQL.match(/SELECT /g)).toHaveLength(1);
  expect(OPERATIONS_STAFF_TASKS_SOURCE_SQL).toContain('FROM staff_tasks');
  expect(OPERATIONS_STAFF_TASKS_SOURCE_SQL).toContain('"completion_approvers_json"');
 });
 test('both owner facts compose the byte-identical FROM clause',()=>{
  const from=`FROM (${OPERATIONS_STAFF_TASKS_SOURCE_SQL}) t`;
  expect(identitySnapshotSources().operations_staff_tasks_facts_staff_tasks_t_from).toBe(from);
  expect(collaborationNotificationSnapshotSources().operations_handover_facts_staff_tasks_t_from).toBe(from);
 });
});

import {readFileSync,readdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {OPERATIONS_TASK_DELEGATION_SOURCE_SQL,OPERATIONS_TASK_TEMPLATE_SOURCE_SQL,OPERATIONS_TASK_SCHEDULE_SOURCE_SQL,operationsActiveDelegationSql,operationsChildJsonSql,operationsChildAggJsonSql,operationsChildJsonByBoundId} from '../../../packages/server-operations/src/adapters/task-delegation-source';
import {workforceActorScopeSql,OPERATIONS_VIEWER_CLOCK_SQL,operationsActorScopeInput,operationsActorScopeInputExists,operationsPurchasedActorScopeClockExists} from '../../../packages/server-operations/src/adapters/workforce-actor-scope-source';
import {operationsActiveStaffLevels} from '../../../packages/server-operations/src/adapters/staff-facts';
import {operationsOwnerVersionExists} from '../../../packages/server-operations/src/adapters/identity-policy-version-guard';
import {finalRootOwnerProjectionSources as organizationProjectionSources} from '@erpx/server-organization/public/final-root-owner-projection-sources';
import {packageClockSql,purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import {scheduledFeaturePredicate} from '@erpx/server-pms/public/feature-availability';
import {OPERATIONS_TASK_START_APPROVER_SQL,OPERATIONS_TASK_COMPLETION_APPROVER_SQL,approverJsonExistsSql} from '../../../packages/server-operations/src/adapters/task-approver-access-source';
import {handoverTaskAccessSql} from '../../../packages/server-operations/src/adapters/handover-policy';
import {taskParticipantReadPolicy} from '../../../packages/server-operations/src/adapters/project-task-record-policy';
import {projectSourceProbe} from '../../../packages/server-operations/src/adapters/record-purchases';
import {operationsOverdueAlertSql} from '../../../packages/server-operations/src/adapters/executive-task-cohort';
import {workCacheOperationsDeadlines} from '../../../packages/server-operations/src/adapters/work-cache-deadlines';
import {operationsSocialTaskOriginExists} from '../../../packages/server-operations/src/adapters/social-task-origin-exists';
import {operationsReceiptVersionExists} from '../../../packages/server-operations/src/adapters/operations-receipt-version-exists';
import {operationsStaffTaskExists} from '../../../packages/server-operations/src/adapters/operations-staff-task-exists';
import {operationsProjectSourceExists} from '../../../packages/server-operations/src/adapters/project-source-exists';
import {operationsAliasSelectAll} from '../../../packages/server-operations/src/adapters/operations-alias-select-all';
import {finalRootOwnerProjectionSources as collaborationProjectionSources} from '@erpx/server-collaboration/public/final-root-owner-projection-sources';
describe('Operations task delegation source dedupe',()=>{
 const fragment='(SELECT jsonb_agg(to_jsonb(delegation) ORDER BY delegation.id) FROM staff_task_delegations delegation WHERE delegation.task_id=t.id AND delegation.company_id=t.company_id)';
 test('the canonical projection owns a single byte-identical delegation read',()=>{
  expect(OPERATIONS_TASK_DELEGATION_SOURCE_SQL).toBe(fragment);
  expect(OPERATIONS_TASK_DELEGATION_SOURCE_SQL.match(/SELECT /g)).toHaveLength(1);
 });
 test('the remaining TypeScript builders compose the canonical interpolation and the native plan freezes it',()=>{
  for(const name of ['task-attachment-read.ts','task-attachment-purchases.ts']){
   const source=readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
   expect(source).toContain('${OPERATIONS_TASK_DELEGATION_SOURCE_SQL}');
   expect(source).not.toContain(fragment);
  }
  const native=readFileSync(new URL('../../../packages/server-operations/src/adapters/native_operations_read_sql.rs',import.meta.url),'utf8');
  expect(native).toContain(fragment);
 });
});

describe('Operations task image template/schedule source dedupe',()=>{
 const templateFragment='(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=t.template_id AND template.company_id=t.company_id)';
 const scheduleFragment='(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=t.schedule_id AND schedule.company_id=t.company_id)';
 test('the canonical projections own one byte-identical template and schedule read each',()=>{
  expect(OPERATIONS_TASK_TEMPLATE_SOURCE_SQL).toBe(templateFragment);
  expect(OPERATIONS_TASK_SCHEDULE_SOURCE_SQL).toBe(scheduleFragment);
  expect(OPERATIONS_TASK_TEMPLATE_SOURCE_SQL.match(/SELECT /g)).toHaveLength(1);
  expect(OPERATIONS_TASK_SCHEDULE_SOURCE_SQL.match(/SELECT /g)).toHaveLength(1);
 });
 test('the remaining TypeScript builders compose the canonical interpolations and the native plan freezes them',()=>{
  for(const name of ['task-attachment-read.ts','task-attachment-purchases.ts']){
   const source=readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
   expect(source).toContain('${OPERATIONS_TASK_TEMPLATE_SOURCE_SQL}');
   expect(source).toContain('${OPERATIONS_TASK_SCHEDULE_SOURCE_SQL}');
   expect(source).not.toContain(templateFragment);
   expect(source).not.toContain(scheduleFragment);
  }
  const native=readFileSync(new URL('../../../packages/server-operations/src/adapters/native_operations_read_sql.rs',import.meta.url),'utf8');
  expect(native).toContain(templateFragment);
  expect(native).toContain(scheduleFragment);
 });
});

describe('Operations workforce actor scope dedupe',()=>{
 const fragment='SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ';
 test('the canonical clause owns one byte-identical current-workforce scope read',()=>{
  expect(workforceActorScopeSql('${current.where}')).toBe(fragment+'${current.where}');
  expect(workforceActorScopeSql('x').match(/SELECT /g)).toHaveLength(2);
 });
 test('both source readers compose the canonical clause and keep no local literal',()=>{
  for(const name of ['record-purchases.ts','task-attachment-read.ts']){
   const source=readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
   expect(source).toContain('workforceActorScopeSql(');
   expect(source).not.toContain(fragment);
  }
 });
});

describe('Operations task approver access dedupe',()=>{
 const startFragment='EXISTS(SELECT 1 FROM erpx_json_each(t.start_approvers_json) WHERE value=?)';
 const completionFragment='EXISTS(SELECT 1 FROM erpx_json_each(t.completion_approvers_json) WHERE value=?)';
 test('the canonical clauses own one byte-identical approver read each',()=>{
  expect(OPERATIONS_TASK_START_APPROVER_SQL).toBe(startFragment);
  expect(OPERATIONS_TASK_COMPLETION_APPROVER_SQL).toBe(completionFragment);
  expect(OPERATIONS_TASK_START_APPROVER_SQL.match(/SELECT /g)).toHaveLength(1);
  expect(OPERATIONS_TASK_COMPLETION_APPROVER_SQL.match(/SELECT /g)).toHaveLength(1);
 });
 test('both owner readers compose the canonical clauses and keep no local literal',()=>{
  for(const name of ['staff-task-read.ts','executive-task-cohort.ts']){
   const source=readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
   expect(source).toContain('${OPERATIONS_TASK_START_APPROVER_SQL}');
   expect(source).toContain('${OPERATIONS_TASK_COMPLETION_APPROVER_SQL}');
   expect(source).not.toContain(startFragment);
   expect(source).not.toContain(completionFragment);
  }
 });
});

describe('Operations approver JSON exists alias/column family dedupe',()=>{
 const canonical=(alias:string,column:string,value:string)=>'EXISTS(SELECT 1 FROM erpx_json_each('+alias+'.'+column+'_approvers_json) WHERE value='+value+')';
 test('one alias/column-parameterized helper renders every prior site byte-identically',()=>{
  const sites:[string,'start'|'completion',string][]=[['t','start','?'],['t','completion','?'],['ht','start','?'],['t','completion','v.user_id'],['project_task_record','start','v.user_id'],['project_task_record','completion','v.user_id']];
  for(const [alias,column,value] of sites)expect(approverJsonExistsSql(alias,column,value)).toBe(canonical(alias,column,value));
  expect(approverJsonExistsSql('t','start','?')).toBe(OPERATIONS_TASK_START_APPROVER_SQL);
  expect(approverJsonExistsSql('t','completion','?')).toBe(OPERATIONS_TASK_COMPLETION_APPROVER_SQL);
 });
 test('handover and project task policies compose the helper without local literals',()=>{
  const handover=handoverTaskAccessSql('ht','?'),project=taskParticipantReadPolicy('?','?','v');
  expect(handover).toContain(canonical('ht','start','?'));
  expect(handover).toContain(canonical('ht','completion','?'));
  expect(project).toContain(canonical('project_task_record','start','v.user_id'));
  expect(project).toContain(canonical('project_task_record','completion','v.user_id'));
  for(const name of ['handover-policy.ts','project-task-record-policy.ts']){
   const source=readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
   expect(source).toContain('approverJsonExistsSql(');
   expect(source).not.toContain('erpx_json_each(');
  }
 });
});

describe('Operations approval-instance image retirement',()=>{
 test('the dead image renderer is gone and the native plan owns both literals',()=>{
  const delegation=readFileSync(new URL('../../../packages/server-operations/src/adapters/task-delegation-source.ts',import.meta.url),'utf8');
  expect(delegation).not.toContain('operationsApprovalImageSql');
  expect(delegation).not.toContain("(SELECT jsonb_agg(jsonb_build_object('instance'");
  const native=readFileSync(new URL('../../../packages/server-operations/src/adapters/native_operations_read_sql.rs',import.meta.url),'utf8');
  for(const [name,literal] of [
   ['work-task-alert-purchases.ts',"'approval',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(instance)"],
   ['work-task-purchases.ts',"'approvals',(SELECT jsonb_agg(jsonb_build_object('instance',to_jsonb(a)"],
  ] as const){
   const source=readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
   expect(source).not.toContain(literal);
   expect(native).toContain(literal);
  }
 });
});

describe('Operations active-delegation clock window dedupe',()=>{
 const window=(select:string,alias:string,task:string,company:string,clock:string,tail='',sep=' ')=>operationsActiveDelegationSql(select,alias,task,company,clock,tail,sep);
 const execWindow="SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id\n      AND sd.status='active' AND sd.starts_at<=? AND sd.expires_at>?\n      ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1";
 const projectWindow="SELECT sd.delegate_user_id FROM staff_task_delegations sd WHERE sd.task_id=project_task_record.id AND sd.company_id=project_task_record.company_id AND sd.status='active' AND sd.starts_at<=${viewer}.now_ms AND sd.expires_at>${viewer}.now_ms ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1";
 const attachmentWindow="SELECT 1 FROM staff_task_delegations delegation WHERE delegation.task_id=work_task_file_source.id AND delegation.company_id=work_task_file_source.company_id AND delegation.status='active' AND delegation.starts_at<=${viewer}.now_ms AND delegation.expires_at>${viewer}.now_ms AND iam_same_person(delegation.delegate_user_id,${viewer}.user_id,delegation.company_id)";
 const clock="floor(extract(epoch FROM clock_timestamp())*1000)::bigint";
 const staffTaskWindow="SELECT sd.id,sd.delegate_user_id,sd.expires_at,sd.reason,sd.version FROM staff_task_delegations sd WHERE sd.task_id=t.id AND sd.company_id=t.company_id AND sd.status='active' AND sd.starts_at<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND sd.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1";
 test('the canonical window renders all four callers byte-identically',()=>{
  expect(window('sd.delegate_user_id','sd','t.id','t.company_id','?','\n      ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1','\n      ')).toBe(execWindow);
  expect(window('sd.delegate_user_id','sd','project_task_record.id','project_task_record.company_id','${viewer}.now_ms',' ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1')).toBe(projectWindow);
  expect(window('1','delegation','work_task_file_source.id','work_task_file_source.company_id','${viewer}.now_ms',' AND iam_same_person(delegation.delegate_user_id,${viewer}.user_id,delegation.company_id)')).toBe(attachmentWindow);
  expect(window('sd.id,sd.delegate_user_id,sd.expires_at,sd.reason,sd.version','sd','t.id','t.company_id',clock,' ORDER BY sd.created_at DESC,sd.id DESC LIMIT 1')).toBe(staffTaskWindow);
  expect(window('1','delegation','x','y','z').match(/SELECT /g)).toHaveLength(1);
 });
 test('all four readers compose the canonical window and keep no local literal',()=>{
  for(const name of ['executive-task-cohort.ts','project-task-record-policy.ts','task-attachment-purchases.ts','staff-task-read.ts']){
   const source=readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
   expect(source).toContain('operationsActiveDelegationSql(');
   expect(source).not.toContain('FROM staff_task_delegations sd WHERE sd.task_id=');
   expect(source).not.toContain('FROM staff_task_delegations delegation WHERE delegation.task_id=');
  }
 });
});

describe('Operations project-source history probe dedupe',()=>{
 const workWhere='(t.project_id IS NULL)',routineWhere='t.published=1 AND t.creator_user_id=?';
 const workProbe=`SELECT 1 FROM staff_tasks t WHERE t.company_id=? AND t.project_id IS NOT NULL AND ${workWhere} LIMIT 1`;
 const routineProbe=`SELECT 1 FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NOT NULL AND ${routineWhere} LIMIT 1`;
 test('the canonical probe renders both tables byte-identically',()=>{
  expect(projectSourceProbe('staff_tasks',workWhere)).toBe(workProbe);
  expect(projectSourceProbe('routine_templates',routineWhere)).toBe(routineProbe);
  expect(projectSourceProbe('staff_tasks',workWhere).match(/SELECT /g)).toHaveLength(1);
 });
 test('both metric gates compose the canonical probe and keep no local literal',()=>{
  const source=readFileSync(new URL('../../../packages/server-operations/src/adapters/record-purchases.ts',import.meta.url),'utf8');
  expect(source).toContain("projectSourceProbe('staff_tasks',source.where)");
  expect(source).toContain("projectSourceProbe('routine_templates',routineScope.where)");
  expect(source).not.toContain('SELECT 1 FROM staff_tasks t WHERE t.company_id=? AND t.project_id IS NOT NULL AND');
  expect(source).not.toContain('SELECT 1 FROM routine_templates t WHERE t.company_id=? AND t.project_id IS NOT NULL AND');
 });
});

describe('Operations task-image child JSON projection dedupe',()=>{
 const tpl=(outer:string)=>`(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=${outer}.template_id AND template.company_id=${outer}.company_id)`;
 const sch=(outer:string)=>`(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=${outer}.schedule_id AND schedule.company_id=${outer}.company_id)`;
 const del=(alias:string,outer:string)=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM staff_task_delegations ${alias} WHERE ${alias}.task_id=${outer}.id AND ${alias}.company_id=${outer}.company_id)`;
 test('the canonical child projections render every site byte-identically',()=>{
  expect(operationsChildJsonSql('routine_templates','template','template_id','t')).toBe(OPERATIONS_TASK_TEMPLATE_SOURCE_SQL);
  expect(operationsChildJsonSql('routine_schedules','schedule','schedule_id','t')).toBe(OPERATIONS_TASK_SCHEDULE_SOURCE_SQL);
  expect(operationsChildAggJsonSql('staff_task_delegations','delegation','t')).toBe(OPERATIONS_TASK_DELEGATION_SOURCE_SQL);
  expect(operationsChildJsonSql('routine_templates','template','template_id','evaluation_task')).toBe(tpl('evaluation_task'));
  expect(operationsChildJsonSql('routine_schedules','schedule','schedule_id','evaluation_task')).toBe(sch('evaluation_task'));
  expect(operationsChildAggJsonSql('staff_task_delegations','d','evaluation_task')).toBe(del('d','evaluation_task'));
  for(const sql of [operationsChildJsonSql('t','a','b','c'),operationsChildAggJsonSql('t','a','c')])expect(sql.match(/SELECT /g)).toHaveLength(1);
 });
 test('the image readers compose the canonical projections and keep no local literal',()=>{
  const evalSrc=readFileSync(new URL('../../../packages/server-operations/src/adapters/people-evaluation-work-facts.ts',import.meta.url),'utf8');
  expect(evalSrc).toContain("operationsChildJsonSql('routine_templates','template','template_id','evaluation_task')");
  expect(evalSrc).toContain("operationsChildJsonSql('routine_schedules','schedule','schedule_id','evaluation_task')");
  expect(evalSrc).toContain("operationsChildAggJsonSql('staff_task_delegations','d','evaluation_task')");
  expect(evalSrc).not.toContain('(SELECT to_jsonb(template) FROM routine_templates template WHERE template.id=evaluation_task');
  expect(evalSrc).not.toContain('(SELECT to_jsonb(schedule) FROM routine_schedules schedule WHERE schedule.id=evaluation_task');
  expect(evalSrc).not.toContain('(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM staff_task_delegations d WHERE d.task_id=evaluation_task');
  const srcSrc=readFileSync(new URL('../../../packages/server-operations/src/adapters/task-delegation-source.ts',import.meta.url),'utf8');
  expect(srcSrc).toContain("operationsChildJsonSql('routine_templates','template','template_id','t')");
  expect(srcSrc).toContain("operationsChildAggJsonSql('staff_task_delegations','delegation','t')");
 });
});

describe('Operations bound-parameter child JSON and viewer clock dedupe',()=>{
 const child=(table:string,alias:string)=>`(SELECT to_jsonb(${alias}) FROM ${table} ${alias} WHERE ${alias}.id=? AND ${alias}.company_id=?)`;
 test('the canonical bound child projection renders both reference sites byte-identically',()=>{
  expect(operationsChildJsonByBoundId('routine_templates','t')).toBe(child('routine_templates','t'));
  expect(operationsChildJsonByBoundId('routine_schedules','s')).toBe(child('routine_schedules','s'));
  expect(operationsChildJsonByBoundId('t','a').match(/SELECT /g)).toHaveLength(1);
 });
 test('the routine reference image composes the canonical projection and keeps no local literal',()=>{
  const src=readFileSync(new URL('../../../packages/server-operations/src/adapters/work-task-purchases.ts',import.meta.url),'utf8');
  expect(src).toContain("operationsChildJsonByBoundId('routine_templates','t')");
  expect(src).toContain("operationsChildJsonByBoundId('routine_schedules','s')");
  expect(src).not.toContain("(SELECT to_jsonb(t) FROM routine_templates t WHERE t.id=?");
  expect(src).not.toContain("(SELECT to_jsonb(s) FROM routine_schedules s WHERE s.id=?");
 });
 test('the projection and attachment guards compose the canonical viewer clock and keep no local literal',()=>{
  expect(OPERATIONS_VIEWER_CLOCK_SQL).toBe('(SELECT ?::text user_id,?::text assignment_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)');
  const src=readFileSync(new URL('../../../packages/server-operations/src/adapters/record-purchases.ts',import.meta.url),'utf8');
  const attachment=readFileSync(new URL('../../../packages/server-operations/src/adapters/task-attachment-purchases.ts',import.meta.url),'utf8');
  expect(src).toContain("operationsViewerClockHead(viewer,' ')");
  expect(attachment).toContain('operationsViewerClockHead(viewer)');
  expect(src).not.toContain('MATERIALIZED(SELECT ?::text user_id,?::text assignment_id,floor(');
  expect(attachment).not.toContain('MATERIALIZED(SELECT ?::text user_id,?::text assignment_id,floor(');
 });
});

describe('Operations read-tail r38 dedupe',()=>{
 const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
 const clock=`,${packageClockSql} now_ms`;
 const levels=`(${organizationProjectionSources.staff_levels}) staff_levels`;
 test('actor scope input family renders every inline variant byte-identically',()=>{
  expect(operationsActorScopeInput('a.company_id','?::text branch_id','?::text department_id')).toBe('(SELECT a.company_id,?::text branch_id,?::text department_id)');
  expect(operationsActorScopeInput(`${'workspace_records.company_id'} company_id`,'workspace_records.branch_id','workspace_records.department_id',clock)).toBe(`(SELECT workspace_records.company_id company_id,workspace_records.branch_id,workspace_records.department_id,${packageClockSql} now_ms)`);
  expect(operationsActorScopeInput('?::text company_id','?::text branch_id','?::text department_id',clock)).toBe(`(SELECT ?::text company_id,?::text branch_id,?::text department_id,${packageClockSql} now_ms)`);
  for(const sql of [operationsActorScopeInput('a','b','c'),operationsActorScopeInput('a','b','c',clock)])expect(sql.match(/SELECT /g)).toHaveLength(1);
 });
 test('actor scope existence family renders both spacing variants byte-identically',()=>{
  const scope='c.current';
  expect(operationsActorScopeInputExists(scope,'')).toBe(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${scope}`);
  expect(operationsActorScopeInputExists(scope)).toBe(`SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) workforce_actor_scope WHERE ${scope}`);
  expect(operationsActorScopeInputExists('x').match(/SELECT /g)).toHaveLength(2);
 });
 test('the three actor-scope readers compose the canonical clauses and keep no local literal',()=>{
  expect(source('work-routine-presentation.ts')).toContain('operationsActorScopeInputExists(scope.where,\'\')');
  expect(source('task-attachment-purchases.ts')).toContain('operationsActorScopeInputExists(current.where)');
  expect(source('work-task-purchases.ts')).toContain('operationsActorScopeInputExists(');
  const purchases=source('record-purchases.ts');
  expect(purchases).toContain("operationsActorScopeInput('a.company_id','?::text branch_id','?::text department_id')");
  expect(purchases).toContain('operationsActorScopeInput(');
  expect(purchases).not.toContain('(SELECT a.company_id,?::text branch_id,?::text department_id)workforce_actor_scope');
  expect(purchases).not.toContain('(SELECT ?::text company_id,?::text branch_id,?::text department_id,${packageClockSql} now_ms)workforce_actor_scope');
 });
 test('active staff-level family renders both readers byte-identically',()=>{
  expect(operationsActiveStaffLevels('id,rank')).toBe(`SELECT id,rank FROM ${levels} WHERE company_id=? AND active=1`);
  expect(operationsActiveStaffLevels('count(*) count',' AND level_number BETWEEN 1 AND 6')).toBe(`SELECT count(*) count FROM ${levels} WHERE company_id=? AND active=1 AND level_number BETWEEN 1 AND 6`);
  expect(operationsActiveStaffLevels('id,rank').match(/SELECT /g)).toHaveLength(2);
  expect(source('staff-facts.ts')).toContain("operationsActiveStaffLevels('id,rank')");
  expect(source('company-acceptance-facts.ts')).toContain("operationsActiveStaffLevels('count(*) count',' AND level_number BETWEEN 1 AND 6')");
 });
 test('owner-version guard family renders all four sites byte-identically',()=>{
  expect(operationsOwnerVersionExists('operations_queue_policies',' AND status=? ')).toBe('EXISTS(SELECT 1 FROM operations_queue_policies WHERE id=? AND company_id=? AND version=? AND status=? )');
  expect(operationsOwnerVersionExists('staff_tasks',' AND state IN (?,?)','id=? AND version=?')).toBe('EXISTS(SELECT 1 FROM staff_tasks WHERE id=? AND version=? AND state IN (?,?))');
  expect(operationsOwnerVersionExists('t',' AND status=\'active\'')).toBe("EXISTS(SELECT 1 FROM t WHERE id=? AND company_id=? AND version=? AND status='active')");
  expect(operationsOwnerVersionExists('t','',undefined)).toBe('EXISTS(SELECT 1 FROM t WHERE id=? AND company_id=? AND version=?)');
  expect(operationsOwnerVersionExists('t').match(/SELECT /g)).toHaveLength(1);
  for(const name of ['identity-policy-version-guard.ts','attachments-service-facts.ts','staff-policy.ts','staff-tasks.ts'])expect(source(name)).toContain('operationsOwnerVersionExists(');
 });
 test('retirement:ts-statements stays strictly below the r38 BEFORE=125 baseline',()=>{
  const root=resolve(__dirname,'../../../packages/server-operations/src');
  let space=0;
  const walk=(dir:string)=>{
   for(const entry of readdirSync(dir,{withFileTypes:true})){
    const path=resolve(dir,entry.name);
    if(entry.isDirectory())walk(path);
    else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
   }
  };
  walk(root);
  // r39 retired both task-image reads and deduped the purchased actor-scope
  // clock probe; the r38 BEFORE=125 baseline itself is unchanged.
  expect(space).toBeLessThanOrEqual(114);
  expect(space).toBeLessThan(125);
 });
});

describe('Operations read-tail r39 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  test('purchased actor-scope clock probe renders both readers byte-identically',()=>{
    const where='<WHERE>';
    const purchased=` AND ${purchasedCapabilityPredicate('workforce_actor_scope.company_id','work.actions','history','workforce_actor_scope.now_ms')}`;
    const feature=` AND ${scheduledFeaturePredicate('workforce_actor_scope.company_id','operations')}`;
    expect(operationsPurchasedActorScopeClockExists('workspace_records.company_id company_id','workspace_records.branch_id','workspace_records.department_id',where)).toBe(`EXISTS(SELECT 1 FROM(SELECT workspace_records.company_id company_id,workspace_records.branch_id,workspace_records.department_id,${packageClockSql} now_ms)workforce_actor_scope WHERE ${where}${purchased}${feature})`);
    expect(operationsPurchasedActorScopeClockExists('?::text company_id','?::text branch_id','?::text department_id',where)).toBe(`EXISTS(SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id,${packageClockSql} now_ms)workforce_actor_scope WHERE ${where}${purchased}${feature})`);
    const rendered=operationsPurchasedActorScopeClockExists('a','b','c',where);
    expect(rendered).toContain('EXISTS(SELECT 1 FROM(SELECT');
    expect(rendered.match(/EXISTS\(SELECT 1 FROM\(SELECT/g)).toHaveLength(1);
  });
  test('the two record-workspace guards compose the canonical probe and keep no local literal',()=>{
    const records=source('record-purchases.ts');
    expect(records).toContain('operationsPurchasedActorScopeClockExists(`${company} company_id`,');
    expect(records).toContain("operationsPurchasedActorScopeClockExists('?::text company_id'");
    expect(records).not.toContain('EXISTS(SELECT 1 FROM${operationsActorScopeInput(');
    expect(records).not.toContain('${packageClockSql} now_ms');
  });
  test('task image reads are owner-planned and the locks remain',()=>{
    const completed=source('work-task-purchases.ts');
    expect(completed).toContain('OPERATIONS_RD_WORK_TASK_IMAGE');
    expect(completed).not.toContain('taskImageSql');
    expect(completed).toContain('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE');
    const queued=source('work-task-alert-purchases.ts');
    expect(queued).toContain('OPERATIONS_RD_WORK_TASK_ALERT_IMAGE');
    expect(queued).not.toContain('imageSql');
    expect(queued).toContain('LOCK TABLE staff_tasks,staff_task_delegations,routine_templates,routine_schedules IN SHARE ROW EXCLUSIVE MODE');
  });
  test('retirement:ts-statements stays strictly below the r39 BEFORE=117 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
     for(const entry of readdirSync(dir,{withFileTypes:true})){
      const path=resolve(dir,entry.name);
      if(entry.isDirectory())walk(path);
      else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
     }
    };
    walk(root);
    // r40 deduped the overdue-alert and count-projection families; the r39
    // BEFORE=117 baseline itself is unchanged.
    expect(space).toBeLessThanOrEqual(111);
    expect(space).toBeLessThan(117);
  });
});

describe('Operations read-tail r40 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  const sn=collaborationProjectionSources.staff_notifications;
  test('overdue-alert family renders both readers byte-identically',()=>{
    expect(`(${operationsOverdueAlertSql('min(n.created_at)','t')})`).toBe(`(SELECT min(n.created_at) FROM (${sn}) n WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert' AND n.record_id=t.id AND n.record_version=t.version AND n.id LIKE 'staff-task-alert-overdue-%')`);
    expect(`EXISTS(${operationsOverdueAlertSql('1','t','\n        ','\n          ')})`).toBe(`EXISTS(SELECT 1 FROM (${sn}) n\n        WHERE n.company_id=t.company_id AND n.user_id=? AND n.record_type='staff_task_alert'\n          AND n.record_id=t.id AND n.record_version=t.version\n          AND n.id LIKE 'staff-task-alert-overdue-%')`);
    // one literal SELECT in the helper plus one inside the embedded projection source
    expect(operationsOverdueAlertSql('1','t').match(/SELECT /g)).toHaveLength(2);
  });
  test('both readers compose the canonical alert and keep no local literal',()=>{
    const evidence=source('executive-main-evidence-sources.ts');
    expect(evidence).toContain("operationsOverdueAlertSql('min(n.created_at)', 't')");
    expect(evidence).not.toContain('(SELECT min(n.created_at) FROM (');
    const cohort=source('executive-task-cohort.ts');
    expect(cohort).toContain("operationsOverdueAlertSql('1', alias, '\\n        ', '\\n          ')");
    expect(cohort).not.toContain('EXISTS(SELECT 1 FROM (${collaborationProjectionSources.staff_notifications}) n');
  });
  test('the three count projections compose one canonical builder',()=>{
    const metric=source('executive-metric-projections.ts');
    expect(metric).toContain("countProjectionSql('t.state', 'staff_tasks',");
    expect(metric).toContain("countProjectionSql(label, 'staff_tasks',");
    expect(metric).toContain("countProjectionSql(\"'Published'\", 'routine_templates',");
    expect(metric).not.toContain('SELECT t.state label,count(*)::text value FROM staff_tasks t WHERE');
    expect(metric).not.toContain("SELECT 'Published' label,count(*)::text value FROM routine_templates t WHERE");
  });
  test('retirement:ts-statements stays strictly below the r40 BEFORE=114 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
      }
    };
    walk(root);
    // r41 deduped the active-delegation deadline UNION family; the r40
    // BEFORE=114 baseline itself is unchanged.
    expect(space).toBeLessThanOrEqual(110);
    expect(space).toBeLessThan(114);
  });
});

describe('Operations read-tail r41 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  const branch=(column:string)=>`SELECT ${column} FROM staff_task_delegations WHERE status='active'`;
  test('the two cache-fence deadline branches render byte-identically',()=>{
    expect(workCacheOperationsDeadlines()).toBe(`${branch('starts_at')} UNION ALL ${branch('expires_at')}`);
    expect(workCacheOperationsDeadlines()).toBe("SELECT starts_at FROM staff_task_delegations WHERE status='active' UNION ALL SELECT expires_at FROM staff_task_delegations WHERE status='active'");
  });
  test('the deadline family keeps one canonical literal and no local SELECT',()=>{
    const src=source('work-cache-deadlines.ts');
    expect(src).toContain("activeDelegationDeadline('starts_at')");
    expect(src).toContain("activeDelegationDeadline('expires_at')");
    expect(src).not.toContain('SELECT starts_at FROM staff_task_delegations');
    expect(src.match(/SELECT /g)).toHaveLength(1);
  });
  test('LOCK TABLE and the r29-r40 helpers remain untouched',()=>{
    expect(source('work-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('project-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('task-delegation-source.ts')).toContain('operationsActiveDelegationSql');
    expect(source('executive-task-cohort.ts')).toContain('operationsOverdueAlertSql');
  });
  test('retirement:ts-statements stays strictly below the r41 BEFORE=111 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
      }
    };
    walk(root);
    // r42 deduped the social task-origin EXISTS family and r43 deduped the
    // command-receipt version guard; the r41 BEFORE=110 baseline is unchanged.
    expect(space).toBe(66);
    expect(space).toBeLessThan(110);
  });
});

describe('Operations read-tail r42 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  const probe=(alias:string,task:string)=>`EXISTS(SELECT 1 FROM (${collaborationProjectionSources.social_task_origins}) ${alias} WHERE ${alias}.task_id=${task})`;
  test('the two social task-origin probes render byte-identically',()=>{
    expect(operationsSocialTaskOriginExists('ox','t.id')).toBe(probe('ox','t.id'));
    expect(operationsSocialTaskOriginExists('ho','ht.id')).toBe(probe('ho','ht.id'));
    expect(operationsSocialTaskOriginExists('ox','t.id')).toBe("EXISTS(SELECT 1 FROM (" + collaborationProjectionSources.social_task_origins + ") ox WHERE ox.task_id=t.id)");
  });
  test('the family keeps one canonical literal and no local SELECT at its call sites',()=>{
    const cohort=source('executive-task-cohort.ts'), handover=source('handover-policy.ts');
    expect(cohort).toContain("operationsSocialTaskOriginExists('ox','t.id')");
    expect(handover).toContain("operationsSocialTaskOriginExists('ho','ht.id')");
    expect(cohort).not.toContain('social_task_origins}) ox WHERE');
    expect(handover).not.toContain('social_task_origins}) ho WHERE');
    expect(source('social-task-origin-exists.ts').match(/SELECT /g) ?? []).toHaveLength(0);
    expect(source('social-task-origin-exists.ts')).toContain('operationsProjectSourceExists(');
  });
  test('LOCK TABLE and the r29-r41 helpers remain untouched',()=>{
    expect(source('work-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('project-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('task-delegation-source.ts')).toContain('operationsActiveDelegationSql');
    expect(source('executive-task-cohort.ts')).toContain('operationsOverdueAlertSql');
    expect(source('work-cache-deadlines.ts')).toContain('activeDelegationDeadline');
  });
  test('retirement:ts-statements stays strictly below the r42 BEFORE=110 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
      }
    };
    walk(root);
    expect(space).toBe(66);
    expect(space).toBeLessThan(110);
  });
});

describe('Operations read-tail r43 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  const guard=(prefix:string,company:string,idClause:string,state:string,version:string)=>`EXISTS(SELECT 1 FROM command_receipts receipt WHERE ${prefix} AND receipt.authority_json::jsonb->>'companyId'=${company}${idClause} AND receipt.result_json::jsonb->>'state'=${state} AND (receipt.result_json::jsonb->>'version')::bigint<=${version})`;
  const taskPrefix="receipt.record_id=t.id AND receipt.operation IN ('task.complete','task.approve','task.cancel')";
  const schedulePrefix="receipt.operation='routine.schedule.end' AND receipt.record_id=s.id";
  const taskIdClause=" AND receipt.result_json::jsonb->>'id'=t.id";
  test('the task and schedule receipt guards render byte-identically',()=>{
    expect(operationsReceiptVersionExists(taskPrefix,'t.company_id',taskIdClause,'t.state','t.version')).toBe(guard(taskPrefix,'t.company_id',taskIdClause,'t.state','t.version'));
    expect(operationsReceiptVersionExists(schedulePrefix,'s.company_id','',"'ended'",'s.version')).toBe(guard(schedulePrefix,'s.company_id','',"'ended'",'s.version'));
    expect(operationsReceiptVersionExists(taskPrefix,'t.company_id',taskIdClause,'t.state','t.version')).toBe("EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=t.id AND receipt.operation IN ('task.complete','task.approve','task.cancel') AND receipt.authority_json::jsonb->>'companyId'=t.company_id AND receipt.result_json::jsonb->>'id'=t.id AND receipt.result_json::jsonb->>'state'=t.state AND (receipt.result_json::jsonb->>'version')::bigint<=t.version)");
    expect(operationsReceiptVersionExists(schedulePrefix,'s.company_id','',"'ended'",'s.version')).toBe("EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.operation='routine.schedule.end' AND receipt.record_id=s.id AND receipt.authority_json::jsonb->>'companyId'=s.company_id AND receipt.result_json::jsonb->>'state'='ended' AND (receipt.result_json::jsonb->>'version')::bigint<=s.version)");
  });
  test('the family keeps one canonical literal and no local SELECT at its call sites',()=>{
    const obligations=source('work-package-obligations.ts');
    expect(obligations).toContain(`operationsReceiptVersionExists("${taskPrefix}"`);
    expect(obligations).toContain(`operationsReceiptVersionExists("${schedulePrefix}"`);
    expect(obligations.match(/operationsReceiptVersionExists\(/g)).toHaveLength(2);
    expect(obligations).not.toContain("SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=t.id");
    expect(obligations).not.toContain("SELECT 1 FROM command_receipts receipt WHERE receipt.operation='routine.schedule.end'");
    expect(source('operations-receipt-version-exists.ts').match(/SELECT /g)).toHaveLength(1);
  });
  test('LOCK TABLE and the r29-r42 helpers remain untouched',()=>{
    expect(source('work-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('project-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('work-package-obligations.ts')).toContain("LOCK TABLE command_receipts IN SHARE MODE");
    expect(source('task-delegation-source.ts')).toContain('operationsActiveDelegationSql');
    expect(source('social-task-origin-exists.ts')).toContain('operationsSocialTaskOriginExists');
    expect(source('executive-task-cohort.ts')).toContain('operationsOverdueAlertSql');
  });
  test('retirement:ts-statements stays strictly below the r43 BEFORE=109 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
      }
    };
    walk(root);
    expect(space).toBe(66);
    expect(space).toBeLessThan(109);
  });
});

describe('Operations read-tail r44 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  const exists=(alias:string,id:string,tail:string)=>`EXISTS(SELECT 1 FROM staff_tasks ${alias} WHERE ${alias}.id=${id} AND ${tail})`;
  test('the project and non-project workspace guards render byte-identically',()=>{
    expect(operationsStaffTaskExists('work_record_project','workspace_records.id','work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL')).toBe(exists('work_record_project','workspace_records.id','work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL'));
    expect(operationsStaffTaskExists('work_record_task','e.source_id','work_record_task.project_id IS NULL')).toBe("EXISTS(SELECT 1 FROM staff_tasks work_record_task WHERE work_record_task.id=e.source_id AND work_record_task.project_id IS NULL)");
    expect(operationsStaffTaskExists('work_record_project','workspace_records.id','work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL')).toBe("EXISTS(SELECT 1 FROM staff_tasks work_record_project WHERE work_record_project.id=workspace_records.id AND work_record_project.company_id=workspace_records.company_id AND work_record_project.project_id IS NOT NULL)");
    expect(operationsStaffTaskExists('a','b','c').match(/SELECT /g)).toHaveLength(1);
  });
  test('the family keeps one canonical literal and no local SELECT at its call sites',()=>{
    const records=source('record-purchases.ts');
    expect(records).toContain("operationsStaffTaskExists('work_record_project',id,");
    expect(records).toContain("operationsStaffTaskExists('work_record_task',id,'work_record_task.project_id IS NULL')");
    expect(records.match(/operationsStaffTaskExists\(/g)).toHaveLength(2);
    expect(records).not.toContain('EXISTS(SELECT 1 FROM staff_tasks work_record_project WHERE');
    expect(records).not.toContain('EXISTS(SELECT 1 FROM staff_tasks work_record_task WHERE');
    expect(source('operations-staff-task-exists.ts').match(/SELECT /g) ?? []).toHaveLength(0);
    expect(source('read-tail-r53-sources.ts')).toContain('SELECT 1 FROM staff_tasks ${alias} ');
  });
  test('LOCK TABLE and the r29-r43 helpers remain untouched',()=>{
    expect(source('work-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('project-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('work-package-obligations.ts')).toContain('LOCK TABLE command_receipts IN SHARE MODE');
    expect(source('task-delegation-source.ts')).toContain('operationsActiveDelegationSql');
    expect(source('social-task-origin-exists.ts')).toContain('operationsSocialTaskOriginExists');
    expect(source('executive-task-cohort.ts')).toContain('operationsOverdueAlertSql');
    expect(source('operations-receipt-version-exists.ts')).toContain('operationsReceiptVersionExists');
    expect(source('work-cache-deadlines.ts')).toContain('activeDelegationDeadline');
  });
  test('retirement:ts-statements stays strictly below the r44 BEFORE=108 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
      }
    };
    walk(root);
    expect(space).toBe(66);
    expect(space).toBeLessThan(108);
  });
});

describe('Operations read-tail r45 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  const probe=(source:string,alias:string,predicate:string)=>`EXISTS(SELECT 1 FROM (${source}) ${alias} WHERE ${predicate})`;
  const ownerPredicate=`sp.id=t.project_id AND sp.company_id=t.company_id
      AND iam_same_person(sp.owner_user_id,?,t.company_id)`;
  const memberPredicate='pm.project_id=t.project_id AND pm.user_id=?';
  test('the project-owner and project-member probes render byte-identically',()=>{
    expect(operationsProjectSourceExists('§staff_projects','sp',ownerPredicate)).toBe(probe('§staff_projects','sp',ownerPredicate));
    expect(operationsProjectSourceExists('§staff_project_members','pm',memberPredicate)).toBe("EXISTS(SELECT 1 FROM (§staff_project_members) pm WHERE pm.project_id=t.project_id AND pm.user_id=?)");
    expect(operationsProjectSourceExists('§staff_projects','sp',ownerPredicate)).toBe("EXISTS(SELECT 1 FROM (§staff_projects) sp WHERE sp.id=t.project_id AND sp.company_id=t.company_id\n      AND iam_same_person(sp.owner_user_id,?,t.company_id))");
    expect(operationsProjectSourceExists('s','a','p').match(/SELECT /g)).toHaveLength(1);
  });
  test('the family keeps one canonical literal and no local SELECT at its call sites',()=>{
    const cohort=source('executive-task-cohort.ts');
    expect(cohort).toContain("operationsProjectSourceExists(projectsProjectionSources.staff_projects,'sp',");
    expect(cohort).toContain("operationsProjectSourceExists(projectsProjectionSources.staff_project_members,'pm',");
    expect(cohort.match(/operationsProjectSourceExists\(/g)).toHaveLength(2);
    expect(cohort).not.toContain('EXISTS(SELECT 1 FROM (${projectsProjectionSources.staff_projects}) sp WHERE');
    expect(cohort).not.toContain('EXISTS(SELECT 1 FROM (${projectsProjectionSources.staff_project_members}) pm WHERE');
    expect(source('project-source-exists.ts').match(/SELECT /g) ?? []).toHaveLength(0);
    expect(source('read-tail-r55-sources.ts').match(/SELECT 1 FROM /g)).toHaveLength(1);
  });
  test('LOCK TABLE and the r29-r44 helpers remain untouched',()=>{
    expect(source('work-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('project-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('work-package-obligations.ts')).toContain('LOCK TABLE command_receipts IN SHARE MODE');
    expect(source('task-delegation-source.ts')).toContain('operationsActiveDelegationSql');
    expect(source('social-task-origin-exists.ts')).toContain('operationsSocialTaskOriginExists');
    expect(source('executive-task-cohort.ts')).toContain('operationsOverdueAlertSql');
    expect(source('operations-receipt-version-exists.ts')).toContain('operationsReceiptVersionExists');
    expect(source('work-cache-deadlines.ts')).toContain('activeDelegationDeadline');
    expect(source('operations-staff-task-exists.ts')).toContain('operationsStaffTaskExists');
  });
  test('retirement:ts-statements stays strictly below the r45 BEFORE=107 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
      }
    };
    walk(root);
    expect(space).toBe(66);
    expect(space).toBeLessThan(107);
  });
});

describe('Operations read-tail r46 retirement',()=>{
  const source=(name:string)=>readFileSync(new URL(`../../../packages/server-operations/src/adapters/${name}`,import.meta.url),'utf8');
  const head=(alias:string,table:string,tail:string)=>`SELECT ${alias}.* FROM ${table} ${alias} WHERE ${tail}`;
  const opsTail=`\${scope} AND e.source_kind<>'field_service_order' ORDER BY e.updated_at DESC LIMIT 200`;
  const metricTail='${visibility.sql} AND ${cohort.sql} AND ${purchase.where}';
  test('the alias-qualified select-all renders byte-identically at both sites',()=>{
    expect(operationsAliasSelectAll('e','operations_exceptions',opsTail)).toBe(head('e','operations_exceptions',opsTail));
    expect(operationsAliasSelectAll('e','operations_exceptions',opsTail)).toBe("SELECT e.* FROM operations_exceptions e WHERE ${scope} AND e.source_kind<>'field_service_order' ORDER BY e.updated_at DESC LIMIT 200");
    expect(operationsAliasSelectAll('t','staff_tasks',metricTail)).toBe("SELECT t.* FROM staff_tasks t WHERE ${visibility.sql} AND ${cohort.sql} AND ${purchase.where}");
    expect(operationsAliasSelectAll('x','y','z').match(/SELECT /g)).toHaveLength(1);
  });
  test('the family keeps one canonical literal and no local SELECT at its call sites',()=>{
    const ops=source('operations-exceptions.ts'),metric=source('executive-metric-projections.ts');
    expect(ops).toContain("operationsAliasSelectAll('e','operations_exceptions',");
    expect(metric).toContain("operationsAliasSelectAll('t','staff_tasks',");
    expect(ops.match(/operationsAliasSelectAll\(/g)).toHaveLength(1);
    expect(metric.match(/operationsAliasSelectAll\(/g)).toHaveLength(1);
    expect(ops).not.toContain('SELECT e.* FROM operations_exceptions e WHERE ${scope}');
    expect(metric).not.toContain('SELECT t.* FROM staff_tasks t WHERE ${visibility.sql}');
    expect(source('operations-alias-select-all.ts').match(/SELECT /g)).toHaveLength(1);
  });
  test('LOCK TABLE and the r29-r45 helpers remain untouched',()=>{
    expect(source('work-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('project-package-obligation-sources.ts')).toContain('LOCK TABLE');
    expect(source('work-package-obligations.ts')).toContain('LOCK TABLE command_receipts IN SHARE MODE');
    expect(source('social-task-origin-exists.ts')).toContain('operationsSocialTaskOriginExists');
    expect(source('operations-receipt-version-exists.ts')).toContain('operationsReceiptVersionExists');
    expect(source('operations-staff-task-exists.ts')).toContain('operationsStaffTaskExists');
    expect(source('project-source-exists.ts')).toContain('operationsProjectSourceExists');
    expect(source('executive-task-cohort.ts')).toContain('operationsOverdueAlertSql');
    expect(source('work-cache-deadlines.ts')).toContain('activeDelegationDeadline');
  });
  test('retirement:ts-statements stays strictly below the r46 BEFORE=106 baseline',()=>{
    const root=resolve(__dirname,'../../../packages/server-operations/src');
    let space=0;
    const walk=(dir:string)=>{
      for(const entry of readdirSync(dir,{withFileTypes:true})){
        const path=resolve(dir,entry.name);
        if(entry.isDirectory())walk(path);
        else if(entry.name.endsWith('.ts'))space+=(readFileSync(path,'utf8').match(/SELECT /g)??[]).length;
      }
    };
    walk(root);
    expect(space).toBe(66);
    expect(space).toBeLessThan(106);
  });
});
