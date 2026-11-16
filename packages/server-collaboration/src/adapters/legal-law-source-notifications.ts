import {collaborationNotificationSnapshotSources as legalSnapshotSources} from '@erpx/server-legal/public/collaboration-notification-snapshot-sources';
const legalSources=legalSnapshotSources();
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function legalSourceNotification(db:SqlDatabase,facts:{identifier:string;now:number;ruleId:string;unchanged:boolean}){return db.prepare("SELECT 1").bind();}
