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
import { reCallWaybillAction } from '@/waybill/recall-action';
import { hasPermission, resolveNextStage } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';
import { loadActor, type ActorWithScope } from '@/server/guard';
import { ensureGlForExpense, ensurePoForExpense as ensurePoForExpenseWithClient, ensurePrForExpense } from '@/waybill/ensureArtifacts';
import { ensurePoPdf } from '@/finance/poPdf';
import { upsertDraftJournal, finalizeDraftJournal, saveDraftJournalLines, setExpenseJournalEntry, validateJournalLines, type DraftGlLine } from '@/finance/postExpenseToGL';
import { upsertProcurementDraftAccrual } from '@/finance/postProcurementToGL';
import { pipsForDomain } from '@/waybill/derive';
import {
  expenseEntryStage,
  isExecutiveRole,
  nextProcurementStage,
  procurementResubmitStage,
  skippedDepartmentStage,
} from '@/waybill/routing';
import {
  assertExpenseClaim,
  authorizeExpenseStage,
  claimExpenseStage,
  loadExpenseFlowContext,
  nextExpenseStage,
  reassignExpenseClaim,
  releaseExpenseClaim,
  type ExpenseActor,
} from '@/waybill/expenseFlow';
import { ensureExpensePaymentDocument } from '@/finance/expenseDocument';
import { aiInvoke } from '@/ai/router';
import {
  actorForWaybill,
  type WbForCheck,
} from './_helpers';

const DEPT_SCOPED_STAGES = new Set<string>([
  'submission',
  'dept_verification',
  'dept_authorization',
  'final_authorization',
]);

const CROSS_DEPT_STAGES = new Set<string>([
  'accounting_verification',
  'accounting_supervision',
  'accounting_authorization',
  'disbursement_authorization',
  'cfo_authorization',
  'ceo_authorization',
  'gl_confirmed',
]);

async function submitterDeptId(wb: WbForCheck): Promise<string | null> {
  if (!wb.submitter_id) return null;
  const r = await _query<{ department_id: string | null }>(
    `SELECT department_id FROM perm.user_departments WHERE user_id = $1`,
    [wb.submitter_id],
  );
  return r.rows[0]?.department_id ?? null;
}

async function canActOnWaybillStage(actor: ActorWithScope, wb: WbForCheck): Promise<boolean> {
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  const stage = wb.current_stage;
  const hasStageAll = actor.permissions.includes(`stage:${stage}:act:all::allow`);
  if (hasStageAll) return true;
  const hasStageScoped = actor.permissions.includes(`stage:${stage}:act::allow`)
    || (!!actor.dept_id && actor.permissions.includes(`stage:${stage}:act:${actor.dept_id}::allow`));
  if (hasStageScoped) {
    if (CROSS_DEPT_STAGES.has(stage)) return true;
    if (DEPT_SCOPED_STAGES.has(stage)) {
      const submitterDept = await submitterDeptId(wb);
      if (!actor.dept_id || actor.dept_id !== submitterDept) return false;
    }
    return true;
  }
  if (actor.id === wb.submitter_id && stage === 'submission' && hasPermission(actor, PERM.finance.expense.create)) {
    return true;
  }
  if (wb.origin === 'expense' || wb.origin === 'so') return false;
  return false;
}

function canRecall(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  if (['disbursed', 'gl_confirmed', 'rejected'].includes(wb.current_stage)) return false;
  return wb.origin === 'expense'
    ? hasPermission(actor, PERM.finance.expense.override)
    : hasPermission(actor, PERM.finance.pr.override_approve);
}

function canRejectWaybill(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (['disbursed', 'gl_confirmed', 'rejected'].includes(wb.current_stage)) return false;
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  if (wb.origin === 'po') return hasPermission(actor, PERM.finance.po.reject);
  if (wb.origin === 'pr') return hasPermission(actor, PERM.finance.pr.approve);
  return hasPermission(actor, PERM.finance.expense.approve);
}

function canFinalApproveExpense(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (!['accounting_authorization', 'final_authorization'].includes(wb.current_stage)) return false;
  if (hasPermission(actor, PERM.finance.expense.approve)) return true;
  if (hasPermission(actor, PERM.finance.expense.settle)) {
    return actor.dept_id === 'finance' || actor.dept_id === 'accounting';
  }
  return false;
}

function canResubmit(actor: ActorWithScope, wb: WbForCheck): boolean {
  return actor.id === wb.submitter_id
    && wb.current_stage === 'rejected'
    && hasPermission(actor, PERM.finance.expense.create);
}

