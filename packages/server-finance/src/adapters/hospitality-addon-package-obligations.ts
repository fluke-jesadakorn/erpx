import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageSettlementSource,type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";
import { assertPosAddonPackageObligationSources,posAddonPackageSettlementSources } from "@erpx/server-pos/public/addon-package-obligations";
import { assertPmsAddonPackageObligationSources,pmsAddonPackageSettlementSources } from "@erpx/server-pms/public/addon-package-obligations";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockHospitalityAddonFinancePackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
  const {database,companyId}=packageSettlementSource(owner,lock);
  await database.prepare('LOCK TABLE finance_invoices,finance_payments,finance_journals,finance_journal_lines,finance_allocations IN SHARE MODE').run();
  sources.set(database,companyId);
}
export async function hospitalityAddonFinancePackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
  owner.assertActiveTransaction(database);
  if(sources.get(database)!==input.companyId)throw new Error('HOSPITALITY_ADDON_FINANCE_SOURCE_REQUIRED');
  if(input.moduleId!=='finance'||!['pos-restaurant','pos-hotel-charge','pms-minibar-linen'].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw new Error('HOSPITALITY_ADDON_FINANCE_SCOPE_REQUIRED');
  const posSource=input.packageId!=='pms-minibar-linen',pmsSource=input.packageId!=='pos-restaurant';
  if(posSource)assertPosAddonPackageObligationSources(owner,database,input.companyId,input.packageId);
  if(pmsSource)assertPmsAddonPackageObligationSources(owner,database,input.companyId,input.packageId);
  // Each query is selected by the exact owner package, not a caller supplied
  // record label. Shared Company clearing balances remain with the base owner.
  const pos=posSource?posAddonPackageSettlementSources(input.packageId):null;
  const pms=pmsSource?pmsAddonPackageSettlementSources(input.packageId):null;
  const posFacts=pos?(await database.prepare(`WITH scope AS (SELECT ?::text company_id,?::text package_id),
    sales AS MATERIALIZED (SELECT s.* FROM (${pos.sales}) s,scope WHERE s.company_id=scope.company_id AND s.package_id=scope.package_id),
    postings AS MATERIALIZED (SELECT p.* FROM (${pos.postings}) p,scope WHERE p.company_id=scope.company_id AND p.package_id=scope.package_id)
    SELECT 'pos-invoice' kind,s.id,COALESCE(i.version,0) version,COALESCE(i.status,'missing') state,COALESCE(i.outstanding_minor,0)::text amount FROM sales s LEFT JOIN finance_invoices i ON i.id=s.invoice_id AND i.company_id=s.company_id AND i.source_type='pos' AND i.source_id=s.id
      WHERE s.invoice_id IS NOT NULL AND (i.id IS NULL OR i.currency<>s.currency OR i.kind IS DISTINCT FROM 'receivable' OR i.total_minor IS DISTINCT FROM s.total_minor OR i.status NOT IN ('paid','voided') OR i.outstanding_minor<>0)
    UNION ALL SELECT 'pos-payment',p.id,COALESCE(f.version,0),COALESCE(f.status,'missing'),COALESCE(f.outstanding_minor,0)::text FROM (${pos.payments}) p JOIN sales s ON s.id=p.sale_id AND s.package_id=p.package_id
      LEFT JOIN finance_payments f ON f.id=p.finance_payment_id AND f.company_id=p.company_id LEFT JOIN postings receipt ON receipt.source_key='receipt:'||p.id
      WHERE p.state='succeeded' AND (f.id IS NULL OR f.status NOT IN ('settled','voided') OR f.outstanding_minor<>0 OR f.kind IS DISTINCT FROM 'receipt' OR f.currency IS DISTINCT FROM s.currency OR f.amount_minor IS DISTINCT FROM p.amount_minor OR f.payment_number IS DISTINCT FROM 'POS-R-'||p.id OR f.journal_id IS DISTINCT FROM receipt.journal_id OR f.bank_account IS DISTINCT FROM CASE p.method WHEN 'cash' THEN s.accounts_json::jsonb->>'cash' WHEN 'transfer' THEN s.accounts_json::jsonb->>'bank' WHEN 'edc' THEN s.accounts_json::jsonb->>'edc' ELSE s.accounts_json::jsonb->>'stripe' END)
    UNION ALL SELECT 'pos-required-posting',required.source_key,0,'missing','0' FROM (${pos.requiredPostings}) required,scope WHERE required.company_id=scope.company_id AND required.package_id=scope.package_id AND NOT EXISTS(SELECT 1 FROM postings p WHERE p.source_key=required.source_key)
    UNION ALL SELECT 'pos-posting',p.source_key,COALESCE(j.version,0),COALESCE(j.status,'missing'),COALESCE(sum(l.base_debit_minor-l.base_credit_minor),0)::text FROM postings p LEFT JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id LEFT JOIN finance_journal_lines l ON l.journal_id=j.id
      GROUP BY p.source_key,p.expected_source_id,p.expected_currency,j.id,j.version,j.status HAVING j.id IS NULL OR j.status<>'posted' OR j.source_type IS DISTINCT FROM 'pos' OR j.source_id IS DISTINCT FROM p.expected_source_id OR j.memo IS DISTINCT FROM p.source_key OR (p.expected_currency IS NOT NULL AND j.currency IS DISTINCT FROM p.expected_currency) OR count(l.id)<2 OR sum(l.base_debit_minor-l.base_credit_minor)<>0
    UNION ALL SELECT 'pos-source-balance',concat(l.account_id,':',p.expected_currency),0,'unsettled',sum(l.debit_minor-l.credit_minor)::text FROM postings p JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id AND j.status='posted' JOIN finance_journal_lines l ON l.journal_id=j.id
      WHERE p.expected_accounts_json IS NOT NULL AND j.currency=p.expected_currency AND l.account_id IN (p.expected_accounts_json::jsonb->>'advance',p.expected_accounts_json::jsonb->>'receivable',p.expected_accounts_json::jsonb->>'refund',p.expected_accounts_json::jsonb->>'wip')
      GROUP BY l.account_id,p.expected_currency HAVING sum(l.debit_minor-l.credit_minor)<>0
    ORDER BY kind,id`).bind(input.companyId,input.packageId).all()).results:[];
  const pmsFacts=pms?(await database.prepare(`WITH scope AS (SELECT ?::text company_id,?::text package_id),
    postings AS MATERIALIZED (SELECT p.* FROM (${pms.postings}) p,scope WHERE p.company_id=scope.company_id AND p.package_id=scope.package_id),
    accounts AS MATERIALIZED (SELECT a.* FROM (${pms.accounts}) a,scope WHERE a.company_id=scope.company_id AND a.package_id=scope.package_id)
    SELECT 'pms-required-posting' kind,concat(required.property_id,':',required.source_key) id,0::bigint version,'missing' state,'0' amount FROM (${pms.requiredPostings}) required,scope
      WHERE required.company_id=scope.company_id AND required.package_id=scope.package_id AND NOT EXISTS(SELECT 1 FROM postings p WHERE p.property_id=required.property_id AND p.source_key=required.source_key)
    UNION ALL SELECT 'pms-posting',concat(p.property_id,':',p.source_key),COALESCE(j.version,0),COALESCE(j.status,'missing'),COALESCE(sum(l.base_debit_minor-l.base_credit_minor),0)::text FROM postings p LEFT JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id LEFT JOIN finance_journal_lines l ON l.journal_id=j.id
      GROUP BY p.property_id,p.source_key,p.expected_source_id,p.expected_currency,j.id,j.version,j.status HAVING j.id IS NULL OR j.status<>'posted' OR j.source_type IS DISTINCT FROM 'pms' OR j.source_id IS DISTINCT FROM p.expected_source_id OR j.memo IS DISTINCT FROM p.source_key OR j.currency IS DISTINCT FROM p.expected_currency OR count(l.id)<2 OR sum(l.base_debit_minor-l.base_credit_minor)<>0
    UNION ALL SELECT 'pms-payment',concat(p.property_id,':',p.entry_id,':',coalesce(p.document_id,'missing')),COALESCE(f.version,p.version),COALESCE(f.status,'missing'),COALESCE(f.outstanding_minor,0)::text FROM (${pms.payments}) p CROSS JOIN scope LEFT JOIN finance_payments f ON f.id=p.finance_id AND f.company_id=p.company_id
      WHERE p.company_id=scope.company_id AND p.package_id=scope.package_id AND (p.document_count<>1 OR p.document_id IS NULL OR p.finance_id IS DISTINCT FROM p.document_id OR f.id IS NULL OR f.id IS DISTINCT FROM p.document_id OR f.kind IS DISTINCT FROM p.expected_kind OR f.payment_number IS DISTINCT FROM 'PMS-R-'||p.document_id OR f.currency IS DISTINCT FROM p.expected_currency OR f.amount_minor IS DISTINCT FROM p.expected_amount OR f.status IS DISTINCT FROM 'settled' OR f.outstanding_minor<>0 OR f.bank_account IS DISTINCT FROM p.expected_bank_account OR p.expected_journal_id IS NULL OR f.journal_id IS DISTINCT FROM p.expected_journal_id OR p.document_amount IS DISTINCT FROM p.expected_amount::text OR p.document_currency IS DISTINCT FROM p.expected_currency OR p.snapshot_entry_count<>1 OR p.snapshot_entry_kind IS DISTINCT FROM p.entry_kind OR p.snapshot_entry_method IS DISTINCT FROM p.entry_method OR p.snapshot_entry_total IS DISTINCT FROM p.entry_total)
    -- The native guest-account is the settlement unit, including reviewed
    -- imported opening pockets. Its opening journal can cover other bills:
    -- never allocate or sweep that aggregate journal into this addon.
    UNION ALL SELECT 'pms-source-balance',concat(a.property_id,':',a.id,':',pocket.kind),a.version,'unsettled',pocket.amount::text FROM accounts a CROSS JOIN LATERAL (VALUES
      ('receivable',CASE WHEN a.track_due THEN COALESCE((a.data->>'due')::bigint,0) ELSE 0 END),
      ('advance',COALESCE((a.data->>'advance')::bigint,0)),
      ('refund-reserved',COALESCE((a.data->>'refundReserved')::bigint,0)),
      ('deposit-refund-reserved',COALESCE((a.data->>'depositRefundReserved')::bigint,0))) pocket(kind,amount)
      WHERE pocket.amount<>0
    ORDER BY kind,id`).bind(input.companyId,input.packageId).all()).results:[];
  const facts=[...posFacts,...pmsFacts];
  return {companyId:input.companyId,packageId:input.packageId,moduleId:'finance',observedAt:input.now,revision:createHash('sha256').update(JSON.stringify({companyId:input.companyId,packageId:input.packageId,packageVersion:input.packageVersion,facts})).digest('hex'),outstanding:facts.length};
}
