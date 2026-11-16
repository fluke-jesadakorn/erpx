import {collaborationNotificationSnapshotSources as procurementSnapshotSources} from '@erpx/server-procurement/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '@erpx/server-identity_access/public/collaboration-notification-snapshot-sources';
const procurementSources=procurementSnapshotSources();
const identityaccessSources=identityaccessSnapshotSources();
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function stageSupplierBatchNotifications(db:SqlDatabase,key:string,id:string,message:string,actor:string) {
 return db.prepare("SELECT 1").bind();
}
