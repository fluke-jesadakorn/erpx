'use server';

import { revalidatePath } from 'next/cache';
import { redirect } from 'next/navigation';
import { cookies } from 'next/headers';
import { z } from 'zod';
import { withTransaction, query as _query } from '@/db';
import { recordEvent } from '@/waybill/events';
import { recordAttachment, getAttachment } from '@/waybill/attachments';
import { loadWaybill, domainOf, loadApproversByStage } from '@/waybill/queries';
import { allowedKindsFor, type WaybillAttachmentKind } from '@/waybill/kinds';
import { addWatcher, removeWatcher } from '@/waybill/watchers';
import { reCallWaybillAction } from '@/waybill/recall';
import { resolveNextStage } from '@/perm/server';
import { matchPerm } from '@/perm';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { ensureGlForExpense, ensurePoForExpense as ensurePoForExpenseWithClient } from '@/waybill/ensureArtifacts';
import { ensurePoPdf } from '@/finance/poPdf';
import { upsertDraftJournal, finalizeDraftJournal, setExpenseJournalEntry } from '@/finance/postExpenseToGL';
import { upsertProcurementDraftAccrual } from '@/finance/postProcurementToGL';
import { pipsForDomain } from '@/waybill/derive';
import {
  actorForWaybill,
  canConfirmGl,
  canSaveProcurementAccrual,
  type WbForCheck,
} from './_helpers';

function canActOnWaybillStage(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  const stage = wb.current_stage;
  if (matchPerm(actor.permissions, `stage:${stage}:act::allow`)) return true;
  if (matchPerm(actor.permissions, `stage:${stage}:act:all::allow`)) return true;
  if (actor.role_name === 'cfo' || actor.role_name === 'ceo' || actor.role_name === 'admin') {
    return true;
  }
  if (wb.origin === 'expense' || wb.origin === 'so') return false;
  if (actor.id === wb.submitter_id && stage === 'submission' && matchPerm(actor.permissions, 'finance:expense:create::allow')) {
    return true;
  }
  return false;
}

function canRecall(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  return ['cfo', 'ceo', 'finance', 'admin'].includes(actor.role_name) && !['disbursed', 'gl_confirmed', 'rejected'].includes(wb.current_stage);
}

function canRejectWaybill(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (['disbursed', 'gl_confirmed', 'rejected'].includes(wb.current_stage)) return false;
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  return ['cfo', 'ceo', 'admin', 'finance', 'account_officer', 'account_supervisor', 'accounting_manager'].includes(actor.role_name);
}

function canFinalApproveExpense(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (!['accounting_authorization', 'final_authorization'].includes(wb.current_stage)) return false;
  if (matchPerm(actor.permissions, 'finance:expense:approve::allow')) return true;
  if (matchPerm(actor.permissions, 'finance:expense:settle::allow')) {
    return ['finance', 'account_officer', 'account_supervisor', 'accounting_manager'].includes(actor.role_name);
  }
  return false;
}

function canResubmit(actor: ActorWithScope, wb: WbForCheck): boolean {
  return actor.id === wb.submitter_id
    && wb.current_stage === 'rejected'
    && matchPerm(actor.permissions, 'finance:expense:create::allow');
}

function canAttachAtStage(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (['disbursed', 'gl_confirmed', 'rejected'].includes(wb.current_stage)) return false;
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  if (matchPerm(actor.permissions, 'finance:waybill:attach::allow')) return true;
  return actor.id === wb.submitter_id
    && wb.current_stage === 'submission'
    && matchPerm(actor.permissions, 'finance:expense:create::allow');
}

function canRemoveAttachment(actor: ActorWithScope): boolean {
  if (matchPerm(actor.permissions, 'admin:system:bypass::allow')) return true;
  return actor.role_name === 'cfo' || actor.role_name === 'ceo' || actor.role_name === 'admin';
}
const ApproveForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  stage: z.string().min(1).max(64).optional(),
});

