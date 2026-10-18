import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
/** This owner-local segment follows Commerce's native source leases and
 * precedes all shared references and the first Company advisory SELECT. */
export function procurementCommerceSourceReadLocks(database:SqlDatabase){return [database.prepare(`LOCK TABLE procurement_rfqs,procurement_rfq_lines,procurement_rfq_offers,procurement_rfq_offer_lines,procurement_rfq_events,
 procurement_request_orders,procurement_request_item_commitments,procurement_delivery_promises,procurement_delivery_promise_lines,
 procurement_bill_matches,procurement_bill_match_lines,procurement_bill_match_events,procurement_bill_match_receipts IN SHARE ROW EXCLUSIVE MODE`)];}
