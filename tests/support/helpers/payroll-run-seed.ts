import {expenseDb} from "./expense-fixture";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";

/** Test-only payroll-run seed. The production create command is native; this
 * preserves the original run row shape for the retained calculation/post oracles. */
export async function createPayrollRun(actor:WorkContext,input:{payGroupId:string;countryPackId?:string|null;periodStart:string;periodEnd:string;payDate:string;currency:string;mode?:"preview"|"production"|"final_pay"},key:string){
 const id=`payrun-${key}`,now=Date.now(),mode=input.mode??"preview";
 await expenseDb.prepare("INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,country_pack_id,period_start,period_end,pay_date,currency,mode,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,'calculating',?,?,?)").bind(id,actor.companyId,input.payGroupId,input.countryPackId??null,input.periodStart,input.periodEnd,input.payDate,input.currency,mode,actor.userId,now,now).run();
 return {id,status:"calculating",mode,version:1};
}
