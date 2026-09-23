import "server-only";
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
  type Kind,
  type Property,
  type Snapshot,
} from "@/lib/pms";
import { minorToDecimal } from "@/lib/finance-money";
import { prepareRoomCharge, roomChargeSchema } from "@/lib/pms-room-charge";
import { assertProductionModuleEnabled } from './hospitality-rollout';
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { WorkItem } from "@/lib/work-item";
import type { WorkContext } from "./authorization";
import { executeCommand, replayCommand } from "./commands";
import { requirePermission } from "./iam-permissions";
import { prepareJournal } from "./finance";
import { ApiError } from "./http";
import { holdingResources } from "./runtime";
import { PostgresDatabase } from "./runtime/postgres";
import type { SqlDatabase } from "./runtime/types";

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
): Promise<Snapshot> {
  const row = await db
    .prepare(
      `SELECT * FROM pms_properties WHERE id=? AND company_id=?${lock ? " FOR UPDATE" : ""}`,
    )
    .bind(id, actor.companyId)
    .first<PropertyRow>();
  if (!row) throw new ApiError(404, "PMS_NOT_FOUND", "Hotel not found");
  const p = property(row);
  scope(actor, p);
  const raw = await query<{
    id: string;
    kind: Kind;
    data_json: string;
    version: number;
    updated_at: number;
  }>(
    db,
    "SELECT * FROM pms_records WHERE property_id=? ORDER BY updated_at,id",
    id,
  );
  return {
    property: p,
    records: raw.map((r) => ({
      id: r.id,
      kind: r.kind,
      data: JSON.parse(r.data_json),
      version: Number(r.version),
      updatedAt: Number(r.updated_at),
    })),
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
] as const;
function visibleSnapshot(s: Snapshot, actor: WorkContext): Snapshot {
  const result = structuredClone(s);
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
export async function pmsWorkspace(actor: WorkContext, propertyId?: string) {
  requirePermission(actor, "pms.read");
  if (actor.departmentId || actor.beta)
    throw new ApiError(
      403,
      "PMS_BRANCH_REQUIRED",
      "Select a company or branch area",
    );
  const { write } = await holdingResources(actor.holdingId);
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
    ? await (write instanceof PostgresDatabase
        ? write.transaction((tx) => loadPms(tx, actor, selected.id))
        : loadPms(write, actor, selected.id))
    : null;
  const configuration =
    hasPermission(actor, "pms.configure") ||
    hasPermission(actor, "pms.accounting.prepare") ||
    hasPermission(actor, "pms.accounting.approve");
  const staff = await query<{ id: string; name: string }>(
    write,
    "SELECT DISTINCT a.user_id id,COALESCE(h.legal_name,a.user_id) name FROM assignments a LEFT JOIN hr_people h ON h.user_id=a.user_id AND h.company_id=a.company_id AND h.merged_into_id IS NULL WHERE a.company_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?)) ORDER BY name",
    actor.companyId,
    Date.now(),
    Date.now(),
    selected?.branch_id ?? actor.branchId ?? "",
  );
  const accounting =
    hasPermission(actor, "pms.accounting.prepare") ||
    hasPermission(actor, "pms.accounting.approve");
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
    hasPermission(actor, "pms.cashier") ||
    hasPermission(actor, "pms.configure") ||
    hasPermission(actor, "pms.reports")
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
    s &&
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
  if (s && hasPermission(actor, "pms.reports")) {
    const lines = await query<{
      account_id: string;
      debit: number;
      credit: number;
    }>(
      write,
      "SELECT l.account_id,SUM(l.debit_minor) debit,SUM(l.credit_minor) credit FROM finance_journal_lines l JOIN pms_postings p ON p.journal_id=l.journal_id JOIN finance_journals j ON j.id=l.journal_id WHERE p.property_id=? AND j.status='posted' GROUP BY l.account_id",
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
  return {
    bookingDelivery:
      s && hasPermission(actor, "pms.configure")
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
    snapshot: s ? visibleSnapshot(s, actor) : null,
    readiness: s ? readiness(s) : [],
    permissions: pmsPermissions.filter((k) => hasPermission(actor, k)),
    userId: actor.userId,
    branches,
    staff,
    accounts,
    charts,
    parties,
    products,
    warehouses,
    documents: docs,
    ledger,
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
async function effects(
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
    (r) => r.data.invoiceId && (e.postings.length || e.documents.length),
  )) {
    await run(
      db,
      "UPDATE finance_invoices SET outstanding_minor=?,status=CASE WHEN ?=0 THEN 'paid' WHEN ?<total_minor THEN 'partially_paid' ELSE 'posted' END,version=version+1,updated_at=? WHERE id=? AND company_id=? AND source_type='pms'",
      f.data.due,
      f.data.due,
      f.data.due,
      Date.now(),
      f.data.invoiceId,
      s.property.companyId,
    );
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
      ["charge", "adjustment", "transfer"].includes(e.data.kind),
    );
    const total = bill.reduce((n, e) => n + e.data.total, 0),
      tax = bill.reduce((n, e) => n + e.data.tax, 0);
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
      f.data.due,
      c.accounts.receivable,
      c.accounts.revenue,
      c.accounts.tax,
      f.id,
      f.data.due ? "posted" : "paid",
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
      item.reference,
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
  if (!f.data.invoiceId) return;
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
    "SELECT id,snapshot_json FROM pms_documents WHERE property_id=? AND folio_id=? AND kind='receipt' ORDER BY created_at,id",
    s.property.id,
    folioId,
  );
  let remaining = Math.max(
    0,
    entries
      .filter((e) => ["charge", "adjustment", "transfer"].includes(e.data.kind))
      .reduce((n, e) => n + e.data.total, 0) - f.data.due,
  );
  await run(
    db,
    "DELETE FROM finance_allocations WHERE invoice_id=? AND company_id=?",
    f.data.invoiceId,
    s.property.companyId,
  );
  for (const r of receipts) {
    const source = JSON.parse(r.snapshot_json).entries?.[0];
    if (!source) continue;
    const available =
      source.data.kind === "deposit"
        ? (applied.get(source.id) ?? 0)
        : Number(source.data.total);
    const n = Math.min(
      remaining,
      Math.max(0, available - (refunds.get(source.id) ?? 0)),
    );
    if (n > 0) {
      await run(
        db,
        "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)",
        randomUUID(),
        s.property.companyId,
        r.id,
        f.data.invoiceId,
        n,
        actor.userId,
        Date.now(),
      );
      remaining -= n;
    }
  }
  if (remaining)
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
  db: PostgresDatabase,
  actor: WorkContext,
  raw: unknown,
  key: string,
  worker = false,
) {
  const command = commandSchema.parse(raw),
    op = command.operation;
  if (op === "charge.import_pos")
    throw new ApiError(403, "PMS_INTERNAL_COMMAND", "Restaurant charges must come from a POS transaction");
  requirePermission(actor, operationPermission(op));
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
          : executePms(s, op, command.input, {
              userId: actor.userId,
              now: Date.now(),
              uuid: randomUUID,
              independent: (other) => !samePerson(actor, other),
            });
      if (op === "booking.save" && s.property.config.booking?.published) {
        if (!process.env.RESEND_API_KEY || !process.env.RESEND_FROM)
          throw new ApiError(
            422,
            "PMS_EMAIL_REQUIRED",
            "Configure booking email delivery before publishing",
          );
      }
      await validateReferences(tx, s, op, command.input);
      if (
        [
          "accounting.save",
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
      const result = {
        id: outcome.id,
        propertyId: s.property.id,
        version: s.property.version,
      };
      return executeCommand(
        tx,
        actor,
        key,
        `pms.${op}`,
        command,
        target,
        s.property.version,
        result,
        [],
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
export async function drainPms(db: PostgresDatabase) {
  const jobs = await query<{
    id: string;
    property_id: string;
    actor_json: string;
  }>(
    db,
    "SELECT id,property_id,actor_json FROM pms_jobs WHERE state IN ('pending','running') ORDER BY updated_at LIMIT 4",
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
  if (!hasPermission(actor, "pms.read") || actor.departmentId || actor.beta)
    return [];
  const { write } = await holdingResources(actor.holdingId);
  const properties = await query<PropertyRow>(
    write,
    `SELECT * FROM pms_properties WHERE company_id=?${actor.branchId ? " AND branch_id=?" : ""}`,
    actor.companyId,
    ...(actor.branchId ? [actor.branchId] : []),
  );
  const items: WorkItem[] = [];
  for (const row of properties) {
    const s = await loadPms(write, actor, row.id);
    for (const r of records(s, "task"))
      if (
        !["done", "superseded"].includes(r.data.state) &&
        (r.data.assigneeId === actor.userId ||
          hasPermission(actor, "pms.inspect"))
      ) {
        items.push({
          id: r.id,
          type: "pms",
          title: `${row.name} · ${record(s, "room", r.data.roomId).data.number} · ${r.data.type}`,
          detail: r.data.notes || "Room task",
          href: `/pms/housekeeping?property=${row.id}`,
          status: r.data.state,
          updatedAt: r.updatedAt,
          queue: "todo",
          relationships: [
            r.data.assigneeId === actor.userId ? "assigned" : "team",
          ],
        });
      }
    for (const r of records(s, "refund"))
      if (
        r.data.state === "requested" &&
        hasPermission(actor, "pms.approve") &&
        !samePerson(actor, r.data.requestedBy)
      )
        items.push({
          id: r.id,
          type: "pms",
          title: `${row.name} · Approve refund`,
          detail: r.data.reason,
          href: `/pms/folios?property=${row.id}`,
          status: r.data.state,
          updatedAt: r.updatedAt,
          queue: "todo",
          relationships: ["approval"],
        });
    for (const r of records(s, "night"))
      if (r.data.state === "blocked" && hasPermission(actor, "pms.audit"))
        items.push({
          id: r.id,
          type: "pms",
          title: `${row.name} · Close day pending`,
          detail: r.data.message,
          href: `/pms/night-audit?property=${row.id}`,
          status: r.data.state,
          updatedAt: r.updatedAt,
          queue: "todo",
          relationships: ["team"],
        });
  }
  return items;
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
}
