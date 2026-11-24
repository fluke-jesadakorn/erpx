import "server-only";
import {followupCustomerOrderRead} from './followup-customer-order-read';
/** Tail-parameterized view of followupCustomerOrderRead kept as a stable r31
 * export: the seven call arguments map positionally onto the primary generator
 * (whereTail -> predicate, order -> order, joinTail -> extraJoin), so both
 * workspace and calendar heads keep their byte-identical SQL and bind order.
 * The statement literal lives only in followupCustomerOrderRead. */
export const followupCustomerOrderTailRead = (
  orders: string,
  columns: string,
  scope: string,
  purchaseWhere: string,
  whereTail: string,
  order: string,
  joinTail = '',
) => followupCustomerOrderRead(orders, columns, scope, purchaseWhere, whereTail, order, joinTail);
