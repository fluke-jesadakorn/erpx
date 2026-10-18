import 'server-only';
import {createHash} from 'node:crypto';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {ApiError} from '../../runtime/public/http';
export type PayrollOtherRun={id:string;company_id:string;version:number;currency:string;period_end:string;status:string;created_by:string;approved_by:string;posted_by:string};
export type PayrollOtherSource={run:PayrollOtherRun;journalId:string;postingCommandId:string;image:string;identity:string};
export const payrollOtherRemittanceTables={runs:'SELECT * FROM hr_payroll_runs',postings:'SELECT * FROM hr_payroll_postings',remittances:'SELECT * FROM hr_payroll_statutory_remittances'} as const;
export function payrollOtherSourceLeases(database:SqlDatabase,write:boolean){return [database.prepare(`LOCK TABLE hr_payroll_runs,hr_payroll_postings,hr_payroll_items,hr_payroll_statutory_remittances IN ${write?'SHARE ROW EXCLUSIVE':'SHARE'} MODE`)];}
export async function payrollOtherSource(database:SqlDatabase,companyId:string,runId:string):Promise<PayrollOtherSource>{
 const source=await database.prepare(`SELECT to_jsonb(r)::text run,p.journal_id,to_jsonb(p)::text posting,to_jsonb(c)::text receipt FROM hr_payroll_runs r JOIN hr_payroll_postings p ON p.run_id=r.id JOIN command_receipts c ON c.operation='people.payroll.post' AND c.record_id=r.id AND c.actor_id=r.posted_by AND c.authority_json::jsonb->>'companyId'=r.company_id AND c.result_json::jsonb->>'id'=r.id AND c.result_json::jsonb->>'status'='posted' AND c.result_json::jsonb->>'journalId'=p.journal_id WHERE r.id=? AND r.company_id=? AND r.status='posted'`).bind(runId,companyId).all<{run:string;journal_id:string;posting:string;receipt:string}>();
 if(source.results.length!==1)throw new ApiError(409,'PAYROLL_POSTING_MISSING','Use the original independently approved and posted Payroll accrual.');
 const row=source.results[0]!,run=JSON.parse(row.run) as PayrollOtherRun;
 const items=(await database.prepare('SELECT to_jsonb(i)::text image FROM hr_payroll_items i WHERE i.run_id=? ORDER BY i.id').bind(runId).all<{image:string}>()).results;
 return {run,journalId:row.journal_id,postingCommandId:String(JSON.parse(row.receipt).command_id),image:JSON.stringify([row,items]),identity:createHash('sha256').update(JSON.stringify([companyId,runId,row.journal_id,row.posting,row.receipt])).digest('hex')};
}
export async function payrollOtherRemittances(database:SqlDatabase,companyId:string,runId:string){return (await database.prepare('SELECT to_jsonb(r)::text image FROM hr_payroll_statutory_remittances r WHERE r.company_id=? AND r.run_id=? ORDER BY r.id').bind(companyId,runId).all<{image:string}>()).results;}
export async function payrollOtherSchemaAvailable(database:SqlDatabase){return !!await database.prepare("SELECT 1 FROM erpx_migration_receipts() WHERE name='1066_payroll_other_liability_remittances.sql'").first();}
