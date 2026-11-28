import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {procurementCommerceSourceReadLocks} from "@erpx/procurement-server/public/commerce-source-locks";
import {commerceOwnerInventorySourceReadLocks} from "@erpx/inventory-server/public/commerce-owner-sources";
import {commerceOwnerQualitySourceReadLocks} from "@erpx/quality-server/public/commerce-owner-sources";
import {commerceOwnerRequestSourceReadLocks} from "@erpx/records-server/public/commerce-owner-sources";
import {commerceOwnerFinanceSourceReadLocks} from "@erpx/finance-server/public/commerce-owner-sources";
import {commerceOwnerPricingSourceReadLocks} from "@erpx/sales-server/public/commerce-owner-sources";

/** Writer leases prevent a later SHARE-to-writer upgrade between Companies.
 * These statements contain no SELECT and establish no SERIALIZABLE snapshot.
 * Shared product/warehouse setup remains an original owner reference. */
export function commerceOwnerSourceReadLocks(database:SqlDatabase){return [
 database.prepare("LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves IN SHARE ROW EXCLUSIVE MODE"),
 database.prepare("LOCK TABLE commerce_settings,commerce_products,commerce_warehouses IN SHARE MODE")
];}

/** Both complete Commerce and Procurement units acquire this closed chain
 * after the global package writer fence, before Company advisory SELECTs,
 * receipts, IAM checks or business reads. Owner ports declare their own tables;
 * this grants neither another package nor another owner's command authority. */
export function commerceCompleteOwnerSourceReadLocks(database:SqlDatabase){return [
 ...commerceOwnerSourceReadLocks(database),
 ...procurementCommerceSourceReadLocks(database),
 ...commerceOwnerInventorySourceReadLocks(database),
 ...commerceOwnerQualitySourceReadLocks(database),
 ...commerceOwnerRequestSourceReadLocks(database),
 ...commerceOwnerFinanceSourceReadLocks(database),
 ...commerceOwnerPricingSourceReadLocks(database)
];}
