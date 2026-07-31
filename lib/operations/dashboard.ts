import 'server-only';

import pg from 'pg';
import { query } from '@/db';
import {
  OPERATIONS_BRANCH_UNAVAILABLE,
  OPERATIONS_SCOPE_UNAVAILABLE,
  type OperationsActorForScope,
  type OperationsActorScope,
  operationsBranchIsAllowed,
  operationsHoldingIsAllowed,
  operationsReadBranchRefs,
  resolveOperationsActorScope,
} from './scope';

export interface OperationsFilters {
  actor: OperationsActorForScope;
  branchId?: number | null;
  activityDays?: 7 | 30 | 90;
  scope?: OperationsActorScope;
}

export type OperationsReadPath = 'erpx_projected' | 'folio_break_glass';

export interface OperationsProjectedSummary {
  projectionName: string;
  holdingRef: string;
  readRowCount: number;
  exceptionCount: number;
  projectionCheckpointCount: number;
  latestProjectedAt: string | null;
}

export interface OperationsProjectedException {
  id: string;
  category: string;
  severity: string;
  status: string;
  assignedTo: string | null;
  managerRef: string | null;
  dueAt: string | null;
  aggregateVersion: number;
  updatedAt: string;
  sourceDomain: string;
  sourceType: string;
}

export interface OperationsStage {
  key: string;
  label: string;
  count: number;
  amountThb: number;
  href: string;
}

export interface OperationsAttentionItem {
  key: string;
  label: string;
  detail: string;
  count: number;
  href: string;
  tone: 'critical' | 'caution' | 'info' | 'neutral';
}

export interface OperationsDashboard {
  generatedAt: string;
  readPath: OperationsReadPath;
  projectedSummary: OperationsProjectedSummary | null;
  projectedExceptions: OperationsProjectedException[];
  projectedSourceCounts: Array<{ sourceDomain: string; count: number }>;
  filters: { branchId: number | null; activityDays: 7 | 30 | 90 };
  branches: Array<{ id: number; code: string; name: string }>;
  metrics: {
    openSalesOrders: number;
    openSalesValueThb: number;
    overdueSalesOrders: number;
    openPurchaseRequests: number;
    openPurchaseOrders: number;
    openPurchaseValueThb: number;
    inventoryValueThb: number;
    activeProducts: number;
    lowStockItems: number;
    openWaybills: number;
  };
  salesStages: OperationsStage[];
  procurementStages: OperationsStage[];
  attention: OperationsAttentionItem[];
  recentSales: Array<{
    id: number;
    number: string;
    customer: string;
    status: string;
    amountThb: number;
    dueDate: string | null;
    branch: string;
  }>;
  recentPurchasing: Array<{
    kind: 'pr' | 'po';
    id: number;
    number: string;
    party: string;
    status: string;
    amountThb: number;
    updatedAt: string;
  }>;
  recentMovements: Array<{
    id: number;
    number: string;
    kind: string;
    date: string;
    branch: string;
    source: string;
    valueThb: number;
  }>;
}

interface StatusMetric {
  status: string;
  count: string;
  amount_thb: string;
}

interface ProcurementStatusMetric extends StatusMetric {
  entity: 'pr' | 'po';
}

const OPERATIONS_READ_PROJECTION = 'operations_read_model';
const OPERATIONS_READ_DATABASE_URL = 'ERPX_OPERATIONS_READ_DATABASE_URL';
const ERPX_TENANT_READ_DATABASE_URL = 'ERPX_TENANT_READ_DATABASE_URL';
const OPERATIONS_HOLDING_REF = 'ERPX_OPERATIONS_HOLDING_REF';
const MAX_PROJECTED_ROWS = 500;

let operationsReadPool: pg.Pool | null = null;
let operationsReadPoolUrl: string | null = null;

interface ProjectedRow {
  aggregate_type: string;
  aggregate_id: string;
  holding_ref: string;
  aggregate_version: string | number;
  lifecycle_status: string;
  state: unknown;
  updated_at: string;
}

type ProjectedRowRecord = Record<string, unknown>;