function canAttachAtStage(actor: ActorWithScope, wb: WbForCheck): boolean {
  if (['disbursed', 'gl_confirmed', 'rejected'].includes(wb.current_stage)) return false;
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  if (hasPermission(actor, 'finance:waybill:attach::allow')) return true;
  return actor.id === wb.submitter_id
    && wb.current_stage === 'submission'
    && hasPermission(actor, PERM.finance.expense.create);
}

function canRemoveAttachment(actor: ActorWithScope): boolean {
  if (hasPermission(actor, PERM.admin.system.bypass)) return true;
  return hasPermission(actor, PERM.finance.expense.override)
    || hasPermission(actor, PERM.finance.pr.override_approve);
}
const ApproveForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
  stage: z.string().min(1).max(64).optional(),
});

function expenseActor(actor: ActorWithScope): ExpenseActor {
  return {
    id: actor.id,
    permissions: actor.permissions,
    deptId: actor.dept_id,
    departmentId: actor.dept_id,
    level: actor.level,
    rank: actor.level,
    roleName: actor.role_name,
  };
}

async function approveExpenseWaybill(actor: ActorWithScope, wb: WbForCheck): Promise<void> {
  const ctx = await loadExpenseFlowContext(wb.id);
  const skipDepartment = actor.id === ctx.submitterId
    && isExecutiveRole(actor.role_name)
    && hasPermission(actor, PERM.finance.expense.create)
    && (ctx.stage === 'submission' || ctx.stage === 'department_approval');
  const decision = skipDepartment
    ? { allow: true as const, reason: 'Executive submitter skips department approval' }
    : await authorizeExpenseStage(expenseActor(actor), ctx);
  if (!decision.allow) throw new Error(decision.reason);
  if (ctx.stage === 'payment' || ctx.stage === 'settlement') {
    throw new Error(`${ctx.stage} requires its dedicated human confirmation action`);
  }
  if (ctx.stage === 'accounting_review') {
    await assertExpenseClaim(actor.id, wb.id, 'accounting_review');
    const draft = await _query<{ id: number }>(
      `SELECT id FROM journal_entries
        WHERE expense_id = $1 AND step = 'accrual' AND is_draft IS TRUE
        ORDER BY id DESC LIMIT 1`,
      [ctx.expenseId],
    );
    if (!draft.rows[0]) {
      redirect(`/waybill/${wb.id}/gl?notice=missing-accrual-draft`);
    }
    const lines = await _query<DraftGlLine>(
      `SELECT account_code, debit::float8 AS debit, credit::float8 AS credit,
              COALESCE(description, '') AS description
         FROM ledger_lines WHERE journal_entry_id = $1 ORDER BY id`,
      [draft.rows[0].id],
    );
    await validateJournalLines(lines.rows);
    await _query(`UPDATE journal_entries SET prepared_by = $2 WHERE id = $1`, [draft.rows[0].id, actor.id]);
  }
  let journalId: number | null = null;
  const next = skipDepartment
    ? expenseEntryStage(actor.role_name)
    : nextExpenseStage(ctx.stage, ctx.amount, actor.id === ctx.submitterId ? actor.role_name : null);
  if (!next || next === 'completed') throw new Error(`No approval transition from ${ctx.stage}`);
  await withTransaction(async (q) => {
    const locked = await q<{ current_stage: string }>(
      `SELECT current_stage FROM waybills WHERE id = $1 FOR UPDATE`,
      [wb.id],
    );
    if (locked.rows[0]?.current_stage !== ctx.stage) {
      throw new Error('Expense stage changed; refresh and try again');
    }
    if (ctx.stage === 'accounting_approval') {
      const finalized = await finalizeDraftJournal({
        expenseId: ctx.expenseId,
        actorId: actor.id,
        step: 'accrual',
        client: q as typeof _query,
      });
      if (!finalized) throw new Error('A reviewed and balanced accrual draft is required');
      journalId = finalized.journalId;
      await ensurePrForExpense(q, ctx.expenseId);
      await ensurePoForExpenseWithClient(q, ctx.expenseId);
    }
    await q(
      `UPDATE expenses SET status = $1, journal_entry_id = COALESCE($2, journal_entry_id), updated_at = now()
        WHERE id = $3`,
      [next, journalId, ctx.expenseId],
    );
    await q(
      `UPDATE waybills
          SET current_stage = $1, current_owner_role = $1,
              current_owner_user_id = NULL, updated_at = now()
        WHERE id = $2 AND current_stage = $3`,
      [next, wb.id, ctx.stage],
    );
    await q(
      `UPDATE waybill_stage_claims
          SET released_at = now(), released_by = $2, release_reason = 'stage completed'
        WHERE waybill_id = $1 AND stage = $3 AND released_at IS NULL`,
      [wb.id, actor.id, ctx.stage],
    );
    if (ctx.stage === 'accounting_approval' && next === 'payment') {
      await recordEvent({
        waybillId: wb.id,
        kind: 'executive-skipped',
        stageFrom: 'accounting_approval',
        stageTo: 'payment',
        actorId: actor.id,
        actorRole: actor.role_name,
        payload: { amount: ctx.amount, thresholdTHB: 200000 },
        client: q as never,
      });
    }
    await recordEvent({
      waybillId: wb.id,
      kind: ctx.stage === 'accounting_approval' ? 'posted-to-gl-accrual' : 'advanced',
      stageFrom: ctx.stage,
      stageTo: next,
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: {
        decision: 'approve',
        journalId,
        ...(skipDepartment
          ? {
              skippedStages: [skippedDepartmentStage('expense')],
              skipReason: 'executive_submitter',
            }
          : {}),
      },
      client: q as never,
    });
  });
  if (next === 'payment') {
    const name = await _query<{ fullname: string }>(`SELECT fullname FROM users WHERE id = $1`, [actor.id]);
    await ensureExpensePaymentDocument({
      waybillId: wb.id,
      actorId: actor.id,
      actorRole: actor.role_name,
      actorName: name.rows[0]?.fullname ?? actor.fullname,
    });
  }
}

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

  const isExpense = wb.origin === 'expense';
  if (isExpense) {
    await approveExpenseWaybill(actor, wb);
    revalidatePath(`/waybill/${wb.id}`);
    redirect(`/waybill/${wb.id}`);
  }

  const executiveProcurementSubmitter = (wb.origin === 'pr' || wb.origin === 'po')
    && wb.current_stage === 'submission'
    && wb.submitter_id === actor.id
    && isExecutiveRole(actor.role_name);
  if (!(await canActOnWaybillStage(actor, wb))) {
    throw new Error('cannot act at this stage');
  }

  const currentStage = wb.current_stage as Parameters<typeof resolveNextStage>[0];
  const domain: 'expense' | 'procurement' | 'sales' =
    wb.origin === 'expense' ? 'expense'
      : wb.origin === 'so' ? 'sales'
        : 'procurement';
  const nextStage = domain === 'procurement'
    ? nextProcurementStage(currentStage, actor.role_name, wb.submitter_id === actor.id)
    : resolveNextStage(currentStage, actor.role_name, undefined, domain)?.stage ?? null;
  const next = nextStage ? { stage: nextStage, completed: false } : null;
  if (!next) throw new Error(`No next stage from "${currentStage}"`);

  let shouldGeneratePo = false;
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
      payload: {
        decision: 'approve',
        ...(executiveProcurementSubmitter
          ? {
              skippedStages: [skippedDepartmentStage(wb.origin)],
              skipReason: 'executive_submitter',
            }
          : {}),
      },
      client: q as never,
    });

    if (
      wb.current_stage === 'accounting_verification' &&
      next.stage === 'accounting_authorization' &&
      (wb.origin === 'expense' || wb.origin === 'po' || wb.origin === 'pr')
    ) {
      shouldGeneratePo = true;
    }
  });

  if (shouldGeneratePo) {
    const actorName = String(actor.role_name ?? 'system');
    const { rows: actorRows } = await _query<{ fullname: string }>(
      `SELECT fullname FROM users WHERE id = $1`,
      [actor.id],
    );
    await ensurePoPdf(wb.id, actorRows[0]?.fullname ?? actorName);
  }

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
  if (wb.origin === 'expense') {
    const flow = await loadExpenseFlowContext(wb.id);
    const decision = await authorizeExpenseStage(expenseActor(actor), flow);
    if (!decision.allow) throw new Error(decision.reason);
  } else if (!canRejectWaybill(actor, wb)) {
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
    await q(
      `UPDATE waybill_stage_claims
          SET released_at = now(), released_by = $2, release_reason = $3
        WHERE waybill_id = $1 AND released_at IS NULL`,
      [wb.id, actor.id, `rejected: ${parsed.reason}`],
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

  const totalAmount = Number(wb.total_amount ?? exp.total_amount ?? 0);
  const needsCeoStages = totalAmount > 200_000;
  const nextStage = needsCeoStages ? 'disbursement_authorization' : 'awaiting_disbursement';

  await withTransaction(async (q) => {
    await q(
      `UPDATE expenses SET status = $1, updated_at = now() WHERE id = $2`,
      [nextStage, wb.origin_id],
    );
    await q(
      `UPDATE waybills SET current_stage = $1,
                          current_owner_role = 'finance',
                          updated_at = now()
        WHERE id = $2`,
      [nextStage, wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'advanced',
      stageFrom: 'accounting_authorization',
      stageTo: nextStage,
      actorId: actor.id,
      actorRole: actor.role_name ?? 'finance',
      payload: { decision: 'final-approve', gl_will_post: !needsCeoStages },
      client: q as never,
    });
  });

  if (needsCeoStages) {
    revalidatePath(`/waybill/${wb.id}`);
    redirect(`/waybill/${wb.id}`);
    return;
  }

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
    stageFrom: 'accounting_authorization',
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
  if (wb.current_stage !== 'final_authorization' && wb.current_stage !== 'accounting_authorization') {
    throw new Error(`final reject only at final_authorization/accounting_authorization (current: ${wb.current_stage})`);
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
      stageFrom: wb.current_stage,
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
    const isProcurement = wb.origin === 'pr' || wb.origin === 'po';
    const resubmitStage = wb.origin === 'expense'
      ? expenseEntryStage(actor.role_name)
      : isProcurement
        ? procurementResubmitStage(actor.role_name)
        : 'submission';
    if (wb.origin === 'expense') {
      await q(
        `UPDATE expenses SET status = $2,
                            rejection_reason = NULL,
                            rejection_actor_id = NULL,
                            rejected_at = NULL,
                            updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, resubmitStage],
      );
    } else if (wb.origin === 'pr') {
      await q(
        `UPDATE purchase_requisitions SET status = $2,
                                        rejection_reason = NULL,
                                        rejection_actor_id = NULL,
                                        rejected_at = NULL,
                                        updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, resubmitStage],
      );
    } else if (wb.origin === 'po') {
      await q(
        `UPDATE purchase_orders SET status = $2,
                                   rejection_reason = NULL,
                                   rejection_actor_id = NULL,
                                   rejected_at = NULL,
                                   updated_at = now()
          WHERE id = $1`,
        [wb.origin_id, resubmitStage],
      );
    }
    await q(
      `UPDATE waybills SET current_stage = $2,
                          status = 'open',
                          current_owner_role = $2,
                          current_owner_user_id = NULL,
                          updated_at = now()
        WHERE id = $1`,
      [wb.id, resubmitStage],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'resubmitted',
      stageFrom: 'rejected',
      stageTo: resubmitStage,
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: {
        origin: wb.origin,
        origin_id: wb.origin_id,
        ...(isExecutiveRole(actor.role_name) && (wb.origin === 'expense' || isProcurement)
          ? {
              skippedStages: [skippedDepartmentStage(wb.origin)],
              skipReason: 'executive_submitter',
            }
          : {}),
      },
      client: q as never,
    });
  });

  revalidatePath(`/waybill/${wb.id}`);
  redirect(`/waybill/${wb.id}`);
}

const ConfirmGlForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

export async function confirmGlRecordedAction(formData: FormData): Promise<void> {
  const parsed = ConfirmGlForm.parse({
    waybillId: String(formData.get('waybillId') ?? ''),
  });

  const wb = await loadWaybill(parsed.waybillId);
  if (!wb) throw new Error('Waybill not found');
  if (wb.origin !== 'expense') {
    throw new Error(`confirm-gl only for expense origin (got ${wb.origin})`);
  }
  if (wb.current_stage !== 'settlement') {
    throw new Error(`Settlement GL can only be posted at settlement (current: ${wb.current_stage})`);
  }

  const actor = await actorForWaybill();
  const flow = await loadExpenseFlowContext(wb.id);
  const decision = await authorizeExpenseStage(expenseActor(actor), flow, 'settlement');
  if (!decision.allow) throw new Error(decision.reason);
  await assertExpenseClaim(actor.id, wb.id, 'settlement');

  const expRes = await _query<{ gl_confirmed_at: string | null }>(
    `SELECT gl_confirmed_at FROM expenses WHERE id = $1`,
    [wb.origin_id],
  );
  if (expRes.rows.length === 0) throw new Error('Expense not found');
  if (expRes.rows[0].gl_confirmed_at != null) {
    throw new Error('Settlement GL already posted');
  }

  await withTransaction(async (q) => {
    const locked = await q<{ current_stage: string }>(
      `SELECT current_stage FROM waybills WHERE id = $1 FOR UPDATE`,
      [wb.id],
    );
    if (locked.rows[0]?.current_stage !== 'settlement') {
      throw new Error('Expense stage changed; refresh and try again');
    }
    const payable = await q<{ status: string; open_foreign: string }>(
      `SELECT status, open_foreign::text
         FROM finance.ap_documents
        WHERE source_type = 'expense' AND source_id = $1
        ORDER BY id DESC LIMIT 1 FOR UPDATE`,
      [String(wb.origin_id)],
    );
    if (!payable.rows[0] || payable.rows[0].status !== 'paid' || Math.abs(Number(payable.rows[0].open_foreign)) > 0.005) {
      throw new Error('The expense payable must be fully allocated before settlement confirmation');
    }
    const payments = await q<{ journal_id: string }>(
      `SELECT ep.journal_id::text
         FROM expense_payments ep
         JOIN finance.journals j ON j.id = ep.journal_id AND j.status = 'posted'
        WHERE ep.expense_id = $1
        ORDER BY ep.id`,
      [wb.origin_id],
    );
    if (!payments.rows.length) throw new Error('No posted payment journal is available for settlement');
    const journalIds = payments.rows.map((item) => Number(item.journal_id));
    await q(
      `UPDATE expenses SET status = 'completed',
                           gl_confirmed_at = now(),
                           gl_confirmed_by = $1,
                           updated_at = now()
        WHERE id = $2`,
      [actor.id, wb.origin_id],
    );
    await q(
      `UPDATE waybills
          SET status = 'completed', current_stage = 'completed',
              current_owner_role = NULL, current_owner_user_id = NULL, updated_at = now()
        WHERE id = $1`,
      [wb.id],
    );
    await q(
      `UPDATE waybill_stage_claims
          SET released_at = now(), released_by = $2, release_reason = 'stage completed'
        WHERE waybill_id = $1 AND stage = 'settlement' AND released_at IS NULL`,
      [wb.id, actor.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'posted-to-gl-settlement',
      stageFrom: 'settlement',
      stageTo: 'completed',
      actorId: actor.id,
      actorRole: actor.role_name ?? 'finance',
      payload: { expenseId: wb.origin_id, journalIds },
      client: q as never,
    });
    await recordEvent({
      waybillId: wb.id,
      kind: 'gl-confirmed-settlement',
      stageFrom: 'settlement',
      stageTo: 'completed',
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: { journalIds },
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

const ClaimExpenseForm = z.object({
  waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
});

export async function claimExpenseStageAction(formData: FormData): Promise<void> {
  const parsed = ClaimExpenseForm.parse({ waybillId: String(formData.get('waybillId') ?? '') });
  const actor = await actorForWaybill();
  await claimExpenseStage(expenseActor(actor), parsed.waybillId);
  revalidatePath(`/waybill/${parsed.waybillId}`);
  redirect(`/waybill/${parsed.waybillId}`);
}

export async function releaseExpenseClaimAction(formData: FormData): Promise<void> {
  const parsed = z.object({
    waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
    stage: z.enum(['accounting_review', 'payment', 'settlement']),
    reason: z.string().min(3).max(500),
  }).parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    stage: String(formData.get('stage') ?? ''),
    reason: String(formData.get('reason') ?? '').trim(),
  });
  const actor = await actorForWaybill();
  await releaseExpenseClaim(expenseActor(actor), parsed.waybillId, parsed.stage, parsed.reason);
  revalidatePath(`/waybill/${parsed.waybillId}`);
  redirect(`/waybill/${parsed.waybillId}`);
}

export async function reassignExpenseClaimAction(formData: FormData): Promise<void> {
  const parsed = z.object({
    waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
    stage: z.enum(['accounting_review', 'payment', 'settlement']),
    targetUserId: z.coerce.number().int().positive(),
    reason: z.string().min(3).max(500),
  }).parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    stage: String(formData.get('stage') ?? ''),
    targetUserId: String(formData.get('targetUserId') ?? ''),
    reason: String(formData.get('reason') ?? '').trim(),
  });
  const actor = await actorForWaybill();
  await reassignExpenseClaim(expenseActor(actor), parsed.waybillId, parsed.stage, parsed.targetUserId, parsed.reason);
  revalidatePath(`/waybill/${parsed.waybillId}`);
  redirect(`/waybill/${parsed.waybillId}`);
}

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
  const flow = await loadExpenseFlowContext(wb.id);
  if (flow.stage !== 'accounting_review' && flow.stage !== 'settlement') {
    throw new Error('Expense GL drafts are available only during accounting review or settlement');
  }
  const decision = await authorizeExpenseStage(expenseActor(actor), flow, flow.stage);
  if (!decision.allow) throw new Error(decision.reason);
  await assertExpenseClaim(actor.id, wb.id, flow.stage);

  const expRes = await _query<{ vendor_name: string | null }>(
    `SELECT vendor_name FROM expenses WHERE id = $1`,
    [wb.origin_id],
  );
  if (expRes.rows.length === 0) throw new Error('Expense not found');

  const step = flow.stage === 'settlement' ? 'settlement' : 'accrual';
  const candidate = await upsertDraftJournal({
    expenseId: wb.origin_id,
    vendorName: expRes.rows[0].vendor_name ?? '',
    step,
    preparedBy: actor.id,
  });
  const ai = await aiInvoke('finance:rag', 'chat', {
    temperature: 0,
    systemPrompt: 'Review the supplied balanced Thai expense journal candidate. Explain why each debit and credit is appropriate, flag any concern, and never post or reverse the debit/credit direction. Return concise advice for a human accountant.',
    text: `${step} journal for EXP-${wb.origin_id}, payee ${expRes.rows[0].vendor_name ?? 'employee'}, amount THB ${flow.amount}. Candidate lines: ${JSON.stringify(candidate.lines)}`,
  }, { actorId: actor.id });
  await upsertDraftJournal({
    expenseId: wb.origin_id,
    vendorName: expRes.rows[0].vendor_name ?? '',
    step,
    preparedBy: actor.id,
    aiSuggestion: ai.ok && ai.text
      ? { text: ai.text, model: ai.modelName ?? null }
      : { fallback: true, error: ai.error ?? 'AI unavailable or low confidence' },
    aiConfidence: ai.ok && ai.text ? 0.7 : 0,
  });
  revalidatePath(`/waybill/${wb.id}`);
  revalidatePath(`/waybill/${wb.id}/gl`);
  redirect(`/waybill/${wb.id}/gl?notice=ai-draft-ready`);
}

