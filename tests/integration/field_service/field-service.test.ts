import {withNativeFieldFixture,type NativeFieldFixture} from '../../support/files/field-files-native-parity';
import {historicalServiceCount} from '../../support/field_service/legacy-count-oracle';
import {attachmentToolEnvironment} from '../../../scripts/testing/runtime-tooling';
import { packageGrantStore, readCompanyPackageGrants } from "@erpx/server-commercial/public/package-grants";


import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb,
  key,
} from "../../support/helpers/expense-fixture";
import {createHash} from 'node:crypto';
import {LocalFileStore} from '@erpx/server-runtime/adapters/runtime/filesystem';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import {assignFlowPrimary,resolveFlow} from '@erpx/server-workflow/public/flow-routing';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
let serviceDatabase:SqlDatabase|undefined;
let db:PostgresDatabase=expenseDb;
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: serviceDatabase??env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
db=expenseDb;
});
/** Recreate the pre-existing legacy source shape from the original compatibility fixture. */
async function mixedHistoricalFile(native:NativeFieldFixture,id:string,purposeKey:string,version:number,compatibility:boolean,body:Buffer){
 const token=key(),setId=`test-attachment-set-${token}`,sourceId=`test-attachment-source-${token}`,linkId=`test-attachment-link-${token}`,uploadId=`test-upload-${token}`,objectKey=`test/attachments/${token}`,checksum=createHash('sha256').update(body).digest('hex'),now=Date.now();
 await new LocalFileStore(native.store).put(objectKey,body,{httpMetadata:{contentType:'application/pdf'}});
 await db.batch([
  db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,owner_version,company_id,detected_media_type) VALUES(?,'field_service_visit',?,?,'evidence.pdf','application/pdf',?,?,?,?,?,'ready',?,?,?,?,?,?,'application/pdf')").bind(uploadId,id,purposeKey,body.length,body.length,checksum,`test/staging/${token}`,objectKey,f.claimant.userId,now+900000,now,now,version,f.company),
  db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES(?,?,'field_service_visit',?,?,?,1,'evidence.pdf','application/pdf',?,?,?, ?,?,'application/pdf','available',?,?)").bind(sourceId,uploadId,id,purposeKey,objectKey,body.length,checksum,f.claimant.userId,now,f.company,now,now),
  db.prepare("INSERT INTO attachment_sets(id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) VALUES(?,?,NULL,NULL,'field_service_visit',?,'record',NULL,?,?,1,'internal','{}',0,10,'draft',?,?,?)").bind(setId,f.company,id,purposeKey,version,f.claimant.userId,now,now),
  db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,?,?)").bind(linkId,setId,sourceId,f.claimant.userId,now,now+7*86400000),
  ...(compatibility?[db.prepare("INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) VALUES(?,?,?,?,'set.created',?,?)").bind(`test-attachment-event-${token}`,f.company,setId,f.claimant.userId,JSON.stringify({compatibilityAdapter:true}),now)]:[]),
 ]);return {setId,sourceId,linkId,bytes:body};
}
// Original visit-completion/evidence assertions now execute through the native
// normal-auth PostgreSQL case native_field_executive_evidence_keeps_cursor_audit_and_source_rollback.
// Its report wire values/forbidden-module contract remain in runtime-executive-services.test.ts.
// Native counterpart: monetary_field_invoice_rejects_no_charge_without_mutating_its_draft.
// Actual partial/ordinary Agreement approval,75000/150000 demand, original
// warranty/date snapshots, independent native posting and balanced persisted journal.

// The same native lifecycle also qualifies real last-unit HTTP competition,
// foreign scope, stale-version wire errors and immutable source/history guards.
// Native monetary lifecycle injects the original final-event23514 failure,
// checks full Finance/Money/release/effect rollback, then exact-key retry/replay.
// Native normal-session Work/readback retains revoked-technician blocking,
// independent reassignment and fresh scoped customer/technician references.
// selected_native_service_startup_customer_agreement_needs_no_finance_ledger
// preserves the USD Department draft, current permissions and original refusals.
// Cross-Department asset omission/refusal and zero effects are qualified by
// the native monetary lifecycle; shared fixture hooks below remain in use.

// Reservation recipient/expiry/deduplication/released-source assertions moved
// to scheduled_field_reservation_alerts_keep_original_phase_identity_and_rollback_sources
// in rust-field-collaboration-selected-postgres.test.rs with native writer fences.

afterEach(() => { serviceDatabase=undefined;vi.unstubAllEnvs(); vi.restoreAllMocks(); });

// Native startup case retains no-purchase Workspace/command403, Work omission
// and zero Agreement drafts through the actual selected HTTP runtime.




// Native Service-only aggregate preserves bound-asset purchase denial, exact
// omission conflict, lawful creation, draining settlement and cancellation.

// Original retained-asset masking/Work/waiting/restoration assertions execute
// in private_workspace_masks_assets_without_parts_history through native HTTP.

// Native normal-session revoked-role calls retain WORK_ROLE_UNAVAILABLE and
// expose no Workspace/Detail/Work data. The new selected-context query returns
// context409; the shared Work consumer omits failed-source rows and signals it.


// Retired TS rollout modes are unsupported. Native startup is enforced even
// under the old ambient shadow flag; no-purchase reads/commands still deny.


// Original direct-intake/immutable-source/draining-settlement assertions now run
// through normal native sessions in selected_native_service_startup_customer_agreement_needs_no_finance_ledger.


// Native startup normal-session linked-Person case retains the original
// Work/Detail denials, no-invoice effect and Agreement/billing review assertions.


// Original direct-intake/immutable-source/draining-settlement assertions now run
// through normal native sessions in selected_native_service_startup_customer_agreement_needs_no_finance_ledger.


// Native startup direct-intake case preserves these source refusals and
// actual Support projection failure/atomic rollback/exact-key retry assertions.


// Native startup qualifies schedule/cancel, persisted Support projection and read-only flags.
// Original actual Support list/detail/Work/evidence/wrong-owner-write assertions now run
// against a native-created case in support/package-surfaces.test.ts.


// Native startup proves outbox was reached before expiry, complete orphan/ledger
// rollback, unavailable new-work view and actual dispatch under an expired active grant.


// Native startup direct-intake case preserves these source refusals and
// actual Support projection failure/atomic rollback/exact-key retry assertions.



// Native startup uses an actual native Agreement retire and proves competing
// intake waits for its writer before observing committed retirement, with zero effects.


// Native startup preserves real deferred outbox source/IAM fault execution,
// complete rollback, exact retry/draining replay and revoked-role replay refusal.


// Native startup runs two normal-session Company writers in the same Holding
// and checks exact case/order/ledger identity plus PostgreSQL deadlocks=0 after pool shutdown.



// Original parts/cost/billing and held journal-evidence assertions now execute
// through native private_field_cost_post_requires_qualified_source_and_real_flow_primary,
// service_only_visit_drains_and_hands_off_native_invoice_without_helpdesk and
// fully_covered_field_invoice_uses_native_independent_confirmation.


// Legacy-parts/addon, forged same-label and foreign invoice/journal link
// assertions now execute through native finance-work-source 'keeps native
// Field Finance history behind actual purchases and Company isolation'
// (missing-addon 403 / with-addon 200 / foreign-Company 403-404, 15.04s)
// and field_service_http_parity_postgres
// private_field_cost_post_requires_qualified_source_and_real_flow_primary
// forged-label GET+POST PACKAGE_REQUIRED with zero effects (14.57s).


// Original deferred reservation/current Finance authority post guards now run
// through native private_field_cost_post_requires_qualified_source_and_real_flow_primary.
// Full journal/reservation/assignment rollback, zero effects, original wire codes
// and same-key post/replay remain asserted against actual PostgreSQL.



// Original released-Money/later-FX/secondary-book/post-replay assertions now
// execute through native monetary and Service-only PostgreSQL variants. Actual
// retained generic product-entry invoice/primary/secondary/scope assertions run
// in finance-work-source.test.ts against native-created, posted documents.


// Native No-Charge cohort retains the original full-warranty draft status/total
// and exact draft:v1 amount_units/usd_units=0 before independent confirmation.



// Original pre-draft-snapshot billing is retained as explicit historical SQL
// fixture in finance-work-source.test.ts; native post/replay while draining
// preserves the original release quote and carried source after a later FX rate.



// Original bound-Asset private Files purchase/module/draining/revoked-role
// assertions now execute through native HTTP on an initially unpurchased
// retained-source fixture in engineering-business-attachments.test.ts (12.313s).


// Record/compatibility, module no-effect, exact Field/Support bind, draining
// replay/cancel and held-history assertions now run on fresh native sources in
// engineering-business-attachments.test.ts (27.690s before old producer removal).


// Native case moved without assertion changes to engineering-business-attachments.test.ts.


// Native case moved without assertion changes to engineering-business-attachments.test.ts.



