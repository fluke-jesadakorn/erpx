import { financeSnapshotSources as posSnapshot } from "@erpx/server-pos/public/finance-snapshot-sources";
import "server-only";
import {one,all,run} from '@erpx/server-runtime/public/runtime/sql';
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function PosDayFinanceFacts(db:SqlDatabase){return {
readFinanceParties1:(facts:{party_id:string | number | true;company_id:string | number | boolean | null;})=>one(db,'SELECT id,name,phone,notification_email,address_json,tax_registration FROM finance_parties WHERE id=? AND company_id=?',facts.party_id,facts.company_id),
readFinanceParties2:(facts:{companyId:string;q:string;pattern:string;pattern2:string;pattern3:string;})=>all(db,`SELECT id,name,phone,notification_email,address_json,version FROM finance_parties
  WHERE company_id=? AND kind='customer' AND status='active'
  AND (?='' OR name ILIKE ? OR phone ILIKE ? OR notification_email ILIKE ?)
  ORDER BY name,id LIMIT 100`,facts.companyId,facts.q,facts.pattern,facts.pattern2,facts.pattern3),
readFinanceParties3:(facts:{companyId:string;name:string;fact2:string;fact3:string;email:string;email2:string;})=>one(db,`SELECT id,name FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active'
  AND (lower(trim(name))=lower(?) OR (?<>'' AND regexp_replace(COALESCE(phone,''),'[^0-9]','','g')=?)
   OR (?<>'' AND lower(notification_email)=lower(?))) LIMIT 1`,facts.companyId,facts.name,facts.fact2,facts.fact3,facts.email,facts.email2),
writeFinanceParties1:(facts:{customer:`${string}-${string}-${string}-${string}-${string}`;companyId:string;fact2:string;name:string;currency:string | number | boolean | null;fact5:string | null;fact6:string | null;fact7:string;fact8:number;fact9:number;})=>run(db,`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,phone,notification_email,address_json,created_at,updated_at)
  VALUES(?,?,'customer',?,?,?,'active',?,?,?::jsonb,?,?)`,facts.customer,facts.companyId,facts.fact2,facts.name,facts.currency,facts.fact5,facts.fact6,facts.fact7,facts.fact8,facts.fact9),
readFinanceJournals1:(facts:{company:string;date:string;branch:string;})=>all(db,`SELECT j.id,j.status,j.entry_date,j.source_id,j.memo,j.version FROM finance_journals j WHERE j.company_id=? AND j.entry_date=? AND j.source_type='pos'
  AND EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.dimension_json::jsonb->>'branchId'=?) ORDER BY j.id`,facts.company,facts.date,facts.branch),
readFinanceJournalLines1:(facts:{company:string;date:string;branch:string;})=>one(db,`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
  ${posSnapshot().dailySettings} WHERE j.company_id=? AND j.entry_date=? AND j.status='posted' AND j.source_type='pos'
  AND l.dimension_json::jsonb->>'branchId'=? AND l.account_id=c.accounts_json::jsonb->>'cogs'`,facts.company,facts.date,facts.branch),
readFinanceJournalLines2:(facts:{company:string;date:string;branch:string;})=>one(db,`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
  ${posSnapshot().dailySettings} WHERE j.company_id=? AND j.entry_date=? AND j.status='posted' AND j.source_type='pos'
  AND l.dimension_json::jsonb->>'branchId'=? AND l.account_id=c.accounts_json::jsonb->>'waste'`,facts.company,facts.date,facts.branch),
readFinanceJournalLines3:(facts:{company:string;})=>one(db,`SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) n FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
  ${posSnapshot().dailySettings} WHERE j.company_id=? AND j.status='posted' AND l.account_id=c.accounts_json::jsonb->>'stripe'`,facts.company)
};}
