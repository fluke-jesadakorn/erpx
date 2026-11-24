import { commercialOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/commercial-owner-projection-sources";

/** Inspection-to-hold evidence/metric head shared by the Executive Quality
 * readers. The projection, FROM indentation, JOIN separator and trailing
 * clauses are fixed owner-owned fragments supplied by callers; the head and
 * spacing are emitted byte-identically to the former inline copies. No read
 * here acquires a lock. */
const inspectionEvidenceHead = (
  projection: string,
  from: string,
  separator: string,
  tail: string,
) => `SELECT ${projection}${from}inventory_quality_inspections i${separator}JOIN inventory_quality_holds h ON h.id=i.hold_id${tail}`;

/** Inspection evidence: holds join plus product and warehouse labels. */
export const inspectionEvidenceSource = (projection: string) => inspectionEvidenceHead(
  projection,
  '\n    FROM ',
  '\n    ',
  `\n    JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=h.product_id AND p.company_id=h.company_id
    JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=h.warehouse_id AND w.company_id=h.company_id`,
);

/** Inspection pass-rate metric head: the same holds join with caller-owned predicates. */
export const inspectionMetricSource = (
  projection: string,
  tail: string,
) => inspectionEvidenceHead(projection, '\n      FROM ', ' ', tail);
