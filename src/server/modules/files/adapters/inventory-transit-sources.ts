import type {SqlDatabase} from '../../runtime/public/runtime/types';
export function inventoryTransitSources(db:SqlDatabase){return {async attachmentTarget(value:string,companyId:string){return db.prepare('SELECT target_type FROM attachment_sets WHERE id=? AND company_id=?').bind(value,companyId).first<{target_type:string}>();}};}
