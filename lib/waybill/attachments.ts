// lib/waybill/attachments.ts
//
// CRUD + helpers for `waybill_attachments`. Writes also append a
// `kind='attached'` row to `waybill_events` so the linked-list audit
// chain preserves chronology (top-to-bottom timeline).
//
// See migration 2026-07-10-A-waybill-attachments.

import 'server-only';
import { query } from '../db';
import { recordEvent, type WaybillEventRow } from './events';
import type { WaybillAttachmentKind } from './kinds';
import { isAllowedKind } from './kinds';

export interface WaybillAttachmentRow {
  id: string;
  waybill_id: string;
  stage_key: string;
  kind: WaybillAttachmentKind;
  storage_backend: string;
  storage_key: string;
  filename: string;
  content_type: string;
  byte_size: number;
  uploaded_by: number;
  uploaded_role: string;
  caption: string | null;
  occurred_at: Date;
  created_at: Date;
}

export interface RecordAttachmentInput {
  waybillId: string;
  stageKey: string;
  kind: WaybillAttachmentKind;
  storageKey: string;
  filename: string;
  contentType: string;
  byteSize: number;
  actorId: number;
  actorRole: string;
  caption?: string | null;
  client?: typeof query;
}

export async function recordAttachment(input: RecordAttachmentInput): Promise<WaybillAttachmentRow> {
  if (!isAllowedKind(input.stageKey, input.kind)) {
    throw new Error(
      `kind '${input.kind}' is not allowed at stage '${input.stageKey}'`,
    );
  }
  if (input.byteSize < 0) throw new Error('byteSize must be >= 0');

  const q = input.client ?? query;
  const inserted = await q<WaybillAttachmentRow>(
    `INSERT INTO waybill_attachments (
       waybill_id, stage_key, kind, storage_key,
       filename, content_type, byte_size,
       uploaded_by, uploaded_role, caption
     ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
     RETURNING *`,
    [
      input.waybillId,
      input.stageKey,
      input.kind,
      input.storageKey,
      input.filename,
      input.contentType,
      input.byteSize,
      input.actorId,
      input.actorRole,
      input.caption ?? null,
    ],
  );
  const row = inserted.rows[0];

  await recordEvent({
    client: q,
    waybillId: input.waybillId,
    kind: 'attached',
    stageFrom: null,
    stageTo: input.stageKey,
    actorId: input.actorId,
    actorRole: input.actorRole,
    payload: {
      attachment_id: row.id,
      filename: input.filename,
      kind: input.kind,
      byte_size: input.byteSize,
    },
  });

  return row;
}

export async function listAttachments(waybillId: string): Promise<WaybillAttachmentRow[]> {
  const r = await query<WaybillAttachmentRow>(
    `SELECT * FROM waybill_attachments
      WHERE waybill_id = $1
      ORDER BY occurred_at DESC, id DESC`,
    [waybillId],
  );
  return r.rows;
}

export async function listAttachmentsByStage(
  waybillId: string,
  stageKey: string,
): Promise<WaybillAttachmentRow[]> {
  const r = await query<WaybillAttachmentRow>(
    `SELECT * FROM waybill_attachments
      WHERE waybill_id = $1 AND stage_key = $2
      ORDER BY occurred_at DESC, id DESC`,
    [waybillId, stageKey],
  );
  return r.rows;
}

export async function getAttachment(id: string | number): Promise<WaybillAttachmentRow | null> {
  const r = await query<WaybillAttachmentRow>(
    `SELECT * FROM waybill_attachments WHERE id = $1 LIMIT 1`,
    [id],
  );
  return r.rows[0] ?? null;
}

export async function attachmentsAndEventsMerged(
  waybillId: string,
): Promise<Array<{ kind: 'event' | 'attachment'; at: Date; event?: WaybillEventRow; attachment?: WaybillAttachmentRow }>> {
  const [att, ev] = await Promise.all([
    listAttachments(waybillId),
    query<WaybillEventRow>(
      `SELECT id::text AS id, waybill_id, sequence, previous_event_id, kind,
              stage_from, stage_to, actor_id, actor_role,
              NULL::bytea AS actor_signature,
              occurred_at, payload
         FROM waybill_events
        WHERE waybill_id = $1
        ORDER BY occurred_at DESC, sequence DESC`,
      [waybillId],
    ),
  ]);
  const merged: Array<{ kind: 'event' | 'attachment'; at: Date; event?: WaybillEventRow; attachment?: WaybillAttachmentRow }> = [];
  for (const e of ev.rows) {
    merged.push({ kind: 'event', at: e.occurred_at, event: e });
  }
  for (const a of att) {
    merged.push({ kind: 'attachment', at: a.occurred_at, attachment: a });
  }
  merged.sort((x, y) => y.at.getTime() - x.at.getTime());
  return merged;
}
