import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import { incidentUpdatedNotificationSource,ticketReopenedNotificationSource,ticketCreatedNotificationSource,ticketUpdatedNotificationSource,ticketReviewersNotificationSource,slaBreachedNotificationSource,sessionReviewersNotificationSource,sessionUpdatedNotificationSource,policyReviewersNotificationSource,policyUpdatedNotificationSource,ticketClockAttachedNotificationSource } from "@erpx/server-support/public/support-notification-sources";
/** Notification effects use the caller's existing transaction and closed Support recipient projections. */
export function supportNotificationEffects(db:SqlDatabase){return {
incidentUpdated:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${incidentUpdatedNotificationSource()} ON CONFLICT DO NOTHING`),
ticketReopened:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${ticketReopenedNotificationSource()} ON CONFLICT DO NOTHING`),
ticketCreated:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${ticketCreatedNotificationSource()}`),
ticketUpdated:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${ticketUpdatedNotificationSource()}`),
ticketReviewers:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${ticketReviewersNotificationSource()} ON CONFLICT DO NOTHING`),
slaBreached:(purchased = "TRUE")=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${slaBreachedNotificationSource(purchased)} ON CONFLICT DO NOTHING`),
sessionReviewers:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${sessionReviewersNotificationSource()} ON CONFLICT DO NOTHING`),
sessionUpdated:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${sessionUpdatedNotificationSource()}`),
policyReviewers:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${policyReviewersNotificationSource()} ON CONFLICT DO NOTHING`),
policyUpdated:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${policyUpdatedNotificationSource()}`),
ticketClockAttached:()=>db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)${ticketClockAttachedNotificationSource()}`),
};}
