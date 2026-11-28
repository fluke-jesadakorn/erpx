import "server-only";
import {billReceiptBinding} from './read-tail-r23';
import {foreignBillReceiptWitness} from './read-tail-r25';
/** Resolve the original consolidated Purchase receipt owner without loading its command adapters. */
export function supplierBillOwnerPredicate(alias:string){return `EXISTS(SELECT 1 FROM commerce_fulfillments primary_receipt JOIN commerce_orders primary_order ON primary_order.id=primary_receipt.order_id WHERE primary_receipt.id=${alias}.fulfillment_id AND primary_receipt.order_id=${alias}.order_id AND primary_order.company_id=${alias}.company_id AND primary_order.kind='purchase' AND primary_order.party_id=${alias}.supplier_id AND primary_order.currency=${alias}.currency) AND ${billReceiptBinding(false,"br",alias)} AND ${foreignBillReceiptWitness(alias)}`;}
