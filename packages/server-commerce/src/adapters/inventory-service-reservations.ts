import {commerceFieldServiceStockLock} from '@erpx/server-inventory/public/commerce-stock-owner-effects';
import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import { inventoryReadProjection } from '@erpx/server-inventory/public/inventory-read-projections';
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
/** Stock serialization and availability guards join the caller's receipt transaction. */
export function prepareFieldServiceStockReservation(db: SqlDatabase, actor: WorkContext, input: {
    key: string;
    branchId: string;
    warehouseId: string;
    productId: string;
    quantityMilli: number;
}) {
    return [
        commerceFieldServiceStockLock(db,input.warehouseId,input.productId),
        assertion(db, input.key, "available", `EXISTS(SELECT 1 FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE s.warehouse_id=? AND s.product_id=? AND w.company_id=? AND w.branch_id=? AND s.quantity_milli-${inventoryReadProjection.reservedStock}>=? AND ${inventoryReadProjection.noActiveQualityHold})`, [input.warehouseId, input.productId, actor.companyId, input.branchId, input.quantityMilli]),
    ];
}
