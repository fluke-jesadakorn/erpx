import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { Row } from "./commerce";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { prepareJournal } from "./finance";
import { minorToDecimal } from "@/lib/finance-money";
import { snapshotMoney } from "./company-money";

const profileSchema = z.object({
  chartId: z.string().min(1), inventoryAccountId: z.string().min(1),
  wipAccountId: z.string().min(1), laborAccountId: z.string().min(1), overheadAccountId: z.string().min(1), scrapAccountId: z.string().min(1),
});
const reviewSchema = z.object({
  expectedVersion: z.number().int().positive(), action: z.enum(["approve", "reject", "retire"]),
  reason: z.string().trim().min(3).max(3000),
});
function companyFinance(a: WorkContext, permission: "finance.write" | "finance.approve") {
  requirePermission(a, permission);
  if (a.branchId || a.departmentId || a.beta)
    throw new ApiError(403, "COMPANY_FINANCE_REQUIRED", "Choose Company Finance to configure the production accounting adapter.");
}
export async function createManufacturingAccountingProfile(a: WorkContext, raw: unknown, key: string) {
  companyFinance(a, "finance.write");
  const i = profileSchema.parse(raw), { write } = await holdingResources(a.holdingId), id = `mfg-accounting-${key}`,
    op = "manufacturing.accounting.create", prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  if ([i.inventoryAccountId, i.laborAccountId, i.overheadAccountId, i.scrapAccountId].includes(i.wipAccountId))
    throw new ApiError(422, "MANUFACTURING_WIP_ACCOUNT", "WIP must be separate from inventory and conversion-cost clearing accounts.");
  const chart = await write.prepare("SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'")
    .bind(i.chartId, a.companyId).first<{ base_currency: string }>();
  if (chart?.base_currency !== "THB")
    throw new ApiError(422, "MANUFACTURING_CURRENCY_ADAPTER", "The production accounting adapter requires a THB functional-currency chart.");
  const now = Date.now();
  return executeCommand(write, a, key, op, i, id, 1, { id, version: 1, status: "draft" }, [
    assertion(write, key, "chart", "EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=? AND chart_id=? AND inventory_account_id=?)", [a.companyId, i.chartId, i.inventoryAccountId]),
    ...[i.inventoryAccountId, i.wipAccountId, i.laborAccountId, i.overheadAccountId, i.scrapAccountId].map((account, n) =>
      assertion(write, key, `account-${n}`, `EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active' AND control_kind='none' ${n < 2 ? "AND category='asset'" : n === 4 ? "AND category IN ('expense','other_expense')" : "AND category IN ('liability','expense')"})`, [account, a.companyId, i.chartId])),
    write.prepare(`INSERT INTO manufacturing_accounting_profiles(id,company_id,chart_id,currency,inventory_account_id,wip_account_id,labor_account_id,overhead_account_id,scrap_account_id,status,version,created_by,created_at,updated_at)
      VALUES(?,?,?,'THB',?,?,?,?,?,'draft',1,?,?,?)`)
      .bind(id, a.companyId, i.chartId, i.inventoryAccountId, i.wipAccountId, i.laborAccountId, i.overheadAccountId, i.scrapAccountId, a.userId, now, now),
  ]);
}
export async function reviewManufacturingAccountingProfile(a: WorkContext, id: string, raw: unknown, key: string) {
  companyFinance(a, "finance.approve");
  const i = reviewSchema.parse(raw), { write } = await holdingResources(a.holdingId), op = `manufacturing.accounting.${i.action}`,
    prior = await replayCommand(write, a, key, op, id, i);
  if (prior) return prior;
  const row = await write.prepare("SELECT * FROM manufacturing_accounting_profiles WHERE id=? AND company_id=?")
    .bind(id, a.companyId).first<Row>();
  if (!row) throw new ApiError(404, "MANUFACTURING_ACCOUNTING_NOT_FOUND", "Accounting profile not found in Company Finance.");
  const expected = i.action === "retire" ? "approved" : "draft", status = i.action === "approve" ? "approved" : i.action === "reject" ? "rejected" : "retired";
  if (row.status !== expected || Number(row.version) !== i.expectedVersion || row.created_by === a.userId)
    throw new ApiError(409, "CONFLICT", "An independent Finance reviewer must use the current profile version.");
  return executeCommand(write, a, key, op, i, id, i.expectedVersion + 1, { id, status, version: i.expectedVersion + 1 }, [
    assertion(write, key, "profile", "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND version=? AND status=? AND created_by<>?)", [id, a.companyId, i.expectedVersion, expected, a.userId]),
    write.prepare("UPDATE manufacturing_accounting_profiles SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?")
      .bind(status, a.userId, Date.now(), id),
  ]);
}

/** Deterministic owner posting: authorization comes from an independently approved
 * Finance account map; the command's source, stock and GL commit atomically. */
