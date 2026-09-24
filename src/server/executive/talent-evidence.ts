import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema, type Filters } from "@/lib/executive/canvas";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { talentExecutiveEvidenceRow } from "../talent";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { ApiError } from "../http";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const talentEvidenceMetrics = ["open-requisitions", "application-stage-counts"] as const;
type TalentEvidenceMetric = typeof talentEvidenceMetrics[number];
const providerId = "talent-recruiting-records";
const requisitionFields = [
  "requisition.id", "requisition.job_title", "requisition.target_start_date", "requisition.status",
  "requisition.version", "requisition.updated_at", "requisition.branch_id", "requisition.department_id",
];
const applicationFields = [
  "application.id", "application.status", "application.version", "application.updated_at",
  "requisition.id", "requisition.job_title", "requisition.branch_id", "requisition.department_id",
];
const inputSchema = z.object({
  metric: z.enum(talentEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();

type RequisitionCursor = { jobTitle: string; id: string };
type ApplicationCursor = { updatedAt: number; id: string };
type TalentEvidenceRow = {
  id: string;
  requisition_id?: string;
  job_title: string;
  target_start_date?: string;
  status: string;
  version: number | string | bigint;
  updated_at: number | string | bigint;
  branch_id: string;
  department_id: string | null;
};

function parseCursor(value: string | undefined, metric: TalentEvidenceMetric) {
  if (!value) return null;
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
    const id = z.string().min(1).max(200).parse(parsed.id);
    return metric === "open-requisitions"
      ? { jobTitle: z.string().min(1).max(160).parse(parsed.jobTitle), id } satisfies RequisitionCursor
      : { updatedAt: z.number().int().positive().parse(parsed.updatedAt), id } satisfies ApplicationCursor;
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function safeVersion(value: number | string | bigint) {
  const version = Number(value);
  if (!Number.isSafeInteger(version) || version < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid Talent revision.");
  return version;
}

function safeTimestamp(value: number | string | bigint) {
  const timestamp = Number(value);
  if (!Number.isSafeInteger(timestamp) || timestamp < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid Talent timestamp.");
  return timestamp;
}

function utcRange(filters: Filters) {
  return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}

function notFound() {
  return new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}

async function requireTalentEvidence(db: SqlDatabase, actor: WorkContext, metric: TalentEvidenceMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read") || !hasPermission(actor, "records.review")
    || !hasModuleAccess(actor.modules, "talent"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "Talent evidence requires current reviewer access.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "talent");
}

async function writeAudit(
  db: SqlDatabase,
  actor: WorkContext,
  metric: TalentEvidenceMetric,
  action: "search" | "open",
  rows: { id: string; version: number }[],
  queryHash: string,
) {
  if (!rows.length) return;
  const fields = metric === "open-requisitions" ? requisitionFields : applicationFields;
  await db.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
    .bind(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action,
      JSON.stringify(fields), queryHash, Date.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))))
    .run();
}

export async function searchTalentEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requireTalentEvidence(db, actor, input.metric);
  const cursor = parseCursor(input.cursor, input.metric);
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  const where = [
    "r.company_id=?", "(?::text IS NULL OR r.branch_id=?)", "(?::text IS NULL OR r.department_id=?)",
  ];
  let rows: TalentEvidenceRow[];

  if (input.metric === "open-requisitions") {
    where.push("r.status='open'");
    if (input.query) {
      where.push(`(position(lower(?) in lower(r.job_title))>0 OR position(lower(?) in lower(r.status))>0
        OR position(lower(?) in r.target_start_date)>0)`);
      values.push(input.query, input.query, input.query);
    }
    if (cursor) {
      const requisitionCursor = cursor as RequisitionCursor;
      where.push("(r.job_title>? OR (r.job_title=? AND r.id>?))");
      values.push(requisitionCursor.jobTitle, requisitionCursor.jobTitle, requisitionCursor.id);
    }
    rows = (await db.prepare(`SELECT r.id,r.job_title,r.target_start_date,r.status,r.version,r.updated_at,r.branch_id,r.department_id
      FROM talent_requisitions r WHERE ${where.join(" AND ")}
      ORDER BY r.job_title,r.id LIMIT ?`)
      .bind(...values, input.limit + 1).all<TalentEvidenceRow>()).results;
  } else {
    const range = utcRange(input.filters);
    where.push("a.updated_at>=?", "a.updated_at<?");
    values.push(range.from, range.toExclusive);
    if (input.query) {
      where.push(`(position(lower(?) in lower(r.job_title))>0 OR position(lower(?) in lower(a.status))>0
        OR position(lower(?) in to_char(to_timestamp(a.updated_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
      values.push(input.query, input.query, input.query);
    }
    if (cursor) {
      const applicationCursor = cursor as ApplicationCursor;
      where.push("(a.updated_at<? OR (a.updated_at=? AND a.id<?))");
      values.push(applicationCursor.updatedAt, applicationCursor.updatedAt, applicationCursor.id);
    }
    rows = (await db.prepare(`SELECT a.id,a.requisition_id,a.status,a.version,a.updated_at,r.job_title,r.branch_id,r.department_id
      FROM talent_applications a JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE ${where.join(" AND ")}
      ORDER BY a.updated_at DESC,a.id DESC LIMIT ?`)
      .bind(...values, input.limit + 1).all<TalentEvidenceRow>()).results;
  }

  const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({
    providerId, metric: input.metric, filters: input.filters, query: input.query,
    cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, input.metric, "search", page.map((row) => ({ id: row.id, version: safeVersion(row.version) })), queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields: input.metric === "open-requisitions" ? requisitionFields : applicationFields,
    items: page.map((row) => ({
      id: row.id,
      label: row.job_title,
      summary: input.metric === "open-requisitions"
        ? `Open · start ${row.target_start_date}`
        : `${row.status.replaceAll("_", " ")} · updated ${new Date(safeTimestamp(row.updated_at)).toISOString().slice(0, 10)}`,
      status: row.status,
      version: safeVersion(row.version),
      href: `/talent/${encodeURIComponent(row.id)}`,
    })),
    hasMore,
    nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify(input.metric === "open-requisitions"
      ? { jobTitle: page.at(-1)!.job_title, id: page.at(-1)!.id }
      : { updatedAt: safeTimestamp(page.at(-1)!.updated_at), id: page.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}

export async function readTalentEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  metric: string,
  recordId: string,
  rawFilters: unknown,
) {
  const parsedMetric = z.enum(talentEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
  await requireTalentEvidence(db, actor, parsedMetric);
  const range = parsedMetric === "application-stage-counts" ? utcRange(filters) : undefined;
  const row = await talentExecutiveEvidenceRow(db, actor, parsedMetric, recordId, range) as TalentEvidenceRow | null;
  if (!row) throw notFound();
  const version = safeVersion(row.version), fields = parsedMetric === "open-requisitions" ? requisitionFields : applicationFields;
  const record = parsedMetric === "open-requisitions" ? {
    id: row.id,
    job_title: row.job_title,
    target_start_date: row.target_start_date!,
    status: row.status,
    version,
    updated_at: safeTimestamp(row.updated_at),
    branch_id: row.branch_id,
    department_id: row.department_id,
    href: `/talent/${encodeURIComponent(row.id)}`,
  } : {
    id: row.id,
    requisition_id: row.requisition_id!,
    job_title: row.job_title,
    status: row.status,
    version,
    updated_at: safeTimestamp(row.updated_at),
    branch_id: row.branch_id,
    department_id: row.department_id,
    href: `/talent/${encodeURIComponent(row.id)}`,
  };
  const queryHash = await sha256(JSON.stringify({
    providerId, metric: parsedMetric, filters,
    scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId,
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, parsedMetric, "open", [{ id: row.id, version }], queryHash);
  await currentAuthority(db, actor);
  return { provider: providerId, metric: parsedMetric, fields, record, queryHash };
}
