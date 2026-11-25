import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { SalesOrderEvidenceRow, SalesEvidenceCursor, SalesFulfillmentEvidenceRow } from '@erpx/server-runtime/public/executive/evidence-contract';
import {
  nativeCommerceReadAll,
  RD_EXEC_SALES_ORDERS_EVIDENCE,
  RD_EXEC_SALES_CANCELLATIONS_EVIDENCE,
  RD_EXEC_SALES_CANCELLATION_VALUE_EVIDENCE,
  RD_EXEC_SALES_FULFILLMENT_EVIDENCE,
} from './native-commerce-read';

type EvidenceFilters = { from: string; to: string; currency: string };
type OrderMetric = 'sales-orders' | 'sales-cancellations' | 'sales-cancellation-value';
const optional = (value: string) => value || null;

/** The owner plans a fixed statement per closed metric variant. Optional
 * query, cursor and record selectors become NULL-safe positional values, so
 * no rendered predicate or caller SQL is assembled in TypeScript. */
function orderEvidencePlan(metric: OrderMetric) {
  if (metric === 'sales-orders') return RD_EXEC_SALES_ORDERS_EVIDENCE;
  if (metric === 'sales-cancellations') return RD_EXEC_SALES_CANCELLATIONS_EVIDENCE;
  return RD_EXEC_SALES_CANCELLATION_VALUE_EVIDENCE;
}

function orderEvidenceValues(actor: WorkContext, metric: OrderMetric, filters: EvidenceFilters, query: string, cursor: SalesEvidenceCursor | undefined, limit: number, recordId?: string): unknown[] {
  const scope = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  const period = metric === 'sales-orders'
    ? [filters.from, filters.to, filters.currency]
    : [Date.parse(filters.from), Date.parse(filters.to) + 86400000, ...(metric === 'sales-cancellation-value' ? [filters.currency] : [])];
  let selectors: unknown[];
  if (!cursor) selectors = [null, null, null];
  else if (metric === 'sales-orders' && 'orderDate' in cursor) selectors = [cursor.orderDate, cursor.orderDate, cursor.id];
  else if (metric !== 'sales-orders' && 'cancelledAt' in cursor) selectors = [cursor.cancelledAt, cursor.cancelledAt, cursor.id];
  else throw new ApiError(422, 'EVIDENCE_CURSOR_INVALID', 'The source record cursor is invalid. Refresh the evidence list.');
  const record = recordId ? [recordId, recordId] : [null, null];
  return [...scope, ...period, optional(query), optional(query), ...selectors, ...record, recordId ? 1 : limit + 1];
}

function fulfillmentEvidenceValues(actor: WorkContext, filters: EvidenceFilters, query: string, cursor: { orderDate: string; id: string } | undefined, limit: number, recordId?: string): unknown[] {
  const scope = [actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
  const period = [filters.from, filters.to];
  const selectors = cursor ? [cursor.orderDate, cursor.orderDate, cursor.id] : [null, null, null];
  const record = recordId ? [recordId, recordId] : [null, null];
  return [...scope, ...period, optional(query), optional(query), ...selectors, ...record, recordId ? 1 : limit + 1];
}

async function salesOrderEvidenceRows(db: SqlDatabase, actor: WorkContext, metric: OrderMetric, filters: EvidenceFilters, query: string, cursor?: SalesEvidenceCursor, limit = 25, recordId?: string) {
  return nativeCommerceReadAll<SalesOrderEvidenceRow>(orderEvidencePlan(metric), orderEvidenceValues(actor, metric, filters, query, cursor, limit, recordId), db);
}

async function salesFulfillmentEvidenceRows(db: SqlDatabase, actor: WorkContext, filters: EvidenceFilters, query: string, cursor?: { orderDate: string; id: string }, limit = 25, recordId?: string) {
  return nativeCommerceReadAll<SalesFulfillmentEvidenceRow>(RD_EXEC_SALES_FULFILLMENT_EVIDENCE, fulfillmentEvidenceValues(actor, filters, query, cursor, limit, recordId), db);
}

export function executiveMainEvidenceSources(db: SqlDatabase) {
  return {
    async factsSearchSalesOrderEvidence(actor: WorkContext, input: {
      filters: { from: string; to: string; currency: string };
      query: string;
      limit: number;
      metric: 'sales-orders' | 'sales-cancellations' | 'sales-cancellation-value';
      cursor?: string | undefined;
    }, cursor: SalesEvidenceCursor | null): Promise<{ rows: SalesOrderEvidenceRow[] }> {
      const rows = await salesOrderEvidenceRows(db, actor, input.metric, input.filters, input.query, cursor ?? undefined, input.limit);
      return { rows };
    },
    async factsReadSalesOrderEvidence(actor: WorkContext, metric: 'sales-orders' | 'sales-cancellations' | 'sales-cancellation-value', filters: { from: string; to: string; currency: string }, recordId: string): Promise<{ row: SalesOrderEvidenceRow }> {
      const [row] = await salesOrderEvidenceRows(db, actor, metric, filters, '', undefined, 1, recordId);
      return { row };
    },
    async factsSearchSalesFulfillmentEvidence(actor: WorkContext, input: {
      filters: { from: string; to: string; currency: string };
      query: string;
      limit: number;
      metric: 'sales-fulfillment-rate';
      cursor?: string | undefined;
    }, cursor: { orderDate: string; id: string } | null): Promise<{ rows: SalesFulfillmentEvidenceRow[] }> {
      const rows = await salesFulfillmentEvidenceRows(db, actor, input.filters, input.query, cursor ?? undefined, input.limit);
      return { rows };
    },
    async factsReadSalesFulfillmentEvidence(actor: WorkContext, filters: { from: string; to: string; currency: string }, recordId: string): Promise<{ row: SalesFulfillmentEvidenceRow }> {
      const [row] = await salesFulfillmentEvidenceRows(db, actor, filters, '', undefined, 1, recordId);
      return { row };
    },
  };
}