// Original controlled cash, Finance-vs-Field settlement and inactive held History
// assertions now run through native finance_service_invoice_workspace_actual_payment_files
// (including actual Commercial/MFA Apply) and the native-produced Finance
// consumer in finance-work-source.test.ts (including generic bank purchase denial).

// Original generic/missing parent purchase denials and exact controlled bank
// mismatch/retarget assertions now run on native-produced sources in
// finance-work-source.test.ts (24.207s), alongside terminal legacy writer
// fences for known Field parents in every old enforcement mode.

// Original deferred cash party/current Finance authority rollback assertions
// now run through native HTTP in finance-work-source.test.ts, including full
// images/no effects, real sequence, same-key retry and replay after settlement
// while draining (24.087s). No legacy payment writer is restored.


// Withdrawal/settlement assertions now execute through native paths:
// five-owner reads plus HTTP preview/apply denial with a live native draft
// in native_no_charge_case (denied PREVIEW 409 PACKAGE_OBLIGATIONS_REMAIN,
// drain-with-duty, zero writes, draft reject/restore), the deferred-drift
// apply rollback with full grant/History/effects equality in
// native_service_commercial_withdrawal, raw obligations/scope/drift in
// native_service_package_obligations_preserve_raw_history_scope_and_unit
// (incl. unregistered-snapshot SCOPE_REQUIRED), asset/inventory settlements,
// service_only addon/base separation, the startup retire/intake lease
// barrier with revoked-role WORK_ROLE_UNAVAILABLE, and commercial
// lifecycle/concurrent grant review cases. Apply shares inspect()'s gate;
// its re-verification is proven by the drift rollback.

// Records/Inbox/Staff consumer assertions now execute through native paths:
// source-record reads via the qualified field-source-record-support-link flow,
// pre-limit hidden-row filtering/paging/cursors via
// native_field_inbox_candidates_keep_source_visibility_and_seek_past_hidden_rows,
// records commands with exact replay via
// selected_records_actor_executes_all_field_collaboration_commands_with_exact_replay,
// stale-context denials via current-context guards, order-source immutability
// via native CHECK guards, and customer-drift fail-closed via
// FIELD_SERVICE_CUSTOMERS_CHANGED (native_service_commercial_withdrawal).
// The TS reader branches in shared Records/Collaboration/Workflow adapters
// stay until native same-unit guards land in every caller (checklist 11).
// Executive consumer assertions now execute through native paths:
// visit evidence search/open, aggregates, cursor/hash/audit fields and
// permission-withdrawal/rollback via
// native_field_executive_evidence_keeps_cursor_audit_and_source_rollback
// (plus actual normal-browser consumer/audit evidence). Catalog gating and
// drilldown scope specifics stay with the TS metric readers until their
// native same-unit consumers land (checklist 11).


// Native case moved without assertion changes to engineering-business-attachments.test.ts.

// Native case moved without assertion changes to engineering-business-attachments.test.ts.


// Original strict Invoice source entry assertions now run on a genuinely
// initially unpurchased retained source and native Finance post in
// finance-work-source.test.ts; no legacy invoice producer is retained.


// No-Charge assertions now execute through native_no_charge_case (full-
// warranty draft, zero quote, ordinary-post denial with zero effects,
// independent confirm with exact replay, Money snapshots, no journals or
// allocations, obligations decrement, addon/base withdrawal, confirmed
// flag) and monetary_field_invoice_rejects_no_charge_without_mutating_its_draft
// (partial/non-warranty refusals); the TS confirm module has no non-test
// consumers (UI posts to the native confirm-no-charge route) and generic
// void behavior stays covered by Finance suites. No native void route
// exists, so the TS-only void combination is retired with the case.


// Native counterparts: native_no_charge_case and monetary_field_invoice_rejects_no_charge_without_mutating_its_draft.
// Preserve stale/forged review, current reader/preparer authority, linked-person
// independence, foreign Company, exact monetary drafts and zero denied effects.

// Deferred signoff/IAM rollback, full assignment images, lawful later Person
// linking, immutable confirmation replay and historical23514 mutation rejection
// execute in native_no_charge_case via fully_covered_field_invoice_uses_native_independent_confirmation.
// Native counterpart: fully_covered_field_invoice_uses_native_independent_confirmation.
// Exact competing receipts, two actual Company/source tuples and zero monetary
// artifacts; selected PostgreSQL server error window retains zero deadlock/serialization
// assertions (the runtime retries only SQLSTATE40001/40P01).

// Native counterpart: fully_covered_field_invoice_retains_actual_visit_files.
// Actual valued spare posting/balanced cost, empty Stock, supplier refusal,
// retained held bytes, native withdrawal/replay and history-only late bind refusal.

// Shared Finance/Recruit lease regression retained in finance-payment-obligations.test.ts.

// Native counterpart: fully_covered_field_invoice_uses_native_independent_confirmation.
// Exact competing receipts, two actual Company/source tuples and zero monetary
// artifacts; selected PostgreSQL server error window retains zero deadlock/serialization
// assertions (the runtime retries only SQLSTATE40001/40P01).



// Native counterpart: no_charge_native_routing, exercised by
// fully_covered_field_invoice_uses_native_independent_confirmation.
// Preserves actual reassignment/default rollback, exact replay, one Flow effect
// and unchanged package grants; the old TS business application is retired.


// Existing-route deferred primary/version rollback and exact-key recovery now
// execute through native no_charge_native_routing; no legacy command remains here.


// Native counterpart: no_charge_native_routing, exercised by
// fully_covered_field_invoice_uses_native_independent_confirmation.
// Preserves actual reassignment/default rollback, exact replay, one Flow effect
// and unchanged package grants; the old TS business application is retired.



