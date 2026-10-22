import {financeSourceAccess} from "@erpx/server-finance/adapters/source-purchases";
import {financePackageEnforcementMode} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate,packageClockSql} from "@erpx/server-commercial/public/purchased-capability-sql";
import {groupHistoryPredicate,groupEditionHistorySql,groupRecordHistorySql} from "@erpx/server-finance/adapters/group-purchase-sources";
import { createFinanceExpansionWork, type DueWorkRow, type CloseWorkPeriod } from '@erpx/server-finance/application/expansion-work';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { requirePermission } from '@erpx/server-identity_access/public/iam-permissions';
import { holdingResources } from '../../../../packages/server-runtime/src/public/runtime';
import { financeExpansionClosePending } from '@erpx/server-finance/public/finance-expansion-close';
export async function financeExpansionWork(actor: WorkContext) {
    actor=requirePermission(actor,"finance.read");
    const write=(await holdingResources(actor.holdingId)).write,mode=await financePackageEnforcementMode(),history=await groupHistoryPredicate();
    const access=await financeSourceAccess(write,actor,["finance.accounting","finance.banking","finance.close","finance.advanced","finance.receivables"]);
    if(!access["finance.accounting"].history)return [];
    const clock=mode==="enforced"?`WITH finance_group_purchase_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) `:"";
    const purchase=(company:string,capability:string)=>mode==="enforced"?`(SELECT ${purchasedCapabilityPredicate(company,capability,"history","finance_group_purchase_clock.now_ms")} FROM finance_group_purchase_clock)`:"TRUE";
    const actionAllowed=(kind:string)=>access[["instrument","instrument-run","instrument_payment","calculation","book-adjustment","group-edition","ownership","acquisition","intercompany-pair"].includes(kind)||kind.includes("financing")?"finance.advanced":kind==="collection"?"finance.receivables":kind==="bank"?"finance.banking":"finance.accounting"].settleExisting;
    return createFinanceExpansionWork({actionAllowed, permission: requirePermission, ready: async () => {}, today: () => new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Bangkok', year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date()), rows: async (actor) => (await write
            .prepare(`${clock}SELECT id,name title,kind,chart_id,'draft' status,prepared_by,created_at updated_at FROM finance_plan_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Book opening and mapping','book',secondary_chart_id,status,prepared_by,created_at FROM finance_book_rules WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Bank match evidence','bank',NULL,status,proposed_by,updated_at FROM finance_bank_match_groups WHERE company_id=? AND status='proposed' AND ${purchase('finance_bank_match_groups.company_id','finance.banking')}
 UNION ALL SELECT id,'Customer collection message','collection',NULL,status,prepared_by,created_at FROM finance_collection_cases WHERE company_id=? AND status IN ('draft','approved') AND ${purchase('finance_collection_cases.company_id','finance.receivables')}
 UNION ALL SELECT id,name,'contract',chart_id,status,prepared_by,created_at FROM finance_business_contracts WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Posted billing credit and replacement','billing-correction',NULL,status,prepared_by,updated_at FROM finance_billing_corrections WHERE company_id=? AND status='draft'
 UNION ALL SELECT b.id,'Zero-usage service evidence','zero-usage',c.chart_id,b.no_charge_status,b.created_by,b.created_at FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.company_id=? AND b.no_charge_status='draft' AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id)
 UNION ALL SELECT r.id,'Service revenue recognition','recognition',c.chart_id,r.status,r.prepared_by,r.updated_at FROM finance_recognition_runs r JOIN finance_business_billing_runs b ON b.id=r.billing_run_id JOIN finance_business_contracts c ON c.id=b.contract_id WHERE r.company_id=? AND r.status='draft'
 UNION ALL SELECT id,'Secondary book policy adjustment','book-adjustment',chart_id,status,prepared_by,created_at FROM finance_book_adjustments WHERE company_id=? AND status='draft' AND ${purchase('finance_book_adjustments.company_id','finance.advanced')}
 UNION ALL SELECT id,name,'instrument',chart_id,status,prepared_by,created_at FROM finance_instrument_editions WHERE company_id=? AND status='draft' AND ${purchase('finance_instrument_editions.company_id','finance.advanced')}
 UNION ALL SELECT r.id,'Financing bank payment','instrument-run',e.chart_id,r.status,r.prepared_by,r.created_at FROM finance_instrument_runs r JOIN finance_instrument_editions e ON e.id=r.instrument_id WHERE r.company_id=? AND r.status='draft' AND ${purchase('r.company_id','finance.advanced')}
 UNION ALL SELECT id,'Asset depreciation policy','asset-policy',NULL,status,prepared_by,created_at FROM finance_asset_policy_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Asset depreciation review','asset-policy-run',NULL,status,prepared_by,created_at FROM finance_asset_policy_runs WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Group reporting edition','group-edition',NULL,status,prepared_by,created_at FROM finance_group_report_editions r WHERE company_id=? AND status='draft' AND ${purchase('r.company_id','finance.advanced')} AND (${groupEditionHistorySql('r',history)})
 UNION ALL SELECT id,'Ownership and control evidence','ownership',NULL,status,created_by,created_at FROM finance_ownership_interests r WHERE company_id=? AND status='draft' AND source_hash IS NOT NULL AND ${purchase('r.company_id','finance.advanced')} AND (${groupRecordHistorySql('ownership','r',history)})
 UNION ALL SELECT id,'Acquisition and goodwill evidence','acquisition',NULL,status,created_by,created_at FROM finance_acquisitions r WHERE company_id=? AND status='draft' AND source_hash IS NOT NULL AND ${purchase('r.company_id','finance.advanced')} AND (${groupRecordHistorySql('acquisition','r',history)})
 UNION ALL SELECT id,'Intercompany document pair','intercompany-pair',NULL,status,prepared_by,created_at FROM finance_intercompany_document_pairs r WHERE company_id=? AND status='draft' AND ${purchase('r.company_id','finance.advanced')} AND (${groupRecordHistorySql('pair','r',history)})
 UNION ALL SELECT id,'Retained financial calculation','calculation',NULL,status,prepared_by,updated_at FROM finance_calculation_runs r WHERE r.company_id=? AND r.status IN ('prepared','reviewed') AND COALESCE(r.explanation_json::jsonb->>'engine','') NOT IN ('source-vat-register-v1','source-acquisition-v1') AND ${purchase('r.company_id','finance.advanced')}
 ORDER BY updated_at DESC,id LIMIT 500`)
            .bind(...Array.from({ length: 18 }, () => actor.companyId))
            .all<{
            id: string;
            title: string;
            kind: string;
            chart_id: string | null;
            status: string;
            prepared_by: string;
            updated_at: number;
        }>()).results, due: async (actor) => (await write.prepare(`${clock}SELECT id,kind,run_on,status,last_message,updated_at FROM finance_due_drafts r WHERE company_id=? AND status IN ('scheduled','blocked') AND (kind<>'instrument_payment' OR ${purchase('r.company_id','finance.advanced')}) ORDER BY run_on,id LIMIT 500`).bind(actor.companyId).all<DueWorkRow>()).results, periods: async (actor, today) => !access["finance.close"].history?[]:(await write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state IN ('open','closing') AND end_date<=? ORDER BY start_date,id LIMIT 6").bind(actor.companyId, today).all<CloseWorkPeriod>()).results, pending: async (company, start, end) => (await financeExpansionClosePending(write,company,start,end)).filter(row=>!row.kind.includes("financing")||access["finance.advanced"].history) })(actor);
}
