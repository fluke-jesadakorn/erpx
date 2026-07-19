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
import { loadPostingActor, postJournalInTransaction, type FinanceQuery } from './journals';

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
    const r = await q<{
      id: number;
      step: ProcurementStep;
      pr_id: number | null;
      po_id: number | null;
      description: string;
      branch_id: string;
      currency: string;
      fx_rate: string;
      vendor_name: string | null;
      vendor_id: string | null;
      total: string;
      document_date: string;
      waybill_id: string | null;
    }>(
      `SELECT j.id, j.step, j.pr_id, j.po_id, j.description,
              coalesce(po.branch_id, (SELECT id FROM finance.branches WHERE code = 'HQ'))::text AS branch_id,
              coalesce(po.currency, pr.currency, 'THB')::text AS currency,
              coalesce(po.fx_rate, 1)::text AS fx_rate,
              coalesce(po.vendor_name, pr.vendor_name) AS vendor_name,
              po.vendor_id::text,
              coalesce(po.total_amount, pr.total_estimate, 0)::text AS total,
              coalesce(po.issued_at::date, pr.created_at::date, current_date)::text AS document_date,
              w.id AS waybill_id
         FROM journal_entries j
         LEFT JOIN purchase_orders po ON po.id = j.po_id
         LEFT JOIN purchase_requisitions pr ON pr.id = coalesce(j.pr_id, po.pr_id)
         LEFT JOIN waybills w ON (w.origin = CASE WHEN j.po_id IS NOT NULL THEN 'po' ELSE 'pr' END AND w.origin_id = coalesce(j.po_id, j.pr_id))
        WHERE j.id = $1 AND j.is_draft = TRUE
        LIMIT 1 FOR UPDATE OF j`,
      [args.journalId],
    );
    const draft = r.rows[0];
    if (!draft) return null;
    const origin: ProcurementOrigin = draft.po_id ? 'po' : 'pr';
    const originId = draft.po_id ?? draft.pr_id;
    if (!originId) throw new Error('Procurement source is missing');
    const actor = await loadPostingActor(q as FinanceQuery, args.actorId);
    const fx = draft.currency.trim() === 'THB' ? 1 : Number(draft.fx_rate);
    const postingDate = new Date().toISOString().slice(0, 10);
    const lines = await q<ProcurementGlLine>(
      `SELECT account_code, debit::float8 AS debit, credit::float8 AS credit,
              coalesce(description, '') AS description
         FROM ledger_lines WHERE journal_entry_id = $1 ORDER BY id`,
      [draft.id],
    );
    const official = await postJournalInTransaction(q as FinanceQuery, {
      postingDate,
      documentDate: draft.step === 'accrual' ? draft.document_date : postingDate,
      description: `${origin.toUpperCase()}-${originId} ${draft.step}`,
      currencyCode: draft.currency.trim(),
      fxRate: fx,
      sourceType: origin,
      sourceId: String(originId),
      sourceEventKey: `procurement:${origin}:${originId}:${draft.step}:v1`,
      branchId: Number(draft.branch_id),
      waybillId: draft.waybill_id,
      lines: lines.rows.map((line) => ({
        accountCode: line.account_code,
        description: line.description,
        debitThb: Math.round(line.debit * fx * 100) / 100,
        creditThb: Math.round(line.credit * fx * 100) / 100,
        foreignAmount: line.debit > 0 ? line.debit : -line.credit,
        currencyCode: draft.currency.trim(),
        branchId: Number(draft.branch_id),
        vendorId: draft.vendor_id ? Number(draft.vendor_id) : null,
        waybillId: draft.waybill_id,
        sourceDocumentType: draft.step === 'accrual' ? 'vendor_invoice' : 'payment',
        sourceDocumentId: `${origin.toUpperCase()}-${originId}`,
      })),
    }, actor);
    if (draft.step === 'accrual') {
      let vendorId = draft.vendor_id ? Number(draft.vendor_id) : null;
      if (!vendorId) {
        const vendor = await q<{ id: string }>(
          `INSERT INTO finance.vendors(code, name)
           VALUES ($1,$2)
           ON CONFLICT (code) DO UPDATE SET name = excluded.name
           RETURNING id::text`,
          [`${origin.toUpperCase()}-${originId}`, draft.vendor_name ?? `Vendor ${origin.toUpperCase()}-${originId}`],
        );
        vendorId = Number(vendor.rows[0].id);
        if (draft.po_id) await q(`UPDATE purchase_orders SET vendor_id = $2 WHERE id = $1`, [draft.po_id, vendorId]);
      }
      const total = Number(draft.total);
      const totalThb = Math.round(total * fx * 100) / 100;
      await q(
        `INSERT INTO finance.ap_documents
           (vendor_id, branch_id, document_no, document_type, source_type, source_id,
            document_date, due_date, currency_code, fx_rate, original_foreign,
            open_foreign, original_thb, open_thb, journal_id)
         VALUES ($1,$2,$3,'vendor_invoice',$4,$5,$6,$6,$7,$8,$9,$9,$10,$10,$11)
         ON CONFLICT (vendor_id, document_no) DO NOTHING`,
        [vendorId, Number(draft.branch_id), `${origin.toUpperCase()}-${originId}`, origin, String(originId), draft.document_date, draft.currency.trim(), fx, total, totalThb, official.id],
      );
    } else {
      const ap = await q<{ id: string; open_foreign: string; open_thb: string }>(
        `SELECT id::text, open_foreign::text, open_thb::text
           FROM finance.ap_documents
          WHERE source_type = $1 AND source_id = $2 AND status IN ('open','partially_paid')
          ORDER BY id DESC LIMIT 1 FOR UPDATE`,
        [origin, String(originId)],
      );
      if (ap.rows[0]) {
        await q(
          `INSERT INTO finance.ap_allocations
             (ap_document_id, allocation_date, foreign_amount, functional_amount, journal_id, allocated_by)
           VALUES ($1,current_date,$2,$3,$4,$5)`,
          [Number(ap.rows[0].id), Number(ap.rows[0].open_foreign), Number(ap.rows[0].open_thb), official.id, args.actorId],
        );
        await q(`UPDATE finance.ap_documents SET open_foreign = 0, open_thb = 0, status = 'paid' WHERE id = $1`, [Number(ap.rows[0].id)]);
      }
    }
    await q(
      `UPDATE journal_entries
          SET is_draft = FALSE,
              finalized_at = now(),
              finalized_by = $1
        WHERE id = $2`,
      [args.actorId, draft.id],
    );
    return { journalId: official.id };
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
