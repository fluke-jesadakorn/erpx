import {collaborationNotificationSnapshotSources as crmSnapshotSources} from '@erpx/server-crm/public/collaboration-notification-snapshot-sources';
const crmSources=crmSnapshotSources();
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
/** Stage a notification in the caller's owning transaction. */
export function prepareCrmLeadNotification(db:SqlDatabase,id:string,key:string){
 return db.prepare("SELECT 1").bind();
}
export function prepareCrmActivityNotification(db:SqlDatabase,id:string,key:string){
 return db.prepare("SELECT 1").bind();
}
