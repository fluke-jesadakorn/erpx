import type { SqlDatabase } from '../../runtime/public/runtime/types';
import { assertion } from '../../workflow/public/commands';
export function inventoryReorderSources(db: SqlDatabase) {
    return { async activeWarehouse(warehouseId: string, companyId: string, branchId: string | null, branchId2: string | null) {
            return db.prepare("SELECT id,branch_id,version FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)")
                .bind(warehouseId, companyId, branchId, branchId2).first<{
                id: string;
                branch_id: string;
                version: number;
            }>();
        },
        async activeProduct(productId: string, companyId: string) {
            return db.prepare("SELECT id,version FROM commerce_products WHERE id=? AND company_id=? AND status='active'")
                .bind(productId, companyId).first<{
                id: string;
                version: number;
            }>();
        },
        guardWarehouseVersion(key: string, value: "warehouse-version", id: string, companyId: string, version: number, branchId: string | null, branchId2: string | null) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND version=? AND status='active' AND (?::text IS NULL OR branch_id=?))", [id, companyId, version, branchId, branchId2]); },
        guardProductVersion(key: string, value: "product-version", id: string, companyId: string, version: number) { return assertion(db, key, value, "EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND version=? AND status='active')", [id, companyId, version]); } };
}
