import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createHash } from "node:crypto";
import sharp from "sharp";
import { applyMigrations, env } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import { backfillSpecializedAttachments, reconcileAttachmentStorage } from "../src/server/attachments/migration";
import { scanAttachmentNow } from "../src/server/attachments/service";
import { posProductImage, savePosProductImage } from "../src/server/pos-images";

const scan = vi.hoisted(() => ({ clean: true }));
vi.mock("../src/server/runtime", async () => {
  const { env } = await import("./runtime-env");
  return { runtimeEnv: async () => env, holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }) };
});

vi.mock("../src/server/modules/files/public/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({ enabled: true, ready: true, engine: "ClamAV POS-source fixture", definitionUpdatedAt: Date.now(), error: null }),
  scanAttachmentBytes: async () => ({ clean: scan.clean, signature: scan.clean ? null : "POS source infected fixture", engine: "ClamAV POS-source fixture", definitionUpdatedAt: Date.now() }),
}));
const db = env.H001_DB, files = env.H001_FILES;
let f: Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async () => { await applyMigrations(db, env.STAFF_TEST_MIGRATIONS); });
beforeEach(async () => { scan.clean = true; f = await seedExpenseCompany(db, files); });
afterEach(() => vi.unstubAllEnvs());
async function catalog() {
  const product = `pos-source-product-${crypto.randomUUID()}`;
  await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,price_minor) VALUES(?,?,?,?,100)").bind(product, f.company, product, "POS intake fixture").run();
  await db.prepare("INSERT INTO pos_catalog(product_id,company_id,kind) VALUES(?,?,'stock')").bind(product, f.company).run();
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
