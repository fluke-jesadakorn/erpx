import "server-only";
import { preparePmsAccounting } from "./hospitality-accounting";
import { createHash, randomUUID } from "node:crypto";
import { z } from "zod";
import {
  executePms,
  newProperty,
  PmsError,
  readiness,
  type Effects,
  type Posting,
} from "@/lib/pms-engine";
import {
  commandSchema,
  dateField,
  idField,
  operationPermission,
  pmsAccountKeys,
  plusDays,
  record,
  records,
  type Config,
  type Property,
  type Snapshot,
} from "@/lib/pms";
import { minorToDecimal } from "@/lib/finance-money";
import { prepareRoomCharge, roomChargeSchema } from "@/lib/pms-room-charge";
import { assertProductionModuleEnabled } from './hospitality-rollout';
import { hospitalityDayControls } from './hospitality-day-controls';
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { WorkItem } from "@/lib/work-item";
import type { WorkContext } from "./authorization";
import { executeCommand, replayCommand } from "./commands";
import { requirePermission } from "./iam-permissions";
import { prepareJournal } from "./finance";
import { prepareAttachmentBinding } from "./attachments/service";
import { ApiError } from "./http";
import { snapshotMoney, snapshotMoneyCarry, snapshotMoneyReversal } from "./company-money";
import { holdingResources } from "./runtime";
import { readAllPmsRecords, readPmsCalendar, readPmsDashboard, readPmsDetail, readPmsSetupFacts } from "./pms/read-repository";
import type { PmsSetupFacts } from "@/lib/pms-read";
import { pmsReadSelectionSchema, type PmsReadSelection } from "@/lib/pms-read-selection";
import { frontWorkspaceHref } from "./front-app";
import {
  isTransactionalSqlDatabase,
  type SqlDatabase,
  type TransactionalSqlDatabase,
} from "./runtime/types";

type ReadSnapshot = Snapshot & { dashboardHasStays?: boolean; setupFacts?: PmsSetupFacts };
type PropertyRow = {
  id: string;
  company_id: string;
  branch_id: string;
  name: string;
  business_date: string;
  state: Property["state"];
  config_json: string;
  version: number;
};
type TransferRequestRow = {
  id:string; company_id:string; property_id:string; folio_id:string; shift_id:string;
  kind:'receipt'|'deposit'; business_date:string; currency:string; amount_minor:number;
  reference:string; state:'pending'|'verified'|'cancelled'; version:number;
  recorded_by:string; bank_transaction_id:string|null; entry_id:string|null;
  finance_payment_id:string|null;
  created_at:number; updated_at:number;
};
type ChargeCorrectionRow = {
  id:string; company_id:string; property_id:string; entry_id:string; folio_id:string;
  status:'pending'|'approved'|'rejected'; version:number; reason:string; requested_by:string;
  reviewed_by:string|null; review_reason:string|null; reversal_entry_id:string|null;
  created_at:number; reviewed_at:number|null; updated_at:number;
};
const query = async <T>(db: SqlDatabase, sql: string, ...values: unknown[]) =>
  (
    await db
      .prepare(sql)
      .bind(...values)
      .all<T>()
  ).results;
const run = async (db: SqlDatabase, sql: string, ...values: unknown[]) =>
  db
    .prepare(sql)
    .bind(...values)
    .run();
