import {notificationWorkspace} from "@erpx/server-collaboration/public/notification-presentation";
import {sendNotice,changeInboxState} from "@erpx/server-collaboration/public/inbox";
import { beforeAll, expect, it, vi } from "vitest";
import { randomUUID } from "node:crypto";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { createProductWorkspace } from "@erpx/server-identity_access/application/product-workspace";
import { productWorkspaceTarget } from "@erpx/server-identity_access/domain/product-workspace";
import { workspaceGrantsForAccount, type AccountAccessContext } from "@erpx/server-identity_access/public/authorization";
import { accountWorkModuleAccess } from "@erpx/server-identity_access/public/work-module-access";
import { initializeWorkSelection, readWorkSelection, selectWorkRole } from "@erpx/server-work_projection/public/work-role-store";
import { packageGrantStore, readCompanyPackageGrants, readCompanyPurchaseFacts } from "@erpx/server-commercial/public/package-grants";
import { ApiError } from "@erpx/server-runtime/public/http";
import { ownerPackageObligations, withOwnerPackageObligations } from "@erpx/server-commercial/public/owner-package-obligations";
import { packageGrantRevision } from "@erpx/server-commercial/public/package-grants";
import { readPurchasedAccess } from "@erpx/server-commercial/public/purchase-command-boundary";

vi.mock("@erpx/server-runtime/public/runtime", () => ({ holdingResources: async () => ({ write: env.H001_DB }), runtimeEnv: async () => env }));
const db = env.H001_DB;
beforeAll(async () => { await Promise.all([applyMigrations(db, env.STAFF_TEST_MIGRATIONS), applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS)]); });
async function fixture(marketing = true, ownerModule = "marketing", initialPackages?:string[],deferCommercial=false) {
  const f = await seedExpenseCompany(db, env.H001_FILES), userId = f.owner.userId, sessionId = randomUUID();
  await db.prepare("UPDATE assignments SET modules_json=? WHERE id=?").bind(JSON.stringify([ownerModule]),f.owner.assignmentId).run();
  const financeId = randomUUID(), salesId = randomUUID();
  for (const [id, role, modules] of [[financeId, "finance_preparer", ["finance"]], [salesId, "sales_operator", ["sales"]]] as const)
    await db.prepare("INSERT INTO assignments(id,user_id,role,company_id,modules_json,valid_from,valid_until,scope_kind) VALUES(?,?,?,?,?,0,9999999999999,'holding')")
      .bind(id, userId, role, f.company, JSON.stringify(modules)).run();
  const store = packageGrantStore(db, { authorize: async (_database, companyId) => {
    if (companyId !== f.company) throw new Error("FICTIONAL_PROVIDER_SCOPE");
    return { userId: "fictional-native-platform-provider" };
  }, obligations: async () => { throw new Error("NO_LIFECYCLE_OPERATION_IN_ENTRY_FIXTURE"); } });
  const grants = initialPackages??["sales", ...(marketing ? ["sales-marketing"] : [])];
  for (const packageId of grants) await store.apply({ expectedVersion: 0, idempotencyKey: randomUUID(), grant: {
    companyId: f.company, packageId, packageVersion: 1, state: "active", startsAt: Date.now() - 1000, endsAt: null,
    version: 1, reason: "Fictional package-only native fixture", evidence: "test:product-role-entry" } });
  const scope = { kind: "holding" as const, holdingId: "h001" as const, holdingSlug: "test", holdingName: "Fixture", companyId: f.company };
  const access = async (): Promise<AccountAccessContext> => {
    const { rows, roles } = await workspaceGrantsForAccount(db, f.company, userId);
    return { scope, sessionId, userId, expiresAt: Date.now() + 600_000, name: "Fictional native user", email: "fixture@example.test", requestHeaders: new Headers(),
      assignments: rows.filter(row => roles.get(row.role)?.scopes.includes(row.scope_kind)).map(row => ({ id: row.id, role: row.role,
        companyId: row.company_id, branchId: row.branch_id, scopeKind: row.scope_kind, scopeNodeId: row.scope_node_id,
        modules: JSON.parse(row.modules_json), version: row.version, validUntil: row.valid_until, label: roles.get(row.role)?.name })) };
  };
  let selections = 0;
  const service = createProductWorkspace({ access, modules: accountWorkModuleAccess, purchaseFacts: account => readCompanyPurchaseFacts(db, account.scope.companyId),
    commercialAdmissionDeferred:()=>deferCommercial,
    ready: async () => true,
    selection: (account, applicable) => initializeWorkSelection(db, { sessionId, userId, companyId: f.company, expiresAt: account.expiresAt }, account.assignments, applicable),
    select: (account, option, revision) => { selections++; return selectWorkRole(db, { sessionId, userId, companyId: f.company, expiresAt: account.expiresAt,
      assignmentId: option.assignmentId, assignmentVersion: option.assignmentVersion, revision }); },
    now: Date.now, error: (status, code, message) => new ApiError(status, code, message) });
  const input = productWorkspaceTarget("sales", f.company, "/holdings/test/customers-sales/sales/pipeline?q=retained#draft")!;
  await selectWorkRole(db, { sessionId, userId, companyId: f.company, assignmentId: financeId, assignmentVersion: 1, revision: 0, expiresAt: Date.now() + 600_000 });
  return { ...f, nativeAccess:access,userId, sessionId, financeId, salesId, service, input, store, selections: () => selections,
    current: () => readWorkSelection(db, sessionId, userId, f.company) };
}

