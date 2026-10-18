import "server-only";
import {createHash} from "node:crypto";
import type {PostgresDatabase} from "../../runtime/public/runtime/postgres";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
import {packageSettlementSource,type PackageSettlementSourceLock} from "../../commercial/public/package-settlement-sources";
import {assertSupplyInventoryPackageObligationSources,supplyInventorySettlementSources as inventory} from "../../inventory/public/supply-package-obligations";
import {qualityPurchaseSourceRows,qualityPurchaseSourceReadLocks} from "./purchase-sources";

type Input={companyId:string;packageId:string;packageVersion:number;moduleId:string;now:number};
const sources=new WeakMap<SqlDatabase,string>();
export async function lockSupplyQualityPackageObligationSources(owner:PostgresDatabase,lock:PackageSettlementSourceLock){
 const {database,companyId}=packageSettlementSource(owner,lock);
 await database.batch(qualityPurchaseSourceReadLocks(database));
 await database.prepare(`LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_cancellations,inventory_quality_disposition_events,
 inventory_quality_alerts,inventory_quality_specs,inventory_quality_spec_metrics,inventory_quality_measurements,inventory_quality_inspection_attachments,command_receipts IN SHARE MODE`).run();
 sources.set(database,companyId);
}
/** Actual basic receiving and advanced Supply Quality retain their unfinished
 * duties. Proven Sales-return quarantine keeps its Sales settlement owner. */
export async function supplyQualityPackageObligations(owner:PostgresDatabase,database:SqlDatabase,input:Input){
 owner.assertActiveTransaction(database);
 if(sources.get(database)!==input.companyId)throw Error("SUPPLY_QUALITY_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
 assertSupplyInventoryPackageObligationSources(owner,database,input.companyId);
 if(input.moduleId!=="quality"||!["supply","supply-quality"].includes(input.packageId)||input.packageVersion!==1||!Number.isSafeInteger(input.now)||input.now<0)throw Error("SUPPLY_QUALITY_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
 const facts=(await database.prepare(`WITH scope AS(SELECT ?::text company_id,?::boolean full),holds AS MATERIALIZED(
 SELECT hold.*,source.kind source_kind,source.witness FROM inventory_quality_holds hold CROSS JOIN scope
 LEFT JOIN (${qualityPurchaseSourceRows}) source ON source.id=hold.id AND source.company_id=hold.company_id
 WHERE hold.company_id=scope.company_id AND source.kind IS DISTINCT FROM 'customer-return' AND (scope.full OR source.kind IS DISTINCT FROM 'receipt'))
 SELECT 'hold' kind,hold.id,jsonb_build_array(to_jsonb(hold),
 (SELECT jsonb_agg(to_jsonb(inspection) ORDER BY inspection.id) FROM inventory_quality_inspections inspection WHERE inspection.hold_id=hold.id),
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_quality_cancellations event WHERE event.hold_id=hold.id),
 (SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM inventory_quality_disposition_events event WHERE event.hold_id=hold.id)) fact,
 NOT COALESCE((hold.status='released' AND hold.last_result='pass' AND hold.released_at IS NOT NULL AND hold.released_by IS NOT NULL
 AND EXISTS(SELECT 1 FROM inventory_quality_inspections inspection WHERE inspection.hold_id=hold.id AND inspection.result='pass' AND inspection.inspected_by=hold.last_inspector)
 AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=hold.id AND receipt.operation='quality.hold.release' AND receipt.actor_id=hold.released_by
 AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=hold.version::text))
 OR(hold.status='cancelled' AND EXISTS(SELECT 1 FROM inventory_quality_cancellations event JOIN command_receipts receipt
 ON event.id='quality-cancel-'||receipt.command_id AND receipt.record_id=hold.id AND receipt.actor_id=event.actor_id AND receipt.operation='quality.hold.cancel.approve'
 AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=hold.version::text
 WHERE event.hold_id=hold.id AND event.version=hold.version AND event.action='approve' AND event.actor_id=hold.cancelled_by))
 OR(hold.status='disposed' AND EXISTS(SELECT 1 FROM (${inventory.counts}) count JOIN inventory_quality_disposition_events event
 ON event.count_id=count.id AND event.hold_id=hold.id AND event.action='post'
 JOIN command_receipts receipt ON event.id='quality-disposition-'||receipt.command_id AND receipt.record_id=count.id AND receipt.operation='inventory.count.post'
 AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=hold.company_id
 WHERE count.id=hold.disposition_count_id AND count.quality_hold_id=hold.id AND count.company_id=hold.company_id AND count.warehouse_id=hold.warehouse_id
 AND count.status='posted' AND count.approved_by=hold.disposed_by)),false) pending FROM holds hold
 UNION ALL SELECT 'alert',alert.id,to_jsonb(alert),hold.status NOT IN ('released','cancelled','disposed') AND
 (alert.acknowledged_by IS NULL OR alert.acknowledged_at IS NULL OR NOT EXISTS(SELECT 1 FROM command_receipts receipt
 WHERE receipt.record_id=alert.id AND receipt.operation='quality.alert.acknowledge' AND receipt.actor_id=alert.acknowledged_by
 AND receipt.authority_json::jsonb->>'companyId'=hold.company_id AND receipt.result_json::jsonb->>'version'=alert.version::text))
 FROM inventory_quality_alerts alert JOIN holds hold ON hold.id=alert.hold_id
 UNION ALL SELECT 'specification',spec.id,jsonb_build_array(to_jsonb(spec),(SELECT jsonb_agg(to_jsonb(metric) ORDER BY metric.id) FROM inventory_quality_spec_metrics metric WHERE metric.spec_id=spec.id)),
 NOT COALESCE((spec.status IN ('active','superseded') AND spec.approved_by IS NOT NULL AND spec.activated_at IS NOT NULL AND EXISTS(
 SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=spec.id AND receipt.operation='quality.spec.activate' AND receipt.actor_id=spec.approved_by
 AND receipt.authority_json::jsonb->>'companyId'=spec.company_id AND receipt.result_json::jsonb->>'status'='active'))
 OR(spec.status='cancelled' AND EXISTS(SELECT 1 FROM command_receipts receipt WHERE receipt.record_id=spec.id AND receipt.operation='quality.spec.cancel'
 AND receipt.authority_json::jsonb->>'companyId'=spec.company_id AND receipt.result_json::jsonb->>'status'='cancelled')),false)
 FROM inventory_quality_specs spec,scope WHERE spec.company_id=scope.company_id ORDER BY kind,id`).bind(input.companyId,input.packageId==="supply").all<{kind:string;id:string;fact:unknown;pending:boolean}>()).results;
 return {companyId:input.companyId,packageId:input.packageId,moduleId:"quality",observedAt:input.now,
 revision:createHash("sha256").update(JSON.stringify({companyId:input.companyId,packageVersion:input.packageVersion,facts})).digest("hex"),outstanding:facts.filter(fact=>fact.pending!==false).length};
}
