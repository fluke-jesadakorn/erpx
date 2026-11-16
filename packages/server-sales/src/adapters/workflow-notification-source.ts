import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function workflowCustomerNotificationId(db:SqlDatabase,fulfillmentId:string){return db.prepare('SELECT id FROM commerce_customer_notifications WHERE fulfillment_id=?').bind(fulfillmentId).first<string>('id');}
