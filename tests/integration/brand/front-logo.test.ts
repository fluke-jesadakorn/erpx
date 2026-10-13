import { afterEach,beforeAll,expect,it,vi } from "vitest";
import { applyMigrations,env } from "../../support/runtime-env";

const fileStore=vi.hoisted(()=>({get:vi.fn()}));

vi.mock("../../../src/server/modules/runtime/adapters/runtime",async importOriginal=>{
  const actual=await importOriginal<typeof import("../../../src/server/modules/runtime/adapters/runtime")>();
  return {
    ...actual,
    holdingBindingBySlug:async(slug:string)=>({id:"h001",slug}),
    holdingResources:async()=>({write:env.H001_DB,files:fileStore}),
  };
});

import { GET as storefrontLogoRoute } from "../../../src/app/front/[holdingSlug]/[companySlug]/[app]/logo/route";

const previousGate=process.env.ERPX_ATTACHMENTS_V2;
const previousReadOnly=process.env.ERPX_ATTACHMENTS_READ_ONLY;

beforeAll(async()=>{await applyMigrations(env.H001_DB,env.STAFF_TEST_MIGRATIONS);});
afterEach(()=>{
  if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;
  if(previousReadOnly===undefined)delete process.env.ERPX_ATTACHMENTS_READ_ONLY;else process.env.ERPX_ATTACHMENTS_READ_ONLY=previousReadOnly;
  fileStore.get.mockReset();
});

it("never serves a legacy storefront original when no publication is approved",async()=>{
  const token=crypto.randomUUID(),companyId=`front-company-${token}`,companySlug=`front-${token}`,intentId=`front-upload-${token}`,attachmentId=`front-attachment-${token}`,now=Date.now(),objectKey=`h001/branding/${token}/original`;
  await env.H001_DB.batch([
    env.H001_DB.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Private storefront brand',?)").bind(companyId,companySlug,companyId),
    env.H001_DB.prepare(`INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,actual_bytes,checksum_sha256,staging_key,quarantine_key,final_key,status,created_by,expires_at,created_at,updated_at,company_id)
      VALUES(?,'company_storefront_brand',?,'logo','private.png','image/png',3,3,?,?,?,?, 'ready','fixture',?,?,?,?)`)
      .bind(intentId,companyId,"a".repeat(64),objectKey,objectKey,objectKey,now+60_000,now,now,companyId),
    env.H001_DB.prepare(`INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,source_state)
      VALUES(?,?,'company_storefront_brand',?,'logo',?,1,'private.png','image/png',3,?,'fixture',?,?,'available')`)
      .bind(attachmentId,intentId,companyId,objectKey,"a".repeat(64),now,companyId),
    env.H001_DB.prepare("INSERT INTO storefront_app_brands(company_id,short_name,accent_color,logo_attachment_id,updated_by,updated_at) VALUES(?,'Private','#123456',?,'fixture',?)")
      .bind(companyId,attachmentId,now),
  ]);
  fileStore.get.mockResolvedValue({body:new Uint8Array([1,2,3]),size:3,arrayBuffer:async()=>new Uint8Array([1,2,3]).buffer});

  for(const mode of [
    {enabled:"true",readOnly:undefined},
    {enabled:"false",readOnly:undefined},
    {enabled:"false",readOnly:"true"},
  ]){
    process.env.ERPX_ATTACHMENTS_V2=mode.enabled;
    if(mode.readOnly===undefined)delete process.env.ERPX_ATTACHMENTS_READ_ONLY;else process.env.ERPX_ATTACHMENTS_READ_ONLY=mode.readOnly;
    fileStore.get.mockClear();
    const response=await storefrontLogoRoute(
      new Request(`http://localhost:3100/front/acme/${companySlug}/pos/logo`),
      {params:Promise.resolve({holdingSlug:"acme",companySlug})},
    );
    expect(response.status).toBe(404);
    expect(fileStore.get).not.toHaveBeenCalled();
  }
});
