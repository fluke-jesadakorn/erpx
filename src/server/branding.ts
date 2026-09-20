import "server-only";

import sharp from "sharp";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, type WorkContext } from "./authorization";
import { assertion, executeCommand } from "./commands";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
import { companyAuthority } from "./staff-policy";
import type { FileStore, SqlDatabase } from "./runtime/types";

export const documentBrandModes = ["holding", "branch", "both"] as const;
export type DocumentBrandMode = (typeof documentBrandModes)[number];
export type BrandVariant = "primary" | "inverse";

type BrandProfileRow = {
  scope_kind: "holding" | "branch";
  scope_id: string;
  company_id: string | null;
  primary_attachment_id: string | null;
  inverse_attachment_id: string | null;
  document_mode: DocumentBrandMode;
  updated_at: number;
};

export type WorkspaceBrand = {
  scopeKind: "holding" | "branch";
  scopeId: string;
  label: string;
  initials: string;
  primaryUrl: string | null;
  inverseUrl: string | null;
  documentMode: DocumentBrandMode;
};

export type DocumentBrand = {
  mode: DocumentBrandMode;
  companyName: string;
  branchName: string | null;
  holding: WorkspaceBrand;
  branch: WorkspaceBrand | null;
};

function initials(label: string) {
  const words = label.trim().split(/\s+/u).filter(Boolean);
  const selected = words.length > 1 ? [words[0], words.at(-1)!] : words;
  return selected.map((word) => Array.from(word)[0] ?? "").join("").toLocaleUpperCase().slice(0, 3) || "—";
}

function attachmentUrl(actor: WorkContext, id: string | null) {
  return id
    ? `/api/v1${workspaceBase(publicScope(actor))}/attachments/${encodeURIComponent(id)}/content`
    : null;
}

async function row(db: SqlDatabase, kind: "holding" | "branch", id: string) {
  return db.prepare("SELECT * FROM organization_brand_profiles WHERE scope_kind=? AND scope_id=?")
    .bind(kind, id).first<BrandProfileRow>();
}

function presentation(actor: WorkContext, kind: "holding" | "branch", id: string, label: string, profile: BrandProfileRow | null): WorkspaceBrand {
  return {
    scopeKind: kind,
    scopeId: id,
    label,
    initials: initials(label),
    primaryUrl: attachmentUrl(actor, profile?.primary_attachment_id ?? null),
    inverseUrl: attachmentUrl(actor, profile?.inverse_attachment_id ?? null),
    documentMode: kind === "holding" ? "holding" : profile?.document_mode ?? "branch",
  };
}

export async function resolveWorkspaceBrand(db: SqlDatabase, actor: WorkContext): Promise<WorkspaceBrand> {
  if (actor.branchId) {
    const branch = await db.prepare("SELECT id,display_name FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'")
      .bind(actor.branchId, actor.companyId).first<{ id: string; display_name: string }>();
    if (branch) return presentation(actor, "branch", branch.id, branch.display_name, await row(db, "branch", branch.id));
  }
  return presentation(actor, "holding", actor.holdingId, actor.holdingName, await row(db, "holding", actor.holdingId));
}

export async function resolveDocumentBrand(
  db: SqlDatabase,
  actor: WorkContext,
  scope: { branchId: string | null; requestedMode?: string | null },
): Promise<DocumentBrand> {
  const company = await db.prepare("SELECT display_name FROM organization_nodes WHERE id=? AND kind='company' AND status='active'")
    .bind(actor.companyId).first<{ display_name: string }>();
  if (!company) throw new ApiError(409, "COMPANY_REQUIRED", "The legal Company is unavailable.");
  const holding = presentation(actor, "holding", actor.holdingId, actor.holdingName, await row(db, "holding", actor.holdingId));
  if (!scope.branchId) return { mode: "holding", companyName: company.display_name, branchName: null, holding, branch: null };
  const branchRow = await db.prepare("SELECT id,display_name FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'")
    .bind(scope.branchId, actor.companyId).first<{ id: string; display_name: string }>();
  if (!branchRow) throw new ApiError(404, "BRANCH_NOT_FOUND", "The document Branch is unavailable.");
  const profile = await row(db, "branch", branchRow.id);
  const branch = presentation(actor, "branch", branchRow.id, branchRow.display_name, profile);
  const requested = documentBrandModes.includes(scope.requestedMode as DocumentBrandMode)
    ? scope.requestedMode as DocumentBrandMode
    : null;
  return {
    mode: requested ?? profile?.document_mode ?? "branch",
    companyName: company.display_name,
    branchName: branchRow.display_name,
    holding,
    branch,
  };
}