it('keeps native development module entry and explicit role CAS without Commercial grants, and refuses revoked authority',async()=>{
 const f=await fixture(true,'marketing',[],true);
 const assignments=(await db.prepare('SELECT * FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results;
 const before=await f.current(),entry=await f.service.read('sales',f.input);
 expect(entry.target).toBeNull();expect(entry.options.some(option=>option.assignmentId===f.salesId)).toBe(true);
 expect(await f.current()).toEqual(before);expect(f.selections()).toBe(0);
 await f.service.choose('sales',f.input,f.salesId,before!.revision,1);
 // Sales Operator has Sales, not CRM pipeline authority; keep its native Today landing.
 expect((await f.service.read('sales',f.input)).target).toBe('/holdings/test/customers-sales/sales/today?q=retained#draft');
 expect((await db.prepare('SELECT * FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(assignments);
 expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
 expect((await db.prepare('SELECT id FROM company_package_grant_events WHERE company_id=?').bind(f.company).all()).results).toEqual([]);
 await db.prepare('UPDATE assignments SET revoked_at=? WHERE id IN (?,?)').bind(Date.now(),f.salesId,f.owner.assignmentId).run();
 await expect(f.service.read('sales',f.input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
});

it("keeps the live Finance role until an explicit normal-service Marketing selection, with assignment data intact", async () => {
  const f = await fixture(), before = (await db.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results;
  const state = await f.service.read("sales", f.input);
  expect(state.target).toBeNull(); expect(state.options.map(option => option.assignmentId)).not.toContain(f.financeId);
  expect(state.options.find(option => option.assignmentId === f.owner.assignmentId)?.target).toBe("/holdings/test/customers-sales/marketing?q=retained#draft");
  expect((await f.current())?.assignment_id).toBe(f.financeId); expect(f.selections()).toBe(0);
  await f.service.choose("sales", f.input, f.owner.assignmentId, state.selection!.revision, 1);
  expect((await f.service.read("sales", f.input)).target).toBe("/holdings/test/customers-sales/marketing?q=retained#draft");
  expect((await f.current())?.revision).toBe(2);
  expect((await db.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(before);
});
it("does not make Marketing available from Sales base alone, while preserving the purchased Sales role", async () => {
  const f = await fixture(false), state = await f.service.read("sales", f.input);
  expect(state.options.map(option => option.assignmentId)).toContain(f.salesId);
  expect(state.options.map(option => option.assignmentId)).not.toContain(f.owner.assignmentId);
  await expect(f.service.choose("sales", f.input, f.owner.assignmentId, state.selection!.revision, 1)).rejects.toMatchObject({ code: "ASSIGNMENT_UNAVAILABLE" });
  expect((await f.current())?.assignment_id).toBe(f.financeId);
});
it("keeps explicit product role entry available after expiry without replacing the live Finance role", async () => {
  const f=await fixture(false),before=(await db.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results;
  const grant=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId==='sales')!;
  await f.store.apply({expectedVersion:1,idempotencyKey:randomUUID(),grant:{...grant,endsAt:Date.now()-1,version:2,reason:'Reviewed native expiry',evidence:'test:entry-after-expiry'}});
  const state=await f.service.read('sales',f.input);
  expect(state.target).toBeNull();expect(state.options.some(option=>option.assignmentId===f.salesId)).toBe(true);
  expect((await f.current())?.assignment_id).toBe(f.financeId);
  await f.service.choose('sales',f.input,f.salesId,state.selection!.revision,1);
  expect((await f.service.read('sales',f.input)).target).toBe('/holdings/test/customers-sales/sales/today?q=retained#draft');
  expect((await db.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(before);
});
it("uses real audited history for explicit Support entry after inactivation and a future renewal", async () => {
  const f=await fixture(false,'support'),store=packageGrantStore(db,{authorize:async()=>({userId:'fictional-native-history-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});
  const value=(version:number,state:'active'|'draining'|'inactive',startsAt=0)=>({companyId:f.company,packageId:'support',packageVersion:1,state,startsAt,endsAt:null,version,reason:'Native retained product history',evidence:'test:audited-product-history'});
  const apply=async(grant:ReturnType<typeof value>)=>store.apply({grant,expectedVersion:grant.version-1,idempotencyKey:randomUUID()});
  await apply(value(1,'active'));await apply(value(2,'draining'));
  await withOwnerPackageObligations(db,{companyId:f.company,packageId:'support',packageVersion:1},async tx=>{
    await store.batch.applyIn(tx,{companyId:f.company,expectedRevision:packageGrantRevision(f.company,await readCompanyPackageGrants(tx,f.company)),idempotencyKey:randomUUID(),changes:[{expectedVersion:2,grant:value(3,'inactive')}]},async()=>{});
  });
  await apply(value(4,'active',Date.now()+60000));
  expect(await readPurchasedAccess(db,'enforced',{companyId:f.company,capability:'support.tickets'})).toMatchObject({newWork:false,settleExisting:false,history:true});
  const input=productWorkspaceTarget('support',f.company,'/holdings/test/services/desk?state=closed#history')!,state=await f.service.read('support',input);
  expect(state.target).toBeNull();expect(state.options.find(option=>option.assignmentId===f.owner.assignmentId)?.purchasePurpose).toBe('history');
  expect((await f.current())?.assignment_id).toBe(f.financeId);
  const before=(await db.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results;
  await f.service.choose('support',input,f.owner.assignmentId,state.selection!.revision,1);
  expect((await f.service.read('support',input)).target).toBe(input.target);
  expect((await db.prepare("SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id").bind(f.company).all()).results).toEqual(before);
});
it("does not invent entry history when a first future Support purchase is cancelled before its start", async () => {
  const f=await fixture(false,'support'),store=packageGrantStore(db,{authorize:async()=>({userId:'fictional-native-history-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)}),startsAt=Date.now()+60000;
  const value=(version:number,state:'active'|'draining'|'inactive')=>({companyId:f.company,packageId:'support',packageVersion:1,state,startsAt,endsAt:null,version,reason:'Native unstarted period cancellation',evidence:'test:no-invented-product-history'});
  for(const [version,state]of [[1,'active'],[2,'draining']]as const)await store.apply({grant:value(version,state),expectedVersion:version-1,idempotencyKey:randomUUID()});
  await withOwnerPackageObligations(db,{companyId:f.company,packageId:'support',packageVersion:1},async tx=>{
    await store.batch.applyIn(tx,{companyId:f.company,expectedRevision:packageGrantRevision(f.company,await readCompanyPackageGrants(tx,f.company)),idempotencyKey:randomUUID(),changes:[{expectedVersion:2,grant:value(3,'inactive')}]},async()=>{});
  });
  expect((await readCompanyPurchaseFacts(db,f.company)).history.filter(row=>row.packageId==='support')).toEqual([]);
  const input=productWorkspaceTarget('support',f.company,'/holdings/test/services/desk')!;
  await expect(f.service.read('support',input)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
  expect((await f.current())?.assignment_id).toBe(f.financeId);
});
it("serializes concurrent product role selections through the existing Holding context revision", async () => {
  const f = await fixture(), state = await f.service.read("sales", f.input);
  const results = await Promise.allSettled([f.owner.assignmentId, f.salesId].map(id => f.service.choose("sales", f.input, id, state.selection!.revision, 1)));
  expect(results.filter(result => result.status === "fulfilled")).toHaveLength(1);
  expect(results.filter(result => result.status === "rejected")).toHaveLength(1);
  expect((await f.current())?.revision).toBe(2);
});
it("rejects a changed assignment version until the user reviews the fresh role choice", async () => {
  const f = await fixture(), state = await f.service.read("sales", f.input);
  await db.prepare("UPDATE assignments SET version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();
  await expect(f.service.choose("sales", f.input, f.owner.assignmentId, state.selection!.revision, 1)).rejects.toMatchObject({ code: "WORK_CONTEXT_CHANGED" });
  expect((await f.current())?.assignment_id).toBe(f.financeId);
  const refreshed = await f.service.read("sales", f.input);
  const option = refreshed.options.find(option => option.assignmentId === f.owner.assignmentId)!;
  expect(option.assignmentVersion).toBe(2);
  await f.service.choose("sales", f.input, option.assignmentId, refreshed.selection!.revision, option.assignmentVersion);
  expect((await f.current())?.assignment_version).toBe(2);
});
it("keeps revocation closed while offering explicit settlement entry for a drained add-on without replacing the live role", async () => {
  const f = await fixture(), state = await f.service.read("sales", f.input);
  await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(), f.salesId).run();
  await expect(f.service.choose("sales", f.input, f.salesId, state.selection!.revision, 1)).rejects.toMatchObject({ code: "ASSIGNMENT_UNAVAILABLE" });
  const current = (await readCompanyPackageGrants(db, f.company)).find(grant => grant.packageId === "sales-marketing")!;
  await f.store.apply({ expectedVersion: current.version, idempotencyKey: randomUUID(), grant: { ...current, state: "draining", version: current.version + 1,
    reason: "Fictional reviewed add-on drain", evidence: "test:entry-draining" } });
  const drained=await f.service.read("sales",f.input);
  expect(drained.target).toBeNull();
  expect(drained.options.find(option=>option.assignmentId===f.owner.assignmentId)).toMatchObject({purchasePurpose:"settle-existing",target:"/holdings/test/customers-sales/marketing?q=retained#draft"});
  expect(drained.options.some(option=>option.assignmentId===f.salesId)).toBe(false);
  expect((await f.current())?.assignment_id).toBe(f.financeId);
  expect((await readCompanyPackageGrants(db, f.company)).find(grant => grant.packageId === "sales-marketing")?.state).toBe("draining");
});


it('opens real POS and Hotel backoffice with the purchased Company and explicit original owner role, preserving assignments through expiry and revocation',async()=>{
 for(const app of ['pos','pms'] as const){
  const f=await fixture(false,app,[]),value={companyId:f.company,packageId:app,packageVersion:1,state:'active' as const,startsAt:Date.now()-1000,endsAt:null as number|null,version:1,reason:'Native Front backoffice migration fixture',evidence:'test:front-product-backoffice'};
  await f.store.apply({expectedVersion:0,idempotencyKey:randomUUID(),grant:value});
  const before=(await db.prepare('SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results;
  const input=productWorkspaceTarget(app,f.company,'/holdings/test/'+app+'/settings?property=retained&tag=one&tag=two#history')!,state=await f.service.read(app,input);
  expect(state.target).toBeNull();expect(state.options.map(option=>option.assignmentId)).toContain(f.owner.assignmentId);expect(state.options.map(option=>option.assignmentId)).not.toContain(f.financeId);expect((await f.current())?.assignment_id).toBe(f.financeId);expect(f.selections()).toBe(0);
  await f.service.choose(app,input,f.owner.assignmentId,state.selection!.revision,1);expect((await f.service.read(app,input)).target).toBe(input.target);expect((await f.current())?.revision).toBe(2);
  expect((await db.prepare('SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(before);
  await f.store.apply({expectedVersion:1,idempotencyKey:randomUUID(),grant:{...value,version:2,endsAt:Date.now()-1}});expect((await f.service.read(app,input)).options.find(option=>option.assignmentId===f.owner.assignmentId)?.purchasePurpose).toBe('settle-existing');
  await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),f.owner.assignmentId).run();await expect(f.service.read(app,input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 }
});


it('uses a real POS-only purchase for exact notification foundation entry while retaining current role and audited package history',async()=>{
 const f=await fixture(false,'pos',[]),grant={companyId:f.company,packageId:'pos',packageVersion:1,state:'active' as const,startsAt:Date.now()-1000,endsAt:null as number|null,version:1,reason:'Native notification foundation fixture',evidence:'test:product-notification-foundation'};await f.store.apply({expectedVersion:0,idempotencyKey:randomUUID(),grant});const before=(await db.prepare('SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results;
 const input=productWorkspaceTarget('work',f.company,'/holdings/test/inbox/notifications?view=all#delivery')!,state=await f.service.read('work',input);expect(state.target).toBe(input.target);expect(state.sourceRecord).toBe(true);expect((await f.current())?.assignment_id).toBe(f.financeId);expect(f.selections()).toBe(0);expect(state.options.every(option=>option.purchasePurpose==='history')).toBe(true);
 const notice=await sendNotice(db,f.claimant,{title:'POS source update',body:'Review the actual delivered source.',recipients:[f.userId]},randomUUID()),currentActor={...f.owner,assignmentId:f.financeId,role:'finance_preparer'};
 const snapshot=await notificationWorkspace(currentActor,true);expect(snapshot.notificationsOnly).toBe(true);const item=snapshot.initial.items.find(row=>row.sourceId===notice.id)!;expect(item.message).toContain('POS source update');expect(snapshot.initial.unread).toBeGreaterThan(0);expect(item.readAt).toBeNull();await changeInboxState(db,currentActor,item.id,{read:true});expect((await notificationWorkspace(currentActor,true)).initial.items.some(row=>row.id===item.id)).toBe(false);expect((await notificationWorkspace(currentActor,false)).notificationsOnly).toBe(false);

 for(const target of ['home','inbox','inbox/messages','inbox/teams','work/actions','calendar/mine'])await expect(f.service.read('work',productWorkspaceTarget('work',f.company,'/holdings/test/'+target)!)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 expect((await db.prepare('SELECT id,version,role,modules_json FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all()).results).toEqual(before);await f.store.apply({expectedVersion:1,idempotencyKey:randomUUID(),grant:{...grant,version:2,endsAt:Date.now()-1}});expect((await f.service.read('work',input)).target).toBe(input.target);
});


it('opens native Company Admin foundation with no packages, preserves a different live role, and uses normal current-version CAS only on explicit choice',async()=>{
 const {createAdminWorkspace}=await import('@erpx/server-identity_access/application/admin-workspace'),{adminWorkspaceTarget}=await import('@erpx/server-identity_access/domain/admin-workspace'),{adminWorkspaceDestinations}=await import('@erpx/server-identity_access/adapters/admin-workspace'),f=await fixture(false,'identity_access',[]),input=adminWorkspaceTarget(f.company,'/holdings/test/administration/technology/settings/roles?q=own#draft')!,service=createAdminWorkspace({access:f.nativeAccess,destinations:adminWorkspaceDestinations,management:async()=>false,selection:(account,applicable)=>initializeWorkSelection(db,{sessionId:f.sessionId,userId:f.userId,companyId:f.company,expiresAt:account.expiresAt},account.assignments,applicable),select:(account,option,revision)=>selectWorkRole(db,{sessionId:f.sessionId,userId:f.userId,companyId:f.company,expiresAt:account.expiresAt,assignmentId:option.assignmentId,assignmentVersion:option.assignmentVersion,revision}),now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const assignmentImage=()=>db.prepare('SELECT id,version,role,modules_json,revoked_at FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all(),before=(await assignmentImage()).results,state=await service.read(input);expect(state.target).toBeNull();expect(state.options.map(option=>option.assignmentId)).toEqual([f.owner.assignmentId]);expect((await f.current())?.assignment_id).toBe(f.financeId);expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);await expect(service.choose(input,f.owner.assignmentId,state.selection!.revision-1,1)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});expect((await f.current())?.assignment_id).toBe(f.financeId);await service.choose(input,f.owner.assignmentId,state.selection!.revision,1);expect((await service.read(input)).target).toBe(input.target);expect((await assignmentImage()).results).toEqual(before);expect(await readCompanyPackageGrants(db,f.company)).toEqual([]);
 const current=(await f.current())!;await db.prepare('UPDATE assignments SET version=version+1 WHERE id=?').bind(f.owner.assignmentId).run();const changed=await service.read(input);expect(changed.target).toBeNull();expect(changed.options).toMatchObject([{assignmentId:f.owner.assignmentId,assignmentVersion:2}]);await expect(service.choose(input,f.owner.assignmentId,current.revision,1)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});await service.choose(input,f.owner.assignmentId,current.revision,2);expect((await service.read(input)).target).toBe(input.target);await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.owner.assignmentId).run();await expect(service.read(input)).rejects.toMatchObject({code:'ADMIN_ROLE_REQUIRED'});
});

it('Company Admin exposes current Brand, Organization, and IAM owner reads without changing roles or package grants',async()=>{
 const f=await fixture(false,'identity_access',[]),modules=['brand','organization','identity_access'];
 await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(modules),f.owner.assignmentId).run();
 const access=await f.nativeAccess(),assignment=access.assignments.find(item=>item.id===f.owner.assignmentId)!;
 const {adminWorkspaceDestinations}=await import('@erpx/server-identity_access/adapters/admin-workspace');
 const {createAdminWorkspace}=await import('@erpx/server-identity_access/application/admin-workspace');
 const {adminWorkspaceTarget}=await import('@erpx/server-identity_access/domain/admin-workspace');
 const {brandIdentityWorkspace}=await import('@erpx/server-brand/public/brand-identity');
 const {organizationAccessData}=await import('@erpx/server-organization/public/organization-access');
 const {iamOverview}=await import('@erpx/server-identity_access/public/iam-service-reads');
 const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');
 const assignmentImage=()=>db.prepare('SELECT id,version,role,modules_json,revoked_at FROM assignments WHERE company_id=? ORDER BY id').bind(f.company).all();
 const assignmentsBefore=(await assignmentImage()).results,packagesBefore=await readCompanyPackageGrants(db,f.company);
 expect(packagesBefore).toEqual([]);
 const iam=await loadIamAuthority(db,f.company,f.userId,assignment.role,f.sessionId),owner={...f.owner,modules:[...assignment.modules],assignments:access.assignments,iam};
 const paths=(await adminWorkspaceDestinations(access,assignment)).map(item=>item.path);
 expect(paths).toEqual(expect.arrayContaining(['administration/brand/identity','administration/technology/organization','administration/technology/settings/roles']));
 const service=createAdminWorkspace({access:f.nativeAccess,destinations:adminWorkspaceDestinations,management:async()=>false,
  selection:async()=>{throw new Error('COMPANY_ADMIN_AVAILABILITY_MUST_NOT_READ_SELECTION');},select:async()=>{throw new Error('COMPANY_ADMIN_AVAILABILITY_MUST_NOT_SELECT_ROLE');},
  now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 for(const path of ['administration/brand/identity','administration/technology/organization','administration/technology/settings/roles']){
  const input=adminWorkspaceTarget(f.company,`/holdings/test/${path}`)!;
  await expect(service.available(input)).resolves.toBe(true);
 }
 const [brand,organization,identity]=await Promise.all([
  brandIdentityWorkspace(owner),organizationAccessData(db,owner),
  iamOverview(db,{...owner,scopeKind:'holding',scopeNodeId:null,platform:false,sessionId:f.sessionId} as import("@erpx/server-identity_access/public/iam-context").IamActor),
 ]);
 expect(brand.actorId).toBe(f.userId);expect(brand.initial.identity.shortName).toBeTruthy();
 expect(organization.actorId).toBe(f.userId);expect(organization.nodes).toContainEqual(expect.objectContaining({id:f.company,kind:'company'}));expect(organization.nodes).toContainEqual(expect.objectContaining({id:f.branch,kind:'branch'}));
 expect(identity).toMatchObject({companyId:f.company,scopeKind:'holding',currentUserId:f.userId,currentAssignmentId:f.owner.assignmentId});
 expect(identity.assignments).toContainEqual(expect.objectContaining({id:f.owner.assignmentId,user_id:f.userId,role:'holding_owner'}));
 expect((await f.current())?.assignment_id).toBe(f.financeId);
 expect(await readCompanyPackageGrants(db,f.company)).toEqual(packagesBefore);
 expect((await assignmentImage()).results).toEqual(assignmentsBefore);
});