function projectedReadConfig(): { connectionString: string; holdingRef: string } {
  const connectionString = process.env[OPERATIONS_READ_DATABASE_URL]?.trim()
    || process.env[ERPX_TENANT_READ_DATABASE_URL]?.trim();
  const holdingRef = process.env[OPERATIONS_HOLDING_REF]?.trim();
  if (!connectionString || !holdingRef) {
    throw new Error('explicit ERPX Operations projected read path is required');
  }
  if (process.env.NODE_ENV === 'production' && !/[?&]sslmode=verify-full(?:&|$)/i.test(connectionString)) {
    throw new Error('ERPX Operations projected read path must use PostgreSQL TLS verify-full');
  }
  return { connectionString, holdingRef };
}

function projectedReadPoolFor(connectionString: string): pg.Pool {
  if (!operationsReadPool || operationsReadPoolUrl !== connectionString) {
    operationsReadPool?.end().catch(() => {});
    operationsReadPool = new pg.Pool({ connectionString, max: 8, idleTimeoutMillis: 30_000 });
    operationsReadPoolUrl = connectionString;
  }
  return operationsReadPool;
}

function asRecord(value: unknown, field: string): ProjectedRowRecord {
  const parsed = typeof value === 'string' ? JSON.parse(value) as unknown : value;
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throw new Error(`ERPX Operations projected row ${field} is invalid`);
  }
  return parsed as ProjectedRowRecord;
}

function stringValue(value: unknown, field: string, optional = false): string | null {
  if (value == null && optional) return null;
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`ERPX Operations projected row ${field} is invalid`);
  }
  return value;
}

function numberValue(value: unknown, field: string): number {
  const parsed = typeof value === 'number' ? value : Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error(`ERPX Operations projected row ${field} is invalid`);
  }
  return parsed;
}

function projectedRowBranchRefs(state: ProjectedRowRecord): string[] {
  const meta = asRecord(state.meta, 'meta');
  const scope = asRecord(meta.scope, 'scope');
  const branch = scope.Branch;
  if (branch) {
    const branchRecord = asRecord(branch, 'branch scope');
    const branchId = stringValue(branchRecord.branch_id, 'branch scope branch_id');
    return [branchId as string];
  }
  const subtree = scope.Subtree;
  if (subtree) {
    const subtreeRecord = asRecord(subtree, 'subtree scope');
    const branchIds = subtreeRecord.branch_ids;
    if (!Array.isArray(branchIds) || branchIds.some((id) => typeof id !== 'string')) {
      throw new Error('ERPX Operations projected row subtree branch_ids is invalid');
    }
    return branchIds as string[];
  }
  return [];
}

function projectedRowMatchesScope(
  state: ProjectedRowRecord,
  scope: OperationsActorScope,
  branchId: number | null,
): boolean {
  const allowedRefs = operationsReadBranchRefs(scope, branchId);
  if (allowedRefs === null) return true;
  const rowBranches = projectedRowBranchRefs(state);
  if (!rowBranches.length) return scope.allowHoldingLevel;
  return rowBranches.some((ref) => allowedRefs.includes(ref));
}

