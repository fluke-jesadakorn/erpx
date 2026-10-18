import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { prepareAttachmentBinding } from "../../../src/server/modules/files/public/attachments/service";
import type { PreparedAttachmentBinding } from "../../../src/server/modules/files/public/attachments/types";
import { applyMigrations, env } from "../../support/runtime-env";
import {expenseFixture} from "../../support/helpers/expense-fixture";
import {seedAttachmentSet,attachmentBindingState} from "../../support/helpers/attachment-v2";
import {bindRecordAttachmentSet,attachmentContent} from "../../../src/server/modules/files/public/attachments/service";
import {createCRMLead,getCRMLead,changeCRMLead} from "../../../src/server/modules/crm/public/crm";
import {changeCRMIdentity} from "../../../src/server/modules/crm/public/crm-identities";
import {saveMaster} from "../../../src/server/modules/commerce/public/commerce";
import {createMarketingAsset} from "../../../src/server/modules/marketing/public/marketing";
import {changeMarketingCampaign} from "../../../src/server/modules/marketing/public/marketing-campaigns";
import {measureReads} from "../../../src/server/modules/runtime/public/runtime/read-metrics";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
afterEach(()=>vi.unstubAllEnvs());

const db = env.H001_DB;
const key = () => crypto.randomUUID();

let companyId: string;
let branchId: string;
let departmentId: string;
let otherBranchId: string;
let otherDepartmentId: string;
let accountId: string;
let leadId: string;
let opportunityId: string;
let engagementId: string;
let actor: WorkContext;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  process.env.ERPX_ATTACHMENTS_V2 = "true";
  delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
});

afterAll(() => {
  delete process.env.ERPX_ATTACHMENTS_V2;
  delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
});

beforeEach(async () => {
  const suffix = key();
  companyId = `attachment-crm-company-${suffix}`;
  branchId = `attachment-crm-branch-${suffix}`;
  departmentId = `attachment-crm-department-${suffix}`;
  otherBranchId = `attachment-crm-other-branch-${suffix}`;
  otherDepartmentId = `attachment-crm-other-department-${suffix}`;
  accountId = `attachment-crm-account-${suffix}`;
  leadId = `attachment-crm-lead-${suffix}`;
  opportunityId = `attachment-crm-opportunity-${suffix}`;
  const userId = `attachment-crm-owner-${suffix}`;
  const assignmentId = `attachment-crm-assignment-${suffix}`;
  const personId = `attachment-crm-person-${suffix}`;
  engagementId = `attachment-crm-engagement-${suffix}`;
  const customerId = `attachment-crm-customer-${suffix}`;
  const now = Date.now();

  actor = {
    userId,
    assignmentId,
    role: "holding_owner",
    companyId,
    branchId,
    departmentId,
    holdingId: "h001",
    holdingSlug: "acme",
    holdingName: "Test",
    name: "CRM attachment owner",
    email: `${userId}@example.test`,
    scope: {
      kind: "holding",
      holdingId: "h001",
      holdingSlug: "acme",
      holdingName: "Test",
      companyId,
    },
    assignmentScope: "holding",
    inherited: false,
    modules: [],
    assignments: [],
    workspaces: [],
  };

  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'CRM attachment company',?)")
      .bind(companyId, companyId, companyId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'CRM attachment branch',?,?,?)")
      .bind(branchId, branchId, companyId, companyId, branchId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'CRM attachment department',?,?,?)")
      .bind(departmentId, departmentId, branchId, companyId, branchId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'Other CRM branch',?,?,?)")
      .bind(otherBranchId, otherBranchId, companyId, companyId, otherBranchId),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'Other CRM department',?,?,?)")
      .bind(otherDepartmentId, otherDepartmentId, otherBranchId, companyId, otherBranchId),
    db.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,'[]',?,0,9999999999999,'holding')")
      .bind(assignmentId, userId, actor.role, companyId),
    db.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,status,created_by,created_at,updated_at) VALUES(?,?,?,'CRM attachment owner','TH','active','fixture',?,?)")
      .bind(personId, userId, companyId, now, now),
    db.prepare(`INSERT INTO hr_engagements(
      id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,'employee','salaried','active','2020-01-01','Sales','THB','fixture',?,?)`)
      .bind(engagementId, personId, companyId, branchId, departmentId, now, now),
    db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,'CRM attachment customer','THB','active',?,?)")
      .bind(customerId, companyId, customerId, now, now),
    db.prepare("INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES(?,?,'business','Scoped CRM account','another-user',?,?)")
      .bind(accountId, companyId, now, now),
    db.prepare(`INSERT INTO crm_leads(
      id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,created_by,created_at,updated_at,account_id
    ) VALUES(?,?,?,?,?,'Customer','private@example.test','Direct','Private brief',?,'2026-12-31','qualified',?,?,?,?)`)
      .bind(leadId, companyId, branchId, departmentId, "Atomic attachment opportunity", userId, userId, now, now, accountId),
    db.prepare("INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES(?,?,?,10000,'THB','2026-12-31',?,?,?)")
      .bind(opportunityId, leadId, customerId, userId, now, now),
  ]);
});

