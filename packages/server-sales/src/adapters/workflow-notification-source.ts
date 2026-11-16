import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function workflowCustomerNotificationId(db:SqlDatabase,fulfillmentId:string){return db.prepare("SELECT 1").bind().first<string>('id');}
