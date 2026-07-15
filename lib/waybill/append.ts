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

async function resolveWaybill(
  origin: 'expense' | 'pr' | 'po' | 'so',
  originId: number,
): Promise<string | null> {
  const r = await query<{ id: string }>(
    `SELECT id FROM waybills WHERE origin = $1 AND origin_id = $2`,
    [origin, originId],
  );
  return r.rows[0]?.id ?? null;
}

async function ensureWaybillExists(
  origin: 'expense' | 'pr' | 'po' | 'so',
  originId: number,
): Promise<string | null> {
  const existing = await resolveWaybill(origin, originId);
  if (existing) return existing;
  const r = await query<{ id: string }>(
    `INSERT INTO waybills
       (id, origin, origin_id, fiscal_year, waybill_kind,
        current_stage, status, created_at, updated_at)
     VALUES (
       next_waybill_number(EXTRACT(YEAR FROM now())::smallint),
       $1, $2,
       EXTRACT(YEAR FROM now())::smallint,
       $3,
       'submission', 'open', now(), now()
     )
     ON CONFLICT (origin, origin_id) DO UPDATE
       SET updated_at = now()
     RETURNING id`,
    [
      origin,
      originId,
      origin === 'expense' ? 'reimbursement'
        : origin === 'so' ? 'sales'
          : 'procurement',
    ],
  );
  return r.rows[0]?.id ?? null;
}

export async function appendWaybillEvent(
  input: AppendEventInput,
): Promise<string | null> {
  const waybillId = await ensureWaybillExists(input.origin, input.originId);
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