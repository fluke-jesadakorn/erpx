import 'server-only';
import { query, withTransaction } from '../db';
import { loadPostingActor, postJournalInTransaction, type FinanceQuery } from './journals';

export interface GlPostResult {
  journalId: number;
}

export interface DraftGlLine {
  account_code: string;
  account_name?: string | null;
  account_name_th?: string | null;
  debit: number;
  credit: number;
  description: string;
}

export class JournalValidationError extends Error {
  issues: string[];

  constructor(issues: string[]) {
    super(issues.join('; '));
    this.name = 'JournalValidationError';
    this.issues = issues;
  }
}

async function expenseAmounts(expenseId: number) {
  const res = await query<{
    vat_amount: string | null;
    total_amount: string | null;
    payee_type: 'employee' | 'vendor';
    payment_method: 'cash' | 'credit_card' | 'transfer' | null;
  }>(`SELECT vat_amount, total_amount, payee_type, payment_method FROM expenses WHERE id = $1`, [expenseId]);
  if (!res.rows[0]) throw new JournalValidationError(['Expense not found']);
  return {
    vat: Number(res.rows[0].vat_amount ?? 0),
    total: Number(res.rows[0].total_amount ?? 0),
    payeeType: res.rows[0].payee_type,
    paymentMethod: res.rows[0].payment_method,
  };
}

async function accountNames(codes: string[]) {
  const res = await query<{ code: string; name: string; name_th: string }>(
    `SELECT code, name, name_th FROM chart_of_accounts WHERE code = ANY($1::text[])`,
    [codes],
  );
  return new Map(res.rows.map((row) => [row.code, row]));
}

export async function buildAccrualLines(expenseId: number): Promise<DraftGlLine[]> {
  const exp = await expenseAmounts(expenseId);
  const items = await query<{
    amount: string;
    mapped_account_code: string | null;
    description: string | null;
    account_name: string | null;
    account_name_th: string | null;
  }>(
    `SELECT i.amount,
            CASE WHEN c.account_type = 'expense' THEN i.mapped_account_code END AS mapped_account_code,
            i.description,
            CASE WHEN c.account_type = 'expense' THEN c.name END AS account_name,
            CASE WHEN c.account_type = 'expense' THEN c.name_th END AS account_name_th
       FROM expense_items i
       LEFT JOIN chart_of_accounts c ON c.code = i.mapped_account_code
      WHERE i.expense_id = $1 ORDER BY i.id`,
    [expenseId],
  );
  const payableCode = exp.payeeType === 'vendor' ? '210100' : '210500';
  const names = await accountNames(['110500', payableCode]);
  const expenseTarget = Math.max(0, exp.total - exp.vat);
  const itemTotal = items.rows.reduce((sum, item) => sum + Number(item.amount), 0);
  const scale = itemTotal > 0 ? expenseTarget / itemTotal : 1;
  const lines: DraftGlLine[] = items.rows.length
    ? items.rows.map((item, index) => {
        const amount = index === items.rows.length - 1
          ? expenseTarget - items.rows.slice(0, -1).reduce((sum, row) => sum + Number((Number(row.amount) * scale).toFixed(2)), 0)
          : Number((Number(item.amount) * scale).toFixed(2));
        return {
          account_code: item.mapped_account_code ?? '510300',
          account_name: item.account_name,
          account_name_th: item.account_name_th,
          debit: amount,
          credit: 0,
          description: item.description ?? `Expense EXP-${expenseId}`,
        };
      })
    : [{
        account_code: '510300',
        debit: expenseTarget,
        credit: 0,
        description: `Expense EXP-${expenseId}`,
      }];
  if (exp.vat > 0) {
    const vat = names.get('110500');
    lines.push({
      account_code: '110500',
      account_name: vat?.name,
      account_name_th: vat?.name_th,
      debit: exp.vat,
      credit: 0,
      description: `Input VAT for EXP-${expenseId}`,
    });
  }
  const payable = names.get(payableCode);
  lines.push({
    account_code: payableCode,
    account_name: payable?.name,
    account_name_th: payable?.name_th,
    debit: 0,
    credit: exp.total,
    description: `${exp.payeeType === 'vendor' ? 'Vendor' : 'Employee'} payable for EXP-${expenseId}`,
  });
  return lines;
}

