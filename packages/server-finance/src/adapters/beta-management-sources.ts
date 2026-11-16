import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function betaManagementFinanceSources(db:SqlDatabase){return {
 aiUsageSource:'SELECT company_id,month,input_tokens,output_tokens FROM finance_ai_usage',
 async requests(companyId:string,month:string){return db.prepare('SELECT count(*)::int count FROM finance_ai_usage WHERE company_id=? AND month=?').bind(companyId,month).first<{count:number}>();},
};}
