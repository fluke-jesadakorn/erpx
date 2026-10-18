import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** CRM's actual guest/reservation relationship remains owned by Hotel. */
export function crmAttachmentHotelSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pms_records,pms_properties IN SHARE ROW EXCLUSIVE MODE');}
export function crmAttachmentHotelSourceImage(database:SqlDatabase,companyId:string,id:string){return database.prepare("SELECT jsonb_build_object('record',to_jsonb(record),'property',to_jsonb(property))::text image FROM pms_records record JOIN pms_properties property ON property.id=record.property_id WHERE record.id=? AND property.company_id=? AND record.kind IN ('guest','reservation')").bind(id,companyId).first<string>('image');}
