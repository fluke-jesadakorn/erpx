import {procurementCommandSourceImage} from "@erpx/server-procurement/adapters/command-source";
import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "@erpx/server-commercial/public/package-settlement-sources";
import {supplyCommerceSettlementSources as commerce,supplyCommerceSettlementSourceReadLocks} from "@erpx/server-commerce/public/supply-package-sources";
import {recordAttachmentVersionSteps,recordAttachmentVersionSourceReadLocks} from "@erpx/server-files/public/record-version-receipts";
type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockSupplyProcurementPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await database.batch(supplyCommerceSettlementSourceReadLocks(database));
 await database.batch(recordAttachmentVersionSourceReadLocks(database));
 await database.prepare(`LOCK TABLE procurement_rfqs,procurement_rfq_lines,procurement_rfq_offers,procurement_rfq_offer_lines,procurement_rfq_events,
 procurement_request_orders,procurement_request_item_commitments,procurement_delivery_promises,procurement_delivery_promise_lines,
 procurement_bill_matches,procurement_bill_match_lines,procurement_bill_match_events,procurement_bill_match_receipts,
 procurement_invoice_policies,procurement_invoice_policy_events,command_receipts IN SHARE MODE`).run();
 sources.set(database,companyId);
}
export function assertSupplyProcurementPackageObligationSources(owner:PostgresDatabase,database:SqlDatabase,companyId:string){owner.assertActiveTransaction(database);if(sources.get(database)!==companyId)throw Error("SUPPLY_PROCUREMENT_PACKAGE_OBLIGATION_SOURCE_REQUIRED");}
export const supplyProcurementSettlementSources={
 rfqs:"SELECT * FROM procurement_rfqs",bills:"SELECT * FROM procurement_bill_matches",billReceipts:"SELECT * FROM procurement_bill_match_receipts",
 billLines:"SELECT * FROM procurement_bill_match_lines",billEvents:"SELECT * FROM procurement_bill_match_events",promises:"SELECT * FROM procurement_delivery_promises",
} as const;
/** Standing supplier/product identity and reviewed matching configuration are
 * retained history. An actual unresolved demand, receipt or promise is duty;
 * an unsupported terminal label cannot erase its original owner evidence. */
