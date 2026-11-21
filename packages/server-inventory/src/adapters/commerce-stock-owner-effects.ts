import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {run} from '@erpx/server-runtime/public/runtime/sql';
import { nativeInventoryStatement } from '@erpx/feature-inventory/adapters/server/native-inventory-command';
export function commerceCommerceStockEffects(db:SqlDatabase){return {prepareCommerceStock1: (facts: {
            fact0: string | number | null;
            fact1: string | number | null;
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => nativeInventoryStatement('inventory_stmt_9', { now: Date.now() }).bind(facts.fact0, facts.fact1, facts.warehouse_id, facts.product_id),
prepareCommerceStock2: (facts: {
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => nativeInventoryStatement('inventory_stmt_10', { now: Date.now() }).bind(facts.warehouse_id, facts.product_id),
prepareCommerceStock3: (facts: {
            fact0: number;
            fact1: number;
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => nativeInventoryStatement('inventory_stmt_9', { now: Date.now() }).bind(facts.fact0, facts.fact1, facts.warehouse_id, facts.product_id),
prepareCommerceStockMoves1: (facts: {
            fact0: string;
            warehouse_id: string | number | null;
            product_id: string | number | null;
            fulfillment: string;
            fact4: number;
            fact5: number;
            fact6: number;
        }) => nativeInventoryStatement('inventory_stmt_11', { now: Date.now() }).bind(facts.fact0, facts.warehouse_id, facts.product_id, facts.fulfillment, facts.fact4, facts.fact5, facts.fact6)};}
export function commerceInventoryConsumptionSourcesStockEffects(db:SqlDatabase){return {prepareStockConsumption(quantityMilli: number, value: number, warehouseId: string, productId: string) { return nativeInventoryStatement('inventory_stmt_12', { now: Date.now() }).bind(quantityMilli, value, warehouseId, productId); }};}
export function commerceInventoryCountsSourcesStockEffects(db:SqlDatabase){return {prepareStock(warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_10', { now: Date.now() }).bind(warehouse_id, product_id); },
prepareCountedStock(counted_quantity_milli: string | number | null, counted_value_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_13', { now: Date.now() }).bind(counted_quantity_milli, counted_value_minor, warehouse_id, product_id); }};}
export function commerceInventoryLandedCostsSourcesStockEffects(db:SqlDatabase){return {prepareLandedStockValue(allocated_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_14', { now: Date.now() }).bind(allocated_minor, warehouse_id, product_id); }};}
export function commerceInventoryTransfersSourcesStockEffects(db:SqlDatabase){return {prepareTargetStock(target_warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_10', { now: Date.now() }).bind(target_warehouse_id, product_id); },
prepareSourceStockTransfer(quantity_milli: string | number | null, value_minor: string | number | null, source_warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_12', { now: Date.now() }).bind(quantity_milli, value_minor, source_warehouse_id, product_id); },
prepareTargetStockTransfer(targetQty: number, targetValue: number, target_warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_13', { now: Date.now() }).bind(targetQty, targetValue, target_warehouse_id, product_id); }};}
export function commerceInventoryTransitSourcesStockEffects(db:SqlDatabase){return {prepareDispatchStock(quantity_milli: string | number | null, value_minor: string | number | null, source_warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_12', { now: Date.now() }).bind(quantity_milli, value_minor, source_warehouse_id, product_id); },
prepareTargetStock(target_warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_10', { now: Date.now() }).bind(target_warehouse_id, product_id); },
prepareReceiveStock(qty: number, value: number, target_warehouse_id: string | number | null, product_id: string | number | null) { return nativeInventoryStatement('inventory_stmt_9', { now: Date.now() }).bind(qty, value, target_warehouse_id, product_id); }};}
export function commercePmsCommandFactsStockEffects(db:SqlDatabase){return {updateCommerceStock: (facts: {
            q: number;
            cost: number;
            warehouseId: string;
            productId: string;
        }) => nativeInventoryStatement('inventory_stmt_12', { now: Date.now() }).bind(facts.q, facts.cost, facts.warehouseId, facts.productId).run()};}
export function commercePosCoreFactsStockEffects(db:SqlDatabase){return {writeCommerceStock1: (facts: {
            quantityMilli: number;
            value: number;
            warehouse_id: string | number | boolean | null;
            productId: string;
        }) => nativeInventoryStatement('inventory_stmt_12', { now: Date.now() }).bind(facts.quantityMilli, facts.value, facts.warehouse_id, facts.productId).run(),
writeCommerceStock2: (facts: {
            quantity_milli: string | number | boolean | null;
            cost: number;
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => nativeInventoryStatement('inventory_stmt_9', { now: Date.now() }).bind(facts.quantity_milli, facts.cost, facts.warehouse_id, facts.product_id).run(),
writeCommerceStock4: (facts: {
            quantityMilli: number;
            restoredCost: number;
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => nativeInventoryStatement('inventory_stmt_9', { now: Date.now() }).bind(facts.quantityMilli, facts.restoredCost, facts.warehouse_id, facts.product_id).run()};}
export function commerceProductionCommandSourcesStockEffects(db:SqlDatabase){return {prepareStockIssue(amount: number, value: number, warehouse_id: string | number | null, product_id: string | number | null) {
            return nativeInventoryStatement('inventory_stmt_12', { now: Date.now() })
                .bind(amount, value, warehouse_id, product_id);
        },
prepareOutputStock(warehouse_id: string | number | null, output_product_id: string | number | null, output: number, total: number) {
            return nativeInventoryStatement('inventory_stmt_15', { now: Date.now() })
                .bind(warehouse_id, output_product_id, output, total);
        }};}
