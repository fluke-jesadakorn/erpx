import { afterEach,beforeAll, expect, it, vi } from "vitest";
import { env, applyMigrations } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import {installStaffLevels,positionSchema,savePosition,decideOrganizationNodeChange} from '@erpx/server-organization/public/organization';
import { openOrganizationDraft, saveOrganizationDraft, getOrganizationDraft, previewOrganizationDraft, submitOrganizationDraft, drainOrganizationDrafts } from "@erpx/server-organization/public/organization-drafts";
import { emptyDraft, draftDefinitionSchema, type DraftItem } from "@erpx/feature-organization/lib/organization-draft";
import { lifecycleInput } from "@erpx/feature-people_payroll/lib/people-lifecycle";
import { saveLifecycle, previewLifecycle,cancelLifecycle } from "@erpx/server-people_payroll/public/people-lifecycle";
import {changeAppointment} from '@erpx/server-organization/public/department-authority';
import {packageGrantStore} from "@erpx/server-commercial/public/package-grants";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({ holdingResources: async () => ({write:db,files:env.H001_FILES}),runtimeEnv:async()=>env }));
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);});
afterEach(()=>vi.unstubAllEnvs());
async function fixture(){
 const f=await expenseFixture(false),hr=await f.member('draft-hr','hr_admin');await installStaffLevels(db,f.owner,key());
 const level=(n:number)=>`org-level-${f.company}-${n}`;
 const ceo=String((await savePosition(db,f.owner,null,positionSchema.parse({title:'CEO',branchId:null,departmentId:null,levelId:level(1),reportsToPositionId:null,isCeo:true,expectedVersion:0,reason:'Prepare organization draft'}),key())).id);
 const make=async(title:string,department=f.department)=>String((await savePosition(db,hr,null,positionSchema.parse({title,branchId:f.branch,departmentId:department,levelId:level(6),reportsToPositionId:ceo,expectedVersion:0,reason:'Prepare organization draft'}),key())).id);
 const position=await make('Source role'),target=await make('Destination role',f.accountsDepartment);
 await db.prepare('UPDATE hr_engagements SET position_id=?,level_id=?,manager_person_id=NULL WHERE person_id=?').bind(ceo,level(1),`person-${f.owner.userId}`).run();
 for(const actor of [f.claimant,f.tie])await db.prepare('UPDATE hr_engagements SET position_id=?,level_id=?,manager_person_id=? WHERE person_id=?').bind(position,level(6),`person-${f.owner.userId}`,`person-${actor.userId}`).run();
 const first=`eng-${f.claimant.userId}`,second=`eng-${f.tie.userId}`;
 async function draft(items:DraftItem[]){const opened=await openOrganizationDraft(db,hr);return (await saveOrganizationDraft(db,hr,opened.draft!.id,opened.draft!.version,{...emptyDraft(),reason:'Arrange company for next quarter',effectiveAt:Date.now()+120000,items})).draft!;}
 return {...f,hr,level,ceo,position,target,first,second,draft};
}
it('persists one private draft, checks ownership and rejects stale saves',async()=>{
 const f=await fixture();const a=await openOrganizationDraft(db,f.hr),b=await openOrganizationDraft(db,f.hr);expect(a.draft!.id).toBe(b.draft!.id);
 const saved=await f.draft([{kind:'person',id:f.first,parked:true}]);expect((await getOrganizationDraft(db,f.hr,saved.id)).draft!.definition.items).toHaveLength(1);
 await expect(getOrganizationDraft(db,f.owner,saved.id)).rejects.toMatchObject({status:404});
 await expect(saveOrganizationDraft(db,f.hr,saved.id,1,saved.definition)).rejects.toMatchObject({code:'DRAFT_VERSION_CHANGED'});
 const preview=await previewOrganizationDraft(db,f.hr,saved.id,saved.version);expect(preview.ready).toBe(false);expect(preview.blockers.join()).toContain('parked');
});
it('People core composes legacy cancellation with the complete Organization draft and rolls late parent authority back atomically',async()=>{
 const f=await fixture();await changeAppointment(db,f.owner,{kind:'hr',departmentId:null,positionId:null,personId:`person-${f.hr.userId}`,expectedVersion:0,reason:'Actual Company HR owns the composed lifecycle handoff'},key());const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-composed-people-provider'}),obligations:async()=>{throw Error('No inactive cutover witness');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:'people',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual People and Organization legacy handoff',evidence:'test:people-organization-owner-composition'}});vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const input=lifecycleInput.parse({kind:'transfer',expectedVersion:0,personId:`person-${f.claimant.userId}`,engagementId:f.first,positionId:f.target,managerPersonId:`person-${f.owner.userId}`,effectiveAt:Date.now()+120000,timezone:'Asia/Bangkok',reason:'Preserved earlier access request'}),legacy=await saveLifecycle(db,f.hr,input,key(),false),unknown=key();await expect(db.transaction(tx=>cancelLifecycle(tx,f.hr,String(legacy.id),1,unknown))).rejects.toMatchObject({code:'PEOPLE_COMPOSITION_REQUIRED'});expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(unknown).first()).toBeNull();
 const draft=await f.draft([{kind:'person',id:f.first,parked:false,positionId:f.target,supersedeLegacyCase:{id:String(legacy.id),expectedVersion:1}}]),preview=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(preview.ready,preview.blockers.join()).toBe(true);const command=key(),fn=`composed_people_${key().replaceAll('-','')}`,prior=await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.hr.assignmentId).first();
 await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.hr.assignmentId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON audit_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);
 try{await expect(submitOrganizationDraft(db,f.hr,draft.id,draft.version,preview.token,command)).rejects.toMatchObject({code:'PEOPLE_ACCESS_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fn} ON audit_events;DROP FUNCTION ${fn}()`);}
 expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.hr.assignmentId).first()).toEqual(prior);expect(await db.prepare('SELECT status,version FROM people_lifecycle_cases WHERE id=?').bind(legacy.id).first()).toEqual({status:'draft',version:1});expect((await getOrganizationDraft(db,f.hr,draft.id)).draft!.state).toBe('draft');expect((await getOrganizationDraft(db,f.hr,draft.id)).requests).toHaveLength(0);for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=? OR (command_id LIKE 'draft-cancel-%' AND ${table==='outbox_events'?'aggregate_id':'record_id'}=?)`).bind(command,legacy.id).first('n')).toBe(0);
 const result=await submitOrganizationDraft(db,f.hr,draft.id,draft.version,preview.token,command);expect(await submitOrganizationDraft(db,f.hr,draft.id,draft.version,preview.token,command)).toEqual(result);expect(await db.prepare('SELECT status FROM people_lifecycle_cases WHERE id=?').bind(legacy.id).first('status')).toBe('cancelled');expect((await getOrganizationDraft(db,f.hr,draft.id)).requests).toHaveLength(1);
});
it('applies a person leaving a moving position atomically; preserves salaries and replays submit',async()=>{
 const f=await fixture();const draft=await f.draft([{kind:'position',id:f.position,parked:false,departmentId:f.accountsDepartment},{kind:'person',id:f.first,parked:false,positionId:f.target}]);
 const preview=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(preview.ready,preview.blockers.join('\n')).toBe(true);expect(preview.affectedPeople).toHaveLength(2);
 const payBefore=(await db.prepare('SELECT * FROM hr_compensation_revisions WHERE engagement_id IN (?,?) ORDER BY id').bind(f.first,f.second).all()).results;
 const k=key();const saved=await submitOrganizationDraft(db,f.hr,draft.id,draft.version,preview.token,k);expect(await submitOrganizationDraft(db,f.hr,draft.id,draft.version,preview.token,k)).toEqual(saved);
 expect((await getOrganizationDraft(db,f.hr,draft.id)).requests).toHaveLength(1);
 expect((await db.prepare('SELECT position_id FROM hr_engagements WHERE id=?').bind(f.first).first())?.position_id).toBe(f.position);
 await drainOrganizationDrafts(db,draft.definition.effectiveAt+1);
 const result=await getOrganizationDraft(db,f.hr,draft.id);expect(result.requests[0],JSON.stringify(result.requests)).toMatchObject({state:'applied'});
 expect(await db.prepare('SELECT position_id,department_id FROM hr_engagements WHERE id=?').bind(f.first).first()).toEqual({position_id:f.target,department_id:f.accountsDepartment});
 expect(await db.prepare('SELECT position_id,department_id FROM hr_engagements WHERE id=?').bind(f.second).first()).toEqual({position_id:f.position,department_id:f.accountsDepartment});
 expect((await db.prepare('SELECT * FROM hr_compensation_revisions WHERE engagement_id IN (?,?) ORDER BY id').bind(f.first,f.second).all()).results).toEqual(payBefore);
});
it('rolls back child requests, receipts, audit and outbox if parent receipt fails',async()=>{
 const f=await fixture(),draft=await f.draft([{kind:'person',id:f.first,parked:false,positionId:f.target}]);const p=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(p.ready,p.blockers.join()).toBe(true);
 const command=key();await db.prepare("CREATE OR REPLACE FUNCTION fail_draft_test() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='organization.draft.submit' THEN RAISE EXCEPTION 'forced final receipt failure'; END IF; RETURN NEW; END $$").run();
 await db.exec('CREATE TRIGGER fail_draft_test BEFORE INSERT ON command_receipts FOR EACH ROW EXECUTE FUNCTION fail_draft_test()');
 try{await expect(submitOrganizationDraft(db,f.hr,draft.id,draft.version,p.token,command)).rejects.toBeTruthy();}finally{await db.exec('DROP TRIGGER fail_draft_test ON command_receipts; DROP FUNCTION fail_draft_test()');}
 expect((await getOrganizationDraft(db,f.hr,draft.id)).draft!.state).toBe('draft');expect((await getOrganizationDraft(db,f.hr,draft.id)).requests).toHaveLength(0);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect((await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first())?.n).toBe(0);
 expect((await db.prepare('SELECT count(*) n FROM organization_draft_reservations WHERE company_id=?').bind(f.company).first())?.n).toBe(0);
 const retried=await submitOrganizationDraft(db,f.hr,draft.id,draft.version,p.token,command);
 expect(await submitOrganizationDraft(db,f.hr,draft.id,draft.version,p.token,command)).toEqual(retried);
 expect((await getOrganizationDraft(db,f.hr,draft.id)).requests).toHaveLength(1);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect((await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first())?.n).toBe(1);
});
it('stops a stale group without partially applying it and releases reservations',async()=>{
 const f=await fixture(),draft=await f.draft([{kind:'position',id:f.position,parked:false,departmentId:f.accountsDepartment}]);const p=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(p.ready,p.blockers.join()).toBe(true);await submitOrganizationDraft(db,f.hr,draft.id,draft.version,p.token,key());
 await db.prepare("UPDATE hr_engagements SET status='ended' WHERE id=?").bind(f.second).run();await drainOrganizationDrafts(db,draft.definition.effectiveAt+1);
 expect((await getOrganizationDraft(db,f.hr,draft.id)).requests[0].state).toBe('needs_review');expect((await db.prepare('SELECT department_id FROM organization_positions WHERE id=?').bind(f.position).first())?.department_id).toBe(f.department);
});
it('blocks individual requests against a confirmed draft and fails closed on removed authority',async()=>{
 const f=await fixture(),draft=await f.draft([{kind:'person',id:f.first,parked:false,positionId:f.target}]);const p=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);await submitOrganizationDraft(db,f.hr,draft.id,draft.version,p.token,key());
 const input=lifecycleInput.parse({kind:'transfer',expectedVersion:0,personId:`person-${f.claimant.userId}`,engagementId:f.first,positionId:f.target,managerPersonId:`person-${f.owner.userId}`,effectiveAt:draft.definition.effectiveAt,timezone:'Asia/Bangkok',reason:'Conflicting individual move'});
 const individual=await previewLifecycle(db,f.hr,input,true);await expect(saveLifecycle(db,f.hr,input,key(),true,individual.token,true)).rejects.toBeTruthy();
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.hr.assignmentId).run();await drainOrganizationDrafts(db,draft.definition.effectiveAt+1);
 expect((await db.prepare('SELECT state FROM organization_draft_requests WHERE draft_id=?').bind(draft.id).first())?.state).toBe('needs_review');
});
it('waits for department approval, maps the replacement department and then applies related placements',async()=>{
 const f=await fixture();const branch=`branch-${key()}`;await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'New branch',?,?,?)").bind(branch,branch,f.company,f.company,branch).run();
 // Existing department holders all require a valid final reporting relationship.
 await db.prepare('UPDATE hr_engagements SET level_id=?,manager_person_id=? WHERE company_id=? AND department_id=? AND position_id IS NULL').bind(f.level(6),`person-${f.owner.userId}`,f.company,f.department).run();
 await db.prepare('UPDATE hr_engagements SET position_id=? WHERE company_id=? AND department_id=? AND position_id IS NULL').bind(f.position,f.company,f.department).run();
 const draft=await f.draft([{kind:'department',id:f.department,parked:false,branchId:branch},{kind:'person',id:f.first,parked:false,positionId:f.target}]);const p=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(p.ready,p.blockers.join()).toBe(true);await submitOrganizationDraft(db,f.hr,draft.id,draft.version,p.token,key());
 await drainOrganizationDrafts(db,draft.definition.effectiveAt+1);const pending=await getOrganizationDraft(db,f.hr,draft.id);expect(pending.requests.find(r=>!r.change_id)?.state).toBe('waiting_dependency');
 const change=pending.requests.find(r=>r.change_id)!;await decideOrganizationNodeChange(db,f.owner,change.change_id!,{expectedVersion:1,decision:'approve',reason:'Independent approval'},key());
 await drainOrganizationDrafts(db,draft.definition.effectiveAt+2);const done=await getOrganizationDraft(db,f.hr,draft.id);expect(done.requests.every(r=>r.state==='applied'),JSON.stringify(done.requests)).toBe(true);
 expect((await db.prepare('SELECT position_id FROM hr_engagements WHERE id=?').bind(f.first).first())?.position_id).toBe(f.target);
});
it('rejects cross-company destinations, duplicate items and changes after preview without writes',async()=>{
 const f=await fixture(),other=await fixture();const draft=await f.draft([{kind:'person',id:f.first,parked:false,positionId:other.target}]);const invalid=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(invalid.ready).toBe(false);
 const valid=(await saveOrganizationDraft(db,f.hr,draft.id,draft.version,{...draft.definition,items:[{kind:'person',id:f.first,parked:false,positionId:f.target}]})).draft!;
 const p=await previewOrganizationDraft(db,f.hr,valid.id,valid.version);expect(p.ready,p.blockers.join()).toBe(true);
 await db.prepare('UPDATE organization_positions SET version=version+1 WHERE id=?').bind(f.target).run();await expect(submitOrganizationDraft(db,f.hr,valid.id,valid.version,p.token,key())).rejects.toMatchObject({status:409});expect((await getOrganizationDraft(db,f.hr,valid.id)).requests).toHaveLength(0);
});
it('a rejected department blocks only its dependent group; independent moves still apply',async()=>{
 const f=await fixture();const branch=`branch-${key()}`;await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Another branch',?,?,?)").bind(branch,branch,f.company,f.company,branch).run();
 const create=async(title:string)=>String((await savePosition(db,f.hr,null,positionSchema.parse({title,branchId:f.branch,departmentId:f.accountsDepartment,levelId:f.level(6),reportsToPositionId:f.ceo,expectedVersion:0,reason:'Independent positions'}),key())).id);
 const independentSource=await create('Independent source'),independentTarget=await create('Independent target');await db.prepare('UPDATE hr_engagements SET position_id=?,department_id=? WHERE id=?').bind(independentSource,f.accountsDepartment,f.second).run();
 await db.prepare('UPDATE hr_engagements SET position_id=?,level_id=?,manager_person_id=? WHERE company_id=? AND department_id=? AND position_id IS NULL').bind(f.position,f.level(6),`person-${f.owner.userId}`,f.company,f.department).run();
 const draft=await f.draft([{kind:'department',id:f.department,parked:false,branchId:branch},{kind:'person',id:f.first,parked:false,positionId:f.target},{kind:'person',id:f.second,parked:false,positionId:independentTarget}]);const p=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(p.ready,p.blockers.join()).toBe(true);await submitOrganizationDraft(db,f.hr,draft.id,draft.version,p.token,key());
 await drainOrganizationDrafts(db,draft.definition.effectiveAt+1);let requests=(await getOrganizationDraft(db,f.hr,draft.id)).requests;
 expect(requests.find(r=>r.item_keys.includes(`person:${f.second}`))?.state).toBe('applied');expect(requests.find(r=>r.item_keys.includes(`person:${f.first}`))?.state).toBe('waiting_dependency');
 await decideOrganizationNodeChange(db,f.owner,requests.find(r=>r.change_id)!.change_id!,{expectedVersion:1,decision:'reject',reason:'Keep department in existing branch'},key());await drainOrganizationDrafts(db,draft.definition.effectiveAt+2);requests=(await getOrganizationDraft(db,f.hr,draft.id)).requests;
 expect(requests.find(r=>r.item_keys.includes(`person:${f.first}`))?.state).toBe('needs_review');expect(requests.find(r=>r.item_keys.includes(`person:${f.second}`))?.state).toBe('applied');
});

it('supports department and Level together while retaining an explicit child destination',async()=>{
 const f=await fixture(),draft=await f.draft([{kind:'position',id:f.position,parked:false,departmentId:f.accountsDepartment,levelId:f.level(5)},{kind:'person',id:f.first,parked:false,positionId:f.target}]);
 const preview=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(preview.ready,preview.blockers.join()).toBe(true);await submitOrganizationDraft(db,f.hr,draft.id,draft.version,preview.token,key());await drainOrganizationDrafts(db,draft.definition.effectiveAt+1);
 expect((await getOrganizationDraft(db,f.hr,draft.id)).requests[0].state).toBe('applied');
 expect((await db.prepare('SELECT level_id FROM hr_engagements WHERE id=?').bind(f.first).first())?.level_id).toBe(f.level(6));expect((await db.prepare('SELECT level_id FROM hr_engagements WHERE id=?').bind(f.second).first())?.level_id).toBe(f.level(5));
});
it('requires explicit rebase after source drift and validates draft bounds',async()=>{
 const f=await fixture(),draft=await f.draft([{kind:'person',id:f.first,parked:false,positionId:f.target}]);
 await db.prepare("UPDATE hr_engagements SET status='suspended' WHERE id=?").bind(f.first).run();let preview=await previewOrganizationDraft(db,f.hr,draft.id,draft.version);expect(preview.staleSources).toEqual([`person:${f.first}`]);expect(preview.ready).toBe(false);
 const rebased=(await saveOrganizationDraft(db,f.hr,draft.id,draft.version,draft.definition,false,true)).draft!;preview=await previewOrganizationDraft(db,f.hr,rebased.id,rebased.version);expect(preview.staleSources).toEqual([]);
 expect(draftDefinitionSchema.safeParse({...draft.definition,items:[...draft.definition.items,...draft.definition.items]}).success).toBe(false);
 expect(draftDefinitionSchema.safeParse({...draft.definition,items:Array.from({length:21},(_,i)=>({kind:'person',id:String(i),parked:true}))}).success).toBe(false);
});
