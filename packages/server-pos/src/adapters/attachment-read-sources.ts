import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {nativePosStatement} from './native-owner-write';
export type PosAttachmentReadTarget='pos_sale'|'pos_refund'|'pos_shift';
export function isPosAttachmentReadTarget(target:string):target is PosAttachmentReadTarget{return ['pos_sale','pos_refund','pos_shift'].includes(target);}
export function posAttachmentReadSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pos_sales,pos_refunds,pos_shifts IN SHARE ROW EXCLUSIVE MODE');}
const SOURCE_KIND:Record<PosAttachmentReadTarget,string>={pos_sale:'pos_attachment_source_pos_sale',pos_refund:'pos_attachment_source_pos_refund',pos_shift:'pos_attachment_source_pos_shift'};
/** Only the closed original owner kind selects a relation. The authoritative
 * SQL now lives in the native POS owner registry (`attachment_read_sources.rs`)
 * and is requested by typed kind only; the table is never supplied by a caller.
 * The full Files policy retains its original scope, read permission and
 * purchased purpose. */
export async function posAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:PosAttachmentReadTarget,id:string){
 const statement=await nativePosStatement(database,companyId,SOURCE_KIND[target],[id,companyId]);
 return statement.first<string>('image');
}
