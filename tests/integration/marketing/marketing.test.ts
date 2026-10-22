import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  changeMarketing,
  createMarketingAsset,
  createMarketingCandidate,
  marketingDetail,
  marketingWorkspace,
} from "@erpx/server-marketing/public/marketing";
import { getCRMLead } from "@erpx/server-crm/public/crm";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
let marketingDatabase:SqlDatabase|undefined;
const handoffFailure=vi.hoisted(()=>({enabled:false}));
// Native publication bytes, sanitizer, policy, binding and independent review
// are real; this cohort declares its clean scanner adapter explicitly.
vi.mock("@erpx/server-files/public/attachments/scanner",()=>({attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:'Declared clean Marketing withdrawal integration scanner',definitionUpdatedAt:Date.now(),error:null}),scanAttachmentBytes:async()=>({clean:true,signature:null,engine:'Declared clean Marketing withdrawal integration scanner',definitionUpdatedAt:Date.now()})}));
vi.mock("@erpx/server-crm/public/lead-handoff",async(importOriginal)=>{
  const actual=await importOriginal<typeof import("@erpx/server-crm/public/lead-handoff")>();
  return {createCRMLeadInTransaction:async(...args:Parameters<typeof actual.createCRMLeadInTransaction>)=>{
    const result=await actual.createCRMLeadInTransaction(...args);
    if(handoffFailure.enabled)await args[0].exec("SELECT 1/0");
    return result;
  }};
});
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: marketingDatabase??env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>,
  creator: WorkContext,
  reviewer: WorkContext;
