import {financeOwnedSnapshotSources as commerceSnapshotSources} from "@erpx/server-commerce/public/finance-owned-snapshot-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-commerce/public/commerce-contracts";
export function commerceFinanceJoinedFacts(db:SqlDatabase){return {
readCommerceSettings1: async (facts: {
            companyId: string;
        }) => { const existing = await db.prepare(`SELECT s.*,c.base_currency,d.id approved_sales_defaults_id,COALESCE(d.receivable_account_id,s.receivable_account_id) receivable_account_id,COALESCE(d.revenue_account_id,s.revenue_account_id) revenue_account_id,CASE WHEN d.id IS NOT NULL THEN d.tax_account_id ELSE s.output_tax_account_id END output_tax_account_id ${commerceSnapshotSources().commerce_joined_facts_commerce_settings_s_from} JOIN finance_charts c ON c.id=s.chart_id AND c.company_id=s.company_id AND c.status='active' LEFT JOIN finance_sales_defaults d ON d.company_id=s.company_id AND d.chart_id=s.chart_id AND d.currency=s.currency AND d.status='approved' AND EXISTS(SELECT 1 FROM finance_accounts ar WHERE ar.id=d.receivable_account_id AND ar.company_id=d.company_id AND ar.chart_id=d.chart_id AND ar.currency=d.currency AND ar.control_kind='receivable' AND ar.status='active') AND EXISTS(SELECT 1 FROM finance_accounts rev WHERE rev.id=d.revenue_account_id AND rev.company_id=d.company_id AND rev.chart_id=d.chart_id AND rev.currency=d.currency AND rev.category='revenue' AND rev.status='active') AND (d.tax_account_id IS NULL OR EXISTS(SELECT 1 FROM finance_accounts vat WHERE vat.id=d.tax_account_id AND vat.company_id=d.company_id AND vat.chart_id=d.chart_id AND vat.currency=d.currency AND vat.category='liability' AND vat.control_kind='none' AND vat.status='active')) WHERE s.company_id=?`).bind(facts.companyId).first<Row>();
            if (existing) return existing;
            return db.prepare(`SELECT d.company_id,d.chart_id,d.currency,c.base_currency,d.id approved_sales_defaults_id,d.receivable_account_id,d.revenue_account_id,d.tax_account_id output_tax_account_id,NULL::text inventory_account_id,NULL::text cogs_account_id,NULL::text clearing_account_id,NULL::text payable_account_id,NULL::text input_tax_account_id,NULL::text cash_account_id FROM finance_sales_defaults d JOIN finance_charts c ON c.id=d.chart_id AND c.company_id=d.company_id AND c.status='active' WHERE d.company_id=? AND d.status='approved' AND EXISTS(SELECT 1 FROM finance_accounts ar WHERE ar.id=d.receivable_account_id AND ar.company_id=d.company_id AND ar.chart_id=d.chart_id AND ar.currency=d.currency AND ar.control_kind='receivable' AND ar.status='active') AND EXISTS(SELECT 1 FROM finance_accounts rev WHERE rev.id=d.revenue_account_id AND rev.company_id=d.company_id AND rev.chart_id=d.chart_id AND rev.currency=d.currency AND rev.category='revenue' AND rev.status='active') AND (d.tax_account_id IS NULL OR EXISTS(SELECT 1 FROM finance_accounts vat WHERE vat.id=d.tax_account_id AND vat.company_id=d.company_id AND vat.chart_id=d.chart_id AND vat.currency=d.currency AND vat.category='liability' AND vat.control_kind='none' AND vat.status='active')) ORDER BY d.created_at DESC,d.id LIMIT 1`).bind(facts.companyId).first<Row>();
        },
readCommerceOrders1: (facts: {
            id: string;
            fact1: Array<string | null>;
        }) => db.prepare(`SELECT o.*,p.name party_name,p.code party_code,p.notification_email,p.notification_consent,p.version party_version,w.name warehouse_name ${commerceSnapshotSources().commerce_joined_facts_commerce_orders_o_from} JOIN finance_parties p ON p.id=o.party_id ${commerceSnapshotSources().commerce_joined_facts_commerce_warehouses_w_join.replace("JOIN ","LEFT JOIN ")} ON w.id=o.warehouse_id WHERE o.id=? AND o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)`).bind(facts.id, ...facts.fact1).first<Row>(),
readCommerceOrders2: (facts: {
            fact0: Array<string | null>;
        }) => db.prepare(`SELECT o.*,p.name party_name ${commerceSnapshotSources().commerce_joined_facts_commerce_orders_o_from} JOIN finance_parties p ON p.id=o.party_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) ORDER BY o.created_at DESC LIMIT 250`).bind(...facts.fact0).all<Row>(),
readCommerceOrders3: (facts: {
            fact0: Array<string | null>;
        }) => db.prepare(`SELECT o.* ${commerceSnapshotSources().commerce_joined_facts_commerce_orders_o_from} JOIN finance_parties p ON p.id=o.party_id ${commerceSnapshotSources().commerce_joined_facts_commerce_warehouses_w_join.replace("JOIN ","LEFT JOIN ")} ON w.id=o.warehouse_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) ORDER BY o.updated_at DESC`).bind(...facts.fact0).all<Row>(),
readCommerceFulfillments1: (facts: {
            id: string;
        }) => db.prepare(`SELECT f.*,i.invoice_number,i.total_minor,i.outstanding_minor,i.status invoice_status,j.status stock_journal_status,c.base_currency stock_currency ${commerceSnapshotSources().commerce_joined_facts_commerce_fulfillments_f_from} LEFT JOIN finance_invoices i ON i.id=f.invoice_id LEFT JOIN finance_journals j ON j.id=f.journal_id LEFT JOIN finance_charts c ON c.id=j.chart_id WHERE f.order_id=? ORDER BY f.created_at DESC`).bind(facts.id).all<Row>(),
readCommerceReturns1: (facts: {
            id: string;
        }) => db.prepare(`SELECT r.*,sj.status stock_journal_status,cj.status credit_journal_status,gf.status goods_fx_journal_status,i.currency invoice_currency,c.base_currency stock_currency ${commerceSnapshotSources().commerce_joined_facts_commerce_returns_r_from} JOIN finance_invoices i ON i.id=r.invoice_id LEFT JOIN finance_journals sj ON sj.id=r.stock_journal_id LEFT JOIN finance_journals cj ON cj.id=r.credit_journal_id LEFT JOIN finance_journals gf ON gf.id=r.goods_fx_journal_id LEFT JOIN finance_charts c ON c.id=sj.chart_id WHERE r.order_id=? ORDER BY r.created_at DESC`).bind(facts.id).all<Row>(),
readCommerceFulfillments2: (facts: {
            fulfillmentId: string;
        }) => db.prepare(`SELECT f.*,i.status invoice_status,i.outstanding_minor,o.id order_id,o.kind,o.company_id,o.branch_id,o.warehouse_id,o.party_id ${commerceSnapshotSources().commerce_joined_facts_commerce_fulfillments_f_from} ${commerceSnapshotSources().commerce_joined_facts_commerce_orders_o_join} ON o.id=f.order_id JOIN finance_invoices i ON i.id=f.invoice_id WHERE f.id=?`).bind(facts.fulfillmentId).first<Row>(),
readCommerceReturns2: (facts: {
            returnId: string;
            companyId: string;
        }) => db.prepare(`SELECT r.*,o.kind order_kind,o.warehouse_id,i.debit_account_id,i.credit_account_id,i.tax_account_id,i.currency invoice_currency ${commerceSnapshotSources().commerce_joined_facts_commerce_returns_r_from} ${commerceSnapshotSources().commerce_joined_facts_commerce_orders_o_join} ON o.id=r.order_id JOIN finance_invoices i ON i.id=r.invoice_id WHERE r.id=? AND r.company_id=?`).bind(facts.returnId, facts.companyId).first<Row>(),
readCommerceFulfillments3: (facts: {
            fulfillment_id: string | number | null;
            companyId: string;
        }) => db.prepare(`SELECT j.id,j.chart_id,j.currency,j.status,c.base_currency ${commerceSnapshotSources().commerce_joined_facts_commerce_fulfillments_f_from} JOIN finance_journals j ON j.id=f.journal_id JOIN finance_charts c ON c.id=j.chart_id WHERE f.id=? AND j.company_id=?`).bind(facts.fulfillment_id, facts.companyId).first<Row>(),
readCommerceReturns3: (facts: {
            returnId: string;
            companyId: string;
        }) => db.prepare(`SELECT r.*,i.outstanding_minor,i.base_outstanding_minor,i.fx_rate_micros,i.version invoice_version,i.currency,i.party_id,i.debit_account_id,i.credit_account_id,j.chart_id,o.department_id ${commerceSnapshotSources().commerce_joined_facts_commerce_returns_r_from} JOIN finance_invoices i ON i.id=r.invoice_id JOIN finance_journals j ON j.id=i.journal_id ${commerceSnapshotSources().commerce_joined_facts_commerce_orders_o_join} ON o.id=r.order_id WHERE r.id=? AND r.company_id=?`).bind(facts.returnId, facts.companyId).first<Row>(),
guardRule4: (facts: {
            key: string;
            guardId: string;
            values: (string | number)[];
        }) => assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 ${commerceSnapshotSources().commerce_joined_facts_commerce_fulfillments_f_from} JOIN finance_invoices i ON i.id=f.invoice_id WHERE f.id=? AND f.version=? AND i.status IN ('posted','partially_paid','paid'))`, facts.values),
guardRule14: (facts: {
            key: string;
            guardId: string;
            values: (string | number | null)[];
        }) => assertion(db, facts.key, facts.guardId, `EXISTS(SELECT 1 ${commerceSnapshotSources().commerce_joined_facts_commerce_returns_r_from} LEFT JOIN finance_journals sj ON sj.id=r.stock_journal_id JOIN finance_journals cj ON cj.id=r.credit_journal_id JOIN finance_invoices i ON i.id=r.invoice_id WHERE r.id=? AND r.company_id=? AND r.status='approved' AND r.version=? AND (r.stock_journal_id IS NULL OR sj.status='posted') AND cj.status='posted' AND i.status IN ('posted','partially_paid','paid') AND i.version=?)`, facts.values),
guardRule16: (facts: {
            key: string;
            guardId: string;
            values: string[];
        }) => assertion(db, facts.key, facts.guardId, `NOT EXISTS(SELECT 1 ${commerceSnapshotSources().commerce_joined_facts_commerce_returns_r_from} WHERE r.id=? AND r.goods_fx_journal_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_journals j JOIN finance_periods p ON p.id=j.period_id WHERE j.id=r.goods_fx_journal_id AND j.company_id=r.company_id AND j.status='posted' AND p.state='open'))`, facts.values)
};}
