import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function commerceEntrypointRecordFacts(db:SqlDatabase){return {
 linkedRequest:(companyId:string,orderId:string)=>db.prepare('SELECT id,title FROM employee_requests WHERE company_id=? AND purchase_order_id=?').bind(companyId,orderId).first<{id:string;title:string}>()
};}