export async function supplyProcurementPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 assertSupplyProcurementPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="procurement"||input.packageId!=="supply"||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("SUPPLY_PROCUREMENT_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id),orders AS MATERIALIZED(SELECT o.* FROM (${commerce.orders}) o,scope WHERE o.company_id=scope.company_id)
 SELECT 'rfq' kind,r.id,(${procurementCommandSourceImage("rfq","r")})::text fact,r.status NOT IN ('ordered','cancelled') OR NOT EXISTS
 (SELECT 1 FROM procurement_rfq_events event JOIN command_receipts command ON event.id='rfq-event-'||command.command_id
  WHERE event.rfq_id=r.id AND event.version<=r.version AND command.authority_json::jsonb->>'companyId'=r.company_id
  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT event.version UNION SELECT binding.next_version FROM versions
   JOIN (${recordAttachmentVersionSteps}) binding ON binding.previous_version=versions.version AND binding.company_id=r.company_id
   AND binding.target_type='procurement_rfq' AND binding.target_id=r.id WHERE binding.next_version<=r.version)
   SELECT 1 FROM versions WHERE version=r.version)
  AND command.actor_id=event.actor_id AND ((r.status='cancelled' AND command.record_id=r.id AND event.action='cancel' AND command.operation='procurement.rfq.cancel') OR
   (r.status='ordered' AND command.record_id=r.order_id AND event.action='order-created' AND command.operation='commerce.order.create' AND command.result_json::jsonb->>'id'=r.order_id)))
 OR (r.status='ordered' AND NOT EXISTS(SELECT 1 FROM orders original JOIN procurement_rfq_offers offer ON offer.id=r.selected_offer_id AND offer.rfq_id=r.id
  WHERE original.id=r.order_id AND original.warehouse_id=r.warehouse_id AND original.party_id=offer.supplier_id AND original.currency=r.currency
  AND r.approved_by IS NOT NULL AND r.approved_by<>r.created_by AND r.approved_by IS DISTINCT FROM r.submitted_by AND r.approved_by<>offer.recorded_by)) pending
 FROM procurement_rfqs r,scope WHERE r.company_id=scope.company_id
 UNION ALL SELECT 'order',o.id,jsonb_build_array(to_jsonb(o),(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM (${commerce.lines}) line WHERE line.order_id=o.id))::text,o.state NOT IN ('fulfilled','cancelled')
 OR (o.state='fulfilled' AND (NOT EXISTS(SELECT 1 FROM (${commerce.lines}) line WHERE line.order_id=o.id) OR EXISTS(SELECT 1 FROM (${commerce.lines}) line WHERE line.order_id=o.id AND line.fulfilled_milli<>line.quantity_milli)))
 OR (o.state='cancelled' AND NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=o.id AND command.operation='commerce.order.cancel' AND command.authority_json::jsonb->>'companyId'=o.company_id AND command.result_json::jsonb->>'state'='cancelled')) FROM orders o
 UNION ALL SELECT 'receipt',receipt.id,jsonb_build_array(to_jsonb(receipt),(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM (${commerce.receiptLines}) line WHERE line.fulfillment_id=receipt.id))::text,receipt.invoice_id IS NULL OR NOT EXISTS(SELECT 1 FROM (${commerce.receiptLines}) line WHERE line.fulfillment_id=receipt.id)
 FROM (${commerce.receipts}) receipt JOIN orders original ON original.id=receipt.order_id
 UNION ALL SELECT 'supplier-return',returned.id,jsonb_build_array(to_jsonb(returned),(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM (${commerce.returnLines}) line WHERE line.return_id=returned.id))::text,
 original.id IS NULL OR receipt.id IS NULL OR receipt.order_id IS DISTINCT FROM returned.order_id OR receipt.invoice_id IS DISTINCT FROM returned.invoice_id
 OR NOT EXISTS(SELECT 1 FROM (${commerce.returnLines}) line WHERE line.return_id=returned.id)
 OR EXISTS(SELECT 1 FROM (${commerce.returnLines}) line LEFT JOIN (${commerce.receiptLines}) actual ON actual.id=line.fulfillment_line_id WHERE line.return_id=returned.id AND (actual.id IS NULL OR actual.fulfillment_id IS DISTINCT FROM returned.fulfillment_id))
 OR NOT EXISTS(SELECT 1 FROM command_receipts requested WHERE requested.record_id=returned.id AND requested.operation='commerce.return.request' AND requested.actor_id=returned.requested_by AND requested.authority_json::jsonb->>'companyId'=returned.company_id AND requested.result_json::jsonb->>'orderId'=returned.order_id)
 OR returned.status NOT IN ('rejected','posted') OR NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=returned.id
 AND command.authority_json::jsonb->>'companyId'=returned.company_id AND command.result_json::jsonb->>'orderId'=returned.order_id AND command.result_json::jsonb->>'status'=returned.status
 AND ((returned.status='rejected' AND command.operation='commerce.return.reject' AND command.actor_id=returned.approved_by AND returned.approved_by<>returned.requested_by)
 OR(returned.status='posted' AND command.operation='commerce.return.complete' AND returned.approved_by IS NOT NULL AND returned.approved_by<>returned.requested_by))
 AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT CASE WHEN command.result_json::jsonb->>'version' ~ '^[1-9][0-9]*$' THEN (command.result_json::jsonb->>'version')::bigint END UNION
 SELECT binding.next_version FROM versions JOIN (${recordAttachmentVersionSteps}) binding ON binding.previous_version=versions.version AND binding.company_id=returned.company_id
 AND binding.target_type='commerce_return' AND binding.target_id=returned.id WHERE binding.next_version<=returned.version) SELECT 1 FROM versions WHERE version=returned.version))
 FROM (${commerce.returns}) returned CROSS JOIN scope LEFT JOIN orders original ON original.id=returned.order_id LEFT JOIN (${commerce.receipts}) receipt ON receipt.id=returned.fulfillment_id WHERE returned.company_id=scope.company_id
 UNION ALL SELECT 'promise',promise.id,jsonb_build_array(to_jsonb(promise),(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_delivery_promise_lines line WHERE line.promise_id=promise.id))::text,original.id IS NULL OR original.party_id IS DISTINCT FROM promise.supplier_id OR original.currency IS DISTINCT FROM promise.currency
 OR NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=promise.order_id AND command.result_json::jsonb->>'promiseId'=promise.id AND command.operation='procurement.promise.record' AND command.actor_id=promise.recorded_by AND command.authority_json::jsonb->>'companyId'=promise.company_id)
 OR EXISTS(SELECT 1 FROM procurement_open_promise_lines pending WHERE pending.promise_id=promise.id AND pending.remaining_milli>0)
 FROM procurement_delivery_promises promise CROSS JOIN scope LEFT JOIN orders original ON original.id=promise.order_id WHERE promise.company_id=scope.company_id
 UNION ALL SELECT 'supplier-bill',bill.id,jsonb_build_array(to_jsonb(bill),(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_bill_match_lines line WHERE line.match_id=bill.id),(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM procurement_bill_match_events event WHERE event.match_id=bill.id),(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.fulfillment_id) FROM procurement_bill_match_receipts binding WHERE binding.match_id=bill.id))::text,original.id IS NULL OR original.party_id IS DISTINCT FROM bill.supplier_id OR original.currency IS DISTINCT FROM bill.currency OR bill.status NOT IN ('posted','returned')
 OR NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts binding WHERE binding.match_id=bill.id AND binding.fulfillment_id=bill.fulfillment_id)
 OR EXISTS(SELECT 1 FROM procurement_bill_match_receipts binding LEFT JOIN (${commerce.receipts}) receipt ON receipt.id=binding.fulfillment_id LEFT JOIN orders parent ON parent.id=receipt.order_id
  WHERE binding.match_id=bill.id AND (parent.id IS NULL OR parent.party_id IS DISTINCT FROM bill.supplier_id OR parent.currency IS DISTINCT FROM bill.currency))
 OR NOT EXISTS(SELECT 1 FROM procurement_bill_match_events event JOIN command_receipts command ON event.id='bill-event-'||command.command_id
  WHERE event.match_id=bill.id AND event.version<=bill.version AND command.authority_json::jsonb->>'companyId'=bill.company_id AND command.actor_id=event.actor_id
  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT event.version UNION SELECT binding.next_version FROM versions
   JOIN (${recordAttachmentVersionSteps}) binding ON binding.previous_version=versions.version AND binding.company_id=bill.company_id
   AND binding.target_type='supplier_bill_match' AND binding.target_id=bill.id WHERE binding.next_version<=bill.version)
   SELECT 1 FROM versions WHERE version=bill.version)
  AND ((bill.status='returned' AND command.record_id=bill.id AND event.action IN ('return','return-ap-draft') AND command.operation='procurement.bill.return') OR (bill.status='posted' AND command.record_id=bill.invoice_id AND event.action='posted' AND command.operation='finance.invoice.post')))
 OR (bill.status='posted' AND bill.invoice_id IS NULL)
 FROM procurement_bill_matches bill CROSS JOIN scope LEFT JOIN orders original ON original.id=bill.order_id WHERE bill.company_id=scope.company_id
 ORDER BY kind,id`).bind(input.companyId).all<{kind:string;id:string;fact:string;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"procurement",observedAt:input.now,revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending!==false).length};
}
