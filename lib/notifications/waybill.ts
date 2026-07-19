import 'server-only';

import { query } from '@/db';
import { STAGE_TO_PERM, stageDepartment } from '@folio-lib/perm/stages';
import { renderNotificationMessage, type NotificationArgs, type NotificationMessageKey } from './catalog';
import type { NotificationCategory } from './queries';

type QueryFn = typeof query;

interface EventInput {
  id: string;
  waybillId: string;
  kind: string;
  stageFrom: string | null;
  stageTo: string | null;
  actorId: number | null;
  payload: Record<string, unknown>;
}

interface WaybillContext {
  id: string;
  origin: 'expense' | 'so' | 'pr' | 'po';
  origin_id: number;
  current_stage: string;
  submitter_id: number | null;
  total_amount: string | null;
  currency: string;
  counterparty: string | null;
  so_number: string | null;
  customer: string | null;
  due_date: string | null;
  invoice_number: string | null;
  submitter_department: string | null;
}

interface Recipient {
  userId: number;
  audience: 'owner' | 'approver' | 'watcher';
}

interface Draft {
  key: NotificationMessageKey;
  category: NotificationCategory;
  audience: Recipient['audience'];
  stageKey: string | null;
  severity: 'info' | 'success' | 'warning' | 'error';
  args: NotificationArgs;
  recipients: Recipient[];
}

const CURRENT_ACTION_KEYS: Record<string, NotificationMessageKey> = {
  department_approval: 'expense.departmentApproval',
  accounting_review: 'expense.accountingReview',
  accounting_approval: 'expense.accountingApproval',
  executive_approval: 'expense.executiveApproval',
  payment: 'expense.payment',
  settlement: 'expense.settlement',
  so_sales_review: 'sales.salesReview',
  so_dept_approval: 'sales.departmentApproval',
  so_credit_check: 'sales.creditCheck',
  so_invoiced: 'sales.invoice',
  so_paid: 'sales.payment',
};

function argsFor(ctx: WaybillContext, event: EventInput): NotificationArgs {
  return {
    waybillId: ctx.id,
    amount: ctx.total_amount ? `${Number(ctx.total_amount).toLocaleString('th-TH')} ${ctx.currency}` : null,
    counterparty: ctx.counterparty,
    soNumber: ctx.so_number,
    customer: ctx.customer,
    dueDate: ctx.due_date ? String(ctx.due_date).slice(0, 10) : null,
    invoiceNumber: ctx.invoice_number,
    actor: event.payload.actorName ?? null,
    submitter: event.payload.submitterName ?? null,
    stage: event.stageFrom ?? ctx.current_stage,
    reason: event.payload.reason ?? null,
    age: event.payload.age ?? null,
    assignee: event.payload.assigneeName ?? null,
  };
}

function addRecipient(map: Map<number, Recipient>, recipient: Recipient): void {
  const existing = map.get(recipient.userId);
  const priority = { watcher: 1, owner: 2, approver: 3 } as const;
  if (!existing || priority[recipient.audience] > priority[existing.audience]) {
    map.set(recipient.userId, recipient);
  }
}

async function loadContext(q: QueryFn, waybillId: string): Promise<WaybillContext | null> {
  const r = await q<WaybillContext>(
    `SELECT w.id, w.origin, w.origin_id, w.current_stage, w.submitter_id,
            w.total_amount::text, w.currency,
            COALESCE(e.vendor_name, so.so_number, pr.vendor_name, po.vendor_name) AS counterparty,
            so.so_number, c.name AS customer, so.due_date::text, so.invoice_number,
            (SELECT ud.department_id FROM perm.user_departments ud WHERE ud.user_id = w.submitter_id LIMIT 1) AS submitter_department
       FROM waybills w
       LEFT JOIN expenses e ON w.origin = 'expense' AND e.id = w.origin_id
       LEFT JOIN sales_orders so ON w.origin = 'so' AND so.id = w.origin_id
       LEFT JOIN customers c ON c.id = so.customer_id
       LEFT JOIN purchase_requisitions pr ON w.origin = 'pr' AND pr.id = w.origin_id
       LEFT JOIN purchase_orders po ON w.origin = 'po' AND po.id = w.origin_id
      WHERE w.id = $1`,
    [waybillId],
  );
  return r.rows[0] ?? null;
}

