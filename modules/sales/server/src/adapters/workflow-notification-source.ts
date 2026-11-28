import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {pointRead} from './point-read';
export function workflowCustomerNotificationId(db:SqlDatabase,fulfillmentId:string){return db.prepare(pointRead('id','commerce_customer_notifications','fulfillment_id')).bind(fulfillmentId).first<string>('id');}
