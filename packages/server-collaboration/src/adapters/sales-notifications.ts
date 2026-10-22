import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {followupNotificationSource} from '@erpx/server-sales/public/followup-notification-source';
export function prepareFollowupNotification(db:SqlDatabase,id:string,key:string){return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) ${followupNotificationSource()} ON CONFLICT DO NOTHING`).bind(`followup-notice-${key}`,id);}
