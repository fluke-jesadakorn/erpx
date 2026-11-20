/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover inventory reservation alert notices.
 *
 * Production notice generation now belongs to the native
 * `inventory_reservation_alerts` worker (src/composition), which seeds the same
 * deduplicated staff_notifications from Sales and Manufacturing reservations.
 * This copy reproduces the INSERT...SELECT for the integration suites without
 * the TS writer in packages/server-*.
 *
 * Tracked for removal by `migrate.inventory.reservation-alert-worker`: delete
 * once the integration harness drives the native worker result directly.
 */
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import { inventoryReservationAlertSelect as salesSelect } from '@erpx/server-commerce/public/inventory-reservation-alert-sources';
import { inventoryReservationAlertSelect as manufacturingSelect } from '@erpx/server-manufacturing/public/inventory-reservation-alert-sources';
import { reservationAlertMessage, type ReservationAlertPhase } from '@erpx/server-inventory/domain/reservation-alerts';

async function raise(
  db: SqlDatabase,
  phase: ReservationAlertPhase,
  now: number,
  horizon: number,
): Promise<number> {
  const timePredicate = phase === 'expiring' ? 'r.expires_at>? AND r.expires_at<=?' : 'r.expires_at<=?';
  const timeArgs = phase === 'expiring' ? [now, horizon] : [now];
  const message = reservationAlertMessage(phase);
  const head = 'INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) ';
  const statements = [
    db.prepare(`${head}${salesSelect(phase, timePredicate)}`).bind(message, now, ...timeArgs),
    db.prepare(`${head}${manufacturingSelect(phase, timePredicate)}`).bind(message, now, ...timeArgs),
  ];
  const results = await db.batch(statements);
  return results.reduce((count, result) => count + Number(result.meta.changes ?? 0), 0);
}

export async function seedInventoryReservationAlertNotices(
  db: SqlDatabase,
  now: number,
  warningWindowMs = 86_400_000,
): Promise<{ expiring: number; expired: number }> {
  const expiring = await raise(db, 'expiring', now, now + warningWindowMs);
  const expired = await raise(db, 'expired', now, now);
  return { expiring, expired };
}
