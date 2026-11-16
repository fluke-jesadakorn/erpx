import {collaborationNotificationSnapshotSources as commerceSnapshotSources} from '@erpx/server-commerce/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '@erpx/server-identity_access/public/collaboration-notification-snapshot-sources';
const commerceSources=commerceSnapshotSources();
const identityaccessSources=identityaccessSnapshotSources();
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function stageSupplierPromiseNotifications(db:SqlDatabase,key:string,id:string){return db.prepare("SELECT 1").bind();}
