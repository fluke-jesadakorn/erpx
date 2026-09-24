import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/permissions";
import { filtersSchema, type Filters } from "@/lib/executive/canvas";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { ApiError } from "../http";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { crmOpportunityEvidenceRow } from "../crm";
import { customerFollowupEvidenceRow } from "../customer-followups";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const crmEvidenceMetrics = ["opportunity-pipeline", "overdue-followups"] as const;
type CrmEvidenceMetric = typeof crmEvidenceMetrics[number];
const opportunityProviderId = "crm-opportunities";
const opportunityMetricId = "opportunity-pipeline" as const;
const followupProviderId = "crm-customer-followups";
const followupMetricId = "overdue-followups" as const;
const openStages = ["discovery", "proposal", "negotiation"] as const;
const fields = [
  "lead.title", "lead.status", "lead.version", "opportunity.stage", "opportunity.expected_minor",
  "opportunity.currency", "opportunity.close_date", "opportunity.version",
];
const followupFields = ["followup.next_action", "followup.status", "followup.due_date", "followup.version", "order.id", "order.number"];
const inputSchema = z.object({
  metric: z.enum(crmEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();

type Cursor = { closeDate: string; id: string };
type OpportunityRow = {
  id: string;
  lead_id: string;
  title: string;
  lead_status: string;
  lead_version: number | string | bigint;
  stage: typeof openStages[number];
  expected_minor: number | string | bigint;
  currency: string;
  close_date: string;
  version: number | string | bigint;
};
type FollowupRow = { id:string;order_id:string;next_action:string;due_date:string;status:"open";version:number|string|bigint;order_number:string };

function parseCursor(value?: string): Cursor | null {
  if (!value) return null;
  try {
    const parsed = JSON.parse(Buffer.from(value, "base64url").toString("utf8")) as { closeDate?: unknown; id?: unknown };
    if (typeof parsed.closeDate !== "string") throw new Error();
    return { closeDate: z.iso.date().parse(parsed.closeDate), id: z.string().min(1).max(200).parse(parsed.id) };
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeCursor(row: Pick<OpportunityRow, "close_date" | "id">) {
  return Buffer.from(JSON.stringify({ closeDate: row.close_date, id: row.id })).toString("base64url");
}

function parseFollowupCursor(value?:string) {
  if(!value)return null;
  try {
    const parsed=JSON.parse(Buffer.from(value,"base64url").toString("utf8")) as {dueDate?:unknown;id?:unknown};
    if(typeof parsed.dueDate!=="string")throw new Error();
    return {dueDate:z.iso.date().parse(parsed.dueDate),id:z.string().min(1).max(200).parse(parsed.id)};
  } catch {
    throw new ApiError(422,"EVIDENCE_CURSOR_INVALID","The source record cursor is invalid. Refresh the evidence list.");
  }
}

function encodeFollowupCursor(row:Pick<FollowupRow,"due_date"|"id">) {
  return Buffer.from(JSON.stringify({dueDate:row.due_date,id:row.id})).toString("base64url");
}

function exactVersion(value: number | string | bigint, field: string) {
  const result = Number(value);
  if (!Number.isSafeInteger(result) || result < 1) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The source returned an invalid ${field}.`);
  return result;
}

function exactMinor(value: number | string | bigint) {
  const result = String(value);
  if (!/^(0|[1-9]\d*)$/.test(result)) throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The source returned an invalid opportunity amount.");
  return result;
}

function notFound() {
  return new ApiError(404, "EVIDENCE_NOT_FOUND", "This source record is unavailable in the selected role, scope or filters.");
}

async function requireCrmEvidence(db: SqlDatabase, actor: WorkContext, metric: CrmEvidenceMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "commerce.read") || !hasModuleAccess(actor.modules, "crm"))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "crm");
}

async function writeAudit(
  db: SqlDatabase,
  actor: WorkContext,
  provider:string,
  metric:CrmEvidenceMetric,
  action: "search" | "open",
  records: { id: string; version: number }[],
  allowedFields:string[],
  queryHash: string,
) {
  if (!records.length) return;
  await db.prepare(`INSERT INTO executive_evidence_access_events
    (id,company_id,actor_id,assignment_id,provider_id,metric_id,action,record_id,record_version,field_ids,query_hash,created_at)
    SELECT 'executive-evidence-'||gen_random_uuid()::text,?,?,?,?,?,?,item.record_id,item.record_version,?::jsonb,?,?
    FROM jsonb_to_recordset(?::jsonb) AS item(record_id text,record_version bigint)`)
    .bind(
      actor.companyId, actor.userId, actor.assignmentId, provider, metric, action,
      JSON.stringify(allowedFields), queryHash, Date.now(), JSON.stringify(records.map(({ id, version }) => ({ record_id: id, record_version: version }))),
    ).run();
}

function scopedConditions(actor: WorkContext, filters: Filters) {
  return {
    sql: [
      "l.company_id=?", "(?::text IS NULL OR l.branch_id=?)", "(?::text IS NULL OR l.department_id=?)",
      "v.currency=?", "v.close_date BETWEEN ? AND ?", "v.stage IN ('discovery','proposal','negotiation')",
    ],
    values: [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, filters.currency, filters.from, filters.to] as unknown[],
  };
}

function auditRecords(rows: OpportunityRow[]) {
  return rows.flatMap((row) => [
    { id: row.id, version: exactVersion(row.version, "opportunity version") },
    { id: row.lead_id, version: exactVersion(row.lead_version, "lead version") },
  ]);
}

export async function searchCrmEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requireCrmEvidence(db, actor, input.metric);
  if(input.metric===followupMetricId)return searchCustomerFollowupEvidence(db,actor,input);
  const cursor = parseCursor(input.cursor);
  const scope = scopedConditions(actor, input.filters);
  const conditions = [...scope.sql];
  const values = [...scope.values];
  if (input.query) {
    conditions.push("(position(lower(?) in lower(l.title))>0 OR position(lower(?) in lower(v.stage))>0 OR position(lower(?) in lower(v.close_date))>0)");
    values.push(input.query, input.query, input.query);
  }
  if (cursor) {
    conditions.push("(v.close_date>? OR (v.close_date=? AND v.id>?))");
    values.push(cursor.closeDate, cursor.closeDate, cursor.id);
  }
  const rows = (await db.prepare(`SELECT v.id,v.lead_id,l.title,l.status lead_status,l.version lead_version,
      v.stage,v.expected_minor,v.currency,v.close_date,v.version
    FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id
    WHERE ${conditions.join(" AND ")} ORDER BY v.close_date,v.id LIMIT ?`)
    .bind(...values, input.limit + 1).all<OpportunityRow>()).results;
  const hasMore = rows.length > input.limit;
  const page = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({
    providerId:opportunityProviderId, metric: input.metric, filters: input.filters, query: input.query, cursor: input.cursor ?? null,
    scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId],
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, opportunityProviderId, opportunityMetricId, "search", auditRecords(page), fields, queryHash);
  await currentAuthority(db, actor);
  return {
    provider: opportunityProviderId,
    metric: input.metric,
    fields,
    items: page.map((row) => ({
      id: row.id,
      label: row.title,
      summary: `${row.stage} · closes ${row.close_date}`,
      amount: exactMinor(row.expected_minor),
      currency: row.currency,
      status: row.stage,
      version: exactVersion(row.version, "opportunity version"),
      href: `/crm/${encodeURIComponent(row.lead_id)}`,
    })),
    hasMore,
    nextCursor: hasMore && page.length ? encodeCursor(page.at(-1)!) : null,
    queryHash,
  };
}

export async function readCrmEvidence(db: SqlDatabase, actor: WorkContext, metric: string, recordId: string, rawFilters: unknown) {
  const parsedMetric = z.enum(crmEvidenceMetrics).parse(metric), filters = filtersSchema.parse(rawFilters);
  await requireCrmEvidence(db, actor, parsedMetric);
  if(parsedMetric===followupMetricId)return readCustomerFollowupEvidence(db,actor,recordId,filters);
  const id = z.string().min(1).max(200).parse(recordId);
  // Recheck through a narrow CRM source reader so branch, department and commerce.read stay aligned with the CRM UI.
  const current = await crmOpportunityEvidenceRow(actor, id) as Record<string, unknown> | null;
  if (!current || current.id !== id || !openStages.includes(String(current.stage) as typeof openStages[number])
    || current.currency !== filters.currency || typeof current.close_date !== "string"
    || current.close_date < filters.from || current.close_date > filters.to) throw notFound();

  const opportunityVersion = exactVersion(current.opportunity_version as number | string | bigint, "opportunity version");
  const leadVersion = exactVersion(current.lead_version as number | string | bigint, "lead version");
  const record = {
    id,
    lead_id: String(current.lead_id),
    title: String(current.title),
    lead_status: String(current.lead_status),
    lead_version: leadVersion,
    stage: String(current.stage),
    expected_minor: exactMinor(current.expected_minor as number | string | bigint),
    currency: String(current.currency),
    close_date: String(current.close_date),
    version: opportunityVersion,
    href: `/crm/${encodeURIComponent(String(current.lead_id))}`,
  };
  const queryHash = await sha256(JSON.stringify({
    providerId:opportunityProviderId, metric: parsedMetric, filters, scope: [actor.companyId, actor.assignmentId, actor.branchId, actor.departmentId], recordId: id,
  }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, opportunityProviderId, opportunityMetricId, "open", [{ id, version: opportunityVersion }, { id: String(current.lead_id), version: leadVersion }], fields, queryHash);
  await currentAuthority(db, actor);
  return { provider: opportunityProviderId, metric: parsedMetric, fields, record, queryHash };
}

async function currentUtcDate(db:SqlDatabase) {
  const row=await db.prepare("SELECT to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') utc_date").first<{utc_date:string}>();
  if(!row||!/^\d{4}-\d{2}-\d{2}$/.test(row.utc_date))throw new ApiError(500,"EVIDENCE_REFERENCE_INVALID","The source did not return a valid UTC date.");
  return row.utc_date;
}

async function searchCustomerFollowupEvidence(
  db:SqlDatabase,
  actor:WorkContext,
  input:z.infer<typeof inputSchema>,
) {
  const referenceDate=await currentUtcDate(db),cursor=parseFollowupCursor(input.cursor);
  const conditions=[
    "o.company_id=?","(?::text IS NULL OR o.branch_id=?)","(?::text IS NULL OR o.department_id=?)",
    "o.kind IN ('sales','quote')","c.status='open'","c.due_date BETWEEN ? AND ?","c.due_date<?",
  ];
  const values:unknown[]=[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,input.filters.from,input.filters.to,referenceDate];
  if(input.query){
    conditions.push("(position(lower(?) in lower(c.next_action))>0 OR position(lower(?) in lower(o.number))>0 OR position(lower(?) in lower(c.due_date))>0)");
    values.push(input.query,input.query,input.query);
  }
  if(cursor){conditions.push("(c.due_date>? OR (c.due_date=? AND c.id>?))");values.push(cursor.dueDate,cursor.dueDate,cursor.id);}
  const rows=(await db.prepare(`SELECT c.id,c.order_id,c.next_action,c.due_date,c.status,c.version,o.number order_number FROM customer_followups c
    JOIN commerce_orders o ON o.id=c.order_id AND o.company_id=c.company_id JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id
    WHERE ${conditions.join(" AND ")} ORDER BY c.due_date,c.id LIMIT ?`)
    .bind(...values,input.limit+1).all<FollowupRow>()).results;
  const hasMore=rows.length>input.limit,page=rows.slice(0,input.limit);
  const queryHash=await sha256(JSON.stringify({providerId:followupProviderId,metric:followupMetricId,filters:input.filters,query:input.query,cursor:input.cursor??null,referenceDate,scope:[actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId]}));
  await currentAuthority(db,actor);
  await writeAudit(db,actor,followupProviderId,followupMetricId,"search",page.map(row=>({id:row.id,version:exactVersion(row.version,"follow-up version")})),followupFields,queryHash);
  await currentAuthority(db,actor);
  return {
    provider:followupProviderId,metric:followupMetricId,fields:followupFields,
    items:page.map(row=>({id:row.id,label:row.next_action,summary:`Open · due ${row.due_date}`,status:row.status,version:exactVersion(row.version,"follow-up version"),href:`/customer-followups/${encodeURIComponent(row.id)}`})),
    hasMore,nextCursor:hasMore&&page.length?encodeFollowupCursor(page.at(-1)!):null,queryHash,
  };
}

async function readCustomerFollowupEvidence(db:SqlDatabase,actor:WorkContext,recordId:string,filters:Filters) {
  const id=z.string().min(1).max(200).parse(recordId),referenceDate=await currentUtcDate(db);
  const row=await customerFollowupEvidenceRow(actor,id,filters.from,filters.to,referenceDate) as Record<string,unknown>|null;
  if(!row)throw notFound();
  const version=exactVersion(row.version as number|string|bigint,"follow-up version");
  const record={
    id:String(row.id),next_action:String(row.next_action),status:String(row.status),due_date:String(row.due_date),version,
    order_id:String(row.order_id),order_number:String(row.order_number),href:`/customer-followups/${encodeURIComponent(String(row.id))}`,
    order_href:`/commerce/orders/${encodeURIComponent(String(row.order_id))}`,
  };
  const queryHash=await sha256(JSON.stringify({providerId:followupProviderId,metric:followupMetricId,filters,referenceDate,scope:[actor.companyId,actor.assignmentId,actor.branchId,actor.departmentId],recordId:id}));
  await currentAuthority(db,actor);
  await writeAudit(db,actor,followupProviderId,followupMetricId,"open",[{id,version}],followupFields,queryHash);
  await currentAuthority(db,actor);
  return {provider:followupProviderId,metric:followupMetricId,fields:followupFields,record,queryHash};
}