async function loadProjectedOperationsDashboard(
  filters: OperationsFilters,
  scope: OperationsActorScope,
  branchId: number | null,
  activityDays: 7 | 30 | 90,
): Promise<OperationsDashboard> {
  const { connectionString, holdingRef } = projectedReadConfig();
  if (!operationsHoldingIsAllowed(scope, holdingRef)) {
    throw new Error(OPERATIONS_SCOPE_UNAVAILABLE);
  }
  if (!operationsBranchIsAllowed(scope, branchId)) {
    throw new Error(OPERATIONS_BRANCH_UNAVAILABLE);
  }
  if (branchId !== null && !operationsReadBranchRefs(scope, branchId)?.length) {
    throw new Error(OPERATIONS_BRANCH_UNAVAILABLE);
  }
  const pool = projectedReadPoolFor(connectionString);
  const client = await pool.connect();
  try {
    const readBranchRefs = operationsReadBranchRefs(scope, branchId);
    const projectedScopePredicate = `
          AND (
            $3::text[] IS NULL
            OR state #>> '{meta,scope,Branch,branch_id}' = ANY($3::text[])
            OR EXISTS (
              SELECT 1
                FROM jsonb_array_elements_text(
                  COALESCE(state #> '{meta,scope,Subtree,branch_ids}', '[]'::jsonb)
                ) AS projected_branch(branch_id)
               WHERE projected_branch.branch_id = ANY($3::text[])
            )
            OR (
              $4::boolean
              AND state #> '{meta,scope,Branch}' IS NULL
              AND state #> '{meta,scope,Subtree}' IS NULL
            )
          )`;
    await client.query('BEGIN');
    await client.query(`SELECT set_config('erpx.holding_ref', $1, true)`, [holdingRef]);
    const summary = await client.query<{
      read_row_count: string;
      exception_count: string;
      latest_projected_at: string | null;
    }>(
      `SELECT count(*)::text AS read_row_count,
              count(*) FILTER (WHERE aggregate_type = 'operations.exception_case')::text AS exception_count,
              max(updated_at)::text AS latest_projected_at
         FROM erpx_tenant.operations_read_rows
        WHERE projection_name = $1 AND holding_ref = $2
          ${projectedScopePredicate}`,
      [OPERATIONS_READ_PROJECTION, holdingRef, readBranchRefs, scope.allowHoldingLevel],
    );
    const checkpoints = await client.query<{ count: string }>(
      `SELECT count(*)::text AS count
         FROM erpx_tenant.operations_projection_checkpoints c
        WHERE c.projection_name = $1 AND c.holding_ref = $2
          AND EXISTS (
            SELECT 1
              FROM erpx_tenant.operations_read_rows r
             WHERE r.projection_name = c.projection_name
               AND r.aggregate_type = c.aggregate_type
               AND r.aggregate_id = c.aggregate_id
               AND r.holding_ref = c.holding_ref
               ${projectedScopePredicate.replaceAll('state ', 'r.state ')}
          )`,
      [OPERATIONS_READ_PROJECTION, holdingRef, readBranchRefs, scope.allowHoldingLevel],
    );
    const projectedSourceCounts = await client.query<{ source_domain: string; count: string }>(
      `SELECT state #>> '{source,source_domain}' AS source_domain, count(*)::text AS count
         FROM erpx_tenant.operations_read_rows
        WHERE projection_name = $1 AND holding_ref = $2
          AND state #>> '{source,source_domain}' IS NOT NULL
          ${projectedScopePredicate}
        GROUP BY state #>> '{source,source_domain}'
        ORDER BY source_domain`,
      [OPERATIONS_READ_PROJECTION, holdingRef, readBranchRefs, scope.allowHoldingLevel],
    );
    const rows = await client.query<ProjectedRow>(
      `SELECT aggregate_type, aggregate_id::text, holding_ref,
              aggregate_version, lifecycle_status, state, updated_at::text
         FROM erpx_tenant.operations_read_rows
        WHERE projection_name = $1 AND holding_ref = $2
          ${projectedScopePredicate}
        ORDER BY updated_at DESC, aggregate_id DESC
        LIMIT $5`,
      [
        OPERATIONS_READ_PROJECTION,
        holdingRef,
        readBranchRefs,
        scope.allowHoldingLevel,
        MAX_PROJECTED_ROWS + 1,
      ],
    );
    await client.query('COMMIT');

    if (rows.rows.length > MAX_PROJECTED_ROWS) {
      throw new Error('ERPX Operations projected exception page exceeded its display bound');
    }

    const summaryRow = summary.rows[0];
    const checkpointCount = Number(checkpoints.rows[0]?.count ?? 0);
    const readRowCount = Number(summaryRow?.read_row_count ?? 0);
    if (!summaryRow || !Number.isSafeInteger(readRowCount) || !Number.isSafeInteger(checkpointCount) || (readRowCount > 0 && checkpointCount === 0)) {
      throw new Error('ERPX Operations projected read model is inconsistent');
    }
    const projectedExceptions: OperationsProjectedException[] = [];
    for (const row of rows.rows) {
      if (row.holding_ref !== holdingRef || row.aggregate_type.trim() === '') {
        throw new Error('ERPX Operations projected row escaped its holding scope');
      }
      const state = asRecord(row.state, 'state');
      if (!projectedRowMatchesScope(state, scope, branchId)) continue;
      const source = state.source ? asRecord(state.source, 'source') : null;
      const sourceDomain = source ? stringValue(source.source_domain, 'source_domain') : null;
      if (row.aggregate_type !== 'operations.exception_case') continue;
      const category = stringValue(state.category, 'category');
      const severity = stringValue(state.severity, 'severity');
      const status = stringValue(state.status ?? row.lifecycle_status, 'status');
      projectedExceptions.push({
        id: stringValue(row.aggregate_id, 'aggregate_id') as string,
        category: category as string,
        severity: severity as string,
        status: status as string,
        assignedTo: stringValue(state.assigned_to, 'assigned_to', true),
        managerRef: stringValue(state.manager_ref, 'manager_ref', true),
        dueAt: stringValue(state.due_at, 'due_at', true),
        aggregateVersion: numberValue(row.aggregate_version, 'aggregate_version'),
        updatedAt: stringValue(row.updated_at, 'updated_at') as string,
        sourceDomain: sourceDomain ?? 'unknown',
        sourceType: source ? (stringValue(source.source_type, 'source_type') as string) : 'unknown',
      });
    }
    const branches = { rows: scope.branches.map((branch) => ({
      id: String(branch.id),
      code: branch.code,
      name: branch.name,
    })) };
    return emptyDashboard({
      filters: { branchId, activityDays },
      readPath: 'erpx_projected',
      projectedSummary: {
        projectionName: OPERATIONS_READ_PROJECTION,
        holdingRef,
        readRowCount,
        exceptionCount: Number(summaryRow.exception_count ?? 0),
        projectionCheckpointCount: checkpointCount,
        latestProjectedAt: summaryRow.latest_projected_at,
      },
      projectedExceptions,
      projectedSourceCounts: projectedSourceCounts.rows.map((row) => ({ sourceDomain: row.source_domain, count: Number(row.count) })),
      branches: branches.rows.map((row) => ({ id: Number(row.id), code: row.code, name: row.name })),
    });
  } catch (error) {
    await client.query('ROLLBACK').catch(() => {});
    throw error;
  } finally {
    client.release();
  }
}

