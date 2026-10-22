import {beforeAll,expect,it} from 'vitest';
import {env,applyMigrations} from "../../support/runtime-env";
import {selectWorkRole,readWorkSelection,restoreWorkSelection} from "@erpx/server-work_projection/public/work-role-store";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
const db=env.H001_DB;
beforeAll(()=>applyMigrations(db,env.STAFF_TEST_MIGRATIONS));
async function fixture(){
 const companyId=crypto.randomUUID(),userId=crypto.randomUUID(),sessionId=crypto.randomUUID();
 await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Role test',?)").bind(companyId,companyId,companyId).run();
 const grants=Object.fromEntries(['employee','finance_approver','holding_owner','treasury'].map(role=>[role,crypto.randomUUID()]));
 for(const [role,id] of Object.entries(grants))await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,modules_json,scope_kind,valid_from,valid_until) VALUES(?,?,?,?, '[]','holding',0,9999999999999)").bind(id,userId,role,companyId).run();
 const choose=(role:string,revision:number,session=sessionId)=>selectWorkRole(db,{sessionId:session,userId,companyId,assignmentId:grants[role],assignmentVersion:1,revision,expiresAt:Date.now()+60000});
 const actor=(role:string,revision:number)=>({userId,companyId,assignmentId:grants[role],role,branchId:null,departmentId:null,authority:{sessionId,revision,assignmentVersion:1}});
 const write=(role:string,revision:number,key=crypto.randomUUID())=>executeCommand(db,actor(role,revision),key,'context.test.write',{},key,1,{id:key},[db.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,'Test','2026-01-01','2026-12-31',0)").bind(key,companyId)]);
 return {companyId,userId,sessionId,grants,choose,actor,write};
}
it('requires one explicit assignment and atomically rejects a stale selector',async()=>{
 const f=await fixture();expect(await readWorkSelection(db,f.sessionId,f.userId,f.companyId)).toBeNull();
 const results=await Promise.allSettled([f.choose('employee',0),f.choose('holding_owner',0)]);
 expect(results.filter(r=>r.status==='fulfilled')).toHaveLength(1);
 expect(results.find(r=>r.status==='rejected')).toMatchObject({reason:{code:'WORK_CONTEXT_CHANGED'}});
 expect((await readWorkSelection(db,f.sessionId,f.userId,f.companyId))?.revision).toBe(1);
});
it('rejects stale writes before recording data, receipts or decisions',async()=>{
 const f=await fixture();await f.choose('finance_approver',0);await f.choose('employee',1);
 const key=crypto.randomUUID();await expect(f.write('finance_approver',1,key)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});
 expect(await db.prepare('SELECT 1 FROM finance_periods WHERE id=?').bind(key).first()).toBeNull();
 expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(key).first()).toBeNull();
});
it('shares context within a session while separating devices and companies',async()=>{
 const f=await fixture();await f.choose('employee',0);await f.choose('treasury',0,'other-device');
 expect((await readWorkSelection(db,f.sessionId,f.userId,f.companyId))?.assignment_id).toBe(f.grants.employee);
 expect((await readWorkSelection(db,'other-device',f.userId,f.companyId))?.assignment_id).toBe(f.grants.treasury);
 const other=await fixture();await other.choose('holding_owner',0,f.sessionId);
 expect((await readWorkSelection(db,f.sessionId,f.userId,f.companyId))?.assignment_id).toBe(f.grants.employee);
});
it('preserves the actual authority in audit and prevents cross-role replay',async()=>{
 const f=await fixture();await f.choose('finance_approver',0);const key=crypto.randomUUID();await f.write('finance_approver',1,key);
 expect(await replayCommand(db,f.actor('finance_approver',1),key,'context.test.write',key,{})).toEqual({id:key});
 const audit=await db.prepare('SELECT authority_json FROM audit_events WHERE command_id=?').bind(key).first<{authority_json:string}>();
 expect(JSON.parse(audit!.authority_json)).toMatchObject({role:'finance_approver',assignmentId:f.grants.finance_approver,assignmentVersion:1,revision:1,scopeKind:"holding",scopeNodeId:null});
 await f.choose('employee',1);
 await expect(replayCommand(db,f.actor('employee',2),key,'context.test.write',key,{})).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
 await expect(replayCommand(db,f.actor('finance_approver',1),key,'context.test.write',key,{})).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});
});
it('never writes under an invisible role when a role switch races a command',async()=>{
 const f=await fixture();await f.choose('finance_approver',0);const key=crypto.randomUUID();
 const [write,selection]=await Promise.allSettled([f.write('finance_approver',1,key),f.choose('employee',1)]);
 expect(selection.status).toBe('fulfilled');
 const saved=await db.prepare('SELECT 1 FROM finance_periods WHERE id=?').bind(key).first();
 if(write.status==='fulfilled')expect(saved).not.toBeNull();else{expect(write.reason).toMatchObject({code:'WORK_CONTEXT_CHANGED'});expect(saved).toBeNull();}
 expect((await readWorkSelection(db,f.sessionId,f.userId,f.companyId))?.assignment_id).toBe(f.grants.employee);
});
it('allows recovery after revocation without using the revoked assignment',async()=>{
 const f=await fixture();await f.choose('finance_approver',0);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.grants.finance_approver).run();
 await expect(f.write('finance_approver',1)).rejects.toMatchObject({status:409});
 await expect(f.choose('finance_approver',1)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});
 expect((await f.choose('employee',1)).revision).toBe(2);
});
it('rejects expired, future, changed and foreign assignments',async()=>{
 const f=await fixture(),other=await fixture();
 for(const patch of ['valid_until=1','valid_from=9999999999998','version=2']){
 await db.prepare(`UPDATE assignments SET valid_from=0,valid_until=9999999999999,version=1 WHERE id=?`).bind(f.grants.employee).run();
 await db.prepare(`UPDATE assignments SET ${patch} WHERE id=?`).bind(f.grants.employee).run();
 await expect(f.choose('employee',0)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});
 }
 await expect(selectWorkRole(db,{sessionId:f.sessionId,userId:f.userId,companyId:f.companyId,assignmentId:other.grants.employee,assignmentVersion:1,revision:0,expiresAt:Date.now()+60000})).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});
});

