import 'server-only';
import {ecommerceNativeOrderRoots} from '@erpx/server-ecommerce/public/package-sources';
import {salesCommerceSettlementSources as sales} from '@erpx/server-commerce/adapters/sales-package-sources';
import {COMMERCE_SETTLEMENT_BY_ORDER_SQL} from './read-tail-r15-dedupe-sql';
const orderIds=`SELECT native.id FROM (${sales.orders}) native JOIN (${ecommerceNativeOrderRoots}) root ON root.order_id=native.id AND root.company_id=native.company_id`;
const deliveries=COMMERCE_SETTLEMENT_BY_ORDER_SQL(sales.deliveries,orderIds);
const returns=COMMERCE_SETTLEMENT_BY_ORDER_SQL(sales.returns,orderIds);
/** Closed subset of the native Sales owner, never a caller-selected kind,
 * Company label or arbitrary SQL source. Unrelated Sales remains independent. */
export const ecommerceCommerceSettlementSources={...sales,
 orders:COMMERCE_SETTLEMENT_BY_ORDER_SQL(sales.orders,orderIds,'id'),
 lines:COMMERCE_SETTLEMENT_BY_ORDER_SQL(sales.lines,orderIds),deliveries,
 deliveryLines:COMMERCE_SETTLEMENT_BY_ORDER_SQL(sales.deliveryLines,orderIds),returns,
 returnLines:`SELECT native.* FROM (${sales.returnLines}) native WHERE native.return_id IN (SELECT id FROM (${returns}) source)`,
 returnLots:`SELECT native.* FROM (${sales.returnLots}) native WHERE native.return_line_id IN (SELECT id FROM (${sales.returnLines}) line WHERE line.return_id IN (SELECT id FROM (${returns}) source))`,
 goodsValuations:`SELECT native.* FROM (${sales.goodsValuations}) native WHERE native.fulfillment_id IN (SELECT id FROM (${deliveries}) source)`,
 invoiceCarrying:`SELECT native.* FROM (${sales.invoiceCarrying}) native WHERE native.invoice_id IN (SELECT invoice_id FROM (${deliveries}) source)`
} as const;
