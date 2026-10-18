import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Fixed native lineage only, never a grant to read arbitrary Finance rows. */
export const payrollFinalPayCancellationJournalSources='SELECT id,company_id,source_type,source_id FROM finance_journals';
export function payrollFinalPayCancellationFinanceSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_journals IN SHARE MODE')];}
export async function payrollFinalPayCancellationMoneyImage(database:SqlDatabase,companyId:string,runId:string){
 const row=await database.prepare("SELECT count(*)>0 present,COALESCE(jsonb_agg(to_jsonb(j) ORDER BY j.id),'[]'::jsonb)::text image FROM finance_journals j WHERE j.company_id=? AND j.source_type='payroll' AND j.source_id=?").bind(companyId,runId).first<{present:boolean;image:string}>();
 if(!row)throw Error('PAYROLL_FINAL_PAY_CANCELLATION_MONEY_SOURCE_REQUIRED');return {hasMoneyEffects:row.present,image:row.image};
}
