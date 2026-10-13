import {beforeAll,beforeEach,expect,it,vi} from "vitest";
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {createAsset,changeAsset} from "../../../src/server/modules/assets/public/asset-maintenance";
import {draftQueuePolicy,reviewQueuePolicy,operationsWorkspace,operationsExceptionDetail,openOperationsException,changeOperationsException,operationsExceptionWork} from "../../../src/server/modules/operations/public/operations-exceptions";
import {listStaffNotifications} from "../../../src/server/modules/collaboration/public/staff-notifications";
import {readWorkSnapshot} from "../../../src/server/modules/work_projection/public/work-items";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>,assetId:string,policyId:string;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{
 f=await expenseFixture();for(const actor of [f.owner,f.head,f.claimant,f.tie])actor.modules=["operations","assets"];
 assetId=String((await createAsset(f.owner,{branchId:f.branch,departmentId:f.department,code:key(),name:"Packing line safety fixture",category:"Equipment",currency:"THB",costMinor:0,acquiredDate:"2026-09-20"},key())).id);
 policyId=String((await draftQueuePolicy(f.owner,{area:"plant_safety",name:"Plant safety follow-up",sourceKinds:["asset"],targetMinutes:60},key())).id);
 await reviewQueuePolicy(f.head,policyId,{action:"publish",expectedVersion:1,reason:"Independent review of safety queue coverage"},key());
});
const input=()=>({source:{kind:"asset" as const,id:assetId,version:1},policyId,policyVersion:2,title:"Inspect the line guard",issue:"Recorded inspection found a loose cover requiring verified correction.",priority:"high" as const,ownerId:f.claimant.userId});
async function exception(){return String((await openOperationsException(f.owner,input(),key())).id);}
async function action(actor:typeof f.owner,id:string,action:string,extra:Record<string,unknown>={}){const data=await operationsExceptionDetail(actor,id);return changeOperationsException(actor,id,{action,expectedVersion:data.row.version,reason:`Fixture evidence for ${action}`,...extra},key());}
it("runs typed acknowledgement, escalation, independent resolution review and closure with Work and Inbox history",async()=>{
 const id=await exception();expect((await operationsExceptionWork(f.claimant)).find(row=>row.id===id)).toMatchObject({queue:"todo",canAct:true,source:{kind:"asset",id:assetId,version:1}});
 expect((await listStaffNotifications(db,f.claimant)).some(row=>row.record_id===id)).toBe(true);
 await action(f.claimant,id,"acknowledge");await action(f.owner,id,"prioritize",{priority:"critical"});await action(f.claimant,id,"escalate",{escalationOwnerId:f.head.userId});
 expect((await operationsWorkspace(f.head)).summary).toMatchObject({active:1,escalated:1,awaitingReview:0,byArea:{plant_safety:1}});
 expect((await listStaffNotifications(db,f.head)).some(row=>row.record_id===id)).toBe(true);
 await action(f.claimant,id,"start");await action(f.claimant,id,"resolve",{resolution:"Guard fastened and independently inspectable evidence recorded."});
 expect((await operationsExceptionWork(f.owner)).find(row=>row.id===id)).toMatchObject({queue:"todo",responsibleQueue:"Independent Operations reviewer"});
 await action(f.owner,id,"approve");await action(f.claimant,id,"close");
 const detail=await operationsExceptionDetail(f.owner,id);expect(detail.row).toMatchObject({status:"closed",priority:"critical",version:8,resolved_by:f.claimant.userId,reviewed_by:f.owner.userId});
 expect(detail.history.map(event=>event.action)).toEqual(["open","acknowledge","prioritize","escalate","start","resolve","approve","close"]);
 expect((await operationsExceptionWork(f.claimant)).find(row=>row.id===id)).toMatchObject({queue:"completed",canAct:false});
 expect((await operationsWorkspace(f.owner)).summary.active).toBe(0);
 await expect(db.prepare("DELETE FROM operations_exception_events WHERE exception_id=?").bind(id).run()).rejects.toThrow("OPERATIONS_EVIDENCE_IMMUTABLE");
});
it("publishes queue revisions independently and preserves existing policy and source snapshots",async()=>{
 const draft=await draftQueuePolicy(f.owner,{area:"plant_safety",name:"Plant safety revised target",sourceKinds:["asset"],targetMinutes:15},key());
 await expect(reviewQueuePolicy(f.owner,String(draft.id),{action:"publish",expectedVersion:1,reason:"Self review must not activate policy"},key())).rejects.toMatchObject({code:"CONFLICT"});
 const id=await exception(),before=(await operationsExceptionDetail(f.owner,id)).row;
 await reviewQueuePolicy(f.head,String(draft.id),{action:"publish",expectedVersion:1,reason:"Independent revised target review"},key());
 const data=await operationsWorkspace(f.owner);expect(data.policies.find(policy=>policy.id===policyId)).toMatchObject({status:"retired",version:3});
 expect(data.policies.find(policy=>policy.id===draft.id)).toMatchObject({status:"published",revision:2,version:2});
 expect((await operationsExceptionDetail(f.owner,id)).row).toMatchObject({due_at:before.due_at,policy_snapshot_json:before.policy_snapshot_json,source_reference_json:before.source_reference_json});
 await expect(openOperationsException(f.owner,input(),key())).rejects.toMatchObject({code:"OPERATIONS_POLICY_UNAVAILABLE"});
 await expect(db.prepare("UPDATE operations_queue_policies SET target_minutes=10 WHERE id=?").bind(draft.id).run()).rejects.toThrow("OPERATIONS_POLICY_IMMUTABLE");
 await expect(db.prepare("UPDATE operations_exceptions SET source_version=2 WHERE id=?").bind(id).run()).rejects.toThrow("OPERATIONS_SOURCE_IMMUTABLE");
});
it("rejects self review after reassignment and supports correction, reassignment, rework and cancellation",async()=>{
 const id=String((await openOperationsException(f.owner,{...input(),ownerId:f.head.userId},key())).id);
 await action(f.head,id,"acknowledge");await action(f.head,id,"start");await action(f.head,id,"resolve",{resolution:"Manager's proposed correction evidence"});
 await expect(action(f.head,id,"approve")).rejects.toMatchObject({code:"OPERATIONS_INDEPENDENT_REVIEW"});
 await action(f.owner,id,"assign",{ownerId:f.claimant.userId});expect((await operationsExceptionDetail(f.owner,id)).row).toMatchObject({status:"in_progress",resolution:null,resolved_by:null,owner_id:f.claimant.userId});
 await action(f.claimant,id,"resolve",{resolution:"Additional evidence from new responsible owner"});await action(f.head,id,"reject");
 await action(f.claimant,id,"resolve",{resolution:"Corrected evidence ready for independent review"});await action(f.head,id,"approve");await action(f.claimant,id,"close");await action(f.owner,id,"reopen");
 expect((await operationsExceptionDetail(f.owner,id)).row).toMatchObject({status:"in_progress",resolution:null,reviewed_by:null});
 await action(f.owner,id,"cancel");expect((await operationsExceptionDetail(f.owner,id)).row.status).toBe("cancelled");
});
it("commits only one concurrent decision and replays a lost response without duplicating evidence",async()=>{
 const id=await exception(),body={action:"acknowledge",expectedVersion:1,reason:"Acknowledged recorded exception"},command=key();
 const result=await changeOperationsException(f.claimant,id,body,command);expect(await changeOperationsException(f.claimant,id,body,command)).toEqual(result);
 expect((await operationsExceptionDetail(f.owner,id)).history).toHaveLength(2);
 const outcomes=await Promise.allSettled([changeOperationsException(f.claimant,id,{action:"start",expectedVersion:2,reason:"Begin correction once"},key()),changeOperationsException(f.claimant,id,{action:"escalate",expectedVersion:2,reason:"Escalate concurrently",escalationOwnerId:f.head.userId},key())]);
 expect(outcomes.filter(outcome=>outcome.status==="fulfilled")).toHaveLength(1);
 expect((await operationsExceptionDetail(f.owner,id)).history).toHaveLength(3);
 await expect(changeOperationsException(f.claimant,id,{action:"start",expectedVersion:1,reason:"Obsolete decision must not mutate"},key())).rejects.toMatchObject({status:409});
});
it("removes inaccessible source content, counts, Work and Inbox immediately after access withdrawal",async()=>{
 const id=await exception();expect((await operationsWorkspace(f.claimant)).exceptions).toHaveLength(1);
 await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();
 const data=await operationsWorkspace(f.claimant);expect(data.exceptions).toEqual([]);expect(data.summary.active).toBe(0);expect(JSON.stringify(data)).not.toContain("Inspect the line guard");
 expect(await operationsExceptionWork(f.claimant)).toEqual([]);expect((await listStaffNotifications(db,f.claimant)).some(row=>row.record_id===id)).toBe(false);
 await expect(changeOperationsException(f.claimant,id,{action:"acknowledge",expectedVersion:1,reason:"Revoked actor must not act"},key())).rejects.toMatchObject({code:"OPERATIONS_EXCEPTION_NOT_FOUND"});
 for(const actor of [{...f.owner,companyId:"other"},{...f.owner,branchId:"other"},{...f.owner,departmentId:"other"}]){expect((await operationsWorkspace(actor)).exceptions).toEqual([]);await expect(operationsExceptionDetail(actor,id)).rejects.toMatchObject({status:404});}
});
it("requires the exact current source version and preserves a handoff when its source advances",async()=>{
 const id=await exception();await changeAsset(f.owner,assetId,{action:"condition",expectedVersion:1,condition:"degraded",reason:"New inspection changes the owning source version"},key());
 await expect(openOperationsException(f.owner,input(),key())).rejects.toMatchObject({code:"SOURCE_RECORD_CHANGED"});
 const current=await openOperationsException(f.owner,{...input(),source:{kind:"asset",id:assetId,version:2}},key());
 expect((await operationsExceptionDetail(f.owner,String(current.id))).row.source_version).toBe(2);
 expect((await operationsExceptionDetail(f.owner,id)).row).toMatchObject({source_version:1,source:{version:2,status:"registered"}});
});
it("reloads native exception decisions in the complete Work queue without a generic flow and removes its withdrawn source",async()=>{
 const id=await exception(),first=await readWorkSnapshot(f.claimant);
 expect(first.unavailableSources).not.toContain("Operational exceptions");
 expect(first.items.find(row=>row.id===id)).toMatchObject({type:"operational_exception",queue:"todo",canAct:true,nextAction:"Acknowledge the exception",source:{kind:"asset",id:assetId,version:1}});
 expect(first.items.find(row=>row.id===id)?.flow).toBeUndefined();
 await action(f.claimant,id,"acknowledge");await action(f.claimant,id,"start");await action(f.claimant,id,"resolve",{resolution:"Native source correction awaits a separate reviewer"});
 const reviewed=await readWorkSnapshot(f.owner);
 expect(reviewed.unavailableSources).not.toContain("Operational exceptions");
 expect(reviewed.items.find(row=>row.id===id)).toMatchObject({queue:"todo",canAct:true,nextAction:"Review the proposed resolution",status:"awaiting_review"});
 await changeAsset(f.owner,assetId,{action:"condition",expectedVersion:1,condition:"degraded",reason:"Reload must authorize the current owner source revision"},key());
 expect((await readWorkSnapshot(f.claimant)).items.find(row=>row.id===id)?.source).toMatchObject({id:assetId,version:2});
 await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();
 const withdrawn=await readWorkSnapshot(f.claimant);
 expect(withdrawn.unavailableSources).not.toContain("Operational exceptions");expect(withdrawn.items.some(row=>row.id===id)).toBe(false);
 expect(JSON.stringify(withdrawn)).not.toContain("Inspect the line guard");expect(JSON.stringify(withdrawn)).not.toContain(`/operations/${id}`);
});
it("rolls back exception source evidence, notice and command records after a late outbox failure, then retries and replays once",async()=>{
 const command=key(),recordId=`operations-exception-${command}`,fault=`operations_late_${key().replace(/\W/g,"")}`,body=input();
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'OPERATIONS_FINAL_OUTBOX_FAILURE' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try {
  await expect(openOperationsException(f.owner,body,command)).rejects.toMatchObject({code:"23514"});
  for(const [table,field] of [["operations_exceptions","id"],["operations_exception_events","exception_id"],["staff_notifications","record_id"]])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE ${field}=?`).bind(recordId).first("n")).toBe(0);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n")).toBe(0);
  expect((await operationsWorkspace(f.owner)).summary.active).toBe(0);
 } finally {await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const result=await openOperationsException(f.owner,body,command);expect(await openOperationsException(f.owner,body,command)).toEqual(result);expect((await operationsExceptionDetail(f.owner,recordId)).history).toHaveLength(1);
 for(const [table,field] of [["staff_notifications","record_id"],["command_receipts","record_id"],["audit_events","record_id"],["outbox_events","aggregate_id"]])expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE ${field}=?`).bind(recordId).first("n")).toBe(1);
});
