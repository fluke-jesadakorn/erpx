import { lastAuditOwnerProjectionSources as marketingLastAuditProjectionSources } from "../../marketing/public/last-audit-owner-projection-sources";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
export type MarketingExpenseSource={source_line_id:string;journal_number:string;entry_date:string;currency:string;account_code:string;account_name:string;description:string;debit_minor:string};
export async function marketingExpenseSources(write:SqlDatabase,companyId:string){ return (await write.prepare(`SELECT l.id source_line_id,j.journal_number,j.entry_date,j.currency,
   acc.code account_code,acc.name account_name,l.description,l.debit_minor::text debit_minor
   FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
   JOIN finance_accounts acc ON acc.id=l.account_id AND acc.company_id=j.company_id
   WHERE j.company_id=? AND j.status='posted' AND acc.category='expense' AND l.debit_minor>l.credit_minor
     AND NOT EXISTS(SELECT 1 FROM (${marketingLastAuditProjectionSources.marketing_campaign_costs}) c WHERE c.journal_line_id=l.id)
   ORDER BY j.entry_date DESC,j.journal_number DESC,l.ordinal LIMIT 200`)
   .bind(companyId).all<MarketingExpenseSource>()).results;}
export function prepareMarketingExpenseGuard(write:SqlDatabase,key:string,companyId:string,lineId:string){return assertion(write,key,'posted-expense',"EXISTS(SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts acc ON acc.id=l.account_id WHERE l.id=? AND j.company_id=? AND j.status='posted' AND acc.company_id=j.company_id AND acc.category='expense' AND l.debit_minor>l.credit_minor)",[lineId,companyId]);}
