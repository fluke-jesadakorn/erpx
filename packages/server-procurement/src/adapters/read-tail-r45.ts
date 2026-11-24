/** Procurement read-tail r45 shared fragment. Text only: the outer frame
 * is shared while each caller keeps its verbatim ARRAY subquery fragments,
 * `?` placeholder positions and `.bind(id,companyId)` order. Reads, ordering,
 * locks and writes are unchanged. r17-r44 helpers stay untouched. */
export const procurementAttachmentReadOuterFrame = (
  table: string,
  image: string,
  warehouseId: string,
  productIds: string,
  supplierIds: string,
  invoiceId: string,
  receiptIds: string,
) =>
  `SELECT ${image}::text image,${warehouseId},${productIds},${supplierIds},r.order_id,${invoiceId},${receiptIds} FROM ${table} r WHERE r.id=? AND r.company_id=?`;
