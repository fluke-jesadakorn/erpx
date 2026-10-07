import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {run} from '../../pos/public/pos-core';
export function commerceCommerceStockEffects(db:SqlDatabase){return {prepareCommerceStock1: (facts: {
            fact0: string | number | null;
            fact1: string | number | null;
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => db.prepare("UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?").bind(facts.fact0, facts.fact1, facts.warehouse_id, facts.product_id),
prepareCommerceStock2: (facts: {
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id) VALUES(?,?)").bind(facts.warehouse_id, facts.product_id),
prepareCommerceStock3: (facts: {
            fact0: number;
            fact1: number;
            warehouse_id: string | number | null;
            product_id: string | number | null;
        }) => db.prepare("UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?").bind(facts.fact0, facts.fact1, facts.warehouse_id, facts.product_id),
prepareCommerceStockMoves1: (facts: {
            fact0: string;
            warehouse_id: string | number | null;
            product_id: string | number | null;
            fulfillment: string;
            fact4: number;
            fact5: number;
            fact6: number;
        }) => db.prepare("INSERT INTO commerce_stock_moves VALUES(?,?,?,?,?,?,?)").bind(facts.fact0, facts.warehouse_id, facts.product_id, facts.fulfillment, facts.fact4, facts.fact5, facts.fact6)};}
export function commerceInventoryConsumptionSourcesStockEffects(db:SqlDatabase){return {prepareStockConsumption(quantityMilli: number, value: number, warehouseId: string, productId: string) { return db.prepare('UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?').bind(quantityMilli, value, warehouseId, productId); }};}
export function commerceInventoryCountsSourcesStockEffects(db:SqlDatabase){return {prepareStock(warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id) VALUES(?,?)").bind(warehouse_id, product_id); },
prepareCountedStock(counted_quantity_milli: string | number | null, counted_value_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare("UPDATE commerce_stock SET quantity_milli=?,value_minor=?,version=version+1 WHERE warehouse_id=? AND product_id=?").bind(counted_quantity_milli, counted_value_minor, warehouse_id, product_id); }};}
export function commerceInventoryLandedCostsSourcesStockEffects(db:SqlDatabase){return {prepareLandedStockValue(allocated_minor: string | number | null, warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare('UPDATE commerce_stock SET value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?').bind(allocated_minor, warehouse_id, product_id); }};}
export function commerceInventoryTransfersSourcesStockEffects(db:SqlDatabase){return {prepareTargetStock(target_warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id) VALUES(?,?)").bind(target_warehouse_id, product_id); },
prepareSourceStockTransfer(quantity_milli: string | number | null, value_minor: string | number | null, source_warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare("UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?").bind(quantity_milli, value_minor, source_warehouse_id, product_id); },
prepareTargetStockTransfer(targetQty: number, targetValue: number, target_warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare("UPDATE commerce_stock SET quantity_milli=?,value_minor=?,version=version+1 WHERE warehouse_id=? AND product_id=?").bind(targetQty, targetValue, target_warehouse_id, product_id); }};}
export function commerceInventoryTransitSourcesStockEffects(db:SqlDatabase){return {prepareDispatchStock(quantity_milli: string | number | null, value_minor: string | number | null, source_warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare('UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?').bind(quantity_milli, value_minor, source_warehouse_id, product_id); },
prepareTargetStock(target_warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare('INSERT INTO commerce_stock(warehouse_id,product_id) VALUES(?,?)').bind(target_warehouse_id, product_id); },
prepareReceiveStock(qty: number, value: number, target_warehouse_id: string | number | null, product_id: string | number | null) { return db.prepare('UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?').bind(qty, value, target_warehouse_id, product_id); }};}
export function commercePmsCommandFactsStockEffects(db:SqlDatabase){return {updateCommerceStock: (facts: {
            q: number;
            cost: number;
            warehouseId: string;
            productId: string;
        }) => run(db, "UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?", facts.q, facts.cost, facts.warehouseId, facts.productId)};}
export function commercePosCoreFactsStockEffects(db:SqlDatabase){return {writeCommerceStock1: (facts: {
            quantityMilli: number;
            value: number;
            warehouse_id: string | number | boolean | null;
            productId: string;
        }) => run(db, 'UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?', facts.quantityMilli, facts.value, facts.warehouse_id, facts.productId),
writeCommerceStock2: (facts: {
            quantity_milli: string | number | boolean | null;
            cost: number;
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => run(db, 'UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?', facts.quantity_milli, facts.cost, facts.warehouse_id, facts.product_id),
writeCommerceStock4: (facts: {
            quantityMilli: number;
            restoredCost: number;
            warehouse_id: string | number | boolean | null;
            product_id: string | number | boolean | null;
        }) => run(db, 'UPDATE commerce_stock SET quantity_milli=quantity_milli+?,value_minor=value_minor+?,version=version+1 WHERE warehouse_id=? AND product_id=?', facts.quantityMilli, facts.restoredCost, facts.warehouse_id, facts.product_id)};}
export function commerceProductionCommandSourcesStockEffects(db:SqlDatabase){return {prepareStockIssue(amount: number, value: number, warehouse_id: string | number | null, product_id: string | number | null) {
            return db
                .prepare("UPDATE commerce_stock SET quantity_milli=quantity_milli-?,value_minor=value_minor-?,version=version+1 WHERE warehouse_id=? AND product_id=?")
                .bind(amount, value, warehouse_id, product_id);
        },
prepareOutputStock(warehouse_id: string | number | null, output_product_id: string | number | null, output: number, total: number) {
            return db
                .prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,?,?,1) ON CONFLICT(warehouse_id,product_id) DO UPDATE SET quantity_milli=commerce_stock.quantity_milli+excluded.quantity_milli,value_minor=commerce_stock.value_minor+excluded.value_minor,version=commerce_stock.version+1")
                .bind(warehouse_id, output_product_id, output, total);
        }};}
export function commerceFieldServiceStockLock(db:SqlDatabase,warehouseId:string,productId:string){return db.prepare("UPDATE commerce_stock SET version=version+1 WHERE warehouse_id=? AND product_id=?").bind(warehouseId,productId);}
