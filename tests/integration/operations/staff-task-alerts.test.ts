import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import {isValidElement,type ReactNode} from 'react';
import { applyMigrations, env, type TestMigration } from "../../support/runtime-env";
import { escalateStaffTasks } from "@erpx/server-operations/public/staff-task-alerts";
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
let taskPresenterActor:WorkContext|undefined;
let taskPresenterHost='console-dev-erpx.jesadakorn.com';
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:taskPresenterHost})}));
vi.mock("@erpx/server-identity_access/public/authorization",async importOriginal=>({...await importOriginal<typeof import("@erpx/server-identity_access/public/authorization")>(),requireWorkContext:async(slug:string)=>{if(!taskPresenterActor||slug!==taskPresenterActor.holdingSlug)throw Error('NATIVE_SELECTED_TASK_ACTOR_REQUIRED');return taskPresenterActor;}}));

const db = env.H001_DB;
const key = () => crypto.randomUUID();

beforeAll(async () => {
  await applyMigrations(
    db,
    (env as typeof env & { STAFF_TEST_MIGRATIONS: TestMigration[] })
      .STAFF_TEST_MIGRATIONS,
  );
  await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);
});
afterEach(()=>{vi.unstubAllEnvs();taskPresenterActor=undefined;taskPresenterHost='console-dev-erpx.jesadakorn.com';});

beforeEach(async () => {
  await db.prepare("DELETE FROM staff_notifications WHERE record_type='staff_task_alert'").run();
  // Delegation and conversation-origin history are immutable. Keep referenced
  // sources intact; unique Company/task IDs isolate subsequent owner cases.
  await db.prepare("DELETE FROM staff_tasks WHERE id LIKE 'task-%' AND NOT EXISTS(SELECT 1 FROM social_task_origins o WHERE o.task_id=staff_tasks.id)").run();
});

async function createTask(dueAt: number | null, state = "ready") {
  const company = `task-alert-${key()}`;
  const id = `task-${key()}`;
  const createdAt = dueAt ?? Date.now();
  await db
    .prepare(
      `INSERT INTO staff_tasks
        (id,company_id,title,description,creator_user_id,assignee_user_id,due_at,
         start_approvers_json,completion_approvers_json,state,created_at,updated_at)
       VALUES(?,?,?,'',?,?,?,'[]','[]',?,?,?)`,
    )
    .bind(id, company, "Review receiving exception", "user-creator", "user-assignee", dueAt, state, createdAt, createdAt)
    .run();
  return { company, id };
}

it("does not send deadline alerts for an ordinary task without a due date", async () => {
  const now = Date.now();
  const task = await createTask(null);
  expect(await escalateStaffTasks(db, now + 7 * 86_400_000)).toEqual({ dueSoon: 0, overdue: 0 });
  const notifications = await db.prepare("SELECT COUNT(*)::int count FROM staff_notifications WHERE record_id=?")
    .bind(task.id).first<{ count: number }>();
  expect(notifications?.count).toBe(0);
});

it("emits a deduplicated due-soon reminder to the assignee", async () => {
  const now = Date.now();
  const task = await createTask(now + 60 * 60 * 1000);
  expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 1, overdue: 0 });
  expect(await escalateStaffTasks(db, now + 30 * 60 * 1000)).toEqual({ dueSoon: 0, overdue: 0 });
  const rows = await db
    .prepare("SELECT user_id,record_type,message FROM staff_notifications WHERE company_id=? AND record_id=?")
    .bind(task.company, task.id)
    .all();
  expect(rows.results).toEqual([
    {
      user_id: "user-assignee",
      record_type: "staff_task_alert",
      message: "Task due within 24 hours: Review receiving exception.",
    },
  ]);
});

it("holds task alerts until Operations is enabled for the company in production", async () => {
  const now = Date.now(), task = await createTask(now + 60 * 60 * 1000);
  vi.stubEnv("NODE_ENV", "production");
  try {
    expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 0, overdue: 0 });
    await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'operations','pilot',1,'Test scheduled alerts','test',?)")
      .bind(task.company,now).run();
    expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 1, overdue: 0 });
  } finally { vi.unstubAllEnvs(); }
});