async function eligibleUsers(
  q: QueryFn,
  ctx: WaybillContext,
  stage: string,
  actorId: number | null,
): Promise<number[]> {
  const permission = STAGE_TO_PERM[stage];
  if (!permission) return [];
  const sameDept = stage === 'department_approval' || stage === 'so_sales_review' || stage === 'so_dept_approval';
  const department = sameDept ? ctx.submitter_department : stageDepartment(stage);
  const params: unknown[] = [permission, ctx.submitter_id, actorId];
  const filters = [
    `u.is_active IS TRUE`,
    `($3::int IS NULL OR u.id <> $3)`,
    `(u.id <> COALESCE($2, -1))`,
    `(
       EXISTS (SELECT 1 FROM perm.user_roles ur JOIN perm.role_permissions rp ON rp.role_id = ur.role_id AND rp.role_kind = ur.role_kind WHERE ur.user_id = u.id AND rp.permission_id = $1)
       OR EXISTS (SELECT 1 FROM perm.user_permissions up WHERE up.user_id = u.id AND up.permission_id = $1 AND up.revoked_at IS NULL AND (up.starts_at IS NULL OR up.starts_at <= now()) AND (up.ends_at IS NULL OR up.ends_at > now()))
       OR EXISTS (SELECT 1 FROM perm.user_departments ud2 JOIN perm.department_permissions dp ON dp.department_id = ud2.department_id WHERE ud2.user_id = u.id AND dp.permission_id = $1)
     )`,
  ];
  if (department) {
    params.push(department);
    filters.push(`ud.department_id = $${params.length}`);
  }
  if (stage === 'executive_approval') {
    filters.push(`split_part(COALESCE(hr.role_id, ''), '::', 1) IN ('cfo', 'ceo')`);
  }
  if (stage === 'payment') filters.push(`ud.department_id = 'finance'`);
  if (stage === 'settlement') filters.push(`ud.department_id = 'accounting'`);
  if (sameDept && !ctx.submitter_department) return [];

  const r = await q<{ id: number }>(
    `SELECT DISTINCT u.id
       FROM users u
       LEFT JOIN perm.user_departments ud ON ud.user_id = u.id
       LEFT JOIN LATERAL (
         SELECT ur.role_id
           FROM perm.user_roles ur
          WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
          ORDER BY ur.granted_at ASC
          LIMIT 1
       ) hr ON TRUE
      WHERE ${filters.join(' AND ')}
      ORDER BY u.id`,
    params,
  );
  return r.rows.map((row) => row.id);
}

async function watchers(q: QueryFn, waybillId: string): Promise<number[]> {
  const r = await q<{ user_id: number }>(
    `SELECT ww.user_id
       FROM waybill_watchers ww
       JOIN users u ON u.id = ww.user_id AND u.is_active IS TRUE
      WHERE ww.waybill_id = $1`,
    [waybillId],
  );
  return r.rows.map((row) => row.user_id);
}

async function owner(q: QueryFn, userId: number | null): Promise<string | null> {
  if (!userId) return null;
  const r = await q<{ fullname: string }>('SELECT fullname FROM users WHERE id = $1', [userId]);
  return r.rows[0]?.fullname ?? null;
}

async function actorName(q: QueryFn, userId: number | null): Promise<string | null> {
  if (!userId) return null;
  const r = await q<{ fullname: string }>('SELECT fullname FROM users WHERE id = $1', [userId]);
  return r.rows[0]?.fullname ?? null;
}

