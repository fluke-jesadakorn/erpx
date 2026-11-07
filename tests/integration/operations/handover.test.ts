import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { branchStatements, memberStatements } from "@erpx/server-manage_erpx/public/beta-onboarding";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { bangkokToday, handoverSuggestion } from "@erpx/feature-shared/lib/handover";
import { acknowledgeHandover, handoverAcknowledgement, handoverView, publishHandover } from "@erpx/server-operations/public/handover";
import {postAction} from '@erpx/server-collaboration/public/social-home';
import {readablePost} from '@erpx/server-collaboration/public/social-source';
import type { WorkSnapshot } from "@erpx/server-work_projection/public/work-items";
import { applyMigrations, env, type TestMigration } from "../../support/runtime-env";

vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, read: env.H001_READ_DB, files: env.H001_FILES }),
}));
const db = env.H001_DB;
const key = () => crypto.randomUUID();
afterEach(()=>vi.unstubAllEnvs());
let company: string, branch: string, owner: WorkContext, receiver: WorkContext, outsider: WorkContext;
let taskId: string, snapshot: WorkSnapshot;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});

beforeEach(async () => {
  company = `co-${key()}`;
  branch = `br-${key()}`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Test company',?)")
      .bind(company, company, company),
    ...branchStatements(db, company, branch, "Branch", branch),
    db.prepare("INSERT INTO staff_levels(id,company_id,code,name,rank,created_at,updated_at) VALUES(?,?,'member','Member',1,1,1)")
      .bind(`level-${company}`, company),
    db.prepare("INSERT INTO beta_policy VALUES(?,20,5,100,1073741824,1)").bind(company),
  ]);
  async function person(name: string, ownerRole: boolean): Promise<WorkContext> {
    const id = key();
    await db.batch(memberStatements(db, company, branch,
      { id, name, email: `${id}@example.test`, emailVerified: true }, ownerRole, null));
    return {
      userId: id, name, email: `${id}@example.test`, assignmentId: `beta-assignment-${id}`,
      role: ownerRole ? "holding_owner" : "employee", companyId: company,
      branchId: ownerRole ? null : branch, departmentId: null,
      holdingId: "h001", holdingSlug: "acme", holdingName: "Test",
      scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "Test", companyId: company },
      assignmentScope: ownerRole ? "holding" : "branch", inherited: false, modules: [], assignments: [], workspaces: [],
    };
  }
  owner = await person("Owner", true);
  receiver = await person("Receiver", false);
  outsider = await person("Unrelated colleague", false);
  taskId = `task-${key()}`;
  const now = Date.now();
  await db.prepare(`INSERT INTO staff_tasks(id,company_id,branch_id,title,creator_user_id,assignee_user_id,
    due_at,state,created_at,updated_at) VALUES(?,?,?,?,?,? ,NULL,'in_progress',?,?)`)
    .bind(taskId, company, branch, "Repair front desk printer", owner.userId, receiver.userId, now, now).run();
  snapshot = { unavailableSources: [], items: [{
    id: taskId, type: "staff_task", title: "Repair front desk printer", detail: "In progress",
    href: `/staff-tasks/${taskId}`, status: "in_progress", updatedAt: now, queue: "todo",
    nextAction: "Check replacement part", blocker: "Waiting for replacement part",
  }] };
});