export async function saveExpenseDraftGlAction(formData: FormData): Promise<void> {
  const parsed = z.object({
    waybillId: z.string().regex(/^WB-\d{4}-\d{6}$/),
    lineCount: z.coerce.number().int().min(2).max(50),
  }).parse({
    waybillId: String(formData.get('waybillId') ?? ''),
    lineCount: String(formData.get('lineCount') ?? ''),
  });
  const wb = await loadWaybill(parsed.waybillId);
  if (!wb || wb.origin !== 'expense') throw new Error('Expense waybill not found');
  const actor = await actorForWaybill();
  const flow = await loadExpenseFlowContext(wb.id);
  if (flow.stage !== 'accounting_review' && flow.stage !== 'settlement') {
    throw new Error('Draft journal editing is not available at this stage');
  }
  const decision = await authorizeExpenseStage(expenseActor(actor), flow, flow.stage);
  if (!decision.allow) throw new Error(decision.reason);
  await assertExpenseClaim(actor.id, wb.id, flow.stage);
  const lines = Array.from({ length: parsed.lineCount }, (_, index) => ({
    account_code: String(formData.get(`accountCode.${index}`) ?? '').trim(),
    debit: Number(formData.get(`debit.${index}`) ?? 0),
    credit: Number(formData.get(`credit.${index}`) ?? 0),
    description: String(formData.get(`description.${index}`) ?? '').trim(),
  }));
  if (lines.some((line) => !line.account_code || !line.description
    || !Number.isFinite(line.debit) || !Number.isFinite(line.credit))) {
    throw new Error('Every GL line requires a valid account, amount, and description');
  }
  await saveDraftJournalLines({
    expenseId: flow.expenseId,
    step: flow.stage === 'settlement' ? 'settlement' : 'accrual',
    actorId: actor.id,
    lines,
  });
  revalidatePath(`/waybill/${wb.id}`);
  revalidatePath(`/waybill/${wb.id}/gl`);
  redirect(`/waybill/${wb.id}/gl?notice=draft-saved`);
}