function makeDraft(
  ctx: WaybillContext,
  event: EventInput,
  key: NotificationMessageKey,
  category: NotificationCategory,
  audience: Recipient['audience'],
  stageKey: string | null,
  recipients: Recipient[],
  severity: Draft['severity'] = category === 'action' ? 'warning' : 'info',
): Draft {
  return { key, category, audience, stageKey, severity, args: argsFor(ctx, event), recipients };
}

function actionDraft(
  ctx: WaybillContext,
  event: EventInput,
  key: NotificationMessageKey,
  stageKey: string,
  userIds: number[],
): Draft {
  return makeDraft(ctx, event, key, 'action', 'approver', stageKey, userIds.map((userId) => ({ userId, audience: 'approver' })));
}

function updateDraft(
  ctx: WaybillContext,
  event: EventInput,
  key: NotificationMessageKey,
  recipients: Recipient[],
  severity: Draft['severity'] = 'info',
): Draft {
  return makeDraft(ctx, event, key, 'update', recipients[0]?.audience ?? 'owner', null, recipients, severity);
}

async function draftForEvent(q: QueryFn, ctx: WaybillContext, event: EventInput): Promise<Draft[]> {
  const ownerName = await owner(q, ctx.submitter_id);
  const actionName = await actorName(q, event.actorId);
  const eventWithNames: EventInput = {
    ...event,
    payload: {
      ...event.payload,
      actorName: event.payload.actorName ?? actionName,
      submitterName: event.payload.submitterName ?? ownerName,
    },
  };
  const recipients = new Map<number, Recipient>();
  if (ctx.submitter_id) addRecipient(recipients, { userId: ctx.submitter_id, audience: 'owner' });
  for (const id of await watchers(q, ctx.id)) addRecipient(recipients, { userId: id, audience: 'watcher' });
  const updates = (key: NotificationMessageKey, severity: Draft['severity'] = 'info') =>
    updateDraft(ctx, eventWithNames, key, [...recipients.values()], severity);
  const actions = async (key: NotificationMessageKey, stage: string) =>
    actionDraft(ctx, eventWithNames, key, stage, await eligibleUsers(q, ctx, stage, event.actorId));

  if (ctx.origin === 'expense') {
    if (event.kind === 'submitted') return [updates('expense.submitted'), await actions('expense.departmentApproval', event.stageTo ?? 'department_approval')];
    if (event.kind === 'rejected') return [updates('expense.rejected', 'error')];
    if (event.kind === 'resubmitted') return [await actions('expense.resubmitted', event.stageTo ?? 'department_approval')];
    if (event.kind === 'payment-confirmed') return [updates('expense.paymentConfirmed', 'success'), await actions('expense.settlement', 'settlement')];
    if (event.kind === 'posted-to-gl-settlement' || event.kind === 'gl-confirmed-settlement') return [updates('expense.completed', 'success')];
    if (event.kind === 'stage-released') return [await actions('expense.released', event.stageTo ?? event.stageFrom ?? ctx.current_stage)];
    if (event.kind === 'stage-claimed') {
      const assigned = Number(event.payload.claimedBy ?? event.actorId ?? 0);
      return assigned ? [makeDraft(ctx, eventWithNames, 'expense.assigned', 'action', 'approver', event.stageTo ?? ctx.current_stage, [{ userId: assigned, audience: 'approver' }])] : [];
    }
    if (event.kind === 'stage-reassigned') {
      const target = Number(event.payload.toUserId ?? 0);
      const previous = Number(event.payload.fromUserId ?? 0);
      const out: Draft[] = [];
      if (target) out.push(makeDraft(ctx, eventWithNames, 'expense.assigned', 'action', 'approver', event.stageTo ?? ctx.current_stage, [{ userId: target, audience: 'approver' }]));
      if (previous) out.push(makeDraft(ctx, eventWithNames, 'expense.reassigned', 'update', 'owner', null, [{ userId: previous, audience: 'owner' }]));
      return out;
    }
    if (event.kind === 'advanced' || event.kind === 'posted-to-gl-accrual' || event.kind === 'executive-skipped') {
      if (event.stageFrom === 'department_approval') return [updates('expense.departmentApproved'), await actions('expense.accountingReview', 'accounting_review')];
      if (event.stageFrom === 'accounting_review') return [updates('expense.accountingReviewed'), await actions('expense.accountingApproval', 'accounting_approval')];
      if (event.stageFrom === 'accounting_approval') {
        const executive = event.stageTo === 'executive_approval';
        return [updates(executive ? 'expense.accountingApprovedExecutive' : 'expense.accountingApprovedPayment'), await actions(executive ? 'expense.executiveApproval' : 'expense.payment', executive ? 'executive_approval' : 'payment')];
      }
      if (event.stageFrom === 'executive_approval') return [updates('expense.executiveApproved'), await actions('expense.payment', 'payment')];
    }
    return [];
  }

  if (ctx.origin === 'so') {
    if (event.kind === 'so-submitted') return [updates('sales.submitted'), await actions('sales.salesReview', 'so_sales_review')];
    if (event.kind === 'so-auto-approved') return [updates('sales.autoApproved'), await actions('sales.creditCheck', 'so_credit_check')];
    if (event.kind === 'so-reviewed') return [updates('sales.reviewed'), await actions('sales.departmentApproval', 'so_dept_approval')];
    if (event.kind === 'so-dept-approved') return [updates('sales.departmentApproved'), await actions('sales.creditCheck', 'so_credit_check')];
    if (event.kind === 'so-credit-checked') return [updates('sales.creditChecked'), await actions('sales.invoice', 'so_invoiced')];
    if (event.kind === 'so-invoiced') return [updates('sales.invoiced'), await actions('sales.payment', 'so_paid')];
    if (event.kind === 'so-paid') return [updates('sales.paid', 'success')];
    if (event.kind === 'so-rejected' || event.kind === 'rejected') return [updates('sales.rejected', 'error')];
    if (event.kind === 'advanced') {
      if (event.stageFrom === 'so_sales_review') return [updates('sales.reviewed'), await actions('sales.departmentApproval', 'so_dept_approval')];
      if (event.stageFrom === 'so_dept_approval') return [updates('sales.departmentApproved'), await actions('sales.creditCheck', 'so_credit_check')];
      if (event.stageFrom === 'so_credit_check') return [updates('sales.creditChecked'), await actions('sales.invoice', 'so_invoiced')];
    }
    return [];
  }
  return [];
}

