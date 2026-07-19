// lib/waybill/events.ts — tamper-evident event log writer.
// Every state change to a waybill must go through `recordEvent()`. The
// linked-list invariant is enforced in the helper (the DB only enforces
// sequence >= 1).

import 'server-only';
import { createHmac } from 'node:crypto';
import { query, withTransaction } from '../db';
import { notifyWaybillEvent } from '../notifications/waybill';

export type WaybillEventKind =
  | 'created'
  | 'submitted'
  | 'advanced'
  | 'rejected'
  | 'corrected'
  | 'settled'
  | 'posted-to-gl'
  | 'gl-confirmed'
  | 'slip-attached'
  | 'attached'
  | 'signed-off'
  | 'reversed'
  | 'authorization-overridden'
  | 'resubmitted'
  | 'superseded'
  | 'so-submitted'
  | 'so-reviewed'
  | 'so-dept-approved'
  | 'so-credit-checked'
  | 'so-auto-approved'
  | 'so-invoiced'
  | 'so-rejected'
  | 'so-paid'
  | 'posted-to-gl-sales-vat'
  | 'posted-to-gl-sales-accrual'
  | 'posted-to-gl-sales-settlement'
  | 'posted-to-gl-accrual'
  | 'posted-to-gl-settlement'
  | 'gl-confirmed-accrual'
  | 'gl-confirmed-settlement'
  | 'gl-confirmed-sales-vat'
  | 'gl-confirmed-sales-accrual'
  | 'gl-confirmed-sales-settlement'
  | 'stage-claimed'
  | 'stage-released'
  | 'stage-reassigned'
  | 'executive-skipped'
  | 'payment-confirmed';

export interface RecordEventInput {
  waybillId: string;
  kind: WaybillEventKind;
  stageFrom?: string | null;
  stageTo?: string | null;
  actorId?: number | null;
  actorRole?: string | null;
  payload?: Record<string, unknown> | null;
  /** When provided, the event is recorded in this client's transaction.
   *  Accepts either a callable query function or an object exposing `.query`. */
  client?: { query: typeof query } | typeof query;
}

export interface WaybillEventRow {
  id: string;
  waybill_id: string;
  sequence: number;
  previous_event_id: string | null;
  kind: WaybillEventKind;
  stage_from: string | null;
  stage_to: string | null;
  actor_id: number | null;
  actor_role: string | null;
  actor_signature: Buffer | null;
  occurred_at: Date;
  payload: Record<string, unknown> | null;
}

export async function recordEvent(input: RecordEventInput): Promise<WaybillEventRow> {
  const run = async (q: typeof query): Promise<WaybillEventRow> => {
    const prev = await q<{ id: string; sequence: number }>(
      `SELECT id, sequence
         FROM waybill_events
        WHERE waybill_id = $1
        ORDER BY sequence DESC
        LIMIT 1`,
      [input.waybillId],
    );
    const nextSeq = (prev.rows[0]?.sequence ?? 0) + 1;
    const prevId = prev.rows[0]?.id ?? null;

    const payloadJson = input.payload ? JSON.stringify(input.payload) : null;
    const sigInput = [
      input.waybillId,
      nextSeq,
      input.kind,
      input.stageFrom ?? '',
      input.stageTo ?? '',
      payloadJson ?? '',
    ].join('|');
    const sig = createHmac('sha256', process.env.SESSION_SECRET ?? 'wb-secret')
      .update(sigInput)
      .digest();

    const r = await q<WaybillEventRow>(
      `INSERT INTO waybill_events
         (waybill_id, sequence, previous_event_id, kind, stage_from, stage_to,
          actor_id, actor_role, actor_signature, payload)
       VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb)
       RETURNING id, waybill_id, sequence, previous_event_id, kind,
                 stage_from, stage_to, actor_id, actor_role,
                 actor_signature, occurred_at, payload`,
      [
        input.waybillId,
        nextSeq,
        prevId,
        input.kind,
        input.stageFrom ?? null,
        input.stageTo ?? null,
        input.actorId ?? null,
        input.actorRole ?? null,
        sig,
        payloadJson,
      ],
    );
    const event = r.rows[0];
    await notifyWaybillEvent(q, {
      id: event.id,
      waybillId: event.waybill_id,
      kind: event.kind,
      stageFrom: event.stage_from,
      stageTo: event.stage_to,
      actorId: event.actor_id,
      payload: event.payload ?? {},
    });
    return event;
  };

  if (input.client) {
    const fn = typeof input.client === 'function' ? input.client : input.client.query;
    return run(fn as typeof query);
  }
  return withTransaction(run);
}

export async function listEvents(waybillId: string): Promise<WaybillEventRow[]> {
  const r = await query<WaybillEventRow>(
    `SELECT id, waybill_id, sequence, previous_event_id, kind,
            stage_from, stage_to, actor_id, actor_role,
            NULL::bytea AS actor_signature, occurred_at, payload
       FROM waybill_events
      WHERE waybill_id = $1
      ORDER BY sequence ASC`,
    [waybillId],
  );
  return r.rows;
}

export async function verifyEventChain(waybillId: string): Promise<{
  ok: boolean;
  reason?: string;
  total: number;
}> {
  const r = await query<{ id: string; sequence: number; previous_event_id: string | null }>(
    `SELECT id, sequence, previous_event_id
       FROM waybill_events
      WHERE waybill_id = $1
      ORDER BY sequence ASC`,
    [waybillId],
  );
  let expectedSeq = 1;
  let expectedPrev: string | null = null;
  for (const row of r.rows) {
    if (row.sequence !== expectedSeq) {
      return { ok: false, reason: `sequence gap at #${expectedSeq}`, total: r.rows.length };
    }
    if (row.previous_event_id !== expectedPrev) {
      return { ok: false, reason: `previous_event_id mismatch at #${expectedSeq}`, total: r.rows.length };
    }
    expectedSeq += 1;
    expectedPrev = row.id;
  }
  return { ok: true, total: r.rows.length };
}