describe("Today handover", () => {
  it("suggests an editable Gregorian handover from readable Work items without changing the task", async () => {
    const view = await handoverView(db, owner, snapshot);
    expect(view).toMatchObject({ date: bangkokToday(), unavailable: false, defaultIds: [taskId] });
    expect(view.people.map(person => person.id)).toContain(receiver.userId);
    const body = handoverSuggestion(view.date, view.items);
    expect(body).toContain("Needs help");
    expect(body).toContain("Waiting for replacement part");
    expect(body).toMatch(/\b20\d{2}\b/);
    expect(await db.prepare("SELECT state,version FROM staff_tasks WHERE id=?").bind(taskId)
      .first()).toMatchObject({ state: "in_progress", version: 1 });
  });

  it("requires every intended recipient to open each source, publishes once, and keeps task state", async () => {
    const view = await handoverView(db, owner, snapshot);
    const body = handoverSuggestion(view.date, view.items);
    const input = { date: view.date, body, sources: [{ id: taskId, sourceVersion: 1 }], recipientIds: [outsider.userId] };
    await expect(publishHandover(db, owner, snapshot, input, key()))
      .rejects.toMatchObject({ code: "HANDOVER_RECIPIENT_ACCESS", status: 403 });
    expect(await db.prepare("SELECT count(*) n FROM social_post_drafts WHERE company_id=?")
      .bind(company).first<number>("n")).toBe(0);
    input.recipientIds = [receiver.userId];
    const publishKey = key();
    const result = await publishHandover(db, owner, snapshot, input, publishKey);
    expect(await publishHandover(db, owner, snapshot, input, publishKey)).toEqual(result);
    expect((await readablePost(db, receiver, result.id)).purpose).toBe("handover");
    await expect(readablePost(db, outsider, result.id)).rejects.toMatchObject({ code: "POST_NOT_FOUND" });
    expect(await db.prepare("SELECT count(*) n FROM social_posts WHERE id=?").bind(result.id).first<number>("n")).toBe(1);
    expect(await db.prepare("SELECT state,version FROM staff_tasks WHERE id=?").bind(taskId)
      .first()).toMatchObject({ state: "in_progress", version: 1 });
  });

  it("confirms receipt only by an intended recipient and resets confirmation after an edit", async () => {
    const view = await handoverView(db, owner, snapshot);
    const published = await publishHandover(db, owner, snapshot, {
      date: view.date, body: "Printer repair is waiting for parts.",
      sources: [{ id: taskId, sourceVersion: 1 }], recipientIds: [receiver.userId],
    }, key());
    expect((await handoverAcknowledgement(db, receiver, published.id)).acknowledged).toBe(false);
    await expect(acknowledgeHandover(db, owner, published.id, key()))
      .rejects.toMatchObject({ code: "HANDOVER_RECIPIENT_REQUIRED", status: 403 });
    const receiptKey = key();
    expect((await acknowledgeHandover(db, receiver, published.id, receiptKey)).acknowledged).toBe(true);
    expect((await acknowledgeHandover(db, receiver, published.id, receiptKey)).acknowledgedCount).toBe(1);
    expect(await db.prepare("SELECT count(*) n FROM social_handover_acknowledgements WHERE post_id=?")
      .bind(published.id).first<number>("n")).toBe(1);
    await postAction(db, owner, published.id, { action: "edit", expectedVersion: 1, body: "Printer part is due tomorrow." }, key());
    const changed = await handoverAcknowledgement(db, receiver, published.id);
    expect(changed).toMatchObject({ postVersion: 2, acknowledged: false, acknowledgedCount: 0 });
    expect(await db.prepare("SELECT state,version FROM staff_tasks WHERE id=?").bind(taskId)
      .first()).toMatchObject({ state: "in_progress", version: 1 });
  });

  it("hides the entire handover when a recipient loses source access", async () => {
    const view = await handoverView(db, owner, snapshot);
    const published = await publishHandover(db, owner, snapshot, {
      date: view.date, body: "Printer repair is waiting for parts.",
      sources: [{ id: taskId, sourceVersion: 1 }], recipientIds: [receiver.userId],
    }, key());
    await db.prepare("UPDATE staff_tasks SET assignee_user_id=?,version=version+1 WHERE id=?")
      .bind(outsider.userId, taskId).run();
    await expect(readablePost(db, receiver, published.id))
      .rejects.toMatchObject({ code: "POST_NOT_FOUND", status: 404 });
    await expect(acknowledgeHandover(db, receiver, published.id, key()))
      .rejects.toMatchObject({ code: "POST_NOT_FOUND", status: 404 });
    expect((await readablePost(db, owner, published.id)).id).toBe(published.id);
  });
});