async function prepareBinding(targetType: "crm_opportunity" | "crm_account", targetId: string) {
  const token = key();
  const purposeKey = "brief";
  const uploadId = `attachment-crm-upload-${token}`;
  const sourceId = `attachment-crm-source-${token}`;
  const setId = `attachment-crm-set-${token}`;
  const linkId = `attachment-crm-link-${token}`;
  const commandId = `attachment-crm-bind-${token}`;
  const checksum = "a".repeat(64);
  const now = Date.now();

  await db.batch([
    db.prepare(`INSERT INTO upload_intents(
      id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id
    ) VALUES(?,?,?,?,?,'application/pdf',4,4,?,?,?,'ready',?,?,?,?,?)`)
      .bind(uploadId, targetType, targetId, purposeKey, "brief.pdf", checksum, `quarantine/${token}`, `source/${token}`, actor.userId, now + 60_000, now, now, companyId),
    db.prepare(`INSERT INTO attachments(
      id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at
    ) VALUES(?,?,?,?,?,?,1,?,'application/pdf',4,?,?,?,?,'application/pdf','available',?,?)`)
      .bind(sourceId, uploadId, targetType, targetId, purposeKey, `source/${token}`, "brief.pdf", checksum, actor.userId, now, companyId, now, now),
    db.prepare(`INSERT INTO attachment_sets(
      id,company_id,branch_id,department_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,
      policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at
    ) VALUES(?,?,?,?,?,?,'record',NULL,?,1,1,'confidential','{}',0,10,'draft',?,?,?)`)
      .bind(setId, companyId, branchId, departmentId, targetType, targetId, purposeKey, actor.userId, now, now),
    db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,?,?)")
      .bind(linkId, setId, sourceId, actor.userId, now, now + 60_000),
  ]);

  const prepared = await prepareAttachmentBinding(db, actor, {
    attachmentSetIds: [setId],
    bindingContract: {
      targetType,
      bindingType: "record_document",
      contexts: [{ purposeKey, componentKind: "record", componentRef: null }],
    },
    targetId,
    expectedTargetVersion: 1,
    commandId,
    bindingId: targetId,
  });
  return { commandId, linkId, prepared, setId };
}

async function expectAtomicGuardRejects(
  prepared: PreparedAttachmentBinding,
  commandId: string,
  setId: string,
  linkId: string,
) {
  await expect(db.batch([
    ...prepared.guards,
    ...prepared.steps,
    db.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(commandId),
  ])).rejects.toMatchObject({ code: "23514" });
  await expect(db.prepare("SELECT state FROM attachment_sets WHERE id=?").bind(setId).first("state")).resolves.toBe("draft");
  await expect(db.prepare("SELECT state FROM attachment_links WHERE id=?").bind(linkId).first("state")).resolves.toBe("draft");
}

