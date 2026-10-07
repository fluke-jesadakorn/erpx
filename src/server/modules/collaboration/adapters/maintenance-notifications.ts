import {collaborationNotificationSnapshotSources as maintenanceSnapshotSources} from '../../maintenance/public/collaboration-notification-snapshot-sources';
const maintenanceSources=maintenanceSnapshotSources();
import type { SqlDatabase } from "../../runtime/public/runtime/types";

/** Append to the caller's existing batch so notification and receipt roll back together. */
export function prepareMaintenanceNotification(db: SqlDatabase, key: string, id: string, created: boolean) {
  return created
    ? db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'maintenance_work',id,version,'New asset maintenance work: review schedule and condition',updated_at ${maintenanceSources.maintenance_notifications_maintenance_work_orders_from} WHERE id=?`).bind(`maintenance-notice-${key}`, id)
    : db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'maintenance_work',id,version,'Asset maintenance work updated: review current next action',updated_at ${maintenanceSources.maintenance_notifications_maintenance_work_orders_from} WHERE id=?`).bind(`maintenance-notice-${key}`, id);
}
export function preparePreventivePlanNotification(db: SqlDatabase, key: string, id: string, message: string, now: number) {
  return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'maintenance_plan',id,version,?,? ${maintenanceSources.maintenance_notifications_maintenance_preventive_plans_from} WHERE id=?`).bind(`preventive-notice-${key}`, message, now, id);
}
