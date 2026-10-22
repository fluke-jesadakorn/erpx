import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function requestQuotation(db:SqlDatabase,requestId:string,companyId:string){return db.prepare("SELECT id FROM procurement_rfqs WHERE request_id=? AND company_id=? AND status<>'cancelled'").bind(requestId,companyId).first<{id:string}>();}
