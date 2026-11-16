import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function requestQuotation(db:SqlDatabase,requestId:string,companyId:string){return db.prepare("SELECT 1").bind().first<{id:string}>();}
