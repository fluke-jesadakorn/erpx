import sharp from "sharp";
import { beforeAll, beforeEach, expect, it } from "vitest";
import {
  completeBrandUpload,
  completeStorefrontBrandUpload,
  normalizeBrandImage,
  recordBranchId,
  requireBrandManagement,
  resolveDocumentBrand,
  updateBrandProfile,
  updateStorefrontAppBrand,
} from "../../../src/server/modules/brand/public/branding";
import { matchesUploadSignature } from "../../../src/server/modules/files/public/upload-validation";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture } from "../../support/helpers/expense-fixture";

let fixture: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});

beforeEach(async () => {
  fixture = await expenseFixture();
});

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
import type { SqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";

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
