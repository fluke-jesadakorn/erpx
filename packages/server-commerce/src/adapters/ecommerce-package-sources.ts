import 'server-only';
import {ecommerceNativeOrderRoots} from '@erpx/server-ecommerce/public/package-sources';
import {salesCommerceSettlementSources as sales} from '@erpx/server-commerce/adapters/sales-package-sources';
const orderIds=`SELECT native.id FROM (${sales.orders}) native JOIN (${ecommerceNativeOrderRoots}) root ON root.order_id=native.id AND root.company_id=native.company_id`;
const deliveries=`SELECT native.* FROM (${sales.deliveries}) native WHERE native.order_id IN (${orderIds})`;
const returns=`SELECT native.* FROM (${sales.returns}) native WHERE native.order_id IN (${orderIds})`;
/** Closed subset of the native Sales owner, never a caller-selected kind,
 * Company label or arbitrary SQL source. Unrelated Sales remains independent. */
export const ecommerceCommerceSettlementSources={...sales,
 orders:`SELECT native.* FROM (${sales.orders}) native WHERE native.id IN (${orderIds})`,
 lines:`SELECT native.* FROM (${sales.lines}) native WHERE native.order_id IN (${orderIds})`,deliveries,
 deliveryLines:`SELECT native.* FROM (${sales.deliveryLines}) native WHERE native.order_id IN (${orderIds})`,returns,
 returnLines:`SELECT native.* FROM (${sales.returnLines}) native WHERE native.return_id IN (SELECT id FROM (${returns}) source)`,
 returnLots:`SELECT native.* FROM (${sales.returnLots}) native WHERE native.return_line_id IN (SELECT id FROM (${sales.returnLines}) line WHERE line.return_id IN (SELECT id FROM (${returns}) source))`,
 goodsValuations:`SELECT native.* FROM (${sales.goodsValuations}) native WHERE native.fulfillment_id IN (SELECT id FROM (${deliveries}) source)`,
 invoiceCarrying:`SELECT native.* FROM (${sales.invoiceCarrying}) native WHERE native.invoice_id IN (SELECT invoice_id FROM (${deliveries}) source)`
} as const;