afterEach(()=>{vi.unstubAllEnvs();handoffFailure.enabled=false;});
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  creator = {
    ...f.owner,
    branchId: f.branch,
    departmentId: f.department,
    modules: ["marketing", "crm"],
  };
  reviewer = {
    ...f.tie,
    branchId: f.branch,
    departmentId: f.department,
    modules: ["marketing", "crm"],
    role: "holding_owner",
    assignmentId: await f.grant(f.tie, "holding_owner"),
  };
});
async function activeForm() {
  const form = await createMarketingAsset(
    creator,
    {
      kind: "form",
      name: "Service inquiry",
      configuration: {
        fields: ["name", "email", "need"],
        consentRequired: true,
      },
      preview: { headline: "Tell us what you need" },
    },
    key(),
  );
  await expect(
    changeMarketing(
      creator,
      "asset",
      String(form.id),
      {
        action: "activate",
        expectedVersion: 1,
        reason: "Self review forbidden",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  await changeMarketing(
    reviewer,
    "asset",
    String(form.id),
    {
      action: "activate",
      expectedVersion: 1,
      reason: "Fields, consent and preview reviewed",
    },
    key(),
  );
  return form;
}
it("reviews form/content/domain/provider assets without treating previews as active", async () => {
  const form = await activeForm();
  for (const kind of ["content", "domain", "provider"] as const) {
    const x = await createMarketingAsset(
      creator,
      {
        kind,
        name: `${kind} readiness`,
        configuration: { verified: true, reference: `SIM-${kind}` },
        preview: { summary: `Reviewed ${kind} preview` },
      },
      key(),
    );
    await changeMarketing(
      reviewer,
      "asset",
      String(x.id),
      {
        action: "activate",
        expectedVersion: 1,
        reason: `Independent ${kind} evidence reviewed`,
      },
      key(),
    );
  }
  const w = await marketingWorkspace(reviewer);
  expect(w.assets.filter((x) => x.status === "active")).toHaveLength(4);
  await expect(
    db
      .prepare("UPDATE marketing_assets SET preview_json='{}' WHERE id=?")
      .bind(form.id)
      .run(),
  ).rejects.toThrow("MARKETING_ASSET_SOURCE_IMMUTABLE");
});
it("requires explicit consent for lead-capture forms", async () => {
  await expect(
    createMarketingAsset(
      creator,
      {
        kind: "form",
        name: "Inquiry without consent",
        configuration: { fields: ["name", "email"] },
        preview: { headline: "Contact us" },
      },
      key(),
    ),
  ).rejects.toMatchObject({
    status: 422,
    code: "MARKETING_CONSENT_REQUIRED",
  });
});
it("hands a consented attributed candidate to the CRM owner service exactly once", async () => {
  const form = await activeForm(),
    candidate = await createMarketingCandidate(
      creator,
      {
        formId: form.id,
        formVersion: 2,
        branchId: f.branch,
        contactName: "Prospect One",
        contactDetail: "prospect@example.test",
        consentEvidence: "Explicit form consent SIM-CONSENT-1",
        attribution: { campaign: "launch", utmSource: "newsletter" },
        brief: "Needs a field service proposal next month.",
      },
      key(),
    );
  const engagementInput = {
      action: "record-engagement" as const,
      expectedVersion: 1,
      eventType: "email_open",
      channel: "email",
      reason: "Provider event SIM-OPEN-1",
    },
    engagementCommand = key(),
    engagementResult = await changeMarketing(
      creator,
      "candidate",
      String(candidate.id),
      engagementInput,
      engagementCommand,
    );
  expect(
    await changeMarketing(
      creator,
      "candidate",
      String(candidate.id),
      engagementInput,
      engagementCommand,
    ),
  ).toEqual(engagementResult);
  await expect(
    changeMarketing(
      creator,
      "candidate",
      String(candidate.id),
      {
        action: "handoff",
        expectedVersion: 2,
        reason: "Self handoff forbidden",
        dueDate: "2026-10-01",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
  const command = key(),
    result = await changeMarketing(
      reviewer,
      "candidate",
      String(candidate.id),
      {
        action: "handoff",
        expectedVersion: 2,
        reason: "Consent, attribution and brief reviewed",
        dueDate: "2026-10-01",
      },
      command,
    );
  expect(
    await changeMarketing(
      reviewer,
      "candidate",
      String(candidate.id),
      {
        action: "handoff",
        expectedVersion: 2,
        reason: "Consent, attribution and brief reviewed",
        dueDate: "2026-10-01",
      },
      command,
    ),
  ).toEqual(result);
  const detail = await marketingDetail(reviewer, String(candidate.id)),
    lead = await getCRMLead(reviewer, String(result.crmLeadId));
  expect(detail.candidate).toMatchObject({
    status: "handed_off",
    version: 3,
    crm_lead_id: result.crmLeadId,
  });
  expect(detail.history.map((x) => x.event_type)).toEqual([
    "form_submit",
    "email_open",
    "crm_handoff",
  ]);
  expect(lead).toMatchObject({
    contact_name: "Prospect One",
    status: "open",
    owner_id: reviewer.userId,
  });
  expect(String(lead.source)).toContain("newsletter");
  await createMarketingCandidate(
    creator,
    {
      formId: form.id,
      formVersion: 2,
      branchId: f.branch,
      contactName: "Prospect Two",
      contactDetail: "second-prospect@example.test",
      consentEvidence: "Explicit form consent SIM-CONSENT-2",
      attribution: { campaign: "launch", source: "newsletter" },
      brief: "Needs a follow-up after the first proposal.",
    },
    key(),
  );
  const summary = await marketingWorkspace(reviewer);
  expect(summary.campaignSummary).toEqual([
    {
      campaign: "launch",
      source: "newsletter",
      captured: 2,
      pending: 1,
      handed_off: 1,
      rejected: 0,
    },
  ]);
  expect(JSON.stringify(summary.campaignSummary)).not.toContain("Prospect One");
  expect(
    (await marketingWorkspace({ ...reviewer, departmentId: "unrelated-department" })).campaignSummary,
  ).toEqual([]);
});
it("rejects stale or inactive form sources", async () => {
  const form = await createMarketingAsset(
    creator,
    {
      kind: "form",
      name: "Draft only",
      configuration: { consentRequired: true },
      preview: { headline: "Not published" },
    },
    key(),
  );
  await expect(
    createMarketingCandidate(
      creator,
      {
        formId: form.id,
        formVersion: 1,
        branchId: f.branch,
        contactName: "Blocked Prospect",
        contactDetail: "blocked@example.test",
        consentEvidence: "Consent evidence exists",
        attribution: { campaign: "draft" },
        brief: "Must not enter an inactive form.",
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "CONFLICT" });
});
it("rolls back CRM, Marketing, touch, notification and command evidence after a late handoff failure",async()=>{
 const form=await activeForm(),candidate=await createMarketingCandidate(creator,{formId:form.id,formVersion:2,branchId:f.branch,contactName:"Atomic handoff",contactDetail:"atomic@example.test",consentEvidence:"Explicit employee approved form consent",attribution:{source:"newsletter"},brief:"Needs an approved proposal"},key()),command=key(),leadId=`lead-${command}-crm`;
 handoffFailure.enabled=true;
 try{await expect(changeMarketing(reviewer,"candidate",String(candidate.id),{action:"handoff",expectedVersion:1,reason:"Independently review consent before handoff",dueDate:"2026-10-10"},command)).rejects.toThrow();}
 finally{handoffFailure.enabled=false;}
 expect(await db.prepare("SELECT status,version,crm_lead_id FROM marketing_lead_candidates WHERE id=?").bind(candidate.id).first()).toEqual({status:"pending",version:1,crm_lead_id:null});
 for(const table of ["crm_leads","crm_accounts","crm_contacts"])expect(await db.prepare(`SELECT count(*) count FROM ${table} WHERE id=?`).bind(table==="crm_leads"?leadId:table==="crm_accounts"?`account-${command}-crm`:`contact-${command}-crm`).first("count")).toBe(0);
 expect(await db.prepare("SELECT count(*) count FROM marketing_touches WHERE lead_id=?").bind(leadId).first("count")).toBe(0);
 expect(await db.prepare("SELECT count(*) count FROM staff_notifications WHERE record_id=?").bind(leadId).first("count")).toBe(0);
 for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) count FROM ${table} WHERE command_id IN (?,?)`).bind(command,`${command}-crm`).first("count")).toBe(0);
});
it("serializes concurrent candidate handoffs and replays the committed response without a second CRM record",async()=>{
 const form=await activeForm(),candidate=await createMarketingCandidate(creator,{formId:form.id,formVersion:2,branchId:f.branch,contactName:"Concurrent handoff",contactDetail:"concurrent@example.test",consentEvidence:"Explicit prospect consent captured",attribution:{source:"newsletter"},brief:"Discuss service proposal"},key()),body={action:"handoff",expectedVersion:1,reason:"Independently review current candidate",dueDate:"2026-10-10"},commands=[key(),key()];
 const outcomes=await Promise.allSettled(commands.map(command=>changeMarketing(reviewer,"candidate",String(candidate.id),body,command)));
 expect(outcomes.filter(outcome=>outcome.status==="fulfilled")).toHaveLength(1);
 const index=outcomes.findIndex(outcome=>outcome.status==="fulfilled"),completed=outcomes[index];
 if(completed.status!=="fulfilled")throw new Error("Expected one committed handoff");
 expect(await changeMarketing(reviewer,"candidate",String(candidate.id),body,commands[index])).toEqual(completed.value);
 expect(await db.prepare("SELECT count(*) count FROM marketing_touches WHERE source_kind='form' AND source_id=?").bind(candidate.id).first("count")).toBe(1);
 expect(await db.prepare("SELECT status,version FROM marketing_lead_candidates WHERE id=?").bind(candidate.id).first()).toEqual({status:"handed_off",version:2});
});

async function purchasedMarketing(packageId:'sales'|'sales-marketing',state:'active'|'draining'='active'){
 const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId===packageId);
 return packageGrantStore(db,{authorize:async()=>({userId:'isolated-marketing-provider'}),obligations:async()=>{throw Error('MARKETING_FIXTURE_CANNOT_INACTIVATE');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Isolated original Marketing owner purchase',evidence:'test:marketing-purchase'}});
}
async function capturedCandidate(){const form=await activeForm(),candidate=await createMarketingCandidate(creator,{formId:form.id,formVersion:2,branchId:f.branch,contactName:'Fictional purchased prospect',contactDetail:'Declared synthetic contact channel',consentEvidence:'Explicit synthetic consent to this exact reviewed form',attribution:{source:'native purchased fixture'},brief:'Retain the actual original candidate handoff source'},key());return {form,candidate};}
it('requires the Marketing addon for new work but settles only an exact existing reviewed candidate through the complete draining CRM handoff',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await expect(createMarketingAsset(creator,{kind:'content',name:'Unpurchased Marketing content',configuration:{},preview:{}},key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(marketingWorkspace(creator)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await purchasedMarketing('sales-marketing');const {form,candidate}=await capturedCandidate(),command=key(),body={action:'handoff' as const,expectedVersion:1,reason:'Independent review of the exact retained consented prospect'};
 await expect(changeMarketing(creator,'candidate',String(candidate.id),body,key())).rejects.toMatchObject({code:'CONFLICT'});
 await purchasedMarketing('sales-marketing','draining');await purchasedMarketing('sales','draining');const ownerView=await marketingWorkspace(reviewer),review=await marketingDetail(reviewer,String(candidate.id)),creatorView=await marketingDetail(creator,String(candidate.id));expect(ownerView).toMatchObject({canWrite:true,costHistory:false,purchase:{newWork:false,settleExisting:true,history:true}});expect(review).toMatchObject({canWrite:true,canHandoff:true,handoffPurchase:{newWork:false,settleExisting:true,history:true}});expect(creatorView.canHandoff).toBe(false);expect(JSON.stringify(review.handoffPurchase)).not.toMatch(/company_package_grants|authority_json|assignmentId|evidence/);await expect(createMarketingAsset(creator,{kind:'content',name:'Forbidden new content',configuration:{},preview:{}},key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});
 const result=await changeMarketing(reviewer,'candidate',String(candidate.id),body,command),leadId=`lead-${command}-crm`;expect(result).toMatchObject({id:candidate.id,status:'handed_off',version:2,crmLeadId:leadId});expect((await marketingDetail(reviewer,String(candidate.id))).candidate).toMatchObject({crm_lead_id:leadId,reviewed_by:reviewer.userId});expect(await getCRMLead(reviewer,leadId)).toMatchObject({contact_name:'Fictional purchased prospect'});
 const {changeCRMLead}=await import('@erpx/server-crm/public/crm');await changeCRMLead(reviewer,leadId,{action:'reassign',expectedVersion:1,ownerId:reviewer.userId,dueDate:'2026-12-31',reason:'A later retained CRM edit must not change the immutable handoff retry'},key());expect(await changeMarketing(reviewer,'candidate',String(candidate.id),body,command)).toEqual(result);
 const {loadPublicMarketingForm}=await import('@erpx/server-marketing/public/marketing-public-form');await expect(loadPublicMarketingForm(db,String(form.id))).rejects.toMatchObject({code:'PACKAGE_DRAINING'});const {readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants');expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId).sort()).toEqual(['sales','sales-marketing']);
});
it('rolls back an actual parent candidate source replacement after its child lead and both command receipts were written',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const {candidate}=await capturedCandidate(),{createCRMLead}=await import('@erpx/server-crm/public/crm'),other=await createCRMLead(reviewer,{branchId:f.branch,title:'Different retained CRM source',contactName:'Other fictional prospect',contactDetail:'Other declared synthetic channel',source:'Staff original source',brief:'Cannot replace the actual reviewed candidate handoff',dueDate:'2026-10-20'},key()),command=key(),childId=`lead-${command}-crm`,fault=`marketing_parent_source_${key().replaceAll('-','')}`,body={action:'handoff' as const,expectedVersion:1,reason:'Independently review this exact source before handoff',dueDate:'2026-10-20'};
 await purchasedMarketing('sales-marketing','draining');await purchasedMarketing('sales','draining');await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE marketing_lead_candidates SET crm_lead_id='${other.id}' WHERE id='${candidate.id}';RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(changeMarketing(reviewer,'candidate',String(candidate.id),body,command)).rejects.toMatchObject({code:'CRM_HANDOFF_SOURCE_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${fault} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect((await marketingDetail(reviewer,String(candidate.id))).candidate).toMatchObject({status:'pending',version:1,crm_lead_id:null});expect(await db.prepare('SELECT 1 FROM crm_leads WHERE id=?').bind(childId).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM marketing_touches WHERE lead_id=?').bind(childId).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])for(const commandId of [command,`${command}-crm`])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(commandId).first()).toBeNull();
 const receipt=await changeMarketing(reviewer,'candidate',String(candidate.id),body,command);expect(await changeMarketing(reviewer,'candidate',String(candidate.id),body,command)).toEqual(receipt);
});
it('rechecks the original Marketing and CRM reviewer authority after the parent handoff effects and preserves exact retry after rollback',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const {candidate}=await capturedCandidate(),command=key(),fault=`marketing_parent_authority_${key().replaceAll('-','')}`,body={action:'handoff' as const,expectedVersion:1,reason:'Retain the original independently reviewed source authority',dueDate:'2026-10-20'};
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE id='${reviewer.assignmentId}';RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(changeMarketing(reviewer,'candidate',String(candidate.id),body,command)).rejects.toMatchObject({status:403});}finally{await db.prepare(`DROP TRIGGER ${fault} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare('SELECT revoked_at FROM assignments WHERE id=?').bind(reviewer.assignmentId).first('revoked_at')).toBeNull();expect((await marketingDetail(reviewer,String(candidate.id))).candidate).toMatchObject({status:'pending',version:1,crm_lead_id:null});expect(await db.prepare('SELECT 1 FROM crm_leads WHERE id=?').bind(`lead-${command}-crm`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])for(const commandId of [command,`${command}-crm`])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(commandId).first()).toBeNull();const receipt=await changeMarketing(reviewer,'candidate',String(candidate.id),body,command);expect(await changeMarketing(reviewer,'candidate',String(candidate.id),body,command)).toEqual(receipt);
});
it('retains exact asset activation and retirement receipts after their original state transitions without reopening new work',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const asset=await createMarketingAsset(creator,{kind:'content',name:'Exact original reviewed content',configuration:{reference:'SYNTHETIC-MARKETING-REVIEW'},preview:{headline:'Retain the original approved meaning'}},key()),activate={action:'activate' as const,expectedVersion:1,reason:'Independent approval of the exact original content'},activationKey=key(),activated=await changeMarketing(reviewer,'asset',String(asset.id),activate,activationKey);expect(await changeMarketing(reviewer,'asset',String(asset.id),activate,activationKey)).toEqual(activated);await purchasedMarketing('sales-marketing','draining');expect(await changeMarketing(reviewer,'asset',String(asset.id),activate,activationKey)).toEqual(activated);const retire={action:'retire' as const,expectedVersion:2,reason:'Close retained publication without starting another campaign'},retireKey=key(),retired=await changeMarketing(reviewer,'asset',String(asset.id),retire,retireKey);expect(await changeMarketing(reviewer,'asset',String(asset.id),retire,retireKey)).toEqual(retired);expect(await db.prepare('SELECT count(*)::int count FROM marketing_asset_events WHERE asset_id=?').bind(asset.id).first('count')).toBe(3);
});
it('serializes two real draining candidate handoffs without consuming the original source twice',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const {candidate}=await capturedCandidate(),body={action:'handoff' as const,expectedVersion:1,reason:'Independent concurrent review of the same original captured consent',dueDate:'2026-10-20'},commands=[key(),key()];await purchasedMarketing('sales-marketing','draining');await purchasedMarketing('sales','draining');const outcomes=await Promise.allSettled(commands.map(command=>changeMarketing(reviewer,'candidate',String(candidate.id),body,command)));expect(outcomes.filter(row=>row.status==='fulfilled')).toHaveLength(1);expect(outcomes.filter(row=>row.status==='rejected')).toHaveLength(1);const saved=(await marketingDetail(reviewer,String(candidate.id))).candidate;expect(saved).toMatchObject({status:'handed_off',version:2});expect(await db.prepare('SELECT count(*)::int count FROM crm_leads WHERE company_id=?').bind(f.company).first('count')).toBe(1);expect(await db.prepare("SELECT count(*)::int count FROM marketing_engagement_events WHERE candidate_id=? AND event_type='crm_handoff'").bind(candidate.id).first('count')).toBe(1);const winning=commands.find(command=>saved.crm_lead_id===`lead-${command}-crm`)!;expect(await changeMarketing(reviewer,'candidate',String(candidate.id),body,winning)).toMatchObject({crmLeadId:saved.crm_lead_id});
});
it('rejects a late Marketing module withdrawal after the parent and child effects and restores the original candidate and command keys',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const {candidate}=await capturedCandidate(),command=key(),fault=`marketing_parent_selection_${key().replaceAll('-','')}`,body={action:'handoff' as const,expectedVersion:1,reason:'Keep the current original module selection through the whole handoff',dueDate:'2026-10-20'},before=await db.prepare('SELECT modules_json FROM assignments WHERE id=?').bind(reviewer.assignmentId).first('modules_json');
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET modules_json='["finance"]' WHERE id='${reviewer.assignmentId}';RETURN NEW;END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(changeMarketing(reviewer,'candidate',String(candidate.id),body,command)).rejects.toMatchObject({code:'CRM_ACCESS_CHANGED'});}finally{await db.prepare(`DROP TRIGGER ${fault} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare('SELECT modules_json FROM assignments WHERE id=?').bind(reviewer.assignmentId).first('modules_json')).toEqual(before);expect((await marketingDetail(reviewer,String(candidate.id))).candidate).toMatchObject({status:'pending',version:1,crm_lead_id:null});expect(await db.prepare('SELECT 1 FROM crm_leads WHERE id=?').bind(`lead-${command}-crm`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])for(const commandId of [command,`${command}-crm`])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(commandId).first()).toBeNull();expect(await changeMarketing(reviewer,'candidate',String(candidate.id),body,command)).toMatchObject({crmLeadId:`lead-${command}-crm`});
});
it('sees the actual consented candidate committed by a same-Company writer before the complete Marketing CRM handoff snapshot',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const {candidate}=await capturedCandidate(),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),command=key();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','disabled');
 const observed=await committedSourceVisibility(db,async tx=>{marketingDatabase=tx;try{await changeMarketing(creator,'candidate',String(candidate.id),{action:'record-engagement',expectedVersion:1,eventType:'contacted',channel:'phone',reason:'Actual original engagement committed before independent review'},key());}finally{marketingDatabase=undefined;}},()=>{vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');return measureReads(()=>changeMarketing(reviewer,'candidate',String(candidate.id),{action:'handoff',expectedVersion:2,reason:'Independent review of the committed original candidate revision',dueDate:'2026-10-20'},command));});
 expect(observed.result).toMatchObject({id:candidate.id,status:'handed_off',version:3,crmLeadId:`lead-${command}-crm`});expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);expect(await getCRMLead(reviewer,`lead-${command}-crm`)).toMatchObject({brief:'Retain the actual original candidate handoff source'});expect((await marketingDetail(reviewer,String(candidate.id))).history).toHaveLength(3);
});
it('filters consented Marketing source authority before shared search Staff and Inbox limits and rechecks CRM handoff actions after Flow',async()=>{
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);const {candidate}=await capturedCandidate(),other=await expenseFixture(),foreign=await createMarketingAsset({...other.owner,modules:['marketing']},{kind:'form',name:'Actual other Company form',configuration:{consentRequired:true},preview:{}},key()),prefix=`marketing-projection-${key()}`,now=Date.now(),ordinary=`ordinary-${prefix}`;
 await db.prepare(`INSERT INTO marketing_lead_candidates(id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at) SELECT ?||n,company_id,branch_id,department_id,?,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,NULL,created_by,NULL,1,?,?::bigint+n FROM marketing_lead_candidates CROSS JOIN generate_series(1,105) n WHERE id=?`).bind(prefix,foreign.id,now,now,candidate.id).run();
 const invalid=(await db.prepare('SELECT id FROM marketing_lead_candidates WHERE company_id=? AND id LIKE ?').bind(f.company,`${prefix}%`).all<{id:string}>()).results.map(row=>row.id),{publishInboxEvent,inboxPage}=await import('@erpx/server-collaboration/public/inbox'),{listRecords,recordsPage,getRecord,searchReferences}=await import('@erpx/server-records/public/records'),{listStaffNotifications,notificationsPage}=await import('@erpx/server-collaboration/public/staff-notifications'),{enrichFlowWork}=await import('@erpx/server-workflow/public/flow-work'),{marketingWork}=await import('@erpx/server-marketing/public/marketing');
 for(const [index,recordId] of [ordinary,String(candidate.id),...invalid].entries())await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,created_by,updated_by,created_at,updated_at) VALUES('marketing',?,'feature-lead-candidates','Marketing purchase projection acceptance','','draft','[]',?,?,?,?,?,?,?)").bind(recordId,f.company,f.branch,f.department,reviewer.userId,reviewer.userId,now,now+index).run();
 for(const [index,recordId] of [String(candidate.id),...invalid].entries()){
  await db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'marketing_candidate',?,1,'Marketing purchase projection acceptance',?)").bind(key(),f.company,reviewer.userId,recordId,now+index).run();await publishInboxEvent(db,{key:key(),companyId:f.company,sourceType:'marketing_candidate',sourceId:recordId,sourceVersion:1,category:'work',preview:'Marketing purchase projection acceptance',recipientIds:[reviewer.userId]});
 }
 const stale=(await marketingWork(reviewer)).filter(item=>item.id===candidate.id);vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');expect((await listRecords(reviewer,'marketing','feature-lead-candidates')).map(row=>row.id)).toEqual([ordinary]);expect((await recordsPage(reviewer,'marketing','feature-lead-candidates',{})).items.map(row=>row.id)).toEqual([ordinary]);expect((await searchReferences(reviewer,'Marketing purchase projection')).filter(row=>'id' in row).map(row=>row.id)).toEqual([ordinary]);await expect(getRecord(reviewer,'marketing',String(candidate.id))).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect((await listStaffNotifications(db,reviewer)).some(row=>row.record_type==='marketing_candidate')).toBe(false);expect((await inboxPage(db,reviewer,{view:'all'})).items.some(row=>row.sourceType==='marketing_candidate')).toBe(false);expect(await enrichFlowWork(reviewer,stale)).toEqual([]);
 await purchasedMarketing('sales-marketing');expect((await listRecords(reviewer,'marketing','feature-lead-candidates')).map(row=>row.id).sort()).toEqual([ordinary,String(candidate.id)].sort());expect((await recordsPage(reviewer,'marketing','feature-lead-candidates',{})).items.map(row=>row.id).sort()).toEqual([ordinary,String(candidate.id)].sort());expect((await searchReferences(reviewer,'Marketing purchase projection')).filter(row=>'id' in row).map(row=>row.id).sort()).toEqual([ordinary,String(candidate.id)].sort());expect((await listStaffNotifications(db,reviewer)).some(row=>row.record_id===candidate.id)).toBe(true);expect((await notificationsPage(db,reviewer,{view:'all'})).items.some(row=>row.record_id===candidate.id)).toBe(true);expect((await inboxPage(db,reviewer,{view:'all'})).items.some(row=>row.sourceId===candidate.id)).toBe(true);await expect(getRecord(reviewer,'marketing',invalid[0])).rejects.toMatchObject({code:'MARKETING_SOURCE_NOT_FOUND'});expect((await getRecord(reviewer,'marketing',String(candidate.id))).id).toBe(candidate.id);
 await purchasedMarketing('sales-marketing','draining');await purchasedMarketing('sales','draining');expect((await enrichFlowWork(reviewer,stale)).find(item=>item.id===candidate.id)?.canAct).toBe(true);await changeMarketing(reviewer,'candidate',String(candidate.id),{action:'handoff',expectedVersion:1,reason:'A completed original owner handoff cannot be revived by Flow',dueDate:'2026-10-20'},key());expect((await enrichFlowWork(reviewer,stale)).find(item=>item.id===candidate.id)?.canAct).toBe(false);await db.prepare("UPDATE assignments SET modules_json='[\"crm\"]' WHERE id=?").bind(reviewer.assignmentId).run();expect((await listStaffNotifications(db,reviewer)).filter(row=>row.record_type==='marketing_candidate')).toEqual([]);expect((await inboxPage(db,reviewer,{view:'all'})).items.filter(row=>row.sourceType==='marketing_candidate')).toEqual([]);expect(await enrichFlowWork(reviewer,stale)).toEqual([]);expect(invalid).toHaveLength(105);
});

const marketingSettlementInput=()=>({companyId:f.company,packageId:'sales-marketing',packageVersion:1});
async function marketingSettlement(){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');return withOwnerPackageObligations(db,marketingSettlementInput(),tx=>readOwnerPackageObligations(db,tx,{...marketingSettlementInput(),now:Date.now()}));}
async function marketingWithdrawal(){const {readCompanyPackageGrants,packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,f.company),current=grants.find(grant=>grant.packageId==='sales-marketing')!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual Marketing publication and candidate handoff settlement reviewed',evidence:'test:marketing-raw-owner-settlement'}}]};}
async function marketingWithdrawalReview(request:Awaited<ReturnType<typeof marketingWithdrawal>>,apply=false,after?:()=>Promise<void>){const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-marketing-settlement-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});return withOwnerPackageObligations(db,marketingSettlementInput(),async tx=>{const result=apply?await store.batch.applyIn(tx,request,async()=>{}):await store.batch.previewIn(tx,request);if(after){marketingDatabase=tx;try{await after();}finally{marketingDatabase=undefined;}}return result;});}
it('withdraws only Marketing after actual retained source settlement while base Sales remains active and held private history and exact handoff replay survive',async()=>{
 vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const {form,candidate}=await capturedCandidate(),{createCRMLead}=await import('@erpx/server-crm/public/crm'),base=await createCRMLead(creator,{branchId:f.branch,title:'Unrelated original base Sales prospect stays open',contactName:'Declared synthetic Sales contact',contactDetail:'Declared original staff contact channel',source:'Original staff direct source',brief:'Marketing withdrawal cannot close base Sales customer work',dueDate:'2026-10-20'},key()),content=await createMarketingAsset(creator,{kind:'content',name:'Original private held campaign evidence',configuration:{},preview:{}},key()),{seedAttachmentSet}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service'),fileActor={...f.owner,modules:['marketing']},document=await seedAttachmentSet(db,fileActor,{targetType:'marketing_asset',targetId:String(content.id),componentKind:'record',purposeKey:'source',expectedTargetVersion:1});await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(fileActor.modules),fileActor.assignmentId).run();await bindRecordAttachmentSet(db,fileActor,document.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,'Held original Marketing source history',{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(document.sourceId).run();
 const opened=await marketingSettlement();expect(opened.observations.map(row=>row.moduleId)).toEqual(['marketing','crm','finance']);expect(opened.total).toBeGreaterThan(0);expect(JSON.stringify(opened)).not.toMatch(/contact_detail|authority_json|Held original|consent_evidence/);await purchasedMarketing('sales-marketing','draining');const request=await marketingWithdrawal();await expect(marketingWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(marketingWithdrawalReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const command=key(),handoff={action:'handoff' as const,expectedVersion:1,reason:'Independent completion of the actual consented Marketing candidate',dueDate:'2026-10-20'},delivered=await changeMarketing(reviewer,'candidate',String(candidate.id),handoff,command);await changeMarketing(reviewer,'asset',String(form.id),{action:'retire',expectedVersion:2,reason:'Close the actual retained public form before package withdrawal'},key());await changeMarketing(reviewer,'asset',String(content.id),{action:'reject',expectedVersion:2,reason:'Independent rejection retains the original private evidence'},key());expect((await marketingSettlement()).total).toBe(0);
 const tables=['marketing_assets','marketing_asset_events','marketing_lead_candidates','marketing_engagement_events','marketing_touches','crm_leads','crm_case_events','finance_journals','attachments','attachment_sets','attachment_links'],history=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT to_jsonb(row)::text value FROM ${table} row ORDER BY to_jsonb(row)::text`).all()).results)),before=await history();expect((await marketingWithdrawalReview(request)).after.find(grant=>grant.packageId==='sales-marketing')?.state).toBe('inactive');const applied=await marketingWithdrawalReview(request,true);expect(await marketingWithdrawalReview(request,true)).toEqual(applied);expect(await history()).toEqual(before);expect(await changeMarketing(reviewer,'candidate',String(candidate.id),handoff,command)).toEqual(delivered);expect((await marketingDetail(reviewer,String(candidate.id))).candidate.status).toBe('handed_off');expect((await getCRMLead(creator,String(base.id))).status).toBe('open');expect(await (await attachmentContent(db,env.H001_FILES,fileActor,document.linkId,'original')).object.text()).toBe('Held original Marketing source history');expect(await db.prepare("SELECT state FROM company_package_grants WHERE company_id=? AND package_id='sales'").bind(f.company).first('state')).toBe('active');await expect(createMarketingAsset(creator,{kind:'content',name:'Inactive cannot open new Marketing work',configuration:{},preview:{}},key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});expect(await createCRMLead(creator,{branchId:f.branch,title:'Base Sales work continues after Marketing withdrawal',contactName:'Another synthetic direct prospect',contactDetail:'Declared direct staff channel',source:'Staff direct source',brief:'This requires only the original base Sales capability',dueDate:'2026-10-20'},key())).toHaveProperty('id');
});
it('rolls back a late normal Marketing obligation and the normal Commercial inactive receipt together after deferred owner effects',async()=>{
 await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');await purchasedMarketing('sales-marketing','draining');const request=await marketingWithdrawal(),command=key();await expect(marketingWithdrawalReview(request,true,()=>createMarketingAsset(creator,{kind:'content',name:'Late original Marketing owner draft',configuration:{},preview:{}},command).then(()=>{}))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await db.prepare("SELECT state,version FROM company_package_grants WHERE company_id=? AND package_id='sales-marketing'").bind(f.company).first()).toEqual({state:'draining',version:2});expect(await db.prepare('SELECT 1 FROM marketing_assets WHERE id=?').bind(`marketing-content-${command}`).first()).toBeNull();for(const table of ['command_receipts','audit_events','outbox_events'])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();expect(await db.prepare('SELECT 1 FROM company_package_change_sets WHERE id=?').bind(request.idempotencyKey).first()).toBeNull();expect((await marketingSettlement()).total).toBe(0);
});
it('observes a concurrent committed Marketing obligation only after its exact source leases without stale snapshot retries',async()=>{
 await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');await purchasedMarketing('sales-marketing','draining');const request=await marketingWithdrawal(),command=key(),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),observed=await committedSourceVisibility(db,async tx=>{marketingDatabase=tx;try{await createMarketingAsset(creator,{kind:'content',name:'Concurrent native Marketing settlement draft',configuration:{},preview:{}},command);}finally{marketingDatabase=undefined;}},()=>measureReads(async()=>{try{await marketingWithdrawalReview(request);return null;}catch(error){return error;}}));expect(observed.result).toMatchObject({message:'PACKAGE_OBLIGATIONS_REMAIN'});expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);expect(await db.prepare('SELECT status FROM marketing_assets WHERE id=?').bind(`marketing-content-${command}`).first('status')).toBe('draft');
});
it('keeps forged handed-off labels and activated provider callbacks unresolved instead of certifying a filtered Marketing zero',async()=>{
 const {form,candidate}=await capturedCandidate(),{createCRMLead}=await import('@erpx/server-crm/public/crm'),unrelated=await createCRMLead(reviewer,{branchId:f.branch,title:'Unrelated valid customer lead cannot stand in for a handoff',contactName:'Declared unrelated synthetic contact',contactDetail:'Independent staff channel',source:'Staff source',brief:'No original Marketing parent or child receipt',dueDate:'2026-10-20'},key());await db.prepare("UPDATE marketing_lead_candidates SET status='handed_off',crm_lead_id=?,reviewed_by=?,version=2 WHERE id=?").bind(unrelated.id,reviewer.userId,candidate.id).run();await changeMarketing(reviewer,'asset',String(form.id),{action:'retire',expectedVersion:2,reason:'Retire the actual form without inventing a completed handoff'},key());await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');await purchasedMarketing('sales-marketing','draining');const observed=await marketingSettlement();expect(observed.observations.find(row=>row.moduleId==='crm')?.outstanding).toBeGreaterThan(0);await expect(marketingWithdrawalReview(await marketingWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const provider=await createMarketingAsset(creator,{kind:'provider',name:'Declared original provider configuration',configuration:{reference:'SYNTHETIC-NO-EXTERNAL-ACCOUNT'},preview:{}},key());await changeMarketing(reviewer,'asset',String(provider.id),{action:'activate',expectedVersion:1,reason:'Original independent provider configuration review'},key());await changeMarketing(reviewer,'asset',String(provider.id),{action:'retire',expectedVersion:2,reason:'Retirement alone cannot invent callback reconciliation'},key());expect((await marketingSettlement()).observations.find(row=>row.moduleId==='marketing')?.outstanding).toBeGreaterThan(0);
});
it('requires the exact minted Marketing source scope and keeps unsupported Marketing and Sales versions closed',async()=>{
 const {marketingPackageObligations}=await import('@erpx/server-marketing/public/package-obligations'),{withOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),input={...marketingSettlementInput(),moduleId:'marketing',now:Date.now()};await expect(db.transaction(tx=>marketingPackageObligations(db,tx,input))).rejects.toThrow('MARKETING_PACKAGE_OBLIGATION_SOURCE_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...marketingSettlementInput(),packageVersion:2},async()=>{})).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');expect(()=>withOwnerPackageObligations(db,{...marketingSettlementInput(),packageId:'sales',packageVersion:2},async()=>{})).toThrow('PACKAGE_OBLIGATION_SOURCES_REQUIRED');await expect(withOwnerPackageObligations(db,marketingSettlementInput(),tx=>marketingPackageObligations(db,tx,{...input,moduleId:'crm'}))).rejects.toThrow('MARKETING_PACKAGE_OBLIGATION_SCOPE_REQUIRED');await expect(withOwnerPackageObligations(db,marketingSettlementInput(),tx=>marketingPackageObligations(db,tx,{...input,companyId:'other-company'}))).rejects.toThrow('MARKETING_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
});
it('observes only exact Marketing-linked independently posted Finance cost and rejects a foreign Company line instead of hiding it',async()=>{
 const {changeMarketingCampaign}=await import('@erpx/server-marketing/public/marketing-campaigns'),{createJournal,createJournalSchema,postJournal}=await import('@erpx/server-finance/public/finance'),{resolveFlow,assignFlowPrimary}=await import('@erpx/server-workflow/public/flow-routing'),{withFinanceFlowAssignment}=await import('@erpx/server-finance/public/document-purchases'),campaign=await changeMarketingCampaign(creator,{action:'create',code:`WITHDRAW_${key().slice(0,8)}`,name:'Exact original Marketing cost attribution'},key()),journal=await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-25',currency:'THB',memo:'Actual independently posted original campaign expense',lines:[{accountId:f.accounts.expense,debit:'10',credit:'0'},{accountId:f.accounts.cash,debit:'0',credit:'10'}]}),key()),journalId=String(journal.id),flow=await resolveFlow(db,f.owner,'journal',journalId),command=key(),assignment={stepKey:'draft',userId:f.manager.userId,reason:'Original independent review of the captured campaign expense',saveDefault:false,expectedVersion:flow.version,recordVersion:flow.recordVersion};await withFinanceFlowAssignment(f.owner,command,tx=>assignFlowPrimary(tx??db,f.owner,'journal',journalId,assignment,command),{kind:'journal',id:journalId});await postJournal(f.manager,journalId,key(),{expectedVersion:1});const line=String(await db.prepare('SELECT id FROM finance_journal_lines WHERE journal_id=? AND account_id=?').bind(journalId,f.accounts.expense).first('id'));await changeMarketingCampaign(creator,{action:'link-cost',campaignId:String(campaign.id),journalLineId:line,evidence:'Original reviewed exact posted campaign cost evidence'},key());
 await createJournal(f.owner,createJournalSchema.parse({chartId:f.chart,periodId:f.period,entryDate:'2026-09-25',currency:'THB',memo:'Unrelated open Finance work does not belong to this add-on',lines:[{accountId:f.accounts.expense,debit:'100',credit:'0'},{accountId:f.accounts.cash,debit:'0',credit:'100'}]}),key());await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');await purchasedMarketing('sales-marketing','draining');expect((await marketingSettlement()).total).toBe(0);const other=await expenseFixture(),foreignJournal=key(),foreignLine=key(),now=Date.now();await db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,'2026-09-25','THB','draft',?,?,?)").bind(foreignJournal,other.company,other.chart,other.period,key(),other.owner.userId,now,now).run();await db.prepare('INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,500,0,500,0,0),(?,?,?,0,500,0,500,1)').bind(foreignLine,foreignJournal,other.accounts.expense,key(),foreignJournal,other.accounts.cash).run();await db.prepare("UPDATE finance_journals SET status='posted',posted_by=?,posted_at=? WHERE id=?").bind(other.manager.userId,now,foreignJournal).run();await db.prepare('INSERT INTO marketing_campaign_costs(id,company_id,campaign_id,journal_line_id,evidence,reviewed_by,created_at) VALUES(?,?,?,?,?,?,?)').bind(key(),f.company,campaign.id,foreignLine,'Forged Company attribution is not the original native cost',creator.userId,now).run();const observed=await marketingSettlement();expect(observed.observations.find(row=>row.moduleId==='finance')?.outstanding).toBe(1);await expect(marketingWithdrawalReview(await marketingWithdrawal())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
});

