import { inventoryExecutiveOwnerProjections as commerceProjectionSources } from "../../commerce/public/inventory-executive-owner-projections";
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { AvailableStockEvidenceRow, InventoryShortageEvidenceRow, InventoryMovementEvidenceRow } from '@/server/modules/runtime/public/executive/evidence-contract';
const availableStockPositions = `WITH reservations AS (
    SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    s.quantity_milli::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    s.version stock_version
  FROM commerce_stock s JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id AND w.company_id=? AND w.status='active'
  JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.product_id AND p.company_id=w.company_id AND p.status='active'
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE (?::text IS NULL OR w.branch_id=?)`;
const inventoryShortagePositions = `WITH reservations AS (
    SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    COALESCE(s.quantity_milli,0)::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(COALESCE(s.quantity_milli,0)::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    policy.current_reorder_point_milli::text approved_reorder_point_milli,policy.version policy_version,COALESCE(s.version,0) stock_version
  FROM inventory_reorder_policies policy
  JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=policy.warehouse_id AND w.company_id=policy.company_id AND w.status='active'
  JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=policy.product_id AND p.company_id=policy.company_id AND p.status='active'
  LEFT JOIN commerce_stock s ON s.warehouse_id=w.id AND s.product_id=p.id
  LEFT JOIN reservations r ON r.warehouse_id=w.id AND r.product_id=p.id
  WHERE policy.company_id=? AND policy.current_reorder_point_milli IS NOT NULL AND policy.retired_at IS NULL
    AND (?::text IS NULL OR w.branch_id=?)`;
const stockedLocationPositions = `WITH reservations AS (
    SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli
    FROM inventory_active_reservations GROUP BY warehouse_id,product_id
  )
  SELECT w.id warehouse_id,p.id product_id,p.sku,p.name product_name,p.unit,w.name warehouse_name,
    s.quantity_milli::text on_hand_milli,COALESCE(r.quantity_milli,0)::text reserved_milli,
    inventory_quarantined_quantity(w.id,p.id)::text quarantined_milli,
    GREATEST(s.quantity_milli::numeric-GREATEST(COALESCE(r.quantity_milli,0),inventory_quarantined_quantity(w.id,p.id)::numeric),0)::text available_milli,
    s.version stock_version
  FROM commerce_stock s JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=s.warehouse_id AND w.company_id=?
  JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=s.product_id AND p.company_id=w.company_id
  LEFT JOIN reservations r ON r.warehouse_id=s.warehouse_id AND r.product_id=s.product_id
  WHERE s.quantity_milli>0`;
