'use server';

import { query, withTransaction } from '@/db';
import { remove as removeFromStorage } from '@/slips/storage';
import { revalidatePath } from 'next/cache';

export async function discardSlip(args: { slipId: number; actorId: number }) {
  try {
    const slipRes = await query(
      `SELECT id, uploaded_by, status, expense_id, pr_id, po_id, file_path
         FROM slips WHERE id = $1`,
      [args.slipId],
    );
    if (slipRes.rows.length === 0) {
      return { success: false, error: 'Slip not found' };
    }
    const slip = slipRes.rows[0];

    if (slip.uploaded_by !== args.actorId) {
      return { success: false, error: 'Only the uploader can remove this slip' };
    }

    let lockedExpenseId: number | null = null;
    if (slip.expense_id) {
      const lockedRes = await query(
        `SELECT 1
           FROM approval_transitions
          WHERE target_type = 'expense'
            AND target_id   = $1
            AND new_status IN ('approved', 'rejected')
          LIMIT 1`,
        [slip.expense_id],
      );
      if (lockedRes.rows.length > 0) {
        return {
          success: false,
          error: 'Slip is locked — the linked expense has already been approved or rejected.',
        };
      }
      lockedExpenseId = slip.expense_id;
    }

    await withTransaction(async (q) => {
      await q(`DELETE FROM slips WHERE id = $1`, [args.slipId]);
      if (lockedExpenseId) {
        await q(`DELETE FROM expense_items WHERE expense_id = $1`, [lockedExpenseId]);
        await q(`DELETE FROM expenses WHERE id = $1`, [lockedExpenseId]);
      }
    });

    try {
      await removeFromStorage(slip.file_path);
    } catch (e) {
      console.error('discardSlip: storage cleanup failed (file may already be missing):', e);
    }

    revalidatePath('/');
    revalidatePath('/');
    return { success: true, removedExpenseId: lockedExpenseId };
  } catch (error: any) {
    console.error('discardSlip failed:', error);
    return { success: false, error: error.message };
  }
}

export async function getSlipLockState(args: { slipId: number; actorId: number }): Promise<{
  exists: boolean;
  status: string | null;
  isUploader: boolean;
  approvedOrRejected: boolean;
  locked: boolean;
  reason: string | null;
}> {
  const empty = {
    exists: false,
    status: null,
    isUploader: false,
    approvedOrRejected: false,
    locked: false,
    reason: null,
  };
  const slipRes = await query(
    `SELECT id, uploaded_by, status, expense_id
       FROM slips WHERE id = $1`,
    [args.slipId],
  );
  if (slipRes.rows.length === 0) return empty;
  const slip = slipRes.rows[0];
  const isUploader = slip.uploaded_by === args.actorId;

  let approvedOrRejected = false;
  if (slip.expense_id) {
    const r = await query(
      `SELECT 1 FROM approval_transitions
        WHERE target_type = 'expense' AND target_id = $1
          AND new_status IN ('approved', 'rejected') LIMIT 1`,
      [slip.expense_id],
    );
    approvedOrRejected = r.rows.length > 0;
  }

  const locked = !isUploader || approvedOrRejected;
  return {
    exists: true,
    status: slip.status,
    isUploader,
    approvedOrRejected,
    locked,
    reason: !isUploader
      ? 'Only the uploader can remove this slip'
      : approvedOrRejected
        ? 'Linked expense has already been approved or rejected'
        : null,
  };
}