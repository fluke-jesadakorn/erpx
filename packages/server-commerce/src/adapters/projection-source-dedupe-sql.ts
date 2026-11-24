import { commercialOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/commercial-owner-projection-sources";
import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";

/** Byte-identical Commerce projection source text shared by the closed owner
 * projection contracts. Text only: every consumer composes and executes its
 * own statement in its supplied transaction and no result is cached here. */
export const COMMERCE_FULFILLMENTS_SQL = 'SELECT "id","order_id","number","fulfillment_date","created_by","created_at","journal_id","invoice_id","version","evidence_note" FROM commerce_fulfillments';
export const COMMERCE_ORDERS_SQL = 'SELECT "id","company_id","branch_id","department_id","kind","number","party_id","warehouse_id","currency","order_date","due_date","note","state","subtotal_minor","tax_minor","source_id","version","created_by","approved_by","created_at","updated_at","price_list_id","price_list_version","discount_reason","discount_approval_required","discount_approved_by","discount_approved_at" FROM commerce_orders';
export const COMMERCE_PRODUCTS_SQL = 'SELECT "id","company_id","sku","name","unit","price_minor","tax_bps","status","version","tracking_mode" FROM commerce_products';
export const COMMERCE_RETURNS_SQL = 'SELECT "id","company_id","branch_id","order_id","fulfillment_id","invoice_id","kind","number","return_date","reason","status","subtotal_minor","tax_minor","total_minor","stock_journal_id","credit_journal_id","version","requested_by","approved_by","created_at","updated_at","credited_minor","refund_minor","base_credit_minor","base_credited_minor","base_refund_minor","fx_journal_id","goods_fx_journal_id" FROM commerce_returns';
export const COMMERCE_SETTINGS_SQL = 'SELECT "company_id","chart_id","currency","receivable_account_id","payable_account_id","revenue_account_id","inventory_account_id","clearing_account_id","cogs_account_id","input_tax_account_id","output_tax_account_id","cash_account_id" FROM commerce_settings';
export const COMMERCE_WAREHOUSES_SQL = 'SELECT "id","company_id","branch_id","code","name","status","version" FROM commerce_warehouses';
export const COMMERCE_FULFILLMENT_LINES_SQL = 'SELECT "id","fulfillment_id","order_line_id","quantity_milli","value_minor","subtotal_minor","tax_minor","stock_currency" FROM commerce_fulfillment_lines';
export const COMMERCE_BRANCH_LABEL_SQL = `COALESCE((SELECT n.display_name FROM (${organizationProjectionSources.organization_nodes}) n WHERE n.id=o.branch_id AND n.company_id=o.company_id),'Company-wide')`;
/** Byte-identical flow parent lookup shared by the prepare-command and
 * assign-primary call sites. Text only: each call site still prepares and
 * executes its own statement in its supplied transaction; no result cached. */
export const COMMERCE_FLOW_PARENT_SQL = `SELECT f.order_id FROM commerce_fulfillments f JOIN (${financeProjectionSources.finance_journals}) j ON j.id=f.journal_id WHERE j.id=? AND j.company_id=?`;
