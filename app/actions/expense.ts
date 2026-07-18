'use server';

import { revalidatePath } from 'next/cache';
import { z } from 'zod';
import { query, withTransaction, query as _query } from '@/db';
import { recordAttachment } from '@/waybill/attachments';
import { recordEvent } from '@/waybill/events';
import { loadWaybill } from '@/waybill/queries';
import { requireActionFor } from '@/server/requireActionFor';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { hasPermission } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { appendWaybillEvent } from '@/waybill/append';
import { aiInvoke } from '@/ai/router';
import { publish as publishEvent } from '@/notifications/events';
import { type WbForCheck } from './_helpers';
import {
  assertExpenseClaim,
  authorizeExpenseStage,
  loadExpenseFlowContext,
  type ExpenseActor,
} from '@/waybill/expenseFlow';

async function semanticCoaMatch(description: string): Promise<{ code: string | null; score: number }> {
  if (!description || !description.trim()) return { code: null, score: 0 };
  const res = await aiInvoke('acct:coa-search', 'embed', { text: description });
  if (!res.ok || !res.embedding) return { code: null, score: 0 };
  const vectorStr = `[${res.embedding.join(',')}]`;
  const matchRes = await query(
    `SELECT code, (1 - (embedding <=> $1::vector)) AS similarity
     FROM chart_of_accounts
     ORDER BY similarity DESC LIMIT 1`,
    [vectorStr]
  );
  if (matchRes.rows.length === 0) return { code: null, score: 0 };
  return { code: matchRes.rows[0].code, score: matchRes.rows[0].similarity };
}