it('blocks Marketing withdrawal on actual approved public creative after source retirement until normal draining revocation while held originals remain readable',async()=>{
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);vi.stubEnv('ERPX_ATTACHMENTS_V2','true');vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const author={...f.owner,modules:['marketing']},independent={...reviewer,branchId:null,departmentId:null,modules:['marketing']};await db.batch([author,independent].map(actor=>db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(actor.modules),actor.assignmentId)));const {controlAccount}=await import('../../support/helpers/engineering-workplace-platform-attachment-owners');await controlAccount(author);await controlAccount(independent);
 const {readFile}=await import('node:fs/promises'),{createAttachmentSet,reserveAttachmentFile,completeAttachmentUpload,drainAttachmentScans,bindRecordAttachmentSet,createAttachmentPublication,approveAttachmentPublication,revokeAttachmentPublication,attachmentContent}=await import('@erpx/server-files/public/attachments/service'),content=await createMarketingAsset(author,{kind:'content',name:'Original independently reviewed native public creative',configuration:{reference:'Repository ERPX icon only; no external advertising account'},preview:{headline:'Declared fictional public campaign'}},key()),image=await readFile('public/icon-192.png'),set=await createAttachmentSet(db,author,{targetType:'marketing_asset',targetId:String(content.id),componentKind:'record',purposeKey:'creative',expectedTargetVersion:1},key()),reservation=await reserveAttachmentFile(db,author,set.id,{filename:'declared-marketing-withdrawal-erpx-icon.png',mediaType:'image/png',bytes:image.byteLength},key()),upload=await completeAttachmentUpload(db,env.H001_FILES,author,reservation.uploadIntentId,Uint8Array.from(image).buffer);expect(await drainAttachmentScans(db,env.H001_FILES,upload.sourceId)).toBe(true);await bindRecordAttachmentSet(db,author,set.id,1,key());const publication=await createAttachmentPublication(db,env.H001_FILES,author,{sourceLinkId:reservation.attachmentLinkId,targetType:'marketing_asset',targetId:String(content.id),altText:'Declared original public creative withdrawal proof',ordinal:0},key());if(!publication)throw Error('Actual native Marketing publication missing');await approveAttachmentPublication(db,independent,String(publication.id),key());await changeMarketing(independent,'asset',String(content.id),{action:'activate',expectedVersion:2,reason:'Independent review of actual sanitized public creative'},key());await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(upload.sourceId).run();await purchasedMarketing('sales-marketing','draining');await changeMarketing(independent,'asset',String(content.id),{action:'retire',expectedVersion:3,reason:'Retire source while actual public publication is still outstanding'},key());const request=await marketingWithdrawal();expect((await marketingSettlement()).observations.find(row=>row.moduleId==='marketing')?.outstanding).toBeGreaterThan(0);await expect(marketingWithdrawalReview(request)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 const command=key(),reason='Original reviewer revokes exact public material during add-on closure',revoked=await revokeAttachmentPublication(db,independent,String(publication.id),reason,command);expect(revoked).toMatchObject({state:'revoked'});expect((await marketingSettlement()).total).toBe(0);await marketingWithdrawalReview(request);await marketingWithdrawalReview(request,true);expect(await revokeAttachmentPublication(db,independent,String(publication.id),reason,command)).toEqual(revoked);await expect(revokeAttachmentPublication(db,independent,String(publication.id),reason,key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await expect(revokeAttachmentPublication(db,independent,String(publication.id),reason+' changed caller reason',command)).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});const otherReviewer={...await f.member('different-current-marketing-reviewer','holding_owner',10),modules:['marketing']};await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(otherReviewer.modules),otherReviewer.assignmentId).run();await controlAccount(otherReviewer);await expect(revokeAttachmentPublication(db,otherReviewer,String(publication.id),reason,command)).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),independent.assignmentId).run();await expect(revokeAttachmentPublication(db,independent,String(publication.id),reason,command)).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(await db.prepare("SELECT count(*)::int n FROM attachment_events WHERE company_id=? AND action='publication.revoked' AND details_json::jsonb->>'publicationId'=?").bind(f.company,publication.id).first('n')).toBe(1);expect(new Uint8Array(await (await attachmentContent(db,env.H001_FILES,author,reservation.attachmentLinkId,'original')).object.arrayBuffer())).toEqual(Uint8Array.from(image));expect(await db.prepare('SELECT legal_hold,source_state FROM attachments WHERE id=?').bind(upload.sourceId).first()).toEqual({legal_hold:1,source_state:'available'});
});


