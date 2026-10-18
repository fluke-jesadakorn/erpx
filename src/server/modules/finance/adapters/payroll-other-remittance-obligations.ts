import 'server-only';
import {payrollOtherRemittanceTables as people} from '../../people_payroll/public/finance-other-remittance-sources';
/** Embedded only in the two actual People/Payroll owner queries, after their
 * query-free native source leases. Original statutory facts are unchanged.
 * These predicates use existing columns/JSON so pre1066 history remains valid. */
export const payrollOtherRemittanceObligationFacts=`
 UNION ALL SELECT 'other-liability',p.run_id||':'||line.description||':'||line.account_id,1,line.description,
 jsonb_build_object('posting',to_jsonb(p),'accountId',line.account_id,'amount',line.amount),
 line.amount<>COALESCE((SELECT SUM(remit.amount_minor) FROM (${people.remittances}) remit
  WHERE remit.company_id=p.company_id AND remit.run_id=p.run_id AND remit.liability_account_id=line.account_id
  AND remit.kind=CASE line.description WHEN 'Payroll benefit' THEN 'benefit' WHEN 'Payroll deduction other' THEN 'deduction_other' WHEN 'Payroll employer other' THEN 'employer_other' END),0)
 FROM postings p CROSS JOIN LATERAL(SELECT l.account_id,l.description,SUM(l.credit_minor-l.debit_minor) amount
  FROM finance_journal_lines l WHERE l.journal_id=p.journal_id AND l.description IN ('Payroll benefit','Payroll deduction other','Payroll employer other') GROUP BY l.account_id,l.description HAVING SUM(l.credit_minor-l.debit_minor)>0) line
 UNION ALL SELECT 'other-remittance',r.id,1,r.kind,to_jsonb(r)||jsonb_build_object('journal',to_jsonb(j),'bank',to_jsonb(bank),'money',to_jsonb(money)),
 run.id IS NULL OR run.status<>'posted' OR accrual.id IS NULL OR j.id IS NULL OR j.status<>'posted' OR j.source_type<>'payroll_remittance' OR j.source_id IS DISTINCT FROM r.id OR j.currency IS DISTINCT FROM run.currency OR j.chart_id IS DISTINCT FROM accrual.chart_id
 OR j.posted_by IS DISTINCT FROM r.recorded_by OR j.entry_date IS DISTINCT FROM bank.transaction_date
 OR (SELECT COUNT(*) FROM finance_journal_lines line WHERE line.journal_id=j.id)<>2
 OR (SELECT SUM(line.debit_minor) FROM finance_journal_lines line WHERE line.journal_id=j.id) IS DISTINCT FROM r.amount_minor
 OR (SELECT SUM(line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=j.id) IS DISTINCT FROM r.amount_minor
 OR (SELECT SUM(line.debit_minor) FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=r.liability_account_id) IS DISTINCT FROM r.amount_minor
 OR (SELECT SUM(line.credit_minor) FROM finance_journal_lines line WHERE line.journal_id=j.id AND line.account_id=r.bank_account_id) IS DISTINCT FROM r.amount_minor
 OR bank.id IS NULL OR bank.company_id IS DISTINCT FROM r.company_id OR bank.match_status<>'matched' OR bank.matched_type<>'payroll_remittance' OR bank.matched_id IS DISTINCT FROM r.id OR bank.amount_minor<>-r.amount_minor OR bank.currency IS DISTINCT FROM run.currency OR bank.external_id IS DISTINCT FROM r.bank_reference OR bank.transaction_date<run.period_end
 OR statement.id IS NULL OR statement.company_id IS DISTINCT FROM r.company_id OR statement.bank_account IS DISTINCT FROM r.bank_account_id
 OR COALESCE(length(btrim(to_jsonb(r)->>'reason')) NOT BETWEEN 3 AND 3000,true)
 OR NOT EXISTS(SELECT 1 FROM command_receipts command WHERE r.id='payroll-remit-'||command.command_id AND command.operation='payroll.liability.remit' AND command.record_id=r.run_id AND command.actor_id=r.recorded_by AND command.authority_json::jsonb->>'companyId'=r.company_id
  AND command.result_json::jsonb->>'id'=r.id AND command.result_json::jsonb->>'runId'=r.run_id AND command.result_json::jsonb->>'kind'=r.kind AND command.result_json::jsonb->>'liabilityAccountId'=r.liability_account_id AND command.result_json::jsonb->>'amountMinor'=r.amount_minor::text AND command.result_json::jsonb->>'journalId'=r.journal_id)
 OR money.source_id IS NULL OR money.currency IS DISTINCT FROM run.currency OR money.currency_scale<>2 OR money.amount_units IS DISTINCT FROM r.amount_minor OR money.transaction_date IS DISTINCT FROM bank.transaction_date::date
 OR COALESCE((SELECT SUM(line.credit_minor-line.debit_minor) FROM finance_journal_lines line WHERE line.journal_id=accrual.id AND line.account_id=r.liability_account_id AND line.description=CASE r.kind WHEN 'benefit' THEN 'Payroll benefit' WHEN 'deduction_other' THEN 'Payroll deduction other' WHEN 'employer_other' THEN 'Payroll employer other' END),0)
  <(SELECT SUM(remit.amount_minor) FROM (${people.remittances}) remit WHERE remit.company_id=r.company_id AND remit.run_id=r.run_id AND remit.kind=r.kind AND remit.liability_account_id=r.liability_account_id)
 FROM (${people.remittances}) r CROSS JOIN scope LEFT JOIN runs run ON run.id=r.run_id LEFT JOIN postings source ON source.run_id=run.id LEFT JOIN finance_journals accrual ON accrual.id=source.journal_id AND accrual.company_id=r.company_id AND accrual.status='posted'
 LEFT JOIN finance_journals j ON j.id=r.journal_id AND j.company_id=r.company_id LEFT JOIN finance_bank_transactions bank ON bank.id=r.bank_transaction_id
 LEFT JOIN finance_bank_statements statement ON statement.id=bank.statement_id LEFT JOIN money_transaction_snapshots money ON money.company_id=r.company_id AND money.source_type='payroll_other_remittance' AND money.source_id=r.id AND money.amount_kind='paid'
 WHERE r.company_id=scope.company_id AND r.kind IN ('benefit','deduction_other','employer_other')
 UNION ALL SELECT 'other-bank-evidence',command.command_id,COALESCE(bank.version,1),bank.match_status,
 to_jsonb(command)||jsonb_build_object('statement',to_jsonb(statement),'bank',to_jsonb(bank)),
 marker.value->>'format' IS DISTINCT FROM 'payroll-other-liability-bank-evidence/v1' OR marker.value->>'companyId' IS DISTINCT FROM scope.company_id OR command.authority_json::jsonb->>'companyId' IS DISTINCT FROM scope.company_id
 OR marker.value->>'actorId' IS DISTINCT FROM command.actor_id OR statement.id IS NULL OR bank.id IS NULL OR run.id IS NULL OR run.status<>'posted'
 OR statement.id IS DISTINCT FROM 'statement-'||command.command_id OR statement.company_id IS DISTINCT FROM scope.company_id OR statement.imported_by IS DISTINCT FROM command.actor_id
 OR bank.id IS DISTINCT FROM 'bankrow-'||command.command_id||'-0' OR bank.statement_id IS DISTINCT FROM statement.id OR bank.company_id IS DISTINCT FROM scope.company_id
 OR bank.amount_minor>=0 OR bank.transaction_date<run.period_end OR bank.currency IS DISTINCT FROM run.currency
 OR command.result_json::jsonb->>'id' IS DISTINCT FROM statement.id OR command.result_json::jsonb->>'bankTransactionId' IS DISTINCT FROM bank.id OR command.result_json::jsonb->>'payrollRunId' IS DISTINCT FROM run.id OR command.result_json::jsonb->>'kind' IS DISTINCT FROM marker.value->>'kind' OR command.result_json::jsonb->>'liabilityAccountId' IS DISTINCT FROM marker.value->>'liabilityAccountId'
 OR marker.value->'statementImage' IS DISTINCT FROM jsonb_build_array(statement.id,statement.company_id,statement.bank_account,statement.statement_number,statement.statement_date,statement.opening_minor,statement.closing_minor,statement.closing_fx_rate_micros,statement.base_closing_minor,statement.source_hash,statement.imported_by,statement.created_at)
 OR marker.value->'movementImage' IS DISTINCT FROM jsonb_build_array(bank.id,bank.company_id,bank.statement_id,bank.transaction_date,bank.external_id,bank.description,bank.amount_minor,bank.currency)
 OR NOT EXISTS(SELECT 1 FROM command_receipts original JOIN postings posting ON posting.run_id=run.id WHERE original.command_id=marker.value->>'postingCommandId' AND original.operation='people.payroll.post' AND original.record_id=run.id AND original.actor_id=run.posted_by AND original.authority_json::jsonb->>'companyId'=scope.company_id AND original.result_json::jsonb->>'journalId'=posting.journal_id AND original.result_json::jsonb->>'status'='posted')
 OR NOT((bank.match_status='matched' AND bank.matched_type='payroll_remittance' AND EXISTS(SELECT 1 FROM (${people.remittances}) remit WHERE remit.id=bank.matched_id AND remit.company_id=scope.company_id AND remit.bank_transaction_id=bank.id AND remit.run_id=run.id AND remit.kind=marker.value->>'kind' AND remit.liability_account_id=marker.value->>'liabilityAccountId'))
 OR (bank.match_status='ignored' AND bank.matched_type IS NULL AND bank.matched_id IS NULL AND EXISTS(SELECT 1 FROM command_receipts decision JOIN audit_events audit ON audit.command_id=decision.command_id AND audit.action=decision.operation AND audit.record_id=bank.id AND audit.actor_id=decision.actor_id
  WHERE decision.operation='payroll.liability.bank-evidence.ignore' AND decision.record_id=bank.id AND decision.authority_json::jsonb->>'companyId'=scope.company_id
   AND decision.result_json::jsonb->>'id'=bank.id AND decision.result_json::jsonb->>'status'='ignored' AND decision.result_json::jsonb->>'version'~'^[1-9][0-9]*$' AND bank.version>=(decision.result_json::jsonb->>'version')::bigint AND audit.record_version=(decision.result_json::jsonb->>'version')::bigint
   AND decision.result_json::jsonb->>'payrollRunId'=run.id AND decision.result_json::jsonb->>'kind'=marker.value->>'kind' AND decision.result_json::jsonb->>'liabilityAccountId'=marker.value->>'liabilityAccountId'
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'format'='payroll-other-liability-bank-ignore/v1' AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'commandId'=decision.command_id AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'importCommandId'=command.command_id
   AND decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'sourceFingerprint'=marker.value->>'sourceFingerprint' AND length(btrim(decision.result_json::jsonb->'__payrollOtherLiabilityBankIgnore'->>'reason')) BETWEEN 3 AND 3000)))
 FROM command_receipts command CROSS JOIN scope CROSS JOIN LATERAL(SELECT command.result_json::jsonb->'__payrollOtherLiabilityBankEvidence' value) marker
 LEFT JOIN finance_bank_statements statement ON statement.id=command.record_id LEFT JOIN finance_bank_transactions bank ON bank.id=command.result_json::jsonb->>'bankTransactionId'
 LEFT JOIN runs run ON run.id=marker.value->>'runId'
 WHERE command.operation='finance.bank.import' AND jsonb_exists(command.result_json::jsonb,'__payrollOtherLiabilityBankEvidence') AND (command.authority_json::jsonb->>'companyId'=scope.company_id OR marker.value->>'companyId'=scope.company_id)
`;
