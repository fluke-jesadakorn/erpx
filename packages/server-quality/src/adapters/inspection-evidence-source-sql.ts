import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";

/** Inspection-to-hold evidence projection shared by the Executive Quality reads. */
export const inspectionEvidenceSource = (projection: string) => `SELECT ${projection}
    FROM inventory_quality_inspections i
    JOIN inventory_quality_holds h ON h.id=i.hold_id
    JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id`;
