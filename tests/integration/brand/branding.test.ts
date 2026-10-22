import sharp from "sharp";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import {
  completeBrandUpload,
  completeStorefrontBrandUpload,
  normalizeBrandImage,
  recordBranchId,
  requireBrandManagement,
  resolveDocumentBrand,
  updateBrandProfile,
  updateStorefrontAppBrand,
} from "@erpx/server-brand/public/branding";
import { matchesUploadSignature } from "@erpx/server-files/public/upload-validation";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import {enableAttachmentV2ForTest,seedAttachmentSet} from "../../support/helpers/attachment-v2";
import type {FileStore} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";

let fixture: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  enableAttachmentV2ForTest();
});

beforeEach(async () => {
  fixture = await expenseFixture();
});
afterEach(()=>{vi.unstubAllEnvs();});

it("accepts real PNG/WebP signatures and rejects a MIME mismatch", async () => {
  const png = await sharp({ create: { width: 64, height: 64, channels: 4, background: "transparent" } }).png().toBuffer();
  const webp = await sharp(png).webp().toBuffer();
  expect(matchesUploadSignature("image/png", png)).toBe(true);
  expect(matchesUploadSignature("image/webp", webp)).toBe(true);
  expect(matchesUploadSignature("image/webp", png)).toBe(false);
  expect(matchesUploadSignature("image/png", webp)).toBe(false);
});

it("re-encodes transparent logos as metadata-free WebP", async () => {
  const source = await sharp({ create: { width: 96, height: 48, channels: 4, background: { r: 47, g: 105, b: 245, alpha: 0.4 } } })
    .withExif({ IFD0: { ImageDescription: "must not survive" } }).withMetadata()
    .png()
    .toBuffer();
  const output = await normalizeBrandImage(source.buffer.slice(source.byteOffset, source.byteOffset + source.byteLength) as ArrayBuffer);
  const metadata = await sharp(output).metadata();
  expect(metadata.format).toBe("webp");
  expect(metadata.width).toBe(96);
  expect(metadata.height).toBe(48);
  expect(metadata.hasAlpha).toBe(true);
  expect(metadata.exif).toBeUndefined();
  expect(metadata.icc).toBeUndefined();
});

it("rejects corrupt, undersized and pixel-bomb logo inputs", async () => {
  await expect(normalizeBrandImage(new Uint8Array([1, 2, 3, 4]).buffer)).rejects.toMatchObject({ code: "LOGO_INVALID" });
  const small = await sharp({ create: { width: 31, height: 32, channels: 4, background: "white" } }).png().toBuffer();
  await expect(normalizeBrandImage(small.buffer.slice(small.byteOffset, small.byteOffset + small.byteLength) as ArrayBuffer)).rejects.toMatchObject({ code: "LOGO_DIMENSIONS" });
  const huge = await sharp({ create: { width: 4097, height: 4097, channels: 3, background: "white" } }).png().toBuffer();
  await expect(normalizeBrandImage(huge.buffer.slice(huge.byteOffset, huge.byteOffset + huge.byteLength) as ArrayBuffer)).rejects.toMatchObject({ code: "LOGO_PIXELS" });
});

it("derives an expense Branch from the record and applies document-mode defaults", async () => {
  const expenseId = `brand-expense-${crypto.randomUUID()}`;
  await db.prepare("INSERT INTO finance_expenses(id,company_id,branch_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'draft',?,?,?)")
    .bind(expenseId, fixture.company, fixture.branch, fixture.claimant.userId, Date.now(), Date.now()).run();
  expect(await recordBranchId(db, fixture.claimant, "expense", expenseId)).toBe(fixture.branch);
  await db.prepare("INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at) VALUES('branch',?,?,?,?,?)")
    .bind(fixture.branch, fixture.company, "both", fixture.owner.userId, Date.now()).run();
  const branded = await resolveDocumentBrand(db, fixture.claimant, { branchId: await recordBranchId(db, fixture.claimant, "expense", expenseId) });
  expect(branded.mode).toBe("both");
  expect(branded.branch?.scopeId).toBe(fixture.branch);
  expect(branded.companyName).toBeTruthy();
  const companyWide = await resolveDocumentBrand(db, fixture.claimant, { branchId: null, requestedMode: "branch" });
  expect(companyWide.mode).toBe("holding");
  expect(companyWide.branch).toBeNull();
});

