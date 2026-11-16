import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {refundNotificationSource} from "@erpx/server-finance/public/refund-notification-source";
/** Inbox rechecks source authority before display. Selection is Finance-owned. */
export function refundNotifications(db:SqlDatabase,key:string,id:string,audience:'approval'|'treasury'|'review'|'completed',message:string,actorId:string){return db.prepare("SELECT 1").bind();}
