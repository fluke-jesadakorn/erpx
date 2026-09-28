import { afterAll,beforeAll,beforeEach,describe,expect,it,vi } from "vitest";
import { createHash } from "node:crypto";
import type { WorkContext } from "../src/server/authorization";
import { applyMigrations,env } from "./runtime-env";

vi.mock("../src/server/attachments/scanner",()=>({
  attachmentScannerReadiness:async()=>({enabled:true,ready:true,engine:"ClamAV test",definitionUpdatedAt:Date.now(),error:null}),
  scanAttachmentBytes:async()=>({clean:true,signature:null,engine:"ClamAV test",definitionUpdatedAt:Date.now()}),
}));
vi.mock("../src/server/attachments/preview",()=>({
  attachmentPreviewReadiness:async()=>({ready:true,heifConvert:"/test/heif-convert",pdftoppm:"/test/pdftoppm",error:null}),
  detectedAttachmentMediaType:(_bytes:Uint8Array,declared:string)=>declared,
  sanitizedAttachmentPreview:async(_bytes:Uint8Array,mediaType:string)=>mediaType.startsWith("image/")?{bytes:new Uint8Array([82,73,70,70,4,0,0,0,87,69,66,80]),mediaType:"image/webp" as const}:null,
}));

import { attachmentContent,attachmentDestructionEligibility,attachmentLinkStatus,approveAttachmentPublication,bindCompatibilityAttachmentSet,bindRecordAttachmentSet,cleanupAttachmentDrafts,createAttachmentPublication,createAttachmentSet,drainAttachmentScans,linkExistingAttachment,listAttachmentPublications,listAttachmentSets,prepareAttachmentBinding,publicAttachmentPublication,reorderAttachmentPublications,reserveAttachmentFile,revokeAttachmentPublication,scanAttachmentNow,completeAttachmentUpload,withdrawAttachmentLink } from "../src/server/attachments/service";
import { attachmentPolicy,attachmentPolicyRegistry,attachmentRetentionSnapshot } from "../src/server/attachments/policy";
import {postView,publishPost,savePostDraft,type PostContent} from "../src/server/social-home";
import { listBrandProfiles } from "../src/server/branding";
import { backfillLegacyAttachments,backfillSpecializedAttachments,reconcileAttachmentStorage } from "../src/server/attachments/migration";
import { sha256 } from "../src/server/crypto";
import { legacyAttachmentPolicy } from "../src/server/attachments/legacy-policy";

const db=env.H001_DB,key=()=>crypto.randomUUID();
let companyA:string,companyB:string,productA:string,productB:string,campaignA:string,actorA:WorkContext,peerA:WorkContext,actorB:WorkContext;
const png=Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=","base64");
function actor(companyId:string,userId:string):WorkContext{return {userId,name:userId,email:`${userId}@example.test`,assignmentId:`assignment-${userId}`,role:"holding_owner",companyId,branchId:null,departmentId:null,holdingId:"h001",holdingSlug:"acme",holdingName:"Test",scope:{kind:"holding",holdingId:"h001",holdingSlug:"acme",holdingName:"Test",companyId},assignmentScope:"holding",inherited:false,modules:[],assignments:[],workspaces:[]};}

beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);process.env.ERPX_ATTACHMENTS_V2="true";delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;});
afterAll(()=>{delete process.env.ERPX_ATTACHMENTS_V2;delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;});
beforeEach(async()=>{
  companyA=`attachment-co-${key()}`;companyB=`attachment-co-${key()}`;productA=`product-${key()}`;productB=`product-${key()}`;campaignA=`campaign-${key()}`;
  actorA=actor(companyA,`owner-${key()}`);peerA=actor(companyA,`peer-${key()}`);actorB=actor(companyB,`other-${key()}`);
  await db.batch([
    db.prepare("INSERT INTO beta_policy VALUES(?,20,5,100,1073741824,1)").bind(companyA),
    db.prepare("INSERT INTO beta_policy VALUES(?,20,5,100,1073741824,1)").bind(companyB),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Attachment company',?)").bind(companyA,companyA,companyA),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Other attachment company',?)").bind(companyB,companyB,companyB),
    ...[actorA,peerA,actorB].map(context=>db.prepare("INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until) VALUES(?,?,? ,?,'holding','[]',0,9999999999999)").bind(context.assignmentId,context.userId,context.role,context.companyId)),
    db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(productA,companyA,`SKU-${key()}`,"Attachment product"),
    db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(productB,companyB,`SKU-${key()}`,"Other Company product"),
    db.prepare("INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES(?,?,?,?,?,?)").bind(campaignA,companyA,`CAM-${key()}`,"Attachment campaign",actorA.userId,Date.now()),
  ]);
});

async function availableProductFile(setId:string,command=key()){
  const reserved=await reserveAttachmentFile(db,actorA,setId,{filename:"product.png",mediaType:"image/png",bytes:png.byteLength},command);
  const uploaded=await completeAttachmentUpload(db,env.H001_FILES,actorA,reserved.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));
  expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);
  return {...reserved,...uploaded};
}