export function executiveMainEvidenceSources(db: SqlDatabase) {
    return { async factsSearchAvailableStockEvidence(input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "available-stock";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            warehouseId: string;
            productId: string;
        } | null): Promise<{
            rows: AvailableStockEvidenceRow[];
        }> {
            const conditions = ["available_milli::numeric>0"];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(sku))>0 OR position(lower(?) in lower(product_name))>0 OR position(lower(?) in lower(warehouse_name))>0)");
                values.push(input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(warehouse_id,product_id)>(?,?)");
                values.push(cursor.warehouseId, cursor.productId);
            }
            const rows = (await db.prepare(`SELECT * FROM (${availableStockPositions}) stock WHERE ${conditions.join(" AND ")} ORDER BY warehouse_id,product_id LIMIT ?`)
                .bind(...values, input.limit + 1).all<AvailableStockEvidenceRow>()).results;
            return { rows };
        }, async factsReadAvailableStockEvidence(actor: WorkContext, ids: {
            warehouseId: string;
            productId: string;
        }): Promise<{
            row: AvailableStockEvidenceRow | null;
        }> {
            const row = await db.prepare(`SELECT * FROM (${availableStockPositions}) stock WHERE available_milli::numeric>0 AND warehouse_id=? AND product_id=?`)
                .bind(actor.companyId, actor.branchId, actor.branchId, ids.warehouseId, ids.productId).first<AvailableStockEvidenceRow>();
            return { row };
        }, async factsSearchInventoryShortageEvidence(input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "inventory-shortages";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            warehouseId: string;
            productId: string;
        } | null): Promise<{
            rows: InventoryShortageEvidenceRow[];
        }> {
            const conditions = ["available_milli::numeric<approved_reorder_point_milli::numeric"];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(sku))>0 OR position(lower(?) in lower(product_name))>0 OR position(lower(?) in lower(warehouse_name))>0)");
                values.push(input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(warehouse_id,product_id)>(?,?)");
                values.push(cursor.warehouseId, cursor.productId);
            }
            const rows = (await db.prepare(`SELECT * FROM (${inventoryShortagePositions}) shortages WHERE ${conditions.join(" AND ")} ORDER BY warehouse_id,product_id LIMIT ?`)
                .bind(...values, input.limit + 1).all<InventoryShortageEvidenceRow>()).results;
            return { rows };
        }, async factsReadInventoryShortageEvidence(actor: WorkContext, ids: {
            warehouseId: string;
            productId: string;
        }): Promise<{
            row: InventoryShortageEvidenceRow | null;
        }> {
            const row = await db.prepare(`SELECT * FROM (${inventoryShortagePositions}) shortages
    WHERE available_milli::numeric<approved_reorder_point_milli::numeric AND warehouse_id=? AND product_id=?`)
                .bind(actor.companyId, actor.branchId, actor.branchId, ids.warehouseId, ids.productId).first<InventoryShortageEvidenceRow>();
            return { row };
        }, async factsSearchStockedLocationEvidence(input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "inventory";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            warehouseId: string;
            productId: string;
        } | null): Promise<{
            rows: AvailableStockEvidenceRow[];
        }> {
            const conditions = ["on_hand_milli::numeric>0"];
            const values: unknown[] = [actor.companyId];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(sku))>0 OR position(lower(?) in lower(product_name))>0 OR position(lower(?) in lower(warehouse_name))>0)");
                values.push(input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(warehouse_id,product_id)>(?,?)");
                values.push(cursor.warehouseId, cursor.productId);
            }
            const rows = (await db.prepare(`SELECT * FROM (${stockedLocationPositions}) stock WHERE ${conditions.join(" AND ")} ORDER BY warehouse_id,product_id LIMIT ?`)
                .bind(...values, input.limit + 1).all<AvailableStockEvidenceRow>()).results;
            return { rows };
        }, async factsReadStockedLocationEvidence(actor: WorkContext, ids: {
            warehouseId: string;
            productId: string;
        }): Promise<{
            row: AvailableStockEvidenceRow | null;
        }> {
            const row = await db.prepare(`SELECT * FROM (${stockedLocationPositions}) stock WHERE on_hand_milli::numeric>0 AND warehouse_id=? AND product_id=?`)
                .bind(actor.companyId, ids.warehouseId, ids.productId).first<AvailableStockEvidenceRow>();
            return { row };
        }, async factsSearchInventoryMovementEvidence(input: {
            filters: {
                from: string;
                to: string;
                currency: string;
            };
            query: string;
            limit: number;
            metric: "inventory-movements";
            cursor?: string | undefined;
        }, actor: WorkContext, cursor: {
            movementDate: string;
            id: string;
        } | null): Promise<{
            rows: InventoryMovementEvidenceRow[];
        }> {
            const conditions = ["w.company_id=?", "(?::text IS NULL OR w.branch_id=?)", "l.movement_date BETWEEN ? AND ?", "l.quantity_milli<>0"];
            const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, input.filters.from, input.filters.to];
            if (input.query) {
                conditions.push("(position(lower(?) in lower(p.sku))>0 OR position(lower(?) in lower(p.name))>0 OR position(lower(?) in lower(w.name))>0 OR position(lower(?) in lower(l.source_number))>0 OR position(lower(?) in lower(l.source_type))>0)");
                values.push(input.query, input.query, input.query, input.query, input.query);
            }
            if (cursor) {
                conditions.push("(l.movement_date,l.id)<(?,?)");
                values.push(cursor.movementDate, cursor.id);
            }
            const rows = (await db.prepare(`SELECT l.id,l.source_type,l.source_id,l.source_number,l.movement_date,l.quantity_milli::text quantity_milli,p.unit,p.sku,p.name product_name,w.id warehouse_id,w.name warehouse_name
    FROM inventory_ledger l JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=l.warehouse_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=l.product_id AND p.company_id=w.company_id
    WHERE ${conditions.join(" AND ")} ORDER BY l.movement_date DESC,l.id DESC LIMIT ?`).bind(...values, input.limit + 1).all<InventoryMovementEvidenceRow>()).results;
            return { rows };
        }, async factsReadInventoryMovementEvidence(actor: WorkContext, filters: {
            from: string;
            to: string;
            currency: string;
        }, ids: {
            movementId: string;
        }): Promise<{
            row: InventoryMovementEvidenceRow | null;
        }> {
            const row = await db.prepare(`SELECT l.id,l.source_type,l.source_id,l.source_number,l.movement_date,l.quantity_milli::text quantity_milli,p.unit,p.sku,p.name product_name,w.id warehouse_id,w.name warehouse_name
    FROM inventory_ledger l JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=l.warehouse_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=l.product_id AND p.company_id=w.company_id
    WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?) AND l.movement_date BETWEEN ? AND ? AND l.quantity_milli<>0 AND l.id=?`)
                .bind(actor.companyId, actor.branchId, actor.branchId, filters.from, filters.to, ids.movementId).first<InventoryMovementEvidenceRow>();
            return { row };
        } };
}
