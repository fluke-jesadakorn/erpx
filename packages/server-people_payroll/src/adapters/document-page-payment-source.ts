import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function documentPageExpensePaymentSource(db:SqlDatabase,paymentId:string,companyId:string){return db.prepare("SELECT 1").bind().first<{expense_id:string}>();}
