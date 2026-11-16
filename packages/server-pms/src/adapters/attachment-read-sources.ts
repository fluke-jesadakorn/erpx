import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export type PmsAttachmentReadTarget='pms_property'|'pms_room_type'|'pms_guest'|'pms_reservation'|'pms_stay'|'pms_task'|'pms_lost_property';
export function isPmsAttachmentReadTarget(target:string):target is PmsAttachmentReadTarget{return ['pms_property','pms_room_type','pms_guest','pms_reservation','pms_stay','pms_task','pms_lost_property'].includes(target);}
/** Query-free leases cover the original property/record scope before the
 * private byte reader takes its first data snapshot. No PMS authority is added. */
export function pmsAttachmentReadSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pms_properties,pms_records IN SHARE ROW EXCLUSIVE MODE');}
export function pmsAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:PmsAttachmentReadTarget,id:string){
 if(target==='pms_property')return database.prepare("SELECT 1").bind().first<string>('image');
 return database.prepare("SELECT 1").bind().first<string>('image');
}
