import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema, type Filters } from "@/lib/executive/canvas";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { planningExecutiveEvidenceRow, type PlanningExecutiveEvidenceMetric } from "../production";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { ApiError } from "../http";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const planningEvidenceMetrics = [
  "planning-shortage-scenarios",
  "proposed-planning-recommendations",
  "capacity-feasible-recommendations",
] as const;
type PlanningEvidenceMetric = typeof planningEvidenceMetrics[number];
const providerId = "planning-scenarios-recommendations";
const scenarioFields = [
  "planning_scenarios.id", "planning_scenarios.company_id", "planning_scenarios.branch_id", "planning_scenarios.warehouse_id",
  "planning_scenarios.specification_id", "planning_scenarios.specification_version", "planning_scenarios.demand_quantity_milli",
  "planning_scenarios.due_date", "planning_scenarios.capacity_minutes", "planning_scenarios.shortages_json", "planning_scenarios.status",
  "planning_scenarios.version", "planning_scenarios.created_at", "planning_scenarios.updated_at", "manufacturing_specifications.name",
  "commerce_products.name", "commerce_warehouses.name", "organization_nodes.display_name", "planning_recommendations.id",
  "planning_recommendations.status", "planning_recommendations.required_minutes", "planning_recommendations.feasible_date",
  "planning_recommendations.version",
];
const recommendationFields = [
  "planning_recommendations.id", "planning_recommendations.scenario_id", "planning_recommendations.status",
  "planning_recommendations.recommended_quantity_milli", "planning_recommendations.required_minutes", "planning_recommendations.feasible_date",
  "planning_recommendations.version", "planning_recommendations.scenario_version", "planning_recommendations.created_at",
  "planning_recommendations.updated_at", "planning_scenarios.id", "planning_scenarios.branch_id", "planning_scenarios.warehouse_id",
  "planning_scenarios.specification_id", "planning_scenarios.specification_version", "planning_scenarios.demand_quantity_milli",
  "planning_scenarios.due_date", "planning_scenarios.capacity_minutes", "planning_scenarios.shortages_json", "planning_scenarios.status",
  "planning_scenarios.version", "manufacturing_specifications.name", "commerce_products.name", "commerce_warehouses.name",
  "organization_nodes.display_name",
];
const inputSchema = z.object({
  metric: z.enum(planningEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();
type Cursor = { metric: PlanningEvidenceMetric; filters: Filters; query: string; scope: string[]; id: string };
type EvidenceRow = {
  id: string;
  scenario_id: string;
  recommendation_id: string | null;
  branch_id: string;
  branch_name: string | null;
  warehouse_id: string;
  warehouse_name: string | null;
  specification_id: string;
  specification_name: string | null;
  output_product_id: string | null;
  output_product_name: string | null;
  specification_version: string | number | bigint;
  demand_quantity_milli: string | number | bigint;
  due_date: string;
  capacity_minutes: string | number | bigint;
  scenario_status: string;
  scenario_version: string | number | bigint;
  scenario_created_at: string | number | bigint;
  scenario_updated_at: string | number | bigint;
  recommendation_status: string | null;
  recommended_quantity_milli: string | number | bigint | null;
  required_minutes: string | number | bigint | null;
  feasible_date: string | null;
  recommendation_version: string | number | bigint | null;
  scenario_version_ref: string | number | bigint | null;
  recommendation_created_at: string | number | bigint | null;
  recommendation_updated_at: string | number | bigint | null;
  shortage_count: string | number | bigint;
  shortage_items_json: string;
};

function fieldsFor(metric: PlanningEvidenceMetric) {
  return metric === "planning-shortage-scenarios" ? scenarioFields : recommendationFields;
}

function scopeKey(actor: WorkContext) {
  return [actor.companyId, actor.assignmentId, actor.branchId ?? "", actor.departmentId ?? ""];
}

function parseCursor(value: string | undefined, input: z.infer<typeof inputSchema>, actor: WorkContext): Cursor | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as Record<string, unknown>;
    const cursor = {
      metric: z.enum(planningEvidenceMetrics).parse(parsed.metric),
      filters: filtersSchema.parse(parsed.filters),
      query: z.string().max(100).parse(parsed.query),
      scope: z.array(z.string()).max(4).parse(parsed.scope),
      id: z.string().min(1).max(200).parse(parsed.id),
    };
    if (cursor.metric !== input.metric || JSON.stringify(cursor.filters) !== JSON.stringify(input.filters)
      || cursor.query !== input.query || JSON.stringify(cursor.scope) !== JSON.stringify(scopeKey(actor))) throw new Error();
    return cursor;
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function safeRevision(value: string | number | bigint | null | undefined) {
  const revision = Number(value);
  if (!Number.isSafeInteger(revision) || revision < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned an invalid record revision.");
  return revision;
}

function safeWholeText(value: string | number | bigint | null | undefined, name: string) {
  const text = String(value);
  if (!/^(0|[1-9][0-9]*)$/.test(text))
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The Planning source returned an invalid ${name}.`);
  return text;
}

function safeTimestamp(value: string | number | bigint | null | undefined) {
  const timestamp = Number(value);
  if (!Number.isSafeInteger(timestamp) || timestamp < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned an invalid record time.");
  return timestamp;
}

function safeDate(value: string | null | undefined) {
  if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value) || !Number.isFinite(Date.parse(`${value}T00:00:00Z`)))
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned an invalid date.");
  return value;
}

function shortages(value: string) {
  const schema = z.array(z.object({
    product_name: z.string().min(1).max(160),
    required_milli: z.string().regex(/^(0|[1-9][0-9]*)$/),
    available_milli: z.string().regex(/^(0|[1-9][0-9]*)$/),
    shortage_milli: z.string().regex(/^[1-9][0-9]*$/),
  }).strict()).max(100);
  try {
    return schema.parse(JSON.parse(value));
  } catch {
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The Planning source returned invalid shortage evidence.");
  }
}

function notFound() {
  return new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}

async function requirePlanningEvidence(db: SqlDatabase, actor: WorkContext, metric: PlanningEvidenceMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "records.read") || !hasModuleAccess(actor.modules, "planning"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "Planning evidence requires current module read access.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "planning");
}

async function writeAudit(
  db: SqlDatabase,
  actor: WorkContext,
  metric: PlanningEvidenceMetric,
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

const sourceFrom = `FROM planning_scenarios s
  LEFT JOIN manufacturing_specifications spec ON spec.id=s.specification_id AND spec.company_id=s.company_id
  LEFT JOIN commerce_products output ON output.id=spec.output_product_id AND output.company_id=s.company_id
  LEFT JOIN commerce_warehouses w ON w.id=s.warehouse_id AND w.company_id=s.company_id
  LEFT JOIN organization_nodes branch ON branch.id=s.branch_id AND branch.company_id=s.company_id
  LEFT JOIN planning_recommendations r ON r.scenario_id=s.id
  LEFT JOIN LATERAL (
    SELECT count(*) FILTER (WHERE (entry.value->>'shortageMilli')::numeric>0)::text shortage_count
    FROM jsonb_array_elements(COALESCE(s.shortages_json::jsonb,'[]'::jsonb)) AS entry(value)
  ) shortage ON TRUE`;
const commonSelect = `s.id scenario_id,s.branch_id,branch.display_name branch_name,s.warehouse_id,w.name warehouse_name,
  s.specification_id,spec.name specification_name,spec.output_product_id,output.name output_product_name,s.specification_version,
  s.demand_quantity_milli::text demand_quantity_milli,s.due_date,s.capacity_minutes::text capacity_minutes,
  s.status scenario_status,s.version scenario_version,s.created_at scenario_created_at,s.updated_at scenario_updated_at,
  r.id recommendation_id,r.status recommendation_status,r.recommended_quantity_milli::text recommended_quantity_milli,
  r.required_minutes::text required_minutes,r.feasible_date,r.version recommendation_version,r.scenario_version scenario_version_ref,
  r.created_at recommendation_created_at,r.updated_at recommendation_updated_at,COALESCE(shortage.shortage_count,'0') shortage_count`;

export async function searchPlanningEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requirePlanningEvidence(db, actor, input.metric);
  const cursor = parseCursor(input.cursor, input, actor), isScenario = input.metric === "planning-shortage-scenarios";
  const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
  const where = ["s.company_id=?", "(?::text IS NULL OR s.branch_id=?)"];
  if (isScenario) {
    where.push("s.due_date BETWEEN ? AND ?", "jsonb_path_exists(COALESCE(s.shortages_json::jsonb,'[]'::jsonb),'$[*] ? (@.shortageMilli > 0)')");
  } else {
    where.push("r.feasible_date BETWEEN ? AND ?", input.metric === "proposed-planning-recommendations"
      ? "r.status='proposed'"
      : "r.status IN ('proposed','approved') AND r.required_minutes<=s.capacity_minutes");
  }
  values.push(input.filters.from, input.filters.to);
  if (input.query) {
    where.push(`(position(lower(?) in lower(COALESCE(s.id,'')))>0 OR position(lower(?) in lower(COALESCE(spec.name,'')))>0
      OR position(lower(?) in lower(COALESCE(w.name,'')))>0 OR position(lower(?) in lower(COALESCE(branch.display_name,'')))>0
      OR position(lower(?) in lower(COALESCE(s.status,'')))>0 OR position(lower(?) in lower(COALESCE(r.status,'')))>0
      OR position(lower(?) in lower(COALESCE(s.due_date,'')))>0 OR position(lower(?) in lower(COALESCE(r.feasible_date,'')))>0)`);
    values.push(...Array(8).fill(input.query));
  }
  if (cursor) {
    where.push(isScenario ? "s.id>?" : "r.id>?");
    values.push(cursor.id);
  }
  const rows = (await db.prepare(`SELECT ${isScenario ? "s.id" : "r.id"} id,${commonSelect}
    ${sourceFrom} WHERE ${where.join(" AND ")} ORDER BY ${isScenario ? "s.id" : "r.id"} LIMIT ?`)
    .bind(...values,input.limit+1).all<EvidenceRow>()).results;
  const hasMore = rows.length > input.limit, page = rows.slice(0,input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query: input.query,
    cursor: input.cursor ?? null, scope: scopeKey(actor) }));
  const auditRows = page.flatMap((row) => isScenario
    ? [{ id: row.scenario_id, version: safeRevision(row.scenario_version) }]
    : [
      { id: row.recommendation_id!, version: safeRevision(row.recommendation_version) },
      { id: row.scenario_id, version: safeRevision(row.scenario_version) },
    ]);
  await currentAuthority(db, actor);
  await writeAudit(db, actor, input.metric, "search", auditRows, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields: fieldsFor(input.metric),
    items: page.map((row) => {
      const scenario = input.metric === "planning-shortage-scenarios";
      return {
        id: row.id,
        label: scenario ? `Scenario · ${row.specification_name ?? row.scenario_id}` : `${row.specification_name ?? row.scenario_id} · ${safeDate(row.feasible_date)}`,
        summary: scenario
          ? `${row.branch_name ?? "Branch"} · ${row.warehouse_name ?? "Warehouse"} · ${safeWholeText(row.shortage_count,"shortage count")} material shortage(s) · due ${safeDate(row.due_date)}`
          : `Scenario ${row.scenario_id} · ${row.recommendation_status} · ${safeWholeText(row.required_minutes,"required minutes")}/${safeWholeText(row.capacity_minutes,"capacity minutes")} minutes · feasible ${safeDate(row.feasible_date)}`,
        status: scenario ? row.scenario_status : row.recommendation_status,
        version: safeRevision(scenario ? row.scenario_version : row.recommendation_version),
        href: "/planning",
      };
    }),
    hasMore,
    nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify({ metric: input.metric, filters: input.filters,
      query: input.query, scope: scopeKey(actor), id: page.at(-1)!.id })).toString("base64url") : null,
    queryHash,
  };
}

export async function readPlanningEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  rawMetric: string,
  recordId: string,
  rawFilters: unknown,
) {
  const metric = z.enum(planningEvidenceMetrics).parse(rawMetric), filters = filtersSchema.parse(rawFilters);
  await requirePlanningEvidence(db, actor, metric);
  const row = await planningExecutiveEvidenceRow(db,actor,metric as PlanningExecutiveEvidenceMetric,recordId,filters.from,filters.to) as EvidenceRow | null;
  if (!row) throw notFound();
  const scenario = metric === "planning-shortage-scenarios";
  const sourceShortages = shortages(row.shortage_items_json);
  const record = scenario ? {
    id: row.scenario_id,
    branch_name: row.branch_name ?? "Branch",
    warehouse_name: row.warehouse_name ?? "Warehouse",
    specification_name: row.specification_name ?? "Specification",
    output_product_name: row.output_product_name,
    specification_version: safeRevision(row.specification_version),
    demand_quantity_milli: safeWholeText(row.demand_quantity_milli,"demand quantity"),
    due_date: safeDate(row.due_date),
    capacity_minutes: safeWholeText(row.capacity_minutes,"capacity minutes"),
    shortages: sourceShortages,
    recommendation_id: row.recommendation_id,
    recommendation_status: row.recommendation_status,
    recommendation_version: row.recommendation_version === null ? null : safeRevision(row.recommendation_version),
    status: row.scenario_status,
    version: safeRevision(row.scenario_version),
    created_at: safeTimestamp(row.scenario_created_at),
    updated_at: safeTimestamp(row.scenario_updated_at),
    href: "/planning",
  } : {
    id: row.recommendation_id!,
    scenario_id: row.scenario_id,
    branch_name: row.branch_name ?? "Branch",
    warehouse_name: row.warehouse_name ?? "Warehouse",
    specification_name: row.specification_name ?? "Specification",
    output_product_name: row.output_product_name,
    specification_version: safeRevision(row.specification_version),
    scenario_version: safeRevision(row.scenario_version),
    demand_quantity_milli: safeWholeText(row.demand_quantity_milli,"demand quantity"),
    scenario_due_date: safeDate(row.due_date),
    capacity_minutes: safeWholeText(row.capacity_minutes,"capacity minutes"),
    shortages: sourceShortages,
    recommended_quantity_milli: safeWholeText(row.recommended_quantity_milli,"recommended quantity"),
    required_minutes: safeWholeText(row.required_minutes,"required minutes"),
    feasible_date: safeDate(row.feasible_date),
    status: row.recommendation_status!,
    version: safeRevision(row.recommendation_version),
    created_at: safeTimestamp(row.recommendation_created_at),
    updated_at: safeTimestamp(row.recommendation_updated_at),
    href: "/planning",
  };
  const queryHash = await sha256(JSON.stringify({ providerId, metric, filters, scope: scopeKey(actor), recordId }));
  const auditRows = scenario
    ? [{ id: row.scenario_id, version: safeRevision(row.scenario_version) }]
    : [
      { id: row.recommendation_id!, version: safeRevision(row.recommendation_version) },
      { id: row.scenario_id, version: safeRevision(row.scenario_version) },
    ];
  await currentAuthority(db, actor);
  await writeAudit(db, actor, metric, "open", auditRows, queryHash);
  await currentAuthority(db, actor);
  return { provider: providerId, metric, fields: fieldsFor(metric), record, queryHash };
}
