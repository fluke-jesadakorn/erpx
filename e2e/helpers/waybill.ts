import type { APIRequestContext, Page } from '@playwright/test';
import { createHmac } from 'node:crypto';
import { exec, q1, userIdByCode } from './db';

export interface CreateWaybillOpts {
  submitter: string;
  amount: number;
  vendorName: string;
  vendorTag: string;
  description?: string;
}

const TAX_RATE = 0.07;

export async function createExpense(opts: CreateWaybillOpts): Promise<{ expenseId: number; waybillId: string }> {
  const submitterId = await userIdByCode(opts.submitter);
  const subtotal = +(opts.amount / (1 + TAX_RATE)).toFixed(2);
  const vat = +(opts.amount - subtotal).toFixed(2);
  const inserted = await q1<{ id: number }>(
    `INSERT INTO expenses (
       submitter_id, vendor_name, transaction_date, subtotal, vat_amount, total_amount,
       payment_method, status, created_to, vendor_address, created_to_address, ocr_raw_json
     ) VALUES ($1, $2, current_date, $3, $4, $5, 'cash', 'submission', $2, $6, $6, $7)
     RETURNING id`,
    [
      submitterId,
      `${opts.vendorTag}::${opts.vendorName}`,
      subtotal,
      vat,
      opts.amount,
      'Test Address',
      JSON.stringify({ vendorName: opts.vendorName, totalAmount: opts.amount, subtotal, vatAmount: vat, items: [] }),
    ],
  );
  const expenseId = inserted!.id;
  const desc = opts.description ?? `Test expense ${opts.vendorTag}`;
  await exec(
    `INSERT INTO expense_items (expense_id, description, qty, unit_price, amount, mapped_account_code)
     VALUES ($1, $2, 1, $3, $3, '510100')`,
    [expenseId, desc, subtotal],
  );
  const wb = await q1<{ id: string }>(
    `INSERT INTO waybills (id, origin, origin_id, fiscal_year, waybill_kind,
                          current_stage, total_amount, status, submitter_id,
                          created_at, updated_at)
     VALUES (
       folio.next_waybill_number(EXTRACT(YEAR FROM now())::smallint),
       'expense', $1,
       EXTRACT(YEAR FROM now())::smallint,
       'reimbursement',
       'submission', $2, 'open', $3, now(), now()
     )
     RETURNING id`,
    [expenseId, opts.amount, submitterId],
  );
  const waybillId = wb?.id ?? null;
  if (!waybillId) throw new Error('createExpense: failed to insert waybill');
  await insertSubmittedEvent(waybillId, submitterId, opts.vendorName, opts.amount);
  return { expenseId, waybillId };
}

function signEvent(
  waybillId: string,
  sequence: number,
  kind: string,
  stageFrom: string,
  stageTo: string,
  payloadJson: string,
): Buffer {
  const sigInput = [waybillId, sequence, kind, stageFrom, stageTo, payloadJson].join('|');
  return createHmac('sha256', process.env.SESSION_SECRET ?? 'wb-secret')
    .update(sigInput)
    .digest();
}

async function insertSubmittedEvent(
  waybillId: string,
  actorId: number,
  vendorName: string,
  totalAmount: number,
): Promise<void> {
  const payload = { vendor: vendorName, totalAmount };
  const payloadJson = JSON.stringify(payload);
  await exec(
    `INSERT INTO waybill_events
       (waybill_id, sequence, previous_event_id, kind, stage_from, stage_to,
        actor_id, actor_role, actor_signature, payload)
     VALUES (
       $1, 1, NULL, 'submitted', NULL, 'submission',
       $2, 'officer', $3, $4::jsonb
     )`,
    [
      waybillId,
      actorId,
      signEvent(waybillId, 1, 'submitted', '', 'submission', payloadJson),
      payloadJson,
    ],
  );
}

export async function advanceWaybill(waybillId: string, toStage: string, amount?: number): Promise<void> {
  const updateAmount = amount ?? null;
  if (updateAmount != null) {
    await exec(`UPDATE waybills SET total_amount = $2 WHERE id = $1`, [waybillId, updateAmount]);
  }
  await exec(`UPDATE waybills SET current_stage = $2, current_owner_role = NULL WHERE id = $1`, [waybillId, toStage]);
  await exec(
    `UPDATE expenses SET status = $2 WHERE id = (SELECT origin_id FROM waybills WHERE id = $1 AND origin='expense')`,
    [waybillId, toStage],
  );
}

export async function signInAs(req: APIRequestContext, employeeCode: string): Promise<{ id: number; role: string }> {
  const id = await userIdByCode(employeeCode);
  const r = await req.post('/api/actor', { data: { id } });
  if (!r.ok()) {
    const body = await r.text();
    throw new Error(`Sign-in failed for ${employeeCode}: ${r.status()} ${body}`);
  }
  const body = (await r.json()) as { user: { id: number }; role: string };
  return { id: body.user.id, role: body.role };
}

export async function signInOnPage(page: Page, employeeCode: string): Promise<void> {
  const id = await userIdByCode(employeeCode);
  const r = await page.request.post('/api/actor', { data: { id } });
  if (!r.ok()) throw new Error(`Sign-in failed for ${employeeCode}: ${r.status()}`);
}

export async function signOut(req: APIRequestContext): Promise<void> {
  await req.delete('/api/actor');
}

export async function actorContext(req: APIRequestContext): Promise<{ id: number; role: string }> {
  const me = await req.get('/api/me');
  if (!me.ok()) return { id: 0, role: 'guest' };
  const body = (await me.json()) as { id?: number; role?: string };
  return { id: body.id ?? 0, role: body.role ?? 'guest' };
}

export async function visibleStageForActor(stage: string, _role: string): Promise<boolean> {
  const r = await q1<{ permission_id: string }>(
    `SELECT permission_id FROM perm.permissions WHERE id = $1`,
    [`stage:${stage}:act::allow`],
  );
  return !!r;
}

export async function tileAccessibleByRole(tileId: string, role: string): Promise<boolean> {
  const r = await q1<{ role_id: string }>(
    `SELECT rp.role_id FROM perm.role_permissions rp
     JOIN perm.tiles t ON t.view_perm_id = rp.permission_id
     WHERE t.id = $1 AND rp.role_id = $2`,
    [tileId, `${role}::5`],
  );
  return !!r;
}