async function handoverPackage(state:'active'|'draining'='active'){
 const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId==='work');return packageGrantStore(db,{authorize:async()=>({userId:'isolated-operational-handover-provider'}),obligations:async()=>{throw Error('Actual Work duties required');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId:'work',packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Actual original Operations handover source',evidence:'test:typed-operational-handover'}});
}
const handoverInput=()=>({date:bangkokToday(Date.now()),body:'Original typed operational handover',sources:[{kind:'staff_task' as const,id:taskId,sourceVersion:1}],recipientIds:[receiver.userId]});
it('purchasedOperationalHandover uses only base Work for the original typed publication and acknowledgement while freeform purpose labels remain Social and inactive history stays exact',async()=>{
 vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const input=handoverInput(),command=key();await expect(publishHandover(db,owner,snapshot,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await handoverPackage();const {savePostDraft,postView}=await import('@erpx/server-collaboration/public/social-home');await expect(savePostDraft(db,owner,null,{body:input.body,audience:{kind:'participants',userIds:input.recipientIds},purpose:'handover',sources:[{kind:'staff_task',id:taskId}],expectedVersion:1},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});const published=await publishHandover(db,owner,snapshot,input,command);expect((await postView(db,receiver,published.id)).body).toBe(input.body);expect(await db.prepare("SELECT operation,record_id,result_json::jsonb->'sourceBindings'=?::jsonb source_matches FROM command_receipts WHERE command_id=?").bind(JSON.stringify([{kind:'staff_task',id:taskId}]),`${command}-operations`).first()).toEqual({operation:'work.handover.publish',record_id:published.id,source_matches:true});expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'work'}]);await postAction(db,receiver,published.id,{action:'comment',body:'Current original work handoff response'},key());await handoverPackage('draining');expect(await publishHandover(db,owner,snapshot,input,command)).toEqual(published);await expect(publishHandover(db,owner,snapshot,input,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});const acknowledgement=key(),accepted=await acknowledgeHandover(db,receiver,published.id,acknowledgement);expect(accepted.acknowledged).toBe(true);
 const {transitionTask,taskActionSchema}=await import('@erpx/server-operations/public/staff-tasks');await transitionTask(db,owner,taskId,taskActionSchema.parse({action:'cancel',expectedVersion:1,note:'Actual original task handoff is settled'}),key());const {changeInboxState}=await import('@erpx/server-collaboration/public/inbox'),notices=(await db.prepare('SELECT id,user_id FROM staff_notifications WHERE company_id=? AND read_at IS NULL').bind(company).all<{id:string;user_id:string}>()).results;for(const notice of notices){const actor=[owner,receiver,outsider].find(actor=>actor.userId===notice.user_id);expect(actor).toBeDefined();await changeInboxState(db,actor!,notice.id,{read:true});}
 const {packageGrantStore,readCompanyPackageGrants,packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),{withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),grants=await readCompanyPackageGrants(db,company),prior=grants.find(row=>row.packageId==='work')!,withdrawal={companyId:company,expectedRevision:packageGrantRevision(company,grants),idempotencyKey:key(),changes:[{expectedVersion:prior.version,grant:{...prior,state:'inactive' as const,version:prior.version+1,reason:'Actual handover and Task settled by original owners',evidence:'test:typed-handover-inactive'}}]},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-operational-handover-provider'}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)}),review=(apply=false)=>withOwnerPackageObligations(db,{companyId:company,packageId:'work',packageVersion:1},database=>apply?store.batch.applyIn(database,withdrawal,async()=>{}):store.batch.previewIn(database,withdrawal)),preview=await review();expect(await review(true)).toEqual(preview);expect((await postView(db,receiver,published.id)).body).toBe(input.body);expect(await acknowledgeHandover(db,receiver,published.id,acknowledgement)).toEqual(accepted);expect(await publishHandover(db,owner,snapshot,input,command)).toEqual(published);await expect(postAction(db,receiver,published.id,{action:'comment',body:'Inactive additional work response'},key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});
});
it('purchasedOperationalHandover keeps both native Social child commands and the immutable Operations receipt atomic after late source or IAM change and rejects an unregistered caller transaction',async()=>{
 await handoverPackage();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const input=handoverInput();await expect(db.transaction(database=>publishHandover(database,owner,snapshot,input,key()))).rejects.toMatchObject({code:'WORK_SOCIAL_TRANSACTION_REQUIRED'});
 for(const fault of ['source','authority'] as const){const command=key(),fn=`typed_handover_${key().replaceAll('-','')}`,tables=['staff_tasks','assignments','social_posts','social_post_drafts','social_post_sources','social_post_participants','social_handover_acknowledgements','command_receipts','audit_events','outbox_events','staff_notifications'],read=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await read(),effect=fault==='source'?`UPDATE staff_tasks SET title='Unreviewed actual source change' WHERE id='${taskId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`;await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}-operations') EXECUTE FUNCTION ${fn}()`);
  try{await expect(publishHandover(db,owner,snapshot,input,command)).rejects.toMatchObject({code:fault==='source'?'WORK_SOCIAL_SOURCE_CHANGED':'WORK_SOCIAL_AUTHORITY_CHANGED'});expect(await read()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}const accepted=await publishHandover(db,owner,snapshot,input,command);expect(await publishHandover(db,owner,snapshot,input,command)).toEqual(accepted);for(const id of [command,`${command}-publish`,`${command}-operations`])expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(id).first('n')).toBe(1);
 }
});

