import 'server-only';
import { withTransaction } from '@/db';
import { postJournalInTransaction, PostingError, type FinanceQuery } from './journals';
import type { PostingActor } from './types';

interface OpenDoc {
  id: string;
  branch_id: string;
  customer_id?: number;
  vendor_id?: string | null;
  employee_id?: number | null;
  document_no: string;
  currency_code: string;
  open_foreign: string;
  open_thb: string;
  status: string;
}

function amount(value: number, name: string) {
  if (!Number.isFinite(value) || value <= 0) throw new PostingError(`${name} must be positive`);
  return Math.round(value * 100) / 100;
}

async function fxRate(q: FinanceQuery, currency: string, date: string, explicit?: number) {
  if (currency === 'THB') return 1;
  if (explicit && Number.isFinite(explicit) && explicit > 0) return explicit;
  const result = await q<{ rate_to_thb: string }>(
    `SELECT rate_to_thb::text FROM finance.fx_rates
      WHERE currency_code = $1 AND rate_date <= $2
      ORDER BY rate_date DESC LIMIT 1`,
    [currency, date],
  );
  if (!result.rows[0]) throw new PostingError(`No approved ${currency} FX rate is available for ${date}`);
  return Number(result.rows[0].rate_to_thb);
}

export async function allocateReceipt(args: {
  arDocumentId: number;
  allocationDate: string;
  foreignAmount: number;
  whtAmountThb?: number;
  bankAccountCode?: string;
  fxRate?: number;
  sourceEventKey: string;
  actor: PostingActor;
}) {
  return withTransaction(async (q) => {
    const prior = await q<{ id: string; journal_id: string }>(
      `SELECT a.id::text, a.journal_id::text
         FROM finance.ar_allocations a
         JOIN finance.journals j ON j.id = a.journal_id
        WHERE j.source_event_key = $1`,
      [args.sourceEventKey],
    );
    if (prior.rows[0]) return { allocationId: Number(prior.rows[0].id), journalId: Number(prior.rows[0].journal_id) };
    const found = await q<OpenDoc>(
      `SELECT id::text, branch_id::text, customer_id, document_no, currency_code,
              open_foreign::text, open_thb::text, status
         FROM finance.ar_documents WHERE id = $1 FOR UPDATE`,
      [args.arDocumentId],
    );
    const doc = found.rows[0];
    if (!doc || !['open', 'partially_paid'].includes(doc.status)) throw new PostingError('AR document is not open');
    const foreign = amount(args.foreignAmount, 'Receipt amount');
    const openForeign = Number(doc.open_foreign);
    const openThb = Number(doc.open_thb);
    if (foreign > openForeign) throw new PostingError('Receipt exceeds the open AR amount');
    const rate = await fxRate(q, doc.currency_code, args.allocationDate, args.fxRate);
    const settledThb = Math.round(foreign * rate * 100) / 100;
    const carryingThb = foreign === openForeign ? openThb : Math.round((openThb * foreign / openForeign) * 100) / 100;
    const wht = Math.round(Number(args.whtAmountThb ?? 0) * 100) / 100;
    if (wht < 0 || wht > settledThb) throw new PostingError('WHT cannot exceed the receipt amount');
    const bank = settledThb - wht;
    const realized = settledThb - carryingThb;
    const lines = [
      { accountCode: args.bankAccountCode ?? '110200', description: `Receipt ${doc.document_no}`, debitThb: bank, branchId: Number(doc.branch_id), customerId: doc.customer_id },
      ...(wht > 0 ? [{ accountCode: '110600', description: `WHT receivable ${doc.document_no}`, debitThb: wht, branchId: Number(doc.branch_id), customerId: doc.customer_id }] : []),
      { accountCode: '110400', description: `Clear AR ${doc.document_no}`, creditThb: carryingThb, foreignAmount: foreign, currencyCode: doc.currency_code, branchId: Number(doc.branch_id), customerId: doc.customer_id },
      ...(realized > 0
        ? [{ accountCode: '420100', description: `Realized FX gain ${doc.document_no}`, creditThb: realized, branchId: Number(doc.branch_id), customerId: doc.customer_id }]
        : realized < 0
          ? [{ accountCode: '520100', description: `Realized FX loss ${doc.document_no}`, debitThb: -realized, branchId: Number(doc.branch_id), customerId: doc.customer_id }]
          : []),
    ];
    const journal = await postJournalInTransaction(q, {
      postingDate: args.allocationDate,
      description: `Customer receipt allocated to ${doc.document_no}`,
      currencyCode: doc.currency_code,
      fxRate: rate,
      sourceType: 'ar_receipt',
      sourceId: String(args.arDocumentId),
      sourceEventKey: args.sourceEventKey,
      branchId: Number(doc.branch_id),
      lines,
    }, args.actor);
    const nextForeign = Math.round((openForeign - foreign) * 100) / 100;
    const nextThb = Math.round((openThb - carryingThb) * 100) / 100;
    const allocation = await q<{ id: string }>(
      `INSERT INTO finance.ar_allocations
         (ar_document_id, allocation_date, foreign_amount, functional_amount,
          wht_amount_thb, realized_fx_thb, journal_id, allocated_by)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
       RETURNING id::text`,
      [args.arDocumentId, args.allocationDate, foreign, settledThb, wht, realized, journal.id, args.actor.id],
    );
    await q(
      `UPDATE finance.ar_documents
          SET open_foreign = $2, open_thb = $3,
              status = CASE WHEN $2::numeric = 0 THEN 'paid' ELSE 'partially_paid' END
        WHERE id = $1`,
      [args.arDocumentId, nextForeign, nextThb],
    );
    return { allocationId: Number(allocation.rows[0].id), journalId: journal.id };
  });
}