// Additive Service-only native authority fixture; all prior assertions stay byte exact.
type ServicePartsFixtureActor=import("@erpx/server-identity_access/public/authorization").WorkContext;
async function zeroLedgerServicePartsCompany() {
  // Seed organization and actual native worker/assignment authority only.
  // This Company has NEVER had a chart/account/period/quote/settings row.
  // It is a disposable Company fixture, not normal Company onboarding proof.
  const company=`zero-service-parts-${key()}`,branch=`${company}-branch`,department=`${company}-department`;
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Zero ledger Service parts fixture',?)").bind(company,company,company),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Native services branch',?,?,?)").bind(branch,branch,company,company,branch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Native Service parts team',?,?,?)").bind(department,department,branch,company,branch),
  ]);
  async function actor(name:string,role:string,scopeKind:'holding'|'branch'|'department'='holding'):Promise<ServicePartsFixtureActor>{
    const userId=`${company}-${name}`,assignmentId=`assignment-${userId}`;
    await db.batch([
      db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind,branch_id,scope_node_id) VALUES(?,?,?,?,?,0,9999999999999,?,?,?)").bind(assignmentId,userId,role,JSON.stringify(['field_service']),company,scopeKind,scopeKind==='holding'?null:branch,scopeKind==='department'?department:scopeKind==='branch'?branch:null),
      db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${userId}`,userId,company,name),
      db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Native worker','THB','fixture',1,1)").bind(`eng-${userId}`,`person-${userId}`,company,branch,department),
    ]);
    const scope:ServicePartsFixtureActor['scope']=scopeKind==='department'?{kind:'department',holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',companyId:company,branchId:branch,departmentId:department,departmentSlug:department,departmentName:'Native Service parts team'}:scopeKind==='branch'?{kind:'branch',holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',companyId:company,branchId:branch,branchSlug:branch,branchName:'Native service branch'}:{kind:'holding',holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',companyId:company};
    const context:ServicePartsFixtureActor={userId,assignmentId,role,companyId:company,branchId:scopeKind==='holding'?null:branch,departmentId:scopeKind==='department'?department:null,holdingId:'h001',holdingSlug:'service-parts-test',holdingName:'Test',name,email:`${name}@example.test`,assignmentScope:scopeKind,inherited:false,modules:['field_service'],assignments:[],workspaces:[],scope};
    const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');
    context.iam=await loadIamAuthority(db,company,userId,role);
    return context;
  }
  const owner=await actor('owner','holding_owner'),reviewer=await actor('reviewer','finance_preparer'),manager=await actor('manager','finance_approver'),serviceReviewer=await actor('service-reviewer','holding_owner'),technician=await actor('technician','employee');
  const {installStaffLevels,savePosition,positionSchema}=await import('@erpx/server-organization/public/organization');
  await installStaffLevels(db,owner,key());
  const ceo=String((await savePosition(db,owner,null,positionSchema.parse({title:'Fixture accountable owner',levelId:`org-level-${company}-1`,branchId:null,departmentId:null,isCeo:true,reportsToPositionId:null,expectedVersion:0,reason:'Native organizational authority fixture'}),key())).id);
  for(const [member,rank] of [[owner,1],[reviewer,5],[manager,4],[serviceReviewer,3],[technician,4]] as const){
    const position=rank===1?ceo:String((await savePosition(db,owner,null,positionSchema.parse({title:`Fixture ${member.role}`,levelId:`org-level-${company}-${rank}`,branchId:null,departmentId:null,reportsToPositionId:ceo,expectedVersion:0,reason:'Native selected worker authority fixture'}),key())).id);
    await db.prepare('UPDATE hr_engagements SET level_id=?,position_id=?,manager_person_id=? WHERE id=?').bind(`org-level-${company}-${rank}`,position,rank===1?null:`person-${owner.userId}`,`eng-${member.userId}`).run();
  }
  await db.prepare("INSERT INTO authority_seats(company_id,kind,scope_id,version,position_id,person_id,effective_at) VALUES(?,'ceo',?,1,?,?,1) ON CONFLICT(company_id,kind,scope_id) DO UPDATE SET version=1,position_id=excluded.position_id,person_id=excluded.person_id,effective_at=1").bind(company,company,ceo,`person-${owner.userId}`).run();
  const counts=()=>Promise.all(['finance_charts','finance_accounts','finance_periods','company_fx_rates','commerce_settings','commerce_orders','finance_journals','finance_invoices','commerce_warehouses','commerce_products'].map(table=>db.prepare(`SELECT count(*) n FROM ${table} WHERE company_id=?`).bind(company).first<number>('n')));
  expect(await counts()).toEqual([0,0,0,0,0,0,0,0,0,0]);
  // Normal packageStore is an explicit isolated provider fixture port, never
  // installed Commercial authority or normal Company onboarding acceptance.
  const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-Service-parts-provider'}),obligations:async()=>{throw Error('Service parts fixture cannot withdraw');}});
  async function purchase(){for(const packageId of ['service','service-parts-assets']){if(await db.prepare("SELECT state FROM company_package_grants WHERE company_id=? AND package_id=?").bind(company,packageId).first('state')==='active')continue;await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Declared native Service-only parts startup',evidence:'test:Service-parts-fresh-Company'}});}}
  vi.stubEnv('ERPX_SERVICE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_SUPPORT_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
  for(const person of [owner,reviewer,manager,serviceReviewer,technician]){const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');person.iam=await loadIamAuthority(db,company,person.userId,person.role);}
  return {company,branch,department,owner,reviewer,manager,serviceReviewer,technician,counts,purchase,store,actor};
}

type PartsCompany=Awaited<ReturnType<typeof zeroLedgerServicePartsCompany>>;
type PartsCommand<T>=(actor:ServicePartsFixtureActor,body:unknown,command:string)=>Promise<T>;
type PartsPorts={
 finance:{servicePartsPrerequisites:(actor:ServicePartsFixtureActor)=>Promise<import('@erpx/feature-field_service/lib/parts-setup-command').ServicePartsPrerequisitesView>;createServicePartsChart:PartsCommand<{id:string}>;createServicePartsAccount:PartsCommand<{id:string}>;createServicePartsPeriod:PartsCommand<{id:string}>;confirmServicePartsRate:PartsCommand<{id:string;currency:string;effectiveDate:string;currencyPerUsd:string;confirmedAt:number}>};
 setup:{servicePartsPostingSetup:(actor:ServicePartsFixtureActor)=>Promise<import('@erpx/feature-field_service/lib/parts-setup-command').ServicePartsView['mapping']>;initializeServicePartsPostingSetup:PartsCommand<{id:string;status:'configured'|'confirmed-existing';version:1}>;confirmExistingServicePartsPostingSetup:PartsCommand<{id:string;status:'configured'|'confirmed-existing';version:1}>};
 masters:{createServiceInitialPhysicalStockProduct:PartsCommand<{id:string;version:1}>;createServiceInitialPhysicalStockWarehouse:PartsCommand<{id:string;version:1}>};
};
async function withMixedPartsFixture<T>(run:(z:PartsCompany,ports:PartsPorts)=>Promise<T>):Promise<T>{
 const saved={db,serviceDatabase};let company:PartsCompany|undefined;
 try{return await withNativeFieldFixture(async native=>{
  if(!company)throw new Error('MIXED_PARTS_COMPANY_MISSING');const z=company,base='/api/v1/holdings/acme/field-service/parts-setup';
  const {loadIamAuthority}=await import('@erpx/server-identity_access/public/iam-store');for(const actor of [z.owner,z.reviewer,z.manager,z.serviceReviewer,z.technician])actor.iam=await loadIamAuthority(db,z.company,actor.userId,actor.role);
  const labels=new Map([[z.owner.userId,'owner'],[z.reviewer.userId,'reviewer'],[z.manager.userId,'manager'],[z.serviceReviewer.userId,'serviceReviewer'],[z.technician.userId,'technician']]);
  async function call<T>(actor:ServicePartsFixtureActor,method:string,path:string,body?:unknown,command?:string):Promise<T>{const label=labels.get(actor.userId);if(!label)throw new Error('MIXED_PARTS_ACTOR_MISSING');const response=await native.requestAs<T>(label,method,base+path,body,command);if(response.status!==200)throw Object.assign(new Error(response.json?.error?.message??'Native parts operation failed'),{status:response.status,code:response.json?.error?.code});return response.json.data;}
  const finance={servicePartsPrerequisites:(actor:ServicePartsFixtureActor)=>call(actor,'GET','/prerequisites'),createServicePartsChart:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','/charts',body,command),createServicePartsAccount:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','/accounts',body,command),createServicePartsPeriod:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','/periods',body,command),confirmServicePartsRate:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','/rates',body,command)} as PartsPorts['finance'];
  const setup={servicePartsPostingSetup:(actor:ServicePartsFixtureActor)=>call(actor,'GET',''),initializeServicePartsPostingSetup:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','',body,command),confirmExistingServicePartsPostingSetup:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','/existing',body,command)} as PartsPorts['setup'];
  const masters={createServiceInitialPhysicalStockProduct:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','/masters/products',body,command),createServiceInitialPhysicalStockWarehouse:(actor:ServicePartsFixtureActor,body:unknown,command:string)=>call(actor,'POST','/masters/warehouses',body,command)} as PartsPorts['masters'];
  return run(z,{finance,setup,masters});
 },async nativeDb=>{db=nativeDb;serviceDatabase=nativeDb;company=await zeroLedgerServicePartsCompany();const actors=[];for(const [label,actor] of [['owner',company.owner],['reviewer',company.reviewer],['manager',company.manager],['serviceReviewer',company.serviceReviewer],['technician',company.technician]] as const){const selected=await nativeDb.prepare('SELECT version,role FROM assignments WHERE id=? AND user_id=? AND company_id=?').bind(actor.assignmentId,actor.userId,company.company).first<{version:number;role:string}>();if(!selected)throw new Error('MIXED_PARTS_ASSIGNMENT_MISSING');actors.push({label,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:Number(selected.version),role:selected.role,modules:actor.modules,branchId:null,departmentId:null});}return {companyId:company.company,actors};},'parts');}finally{({db,serviceDatabase}=saved);}
}

async function zeroLedgerServicePartsSource({z,ports}:{z:PartsCompany;ports:PartsPorts}){
 const {finance,setup,masters}=ports;
expect(await finance.servicePartsPrerequisites(z.reviewer)).toMatchObject({available:true,charts:[],accounts:[]});await z.purchase();
 const envelope=async()=>{const view=await finance.servicePartsPrerequisites(z.reviewer);expect(view.available).toBe(true);if(!view.contextKey)throw Error('Real current Finance prerequisite context required');return {expectedVersion:0 as const,contextKey:view.contextKey};};
 const chartInput={...await envelope(),input:{name:'Actual Service parts chart',baseCurrency:'THB',accountingBasis:'TFRS'}},chartKey=key(),chart=await finance.createServicePartsChart(z.reviewer,chartInput,chartKey);expect(await finance.createServicePartsChart(z.reviewer,chartInput,chartKey)).toEqual(chart);const accounts:Record<string,string>={};
 for(const [name,code,category,controlKind] of [['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank'],['variance','5400','expense','none']] as const){const created=await finance.createServicePartsAccount(z.reviewer,{...await envelope(),input:{chartId:chart.id,code,name:`Actual Service parts ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());accounts[name]=created.id;}
 const period=await finance.createServicePartsPeriod(z.reviewer,{...await envelope(),input:{name:'Actual Service current period',startDate:'2020-01-01',endDate:'2035-12-31'}},key()),rateSeed=await finance.servicePartsPrerequisites(z.manager);if(!rateSeed.contextKey)throw Error('Actual Company rate authority required');await expect(finance.confirmServicePartsRate(z.reviewer,{expectedVersion:0,contextKey:rateSeed.contextKey,input:{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Preparer cannot confirm own authority'}},key())).rejects.toMatchObject({status:403});await finance.confirmServicePartsRate(z.manager,{expectedVersion:0,contextKey:rateSeed.contextKey,input:{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Independent actual Company Finance rate confirmation'}},key());
 const view=await setup.servicePartsPostingSetup(z.reviewer);expect(view).toMatchObject({available:true,configured:false,sourceReady:false,schemaAvailable:true,canConfigure:true});if(!view.contextKey)throw Error('Actual Service parts mapping context required');const mappingKey=key(),mapping={expectedVersion:0 as const,contextKey:view.contextKey,chartId:chart.id,currency:'THB',receivableAccountId:accounts.ar,payableAccountId:accounts.ap,revenueAccountId:accounts.revenue,inventoryAccountId:accounts.inventory,clearingAccountId:accounts.clearing,cogsAccountId:accounts.cogs,inputTaxAccountId:accounts.inputTax,outputTaxAccountId:accounts.outputTax,cashAccountId:accounts.cash,reason:'Deliberately select all nine actual Service-only posting roles'},configured=await setup.initializeServicePartsPostingSetup(z.reviewer,mapping,mappingKey);expect(configured).toEqual({id:z.company,status:'configured',version:1});expect(await setup.initializeServicePartsPostingSetup(z.reviewer,mapping,mappingKey)).toEqual(configured);
 const warehouse=await masters.createServiceInitialPhysicalStockWarehouse(z.owner,{branchId:z.branch,code:'PARTS',name:'Actual first Service parts warehouse',status:'active'},key()),product=await masters.createServiceInitialPhysicalStockProduct(z.owner,{sku:'FIRST-PART',name:'Actual physically observed Service spare',itemType:'stock',trackingMode:'none',unit:'piece',price:'100',taxBps:0,status:'active'},key());
 const snapshot=async()=>Object.fromEntries(await Promise.all(['commerce_settings','commerce_service_parts_posting_sources','inventory_service_initial_physical_sources','inventory_counts','inventory_count_lines','inventory_count_lot_lines','inventory_count_decisions','inventory_adjustment_moves','inventory_ledger','inventory_lots','inventory_lot_balances','inventory_lot_moves','commerce_stock','commerce_stock_moves','finance_journals','finance_journal_lines','finance_book_postings','finance_financial_events','money_transaction_snapshots','command_receipts','command_assertions','audit_events','outbox_events','commerce_products','assignments'].map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')])));
 expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,product.id).first('n')).toBe(0);expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
 return {z,finance,setup,masters,chart,accounts,period,mapping,mappingKey,warehouse,product,snapshot,input:{warehouseId:warehouse.id,countDate:new Date().toISOString().slice(0,10),reason:'Actual first physical Service spare observation, current-period variance only',lines:[{productId:product.id,countedQuantity:'2',unitCost:'6',allocations:[]}]}};
}
it('HISTORICAL COMPATIBILITY: Service-only fresh parts setup independently posts the first physical spare with exact stock/journal evidence',async()=>{
 await withMixedPartsFixture(async(z,ports)=>{
 const s=await zeroLedgerServicePartsSource({z,ports}),initial=await historicalServiceCount(),counts=await import('@erpx/server-inventory/public/inventory-counts'),before=await s.snapshot();
 expect(counts.canPrepareCounts(s.z.owner)).toBe(true);expect(counts.canApproveCounts(s.z.manager)).toBe(true);await expect(counts.saveCount(s.z.owner,null,s.input,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await s.snapshot()).toEqual(before);
 const selection={warehouseId:s.warehouse.id,productId:s.product.id,countDate:s.input.countDate},seed=await initial.serviceInitialPhysicalStockCountWorkspace(s.z.owner,selection);expect(seed).toMatchObject({available:true,canPrepare:true,readiness:{schemaAvailable:true,postingConfigured:true,varianceAccountAvailable:true,postingPeriodAvailable:true}});if(!seed.contextKey)throw Error('Actual Service first Count context required');const command=key(),body={expectedVersion:0 as const,contextKey:seed.contextKey,input:s.input},created=await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command),id=String(created.id);expect(created).toEqual({id:`count-${command}`,status:'draft',version:1});expect(await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).toEqual(created);
 const {enableAttachmentV2ForTest,seedAttachmentSet}=await import('../../support/helpers/attachment-v2');enableAttachmentV2ForTest();const sheet=await seedAttachmentSet(db,s.z.owner,{targetType:'stock_count',targetId:id,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await initial.transitionServiceInitialPhysicalStockCount(s.z.owner,id,'submit',{expectedVersion:1,attachmentSetIds:[sheet.setId]},key());
 for(const [step,version] of [['submitted',2],['approved',3]] as const){const flow=await initial.serviceInitialPhysicalStockCountFlow(s.z.owner,id);await initial.assignServiceInitialPhysicalStockCountFlow(s.z.owner,id,{stepKey:step,userId:s.z.manager.userId,expectedVersion:flow.version,recordVersion:version,reason:'Original Company Flow authority chooses independent real Stock reviewer',saveDefault:false},key());if(step==='submitted')await initial.transitionServiceInitialPhysicalStockCount(s.z.manager,id,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Independently verify the actual physical spare quantity and unit cost'},key());}
 const postKey=key(),postBody={expectedVersion:3,attachmentSetIds:[],reason:'Current-period physical variance, not opening equity or a purchase'},posted=await initial.transitionServiceInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey);expect(posted).toMatchObject({id,status:'posted',version:4});expect(await initial.transitionServiceInitialPhysicalStockCount(s.z.manager,id,'post',postBody,postKey)).toEqual(posted);const detail=await initial.serviceInitialPhysicalStockCountWorkspace(s.z.reviewer,{countId:id}),journal=detail.record?.journalId;if(!journal)throw Error('Actual Count posted journal required');expect(detail.record).toMatchObject({id,sourceKind:'service-initial-physical-stock-count',status:'posted',version:4});
 expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first()).toEqual({quantity_milli:2000,value_minor:1200});expect(await db.prepare('SELECT source_type,source_id,status,is_opening FROM finance_journals WHERE id=? AND company_id=?').bind(journal,s.z.company).first()).toEqual({source_type:'stock_count',source_id:id,status:'posted',is_opening:0});expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=?').bind(journal).all()).results).toEqual(expect.arrayContaining([{account_id:s.accounts.inventory,debit_minor:1200,credit_minor:0},{account_id:s.accounts.variance,debit_minor:0,credit_minor:1200}]));
 const {journalDetail,ledgerWorkspace}=await import('@erpx/server-finance/public/ledger');expect((await journalDetail(s.z.reviewer,journal)).journal.id).toBe(journal);await expect(ledgerWorkspace(s.z.reviewer,s.chart.id)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 // Spare consumption, reviewed cost posting and billing release now execute
 // through native service_only_visit and fully_covered native invoice flows;
 // the TS Field command producers are retired (no legacy invoice producer).
});
},120000);

it('HISTORICAL COMPATIBILITY: Service first physical Count refuses stale and foreign selections and rolls a deferred real master-source fault back before an exact-key retry',async()=>{
 await withMixedPartsFixture(async(z,ports)=>{
 const s=await zeroLedgerServicePartsSource({z,ports}),initial=await historicalServiceCount(),selection={warehouseId:s.warehouse.id,productId:s.product.id,countDate:s.input.countDate},seed=await initial.serviceInitialPhysicalStockCountWorkspace(s.z.owner,selection);if(!seed.contextKey)throw Error('Real selected Service Count source required');const command=key(),body={expectedVersion:0 as const,contextKey:seed.contextKey,input:s.input},before=await s.snapshot();
 await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,{...body,contextKey:'a'.repeat(64)},key())).rejects.toMatchObject({status:409,code:'SERVICE_INITIAL_PHYSICAL_STOCK_CONTEXT_CHANGED'});expect(await s.snapshot()).toEqual(before);
 // The owned native fixture's persisted warehouse belongs to another Company.
 const foreignWarehouse='warehouse';expect(await db.prepare('SELECT company_id FROM commerce_warehouses WHERE id=?').bind(foreignWarehouse).first('company_id')).toBe('company');expect(z.company).not.toBe('company');
 await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,{...body,input:{...s.input,warehouseId:foreignWarehouse}},key())).rejects.toMatchObject({status:404,code:'SERVICE_INITIAL_PHYSICAL_STOCK_SELECTION_UNAVAILABLE'});expect(await s.snapshot()).toEqual(before);
 const fault=`service_first_count_${key().replaceAll('-','')}`;
 // Native fault only: a real master row changes after command effects, inside
 // the owned transaction. The complete rollback restores that row as well.
 await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${s.z.company}' THEN UPDATE commerce_products SET name=name||' late source drift' WHERE id='${s.product.id}'; END IF;RETURN NULL;END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON inventory_service_initial_physical_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`).run();
 try{await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).rejects.toMatchObject({status:409,code:'SERVICE_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED'});expect(await s.snapshot()).toEqual(before);}finally{await db.prepare(`DROP TRIGGER ${fault} ON inventory_service_initial_physical_sources`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 const created=await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command);expect(created).toEqual({id:`count-${command}`,status:'draft',version:1});expect(await initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).toEqual(created);
 const cancelled=await initial.transitionServiceInitialPhysicalStockCount(s.z.owner,String(created.id),'cancel',{expectedVersion:1,attachmentSetIds:[],reason:'Normal cancellation leaves no synthetic positive Stock'},key());expect(cancelled).toMatchObject({status:'cancelled',version:2,journalId:null});expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(s.warehouse.id,s.product.id).first('n')).toBe(0);
 const after=await s.snapshot();await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(s.z.owner.assignmentId).run();await expect(initial.prepareServiceInitialPhysicalStockCount(s.z.owner,body,command)).rejects.toMatchObject({status:403,code:'QUEUED_AUTHORITY_CHANGED'});expect(await db.prepare('SELECT status,version FROM inventory_counts WHERE id=?').bind(created.id).first()).toEqual({status:'cancelled',version:2});expect(await db.prepare('SELECT count(*)::int n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(1);const {assignments:beforeAuthority,...beforeUnchanged}=after,{assignments:revokedAuthority,...afterUnchanged}=await s.snapshot();expect(afterUnchanged).toEqual(beforeUnchanged);expect(revokedAuthority).not.toBe(beforeAuthority); });
},120000);

it('HISTORICAL COMPATIBILITY: Service confirms an unchanged historical Supply mapping after normal Supply withdrawal and owns only a new absent-part physical Count',async()=>{
 // The separate browser fixture uses this same owner path on a distinct lease.
 await withMixedPartsFixture(async z=>{const {seedHistoricalSupplyMapping}=await import('../../support/field_service/historical-supply-parts');const prepared=await seedHistoricalSupplyMapping(db,z);expect(prepared.mappingCommandId).toBeTruthy();expect(prepared.mappingImage).toBeTruthy();});
 await withMixedPartsFixture(async(z,ports)=>{
 const finance=await import('@erpx/server-finance/public/finance'),money=await import('@erpx/server-finance/public/company-money'),supply=await import('@erpx/server-commerce/public/posting-setup'),commerce=await import('@erpx/server-commerce/public/commerce'),inventory=await import('@erpx/server-inventory/public/inventory-counts'),setup=ports.setup,initial=await historicalServiceCount(),masters=ports.masters;
 const {packageGrantRevision,readCompanyPackageHistoryRights}=await import('@erpx/server-commercial/public/package-grants'),{withOwnerPackageObligations,ownerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');
 // Explicit historical fixture scope: this Company really bought Supply first.
 // Its old owner creates all business rows; it ends through actual owner-registry
 // review before Service confirmation. This is not a never-Supply Company claim.
 const grant={companyId:z.company,packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Actual original Supply mapping and physical observation before Service adoption',evidence:'test:historical-Service-parts-map'},provider=packageGrantStore(db,{authorize:async()=>({userId:'isolated-Service-existing-map-provider'}),obligations:(transaction,companyId,packageId)=>ownerPackageObligations(db,transaction,companyId,packageId)});await provider.apply({expectedVersion:0,idempotencyKey:key(),grant});
 const chart=await finance.createChart(z.reviewer,{name:'Actual retained original Supply chart',baseCurrency:'THB',accountingBasis:'TFRS'},key()),accounts:Record<string,string>={};
 for(const [field,code,category,controlKind] of [['receivableAccountId','1100','asset','receivable'],['payableAccountId','2100','liability','payable'],['revenueAccountId','4100','revenue','none'],['inventoryAccountId','1300','asset','none'],['clearingAccountId','2150','liability','none'],['cogsAccountId','5100','expense','none'],['inputTaxAccountId','1150','asset','none'],['outputTaxAccountId','2151','liability','none'],['cashAccountId','1001','asset','bank'],['varianceAccountId','5400','expense','none']] as const)accounts[field]=String((await finance.createAccount(z.reviewer,{chartId:chart.id,code,name:`Actual original retained ${field}`,category,currency:'THB',isControl:controlKind!=='none',controlKind},key())).id);
 await finance.createPeriod(z.reviewer,{name:'Actual original Supply observation year',startDate:'2020-01-01',endDate:'2035-12-31'},key());await money.confirmManualRate(z.manager,{currency:'THB',effectiveDate:'2020-01-01',currencyPerUsd:'35',note:'Actual independent Company quote before Service source confirmation'},key());
 const preview=await supply.commercePostingSetup(z.reviewer);if(!preview.contextKey)throw Error('Actual original shared Commerce mapping context required');const nine=Object.fromEntries(Object.entries(accounts).filter(([field])=>field!=='varianceAccountId')),mappingBody={expectedVersion:0 as const,contextKey:preview.contextKey,chartId:chart.id,currency:'THB',reason:'Original Company owner selects the immutable shared nine-role mapping',...nine},mappingKey=key();expect(await supply.initializeCommercePostingSetup(z.reviewer,mappingBody,mappingKey)).toEqual({id:z.company,status:'configured',version:1});
 const warehouse=await commerce.saveMaster(z.owner,'warehouses',null,{branchId:z.branch,code:key(),name:'Actual retained original Supply warehouse'},key()),oldProduct=await commerce.saveMaster(z.owner,'products',null,{sku:key(),name:'Actual original independently observed spare',itemType:'stock',price:'6',taxBps:0},key()),countDate=new Date().toISOString().slice(0,10),oldKey=key(),oldInput=inventory.countSchema.parse({warehouseId:String(warehouse.id),countDate,reason:'Original Supply current-period physical observation, never opening equity',lines:[{productId:String(oldProduct.id),countedQuantity:'2',unitCost:'6'}]}),oldCount=await inventory.saveCount(z.owner,null,oldInput,oldKey),oldId=String(oldCount.id);
 const {enableAttachmentV2ForTest,seedAttachmentSet}=await import('../../support/helpers/attachment-v2');enableAttachmentV2ForTest();const oldSheet=await seedAttachmentSet(db,z.owner,{targetType:'stock_count',targetId:oldId,componentKind:'action',componentRef:'submit',purposeKey:'count_sheet',expectedTargetVersion:1});await inventory.transitionCount(z.owner,oldId,'submit',{expectedVersion:1,attachmentSetIds:[oldSheet.setId]},key());
 for(const [step,version] of [['submitted',2],['approved',3]] as const){const flow=await resolveFlow(db,z.owner,'stock_count',oldId);await assignFlowPrimary(db,z.owner,'stock_count',oldId,{stepKey:step,userId:z.manager.userId,expectedVersion:flow.version,recordVersion:version,reason:'Original Company Flow chooses the real different Stock reviewer',saveDefault:false},key());if(step==='submitted')await inventory.transitionCount(z.manager,oldId,'approve',{expectedVersion:2,attachmentSetIds:[],reason:'Original independent observation confirms two real physical units'},key());}
 await inventory.transitionCount(z.manager,oldId,'post',{expectedVersion:3,attachmentSetIds:[],reason:'Original Supply posts real current-period inventory variance'},key());expect(await db.prepare('SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,oldProduct.id).first()).toEqual({quantity_milli:2000,value_minor:1200});const originalCreate=JSON.parse(String(await db.prepare('SELECT result_json FROM command_receipts WHERE command_id=?').bind(oldKey).first('result_json')));expect(originalCreate).not.toHaveProperty('_serviceInitialPhysicalStockCount');
 const oldJournal=String(await db.prepare('SELECT journal_id FROM inventory_counts WHERE id=?').bind(oldId).first('journal_id')),retained=async()=>({mapping:await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?').bind(z.company).first<string>('image'),chart:await db.prepare('SELECT to_jsonb(c)::text image FROM finance_charts c WHERE id=?').bind(chart.id).first<string>('image'),accounts:(await db.prepare('SELECT to_jsonb(a)::text image FROM finance_accounts a WHERE company_id=? ORDER BY id').bind(z.company).all()).results,defaults:(await db.prepare('SELECT to_jsonb(d)::text image FROM finance_sales_defaults d WHERE company_id=? ORDER BY id').bind(z.company).all()).results,count:await db.prepare('SELECT to_jsonb(c)::text image FROM inventory_counts c WHERE id=?').bind(oldId).first<string>('image'),lines:(await db.prepare('SELECT to_jsonb(l)::text image FROM inventory_count_lines l WHERE count_id=? ORDER BY id').bind(oldId).all()).results,decisions:(await db.prepare('SELECT to_jsonb(d)::text image FROM inventory_count_decisions d WHERE count_id=? ORDER BY id').bind(oldId).all()).results,journal:await db.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE id=?').bind(oldJournal).first<string>('image'),journalLines:(await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE journal_id=? ORDER BY id').bind(oldJournal).all()).results,stock:await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_stock s WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,oldProduct.id).first<string>('image'),receipt:await db.prepare('SELECT to_jsonb(c)::text image FROM command_receipts c WHERE command_id=?').bind(oldKey).first<string>('image')}),original=await retained();
 // Real Preview/Apply, current versions and actual all-owner settlement readers.
 // Positive terminal Stock is retained as a revision fact, not deleted or seeded.
 const settlement={companyId:z.company,packageId:'supply',packageVersion:1};for(const [state,expectedVersion] of [['draining',1],['inactive',2]] as const){const grants=await readCompanyPackageGrants(db,z.company),request={companyId:z.company,expectedRevision:packageGrantRevision(z.company,grants),idempotencyKey:key(),changes:[{expectedVersion,grant:{...grant,state,version:expectedVersion+1,reason:state==='draining'?'Stop new Supply work before normal original owner settlement':'Actual Supply obligations are terminal; retain original stock and history',evidence:'test:real-Service-existing-map-Supply-settlement'}}]},decision=await withOwnerPackageObligations(db,settlement,async transaction=>{expect((await readOwnerPackageObligations(db,transaction,{...settlement,now:Date.now()})).total).toBe(0);return provider.batch.previewIn(transaction,request);});expect(await withOwnerPackageObligations(db,settlement,transaction=>provider.batch.applyIn(transaction,request,async current=>{expect(current).toEqual(decision);}))).toEqual(decision);}
 await z.purchase();expect((await readCompanyPackageGrants(db,z.company)).filter(row=>row.state==='active').map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);expect((await readCompanyPackageHistoryRights(db,z.company)).some(row=>row.packageId==='supply')).toBe(true);expect(await retained()).toEqual(original);
 const seed=await setup.servicePartsPostingSetup(z.reviewer);expect(seed).toMatchObject({available:true,schemaAvailable:true,configured:true,sourceReady:false,version:0,canConfigure:false,canConfirmExisting:true});if(!seed.contextKey)throw Error('Actual unchanged existing mapping confirmation context required');const body={expectedVersion:0 as const,contextKey:seed.contextKey,reason:'Current Company Finance deliberately confirms the actual historical nine-role mapping'},confirmKey=key(),noEffects=async()=>({old:await retained(),sources:(await db.prepare('SELECT to_jsonb(s)::text image FROM commerce_service_parts_posting_sources s WHERE company_id=?').bind(z.company).all()).results,commands:await Promise.all(['command_receipts','audit_events','outbox_events','command_assertions'].map(async table=>(await db.prepare(`SELECT to_jsonb(c)::text image FROM ${table} c WHERE command_id=? ORDER BY image`).bind(confirmKey).all()).results))}),before=await noEffects();
 await expect(setup.initializeServicePartsPostingSetup(z.reviewer,{...mappingBody,contextKey:seed.contextKey},key())).rejects.toMatchObject({status:409,code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});expect(await noEffects()).toEqual(before);await expect(setup.confirmExistingServicePartsPostingSetup(z.reviewer,{...body,contextKey:'a'.repeat(64)},confirmKey)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_CONTEXT_CHANGED'});expect(await noEffects()).toEqual(before);
 const confirmed=await setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,confirmKey);expect(confirmed).toEqual({id:z.company,status:'confirmed-existing',version:1});expect(Object.keys(confirmed).sort()).toEqual(['id','status','version']);expect(await retained()).toEqual(original);const after=await noEffects();expect(await setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await noEffects()).toEqual(after);await expect(setup.confirmExistingServicePartsPostingSetup(z.reviewer,{...body,reason:'Changed input cannot replace the committed actual confirmation'},confirmKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});await expect(setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,key())).rejects.toMatchObject({code:'SERVICE_PARTS_ALREADY_CONFIGURED'});expect(await noEffects()).toEqual(after);
 expect(await db.prepare("SELECT count(*)::int n FROM command_receipts WHERE record_id=? AND operation='commerce.service-parts-posting-setup.initialize'").bind(z.company).first('n')).toBe(0);const proof=await db.prepare('SELECT mode,proof_json::text proof,mapping_json::text mapping FROM commerce_service_parts_posting_sources WHERE company_id=?').bind(z.company).first<{mode:string;proof:string;mapping:string}>();if(!proof)throw Error('Actual distinct existing confirmation provenance required');expect(proof.mode).toBe('confirm-existing');expect(JSON.parse(proof.mapping)).toEqual(JSON.parse(original.mapping!));expect(JSON.parse(proof.proof)).toMatchObject({kind:'service-parts-posting-setup',commandId:confirmKey,mode:'confirm-existing'});
 const nextProduct=await masters.createServiceInitialPhysicalStockProduct(z.owner,{sku:'SERVICE-ABSENT',name:'Actual new absent Service spare',itemType:'stock',trackingMode:'none',unit:'piece',price:'100',taxBps:0,status:'active'},key()),input={warehouseId:String(warehouse.id),countDate,reason:'New absent Service spare follows original physical Count source',lines:[{productId:nextProduct.id,countedQuantity:'1',unitCost:'7',allocations:[]}]},selection={warehouseId:String(warehouse.id),productId:nextProduct.id,countDate};expect(await db.prepare('SELECT count(*)::int n FROM commerce_stock WHERE warehouse_id=? AND product_id=?').bind(warehouse.id,nextProduct.id).first('n')).toBe(0);const countSeed=await initial.serviceInitialPhysicalStockCountWorkspace(z.owner,selection);expect(countSeed.canPrepare).toBe(true);if(!countSeed.contextKey)throw Error('Actual new Service Count selected context required');const nextKey=key(),nextBody={expectedVersion:0 as const,contextKey:countSeed.contextKey,input},next=await initial.prepareServiceInitialPhysicalStockCount(z.owner,nextBody,nextKey);expect(next).toEqual({id:`count-${nextKey}`,status:'draft',version:1});const nativeProof=JSON.parse(String(await db.prepare('SELECT proof_json FROM inventory_service_initial_physical_sources WHERE count_id=?').bind(next.id).first('proof_json')));expect(nativeProof).toMatchObject({kind:'service-initial-physical-stock-count',serviceSetupCommandId:confirmKey,chartId:chart.id,inventoryAccountId:accounts.inventoryAccountId});
 await initial.transitionServiceInitialPhysicalStockCount(z.owner,String(next.id),'cancel',{expectedVersion:1,attachmentSetIds:[],reason:'Normal retained draft cancellation leaves the old physical stock untouched'},key());expect(await retained()).toEqual(original);expect(await setup.confirmExistingServicePartsPostingSetup(z.reviewer,body,confirmKey)).toEqual(confirmed);expect(await initial.prepareServiceInitialPhysicalStockCount(z.owner,nextBody,nextKey)).toEqual(next);expect((await initial.serviceInitialPhysicalStockCountWorkspace(z.owner,{countId:String(next.id)})).record).toMatchObject({status:'cancelled',version:2});expect((await readCompanyPackageGrants(db,z.company)).filter(row=>row.state==='active').map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']); });
},120000);

it('Service mapping rolls a deferred actual Finance reference change back before an exact-body and key retry',async()=>{
 await withMixedPartsFixture(async(z,{finance,setup})=>{await z.purchase();
 const envelope=async()=>{const view=await finance.servicePartsPrerequisites(z.reviewer);expect(view.available).toBe(true);if(!view.contextKey)throw Error('Actual Service Finance prerequisite context required');return {expectedVersion:0 as const,contextKey:view.contextKey};};
 const chart=await finance.createServicePartsChart(z.reviewer,{...await envelope(),input:{name:'Actual Service mapping reference finality chart',baseCurrency:'THB',accountingBasis:'TFRS'}},key()),accounts:Record<string,string>={};
 for(const [name,code,category,controlKind] of [['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank']] as const){const account=await finance.createServicePartsAccount(z.reviewer,{...await envelope(),input:{chartId:chart.id,code,name:`Actual Service mapping ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key());accounts[name]=account.id;}
 const seed=await setup.servicePartsPostingSetup(z.reviewer);expect(seed).toMatchObject({available:true,schemaAvailable:true,configured:false,sourceReady:false,canConfigure:true});if(!seed.contextKey)throw Error('Actual fresh Service mapping context required');
 const command=key(),body={expectedVersion:0 as const,contextKey:seed.contextKey,chartId:chart.id,currency:'THB',receivableAccountId:accounts.ar,payableAccountId:accounts.ap,revenueAccountId:accounts.revenue,inventoryAccountId:accounts.inventory,clearingAccountId:accounts.clearing,cogsAccountId:accounts.cogs,inputTaxAccountId:accounts.inputTax,outputTaxAccountId:accounts.outputTax,cashAccountId:accounts.cash,reason:'Current Finance explicitly selects the actual nine original Service posting roles'};
 const image=async()=>Object.fromEntries(await Promise.all(['finance_charts','finance_accounts','company_money_settings','company_currency_scales','company_fx_rates','commerce_settings','commerce_service_parts_posting_sources','finance_journals','finance_journal_lines','finance_book_postings','finance_financial_events','money_transaction_snapshots','command_receipts','command_assertions','audit_events','outbox_events'].map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')]))),before=await image(),fault=`service_mapping_finance_${key().replaceAll('-','')}`;
 // Native diagnostic fault only: the real chart changes after source effects
 // inside the owned command. No positive Finance or Stock row is SQL-seeded.
 await db.exec(`CREATE SEQUENCE ${fault}_attempt;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${z.company}' THEN PERFORM nextval('${fault}_attempt');UPDATE finance_charts SET name=name||' deferred reference drift' WHERE id='${chart.id}' AND company_id='${z.company}';END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON commerce_service_parts_posting_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
 try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,body,command)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first('is_called')).toBe(true);expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON commerce_service_parts_posting_sources;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}_attempt`);}
 const deleteFault=`service_mapping_delete_receipt_${key().replaceAll('-','')}`;
 // The ledger's real assertion cleanup occurs after the original receipt is
 // created, even when no assertion rows exist. Its statement trigger must not
 // alter the sealed receipt before final validation.
 await db.exec(`CREATE SEQUENCE ${deleteFault}_attempt;CREATE FUNCTION ${deleteFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF EXISTS(SELECT 1 FROM command_receipts WHERE command_id='${command}') THEN PERFORM nextval('${deleteFault}_attempt');UPDATE command_receipts SET created_at=created_at+1 WHERE command_id='${command}';END IF;RETURN NULL;END $$;CREATE TRIGGER ${deleteFault} AFTER DELETE ON command_assertions FOR EACH STATEMENT EXECUTE FUNCTION ${deleteFault}()`);
 try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,body,command)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${deleteFault}_attempt`).first('is_called')).toBe(true);expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${deleteFault} ON command_assertions;DROP FUNCTION ${deleteFault}();DROP SEQUENCE ${deleteFault}_attempt`);}
 const receiptFault=`service_mapping_receipt_${key().replaceAll('-','')}`;
 // A deferred outbox-triggered change to the actual new receipt must not
 // become the accepted post-flush baseline; the sequence proves execution.
 await db.exec(`CREATE SEQUENCE ${receiptFault}_attempt;CREATE FUNCTION ${receiptFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id='${command}' THEN PERFORM nextval('${receiptFault}_attempt');UPDATE command_receipts SET created_at=created_at+1 WHERE command_id='${command}';END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${receiptFault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${receiptFault}()`);
 try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,body,command)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${receiptFault}_attempt`).first('is_called')).toBe(true);expect(await image()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${receiptFault} ON outbox_events;DROP FUNCTION ${receiptFault}();DROP SEQUENCE ${receiptFault}_attempt`);}
 const configured=await setup.initializeServicePartsPostingSetup(z.reviewer,body,command);expect(configured).toEqual({id:z.company,status:'configured',version:1});expect(await db.prepare('SELECT name FROM finance_charts WHERE id=?').bind(chart.id).first('name')).toBe('Actual Service mapping reference finality chart');
 const committed=await image();expect(await setup.initializeServicePartsPostingSetup(z.reviewer,body,command)).toEqual(configured);expect(await image()).toEqual(committed);expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
 });
},120000);


it('HISTORICAL COMPATIBILITY: Service product entry uses the actual current Finance or Stock assignment and named Count source without generic package authority',async()=>{
 await withMixedPartsFixture(async(z,ports)=>{
 const fixture=await zeroLedgerServicePartsSource({z,ports});
 const setup=await import('@erpx/server-commerce/public/service-parts-posting-setup'),counts=await historicalServiceCount();
 async function scope(actor:ServicePartsFixtureActor){const version=await db.prepare('SELECT version FROM assignments WHERE id=? AND company_id=? AND user_id=?').bind(actor.assignmentId,actor.companyId,actor.userId).first<number>('version');if(!version)throw Error('Original native assignment version required');return {companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,userId:actor.userId,assignmentId:actor.assignmentId,assignmentVersion:version,role:actor.role,sessionId:'isolated-native-entry-session'};}
 const finance=await scope(z.reviewer),stock=await scope(z.owner),employee=await scope(z.technician);
 expect(await setup.servicePartsPostingSetupProductEntry(db,finance)).toMatchObject({revision:expect.stringMatching(/^[a-f0-9]{64}$/),purchasePurpose:'history'});
 expect(await setup.servicePartsPostingSetupProductEntry(db,stock)).toMatchObject({purchasePurpose:'history'});
 expect(await setup.servicePartsPostingSetupProductEntry(db,employee)).toBeNull();
 for(const change of [{assignmentVersion:finance.assignmentVersion+1},{sessionId:''},{departmentId:z.department},{branchId:z.branch}])expect(await setup.servicePartsPostingSetupProductEntry(db,{...finance,...change})).toBeNull();
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,stock,{})).toEqual({capability:'service.parts-assets',purchasePurpose:'history'});
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,finance,{})).toBeNull();
 const input=fixture.input,view=await counts.serviceInitialPhysicalStockCountWorkspace(z.owner,{warehouseId:input.warehouseId,productId:fixture.product.id,countDate:input.countDate});if(!view.contextKey)throw Error('Actual Count source review required');
 const created=await counts.prepareServiceInitialPhysicalStockCount(z.owner,{expectedVersion:0,contextKey:view.contextKey,input},key());if(typeof created.id!=='string')throw Error('Original Count receipt id required');const countId=created.id;
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,stock,{countId})).toEqual({capability:'service.parts-assets',purchasePurpose:'history'});
 for(const target of [{countId:'count-foreign-source'},{countId:fixture.product.id}])expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,stock,target)).toBeNull();
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,{...stock,assignmentVersion:stock.assignmentVersion+1},{countId})).toBeNull();
 expect(await counts.serviceInitialPhysicalStockCountProductEntry(db,{...stock,departmentId:z.department},{countId})).toBeNull();
 expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']); });
});


it('Service exact mapping replay rolls a late current mapping drift back without pinning lawful historical references',async()=>{
 await withMixedPartsFixture(async(z,ports)=>{const fixture=await zeroLedgerServicePartsSource({z,ports}),setup=ports.setup;
 const committed=await fixture.snapshot(),fault=`aaa_service_replay_mapping_${key().replaceAll('-','')}`;
 // The native replay has no assertion DELETE. A scoped RLS read predicate
 // injects the same in-unit late mapping drift at the actual ledger receipt
 // read; the real receipt and its original immutable history are not changed.
 const rls=await db.prepare("SELECT relrowsecurity enabled,relforcerowsecurity forced FROM pg_class WHERE oid='command_receipts'::regclass").first<{enabled:boolean;forced:boolean}>(),policies=await db.prepare("SELECT count(*)::int n FROM pg_policy WHERE polrelid='command_receipts'::regclass").first<number>('n');if(!rls||policies!==0)throw Error('Disposable receipt RLS fixture must have no existing policies');
 await db.exec(`CREATE SEQUENCE ${fault}_attempt;CREATE FUNCTION ${fault}(receipt_id text) RETURNS boolean LANGUAGE plpgsql VOLATILE AS $$ BEGIN IF receipt_id='${fixture.mappingKey}' AND position('FROM command_receipts WHERE command_id=$1 FOR SHARE' IN current_query())>0 AND NOT (SELECT is_called FROM ${fault}_attempt) THEN PERFORM nextval('${fault}_attempt');UPDATE commerce_settings SET currency='USD' WHERE company_id='${z.company}';END IF;RETURN true;END $$;ALTER TABLE command_receipts ENABLE ROW LEVEL SECURITY;ALTER TABLE command_receipts FORCE ROW LEVEL SECURITY;CREATE POLICY ${fault} ON command_receipts FOR ALL USING (${fault}(command_id)) WITH CHECK (true)`);
 try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,fixture.mapping,fixture.mappingKey)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first('is_called')).toBe(true);expect(await fixture.snapshot()).toEqual(committed);}finally{await db.exec(`DROP POLICY ${fault} ON command_receipts;DROP FUNCTION ${fault}(text);ALTER TABLE command_receipts ${rls.enabled?'ENABLE':'DISABLE'} ROW LEVEL SECURITY;ALTER TABLE command_receipts ${rls.forced?'FORCE':'NO FORCE'} ROW LEVEL SECURITY;DROP SEQUENCE ${fault}_attempt`);}
 expect(await setup.initializeServicePartsPostingSetup(z.reviewer,fixture.mapping,fixture.mappingKey)).toEqual({id:z.company,status:'configured',version:1});expect(await fixture.snapshot()).toEqual(committed);
 });
},120000);

it('Service fresh mapping preserves independently approved Sales defaults including omitted VAT and rejects late edition changes',async()=>{
 const sales=await import('@erpx/server-finance/public/sales'),{commerceFinanceJoinedFacts}=await import('@erpx/server-finance/public/commerce-joined-facts');
 for(const withVat of [true,false]){
  await withMixedPartsFixture(async(z,ports)=>{const service=ports.finance,setup=ports.setup;await z.purchase();
  const envelope=async()=>{const view=await service.servicePartsPrerequisites(z.reviewer);expect(view.available).toBe(true);if(!view.contextKey)throw Error('Actual current Service Finance reference context required');return {expectedVersion:0 as const,contextKey:view.contextKey};};
  const refs=async(label:string)=>{
   const chart=await service.createServicePartsChart(z.reviewer,{...await envelope(),input:{name:`Actual ${label} Company chart`,baseCurrency:'THB',accountingBasis:'TFRS'}},key()),accounts:Record<string,string>={};
   for(const [name,code,category,controlKind] of [['ar','1100','asset','receivable'],['ap','2100','liability','payable'],['revenue','4100','revenue','none'],['inventory','1300','asset','none'],['clearing','2150','liability','none'],['cogs','5100','expense','none'],['inputTax','1150','asset','none'],['outputTax','2151','liability','none'],['cash','1001','asset','bank'],['otherAr','1101','asset','receivable'],['otherRevenue','4101','revenue','none'],['otherVat','2152','liability','none']] as const)accounts[name]=(await service.createServicePartsAccount(z.reviewer,{...await envelope(),input:{chartId:chart.id,code,name:`Actual ${label} ${name}`,category,currency:'THB',isControl:controlKind!=='none',controlKind}},key())).id;
   return {chart,accounts};
  };
  const reviewed=await refs('reviewed Sales intent'),other=await refs('different valid Service intent');
  // Normal original pre-enforcement Finance preparation and independent review
  // model retained approved intent. No Sales/Finance package or business SQL is
  // added to the actual Service-only Company; no economic document exists yet.
  async function approveDefaults(reason:string){
   vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
   try{const draft=await sales.prepareSalesDefaults(z.owner,{chartId:reviewed.chart.id,currency:'THB',receivableAccountId:reviewed.accounts.ar,revenueAccountId:reviewed.accounts.revenue,...withVat?{taxAccountId:reviewed.accounts.outputTax}:{},reason},key());await expect(sales.decideSalesDefaults(z.owner,draft.id,{expectedVersion:1,action:'approve',reason:'Original same-person approval remains forbidden'},key())).rejects.toMatchObject({code:'INDEPENDENT_APPROVER_REQUIRED'});const decision=await sales.decideSalesDefaults(z.manager,draft.id,{expectedVersion:1,action:'approve',reason:'Actual independent Finance review of retained Sales intent'},key());expect(decision).toMatchObject({id:draft.id,status:'approved',version:2});return {id:draft.id,version:2};}finally{vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');}
  }
  const edition=await approveDefaults('Actual independently reviewed pre-existing Sales defaults');
  for(const table of ['commerce_settings','commerce_orders','finance_invoices','finance_payments','finance_journals'])expect(await db.prepare(`SELECT count(*)::int n FROM ${table} WHERE company_id=?`).bind(z.company).first('n')).toBe(0);
  const effective=async()=>{const row=await commerceFinanceJoinedFacts(db).readCommerceSettings1({companyId:z.company});return row?Object.fromEntries(['chart_id','currency','receivable_account_id','revenue_account_id','output_tax_account_id'].map(field=>[field,row[field]??null])):null;},originalEffective=await effective();
  expect(originalEffective).toEqual({chart_id:reviewed.chart.id,currency:'THB',receivable_account_id:reviewed.accounts.ar,revenue_account_id:reviewed.accounts.revenue,output_tax_account_id:withVat?reviewed.accounts.outputTax:null});
  const seed=await setup.servicePartsPostingSetup(z.reviewer);expect(seed).toMatchObject({available:true,configured:false,sourceReady:false,canConfigure:true});if(!seed.contextKey)throw Error('Current Service mapping context required');
  // On the original source, absent metadata remains absent in the original
  // strict schema. The expected rejection therefore reaches the real policy,
  // rather than failing because the diagnostic added an unknown request field.
  const identity='expectedSalesDefaults' in seed?{expectedSalesDefaults:seed.expectedSalesDefaults}:{},body=(selected:typeof reviewed)=>({expectedVersion:0 as const,contextKey:seed.contextKey,...identity,chartId:selected.chart.id,currency:'THB',receivableAccountId:selected.accounts.ar,payableAccountId:selected.accounts.ap,revenueAccountId:selected.accounts.revenue,inventoryAccountId:selected.accounts.inventory,clearingAccountId:selected.accounts.clearing,cogsAccountId:selected.accounts.cogs,inputTaxAccountId:selected.accounts.inputTax,outputTaxAccountId:selected.accounts.outputTax,cashAccountId:selected.accounts.cash,reason:'Current Finance deliberately preserves reviewed Sales intent while adding Service parts roles'});
  const image=async()=>Object.fromEntries(await Promise.all(['finance_charts','finance_accounts','finance_sales_defaults','commerce_settings','commerce_service_parts_posting_sources','finance_journals','finance_journal_lines','finance_invoices','finance_payments','command_receipts','command_assertions','audit_events','outbox_events'].map(async table=>[table,await db.prepare(`SELECT COALESCE(jsonb_agg(to_jsonb(actual) ORDER BY to_jsonb(actual)::text),'[]'::jsonb)::text image FROM ${table} actual`).first<string>('image')]))),before=await image();
  await expect(setup.initializeServicePartsPostingSetup(z.reviewer,body(other),key())).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_DEFAULTS_CHANGED'});expect(await image()).toEqual(before);expect(await effective()).toEqual(originalEffective);
  expect(seed).toMatchObject({expectedSalesDefaults:edition,approvedSalesDefaults:{...edition,chartId:reviewed.chart.id,currency:'THB',receivableAccountId:reviewed.accounts.ar,revenueAccountId:reviewed.accounts.revenue,taxAccountId:withVat?reviewed.accounts.outputTax:null}});
  for(const change of [{receivableAccountId:reviewed.accounts.otherAr},{revenueAccountId:reviewed.accounts.otherRevenue},...withVat?[{outputTaxAccountId:reviewed.accounts.otherVat}]:[]]){await expect(setup.initializeServicePartsPostingSetup(z.reviewer,{...body(reviewed),...change},key())).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_DEFAULTS_CHANGED'});expect(await image()).toEqual(before);}
  const command=key(),input=body(reviewed),fault=`service_defaults_final_${key().replaceAll('-','')}`;
  // Adverse native-only fault changes the actual reviewed edition after the
  // new mapping effects. Both the edition and all command effects must abort.
  await db.exec(`CREATE SEQUENCE ${fault}_attempt;CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${z.company}' THEN PERFORM nextval('${fault}_attempt');UPDATE finance_sales_defaults SET reason=reason||' deferred contradiction',version=version+1 WHERE id='${edition.id}';END IF;RETURN NULL;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON commerce_service_parts_posting_sources DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${fault}()`);
  try{await expect(setup.initializeServicePartsPostingSetup(z.reviewer,input,command)).rejects.toMatchObject({status:409,code:'SERVICE_PARTS_POSTING_SOURCE_CHANGED'});expect(await db.prepare(`SELECT is_called FROM ${fault}_attempt`).first('is_called')).toBe(true);expect(await image()).toEqual(before);expect(await effective()).toEqual(originalEffective);}finally{await db.exec(`DROP TRIGGER ${fault} ON commerce_service_parts_posting_sources;DROP FUNCTION ${fault}();DROP SEQUENCE ${fault}_attempt`);}
  const configured=await setup.initializeServicePartsPostingSetup(z.reviewer,input,command);expect(configured).toEqual({id:z.company,status:'configured',version:1});expect(await effective()).toEqual(originalEffective);expect(await db.prepare('SELECT output_tax_account_id FROM commerce_settings WHERE company_id=?').bind(z.company).first('output_tax_account_id')).toBe(reviewed.accounts.outputTax);
  const committed=await image();expect(await setup.initializeServicePartsPostingSetup(z.reviewer,input,command)).toEqual(configured);expect(await image()).toEqual(committed);
  const later=await approveDefaults('A lawful later independent edition preserves the same reviewed roles');expect(later.id).not.toBe(edition.id);const current=await image();expect(await setup.initializeServicePartsPostingSetup(z.reviewer,input,command)).toEqual(configured);expect(await image()).toEqual(current);expect(await effective()).toEqual(originalEffective);
  expect((await readCompanyPackageGrants(db,z.company)).map(row=>row.packageId).sort()).toEqual(['service','service-parts-assets']);
  });
 }
},120000);

// Service customer constructor assertions moved to native selected HTTP cases:
// selected_field_customer_http_keeps_creation_receipt_and_current_confirmation_separate
// selected_native_service_startup_customer_agreement_needs_no_finance_ledger
// in collaboration/rust-field-collaboration-selected-postgres.test.rs.

// This suite's native Files intake uses the existing actual development ClamAV
// socket/definitions. No simulated readiness, scan result or installed setting.
const serviceScannerEnvironment=attachmentToolEnvironment(process.cwd());
beforeEach(()=>{
 vi.stubEnv('ERPX_CLAMD_SOCKET',serviceScannerEnvironment.ERPX_CLAMD_SOCKET);
 vi.stubEnv('ERPX_CLAMAV_DATABASE_DIR',serviceScannerEnvironment.ERPX_CLAMAV_DATABASE_DIR);
});