it('Marketing private bytes keep exact native consent/content and campaign sources through lazy consumption and deferred source or current IAM effects',async()=>{
 const {seedAttachmentSet,enableAttachmentV2ForTest}=await import('../../support/helpers/attachment-v2'),{bindRecordAttachmentSet,attachmentContent}=await import('@erpx/server-files/public/attachments/service'),{changeMarketingCampaign}=await import('@erpx/server-marketing/public/marketing-campaigns');enableAttachmentV2ForTest();vi.stubEnv('ERPX_COMMERCE_PACKAGE_ENFORCEMENT','enforced');await purchasedMarketing('sales');await purchasedMarketing('sales-marketing');const author={...f.owner,modules:['marketing']};await db.prepare('UPDATE assignments SET modules_json=? WHERE id=?').bind(JSON.stringify(author.modules),author.assignmentId).run();const asset=await createMarketingAsset(author,{kind:'content',name:'Actual retained consent source',configuration:{reference:'Actual original consent reference'},preview:{headline:'Private original campaign source'}},key()),campaign=await changeMarketingCampaign(author,{action:'create',code:`private_${key().replaceAll('-','')}`,name:'Actual retained private campaign'},key()),documents:Awaited<ReturnType<typeof seedAttachmentSet>>[]=[];
 for(const [targetType,targetId,purposeKey] of [['marketing_asset',String(asset.id),'consent'],['marketing_campaign',String(campaign.id),'source']] as const){const file=await seedAttachmentSet(db,author,{targetType,targetId,componentKind:'record',purposeKey,expectedTargetVersion:1});await bindRecordAttachmentSet(db,author,file.setId,1,key());const objectKey=String(await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first('object_key'));await env.H001_FILES.put(objectKey,`Exact native ${targetType} private consent evidence`,{httpMetadata:{contentType:'application/pdf'}});await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(file.sourceId).run();documents.push(file);}
 await purchasedMarketing('sales-marketing','draining');let consumed=0;const files:import("@erpx/server-runtime/public/runtime/types").FileStore={get:async(...args)=>{const object=await env.H001_FILES.get(...args);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;},put:(...args)=>env.H001_FILES.put(...args),head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
 for(const [index,file] of documents.entries()){const target=index===0?'marketing_asset':'marketing_campaign',table=index===0?'marketing_assets':'marketing_campaigns',id=index===0?asset.id:campaign.id;expect(await(await attachmentContent(db,files,author,file.linkId,'original')).object.text()).toBe(`Exact native ${target} private consent evidence`);await expect(db.transaction(tx=>attachmentContent(tx,files,author,file.linkId,'original'))).rejects.toMatchObject({code:'MARKETING_FILE_READ_TRANSACTION_REQUIRED'});for(const fault of ['source','iam'] as const){const trigger=`marketing_byte_${key().replaceAll('-','')}`,effect=fault==='source'?`UPDATE ${table} SET name=name||' unexpected private edit' WHERE id='${id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${author.assignmentId}'`,before=await db.prepare(`SELECT name FROM ${table} WHERE id=?`).bind(id).first('name');await db.exec(`CREATE FUNCTION ${trigger}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${trigger} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${trigger}()`);try{await expect(attachmentContent(db,files,author,file.linkId,'original')).rejects.toMatchObject({code:fault==='source'?'MARKETING_FILE_READ_CHANGED':'ATTACHMENT_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${trigger} ON attachment_access_events;DROP FUNCTION ${trigger}()`);}expect(await db.prepare(`SELECT name FROM ${table} WHERE id=?`).bind(id).first('name')).toBe(before);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(author.assignmentId).first()).toEqual({revoked_at:null,version:1});expect(Number(await db.prepare('SELECT count(*) n FROM attachment_access_events WHERE link_id=?').bind(file.linkId).first('n'))).toBe(1);}expect(await(await attachmentContent(db,files,author,file.linkId,'original')).object.text()).toBe(`Exact native ${target} private consent evidence`);expect(await db.prepare('SELECT legal_hold,revoked_at FROM attachments WHERE id=?').bind(file.sourceId).first()).toEqual({legal_hold:1,revoked_at:null});}
 expect(consumed).toBe(8);await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(author.assignmentId).run();await expect(attachmentContent(db,files,author,documents[1].linkId,'original')).rejects.toMatchObject({status:404,code:'ATTACHMENT_TARGET_NOT_FOUND'});expect(consumed).toBe(8);expect((await db.prepare("SELECT outcome,reason FROM attachment_access_events WHERE link_id=? AND outcome='denied'").bind(documents[1].linkId).all()).results).toEqual([{outcome:'denied',reason:'ATTACHMENT_TARGET_NOT_FOUND'}]);
});
