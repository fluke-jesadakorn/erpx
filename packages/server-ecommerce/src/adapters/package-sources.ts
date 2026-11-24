import 'server-only';
/** This identity is created only by the original cart→Sales handoff. The raw
 * cart root is retained even if its checkout pointer or receipt is malformed;
 * the Ecommerce owner counts that defect separately instead of losing it. */
export const ecommerceNativeOrderRoots=`SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart`;
