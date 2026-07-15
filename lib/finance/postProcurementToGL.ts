// folio/lib/finance/postProcurementToGL.ts
//
// 2-step GL for procurement waybills (PR / PO):
//   Step 1 — Accrual (before pay):
//     Dr expense/use (mapped account per item)   sum(item amounts)
//     Dr Input VAT (if any)                      vat_amount
//     Cr Accounts Payable (210100)               total
//
//   Step 2 — Settlement (after pay):
//     Dr Accounts Payable (210100)               total
//     Cr Cash at Bank (110200)                   total
//
// PR/PO schema does not carry a separate `vat_amount` column today, so
// accrual lines collapse to: Dr item-mapped-account, Cr 210100.
// Settlement is always a clean 2-liner.

import 'server-only';
import { query, withTransaction } from '../db';

export type ProcurementOrigin = 'pr' | 'po';
export type ProcurementStep = 'accrual' | 'settlement';

export interface ProcurementGlLine {
  account_code: string;
  account_name?: string | null;
  account_name_th?: string | null;
  debit: number;
  credit: number;
  description: string;
}

export interface ProcurementGlResult {
  journalId: number;
  lines: ProcurementGlLine[];
}

const ACCRUAL_FALLBACK_CODES = ['110500', '210100'];
const SETTLEMENT_FALLBACK_CODES = ['210100', '110200'];

async function loadCoaNames(codes: string[]): Promise<Map<string, { name: string | null; name_th: string | null }>> {
  if (codes.length === 0) return new Map();
  const r = await query<{ code: string; name: string | null; name_th: string | null }>(
    `SELECT code, name, name_th FROM chart_of_accounts WHERE code = ANY($1::text[])`,
    [codes],
  );
  return new Map(r.rows.map((a) => [a.code, a]));
}

async function buildAccrualLines(origin: ProcurementOrigin, originId: number): Promise<ProcurementGlLine[]> {
  const itemTable = origin === 'pr' ? 'pr_items' : 'po_items';
  const fkCol = origin === 'pr' ? 'pr_id' : 'po_id';
  const itemsRes = await query<{
    account_code: string | null;
    description: string | null;
    amount: string;
    account_name: string | null;
    account_name_th: string | null;
  }>(
    `SELECT i.mapped_account_code,
            i.description,
            (i.qty * i.unit_price)::text AS amount,
            c.name AS account_name,
            c.name_th AS account_name_th
       FROM ${itemTable} i
       LEFT JOIN chart_of_accounts c ON c.code = i.mapped_account_code
      WHERE i.${fkCol} = $1
   ORDER BY i.id ASC`,
    [originId],
  );

  const parentTable = origin === 'pr' ? 'purchase_requisitions' : 'purchase_orders';
  const totalCol = origin === 'pr' ? 'total_estimate' : 'total_amount';
  const parentRes = await query<{ vendor_name: string | null; total: string }>(
    `SELECT vendor_name, ${totalCol}::text AS total FROM ${parentTable} WHERE id = $1`,
    [originId],
  );
  const total = parentRes.rows[0] ? parseFloat(parentRes.rows[0].total) : 0;

  const coaCodes = new Set<string>(['210100']);
  for (const it of itemsRes.rows) {
    if (it.account_code) coaCodes.add(it.account_code);
  }
  const coaMap = await loadCoaNames(Array.from(coaCodes));

  const lines: ProcurementGlLine[] = [];
  for (const it of itemsRes.rows) {
    const code = it.account_code || '510300';
    const amt = parseFloat(it.amount);
    if (amt <= 0) continue;
    const acct = coaMap.get(code);
    lines.push({
      account_code: code,
      account_name: it.account_name ?? acct?.name ?? null,
      account_name_th: it.account_name_th ?? acct?.name_th ?? null,
      debit: amt,
      credit: 0,
      description: it.description ?? `${origin.toUpperCase()}-${originId} line`,
    });
  }

  const acct210100 = await loadCoaNames(['210100']).then((m) => m.get('210100'));
  lines.push({
    account_code: '210100',
    account_name: acct210100?.name ?? null,
    account_name_th: acct210100?.name_th ?? null,
    debit: 0,
    credit: total,
    description: `${origin.toUpperCase()}-${originId} accrual → Accounts Payable`,
  });

  return lines;
}