it('separates identically named branch assignments and inherits only into their own departments',async()=>{
 const f=await fixture(),branch=crypto.randomUUID(),otherBranch=crypto.randomUUID(),department=crypto.randomUUID();
 for(const id of [branch,otherBranch])await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Branch',?,?,?)").bind(id,id,f.companyId,f.companyId,id).run();
 await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Department',?,?,?)").bind(department,department,branch,f.companyId,branch).run();
 await db.prepare("UPDATE assignments SET scope_kind='branch',scope_node_id=?,branch_id=? WHERE id=?").bind(branch,branch,f.grants.employee).run();
 const twin=crypto.randomUUID();await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,modules_json,scope_kind,scope_node_id,branch_id,valid_from,valid_until) VALUES(?,?,'employee',?,'[]','branch',?,?,0,9999999999999)").bind(twin,f.userId,f.companyId,otherBranch,otherBranch).run();
 await f.choose('employee',0);
 const childActor={...f.actor('employee',1),branchId:branch,departmentId:department};
 const key=crypto.randomUUID();await executeCommand(db,childActor,key,'context.test.inherit',{},key,1,{id:key},[]);
 await expect(executeCommand(db,{...childActor,branchId:otherBranch,departmentId:null},crypto.randomUUID(),'context.test.crossBranch',{},'none',1,{},[])).rejects.toMatchObject({status:409});
 await selectWorkRole(db,{sessionId:f.sessionId,userId:f.userId,companyId:f.companyId,assignmentId:twin,assignmentVersion:1,revision:1,expiresAt:Date.now()+60000});
 await expect(executeCommand(db,childActor,crypto.randomUUID(),'context.test.oldBranch',{},'none',1,{},[])).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});
});

it('restores a saved role into a new session without changing an existing session',async()=>{
 const f=await fixture();await f.choose('finance_approver',0);
 const assignments=[{id:f.grants.finance_approver,version:1},{id:f.grants.employee,version:1}];
 const input={sessionId:crypto.randomUUID(),userId:f.userId,companyId:f.companyId,expiresAt:Date.now()+60000};
 const restored=await restoreWorkSelection(db,input,assignments);
 expect(restored).toMatchObject({assignment_id:f.grants.finance_approver,revision:1});
 await f.choose('employee',1);
 expect((await restoreWorkSelection(db,input,assignments))?.assignment_id).toBe(f.grants.finance_approver);
 expect((await restoreWorkSelection(db,{...input,sessionId:crypto.randomUUID()},assignments))?.assignment_id).toBe(f.grants.employee);
});
it('never restores an expired, revoked, or changed saved grant',async()=>{
 for(const change of ["revoked_at=1","valid_until=1","version=version+1"]){
 const f=await fixture();await f.choose('finance_approver',0);
 await db.prepare(`UPDATE assignments SET ${change} WHERE id=?`).bind(f.grants.finance_approver).run();
 const result=await restoreWorkSelection(db,{sessionId:crypto.randomUUID(),userId:f.userId,companyId:f.companyId,expiresAt:Date.now()+60000},[{id:f.grants.finance_approver,version:1}]);
 expect(result).toBeNull();
 }
});
it('concurrent restoration initializes one revision and retains an audit trail',async()=>{
 const f=await fixture();await f.choose('employee',0);
 const input={sessionId:crypto.randomUUID(),userId:f.userId,companyId:f.companyId,expiresAt:Date.now()+60000};
 const results=await Promise.all([restoreWorkSelection(db,input,[{id:f.grants.employee,version:1}]),restoreWorkSelection(db,input,[{id:f.grants.employee,version:1}])]);
 expect(results.every(r=>r?.revision===1)).toBe(true);
 const audit=await db.prepare("SELECT purpose FROM audit_events WHERE actor_id=? AND purpose='Restored saved work assignment for a new session'").bind(f.userId).all();
 expect(audit.results).toHaveLength(1);
});