describe("CRM attachment binding atomic authorization", () => {
  it.each([
    {
      name: "owner reassignment",
      mutate: () => db.prepare("UPDATE crm_leads SET owner_id=?,version=version+1,updated_at=? WHERE id=?")
        .bind(`replacement-owner-${key()}`, Date.now(), leadId).run(),
    },
    {
      name: "owner eligibility suspension",
      mutate: () => db.prepare("UPDATE hr_engagements SET status='suspended',version=version+1,updated_at=? WHERE id=?")
        .bind(Date.now(), engagementId).run(),
    },
    {
      name: "Branch scope drift",
      mutate: () => db.prepare("UPDATE crm_leads SET branch_id=?,version=version+1,updated_at=? WHERE id=?")
        .bind(otherBranchId, Date.now(), leadId).run(),
    },
    {
      name: "Department scope drift",
      mutate: () => db.prepare("UPDATE crm_leads SET department_id=?,version=version+1,updated_at=? WHERE id=?")
        .bind(otherDepartmentId, Date.now(), leadId).run(),
    },
  ])("rejects $name without relying on the unchanged opportunity version", async ({ mutate }) => {
    const binding = await prepareBinding("crm_opportunity", opportunityId);
    await mutate();
    expect(await db.prepare("SELECT version FROM crm_opportunities WHERE id=?").bind(opportunityId).first("version")).toBe(1);
    await expectAtomicGuardRejects(binding.prepared, binding.commandId, binding.setId, binding.linkId);
  });

  it("rejects CRM account binding after its only scoped lead leaves the actor's Branch", async () => {
    const binding = await prepareBinding("crm_account", accountId);
    await db.prepare("UPDATE crm_leads SET branch_id=?,version=version+1,updated_at=? WHERE id=?")
      .bind(otherBranchId, Date.now(), leadId).run();
    expect(await db.prepare("SELECT version FROM crm_accounts WHERE id=?").bind(accountId).first("version")).toBe(1);
    await expectAtomicGuardRejects(binding.prepared, binding.commandId, binding.setId, binding.linkId);
  });
});

async function packageGrant(company:string,packageId:"sales"|"sales-marketing",state:"active"|"draining"="active"){
 const {packageGrantStore,readCompanyPackageGrants}=await import("../../../src/server/modules/commercial/public/package-grants"),prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId===packageId);
 return packageGrantStore(db,{authorize:async()=>({userId:"isolated-crm-files-provider"}),obligations:async()=>{throw Error("CRM_FILES_FIXTURE_CANNOT_INACTIVATE");}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:"Isolated original CRM Marketing Files purchase",evidence:"test:crm-marketing-files"}});
}
async function nativeCRM(){
 const fixture=await expenseFixture(),lead=await createCRMLead(fixture.owner,{branchId:fixture.branch,title:"Exact original CRM file source",contactName:"Fictional prospect",contactDetail:"Declared synthetic channel",source:"Native staff inquiry",brief:"Original Company evidence and current Sales authority",dueDate:"2026-12-20"},key()),detail=await getCRMLead(fixture.owner,String(lead.id));
 return {fixture,lead,detail};
}
async function heldBytes(sourceId:string,body:string){const objectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(sourceId).first("object_key"));await env.H001_FILES.put(objectKey,body,{httpMetadata:{contentType:"application/pdf"}});await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(sourceId).run();return objectKey;}

it("checks actual Sales-only CRM files without Marketing and retains draining held bytes and exact replay under current authority",async()=>{
 const {fixture:f,detail}=await nativeCRM(),account=String(detail.account_id),file=await seedAttachmentSet(db,f.owner,{targetType:"crm_account",targetId:account,componentKind:"record",purposeKey:"brief",expectedTargetVersion:1}),body="Original fictional CRM brief retained under Legal hold";await heldBytes(file.sourceId,body);vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");
 await expect(bindRecordAttachmentSet(db,f.owner,file.setId,1,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await packageGrant(f.company,"sales");const command=key(),receipt=await bindRecordAttachmentSet(db,f.owner,file.setId,1,command);await packageGrant(f.company,"sales","draining");expect(await bindRecordAttachmentSet(db,f.owner,file.setId,1,command)).toEqual(receipt);expect(await (await attachmentContent(db,env.H001_FILES,f.owner,file.linkId,"original")).object.text()).toBe(body);
 const {readCompanyPackageGrants}=await import("../../../src/server/modules/commercial/public/package-grants");expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId)).toEqual(["sales"]);await db.prepare("UPDATE assignments SET revoked_at=1 WHERE id=?").bind(f.owner.assignmentId).run();await expect(bindRecordAttachmentSet(db,f.owner,file.setId,1,command)).rejects.toMatchObject({status:404});await expect(attachmentContent(db,env.H001_FILES,f.owner,file.linkId,"original")).rejects.toMatchObject({status:404});
});