async function buildSettlementLines(origin: ProcurementOrigin, originId: number): Promise<ProcurementGlLine[]> {
  const parentTable = origin === 'pr' ? 'purchase_requisitions' : 'purchase_orders';
  const totalCol = origin === 'pr' ? 'total_estimate' : 'total_amount';
  const parentRes = await query<{ total: string }>(
    `SELECT ${totalCol}::text AS total FROM ${parentTable} WHERE id = $1`,
    [originId],
  );
  const total = parentRes.rows[0] ? parseFloat(parentRes.rows[0].total) : 0;

  const coaMap = await loadCoaNames(SETTLEMENT_FALLBACK_CODES);
  const ap = coaMap.get('210100');
  const cb = coaMap.get('110200');
  const ref = `${origin.toUpperCase()}-${originId}`;

  return [
    {
      account_code: '210100',
      account_name: ap?.name ?? null,
      account_name_th: ap?.name_th ?? null,
      debit: total,
      credit: 0,
      description: `Clear ${ref} payable`,
    },
    {
      account_code: '110200',
      account_name: cb?.name ?? null,
      account_name_th: cb?.name_th ?? null,
      debit: 0,
      credit: total,
      description: `${ref} disbursed from cash at bank`,
    },
  ];
}

async function insertLines(journalId: number, lines: ProcurementGlLine[]): Promise<void> {
  for (const ln of lines) {
    await query(
      `INSERT INTO ledger_lines (journal_entry_id, account_code, debit, credit, description)
       VALUES ($1, $2, $3, $4, $5)`,
      [journalId, ln.account_code, ln.debit, ln.credit, ln.description],
    );
  }
}

export async function upsertProcurementDraftAccrual(args: {
  origin: ProcurementOrigin;
  originId: number;
  vendorName: string;
}): Promise<ProcurementGlResult> {
  const lines = await buildAccrualLines(args.origin, args.originId);
  const journalId = await withTransaction(async (q) => {
    const col = args.origin === 'pr' ? 'pr_id' : 'po_id';
    const existing = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE ${col} = $1 AND step = 'accrual' AND is_draft = TRUE
        LIMIT 1`,
      [args.originId],
    );
    let id: number;
    if (existing.rows[0]) {
      id = existing.rows[0].id;
    } else {
      const desc = `DRAFT accrual: ${args.vendorName} (${args.origin.toUpperCase()}-${args.originId})`;
      const ins = await q<{ id: number }>(
        `INSERT INTO journal_entries (${col}, description, is_draft, draft_source, step)
         VALUES ($1, $2, TRUE, $3, 'accrual')
         RETURNING id`,
        [args.originId, desc, args.origin],
      );
      id = ins.rows[0].id;
    }
    await q(`DELETE FROM ledger_lines WHERE journal_entry_id = $1`, [id]);
    return id;
  });
  await insertLines(journalId, lines);
  return { journalId, lines };
}

export async function upsertProcurementDraftSettlement(args: {
  origin: ProcurementOrigin;
  originId: number;
  vendorName: string;
}): Promise<ProcurementGlResult> {
  const lines = await buildSettlementLines(args.origin, args.originId);
  const journalId = await withTransaction(async (q) => {
    const col = args.origin === 'pr' ? 'pr_id' : 'po_id';
    const existing = await q<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE ${col} = $1 AND step = 'settlement' AND is_draft = TRUE
        LIMIT 1`,
      [args.originId],
    );
    let id: number;
    if (existing.rows[0]) {
      id = existing.rows[0].id;
    } else {
      const desc = `DRAFT settlement: ${args.vendorName} (${args.origin.toUpperCase()}-${args.originId})`;
      const ins = await q<{ id: number }>(
        `INSERT INTO journal_entries (${col}, description, is_draft, draft_source, step)
         VALUES ($1, $2, TRUE, $3, 'settlement')
         RETURNING id`,
        [args.originId, desc, args.origin],
      );
      id = ins.rows[0].id;
    }
    await q(`DELETE FROM ledger_lines WHERE journal_entry_id = $1`, [id]);
    return id;
  });
  await insertLines(journalId, lines);
  return { journalId, lines };
}

export async function finalizeProcurementDraft(args: {
  journalId: number;
  actorId: number;
}): Promise<{ journalId: number } | null> {
  return withTransaction(async (q) => {
    const r = await q<{ id: number; step: string; pr_id: number | null; po_id: number | null }>(
      `SELECT id, step, pr_id, po_id
         FROM journal_entries
        WHERE id = $1 AND is_draft = TRUE
        LIMIT 1`,
      [args.journalId],
    );
    const draft = r.rows[0];
    if (!draft) return null;
    await q(
      `UPDATE journal_entries
          SET is_draft = FALSE,
              finalized_at = now(),
              finalized_by = $1
        WHERE id = $2`,
      [args.actorId, draft.id],
    );
    return { journalId: draft.id };
  });
}

export async function loadDraftProcurementJournal(args: {
  origin: ProcurementOrigin;
  originId: number;
  step: ProcurementStep;
}): Promise<{ journalId: number } | null> {
  const col = args.origin === 'pr' ? 'pr_id' : 'po_id';
  const r = await query<{ id: number }>(
    `SELECT id FROM journal_entries
      WHERE ${col} = $1 AND step = $2 AND is_draft = TRUE
      LIMIT 1`,
    [args.originId, args.step],
  );
  return r.rows[0] ? { journalId: r.rows[0].id } : null;
}
