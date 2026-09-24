import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema, type Filters } from "@/lib/executive/canvas";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { brandProfileEvidenceRow } from "../branding";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { companyAuthority } from "../staff-policy";
import { ApiError } from "../http";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const brandEvidenceMetrics = ["brand-profiles", "brand-profile-updates"] as const;
type BrandEvidenceMetric = typeof brandEvidenceMetrics[number];
const providerId = "brand-profile-records";
const fields = [
  "profile.scope_id", "branch.display_name", "profile.document_mode", "profile.updated_at",
  "profile.primary_attachment_id", "profile.inverse_attachment_id",
];
const inputSchema = z.object({
  metric: z.enum(brandEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();

type BrandProfileRow = {
  id: string;
  branch_name: string;
  document_mode: "holding" | "branch" | "both";
  updated_at: number | string | bigint;
  primary_attachment_id: string | null;
  inverse_attachment_id: string | null;
};
type Cursor = { branchName: string; id: string };

function parseCursor(value?: string): Cursor | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as { branchName?: unknown; id?: unknown };
    return {
      branchName: z.string().min(1).max(200).parse(parsed.branchName),
      id: z.string().min(1).max(200).parse(parsed.id),
    };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeCursor(row: Pick<BrandProfileRow, "branch_name" | "id">) {
  return Buffer.from(JSON.stringify({ branchName: row.branch_name, id: row.id })).toString("base64url");
}

function safeRevision(value: number | string | bigint) {
  const revision = Number(value);
  if (!Number.isSafeInteger(revision) || revision < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid profile revision.");
  return revision;
}

function notFound() {
  return new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}

async function requireBrandEvidence(db: SqlDatabase, actor: WorkContext, metric: BrandEvidenceMetric) {
  await currentAuthority(db, actor);
  const catalog = await metricCatalogFor(db, actor);
  if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "brand")
    || actor.branchId || actor.departmentId || !await companyAuthority(db, actor)
    || !catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "brand");
}

async function writeAudit(
  db: SqlDatabase,
  actor: WorkContext,
  metric: BrandEvidenceMetric,
  action: "search" | "open",
  rows: { id: string; version: number }[],
  queryHash: string,
) {
  if (!rows.length) return;
  await db.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
    .bind(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action,
      JSON.stringify(fields), queryHash, Date.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))))
    .run();
}

function timeRange(filters: Filters) {
  return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}

export async function searchBrandEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requireBrandEvidence(db, actor, input.metric);
  const cursor = parseCursor(input.cursor), conditions = [
    "p.scope_kind='branch'", "p.company_id=?", "b.kind='branch'", "b.status='active'",
    "(?::text IS NULL OR p.scope_id=?)",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
  if (input.metric === "brand-profile-updates") {
    const range = timeRange(input.filters);
    conditions.push("p.updated_at>=?", "p.updated_at<?");
    values.push(range.from, range.toExclusive);
  }
  if (input.query) {
    conditions.push(`(position(lower(?) in lower(b.display_name))>0
      OR position(lower(?) in lower(p.document_mode))>0
      OR position(lower(?) in to_char(to_timestamp(p.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
    values.push(input.query, input.query, input.query);
  }
  if (cursor) {
    conditions.push("(b.display_name>? OR (b.display_name=? AND p.scope_id>?))");
    values.push(cursor.branchName, cursor.branchName, cursor.id);
  }
  const rows = (await db.prepare(`SELECT p.scope_id id,b.display_name branch_name,p.document_mode,p.updated_at,
      p.primary_attachment_id,p.inverse_attachment_id
    FROM organization_brand_profiles p
    JOIN organization_nodes b ON b.id=p.scope_id AND b.company_id=p.company_id
    WHERE ${conditions.join(" AND ")} ORDER BY b.display_name,p.scope_id LIMIT ?`)
    .bind(...values, input.limit + 1).all<BrandProfileRow>()).results;
  const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({
    providerId, metric: input.metric, filters: input.filters, query: input.query,
    cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId],
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, input.metric, "search", page.map((row) => ({ id: row.id, version: safeRevision(row.updated_at) })), queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields,
    items: page.map((row) => ({
      id: row.id,
      label: row.branch_name,
      summary: `${row.document_mode} documents · updated ${new Date(safeRevision(row.updated_at)).toISOString().slice(0, 10)}`,
      status: row.document_mode,
      version: safeRevision(row.updated_at),
      href: "/settings?section=organization",
    })),
    hasMore,
    nextCursor: hasMore && page.length ? encodeCursor(page.at(-1)!) : null,
    queryHash,
  };
}

export async function readBrandEvidence(db: SqlDatabase, actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) {
  const parsedMetric = z.enum(brandEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
  await requireBrandEvidence(db, actor, parsedMetric);
  const id = z.string().min(1).max(200).parse(recordId);
  const row = await brandProfileEvidenceRow(db, actor, id) as BrandProfileRow | null;
  if (!row) throw notFound();
  const revision = safeRevision(row.updated_at);
  if (parsedMetric === "brand-profile-updates") {
    const range = timeRange(filters);
    if (revision < range.from || revision >= range.toExclusive) throw notFound();
  }
  const record = {
    id: row.id,
    branch_name: row.branch_name,
    document_mode: row.document_mode,
    updated_at: revision,
    primary_configured: row.primary_attachment_id !== null,
    inverse_configured: row.inverse_attachment_id !== null,
    version: revision,
    href: "/settings?section=organization",
  };
  const queryHash = await sha256(JSON.stringify({
    providerId, metric: parsedMetric, filters,
    scope: [actor.companyId, actor.assignmentId], recordId: id,
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, parsedMetric, "open", [{ id, version: revision }], queryHash);
  await currentAuthority(db, actor);
  return { provider: providerId, metric: parsedMetric, fields, record, queryHash };
}