it("rolls back CRM opportunity file binding on a final actual contact lineage change with unchanged original opportunity version",async()=>{
 const {fixture:f,lead,detail}=await nativeCRM(),foreign=await nativeCRM();f.tie.role="sales_manager";f.tie.branchId=null;f.tie.departmentId=null;f.tie.assignmentId=await f.grant(f.tie,"sales_manager");const customer=String((await saveMaster(f.owner,"parties",null,{kind:"customer",code:key(),name:"Reviewed native CRM customer"},key())).id);await changeCRMIdentity(f.tie,{action:"link-finance",accountId:detail.account_id,customerId:customer,expectedVersion:1,evidence:"Independent review of the real original customer"},key());await changeCRMLead(f.owner,String(lead.id),{action:"qualify",expectedVersion:1,customerId:customer,expectedValue:"25",closeDate:"2026-12-20",reason:"Original actual Company prospect qualified"},key());
 const id=String(await db.prepare("SELECT id FROM crm_opportunities WHERE lead_id=?").bind(lead.id).first("id")),file=await seedAttachmentSet(db,f.owner,{targetType:"crm_opportunity",targetId:id,componentKind:"record",purposeKey:"brief",expectedTargetVersion:1}),command=key(),fault=`crm_file_source_${key().replaceAll("-","")}`;vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");await packageGrant(f.company,"sales");
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE crm_leads SET contact_id='${foreign.detail.contact_id}' WHERE id='${lead.id}';RETURN NEW;END;$$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(bindRecordAttachmentSet(db,f.owner,file.setId,1,command)).rejects.toMatchObject({code:"CRM_ATTACHMENT_CHANGED"});}finally{await db.prepare(`DROP TRIGGER ${fault} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare("SELECT contact_id FROM crm_leads WHERE id=?").bind(lead.id).first("contact_id")).toBe(detail.contact_id);expect(await db.prepare("SELECT version FROM crm_opportunities WHERE id=?").bind(id).first("version")).toBe(1);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:"draft",link_state:"draft"});for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();const receipt=await bindRecordAttachmentSet(db,f.owner,file.setId,1,command);expect(await bindRecordAttachmentSet(db,f.owner,file.setId,1,command)).toEqual(receipt);
});

it("rolls back CRM account Files and immutable receipts after deferred current IAM revocation then retries the original command",async()=>{
 const {fixture:f,detail}=await nativeCRM(),id=String(detail.account_id),file=await seedAttachmentSet(db,f.owner,{targetType:"crm_account",targetId:id,componentKind:"record",purposeKey:"brief",expectedTargetVersion:1}),command=key(),fault=`crm_file_iam_${key().replaceAll("-","")}`;vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");await packageGrant(f.company,"sales");
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}';RETURN NEW;END;$$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(bindRecordAttachmentSet(db,f.owner,file.setId,1,command)).rejects.toMatchObject({status:409});}finally{await db.prepare(`DROP TRIGGER ${fault} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare("SELECT revoked_at FROM assignments WHERE id=?").bind(f.owner.assignmentId).first("revoked_at")).toBeNull();expect(await db.prepare("SELECT version FROM crm_accounts WHERE id=?").bind(id).first("version")).toBe(1);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:"draft",link_state:"draft"});for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();await bindRecordAttachmentSet(db,f.owner,file.setId,1,command);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:"bound",link_state:"bound"});
});

it("requires the Marketing addon for actual creative files while preserving retained campaign settlement and exact creative retry",async()=>{
 const f=await expenseFixture(),owner={...f.owner,modules:["marketing"]};await db.prepare("UPDATE assignments SET modules_json='[\"marketing\"]' WHERE id=?").bind(owner.assignmentId).run();const asset=await createMarketingAsset(owner,{kind:"content",name:"Reviewed fictional creative source",configuration:{},preview:{headline:"Declared synthetic content"}},key()),campaign=await changeMarketingCampaign(owner,{action:"create",code:`M${key().slice(0,8)}`,name:"Original retained campaign"},key()),file=await seedAttachmentSet(db,owner,{targetType:"marketing_asset",targetId:String(asset.id),componentKind:"record",purposeKey:"source",expectedTargetVersion:1}),evidence=await seedAttachmentSet(db,owner,{targetType:"marketing_campaign",targetId:String(campaign.id),componentKind:"record",purposeKey:"source",expectedTargetVersion:1}),body="Private creative source retained with original Legal hold";await heldBytes(file.sourceId,body);vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");await packageGrant(f.company,"sales");await expect(bindRecordAttachmentSet(db,owner,file.setId,1,key())).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});await packageGrant(f.company,"sales-marketing");const command=key(),receipt=await bindRecordAttachmentSet(db,owner,file.setId,1,command),next=await seedAttachmentSet(db,owner,{targetType:"marketing_asset",targetId:String(asset.id),componentKind:"record",purposeKey:"source",expectedTargetVersion:2});await packageGrant(f.company,"sales-marketing","draining");
 expect(await bindRecordAttachmentSet(db,owner,file.setId,1,command)).toEqual(receipt);expect(await (await attachmentContent(db,env.H001_FILES,owner,file.linkId,"original")).object.text()).toBe(body);await expect(bindRecordAttachmentSet(db,owner,next.setId,2,key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});const newCreative=await seedAttachmentSet(db,owner,{targetType:"marketing_campaign",targetId:String(campaign.id),componentKind:"record",purposeKey:"creative",expectedTargetVersion:1,mediaType:"image/png"});await expect(bindRecordAttachmentSet(db,owner,newCreative.setId,1,key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});await bindRecordAttachmentSet(db,owner,evidence.setId,1,key());expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:"bound",link_state:"bound"});const {readCompanyPackageGrants}=await import("../../../src/server/modules/commercial/public/package-grants");expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId).sort()).toEqual(["sales","sales-marketing"]);
});

