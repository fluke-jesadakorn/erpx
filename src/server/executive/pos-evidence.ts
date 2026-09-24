import "server-only";
import { z } from "zod";
import { hasModuleAccess } from "@/data/module-access";
import { filtersSchema, type Filters } from "@/lib/executive/canvas";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "../authorization";
import type { SqlDatabase } from "../runtime/types";
import { posExecutiveEvidenceRow } from "../pos";
import { assertProductionModuleEnabled } from "../hospitality-rollout";
import { ApiError } from "../http";
import { sha256 } from "../crypto";
import { currentAuthority } from "./access";
import { metricCatalogFor } from "./metrics";

export const posEvidenceMetrics = ["pos-sales", "net-pos-sales", "pos-payment-mix"] as const;
type PosEvidenceMetric = typeof posEvidenceMetrics[number];
const providerId = "pos-transaction-records";
const grossSalesFields = ["sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.total_minor", "sale.version"];
const netSalesFields = [
  "sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.total_minor", "sale.version",
  "refund.id", "refund.sale_id", "refund.branch_id", "refund.accounting_date", "refund.kind", "refund.state", "refund.amount_minor", "refund.version",
];
const paymentMixFields = [
  "sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.version",
  "payment.id", "payment.method", "payment.state", "payment.amount_minor", "payment.version",
  "refund.kind", "refund_payment.amount_minor", "refund_payment.state",
];
const inputSchema = z.object({
  metric: z.enum(posEvidenceMetrics),
  filters: filtersSchema,
  query: z.string().trim().max(100).default(""),
  cursor: z.string().max(2000).optional(),
  limit: z.coerce.number().int().min(1).max(50).default(25),
}).strict();

type EvidenceRow = {
  id: string;
  source_id: string;
  source_type: "sale" | "refund" | "payment";
  sale_id: string;
  event_date: string;
  receipt_number: string;
  status: string;
  sale_status: string;
  currency: string;
  method: string | null;
  amount_minor: string | number | bigint;
  contribution_minor: string | number | bigint;
  refunded_minor: string | number | bigint;
  revision: string | number | bigint;
  sale_version: string | number | bigint;
};
type PosDetailRow = {
  source_id: string;
  sale_id: string;
  source_type: "sale" | "refund" | "payment";
  receipt_number: string;
  business_date: string;
  accounting_date: string | null;
  currency: string;
  state: string;
  sale_state?: string;
  method?: string;
  amount_minor: string | number | bigint;
  contribution_minor: string | number | bigint;
  refunded_minor: string | number | bigint;
  revision: string | number | bigint;
  sale_version: string | number | bigint;
};
type PosCursor = { metric: PosEvidenceMetric; filters: Filters; query: string; scope: string[]; eventDate: string; id: string };

function fieldsFor(metric: PosEvidenceMetric) {
  return metric === "pos-sales" ? grossSalesFields : metric === "net-pos-sales" ? netSalesFields : paymentMixFields;
}

function safeIntegerString(value: string | number | bigint, name: string, allowNegative = false) {
  const text = String(value);
  if (!(allowNegative ? /^-?(0|[1-9][0-9]*)$/ : /^(0|[1-9][0-9]*)$/).test(text))
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", `The POS source returned an invalid ${name}.`);
  return text;
}

function safeVersion(value: string | number | bigint) {
  const version = Number(value);
  if (!Number.isSafeInteger(version) || version < 1)
    throw new ApiError(500, "EVIDENCE_VALUE_INVALID", "The POS source returned an invalid record revision.");
  return version;
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
    const cursor: PosCursor = {
      metric: z.enum(posEvidenceMetrics).parse(parsed.metric),
      filters: filtersSchema.parse(parsed.filters),
      query: z.string().max(100).parse(parsed.query),
      scope: z.array(z.string()).max(4).parse(parsed.scope),
      eventDate: z.string().date().parse(parsed.eventDate),
      id: z.string().min(1).max(240).parse(parsed.id),
    };
    if (cursor.metric !== input.metric || JSON.stringify(cursor.filters) !== JSON.stringify(input.filters)
      || cursor.query !== input.query || JSON.stringify(cursor.scope) !== JSON.stringify(scopeKey(actor))) throw new Error();
    return cursor;
  } catch {
    throw new ApiError(422, "EVIDENCE_CURSOR_INVALID", "The source record cursor is invalid. Refresh the evidence list.");
  }
}