export interface AllocatePaymentArgs {
  apDocumentId: number;
  allocationDate: string;
  foreignAmount: number;
  whtAmountThb?: number;
  bankAccountCode?: string;
  fxRate?: number;
  sourceEventKey: string;
  actor: PostingActor;
  waybillId?: string | null;
}

export async function allocatePaymentInTransaction(q: FinanceQuery, args: AllocatePaymentArgs) {
    const prior = await q<{ id: string; journal_id: string }>(
      `SELECT a.id::text, a.journal_id::text
         FROM finance.ap_allocations a
         JOIN finance.journals j ON j.id = a.journal_id
        WHERE j.source_event_key = $1`,
      [args.sourceEventKey],
    );
    if (prior.rows[0]) return { allocationId: Number(prior.rows[0].id), journalId: Number(prior.rows[0].journal_id) };
    const found = await q<OpenDoc>(
      `SELECT id::text, branch_id::text, vendor_id::text, employee_id, document_no,
              currency_code, open_foreign::text, open_thb::text, status
         FROM finance.ap_documents WHERE id = $1 FOR UPDATE`,
      [args.apDocumentId],
    );
    const doc = found.rows[0];
    if (!doc || !['open', 'partially_paid'].includes(doc.status)) throw new PostingError('AP document is not open');
    const foreign = amount(args.foreignAmount, 'Payment amount');
    const openForeign = Number(doc.open_foreign);
    const openThb = Number(doc.open_thb);
    if (foreign > openForeign) throw new PostingError('Payment exceeds the open AP amount');
    const rate = await fxRate(q, doc.currency_code, args.allocationDate, args.fxRate);
    const settledThb = Math.round(foreign * rate * 100) / 100;
    const carryingThb = foreign === openForeign ? openThb : Math.round((openThb * foreign / openForeign) * 100) / 100;
    const wht = Math.round(Number(args.whtAmountThb ?? 0) * 100) / 100;
    if (wht < 0 || wht > settledThb) throw new PostingError('WHT cannot exceed the payment amount');
    const bank = settledThb - wht;
    const realized = settledThb - carryingThb;
    const payableCode = doc.employee_id ? '210500' : '210100';
    const lines = [
      { accountCode: payableCode, description: `Clear AP ${doc.document_no}`, debitThb: carryingThb, foreignAmount: foreign, currencyCode: doc.currency_code, branchId: Number(doc.branch_id), vendorId: doc.vendor_id ? Number(doc.vendor_id) : null, employeeId: doc.employee_id },
      { accountCode: args.bankAccountCode ?? '110200', description: `Payment ${doc.document_no}`, creditThb: bank, branchId: Number(doc.branch_id), vendorId: doc.vendor_id ? Number(doc.vendor_id) : null, employeeId: doc.employee_id },
      ...(wht > 0 ? [{ accountCode: '210400', description: `WHT payable ${doc.document_no}`, creditThb: wht, branchId: Number(doc.branch_id), vendorId: doc.vendor_id ? Number(doc.vendor_id) : null, employeeId: doc.employee_id }] : []),
      ...(realized > 0
        ? [{ accountCode: '520100', description: `Realized FX loss ${doc.document_no}`, debitThb: realized, branchId: Number(doc.branch_id) }]
        : realized < 0
          ? [{ accountCode: '420100', description: `Realized FX gain ${doc.document_no}`, creditThb: -realized, branchId: Number(doc.branch_id) }]
          : []),
    ];
    const journal = await postJournalInTransaction(q, {
      postingDate: args.allocationDate,
      description: `Payment allocated to ${doc.document_no}`,
      currencyCode: doc.currency_code,
      fxRate: rate,
      sourceType: 'ap_payment',
      sourceId: String(args.apDocumentId),
      sourceEventKey: args.sourceEventKey,
      branchId: Number(doc.branch_id),
      waybillId: args.waybillId,
      lines: lines.map((line) => ({ ...line, waybillId: args.waybillId })),
    }, args.actor);
    const nextForeign = Math.round((openForeign - foreign) * 100) / 100;
    const nextThb = Math.round((openThb - carryingThb) * 100) / 100;
    const allocation = await q<{ id: string }>(
      `INSERT INTO finance.ap_allocations
         (ap_document_id, allocation_date, foreign_amount, functional_amount,
          wht_amount_thb, realized_fx_thb, journal_id, allocated_by)
       VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
       RETURNING id::text`,
      [args.apDocumentId, args.allocationDate, foreign, settledThb, wht, realized, journal.id, args.actor.id],
    );
    await q(
      `UPDATE finance.ap_documents
          SET open_foreign = $2, open_thb = $3,
              status = CASE WHEN $2::numeric = 0 THEN 'paid' ELSE 'partially_paid' END
        WHERE id = $1`,
      [args.apDocumentId, nextForeign, nextThb],
    );
    return { allocationId: Number(allocation.rows[0].id), journalId: journal.id };
}

