import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export type RuntimePendingEvent = { id: string; event_type: string; aggregate_id: string; aggregate_version: number; payload_json: string; attempts: number; lease_token: string };
/** Finite job lease effects on the worker's supplied connection. */
export function runtimeJobEffects(db: SqlDatabase, excludedSourceSql?: string) {
  return {
    claim(token: string, until: number, readyAt: number, expiredAt: number) {
      return db.prepare(`UPDATE outbox_events SET lease_token=?,claim_until=?,attempts=attempts+1
    WHERE id=(SELECT e.id FROM outbox_events e WHERE e.processed_at IS NULL AND e.dead_letter_at IS NULL
      AND e.next_attempt_at<=? AND (e.claim_until IS NULL OR e.claim_until<?)
      ${excludedSourceSql ? `AND NOT (${excludedSourceSql})` : ''}
      ORDER BY e.created_at,e.id FOR UPDATE OF e SKIP LOCKED LIMIT 1)
    RETURNING id,event_type,aggregate_id,aggregate_version,payload_json,attempts,lease_token`).bind(token, until, readyAt, expiredAt).first<RuntimePendingEvent>();
    },
    renew(until: number, id: string, token: string) { return db.prepare('UPDATE outbox_events SET claim_until=? WHERE id=? AND lease_token=? AND processed_at IS NULL').bind(until, id, token).run(); },
    complete(processedAt: number, publishedAt: number, id: string, token: string) { return db.prepare('UPDATE outbox_events SET processed_at=?,published_at=?,claim_until=NULL,lease_token=NULL,last_error=NULL WHERE id=? AND lease_token=?').bind(processedAt, publishedAt, id, token).run(); },
    failed(retryAt: number, code: string, deadAt: number | null, id: string, token: string) { return db.prepare('UPDATE outbox_events SET claim_until=NULL,lease_token=NULL,next_attempt_at=?,last_error=?,dead_letter_at=? WHERE id=? AND lease_token=?').bind(retryAt, code, deadAt, id, token).run(); },
  };
}