it("escalates overdue work to the creator and project owner without flooding retries", async () => {
  const now = Date.now();
  const task = await createTask(now + 60 * 60 * 1000);
  const project = `project-${key()}`;
  await db
    .prepare(
      "INSERT INTO staff_projects(id,company_id,title,owner_user_id,created_at,updated_at) VALUES(?,?,?,'user-owner',?,?)",
    )
    .bind(project, task.company, "Receiving controls", now, now)
    .run();
  await db.prepare("UPDATE staff_tasks SET project_id=? WHERE id=?").bind(project, task.id).run();
  await db.prepare("INSERT INTO staff_task_delegations(id,company_id,task_id,delegator_user_id,delegate_user_id,starts_at,expires_at,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
    .bind(`delegation-${key()}`, task.company, task.id, "user-creator", "user-delegate", now - 1000, now + 24 * 60 * 60 * 1000, "Coverage", now, now).run();
  expect(await escalateStaffTasks(db, now)).toEqual({ dueSoon: 2, overdue: 0 });
  // Once the clock advances, the separate overdue escalation is emitted once.
  expect(await escalateStaffTasks(db, now + 2 * 60 * 60 * 1000)).toEqual({ dueSoon: 0, overdue: 4 });
  expect(await escalateStaffTasks(db, now + 3 * 60 * 60 * 1000)).toEqual({ dueSoon: 0, overdue: 0 });
  const rows = await db
    .prepare("SELECT user_id,message FROM staff_notifications WHERE company_id=? AND record_id=? ORDER BY user_id,message")
    .bind(task.company, task.id)
    .all();
  expect(rows.results).toHaveLength(6);
  expect(rows.results.map((row) => row.user_id)).toEqual([
    "user-assignee",
    "user-assignee",
    "user-creator",
    "user-delegate",
    "user-delegate",
    "user-owner",
  ]);
});


import {expenseFixture} from "../../support/helpers/expense-fixture";
import {saveProject,projectSchema} from "@erpx/server-projects/public/staff-projects";
import {saveTask,taskSchema} from '@erpx/server-operations/public/staff-tasks';
import {packageGrantStore,readCompanyPackageGrants} from "@erpx/server-commercial/public/package-grants";
vi.mock("@erpx/server-runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
async function taskProjectFixture(){const f=await expenseFixture();for(const actor of [f.owner,f.claimant])actor.modules=['projects'];const project=await saveProject(db,f.owner,null,projectSchema.parse({title:'Queued purchased Project owner',memberUserIds:[f.claimant.userId]}),key()),now=Date.now(),task=await saveTask(db,f.owner,null,taskSchema.parse({title:'Actual queued Project delivery',projectId:project.id,assigneeUserId:f.claimant.userId,dueAt:new Date(now+60_000).toISOString()}),key());return {f,project,task,now};}
async function alertProjectGrant(companyId:string,state:'active'|'draining'='active'){
 const prior=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId==='projects');return packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-job-provider'}),obligations:async()=>{throw Error('No synthetic Project withdrawal proof');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId,packageId:'projects',packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Native queued Project task source',evidence:'test:project-task-alerts'}});
}
it('filters unpurchased Project tasks before the 500-row job limit while ordinary Work keeps its original owner',async()=>{
 const {f,project,task,now}=await taskProjectFixture();await db.prepare(`INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,project_id,start_approvers_json,completion_approvers_json,state,created_at,updated_at) SELECT 'task-unlicensed-'||?||'-'||n,company_id,branch_id,department_id,title,description,creator_user_id,'not-current-recipient',?,project_id,'[]','[]','ready',created_at,updated_at FROM staff_tasks CROSS JOIN generate_series(1,501) n WHERE id=?`).bind(key(),now+1000,task.id).run();
 const ordinary=await createTask(now+2000);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});expect(await db.prepare("SELECT 1 FROM staff_notifications WHERE record_type='staff_task_alert' AND record_id=?").bind(ordinary.id).first()).not.toBeNull();expect(await db.prepare("SELECT 1 FROM staff_notifications WHERE record_type='staff_task_alert' AND company_id=? AND record_id<>?").bind(f.company,ordinary.id).first()).toBeNull();
 await alertProjectGrant(f.company);expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});expect(await db.prepare("SELECT 1 FROM staff_notifications WHERE record_type='staff_task_alert' AND record_id=? AND user_id=?").bind(task.id,f.claimant.userId).first()).not.toBeNull();expect(await db.prepare("SELECT COUNT(*)::int count FROM staff_notifications WHERE record_type='staff_task_alert' AND company_id=?").bind(f.company).first()).toEqual({count:1});expect(project.id).toBeTruthy();
});
it('monitors existing draining Project tasks only for currently authorized source recipients without requiring People',async()=>{
 const {f,task,now}=await taskProjectFixture();await alertProjectGrant(f.company);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});await alertProjectGrant(f.company,'draining');await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();expect(await escalateStaffTasks(db,now+120_000)).toEqual({dueSoon:0,overdue:1});expect(await db.prepare("SELECT user_id FROM staff_notifications WHERE record_id=? AND id LIKE 'staff-task-alert-overdue-%'").bind(task.id).all()).toMatchObject({results:[{user_id:f.owner.userId}]});expect(await escalateStaffTasks(db,now+180_000)).toEqual({dueSoon:0,overdue:0});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);
});
it('rolls back queued Project alerts after deferred source or IAM changes and rejects unaudited Commercial faults before retry',async()=>{
 const {f,project,task,now}=await taskProjectFixture();await alertProjectGrant(f.company);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');
 for(const [suffix,mutation,code] of [['source',`UPDATE staff_tasks SET project_id=NULL WHERE id='${task.id}'`,'PROJECT_ALERT_SOURCE_CHANGED'],['authority',`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`,'PROJECT_ALERT_AUTHORITY_CHANGED'],['purchase',`UPDATE company_package_grants SET package_version=999 WHERE company_id='${f.company}' AND package_id='projects'`,'23514']]){
  const name=`project_alert_${suffix}_${key().replaceAll('-','')}`;await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${mutation}; RETURN NEW; END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON staff_notifications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.record_id='${task.id}' AND NEW.user_id='${f.claimant.userId}') EXECUTE FUNCTION ${name}()`).run();try{const delivery=escalateStaffTasks(db,now);await expect(delivery).rejects.toMatchObject({code});if(suffix==='purchase')await expect(delivery).rejects.toThrow('PACKAGE_HISTORY_CURRENT_AUDIT_REQUIRED');}finally{await db.prepare(`DROP TRIGGER ${name} ON staff_notifications`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
  expect(await db.prepare("SELECT project_id FROM staff_tasks WHERE id=?").bind(task.id).first()).toEqual({project_id:project.id});expect(await db.prepare("SELECT revoked_at FROM assignments WHERE id=?").bind(f.claimant.assignmentId).first()).toEqual({revoked_at:null});expect((await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='projects')?.packageVersion).toBe(1);expect(await db.prepare("SELECT 1 FROM staff_notifications WHERE record_type='staff_task_alert' AND record_id=?").bind(task.id).first()).toBeNull();
 }
 expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:0,overdue:0});
});

import {projectTaskProductSourceEntry,projectTaskSourceRecordBackdrop} from '@erpx/server-projects/public/product-entry';
it('opens actual included Project task entry and minimal strict Work backdrop independently of legacy enforcement modes',async()=>{
 const {f,task}=await taskProjectFixture(),version=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first<number>('version');
 const scope={companyId:f.company,branchId:f.claimant.branchId,departmentId:f.claimant.departmentId,userId:f.claimant.userId,assignmentId:f.claimant.assignmentId,assignmentVersion:version!,role:f.claimant.role},target={kind:'task' as const,id:task.id};
 expect(await projectTaskProductSourceEntry(db,scope,target)).toBeNull();
 await alertProjectGrant(f.company);
 for(const mode of ['disabled','shadow','enforced']){
  vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT',mode);
  expect(await projectTaskProductSourceEntry(db,scope,target)).toEqual({capability:'projects.tasks',purchasePurpose:'history'});
  expect(await projectTaskSourceRecordBackdrop(f.claimant,target,{strictProduct:true})).toEqual({kind:'task',title:'Actual queued Project delivery',status:'ready'});
  if(mode!=='enforced')expect(await projectTaskSourceRecordBackdrop(f.claimant,target)).toBeNull();
 }
 expect(await projectTaskProductSourceEntry(db,{...scope,assignmentVersion:version!+1},target)).toBeNull();
 expect(await projectTaskProductSourceEntry(db,{...scope,companyId:'another-company'},target)).toBeNull();
 await alertProjectGrant(f.company,'draining');expect(await projectTaskSourceRecordBackdrop(f.claimant,target,{strictProduct:true})).toMatchObject({kind:'task',status:'ready'});
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(scope.assignmentId).run();
 expect(await projectTaskProductSourceEntry(db,scope,target)).toBeNull();
 await expect(projectTaskSourceRecordBackdrop(f.claimant,target,{strictProduct:true})).rejects.toMatchObject({status:404});
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);
});
it('opens only the actual retained Project task for a current source participant without granting the Work product',async()=>{
 const {f,task}=await taskProjectFixture(),ordinary=await saveTask(db,f.owner,null,taskSchema.parse({title:'Ordinary Work retains its own entry',assigneeUserId:f.claimant.userId}),key()),version=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.claimant.assignmentId).first<number>('version'),scope={companyId:f.company,branchId:f.claimant.branchId,departmentId:f.claimant.departmentId,userId:f.claimant.userId,assignmentId:f.claimant.assignmentId,assignmentVersion:version!,role:f.claimant.role},target={kind:'task' as const,id:task.id};
 vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await projectTaskProductSourceEntry(db,scope,target)).toBeNull();await alertProjectGrant(f.company);expect(await projectTaskProductSourceEntry(db,scope,target)).toEqual({capability:'projects.tasks',purchasePurpose:'history'});expect(await projectTaskProductSourceEntry(db,scope,{kind:'task',id:ordinary.id})).toBeNull();expect(await projectTaskProductSourceEntry(db,scope,{kind:'task',id:'missing-source'})).toBeNull();expect(await projectTaskProductSourceEntry(db,{...scope,companyId:'another-company'},target)).toBeNull();expect(await projectTaskProductSourceEntry(db,{...scope,assignmentVersion:version!+1},target)).toBeNull();
 const unrelatedVersion=await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.tie.assignmentId).first<number>('version');expect(await projectTaskProductSourceEntry(db,{...scope,userId:f.tie.userId,assignmentId:f.tie.assignmentId,assignmentVersion:unrelatedVersion!,role:f.tie.role,branchId:f.tie.branchId,departmentId:f.tie.departmentId},target)).toBeNull();
 const backdrop=await projectTaskSourceRecordBackdrop(f.claimant,target);expect(backdrop).toEqual({kind:'task',title:'Actual queued Project delivery',status:'ready'});expect(Object.keys(backdrop!).sort()).toEqual(['kind','status','title']);await alertProjectGrant(f.company,'draining');expect(await projectTaskProductSourceEntry(db,scope,target)).toEqual({capability:'projects.tasks',purchasePurpose:'history'});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),scope.assignmentId).run();expect(await projectTaskProductSourceEntry(db,scope,target)).toBeNull();await expect(projectTaskSourceRecordBackdrop(f.claimant,target)).rejects.toMatchObject({code:'PROJECT_ACCESS_CHANGED'});expect(await db.prepare('SELECT project_id FROM staff_tasks WHERE id=?').bind(task.id).first()).toHaveProperty('project_id');
});

import {projectTaskCreationWorkspace} from '@erpx/server-projects/public/task-creation';
it('creates a task from a typed actual Project workspace without Work or People purchase and stops new work while draining',async()=>{
 const {f,project}=await taskProjectFixture();vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');await expect(projectTaskCreationWorkspace(f.owner,project.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await alertProjectGrant(f.company);const workspace=await projectTaskCreationWorkspace(f.owner,project.id);expect(workspace.canCreate).toBe(true);expect(workspace.project).toEqual({id:project.id,title:'Queued purchased Project owner',status:'active'});expect(Object.keys(workspace.options).sort()).toEqual(['departments','levels','people']);expect(workspace.options.people.map(row=>row.user_id)).toContain(f.claimant.userId);const worker=workspace.options.people.find(row=>row.user_id===f.claimant.userId);expect(worker).toBeDefined();expect(Object.keys(worker!).sort()).toEqual(['department_id','legal_name','level_id','level_name','user_id']);for(const department of workspace.options.departments)expect(Object.keys(department).sort()).toEqual(['display_name','id']);for(const level of workspace.options.levels)expect(Object.keys(level).sort()).toEqual(['id','name','rank']);await expect(projectTaskCreationWorkspace(f.tie,project.id)).rejects.toMatchObject({code:'PROJECT_NOT_FOUND'});
 const commandKey=key(),input=taskSchema.parse({title:'Task created on its Project native route',projectId:workspace.project.id,assigneeUserId:f.claimant.userId}),created=await saveTask(db,f.owner,null,input,commandKey);expect(await db.prepare('SELECT project_id,company_id FROM staff_tasks WHERE id=?').bind(created.id).first()).toEqual({project_id:project.id,company_id:f.company});await alertProjectGrant(f.company,'draining');const retained=await projectTaskCreationWorkspace(f.owner,project.id);expect(retained.canCreate).toBe(false);expect(retained.options.people).toEqual([]);await expect(saveTask(db,f.owner,null,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});expect(await saveTask(db,f.owner,null,input,commandKey)).toEqual(created);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();await expect(projectTaskCreationWorkspace(f.owner,project.id)).rejects.toMatchObject({code:'PROJECT_ACCESS_CHANGED'});
});

async function alertWorkGrant(companyId:string,state:'active'|'draining'='active',endsAt:number|null=null){
 const prior=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId==='work');return packageGrantStore(db,{authorize:async()=>({userId:'isolated-work-job-provider'}),obligations:async()=>{throw Error('No synthetic Work withdrawal proof');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId,packageId:'work',packageVersion:1,state,startsAt:0,endsAt,version:(prior?.version??0)+1,reason:'Native original ordinary Work task delivery',evidence:'test:work-task-alerts'}});
}
async function ordinaryAlertFixture(){const f=await expenseFixture();for(const actor of [f.owner,f.claimant])actor.modules=[];const now=Date.now(),task=await saveTask(db,f.owner,null,taskSchema.parse({title:'Actual ordinary queued Work delivery',assigneeUserId:f.claimant.userId,dueAt:new Date(now+60_000).toISOString()}),key());return {f,task,now};}
it('purchasedWorkAlert filters actual unpurchased ordinary sources before500 while retaining original modules-empty recipient and draining current authority',async()=>{
 const {f,task,now}=await ordinaryAlertFixture(),missing=await createTask(now+1000);await db.prepare(`INSERT INTO staff_tasks(id,company_id,title,description,creator_user_id,assignee_user_id,due_at,start_approvers_json,completion_approvers_json,state,created_at,updated_at) SELECT 'task-work-missing-'||?||'-'||n,company_id,title,description,creator_user_id,assignee_user_id,due_at,'[]','[]','ready',created_at,updated_at FROM staff_tasks CROSS JOIN generate_series(1,501)n WHERE id=?`).bind(key(),missing.id).run();
 vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:0,overdue:0});await alertWorkGrant(f.company);expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});expect(await db.prepare("SELECT user_id,record_id FROM staff_notifications WHERE record_type='staff_task_alert'").all()).toEqual({success:true,meta:expect.any(Object),results:[{user_id:f.claimant.userId,record_id:task.id}]});
 await alertWorkGrant(f.company,'draining');await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.claimant.assignmentId).run();expect(await escalateStaffTasks(db,now+120_000)).toEqual({dueSoon:0,overdue:1});expect((await db.prepare("SELECT user_id FROM staff_notifications WHERE record_type='staff_task_alert' AND id LIKE 'staff-task-alert-overdue-%'").all()).results).toEqual([{user_id:f.owner.userId}]);expect(await escalateStaffTasks(db,now+120_000)).toEqual({dueSoon:0,overdue:0});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['work']);
});
it('purchasedWorkAlert keeps actual Project Tasks on their purchased Projects foundation without Work or People grants',async()=>{
 const {f,task,now}=await taskProjectFixture();await alertProjectGrant(f.company);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});expect((await db.prepare("SELECT record_id,user_id FROM staff_notifications WHERE record_type='staff_task_alert'").all()).results).toEqual([{record_id:task.id,user_id:f.claimant.userId}]);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['projects']);
});
it('purchasedWorkAlert rechecks exact queued source current recipient after deferred effects and keeps expiry as draining existing delivery',async()=>{
 const {f,task,now}=await ordinaryAlertFixture();await alertWorkGrant(f.company);vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');
 const snapshot=async()=>JSON.stringify(await db.prepare(`SELECT jsonb_build_object('task',(SELECT to_jsonb(t) FROM staff_tasks t WHERE id=?),'assignment',(SELECT to_jsonb(a) FROM assignments a WHERE id=?),'notices',(SELECT jsonb_agg(to_jsonb(n) ORDER BY id) FROM staff_notifications n WHERE record_id=?),'grants',(SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?)) image`).bind(task.id,f.claimant.assignmentId,task.id,f.company).first('image'));
 for(const [suffix,mutation,code] of [['source',`UPDATE staff_tasks SET title='Late unrelated source body' WHERE id='${task.id}'`,'WORK_ALERT_SOURCE_CHANGED'],['authority',`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.claimant.assignmentId}'`,'WORK_ALERT_AUTHORITY_CHANGED']] as const){
  const name=`work_alert_${suffix}_${key().replaceAll('-','')}`;await db.prepare(`CREATE SEQUENCE ${name}_fired`).run();await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}_fired'); ${mutation}; RETURN NEW; END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON staff_notifications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.record_id='${task.id}' AND NEW.user_id='${f.claimant.userId}') EXECUTE FUNCTION ${name}()`).run();const before=await snapshot();try{await expect(escalateStaffTasks(db,now)).rejects.toMatchObject({code});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${name}_fired`).first('is_called')).toBe(true);}finally{await db.prepare(`DROP TRIGGER ${name} ON staff_notifications`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();await db.prepare(`DROP SEQUENCE ${name}_fired`).run();}
 }
 await alertWorkGrant(f.company,'active',Date.now()+5000);const name=`work_alert_expiry_${key().replaceAll('-','')}`;await db.prepare(`CREATE SEQUENCE ${name}_fired`).run();await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}_fired'); PERFORM pg_sleep(5.2); RETURN NEW; END $$`).run();await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON staff_notifications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN (NEW.record_id='${task.id}' AND NEW.user_id='${f.claimant.userId}') EXECUTE FUNCTION ${name}()`).run();const before=await snapshot();try{expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});expect(await snapshot()).not.toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${name}_fired`).first('is_called')).toBe(true);expect((await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='work')!.endsAt).toBeLessThan(Date.now());}finally{await db.prepare(`DROP TRIGGER ${name} ON staff_notifications`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();await db.prepare(`DROP SEQUENCE ${name}_fired`).run();}
 expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:0,overdue:0});expect((await db.prepare("SELECT user_id FROM staff_notifications WHERE record_type='staff_task_alert' AND record_id=?").bind(task.id).all()).results).toEqual([{user_id:f.claimant.userId}]);
});

