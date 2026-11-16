import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** CRM's actual guest/reservation relationship remains owned by Hotel. */
export function crmAttachmentHotelSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pms_records,pms_properties IN SHARE ROW EXCLUSIVE MODE');}
export function crmAttachmentHotelSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare("SELECT 1").bind().first<string>('image');}
