import 'server-only';
import { query, withTransaction } from '../db';
import { authorize, type ActorForAuth } from '../perm/authorize';
import { matchPerm } from '../perm/grammar';
import { recordEvent } from './events';
import {
  evaluateExpenseStageRule,
  type ExpenseStage,
} from './expenseRules';

export {
  EXECUTIVE_THRESHOLD_THB,
  nextExpenseStage,
  requiresExecutiveApproval,
  type ExpenseStage,
} from './expenseRules';

export type ClaimableExpenseStage = 'accounting_review' | 'payment' | 'settlement';

export interface ExpenseActor extends ActorForAuth {
  departmentId: string | null;
  rank: number;
  roleName: string;
}

export interface ExpenseFlowContext {
  waybillId: string;
  expenseId: number;
  stage: ExpenseStage;
  amount: number;
  submitterId: number;
  submitterDepartmentId: string | null;
  departmentHeadId: number | null;
  departmentTopRank: number | null;
  accountingHeadId: number | null;
  accountingTopRank: number | null;
  accrualPreparerId: number | null;
}

export class ExpenseFlowError extends Error {
  status: number;

  constructor(message: string, status = 400) {
    super(message);
    this.name = 'ExpenseFlowError';
    this.status = status;
  }
}

export async function loadExpenseFlowContext(waybillId: string): Promise<ExpenseFlowContext> {
  const res = await query<{
    waybill_id: string;
    expense_id: number;
    stage: ExpenseStage;
    amount: number;
    submitter_id: number;
    submitter_department_id: string | null;
    department_head_id: number | null;
    department_top_rank: number | null;
    accounting_head_id: number | null;
    accounting_top_rank: number | null;
    accrual_preparer_id: number | null;
  }>(
    `SELECT wb.id AS waybill_id,
            e.id AS expense_id,
            wb.current_stage AS stage,
            COALESCE(wb.total_amount, e.total_amount, 0)::float8 AS amount,
            e.submitter_id,
            sud.department_id AS submitter_department_id,
            sd.head_user_id AS department_head_id,
            (SELECT min(r.rank)
               FROM perm.user_departments ud
               JOIN users u ON u.id = ud.user_id AND u.is_active IS TRUE
               JOIN perm.user_roles ur ON ur.user_id = ud.user_id AND ur.role_kind = 'hierarchy'
               JOIN perm.roles r ON r.id = ur.role_id
              WHERE ud.department_id = sud.department_id) AS department_top_rank,
            ad.head_user_id AS accounting_head_id,
            (SELECT min(r.rank)
               FROM perm.user_departments ud
               JOIN users u ON u.id = ud.user_id AND u.is_active IS TRUE
               JOIN perm.user_roles ur ON ur.user_id = ud.user_id AND ur.role_kind = 'hierarchy'
               JOIN perm.roles r ON r.id = ur.role_id
              WHERE ud.department_id = 'accounting') AS accounting_top_rank,
            (SELECT je.prepared_by FROM journal_entries je
              WHERE je.expense_id = e.id AND je.step = 'accrual'
              ORDER BY je.id DESC LIMIT 1) AS accrual_preparer_id
       FROM waybills wb
       JOIN expenses e ON wb.origin = 'expense' AND e.id = wb.origin_id
       LEFT JOIN perm.user_departments sud ON sud.user_id = e.submitter_id
       LEFT JOIN perm.departments sd ON sd.id = sud.department_id
       LEFT JOIN perm.departments ad ON ad.id = 'accounting'
      WHERE wb.id = $1`,
    [waybillId],
  );
  const row = res.rows[0];
  if (!row) throw new ExpenseFlowError('Expense waybill not found', 404);
  return {
    waybillId: row.waybill_id,
    expenseId: row.expense_id,
    stage: row.stage,
    amount: Number(row.amount),
    submitterId: row.submitter_id,
    submitterDepartmentId: row.submitter_department_id,
    departmentHeadId: row.department_head_id,
    departmentTopRank: row.department_top_rank,
    accountingHeadId: row.accounting_head_id,
    accountingTopRank: row.accounting_top_rank,
    accrualPreparerId: row.accrual_preparer_id,
  };
}

export async function authorizeExpenseStage(
  actor: ExpenseActor,
  ctx: ExpenseFlowContext,
  stage: ExpenseStage = ctx.stage,
): Promise<{ allow: true; reason: string } | { allow: false; reason: string }> {
  const decision = await authorize(
    actor,
    { kind: 'stage', stage },
    {
      type: 'expense',
      id: ctx.expenseId,
      submitterId: ctx.submitterId,
      submitterDeptId: ctx.submitterDepartmentId,
      deptId: ctx.submitterDepartmentId,
      totalAmount: ctx.amount,
      currentStage: ctx.stage,
    },
  );
  if (!decision.allow) return decision;

  const rule = evaluateExpenseStageRule(actor, ctx, stage);
  return rule.allow ? { allow: true, reason: decision.reason } : rule;
}