export async function notifyWaybillEvent(q: QueryFn, event: EventInput): Promise<void> {
  const ctx = await loadContext(q, event.waybillId);
  if (!ctx) return;

  if (event.stageFrom) {
    await q(
      `UPDATE notifications
          SET resolved_at = COALESCE(resolved_at, now()), resolved_by = COALESCE(resolved_by, $3)
        WHERE waybill_id = $1 AND category = 'action' AND resolved_at IS NULL
          AND stage_key = $2`,
      [ctx.id, event.stageFrom, event.actorId],
    );
  }
  if (event.kind === 'rejected' || event.kind === 'so-rejected') {
    await q(
      `UPDATE notifications SET resolved_at = COALESCE(resolved_at, now()), resolved_by = COALESCE(resolved_by, $2)
        WHERE waybill_id = $1 AND category = 'action' AND resolved_at IS NULL`,
      [ctx.id, event.actorId],
    );
  }

  const drafts = (await draftForEvent(q, ctx, event)).filter((draft) => draft.recipients.length > 0);
  for (const draft of drafts) {
    const args = { ...draft.args, message: renderNotificationMessage(draft.key, draft.args) };
    for (const recipient of draft.recipients) {
      await q(
        `INSERT INTO notifications
           (user_id, type, target_type, target_id, waybill_id, event_id, category, audience,
            stage_key, message_key, payload_json, severity, href, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11::jsonb, $12, $13, now())
         ON CONFLICT (event_id, user_id, message_key) DO NOTHING`,
        [
          recipient.userId,
          `waybill.${ctx.origin}.${event.kind}`,
          ctx.origin,
          ctx.origin_id,
          ctx.id,
          event.id,
          draft.category,
          recipient.audience,
          draft.stageKey,
          draft.key,
          JSON.stringify(args),
          draft.severity,
          `/waybill/${encodeURIComponent(ctx.id)}`,
        ],
      );
    }
  }
}

