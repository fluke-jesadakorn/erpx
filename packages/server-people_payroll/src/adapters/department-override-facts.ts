import { PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL } from './pp-read-tail-r23-sources';
import "server-only";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
const tables={expense:'finance_expenses',leave:'leave_requests',overtime:'overtime_requests'} as const;
export async function departmentOverrideRecordVersion(db:SqlDatabase,companyId:string,kind:keyof typeof tables,id:string){return (await db.prepare(`SELECT version FROM ${tables[kind]} WHERE id=? AND company_id=?`).bind(id,companyId).first<{version:number}>())?.version??null;}
export async function departmentOverrideApproverName(db:SqlDatabase,companyId:string,id:string){return (await db.prepare(PP_RD23_HR_PEOPLE_APPROVER_LEGAL_NAME_SQL).bind(companyId,id).first<{legal_name:string}>())?.legal_name??null;}
export async function departmentOverrideOvertimeState(db:SqlDatabase,companyId:string,id:string){return (await db.prepare('SELECT state FROM overtime_requests WHERE id=? AND company_id=?').bind(id,companyId).first<{state:string}>())?.state??null;}