function property(row: PropertyRow): Property {
  return {
    id: row.id,
    companyId: row.company_id,
    branchId: row.branch_id,
    name: row.name,
    businessDate: row.business_date,
    state: row.state,
    config: JSON.parse(row.config_json),
    version: Number(row.version),
  };
}
function scope(actor: WorkContext, p: Property) {
  if (
    p.companyId !== actor.companyId ||
    (actor.branchId && p.branchId !== actor.branchId) ||
    actor.departmentId ||
    actor.beta
  )
    throw new ApiError(
      404,
      "PMS_NOT_FOUND",
      "Hotel not found in this workspace",
    );
}
export async function loadPms(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  lock = false,
  selection?: PmsReadSelection,
): Promise<ReadSnapshot> {
  if (lock && selection) throw new Error("PMS_COMMAND_REQUIRES_COMPLETE_STATE");
  const row = await db
    .prepare(
      `SELECT * FROM pms_properties WHERE id=? AND company_id=?${lock ? " FOR UPDATE" : ""}`,
    )
    .bind(id, actor.companyId)
    .first<PropertyRow>();
  if (!row) throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found");
  const p = property(row);
  scope(actor, p);
  const selected = selection ? pmsReadSelectionSchema.parse(selection) : undefined;
  const dashboard = selected?.view === "dashboard" ? await readPmsDashboard(db, id, p.businessDate) : undefined;
  const rows = dashboard?.records ?? (selected?.view === "calendar"
    ? await readPmsCalendar(db, id, selected.from, selected.to)
    : selected?.view === "detail" ? await readPmsDetail(db, id, selected.id)
    : await readAllPmsRecords(db, id));
  if (selected?.view === "detail" && !rows.some(row => row.id === selected.id))
    throw new ApiError(404, "PMS_RECORD_NOT_FOUND", "Hotel item not found");
  const setupFacts = selected?.view === "calendar" || selected?.view === "detail"
    ? await readPmsSetupFacts(db, id) : undefined;
  return {
    property: p,
    records: rows,
    ...(dashboard ? { dashboardHasStays: dashboard.hasStays } : {}),
    ...(setupFacts ? { setupFacts } : {}),
  };
}
export const pmsPermissions = [
  "pms.read",
  "pms.configure",
  "pms.reservations",
  "pms.frontdesk",
  "pms.cashier",
  "pms.housekeeping",
  "pms.inspect",
  "pms.maintenance",
  "pms.audit",
  "pms.approve",
  "pms.accounting.prepare",
  "pms.accounting.approve",
  "pms.reports",
  "finance.approve",
] as const;
function visibleSnapshot(s: ReadSnapshot, actor: WorkContext): ReadSnapshot {
  const result = structuredClone(s);
  delete result.setupFacts;
  const financial =
    hasPermission(actor, "pms.cashier") || hasPermission(actor, "pms.reports");
  const guest =
    hasPermission(actor, "pms.reservations") ||
    hasPermission(actor, "pms.frontdesk") ||
    financial;
  if (!guest)
    result.records = result.records.filter(
      (r) =>
        ![
          "guest",
          "reservation",
          "folio",
          "entry",
          "refund",
          "credit",
          "shift",
        ].includes(r.kind),
    );
  else if (!financial)
    result.records = result.records.filter(
      (r) => !["entry", "refund", "credit", "shift"].includes(r.kind),
    );
  if (!financial)
    result.records = result.records.map((r) =>
      r.kind === "stay"
        ? { ...r, data: { ...r.data, prices: [] } }
        : r.kind === "folio"
          ? {
              ...r,
              data: {
                ...r.data,
                due: 0,
                advance: 0,
                advanceTax: 0,
                deposit: 0,
              },
            }
          : r,
    );
  if (
    !hasPermission(actor, "pms.accounting.prepare") &&
    !hasPermission(actor, "pms.accounting.approve") &&
    !hasPermission(actor, "pms.reports")
  )
    result.property.config = {
      ...result.property.config,
      accounts: Object.fromEntries(
        pmsAccountKeys.map((k) => [k, ""]),
      ) as Config["accounts"],
      chartId: "",
    };
  return result;
}
export async function pmsWorkspace(actor: WorkContext, propertyId?: string, includeDayControls=false, frontMode=false, selection?: PmsReadSelection) {
  requirePermission(actor, "pms.read");
  if (actor.departmentId || actor.beta)
    throw new ApiError(
      403,
      "PMS_BRANCH_REQUIRED",
      "Select a company or branch area",
    );
  const { write } = await holdingResources(actor.holdingId);
  const readSelection = selection ? pmsReadSelectionSchema.parse(selection) : undefined;
  const complete = !readSelection;
  const rows = await query<PropertyRow>(
    write,
    `SELECT * FROM pms_properties WHERE company_id=?${actor.branchId ? " AND branch_id=?" : ""} ORDER BY name`,
    actor.companyId,
    ...(actor.branchId ? [actor.branchId] : []),
  );
  const selected = propertyId ? rows.find((r) => r.id === propertyId) : rows[0];
  if (propertyId && !selected)
    throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found");
  const s = selected
    ? await (isTransactionalSqlDatabase(write)
        ? write.transaction((tx) => loadPms(tx, actor, selected.id, false, readSelection))
        : loadPms(write, actor, selected.id, false, readSelection))
    : null;
  const configuration = !frontMode && (complete || !s) && (
    hasPermission(actor, "pms.configure") ||
    hasPermission(actor, "pms.accounting.prepare") ||
    hasPermission(actor, "pms.accounting.approve"));
  const staff = complete ? await query<{ id: string; name: string }>(
    write,
    "SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name FROM assignments a LEFT JOIN hr_people h ON h.user_id=a.user_id AND h.company_id=a.company_id AND h.merged_into_id IS NULL WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)) ORDER BY name",
    actor.companyId,
    Date.now(),
    Date.now(),
    selected?.branch_id ?? actor.branchId ?? "",
  ) : [];
  const accounting = complete && !frontMode && (
    hasPermission(actor, "pms.accounting.prepare") ||
    hasPermission(actor, "pms.accounting.approve"));
  const branches = configuration
    ? await query<{ id: string; name: string }>(
        write,
        `SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active'${actor.branchId ? " AND id=?" : ""}`,
        actor.companyId,
        ...(actor.branchId ? [actor.branchId] : []),
      )
    : [];
  const accounts = accounting
    ? await query<{ id: string; name: string; code: string; chart_id: string }>(
        write,
        "SELECT id,name,code,chart_id FROM finance_accounts WHERE company_id=? AND status='active' AND currency='THB' ORDER BY code",
        actor.companyId,
      )
    : [];
  const charts = accounting
    ? await query<{ id: string; name: string }>(
        write,
        "SELECT id,name FROM finance_charts WHERE company_id=? AND base_currency='THB' AND status='active'",
        actor.companyId,
      )
    : [];
  const parties =
    complete && !frontMode && (hasPermission(actor, "pms.cashier") ||
    hasPermission(actor, "pms.configure") ||
    hasPermission(actor, "pms.reports"))
      ? await query<{ id: string; name: string }>(
          write,
          "SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND currency='THB' AND person_id IS NULL",
          actor.companyId,
        )
      : [];
  const products = configuration
    ? await query<{ id: string; name: string }>(
        write,
        "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active'",
        actor.companyId,
      )
    : [];
  const warehouses = configuration
    ? await query<{ id: string; name: string }>(
        write,
        "SELECT id,name FROM commerce_warehouses WHERE company_id=? AND branch_id=? AND status='active'",
        actor.companyId,
        selected?.branch_id ?? actor.branchId ?? "",
      )
    : [];
  const docs =
    complete && s &&
    (hasPermission(actor, "pms.cashier") || hasPermission(actor, "pms.reports"))
      ? await query<{
          id: string;
          folio_id: string;
          kind: string;
          created_at: number;
        }>(
          write,
          "SELECT id,folio_id,kind,created_at FROM pms_documents WHERE property_id=? ORDER BY created_at DESC",
          s.property.id,
        )
      : [];
  let ledger: null | {
    debit: number;
    credit: number;
    receivable: number;
    advance: number;
    deposit: number;
  } = null;
  if (complete && !frontMode && s && hasPermission(actor, "pms.reports")) {
    const lines = await query<{
      account_id: string;
      debit: number;
      credit: number;
    }>(
      write,
      "SELECT l.account_id,SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journal_lines l JOIN (SELECT property_id,journal_id FROM pms_postings UNION SELECT property_id,opening_journal_id FROM pms_import_batches WHERE state='applied' AND opening_journal_id IS NOT NULL) p ON p.journal_id=l.journal_id JOIN finance_journals j ON j.id=l.journal_id WHERE p.property_id=? AND j.status='posted' GROUP BY l.account_id",
      s.property.id,
    );
    const balance = (account: string) =>
      lines
        .filter((l) => l.account_id === account)
        .reduce((n, l) => n + Number(l.debit) - Number(l.credit), 0);
    ledger = {
      debit: lines.reduce((n, l) => n + Number(l.debit), 0),
      credit: lines.reduce((n, l) => n + Number(l.credit), 0),
      receivable: balance(s.property.config.accounts.receivable),
      advance: -balance(s.property.config.accounts.advance),
      deposit: -balance(s.property.config.accounts.deposit),
    };
  }
  const snapshot = s ? visibleSnapshot(s, actor) : null;
  if (frontMode && snapshot) {
    const { currency, minorUnit, timezone } = snapshot.property.config;
    snapshot.property.config = {
      country: "",
      currency,
      minorUnit,
      timezone,
      checkIn: "",
      checkOut: "",
      cancellationPolicy: "",
      chartId: "",
      taxBps: 0,
      serviceBps: 0,
      taxInclusive: false,
      advanceTax: false,
      preparedBy: "",
      approvedBy: null,
      revision: 0,
      accounts: Object.fromEntries(pmsAccountKeys.map((key) => [key, ""])) as Config["accounts"],
    };
    snapshot.records = snapshot.records
      .filter((row) => {
      const rowData = row.data as { requestedBy?: string; userId?: string };
      return ["room_type", "room", "rate", "guest", "reservation", "stay", "folio", "entry", "task", "block", "shift", "refund", "night"].includes(row.kind) &&
        row.kind !== "credit" &&
        (row.kind !== "refund" || rowData.requestedBy === actor.userId) &&
        (row.kind !== "shift" || rowData.userId === actor.userId);
      })
      .map((row) => row.kind === "entry"
        ? { ...row, data: { ...row.data, sourceKey: "sourceKey" in row.data && typeof row.data.sourceKey === "string" && row.data.sourceKey.startsWith("pos:") ? "pos" : "front" } }
        : row);
  }
  if (readSelection?.view === "detail" && snapshot && !snapshot.records.some(row => row.id === readSelection.id))
    throw new ApiError(404, "PMS_RECORD_NOT_FOUND", "Hotel item not found");
  const frontPermissions = [
    "pms.read",
    "pms.reservations",
    "pms.frontdesk",
    "pms.cashier",
    "pms.housekeeping",
    "pms.inspect",
    "pms.maintenance",
    "pms.audit",
  ] as const;
  const transferRequests = complete && s && (hasPermission(actor, "pms.cashier") || hasPermission(actor, "finance.approve"))
    ? await query<TransferRequestRow>(
        write,
        "SELECT * FROM pms_transfer_requests WHERE company_id=? AND property_id=? ORDER BY created_at DESC LIMIT 100",
        actor.companyId,
        s.property.id,
      )
      : [];
  const chargeCorrections = complete && s && (hasPermission(actor,"pms.cashier") || hasPermission(actor,"pms.approve"))
    ? await query<{
        id:string; entry_id:string; folio_id:string; status:string; version:number; reason:string;
        requested_by:string; reviewed_by:string|null; review_reason:string|null; reversal_entry_id:string|null;
        created_at:number; entry_kind:string; entry_description:string; entry_total:number;
      }>(write,`SELECT c.id,c.entry_id,c.folio_id,c.status,c.version,c.reason,c.requested_by,c.reviewed_by,c.review_reason,c.reversal_entry_id,c.created_at,
        e.data_json::jsonb->>'kind' entry_kind,e.data_json::jsonb->>'description' entry_description,(e.data_json::jsonb->>'total')::bigint entry_total
        FROM pms_charge_corrections c JOIN pms_records e ON e.id=c.entry_id AND e.property_id=c.property_id
        WHERE c.company_id=? AND c.property_id=? ${frontMode?" AND c.requested_by=?":" AND c.status='pending'"} ORDER BY c.created_at DESC LIMIT 100`,actor.companyId,s.property.id,...(frontMode?[actor.userId]:[]))
    : [];
  const readinessChecks = s ? readiness(s, s.setupFacts) : [];
  return {
    bookingDelivery:
      complete && !frontMode && s && hasPermission(actor, "pms.configure")
        ? await query<{
            id: string;
            reservation_id: string;
            state: string;
            attempts: number;
            last_error: string | null;
          }>(
            write,
            "SELECT id,reservation_id,state,attempts,last_error FROM pms_booking_emails WHERE property_id=? AND state!='sent' ORDER BY created_at DESC LIMIT 50",
            s.property.id,
          )
        : [],
    transferRequests: frontMode
      ? transferRequests.filter((row) => row.recorded_by === actor.userId)
      : transferRequests,
    chargeCorrections,
    stripeConnectionReady: complete && !frontMode && hasPermission(actor,'pms.cashier')&&Boolean(await write.prepare("SELECT 1 FROM pos_connections WHERE company_id=? AND state='ready' AND promptpay_enabled=true AND thb_bank_ready=true AND payout_interval='daily'").bind(actor.companyId).first()),
    stripePayments: complete && !frontMode && s&&hasPermission(actor,'pms.cashier')?await query<{id:string;folio_id:string;state:string;kind:string;amount_minor:number;refund_state:string|null;payer_email:string|null}>(write,"SELECT id,folio_id,state,kind,amount_minor,refund_state,payer_email FROM pms_stripe_payments WHERE property_id=? ORDER BY created_at DESC LIMIT 100",s.property.id):[],
    bookingRefunds: complete && !frontMode && hasPermission(actor,'finance.approve')&&!actor.branchId
      ? await query<{id:string;property_id:string;amount_minor:number;status:string;refund_state:string;created_at:number;last_error:string|null}>(write,"SELECT id,property_id,amount_minor,status,refund_state,created_at,last_error FROM pms_booking_payments WHERE company_id=? AND refund_state IS NOT NULL ORDER BY created_at DESC LIMIT 100",actor.companyId)
      : [],
    transferBankEvidence: complete && !frontMode && s && hasPermission(actor,'finance.approve')
      ? await query<{id:string;transaction_date:string;external_id:string;amount_minor:number;currency:string;version:number}>(write,`SELECT t.id,t.transaction_date,t.external_id,t.amount_minor,t.currency,t.version
        FROM finance_bank_transactions t JOIN finance_bank_statements b ON b.id=t.statement_id
        WHERE t.company_id=? AND b.bank_account=? AND t.currency=? AND t.amount_minor>0 AND t.match_status IN ('unmatched','proposed')
        ORDER BY t.transaction_date DESC,t.id LIMIT 100`,actor.companyId,s.property.config.accounts.bank,s.property.config.currency)
      : [],
    defaultDate: new Intl.DateTimeFormat("en-CA", {
      timeZone: "Asia/Bangkok",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
    }).format(new Date()),
    properties: rows.map((r) => ({
      id: r.id,
      name: r.name,
      branchId: r.branch_id,
      state: r.state,
      businessDate: r.business_date,
    })),
    snapshot,
    readiness: readinessChecks,
    frontReady: frontMode ? Boolean(s && s.property.state === "open" && readinessChecks.length === 0) : undefined,
    canManageSetup: hasPermission(actor, "pms.configure") || hasPermission(actor, "pms.accounting.prepare") || hasPermission(actor, "pms.accounting.approve"),
    permissions: (frontMode ? frontPermissions : pmsPermissions).filter((k) => hasPermission(actor, k)),
    availablePaymentMethods: ['cash'],
    userId: actor.userId,
    branches,
    staff,
    accounts: frontMode ? [] : accounts,
    charts: frontMode ? [] : charts,
    parties: frontMode ? [] : parties,
    products: frontMode ? [] : products,
    warehouses: frontMode ? [] : warehouses,
    documents: docs,
    ledger: frontMode ? null : ledger,
    dayControls:complete && s && includeDayControls && (hasPermission(actor,'pms.audit')||(!frontMode&&hasPermission(actor,'pms.reports')))
      ? await hospitalityDayControls(write,actor.companyId,s.property.branchId,s.property.id,s.property.businessDate)
      : null,
  };
}
export type PmsWorkspace = Awaited<ReturnType<typeof pmsWorkspace>>;

