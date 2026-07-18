import { revalidatePath } from 'next/cache';
import { z } from 'zod';
import { query } from '@/db';
import { recordEvent } from '@/waybill/events';
import { loadWaybill } from '@/waybill/queries';
import { upsertDraftJournal } from '@/finance/postExpenseToGL';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { hasPermission } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

const CoaApplyForm = z.object({
  itemId: z.coerce.number().int().positive(),
  code: z.string().min(1).max(20),
  normalSide: z.enum(['debit', 'credit']),
  expenseId: z.coerce.number().int().positive(),
});

export type ApplyCoaSuggestionArgs = z.infer<typeof CoaApplyForm>;

export interface ApplyCoaOk { ok: true; }
export interface ApplyCoaFail { ok: false; error: string; }
export type ApplyCoaResult = ApplyCoaOk | ApplyCoaFail;

function canActAtAccountingReview(
  actor: ActorWithScope,
  current_stage: string,
  origin: string,
): boolean {
  if (origin !== 'expense') return false;
  if (!['accounting_verification', 'accounting_review'].includes(current_stage)) return false;
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  if (hasPermission(actor, `stage:${current_stage}:act::allow`)) return true;
  if (hasPermission(actor, `stage:${current_stage}:act:all::allow`)) return true;
  if (actor.role_name === 'cfo' || actor.role_name === 'ceo' || actor.role_name === 'admin') {
    return true;
  }
  return false;
}

export async function applyCoaSuggestionCore(args: ApplyCoaSuggestionArgs): Promise<ApplyCoaResult> {
  const parsed = CoaApplyForm.safeParse(args);
  if (!parsed.success) {
    return { ok: false, error: parsed.error.issues[0]?.message ?? 'invalid input' };
  }
  const data = parsed.data;

  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthorized' };

  const wbRow = await query<{ id: string }>(
    `SELECT id FROM waybills WHERE origin = 'expense' AND origin_id = $1 LIMIT 1`,
    [data.expenseId],
  );
  const wb = wbRow.rows[0];
  if (!wb) return { ok: false, error: 'waybill not found for expense' };

  const wbFull = await loadWaybill(wb.id);
  if (!wbFull) return { ok: false, error: 'waybill not found' };
  if (!canActAtAccountingReview(actor, wbFull.current_stage, wbFull.origin)) {
    return { ok: false, error: 'cannot act at this stage' };
  }

  const acctRes = await query<{ code: string; normal_side: 'debit' | 'credit' | null }>(
    `SELECT code, normal_side FROM chart_of_accounts WHERE code = $1`,
    [data.code],
  );
  const acct = acctRes.rows[0];
  if (!acct) return { ok: false, error: 'invalid code or normal_side mismatch' };
  if (acct.normal_side !== data.normalSide) {
    return { ok: false, error: 'invalid code or normal_side mismatch' };
  }

  const upd = await query(
    `UPDATE folio.expense_items
        SET mapped_account_code = $1, updated_at = now()
      WHERE id = $2 AND expense_id = $3`,
    [data.code, data.itemId, data.expenseId],
  );
  if (upd.rowCount === 0) {
    return { ok: false, error: 'expense item not found' };
  }

  const expRes = await query<{ vendor_name: string | null }>(
    `SELECT vendor_name FROM expenses WHERE id = $1`,
    [data.expenseId],
  );
  const vendorName = expRes.rows[0]?.vendor_name ?? '';

  await upsertDraftJournal({
    expenseId: data.expenseId,
    vendorName,
  });

  await recordEvent({
    waybillId: wb.id,
    kind: 'coa-applied' as never,
    stageFrom: wbFull.current_stage,
    stageTo: wbFull.current_stage,
    actorId: actor.id,
    actorRole: actor.role_name ?? 'officer',
    payload: {
      itemId: data.itemId,
      code: data.code,
      normalSide: data.normalSide,
      appliedBy: actor.id,
    },
  });

  revalidatePath(`/waybill/${wb.id}`);
  return { ok: true };
}