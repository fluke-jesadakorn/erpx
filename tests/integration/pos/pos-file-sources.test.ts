import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import sharp from "sharp";
import { applyMigrations, env } from "../../support/runtime-env";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import { backfillSpecializedAttachments, reconcileAttachmentStorage } from "@erpx/server-files/public/attachments/migration";
import { attachmentContent,scanAttachmentNow } from "@erpx/server-files/public/attachments/service";
import {seedAttachmentSet} from '../../support/helpers/attachment-v2';
import {packageGrantStore} from '@erpx/server-commercial/public/package-grants';
import type {FileStore} from '@erpx/server-runtime/public/runtime/types';
import { posProductImage, savePosProductImage } from "@erpx/server-pos/public/pos-images";

const scan = vi.hoisted(() => ({ clean: true }));
vi.mock("@erpx/server-runtime/adapters/runtime", async () => {
  const { env } = await import("../../support/runtime-env");
  return { runtimeEnv: async () => env, holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }) };
});

vi.mock("@erpx/server-files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "ClamAV POS-source fixture", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: scan.clean, signature: scan.clean ? null : "POS source infected fixture", engine: "ClamAV POS-source fixture", definitionUpdatedAt: Date.now() }),
}));
const db = env.H001_DB, files = env.H001_FILES;
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { scan.clean = true; f = await seedExpenseCompany(db, files); });
afterEach(() => vi.unstubAllEnvs());
async function catalog(source=f) {
  const product = `pos-source-product-${crypto.randomUUID()}`;
  await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(product, source.company, product, "POS intake fixture").run();
  await db.prepare("INSERT INTO pos_catalog(product_id,company_id,kind) VALUES(?,?,'stock')").bind(product, source.company).run();
  const bytes = new Uint8Array(await sharp({ create: { width: 20, height: 10, channels: 4, background: "#325eaa" } }).png().toBuffer());
  return { product, bytes };
}
async function image(options: { badThumbnail?: boolean; badChecksum?: boolean } = {}) {
  const product = `pos-source-product-${crypto.randomUUID()}`, id = `pos-source-image-${crypto.randomUUID()}`;
  const bytes = await sharp({ create: { width: 20, height: 10, channels: 4, background: "#325eaa" } }).png().toBuffer();
  const thumbnail = options.badThumbnail ? await sharp({ create: { width: 480, height: 480, channels: 4, background: "#aa1111" } }).webp({ quality: 82 }).toBuffer()
    : await sharp(bytes).rotate().resize(480, 480, { fit: "cover" }).webp({ quality: 82 }).toBuffer();
  const checksum = createHash("sha256").update(bytes).digest("hex"), thumbnailChecksum = createHash("sha256").update(thumbnail).digest("hex");
  const objectKey = `pos/${f.company}/${product}/${checksum}/original`, thumbnailKey = `pos/${f.company}/${product}/${checksum}/thumbnail.webp`;
  await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(product, f.company, product, "POS source fixture").run();
  await files.put(objectKey, bytes, { httpMetadata: { contentType: "image/png" } });
  await files.put(thumbnailKey, thumbnail, { httpMetadata: { contentType: "image/webp" } });
  await db.prepare(`INSERT INTO pos_product_images(id,company_id,product_id,object_key,thumbnail_key,sha256,width,height,mime_type,source,created_by,created_at)
    VALUES(?,?,?,?,?,?,20,10,'image/png','generated',?,?)`).bind(id, f.company, product, objectKey, thumbnailKey, options.badChecksum ? "0".repeat(64) : checksum, f.owner.userId, Date.now()).run();
  return { id, product, checksum, thumbnailChecksum, objectKey, thumbnailKey, originalBytes: bytes.length, thumbnailBytes: thumbnail.length };
}
describe("POS product images share canonical File Sources", () => {
  it("previews verified provenance then imports, scans and reconciles originals and exact thumbnails once", async () => {
    const source = await image();
    const preview = await backfillSpecializedAttachments(db, { files });
    expect(preview).toMatchObject({ ready: 1, backfilled: 0, unknown: [] });
    expect(await db.prepare("SELECT attachment_source_id FROM pos_product_images WHERE id=?").bind(source.id).first()).toEqual({ attachment_source_id: null });
    const suffix=crypto.randomUUID().replaceAll("-",""),trigger="migration_late_"+suffix,fn=trigger+"_failure";
    await db.prepare(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'late source binding failure' USING ERRCODE='23514'; END $$`).run();
    await db.prepare(`CREATE TRIGGER ${trigger} BEFORE UPDATE ON attachment_sets FOR EACH ROW WHEN (NEW.target_id='${source.product}' AND NEW.state='bound') EXECUTE FUNCTION ${fn}()`).run();
    try {
      await expect(backfillSpecializedAttachments(db,{files,apply:true})).rejects.toThrow("late source binding failure");
      expect(await db.prepare("SELECT attachment_source_id,attachment_set_id,attachment_link_id,thumbnail_checksum_sha256 FROM pos_product_images WHERE id=?").bind(source.id).first()).toEqual({attachment_source_id:null,attachment_set_id:null,attachment_link_id:null,thumbnail_checksum_sha256:null});
      expect(await db.prepare("SELECT COUNT(*)::int n FROM attachments WHERE owner_id=?").bind(source.product).first("n")).toBe(0);
      expect(await db.prepare("SELECT COUNT(*)::int n FROM attachment_sets WHERE target_id=?").bind(source.product).first("n")).toBe(0);
      expect(await db.prepare("SELECT COUNT(*)::int n FROM attachment_storage_ledger WHERE object_key IN (?,?)").bind(source.objectKey,source.thumbnailKey).first("n")).toBe(0);
      expect((await files.head(source.objectKey))?.etag).toBe(source.checksum);
      expect((await files.head(source.thumbnailKey))?.etag).toBe(source.thumbnailChecksum);
    } finally {await db.prepare(`DROP TRIGGER ${trigger} ON attachment_sets`).run();await db.prepare(`DROP FUNCTION ${fn}()`).run();}
    expect(await backfillSpecializedAttachments(db, { files, apply: true })).toMatchObject({ ready: 1, backfilled: 1, unknown: [] });
    const row = await db.prepare("SELECT attachment_source_id,attachment_set_id,thumbnail_checksum_sha256 FROM pos_product_images WHERE id=?").bind(source.id).first<{ attachment_source_id: string; attachment_set_id: string; thumbnail_checksum_sha256: string }>();
    expect(row!.thumbnail_checksum_sha256).toBe(source.thumbnailChecksum);
    expect(await db.prepare("SELECT company_id,owner_type,owner_id,source_state,checksum_sha256 FROM attachments WHERE id=?").bind(row!.attachment_source_id).first())
      .toEqual({ company_id: f.company, owner_type: "product", owner_id: source.product, source_state: "quarantined", checksum_sha256: source.checksum });
    expect(await db.prepare("SELECT state,target_type,target_id FROM attachment_sets WHERE id=?").bind(row!.attachment_set_id).first())
      .toEqual({ state: "bound", target_type: "product", target_id: source.product });
    expect(await scanAttachmentNow(db, files, row!.attachment_source_id)).toBe("available");
    expect(await reconcileAttachmentStorage(db, files, { verifyChecksums: true })).toMatchObject({ unknownOwnership: 0, unavailable: 0,
      orphanObjects: [], missingLedger: [], checksumMismatches: [], quotaDeltaBytes: 0, crossCompanyStorage: [] });
    expect(await backfillSpecializedAttachments(db, { files, apply: true })).toMatchObject({ ready: 0, backfilled: 0, unknown: [] });
    const ledger = await db.prepare("SELECT SUM(bytes)::text bytes FROM attachment_storage_ledger WHERE attachment_id=? AND variant IN ('original','derivative')").bind(row!.attachment_source_id).first();
    expect(ledger).toEqual({ bytes: String(source.originalBytes + source.thumbnailBytes) });
  });
  it("refuses filename-based ownership when original checksum or thumbnail provenance differs", async () => {
    const wrongThumbnail = await image({ badThumbnail: true }), wrongOriginal = await image({ badChecksum: true });
    const result = await backfillSpecializedAttachments(db, { files, apply: true });
    expect(result.unknown).toEqual(expect.arrayContaining([
      { kind: "pos_product_image", id: wrongThumbnail.id, reason: "POS_THUMBNAIL_PROVENANCE_MISMATCH" },
      { kind: "pos_product_image", id: wrongOriginal.id, reason: "POS_IMAGE_CHECKSUM_MISMATCH" },
    ]));
    expect(await db.prepare("SELECT COUNT(*)::int n FROM pos_product_images WHERE id IN (?,?) AND attachment_source_id IS NOT NULL").bind(wrongThumbnail.id, wrongOriginal.id).first()).toEqual({ n: 0 });
  });
  it("binds new intake in the POS transaction and replays a lost response without duplicating sources or quota", async () => {
    const { product, bytes } = await catalog(), key = crypto.randomUUID();
    const result = await savePosProductImage(f.owner, product, bytes, "image/png", "upload", key, 1);
    expect(await savePosProductImage(f.owner, product, bytes, "image/png", "upload", key, 1)).toEqual(result);
    expect(await db.prepare("SELECT version,image_id FROM pos_catalog WHERE product_id=?").bind(product).first()).toEqual({ version: 2, image_id: result.imageId });
    const source = await db.prepare("SELECT a.source_state,l.state link_state,s.state set_state FROM pos_product_images i JOIN attachments a ON a.id=i.attachment_source_id JOIN attachment_links l ON l.id=i.attachment_link_id JOIN attachment_sets s ON s.id=i.attachment_set_id WHERE i.id=?").bind(result.imageId).first();
    expect(source).toEqual({ source_state: "available", link_state: "bound", set_state: "bound" });
    const response = await posProductImage(f.owner, String(result.imageId));
    expect(response.headers.get("cache-control")).toBe("private, no-store");
    expect((await response.arrayBuffer()).byteLength).toBeGreaterThan(0);
    expect(await db.prepare("SELECT COUNT(*)::int n FROM attachments WHERE company_id=? AND owner_id=?").bind(f.company, product).first()).toEqual({ n: 1 });
    expect(await db.prepare("SELECT COUNT(*)::int n FROM command_assertions WHERE command_id=?").bind(key).first()).toEqual({ n: 0 });
    await db.prepare("UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?").bind(Date.now(), f.owner.assignmentId).run();
    await expect(posProductImage(f.owner, String(result.imageId))).rejects.toMatchObject({ status: 404 });
    await expect(savePosProductImage(f.owner, product, bytes, "image/png", "upload", key, 1)).rejects.toMatchObject({ code: "POS_ACCESS_CHANGED" });
  });
  it("keeps infected intake sealed and refuses stale catalog writes without publishing an image", async () => {
    const { product, bytes } = await catalog();
    await expect(savePosProductImage(f.owner, product, bytes, "image/png", "upload", crypto.randomUUID(), 2)).rejects.toMatchObject({ code: "POS_VERSION_CONFLICT" });
    scan.clean = false;
    await expect(savePosProductImage(f.owner, product, bytes, "image/png", "upload", crypto.randomUUID(), 1)).rejects.toMatchObject({ code: "POS_IMAGE_REJECTED" });
    expect(await db.prepare("SELECT image_id,version FROM pos_catalog WHERE product_id=?").bind(product).first()).toEqual({ image_id: null, version: 1 });
    expect(await db.prepare("SELECT source_state FROM attachments WHERE company_id=? AND owner_id=?").bind(f.company, product).first()).toEqual({ source_state: "malware_rejected" });
    expect(await db.prepare("SELECT COUNT(*)::int n FROM pos_product_images WHERE company_id=? AND product_id=?").bind(f.company, product).first()).toEqual({ n: 0 });
  });
});

it.each(['disabled','enforced'] as const)('keeps specialized POS image bytes and content audits inside current owner checks (%s)',async mode=>{
 const {product,bytes}=await catalog(),saved=await savePosProductImage(f.owner,product,bytes,'image/png','upload',crypto.randomUUID(),1),imageId=String(saved.imageId),sourceId=await db.prepare('SELECT attachment_source_id FROM pos_product_images WHERE id=?').bind(imageId).first<string>('attachment_source_id');if(!sourceId)throw Error('ACTUAL_POS_IMAGE_SOURCE_REQUIRED');
 await packageGrantStore(db,{authorize:async()=>({userId:'isolated-POS-image-provider'}),obligations:async()=>{throw Error('READ_FIXTURE_CANNOT_INACTIVATE');}}).apply({expectedVersion:0,idempotencyKey:crypto.randomUUID(),grant:{companyId:f.company,packageId:'pos',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual specialized POS image read fixture',evidence:'test:pos-image-read'}});
 vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT',mode);let consumed=0;const get=files.get.bind(files),spy=vi.spyOn(files,'get').mockImplementation(async objectKey=>{const object=await get(objectKey);return object?{...object,arrayBuffer:async()=>{consumed++;return object.arrayBuffer();}}:null;});
 try{
  for(const kind of ['product','catalog','image','file','iam'] as const){
   const fault=`pos_image_read_${crypto.randomUUID().replaceAll('-','')}`,effect=kind==='product'?`UPDATE commerce_products SET version=version+1 WHERE id='${product}'`:kind==='catalog'?`UPDATE pos_catalog SET version=version+1 WHERE product_id='${product}'`:kind==='image'?`UPDATE pos_product_images SET width=width+1 WHERE id='${imageId}'`:kind==='file'?`UPDATE attachments SET legal_hold=1 WHERE id='${sourceId}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'`;
   await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.attachment_id='${sourceId}' AND NEW.action='content.read') EXECUTE FUNCTION ${fault}()`);
   try{await expect(posProductImage(f.owner,imageId,true)).rejects.toMatchObject({code:'POS_IMAGE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_events;DROP FUNCTION ${fault}()`);}
   expect(await db.prepare("SELECT count(*)::int n FROM attachment_events WHERE attachment_id=? AND action='content.read'").bind(sourceId).first('n')).toBe(0);
  }
  expect(consumed).toBe(5);const response=await posProductImage(f.owner,imageId,true);expect(new Uint8Array(await response.arrayBuffer())).toEqual(bytes);expect(consumed).toBe(6);expect(response.headers.get('cache-control')).toBe('private, no-store');
  expect(await db.prepare("SELECT count(*)::int n FROM attachment_events WHERE attachment_id=? AND action='content.read'").bind(sourceId).first('n')).toBe(1);
  expect(await db.prepare('SELECT version FROM pos_catalog WHERE product_id=?').bind(product).first('version')).toBe(2);expect(await db.prepare('SELECT revoked_at,version FROM assignments WHERE id=?').bind(f.owner.assignmentId).first()).toEqual({revoked_at:null,version:1});
  const {withPosProductImageRead}=await import('@erpx/server-pos/public/product-image-read');await expect(db.transaction(tx=>withPosProductImageRead(tx,f.owner,imageId,async()=>{throw Error('ARBITRARY_SNAPSHOT_MUST_NOT_RUN');}))).rejects.toMatchObject({code:'POS_IMAGE_READ_TRANSACTION_REQUIRED'});
 }finally{spy.mockRestore();}
});

it('observes committed native POS image sources before its read snapshot and reads two Companies without hidden transaction retries',async()=>{
 const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),{committedSourceVisibility}=await import('../../support/helpers/source-writer-visibility'),{companyPurchaseReadLocks}=await import('@erpx/server-commercial/public/purchase-command-boundary');
 const other=await seedExpenseCompany(db,files),sources:{fixture:typeof f;product:string;bytes:Uint8Array;imageId:string}[]=[];
 for(const fixture of [f,other]){const {product,bytes}=await catalog(fixture),saved=await savePosProductImage(fixture.owner,product,bytes,'image/png','upload',crypto.randomUUID(),1);sources.push({fixture,product,bytes,imageId:String(saved.imageId)});}
 vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','disabled');const first=sources[0],fault=`pos_image_visible_${crypto.randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF (SELECT category FROM pos_catalog WHERE product_id='${first.product}')<>'Committed original POS image source' THEN RAISE EXCEPTION 'STALE_POS_IMAGE_SOURCE_SNAPSHOT';END IF;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.action='content.read' AND NEW.company_id='${f.company}') EXECUTE FUNCTION ${fault}()`);
 try{const observed=await committedSourceVisibility(db,async tx=>{await tx.batch(companyPurchaseReadLocks(tx,f.company));await tx.prepare("UPDATE pos_catalog SET category='Committed original POS image source',version=version+1 WHERE product_id=?").bind(first.product).run();},()=>measureReads(()=>posProductImage(first.fixture.owner,first.imageId,true)));expect(new Uint8Array(await observed.result.arrayBuffer())).toEqual(first.bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_events;DROP FUNCTION ${fault}()`);}
 const parallel=`pos_image_parallel_${crypto.randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${parallel}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.08);UPDATE pos_catalog SET version=version WHERE product_id=CASE WHEN NEW.company_id='${f.company}' THEN '${first.product}' ELSE '${sources[1].product}' END;RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${parallel} AFTER INSERT ON attachment_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.action='content.read' AND NEW.company_id IN ('${f.company}','${other.company}')) EXECUTE FUNCTION ${parallel}()`);
 try{const observed=await measureReads(()=>Promise.all(sources.map(source=>posProductImage(source.fixture.owner,source.imageId,true))));for(let index=0;index<sources.length;index++)expect(new Uint8Array(await observed.result[index].arrayBuffer())).toEqual(sources[index].bytes);expect(observed.metrics.deadlockFailures??0).toBe(0);expect(observed.metrics.serializationFailures??0).toBe(0);expect(observed.metrics.transactionRetries??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${parallel} ON attachment_events;DROP FUNCTION ${parallel}()`);}
});

it.each(['pos_sale','pos_refund','pos_shift'] as const)('buffers retained %s evidence through original source and current account checks without full Finance',async target=>{
 vi.stubEnv('ERPX_ATTACHMENTS_V2','true');vi.stubEnv('ERPX_POS_PACKAGE_ENFORCEMENT','enforced');
 await packageGrantStore(db,{authorize:async()=>({userId:'isolated-private-POS-files-provider'}),obligations:async()=>{throw Error('READ_FIXTURE_CANNOT_INACTIVATE');}}).apply({expectedVersion:0,idempotencyKey:crypto.randomUUID(),grant:{companyId:f.company,packageId:'pos',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Original retained POS evidence fixture',evidence:'test:private-pos-owner-read'}});
 // Retained source fixtures exercise only the original File read policy. No
 // sale, refund payment, accounting or business-day completion is inferred.
 const ids={warehouse:crypto.randomUUID(),register:crypto.randomUUID(),shift:crypto.randomUUID(),sale:crypto.randomUUID(),refund:crypto.randomUUID()};
 await db.batch([
  db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name) VALUES(?,?,?,?,?)").bind(ids.warehouse,f.company,f.branch,ids.warehouse,'Retained evidence warehouse'),
  db.prepare("INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode) VALUES(?,?,?,?,?,'retail')").bind(ids.register,f.company,f.branch,ids.warehouse,'Retained evidence register'),
  db.prepare("INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state) VALUES(?,?,?,?,?,1,0,'closed')").bind(ids.shift,ids.register,f.company,f.branch,f.owner.userId),
  db.prepare("INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,created_by,created_at,updated_at,accounts_json) VALUES(?,?,?,?,?,?,'THB','2026-09-01','cancelled',?,1,1,'{}')").bind(ids.sale,f.company,f.branch,ids.register,ids.shift,ids.sale,f.owner.userId),
  db.prepare("INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,reason,lines_json,amount_minor,state,requested_by,created_at,accounting_date) VALUES(?,?,?,?,'Retained rejected request','[]',1,'rejected',?,1,'2026-09-01')").bind(ids.refund,ids.sale,f.company,f.branch,f.owner.userId),
 ]);
 const id=target==='pos_sale'?ids.sale:target==='pos_refund'?ids.refund:ids.shift,purpose=target==='pos_sale'?'void_evidence':target==='pos_refund'?'refund_evidence':'cash_variance',table=target==='pos_sale'?'pos_sales':target==='pos_refund'?'pos_refunds':'pos_shifts',evidence=await seedAttachmentSet(db,f.owner,{targetType:target,targetId:id,componentKind:'action',purposeKey:purpose,expectedTargetVersion:1}),objectKey=await db.prepare('SELECT object_key FROM attachments WHERE id=?').bind(evidence.sourceId).first<string>('object_key');if(!objectKey)throw Error('NATIVE_POS_FILE_SOURCE_REQUIRED');
 const bytes='%PDF-1.7 retained actual POS evidence';await files.put(objectKey,bytes);let consumed=0;
 const lazy:FileStore={put:(...args)=>files.put(...args),get:async(...args)=>{const result=await files.get(...args);return result?{...result,arrayBuffer:async()=>{consumed++;return result.arrayBuffer();}}:null;},head:(...args)=>files.head(...args),delete:(...args)=>files.delete(...args),list:(...args)=>files.list(...args)};
 for(const change of ['source','iam'] as const){
  const fault=`pos_private_${crypto.randomUUID().replaceAll('-','')}`,effect=change==='source'?`UPDATE ${table} SET version=version+1 WHERE id='${id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.owner.assignmentId}'`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON attachment_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.link_id='${evidence.linkId}' AND NEW.outcome='allowed') EXECUTE FUNCTION ${fault}()`);
  try{await expect(attachmentContent(db,lazy,f.owner,evidence.linkId,'original')).rejects.toMatchObject({code:'HOSPITALITY_FILE_READ_CHANGED'});}finally{await db.exec(`DROP TRIGGER ${fault} ON attachment_access_events;DROP FUNCTION ${fault}()`);}
  expect(Number(await db.prepare("SELECT count(*) n FROM attachment_access_events WHERE link_id=? AND outcome='allowed'").bind(evidence.linkId).first('n'))).toBe(0);
 }
 expect(consumed).toBe(2);expect(await(await attachmentContent(db,lazy,f.owner,evidence.linkId,'original')).object.text()).toBe(bytes);expect(consumed).toBe(3);
 expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=?').bind(f.company).all<{package_id:string}>()).results).toEqual([{package_id:'pos'}]);
 expect(await db.prepare(`SELECT version FROM ${table} WHERE id=?`).bind(id).first('version')).toBe(1);
});
