import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import { employeeRequestColumnsSql } from './read-tail-employee-request-columns';
export function commerceEntrypointRecordFacts(db:SqlDatabase){return {
 linkedRequest:(companyId:string,orderId:string)=>db.prepare(employeeRequestColumnsSql('id,title','company_id=? AND purchase_order_id=?')).bind(companyId,orderId).first<{id:string;title:string}>()
};}