it('seeds exact handover receipt rows from the native current source and preserves original counts after acknowledgement amendment and access loss',async()=>{
 const {handoverAcknowledgements}=await import('@erpx/server-operations/public/handover-acknowledgement-read'),{postView}=await import('@erpx/server-collaboration/public/social-home'),view=await handoverView(db,owner,snapshot),published=await publishHandover(db,owner,snapshot,{date:view.date,body:'Original rendered receipt source',sources:[{id:taskId,sourceVersion:1}],recipientIds:[receiver.userId]},key());
 const initial=await handoverAcknowledgement(db,receiver,published.id);expect(await handoverAcknowledgements(db,receiver,[published.id,'missing'])).toEqual([initial]);expect((await postView(db,receiver,published.id)).handoverReceipt).toEqual(initial);expect(await handoverAcknowledgements(db,outsider,[published.id])).toEqual([]);
 await acknowledgeHandover(db,receiver,published.id,key());expect(await handoverAcknowledgements(db,receiver,[published.id])).toEqual([await handoverAcknowledgement(db,receiver,published.id)]);await postAction(db,owner,published.id,{action:'edit',expectedVersion:1,body:'Amended receipt source must be read again'},key());expect((await handoverAcknowledgements(db,receiver,[published.id]))[0]).toMatchObject({postVersion:2,acknowledged:false,acknowledgedCount:0,recipientCount:1,canAcknowledge:true});await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(receiver.assignmentId).run();expect(await handoverAcknowledgements(db,receiver,[published.id])).toEqual([]);
});
it('binds new confirmations to the actual displayed version while exact prior native receipt replay never acknowledges an amended handover',async()=>{
 const {acknowledgeHandoverVersion}=await import('@erpx/server-operations/public/handover'),view=await handoverView(db,owner,snapshot),published=await publishHandover(db,owner,snapshot,{date:view.date,body:'Read version one before confirming',sources:[{id:taskId,sourceVersion:1}],recipientIds:[receiver.userId]},key()),first=key();expect((await acknowledgeHandoverVersion(db,receiver,published.id,first,1)).acknowledged).toBe(true);await postAction(db,owner,published.id,{action:'edit',expectedVersion:1,body:'Version two needs a separate review'},key());expect(await acknowledgeHandoverVersion(db,receiver,published.id,first,1)).toEqual({postId:published.id,postVersion:1,acknowledged:true});await expect(acknowledgeHandoverVersion(db,receiver,published.id,key(),1)).rejects.toMatchObject({status:409,code:'HANDOVER_SOURCE_CHANGED'});expect(await db.prepare('SELECT count(*) n FROM social_handover_acknowledgements WHERE post_id=? AND post_version=2').bind(published.id).first('n')).toBe(0);
 expect((await acknowledgeHandover(db,receiver,published.id,key())).postVersion).toBe(2);expect((await acknowledgeHandoverVersion(db,receiver,published.id,key(),2)).acknowledged).toBe(true);expect(await db.prepare('SELECT count(*) n FROM social_handover_acknowledgements WHERE post_id=? AND post_version=2').bind(published.id).first('n')).toBe(1);
});

