// Persistent domain event log. Each mutation calls `publish` to record
// the event in `domain_events` for audit + UI surfacing. Live SSE
// has been replaced by Next.js server-component revalidation.
//
// publish() also fans out one row per recipient into the `notifications`
// inbox so the bell can show per-persona feeds with read-state.

import 'server-only';
import { query } from '@/db';
import { computeRecipients } from '@/notifications/recipients';

export interface PublishOpts {
  actorId?: number | null;
  refType?: string | null;
  refId?: number | null;
  severity?: 'info' | 'success' | 'warning' | 'error';
  message?: string;
}

export async function publish(
  type: string,
  payload: any,
  opts: PublishOpts = {}
): Promise<void> {
  const ts = Date.now();
  const severity = opts.severity || 'info';
  const message = opts.message ?? null;
  const enrichedPayload = { ...payload, message };

  try {
    await query(
      `INSERT INTO domain_events (type, actor_id, ref_type, ref_id, payload, severity, created_at)
       VALUES ($1, $2, $3, $4, $5, $6, to_timestamp($7 / 1000.0))`,
      [
        type,
        opts.actorId ?? null,
        opts.refType ?? null,
        opts.refId ?? null,
        JSON.stringify(enrichedPayload),
        severity,
        ts,
      ]
    );
  } catch {
    // silent — never break the action because logging failed
    return;
  }

  await fanout(type, enrichedPayload, opts, ts, severity);
}

async function fanout(
  type: string,
  payload: any,
  opts: PublishOpts,
  ts: number,
  severity: string
): Promise<void> {
  try {
    const actorName = opts.actorId ? await lookupName(opts.actorId) : null;
    const recipientIds = await computeRecipients({
      type,
      refType: opts.refType ?? null,
      refId: opts.refId ?? null,
      actorId: opts.actorId ?? null,
      payload,
    });
    if (recipientIds.length === 0) return;

    const inboxPayload = {
      ...payload,
      actorId: opts.actorId ?? null,
      actorName,
      severity,
    };

    const rows = recipientIds.map((uid) => [
      uid,
      type,
      opts.refType ?? null,
      opts.refId ?? null,
      JSON.stringify(inboxPayload),
      new Date(ts),
    ]);

    await query(
      `INSERT INTO notifications (user_id, type, target_type, target_id, payload_json, created_at)
       SELECT * FROM UNNEST($1::int[], $2::text[], $3::text[], $4::int[], $5::jsonb[], $6::timestamptz[])`,
      [
        rows.map((r) => r[0]),
        rows.map((r) => r[1]),
        rows.map((r) => r[2]),
        rows.map((r) => r[3]),
        rows.map((r) => r[4]),
        rows.map((r) => r[5]),
      ]
    );
  } catch {
    // silent — fanout failure must not break the action
  }
}

async function lookupName(userId: number): Promise<string | null> {
  const r = await query<{ fullname: string }>(`SELECT fullname FROM users WHERE id = $1`, [userId]);
  return r.rows[0]?.fullname ?? null;
}
