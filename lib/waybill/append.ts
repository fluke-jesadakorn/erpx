// lib/waybill/append.ts — small helper to attach waybill_events to existing
// state-change actions (advance, reject, settle, etc.) without rewriting the
// whole actions.ts module. Each helper runs in its own transaction; the
// caller's primary state mutation is already committed by the time this fires
// (audit log is best-effort: a failure here does NOT roll back the state).

import 'server-only';
import { query } from '../db';
import { recordEvent, type WaybillEventKind } from './events';

export interface AppendEventInput {
  origin: 'expense' | 'pr' | 'po' | 'so';
  originId: number;
  kind: WaybillEventKind;
  stageFrom?: string | null;
  stageTo?: string | null;
  actorId?: number | null;
  actorRole?: string | null;
  payload?: Record<string, unknown> | null;
}

interface WaybillOriginMeta {
  submitterId: number | null;
  vendorName: string | null;
  totalAmount: string | null;
  currency: string;
}

async function loadOriginMeta(
  origin: 'expense' | 'pr' | 'po' | 'so',
  originId: number,
): Promise<WaybillOriginMeta> {
  if (origin === 'expense') {
    const r = await query<WaybillOriginMeta>(
      `SELECT submitter_id AS "submitterId", vendor_name AS "vendorName",
              total_amount::text AS "totalAmount", 'THB' AS currency
         FROM expenses WHERE id = $1`,
      [originId],
    );
    return r.rows[0] ?? { submitterId: null, vendorName: null, totalAmount: null, currency: 'THB' };
  }
  if (origin === 'pr') {
    const r = await query<WaybillOriginMeta>(
      `SELECT requester_id AS "submitterId", vendor_name AS "vendorName",
              total_estimate::text AS "totalAmount", COALESCE(currency, 'THB') AS currency
         FROM purchase_requisitions WHERE id = $1`,
      [originId],
    );
    return r.rows[0] ?? { submitterId: null, vendorName: null, totalAmount: null, currency: 'THB' };
  }
  if (origin === 'po') {
    const r = await query<WaybillOriginMeta>(
      `SELECT pr.requester_id AS "submitterId", po.vendor_name AS "vendorName",
              po.total_amount::text AS "totalAmount", COALESCE(po.currency, 'THB') AS currency
         FROM purchase_orders po
         LEFT JOIN purchase_requisitions pr ON pr.id = po.pr_id
        WHERE po.id = $1`,
      [originId],
    );
    return r.rows[0] ?? { submitterId: null, vendorName: null, totalAmount: null, currency: 'THB' };
  }
  const r = await query<WaybillOriginMeta>(
    `SELECT sales_rep_id AS "submitterId", NULL::text AS "vendorName",
            total_amount::text AS "totalAmount", COALESCE(currency, 'THB') AS currency
       FROM sales_orders WHERE id = $1`,
    [originId],
  );
  return r.rows[0] ?? { submitterId: null, vendorName: null, totalAmount: null, currency: 'THB' };
}

async function ensureWaybillExists(
  origin: 'expense' | 'pr' | 'po' | 'so',
  originId: number,
  stageTo?: string | null,
  stageFrom?: string | null,
): Promise<string | null> {
  const meta = await loadOriginMeta(origin, originId);
  const stageForUpdate = stageTo ?? stageFrom ?? null;
  const stage = stageTo ?? stageFrom ?? 'submission';
  const r = await query<{ id: string }>(
    `INSERT INTO waybills
       (id, origin, origin_id, fiscal_year, waybill_kind,
        submitter_id, vendor_name, total_amount, currency,
        current_stage, current_owner_role, status, created_at, updated_at)
     VALUES (
       next_waybill_number(EXTRACT(YEAR FROM now())::smallint),
       $1, $2,
       EXTRACT(YEAR FROM now())::smallint,
       $3, $4, $5, $6, $7,
       $8, $8, 'open', now(), now()
     )
    ON CONFLICT (origin, origin_id) DO UPDATE
       SET submitter_id = COALESCE(waybills.submitter_id, EXCLUDED.submitter_id),
           vendor_name = COALESCE(waybills.vendor_name, EXCLUDED.vendor_name),
           total_amount = COALESCE(waybills.total_amount, EXCLUDED.total_amount),
           currency = COALESCE(waybills.currency, EXCLUDED.currency),
           current_stage = CASE
             WHEN $9::text IS NULL THEN waybills.current_stage
             ELSE EXCLUDED.current_stage
           END,
           current_owner_role = CASE
             WHEN $9::text IS NULL THEN waybills.current_owner_role
             ELSE EXCLUDED.current_owner_role
           END,
           updated_at = now()
     RETURNING id`,
    [
      origin,
      originId,
      origin === 'expense' ? 'reimbursement'
        : origin === 'so' ? 'sales'
          : 'procurement',
      meta.submitterId,
      meta.vendorName,
      meta.totalAmount,
      meta.currency,
      stage,
      stageForUpdate,
    ],
  );
  return r.rows[0]?.id ?? null;
}

export async function appendWaybillEvent(
  input: AppendEventInput,
): Promise<string | null> {
  const waybillId = await ensureWaybillExists(input.origin, input.originId, input.stageTo, input.stageFrom);
  if (!waybillId) return null;
  await recordEvent({
    waybillId,
    kind: input.kind,
    stageFrom: input.stageFrom,
    stageTo: input.stageTo,
    actorId: input.actorId,
    actorRole: input.actorRole,
    payload: input.payload,
  });
  return waybillId;
}