it('recovers an actually committed native handover from an incomplete transport reply with the exact original key body and recipients while preserving later draft edits',async()=>{
 const {createHandoverPublishCommands}=await import('@erpx/feature-work_projection/lib/handover-command'),{publishRecoverableHandover}=await import('@erpx/server-operations/public/handover');await handoverPackage();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const input=handoverInput(),commandKey=key(),sent:string[]=[],commands=createHandoverPublishCommands(async(_url,init)=>{const requestBody=String(init?.body);sent.push(requestBody);expect(new Headers(init?.headers).get('x-erpx-handover-publication')).toBe('recoverable');const result=await publishRecoverableHandover(db,owner,snapshot,JSON.parse(requestBody),new Headers(init?.headers).get('Idempotency-Key')!);return Response.json({data:sent.length===1?{id:result.id}:result});},()=>commandKey),attempt=commands.prepare('/api/v1/holdings/acme/handover',input,'original-rendered-context',1);await expect(commands.execute(attempt,()=> 'original-rendered-context',()=>1)).rejects.toThrow('acknowledgement is incomplete');expect(await db.prepare('SELECT count(*) n FROM social_posts WHERE company_id=?').bind(company).first('n')).toBe(1);
 input.body='Newer handover must remain a local draft';input.recipientIds=[outsider.userId];expect(()=>commands.prepare('/api/v1/holdings/acme/handover',input,'original-rendered-context',2)).toThrow('Retry the submitted handover first');const recovered=await commands.execute(attempt,()=> 'original-rendered-context',()=>2);expect(recovered).toEqual({receipt:{id:`post-post-draft-${commandKey}`,version:1},unchanged:false});expect(sent[1]).toBe(sent[0]);expect((await readablePost(db,receiver,recovered.receipt.id)).body).toBe('Original typed operational handover');await expect(readablePost(db,outsider,recovered.receipt.id)).rejects.toMatchObject({code:'POST_NOT_FOUND'});expect(await db.prepare('SELECT count(*) n FROM social_posts WHERE company_id=?').bind(company).first('n')).toBe(1);for(const id of [commandKey,`${commandKey}-publish`,`${commandKey}-operations`])expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(id).first('n')).toBe(1);expect((await db.prepare('SELECT state,version FROM staff_tasks WHERE id=?').bind(taskId).first())).toEqual({state:'in_progress',version:1});
});

