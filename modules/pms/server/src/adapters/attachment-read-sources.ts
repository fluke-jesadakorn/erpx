import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {nativePmsReadFirst,PMS_RD_ATTACHMENT_PROPERTY_IMAGE,PMS_RD_ATTACHMENT_RECORD_IMAGE} from './native-pms-read';
export type PmsAttachmentReadTarget='pms_property'|'pms_room_type'|'pms_guest'|'pms_reservation'|'pms_stay'|'pms_task'|'pms_lost_property';
export function isPmsAttachmentReadTarget(target:string):target is PmsAttachmentReadTarget{return ['pms_property','pms_room_type','pms_guest','pms_reservation','pms_stay','pms_task','pms_lost_property'].includes(target);}
/** Query-free leases cover the original property/record scope before the
 * private byte reader takes its first data snapshot. No PMS authority is added. */
export function pmsAttachmentReadSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pms_properties,pms_records IN SHARE ROW EXCLUSIVE MODE');}
/** The image SQL is planned in Rust and run on the caller's already-open read
 * unit, so the caller's source lease is observed; no TypeScript SQL remains. */
export function pmsAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:PmsAttachmentReadTarget,id:string){
 const kind=target==='pms_property'?PMS_RD_ATTACHMENT_PROPERTY_IMAGE:PMS_RD_ATTACHMENT_RECORD_IMAGE;
 return nativePmsReadFirst<{image:string}>(kind,[id,companyId],database).then(row=>row?.image??null);
}
