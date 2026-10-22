import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {inventoryReservationAlertSelect as owner0} from '@erpx/server-commerce/public/inventory-reservation-alert-sources';
import {inventoryReservationAlertSelect as owner1} from '@erpx/server-field_service/public/inventory-reservation-alert-sources';
import {inventoryReservationAlertSelect as owner2} from '@erpx/server-manufacturing/public/inventory-reservation-alert-sources';
export async function raiseInventoryReservationAlerts(db: SqlDatabase, phase: "expiring" | "expired", now: number, horizon: number,message:string) {
  const timePredicate = phase === "expiring" ? "r.expires_at>? AND r.expires_at<=?" : "r.expires_at<=?";
  const timeArgs = phase === "expiring" ? [now, horizon] : [now];
  const statements = [
    db.prepare('INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) '+owner0(phase,timePredicate)).bind(message, now, ...timeArgs),
    db.prepare('INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) '+owner1(phase,timePredicate)).bind(message, now, ...timeArgs),
    db.prepare('INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) '+owner2(phase,timePredicate)).bind(message, now, ...timeArgs),
  ];
  const results = await db.batch(statements);
  return results.reduce((count, result) => count + Number(result.meta.changes ?? 0), 0);
}
