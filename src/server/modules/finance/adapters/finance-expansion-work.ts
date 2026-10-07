import { createFinanceExpansionWork, type DueWorkRow, type CloseWorkPeriod } from '../application/expansion-work';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import { requirePermission } from '@/server/modules/identity_access/public/iam-permissions';
import { holdingResources } from '@/server/modules/runtime/public/runtime';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { financeExpansionClosePending } from '../public/finance-expansion-close';
export async function financeExpansionWork(actor: WorkContext) {
    let write: SqlDatabase;
    return createFinanceExpansionWork({ permission: requirePermission, ready: async (actor) => { write = (await holdingResources(actor.holdingId)).write; }, today: () => new Intl.DateTimeFormat('en-CA', { timeZone: 'Asia/Bangkok', year: 'numeric', month: '2-digit', day: '2-digit' }).format(new Date()), rows: async (actor) => (await write
            .prepare(`SELECT id,name title,kind,chart_id,'draft' status,prepared_by,created_at updated_at FROM finance_plan_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Book opening and mapping','book',secondary_chart_id,status,prepared_by,created_at FROM finance_book_rules WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Bank match evidence','bank',NULL,status,proposed_by,updated_at FROM finance_bank_match_groups WHERE company_id=? AND status='proposed'
 UNION ALL SELECT id,'Customer collection message','collection',NULL,status,prepared_by,created_at FROM finance_collection_cases WHERE company_id=? AND status IN ('draft','approved')
 UNION ALL SELECT id,name,'contract',chart_id,status,prepared_by,created_at FROM finance_business_contracts WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Posted billing credit and replacement','billing-correction',NULL,status,prepared_by,updated_at FROM finance_billing_corrections WHERE company_id=? AND status='draft'
 UNION ALL SELECT b.id,'Zero-usage service evidence','zero-usage',c.chart_id,b.no_charge_status,b.created_by,b.created_at FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.company_id=? AND b.no_charge_status='draft' AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id)
 UNION ALL SELECT r.id,'Service revenue recognition','recognition',c.chart_id,r.status,r.prepared_by,r.updated_at FROM finance_recognition_runs r JOIN finance_business_billing_runs b ON b.id=r.billing_run_id JOIN finance_business_contracts c ON c.id=b.contract_id WHERE r.company_id=? AND r.status='draft'
 UNION ALL SELECT id,'Secondary book policy adjustment','book-adjustment',chart_id,status,prepared_by,created_at FROM finance_book_adjustments WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,name,'instrument',chart_id,status,prepared_by,created_at FROM finance_instrument_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT r.id,'Financing bank payment','instrument-run',e.chart_id,r.status,r.prepared_by,r.created_at FROM finance_instrument_runs r JOIN finance_instrument_editions e ON e.id=r.instrument_id WHERE r.company_id=? AND r.status='draft'
 UNION ALL SELECT id,'Asset depreciation policy','asset-policy',NULL,status,prepared_by,created_at FROM finance_asset_policy_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Asset depreciation review','asset-policy-run',NULL,status,prepared_by,created_at FROM finance_asset_policy_runs WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Group reporting edition','group-edition',NULL,status,prepared_by,created_at FROM finance_group_report_editions WHERE company_id=? AND status='draft'
 UNION ALL SELECT id,'Ownership and control evidence','ownership',NULL,status,created_by,created_at FROM finance_ownership_interests WHERE company_id=? AND status='draft' AND source_hash IS NOT NULL
 UNION ALL SELECT id,'Acquisition and goodwill evidence','acquisition',NULL,status,created_by,created_at FROM finance_acquisitions WHERE company_id=? AND status='draft' AND source_hash IS NOT NULL
 UNION ALL SELECT id,'Intercompany document pair','intercompany-pair',NULL,status,prepared_by,created_at FROM finance_intercompany_document_pairs WHERE company_id=? AND status='draft'
 ORDER BY updated_at DESC,id LIMIT 500`)
            .bind(...Array.from({ length: 17 }, () => actor.companyId))
            .all<{
            id: string;
            title: string;
            kind: string;
            chart_id: string | null;
            status: string;
            prepared_by: string;
            updated_at: number;
        }>()).results, due: async (actor) => (await write.prepare("SELECT id,kind,run_on,status,last_message,updated_at FROM finance_due_drafts WHERE company_id=? AND status IN ('scheduled','blocked') ORDER BY run_on,id LIMIT 500").bind(actor.companyId).all<DueWorkRow>()).results, periods: async (actor, today) => (await write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state IN ('open','closing') AND end_date<=? ORDER BY start_date,id LIMIT 6").bind(actor.companyId, today).all<CloseWorkPeriod>()).results, pending: async (company, start, end) => financeExpansionClosePending(write, company, start, end) })(actor);
}