it.each(['disabled','enforced'] as const)('recovers only a proven exact modern publication across business days under %s without weakening legacy active-day checks or minting another post',async mode=>{
 const {publishRecoverableHandover}=await import('@erpx/server-operations/public/handover');if(mode==='enforced'){await handoverPackage();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');}const input=handoverInput(),command=key(),original=await publishRecoverableHandover(db,owner,snapshot,input,command),tables=['social_posts','social_post_drafts','social_post_sources','social_post_participants','staff_tasks','command_receipts','audit_events','outbox_events'],read=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await read();if(mode==='enforced')await handoverPackage('draining');const retained=await read(),future=Date.now()+2*86400000,clock=vi.spyOn(Date,'now').mockReturnValue(future);
 try{expect(await publishRecoverableHandover(db,owner,snapshot,input,command)).toEqual(original);await expect(publishHandover(db,owner,snapshot,input,command)).rejects.toMatchObject({code:'HANDOVER_DATE_CHANGED'});await expect(publishRecoverableHandover(db,owner,snapshot,{...input,body:'Changed submitted body'},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(publishRecoverableHandover(db,receiver,snapshot,input,command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(publishRecoverableHandover(db,owner,snapshot,input,key())).rejects.toMatchObject({code:mode==='enforced'?'PACKAGE_DRAINING':'HANDOVER_DATE_CHANGED'});expect(await read()).toEqual(mode==='enforced'?retained:before);expect(await db.prepare('SELECT operation FROM command_receipts WHERE command_id=?').bind(`${command}-operations`).first('operation')).toBe('work.handover.publish');}
 finally{clock.mockRestore();}
 await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(owner.assignmentId).run();await expect(publishRecoverableHandover(db,owner,snapshot,input,command)).rejects.toMatchObject({code:'WORK_SOCIAL_AUTHORITY_CHANGED'});expect(await db.prepare('SELECT count(*) n FROM social_posts WHERE company_id=?').bind(company).first('n')).toBe(1);
});
it('does not promote an unproven legacy publication into a recoverable Operations receipt and rolls the new default-disabled complete owned publication back after late native source or IAM changes',async()=>{
 const {publishRecoverableHandover}=await import('@erpx/server-operations/public/handover'),input=handoverInput(),legacyKey=key(),legacy=await publishHandover(db,owner,snapshot,input,legacyKey);await expect(publishRecoverableHandover(db,owner,snapshot,input,legacyKey)).rejects.toMatchObject({code:'HANDOVER_RECOVERY_UNPROVEN'});expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(`${legacyKey}-operations`).first('n')).toBe(0);expect((await readablePost(db,owner,legacy.id)).body).toBe(input.body);
 await expect(db.transaction(tx=>publishRecoverableHandover(tx,owner,snapshot,input,key()))).rejects.toMatchObject({code:'WORK_SOCIAL_TRANSACTION_REQUIRED'});
 for(const fault of ['source','authority','receipt'] as const){const command=key(),fn=`recoverable_handover_${key().replaceAll('-','')}`,tables=['staff_tasks','assignments','social_posts','social_post_drafts','social_post_sources','social_post_participants','command_receipts','audit_events','outbox_events'],read=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r ORDER BY image`).all()).results)),before=await read(),effect=fault==='source'?`UPDATE staff_tasks SET title='Unreviewed late source' WHERE id='${taskId}'`:fault==='authority'?`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${owner.assignmentId}'`:"UPDATE command_receipts SET request_hash=repeat('0',64) WHERE command_id=NEW.command_id";await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}-operations') EXECUTE FUNCTION ${fn}()`);
  try{await expect(publishRecoverableHandover(db,owner,snapshot,input,command)).rejects.toMatchObject({code:fault==='authority'?'WORK_SOCIAL_AUTHORITY_CHANGED':'WORK_SOCIAL_SOURCE_CHANGED'});expect(await read()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}const recovered=await publishRecoverableHandover(db,owner,snapshot,input,command);expect(await publishRecoverableHandover(db,owner,snapshot,input,command)).toEqual(recovered);
 }
});
it('keeps the recoverable original Project-task handover on its real Projects foundation without buying Work or full People',async()=>{
 const {publishRecoverableHandover}=await import('@erpx/server-operations/public/handover'),{saveProject,projectSchema}=await import('@erpx/server-projects/public/staff-projects'),{saveTask,taskSchema}=await import('@erpx/server-operations/public/staff-tasks'),{getTask}=await import('@erpx/server-operations/public/staff-task-read'),{packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');await packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-owner'}),obligations:async()=>{throw Error('Actual owner duties required');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual Project-task handover foundation',evidence:'test:recoverable-project-handover'}});vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Real Project handover foundation',memberUserIds:[receiver.userId]}),key()),created=await saveTask(db,owner,null,taskSchema.parse({title:'Actual Project task to hand over',projectId:project.id,assigneeUserId:receiver.userId}),key());const task=await getTask(db,owner,String(created.id)),work:WorkSnapshot={items:[{id:task.id,type:'staff_task',title:task.title,detail:'Actual Project work',href:`/staff-tasks/${task.id}`,status:task.state,updatedAt:task.updated_at,queue:'todo'}],unavailableSources:[]},input={date:bangkokToday(),body:'Project-only original operational handover',sources:[{kind:'staff_task' as const,id:task.id,sourceVersion:task.version}],recipientIds:[receiver.userId]},command=key(),result=await publishRecoverableHandover(db,owner,work,input,command);expect((await readablePost(db,receiver,result.id)).body).toBe(input.body);expect(await publishRecoverableHandover(db,owner,work,input,command)).toEqual(result);expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(company).all()).results).toEqual([{package_id:'projects'}]);expect(owner.modules).toEqual([]);expect(receiver.modules).toEqual([]);
});
