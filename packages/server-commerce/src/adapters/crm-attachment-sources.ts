import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeCommerceReadFirst,RD_CRM_ORDER_SOURCE_IMAGE} from './native-commerce-read';
/** Fixed original quote/sale identity, never a caller-supplied source label. */
export function crmAttachmentOrderSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE commerce_orders IN SHARE ROW EXCLUSIVE MODE');}
export function crmAttachmentOrderSourceImage(database:SqlDatabase,companyId:string,id:string){return nativeCommerceReadFirst<string>(RD_CRM_ORDER_SOURCE_IMAGE,[id,companyId],database);}