export async function approveWaybillAction(formData: FormData): Promise<void> {
  const stageRaw = String(formData.get('stage') ?? '').trim();
  const parsed = ApproveForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    stage: stageRaw === '' ? undefined : stageRaw,
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.status !== 'open') throw new Error('Waybill is not open');

  const actor = await actorForWaybill();

  if (parsed.stage && parsed.stage !== wb.current_stage) {
    if (canRecall(actor, wb)) {
      const r = await reCallWaybillAction({
        waybillId: parsed.waybillId,
        targetStage: parsed.stage,
        actorId: actor.id,
        actorRole: actor.role_name,
        reason: 'cfo override',
      });
      if (!r.ok) throw new Error(r.error);
      revalidatePath(`/waybill/${parsed.waybillId}`);
      redirect(`/waybill/${parsed.waybillId}`);
    }
  }

  if (!canActOnWaybillStage(actor, wb)) {
    throw new Error('cannot act at this stage');
  }

  const currentStage = wb.current_stage as Parameters<typeof resolveNextStage>[0];
  const domain: 'expense' | 'procurement' | 'sales' =
    wb.origin === 'expense' ? 'expense'
      : wb.origin === 'so' ? 'sales'
        : 'procurement';
  const next = resolveNextStage(currentStage, actor.role_name, undefined, domain);
  if (!next) throw new Error(`No next stage from "${currentStage}"`);

  await withTransaction(async (q) => {
    if (wb.origin === 'expense') {
      await q(
        `UPDATE expenses SET status = $1, updated_at = now() WHERE id = $2`,
        [next.stage, wb.origin_id],
      );
    } else if (wb.origin === 'pr') {
      await q(
        `UPDATE purchase_requisitions SET status = $1, updated_at = now() WHERE id = $2`,
        [next.stage, wb.origin_id],
      );
    } else if (wb.origin === 'po') {
      await q(
        `UPDATE purchase_orders SET status = $1, updated_at = now() WHERE id = $2`,
        [next.stage, wb.origin_id],
      );
    } else if (wb.origin === 'so') {
      await q(
        `UPDATE sales_orders SET status = $1, updated_at = now() WHERE id = $2`,
        [next.stage, wb.origin_id],
      );
    }
    if (wb.origin === 'expense' && wb.current_stage === 'accounting_verification' && next.stage === 'accounting_authorization') {
      await ensurePoForExpenseWithClient(q, wb.origin_id);
      const expRow = await q<{ vendor_name: string | null }>(
        `SELECT vendor_name FROM expenses WHERE id = $1`,
        [wb.origin_id],
      );
      await upsertDraftJournal({
        expenseId: wb.origin_id,
        vendorName: expRow.rows[0]?.vendor_name ?? '',
      });
    }
    if (
      wb.origin === 'pr' &&
      wb.current_stage === 'accounting_verification' &&
      next.stage === 'accounting_authorization'
    ) {
      const prRow = await q<{ vendor_name: string | null }>(
        `SELECT vendor_name FROM purchase_requisitions WHERE id = $1`,
        [wb.origin_id],
      );
      await upsertProcurementDraftAccrual({
        origin: 'pr',
        originId: wb.origin_id,
        vendorName: prRow.rows[0]?.vendor_name ?? '',
      });
    }
    if (
      wb.origin === 'po' &&
      wb.current_stage === 'accounting_verification' &&
      next.stage === 'accounting_authorization'
    ) {
      const poRow = await q<{ vendor_name: string | null }>(
        `SELECT vendor_name FROM purchase_orders WHERE id = $1`,
        [wb.origin_id],
      );
      await upsertProcurementDraftAccrual({
        origin: 'po',
        originId: wb.origin_id,
        vendorName: poRow.rows[0]?.vendor_name ?? '',
      });
    }
    if (wb.origin === 'expense' && next.stage === 'final_authorization') {
      await ensureGlForExpense(q, wb.origin_id, actor.id);
    }
    await q(
      `UPDATE waybills SET current_stage = $1, current_owner_role = $2, updated_at = now()
        WHERE id = $3`,
      [next.stage, next.completed ? 'finance' : next.stage, wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'advanced',
      stageFrom: wb.current_stage,
      stageTo: next.stage,
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: { decision: 'approve' },
      client: q as never,
    });

    if (
      wb.current_stage === 'accounting_verification' &&
      next.stage === 'accounting_authorization' &&
      (wb.origin === 'expense' || wb.origin === 'po' || wb.origin === 'pr')
    ) {
      const actorName = String(actor.role_name ?? 'system');
      const { rows: actorRows } = await q<{ fullname: string }>(
        `SELECT fullname FROM users WHERE id = $1`,
        [actor.id],
      );
      const fullname = actorRows[0]?.fullname ?? actorName;
      await ensurePoPdf(wb.id, fullname);
    }
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const RejectForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  reason: z.string().min(5).max(2000),
});

