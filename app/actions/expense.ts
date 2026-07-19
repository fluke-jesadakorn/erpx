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
import { allocatePaymentInTransaction } from '@/finance/subledger';
import { loadPostingActor } from '@/finance/journals';
import { buildMockExpensePaymentSlip, loadExpensePaymentPreview } from '@/finance/expenseDocument';
import { put, remove } from '@/slips/storage';
import { type WbForCheck } from './_helpers';
import { expenseEntryStage, isExecutiveRole, skippedDepartmentStage } from '@/waybill/routing';
import {
  assertExpenseClaim,
  authorizeExpenseStage,
  loadExpenseFlowContext,
  type ExpenseActor,
} from '@/waybill/expenseFlow';

async function semanticCoaMatch(
  description: string,
  runQuery: typeof query = query,
  actorId?: number,
): Promise<{ code: string | null; score: number }> {
  if (!description || !description.trim()) return { code: null, score: 0 };
  const res = await aiInvoke('acct:coa-search', 'embed', { text: description }, { actorId });
  if (!res.ok || !res.embedding) return { code: null, score: 0 };
  const vectorStr = `[${res.embedding.join(',')}]`;
  const matchRes = await runQuery(
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
    const { actor } = await requireActionFor(args.actorId, 'submit_expense', { perm: PERM.finance.expense.create });
    const submittedStage = expenseEntryStage(actor.role_name);

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

    const { expenseId, initialStatus } = await withTransaction(async (query) => {
      let expenseId: number;
      let previousStatus: string | null = null;
      if (draftContext) {
      expenseId = draftContext.expenseId;
      previousStatus = 'draft';
      await query(
        `UPDATE expenses
            SET vendor_name = $1, transaction_date = $2, subtotal = $3, vat_amount = $4, total_amount = $5,
                payment_method = $6, is_corrupted = $7, correction_notes = $8, ocr_raw_json = $9,
                document_url = $10, status = $11, created_to = $13, vendor_address = $14, created_to_address = $15,
                payee_type = $16,
                branch_id = COALESCE(branch_id, (SELECT id FROM finance.branches WHERE active ORDER BY id LIMIT 1)),
                department_id = COALESCE(department_id, (SELECT department_id FROM perm.user_departments WHERE user_id = submitter_id LIMIT 1)),
                updated_at = CURRENT_TIMESTAMP
          WHERE id = $12`,
        [
          vendor, txnDate, subtotal, vatAmount, totalAmount, paymentMethod,
          isCorrupted, correctionNotes, JSON.stringify(parsed),
          `/api/slips/file?key=${encodeURIComponent(slip.file_path)}`,
          submittedStage,
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
            SET vendor_name = $1, total_amount = $2, current_stage = $3,
                created_to = $5, vendor_address = $6, created_to_address = $7, currency = 'THB', updated_at = now()
          WHERE id = $4`,
        [vendor, totalAmount, submittedStage, draftContext.waybillId, createdTo || null, vendorAddress || null, createdToAddress || null],
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
                payee_type = $15,
                branch_id = COALESCE(branch_id, (SELECT id FROM finance.branches WHERE active ORDER BY id LIMIT 1)),
                department_id = COALESCE(department_id, (SELECT department_id FROM perm.user_departments WHERE user_id = submitter_id LIMIT 1)),
                updated_at = CURRENT_TIMESTAMP
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
           created_to, vendor_address, created_to_address, payee_type, branch_id, department_id
         )
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,
                 (SELECT id FROM finance.branches WHERE active ORDER BY id LIMIT 1),
                 (SELECT department_id FROM perm.user_departments WHERE user_id = $1 LIMIT 1))
         RETURNING id`,
        [
          args.actorId, vendor, txnDate, subtotal, vatAmount, totalAmount, paymentMethod, submittedStage,
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
        const match = await semanticCoaMatch(item.description, query, args.actorId);
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
        ? (isExecutiveRole(actor.role_name) && previousStatus === 'department_approval'
          ? submittedStage
          : previousStatus || submittedStage)
        : submittedStage;

      if (!preExistingExpenseId || initialStatus !== previousStatus) {
        await query(`UPDATE expenses SET status = $1 WHERE id = $2`, [initialStatus, expenseId]);
      }

      return { expenseId, initialStatus };
    });
    const waybillId = await appendWaybillEvent({
      origin: 'expense',
      originId: expenseId,
      kind: 'submitted',
      stageFrom: draftContext ? 'draft' : null,
      stageTo: initialStatus,
      actorId: args.actorId,
      actorRole: actor.role_name,
      payload: {
        vendor,
        totalAmount,
        vatAmount,
        ...(isExecutiveRole(actor.role_name) && initialStatus === submittedStage && submittedStage === 'accounting_review'
          ? {
              skippedStages: [skippedDepartmentStage('expense')],
              skipReason: 'executive_submitter',
            }
          : {}),
      },
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
    revalidatePath('/');
    return { success: true, expenseId, waybillId, status: initialStatus, policy: null, slipStatus: 'confirmed' };
  } catch (error: any) {
    console.error('submitExpenseFromSlip failed:', error);
    return { success: false, error: error.message };
  }
}

const AttachPaymentSlipForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  expenseId: z.coerce.number().int().positive(),
  paymentDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  amount: z.coerce.number().positive(),
});

export async function attachPaymentSlipAction(formData: FormData): Promise<{ ok: boolean; error?: string }> {
  const parsed = AttachPaymentSlipForm.safeParse({
    waybillId: String(formData.get('waybillId') ?? ''),
    expenseId: String(formData.get('expenseId') ?? '0'),
    paymentDate: String(formData.get('paymentDate') ?? ''),
    amount: String(formData.get('amount') ?? '0'),
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

  let storedKey: string | null = null;
  try {
    await withTransaction(async (q) => {
      const locked = await q<{ current_stage: string }>(
        `SELECT current_stage FROM waybills WHERE id = $1 FOR UPDATE`,
        [wb.id],
      );
      if (locked.rows[0]?.current_stage !== 'payment') {
        throw new Error('Expense stage changed; refresh and try again');
      }
      const apRes = await q<{ id: string; open_foreign: string; currency_code: string }>(
        `SELECT id::text, open_foreign::text, currency_code
           FROM finance.ap_documents
          WHERE source_type = 'expense' AND source_id = $1
            AND status IN ('open', 'partially_paid')
          ORDER BY id DESC LIMIT 1 FOR UPDATE`,
        [String(parsed.data.expenseId)],
      );
      const ap = apRes.rows[0];
      if (!ap) throw new Error('No open expense payable is available for payment');
      const open = Number(ap.open_foreign);
      if (parsed.data.amount > open + 0.005) {
        throw new Error(`Payment exceeds the remaining ${ap.currency_code} ${open.toFixed(2)}`);
      }
      const payment = await loadExpensePaymentPreview(wb.id, q as typeof query);
      if (!payment) throw new Error('Payment details are unavailable');
      if (!payment.ready) throw new Error(payment.blocker ?? 'Payment details are incomplete');
      const paymentNo = await q<{ n: number }>(
        `SELECT COUNT(*)::int + 1 AS n FROM expense_payments WHERE waybill_id = $1`,
        [wb.id],
      );
      const reference = `${wb.id}-PAY-${String(paymentNo.rows[0]?.n ?? 1).padStart(3, '0')}`;
      const file = await buildMockExpensePaymentSlip({
        payment,
        amount: parsed.data.amount,
        paymentDate: parsed.data.paymentDate,
        reference,
        recordedBy: actor.fullname,
      });
      storedKey = `payments/${wb.id}/${reference}.svg`;
      await put(storedKey, file, 'image/svg+xml');
      const generated = {
        source: 'folio_simulated_payment',
        simulated: true,
        amount: parsed.data.amount,
        currency: payment.currency,
        paymentDate: parsed.data.paymentDate,
        payee: payment.payee,
        bankName: payment.bankName,
        bankBranch: payment.bankBranch,
        accountNumber: payment.accountNumber,
        sourceAttachmentKey: payment.sourceAttachmentKey,
        reference,
      };
      const slipRes = await q<{ id: number }>(
        `INSERT INTO slips (
           file_path, mime_type, file_size, ocr_raw_json, ocr_confidence,
           uploaded_by, status, confirmed_at, kind, expense_id,
           bank_name, bank_branch, account_number, account_name
         ) VALUES ($1, 'image/svg+xml', $2, $3::jsonb, NULL, $4, 'confirmed', now(),
                   'payment_slip', $5, $6, $7, $8, $9)
         RETURNING id`,
        [
          storedKey,
          file.length,
          JSON.stringify(generated),
          actor.id,
          parsed.data.expenseId,
          payment.bankName,
          payment.bankBranch,
          payment.accountNumber,
          payment.payee,
        ],
      );
      const slipId = slipRes.rows[0].id;
      const postingActor = await loadPostingActor(q, actor.id);
      const allocation = await allocatePaymentInTransaction(q, {
        apDocumentId: Number(ap.id),
        allocationDate: parsed.data.paymentDate,
        foreignAmount: parsed.data.amount,
        bankAccountCode: payment.method === 'cash'
          ? '110100'
          : payment.method === 'credit_card'
            ? '110300'
            : '110200',
        sourceEventKey: `expense:${parsed.data.expenseId}:payment:${reference}`,
        actor: postingActor,
        waybillId: wb.id,
      });
      const remaining = Math.round((open - parsed.data.amount) * 100) / 100;
      const finalPayment = remaining <= 0.005;
      const next = finalPayment ? 'settlement' : 'payment';
      await q(
      `INSERT INTO expense_payments
         (waybill_id, expense_id, slip_id, amount, payment_date, bank_name,
          account_number, payee, reference, ocr_payload, ocr_confidence, confirmed_by,
          journal_id, allocation_id)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)`,
      [
        wb.id,
        parsed.data.expenseId,
        slipId,
        parsed.data.amount,
        parsed.data.paymentDate,
        payment.bankName,
        payment.accountNumber,
        payment.payee,
        reference,
        JSON.stringify(generated),
        null,
        actor.id,
        allocation.journalId,
        allocation.allocationId,
      ],
    );
      await q(
      `UPDATE expenses SET status = $1,
                          payment_method = $2,
                          disbursed_at = CASE WHEN $3 THEN now() ELSE disbursed_at END,
                          disbursed_by = CASE WHEN $3 THEN $4 ELSE disbursed_by END,
                          updated_at = now()
        WHERE id = $5`,
      [next, payment.method, finalPayment, actor.id, parsed.data.expenseId],
    );
      await q(
      `UPDATE waybills SET current_stage = $2,
                          status = 'open',
                          updated_at = now()
        WHERE id = $1`,
      [wb.id, next],
    );
      const attachment = await recordAttachment({
        waybillId: wb.id,
        stageKey: 'payment',
        kind: 'payment_slip',
        storageKey: storedKey,
        filename: `${reference}.svg`,
        contentType: 'image/svg+xml',
        byteSize: file.length,
        actorId: actor.id,
        actorRole: actor.role_name,
        caption: `Simulated paid slip · ${parsed.data.amount.toFixed(2)} ${payment.currency} · ${payment.payee}`,
        client: q as typeof query,
      });
      await recordEvent({
        waybillId: wb.id,
        kind: 'payment-confirmed',
        stageFrom: 'payment',
        stageTo: next,
        actorId: actor.id,
        actorRole: actor.role_name ?? 'finance',
        payload: {
          paymentMethod: payment.method,
          slipId,
          amount: parsed.data.amount,
          paymentDate: parsed.data.paymentDate,
          bankName: payment.bankName,
          payee: payment.payee,
          reference,
          remaining,
          currency: payment.currency,
          journalId: allocation.journalId,
          allocationId: allocation.allocationId,
          attachmentId: attachment.id,
          attachmentKey: storedKey,
          simulated: true,
        },
        client: q as never,
      });
      if (finalPayment) await q(
      `UPDATE waybill_stage_claims
          SET released_at = now(), released_by = $2, release_reason = 'stage completed'
        WHERE waybill_id = $1 AND stage = 'payment' AND released_at IS NULL`,
      [wb.id, actor.id],
    );
    });
  } catch (error) {
    if (storedKey) await remove(storedKey);
    if ((error as { code?: string }).code === '23505') {
      return { ok: false, error: 'This payment has already been recorded' };
    }
    return { ok: false, error: error instanceof Error ? error.message : 'Payment confirmation failed' };
  }

  revalidatePath(`/waybill/${wb.id}`);
  return { ok: true };
}