it('purchasedWorkAlert filters nonparticipant conversation origins before500 and repeats the actual included Project participant after deferred delivery before native retry',async()=>{
 const f=await expenseFixture();for(const actor of [f.owner,f.head])actor.modules=[];await alertWorkGrant(f.company);await alertProjectGrant(f.company);vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const {savePostDraft,publishPost}=await import('@erpx/server-collaboration/public/social-home'),{sourceSnapshot}=await import('@erpx/server-collaboration/public/social-source'),{getTask}=await import('@erpx/server-operations/public/staff-task-read'),{listTasks}=await import('@erpx/server-operations/public/staff-tasks'),now=Date.now(),project=await saveProject(db,f.owner,null,projectSchema.parse({title:'Actual included conversation queued authority',memberUserIds:[f.head.userId]}),key()),draft=await savePostDraft(db,f.owner,null,{body:'Actual Project source queued Task',audience:{kind:'company'},sources:[{kind:'staff_project',id:project.id}],purpose:'update',expectedVersion:1},key()),post=await publishPost(db,f.owner,draft.id,draft.version,key()),snapshot=await sourceSnapshot(db,f.owner,post.id,[]),task=await saveTask(db,f.owner,null,taskSchema.parse({title:'Actual included source deadline',assigneeUserId:f.head.userId,dueAt:new Date(now+60_000).toISOString()}),key(),false,{postId:post.id,sourceVersion:snapshot.post.version,sourceHash:snapshot.hash,commentIds:[],aiDraftId:null}),prefix=key(),unrelated=await saveProject(db,f.owner,null,projectSchema.parse({title:'Actual unrelated legacy conversation sources',memberUserIds:[]}),key());
 await db.prepare(`INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,start_approvers_json,completion_approvers_json,state,created_at,updated_at) SELECT 'task-orphan-social-'||?||'-'||n,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,?,'[]','[]','ready',created_at,updated_at FROM staff_tasks CROSS JOIN generate_series(1,501)n WHERE id=?`).bind(prefix,now+1000,task.id).run();await db.prepare(`INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,department_id,body,state,version,last_command_id,created_at,updated_at,audience_kind,purpose) SELECT 'nonparticipant-post-'||t.id,p.kind,p.author_id,p.author_name,p.company_id,p.branch_id,p.department_id,p.body,p.state,p.version,p.last_command_id,p.created_at,p.updated_at,p.audience_kind,p.purpose FROM staff_tasks t CROSS JOIN social_posts p WHERE t.id LIKE ? AND p.id=?`).bind(`task-orphan-social-${prefix}-%`,post.id).run();await db.prepare(`INSERT INTO social_post_sources(post_id,ordinal,source_kind,source_id) SELECT 'nonparticipant-post-'||id,0,'staff_project',? FROM staff_tasks WHERE id LIKE ?`).bind(unrelated.id,`task-orphan-social-${prefix}-%`).run();await db.prepare(`INSERT INTO social_task_origins(post_id,task_id,source_version,source_hash,created_by,created_at) SELECT 'nonparticipant-post-'||id,id,1,'actual-populated-legacy-origin',creator_user_id,created_at FROM staff_tasks WHERE id LIKE ?`).bind(`task-orphan-social-${prefix}-%`).run();
 const fn=`work_alert_conversation_${key().replaceAll('-','')}`,tables=['staff_tasks','staff_project_members','staff_notifications','command_receipts','audit_events','outbox_events'],read=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await read();await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');DELETE FROM staff_project_members WHERE project_id='${project.id}' AND user_id='${f.head.userId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON staff_notifications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.record_id='${task.id}' AND NEW.user_id='${f.head.userId}') EXECUTE FUNCTION ${fn}()`);
 try{await expect(escalateStaffTasks(db,now)).rejects.toMatchObject({code:'WORK_ALERT_AUTHORITY_CHANGED'});expect(await read()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON staff_notifications;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
 await saveProject(db,f.owner,project.id,projectSchema.parse({title:'Actual included conversation queued authority',memberUserIds:[],expectedVersion:1}),key());await expect(getTask(db,f.head,task.id)).rejects.toMatchObject({code:'WORK_TASK_SOURCE_UNAVAILABLE'});expect((await listTasks(db,f.head)).some(row=>row.id===task.id)).toBe(false);expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:0,overdue:0});await saveProject(db,f.owner,project.id,projectSchema.parse({title:'Actual included conversation queued authority',memberUserIds:[f.head.userId],expectedVersion:2}),key());expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:1,overdue:0});expect((await db.prepare("SELECT record_id,user_id FROM staff_notifications WHERE record_type='staff_task_alert'").all()).results).toEqual([{record_id:task.id,user_id:f.head.userId}]);expect(await escalateStaffTasks(db,now)).toEqual({dueSoon:0,overdue:0});expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId).sort()).toEqual(['projects','work']);
});

function nativeTaskProjectChoices(tree:ReactNode):unknown[]{if(Array.isArray(tree))return tree.flatMap(nativeTaskProjectChoices);if(!isValidElement<{projects?:unknown;children?:ReactNode}>(tree))return [];return [...('projects'in tree.props?[tree.props.projects]:[]),...nativeTaskProjectChoices(tree.props.children)];}
it.each([['disabled','new'],['disabled','detail'],['shadow','new'],['shadow','detail'],['enforced','new'],['enforced','detail']] as const)('taskOptionalProjectPresenter opens the actual %s/%s Task presenter for a current Work-only native actor without a Projects purchase',async(mode,kind)=>{
 const f=await expenseFixture(),task=await saveTask(db,f.claimant,null,taskSchema.parse({title:'Actual Work-only task presenter source',assigneeUserId:f.claimant.userId}),key());await alertWorkGrant(f.company);vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT',mode);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');taskPresenterActor=f.claimant;taskPresenterHost='work-dev-erpx.jesadakorn.com';
 const before=(await db.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results;
 if(kind==='new'){const {default:NewTask}=await import('@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/staff-tasks/new/screen');const page=await NewTask({params:Promise.resolve({holdingSlug:f.claimant.holdingSlug}),searchParams:Promise.resolve({})});expect(nativeTaskProjectChoices(page)).toEqual([[]]);}else{const {default:Task}=await import('@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/staff-tasks/[id]/screen');const page=await Task({params:Promise.resolve({holdingSlug:f.claimant.holdingSlug,id:task.id}),searchParams:Promise.resolve({})});expect(nativeTaskProjectChoices(page).every(choices=>JSON.stringify(choices)==='[]')).toBe(true);}
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(['work']);expect((await db.prepare('SELECT id,role,modules_json,version FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(before);
});

it.each(['disabled','shadow','enforced'] as const)('taskOptionalProjectPicker preserves actual membership, retained controls and Console defaults in %s mode without People purchase',async mode=>{
 const f=await expenseFixture(),project=await saveProject(db,f.owner,null,projectSchema.parse({title:'Actual optional Project member',memberUserIds:[f.claimant.userId]}),key()),task=await saveTask(db,f.claimant,null,taskSchema.parse({title:'Standalone Task with optional retained Project choices',assigneeUserId:f.claimant.userId}),key());await saveProject(db,f.owner,null,projectSchema.parse({title:'Actual unrelated private Project',memberUserIds:[]}),key());await alertWorkGrant(f.company);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT',mode);vi.stubEnv('ERPX_WORK_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');taskPresenterActor=f.claimant;taskPresenterHost='work-dev-erpx.jesadakorn.com';
 const {staffPresentationQueries}=await import('@erpx/server-work_projection/public/presentation-context-queries'),{staffPage}=await import('@erpx/server-work_projection/public/staff-page');
 const optional=()=>staffPresentationQueries(f.claimant.holdingSlug,{projectOptions:'optional'});
 expect((await optional()).projects).toEqual([]);expect((await staffPresentationQueries(f.claimant.holdingSlug,{routineProjects:true})).projects).toEqual([]);
 // The normal Project context remains required. The Console keeps its original
 // disabled/shadow compatibility rather than globally enabling enforcement.
 taskPresenterHost='console-dev-erpx.jesadakorn.com';
 if(mode==='enforced'){await expect(staffPage(f.claimant.holdingSlug)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect((await optional()).projects).toEqual([]);}else{expect((await staffPage(f.claimant.holdingSlug)).projects.map(row=>row.id)).toEqual([project.id]);expect((await optional()).projects.map(row=>row.id)).toEqual([project.id]);}
 taskPresenterHost='work-dev-erpx.jesadakorn.com';await alertProjectGrant(f.company);
 const active=await optional();expect(active.projects.map(row=>row.id)).toEqual([project.id]);expect((await active.queries.projectPurchases())?.['projects.wbs']).toMatchObject({newWork:true,settleExisting:true,history:true});
 await alertProjectGrant(f.company,'draining');const retained=await optional();expect(retained.projects.map(row=>row.id)).toEqual([project.id]);const access=(await retained.queries.projectPurchases())?.['projects.wbs'];expect(access).toMatchObject({newWork:false,settleExisting:true,history:true});expect(Object.keys(access!).sort()).toEqual(['history','newWork','revision','settleExisting']);
 const {default:Task}=await import('@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/staff-tasks/[id]/screen'),page=await Task({params:Promise.resolve({holdingSlug:f.claimant.holdingSlug,id:task.id}),searchParams:Promise.resolve({})});expect(isValidElement<{purchases?:unknown}>(page)&&page.props.purchases).toMatchObject({'projects.tasks':{newWork:false,settleExisting:true,history:true}});expect(nativeTaskProjectChoices(page)).toEqual([[{id:project.id,title:'Actual optional Project member'}]]);
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId).sort()).toEqual(['projects','work']);await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.claimant.assignmentId).run();await expect(optional()).rejects.toMatchObject({code:'PROJECT_ACCESS_CHANGED'});
});

it.each(['release','beta'] as const)('taskOptionalProjectCeiling omits actual purchased Project choices when the original %s ceiling is unavailable',async ceiling=>{
 const f=await expenseFixture(),project=await saveProject(db,f.owner,null,projectSchema.parse({title:'Actual purchased reference still respects release and Beta',memberUserIds:[f.claimant.userId]}),key());await alertWorkGrant(f.company);await alertProjectGrant(f.company);vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');taskPresenterActor=f.claimant;taskPresenterHost='work-dev-erpx.jesadakorn.com';
 if(ceiling==='release'){vi.stubEnv('NODE_ENV','production');expect(await db.prepare("SELECT 1 FROM company_feature_rollouts WHERE company_id=? AND module_id='projects'").bind(f.company).first()).toBeNull();}else{await db.prepare('INSERT INTO beta_policy(company_id,member_limit,branch_limit,ai_monthly_limit,storage_bytes,created_at) VALUES(?,10,2,100,10000000,?)').bind(f.company,Date.now()).run();const {betaAccess}=await import('@erpx/server-identity_access/public/beta-access');taskPresenterActor={...f.claimant,beta:await betaAccess(db,f.company,new Headers())};expect(taskPresenterActor.beta).toBe(true);}
 const {staffPresentationQueries}=await import('@erpx/server-work_projection/public/presentation-context-queries');expect((await staffPresentationQueries(f.claimant.holdingSlug,{projectOptions:'optional'})).projects).toEqual([]);expect((await staffPresentationQueries(f.claimant.holdingSlug,{routineProjects:true})).projects).toEqual([]);
 if(ceiling==='beta'){taskPresenterActor=f.claimant;expect((await staffPresentationQueries(f.claimant.holdingSlug,{projectOptions:'optional'})).projects).toEqual([]);}
 // The compatibility Console remains on the original owner reader, rather
 // than acquiring an unreviewed global release or Beta behavior change.
 taskPresenterHost='console-dev-erpx.jesadakorn.com';expect((await staffPresentationQueries(f.claimant.holdingSlug,{projectOptions:'optional'})).projects.map(row=>row.id)).toEqual([project.id]);expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId).sort()).toEqual(['projects','work']);
});
