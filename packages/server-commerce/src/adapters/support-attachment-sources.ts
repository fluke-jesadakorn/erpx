import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeCommerceReadFirst,RD_SUPPORT_ORDER_SOURCE_IMAGE} from './native-commerce-read';
/** The existing Helpdesk ticket's actual order FK, including its native
 * version and customer identity. This does not grant Commerce collection read. */
export function supportOrderAttachmentSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE commerce_orders IN SHARE ROW EXCLUSIVE MODE');}
export function supportOrderAttachmentSourceImage(database:SqlDatabase,companyId:string,id:string){return nativeCommerceReadFirst<string>(RD_SUPPORT_ORDER_SOURCE_IMAGE,[id,companyId],database);}
