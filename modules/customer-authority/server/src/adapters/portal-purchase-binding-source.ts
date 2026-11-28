import 'server-only';
import { finalRootOwnerProjectionSources as commerce } from '@erpx/commerce-server/public/final-root-owner-projection-sources';
import {portalBindingOrderCore} from './customer-authority-read-tail-r44-sources';

/** One byte-identical portal binding/order read core shared by the portal
 * purchase source, the attachment purchase witness and the attachment read
 * witness. Caller-owned projection and kind ordering keep each reader's exact
 * SQL bytes while the core now comes from the r44 universal builder. */
export const portalPurchaseBindingSource = (projection: string, kinds = "'sales','quote'") => portalBindingOrderCore(
  projection, '\n ', ' ', commerce.commerce_orders, ` AND o.kind IN (${kinds})`,
);
