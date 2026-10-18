import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
export const expensePostingSetupNativeSources='SELECT id,company_id,invoice_id,journal_id,reversal_journal_id FROM finance_expenses';
/** Initial Finance mapping cannot adopt an original posted or reversed claim.
 * Draft claim intent remains unchanged, not relabelled as executed money. */
export function expensePostingSetupSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_expenses,expense_payment_drafts IN SHARE MODE')];}
export async function expensePostingSetupSources(database:SqlDatabase,companyId:string){
 const row=await database.prepare(`WITH scope AS(SELECT ?::text company_id),claims AS MATERIALIZED(SELECT e.* FROM finance_expenses e,scope WHERE e.company_id=scope.company_id),payments AS MATERIALIZED(SELECT p.* FROM expense_payment_drafts p,scope WHERE p.company_id=scope.company_id)
 SELECT EXISTS(SELECT 1 FROM claims WHERE invoice_id IS NOT NULL OR journal_id IS NOT NULL OR reversal_journal_id IS NOT NULL OR status IN('posted','paid')) OR EXISTS(SELECT 1 FROM payments WHERE payment_id IS NOT NULL OR status='confirmed') economics,
 jsonb_build_array((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM claims e),(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM payments p))::text image`).bind(companyId).first<{economics:boolean;image:string}>();
 if(!row)throw Error('Original Expense posting source unavailable.');return row;
}