export async function buildSettlementLines(expenseId: number, bankAccountCode?: string): Promise<DraftGlLine[]> {
  const exp = await expenseAmounts(expenseId);
  const cashCode = bankAccountCode
    ?? (exp.paymentMethod === 'cash' ? '110100' : exp.paymentMethod === 'credit_card' ? '110300' : '110200');
  const payableCode = exp.payeeType === 'vendor' ? '210100' : '210500';
  const names = await accountNames([payableCode, cashCode]);
  return [
    {
      account_code: payableCode,
      account_name: names.get(payableCode)?.name,
      account_name_th: names.get(payableCode)?.name_th,
      debit: exp.total,
      credit: 0,
      description: `Clear payable for EXP-${expenseId}`,
    },
    {
      account_code: cashCode,
      account_name: names.get(cashCode)?.name,
      account_name_th: names.get(cashCode)?.name_th,
      debit: 0,
      credit: exp.total,
      description: `Payment from bank or cash for EXP-${expenseId}`,
    },
  ];
}

export async function validateJournalLines(lines: DraftGlLine[], client: typeof query = query): Promise<void> {
  const issues: string[] = [];
  if (lines.length < 2) issues.push('Journal must contain at least two lines');
  const codes = [...new Set(lines.map((line) => line.account_code))];
  const accounts = await client<{ code: string }>(
    `SELECT code FROM chart_of_accounts WHERE code = ANY($1::text[])`,
    [codes],
  );
  const valid = new Set(accounts.rows.map((row) => row.code));
  let debit = 0;
  let credit = 0;
  lines.forEach((line, index) => {
    if (!valid.has(line.account_code)) issues.push(`Line ${index + 1} has an invalid COA account`);
    if (line.debit < 0 || line.credit < 0) issues.push(`Line ${index + 1} has a negative amount`);
    if ((line.debit > 0) === (line.credit > 0)) issues.push(`Line ${index + 1} must be one-sided`);
    debit += Number(line.debit);
    credit += Number(line.credit);
  });
  if (Math.abs(debit - credit) > 0.005) issues.push('Journal is not balanced');
  if (debit <= 0 || credit <= 0) issues.push('Journal totals must be positive');
  if (issues.length) throw new JournalValidationError(issues);
}

export async function upsertDraftJournal(args: {
  expenseId: number;
  vendorName: string;
  step?: 'accrual' | 'settlement';
  preparedBy?: number;
  bankAccountCode?: string;
  aiSuggestion?: Record<string, unknown> | null;
  aiConfidence?: number | null;
}): Promise<{ journalId: number; lines: DraftGlLine[] }> {
  const step = args.step ?? 'accrual';
  const lines = step === 'settlement'
    ? await buildSettlementLines(args.expenseId, args.bankAccountCode)
    : await buildAccrualLines(args.expenseId);
  await validateJournalLines(lines);
  const journalId = await withTransaction(async (q) => {
    const existing = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE expense_id = $1 AND step = $2 AND is_draft = TRUE LIMIT 1`,
      [args.expenseId, step],
    );
    const description = step === 'accrual'
      ? `DRAFT: Accrual for ${args.vendorName} (EXP-${args.expenseId})`
      : `DRAFT: Settlement for ${args.vendorName} (EXP-${args.expenseId})`;
    const id = existing.rows[0]?.id ?? (await q<{ id: number }>(
      `INSERT INTO journal_entries
         (expense_id, description, is_draft, draft_source, step, prepared_by, ai_suggestion, ai_confidence)
       VALUES ($1, $2, TRUE, 'expense', $3, $4, $5, $6) RETURNING id`,
      [args.expenseId, description, step, args.preparedBy ?? null, args.aiSuggestion ?? null, args.aiConfidence ?? null],
    )).rows[0].id;
    await q(
      `UPDATE journal_entries
          SET description = $2,
              prepared_by = COALESCE($3, prepared_by),
              ai_suggestion = $4,
              ai_confidence = $5
        WHERE id = $1`,
      [id, description, args.preparedBy ?? null, args.aiSuggestion ?? null, args.aiConfidence ?? null],
    );
    await q(`DELETE FROM ledger_lines WHERE journal_entry_id = $1`, [id]);
    for (const line of lines) {
      await q(
        `INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
         VALUES ($1, $2, $3, $4, $5)`,
        [id, line.account_code, line.debit, line.credit, line.description],
      );
    }
    return id;
  });
  return { journalId, lines };
}

export async function saveDraftJournalLines(args: {
  expenseId: number;
  step: 'accrual' | 'settlement';
  actorId: number;
  lines: DraftGlLine[];
}) {
  await validateJournalLines(args.lines);
  return withTransaction(async (q) => {
    const journal = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE expense_id = $1 AND step = $2 AND is_draft = TRUE FOR UPDATE`,
      [args.expenseId, args.step],
    );
    if (!journal.rows[0]) throw new JournalValidationError(['Draft journal not found']);
    await q(`DELETE FROM ledger_lines WHERE journal_entry_id = $1`, [journal.rows[0].id]);
    for (const line of args.lines) {
      await q(
        `INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
         VALUES ($1, $2, $3, $4, $5)`,
        [journal.rows[0].id, line.account_code, line.debit, line.credit, line.description],
      );
    }
    await q(`UPDATE journal_entries SET prepared_by = $2 WHERE id = $1`, [journal.rows[0].id, args.actorId]);
    return { journalId: journal.rows[0].id };
  });
}