describe("canonical attachment lifecycle",()=>{
  it("keeps a closed exact policy inventory",()=>{
    const combinations=attachmentPolicyRegistry.flatMap(policy=>policy.componentKinds.map(kind=>`${policy.targetType}\0${policy.purposeKey}\0${kind}`));
    const exactInventoryHash=createHash("sha256").update([...combinations].sort().join("\n")).digest("hex");
    expect(attachmentPolicyRegistry).toHaveLength(201);
    expect(combinations).toHaveLength(216);
    expect(exactInventoryHash).toBe("bf3ed07afb05911fb8a57061075a914f757feb0af1ab1f2fd124b97f017135b9");
    expect(new Set(combinations).size).toBe(combinations.length);
    expect(attachmentPolicyRegistry.every(policy=>policy.maxFiles>=1&&policy.maxFiles<=100)).toBe(true);
    expect(attachmentPolicy("product","product_image","record").componentKinds).toEqual(["record"]);
    expect(attachmentPolicy("inventory_transit","receipt_evidence","event").componentKinds).toEqual(["event"]);
    expect(attachmentPolicy("purchase_order","supplier_promise","action").classification).toBe("confidential");
    expect(attachmentPolicy("supplier_bill_match","tax_document","record").maxFiles).toBe(50);
    expect(attachmentPolicy("inventory_transfer","direct_receipt_evidence","action").classification).toBe("confidential");
    expect(attachmentPolicy("inventory_quality_spec","controlled_specification","record").maxFiles).toBe(50);
    expect(attachmentPolicy("ecommerce_listing","listing_media","record").publication).toBe(true);
    expect(attachmentPolicy("marketing_asset","creative","record").publication).toBe(true);
    expect(attachmentPolicy("manufacturing_spec","formula","record").classification).toBe("restricted");
    expect(attachmentPolicy("production_run","qc_evidence","action").classification).toBe("confidential");
    expect(attachmentPolicy("planning_scenario","capacity_source","record").maxFiles).toBe(50);
    expect(attachmentRetentionSnapshot(attachmentPolicy("legal_item","legal_document","record"))).toEqual({key:"legal-managed",version:1,retainUntil:null,destructionMode:"review"});
    expect(legacyAttachmentPolicy("support_ticket","support_evidence")).toEqual({purposeKey:"internal_evidence",componentKind:"action",componentRef:"support-evidence:internal_evidence",completion:"bind_immediately"});
    expect(legacyAttachmentPolicy("leave_request","evidence")).toEqual({purposeKey:"leave_supporting_photo",componentKind:"record",componentRef:"leave-evidence:leave_supporting_photo",completion:"stage_for_domain"});
    expect(legacyAttachmentPolicy("leave_request","leave evidence")).toBeNull();
    expect(attachmentPolicy("staff_task","task_evidence","action").mediaTypes).not.toContain("text/csv");
    expect(legacyAttachmentPolicy("expense_claim","merchant_receipt")).toMatchObject({componentKind:"record",completion:"stage_for_domain"});
    expect(legacyAttachmentPolicy("quality_hold","quality_inspection_evidence")).toEqual({purposeKey:"quality_inspection_evidence",componentKind:"action",componentRef:"inspection",completion:"stage_for_domain"});
    expect(legacyAttachmentPolicy("commerce_order_delivery","proof_of_delivery")).toEqual({purposeKey:"proof_of_delivery",componentKind:"action",componentRef:"delivery",completion:"stage_for_domain"});
    expect(legacyAttachmentPolicy("expense_claim","unregistered_receipt_kind")).toBeNull();
    expect(()=>attachmentPolicy("product","product_image","action")).toThrow(/not registered/i);
    expect(()=>attachmentPolicy("unknown_owner","evidence","record")).toThrow(/not registered/i);
  });

  it("quarantines a historical source whose legacy purpose is not in the closed inventory",async()=>{
    const token=key(),uploadId=`legacy-unknown-upload-${token}`,sourceId=`legacy-unknown-source-${token}`,now=Date.now();
    await expect(db.transaction(async tx=>{
      await tx.batch([
        tx.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at)
          VALUES(?,'holding_brand','h001','unregistered_brand_variant','legacy.png','image/png',1,1,?,?,?,'ready',?,?,?,?)`).bind(uploadId,"0".repeat(64),`legacy/${token}`,`legacy/${token}`,actorA.userId,now+60_000,now,now),
        tx.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at)
          VALUES(?,?,'holding_brand','h001','unregistered_brand_variant',?,1,'legacy.png','image/png',1,?,?,?)`).bind(sourceId,uploadId,`legacy/${token}`,"0".repeat(64),actorA.userId,now),
      ]);
      const report=await backfillLegacyAttachments(tx,{apply:true});
      expect(report.unknown).toContainEqual({id:sourceId,ownerType:"holding_brand",ownerId:"h001",reason:"POLICY_UNKNOWN"});
      expect(await tx.prepare("SELECT company_id,source_state FROM attachments WHERE id=?").bind(sourceId).first()).toEqual({company_id:null,source_state:"quarantined"});
      expect(await tx.prepare("SELECT count(*) count FROM attachment_links WHERE attachment_id=?").bind(sourceId).first("count")).toBe(0);
      throw new Error("ROLLBACK_UNKNOWN_LEGACY_FIXTURE");
    })).rejects.toThrow("ROLLBACK_UNKNOWN_LEGACY_FIXTURE");
  });

  it("preserves the exact legacy component context while backfilling a known source",async()=>{
    const token=key(),draftId=`legacy-draft-${token}`,uploadId=`legacy-upload-${token}`,sourceId=`legacy-source-${token}`,objectKey=`legacy/${token}`,now=Date.now();
    await expect(db.transaction(async tx=>{
      await tx.batch([
        tx.prepare("INSERT INTO social_post_drafts(id,author_id,company_id,body,version,created_at,updated_at) VALUES(?,?,?,'Legacy image post',1,?,?)").bind(draftId,actorA.userId,companyA,now,now),
        tx.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at)
          VALUES(?,'social_draft',?,'post-image','legacy.png','image/png',1,1,?,?,?,'ready',?,?,?,?)`).bind(uploadId,draftId,"0".repeat(64),`legacy/staging/${token}`,objectKey,actorA.userId,now+60_000,now,now),
        tx.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at)
          VALUES(?,?,'social_draft',?,'post-image',?,1,'legacy.png','image/png',1,?,?,?)`).bind(sourceId,uploadId,draftId,objectKey,"0".repeat(64),actorA.userId,now),
      ]);
      const report=await backfillLegacyAttachments(tx,{apply:true});
      expect(report).toMatchObject({ready:1,backfilled:1,unknown:[]});
      expect(await tx.prepare("SELECT s.component_ref,s.purpose_key,s.company_id FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id WHERE l.attachment_id=?").bind(sourceId).first())
        .toEqual({component_ref:"legacy-upload-adapter",purpose_key:"post_image",company_id:companyA});
      throw new Error("ROLLBACK_KNOWN_LEGACY_FIXTURE");
    })).rejects.toThrow("ROLLBACK_KNOWN_LEGACY_FIXTURE");
  });

  it("authorizes and versions a real marketing asset owner without cross-Company leakage",async()=>{
    const marketingActor={...actorA,modules:["marketing"]},foreignActor={...actorB,modules:["marketing"]},assetId=`marketing-asset-${key()}`,formId=`marketing-form-${key()}`;
    await db.prepare("INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,version,created_at,updated_at) VALUES(?,?,'content','Canonical campaign image','{}','{}','draft',?,1,?,?)").bind(assetId,companyA,marketingActor.userId,Date.now(),Date.now()).run();
    await db.prepare("INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,version,created_at,updated_at) VALUES(?,?,'form','Lead form','{}','{}','draft',?,1,?,?)").bind(formId,companyA,marketingActor.userId,Date.now(),Date.now()).run();
    await expect(createAttachmentSet(db,actorA,{targetType:"marketing_asset",targetId:assetId,componentKind:"record",purposeKey:"source",expectedTargetVersion:1},key())).rejects.toMatchObject({status:403,code:"ATTACHMENT_FORBIDDEN"});
    await expect(createAttachmentSet(db,marketingActor,{targetType:"marketing_asset",targetId:formId,componentKind:"record",purposeKey:"creative",expectedTargetVersion:1},key())).rejects.toMatchObject({status:422,code:"ATTACHMENT_PURPOSE_OWNER"});
    await expect(createAttachmentSet(db,foreignActor,{targetType:"marketing_asset",targetId:assetId,componentKind:"record",purposeKey:"source",expectedTargetVersion:1},key())).rejects.toMatchObject({status:404,code:"ATTACHMENT_TARGET_NOT_FOUND"});
    const set=await createAttachmentSet(db,marketingActor,{targetType:"marketing_asset",targetId:assetId,componentKind:"record",purposeKey:"source",expectedTargetVersion:1},key());
    actorA=marketingActor;
    await availableProductFile(set.id);
    const bound=await bindRecordAttachmentSet(db,marketingActor,set.id,1,key());
    expect(bound.version).toBe(2);
    expect(await db.prepare("SELECT version FROM marketing_assets WHERE id=?").bind(assetId).first("version")).toBe(2);
    await expect(createAttachmentSet(db,marketingActor,{targetType:"marketing_asset",targetId:assetId,componentKind:"record",purposeKey:"source",expectedTargetVersion:1},key())).rejects.toMatchObject({status:409,code:"ATTACHMENT_TARGET_CHANGED"});
  });

  it("does not treat generic record permissions as CRM or Marketing file authority",async()=>{
    const recordsOnly={...actorA,role:"custom",modules:[],permissions:["records.read","records.write"]} as WorkContext&{permissions:string[]};
    await expect(createAttachmentSet(db,recordsOnly,{targetType:"marketing_campaign",targetId:campaignA,componentKind:"record",purposeKey:"source",expectedTargetVersion:1},key()))
      .rejects.toMatchObject({status:403,code:"ATTACHMENT_FORBIDDEN"});
    await expect(createAttachmentSet(db,{...recordsOnly,modules:["marketing"]},{targetType:"marketing_campaign",targetId:campaignA,componentKind:"record",purposeKey:"source",expectedTargetVersion:1},key()))
      .resolves.toMatchObject({state:"draft"});

    const accountId=`crm-account-${key()}`,branchId=`crm-branch-${key()}`,customerId=`crm-customer-${key()}`,leadId=`crm-lead-${key()}`,opportunityId=`crm-opportunity-${key()}`,now=Date.now();
    await db.batch([
      db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'branch',?,'CRM branch',?)").bind(branchId,branchId,companyA),
      db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'CRM customer','THB',?,?)").bind(customerId,companyA,customerId,now,now),
      db.prepare("INSERT INTO crm_accounts(id,company_id,kind,name,created_by,created_at,updated_at) VALUES(?,?,'business','Restricted CRM account',?,?,?)").bind(accountId,companyA,actorA.userId,now,now),
      db.prepare("INSERT INTO crm_leads(id,company_id,branch_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,created_by,created_at,updated_at,account_id) VALUES(?,?,?,'Restricted deal','Customer','Private contact','Direct','Confidential brief',?,'2026-12-31','qualified',?,?,?,?)").bind(leadId,companyA,branchId,actorA.userId,actorA.userId,now,now,accountId),
      db.prepare("INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at) VALUES(?,?,?,10000,'THB','2026-12-31',?,?,?)").bind(opportunityId,leadId,customerId,actorA.userId,now,now),
    ]);
    await expect(createAttachmentSet(db,recordsOnly,{targetType:"crm_account",targetId:accountId,componentKind:"record",purposeKey:"brief",expectedTargetVersion:1},key()))
      .rejects.toMatchObject({status:403,code:"ATTACHMENT_FORBIDDEN"});
    await expect(createAttachmentSet(db,recordsOnly,{targetType:"crm_opportunity",targetId:opportunityId,componentKind:"record",purposeKey:"rfp",expectedTargetVersion:1},key()))
      .rejects.toMatchObject({status:403,code:"ATTACHMENT_FORBIDDEN"});
  });

  it("replays concurrent set/file reservations and hides another creator's draft metadata",async()=>{
    const input={targetType:"product",targetId:productA,componentKind:"record" as const,purposeKey:"product_image",expectedTargetVersion:1},command=key();
    const [first,replay]=await Promise.all([createAttachmentSet(db,actorA,input,command),createAttachmentSet(db,actorA,input,command)]);
    expect(replay.id).toBe(first.id);
    expect(await listAttachmentSets(db,peerA,{targetType:"product",targetId:productA})).toEqual([]);
    const file={filename:"product.png",mediaType:"image/png",bytes:png.byteLength},reserveKey=key(),[reserved,reservedReplay]=await Promise.all([reserveAttachmentFile(db,actorA,first.id,file,reserveKey),reserveAttachmentFile(db,actorA,first.id,file,reserveKey)]);
    expect(reservedReplay).toEqual(reserved);
    await expect(reserveAttachmentFile(db,actorA,first.id,{...file,caption:"A different payload"},reserveKey)).rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});
    const second=await createAttachmentSet(db,actorA,{...input,purposeKey:"specification"},key());
    await expect(reserveAttachmentFile(db,actorA,second.id,file,reserveKey)).rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});
    const bytes=png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength);
    const [uploaded,replayedUpload]=await Promise.all([
      completeAttachmentUpload(db,env.H001_FILES,actorA,reserved.uploadIntentId,bytes),
      completeAttachmentUpload(db,env.H001_FILES,actorA,reserved.uploadIntentId,bytes),
    ]);
    expect(replayedUpload.sourceId).toBe(uploaded.sourceId);
    expect(uploaded.status).toBe("scan_pending");
    await expect(reserveAttachmentFile(db,actorB,first.id,file,key())).rejects.toMatchObject({status:404,code:"ATTACHMENT_SET_NOT_FOUND"});
  });

  it("keeps the one-release compatibility lifecycle canonical while the UI gate is off",async()=>{
    const previousGate=process.env.ERPX_ATTACHMENTS_V2,previousRollback=process.env.ERPX_ATTACHMENTS_READ_ONLY;
    process.env.ERPX_ATTACHMENTS_V2="false";delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
    try{
      const options={compatibilityAdapter:true} as const,set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",componentRef:"legacy-upload-adapter",purposeKey:"specification",expectedTargetVersion:1},key(),options);
      await expect(createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:1},key())).rejects.toMatchObject({status:404,code:"ATTACHMENTS_DISABLED"});
      const reserved=await reserveAttachmentFile(db,actorA,set.id,{filename:"legacy.pdf",mediaType:"application/pdf",bytes:png.byteLength},key(),options);
      const uploaded=await completeAttachmentUpload(db,env.H001_FILES,actorA,reserved.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength),options);
      expect(uploaded).toMatchObject({state:"quarantined",status:"scan_pending"});
      expect(await scanAttachmentNow(db,env.H001_FILES,uploaded.sourceId)).toBe("available");
      const bound=await bindCompatibilityAttachmentSet(db,actorA,set.id,1,key(),options);
      expect(bound).toMatchObject({id:uploaded.sourceId,ownerVersion:2,state:"available"});
      expect(await db.prepare("SELECT state FROM attachment_links WHERE id=?").bind(reserved.attachmentLinkId).first("state")).toBe("bound");

      process.env.ERPX_ATTACHMENTS_READ_ONLY="true";
      await expect(createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:2},key(),options)).rejects.toMatchObject({status:503,code:"ATTACHMENTS_READ_ONLY"});
    }finally{
      if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;
      if(previousRollback===undefined)delete process.env.ERPX_ATTACHMENTS_READ_ONLY;else process.env.ERPX_ATTACHMENTS_READ_ONLY=previousRollback;
    }
  });

  it("preserves the Social composer upload contract through canonical scan and binding",async()=>{
    const slotId=key(),pending:PostContent=[{type:"text",text:"Store opening update"},{type:"image",slotId,attachmentId:null,name:"opening.png",alt:"Store entrance"}];
    const draft=await savePostDraft(db,actorA,null,{body:"Store opening update",content:pending,branchId:null,expectedVersion:1},key());
    const set=await createAttachmentSet(db,actorA,{targetType:"social_draft",targetId:draft.id,componentKind:"record",componentRef:"legacy-upload-adapter",purposeKey:"post_image",expectedTargetVersion:draft.version},key());
    const reserved=await reserveAttachmentFile(db,actorA,set.id,{filename:"opening.png",mediaType:"image/png",bytes:png.byteLength},key());
    const uploaded=await completeAttachmentUpload(db,env.H001_FILES,actorA,reserved.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));
    await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId);
    const compatible=await bindCompatibilityAttachmentSet(db,actorA,set.id,draft.version,`legacy-upload-bind-${reserved.uploadIntentId}`);
    expect(compatible).toMatchObject({id:uploaded.sourceId,ownerVersion:2,attachmentSetId:set.id,attachmentLinkId:reserved.attachmentLinkId,state:"available"});
    const content:PostContent=[{type:"text",text:"Store opening update"},{type:"image",slotId,attachmentId:compatible.id,name:"opening.png",alt:"Store entrance"}];
    const finalized=await savePostDraft(db,actorA,draft.id,{body:"Store opening update",content,branchId:null,expectedVersion:compatible.ownerVersion},key());
    const published=await publishPost(db,actorA,draft.id,finalized.version,key()),view=await postView(db,actorA,published.id);
    expect(view.content).toEqual(content);expect(view.files.map(file=>file.id)).toEqual([uploaded.sourceId]);
    expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(reserved.uploadIntentId).first("status")).toBe("ready");
    expect(await db.prepare("SELECT state FROM attachment_sets WHERE id=?").bind(set.id).first("state")).toBe("bound");
  });

  it("enforces the 10 MiB boundary, set count and concurrent Company quota",async()=>{
    const marketingActor={...actorA,modules:["marketing"]};
    const set=await createAttachmentSet(db,marketingActor,{targetType:"marketing_campaign",targetId:campaignA,componentKind:"record",purposeKey:"creative",expectedTargetVersion:1},key());
    await expect(reserveAttachmentFile(db,marketingActor,set.id,{filename:"boundary.png",mediaType:"image/png",bytes:10*1024*1024},key())).resolves.toMatchObject({attachmentLinkId:expect.any(String)});
    await expect(reserveAttachmentFile(db,marketingActor,set.id,{filename:"too-large.png",mediaType:"image/png",bytes:10*1024*1024+1},key())).rejects.toMatchObject({status:422,code:"ATTACHMENT_SIZE"});
    for(let index=1;index<10;index++)await reserveAttachmentFile(db,marketingActor,set.id,{filename:`${index}.png`,mediaType:"image/png",bytes:1},key());
    await expect(reserveAttachmentFile(db,marketingActor,set.id,{filename:"eleven.png",mediaType:"image/png",bytes:1},key())).rejects.toMatchObject({status:422,code:"ATTACHMENT_LIMIT"});

    const quotaCompany=`attachment-quota-${key()}`,quotaCampaign=`campaign-${key()}`,quotaActor={...actor(quotaCompany,`quota-${key()}`),modules:["marketing"]};
    await db.batch([
      db.prepare("INSERT INTO beta_policy VALUES(?,20,5,100,15,1)").bind(quotaCompany),
      db.prepare("INSERT INTO assignments(id,user_id,role,company_id,scope_kind,modules_json,valid_from,valid_until) VALUES(?,?,? ,?,'holding','[]',0,9999999999999)").bind(quotaActor.assignmentId,quotaActor.userId,quotaActor.role,quotaCompany),
      db.prepare("INSERT INTO marketing_campaigns(id,company_id,code,name,created_by,created_at) VALUES(?,?,?,?,?,?)").bind(quotaCampaign,quotaCompany,`CAM-${key()}`,"Quota campaign",quotaActor.userId,Date.now()),
    ]);
    const [source,consent]=await Promise.all([
      createAttachmentSet(db,quotaActor,{targetType:"marketing_campaign",targetId:quotaCampaign,componentKind:"record",purposeKey:"source",expectedTargetVersion:1},key()),
      createAttachmentSet(db,quotaActor,{targetType:"marketing_campaign",targetId:quotaCampaign,componentKind:"record",purposeKey:"consent",expectedTargetVersion:1},key()),
    ]);
    const results=await Promise.allSettled([
      reserveAttachmentFile(db,quotaActor,source.id,{filename:"source.png",mediaType:"image/png",bytes:10},key()),
      reserveAttachmentFile(db,quotaActor,consent.id,{filename:"consent.png",mediaType:"image/png",bytes:10},key()),
    ]);
    expect(results.filter(result=>result.status==="fulfilled")).toHaveLength(1);
    expect(results.filter(result=>result.status==="rejected")).toHaveLength(1);
  });

  it("quarantines, scans, binds immutably and denies a cross-Company guessed link",async()=>{
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:1},key()),reserved=await reserveAttachmentFile(db,actorA,set.id,{filename:"product.png",mediaType:"image/png",bytes:png.byteLength},key());
    const uploaded=await completeAttachmentUpload(db,env.H001_FILES,actorA,reserved.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));
    expect(uploaded.state).toBe("quarantined");expect(await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId)).toBe(true);
    await expect(attachmentLinkStatus(db,actorB,reserved.attachmentLinkId)).rejects.toMatchObject({status:404,code:"ATTACHMENT_LINK_NOT_FOUND"});
    await expect(attachmentContent(db,env.H001_FILES,actorB,reserved.attachmentLinkId,"original")).rejects.toMatchObject({status:404,code:"ATTACHMENT_LINK_NOT_FOUND"});
    await expect(withdrawAttachmentLink(db,actorB,reserved.attachmentLinkId,"foreign withdrawal",key())).rejects.toMatchObject({status:404,code:"ATTACHMENT_LINK_NOT_FOUND"});
    await expect(prepareAttachmentBinding(db,actorB,{attachmentSetIds:[set.id],bindingContract:{targetType:"product",bindingType:"record_document",contexts:[{purposeKey:"product_image",componentKind:"record",componentRef:null}]},targetId:productA,expectedTargetVersion:1,commandId:key(),bindingId:productA})).rejects.toMatchObject({status:404,code:"ATTACHMENT_SET_NOT_FOUND"});
    const bound=await bindRecordAttachmentSet(db,actorA,set.id,1,key());expect(bound.version).toBe(2);
    expect((await listAttachmentSets(db,peerA,{targetType:"product",targetId:productA})).map(item=>item.id)).toContain(set.id);
    await expect(db.prepare("UPDATE attachment_sets SET purpose_key='specification' WHERE id=?").bind(set.id).run()).rejects.toThrow(/BOUND_ATTACHMENT_SET_IMMUTABLE/);
    await expect(db.prepare("UPDATE attachment_sets SET updated_at=updated_at+1 WHERE id=?").bind(set.id).run()).rejects.toThrow(/BOUND_ATTACHMENT_SET_IMMUTABLE/);
    for(const [column,value] of [["object_key","tampered"],["media_type","image/jpeg"],["purpose","tampered"],["revision",99],["created_at",1],["company_id",companyB]] as const)
      await expect(db.prepare(`UPDATE attachments SET ${column}=? WHERE id=?`).bind(value,uploaded.sourceId).run()).rejects.toThrow(/ATTACHMENT_SOURCE_IMMUTABLE/);
    await expect(db.prepare("DELETE FROM attachments WHERE id=?").bind(uploaded.sourceId).run()).rejects.toThrow(/ATTACHMENT_SOURCE_IMMUTABLE/);

    const foreignSet=await createAttachmentSet(db,actorB,{targetType:"product",targetId:productB,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key());
    await expect(linkExistingAttachment(db,actorB,foreignSet.id,{sourceLinkId:reserved.attachmentLinkId},key())).rejects.toMatchObject({status:404,code:"ATTACHMENT_SOURCE_NOT_FOUND"});

    const sameCompanyProduct=`product-${key()}`;
    await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(sameCompanyProduct,companyA,`SKU-${key()}`,"Reuse product").run();
    const reuseSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:sameCompanyProduct,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key());
    const reuse=await linkExistingAttachment(db,actorA,reuseSet.id,{sourceLinkId:reserved.attachmentLinkId,caption:"Canonical reuse"},key());
    expect(reuse.sourceId).toBe(uploaded.sourceId);
    const reuseEvent=await db.prepare("SELECT details_json FROM attachment_events WHERE link_id=? AND action='link.reused'").bind(reuse.attachmentLinkId).first<{details_json:string}>();
    expect(JSON.parse(String(reuseEvent?.details_json))).toMatchObject({crossRecord:true,source:{targetType:"product",targetId:productA},destination:{targetType:"product",targetId:sameCompanyProduct},audienceExpansion:{occurred:true,addedOwner:{targetId:sameCompanyProduct}}});

    const correctionSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:2},key());
    const correction=await reserveAttachmentFile(db,actorA,correctionSet.id,{filename:"correction.png",mediaType:"image/png",bytes:png.byteLength,supersedesLinkId:reserved.attachmentLinkId},key());
    expect(await db.prepare("SELECT supersedes_link_id FROM attachment_links WHERE id=?").bind(correction.attachmentLinkId).first("supersedes_link_id")).toBe(reserved.attachmentLinkId);
    const withdrawKey=key();
    const withdrawn=await Promise.all([
      withdrawAttachmentLink(db,actorA,correction.attachmentLinkId,"Replace incorrect image",withdrawKey),
      withdrawAttachmentLink(db,actorA,correction.attachmentLinkId,"Replace incorrect image",withdrawKey),
    ]);
    expect(withdrawn[0]).toEqual(withdrawn[1]);
    await expect(withdrawAttachmentLink(db,actorA,correction.attachmentLinkId,"Different reason",key())).rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});

    const hiddenDraft=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"specification",expectedTargetVersion:2},key());
    const hiddenReserved=await reserveAttachmentFile(db,actorA,hiddenDraft.id,{filename:"hidden.png",mediaType:"image/png",bytes:png.byteLength},key());
    process.env.ERPX_ATTACHMENTS_V2="false";process.env.ERPX_ATTACHMENTS_READ_ONLY="true";
    try{
      expect((await listAttachmentSets(db,actorA,{targetType:"product",targetId:productA})).map(item=>item.id)).toEqual([set.id]);
      await expect(attachmentLinkStatus(db,actorA,reserved.attachmentLinkId)).resolves.toMatchObject({linkState:"bound",sourceState:"available"});
      await expect(attachmentContent(db,env.H001_FILES,actorA,reserved.attachmentLinkId,"original")).resolves.toMatchObject({inline:false});
      await expect(attachmentLinkStatus(db,actorA,hiddenReserved.attachmentLinkId)).rejects.toMatchObject({status:404});
      await expect(createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"specification",expectedTargetVersion:2},key())).rejects.toMatchObject({status:404,code:"ATTACHMENTS_DISABLED"});
    }finally{process.env.ERPX_ATTACHMENTS_V2="true";delete process.env.ERPX_ATTACHMENTS_READ_ONLY;}
  });

  it("fails closed when a malformed link points at a source from another Company",async()=>{
    const foreignSet=await createAttachmentSet(db,actorB,{targetType:"product",targetId:productB,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:1},key());
    const foreignReservation=await reserveAttachmentFile(db,actorB,foreignSet.id,{filename:"foreign.png",mediaType:"image/png",bytes:png.byteLength},key());
    const foreignUpload=await completeAttachmentUpload(db,env.H001_FILES,actorB,foreignReservation.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));
    await drainAttachmentScans(db,env.H001_FILES,foreignUpload.sourceId);
    const localSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:1},key()),malformedLink=`attachment-link-cross-company-${key()}`;
    await expect(db.transaction(async tx=>{
      const now=Date.now();
      await tx.prepare(`INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at,bound_at,binding_type,binding_id)
        VALUES(?,?,?,'bound',?,?,?,?, 'test_corruption',?)`).bind(malformedLink,localSet.id,foreignUpload.sourceId,actorA.userId,now,now+60_000,now,productA).run();
      const listed=await listAttachmentSets(tx,actorA,{targetType:"product",targetId:productA});
      expect(listed.find(set=>set.id===localSet.id)?.links).toEqual([]);
      await expect(attachmentLinkStatus(tx,actorA,malformedLink)).rejects.toMatchObject({status:404,code:"ATTACHMENT_LINK_NOT_FOUND"});
      await expect(attachmentContent(tx,env.H001_FILES,actorA,malformedLink,"original")).rejects.toMatchObject({status:404,code:"ATTACHMENT_LINK_NOT_FOUND"});
      await expect(createAttachmentPublication(tx,env.H001_FILES,actorA,{sourceLinkId:malformedLink,targetType:"product",targetId:productA,altText:"Must not publish",ordinal:0},key()))
        .rejects.toMatchObject({status:404,code:"PUBLICATION_SOURCE_NOT_FOUND"});
      const reconciliation=await reconcileAttachmentStorage(tx,env.H001_FILES);
      expect(reconciliation.crossCompanyLinks).toContain(malformedLink);
      throw new Error("ROLLBACK_CROSS_COMPANY_LINK_FIXTURE");
    })).rejects.toThrow("ROLLBACK_CROSS_COMPANY_LINK_FIXTURE");
  });

  it("separates unresolved legacy ownership from a known cross-Company derivative",async()=>{
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:1},key()),file=await availableProductFile(set.id);
    const unresolvedUpload=`unresolved-upload-${key()}`,unresolvedSource=`unresolved-source-${key()}`,unresolvedLedger=`unresolved-ledger-${key()}`,now=Date.now();
    await expect(db.transaction(async tx=>{
      await tx.batch([
        tx.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at)
          VALUES(?,'holding_brand','h001','unregistered','legacy.png','image/png',1,1,?,?,?,'ready',?,?,?,?)`).bind(unresolvedUpload,"0".repeat(64),`legacy/${unresolvedUpload}`,`legacy/${unresolvedUpload}`,actorA.userId,now+60_000,now,now),
        tx.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at)
          VALUES(?,?,'holding_brand','h001','unregistered',?,1,'legacy.png','image/png',1,?,?,?)`).bind(unresolvedSource,unresolvedUpload,`legacy/${unresolvedUpload}`,"0".repeat(64),actorA.userId,now),
        tx.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'original',?,1,'stored',?)")
          .bind(unresolvedLedger,companyA,unresolvedSource,`legacy/${unresolvedUpload}`,now),
      ]);
      const unresolved=await reconcileAttachmentStorage(tx,env.H001_FILES);
      expect(unresolved.unknownOwnership).toBeGreaterThanOrEqual(1);
      expect(unresolved.crossCompanyStorage).not.toContain(unresolvedLedger);

      const derivativeId=`cross-company-derivative-${key()}`;
      await tx.batch([
        tx.prepare("INSERT INTO attachment_derivatives(id,attachment_id,company_id,page_number,object_key,checksum_sha256,original_checksum,bytes,created_by,created_at) VALUES(?,?,?,1,?,?,?,?,?,?)")
          .bind(derivativeId,file.sourceId,companyB,`missing/${derivativeId}`,"1".repeat(64),"2".repeat(64),1,actorA.userId,now),
        tx.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'derivative',?,1,'stored',?)")
          .bind(`storage-${derivativeId}`,companyB,file.sourceId,`missing/${derivativeId}`,now),
      ]);
      const malformed=await reconcileAttachmentStorage(tx,env.H001_FILES);
      expect(malformed.crossCompanyDerivatives).toContain(derivativeId);
      expect(malformed.crossCompanyStorage).toContain(`storage-${derivativeId}`);
      throw new Error("ROLLBACK_DERIVATIVE_SCOPE_FIXTURE");
    })).rejects.toThrow("ROLLBACK_DERIVATIVE_SCOPE_FIXTURE");
  });

  it("fails closed when a binding contract does not exactly match its purpose, component kind and reference",async()=>{
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",componentRef:"approved-specification",purposeKey:"specification",expectedTargetVersion:1},key());
    await availableProductFile(set.id);
    await expect(prepareAttachmentBinding(db,actorA,{attachmentSetIds:[set.id],bindingContract:{targetType:"product",bindingType:"record_document",contexts:[{purposeKey:"specification",componentKind:"record",componentRef:"draft-specification"}]},targetId:productA,expectedTargetVersion:1,commandId:key(),bindingId:productA})).rejects.toMatchObject({status:422,code:"ATTACHMENT_BINDING_CONTEXT"});
    expect(await db.prepare("SELECT state FROM attachment_sets WHERE id=?").bind(set.id).first("state")).toBe("draft");
    expect(await db.prepare("SELECT version FROM commerce_products WHERE id=?").bind(productA).first("version")).toBe(1);
    await expect(prepareAttachmentBinding(db,actorA,{attachmentSetIds:[set.id],bindingContract:{targetType:"product",bindingType:"record_document",contexts:[{purposeKey:"unknown-purpose",componentKind:"record",componentRef:"approved-specification"}]},targetId:productA,expectedTargetVersion:1,commandId:key(),bindingId:productA})).rejects.toMatchObject({status:422,code:"ATTACHMENT_POLICY_UNKNOWN"});
  });

  it("uses 15-minute reservation and exact seven-day clean-draft cleanup boundaries",async()=>{
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),started=Date.now();
    const reserved=await reserveAttachmentFile(db,actorA,set.id,{filename:"manual.pdf",mediaType:"application/pdf",bytes:png.byteLength},key());
    expect(reserved.expiresAt-started).toBeGreaterThanOrEqual(15*60_000-1000);
    expect(reserved.expiresAt-started).toBeLessThanOrEqual(15*60_000+1000);
    const uploaded=await completeAttachmentUpload(db,env.H001_FILES,actorA,reserved.uploadIntentId,png.buffer.slice(png.byteOffset,png.byteOffset+png.byteLength));
    await drainAttachmentScans(db,env.H001_FILES,uploaded.sourceId);
    const clean=await attachmentLinkStatus(db,actorA,reserved.attachmentLinkId),cleanedAt=Date.now();
    expect(clean.expiresAt-cleanedAt).toBeGreaterThanOrEqual(7*24*60*60_000-1000);
    expect(clean.expiresAt-cleanedAt).toBeLessThanOrEqual(7*24*60*60_000+1000);
    await db.prepare("UPDATE attachment_links SET expires_at=? WHERE id=?").bind(Date.now()+60_000,reserved.attachmentLinkId).run();
    await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(await db.prepare("SELECT state FROM attachment_links WHERE id=?").bind(reserved.attachmentLinkId).first("state")).toBe("draft");
    await db.prepare("UPDATE attachment_links SET expires_at=? WHERE id=?").bind(Date.now(),reserved.attachmentLinkId).run();
    const cleaned=await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(cleaned).toMatchObject({expired:1,expiredSets:1,destroyed:1});
    expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(uploaded.sourceId).first("source_state")).toBe("destroyed");
    await expect(createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key())).resolves.toMatchObject({id:expect.any(String),state:"draft"});

    const heldProduct=`product-${key()}`;
    await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(heldProduct,companyA,`SKU-${key()}`,"Held product").run();
    const heldSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:heldProduct,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),held=await availableProductFile(heldSet.id);
    await db.batch([
      db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(held.sourceId),
      db.prepare("UPDATE attachment_links SET expires_at=? WHERE id=?").bind(Date.now(),held.attachmentLinkId),
    ]);
    await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(held.sourceId).first("source_state")).toBe("available");

    const retainedProduct=`product-${key()}`;
    await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(retainedProduct,companyA,`SKU-${key()}`,"Retained product").run();
    const retainedSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:retainedProduct,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),retained=await availableProductFile(retainedSet.id);
    await db.batch([
      db.prepare("UPDATE attachment_sets SET retention_policy_key='legal-managed',retention_policy_version=1,destruction_mode='review' WHERE id=?").bind(retainedSet.id),
      db.prepare("UPDATE attachment_links SET expires_at=? WHERE id=?").bind(Date.now(),retained.attachmentLinkId),
    ]);
    await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(retained.sourceId).first("source_state")).toBe("available");

    const preservedProduct=`product-${key()}`,legalItem=`legal-item-${key()}`,legalHold=`legal-hold-${key()}`;
    await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(preservedProduct,companyA,`SKU-${key()}`,"Legally preserved product").run();
    const preservedSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:preservedProduct,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),preserved=await availableProductFile(preservedSet.id),preservedObjectKey=String(await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(preserved.sourceId).first("object_key"));
    await withdrawAttachmentLink(db,actorA,preserved.attachmentLinkId,"Legal preservation fixture",key());
    await db.batch([
      db.prepare("UPDATE attachment_links SET expires_at=? WHERE id=?").bind(Date.now(),preserved.attachmentLinkId),
      db.prepare("INSERT INTO legal_items(id,company_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,state,created_at,updated_at) VALUES(?,?,'matter','Attachment preservation','','restricted','TH','TH','th','THB','Asia/Bangkok','normal',?,?,'archived',?,?)").bind(legalItem,companyA,actorA.userId,actorA.userId,Date.now(),Date.now()),
      db.prepare("INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at) VALUES(?,?,'Preserve linked canonical source','active',?,?)").bind(legalHold,legalItem,actorA.userId,Date.now()),
      db.prepare("INSERT INTO legal_hold_targets(hold_id,item_id) VALUES(?,?)").bind(legalHold,legalItem),
      db.prepare("INSERT INTO legal_file_preservations(object_key,item_id,source_type,source_id,checksum,created_by,created_at) VALUES(?,?,'attachment',?,'fixture',?,?)").bind(preservedObjectKey,legalItem,preserved.sourceId,actorA.userId,Date.now()),
    ]);
    expect(await attachmentDestructionEligibility(db,preserved.sourceId)).toMatchObject({eligible:false,reason:"legal_preservation"});
    await expect(db.prepare("UPDATE attachments SET source_state='destroyed',destroyed_at=? WHERE id=?").bind(Date.now(),preserved.sourceId).run()).rejects.toThrow(/LEGAL_HOLD_BLOCKS_FILE_MUTATION/);
    await expect(db.prepare("UPDATE attachment_storage_ledger SET state='destroyed',destroyed_at=? WHERE attachment_id=? AND state='stored'").bind(Date.now(),preserved.sourceId).run()).rejects.toThrow(/LEGAL_HOLD_BLOCKS_FILE_MUTATION/);
    await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(preserved.sourceId).first("source_state")).toBe("available");
    await db.prepare("UPDATE legal_holds SET state='released',released_by=?,released_at=? WHERE id=?").bind(peerA.userId,Date.now(),legalHold).run();
    await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(preserved.sourceId).first("source_state")).toBe("destroyed");
    await expect(db.transaction(async tx=>{
      const lateHold=`legal-hold-${key()}`;
      await tx.prepare("INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at) VALUES(?,?,'Too late to preserve destroyed bytes','active',?,?)").bind(lateHold,legalItem,actorA.userId,Date.now()).run();
      await tx.prepare("INSERT INTO legal_hold_targets(hold_id,item_id) VALUES(?,?)").bind(lateHold,legalItem).run();
    })).rejects.toThrow(/LEGAL_HOLD_SOURCE_ALREADY_DESTROYED/);
    expect(await db.prepare("SELECT count(*) FROM legal_holds WHERE item_id=? AND state='active'").bind(legalItem).first("count")).toBe(0);

    const sharedProduct=`product-${key()}`;
    await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(sharedProduct,companyA,`SKU-${key()}`,"Shared-retention product").run();
    const firstSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:sharedProduct,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:1},key()),shared=await availableProductFile(firstSet.id);
    const secondSet=await createAttachmentSet(db,actorA,{targetType:"product",targetId:sharedProduct,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),reused={attachmentLinkId:`attachment-link-${key()}`};
    await db.prepare("INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES(?,?,?,'draft',?,?,?)").bind(reused.attachmentLinkId,secondSet.id,shared.sourceId,actorA.userId,Date.now(),Date.now()+7*24*60*60_000).run();
    await withdrawAttachmentLink(db,actorA,shared.attachmentLinkId,"Shared retention test",key());
    await withdrawAttachmentLink(db,actorA,reused.attachmentLinkId,"Shared retention test",key());
    const boundary=Date.now();
    await db.batch([
      db.prepare("UPDATE attachment_links SET expires_at=? WHERE id IN (?,?)").bind(boundary,shared.attachmentLinkId,reused.attachmentLinkId),
      db.prepare("UPDATE attachment_sets SET retention_policy_key='test-auto',retention_policy_version=1,destruction_mode='automatic',retain_until=? WHERE id=?").bind(boundary-1,firstSet.id),
      db.prepare("UPDATE attachment_sets SET retention_policy_key='test-auto',retention_policy_version=1,destruction_mode='automatic',retain_until=? WHERE id=?").bind(boundary+60_000,secondSet.id),
    ]);
    expect(await attachmentDestructionEligibility(db,shared.sourceId,boundary)).toMatchObject({eligible:false,reason:"retain_until",retainUntil:boundary+60_000});
    await db.prepare("UPDATE attachment_sets SET destruction_mode='review',retain_until=NULL WHERE id=?").bind(secondSet.id).run();
    expect(await attachmentDestructionEligibility(db,shared.sourceId,boundary)).toMatchObject({eligible:false,reason:"retention_review"});
    await db.prepare("UPDATE attachment_sets SET destruction_mode='automatic',retain_until=? WHERE id=?").bind(boundary-1,secondSet.id).run();
    await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(shared.sourceId).run();
    expect(await attachmentDestructionEligibility(db,shared.sourceId,boundary)).toMatchObject({eligible:false,reason:"legal_hold"});
    await db.prepare("UPDATE attachments SET legal_hold=0 WHERE id=?").bind(shared.sourceId).run();
    expect(await attachmentDestructionEligibility(db,shared.sourceId,boundary)).toMatchObject({eligible:true,reason:"eligible",retainUntil:boundary-1});
    await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(shared.sourceId).first("source_state")).toBe("destroyed");
  });

  it("pauses cleanup during upload freeze and removes every physical derivative after expiry",async()=>{
    const cleanupProduct=`product-${key()}`;
    await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(cleanupProduct,companyA,`SKU-${key()}`,"Frozen cleanup product").run();
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:cleanupProduct,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),file=await availableProductFile(set.id);
    const source=await db.prepare("SELECT object_key,preview_object_key,checksum_sha256 FROM attachments WHERE id=?").bind(file.sourceId).first<{object_key:string;preview_object_key:string|null;checksum_sha256:string}>();
    const derivativeId=`attachment-derivative-${key()}`,derivativeKey=`h001/derivatives/${derivativeId}.jpg`,derivativeBytes=new TextEncoder().encode("sanitized cleanup derivative"),derivativeChecksum=await sha256(derivativeBytes.buffer);
    await env.H001_FILES.put(derivativeKey,derivativeBytes,{httpMetadata:{contentType:"image/jpeg"},customMetadata:{sha256:derivativeChecksum}});
    await db.batch([
      db.prepare("INSERT INTO attachment_derivatives(id,attachment_id,company_id,page_number,object_key,checksum_sha256,original_checksum,bytes,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(derivativeId,file.sourceId,companyA,1,derivativeKey,derivativeChecksum,source!.checksum_sha256,derivativeBytes.byteLength,actorA.userId,Date.now()),
      db.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'derivative',?,?,'stored',?)")
        .bind(`attachment-storage-${derivativeId}`,companyA,file.sourceId,derivativeKey,derivativeBytes.byteLength,Date.now()),
      db.prepare("UPDATE attachment_links SET expires_at=? WHERE id=?").bind(Date.now(),file.attachmentLinkId),
    ]);
    process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE="true";
    try{
      await expect(cleanupAttachmentDrafts(db,env.H001_FILES)).resolves.toEqual({expired:0,expiredSets:0,destroyed:0,bytesDeleted:0,deletionFailures:0});
      expect(await db.prepare("SELECT state FROM attachment_links WHERE id=?").bind(file.attachmentLinkId).first("state")).toBe("draft");
    }finally{delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;}
    const cleaned=await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(cleaned).toMatchObject({expired:1,expiredSets:1,destroyed:1,bytesDeleted:1,deletionFailures:0});
    await expect(env.H001_FILES.head(source!.object_key)).resolves.toBeNull();
    if(source!.preview_object_key)await expect(env.H001_FILES.head(source!.preview_object_key)).resolves.toBeNull();
    await expect(env.H001_FILES.head(derivativeKey)).resolves.toBeNull();
    expect(await db.prepare("SELECT count(*) FROM attachment_storage_ledger WHERE attachment_id=? AND state='stored'").bind(file.sourceId).first("count")).toBe(0);
  });

  it("publishes only an independently approved sanitized derivative and revokes it immediately",async()=>{
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"product_image",expectedTargetVersion:1},key());
    const file=await availableProductFile(set.id);
    await bindRecordAttachmentSet(db,actorA,set.id,1,key());
    process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE="true";
    await expect(createAttachmentPublication(db,env.H001_FILES,actorA,{sourceLinkId:file.attachmentLinkId,targetType:"product",targetId:productA,altText:"Frozen publication",ordinal:0},key())).rejects.toMatchObject({status:503,code:"ATTACHMENT_UPLOAD_FREEZE"});
    delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
    const first=await createAttachmentPublication(db,env.H001_FILES,actorA,{sourceLinkId:file.attachmentLinkId,targetType:"product",targetId:productA,altText:"Front view",ordinal:0},key());
    const firstId=String(first?.id);
    await expect(publicAttachmentPublication(db,env.H001_FILES,firstId)).rejects.toMatchObject({status:404,code:"PUBLICATION_NOT_FOUND"});
    await expect(approveAttachmentPublication(db,actorA,firstId,key())).rejects.toMatchObject({status:403,code:"INDEPENDENT_PUBLICATION_REVIEW"});
    const approvalKey=key(),[approved,replayedApproval]=await Promise.all([
      approveAttachmentPublication(db,peerA,firstId,approvalKey),
      approveAttachmentPublication(db,peerA,firstId,approvalKey),
    ]);
    expect(replayedApproval?.state).toBe(approved?.state);
    expect((await publicAttachmentPublication(db,env.H001_FILES,firstId)).row.alt_text).toBe("Front view");
    const stored=await env.H001_FILES.head(String(approved?.derivative_object_key));
    expect(stored?.httpMetadata.cacheControl).toBe("no-store");

    const second=await createAttachmentPublication(db,env.H001_FILES,actorA,{sourceLinkId:file.attachmentLinkId,targetType:"product",targetId:productA,altText:"Detail view",ordinal:1},key()),secondId=String(second?.id);
    await approveAttachmentPublication(db,peerA,secondId,key());
    const reorderInput={targetType:"product",targetId:productA,expectedVersion:1,items:[{id:firstId,ordinal:1},{id:secondId,ordinal:0}]},reorderKey=key();
    const reordered=await reorderAttachmentPublications(db,actorA,reorderInput,reorderKey);
    expect(reordered.orderVersion).toBe(2);
    await expect(reorderAttachmentPublications(db,actorA,reorderInput,reorderKey)).resolves.toEqual(reordered);
    await expect(reorderAttachmentPublications(db,actorA,{...reorderInput,expectedVersion:2,items:[{id:firstId,ordinal:0},{id:secondId,ordinal:1}]},reorderKey)).rejects.toMatchObject({status:409,code:"IDEMPOTENCY_CONFLICT"});
    const listed=await listAttachmentPublications(db,actorA,"product",productA);
    expect(listed.items.map(item=>item.id)).toEqual([secondId,firstId]);
    await revokeAttachmentPublication(db,peerA,firstId,"Replaced by a newer approved view",key());
    await expect(publicAttachmentPublication(db,env.H001_FILES,firstId)).rejects.toMatchObject({status:404,code:"PUBLICATION_NOT_FOUND"});
    await expect(publicAttachmentPublication(db,env.H001_FILES,secondId)).resolves.toMatchObject({row:{alt_text:"Detail view"}});
  });

  it("binds Brand media privately before independent approval selects the sanitized public pointer",async()=>{
    const set=await createAttachmentSet(db,actorA,{targetType:"holding_brand",targetId:actorA.holdingId,componentKind:"publication",purposeKey:"primary",expectedTargetVersion:1},key());
    const file=await availableProductFile(set.id);
    const bound=await bindRecordAttachmentSet(db,actorA,set.id,1,key());
    expect(bound).toMatchObject({version:1,manifest:[{linkId:file.attachmentLinkId,sourceId:file.sourceId}]});
    expect(await db.prepare("SELECT primary_publication_id FROM organization_brand_profiles WHERE scope_kind='holding' AND scope_id=?").bind(actorA.holdingId).first()).toBeNull();

    const draft=await createAttachmentPublication(db,env.H001_FILES,actorA,{sourceLinkId:file.attachmentLinkId,targetType:"holding_brand",targetId:actorA.holdingId,altText:"Test Holding mark",ordinal:0},key()),publicationId=String(draft?.id);
    await expect(publicAttachmentPublication(db,env.H001_FILES,publicationId)).rejects.toMatchObject({status:404});
    await approveAttachmentPublication(db,peerA,publicationId,key());
    expect(await db.prepare("SELECT primary_publication_id FROM organization_brand_profiles WHERE scope_kind='holding' AND scope_id=?").bind(actorA.holdingId).first("primary_publication_id")).toBe(publicationId);
    expect((await listBrandProfiles(db,peerA)).holding.primaryUrl).toContain(`/media-publications/${publicationId}/content`);
    await expect(publicAttachmentPublication(db,env.H001_FILES,publicationId)).resolves.toMatchObject({row:{alt_text:"Test Holding mark"}});

    await expect(db.prepare("UPDATE attachment_publications SET alt_text='tampered' WHERE id=?").bind(publicationId).run()).rejects.toThrow(/ATTACHMENT_PUBLICATION_IMMUTABLE/);
    await expect(db.prepare("DELETE FROM attachment_publications WHERE id=?").bind(publicationId).run()).rejects.toThrow(/ATTACHMENT_PUBLICATION_IMMUTABLE/);
    await expect(db.prepare("UPDATE attachment_storage_ledger SET bytes=bytes+1 WHERE attachment_id=?").bind(file.sourceId).run()).rejects.toThrow(/ATTACHMENT_STORAGE_IMMUTABLE/);
    await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(file.sourceId).run();
    await expect(db.prepare("UPDATE attachments SET source_state='destroyed',destroyed_at=? WHERE id=?").bind(Date.now(),file.sourceId).run()).rejects.toThrow(/ATTACHMENT_LEGAL_HOLD/);

    await revokeAttachmentPublication(db,peerA,publicationId,"Replaced after brand review",key());
    expect(await db.prepare("SELECT primary_publication_id FROM organization_brand_profiles WHERE scope_kind='holding' AND scope_id=?").bind(actorA.holdingId).first("primary_publication_id")).toBeNull();
    await expect(publicAttachmentPublication(db,env.H001_FILES,publicationId)).rejects.toMatchObject({status:404});
  });

  it("seals database state before byte deletion and rechecks Legal Hold before retrying",async()=>{
    const cleanupProduct=`product-${key()}`;
    await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(cleanupProduct,companyA,`SKU-${key()}`,"Cleanup race product").run();
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:cleanupProduct,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),file=await availableProductFile(set.id);
    const storedKeys=await db.prepare("SELECT object_key,preview_object_key FROM attachments WHERE id=?").bind(file.sourceId).first<{object_key:string;preview_object_key:string|null}>(),objectKey=storedKeys!.object_key;
    await withdrawAttachmentLink(db,actorA,file.attachmentLinkId,"Cleanup ordering fixture",key());
    await db.prepare("UPDATE attachment_links SET expires_at=? WHERE id=?").bind(Date.now(),file.attachmentLinkId).run();
    const failingFiles=new Proxy(env.H001_FILES,{
      get(target,property,receiver){
        if(property==="delete")return async()=>{
          expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(file.sourceId).first("source_state")).toBe("destroyed");
          throw new Error("TEST_OBJECT_DELETE_FAILED");
        };
        const value=Reflect.get(target,property,receiver);
        return typeof value==="function"?value.bind(target):value;
      },
    });
    const first=await cleanupAttachmentDrafts(db,failingFiles);
    expect(first.deletionFailures).toBeGreaterThanOrEqual(1);
    expect(await db.prepare("SELECT source_state FROM attachments WHERE id=?").bind(file.sourceId).first("source_state")).toBe("destroyed");
    expect(await env.H001_FILES.head(objectKey)).not.toBeNull();

    await db.prepare("UPDATE attachments SET legal_hold=1 WHERE id=?").bind(file.sourceId).run();
    await cleanupAttachmentDrafts(db,env.H001_FILES);
    expect(await env.H001_FILES.head(objectKey)).not.toBeNull();
    // Test-fixture teardown only: the cleanup service itself must not cross the
    // hold boundary, while reconciliation must not inherit this test object.
    await env.H001_FILES.delete([objectKey,...(storedKeys!.preview_object_key?[storedKeys!.preview_object_key]:[])]);
  });

  it.each(["scan_error","malware_rejected"])("blocks generic destruction of sealed %s sources",async sourceState=>{
    const intentId=`sealed-intent-${key()}`,sourceId=`sealed-source-${key()}`,now=Date.now();
    await expect(db.transaction(async tx=>{
      await tx.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id,quarantine_key)
        VALUES(?, 'product',?,'specification','sealed.pdf','application/pdf',1,1,?, ?,?,'uploaded',?,?,?,?,?,?)`)
        .bind(intentId,productA,"0".repeat(64),`h001/quarantine/${intentId}/original`,`h001/quarantine/${intentId}/original`,actorA.userId,now+60_000,now,now,companyA,`h001/quarantine/${intentId}/original`).run();
      await tx.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state)
        VALUES(?,?, 'product',?,'specification',?,1,'sealed.pdf','application/pdf',1,?,?,?, ?,?)`)
        .bind(sourceId,intentId,productA,`h001/quarantine/${sourceId}/original`,"0".repeat(64),actorA.userId,now,companyA,sourceState).run();
      await tx.prepare("UPDATE attachments SET source_state='destroyed',destroyed_at=? WHERE id=?").bind(now,sourceId).run();
    })).rejects.toThrow(/ATTACHMENT_SECURITY_DESTRUCTION_POLICY_REQUIRED/);
  });

  it("canonically backfills historical Messaging and Legal objects so reconciliation does not report them as orphaned",async()=>{
    const now=Date.now(),roomId=`room-${key()}`,messageId=`message-${key()}`,messageFileId=`message-file-${key()}`,legalItemId=`legal-item-${key()}`,documentId=`legal-document-${key()}`,shareId=`legal-share-${key()}`,externalId=`legal-external-${key()}`;
    const messageBytes=png,pdfBytes=Buffer.from("%PDF-1.4\n% attachment migration fixture\n%%EOF"),messageKey=`h001/messaging/legacy/${messageFileId}`,documentKey=`h001/legal/legacy/${documentId}`,externalKey=`h001/legal/legacy/${externalId}`;
    const [messageChecksum,documentChecksum,externalChecksum]=await Promise.all([messageBytes,pdfBytes,pdfBytes].map(bytes=>sha256(bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength))));
    await Promise.all([
      env.H001_FILES.put(messageKey,messageBytes,{httpMetadata:{contentType:"image/png"},customMetadata:{sha256:messageChecksum}}),
      env.H001_FILES.put(documentKey,pdfBytes,{httpMetadata:{contentType:"application/pdf"},customMetadata:{sha256:documentChecksum}}),
      env.H001_FILES.put(externalKey,pdfBytes,{httpMetadata:{contentType:"application/pdf"},customMetadata:{sha256:externalChecksum}}),
    ]);
    await db.batch([
      db.prepare("INSERT INTO messaging_rooms(id,company_id,kind,name,owner_id,direct_key,created_at) VALUES(?,?,'direct','Legacy room',?,?,?)").bind(roomId,companyA,actorA.userId,`${actorA.userId}:${peerA.userId}`,now),
      db.prepare("INSERT INTO messaging_messages(id,room_id,seq,sender_id,sender_name,body,created_at) VALUES(?,?,1,?,'Owner','Legacy attachment',?)").bind(messageId,roomId,actorA.userId,now),
      db.prepare("INSERT INTO messaging_files(id,room_id,message_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(messageFileId,roomId,messageId,actorA.userId,"legacy.png","image/png",messageBytes.byteLength,messageKey,messageChecksum,now),
      db.prepare(`INSERT INTO legal_items(id,company_id,kind,title,classification,country,jurisdiction,language,currency,timezone,priority,owner_id,created_by,created_at,updated_at)
        VALUES(?,?,'matter','Legacy legal object','restricted','TH','TH','th','THB','Asia/Bangkok','normal',?,?,?,?)`).bind(legalItemId,companyA,actorA.userId,actorA.userId,now,now),
      db.prepare("INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,created_by,created_at) VALUES(?,?,1,'legacy.pdf','application/pdf',?,?,?,?,?)").bind(documentId,legalItemId,documentChecksum,pdfBytes.byteLength,documentKey,actorA.userId,now),
      db.prepare("INSERT INTO legal_shares(id,item_id,document_id,email,expires_at,created_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(shareId,legalItemId,documentId,"portal@example.test",now+60_000,actorA.userId,now),
      db.prepare("INSERT INTO legal_external_uploads(id,share_id,object_key,checksum,name,media_type,bytes,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(externalId,shareId,externalKey,externalChecksum,"portal.pdf","application/pdf",pdfBytes.byteLength,"portal-user",now),
    ]);

    const migrated=await backfillSpecializedAttachments(db,{apply:true});
    expect(migrated).toMatchObject({examined:3,ready:3,backfilled:3,unknown:[]});
    expect(await db.prepare("SELECT attachment_source_id FROM messaging_files WHERE id=?").bind(messageFileId).first("attachment_source_id")).toMatch(/^attachment-specialized-/);
    expect(await db.prepare("SELECT attachment_source_id FROM legal_documents WHERE id=?").bind(documentId).first("attachment_source_id")).toMatch(/^attachment-specialized-/);
    expect(await db.prepare("SELECT attachment_source_id FROM legal_external_uploads WHERE id=?").bind(externalId).first("attachment_source_id")).toMatch(/^attachment-specialized-/);
    const beforeScan=await reconcileAttachmentStorage(db,env.H001_FILES);
    expect(beforeScan.orphanObjects).not.toEqual(expect.arrayContaining([messageKey,documentKey,externalKey]));
    let drained=0;while(await drainAttachmentScans(db,env.H001_FILES)){drained++;if(drained>20)throw new Error("SPECIALIZED_SCAN_DID_NOT_DRAIN");}
    expect(drained).toBeGreaterThanOrEqual(3);
    const reconciled=await reconcileAttachmentStorage(db,env.H001_FILES,{verifyChecksums:true});
    expect(reconciled).toMatchObject({unknownOwnership:0,unavailable:0,missingObjects:[],checksumMismatches:[],missingLedger:[],orphanObjects:[],quotaDeltaBytes:0,duplicateLedgerObjects:0});
  });

  it("classifies legacy document derivatives and generated exports while retaining true orphan detection",async()=>{
    const set=await createAttachmentSet(db,actorA,{targetType:"product",targetId:productA,componentKind:"record",purposeKey:"specification",expectedTargetVersion:1},key()),source=await availableProductFile(set.id);
    const originalChecksum=String(await db.prepare("SELECT checksum_sha256 FROM attachments WHERE id=?").bind(source.sourceId).first("checksum_sha256"));
    const derivativeId=`derivative-${key()}`,derivativeKey=`test/document-pages/${key()}`,derivativeBytes=new TextEncoder().encode("sanitized page"),derivativeChecksum=await sha256(derivativeBytes.buffer);
    const exportId=`payment-export-${key()}`,exportKey=`test/generated/${exportId}.csv`,exportBytes=new TextEncoder().encode("reference,amount\nSIM-1,100\n"),exportChecksum=await sha256(exportBytes.buffer);
    const archiveBytes=new TextEncoder().encode("reference,amount\nSIM-OLD,90\n"),archiveChecksum=await sha256(archiveBytes.buffer),archiveKey=`h001/seed-archive/${exportId}-${archiveChecksum}.csv`;
    const orphanKey=`test/orphan/${key()}`;
    await Promise.all([
      env.H001_FILES.put(derivativeKey,derivativeBytes,{httpMetadata:{contentType:"image/jpeg"},customMetadata:{sha256:derivativeChecksum}}),
      env.H001_FILES.put(exportKey,exportBytes,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:exportChecksum}}),
      env.H001_FILES.put(archiveKey,archiveBytes,{httpMetadata:{contentType:"text/csv"},customMetadata:{sha256:archiveChecksum}}),
      env.H001_FILES.put(orphanKey,new Uint8Array([1,2,3])),
    ]);
    await db.batch([
      db.prepare("INSERT INTO attachment_derivatives(id,attachment_id,company_id,page_number,object_key,checksum_sha256,original_checksum,bytes,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(derivativeId,source.sourceId,companyA,1,derivativeKey,derivativeChecksum,originalChecksum,derivativeBytes.byteLength,actorA.userId,Date.now()),
      db.prepare("INSERT INTO attachment_storage_ledger(id,company_id,attachment_id,variant,object_key,bytes,state,created_at) VALUES(?,?,?,'derivative',?,?,'stored',?)").bind(`attachment-storage-derivative-${derivativeId}`,companyA,source.sourceId,derivativeKey,derivativeBytes.byteLength,Date.now()),
      db.prepare("INSERT INTO hr_payment_batches(id,company_id,source_type,source_id,currency,status,format_code,file_key,file_checksum,total_minor,version,created_by,exported_at,created_at,updated_at) VALUES(?,?,'expense',?,'THB','exported','generic-csv',?,?,100,1,?,?,?,?)").bind(exportId,companyA,`expense-${key()}`,exportKey,exportChecksum,actorA.userId,Date.now(),Date.now(),Date.now()),
    ]);
    const reconciled=await reconcileAttachmentStorage(db,env.H001_FILES,{verifyChecksums:true});
    expect(reconciled).toMatchObject({attachmentDerivatives:expect.any(Number),generatedArtifacts:expect.any(Number),archivedGeneratedArtifacts:expect.any(Number)});
    expect(reconciled.attachmentDerivatives).toBeGreaterThanOrEqual(1);
    expect(reconciled.generatedArtifacts).toBeGreaterThanOrEqual(2);
    expect(reconciled.archivedGeneratedArtifacts).toBeGreaterThanOrEqual(1);
    expect(reconciled.orphanObjects).toContain(orphanKey);
    expect(reconciled.orphanObjects).not.toEqual(expect.arrayContaining([derivativeKey,exportKey,archiveKey]));
    await env.H001_FILES.delete(orphanKey);
  });
});