async function requirePosEvidence(db: SqlDatabase, actor: WorkContext, metric: PosEvidenceMetric) {
  await currentAuthority(db, actor);
  if (!hasPermission(actor, "pos.read") || !hasPermission(actor, "pos.reports") || !hasModuleAccess(actor.modules, "pos") || actor.departmentId)
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "POS evidence requires report access at Company or Branch scope.");
  const catalog = await metricCatalogFor(db, actor);
  if (!catalog.some((item) => item.id === metric && item.available))
    throw new ApiError(403, "EVIDENCE_FORBIDDEN", "This source record is not available in the current role or scope.");
  await assertProductionModuleEnabled(db, actor.companyId, "pos");
}

async function writeAudit(
  db: SqlDatabase,
  actor: WorkContext,
  metric: PosEvidenceMetric,
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

function sourceQuery(metric: PosEvidenceMetric) {
  if (metric === "pos-sales") return `SELECT 'sale:'||s.id id,s.id source_id,'sale' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,s.state status,s.state sale_status,s.currency,NULL::text method,
      s.total_minor::numeric amount_minor,s.total_minor::numeric contribution_minor,0::numeric refunded_minor,
      s.version revision,s.version sale_version
    FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)`;
  if (metric === "net-pos-sales") return `SELECT 'sale:'||s.id id,s.id source_id,'sale' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,s.state status,s.state sale_status,s.currency,NULL::text method,
      s.total_minor::numeric amount_minor,s.total_minor::numeric contribution_minor,0::numeric refunded_minor,
      s.version revision,s.version sale_version
    FROM pos_sales s WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)
    UNION ALL
    SELECT 'refund:'||r.id id,r.id source_id,'refund' source_type,s.id sale_id,r.accounting_date event_date,
      s.number receipt_number,r.state status,s.state sale_status,s.currency,NULL::text method,
      r.amount_minor::numeric amount_minor,-r.amount_minor::numeric contribution_minor,0::numeric refunded_minor,
      r.version revision,s.version sale_version
    FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
    WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
      AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed'
      AND (?='' OR position(lower(?) in lower(s.number))>0)`;
  return `SELECT 'payment:'||p.id id,p.id source_id,'payment' source_type,s.id sale_id,s.business_date event_date,
      s.number receipt_number,p.state status,s.state sale_status,s.currency,p.method,
      p.amount_minor::numeric amount_minor,
      (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)) contribution_minor,
      COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp
        JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
        WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0) refunded_minor,
      p.version revision,s.version sale_version
    FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
    WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
      AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded'
      AND (?='' OR position(lower(?) in lower(s.number))>0 OR position(lower(?) in lower(p.method))>0)`;
}

function sourceQueryValues(metric: PosEvidenceMetric, actor: WorkContext, filters: Filters, query: string) {
  const base = [actor.companyId, actor.branchId, actor.branchId, filters.currency, filters.from, filters.to];
  if (metric === "pos-sales" || metric === "net-pos-sales") {
    const search = [query, query];
    return metric === "pos-sales" ? [...base, ...search] : [...base, ...search, ...base, ...search];
  }
  return [...base, ...[query, query, query]];
}

function hrefFor(row: Pick<EvidenceRow, "sale_id">) {
  return `/pos?tab=history&sale=${encodeURIComponent(row.sale_id)}`;
}

export async function searchPosEvidence(db: SqlDatabase, actor: WorkContext, raw: unknown) {
  const input = inputSchema.parse(raw);
  await requirePosEvidence(db, actor, input.metric);
  const cursor = parseCursor(input.cursor, input, actor);
  const values: unknown[] = sourceQueryValues(input.metric, actor, input.filters, input.query);
  const cursorSql = cursor ? "WHERE (event_date<? OR (event_date=? AND id<?))" : "";
  if (cursor) values.push(cursor.eventDate, cursor.eventDate, cursor.id);
  values.push(input.limit + 1);
  const rows = (await db.prepare(`WITH events AS (${sourceQuery(input.metric)})
    SELECT * FROM events ${cursorSql} ORDER BY event_date DESC,id DESC LIMIT ?`)
    .bind(...values).all<EvidenceRow>()).results;
  const hasMore = rows.length > input.limit, page = rows.slice(0, input.limit);
  const queryHash = await sha256(JSON.stringify({ providerId, metric: input.metric, filters: input.filters, query: input.query,
    cursor: input.cursor ?? null, scope: scopeKey(actor) }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, input.metric, "search", page.map((row) => ({ id: row.id, version: safeVersion(row.revision) })), queryHash);
  await currentAuthority(db, actor);
  return {
    provider: providerId,
    metric: input.metric,
    fields: fieldsFor(input.metric),
    items: page.map((row) => {
      const eventDate = z.string().date().parse(row.event_date);
      const amount = safeIntegerString(row.contribution_minor, "contribution", true);
      const label = row.source_type === "sale" ? `Sale ${row.receipt_number}`
        : row.source_type === "refund" ? `Refund · ${row.receipt_number}`
          : `Payment · ${row.method} · ${row.receipt_number}`;
      const summary = row.source_type === "refund" ? `Accounted ${eventDate} · ${row.status}`
        : row.source_type === "payment" ? `Business date ${eventDate} · ${row.status}`
          : `Business date ${eventDate} · ${row.status}`;
      return { id: row.id, label, summary, amount, currency: row.currency, status: row.status,
        version: safeVersion(row.revision), href: hrefFor(row) };
    }),
    hasMore,
    nextCursor: hasMore && page.length ? Buffer.from(JSON.stringify({ metric: input.metric, filters: input.filters, query: input.query,
      scope: scopeKey(actor), eventDate: page.at(-1)!.event_date, id: page.at(-1)!.id } satisfies PosCursor)).toString("base64url") : null,
    queryHash,
  };
}

export async function readPosEvidence(db: SqlDatabase, actor: WorkContext, rawMetric: string, recordId: string, rawFilters: unknown) {
  const metric = z.enum(posEvidenceMetrics).parse(rawMetric), filters = filtersSchema.parse(rawFilters);
  await requirePosEvidence(db, actor, metric);
  const row = await posExecutiveEvidenceRow(db, actor, metric, recordId, filters) as PosDetailRow | null;
  if (!row || (metric === "pos-sales" && row.source_type !== "sale")
    || (metric === "net-pos-sales" && !["sale", "refund"].includes(row.source_type))
    || (metric === "pos-payment-mix" && row.source_type !== "payment")) throw notFound();
  const revision = safeVersion(row.revision), saleVersion = safeVersion(row.sale_version);
  const amount = safeIntegerString(row.amount_minor, "amount");
  const contribution = safeIntegerString(row.contribution_minor, "contribution", true);
  const refunded = safeIntegerString(row.refunded_minor, "refunded amount");
  const fields = metric === "pos-sales" ? grossSalesFields
    : metric === "net-pos-sales" ? (row.source_type === "refund"
      ? ["refund.id", "refund.sale_id", "refund.branch_id", "refund.accounting_date", "refund.kind", "refund.state", "refund.amount_minor", "refund.version", "sale.number", "sale.currency", "sale.version"]
      : ["sale.id", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.total_minor", "sale.version"])
      : ["payment.id", "payment.sale_id", "payment.method", "payment.state", "payment.amount_minor", "payment.version", "sale.number", "sale.business_date", "sale.currency", "sale.branch_id", "sale.state", "sale.version", "refund.kind", "refund_payment.amount_minor", "refund_payment.state"];
  const record = {
    id: recordId,
    source_type: row.source_type,
    receipt_number: row.receipt_number,
    business_date: row.business_date,
    accounting_date: row.accounting_date ?? null,
    currency: row.currency,
    status: row.state,
    sale_status: row.sale_state ?? row.state,
    method: row.method ?? null,
    amount_minor: amount,
    contribution_minor: contribution,
    refunded_minor: refunded,
    version: revision,
    sale_version: saleVersion,
    href: hrefFor(row),
  };
  const queryHash = await sha256(JSON.stringify({ providerId, metric, filters, scope: scopeKey(actor), recordId }));
  await currentAuthority(db, actor);
  await writeAudit(db, actor, metric, "open", [{ id: recordId, version: revision }], queryHash);
  await currentAuthority(db, actor);
  return { provider: providerId, metric, fields, record, queryHash };
}