it("allows the Holding owner to manage brands and rejects ordinary staff", async () => {
  await expect(requireBrandManagement(db, fixture.owner, "holding_brand", fixture.owner.holdingId)).resolves.toBeUndefined();
  await expect(requireBrandManagement(db, fixture.owner, "branch_brand", fixture.branch)).resolves.toBeUndefined();
  await expect(requireBrandManagement(db, fixture.owner, "company_storefront_brand", fixture.company)).resolves.toBeUndefined();
  await expect(requireBrandManagement(db, fixture.claimant, "holding_brand", fixture.owner.holdingId)).rejects.toMatchObject({ code: "BRAND_MANAGEMENT_FORBIDDEN" });
  await expect(requireBrandManagement(db, fixture.claimant, "branch_brand", fixture.branch)).rejects.toMatchObject({ code: "BRAND_MANAGEMENT_FORBIDDEN" });
});

it("blocks legacy brand file writes only during explicit attachment rollback", async () => {
  const previousGate = process.env.ERPX_ATTACHMENTS_V2;
  const previousReadOnly = process.env.ERPX_ATTACHMENTS_READ_ONLY;
  process.env.ERPX_ATTACHMENTS_V2 = "false";
  delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  try {
    const invalidImage = new Uint8Array([1, 2, 3]).buffer;
    await expect(completeBrandUpload({
      db,
      files: env.H001_FILES,
      actor: fixture.owner,
      intent: { id: `brand-upload-${crypto.randomUUID()}`, owner_type: "branch_brand", owner_id: fixture.branch, purpose: "primary", revision: 1, original_name: "invalid.png" },
      buffer: invalidImage,
    })).rejects.toMatchObject({ code: "LOGO_INVALID" });
    await expect(completeStorefrontBrandUpload({
      db,
      files: env.H001_FILES,
      actor: fixture.owner,
      intent: { id: `storefront-upload-${crypto.randomUUID()}`, owner_id: fixture.company, revision: 1, original_name: "invalid.png" },
      buffer: invalidImage,
    })).rejects.toMatchObject({ code: "LOGO_INVALID" });
    await expect(updateBrandProfile(db, fixture.owner, {
      scopeKind: "branch",
      scopeId: fixture.branch,
      remove: "primary",
      commandId: crypto.randomUUID(),
    })).resolves.toMatchObject({ result: { scopeId: fixture.branch } });
    await expect(updateStorefrontAppBrand(db, fixture.owner, {
      removeLogo: true,
      commandId: crypto.randomUUID(),
    })).resolves.toMatchObject({ result: { companyId: fixture.company, logoRemoved: true } });

    process.env.ERPX_ATTACHMENTS_READ_ONLY = "true";
    await expect(completeBrandUpload({
      db,
      files: env.H001_FILES,
      actor: fixture.owner,
      intent: { id: `brand-upload-${crypto.randomUUID()}`, owner_type: "branch_brand", owner_id: fixture.branch, purpose: "primary", revision: 1, original_name: "invalid.png" },
      buffer: invalidImage,
    })).rejects.toMatchObject({ status: 503, code: "ATTACHMENTS_READ_ONLY" });
    await expect(completeStorefrontBrandUpload({
      db,
      files: env.H001_FILES,
      actor: fixture.owner,
      intent: { id: `storefront-upload-${crypto.randomUUID()}`, owner_id: fixture.company, revision: 1, original_name: "invalid.png" },
      buffer: invalidImage,
    })).rejects.toMatchObject({ status: 503, code: "ATTACHMENTS_READ_ONLY" });
    await expect(updateBrandProfile(db, fixture.owner, {
      scopeKind: "branch",
      scopeId: fixture.branch,
      remove: "inverse",
      commandId: crypto.randomUUID(),
    })).rejects.toMatchObject({ status: 503, code: "ATTACHMENTS_READ_ONLY" });
    await expect(updateStorefrontAppBrand(db, fixture.owner, {
      removeLogo: true,
      commandId: crypto.randomUUID(),
    })).rejects.toMatchObject({ status: 503, code: "ATTACHMENTS_READ_ONLY" });

    await expect(updateBrandProfile(db, fixture.owner, {
      scopeKind: "branch",
      scopeId: fixture.branch,
      documentMode: "both",
      commandId: crypto.randomUUID(),
    })).resolves.toMatchObject({ result: { documentMode: "both" } });
    await expect(updateStorefrontAppBrand(db, fixture.owner, {
      shortName: "Rollback-safe storefront",
      accentColor: "#123456",
      commandId: crypto.randomUUID(),
    })).resolves.toMatchObject({ result: { shortName: "Rollback-safe storefront", accentColor: "#123456", logoRemoved: false } });
  } finally {
    if (previousGate === undefined) delete process.env.ERPX_ATTACHMENTS_V2;
    else process.env.ERPX_ATTACHMENTS_V2 = previousGate;
    if (previousReadOnly === undefined) delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
    else process.env.ERPX_ATTACHMENTS_READ_ONLY = previousReadOnly;
  }
});
import type { SqlDatabase } from "@erpx/server-runtime/adapters/runtime/types";

