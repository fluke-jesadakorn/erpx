import "server-only";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
const tables={expense:'finance_expenses',leave:'leave_requests',overtime:'overtime_requests'} as const;
export async function departmentOverrideRecordVersion(db:SqlDatabase,companyId:string,kind:keyof typeof tables,id:string){return (await db.prepare(`SELECT version FROM ${tables[kind]} WHERE id=? AND company_id=?`).bind(id,companyId).first<{version:number}>())?.version??null;}
export async function departmentOverrideApproverName(db:SqlDatabase,companyId:string,id:string){return (await db.prepare('SELECT legal_name FROM hr_people WHERE company_id=? AND iam_same_person(user_id,?,company_id) AND merged_into_id IS NULL LIMIT 1').bind(companyId,id).first<{legal_name:string}>())?.legal_name??null;}
export async function departmentOverrideOvertimeState(db:SqlDatabase,companyId:string,id:string){return (await db.prepare('SELECT state FROM overtime_requests WHERE id=? AND company_id=?').bind(id,companyId).first<{state:string}>())?.state??null;}