export async function allocatePayment(args: AllocatePaymentArgs) {
  return withTransaction((q) => allocatePaymentInTransaction(q, args));
}

export async function refundArCredit(args: {
  arDocumentId: number;
  refundDate: string;
  foreignAmount: number;
  bankAccountCode?: string;
  fxRate?: number;
  sourceEventKey: string;
  actor: PostingActor;
}) {
  return withTransaction(async (q) => {
    const prior = await q<{ id: string; journal_id: string }>(
      `SELECT a.id::text, a.journal_id::text
         FROM finance.ar_allocations a
         JOIN finance.journals j ON j.id = a.journal_id
        WHERE j.source_event_key = $1`,
      [args.sourceEventKey],
    );
    if (prior.rows[0]) return { allocationId: Number(prior.rows[0].id), journalId: Number(prior.rows[0].journal_id) };
    const found = await q<OpenDoc & { document_type: string }>(
      `SELECT id::text, branch_id::text, customer_id, document_no, document_type,
              currency_code, open_foreign::text, open_thb::text, status
         FROM finance.ar_documents WHERE id = $1 FOR UPDATE`,
      [args.arDocumentId],
    );
    const doc = found.rows[0];
    if (!doc || doc.document_type !== 'credit_note' || !['open', 'partially_paid'].includes(doc.status)) {
      throw new PostingError('AR credit note is not open');
    }
    const foreign = amount(args.foreignAmount, 'Refund amount');
    const openForeign = Math.abs(Number(doc.open_foreign));
    const openThb = Math.abs(Number(doc.open_thb));
    if (foreign > openForeign) throw new PostingError('Refund exceeds the open credit note amount');
    const rate = await fxRate(q, doc.currency_code, args.refundDate, args.fxRate);
    const paidThb = Math.round(foreign * rate * 100) / 100;
    const carryingThb = foreign === openForeign ? openThb : Math.round(openThb * foreign / openForeign * 100) / 100;
    const realized = paidThb - carryingThb;
    const lines = [
      { accountCode: '110400', description: `Refund credit ${doc.document_no}`, debitThb: carryingThb, foreignAmount: foreign, currencyCode: doc.currency_code, branchId: Number(doc.branch_id), customerId: doc.customer_id },
      { accountCode: args.bankAccountCode ?? '110200', description: `Customer refund ${doc.document_no}`, creditThb: paidThb, branchId: Number(doc.branch_id), customerId: doc.customer_id },
      ...(realized > 0
        ? [{ accountCode: '520100', description: `Refund FX loss ${doc.document_no}`, debitThb: realized, branchId: Number(doc.branch_id), customerId: doc.customer_id }]
        : realized < 0
          ? [{ accountCode: '420100', description: `Refund FX gain ${doc.document_no}`, creditThb: -realized, branchId: Number(doc.branch_id), customerId: doc.customer_id }]
          : []),
    ];
    const journal = await postJournalInTransaction(q, {
      postingDate: args.refundDate,
      description: `Customer refund for ${doc.document_no}`,
      currencyCode: doc.currency_code,
      fxRate: rate,
      sourceType: 'ar_refund',
      sourceId: String(args.arDocumentId),
      sourceEventKey: args.sourceEventKey,
      branchId: Number(doc.branch_id),
      lines,
    }, args.actor);
    const nextForeign = Math.round((openForeign - foreign) * 100) / 100;
    const nextThb = Math.round((openThb - carryingThb) * 100) / 100;
    const allocation = await q<{ id: string }>(
      `INSERT INTO finance.ar_allocations
         (ar_document_id, allocation_date, foreign_amount, functional_amount,
          realized_fx_thb, journal_id, allocated_by)
       VALUES ($1,$2,$3,$4,$5,$6,$7)
       RETURNING id::text`,
      [args.arDocumentId, args.refundDate, foreign, paidThb, realized, journal.id, args.actor.id],
    );
    await q(
      `UPDATE finance.ar_documents
          SET open_foreign = -$2, open_thb = -$3,
              status = CASE WHEN $2::numeric = 0 THEN 'paid' ELSE 'partially_paid' END
        WHERE id = $1`,
      [args.arDocumentId, nextForeign, nextThb],
    );
    return { allocationId: Number(allocation.rows[0].id), journalId: journal.id };
  });
}