export async function recordBranchId(
  db: SqlDatabase,
  actor: WorkContext,
  kind: "order" | "invoice" | "payment" | "journal" | "expense",
  id: string,
): Promise<string | null> {
  if (kind === "order" || kind === "expense") {
    const table = kind === "order" ? "commerce_orders" : "finance_expenses";
    const found = await db.prepare(`SELECT branch_id FROM ${table} WHERE id=? AND company_id=?`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
    return found?.branch_id ?? null;
  }
  if (kind === "invoice") {
    const found = await db.prepare(`SELECT CASE
      WHEN i.source_type='commerce_fulfillment' THEN (SELECT o.branch_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=i.source_id)
      WHEN i.source_type='expense_claim' THEN (SELECT e.branch_id FROM finance_expenses e WHERE e.id=i.source_id)
      ELSE NULL END branch_id FROM finance_invoices i WHERE i.id=? AND i.company_id=?`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
    return found?.branch_id ?? null;
  }
  if (kind === "payment") {
    const found = await db.prepare(`SELECT CASE WHEN COUNT(DISTINCT branch_id)=1 THEN MIN(branch_id) ELSE NULL END branch_id FROM (
      SELECT CASE WHEN i.source_type='commerce_fulfillment' THEN (SELECT o.branch_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE f.id=i.source_id)
        WHEN i.source_type='expense_claim' THEN (SELECT e.branch_id FROM finance_expenses e WHERE e.id=i.source_id) ELSE NULL END branch_id
      FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=? AND a.company_id=?) branded WHERE branch_id IS NOT NULL`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
    return found?.branch_id ?? null;
  }
  const found = await db.prepare(`SELECT CASE
    WHEN j.source_type='expense_claim' THEN (SELECT e.branch_id FROM finance_expenses e WHERE e.id=j.source_id)
    WHEN j.source_type='stock_count' THEN (SELECT c.branch_id FROM inventory_counts c WHERE c.id=j.source_id)
    ELSE NULL END branch_id FROM finance_journals j WHERE j.id=? AND j.company_id=?`).bind(id, actor.companyId).first<{ branch_id: string | null }>();
  return found?.branch_id ?? null;
}

export async function listBrandProfiles(db: SqlDatabase, actor: WorkContext) {
  const branches = (await db.prepare("SELECT id,display_name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' ORDER BY display_name,id")
    .bind(actor.companyId).all<{ id: string; display_name: string }>()).results;
  const profiles = (await db.prepare("SELECT * FROM organization_brand_profiles WHERE scope_kind='holding' OR company_id=?")
    .bind(actor.companyId).all<BrandProfileRow>()).results;
  const holdingRow = profiles.find((item) => item.scope_kind === "holding") ?? null;
  return {
    holding: presentation(actor, "holding", actor.holdingId, actor.holdingName, holdingRow),
    branches: branches.map((branch) => presentation(actor, "branch", branch.id, branch.display_name, profiles.find((item) => item.scope_kind === "branch" && item.scope_id === branch.id) ?? null)),
  };
}

export async function requireBrandManagement(
  db: SqlDatabase,
  actor: WorkContext,
  ownerType: "holding_brand" | "branch_brand",
  ownerId: string,
) {
  const allowed = ownerType === "holding_brand"
    ? ownerId === actor.holdingId && await companyAuthority(db, actor, ["holding_owner"])
    : await companyAuthority(db, actor, "organization.manage") && !!(await db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active'").bind(ownerId, actor.companyId).first());
  if (!allowed) throw new ApiError(403, "BRAND_MANAGEMENT_FORBIDDEN", "A Holding owner or authorized organization administrator is required.");
}

export async function normalizeBrandImage(buffer: ArrayBuffer) {
  const input = Buffer.from(buffer);
  let metadata;
  try { metadata = await sharp(input, { failOn: "warning" }).metadata(); }
  catch { throw new ApiError(422, "LOGO_INVALID", "Upload a valid PNG or WebP image."); }
  if (!metadata.width || !metadata.height || metadata.width < 32 || metadata.height < 32)
    throw new ApiError(422, "LOGO_DIMENSIONS", "Logo dimensions must be at least 32 by 32 pixels.");
  if (metadata.width * metadata.height > 16_777_216)
    throw new ApiError(422, "LOGO_PIXELS", "Logo dimensions must not exceed 16 megapixels.");
  return sharp(input, { failOn: "warning" }).rotate().resize({ width: 4096, height: 4096, fit: "inside", withoutEnlargement: true }).webp({ quality: 92, alphaQuality: 100 }).toBuffer();
}

export async function completeBrandUpload(args: {
  db: SqlDatabase;
  files: FileStore;
  actor: WorkContext;
  intent: { id: string; owner_type: "holding_brand" | "branch_brand"; owner_id: string; purpose: BrandVariant; revision: number; original_name: string };
  buffer: ArrayBuffer;
}) {
  const { db, files, actor, intent } = args;
  await requireBrandManagement(db, actor, intent.owner_type, intent.owner_id);
  const normalized = await normalizeBrandImage(args.buffer);
  const checksum = await sha256(normalized.buffer.slice(normalized.byteOffset, normalized.byteOffset + normalized.byteLength) as ArrayBuffer);
  const commandId = `upload-${intent.id}`;
  const kind = intent.owner_type === "holding_brand" ? "holding" : "branch";
  const column = intent.purpose === "inverse" ? "inverse_attachment_id" : "primary_attachment_id";
  const existing = await row(db, kind, intent.owner_id);
  const previousId = intent.purpose === "inverse" ? existing?.inverse_attachment_id : existing?.primary_attachment_id;
  const previous = previousId ? await db.prepare("SELECT object_key FROM attachments WHERE id=?").bind(previousId).first<{ object_key: string }>() : null;
  const finalKey = `${actor.holdingId}/branding/${kind}/${intent.owner_id}/${intent.purpose}-${intent.id}.webp`;
  await files.put(finalKey, normalized, { onlyIf: { etagDoesNotMatch: "*" }, httpMetadata: { contentType: "image/webp" }, customMetadata: { sha256: checksum } });
  const attachmentId = `attachment-${intent.id}`;
  const now = Date.now();
  const result = { id: attachmentId, mediaType: "image/webp", bytes: normalized.byteLength, checksum };
  try {
    await executeCommand(db, actor, commandId, "organization.brand.replace", { ownerType: intent.owner_type, ownerId: intent.owner_id, variant: intent.purpose, checksum }, intent.owner_id, 1, result, [
      assertion(db, commandId, "brand-owner", kind === "holding"
        ? "?=?"
        : "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active')",
      kind === "holding" ? [intent.owner_id, actor.holdingId] : [intent.owner_id, actor.companyId]),
      db.prepare("UPDATE upload_intents SET status='ready',actual_bytes=?,checksum_sha256=?,final_key=?,updated_at=? WHERE id=? AND status='pending'").bind(normalized.byteLength, checksum, finalKey, now, intent.id),
      db.prepare("INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(attachmentId, intent.id, intent.owner_type, intent.owner_id, intent.purpose, finalKey, intent.revision, intent.original_name.replace(/\.[^.]+$/, "") + ".webp", "image/webp", normalized.byteLength, checksum, actor.userId, now),
      db.prepare(`INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,${column},document_mode,updated_by,updated_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET ${column}=excluded.${column},updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(kind, intent.owner_id, kind === "branch" ? actor.companyId : null, attachmentId, kind === "holding" ? "holding" : "branch", actor.userId, now),
      ...(previousId ? [db.prepare("UPDATE attachments SET revoked_at=? WHERE id=? AND revoked_at IS NULL").bind(now, previousId)] : []),
    ], { companyId: actor.companyId, branchId: kind === "branch" ? intent.owner_id : null, departmentId: null });
  } catch (error) {
    await files.delete(finalKey).catch(() => undefined);
    throw error;
  }
  if (previous?.object_key) await files.delete(previous.object_key).catch(() => undefined);
  return result;
}

export async function updateBrandProfile(db: SqlDatabase, actor: WorkContext, input: {
  scopeKind: "holding" | "branch";
  scopeId: string;
  documentMode?: DocumentBrandMode;
  remove?: BrandVariant;
  commandId: string;
}) {
  await requireBrandManagement(db, actor, input.scopeKind === "holding" ? "holding_brand" : "branch_brand", input.scopeId);
  if (input.scopeKind === "holding" && input.documentMode && input.documentMode !== "holding")
    throw new ApiError(422, "BRAND_MODE_INVALID", "Holding branding uses Holding mode.");
  const current = await row(db, input.scopeKind, input.scopeId);
  const removeColumn = input.remove === "primary" ? "primary_attachment_id" : input.remove === "inverse" ? "inverse_attachment_id" : null;
  const previousId = removeColumn ? (removeColumn === "primary_attachment_id" ? current?.primary_attachment_id : current?.inverse_attachment_id) : null;
  const now = Date.now();
  const result = { scopeKind: input.scopeKind, scopeId: input.scopeId, documentMode: input.scopeKind === "holding" ? "holding" : input.documentMode ?? current?.document_mode ?? "branch" };
  await executeCommand(db, actor, input.commandId, "organization.brand.update", input, input.scopeId, 1, result, [
    db.prepare(`INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(scope_kind,scope_id) DO UPDATE SET document_mode=excluded.document_mode,${removeColumn ? `${removeColumn}=NULL,` : ""}updated_by=excluded.updated_by,updated_at=excluded.updated_at`).bind(input.scopeKind, input.scopeId, input.scopeKind === "branch" ? actor.companyId : null, result.documentMode, actor.userId, now),
    ...(previousId ? [db.prepare("UPDATE attachments SET revoked_at=? WHERE id=? AND revoked_at IS NULL").bind(now, previousId)] : []),
  ], { companyId: actor.companyId, branchId: input.scopeKind === "branch" ? input.scopeId : null, departmentId: null });
  return { result, previousId };
}
