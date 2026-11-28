import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {planningAttachmentScenarioSourceRows,planningAttachmentRecommendationSourceRows} from '@erpx/planning-server/public/attachment-read-sources';
import {manufacturingPurchaseSourceReadLocks} from '@erpx/manufacturing-server/adapters/purchases';
import {manufacturingProductSources,manufacturingWarehouseSources} from '@erpx/commerce-server/public/manufacturing-purchase-sources';
import {manufacturingJsonbAggSql,manufacturingOperationAggSql,manufacturingSpecificationLineAggSql,manufacturingSpecificationProductsAggSql} from './manufacturing-read-sql-fragments';
import {manufacturingAttachmentImageSelect} from './manufacturing-read-tail-r21-sql';

/** Text-only order-lineage aggregate shared by the production-run attachment
 * image. The reservations, genealogy and cost-movement subqueries were
 * byte-identical apart from the table/alias and the optional company predicate;
 * the helper emits each one's SQL verbatim on the caller's prepared statement
 * and stays outside the RD_* read registry. */
const manufacturingOrderLineageAgg=(alias:string,table:string,extra="")=>manufacturingJsonbAggSql(`to_jsonb(${alias})`,`${alias}.id`,`${table} ${alias}`,` WHERE ${alias}.order_id=record.id${extra}`);

export function isManufacturingAttachmentReadTarget(target:string){return target==='manufacturing_spec'||target==='production_run'||target==='planning_scenario';}
const specificationRows=`SELECT specification.id,specification.company_id,
 jsonb_build_object('specification',to_jsonb(specification),'lines',
 ${manufacturingSpecificationLineAggSql("specification")},
 'operations',${manufacturingOperationAggSql("manufacturing_specification_operations operation","WHERE operation.specification_id=specification.id")},
 'products',${manufacturingSpecificationProductsAggSql("specification",manufacturingProductSources)})) image
 FROM manufacturing_specifications specification`;
export function manufacturingAttachmentReadSourceLocks(database:SqlDatabase){
 return [...manufacturingPurchaseSourceReadLocks(database,true),database.prepare('LOCK TABLE manufacturing_material_reservations,manufacturing_genealogy,manufacturing_cost_snapshots,manufacturing_operation_events IN SHARE ROW EXCLUSIVE MODE')];
}
/** Full native current headers supplement the original command lineage, which
 * intentionally omits fields a command is allowed to change itself. Optional
 * lineage remains optional for an original default-disabled legacy File read. */
export function manufacturingAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:string,id:string){
 if(!isManufacturingAttachmentReadTarget(target))throw new Error('MANUFACTURING_FILE_SOURCE_TARGET_REQUIRED');
 const query=target==='manufacturing_spec'?manufacturingAttachmentImageSelect("record.image",` FROM (${specificationRows})`):
 target==='planning_scenario'?manufacturingAttachmentImageSelect("jsonb_build_object('native',record.image,'specification',specification.image,'warehouse',to_jsonb(warehouse))",`\n FROM (${planningAttachmentScenarioSourceRows})`,` LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id`):
 manufacturingAttachmentImageSelect(`jsonb_build_object('order',to_jsonb(record),'specification',specification.image,'recommendation',recommendation.image,'warehouse',to_jsonb(warehouse),
 'reservations',${manufacturingOrderLineageAgg("reservation","manufacturing_material_reservations")},
 'genealogy',${manufacturingOrderLineageAgg("genealogy","manufacturing_genealogy")},
 'costSnapshot',(SELECT to_jsonb(snapshot) FROM manufacturing_cost_snapshots snapshot WHERE snapshot.order_id=record.id),
 'costMovements',${manufacturingOrderLineageAgg("movement","manufacturing_cost_movements"," AND movement.company_id=record.company_id")},
 'postings',${manufacturingJsonbAggSql("jsonb_build_object('posting',to_jsonb(posting),'profile',to_jsonb(profile))","posting.id","manufacturing_order_postings posting LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id"," WHERE posting.order_id=record.id AND posting.company_id=record.company_id")},
 'operations',${manufacturingOperationAggSql("manufacturing_operation_events operation","WHERE operation.order_id=record.id")})`,`\n FROM manufacturing_orders`,` LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id\n LEFT JOIN (${planningAttachmentRecommendationSourceRows}) recommendation ON recommendation.id=record.recommendation_id AND recommendation.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id`);
 return database.prepare(query).bind(id,companyId).first<string>('image');
}
