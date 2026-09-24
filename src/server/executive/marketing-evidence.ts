import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema, type Filters } from "@/lib/executive/canvas";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { marketingExecutiveEvidenceRow } from "../marketing";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { ApiError } from "../http";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const marketingEvidenceMetrics = ["marketing-lead-candidates", "marketing-engagement-events"] as const;
type MarketingEvidenceMetric = typeof marketingEvidenceMetrics[number];
const providerId = "marketing-candidate-engagement";
const candidateFields = [
  "candidate.id", "candidate.contact_name", "candidate.form_id", "candidate.form_version", "candidate.status",
  "candidate.version", "candidate.created_at", "candidate.updated_at", "candidate.branch_id", "candidate.department_id", "marketing_assets.name",
];
const eventFields = [
  "event.id", "event.candidate_id", "event.event_type", "event.channel", "event.version", "event.created_at",
  "candidate.contact_name", "candidate.status", "candidate.version", "candidate.branch_id", "candidate.department_id", "marketing_assets.name",
];
const inputSchema = z.object({
  metric: z.enum(marketingEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();

type CandidateCursor = { metric: "marketing-lead-candidates"; query: string; filters: Filters; scope: string[]; name: string; id: string };
type EventCursor = { metric: "marketing-engagement-events"; query: string; filters: Filters; scope: string[]; createdAt: number; id: string };
type MarketingEvidenceRow = {
  id: string;
  candidate_id?: string;
  contact_name: string;
  form_name: string;
  status?: string;
  candidate_status?: string;
  event_type?: string;
  channel?: string;
  form_version?: number | string | bigint;
  version?: number | string | bigint;
  event_version?: number | string | bigint;
  candidate_version?: number | string | bigint;
  created_at: number | string | bigint;
  updated_at?: number | string | bigint;
  branch_id: string;
  department_id: string | null;
};

function fieldsFor(metric: MarketingEvidenceMetric) {
  return metric === "marketing-lead-candidates" ? candidateFields : eventFields;
}

function safeRevision(value: number | string | bigint | undefined) {
  const revision = Number(value);
  if (!Number.isSafeInteger(revision) || revision < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The Marketing source returned an invalid record revision.");
  return revision;
}

function safeTimestamp(value: number | string | bigint | undefined) {
  const timestamp = Number(value);
  if (!Number.isSafeInteger(timestamp) || timestamp < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The Marketing source returned an invalid event time.");
  return timestamp;
}

function notFound() {
  return new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}

function scopeKey(actor: WorkContext) {
  return [actor.companyId, actor.assignmentId, actor.branchId ?? "", actor.departmentId ?? ""];
}

function parseCursor(value: string | undefined, input: z.infer<typeof inputSchema>, actor: WorkContext) {
  if (!value) return null;
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
    const common = {
      metric: z.enum(marketingEvidenceMetrics).parse(parsed.metric),
      query: z.string().max(100).parse(parsed.query),
      filters: filtersSchema.parse(parsed.filters),
      scope: z.array(z.string()).max(4).parse(parsed.scope),
      id: z.string().min(1).max(200).parse(parsed.id),
    };
    if (common.metric !== input.metric || common.query !== input.query || JSON.stringify(common.filters) !== JSON.stringify(input.filters)
      || JSON.stringify(common.scope) !== JSON.stringify(scopeKey(actor))) throw new Error();
    if (input.metric === "marketing-lead-candidates" && common.metric === input.metric)
      return { ...common, metric: input.metric, name: z.string().min(1).max(160).parse(parsed.name) } satisfies CandidateCursor;
    if (input.metric === "marketing-engagement-events" && common.metric === input.metric)
      return { ...common, metric: input.metric, createdAt: z.number().int().positive().parse(parsed.createdAt) } satisfies EventCursor;
    throw new Error();
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function timeRange(filters: Filters) {
  return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}

async function requireMarketingEvidence(db: SqlDatabase, actor: WorkContext, metric: MarketingEvidenceMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "marketing"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "Marketing evidence requires current module read access.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "marketing");
}

async function writeAudit(
  db: SqlDatabase,
  actor: WorkContext,
  metric: MarketingEvidenceMetric,
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
      JSON.stringify(fieldsFor(metric)), queryHash, Date.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))))
    .run();
}

export async function searchMarketingEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requireMarketingEvidence(db, actor, input.metric);
  const cursor = parseCursor(input.cursor, input, actor), query = input.query;
  let rows: MarketingEvidenceRow[];
  if (input.metric === "marketing-lead-candidates") {
    const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
    const where = ["c.company_id=?", "(?::text IS NULL OR c.branch_id=?)", "(?::text IS NULL OR c.department_id=?)"];
    if (query) {
      where.push(`(position(lower(?) in lower(c.contact_name))>0 OR position(lower(?) in lower(f.name))>0
        OR position(lower(?) in lower(c.status))>0 OR position(lower(?) in to_char(to_timestamp(c.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
      values.push(query, query, query, query);
    }
    if (cursor) {
      const candidateCursor = cursor as CandidateCursor;
      where.push("(c.contact_name>? OR (c.contact_name=? AND c.id>?))");
      values.push(candidateCursor.name, candidateCursor.name, candidateCursor.id);
    }
    rows = (await db.prepare(`SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name
      FROM marketing_lead_candidates c JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
      WHERE ${where.join(" AND ")} ORDER BY c.contact_name,c.id LIMIT ?`)
      .bind(...values, input.limit + 1).all<MarketingEvidenceRow>()).results;
  } else {
    const range = timeRange(input.filters), values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, range.from, range.toExclusive];
    const where = ["c.company_id=?", "(?::text IS NULL OR c.branch_id=?)", "(?::text IS NULL OR c.department_id=?)", "e.created_at>=?", "e.created_at<?"];
    if (query) {
      where.push(`(position(lower(?) in lower(c.contact_name))>0 OR position(lower(?) in lower(f.name))>0
        OR position(lower(?) in lower(e.event_type))>0 OR position(lower(?) in lower(e.channel))>0
        OR position(lower(?) in to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
      values.push(query, query, query, query, query);
    }
    if (cursor) {
      const eventCursor = cursor as EventCursor;
      where.push("(e.created_at<? OR (e.created_at=? AND e.id<?))");
      values.push(eventCursor.createdAt, eventCursor.createdAt, eventCursor.id);
    }
    rows = (await db.prepare(`SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,
        c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name
      FROM marketing_engagement_events e JOIN marketing_lead_candidates c ON c.id=e.candidate_id
      JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id
      WHERE ${where.join(" AND ")} ORDER BY e.created_at DESC,e.id DESC LIMIT ?`)
      .bind(...values, input.limit + 1).all<MarketingEvidenceRow>()).results;
  }
  const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query,
    cursor: input.cursor ?? null, scope: scopeKey(actor) }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, input.metric, "search", page.map((row) => ({
    id: row.id, version: safeRevision(input.metric === "marketing-lead-candidates" ? row.version : row.event_version),
  })), queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields: fieldsFor(input.metric),
    items: page.map((row) => {
      const eventDate = new Date(safeTimestamp(row.created_at)).toISOString().slice(0, 10);
      const candidate = input.metric === "marketing-lead-candidates";
      return {
        id: row.id,
        label: candidate ? row.contact_name : `${row.event_type} · ${row.contact_name}`,
        summary: candidate ? `${row.form_name} · ${row.status} · created ${eventDate}` : `${row.form_name} · ${row.channel} · ${eventDate}`,
        status: candidate ? row.status : row.candidate_status,
        version: safeRevision(candidate ? row.version : row.event_version),
        href: `/marketing/${encodeURIComponent(candidate ? row.id : row.candidate_id!)}`,
      };
    }),
    hasMore,
    nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify(input.metric === "marketing-lead-candidates"
      ? { metric: input.metric, query, filters: input.filters, scope: scopeKey(actor), name: page.at(-1)!.contact_name, id: page.at(-1)!.id }
      : { metric: input.metric, query, filters: input.filters, scope: scopeKey(actor), createdAt: safeTimestamp(page.at(-1)!.created_at), id: page.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}

export async function readMarketingEvidence(db: SqlDatabase, actor: WorkContext, rawMetric: string, recordId: string, rawFilters: unknown) {
  const metric = z.enum(marketingEvidenceMetrics).parse(rawMetric), filters = filtersSchema.parse(rawFilters);
  await requireMarketingEvidence(db, actor, metric);
  const row = await marketingExecutiveEvidenceRow(db, actor, metric, recordId, metric === "marketing-engagement-events" ? timeRange(filters) : undefined) as MarketingEvidenceRow | null;
  if (!row) throw notFound();
  const candidate = metric === "marketing-lead-candidates", revision = safeRevision(candidate ? row.version : row.event_version);
  const record = candidate ? {
    id: row.id,
    contact_name: row.contact_name,
    form_name: row.form_name,
    form_version: safeRevision(row.form_version),
    status: row.status!,
    version: revision,
    created_at: safeTimestamp(row.created_at),
    updated_at: safeTimestamp(row.updated_at),
    branch_id: row.branch_id,
    department_id: row.department_id,
    href: `/marketing/${encodeURIComponent(row.id)}`,
  } : {
    id: row.id,
    candidate_id: row.candidate_id!,
    contact_name: row.contact_name,
    form_name: row.form_name,
    candidate_status: row.candidate_status!,
    event_type: row.event_type!,
    channel: row.channel!,
    created_at: safeTimestamp(row.created_at),
    version: revision,
    candidate_version: safeRevision(row.candidate_version),
    href: `/marketing/${encodeURIComponent(row.candidate_id!)}`,
  };
  const queryHash = await sha256(JSON.stringify({ providerId, metric, filters, scope: scopeKey(actor), recordId }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, metric, "open", [{ id: row.id, version: revision }], queryHash);
  await currentAuthority(db, actor);
  return { provider: providerId, metric, fields: fieldsFor(metric), record, queryHash };
}
