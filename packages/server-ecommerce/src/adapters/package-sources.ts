import 'server-only';
/** This identity is created only by the original cart→Sales handoff. The raw
 * cart root is retained even if its checkout pointer or receipt is malformed;
 * the Ecommerce owner counts that defect separately instead of losing it. */
export const ecommerceNativeOrderRoots=`SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart`;
export const ecommerceSettlementSources={channels:'SELECT * FROM ecommerce_channels',listings:'SELECT listing.*,channel.company_id source_company FROM ecommerce_listings listing LEFT JOIN ecommerce_channels channel ON channel.id=listing.channel_id',carts:'SELECT * FROM ecommerce_carts',cartLines:'SELECT * FROM ecommerce_cart_lines',checkouts:'SELECT checkout.*,cart.company_id source_company FROM ecommerce_checkouts checkout LEFT JOIN ecommerce_carts cart ON cart.id=checkout.cart_id',fulfillmentLinks:'SELECT * FROM ecommerce_fulfillment_links',returnLinks:'SELECT * FROM ecommerce_return_links'} as const;