async function brandUploadFixture() {
  const token=crypto.randomUUID(),id=`brand-upload-${token}`,now=Date.now();
  const source=await sharp({create:{width:64,height:64,channels:4,background:"transparent"}}).png().toBuffer();
  await db.prepare("INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES(?,'branch_brand',?,'primary','logo.png','image/png',?,?,'pending',?,?,?,?)").bind(id,fixture.branch,source.byteLength,`stage/${id}`,fixture.owner.userId,now+60_000,now,now).run();
  return {intent:{id,owner_type:"branch_brand" as const,owner_id:fixture.branch,purpose:"primary" as const,revision:1,original_name:"logo.png"},buffer:source.buffer.slice(source.byteOffset,source.byteOffset+source.byteLength) as ArrayBuffer,key:`${fixture.owner.holdingId}/branding/branch/${fixture.branch}/primary-${id}.webp`};
}
it("commits Brand profile, File metadata, receipt, audit and outbox as one replacement",async()=>{
  const upload=await brandUploadFixture();
  const result=await completeBrandUpload({db,files:env.H001_FILES,actor:fixture.owner,intent:upload.intent,buffer:upload.buffer});
  expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(upload.intent.id).first("status")).toBe("ready");
  expect(await db.prepare("SELECT primary_attachment_id FROM organization_brand_profiles WHERE scope_kind='branch' AND scope_id=?").bind(fixture.branch).first("primary_attachment_id")).toBe(result.id);
  expect(await db.prepare("SELECT id FROM attachments WHERE upload_id=?").bind(upload.intent.id).first("id")).toBe(result.id);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) count FROM ${table} WHERE command_id=?`).bind(`upload-${upload.intent.id}`).first("count")).toBe(1);
  expect(await env.H001_FILES.head(upload.key)).not.toBeNull();
});
it("rolls back every owner write and compensates the stored object after a late PostgreSQL failure",async()=>{
  const upload=await brandUploadFixture();
  const failAfterWrites:SqlDatabase={prepare:sql=>db.prepare(sql),exec:sql=>db.exec(sql),batch:statements=>db.batch([...statements,db.prepare("SELECT 1/0")])};
  await expect(completeBrandUpload({db:failAfterWrites,files:env.H001_FILES,actor:fixture.owner,intent:upload.intent,buffer:upload.buffer})).rejects.toThrow();
  expect(await db.prepare("SELECT status FROM upload_intents WHERE id=?").bind(upload.intent.id).first("status")).toBe("pending");
  expect(await db.prepare("SELECT count(*) count FROM attachments WHERE upload_id=?").bind(upload.intent.id).first("count")).toBe(0);
  expect(await db.prepare("SELECT count(*) count FROM organization_brand_profiles WHERE scope_kind='branch' AND scope_id=?").bind(fixture.branch).first("count")).toBe(0);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(await db.prepare(`SELECT count(*) count FROM ${table} WHERE command_id=?`).bind(`upload-${upload.intent.id}`).first("count")).toBe(0);
  expect(await env.H001_FILES.head(upload.key)).toBeNull();
});

it.each(['disabled','enforced'] as const)('buffers the original three Brand sources without purchasing a product or granting employee management (%s)',async mode=>{
 const {attachmentContent,bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service');
 vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT',mode);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT',mode);vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT',mode);
 for(const source of [{targetType:'holding_brand',targetId:fixture.owner.holdingId,purposeKey:'primary'},{targetType:'branch_brand',targetId:fixture.branch,purposeKey:'primary'},{targetType:'company_storefront_brand',targetId:fixture.company,purposeKey:'logo'}] as const){
  if(source.targetType==='company_storefront_brand')await updateStorefrontAppBrand(db,fixture.owner,{shortName:'Native private Brand',accentColor:'#123456',commandId:key()});else await updateBrandProfile(db,fixture.owner,{scopeKind:source.targetType==='holding_brand'?'holding':'branch',scopeId:source.targetId,documentMode:source.targetType==='holding_brand'?'holding':'branch',commandId:key()});
  const document=await seedAttachmentSet(db,fixture.owner,{...source,componentKind:'publication',mediaType:'image/png',expectedTargetVersion:1});await bindRecordAttachmentSet(db,fixture.owner,document.setId,1,key());const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_PRIVATE_BRAND_SOURCE_REQUIRED');const bytes=`Original ${source.targetType} source before publication`;await env.H001_FILES.put(objectKey,bytes);let consumed=0;
  const files:FileStore={put:(...args)=>env.H001_FILES.put(...args),get:async(...args)=>{const value=await env.H001_FILES.get(...args);return value?{...value,arrayBuffer:async()=>{consumed++;return value.arrayBuffer();}}:null;},head:(...args)=>env.H001_FILES.head(...args),delete:(...args)=>env.H001_FILES.delete(...args),list:(...args)=>env.H001_FILES.list(...args)};
  const tables=['organization_brand_profiles','storefront_app_brands','organization_nodes','attachments','attachment_links','attachment_access_events','assignments'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(r)::text image FROM ${table} r ORDER BY row_to_json(r)::text`).all()).results));
  for(const kind of ['brand','organization','file','iam'] as const){
   const fault=`brand_private_${key().replaceAll('-','')}`,effect=kind==='brand'?(source.targetType==='company_storefront_brand'?`UPDATE storefront_app_brands SET short_name='Late native storefront source' WHERE company_id='${fixture.company}'`:`UPDATE organization_brand_profiles SET updated_at=updated_at+1 WHERE scope_kind='${source.targetType==='holding_brand'?'holding':'branch'}' AND scope_id='${source.targetId}'`):kind==='organization'?`UPDATE organization_nodes SET display_name='Late native Company identity' WHERE id='${fixture.company}'`:kind==='file'?`UPDATE attachments SET legal_hold=1 WHERE id='${document.sourceId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${fixture.claimant.assignmentId}'`,before=await snapshot();
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${document.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
   try{await expect(attachmentContent(db,files,fixture.claimant,document.linkId,'original')).rejects.toMatchObject({code:'BRAND_FILE_READ_CHANGED'});expect(await snapshot()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  }
  expect(consumed).toBe(4);await db.prepare('UPDATE attachments SET legal_hold=1 WHERE id=?').bind(document.sourceId).run();expect(await(await attachmentContent(db,files,fixture.claimant,document.linkId,'original')).object.text()).toBe(bytes);expect(consumed).toBe(5);await expect(db.transaction(tx=>attachmentContent(tx,files,fixture.claimant,document.linkId,'original'))).rejects.toMatchObject({code:'BRAND_FILE_READ_TRANSACTION_REQUIRED'});expect(consumed).toBe(5);
  await expect(requireBrandManagement(db,fixture.claimant,source.targetType,source.targetId)).rejects.toMatchObject({code:'BRAND_MANAGEMENT_FORBIDDEN'});
 }
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(fixture.company).all()).results).toEqual([]);
});
it('observes a committed normal Brand profile command before its private snapshot and isolates two Company source-writing reads',async()=>{
 const {attachmentContent,bindRecordAttachmentSet}=await import('@erpx/server-files/public/attachments/service'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{companyPurchaseReadLocks}=await import('@erpx/server-commercial/public/purchase-command-boundary'),{measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),first=fixture;
 await updateBrandProfile(db,first.owner,{scopeKind:'branch',scopeId:first.branch,documentMode:'branch',commandId:key()});const file=await seedAttachmentSet(db,first.owner,{targetType:'branch_brand',targetId:first.branch,purposeKey:'primary',componentKind:'publication',mediaType:'image/png',expectedTargetVersion:1});await bindRecordAttachmentSet(db,first.owner,file.setId,1,key());const objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(file.sourceId).first<string>('object_key');if(!objectKey)throw Error('ACTUAL_BRAND_PROFILE_DOCUMENT_REQUIRED');await env.H001_FILES.put(objectKey,'Native retained Branch image');const fault=`brand_visible_${key().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF (SELECT document_mode FROM organization_brand_profiles WHERE scope_kind='branch' AND scope_id='${first.branch}')<>'both' THEN RAISE EXCEPTION 'STALE_NATIVE_BRAND_PROFILE';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${file.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
 try{const observed=await committedSourceVisibility(db,async tx=>{await tx.batch(companyPurchaseReadLocks(tx,first.company));await updateBrandProfile(tx,first.owner,{scopeKind:'branch',scopeId:first.branch,documentMode:'both',commandId:key()});},()=>measureReads(()=>attachmentContent(db,env.H001_FILES,first.claimant,file.linkId,'original')));expect(await observed.result.object.text()).toBe('Native retained Branch image');expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
 const second=await expenseFixture(),rows:{actor:WorkContext;file:Awaited<ReturnType<typeof seedAttachmentSet>>;bytes:string}[]=[];
 for(const source of [first,second]){await updateStorefrontAppBrand(db,source.owner,{shortName:'Private native storefront',accentColor:'#123456',commandId:key()});const document=await seedAttachmentSet(db,source.owner,{targetType:'company_storefront_brand',targetId:source.company,purposeKey:'logo',componentKind:'publication',mediaType:'image/png',expectedTargetVersion:1});await bindRecordAttachmentSet(db,source.owner,document.setId,1,key());const stored=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(document.sourceId).first<string>('object_key');if(!stored)throw Error('ACTUAL_COMPANY_BRAND_FILE_REQUIRED');const bytes=`Company native brand ${source.company}`;await env.H001_FILES.put(stored,bytes);rows.push({actor:source.claimant,file:document,bytes});}
 const parallel=`brand_bytes_parallel_${key().replaceAll('-','')}`;await db.exec(`CREATE FUNCTION ${parallel}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE storefront_app_brands SET short_name=short_name WHERE company_id=NEW.company_id;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${parallel} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id IN ('${rows[0].file.linkId}','${rows[1].file.linkId}') AND NEW.outcome='allowed') EXECUTE FUNCTION ${parallel}()`);
 try{const observed=await measureReads(()=>Promise.all(rows.map(row=>attachmentContent(db,env.H001_FILES,row.actor,row.file.linkId,'original'))));for(let index=0;index<rows.length;index++)expect(await observed.result[index].object.text()).toBe(rows[index].bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${parallel} ON attachment_access_events;DROP FUNCTION ${parallel}()`);}
});
