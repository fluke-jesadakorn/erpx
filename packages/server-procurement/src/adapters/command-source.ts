import "server-only";
import {supplyCommerceSettlementSources as commerce} from "@erpx/server-commerce/public/supply-package-sources";
import {jsonbAggBy} from './read-tail-r21';
import {jsonbAggJoinBy} from './read-tail-r22';
/** The full child image, including immutable evidence, must survive deferred
 * effects. Guarding only the parent's version misses inserted source children. */
export const rfqLinesAgg = jsonbAggBy("procurement_rfq_lines","line","rfq_id","r.id","line.id");
export const rfqOffersAgg = jsonbAggBy("procurement_rfq_offers","offer","rfq_id","r.id","offer.id");
export const rfqOfferLinesAgg = jsonbAggJoinBy("procurement_rfq_offer_lines","line","procurement_rfq_offers","offer","offer.id=line.offer_id","rfq_id","r.id","line.id");
export function procurementCommandSourceImage(kind:"rfq"|"purchase",alias:string){
 if(alias!=="r")throw Error("PROCUREMENT_SOURCE_ALIAS_REQUIRED");
 if(kind==="rfq")return `jsonb_build_object('record',to_jsonb(r),
 'lines',${rfqLinesAgg},
 'offers',${rfqOffersAgg},
 'offerLines',${rfqOfferLinesAgg},
 'events',${jsonbAggBy("procurement_rfq_events","event","rfq_id","r.id","event.id")},
 'handoffs',${jsonbAggBy("procurement_request_orders","binding","rfq_id","r.id","binding.request_id,binding.order_id")}))`;
 return `jsonb_build_object('record',to_jsonb(r),
 'lines',${jsonbAggBy(`(${commerce.lines})`,"line","order_id","r.id","line.id")},
 'receipts',${jsonbAggBy(`(${commerce.receipts})`,"receipt","order_id","r.id","receipt.id")},
 'promises',${jsonbAggBy("procurement_delivery_promises","promise","order_id","r.id","promise.id")},
 'promiseLines',${jsonbAggJoinBy("procurement_delivery_promise_lines","line","procurement_delivery_promises","promise","promise.id=line.promise_id","order_id","r.id","line.id")},
 'handoffs',${jsonbAggBy("procurement_request_orders","binding","order_id","r.id","binding.request_id,binding.order_id")}))`;
}