export async function recomputeDraftJournal(args: {
  expenseId: number;
  vendorName: string;
  step?: 'accrual' | 'settlement';
  preparedBy?: number;
}): Promise<{ journalId: number; lines: DraftGlLine[] } | null> {
  const step = args.step ?? 'accrual';
  const draft = await query<{ id: number }>(
    `SELECT id FROM journal_entries WHERE expense_id = $1 AND step = $2 AND is_draft = TRUE LIMIT 1`,
    [args.expenseId, step],
  );
  if (!draft.rows[0]) return null;
  return upsertDraftJournal({ ...args, step });
}

export async function finalizeDraftJournal(args: {
  expenseId: number;
  actorId: number;
  step?: 'accrual' | 'settlement';
  client?: typeof query;
}): Promise<{ journalId: number } | null> {
  const step = args.step ?? 'accrual';
  const q = args.client ?? query;
  const journal = await q<{ id: number }>(
    `SELECT id FROM journal_entries
      WHERE expense_id = $1 AND step = $2 AND is_draft = TRUE
      ORDER BY id DESC LIMIT 1 FOR UPDATE`,
    [args.expenseId, step],
  );
  if (!journal.rows[0]) return null;
  const rows = await q<DraftGlLine>(
    `SELECT account_code, debit::float8 AS debit, credit::float8 AS credit, COALESCE(description, '') AS description
       FROM ledger_lines WHERE journal_entry_id = $1 ORDER BY id`,
    [journal.rows[0].id],
  );
  await validateJournalLines(rows.rows, q);
  const context = await q<{
    transaction_date: string | null;
    vendor_name: string | null;
    total_amount: string;
    currency_code: string;
    fx_rate: string;
    branch_id: string;
    department_id: string | null;
    vendor_id: string | null;
    submitter_id: number | null;
    payee_type: 'employee' | 'vendor';
    invoice_number: string | null;
    waybill_id: string | null;
  }>(
    `SELECT e.transaction_date::text, e.vendor_name, e.total_amount::text,
            e.currency_code, e.fx_rate::text,
            COALESCE(e.branch_id, (SELECT id FROM finance.branches WHERE active ORDER BY id LIMIT 1))::text AS branch_id,
            e.department_id,
            e.vendor_id::text, e.submitter_id, e.payee_type, e.invoice_number,
            w.id AS waybill_id
       FROM expenses e
       LEFT JOIN waybills w ON w.origin = 'expense' AND w.origin_id = e.id
      WHERE e.id = $1 FOR UPDATE OF e`,
    [args.expenseId],
  );
  const exp = context.rows[0];
  if (!exp) throw new JournalValidationError(['Expense not found']);
  const actor = await loadPostingActor(q as FinanceQuery, args.actorId);
  const fx = exp.currency_code.trim() === 'THB' ? 1 : Number(exp.fx_rate);
  const postingDate = new Date().toISOString().slice(0, 10);
  const documentDate = step === 'accrual' ? exp.transaction_date ?? postingDate : postingDate;
  const official = await postJournalInTransaction(q as FinanceQuery, {
    postingDate,
    documentDate,
    description: `${step === 'accrual' ? 'Expense accrual' : 'Expense payment'} EXP-${args.expenseId}`,
    currencyCode: exp.currency_code.trim(),
    fxRate: fx,
    sourceType: 'expense',
    sourceId: String(args.expenseId),
    sourceEventKey: `expense:${args.expenseId}:${step}:v1`,
    branchId: Number(exp.branch_id),
    waybillId: exp.waybill_id,
    lines: rows.rows.map((line) => ({
      accountCode: line.account_code,
      description: line.description,
      debitThb: Math.round(line.debit * fx * 100) / 100,
      creditThb: Math.round(line.credit * fx * 100) / 100,
      foreignAmount: line.debit > 0 ? line.debit : -line.credit,
      currencyCode: exp.currency_code.trim(),
      branchId: Number(exp.branch_id),
      departmentId: exp.department_id,
      vendorId: exp.vendor_id ? Number(exp.vendor_id) : null,
      employeeId: exp.payee_type === 'employee' ? exp.submitter_id : null,
      waybillId: exp.waybill_id,
      sourceDocumentType: step === 'accrual' ? 'expense' : 'payment',
      sourceDocumentId: String(args.expenseId),
    })),
  }, actor);
  if (step === 'accrual') {
    let vendorId = exp.vendor_id ? Number(exp.vendor_id) : null;
    if (exp.payee_type === 'vendor' && !vendorId) {
      const vendor = await q<{ id: string }>(
        `INSERT INTO finance.vendors(code, name)
         VALUES ($1,$2)
         ON CONFLICT (code) DO UPDATE SET name = excluded.name
         RETURNING id::text`,
        [`EXP-${args.expenseId}`, exp.vendor_name ?? `Vendor EXP-${args.expenseId}`],
      );
      vendorId = Number(vendor.rows[0].id);
      await q(`UPDATE expenses SET vendor_id = $2 WHERE id = $1`, [args.expenseId, vendorId]);
    }
    const total = Number(exp.total_amount);
    const totalThb = Math.round(total * fx * 100) / 100;
    await q(
      `INSERT INTO finance.ap_documents
         (vendor_id, employee_id, branch_id, document_no, document_type,
          source_type, source_id, document_date, due_date, currency_code,
          fx_rate, original_foreign, open_foreign, original_thb, open_thb, journal_id)
       VALUES ($1,$2,$3,$4,$5,'expense',$6,$7,$7,$8,$9,$10,$10,$11,$11,$12)
       ON CONFLICT (vendor_id, document_no) DO NOTHING`,
      [vendorId, exp.payee_type === 'employee' ? exp.submitter_id : null, Number(exp.branch_id), exp.invoice_number ?? `EXP-${args.expenseId}`, exp.payee_type === 'employee' ? 'employee_expense' : 'vendor_invoice', String(args.expenseId), documentDate, exp.currency_code.trim(), fx, total, totalThb, official.id],
    );
  } else {
    const ap = await q<{ id: string; open_foreign: string; open_thb: string }>(
      `SELECT id::text, open_foreign::text, open_thb::text
         FROM finance.ap_documents
        WHERE source_type = 'expense' AND source_id = $1 AND status IN ('open','partially_paid')
        ORDER BY id DESC LIMIT 1 FOR UPDATE`,
      [String(args.expenseId)],
    );
    if (ap.rows[0]) {
      await q(
        `INSERT INTO finance.ap_allocations
           (ap_document_id, allocation_date, foreign_amount, functional_amount, journal_id, allocated_by)
         VALUES ($1,$2,$3,$4,$5,$6)`,
        [Number(ap.rows[0].id), postingDate, Number(ap.rows[0].open_foreign), Number(ap.rows[0].open_thb), official.id, args.actorId],
      );
      await q(`UPDATE finance.ap_documents SET open_foreign = 0, open_thb = 0, status = 'paid' WHERE id = $1`, [Number(ap.rows[0].id)]);
    }
  }
  await q(
    `UPDATE journal_entries
        SET is_draft = FALSE,
            finalized_at = now(),
            finalized_by = $1,
            approved_by = $1
      WHERE id = $2`,
    [args.actorId, journal.rows[0].id],
  );
  return { journalId: journal.rows[0].id };
}

export async function postExpenseToGL(args: {
  expenseId: number;
  vendorName: string;
  totalAmount: number | string;
  vatAmount: number | string;
}): Promise<GlPostResult> {
  void args.totalAmount;
  void args.vatAmount;
  const draft = await upsertDraftJournal({ expenseId: args.expenseId, vendorName: args.vendorName, step: 'accrual' });
  return { journalId: draft.journalId };
}

export async function setExpenseJournalEntry(
  client: typeof import('../db').query,
  expenseId: number,
  journalId: number,
  actorId: number,
): Promise<void> {
  void actorId;
  await client(
    `UPDATE expenses SET journal_entry_id = $1, updated_at = now() WHERE id = $2`,
    [journalId, expenseId],
  );
}
