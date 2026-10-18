import {expenseCompanyHistoryScope} from "./expense-record-purchases";
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import type {Row} from '../../workflow/public/flow-routing-contract';
const tables={'expense':'finance_expenses','leave':'leave_requests','payroll':'hr_payroll_runs'};
export async function workflowDocument(db:SqlDatabase,kind:'expense' | 'leave' | 'payroll',companyId:string,id:string){const expense=kind==='expense'?await expenseCompanyHistoryScope(db,companyId):'TRUE';return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id=? AND ${expense}`).bind(companyId,id).first<Row>();}
export async function workflowDocumentPage(db:SqlDatabase,kind:'expense' | 'leave' | 'payroll',companyId:string,after:string){const expense=kind==='expense'?await expenseCompanyHistoryScope(db,companyId):'TRUE';return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? AND id>? AND ${expense} ORDER BY id LIMIT 100`).bind(companyId,after).all<Row>();}
export function workflowReferenceRows(db:SqlDatabase,kind:'payroll',companyId:string){const updated='updated_at';return db.prepare(`SELECT * FROM ${tables[kind]} WHERE company_id=? ORDER BY ${updated} DESC`).bind(companyId).all<Row>();}
