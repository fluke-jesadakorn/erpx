/** Closed, byte-identical parent-existence guard shared by the Commerce
 * attachment row sources. Text only: each rows() call still composes its own
 * fragment and the single outer prepare/execute is unchanged. The parent/child
 * columns are closed owner-local choices, so every rendered string matches the
 * retired literal byte-for-byte. */
export const fulfillmentParentGuardSql = (parent: 'id' | 'invoice_id', child: 'fulfillment_id' | 'invoice_id') =>
  `EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.${parent}=child.${child} AND f.order_id=o.id)`;