export async function rejectWaybillAction(formData: FormData): Promise<void> {
  const parsed = RejectForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    reason: String(formData.get('reason') ?? '').trim(),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');

  const actor = await actorForWaybill();
  if (!canRejectWaybill(actor, wb)) {
    throw new Error('cannot reject at this stage');
  }

  await withTransaction(async (q) => {
    if (wb.origin === 'expense') {
      await q(
        `UPDATE expenses SET status = 'rejected',
                            rejection_reason = $2,
                            rejection_actor_id = $3,
                            rejected_at = now(),
                            updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, parsed.reason, actor.id],
      );
      if (wb.current_stage === 'accounting_authorization') {
        await q(
          `UPDATE purchase_orders
              SET status = 'rejected',
                  rejection_reason = $2,
                  rejection_actor_id = $3,
                  rejected_at = now(),
                  updated_at = now()
            WHERE id = (SELECT po_id FROM expenses WHERE id = $1)`,
          [wb.origin_id, parsed.reason, actor.id],
        );
      }
    } else if (wb.origin === 'pr') {
      await q(
        `UPDATE purchase_requisitions SET status = 'rejected',
                                        rejection_reason = $2,
                                        rejection_actor_id = $3,
                                        rejected_at = now(),
                                        updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, parsed.reason, actor.id],
      );
    } else if (wb.origin === 'po') {
      await q(
        `UPDATE purchase_orders SET status = 'rejected',
                                  rejection_reason = $2,
                                  rejection_actor_id = $3,
                                  rejected_at = now(),
                                  updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, parsed.reason, actor.id],
      );
    }
    await q(
      `UPDATE waybills SET current_stage = 'rejected',
                          status = 'rejected',
                          updated_at = now()
        WHERE id = $1`,
      [wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'rejected',
      stageFrom: wb.current_stage,
      stageTo: 'rejected',
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: { reason: parsed.reason },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const FinalApproveForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

export async function finalApproveWaybillAction(formData: FormData): Promise<void> {
  const parsed = FinalApproveForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'expense') {
    throw new Error(`final approve currently limited to expense origin (got ${wb.origin})`);
  }

  const actor = await actorForWaybill();
  if (!canFinalApproveExpense(actor, wb)) {
    throw new Error('cannot final approve at this stage');
  }

  const expRes = await _query<{ vendor_name: string; total_amount: string; vat_amount: string }>(
    `SELECT vendor_name, total_amount, vat_amount FROM expenses WHERE id = $1`,
    [wb.origin_id],
  );
  if (expRes.rows.length === 0) throw new Error('Expense not found');
  const exp = expRes.rows[0];

  await withTransaction(async (q) => {
    await q(
      `UPDATE expenses SET status = 'awaiting_disbursement', updated_at = now() WHERE id = $1`,
      [wb.origin_id],
    );
    await q(
      `UPDATE waybills SET current_stage = 'awaiting_disbursement',
                          current_owner_role = 'finance',
                          updated_at = now()
        WHERE id = $1`,
      [wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'advanced',
      stageFrom: 'final_authorization',
      stageTo: 'awaiting_disbursement',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'finance',
      payload: { decision: 'final-approve', gl_will_post: true },
      client: q as never,
    });
  });

  let journalId: number;
  const draft = await finalizeDraftJournal({ expenseId: wb.origin_id, actorId: actor.id });
  if (draft) {
    journalId = draft.journalId;
  } else {
    const upsert = await upsertDraftJournal({
      expenseId: wb.origin_id,
      vendorName: exp.vendor_name,
    });
    journalId = upsert.journalId;
    const fin = await finalizeDraftJournal({ expenseId: wb.origin_id, actorId: actor.id });
    if (!fin) throw new Error('failed to finalize draft journal');
    journalId = fin.journalId;
  }
  await withTransaction(async (q) => {
    await setExpenseJournalEntry(q, wb.origin_id, journalId, actor.id);
  });

  await recordEvent({
    waybillId: wb.id,
    kind: 'posted-to-gl',
    stageFrom: 'final_authorization',
    stageTo: 'awaiting_disbursement',
    actorId: actor.id,
    actorRole: actor.role_name ?? 'finance',
    payload: { journalId, expenseId: wb.origin_id },
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const FinalRejectForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  reason: z.string().min(5).max(2000),
});

export async function finalRejectWaybillAction(formData: FormData): Promise<void> {
  const parsed = FinalRejectForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    reason: String(formData.get('reason') ?? '').trim(),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.status !== 'open') throw new Error(`Waybill status is '${wb.status}', not open`);
  if (wb.current_stage !== 'final_authorization') {
    throw new Error(`final reject only at final_authorization (current: ${wb.current_stage})`);
  }

  const actor = await actorForWaybill();
  if (!canRejectWaybill(actor, wb)) {
    throw new Error('cannot reject at this stage');
  }

  await withTransaction(async (q) => {
    if (wb.origin === 'expense') {
      await q(
        `UPDATE expenses SET status = 'rejected',
                            rejection_reason = $2,
                            rejection_actor_id = $3,
                            rejected_at = now(),
                            updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, parsed.reason, actor.id],
      );
    } else if (wb.origin === 'pr') {
      await q(
        `UPDATE purchase_requisitions SET status = 'rejected',
                                        rejection_reason = $2,
                                        rejection_actor_id = $3,
                                        rejected_at = now(),
                                        updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, parsed.reason, actor.id],
      );
    } else if (wb.origin === 'po') {
      await q(
        `UPDATE purchase_orders SET status = 'rejected',
                                  rejection_reason = $2,
                                  rejection_actor_id = $3,
                                  rejected_at = now(),
                                  updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, parsed.reason, actor.id],
      );
    }
    await q(
      `UPDATE waybills SET current_stage = 'rejected',
                          status = 'rejected',
                          updated_at = now()
        WHERE id = $1`,
      [wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'rejected',
      stageFrom: 'final_authorization',
      stageTo: 'rejected',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'finance',
      payload: { decision: 'final-reject', reason: parsed.reason, gl_posted: false },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const ResubmitForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

export async function resubmitWaybillAction(formData: FormData): Promise<void> {
  const parsed = ResubmitForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb || wb.status !== 'rejected') throw new Error('Not in rejected state');

  const actor = await actorForWaybill();
  if (!canResubmit(actor, wb)) {
    throw new Error('cannot resubmit at this stage');
  }

  await withTransaction(async (q) => {
    if (wb.origin === 'expense') {
      await q(
        `UPDATE expenses SET status = 'submission',
                            rejection_reason = NULL,
                            rejection_actor_id = NULL,
                            rejected_at = NULL,
                            updated_at = now()
          WHERE id = $1`,
        [wb.origin_id],
      );
    } else if (wb.origin === 'pr') {
      await q(
        `UPDATE purchase_requisitions SET status = 'submission',
                                        rejection_reason = NULL,
                                        rejection_actor_id = NULL,
                                        rejected_at = NULL,
                                        updated_at = now()
          WHERE id = $1`,
        [wb.origin_id],
      );
    }
    await q(
      `UPDATE waybills SET current_stage = 'submission',
                          status = 'open',
                          current_owner_role = 'supervisor',
                          updated_at = now()
        WHERE id = $1`,
      [wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'resubmitted',
      stageFrom: 'rejected',
      stageTo: 'submission',
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: { origin: wb.origin, origin_id: wb.origin_id },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const ConfirmGlForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  expenseId: z.coerce.number().int().positive(),
});

export async function confirmGlRecordedAction(formData: FormData): Promise<void> {
  const parsed = ConfirmGlForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    expenseId: String(formData.get('expenseId') ?? '0'),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'expense') {
    throw new Error(`confirm-gl only for expense origin (got ${wb.origin})`);
  }
  if (wb.current_stage !== 'disbursed') {
    throw new Error(
      `GL can only be confirmed after disbursement (current: ${wb.current_stage})`,
    );
  }

  const actor = await actorForWaybill();
  if (!canConfirmGl(actor, wb)) {
    throw new Error('cannot confirm GL at this stage');
  }

  const postedRes = await _query<{ exists: boolean }>(
    `SELECT EXISTS (
       SELECT 1 FROM waybill_events
        WHERE waybill_id = $1 AND kind = 'posted-to-gl'
     ) AS exists`,
    [wb.id],
  );
  if (!postedRes.rows[0]?.exists) {
    throw new Error('No posted-to-gl event on this waybill yet');
  }

  const expRes = await _query<{ gl_confirmed_at: string | null }>(
    `SELECT gl_confirmed_at FROM expenses WHERE id = $1`,
    [parsed.expenseId],
  );
  if (expRes.rows.length === 0) throw new Error('Expense not found');
  if (expRes.rows[0].gl_confirmed_at != null) {
    throw new Error('GL post already confirmed');
  }

  await withTransaction(async (q) => {
    await q(
      `UPDATE expenses SET gl_confirmed_at = now(),
                           gl_confirmed_by = $1,
                           updated_at = now()
        WHERE id = $2`,
      [actor.id, parsed.expenseId],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'gl-confirmed',
      stageFrom: 'disbursed',
      stageTo: 'disbursed',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'finance',
      payload: { expenseId: parsed.expenseId },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const AttachForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  storageKey: z.string().min(1).max(500),
  filename: z.string().min(1).max(255),
  contentType: z.string().min(1).max(120),
  byteSize: z.coerce.number().int().min(0).max(50 * 1024 * 1024),
  kind: z.enum([
    'slip','pr_doc','po_doc','payment_receipt','signoff_memo',
    'invoice','wht_cert','photo','memo','other',
  ]),
  caption: z.string().max(2000).optional(),
});

export async function attachWaybillDocumentAction(formData: FormData): Promise<{ ok: boolean; error?: string }> {
  const parsed = AttachForm.safeParse({
    waybillId: String(formData.get('waybillId') ?? ''),
    storageKey: String(formData.get('storageKey') ?? ''),
    filename: String(formData.get('filename') ?? ''),
    contentType: String(formData.get('contentType') ?? 'application/octet-stream'),
    byteSize: String(formData.get('byteSize') ?? '0'),
    kind: String(formData.get('kind') ?? 'other'),
    caption: String(formData.get('caption') ?? '').trim() || undefined,
  });
  if (!parsed.success) {
    return { ok: false, error: parsed.error.issues[0]?.message ?? 'invalid input' };
  }
  const data = parsed.data;

  const wb = await loadWaybill(data.waybillId);
  if (!wb) return { ok: false, error: 'waybill not found' };

  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthorized' };
  if (!canAttachAtStage(actor, wb)) {
    return { ok: false, error: 'cannot attach at this stage' };
  }

  if (!allowedKindsFor(wb.current_stage).includes(data.kind as WaybillAttachmentKind)) {
    return { ok: false, error: `kind '${data.kind}' not allowed at stage '${wb.current_stage}'` };
  }

  await recordAttachment({
    waybillId: wb.id,
    stageKey: wb.current_stage,
    kind: data.kind as WaybillAttachmentKind,
    storageKey: data.storageKey,
    filename: data.filename,
    contentType: data.contentType,
    byteSize: data.byteSize,
    actorId: actor.id,
    actorRole: actor.role_name ?? 'officer',
    caption: data.caption ?? null,
  });

  revalidatePath(`/waybill/${wb.id}`);
  return { ok: true };
}

const RemoveAttachForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  attachmentId: z.coerce.number().int().positive(),
});

export async function removeWaybillAttachmentAction(formData: FormData): Promise<void> {
  const parsed = RemoveAttachForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    attachmentId: String(formData.get('attachmentId') ?? '0'),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');

  const actor = await actorForWaybill();
  if (!canRemoveAttachment(actor)) {
    throw new Error('cannot remove attachment');
  }

  const att = await getAttachment(parsed.attachmentId);
  if (!att || att.waybill_id !== parsed.waybillId) {
    throw new Error('Attachment not found on this waybill');
  }

  await withTransaction(async (q) => {
    await q(
      `DELETE FROM waybill_attachments WHERE id = $1`,
      [parsed.attachmentId],
    );
    await recordEvent({
      waybillId: parsed.waybillId,
      kind: 'advanced',
      stageFrom: null,
      stageTo: null,
      actorId: actor.id,
      actorRole: actor.role_name ?? 'officer',
      payload: {
        decision: 'attachment_removed',
        attachment_id: parsed.attachmentId,
        filename: att.filename,
      },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${parsed.waybillId}`);
  redirect(`/waybill/${parsed.waybillId}`);
}

const SubscribeForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  stageKey: z.string().min(1).max(64),
});

export async function subscribeWaybillAction(formData: FormData): Promise<{ ok: boolean; error?: string }> {
  const parsed = SubscribeForm.safeParse({
    waybillId: String(formData.get('waybillId') ?? ''),
    stageKey: String(formData.get('stageKey') ?? ''),
  });
  if (!parsed.success) {
    return { ok: false, error: parsed.error.issues[0]?.message ?? 'invalid input' };
  }

  const cookieValue = (await cookies()).get('folio_session')?.value ?? null;
  void cookieValue;
  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthenticated' };

  const wb = await loadWaybill(parsed.data.waybillId);
  if (!wb) return { ok: false, error: 'not found' };

  const domain = domainOf(wb);
  const pip = pipsForDomain(domain).find((p) => p.key === parsed.data.stageKey);
  if (!pip) return { ok: false, error: 'invalid stage' };

  const approvers = await loadApproversByStage(parsed.data.waybillId);
  const list = approvers[parsed.data.stageKey] ?? [];
  if (!list.some((a) => a.user_id === actor.id)) {
    return { ok: false, error: 'only listed approvers can subscribe' };
  }

  await addWatcher({
    waybillId: parsed.data.waybillId,
    stageKey: parsed.data.stageKey,
    userId: actor.id,
  });
  revalidatePath(`/waybill/${parsed.data.waybillId}`);
  return { ok: true };
}

export async function unsubscribeWaybillAction(formData: FormData): Promise<{ ok: boolean; error?: string }> {
  const parsed = SubscribeForm.safeParse({
    waybillId: String(formData.get('waybillId') ?? ''),
    stageKey: String(formData.get('stageKey') ?? ''),
  });
  if (!parsed.success) {
    return { ok: false, error: parsed.error.issues[0]?.message ?? 'invalid input' };
  }

  const actor = await loadActor();
  if (!actor) return { ok: false, error: 'unauthenticated' };

  await removeWatcher({
    waybillId: parsed.data.waybillId,
    stageKey: parsed.data.stageKey,
    userId: actor.id,
  });
  revalidatePath(`/waybill/${parsed.data.waybillId}`);
  return { ok: true };
}

const RecomputeDraftGlForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

export async function recomputeExpenseDraftGlAction(formData: FormData): Promise<void> {
  const parsed = RecomputeDraftGlForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'expense') {
    throw new Error(`recompute-draft-gl only for expense origin (got ${wb.origin})`);
  }

  const actor = await actorForWaybill();
  if (!canSaveProcurementAccrual(actor)) {
    throw new Error('cannot recompute expense draft GL');
  }

  const expRes = await _query<{ vendor_name: string | null }>(
    `SELECT vendor_name FROM expenses WHERE id = $1`,
    [wb.origin_id],
  );
  if (expRes.rows.length === 0) throw new Error('Expense not found');

  await upsertDraftJournal({
    expenseId: wb.origin_id,
    vendorName: expRes.rows[0].vendor_name ?? '',
  });
  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}