export async function reconcileOpenActionsForUser(userId: number): Promise<void> {
  const r = await query<{
    id: string;
    current_stage: string;
    origin: 'expense' | 'so';
    submitter_id: number | null;
    event_id: string | null;
    event_kind: string | null;
    event_stage_from: string | null;
    event_stage_to: string | null;
    event_actor_id: number | null;
    event_payload: Record<string, unknown> | null;
    claimed_by: number | null;
  }>(
    `SELECT w.id, w.current_stage, w.origin, w.submitter_id,
            e.id AS event_id, e.kind AS event_kind, e.stage_from AS event_stage_from,
            e.stage_to AS event_stage_to, e.actor_id AS event_actor_id, e.payload AS event_payload,
            c.claimed_by
       FROM waybills w
       LEFT JOIN LATERAL (
         SELECT id, kind, stage_from, stage_to, actor_id, payload
           FROM waybill_events
          WHERE waybill_id = w.id
          ORDER BY sequence DESC
          LIMIT 1
       ) e ON TRUE
       LEFT JOIN LATERAL (
         SELECT claimed_by
           FROM waybill_stage_claims
          WHERE waybill_id = w.id AND stage = w.current_stage AND released_at IS NULL
          LIMIT 1
       ) c ON TRUE
      WHERE w.status = 'open' AND w.origin IN ('expense', 'so')
        AND w.current_stage = ANY($1::text[])`,
    [Object.keys(CURRENT_ACTION_KEYS)],
  );
  for (const wb of r.rows) {
    if (wb.submitter_id === userId || !wb.event_id || wb.claimed_by) continue;
    const ctx = await loadContext(query, wb.id);
    if (!ctx) continue;
    const eligible = await eligibleUsers(query, ctx, wb.current_stage, null);
    if (!eligible.includes(userId)) continue;
    const key = CURRENT_ACTION_KEYS[wb.current_stage];
    if (!key) continue;
    const event: EventInput = {
      id: wb.event_id,
      waybillId: wb.id,
      kind: wb.event_kind ?? 'reconciled',
      stageFrom: wb.event_stage_from,
      stageTo: wb.event_stage_to,
      actorId: wb.event_actor_id,
      payload: wb.event_payload ?? {},
    };
    const [ownerName, actionName] = await Promise.all([owner(query, ctx.submitter_id), actorName(query, event.actorId)]);
    const args = {
      ...argsFor(ctx, event),
      actorName: actionName,
      submitterName: ownerName,
      message: renderNotificationMessage(key, {
        ...argsFor(ctx, event),
        actor: actionName,
        submitter: ownerName,
      }),
    };
    await query(
      `UPDATE notifications
          SET resolved_at = NULL, resolved_by = NULL
        WHERE user_id = $1 AND waybill_id = $2 AND category = 'action' AND stage_key = $3
          AND resolved_at IS NOT NULL`,
      [userId, wb.id, wb.current_stage],
    );
    await query(
      `INSERT INTO notifications
         (user_id, type, target_type, target_id, waybill_id, event_id, category, audience,
          stage_key, message_key, payload_json, severity, href, created_at)
       VALUES ($1, $2, $3, $4, $5, $6, 'action', 'approver', $7, $8, $9::jsonb, 'warning', $10, now())
       ON CONFLICT (event_id, user_id, message_key) DO NOTHING`,
      [
        userId,
        `waybill.${ctx.origin}.reconciled`,
        ctx.origin,
        ctx.origin_id,
        ctx.id,
        wb.event_id,
        wb.current_stage,
        key,
        JSON.stringify(args),
        `/waybill/${encodeURIComponent(ctx.id)}`,
      ],
    );
  }
}