function emptyDashboard(args: Pick<OperationsDashboard, 'filters' | 'readPath' | 'projectedSummary' | 'projectedExceptions' | 'projectedSourceCounts' | 'branches'>): OperationsDashboard {
  return {
    generatedAt: new Date().toISOString(),
    readPath: args.readPath,
    projectedSummary: args.projectedSummary,
    projectedExceptions: args.projectedExceptions,
    projectedSourceCounts: args.projectedSourceCounts,
    filters: args.filters,
    branches: args.branches,
    metrics: {
      openSalesOrders: 0,
      openSalesValueThb: 0,
      overdueSalesOrders: 0,
      openPurchaseRequests: 0,
      openPurchaseOrders: 0,
      openPurchaseValueThb: 0,
      inventoryValueThb: 0,
      activeProducts: 0,
      lowStockItems: 0,
      openWaybills: 0,
    },
    salesStages: [],
    procurementStages: [],
    attention: [],
    recentSales: [],
    recentPurchasing: [],
    recentMovements: [],
  };
}

function sumStatuses(rows: StatusMetric[], statuses: string[]) {
  const selected = rows.filter((row) => statuses.includes(row.status));
  return {
    count: selected.reduce((sum, row) => sum + Number(row.count), 0),
    amountThb: selected.reduce((sum, row) => sum + Number(row.amount_thb), 0),
  };
}

function stage(key: string, label: string, href: string, rows: StatusMetric[], statuses: string[]): OperationsStage {
  return { key, label, href, ...sumStatuses(rows, statuses) };
}

function hasProjectedReadConfiguration(): boolean {
  return Boolean(
    (process.env[OPERATIONS_READ_DATABASE_URL] ?? '').trim()
      && (process.env[OPERATIONS_HOLDING_REF] ?? '').trim(),
  ) || Boolean(
    (process.env[ERPX_TENANT_READ_DATABASE_URL] ?? '').trim()
      && (process.env[OPERATIONS_HOLDING_REF] ?? '').trim(),
  );
}

