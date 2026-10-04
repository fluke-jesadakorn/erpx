import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {env,applyMigrations} from './runtime-env';
import {expenseDb as db,expenseFixture,key} from './helpers/expense-fixture';
import {getProject,projectSchema,saveProject,saveTask,taskSchema,taskActionSchema,transitionTask} from '../src/server/staff-tasks';
import {projectMilestoneSchema,saveProjectMilestone,transitionProjectMilestone} from '../src/server/project-milestones';
import {projectGraph,projectHandoffs,projectHandoffWork,projectStructureCommand} from '../src/server/project-structure';
import {listStaffNotifications} from '../src/server/staff-notifications';
import {createProjectCostAllocation,decideProjectCostAllocation,decideProjectWorkLog,projectCostAllocationSchema,projectCostAllocationDecisionSchema,projectWorkLogSchema,projectWorkLogDecisionSchema,saveProjectWorkLog} from '../src/server/project-actuals';
import {createJournal,createJournalSchema,postJournal} from '../src/server/finance';
vi.mock('../src/server/runtime',()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
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
it('closes hierarchical work packages with real tasks, independently reviewed time and posted costs, and preserves every issued handoff after rejection, acceptance and reopening',async()=>{
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