async function validSetup(db: SqlDatabase, s: Snapshot) {
  const p = s.property,
    c = p.config;
  const chart = await db
    .prepare(
      "SELECT id FROM finance_charts WHERE id=? AND company_id=? AND status='active' AND base_currency=?",
    )
    .bind(c.chartId, p.companyId, c.currency)
    .first();
  if (!chart)
    throw new ApiError(
      422,
      "PMS_CHART_INVALID",
      "Select this company's THB chart of accounts",
    );
  if (new Set(Object.values(c.accounts)).size !== pmsAccountKeys.length)
    throw new ApiError(
      422,
      "PMS_ACCOUNTS_DISTINCT",
      "Configure a separate control account for each category",
    );
  for (const [kind, id] of Object.entries(c.accounts)) {
    const account = await db
      .prepare(
        "SELECT category FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND status='active' FOR SHARE",
      )
      .bind(id, p.companyId, c.chartId, c.currency)
      .first<{ category: string }>();
    const category = ["revenue", "service"].includes(kind)
      ? "revenue"
      : ["advance", "deposit", "tax"].includes(kind)
        ? "liability"
        : ["consumption", "commission", "cashDifference"].includes(kind)
          ? "expense"
          : "asset";
    if (!account || account.category !== category)
      throw new ApiError(
        422,
        "PMS_ACCOUNT_INVALID",
        `Account ${kind} must be ${category} in the same chart and currency`,
      );
  }
}
async function post(
  db: SqlDatabase,
  actor: WorkContext,
  s: Snapshot,
  effect: Posting,
) {
  const p = s.property,
    c = p.config;
  const periods = await query<{ id: string }>(
    db,
    "SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=? FOR SHARE",
    p.companyId,
    effect.date,
    effect.date,
  );
  if (periods.length !== 1)
    throw new ApiError(
      409,
      "PMS_PERIOD_CLOSED",
      "An open accounting period is required for this business date",
    );
  const id = randomUUID();
  const prepared = await prepareJournal(
    db,
    actor,
    {
      chartId: c.chartId,
      periodId: periods[0].id,
      entryDate: effect.date,
      currency: c.currency,
      sourceType: "pms",
      sourceId: p.id,
      memo: effect.source,
      isOpening: false,
      lines: effect.lines.map((l) => ({
        accountId: c.accounts[l.account],
        debit: minorToDecimal(l.debit),
        credit: minorToDecimal(l.credit),
        dimensions: {
          branchId: p.branchId,
          propertyId: p.id,
          recordId: effect.recordId,
        },
        description: effect.source,
      })),
    },
    id,
    "posted",
  );
  await db.batch(prepared.steps);
  await run(
    db,
    "INSERT INTO pms_postings(id,property_id,source_key,record_id,journal_id,business_date) VALUES(?,?,?,?,?,?)",
    id,
    p.id,
    effect.source,
    effect.recordId,
    prepared.result.id,
    effect.date,
  );
  await run(db, "DELETE FROM command_assertions WHERE command_id=?", id);
  return prepared.result.id;
}
export async function effects(
  db: SqlDatabase,
  actor: WorkContext,
  s: Snapshot,
  e: Effects,
) {
  for (const effect of e.postings) await post(db, actor, s, effect);
  for (const move of e.stock) {
    const warehouse = await db
      .prepare(
        "SELECT id FROM commerce_warehouses WHERE id=? AND company_id=? AND branch_id=? AND status='active'",
      )
      .bind(move.warehouseId, s.property.companyId, s.property.branchId)
      .first();
    if (!warehouse)
      throw new ApiError(
        422,
        "PMS_WAREHOUSE_SCOPE",
        "Warehouse is not in this hotel",
      );
    const stock = await db
      .prepare(
        "SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=? FOR UPDATE",
      )
      .bind(move.warehouseId, move.productId)
      .first<{ quantity_milli: number; value_minor: number }>();
    const q = -move.quantityMilli;
    if (!stock || Number(stock.quantity_milli) < q)
      throw new ApiError(409, "PMS_STOCK_SHORT", "Insufficient inventory");
    if (
      await db
        .prepare(
          "SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>? LIMIT 1",
        )
        .bind(move.warehouseId, move.productId, move.date)
        .first()
    )
      throw new ApiError(
        409,
        "PMS_STOCK_BACKDATED",
        "Movement already exists after this business date",
      );
    const cost =
      q === Number(stock.quantity_milli)
        ? Number(stock.value_minor)
        : Number(
            (BigInt(stock.value_minor) * BigInt(q) +
              BigInt(stock.quantity_milli) / BigInt(2)) /
              BigInt(stock.quantity_milli),
          );
    await run(
      db,
      "UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?",
      q,
      cost,
      move.warehouseId,
      move.productId,
    );
    await run(
      db,
      "INSERT INTO pms_stock_moves(id,property_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at,source_number) VALUES(?,?,?,?,?,?,?,?,?)",
      move.id,
      s.property.id,
      move.warehouseId,
      move.productId,
      -q,
      -cost,
      move.date,
      Date.now(),
      move.description,
    );
    if (cost)
      await post(db, actor, s, {
        source: `stock:${move.id}`,
        recordId: move.id,
        date: move.date,
        lines: [
          { account: "consumption", debit: cost, credit: 0 },
          { account: "inventory", debit: 0, credit: cost },
        ],
      });
  }
  for (const source of e.sources)
    await run(
      db,
      "INSERT INTO pms_sources(property_id,source_key,record_id) VALUES(?,?,?)",
      s.property.id,
      source.key,
      source.id,
    );
  for (const doc of e.documents) await document(db, actor, s, doc);
  // Keep issued PMS receivables in Finance synchronized through the owner workflow.
  for (const f of records(s, "folio").filter(
    (r) => (r.data.invoiceId || records(s,"entry").some(entry=>entry.data.folioId===r.id&&entry.data.method==='opening'&&entry.data.kind==='charge')) && (e.postings.length || e.documents.length),
  )) {
    await syncAllocations(db, actor, s, f.id);
  }
}
async function document(
  db: SqlDatabase,
  actor: WorkContext,
  s: Snapshot,
  doc: Effects["documents"][number],
) {
  const p = s.property,
    c = p.config,
    f = record(s, "folio", doc.folioId),
    guest = record(s, "guest", f.data.guestId);
  const entries = records(s, "entry").filter((r) => r.data.folioId === f.id);
  const now = Date.now();
  let financeId: string | null = null;
  const party = f.data.partyId || `pms-guest-${guest.id}`;
  if (!f.data.partyId)
    await run(
      db,
      "INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,created_at,updated_at) VALUES(?,?,'customer',?,?,?,'active',?,?) ON CONFLICT(id) DO NOTHING",
      party,
      p.companyId,
      `PMS-${guest.id}`,
      guest.data.name,
      c.currency,
      now,
      now,
    );
  if (doc.kind === "invoice") {
    const bill = entries.filter((e) =>
      ["charge", "adjustment", "transfer"].includes(e.data.kind) && e.data.method!=="opening",
    );
    const total = bill.reduce((n, e) => n + e.data.total, 0),
      tax = bill.reduce((n, e) => n + e.data.tax, 0);
    const openingDue=entries.filter(entry=>entry.data.method==="opening"&&entry.data.kind==="charge").reduce((sum,entry)=>sum+entry.data.total,0);
    const currentDue=Math.min(total,Math.max(0,f.data.due-openingDue));
    if (total < 0 || tax < 0)
      throw new ApiError(
        409,
        "PMS_INVOICE_NEGATIVE",
        "Review adjustments before issuing the document",
      );
    const journal = await db
      .prepare(
        "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id IN (SELECT id FROM pms_records WHERE property_id=? AND kind='entry' AND data_json::jsonb->>'folioId'=?) AND source_key LIKE '%:charge' ORDER BY business_date,source_key LIMIT 1",
      )
      .bind(p.id, p.id, f.id)
      .first<{ journal_id: string }>();
    const credit = f.data.creditId
      ? record(s, "credit", f.data.creditId).data
      : null;
    await run(
      db,
      "INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,status,journal_id,created_by,approved_by,posted_at,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,?,?,?,?,?,?,?,?,'pms',?,?,?,?,?,?,?,?)",
      doc.id,
      p.companyId,
      party,
      `PMS-I-${doc.id}`,
      p.businessDate,
      plusDays(p.businessDate, credit?.terms ?? 0),
      c.currency,
      total - tax,
      tax,
      total,
      currentDue,
      c.accounts.receivable,
      c.accounts.revenue,
      c.accounts.tax,
      f.id,
      currentDue ? "posted" : "paid",
      journal?.journal_id ?? null,
      actor.userId,
      c.approvedBy,
      now,
      now,
      now,
    );
    financeId = doc.id;
  } else if (doc.kind === "receipt" || doc.kind === "refund") {
    const item = record(s, "entry", doc.entryId!).data;
    const posting = await db
      .prepare(
        "SELECT journal_id FROM pms_postings WHERE property_id=? AND record_id=? ORDER BY source_key LIMIT 1",
      )
      .bind(p.id, doc.entryId)
      .first<{ journal_id: string }>();
    await run(
      db,
      "INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,created_by,approved_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?)",
      doc.id,
      p.companyId,
      party,
      doc.kind === "refund" ? "refund" : "receipt",
      `PMS-R-${doc.id}`,
      p.businessDate,
      c.currency,
      doc.amount,
      item.method==='transfer'?c.accounts.bank:item.method==='cash'?c.accounts.cash:c.accounts.edc,
      posting?.journal_id ?? null,
      actor.userId,
      actor.userId,
      now,
      now,
    );
    financeId = doc.id;
  }
  await run(
    db,
    "INSERT INTO pms_documents(id,property_id,folio_id,kind,finance_id,snapshot_json,created_at) VALUES(?,?,?,?,?,?,?)",
    doc.id,
    p.id,
    f.id,
    doc.kind,
    financeId,
    JSON.stringify({
      property: {
        name: p.name,
        country: c.country,
        currency: c.currency,
        branchId: p.branchId,
      },
      guest: guest.data,
      folio: f.data,
      entries: doc.entryId
        ? entries.filter((e) => e.id === doc.entryId)
        : entries,
      amount: doc.amount,
      businessDate: p.businessDate,
      issuedBy: actor.userId,
      accountingRevision: c.revision,
    }),
    now,
  );
}
async function syncAllocations(
  db: SqlDatabase,
  actor: WorkContext,
  s: Snapshot,
  folioId: string,
) {
  const f = record(s, "folio", folioId);
  const opening=(await db.prepare(`SELECT b.finance_invoice_id id,b.amount_minor total FROM pms_import_balances b
    JOIN pms_records r ON r.id=b.entry_id AND r.property_id=b.property_id
    WHERE b.property_id=? AND r.data_json::jsonb->>'folioId'=? AND b.pocket='due' AND b.finance_invoice_id IS NOT NULL ORDER BY b.source_key`)
    .bind(s.property.id,folioId).all<{id:string;total:number}>()).results;
  if (!f.data.invoiceId && !opening.length) return;
  const entries = records(s, "entry").filter((e) => e.data.folioId === folioId),
    refunds = new Map<string, number>(),
    applied = new Map<string, number>();
  for (const e of entries) {
    const target =
      e.data.kind === "refund"
        ? refunds
        : e.data.reference === "deposit-application"
          ? applied
          : null;
    if (target)
      for (const part of e.data.funding ?? []) {
        if (
          e.data.kind === "refund" &&
          record(s, "entry", part.entryId).data.kind === "deposit" &&
          record(s, "refund", e.data.originalId).data.pocket === "deposit"
        )
          continue;
        target.set(part.entryId, (target.get(part.entryId) ?? 0) + part.amount);
      }
  }
  const receipts = await query<{ id: string; snapshot_json: string }>(
    db,
    "SELECT COALESCE(finance_id,id) id,snapshot_json FROM pms_documents WHERE property_id=? AND folio_id=? AND (kind='receipt' OR (kind='opening' AND finance_id IS NOT NULL)) ORDER BY created_at,id",
    s.property.id,
    folioId,
  );
  let remaining = Math.max(
    0,
    entries
      .filter((e) => ["charge", "adjustment", "transfer"].includes(e.data.kind))
      .reduce((n, e) => n + e.data.total, 0) - f.data.due,
  );
  const currentTotal=entries.filter(e=>["charge","adjustment","transfer"].includes(e.data.kind)&&e.data.method!=="opening").reduce((sum,e)=>sum+e.data.total,0);
  const targets=[...opening,...(f.data.invoiceId?[{id:f.data.invoiceId,total:currentTotal}]:[])].map(target=>{
    const paid=Math.min(remaining,Number(target.total));remaining-=paid;
    return {...target,total:Number(target.total),paid,remaining:paid};
  });
  for(const target of targets){
    await run(db,"DELETE FROM finance_allocations WHERE invoice_id=? AND company_id=?",target.id,s.property.companyId);
    const outstanding=target.total-target.paid;
    await run(db,"UPDATE finance_invoices SET outstanding_minor=?,base_outstanding_minor=?,status=CASE WHEN ?=0 THEN 'paid' WHEN ?<total_minor THEN 'partially_paid' ELSE 'posted' END,version=version+1,updated_at=? WHERE id=? AND company_id=?",outstanding,outstanding,outstanding,outstanding,Date.now(),target.id,s.property.companyId);
  }
  for (const r of receipts) {
    const source = JSON.parse(r.snapshot_json).entries?.[0];
    if (!source||!["payment","deposit"].includes(source.data.kind)) continue;
    const available =
      source.data.kind === "deposit"
        ? (applied.get(source.id) ?? 0)
        : Number(source.data.total);
    let budget=Math.max(0, available - (refunds.get(source.id) ?? 0));
    for(const target of targets){
      const n=Math.min(target.remaining,budget);
      if (n <= 0) continue;
      await run(
        db,
        "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)",
        randomUUID(),
        s.property.companyId,
        r.id,
        target.id,
        n,
        actor.userId,
        Date.now(),
      );
      target.remaining -= n;budget-=n;
    }
  }
  if (targets.some(target=>target.remaining>0))
    throw new ApiError(
      409,
      "PMS_ALLOCATION_TRACE",
      "Allocation does not match payment evidence",
    );
}

