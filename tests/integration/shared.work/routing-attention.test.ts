import {beforeAll,beforeEach,expect,it,vi} from 'vitest';
import {applyMigrations,env} from "../../support/runtime-env";
import {expenseDb as db,expenseFixture,key} from "../../support/helpers/expense-fixture";
import {routingAttention} from "../../../src/server/modules/workflow/public/routing-attention";
import {resolveFlow,assignFlowPrimary} from "../../../src/server/modules/workflow/public/flow-routing";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
async function blockedClaim() {
 const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,'submit');await f.action(f.head,id,'approve');return id;
}
it('pages only source-authorized blockers and removes a repaired source without approving it',async()=>{
 await f.grant(f.tie,'finance_preparer');await db.prepare("DELETE FROM flow_defaults WHERE company_id=? AND kind='expense'").bind(f.company).run();
 const ids=[await blockedClaim(),await blockedClaim()].sort();
 const first=await routingAttention(f.owner,{kind:'expense',limit:1});
 expect(first.items.map(i=>i.id)).toEqual([ids[0]]);expect(first.nextAfter).toBe(ids[0]);
 const second=await routingAttention(f.owner,{kind:'expense',limit:1,after:first.nextAfter});
 expect(second.items.map(i=>i.id)).toEqual([ids[1]]);expect(second.nextAfter).toBeNull();
 await expect(routingAttention(f.reviewer,{kind:'expense'})).rejects.toMatchObject({code:'ROUTING_MANAGER_REQUIRED'});
 await expect(routingAttention({...f.owner,branchId:f.branch},{kind:'expense'})).rejects.toMatchObject({code:'ROUTING_MANAGER_REQUIRED'});
 const before=await resolveFlow(db,f.owner,'expense',ids[0]);
 await assignFlowPrimary(db,f.owner,'expense',ids[0],{stepKey:'accounts_review',userId:f.reviewer.userId,reason:'Restore responsibility found by Company routing scan',saveDefault:false,expectedVersion:before.version,recordVersion:before.recordVersion},key());
 expect((await routingAttention(f.owner,{kind:'expense'})).items.map(i=>i.id)).toEqual([ids[1]]);
 expect(await db.prepare('SELECT workflow_stage FROM finance_expenses WHERE id=?').bind(ids[0]).first('workflow_stage')).toBe('accounts_review');
 const after=await resolveFlow(db,f.owner,'expense',ids[0]);expect(after.history.at(-1)?.reason).toContain('Company routing scan');
 await db.prepare("UPDATE assignments SET revoked_at=? WHERE company_id=? AND user_id=? AND role='finance_preparer'").bind(Date.now(),f.company,f.reviewer.userId).run();
 expect((await routingAttention(f.owner,{kind:'expense'})).items.find(i=>i.id===ids[0])?.blocker).toContain('no longer eligible');
});
it('finds an old blocker after more than 250 newer owner-created documents',async()=>{
 await f.grant(f.tie,'finance_preparer');await db.prepare("DELETE FROM flow_defaults WHERE company_id=? AND kind='expense'").bind(f.company).run();
 const old=await blockedClaim();
 // Non-terminal drafts are created through the owner command, not fabricated approval records.
 for(let index=0;index<251;index++)await f.claim();
 const result=await routingAttention(f.owner,{kind:'expense'});
 expect(result.items.map(i=>i.id)).toContain(old);
 expect(result.nextAfter).toBeNull();
},120000);
it('rechecks manager assignment validity and never treats another Company cursor as authority',async()=>{
 expect((await routingAttention(f.owner,{kind:'expense',after:'foreign-document-id'})).items).toEqual([]);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();
 await expect(routingAttention(f.owner,{kind:'expense'})).rejects.toMatchObject({code:'ROUTING_MANAGER_REQUIRED'});
});
