/** Procurement read-tail r40 shared fragment. Text only: the builder renders the
 * exact original statement prefix so binds, ordering, locks and writes are
 * unchanged. r17-r39 helpers, FOR UPDATE/FOR SHARE locks and command_receipts
 * stay untouched. */
/** Supplier-bill match projection shared by the point read and workspace list.
 * The caller supplies the column tail, the two owner projection sources and the
 * trailing JOIN/WHERE fragment verbatim; no bind, lock or predicate is added. */
export const billMatchProjection = (
  columns: string,
  parties: string,
  orders: string,
  tail: string,
) =>
  `    SELECT m.*,p.name supplier_name,o.number order_number,${columns}
    FROM procurement_bill_matches m
    JOIN (${parties}) p ON p.id=m.supplier_id
    JOIN (${orders}) o ON o.id=m.order_id${tail}`;
