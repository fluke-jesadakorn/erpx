import 'server-only';
import { finalRootOwnerProjectionSources as commerce } from '@erpx/server-commerce/public/final-root-owner-projection-sources';

/** One byte-identical portal binding/order read core shared by the portal
 * purchase source, the attachment purchase witness and the attachment read
 * witness. Caller-owned projection and kind ordering keep each reader's exact
 * SQL bytes while the core cannot drift. */
export const portalPurchaseBindingSource = (projection: string, kinds = "'sales','quote'") => `SELECT ${projection}
 FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN (${kinds})`;
