import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativeSupportReadAll, SUPPORT_RD_TICKET_SOURCE_IMAGE} from './native-support-read';
/** Public customer sharing uses its own verified Portal binding. This port
 * witnesses the actual existing case; it grants no Helpdesk collection read. */
export function portalPreviewSupportSourceReadLocks(database:SqlDatabase){return database.prepare('LOCK TABLE support_tickets IN SHARE ROW EXCLUSIVE MODE');}
/** Owner-native read: the statement is planned by the Support native read
 * port (`RD_TICKET_SOURCE_IMAGE`) and executed on the caller's open unit.
 * This module carries no SQL for this read. */
export function portalPreviewSupportSourceImage(database:SqlDatabase,companyId:string,id:string){return nativeSupportReadAll<{image:string}>(SUPPORT_RD_TICKET_SOURCE_IMAGE,[id,companyId],database).then(result => result.results[0]?.image ?? null);}
