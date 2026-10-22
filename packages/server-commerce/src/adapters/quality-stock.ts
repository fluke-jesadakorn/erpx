import { commercialOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/commercial-owner-projection-sources";
import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
type Row = Record<string, string | number | null>;
export function qualityStockFacts(write: SqlDatabase, actor: Pick<WorkContext, "companyId" | "branchId">, input: {
    warehouseId: string;
    productId: string;
}) { return write.prepare(`SELECT s.*,w.branch_id FROM (${inventoryProjectionSources.commerce_stock}) s JOIN commerce_warehouses w ON w.id=s.warehouse_id JOIN commerce_products p ON p.id=s.product_id WHERE s.warehouse_id=? AND s.product_id=? AND w.company_id=? AND p.company_id=w.company_id AND w.status='active' AND p.status='active' AND (?::text IS NULL OR w.branch_id=?)`).bind(input.warehouseId, input.productId, actor.companyId, actor.branchId, actor.branchId).first<Row>(); }
