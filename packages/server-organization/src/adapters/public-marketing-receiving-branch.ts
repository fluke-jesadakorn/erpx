import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function publicMarketingReceivingBranch(db:SqlDatabase,companyId:string){return db.prepare("SELECT 1").bind().first<{id:string}>();}