it("rechecks original Marketing module selection after deferred file effects and preserves exact retry without broadening authority",async()=>{
 const f=await expenseFixture(),owner={...f.owner,modules:["marketing"]};await db.prepare("UPDATE assignments SET modules_json='[\"marketing\"]' WHERE id=?").bind(owner.assignmentId).run();const asset=await createMarketingAsset(owner,{kind:"content",name:"Exact private Marketing file source",configuration:{},preview:{}},key()),file=await seedAttachmentSet(db,owner,{targetType:"marketing_asset",targetId:String(asset.id),componentKind:"record",purposeKey:"source",expectedTargetVersion:1}),command=key(),fault=`marketing_file_module_${key().replaceAll("-","")}`;vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");await packageGrant(f.company,"sales");await packageGrant(f.company,"sales-marketing");await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET modules_json='["commerce"]' WHERE id='${owner.assignmentId}';RETURN NEW;END;$$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}();`);
 try{await expect(bindRecordAttachmentSet(db,owner,file.setId,1,command)).rejects.toMatchObject({code:"MARKETING_ATTACHMENT_CHANGED"});}finally{await db.prepare(`DROP TRIGGER ${fault} ON command_receipts`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 expect(await db.prepare("SELECT modules_json FROM assignments WHERE id=?").bind(owner.assignmentId).first("modules_json")).toBe('["marketing"]');expect(await db.prepare("SELECT version FROM marketing_assets WHERE id=?").bind(asset.id).first("version")).toBe(1);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:"draft",link_state:"draft"});for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT 1 FROM ${table} WHERE command_id=?`).bind(command).first()).toBeNull();await bindRecordAttachmentSet(db,owner,file.setId,1,command);expect(await attachmentBindingState(db,file.setId)).toMatchObject({set_state:"bound",link_state:"bound"});
});

it("serializes actual separate Company CRM public Files binds without automatic deadlock retries or crossed receipts",async()=>{
 const first=await nativeCRM(),second=await nativeCRM(),files=await Promise.all([first,second].map(source=>seedAttachmentSet(db,source.fixture.owner,{targetType:"crm_account",targetId:String(source.detail.account_id),componentKind:"record",purposeKey:"brief",expectedTargetVersion:1}))),commands=[key(),key()],fault=`crm_file_parallel_${key().replaceAll("-","")}`;
 await packageGrant(first.fixture.company,"sales");await packageGrant(second.fixture.company,"sales");vi.stubEnv("ERPX_COMMERCE_PACKAGE_ENFORCEMENT","enforced");await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.command_id LIKE 'crm-file-before:%' THEN PERFORM pg_sleep(0.1);END IF;RETURN NEW;END;$$;CREATE TRIGGER ${fault} INSTEAD OF INSERT ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${fault}();`);
 try{const measured=await measureReads(()=>Promise.all([first,second].map((source,index)=>bindRecordAttachmentSet(db,source.fixture.owner,files[index].setId,1,commands[index]))));expect(measured.result).toHaveLength(2);expect(measured.metrics.deadlockFailures??0).toBe(0);}finally{await db.prepare(`DROP TRIGGER ${fault} ON command_assertions`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
 for(const [index,source] of [first,second].entries()){expect(await db.prepare("SELECT company_id,version FROM crm_accounts WHERE id=?").bind(source.detail.account_id).first()).toEqual({company_id:source.fixture.company,version:2});expect(await attachmentBindingState(db,files[index].setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"record_document"});expect(await db.prepare("SELECT count(*)::int count FROM command_receipts WHERE command_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(commands[index],source.fixture.owner.userId,source.fixture.company).first("count")).toBe(1);await bindRecordAttachmentSet(db,source.fixture.owner,files[index].setId,1,commands[index]);}
});