export async function claimExpenseStage(actor: ExpenseActor, waybillId: string) {
  const ctx = await loadExpenseFlowContext(waybillId);
  if (!['accounting_review', 'payment', 'settlement'].includes(ctx.stage)) {
    throw new ExpenseFlowError('This stage cannot be claimed', 409);
  }
  const stage = ctx.stage as ClaimableExpenseStage;
  const decision = await authorizeExpenseStage(actor, ctx, stage);
  if (!decision.allow) throw new ExpenseFlowError(decision.reason, 403);
  try {
    return await withTransaction(async (q) => {
      const existing = await q<{ id: number; claimed_by: number }>(
        `SELECT id, claimed_by FROM waybill_stage_claims
          WHERE waybill_id = $1 AND stage = $2 AND released_at IS NULL FOR UPDATE`,
        [waybillId, stage],
      );
      if (existing.rows[0]) {
        if (existing.rows[0].claimed_by === actor.id) return existing.rows[0];
        throw new ExpenseFlowError('This task is already claimed', 409);
      }
      const inserted = await q<{ id: number; claimed_by: number }>(
        `INSERT INTO waybill_stage_claims (waybill_id, stage, claimed_by)
         VALUES ($1, $2, $3) RETURNING id, claimed_by`,
        [waybillId, stage, actor.id],
      );
      await recordEvent({
        client: q as typeof query,
        waybillId,
        kind: 'stage-claimed',
        stageFrom: stage,
        stageTo: stage,
        actorId: actor.id,
        actorRole: actor.roleName,
      });
      return inserted.rows[0];
    });
  } catch (error) {
    if ((error as { code?: string }).code === '23505') {
      throw new ExpenseFlowError('This task is already claimed', 409);
    }
    throw error;
  }
}

export async function assertExpenseClaim(actorId: number, waybillId: string, stage: ClaimableExpenseStage) {
  const claim = await query<{ id: number }>(
    `SELECT id FROM waybill_stage_claims
      WHERE waybill_id = $1 AND stage = $2 AND claimed_by = $3 AND released_at IS NULL`,
    [waybillId, stage, actorId],
  );
  if (!claim.rows[0]) throw new ExpenseFlowError('Claim this task before acting', 409);
  return claim.rows[0];
}

export async function releaseExpenseClaim(
  actor: ExpenseActor,
  waybillId: string,
  stage: ClaimableExpenseStage,
  reason: string,
) {
  if (!reason.trim()) throw new ExpenseFlowError('A release reason is required');
  const expectedDepartment = stage === 'payment' ? 'finance' : 'accounting';
  const manager = actor.departmentId === expectedDepartment && actor.rank <= 4;
  const admin = matchPerm(actor.permissions, 'admin:system:bypass::allow');
  if (!manager && !admin) throw new ExpenseFlowError('Only a relevant department manager may release claims', 403);
  return withTransaction(async (q) => {
    const released = await q<{ id: number }>(
      `UPDATE waybill_stage_claims
          SET released_at = now(), released_by = $3, release_reason = $4
        WHERE waybill_id = $1 AND stage = $2 AND released_at IS NULL
        RETURNING id`,
      [waybillId, stage, actor.id, reason.trim()],
    );
    if (!released.rows[0]) throw new ExpenseFlowError('No open claim found', 404);
    await recordEvent({
      client: q as typeof query,
      waybillId,
      kind: 'stage-released',
      stageFrom: stage,
      stageTo: stage,
      actorId: actor.id,
      actorRole: actor.roleName,
      payload: { reason: reason.trim() },
    });
    return released.rows[0];
  });
}

export async function reassignExpenseClaim(
  actor: ExpenseActor,
  waybillId: string,
  stage: ClaimableExpenseStage,
  targetUserId: number,
  reason: string,
) {
  if (!reason.trim()) throw new ExpenseFlowError('A reassignment reason is required');
  const expectedDepartment = stage === 'payment' ? 'finance' : 'accounting';
  const manager = actor.departmentId === expectedDepartment && actor.rank <= 4;
  const admin = matchPerm(actor.permissions, 'admin:system:bypass::allow');
  if (!manager && !admin) throw new ExpenseFlowError('Only a relevant department manager may reassign claims', 403);
  const ctx = await loadExpenseFlowContext(waybillId);
  if (ctx.stage !== stage) throw new ExpenseFlowError('Waybill is no longer at this stage', 409);
  if (stage === 'payment' && ctx.submitterId === targetUserId) {
    throw new ExpenseFlowError('The submitter cannot pay their own claim', 409);
  }
  return withTransaction(async (q) => {
    const target = await q<{ id: number }>(
      `SELECT u.id FROM users u
        JOIN perm.user_departments ud ON ud.user_id = u.id
        JOIN perm.user_roles ur ON ur.user_id = u.id AND ur.role_kind = 'hierarchy'
       WHERE u.id = $1 AND u.is_active IS TRUE AND ud.department_id = $2`,
      [targetUserId, expectedDepartment],
    );
    if (!target.rows[0]) throw new ExpenseFlowError('Target user is not an eligible active department member');
    const prior = await q<{ id: number; claimed_by: number }>(
      `UPDATE waybill_stage_claims
          SET released_at = now(), released_by = $3, release_reason = $4
        WHERE waybill_id = $1 AND stage = $2 AND released_at IS NULL
        RETURNING id, claimed_by`,
      [waybillId, stage, actor.id, reason.trim()],
    );
    if (!prior.rows[0]) throw new ExpenseFlowError('No open claim found', 404);
    const next = await q<{ id: number; claimed_by: number }>(
      `INSERT INTO waybill_stage_claims (waybill_id, stage, claimed_by)
       VALUES ($1, $2, $3) RETURNING id, claimed_by`,
      [waybillId, stage, targetUserId],
    );
    await recordEvent({
      client: q as typeof query,
      waybillId,
      kind: 'stage-reassigned',
      stageFrom: stage,
      stageTo: stage,
      actorId: actor.id,
      actorRole: actor.roleName,
      payload: { fromUserId: prior.rows[0].claimed_by, toUserId: targetUserId, reason: reason.trim() },
    });
    return next.rows[0];
  });
}
