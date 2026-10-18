import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export type PosAttachmentReadTarget='pos_sale'|'pos_refund'|'pos_shift';
export function isPosAttachmentReadTarget(target:string):target is PosAttachmentReadTarget{return ['pos_sale','pos_refund','pos_shift'].includes(target);}
export function posAttachmentReadSourceLocks(database:SqlDatabase){return database.prepare('LOCK TABLE pos_sales,pos_refunds,pos_shifts IN SHARE ROW EXCLUSIVE MODE');}
/** Only the closed original owner kind selects a relation. The full Files
 * policy retains its original scope, read permission and purchased purpose. */
export function posAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:PosAttachmentReadTarget,id:string){
 const table={pos_sale:'pos_sales',pos_refund:'pos_refunds',pos_shift:'pos_shifts'}[target];
 return database.prepare(`SELECT to_jsonb(source)::text image FROM ${table} source WHERE source.id=? AND source.company_id=?`).bind(id,companyId).first<string>('image');
}