function canSettleExpense(actor: ActorWithScope, wb: WbForCheck): boolean {
  return wb.current_stage === 'payment'
    && hasPermission(actor, PERM.finance.expense.pay);
}
export async function submitExpenseFromSlip(args: {
  slipId: number;
  actorId: number;
  draftWaybillId?: string;
  overrides?: {
    vendorName?: string;
    vendorAddress?: string;
    createdTo?: string;
    createdToAddress?: string;
    transactionDate?: string;
    subtotal?: number;
    vatAmount?: number;
    totalAmount?: number;
    paymentMethod?: string;
    payeeType?: 'employee' | 'vendor';
    bookBankSlipId?: number;
    bookBankFields?: {
      bankName?: string;
      bankBranch?: string;
      accountNumber?: string;
      accountName?: string;
    };
    items?: Array<{ qty?: number; unitPrice?: number; amount?: number; description: string }>;
  };
}) {
  try {
    await requireActionFor(args.actorId, 'submit_expense', { perm: PERM.finance.expense.create });

    const slipRes = await query(`SELECT * FROM slips WHERE id = $1`, [args.slipId]);
    if (slipRes.rows.length === 0) throw new Error('Slip not found');
    const slip = slipRes.rows[0];
    if (slip.kind === 'book_bank') {
      throw new Error('Book bank slips are confirmed together with a receipt slip');
    }
    const parsed = slip.ocr_raw_json || {};

    let draftContext: { waybillId: string; expenseId: number } | null = null;
    if (args.draftWaybillId) {
      const wbRes = await query<{ id: string; submitter_id: number; origin_id: number; current_stage: string }>(
        `SELECT id, submitter_id, origin_id, current_stage
           FROM waybills WHERE id = $1`,
        [args.draftWaybillId],
      );
      const wb = wbRes.rows[0];
      if (!wb) throw new Error('Draft waybill not found');
      if (wb.submitter_id !== args.actorId) throw new Error('Not your draft');
      if (wb.current_stage !== 'draft') throw new Error('Draft already finalized');
      draftContext = { waybillId: wb.id, expenseId: wb.origin_id };
    }

    const bookBankSlipId = args.overrides?.bookBankSlipId;
    const bookBankFields = args.overrides?.bookBankFields;
    let bookBankSlip: any = null;
    if (bookBankSlipId) {
      if (!bookBankFields || !bookBankFields.bankName || !bookBankFields.accountNumber || !bookBankFields.accountName) {
        throw new Error('Transfer expenses require bank name, account number, and account name');
      }
      if (String(args.overrides?.paymentMethod ?? parsed.paymentMethod ?? 'cash') !== 'transfer') {
        throw new Error('Book bank slip is only valid with payment_method=transfer');
      }
      const bbRes = await query(`SELECT * FROM slips WHERE id = $1`, [bookBankSlipId]);
      if (bbRes.rows.length === 0) throw new Error('Book bank slip not found');
      bookBankSlip = bbRes.rows[0];
      if (bookBankSlip.kind !== 'book_bank') {
        throw new Error('Slip is not a book bank slip');
      }
      if (bookBankSlip.uploaded_by !== args.actorId) {
        throw new Error('Only the uploader can attach a book bank slip');
      }
      if (bookBankSlip.status !== 'pending' || bookBankSlip.expense_id != null) {
        throw new Error('Book bank slip is already attached to another expense');
      }
    }

    const vendor = args.overrides?.vendorName || parsed.vendorName || 'Unknown Vendor';
    const vendorAddress = args.overrides?.vendorAddress || parsed.vendorAddress || '';
    const createdTo = args.overrides?.createdTo || parsed.createdTo || '';
    const createdToAddress = args.overrides?.createdToAddress || parsed.createdToAddress || '';
    const txnDate = args.overrides?.transactionDate || parsed.transactionDate || new Date().toISOString().split('T')[0];
    const subtotal = Number(args.overrides?.subtotal ?? parsed.subtotal ?? 0);
    const vatAmount = Number(args.overrides?.vatAmount ?? parsed.vatAmount ?? 0);
    const totalAmount = Number(args.overrides?.totalAmount ?? parsed.totalAmount ?? subtotal + vatAmount);
    const paymentMethod = args.overrides?.paymentMethod || parsed.paymentMethod || 'cash';
    const payeeType = args.overrides?.payeeType === 'vendor' ? 'vendor' : 'employee';
    const isCorrupted = !!parsed.isCorrupted;
    const correctionNotes = parsed.correctionNotes || '';
    const preExistingExpenseId = draftContext ? null : (slip.expense_id ?? null);

    await query('BEGIN');

    let expenseId: number;
    let previousStatus: string | null = null;
    if (draftContext) {
      expenseId = draftContext.expenseId;
      previousStatus = 'draft';
      await query(
        `UPDATE expenses
            SET vendor_name = $1, transaction_date = $2, subtotal = $3, vat_amount = $4, total_amount = $5,
                payment_method = $6, is_corrupted = $7, correction_notes = $8, ocr_raw_json = $9,
                document_url = $10, status = 'department_approval', created_to = $12, vendor_address = $13, created_to_address = $14,
                payee_type = $15, updated_at = CURRENT_TIMESTAMP
          WHERE id = $11`,
        [
          vendor, txnDate, subtotal, vatAmount, totalAmount, paymentMethod,
          isCorrupted, correctionNotes, JSON.stringify(parsed),
          `/api/slips/file?key=${encodeURIComponent(slip.file_path)}`,
          expenseId,
          createdTo || null,
          vendorAddress || null,
          createdToAddress || null,
          payeeType,
        ],
      );
      await query(`DELETE FROM expense_items WHERE expense_id = $1`, [expenseId]);
      await query(
        `UPDATE slips
            SET expense_id = $1, status = 'confirmed', confirmed_at = CURRENT_TIMESTAMP
          WHERE id = $2`,
        [expenseId, args.slipId],
      );
      await query(
        `UPDATE waybills
            SET vendor_name = $1, total_amount = $2, current_stage = 'department_approval',
                created_to = $4, vendor_address = $5, created_to_address = $6, currency = 'THB', updated_at = now()
          WHERE id = $3`,
        [vendor, totalAmount, draftContext.waybillId, createdTo || null, vendorAddress || null, createdToAddress || null],
      );
    } else if (preExistingExpenseId) {
      expenseId = preExistingExpenseId;
      const cur = await query(`SELECT status FROM expenses WHERE id = $1`, [expenseId]);
      previousStatus = cur.rows[0]?.status ?? null;
      await query(
        `UPDATE expenses
            SET vendor_name = $1, transaction_date = $2, subtotal = $3, vat_amount = $4, total_amount = $5,
                payment_method = $6, is_corrupted = $7, correction_notes = $8, ocr_raw_json = $9,
                document_url = $10, created_to = $12, vendor_address = $13, created_to_address = $14,
                payee_type = $15, updated_at = CURRENT_TIMESTAMP
          WHERE id = $11`,
        [
          vendor, txnDate, subtotal, vatAmount, totalAmount, paymentMethod,
          isCorrupted, correctionNotes, JSON.stringify(parsed),
          `/api/slips/file?key=${encodeURIComponent(slip.file_path)}`,
          expenseId,
          createdTo || null,
          vendorAddress || null,
          createdToAddress || null,
          payeeType,
        ],
      );
      await query(`DELETE FROM expense_items WHERE expense_id = $1`, [expenseId]);
    } else {
      const headerRes = await query(
        `INSERT INTO expenses (
           submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount,
           payment_method, status, is_corrupted, correction_notes, ocr_raw_json, document_url,
           created_to, vendor_address, created_to_address, payee_type
         )
         VALUES ($1,$2,$3,$4,$5,$6,$7,'department_approval',$8,$9,$10,$11,$12,$13,$14,$15)
         RETURNING id`,
        [
          args.actorId, vendor, txnDate, subtotal, vatAmount, totalAmount, paymentMethod,
          isCorrupted, correctionNotes, JSON.stringify(parsed),
          `/api/slips/file?key=${encodeURIComponent(slip.file_path)}`,
          createdTo || null,
          vendorAddress || null,
          createdToAddress || null,
          payeeType,
        ]
      );
      expenseId = headerRes.rows[0].id;

      await query(
        `UPDATE slips
            SET expense_id = $1, status = 'confirmed', confirmed_at = CURRENT_TIMESTAMP
          WHERE id = $2`,
        [expenseId, args.slipId],
      );
    }

    if (bookBankSlip) {
      const accountNumber = bookBankFields!.accountNumber!.replace(/[^\d]/g, '');
      const bankBranch = bookBankFields!.bankBranch?.trim() || null;
      await query(
        `UPDATE slips
            SET expense_id      = $1,
                status          = 'confirmed',
                confirmed_at    = CURRENT_TIMESTAMP,
                bank_name       = $2,
                bank_branch     = $3,
                account_number  = $4,
                account_name    = $5
          WHERE id = $6`,
        [
          expenseId,
          bookBankFields!.bankName,
          bankBranch,
          accountNumber,
          bookBankFields!.accountName,
          bookBankSlip.id,
        ],
      );
    }

    const items = args.overrides?.items ?? (Array.isArray(parsed.items) ? parsed.items : []);
    for (const item of items) {
      let bestCode: string | null = null;
      let score = 0;
      try {
        const match = await semanticCoaMatch(item.description);
        if (match.code) { bestCode = match.code; score = match.score; }
      } catch {}
      const qty = Number(item.qty ?? 1.00);
      const unitPrice = Number(item.unitPrice ?? item.amount ?? 0.00);
      await query(
        `INSERT INTO expense_items (expense_id, description, qty, unit_price, amount, mapped_account_code, confidence_score)
         VALUES ($1,$2,$3,$4,$5,$6,$7)`,
        [expenseId, item.description, qty, unitPrice, Number(item.amount) || 0, bestCode, score]
      );
    }

    const initialStatus = preExistingExpenseId
      ? previousStatus || 'department_approval'
      : 'department_approval';

    if (!preExistingExpenseId || initialStatus !== previousStatus) {
      await query(`UPDATE expenses SET status = $1 WHERE id = $2`, [initialStatus, expenseId]);
    }

    await query('COMMIT');
    const waybillId = await appendWaybillEvent({
      origin: 'expense',
      originId: expenseId,
      kind: 'submitted',
      stageFrom: draftContext ? 'draft' : null,
      stageTo: initialStatus,
      actorId: args.actorId,
      payload: { vendor, totalAmount, vatAmount },
    });
    if (waybillId) {
      await query(
        `UPDATE waybills
            SET current_stage = $2, current_owner_role = $2, current_owner_user_id = NULL, updated_at = now()
          WHERE id = $1`,
        [waybillId, initialStatus],
      );
    }
    if (waybillId && bookBankSlip) {
      const caption = bookBankFields!.bankName
        ? `Book bank · ${bookBankFields!.bankName}${bookBankFields!.accountNumber ? ' · ' + bookBankFields!.accountNumber : ''}`
        : 'Book bank slip';
      await recordAttachment({
        waybillId,
        stageKey: 'submission',
        kind: 'slip',
        storageKey: bookBankSlip.file_path,
        filename: bookBankSlip.file_path.split('/').pop() || 'book-bank',
        contentType: bookBankSlip.mime_type || 'application/octet-stream',
        byteSize: bookBankSlip.file_size || 0,
        actorId: bookBankSlip.uploaded_by,
        actorRole: 'staff',
        caption,
      });
    }
    await publishEvent('expense.submitted', { expenseId, status: initialStatus }, {
      actorId: args.actorId, refType: 'expense', refId: Number(expenseId),
      severity: 'info',
      message: `Submitted expense #EXP-${expenseId} initial status ${initialStatus}`,
    });
    revalidatePath('/');
    return { success: true, expenseId, waybillId, status: initialStatus, policy: null, slipStatus: 'confirmed' };
  } catch (error: any) {
    await query('ROLLBACK');
    console.error('submitExpenseFromSlip failed:', error);
    return { success: false, error: error.message };
  }
}

const AttachPaymentSlipForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  expenseId: z.coerce.number().int().positive(),
  slipId: z.coerce.number().int().positive(),
  paymentMethod: z.enum(['cash', 'credit_card', 'transfer']),
  paymentDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  amount: z.coerce.number().positive(),
  bankName: z.string().min(1).max(150),
  accountNumber: z.string().max(50).optional(),
  payee: z.string().min(1).max(180),
  reference: z.string().min(1).max(180),
});

export async function attachPaymentSlipAction(formData: FormData): Promise<{ ok: boolean; error?: string }> {
  const parsed = AttachPaymentSlipForm.safeParse({
    waybillId: String(formData.get('waybillId') ?? ''),
    expenseId: String(formData.get('expenseId') ?? '0'),
    slipId: String(formData.get('slipId') ?? '0'),
    paymentMethod: String(formData.get('paymentMethod') ?? 'transfer'),
    paymentDate: String(formData.get('paymentDate') ?? ''),
    amount: String(formData.get('amount') ?? '0'),
    bankName: String(formData.get('bankName') ?? '').trim(),
    accountNumber: String(formData.get('accountNumber') ?? '').trim(),
    payee: String(formData.get('payee') ?? '').trim(),
    reference: String(formData.get('reference') ?? '').trim(),
  });
  if (!parsed.success) {
    return { ok: false, error: parsed.error.issues[0]?.message ?? 'invalid input' };
  }

  const wb = await loadWaybill(parsed.data.waybillId);
  if (!wb) return { ok: false, error: 'waybill not found' };
  if (wb.origin !== 'expense') {
    return { ok: false, error: `attach-payment-slip only for expense origin (got ${wb.origin})` };
  }
  if (wb.origin_id !== parsed.data.expenseId) {
    return { ok: false, error: 'expense does not belong to this waybill' };
  }
  if (wb.current_stage !== 'payment') {
    return { ok: false, error: `expense must be at payment (current: ${wb.current_stage})` };
  }

  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthorized' };
  if (!canSettleExpense(actor, wb)) {
    return { ok: false, error: 'cannot settle at this stage' };
  }
  const flowActor: ExpenseActor = {
    id: actor.id,
    permissions: actor.permissions,
    deptId: actor.dept_id,
    departmentId: actor.dept_id,
    level: actor.level,
    rank: actor.level,
    roleName: actor.role_name,
  };
  const flow = await loadExpenseFlowContext(wb.id);
  const decision = await authorizeExpenseStage(flowActor, flow, 'payment');
  if (!decision.allow) return { ok: false, error: decision.reason };
  try {
    await assertExpenseClaim(actor.id, wb.id, 'payment');
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : 'claim required' };
  }

  const slipRes = await _query<{
    id: number;
    uploaded_by: number;
    status: string;
    expense_id: number | null;
    ocr_raw_json: unknown;
    ocr_confidence: number | null;
    file_path: string;
    mime_type: string;
    file_size: number;
  }>(
    `SELECT id, uploaded_by, status, expense_id, ocr_raw_json, ocr_confidence,
            file_path, mime_type, file_size
       FROM slips WHERE id = $1`,
    [parsed.data.slipId],
  );
  if (slipRes.rows.length === 0) return { ok: false, error: 'slip not found' };
  const slip = slipRes.rows[0];
  if (slip.status !== 'pending') return { ok: false, error: 'slip must be in pending state' };
  if (slip.uploaded_by !== actor.id) return { ok: false, error: 'Only the task claimant may upload the payment slip' };
  if (slip.expense_id != null && slip.expense_id !== parsed.data.expenseId) {
    return { ok: false, error: 'slip already attached to another expense' };
  }

  const expRes = await _query<{ vendor_name: string; total_amount: string; vat_amount: string }>(
    `SELECT vendor_name, total_amount, vat_amount FROM expenses WHERE id = $1`,
    [parsed.data.expenseId],
  );
  if (expRes.rows.length === 0) return { ok: false, error: 'expense not found' };
  const exp = expRes.rows[0];
  if (Math.abs(Number(exp.total_amount) - parsed.data.amount) > 0.005) {
    return { ok: false, error: 'Payment amount must exactly match the approved amount' };
  }

  try {
    await withTransaction(async (q) => {
      const locked = await q<{ current_stage: string }>(
        `SELECT current_stage FROM waybills WHERE id = $1 FOR UPDATE`,
        [wb.id],
      );
      if (locked.rows[0]?.current_stage !== 'payment') {
        throw new Error('Expense stage changed; refresh and try again');
      }
      await q(
      `INSERT INTO expense_payments
         (waybill_id, expense_id, slip_id, amount, payment_date, bank_name,
          account_number, payee, reference, ocr_payload, ocr_confidence, confirmed_by)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)`,
      [
        wb.id,
        parsed.data.expenseId,
        parsed.data.slipId,
        parsed.data.amount,
        parsed.data.paymentDate,
        parsed.data.bankName,
        parsed.data.accountNumber || null,
        parsed.data.payee,
        parsed.data.reference,
        slip.ocr_raw_json ?? null,
        slip.ocr_confidence,
        actor.id,
      ],
    );
      await q(
      `UPDATE slips SET expense_id = $1, status = 'confirmed', confirmed_at = now(), kind = 'payment_slip'
        WHERE id = $2`,
      [parsed.data.expenseId, parsed.data.slipId],
    );
      await q(
      `UPDATE expenses SET status = 'settlement',
                          payment_method = $1,
                          disbursed_at = now(),
                          disbursed_by = $2,
                          updated_at = now()
        WHERE id = $3`,
      [parsed.data.paymentMethod, actor.id, parsed.data.expenseId],
    );
      await q(
      `UPDATE waybills SET current_stage = 'settlement',
                          status = 'open',
                          updated_at = now()
        WHERE id = $1`,
      [wb.id],
    );
      await recordEvent({
      waybillId: wb.id,
      kind: 'payment-confirmed',
      stageFrom: 'payment',
      stageTo: 'settlement',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'finance',
      payload: {
        paymentMethod: parsed.data.paymentMethod,
        slipId: parsed.data.slipId,
        amount: parsed.data.amount,
        paymentDate: parsed.data.paymentDate,
        bankName: parsed.data.bankName,
        reference: parsed.data.reference,
      },
      client: q as never,
    });
      await recordAttachment({
        waybillId: wb.id,
        stageKey: 'payment',
        kind: 'payment_slip',
        storageKey: slip.file_path,
        filename: `payment-slip-${slip.id}`,
        contentType: slip.mime_type,
        byteSize: slip.file_size,
        actorId: actor.id,
        actorRole: actor.role_name,
        caption: `Payment ${parsed.data.reference}`,
        client: q as typeof query,
      });
      await q(
      `UPDATE waybill_stage_claims
          SET released_at = now(), released_by = $2, release_reason = 'stage completed'
        WHERE waybill_id = $1 AND stage = 'payment' AND released_at IS NULL`,
      [wb.id, actor.id],
    );
    });
  } catch (error) {
    if ((error as { code?: string }).code === '23505') {
      return { ok: false, error: 'This expense has already been paid' };
    }
    return { ok: false, error: error instanceof Error ? error.message : 'Payment confirmation failed' };
  }

  revalidatePath(`/waybill/${wb.id}`);
  return { ok: true };
}
