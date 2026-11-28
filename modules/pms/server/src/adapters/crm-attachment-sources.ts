import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import { pmsRecordPropertyJoinSql } from './pms-read-tail-r45-sql';
/** CRM's actual guest/reservation relationship remains owned by Hotel. */
export function crmAttachmentHotelSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pms_records,pms_properties IN SHARE ROW EXCLUSIVE MODE');}
export function crmAttachmentHotelSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare(pmsRecordPropertyJoinSql('crmAttachmentImage') + " WHERE record.id=? AND property.company_id=? AND record.kind IN ('guest','reservation')").bind(id,companyId).first<string>('image');}
