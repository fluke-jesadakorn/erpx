import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
/** Local writer leases contain no query and precede the complete handoff's
 * first snapshot. SRE also avoids cross-Company SHARE-to-writer upgrades. */
export function ecommerceCommerceSourceReadLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE')];}
