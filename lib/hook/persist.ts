import 'server-only';
import { pool } from '@folio-lib/db';
import type { HookEventInput, HookEventRow, HookStatus, HookProvider } from './types';

export async function loadProvider(id: string): Promise<HookProvider | null> {
  const r = await pool.query<{
    id: string;
    display_name: string;
    kind: 'line' | 'generic';
    secret_env: string;
    enabled: boolean;
  }>(
    `SELECT id, display_name, kind, secret_env, enabled FROM hook_providers WHERE id = $1`,
    [id],
  );
  if (r.rows.length === 0) return null;
  const row = r.rows[0];
  return {
    id: row.id,
    displayName: row.display_name,
    kind: row.kind,
    secretEnv: row.secret_env,
    enabled: row.enabled,
  };
}

export async function persistHookEvent(
  input: HookEventInput,
): Promise<{ id: number; duplicate: boolean }> {
  const r = await pool.query<{ id: number }>(
    `INSERT INTO hook_events
       (provider_id, external_id, event_type, payload, headers, signature_ok)
     VALUES ($1, $2, $3, $4::jsonb, $5::jsonb, $6)
     ON CONFLICT (provider_id, external_id) DO NOTHING
     RETURNING id`,
    [
      input.providerId,
      input.externalId,
      input.eventType,
      JSON.stringify(input.payload),
      JSON.stringify(input.headers),
      input.signatureOk,
    ],
  );
  if (r.rows.length === 0) return { id: -1, duplicate: true };
  return { id: r.rows[0].id, duplicate: false };
}

export async function listHookEvents(filter: {
  providerId?: string;
  status?: HookStatus;
  limit?: number;
}): Promise<HookEventRow[]> {
  const where: string[] = [];
  const params: unknown[] = [];
  if (filter.providerId) {
    params.push(filter.providerId);
    where.push(`provider_id = $${params.length}`);
  }
  if (filter.status) {
    params.push(filter.status);
    where.push(`status = $${params.length}`);
  }
  const limit = Math.min(filter.limit ?? 50, 200);
  params.push(limit);
  const r = await pool.query<{
    id: number;
    provider_id: string;
    external_id: string | null;
    event_type: string;
    received_at: string;
    status: HookStatus;
    signature_ok: boolean;
    replay_count: number;
  }>(
    `SELECT id, provider_id, external_id, event_type, received_at, status,
            signature_ok, replay_count
       FROM hook_events
       ${where.length ? 'WHERE ' + where.join(' AND ') : ''}
       ORDER BY received_at DESC
       LIMIT $${params.length}`,
    params,
  );
  return r.rows.map((row) => ({
    id: row.id,
    providerId: row.provider_id,
    externalId: row.external_id,
    eventType: row.event_type,
    receivedAt: row.received_at,
    status: row.status,
    signatureOk: row.signature_ok,
    replayCount: row.replay_count,
  }));
}

export async function markHookEventProcessed(
  id: number,
  actor: string,
): Promise<boolean> {
  const r = await pool.query(
    `UPDATE hook_events
        SET status = 'processed',
            processed_at = now(),
            processed_by = $2,
            error = NULL
      WHERE id = $1
      RETURNING id`,
    [id, actor],
  );
  return r.rows.length > 0;
}

export async function markHookEventFailed(id: number, error: string): Promise<boolean> {
  const r = await pool.query(
    `UPDATE hook_events
        SET status = 'failed',
            error = $2
      WHERE id = $1
      RETURNING id`,
    [id, error],
  );
  return r.rows.length > 0;
}

export async function bumpHookEventReplay(id: number): Promise<boolean> {
  const r = await pool.query(
    `UPDATE hook_events
        SET status = 'received',
            processed_at = NULL,
            processed_by = NULL,
            error = NULL,
            replay_count = replay_count + 1
      WHERE id = $1
      RETURNING id`,
    [id],
  );
  return r.rows.length > 0;
}

export async function loadHookEvent(id: number) {
  const r = await pool.query<{
    id: number;
    provider_id: string;
    external_id: string | null;
    event_type: string;
    received_at: string;
    payload: unknown;
    headers: unknown;
    status: HookStatus;
    signature_ok: boolean;
    replay_count: number;
    processed_at: string | null;
    processed_by: string | null;
    error: string | null;
  }>(
    `SELECT id, provider_id, external_id, event_type, received_at,
            payload, headers, status, signature_ok, replay_count,
            processed_at, processed_by, error
       FROM hook_events WHERE id = $1`,
    [id],
  );
  return r.rows[0] ?? null;
}