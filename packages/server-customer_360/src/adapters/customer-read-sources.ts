import "server-only";

/** Fixed customer-visibility predicate shared by the directory and timeline
 * visible-customer reads. Each caller keeps its own binding and scope; only the
 * module/permission gates are passed in. */
export function customerVisiblePredicate(
  serviceVisible: boolean,
  financeVisible: boolean,
  legalVisible: boolean,
) {
  return `(EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id) OR ${serviceVisible ? 'EXISTS(SELECT 1 FROM scoped_service g WHERE g.customer_id=p.id)' : 'FALSE'} OR ${financeVisible ? 'EXISTS(SELECT 1 FROM scoped_invoices i WHERE i.party_id=p.id)' : 'FALSE'} OR ${legalVisible ? 'EXISTS(SELECT 1 FROM scoped_legal lg WHERE lg.order_id IN (SELECT id FROM scoped_orders so WHERE so.party_id=p.id))' : 'FALSE'})`;
}