export async function prepareManufacturingPosting(db: SqlDatabase, a: WorkContext, input: {
  key: string; orderId: string; orderVersion: number; phase: "issue" | "report" | "output" | "scrap";
  entryDate: string; material?: number; labor?: number; overhead?: number; output?: number; quantityMilli?: number;
}): Promise<{ steps: SqlStatement[]; journalId: string | null }> {
  const profile = await db.prepare("SELECT * FROM manufacturing_accounting_profiles WHERE company_id=? AND status='approved'")
    .bind(a.companyId).first<Row>();
  if (!profile)
    throw new ApiError(422, "MANUFACTURING_ACCOUNTING_REQUIRED", "Finance must independently approve a production account map before materials or costs can be posted.");
  const amounts = [input.material ?? 0, input.labor ?? 0, input.overhead ?? 0, input.output ?? 0];
  if (amounts.some(amount => !Number.isSafeInteger(amount) || amount < 0))
    throw new ApiError(422, "MONEY_OVERFLOW", "Production costs must be nonnegative integer minor units.");
  const total = amounts.reduce((sum, amount) => sum + amount, 0);
  if (!Number.isSafeInteger(total)) throw new ApiError(422, "MONEY_OVERFLOW", "Production costs exceed ledger precision.");
  const guards: SqlStatement[] = [
    assertion(db, input.key, "production-accounting-profile", "EXISTS(SELECT 1 FROM manufacturing_accounting_profiles WHERE id=? AND company_id=? AND status='approved' AND version=?)", [String(profile.id), a.companyId, Number(profile.version)]),
    assertion(db, input.key, "production-currency", "EXISTS(SELECT 1 FROM commerce_settings s JOIN finance_charts c ON c.id=s.chart_id WHERE s.company_id=? AND s.chart_id=? AND c.base_currency='THB' AND s.inventory_account_id=?)", [a.companyId, String(profile.chart_id), String(profile.inventory_account_id)]),
  ];
  if (!total) return { steps: guards, journalId: null };
  const period = await db.prepare("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=? ORDER BY start_date DESC,id LIMIT 1")
    .bind(a.companyId, input.entryDate, input.entryDate).first<{ id: string }>();
  if (!period) throw new ApiError(422, "MANUFACTURING_PERIOD_REQUIRED", "Open a Finance period covering the production entry date.");
  const lines: Array<{ accountId: string; debit: string; credit: string; description: string }> = [];
  const add = (account: unknown, debit: number, credit: number, description: string) => {
    if (debit || credit) lines.push({ accountId: String(account), debit: minorToDecimal(debit), credit: minorToDecimal(credit), description });
  };
  if (input.phase === "output" || input.phase === "scrap") {
    add(input.phase === "output" ? profile.inventory_account_id : profile.scrap_account_id, total, 0, input.phase === "output" ? "Finished output capitalized" : "Failed output written off");
    add(profile.wip_account_id, 0, total, "WIP relieved at independent disposition");
  } else {
    add(profile.wip_account_id, total, 0, "Production work in progress");
    add(profile.inventory_account_id, 0, input.material ?? 0, "Materials issued from inventory");
    add(profile.labor_account_id, 0, input.labor ?? 0, "Actual labor clearing");
    add(profile.overhead_account_id, 0, input.overhead ?? 0, "Actual overhead clearing");
  }
  const journal = await prepareJournal(db, a, {
    chartId: String(profile.chart_id), periodId: period.id, entryDate: input.entryDate, currency: "THB",
    memo: `Production ${input.phase} · ${input.orderId} v${input.orderVersion}`, sourceType: `manufacturing_${input.phase}`,
    sourceId: input.orderId, isOpening: false, lines,
  }, input.key, "posted");
  const steps = [...guards, ...journal.steps], sourceId = `mfg-event-${input.key}`, now = Date.now();
  steps.push(db.prepare(`INSERT INTO manufacturing_order_postings(id,company_id,order_id,source_order_version,phase,profile_id,profile_version,journal_id,currency,amount_minor,account_snapshot_json,created_by,created_at)
    VALUES(?,?,?,?,?,?,?,?,'THB',?,?,?,?)`).bind(`mfg-posting-${input.key}`, a.companyId, input.orderId, input.orderVersion, input.phase,
      profile.id, profile.version, journal.result.id, total, JSON.stringify({ inventory: profile.inventory_account_id, wip: profile.wip_account_id, labor: profile.labor_account_id, overhead: profile.overhead_account_id }), a.userId, now));
  const movements: Array<{ kind: string; amount: number }> = input.phase === "output" || input.phase === "scrap"
    ? [{ kind: input.phase === "output" ? "output" : "abnormal_scrap", amount: total }]
    : [{ kind: "material_issue", amount: input.material ?? 0 }, { kind: "labor", amount: input.labor ?? 0 }, { kind: "overhead", amount: input.overhead ?? 0 }];
  for (const [n, movement] of movements.filter(m => m.amount > 0).entries()) {
    const id = `mfg-movement-${input.key}-${n}`;
    steps.push(db.prepare(`INSERT INTO manufacturing_cost_movements(id,company_id,order_id,movement_kind,quantity_milli,amount_minor,source_type,source_id,journal_id,occurred_on,version,created_by,created_at)
      VALUES(?,?,?,?,?,?,'production_event',?,?,?,1,?,?)`)
      .bind(id, a.companyId, input.orderId, movement.kind, input.quantityMilli ?? 0, movement.amount, sourceId, journal.result.id, input.entryDate, a.userId, now));
    steps.push((await snapshotMoney(db, a, { sourceType: "manufacturing_cost_movement", sourceId: id, amountKind: movement.kind,
      transactionDate: input.entryDate, currency: "THB", currencyScale: 2, amountUnits: String(movement.amount) })).statement);
  }
  return { steps, journalId: journal.result.id };
}
