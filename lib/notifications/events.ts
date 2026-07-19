import 'server-only';

import { query } from '@/db';

export interface PublishOpts {
  actorId?: number | null;
  refType?: string | null;
  refId?: number | null;
  severity?: 'info' | 'success' | 'warning' | 'error';
  message?: string;
}

export async function publish(
  type: string,
  payload: Record<string, unknown>,
  opts: PublishOpts = {},
): Promise<void> {
  const enrichedPayload = { ...payload, message: opts.message ?? null };
  try {
    await query(
      `INSERT INTO domain_events (type, actor_id, ref_type, ref_id, payload, severity, created_at)
       VALUES ($1, $2, $3, $4, $5, $6, now())`,
      [
        type,
        opts.actorId ?? null,
        opts.refType ?? null,
        opts.refId ?? null,
        JSON.stringify(enrichedPayload),
        opts.severity ?? 'info',
      ],
    );
  } catch {
    // Domain-event logging is intentionally best-effort for non-Waybill events.
  }
}
