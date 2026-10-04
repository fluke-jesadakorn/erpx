import "server-only";
import type { WorkContext } from "./authorization";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { assertion } from "./commands";
import { prepareJournal } from "./finance";
import { minorToDecimal } from "@/lib/finance-money";
import { ApiError } from "./http";
import { sha256 } from "./crypto";

type Row = Record<string, string | number | null>;

async function fxAccounts(db: SqlDatabase, actor: WorkContext, chartId: string, currency: string, gain?: string, loss?: string) {
  for (const [id, category] of [[gain, "other_income"], [loss, "other_expense"]] as const) {
    if (!id) throw new ApiError(422, category === "other_income" ? "FX_GAIN_ACCOUNT_REQUIRED" : "FX_LOSS_ACCOUNT_REQUIRED", "Review both functional-currency FX accounts before posting a foreign goods invoice.");
    if (!await db.prepare("SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category=? AND status='active'")
      .bind(id, actor.companyId, chartId, currency, category).first())
      throw new ApiError(422, "FX_ACCOUNT_INVALID", "Use active realized FX gain and loss accounts in the original Company chart and functional currency.");
  }
}

/** AP retains invoice-date FX; a separate source journal clears original goods carrying. */
export async function prepareCommerceInvoiceGoods(db: SqlDatabase, actor: WorkContext, input: {
  invoice: Row; journal: Awaited<ReturnType<typeof prepareJournal>>; key: string;
  periodId: string; entryDate: string; gainAccountId?: string; lossAccountId?: string;
}) {
  const invoice = input.invoice;
  if (invoice.source_type !== "commerce_fulfillment" || invoice.kind !== "payable") return null;
  const account=await db.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=? AND status='active'").bind(invoice.debit_account_id,actor.companyId).first<{chart_id:string}>();
  if(!account)throw new ApiError(422,'ACCOUNT_INVALID','The original goods clearing account is unavailable.');
  const chartId=account.chart_id;
  const sources = (await db.prepare(`SELECT f.id,f.version,j.id journal_id,j.version journal_version,j.currency,j.fx_rate_micros,
      c.base_currency,o.currency document_currency,fl.id line_id,fl.quantity_milli,fl.value_minor,fl.subtotal_minor,fl.tax_minor,fl.stock_currency,
      (SELECT SUM(l.base_credit_minor-l.base_debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.account_id=?) clearing_base,
      (SELECT SUM(l.credit_minor-l.debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.account_id=?) clearing_transaction
    FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id
    JOIN finance_journals j ON j.id=f.journal_id AND j.company_id=o.company_id AND j.status='posted'
    JOIN finance_charts c ON c.id=j.chart_id JOIN commerce_fulfillment_lines fl ON fl.fulfillment_id=f.id
    WHERE f.invoice_id=? AND o.company_id=? AND j.chart_id=?
      AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)
    ORDER BY f.id,fl.id`).bind(invoice.debit_account_id, invoice.debit_account_id, invoice.id, actor.companyId,
      chartId,
      actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<Row>()).results;
  if (!sources.length) throw new ApiError(409, "GOODS_CARRYING_SOURCE_REQUIRED", "Review the posted goods receipts in this Finance scope before posting AP.");
  const currency = input.journal.result.baseCurrency;
  if (sources.some(row => row.base_currency !== currency || row.document_currency !== invoice.currency ||
    row.stock_currency !== currency && !(row.stock_currency === null && row.document_currency === currency && row.currency === currency && Number(row.fx_rate_micros) === 1_000_000)))
    throw new ApiError(409, "GOODS_VALUATION_REVIEW_REQUIRED", "Reconcile historical foreign goods through their owner; their original values cannot be relabelled automatically.");
  const receipts = [...new Set(sources.map(row => String(row.id)))];
  for (const id of receipts) {
    const lines = sources.filter(row => row.id === id), source = lines[0];
    if (lines.reduce((sum, row) => sum + Number(row.value_minor), 0) !== Number(source.clearing_base) ||
      lines.reduce((sum, row) => sum + Number(row.subtotal_minor), 0) !== Number(source.clearing_transaction))
      throw new ApiError(409, "GOODS_CARRYING_RECONCILIATION_REQUIRED", "The original posted clearing account must exactly match received stock and document net amounts.");
  }
  const receiptBase = sources.reduce((sum, row) => sum + Number(row.value_minor), 0),
    invoiceBase = input.journal.preparedLines.filter(row => row.accountId === invoice.debit_account_id).reduce((sum, row) => sum + row.baseDebit - row.baseCredit, 0),
    difference = invoiceBase - receiptBase;
  if (invoice.currency !== currency || difference)
    await fxAccounts(db, actor, chartId, currency, input.gainAccountId, input.lossAccountId);
  const fx = difference ? await prepareJournal(db, actor, {
    chartId, periodId: input.periodId, entryDate: input.entryDate, currency, isOpening: false,
    sourceType: "commerce_invoice_goods_fx", sourceId: String(invoice.id), memo: `Original goods carrying · ${invoice.invoice_number}`,
    lines: difference > 0 ? [
      { accountId: input.lossAccountId!, debit: minorToDecimal(difference), credit: "0" },
      { accountId: String(invoice.debit_account_id), debit: "0", credit: minorToDecimal(difference) },
    ] : [
      { accountId: String(invoice.debit_account_id), debit: minorToDecimal(-difference), credit: "0" },
      { accountId: input.gainAccountId!, debit: "0", credit: minorToDecimal(-difference) },
    ],
  }, `${input.key}:goods-fx`, "posted") : null;
  const sourceJson = JSON.stringify(sources), steps: SqlStatement[] = [];
  for (const [index, row] of sources.entries()) steps.push(
    db.prepare("SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE").bind(row.id),
    assertion(db, input.key, `goods-carrying-${index}`, `EXISTS(SELECT 1 FROM commerce_fulfillments f
      JOIN commerce_fulfillment_lines l ON l.fulfillment_id=f.id JOIN finance_journals j ON j.id=f.journal_id
      WHERE f.id=? AND f.invoice_id=? AND f.version=? AND j.id=? AND j.version=? AND j.status='posted'
      AND l.id=? AND l.quantity_milli=? AND l.value_minor=? AND l.subtotal_minor=? AND l.tax_minor=?
      AND l.stock_currency IS NOT DISTINCT FROM ?::text)`, [row.id, invoice.id, row.version, row.journal_id, row.journal_version,
      row.line_id, row.quantity_milli, row.value_minor, row.subtotal_minor, row.tax_minor, row.stock_currency]),
  );
  if (fx) steps.push(...fx.steps);
  for (const [accountId, category] of [[input.gainAccountId, "other_income"], [input.lossAccountId, "other_expense"]] as const) if (accountId)
    steps.push(assertion(db, input.key, `goods-fx-account-${category}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category=? AND status='active')", [accountId, actor.companyId, chartId, currency, category]));
  const invoiceQuote=input.journal.preparedFxEvidence;
  steps.push(db.prepare(`INSERT INTO commerce_invoice_goods_carrying(invoice_id,company_id,chart_id,stock_currency,clearing_account_id,
    invoice_fx_rate_micros,invoice_rate_basis,invoice_source_rate_id,invoice_target_rate_id,invoice_used_latest_fallback,
    source_digest,source_json,receipt_base_minor,invoice_clearing_base_minor,adjustment_minor,gain_account_id,loss_account_id,fx_journal_id,reviewed_by,reviewed_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(invoice.id, actor.companyId, chartId, currency, invoice.debit_account_id,
      input.journal.result.fxRateMicros,invoiceQuote?'confirmed_company_quote':'reviewed_input',invoiceQuote?.sourceRateId??null,invoiceQuote?.targetRateId??null,invoiceQuote?.usedLatestFallback??false,
      await sha256(sourceJson), sourceJson, receiptBase, invoiceBase, difference, input.gainAccountId ?? null, input.lossAccountId ?? null, fx?.result.id ?? null, actor.userId, Date.now()));
  return { steps, fxJournalId: fx?.result.id ?? null };
}

/** Goods returns reverse original stock separately from original invoice-date credit. */
export async function prepareReturnGoodsFx(db: SqlDatabase, actor: WorkContext, input: {
  invoiceId: string; returnId: string; key: string; periodId: string; entryDate: string;
  stockBase: number; clearingAccountId: string; creditPostings: Array<{ accountId: string; baseDebit: number; baseCredit: number }>;
}) {
  const clearingCredit = input.creditPostings.filter(row => row.accountId === input.clearingAccountId).reduce((sum, row) => sum + row.baseCredit - row.baseDebit, 0),
    difference = clearingCredit - input.stockBase;
  if (!difference) return null;
  const source = await db.prepare("SELECT * FROM commerce_invoice_goods_carrying WHERE invoice_id=? AND company_id=?").bind(input.invoiceId, actor.companyId).first<Row>();
  if (!source || source.clearing_account_id !== input.clearingAccountId) throw new ApiError(409, "RETURN_GOODS_FX_REVIEW_REQUIRED", "Review the original independently approved goods carrying and FX mappings before returning foreign stock.");
  await fxAccounts(db, actor, String(source.chart_id), String(source.stock_currency), String(source.gain_account_id ?? ""), String(source.loss_account_id ?? ""));
  // Retain the original invoice's reviewed account for every reversal, including
  // opposite partial-rounding cents; a full return reverses that account exactly.
  const amount = minorToDecimal(Math.abs(difference)), account = String(Number(source.adjustment_minor)>=0 ? source.loss_account_id : source.gain_account_id);
  const journal = await prepareJournal(db, actor, {
    chartId: String(source.chart_id), periodId: input.periodId, entryDate: input.entryDate, currency: String(source.stock_currency),
    sourceType: "commerce_return_goods_fx", sourceId: input.returnId, memo: `Reverse original goods carrying · ${input.returnId}`, isOpening: false,
    lines: difference > 0 ? [{ accountId: input.clearingAccountId, debit: amount, credit: "0" }, { accountId: account, debit: "0", credit: amount }]
      : [{ accountId: account, debit: amount, credit: "0" }, { accountId: input.clearingAccountId, debit: "0", credit: amount }],
  }, `${input.key}:goods-fx`, "draft");
  return journal;
}