async function validateReferences(
  db: SqlDatabase,
  s: Snapshot,
  operation: string,
  input: unknown,
) {
  const value = input as Record<string, unknown> | null;
  if (operation === "task.assign" && value?.assigneeId) {
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?))",
        )
        .bind(
          value.assigneeId,
          s.property.companyId,
          Date.now(),
          Date.now(),
          s.property.branchId,
        )
        .first())
    )
      throw new ApiError(
        422,
        "PMS_ASSIGNEE_SCOPE",
        "Owner is not authorized for this hotel",
      );
  }
  for (const partyId of new Set(
    [
      ...records(s, "rate").map((r) => r.data.partyId),
      ...records(s, "credit").map((r) => r.data.partyId),
    ].filter(Boolean),
  )) {
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active' AND currency=? AND person_id IS NULL",
        )
        .bind(partyId, s.property.companyId, s.property.config.currency)
        .first())
    )
      throw new ApiError(
        422,
        "PMS_PARTY_INVALID",
        "Company/agent must be an active customer in this company",
      );
  }
  for (const item of records(s, "item"))
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM commerce_products p JOIN commerce_warehouses w ON w.id=? WHERE p.id=? AND p.company_id=? AND w.company_id=p.company_id AND w.branch_id=? AND p.status='active' AND w.status='active'",
        )
        .bind(
          item.data.warehouseId,
          item.data.productId,
          s.property.companyId,
          s.property.branchId,
        )
        .first())
    )
      throw new ApiError(
        422,
        "PMS_ITEM_SCOPE",
        "Product and warehouse must belong to this company and branch",
      );
}
export async function pmsCommand(
  db: TransactionalSqlDatabase,
  actor: WorkContext,
  raw: unknown,
  key: string,
  worker = false,
) {
  const command = commandSchema.parse(raw),
    op = command.operation;
  if (op === "charge.import_pos")
    throw new ApiError(403, "PMS_INTERNAL_COMMAND", "Restaurant charges must come from a POS transaction");
  if (process.env.NODE_ENV === 'production' && (op === 'payment.receive' || op === 'payment.deposit' || op === 'refund.pay')) {
    const method = z.object({ method: z.string() }).parse(command.input).method;
    if (method !== 'cash')
      throw new ApiError(403, 'PMS_BANK_VERIFICATION_REQUIRED', 'Hotel bank and card payments or refunds require verified evidence before posting');
  }
  requirePermission(actor, operationPermission(op));
  if (op === "accounting.bootstrap") requirePermission(actor, "finance.write");
  if (op === "night.step" && !worker)
    throw new ApiError(
      403,
      "PMS_WORKER_ONLY",
      "This command is only used by day-close work",
    );
  if (actor.departmentId || actor.beta)
    throw new ApiError(
      403,
      "PMS_BRANCH_REQUIRED",
      "Select a company or branch area",
    );
  try {
    return await db.transaction(async (tx) => {
      await assertProductionModuleEnabled(tx, actor.companyId, 'pms');
      await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
      const target =
        op === "property.create"
          ? `property-${createHash("sha256").update(key).digest("hex").slice(0, 32)}`
          : (command.propertyId ??
            `property-${createHash("sha256").update(key).digest("hex").slice(0, 32)}`);
      const old = await replayCommand<{
        id: string;
        propertyId: string;
        version: number;
      }>(tx, actor, key, `pms.${op}`, target, command);
      if (old) return old;
      let s: Snapshot;
      if (op === "property.create") {
        const v = z
          .object({
            name: z.string().trim().min(1).max(200),
            branchId: idField,
            businessDate: dateField,
          })
          .parse(command.input);
        if (actor.branchId && actor.branchId !== v.branchId)
          throw new ApiError(404, "PMS_BRANCH_NOT_FOUND", "Branch not found");
        if (
          !(await tx
            .prepare(
              "SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active' FOR SHARE",
            )
            .bind(v.branchId, actor.companyId)
            .first())
        )
          throw new ApiError(
            422,
            "PMS_BRANCH_NOT_FOUND",
            "Select a branch of this company",
          );
        s = newProperty(
          target,
          actor.companyId,
          v.branchId,
          v.name,
          v.businessDate,
        );
        await run(
          tx,
          "INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,?,?,?,?,?)",
          target,
          actor.companyId,
          v.branchId,
          v.name,
          v.businessDate,
          JSON.stringify(s.property.config),
          actor.userId,
          Date.now(),
        );
      } else {
        if (!command.propertyId)
          throw new ApiError(422, "PMS_PROPERTY_REQUIRED", "Select a hotel");
        s = await loadPms(tx, actor, command.propertyId, true);
        if (op === "charge.reverse") {
          const entryId = z.object({ id: idField }).parse(command.input).id;
          const entry = records(s, "entry").find((r) => r.id === entryId);
          if (entry?.data.sourceKey?.startsWith("pos:"))
            throw new ApiError(409, "PMS_POS_REVERSAL_REQUIRED", "Reverse restaurant charges from the linked POS bill");
        }
        if (!worker && s.property.version !== command.expectedVersion)
          throw new ApiError(
            409,
            "PMS_VERSION_CONFLICT",
            "Hotel data changed. Load the latest version and review again",
          );
      }
      if (op === "charge.reverse")
        throw new ApiError(409,"PMS_CORRECTION_REQUEST_REQUIRED","Request a charge correction and have another authorized person review it.");
      if (op === "charge.correction.request") {
        const input=z.object({id:idField,reason:z.string().trim().min(3).max(2000)}).strict().parse(command.input);
        if(s.property.state!=="open")throw new ApiError(409,"PMS_DAY_CLOSING","Charge corrections require an open hotel day.");
        const entry=records(s,"entry").find(row=>row.id===input.id);
        if(!entry||!["charge","transfer"].includes(entry.data.kind)||entry.data.total<=0)
          throw new ApiError(409,"PMS_CORRECTION_ENTRY_INVALID","Only a positive hotel folio charge or transfer can be corrected.");
        if(entry.data.sourceKey?.startsWith("pos:"))
          throw new ApiError(409,"PMS_POS_REVERSAL_REQUIRED","Reverse restaurant charges from the linked POS bill.");
        if(records(s,"entry").some(row=>row.data.originalId===entry.id))
          throw new ApiError(409,"PMS_ALREADY_REVERSED","This folio entry already has a reversal.");
        if(await tx.prepare("SELECT 1 FROM pms_charge_corrections WHERE entry_id=? AND status='pending'").bind(entry.id).first())
          throw new ApiError(409,"PMS_CORRECTION_ALREADY_PENDING","Another correction request for this entry is awaiting review.");
        const id=`pms-correction-${createHash("sha256").update(key).digest("hex").slice(0,32)}`;
        const now=Date.now(),nextVersion=s.property.version+1;
        const result={id,propertyId:s.property.id,version:nextVersion,status:"pending",correctionVersion:1};
        return executeCommand(tx,actor,key,`pms.${op}`,command,target,nextVersion,result,[
          tx.prepare(`INSERT INTO pms_charge_corrections(id,company_id,property_id,entry_id,folio_id,status,version,reason,requested_by,created_at,updated_at)
            VALUES(?,?,?,?,?,'pending',1,?,?,?,?)`).bind(id,actor.companyId,s.property.id,entry.id,entry.data.folioId,input.reason,actor.userId,now,now),
          tx.prepare(`INSERT INTO pms_charge_correction_events(id,correction_id,company_id,property_id,entry_id,action,actor_id,correction_version,reason,created_at)
            VALUES(?,?,?,?,?,'requested',?,1,?,?)`).bind(`pms-correction-event-${key}`,id,actor.companyId,s.property.id,entry.id,actor.userId,input.reason,now),
          tx.prepare("UPDATE pms_properties SET version=?,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(nextVersion,now,s.property.id,actor.companyId,s.property.version),
        ],{companyId:s.property.companyId,branchId:s.property.branchId,departmentId:null});
      }
      if (op === "charge.correction.review") {
        const input=z.object({id:idField,expectedVersion:z.number().int().positive(),action:z.enum(["approve","reject"]),reason:z.string().trim().min(3).max(2000)}).strict().parse(command.input);
        if(s.property.state!=="open")throw new ApiError(409,"PMS_DAY_CLOSING","Review charge corrections before closing the hotel day.");
        const correction=await tx.prepare("SELECT * FROM pms_charge_corrections WHERE id=? AND company_id=? AND property_id=? FOR UPDATE").bind(input.id,actor.companyId,s.property.id).first<ChargeCorrectionRow>();
        if(!correction)throw new ApiError(404,"PMS_CORRECTION_NOT_FOUND","Charge correction request not found in this hotel.");
        if(correction.status!=="pending"||Number(correction.version)!==input.expectedVersion)
          throw new ApiError(409,"PMS_CORRECTION_CHANGED","This correction request changed. Reload before deciding.");
        if(samePerson(actor,correction.requested_by))
          throw new ApiError(403,"PMS_INDEPENDENT_REVIEW_REQUIRED","A different person must review the charge correction.");
        const entry=records(s,"entry").find(row=>row.id===correction.entry_id);
        if(!entry||entry.data.folioId!==correction.folio_id||entry.data.sourceKey?.startsWith("pos:")||records(s,"entry").some(row=>row.data.originalId===correction.entry_id))
          throw new ApiError(409,"PMS_CORRECTION_SOURCE_CHANGED","The folio entry changed or already has a reversal.");
        const propertyVersion=s.property.version,now=Date.now(),correctionVersion=Number(correction.version)+1;
        let reversalEntryId:string|null=null,propertyVersionAfter=propertyVersion;
        if(input.action==="approve"){
          const original=new Map(s.records.map(row=>[row.id,JSON.stringify(row)]));
          let outcome;
          try{outcome=executePms(s,"charge.reverse",{id:entry.id,reason:input.reason},{userId:actor.userId,now,uuid:randomUUID,independent:other=>!samePerson(actor,other)});}
          catch(error){if(error instanceof PmsError)throw new ApiError(error.status,error.code,error.message);throw error;}
          reversalEntryId=outcome.id;propertyVersionAfter=s.property.version;
          await validSetup(tx,s);await persistPmsRecords(tx,s,original);await effects(tx,actor,s,outcome.effects);
        }else propertyVersionAfter++;
        const result={id:correction.id,propertyId:s.property.id,version:propertyVersionAfter,status:input.action==="approve"?"approved":"rejected",correctionVersion,reversalEntryId};
        return executeCommand(tx,actor,key,`pms.${op}`,command,target,propertyVersionAfter,result,[
          tx.prepare(`UPDATE pms_charge_corrections SET status=?,version=?,reviewed_by=?,review_reason=?,reversal_entry_id=?,reviewed_at=?,updated_at=?
            WHERE id=? AND company_id=? AND property_id=? AND status='pending' AND version=?`)
            .bind(result.status,correctionVersion,actor.userId,input.reason,reversalEntryId,now,now,correction.id,actor.companyId,s.property.id,input.expectedVersion),
          tx.prepare(`INSERT INTO pms_charge_correction_events(id,correction_id,company_id,property_id,entry_id,action,actor_id,correction_version,reason,reversal_entry_id,created_at)
            VALUES(?,?,?,?,?,?,?,?,?,?,?)`).bind(`pms-correction-event-${key}`,correction.id,actor.companyId,s.property.id,correction.entry_id,result.status,actor.userId,correctionVersion,input.reason,reversalEntryId,now),
          tx.prepare("UPDATE pms_properties SET version=?,updated_at=? WHERE id=? AND company_id=? AND version=?").bind(propertyVersionAfter,now,s.property.id,actor.companyId,propertyVersion),
        ],{companyId:s.property.companyId,branchId:s.property.branchId,departmentId:null});
      }
      let attachmentBinding: Awaited<ReturnType<typeof prepareAttachmentBinding>> | null = null;
      if (command.attachmentSetIds.length) {
        if (!["task.complete", "lost.release", "ota.settle"].includes(op))
          throw new ApiError(
            422,
            "PMS_ATTACHMENT_ACTION",
            "Files can be bound only to room-task completion, lost-property release, or OTA settlement.",
          );
        const targetId = op === "ota.settle"
          ? s.property.id
          : z.object({ id: idField }).parse(command.input).id;
        const targetRecord = op === "task.complete"
          ? records(s, "task").find((row) => row.id === targetId)
          : op === "lost.release"
            ? records(s, "lost").find((row) => row.id === targetId)
            : undefined;
        const descriptor = op === "task.complete"
          ? {
              targetType: "pms_task",
              purposes: new Set(["room_photo", "damage_photo"]),
              bindingType: "pms_task_completion",
              expectedTargetVersion: targetRecord?.version,
            }
          : op === "lost.release"
            ? {
                targetType: "pms_lost_property",
                purposes: new Set(["handover_evidence","lost_property_photo"]),
                bindingType: "pms_lost_property_release",
                expectedTargetVersion: targetRecord?.version,
              }
            : op === "ota.settle"
              ? {
                  targetType: "pms_property",
                  purposes: new Set(["ota_reconciliation"]),
                  bindingType: "pms_ota_settlement",
                  expectedTargetVersion: s.property.version,
                }
              : null;
        if (!descriptor)
          throw new ApiError(
            422,
            "PMS_ATTACHMENT_ACTION",
            "Files can be bound only to room-task completion, lost-property release, or OTA settlement.",
          );
        if (descriptor.expectedTargetVersion === undefined)
          throw new ApiError(404, "PMS_ATTACHMENT_TARGET", "Attachment target not found in this hotel");
        const placeholders = command.attachmentSetIds.map(() => "?").join(",");
        const sets = (
          await tx
            .prepare(
              `SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`,
            )
            .bind(actor.companyId, ...command.attachmentSetIds)
            .all<{ id: string; purpose_key: string; component_kind: string; component_ref: string | null }>()
        ).results;
        if (
          sets.length !== new Set(command.attachmentSetIds).size ||
          sets.some(
            (set) =>
              set.component_kind !== "action" ||
              set.component_ref !== op ||
              !descriptor.purposes.has(set.purpose_key),
          )
        )
          throw new ApiError(
            422,
            "PMS_ATTACHMENT_PURPOSE",
            "Choose only evidence created for this PMS action.",
          );
        attachmentBinding = await prepareAttachmentBinding(tx, actor, {
          attachmentSetIds: command.attachmentSetIds,
          bindingContract: {
            targetType: descriptor.targetType,
            bindingType: descriptor.bindingType,
            contexts: [...descriptor.purposes].map(purposeKey => ({ purposeKey, componentKind: "action" as const, componentRef: op })),
          },
          targetId,
          expectedTargetVersion: descriptor.expectedTargetVersion,
          commandId: key,
          bindingId: `${targetId}:${op}:${descriptor.expectedTargetVersion + 1}`,
        });
        await tx.batch(attachmentBinding.guards);
      }
      if (op === 'payment.request_transfer') {
        if (s.property.state !== 'open') throw new ApiError(409,'PMS_DAY_CLOSING','The hotel day is not open for a new transfer');
        const v=z.object({folioId:idField,shiftId:idField,amount:z.number().int().positive().max(99999999999),reference:z.string().trim().min(2).max(150),kind:z.enum(['receipt','deposit']).default('receipt')}).parse(command.input);
        const folio=records(s,'folio').find(row=>row.id===v.folioId);
        if (!folio || folio.data.state==='closed' || folio.data.invoiceId) throw new ApiError(409,'PMS_FOLIO_CLOSED','The selected folio cannot receive a transfer');
        const shift=records(s,'shift').find(row=>row.id===v.shiftId);
        if (!shift || shift.data.state!=='open' || shift.data.userId!==actor.userId || shift.data.date!==s.property.businessDate)
          throw new ApiError(409,'PMS_SHIFT_REQUIRED','Open your hotel cashier shift before recording a transfer');
        const id=randomUUID(),now=Date.now();
        const result={id,propertyId:s.property.id,version:s.property.version,state:'pending',transferVersion:1};
        return executeCommand(tx,actor,key,`pms.${op}`,command,target,s.property.version,result,[
          tx.prepare(`INSERT INTO pms_transfer_requests(id,company_id,property_id,folio_id,shift_id,kind,business_date,currency,amount_minor,reference,state,recorded_by,created_at,updated_at)
            VALUES(?,?,?,?,?,?,?,?,?,?,'pending',?,?,?)`).bind(id,actor.companyId,s.property.id,v.folioId,v.shiftId,v.kind,s.property.businessDate,s.property.config.currency,v.amount,v.reference,actor.userId,now,now),
        ],{companyId:s.property.companyId,branchId:s.property.branchId,departmentId:null});
      }
      if (op === 'payment.cancel_transfer') {
        const v=z.object({id:idField,expectedTransferVersion:z.number().int().positive()}).parse(command.input);
        const row=await tx.prepare('SELECT * FROM pms_transfer_requests WHERE id=? AND company_id=? AND property_id=? FOR UPDATE').bind(v.id,actor.companyId,s.property.id).first<TransferRequestRow>();
        if (!row) throw new ApiError(404,'PMS_TRANSFER_NOT_FOUND','Transfer request not found in this hotel');
        if (Number(row.version)!==v.expectedTransferVersion || row.state!=='pending') throw new ApiError(409,'PMS_TRANSFER_CHANGED','Transfer state changed. Reload before cancelling');
        if (row.recorded_by!==actor.userId && !hasPermission(actor,'pms.approve')) throw new ApiError(403,'PMS_TRANSFER_OWNER','Only the recording cashier or a hotel manager can cancel this request');
        const result={id:row.id,propertyId:s.property.id,version:s.property.version,state:'cancelled',transferVersion:Number(row.version)+1};
        return executeCommand(tx,actor,key,`pms.${op}`,command,target,s.property.version,result,[
          tx.prepare("UPDATE pms_transfer_requests SET state='cancelled',cancelled_by=?,version=version+1,updated_at=? WHERE id=? AND state='pending' AND version=?").bind(actor.userId,Date.now(),row.id,row.version),
        ],{companyId:s.property.companyId,branchId:s.property.branchId,departmentId:null});
      }
      let verifiedTransfer:TransferRequestRow|null=null;
      let matchedBankId:string|null=null;
      let engineOp=op,engineInput=command.input;
      if(op === "accounting.bootstrap") {
        if(records(s,"entry").length) throw new ApiError(409,"PMS_ACCOUNTING_IN_USE","Accounting is frozen after financial activity.");
        engineInput=await preparePmsAccounting(tx,actor,s,command.input);
        engineOp="accounting.save";
      }
      if(op==='reservation.create' && command.input && typeof command.input==='object'){
        const requestedCurrency=(command.input as Record<string,unknown>).currency;
        if(typeof requestedCurrency==='string'){
          if(requestedCurrency!==s.property.config.currency)throw new ApiError(422,'PMS_CURRENCY_SETUP_REQUIRED',`The selected ${requestedCurrency} currency needs a hotel rate plan and supported ledger/payment accounts before booking.`);
          if(requestedCurrency!=='USD'&&!await tx.prepare('SELECT id FROM company_fx_rates WHERE company_id=? AND currency=? ORDER BY effective_date DESC,confirmed_at DESC LIMIT 1').bind(actor.companyId,requestedCurrency).first())
            throw new ApiError(422,'MANUAL_RATE_REQUIRED',`Confirm a manual USD/${requestedCurrency} rate before booking.`);
        }
        const campaignCode=(command.input as Record<string,unknown>).campaignCode;
        if(typeof campaignCode==='string' && campaignCode.trim()){
          const campaign=await tx.prepare('SELECT 1 FROM marketing_campaigns WHERE company_id=? AND code=?').bind(actor.companyId,campaignCode.trim()).first();
          if(!campaign)throw new ApiError(422,'PMS_CAMPAIGN_UNKNOWN','Select a campaign code registered for this company.');
        }
      }
      if (op === 'payment.verify_transfer') {
        if (s.property.state!=='open') throw new ApiError(409,'PMS_DAY_CLOSING','The hotel day must be open to verify a transfer');
        const v=z.object({id:idField,expectedTransferVersion:z.number().int().positive(),bankTransactionId:idField,bankVersion:z.number().int().positive()}).parse(command.input);
        const row=await tx.prepare('SELECT * FROM pms_transfer_requests WHERE id=? AND company_id=? AND property_id=? FOR UPDATE').bind(v.id,actor.companyId,s.property.id).first<TransferRequestRow>();
        if (!row) throw new ApiError(404,'PMS_TRANSFER_NOT_FOUND','Transfer request not found in this hotel');
        if (Number(row.version)!==v.expectedTransferVersion || row.state!=='pending') throw new ApiError(409,'PMS_TRANSFER_CHANGED','Transfer state changed. Reload before verifying');
        if (samePerson(actor,row.recorded_by)) throw new ApiError(403,'PMS_INDEPENDENT_VERIFICATION','A different Finance user must verify the bank transfer');
        const bank=await tx.prepare(`SELECT t.*,b.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements b ON b.id=t.statement_id
          WHERE t.id=? AND t.company_id=? FOR UPDATE OF t`).bind(v.bankTransactionId,actor.companyId).first<{id:string;version:number;match_status:string;bank_account:string;currency:string;amount_minor:number;external_id:string;transaction_date:string}>();
        if (!bank || Number(bank.version)!==v.bankVersion || !['unmatched','proposed'].includes(bank.match_status)
          || bank.bank_account!==s.property.config.accounts.bank || bank.currency!==row.currency
          || Number(bank.amount_minor)!==Number(row.amount_minor) || bank.external_id.trim()!==row.reference
          || bank.transaction_date!==row.business_date || s.property.businessDate!==row.business_date)
          throw new ApiError(409,'PMS_BANK_EVIDENCE_MISMATCH','Match the exact imported bank account, reference, business date, currency, and amount');
        verifiedTransfer=row;matchedBankId=bank.id;
        engineOp=row.kind==='deposit'?'payment.deposit':'payment.receive';
        engineInput={folioId:row.folio_id,shiftId:row.shift_id,amount:Number(row.amount_minor),method:'transfer',reference:row.reference};
      }
      if(op==='night.start') {
        const controls=await hospitalityDayControls(tx,actor.companyId,s.property.branchId,s.property.id,s.property.businessDate);
        if(controls.blockers.length) throw new ApiError(409,'PMS_DAY_RECONCILIATION_BLOCKED',controls.blockers.join(' · '));
      }
      const original = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
      const outcome =
        op === "property.create"
          ? {
              id: s.property.id,
              effects: {
                postings: [],
                stock: [],
                documents: [],
                sources: [],
              } as Effects,
            }
          : executePms(s, engineOp, engineInput, {
              userId: actor.userId,
              now: Date.now(),
              uuid: randomUUID,
              independent: (other) => !samePerson(actor, other),
              verifiedBank:Boolean(verifiedTransfer),
            });
      if (op === "booking.save" && s.property.config.booking?.published) {
        if (!process.env.RESEND_API_KEY || !process.env.RESEND_FROM)
          throw new ApiError(
            422,
            "PMS_EMAIL_REQUIRED",
            "Configure booking email delivery before publishing",
          );
      }
      await validateReferences(tx, s, engineOp, engineInput);
      if (
        [
          "accounting.save",
          "accounting.bootstrap",
          "accounting.approve",
          "property.enable",
          "night.start",
          "night.resume",
        ].includes(op) ||
        outcome.effects.postings.length ||
        outcome.effects.stock.length
      )
        await validSetup(tx, s);
      if (op === "booking.retry_email") {
        const emailId = z.object({ id: idField }).parse(command.input).id;
        const retry = await tx
          .prepare(
            "UPDATE pms_booking_emails SET state='pending',attempts=0,next_attempt_at=?,lease_until=NULL,last_error=NULL WHERE id=? AND property_id=? AND state='failed' RETURNING id",
          )
          .bind(Date.now(), emailId, s.property.id)
          .first();
        if (!retry)
          throw new ApiError(
            409,
            "PMS_EMAIL_NOT_FAILED",
            "Only failed hotel emails can be retried",
          );
      }
      await persistPmsRecords(tx, s, original);
      if(op==='reservation.create'&&records(s,'reservation').some(row=>row.id===outcome.id&&row.data.state==='confirmed')){
        const total=records(s,'stay').filter(row=>row.data.reservationId===outcome.id)
          .reduce((sum,row)=>sum+row.data.prices.reduce((nightSum,night)=>nightSum+night.total,0),0);
        const evidence=await snapshotMoney(tx,actor,{sourceType:'pms_reservation',sourceId:outcome.id,amountKind:'room_total',
          transactionDate:s.property.businessDate,currency:s.property.config.currency,currencyScale:2,amountUnits:String(total)});
        await evidence.statement.run();
      }
      if (op === "folio.merge") {
        const v = command.input as { id: string; toFolioId: string };
        await run(
          tx,
          "UPDATE pms_documents SET folio_id=? WHERE property_id=? AND folio_id=?",
          v.toFolioId,
          s.property.id,
          v.id,
        );
      }
      await effects(tx, actor, s, outcome.effects);
      if (verifiedTransfer && matchedBankId) {
        const receipt=outcome.effects.documents.find(doc=>doc.kind==='receipt' && doc.entryId===outcome.id);
        if (!receipt) throw new ApiError(409,'PMS_RECEIPT_MISSING','The verified transfer did not produce a receipt');
        await run(tx,"UPDATE pms_transfer_requests SET state='verified',verified_by=?,bank_transaction_id=?,entry_id=?,finance_payment_id=?,version=version+1,updated_at=? WHERE id=? AND state='pending'",actor.userId,matchedBankId,outcome.id,receipt.id,Date.now(),verifiedTransfer.id);
        await run(tx,"UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=? AND match_status IN ('unmatched','proposed')",receipt.id,matchedBankId);
      }
      await run(
        tx,
        "UPDATE pms_properties SET name=?,business_date=?,state=?,config_json=?,version=?,updated_at=? WHERE id=?",
        s.property.name,
        s.property.businessDate,
        s.property.state,
        JSON.stringify(s.property.config),
        s.property.version,
        Date.now(),
        s.property.id,
      );
      if (outcome.effects.nightJob)
        await run(
          tx,
          "INSERT INTO pms_jobs(id,property_id,business_date,actor_json,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(property_id,business_date) DO UPDATE SET state='pending',actor_json=excluded.actor_json,error_code=NULL,updated_at=excluded.updated_at",
          outcome.effects.nightJob,
          s.property.id,
          s.property.businessDate,
          JSON.stringify(actor),
          Date.now(),
        );
      if (op === "night.step")
        await run(
          tx,
          "UPDATE pms_jobs SET state=?,error_code=NULL,updated_at=? WHERE id=?",
          s.property.state === "open" ? "completed" : "pending",
          Date.now(),
          outcome.id,
        );
      const result = verifiedTransfer
        ? {id:verifiedTransfer.id,propertyId:s.property.id,version:s.property.version,state:'verified',transferVersion:Number(verifiedTransfer.version)+1,entryId:outcome.id,bankTransactionId:matchedBankId}
        : {id:outcome.id,propertyId:s.property.id,version:s.property.version};
      return executeCommand(
        tx,
        actor,
        key,
        `pms.${op}`,
        command,
        target,
        s.property.version,
        result,
        attachmentBinding?.steps ?? [],
        {
          companyId: s.property.companyId,
          branchId: s.property.branchId,
          departmentId: null,
        },
      );
    });
  } catch (error) {
    if (error instanceof PmsError)
      throw new ApiError(error.status, error.code, error.message);
    if (
      ["23505", "23514", "40001", "40P01"].includes(
        (error as { code?: string }).code ?? "",
      )
    )
      throw new ApiError(
        409,
        "PMS_CONFLICT",
        "Data or balances changed. Load the latest version",
      );
    throw error;
  }
}
export async function drainPms(db: TransactionalSqlDatabase, propertyId?: string, jobId?: string) {
  const jobs = await query<{
    id: string;
    property_id: string;
    actor_json: string;
  }>(
    db,
    "SELECT id,property_id,actor_json FROM pms_jobs WHERE state IN ('pending','running') AND (?::text IS NULL OR property_id=?) AND (?::text IS NULL OR id=?) ORDER BY updated_at LIMIT 4",
    propertyId ?? null, propertyId ?? null, jobId ?? null, jobId ?? null,
  );
  for (const job of jobs) {
    try {
      const actor = JSON.parse(job.actor_json) as WorkContext;
      // User-originated jobs retain their original authority. Session/role revocation
      // blocks posting; a current Night Auditor can explicitly resume the frozen day.
      await pmsCommand(
        db,
        actor,
        {
          operation: "night.step",
          propertyId: job.property_id,
          input: { id: job.id },
        },
        randomUUID(),
        true,
      );
    } catch (error) {
      if (error instanceof ApiError && error.code === "PMS_CONFLICT") continue;
      const code = error instanceof ApiError ? error.code : "PMS_JOB_FAILED";
      await db.transaction(async (tx) => {
        await tx
          .prepare("SELECT id FROM pms_properties WHERE id=? FOR UPDATE")
          .bind(job.property_id)
          .first();
        await run(
          tx,
          "UPDATE pms_jobs SET state='blocked',error_code=?,updated_at=? WHERE id=? AND state!='completed'",
          code,
          Date.now(),
          job.id,
        );
        await run(
          tx,
          "UPDATE pms_records SET data_json=jsonb_set(jsonb_set(data_json::jsonb,'{state}','\"blocked\"'),'{message}',to_jsonb(?::text))::text,version=version+1,updated_at=? WHERE id=? AND data_json::jsonb->>'state'!='completed'",
          code,
          Date.now(),
          job.id,
        );
      });
    }
  }
}
export async function pmsWork(actor: WorkContext): Promise<WorkItem[]> {
  if (!hasPermission(actor, "pms.read") || actor.departmentId || actor.beta) return [];
  const { write } = await holdingResources(actor.holdingId);
  const { pmsOperationalWork } = await import("./pms/home");
  return pmsOperationalWork(write, actor);
}

/** Called inside the POS command transaction: no second command or ledger owner. */
export async function applyPosRoomCharge(
  tx: SqlDatabase,
  actor: WorkContext,
  raw: z.infer<typeof roomChargeSchema>,
  expectedPropertyVersion: number,
) {
  await assertProductionModuleEnabled(tx, actor.companyId, 'pms');
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const s = await loadPms(tx, actor, raw.propertyId, true);
  if (s.property.version !== expectedPropertyVersion)
    throw new ApiError(409, "PMS_VERSION_CONFLICT", "Hotel data changed. Load the latest stay and review again");
  let prepared;
  try { prepared = prepareRoomCharge(s, raw); }
  catch (error) { if (error instanceof PmsError) throw new ApiError(error.status, error.code, error.message); throw error; }
  if (prepared.duplicate) return { entryId: prepared.entryId, propertyVersion: s.property.version };
  const original = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
  let outcome;
  try { outcome = executePms(s, "charge.import_pos", {
    folioId: prepared.folioId,
    stayId: raw.stayId,
    sourceKey: prepared.sourceKey,
    description: raw.description,
    net: raw.net,
    service: raw.service,
    tax: raw.tax,
    total: raw.total,
  }, {
    userId: actor.userId,
    now: Date.now(),
    uuid: randomUUID,
    independent: (other) => !samePerson(actor, other),
  }); }
  catch (error) { if (error instanceof PmsError) throw new ApiError(error.status, error.code, error.message); throw error; }
  await validSetup(tx, s);
  await persistPmsRecords(tx, s, original);
  await effects(tx, actor, s, outcome.effects);
  await run(tx,
    "UPDATE pms_properties SET version=?,updated_at=? WHERE id=?",
    s.property.version, Date.now(), s.property.id);
  return { entryId: outcome.id, propertyVersion: s.property.version };
}

/** POS owns the correction, so the folio reversal shares its transaction. */
export async function reversePosRoomCharge(
  tx: SqlDatabase,
  actor: WorkContext,
  input: { propertyId: string; sourceKey: string; expectedPropertyVersion: number; reason: string; total: number },
) {
  await tx.prepare("SELECT set_config('erpx.pms_write','on',true)").first();
  const s = await loadPms(tx, actor, input.propertyId, true);
  if (s.property.version !== input.expectedPropertyVersion)
    throw new ApiError(409, "PMS_VERSION_CONFLICT", "Hotel data changed. Load the latest folio and review again");
  const entry = records(s, "entry").find((r) => r.data.sourceKey === input.sourceKey);
  if (!entry || entry.data.total !== input.total)
    throw new ApiError(409, "PMS_CHARGE_CHANGED", "The linked hotel charge changed. Review the bill again");
  const original = new Map(s.records.map((r) => [r.id, JSON.stringify(r)]));
  let outcome;
  try { outcome = executePms(s, "charge.reverse", { id: entry.id, reason: input.reason }, {
    userId: actor.userId, now: Date.now(), uuid: randomUUID,
    independent: (other) => !samePerson(actor, other),
  }); }
  catch (error) { if (error instanceof PmsError) throw new ApiError(error.status, error.code, error.message); throw error; }
  await validSetup(tx, s);
  await persistPmsRecords(tx, s, original);
  await effects(tx, actor, s, outcome.effects);
  await run(tx, "UPDATE pms_properties SET version=?,updated_at=? WHERE id=?", s.property.version, Date.now(), s.property.id);
  return { entryId: entry.id, reversalEntryId: outcome.id, businessDate: s.property.businessDate };
}

export async function persistPmsRecords(
  tx: SqlDatabase,
  s: Snapshot,
  original: Map<string, string>,
) {
  for (const r of s.records)
    if (original.get(r.id) !== JSON.stringify(r))
      await run(
        tx,
        "INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET data_json=excluded.data_json,version=excluded.version,updated_at=excluded.updated_at",
        r.id,
        s.property.id,
        r.kind,
        JSON.stringify(r.data),
        r.version,
        r.updatedAt,
      );
  await capturePmsMoneyRecords(tx, s, original);
  // Provider confirmation can persist several folios in one transaction. The
  // next flush compares with this committed-in-transaction state, not its first
  // in-memory snapshot, so earlier source evidence is never inserted twice.
  for (const row of s.records) original.set(row.id, JSON.stringify(row));
}

/** Owner rows and shared money evidence commit together, including imports and workers. */
async function capturePmsMoneyRecords(tx: SqlDatabase, s: Snapshot, original: Map<string, string>) {
  const company = { companyId: s.property.companyId };
  const capture = async (sourceType: string, sourceId: string, amountKind: string, amount: number, date = s.property.businessDate) => {
    const value = await snapshotMoney(tx, company, { sourceType, sourceId, amountKind, amountUnits: String(amount),
      transactionDate: date, currency: s.property.config.currency, currencyScale: s.property.config.minorUnit });
    await value.statement.run();
  };
  // Versioned prices remain the quote selected when booked or explicitly repriced.
  for (const stay of records(s, "stay")) {
    const previous = original.has(stay.id) ? JSON.parse(original.get(stay.id)!).data.prices : undefined;
    if (JSON.stringify(previous) === JSON.stringify(stay.data.prices)) continue;
    for (const price of stay.data.prices)
      await capture("pms_stay_price", `${stay.id}:${stay.version}`, price.date, price.total);
  }
  for (const row of s.records) {
    if (original.get(row.id) === JSON.stringify(row) || row.kind === "entry") continue;
    const previous = original.has(row.id) ? JSON.parse(original.get(row.id)!).data as Record<string, unknown> : undefined;
    const values: Array<[string, number]> = [];
    if (row.kind === "rate") {
      const data = record(s, "rate", row.id).data;
      values.push(["price", data.price], ["extra_adult", data.extraAdult]);
      for (const [index, band] of (data.childBands ?? []).entries()) values.push([`child_band:${index}`, band.price]);
      for (const date of data.calendar ?? []) values.push([`calendar:${date.date}`, date.price]);
    } else if (row.kind === "item") values.push(["price", record(s, "item", row.id).data.price]);
    else if (row.kind === "credit") values.push(["limit", record(s, "credit", row.id).data.limit]);
    else if (row.kind === "refund") values.push(["requested", record(s, "refund", row.id).data.amount]);
    else if (row.kind === "shift") {
      const data = record(s, "shift", row.id).data;
      values.push(["opening", data.opening], ["counted", data.counted], ["variance", data.variance]);
      for (const [index, movement] of data.movements.entries()) values.push([`movement:${index}`, movement.amount]);
    }
    // Cash totals and folio balances are projections of these immutable entries,
    // not fresh transactions to quote again on every room or shift update.
    for (const [kind, amount] of values) {
      const before = row.kind === "rate" && kind.startsWith("child_band:")
        ? (previous?.childBands as Array<{ price: number }> | undefined)?.[Number(kind.split(":")[1])]?.price
        : row.kind === "rate" && kind.startsWith("calendar:")
          ? (previous?.calendar as Array<{ date: string; price: number }> | undefined)?.find(value => value.date === kind.slice(9))?.price
          : row.kind === "shift" && kind.startsWith("movement:")
            ? (previous?.movements as Array<{ amount: number }> | undefined)?.[Number(kind.split(":")[1])]?.amount
          : previous?.[kind === "extra_adult" ? "extraAdult" : kind === "requested" ? "amount" : kind];
      if (before !== amount) await capture(`pms_${row.kind}`, `${row.id}:${row.version}`, kind, amount);
    }
  }
  for (const row of records(s, "entry")) {
    if (original.has(row.id)) continue; // Entries are immutable economic source records.
    const input = { sourceType: "pms_entry", sourceId: row.id, amountKind: "total", transactionDate: row.data.date };
    if (row.data.total < 0) {
      if (row.data.funding?.length) {
        if (row.data.funding.reduce((sum, value) => sum + value.amount, 0) !== -row.data.total)
          throw new ApiError(409, "PMS_REFUND_FUNDING_CHANGED", "The refund must retain all of its original payment sources.");
        for (const source of row.data.funding) {
          const value = await snapshotMoneyReversal(tx, company, { ...input, amountKind: `funding:${source.entryId}`,
            amountUnits: String(source.amount), original: { sourceType: "pms_entry", sourceId: source.entryId, amountKind: "total" } });
          await value.statement.run();
        }
      } else {
        const value = await snapshotMoneyReversal(tx, company, { ...input, amountUnits: String(-row.data.total),
          original: { sourceType: "pms_entry", sourceId: row.data.originalId, amountKind: "total" } });
        await value.statement.run();
      }
      continue;
    }
    if (row.data.sourceKey.startsWith("online-payment:") || row.data.sourceKey.startsWith("counter-payment:")) {
      const online = row.data.sourceKey.startsWith("online-payment:"), sourceId = row.data.sourceKey.slice(online ? 15 : 16);
      const value = await snapshotMoneyCarry(tx, company, { ...input, amountUnits: String(row.data.total),
        allocationOffsetUnits: String(row.data.providerPaymentOffset ?? 0),
        original: { sourceType: online ? "pms_booking_payment" : "pms_stripe_payment", sourceId, amountKind: "requested" } });
      await value.statement.run();
    } else if (row.data.kind === "transfer" && row.data.originalId) {
      const counter = record(s, "entry", row.data.originalId);
      const value = await snapshotMoneyCarry(tx, company, { ...input, amountUnits: String(row.data.total),
        original: { sourceType: "pms_entry", sourceId: counter.data.originalId, amountKind: "total" } });
      await value.statement.run();
    } else if (row.data.sourceKey.startsWith("night:")) {
      const date = row.data.sourceKey.slice(-10), stay = record(s, "stay", row.data.stayId);
      const candidates = (await tx.prepare(`SELECT source_id FROM money_transaction_snapshots WHERE company_id=? AND source_type='pms_stay_price'
        AND source_id LIKE ? AND amount_kind=? AND amount_units=?`).bind(company.companyId, `${stay.id}:%`, date, String(row.data.total))
        .all<{ source_id: string }>()).results.sort((a, b) => Number(b.source_id.split(":").at(-1)) - Number(a.source_id.split(":").at(-1)));
      if (!candidates[0]) throw new ApiError(409, "PMS_PRICE_SNAPSHOT_REQUIRED", "The room charge must retain its reviewed booking price.");
      const value = await snapshotMoneyCarry(tx, company, { ...input, amountUnits: String(row.data.total),
        original: { sourceType: "pms_stay_price", sourceId: candidates[0].source_id, amountKind: date } });
      await value.statement.run();
    } else if (row.data.funding?.length) {
      if (row.data.funding.reduce((sum, value) => sum + value.amount, 0) !== row.data.total)
        throw new ApiError(409, "PMS_ALLOCATION_FUNDING_CHANGED", "The allocation must retain its original payment sources.");
      for (const source of row.data.funding) {
        const value = await snapshotMoneyCarry(tx, company, { ...input, amountKind: `funding:${source.entryId}`,
          amountUnits: String(source.amount), original: { sourceType: "pms_entry", sourceId: source.entryId, amountKind: "total" } });
        await value.statement.run();
      }
    } else await capture(input.sourceType, input.sourceId, input.amountKind, row.data.total, row.data.date);
  }
}
