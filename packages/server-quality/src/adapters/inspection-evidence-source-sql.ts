import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";

/** Product/warehouse label join shared byte-for-byte by the inspection and
 * hold evidence full heads. Owned here so neither reader keeps a duplicate. */
const holdLabels = `\n    JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id`;

/** Single full-head template for the Executive Quality evidence/metric reads.
 * Callers own the projection, FROM relation and tail; spacing is emitted
 * byte-identically to the former inline copies. No read here acquires a lock. */
export const evidenceHead = (projection: string, from: string, tail: string) => `SELECT ${projection}${from}${tail}`;

/** Inspection-to-hold evidence: holds join plus product and warehouse labels. */
export const inspectionEvidenceSource = (projection: string) => evidenceHead(
  projection,
  '\n    FROM inventory_quality_inspections i\n    JOIN inventory_quality_holds h ON h.id=i.hold_id',
  holdLabels,
);

/** Hold evidence: the same product/warehouse labels on the holds base relation. */
export const holdEvidenceSource = (projection: string) => evidenceHead(
  projection,
  '\n    FROM inventory_quality_holds h',
  holdLabels,
);

/** Inspection pass-rate metric head: the same holds join with caller-owned predicates. */
export const inspectionMetricSource = (
  projection: string,
  tail: string,
) => evidenceHead(
  projection,
  '\n      FROM inventory_quality_inspections i JOIN inventory_quality_holds h ON h.id=i.hold_id',
  tail,
);