export async function loadOperationsDashboard(filters: OperationsFilters): Promise<OperationsDashboard> {
  const branchId = filters.branchId ?? null;
  const activityDays = filters.activityDays ?? 30;
  const scope = filters.scope ?? await resolveOperationsActorScope(filters.actor);
  if (branchId !== null && !operationsBranchIsAllowed(scope, branchId)) {
    throw new Error(OPERATIONS_BRANCH_UNAVAILABLE);
  }
  if (hasProjectedReadConfiguration()) {
    return loadProjectedOperationsDashboard(filters, scope, branchId, activityDays);
  }
  if (process.env.NODE_ENV !== 'production' && process.env.ERPX_LOCAL_BREAK_GLASS_READ === 'true') {
    return loadLegacyOperationsDashboard(filters, scope);
  }
  throw new Error('explicit ERPX Operations projected read path is required');
}

async function loadLegacyOperationsDashboard(
  filters: OperationsFilters,
  scope: OperationsActorScope,
): Promise<OperationsDashboard> {
  const branchId = filters.branchId ?? null;
  const activityDays = filters.activityDays ?? 30;
  const branches = await query<{ id: string; code: string; name: string }>(
    `SELECT id::text, code, name FROM finance.branches WHERE active ORDER BY code`,
  );
  const visibleBranches = scope.platformWide
    ? branches.rows
    : branches.rows.filter((branch) => scope.branches.some((allowed) => allowed.id === Number(branch.id)));
  if (!visibleBranches.length) {
    throw new Error(OPERATIONS_SCOPE_UNAVAILABLE);
  }
  if (branchId !== null && !visibleBranches.some((branch) => Number(branch.id) === branchId)) {
    throw new Error(OPERATIONS_BRANCH_UNAVAILABLE);
  }
  const branchParam = branchId;
  const branchFilter = (column: string) => scope.platformWide
    ? `($1::bigint IS NULL OR ${column} = $1)`
    : `(${column} = ANY($2::bigint[])) AND ($1::bigint IS NULL OR ${column} = $1)`;
  const branchParams: unknown[] = scope.platformWide
    ? [branchParam]
    : [branchParam, scope.branches.map((branch) => branch.id)];
  const requestScopeFilter = (alias: string) => scope.platformWide && branchId === null
    ? 'TRUE'
    : `EXISTS (
          SELECT 1
            FROM folio.purchase_orders scoped_po
           WHERE scoped_po.pr_id = ${alias}.id
             AND ${branchFilter('scoped_po.branch_id')}
        )`;
  const waybillScopeFilter = scope.platformWide && branchId === null
    ? 'TRUE'
    : `(
         (w.origin = 'so' AND EXISTS (
            SELECT 1 FROM folio.sales_orders scoped_so
             WHERE scoped_so.id = w.origin_id
               AND ${branchFilter('scoped_so.branch_id')}
         ))
         OR (w.origin = 'po' AND EXISTS (
            SELECT 1 FROM folio.purchase_orders scoped_po
             WHERE scoped_po.id = w.origin_id
               AND ${branchFilter('scoped_po.branch_id')}
         ))
         OR (w.origin = 'pr' AND EXISTS (
               SELECT 1 FROM folio.purchase_requisitions scoped_pr
                WHERE scoped_pr.id = w.origin_id
                  AND ${requestScopeFilter('scoped_pr')}
             ))
       )`;
  const productScopeFilter = scope.platformWide && branchId === null
    ? 'TRUE'
    : `EXISTS (
          SELECT 1
            FROM inventory.stock_balances scoped_sb
            JOIN inventory.warehouses scoped_w ON scoped_w.id = scoped_sb.warehouse_id
           WHERE scoped_sb.product_id = p.id
             AND ${branchFilter('scoped_w.branch_id')}
        )`;
  const [salesMetrics, purchasingMetrics, inventoryMetrics, salesStatus, purchasingStatus, attentionMetrics, recentSales, recentPurchasing, recentMovements] = await Promise.all([
    query<{ open_count: string; open_value: string; overdue_count: string }>(
      `SELECT
         count(*) FILTER (WHERE status NOT IN ('so_paid', 'rejected'))::text AS open_count,
         coalesce(sum(total_amount * fx_rate) FILTER (WHERE status NOT IN ('so_paid', 'rejected')), 0)::text AS open_value,
         count(*) FILTER (WHERE due_date < current_date AND status NOT IN ('so_paid', 'rejected'))::text AS overdue_count
       FROM folio.sales_orders
       WHERE ${branchFilter('branch_id')}`,
      branchParams,
    ),
    query<{ open_pr: string; overdue_pr: string; open_po: string; open_po_value: string; unreceived_lines: string }>(
      `SELECT
         (SELECT count(*) FROM folio.purchase_requisitions pr WHERE pr.status NOT IN ('disbursed', 'rejected') AND ${requestScopeFilter('pr')})::text AS open_pr,
         (SELECT count(*) FROM folio.purchase_requisitions pr WHERE pr.need_by_date < current_date AND pr.status NOT IN ('disbursed', 'rejected') AND ${requestScopeFilter('pr')})::text AS overdue_pr,
         (SELECT count(*) FROM folio.purchase_orders WHERE status NOT IN ('disbursed', 'rejected') AND ${branchFilter('branch_id')})::text AS open_po,
         (SELECT coalesce(sum(total_amount * fx_rate), 0) FROM folio.purchase_orders WHERE status NOT IN ('disbursed', 'rejected') AND ${branchFilter('branch_id')})::text AS open_po_value,
         (SELECT count(*) FROM folio.po_items i JOIN folio.purchase_orders po ON po.id = i.po_id WHERE i.received_qty < i.qty AND po.status NOT IN ('disbursed', 'rejected') AND ${branchFilter('po.branch_id')})::text AS unreceived_lines`,
      branchParams,
    ),
    query<{ inventory_value: string; products: string; low_stock: string; active_reservations: string }>(
      `SELECT
         (SELECT coalesce(sum(sb.quantity * sb.avg_cost_thb), 0)
            FROM inventory.stock_balances sb
            JOIN inventory.warehouses w ON w.id = sb.warehouse_id
           WHERE ${branchFilter('w.branch_id')})::text AS inventory_value,
         (SELECT count(*) FROM inventory.products p WHERE p.active AND ${productScopeFilter})::text AS products,
         (SELECT count(*)
            FROM inventory.reorder_settings rs
            JOIN inventory.warehouses w ON w.id = rs.warehouse_id
            LEFT JOIN (
              SELECT product_id, warehouse_id, sum(quantity) AS quantity
                FROM inventory.stock_balances
               GROUP BY product_id, warehouse_id
            ) stock ON stock.product_id = rs.product_id AND stock.warehouse_id = rs.warehouse_id
           WHERE coalesce(stock.quantity, 0) <= rs.reorder_point
             AND ${branchFilter('w.branch_id')})::text AS low_stock,
         (SELECT count(*)
            FROM inventory.reservations r
            JOIN inventory.warehouses w ON w.id = r.warehouse_id
           WHERE r.status = 'active' AND ${branchFilter('w.branch_id')})::text AS active_reservations`,
      branchParams,
    ),
    query<StatusMetric>(
      `SELECT status, count(*)::text, coalesce(sum(total_amount * fx_rate), 0)::text AS amount_thb
         FROM folio.sales_orders
        WHERE ${branchFilter('branch_id')}
        GROUP BY status`,
      branchParams,
    ),
    query<ProcurementStatusMetric>(
      `SELECT 'pr'::text AS entity, status::text, count(*)::text,
              coalesce(sum(total_estimate), 0)::text AS amount_thb
         FROM folio.purchase_requisitions pr
        WHERE ${requestScopeFilter('pr')}
        GROUP BY status
       UNION ALL
       SELECT 'po'::text AS entity, status::text, count(*)::text,
              coalesce(sum(total_amount * fx_rate), 0)::text AS amount_thb
         FROM folio.purchase_orders
        WHERE ${branchFilter('branch_id')}
        GROUP BY status`,
      branchParams,
    ),
    query<{ open_waybills: string; failed_recost: string; open_counts: string }>(
      `SELECT
         (SELECT count(*) FROM folio.waybills w WHERE w.status = 'open' AND w.origin IN ('pr', 'po', 'so') AND ${waybillScopeFilter})::text AS open_waybills,
         (SELECT count(*)
            FROM inventory.recost_jobs r
            JOIN inventory.warehouses w ON w.id = r.warehouse_id
           WHERE r.status = 'failed' AND ${branchFilter('w.branch_id')})::text AS failed_recost,
         (SELECT count(*)
            FROM inventory.stock_counts c
            JOIN inventory.warehouses w ON w.id = c.warehouse_id
           WHERE c.status IN ('draft', 'counted') AND ${branchFilter('w.branch_id')})::text AS open_counts`,
      branchParams,
    ),
    query<{ id: string; so_number: string; customer: string; status: string; amount_thb: string; due_date: string | null; branch: string }>(
      `SELECT so.id::text, so.so_number, c.name AS customer, so.status,
              (so.total_amount * so.fx_rate)::text AS amount_thb,
              so.due_date::text, coalesce(b.code, 'Unassigned') AS branch
         FROM folio.sales_orders so
         JOIN folio.customers c ON c.id = so.customer_id
         LEFT JOIN finance.branches b ON b.id = so.branch_id
        WHERE ${branchFilter('so.branch_id')}
        ORDER BY so.updated_at DESC, so.id DESC
        LIMIT 8`,
      branchParams,
    ),
    query<{ kind: 'pr' | 'po'; id: string; number: string; party: string; status: string; amount_thb: string; updated_at: string }>(
      `SELECT * FROM (
         SELECT 'pr'::text AS kind, pr.id::text, pr.pr_number AS number,
                coalesce(pr.vendor_name, 'Vendor not selected') AS party,
                pr.status::text, pr.total_estimate::text AS amount_thb, pr.updated_at::text
           FROM folio.purchase_requisitions pr
          WHERE ${requestScopeFilter('pr')}
         UNION ALL
         SELECT 'po'::text AS kind, po.id::text, po.po_number AS number,
                coalesce(po.vendor_name, 'Vendor not selected') AS party,
                po.status::text, (po.total_amount * po.fx_rate)::text AS amount_thb, po.updated_at::text
           FROM folio.purchase_orders po
          WHERE ${branchFilter('po.branch_id')}
       ) activity
       ORDER BY updated_at DESC
       LIMIT 8`,
      branchParams,
    ),
    query<{ id: string; movement_no: string; kind: string; movement_date: string; branch: string; source: string; value_thb: string }>(
      `SELECT m.id::text, coalesce(m.movement_no, 'Movement #' || m.id::text) AS movement_no,
              m.kind, m.movement_date::text, b.code AS branch,
              m.source_type || ' · ' || m.source_id AS source,
              coalesce(sum(l.extended_cost_thb), 0)::text AS value_thb
         FROM inventory.stock_movements m
         JOIN finance.branches b ON b.id = m.branch_id
         LEFT JOIN inventory.stock_movement_lines l ON l.movement_id = m.id
        WHERE m.status = 'posted'
          AND m.movement_date >= current_date - $${branchParams.length + 1}::integer
          AND ${branchFilter('m.branch_id')}
        GROUP BY m.id, b.code
        ORDER BY m.movement_date DESC, m.id DESC
        LIMIT 8`,
      [...branchParams, activityDays],
    ),
  ]);

  const sales = salesMetrics.rows[0];
  const purchasing = purchasingMetrics.rows[0];
  const inventory = inventoryMetrics.rows[0];
  const attention = attentionMetrics.rows[0];
  const salesRows = salesStatus.rows;
  const prRows = purchasingStatus.rows.filter((row) => row.entity === 'pr');
  const poRows = purchasingStatus.rows.filter((row) => row.entity === 'po');
  const openWaybills = Number(attention?.open_waybills ?? 0);
  const overdueSales = Number(sales?.overdue_count ?? 0);
  const overduePr = Number(purchasing?.overdue_pr ?? 0);
  const unreceivedLines = Number(purchasing?.unreceived_lines ?? 0);
  const failedRecost = Number(attention?.failed_recost ?? 0);
  const openCounts = Number(attention?.open_counts ?? 0);
  const activeReservations = Number(inventory?.active_reservations ?? 0);

  return {
    generatedAt: new Date().toISOString(),
    readPath: 'folio_break_glass',
    projectedSummary: null,
    projectedExceptions: [],
    projectedSourceCounts: [],
    filters: { branchId, activityDays },
    branches: visibleBranches.map((row) => ({ id: Number(row.id), code: row.code, name: row.name })),
    metrics: {
      openSalesOrders: Number(sales?.open_count ?? 0),
      openSalesValueThb: Number(sales?.open_value ?? 0),
      overdueSalesOrders: overdueSales,
      openPurchaseRequests: Number(purchasing?.open_pr ?? 0),
      openPurchaseOrders: Number(purchasing?.open_po ?? 0),
      openPurchaseValueThb: Number(purchasing?.open_po_value ?? 0),
      inventoryValueThb: Number(inventory?.inventory_value ?? 0),
      activeProducts: Number(inventory?.products ?? 0),
      lowStockItems: Number(inventory?.low_stock ?? 0),
      openWaybills,
    },
    salesStages: [
      stage('sales-draft', 'Draft and review', '/sales?scope=all', salesRows, ['so_draft', 'so_sales_review']),
      stage('sales-approval', 'Approval and credit', '/sales?scope=queue', salesRows, ['so_dept_approval', 'so_credit_check']),
      stage('sales-collection', 'Invoice and collect', '/sales?scope=all', salesRows, ['so_invoiced']),
      stage('sales-complete', 'Paid', '/sales?scope=all', salesRows, ['so_paid']),
    ],
    procurementStages: [
      stage('procurement-request', 'Request', '/pr', prRows, ['draft', 'submission']),
      stage('procurement-approval', 'Approval', '/pr', prRows, ['dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision', 'accounting_authorization', 'disbursement_authorization', 'cfo_authorization', 'ceo_authorization', 'awaiting_disbursement']),
      stage('procurement-order', 'Order and receive', '/po', poRows, ['draft', 'submission', 'dept_verification', 'dept_authorization', 'accounting_verification', 'accounting_supervision', 'accounting_authorization', 'disbursement_authorization', 'cfo_authorization', 'ceo_authorization', 'awaiting_disbursement']),
      stage('procurement-complete', 'Disbursed', '/po', poRows, ['disbursed']),
    ],
    attention: [
      { key: 'overdue-sales', label: 'Overdue sales orders', detail: 'Due date passed before payment', count: overdueSales, href: '/sales?scope=all', tone: overdueSales > 0 ? 'critical' : 'neutral' },
      { key: 'overdue-pr', label: 'Overdue purchase requests', detail: 'Need-by date has passed', count: overduePr, href: '/pr', tone: overduePr > 0 ? 'caution' : 'neutral' },
      { key: 'unreceived-po', label: 'PO lines awaiting receipt', detail: 'Ordered quantity exceeds received quantity', count: unreceivedLines, href: '/inventory', tone: unreceivedLines > 0 ? 'caution' : 'neutral' },
      { key: 'low-stock', label: 'Low-stock items', detail: 'At or below configured reorder point', count: Number(inventory?.low_stock ?? 0), href: '/inventory', tone: Number(inventory?.low_stock ?? 0) > 0 ? 'caution' : 'neutral' },
      { key: 'recost', label: 'Failed recost jobs', detail: 'Inventory costing replay requires attention', count: failedRecost, href: '/inventory', tone: failedRecost > 0 ? 'critical' : 'neutral' },
      { key: 'stock-count', label: 'Open stock counts', detail: 'Draft or counted but not posted', count: openCounts, href: '/inventory', tone: openCounts > 0 ? 'info' : 'neutral' },
      { key: 'reservations', label: 'Active reservations', detail: 'Stock committed to open demand', count: activeReservations, href: '/inventory', tone: 'info' },
      { key: 'waybills', label: 'Open operational workflows', detail: 'Sales and procurement approval flows', count: openWaybills, href: '/inbox?scope=waiting', tone: openWaybills > 0 ? 'info' : 'neutral' },
    ],
    recentSales: recentSales.rows.map((row) => ({
      id: Number(row.id), number: row.so_number, customer: row.customer, status: row.status,
      amountThb: Number(row.amount_thb), dueDate: row.due_date, branch: row.branch,
    })),
    recentPurchasing: recentPurchasing.rows.map((row) => ({
      kind: row.kind, id: Number(row.id), number: row.number, party: row.party,
      status: row.status, amountThb: Number(row.amount_thb), updatedAt: row.updated_at,
    })),
    recentMovements: recentMovements.rows.map((row) => ({
      id: Number(row.id), number: row.movement_no, kind: row.kind, date: row.movement_date,
      branch: row.branch, source: row.source, valueThb: Number(row.value_thb),
    })),
  };
}