export async function revalueForeignBalances(args: {
  asOfDate: string;
  branchId: number;
  sourceEventKey: string;
  actor: PostingActor;
}) {
  return withTransaction(async (q) => {
    const rows = await q<OpenDoc & { subledger: 'AR' | 'AP' }>(
      `SELECT 'AR'::text AS subledger, id::text, branch_id::text, customer_id,
              NULL::text AS vendor_id, NULL::integer AS employee_id, document_no,
              currency_code, open_foreign::text, open_thb::text, status
         FROM finance.ar_documents
        WHERE branch_id = $1 AND status IN ('open','partially_paid') AND currency_code <> 'THB'
       UNION ALL
       SELECT 'AP', id::text, branch_id::text, NULL, vendor_id::text, employee_id,
              document_no, currency_code, open_foreign::text, open_thb::text, status
         FROM finance.ap_documents
        WHERE branch_id = $1 AND status IN ('open','partially_paid') AND currency_code <> 'THB'
       FOR UPDATE`,
      [args.branchId],
    );
    const lines: Array<{
      accountCode: string;
      description: string;
      debitThb?: number;
      creditThb?: number;
      branchId: number;
      customerId?: number | null;
      vendorId?: number | null;
      employeeId?: number | null;
    }> = [];
    const changes: Array<{ subledger: 'AR' | 'AP'; id: number; value: number }> = [];
    for (const doc of rows.rows) {
      const rate = await fxRate(q, doc.currency_code, args.asOfDate);
      const value = Math.round(Number(doc.open_foreign) * rate * 100) / 100;
      const delta = Math.round((value - Number(doc.open_thb)) * 100) / 100;
      if (delta === 0) continue;
      const control = doc.subledger === 'AR' ? '110400' : doc.employee_id ? '210500' : '210100';
      const dims = { branchId: args.branchId, customerId: doc.customer_id, vendorId: doc.vendor_id ? Number(doc.vendor_id) : null, employeeId: doc.employee_id };
      if (doc.subledger === 'AR') {
        lines.push(
          delta > 0
            ? { accountCode: control, description: `AR revaluation ${doc.document_no}`, debitThb: delta, ...dims }
            : { accountCode: control, description: `AR revaluation ${doc.document_no}`, creditThb: -delta, ...dims },
          delta > 0
            ? { accountCode: '420200', description: `Unrealized FX gain ${doc.document_no}`, creditThb: delta, ...dims }
            : { accountCode: '520300', description: `Unrealized FX loss ${doc.document_no}`, debitThb: -delta, ...dims },
        );
      } else {
        lines.push(
          delta > 0
            ? { accountCode: control, description: `AP revaluation ${doc.document_no}`, creditThb: delta, ...dims }
            : { accountCode: control, description: `AP revaluation ${doc.document_no}`, debitThb: -delta, ...dims },
          delta > 0
            ? { accountCode: '520300', description: `Unrealized FX loss ${doc.document_no}`, debitThb: delta, ...dims }
            : { accountCode: '420200', description: `Unrealized FX gain ${doc.document_no}`, creditThb: -delta, ...dims },
        );
      }
      changes.push({ subledger: doc.subledger, id: Number(doc.id), value });
    }
    if (!lines.length) return { journalId: null, revalued: 0 };
    const journal = await postJournalInTransaction(q, {
      postingDate: args.asOfDate,
      description: `Foreign currency revaluation at ${args.asOfDate}`,
      sourceType: 'fx_revaluation',
      sourceId: `${args.branchId}:${args.asOfDate}`,
      sourceEventKey: args.sourceEventKey,
      branchId: args.branchId,
      lines,
    }, args.actor);
    for (const change of changes) {
      await q(
        `UPDATE finance.${change.subledger === 'AR' ? 'ar_documents' : 'ap_documents'} SET open_thb = $2 WHERE id = $1`,
        [change.id, change.value],
      );
    }
    return { journalId: journal.id, revalued: changes.length };
  });
}
