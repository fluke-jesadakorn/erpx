import "server-only";
import { commerceOrdersReadSql } from "./read-tail-r21-dedupe-sql";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import { COMMERCE_WAREHOUSE_IDENTITY_SQL } from "./guarded-source-dedupe-sql";
import { commerceSettlementLineSql } from "./read-tail-r25-dedupe-sql";
/** Raw native purchasing lineage for Supply withdrawal and its own source
 * witnesses. A source label does not grant access to a foreign Sales order. */
export const supplyCommerceSettlementSources={
 orders:commerceOrdersReadSql('*', "kind='purchase'"),
 goodsValuations:"SELECT * FROM commerce_goods_valuations",
 invoiceCarrying:"SELECT * FROM commerce_invoice_goods_carrying",
 lines:commerceSettlementLineSql("parent.kind='purchase'"),
 receipts:"SELECT receipt.* FROM commerce_fulfillments receipt JOIN commerce_orders parent ON parent.id=receipt.order_id WHERE parent.kind='purchase'",
 receiptLines:"SELECT line.* FROM commerce_fulfillment_lines line JOIN commerce_fulfillments receipt ON receipt.id=line.fulfillment_id JOIN commerce_orders parent ON parent.id=receipt.order_id WHERE parent.kind='purchase'",
 returns:"SELECT returned.* FROM commerce_returns returned JOIN commerce_orders parent ON parent.id=returned.order_id AND parent.company_id=returned.company_id WHERE parent.kind='purchase' AND returned.kind='supplier_return'",
 returnLines:"SELECT line.* FROM commerce_return_lines line JOIN commerce_returns returned ON returned.id=line.return_id JOIN commerce_orders parent ON parent.id=returned.order_id AND parent.company_id=returned.company_id WHERE parent.kind='purchase' AND returned.kind='supplier_return'",
 warehouses:COMMERCE_WAREHOUSE_IDENTITY_SQL,
 products:"SELECT id,company_id,status,item_type,tracking_mode FROM commerce_products",
} as const;
export function supplyCommerceSettlementSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_returns,commerce_return_lines,commerce_warehouses,commerce_products,commerce_goods_valuations,commerce_invoice_goods_carrying IN SHARE MODE")];}
