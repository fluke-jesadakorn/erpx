import "server-only";

import type { SqlDatabase } from "./runtime/types";

type AlertResult = { expiring: number; expired: number };

async function insertAlerts(db: SqlDatabase, phase: "expiring" | "expired", now: number, horizon: number) {
  const timePredicate = phase === "expiring" ? "r.expires_at>? AND r.expires_at<=?" : "r.expires_at<=?";
  const timeArgs = phase === "expiring" ? [now, horizon] : [now];
  const message = phase === "expiring"
    ? "Stock reservation expires within 24 hours; use, release, or re-reserve it from the source work"
    : "Stock reservation expired and no longer protects availability; re-reserve or correct the source work";

  const statements = [
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT 'inventory-reservation-${phase}-'||md5('sales:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'commerce_order',o.id,o.version,?,?
      FROM inventory_order_reservations r JOIN commerce_order_lines l ON l.id=r.order_line_id JOIN commerce_orders o ON o.id=l.order_id
      CROSS JOIN LATERAL(SELECT o.created_by user_id UNION SELECT r.created_by) recipient
      WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND ${timePredicate} AND o.state IN ('confirmed','partially_fulfilled')
      ON CONFLICT DO NOTHING`).bind(message, now, ...timeArgs),
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT 'inventory-reservation-${phase}-'||md5('field:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'field_service_order',o.id,o.version,?,?
      FROM field_service_part_reservations r JOIN field_service_orders o ON o.id=r.order_id
      CROSS JOIN LATERAL(SELECT o.technician_id user_id UNION SELECT o.created_by UNION SELECT r.created_by) recipient
      WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND ${timePredicate} AND o.status IN ('open','scheduled','dispatched','in_visit','exception')
      ON CONFLICT DO NOTHING`).bind(message, now, ...timeArgs),
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      SELECT 'inventory-reservation-${phase}-'||md5('manufacturing:'||r.id||':'||r.expires_at::text||':'||recipient.user_id),o.company_id,recipient.user_id,'manufacturing_order',o.id,o.version,?,?
      FROM manufacturing_material_reservations r JOIN manufacturing_orders o ON o.id=r.order_id
      CROSS JOIN LATERAL(SELECT o.created_by user_id UNION SELECT r.created_by) recipient
      WHERE r.quantity_milli>r.consumed_milli+r.released_milli AND ${timePredicate} AND o.status='reserved'
      ON CONFLICT DO NOTHING`).bind(message, now, ...timeArgs),
  ];
  const results = await db.batch(statements);
  return results.reduce((count, result) => count + Number(result.meta.changes ?? 0), 0);
}

/**
 * Raises durable, deduplicated owner alerts before a reservation expires and
 * again when its availability protection has ended. The source workflow stays
 * authoritative for re-reserving, consuming, releasing, or cancelling stock.
 */
export async function escalateInventoryReservations(db: SqlDatabase, now = Date.now(), warningWindowMs = 86_400_000): Promise<AlertResult> {
  // Current reservation alerts belong to Sales, Field Service and Manufacturing, all future releases.
  if (process.env.NODE_ENV === "production") return { expiring: 0, expired: 0 };
  const expiring = await insertAlerts(db, "expiring", now, now + warningWindowMs);
  const expired = await insertAlerts(db, "expired", now, now);
  return { expiring, expired };
}
