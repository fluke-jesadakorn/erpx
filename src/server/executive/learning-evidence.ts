import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema, type Filters } from "@/lib/executive/canvas";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { learningExecutiveEvidenceRow } from "../learning";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { ApiError } from "../http";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const learningEvidenceMetrics = ["learning-completions", "overdue-required-learning"] as const;
type LearningEvidenceMetric = typeof learningEvidenceMetrics[number];
const providerId = "learning-assignment-records";
const completionFields = [
  "event.id", "event.assignment_id", "event.version", "event.action", "event.status", "event.created_at", "course.title",
];
const overdueFields = [
  "assignment.id", "assignment.version", "assignment.status", "assignment.due_date", "assignment.updated_at", "course.title", "course.required_for",
];
const inputSchema = z.object({
  metric: z.enum(learningEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();

type CompletionCursor = { createdAt: number; id: string };
type OverdueCursor = { dueDate: string; id: string };
type LearningEvidenceRow = {
  id: string;
  assignment_id?: string;
  event_version?: number | string | bigint;
  version?: number | string | bigint;
  status: string;
  created_at?: number | string | bigint;
  due_date?: string;
  updated_at?: number | string | bigint;
  course_title: string;
  required_for?: string | null;
};

function parseCursor(value: string | undefined, metric: LearningEvidenceMetric) {
  if (!value) return null;
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
    const id = z.string().min(1).max(200).parse(parsed.id);
    if (metric === "learning-completions") return {
      createdAt: z.number().int().positive().parse(parsed.createdAt), id,
    } satisfies CompletionCursor;
    return { dueDate: z.iso.date().parse(parsed.dueDate), id } satisfies OverdueCursor;
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeCursor(metric: LearningEvidenceMetric, row: LearningEvidenceRow) {
  const cursor = metric === "learning-completions"
    ? { createdAt: safeTimestamp(row.created_at), id: row.id }
    : { dueDate: z.iso.date().parse(row.due_date), id: row.id };
  return Buffer.from(JSON.stringify(cursor)).toString("base64url");
}

function safeRevision(value: number | string | bigint | undefined) {
  const revision = Number(value);
  if (!Number.isSafeInteger(revision) || revision < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid learning revision.");
  return revision;
}

function safeTimestamp(value: number | string | bigint | undefined) {
  const timestamp = Number(value);
  if (!Number.isSafeInteger(timestamp) || timestamp < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid learning timestamp.");
  return timestamp;
}

function notFound() {
  return new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}

function utcRange(filters: Filters) {
  return { from: Date.parse(filters.from), toExclusive: Date.parse(filters.to) + 86400000 };
}

async function requireLearningEvidence(db: SqlDatabase, actor: WorkContext, metric: LearningEvidenceMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "learning"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "learning");
}

async function writeAudit(
  db: SqlDatabase,
  actor: WorkContext,
  metric: LearningEvidenceMetric,
  action: "search" | "open",
  rows: { id: string; version: number }[],
  queryHash: string,
) {
  if (!rows.length) return;
  const fields = metric === "learning-completions" ? completionFields : overdueFields;
  await db.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
    .bind(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action,
      JSON.stringify(fields), queryHash, Date.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version }))))
    .run();
}

export async function searchLearningEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requireLearningEvidence(db, actor, input.metric);
  const cursor = parseCursor(input.cursor, input.metric);
  const scope = [
    "a.company_id=?", "(?::text IS NULL OR a.branch_id=?)", "(?::text IS NULL OR a.department_id=?)",
  ];
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  let rows: LearningEvidenceRow[];

  if (input.metric === "learning-completions") {
    const range = utcRange(input.filters);
    scope.push("e.action='complete'", "e.status='completed'", "e.created_at>=?", "e.created_at<?");
    values.push(range.from, range.toExclusive);
    if (input.query) {
      scope.push(`(position(lower(?) in lower(c.title))>0 OR position(lower(?) in to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD'))>0)`);
      values.push(input.query, input.query);
    }
    if (cursor) {
      const completionCursor = cursor as CompletionCursor;
      scope.push("(e.created_at<? OR (e.created_at=? AND e.id<?))");
      values.push(completionCursor.createdAt, completionCursor.createdAt, completionCursor.id);
    }
    rows = (await db.prepare(`SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title
      FROM learning_assignment_events e
      JOIN learning_assignments a ON a.id=e.assignment_id
      JOIN learning_courses c ON c.id=a.course_id
      WHERE ${scope.join(" AND ")}
      ORDER BY e.created_at DESC,e.id DESC LIMIT ?`)
      .bind(...values, input.limit + 1).all<LearningEvidenceRow>()).results;
  } else {
    scope.push("c.required_for IS NOT NULL", "a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')", "a.status IN ('assigned','in_progress')");
    if (input.query) {
      scope.push(`(position(lower(?) in lower(c.title))>0 OR position(lower(?) in lower(a.status))>0
        OR position(lower(?) in a.due_date)>0 OR position(lower(?) in lower(c.required_for))>0)`);
      values.push(input.query, input.query, input.query, input.query);
    }
    if (cursor) {
      const overdueCursor = cursor as OverdueCursor;
      scope.push("(a.due_date>? OR (a.due_date=? AND a.id>?))");
      values.push(overdueCursor.dueDate, overdueCursor.dueDate, overdueCursor.id);
    }
    rows = (await db.prepare(`SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for
      FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id
      WHERE ${scope.join(" AND ")}
      ORDER BY a.due_date,a.id LIMIT ?`)
      .bind(...values, input.limit + 1).all<LearningEvidenceRow>()).results;
  }

  const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({
    providerId, metric: input.metric, filters: input.filters, query: input.query,
    cursor: input.cursor ?? null, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, input.metric, "search", page.map((row) => ({
    id: row.id, version: safeRevision(input.metric === "learning-completions" ? row.event_version : row.version),
  })), queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields: input.metric === "learning-completions" ? completionFields : overdueFields,
    items: page.map((row) => {
      const completion = input.metric === "learning-completions";
      const updatedAt = completion ? safeTimestamp(row.created_at) : safeTimestamp(row.updated_at);
      const assignmentId = completion ? row.assignment_id! : row.id;
      return {
        id: row.id,
        label: row.course_title,
        summary: completion
          ? `Completed · ${new Date(updatedAt).toISOString().slice(0, 10)}`
          : `${row.status.replaceAll("_", " ")} · due ${row.due_date}`,
        status: completion ? "completed" : row.status,
        version: safeRevision(completion ? row.event_version : row.version),
        href: `/learning/${encodeURIComponent(assignmentId)}`,
      };
    }),
    hasMore,
    nextCursor: hasMore && page.length ? encodeCursor(input.metric, page.at(-1)!) : null,
    queryHash,
  };
}

export async function readLearningEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  metric: string,
  recordId: string,
  rawFilters: unknown,
) {
  const parsedMetric = z.enum(learningEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
  await requireLearningEvidence(db, actor, parsedMetric);
  const range = parsedMetric === "learning-completions" ? utcRange(filters) : undefined;
  const row = await learningExecutiveEvidenceRow(db, actor, parsedMetric, recordId, range) as LearningEvidenceRow | null;
  if (!row) throw notFound();
  const completion = parsedMetric === "learning-completions";
  const version = safeRevision(completion ? row.event_version : row.version);
  const record = {
    id: row.id,
    assignment_id: completion ? row.assignment_id! : row.id,
    course_title: row.course_title,
    status: row.status,
    created_at: completion ? safeTimestamp(row.created_at) : null,
    due_date: completion ? null : row.due_date!,
    required_for: completion ? null : row.required_for!,
    version,
    href: `/learning/${encodeURIComponent(completion ? row.assignment_id! : row.id)}`,
  };
  const queryHash = await sha256(JSON.stringify({
    providerId, metric: parsedMetric, filters,
    scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId,
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, parsedMetric, "open", [{ id: row.id, version }], queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: parsedMetric,
    fields: completion ? completionFields : overdueFields,
    record,
    queryHash,
  };
}
