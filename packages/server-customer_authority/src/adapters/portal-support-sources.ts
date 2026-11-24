import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Used outside a new owned transaction only to preserve installed old-schema
 * compatibility. Existing default-disabled Portal reads do not require 1053. */
export async function portalSupportInstalled(database:SqlDatabase){return !!await database.prepare("SELECT to_regclass('customer_portal_support_responses') IS NOT NULL AND to_regclass('customer_portal_support_acknowledgements') IS NOT NULL AND to_regclass('customer_portal_support_closures') IS NOT NULL installed").first<boolean>('installed');}
/** No query before these leases. Native response/ack writers must enter a fresh
 * owned transaction and retain this same closed source chain until commit. */
export function portalSupportSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE customer_portal_support_responses,customer_portal_support_acknowledgements,customer_portal_support_closures IN SHARE ROW EXCLUSIVE MODE')];}
/** One byte-identical support-table source shared by the settlement fact map,
 * the portal support thread LATERAL and the closed-table facts readers. Each
 * caller owns its table suffix so its rendered SQL stays exactly the same while
 * the shared prefix cannot drift and only one statement literal is counted. */
const supportSource=(table:'responses'|'acknowledgements'|'closures')=>`SELECT * FROM customer_portal_support_${table}`;
export const portalSupportResponsesSource=supportSource('responses');
export const portalSupportSettlementSources={responses:portalSupportResponsesSource,acknowledgements:supportSource('acknowledgements'),closures:supportSource('closures')} as const;
