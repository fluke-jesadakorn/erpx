import {collaborationNotificationSnapshotSources as crmSnapshotSources} from '@erpx/server-crm/public/collaboration-notification-snapshot-sources';
const crmSources=crmSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
/** Stage a notification in the caller's owning transaction. */
export function prepareCrmLeadNotification(db:SqlDatabase,id:string,key:string){
 return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'crm_lead',id,version,'CRM handoff changed: review the source and next action',updated_at ${crmSources.crm_notifications_crm_leads_from} WHERE id=? ON CONFLICT DO NOTHING`).bind(`crm-notice-${key}`,id);
}
export function prepareCrmActivityNotification(db:SqlDatabase,id:string,key:string){
 return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT ?,company_id,owner_id,'crm_activity',id,version,CASE WHEN status='open' THEN 'CRM activity assigned: review the due date and next action' ELSE 'CRM activity outcome recorded' END,updated_at ${crmSources.crm_notifications_crm_lead_activities_from} WHERE id=? ON CONFLICT DO NOTHING`).bind(`crm-activity-notice-${key}`,id);
}
