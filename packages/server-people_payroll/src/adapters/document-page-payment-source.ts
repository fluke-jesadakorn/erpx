import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
export function documentPageExpensePaymentSource(db:SqlDatabase,paymentId:string,companyId:string){return db.prepare('SELECT expense_id FROM expense_payment_drafts WHERE id=? AND company_id=?').bind(paymentId,companyId).first<{expense_id:string}>();}
