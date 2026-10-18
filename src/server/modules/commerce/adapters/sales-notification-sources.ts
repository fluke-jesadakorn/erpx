import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Closed immutable dispatch identity; this declaration does not grant Sales
 * access to shared parties, product catalogues or another product's delivery. */
export const salesNotificationDeliverySources=`SELECT f.id fulfillment_id,f.version fulfillment_version,o.id order_id,o.company_id,o.branch_id,o.department_id,o.kind order_kind,o.state order_state,o.currency,o.party_id FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id AND o.kind='sales'`;
export function salesNotificationDeliverySourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_products IN SHARE MODE')];}
