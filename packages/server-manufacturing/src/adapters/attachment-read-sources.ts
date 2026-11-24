import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {planningAttachmentScenarioSourceRows,planningAttachmentRecommendationSourceRows} from '@erpx/server-planning/public/attachment-read-sources';
import {manufacturingPurchaseSourceReadLocks} from '@erpx/server-manufacturing/adapters/purchases';
import {manufacturingProductSources,manufacturingWarehouseSources} from '@erpx/server-commerce/public/manufacturing-purchase-sources';
import {manufacturingOperationAggSql,manufacturingSpecificationLineAggSql,manufacturingSpecificationProductsAggSql} from './manufacturing-read-sql-fragments';

/** Text-only order-lineage aggregate shared by the production-run attachment
 * image. The reservations, genealogy and cost-movement subqueries were
 * byte-identical apart from the table/alias and the optional company predicate;
 * the helper emits each one's SQL verbatim on the caller's prepared statement
 * and stays outside the RD_* read registry. */
const manufacturingOrderLineageAgg=(alias:string,table:string,extra="")=>`(SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${table} ${alias} WHERE ${alias}.order_id=record.id${extra})`;

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
 const query=target==='manufacturing_spec'?`SELECT record.image::text image FROM (${specificationRows}) record WHERE record.id=? AND record.company_id=?`:
 target==='planning_scenario'?`SELECT jsonb_build_object('native',record.image,'specification',specification.image,'warehouse',to_jsonb(warehouse))::text image
 FROM (${planningAttachmentScenarioSourceRows}) record LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id WHERE record.id=? AND record.company_id=?`:
 `SELECT jsonb_build_object('order',to_jsonb(record),'specification',specification.image,'recommendation',recommendation.image,'warehouse',to_jsonb(warehouse),
 'reservations',${manufacturingOrderLineageAgg("reservation","manufacturing_material_reservations")},
 'genealogy',${manufacturingOrderLineageAgg("genealogy","manufacturing_genealogy")},
 'costSnapshot',(SELECT to_jsonb(snapshot) FROM manufacturing_cost_snapshots snapshot WHERE snapshot.order_id=record.id),
 'costMovements',${manufacturingOrderLineageAgg("movement","manufacturing_cost_movements"," AND movement.company_id=record.company_id")},
 'postings',(SELECT jsonb_agg(jsonb_build_object('posting',to_jsonb(posting),'profile',to_jsonb(profile)) ORDER BY posting.id) FROM manufacturing_order_postings posting LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id WHERE posting.order_id=record.id AND posting.company_id=record.company_id),
 'operations',${manufacturingOperationAggSql("manufacturing_operation_events operation","WHERE operation.order_id=record.id")})::text image
 FROM manufacturing_orders record LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id
 LEFT JOIN (${planningAttachmentRecommendationSourceRows}) recommendation ON recommendation.id=record.recommendation_id AND recommendation.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id WHERE record.id=? AND record.company_id=?`;
 return database.prepare(query).bind(id,companyId).first<string>('image');
}
