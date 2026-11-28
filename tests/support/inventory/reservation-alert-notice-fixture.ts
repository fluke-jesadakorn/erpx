/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover inventory reservation alert notices.
 *
 * Production notice generation now belongs to the native
 * `inventory_reservation_alerts` worker (platform/composition), which seeds the same
 * deduplicated staff_notifications from Sales and Manufacturing reservations.
 * This copy reproduces the INSERT...SELECT for the integration suites without
 * the TS writer in packages/server-*.
 *
 * Tracked for removal by `migrate.inventory.reservation-alert-worker`: delete
 * once the integration harness drives the native worker result directly.
 */
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from '@erpx/inventory-server/public/commercial-owner-projection-sources';
import { scheduledFeaturePredicate } from '@erpx/pms-server/public/feature-availability';
import { inventoryReservationAlertSelect as manufacturingSelect } from '@erpx/manufacturing-server/public/inventory-reservation-alert-sources';
import { reservationAlertMessage, type ReservationAlertPhase } from '@erpx/inventory-server/domain/reservation-alerts';

// Native-owned Commerce/Sales reservation projection (retired TS source). The
// native projection lives in the Inventory owner's Rust; no TS bridge exists,
// so the fixture reproduces the fixed statement locally.
function salesSelect(phase: 'expiring' | 'expired', timePredicate: string) {
  return `SELECT 'inventory-reservation-${phase}-'||md5('sales:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'commerce_order',o.id,o.version,?,?
    FROM (${inventoryProjectionSources.inventory_order_reservations}) r JOIN commerce_order_lines l ON l.id=r.order_line_id JOIN commerce_orders o ON o.id=l.order_id
    CROSS JOIN LATERAL(SELECT o.created_by user_id UNION SELECT r.created_by) recipient
    WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND ${timePredicate} AND o.state IN ('confirmed','partially_fulfilled')
      AND ${scheduledFeaturePredicate("o.company_id", "inventory")}
    ON CONFLICT DO NOTHING`;
}

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
