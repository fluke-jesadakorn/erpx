import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Raw native identities for Sales withdrawal. Invalid same-Company lineage is
 * retained for the owner to count, rather than disappearing in valid-only joins. */
export const salesCommerceSettlementSources={
 orders:"SELECT * FROM commerce_orders WHERE kind IN ('quote','sales')",
 lines:"SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE parent.kind IN ('quote','sales')",
 deliveries:"SELECT delivery.*,parent.company_id,parent.branch_id,parent.department_id,parent.party_id,parent.kind order_kind,parent.currency order_currency,parent.warehouse_id FROM commerce_fulfillments delivery JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales'",
 deliveryLines:"SELECT line.*,delivery.order_id,original.product_id FROM commerce_fulfillment_lines line JOIN commerce_fulfillments delivery ON delivery.id=line.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id LEFT JOIN commerce_order_lines original ON original.id=line.order_line_id WHERE parent.kind='sales'",
 returns:"SELECT returned.*,parent.company_id source_company,parent.branch_id source_branch,parent.department_id source_department,parent.kind order_kind,parent.party_id,parent.warehouse_id,parent.currency order_currency FROM commerce_returns returned LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales'",
 returnLines:"SELECT line.* FROM commerce_return_lines line JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales'",
 returnLots:"SELECT allocation.* FROM commerce_return_lot_allocations allocation JOIN commerce_return_lines line ON line.id=allocation.return_line_id JOIN commerce_returns returned ON returned.id=line.return_id LEFT JOIN commerce_orders parent ON parent.id=returned.order_id WHERE returned.kind='customer_return' OR parent.kind='sales'",
 goodsValuations:"SELECT valuation.*,parent.company_id source_company FROM commerce_goods_valuations valuation JOIN commerce_fulfillments delivery ON delivery.id=valuation.fulfillment_id JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales'",
 invoiceCarrying:"SELECT carrying.*,parent.company_id source_company FROM commerce_invoice_goods_carrying carrying JOIN commerce_fulfillments delivery ON delivery.invoice_id=carrying.invoice_id JOIN commerce_orders parent ON parent.id=delivery.order_id WHERE parent.kind='sales'",
 warehouses:"SELECT id,company_id,branch_id,status FROM commerce_warehouses",
 products:"SELECT id,company_id,item_type,tracking_mode,status FROM commerce_products"
 ,stock:'SELECT * FROM commerce_stock'
 ,stockMoves:'SELECT * FROM commerce_stock_moves'
 ,returnStockMoves:'SELECT * FROM commerce_return_stock_moves'
} as const;
export function salesCommerceSettlementSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves,commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE')];}
