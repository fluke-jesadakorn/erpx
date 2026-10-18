import "server-only";
import {supplyCommerceSettlementSources as commerce} from "../../commerce/public/supply-package-sources";
/** The full child image, including immutable evidence, must survive deferred
 * effects. Guarding only the parent's version misses inserted source children. */
export function procurementCommandSourceImage(kind:"rfq"|"purchase",alias:string){
 if(alias!=="r")throw Error("PROCUREMENT_SOURCE_ALIAS_REQUIRED");
 if(kind==="rfq")return `jsonb_build_object('record',to_jsonb(r),
 'lines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_rfq_lines line WHERE line.rfq_id=r.id),
 'offers',(SELECT jsonb_agg(to_jsonb(offer) ORDER BY offer.id) FROM procurement_rfq_offers offer WHERE offer.rfq_id=r.id),
 'offerLines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_rfq_offer_lines line JOIN procurement_rfq_offers offer ON offer.id=line.offer_id WHERE offer.rfq_id=r.id),
 'events',(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM procurement_rfq_events event WHERE event.rfq_id=r.id),
 'handoffs',(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.request_id,binding.order_id) FROM procurement_request_orders binding WHERE binding.rfq_id=r.id))`;
 return `jsonb_build_object('record',to_jsonb(r),
 'lines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM (${commerce.lines}) line WHERE line.order_id=r.id),
 'receipts',(SELECT jsonb_agg(to_jsonb(receipt) ORDER BY receipt.id) FROM (${commerce.receipts}) receipt WHERE receipt.order_id=r.id),
 'promises',(SELECT jsonb_agg(to_jsonb(promise) ORDER BY promise.id) FROM procurement_delivery_promises promise WHERE promise.order_id=r.id),
 'promiseLines',(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_delivery_promise_lines line JOIN procurement_delivery_promises promise ON promise.id=line.promise_id WHERE promise.order_id=r.id),
 'handoffs',(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.request_id,binding.order_id) FROM procurement_request_orders binding WHERE binding.order_id=r.id))`;
}
