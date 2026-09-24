import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/permissions";
import { filtersSchema, type MetricId } from "@/lib/executive/canvas";
import type { WorkContext } from "../authorization";
import { ApiError } from "../http";
import type { SqlDatabase } from "../runtime/types";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const agentEvidenceMetrics = ["approved-agent-versions", "agent-invocation-outcomes"] as const;
type AgentEvidenceMetric = typeof agentEvidenceMetrics[number];
const providerId = "agent-governance";
const approvedVersionMetric = "approved-agent-versions" as const;
const approvedVersionFields = ["agent.name", "agent.status", "version.version_number", "version.status", "version.version", "version.created_at", "version.updated_at"];
const invocationFields = ["turn_id", "outcome", "created_at"];
const inputSchema = z.object({
  metric: z.enum(agentEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();

type Cursor = { createdAt: number; id: string };
type ApprovedVersionRow = {
  id: string; agent_id: string; agent_name: string; agent_status: string; version_status: string;
  version_number: number | string; version: number | string; created_at: number | string; updated_at: number | string;
};
type InvocationRow = { id: string; turn_id: string; outcome: "succeeded" | "failed"; created_at: number | string };

function parseCursor(value?: string): Cursor | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as { createdAt?: unknown; id?: unknown };
    if (!Number.isSafeInteger(parsed.createdAt) || typeof parsed.id !== "string") throw new Error();
    return { createdAt: Number(parsed.createdAt), id: z.string().min(1).max(200).parse(parsed.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeCursor(row: { created_at: number | string; id: string }) {
  return Buffer.from(JSON.stringify({ createdAt: Number(row.created_at), id: row.id })).toString("base64url");
}

function exactNumber(value: number | string, field: string) {
  const result = Number(value);
  if (!Number.isSafeInteger(result) || result < 0) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return result;
}

async function requireAgentEvidence(db: SqlDatabase, actor: WorkContext, metric: AgentEvidenceMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "agent"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "agent");
}

async function writeAudit(db: SqlDatabase, actor: WorkContext, metric: AgentEvidenceMetric, action: "search" | "open", records: { id: string; version: number | null }[], fields: string[], queryHash: string) {
  if (!records.length && action === "search") return;
  const rows = records.length ? records : [{ id: "", version: 0 }];
  await db.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,NULLIF(item.record_id,''),NULLIF(item.record_version,0),?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
    .bind(actor.companyId, actor.userId, actor.assignmentId, providerId, metric, action, JSON.stringify(fields), queryHash, Date.now(), JSON.stringify(rows.map((row) => ({ record_id: row.id, record_version: row.version })))).run();
}

export async function searchAgentEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requireAgentEvidence(db, actor, input.metric);
  const cursor = parseCursor(input.cursor), conditions: string[] = ["a.company_id=?", "a.status='approved'", "v.status='approved'"], values: unknown[] = [actor.companyId];
  let rows: ApprovedVersionRow[] | InvocationRow[];
  let fields: string[];
  if (input.metric === approvedVersionMetric) {
    if (input.query) {
      conditions.push("(position(lower(?) in lower(a.name))>0 OR position(lower(?) in lower(v.version_number::text))>0)");
      values.push(input.query, input.query);
    }
    if (cursor) {
      conditions.push("(v.created_at<? OR (v.created_at=? AND v.id<?))");
      values.push(cursor.createdAt, cursor.createdAt, cursor.id);
    }
    rows = (await db.prepare(`SELECT v.id,v.agent_id,a.name agent_name,a.status agent_status,v.status version_status,
        v.version_number,v.version,v.created_at,v.updated_at
      FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id
      WHERE ${conditions.join(" AND ")} ORDER BY v.created_at DESC,v.id DESC LIMIT ?`)
      .bind(...values, input.limit + 1).all<ApprovedVersionRow>()).results;
    fields = approvedVersionFields;
  } else {
    const invocationConditions = ["e.company_id=?", "e.created_at>=?", "e.created_at<?"];
    const invocationValues: unknown[] = [actor.companyId, Date.parse(input.filters.from), Date.parse(input.filters.to) + 86400000];
    if (input.query) {
      invocationConditions.push("(position(lower(?) in lower(e.outcome))>0 OR position(lower(?) in lower(e.turn_id))>0)");
      invocationValues.push(input.query, input.query);
    }
    if (cursor) {
      invocationConditions.push("(e.created_at<? OR (e.created_at=? AND e.id<?))");
      invocationValues.push(cursor.createdAt, cursor.createdAt, cursor.id);
    }
    rows = (await db.prepare(`SELECT e.id,e.turn_id,e.outcome,e.created_at FROM ai_invocation_evidence e
      WHERE ${invocationConditions.join(" AND ")} ORDER BY e.created_at DESC,e.id DESC LIMIT ?`)
      .bind(...invocationValues, input.limit + 1).all<InvocationRow>()).results;
    fields = invocationFields;
  }
  const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit), queryHash = await sha256(JSON.stringify({
    providerId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null,
    scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, input.metric, "search", page.map((row) => ({ id: row.id, version: input.metric === approvedVersionMetric ? exactNumber((row as ApprovedVersionRow).version, "agent version") : 1 })), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields,
    items: input.metric === approvedVersionMetric
      ? (page as ApprovedVersionRow[]).map((row) => ({
        id: row.id,
        label: `${row.agent_name} · Version ${exactNumber(row.version_number, "approved version number")}`,
        summary: `Approved agent · Revision ${exactNumber(row.version, "agent version")}`,
        version: exactNumber(row.version, "agent version"),
        href: "/agents",
      }))
      : (page as InvocationRow[]).map((row) => ({
        id: row.id,
        label: `${row.outcome === "succeeded" ? "Succeeded" : "Failed"} invocation`,
        summary: `${row.turn_id} · ${new Date(exactNumber(row.created_at, "invocation time")).toISOString()}`,
        version: 1,
        href: "/agents",
      })),
    hasMore,
    nextCursor: hasMore && page.length ? encodeCursor(page.at(-1)!) : null,
    queryHash,
  };
}

export async function readAgentEvidence(db: SqlDatabase, actor: WorkContext, metric: MetricId, recordId: string, rawFilters: unknown) {
  const parsedMetric = z.enum(agentEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
  await requireAgentEvidence(db, actor, parsedMetric);
  const id = z.string().min(1).max(200).parse(recordId);
  let fields: string[];
  let record: Record<string, string | number> & { id: string; href: string };
  let version: number | null;
  if (parsedMetric === approvedVersionMetric) {
    const row = await db.prepare(`SELECT v.id,v.agent_id,a.name agent_name,a.status agent_status,v.status version_status,
        v.version_number,v.version,v.created_at,v.updated_at
      FROM agent_versions v JOIN agent_catalog a ON a.id=v.agent_id
      WHERE a.company_id=? AND a.status='approved' AND v.status='approved' AND v.id=? LIMIT 1`)
      .bind(actor.companyId, id).first<ApprovedVersionRow>();
    if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    fields = approvedVersionFields;
    version = exactNumber(row.version, "agent version");
    record = {
      id: row.id, agent_id: row.agent_id, agent_name: row.agent_name, agent_status: row.agent_status,
      version_status: row.version_status, version_number: exactNumber(row.version_number, "approved version number"),
      version, created_at: exactNumber(row.created_at, "version creation time"), updated_at: exactNumber(row.updated_at, "version update time"), href: "/agents",
    };
  } else {
    const row = await db.prepare(`SELECT e.id,e.turn_id,e.outcome,e.created_at FROM ai_invocation_evidence e
      WHERE e.company_id=? AND e.created_at>=? AND e.created_at<? AND e.id=? LIMIT 1`)
      .bind(actor.companyId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, id).first<InvocationRow>();
    if (!row) throw new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
    fields = invocationFields;
    version = 1;
    record = { id: row.id, turn_id: row.turn_id, outcome: row.outcome, created_at: exactNumber(row.created_at, "invocation time"), href: "/agents" };
  }
  const queryHash = await sha256(JSON.stringify({ providerId, metric: parsedMetric, filters, scope: [actor.companyId, actor.assignmentId], recordId: id }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, parsedMetric, "open", [{ id, version }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: providerId, metric: parsedMetric, fields, record, queryHash };
}
