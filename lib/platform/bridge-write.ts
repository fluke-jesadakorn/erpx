import 'server-only';

import { randomUUID } from 'node:crypto';
import { query, withControlWriteTransaction, withTransaction } from '@/db';
import { buildBridgeShadowEvent } from './bridge-contract';

export interface BridgeEventInput {
  eventType: string;
  aggregateType: string;
  aggregateId: string;
  aggregateVersion: number;
  idempotencyKey: string;
  payload: Record<string, unknown>;
  occurredAt?: Date;
}

export interface BridgeDrainResult {
  enabled: boolean;
  claimed: number;
  published: number;
  failed: number;
}

const MAX_BRIDGE_IDENTITY_BYTES = 256;

type Query = <R extends import('pg').QueryResultRow = import('pg').QueryResultRow>(
  text: string,
  params?: unknown[],
) => Promise<import('pg').QueryResult<R>>;

function validateEvent(input: BridgeEventInput): void {
  if (!input.eventType || !input.aggregateType || !input.aggregateId || !input.idempotencyKey) {
    throw new Error('bridge event identity is required');
  }
  for (const [name, value] of [
    ['event type', input.eventType],
    ['aggregate type', input.aggregateType],
    ['aggregate id', input.aggregateId],
    ['idempotency key', input.idempotencyKey],
  ] as const) {
    if (Buffer.byteLength(value, 'utf8') > MAX_BRIDGE_IDENTITY_BYTES || /[\u0000-\u001f\u007f]/.test(value)) {
      throw new Error(`bridge ${name} is invalid`);
    }
  }
  if (!Number.isSafeInteger(input.aggregateVersion) || input.aggregateVersion < 1) {
    throw new Error('bridge aggregate version must be a positive safe integer');
  }
  if (!input.payload || Array.isArray(input.payload)) throw new Error('bridge event payload must be an object');
}

/** Enqueue in the same Folio transaction as the command that produced it. */
export async function enqueueBridgeEvent(tx: Query, input: BridgeEventInput): Promise<void> {
  validateEvent(input);
  await tx(
    `INSERT INTO platform.bridge_outbox
       (event_type, aggregate_type, aggregate_id, aggregate_version,
        idempotency_key, payload, occurred_at)
     VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, now()))
     ON CONFLICT (idempotency_key) DO NOTHING`,
    [input.eventType, input.aggregateType, input.aggregateId, input.aggregateVersion, input.idempotencyKey, JSON.stringify(input.payload), input.occurredAt ?? null],
  );
}

export async function enqueueBridgeEventStandalone(input: BridgeEventInput): Promise<void> {
  await withTransaction(async (tx) => enqueueBridgeEvent(tx, input));
}

/**
 * Publish shadow events into the ERPX control outbox. This is deliberately
 * opt-in: absent ERPX_BRIDGE_SHADOW_WRITES=true the queue remains durable but
 * Folio stays the only command owner. Failures remain retryable in Folio.
 */
export async function drainBridgeOutbox(limit = 100): Promise<BridgeDrainResult> {
  const boundedLimit = Number.isFinite(limit)
    ? Math.max(1, Math.min(500, Math.floor(limit)))
    : 100;
  const enabled = process.env.ERPX_BRIDGE_SHADOW_WRITES === 'true'
    && Boolean(process.env.ERPX_CONTROL_WRITE_DATABASE_URL?.trim());
  if (!enabled) return { enabled: false, claimed: 0, published: 0, failed: 0 };

  const claimId = randomUUID();
  const rows = await withTransaction(async (tx) => {
    const result = await tx<{
      id: string;
      event_type: string;
      aggregate_type: string;
      aggregate_id: string;
      aggregate_version: number;
      idempotency_key: string;
      payload: Record<string, unknown>;
      occurred_at: string;
    }>(
      `WITH candidates AS (
         SELECT id
           FROM platform.bridge_outbox
          WHERE published_at IS NULL
            AND available_at <= now()
            AND (claimed_at IS NULL OR claimed_at < now() - interval '5 minutes')
          ORDER BY occurred_at, id
          FOR UPDATE SKIP LOCKED
          LIMIT $1
       )
       UPDATE platform.bridge_outbox b
          SET claimed_at = now(), claimed_by = $2, attempts = attempts + 1
         FROM candidates c
        WHERE b.id = c.id
       RETURNING b.id::text, b.event_type, b.aggregate_type, b.aggregate_id,
                 b.aggregate_version,
                 b.idempotency_key, b.payload, b.occurred_at::text`,
      [boundedLimit, claimId],
    );
    return result.rows;
  });

  let published = 0;
  let failed = 0;
  for (const row of rows) {
    try {
      const event = buildBridgeShadowEvent({
        eventType: row.event_type,
        aggregateType: row.aggregate_type,
        aggregateId: row.aggregate_id,
        aggregateVersion: row.aggregate_version,
        idempotencyKey: row.idempotency_key,
        payload: row.payload,
        occurredAt: row.occurred_at,
      });
      await withControlWriteTransaction(async (tx) => {
        await tx(
          `INSERT INTO erpx_control.outbox_events
             (id, event_type, event_version, aggregate_type, aggregate_id,
              aggregate_version, tenant_scope, actor, correlation_id,
              idempotency_key, payload, occurred_at)
           VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $1, $8, $9, $10)
           ON CONFLICT (idempotency_key) DO NOTHING`,
          [row.id, event.eventType, event.aggregateType, event.aggregateId,
            event.aggregateVersion, JSON.stringify({ source: 'folio' }), JSON.stringify({ kind: 'folio_bridge', claim_id: claimId }), event.idempotencyKey, JSON.stringify(event.payload), event.occurredAt],
        );
      });
      await query(
        `UPDATE platform.bridge_outbox
            SET published_at = now(), claimed_at = NULL, claimed_by = NULL, last_error = NULL
          WHERE id = $1 AND claimed_by = $2`,
        [row.id, claimId],
      );
      published += 1;
    } catch (error) {
      failed += 1;
      const message = error instanceof Error ? error.message.slice(0, 1000) : 'bridge publish failed';
      await query(
        `UPDATE platform.bridge_outbox
            SET available_at = now() + LEAST(interval '1 hour', (2 ^ LEAST(attempts, 10)) * interval '1 second'),
                claimed_at = NULL, claimed_by = NULL, last_error = $2
          WHERE id = $1 AND claimed_by = $3`,
        [row.id, message, claimId],
      );
    }
  }
  return { enabled, claimed: rows.length, published, failed };
}
