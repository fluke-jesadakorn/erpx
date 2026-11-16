import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function inventoryTransitSources(db:SqlDatabase){return {async attachmentTarget(value:string,companyId:string){return db.prepare("SELECT 1").bind().first<{target_type:string}>();}};}
