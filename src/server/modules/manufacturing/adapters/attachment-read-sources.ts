import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {planningAttachmentScenarioSourceRows,planningAttachmentRecommendationSourceRows} from '../../planning/public/attachment-read-sources';
import {manufacturingPurchaseSourceReadLocks} from './purchases';
import {manufacturingProductSources,manufacturingWarehouseSources} from '../../commerce/public/manufacturing-purchase-sources';

export function isManufacturingAttachmentReadTarget(target:string){return target==='manufacturing_spec'||target==='production_run'||target==='planning_scenario';}
const specificationRows=`SELECT specification.id,specification.company_id,
 jsonb_build_object('specification',to_jsonb(specification),'lines',
 (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM manufacturing_specification_lines line WHERE line.specification_id=specification.id),
 'operations',(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_specification_operations operation WHERE operation.specification_id=specification.id),
 'products',(SELECT jsonb_agg(to_jsonb(product) ORDER BY product.id) FROM (${manufacturingProductSources}) product WHERE product.company_id=specification.company_id AND (product.id=specification.output_product_id OR product.id IN(SELECT product_id FROM manufacturing_specification_lines WHERE specification_id=specification.id)))) image
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
 'reservations',(SELECT jsonb_agg(to_jsonb(reservation) ORDER BY reservation.id) FROM manufacturing_material_reservations reservation WHERE reservation.order_id=record.id),
 'genealogy',(SELECT jsonb_agg(to_jsonb(genealogy) ORDER BY genealogy.id) FROM manufacturing_genealogy genealogy WHERE genealogy.order_id=record.id),
 'costSnapshot',(SELECT to_jsonb(snapshot) FROM manufacturing_cost_snapshots snapshot WHERE snapshot.order_id=record.id),
 'costMovements',(SELECT jsonb_agg(to_jsonb(movement) ORDER BY movement.id) FROM manufacturing_cost_movements movement WHERE movement.order_id=record.id AND movement.company_id=record.company_id),
 'postings',(SELECT jsonb_agg(jsonb_build_object('posting',to_jsonb(posting),'profile',to_jsonb(profile)) ORDER BY posting.id) FROM manufacturing_order_postings posting LEFT JOIN manufacturing_accounting_profiles profile ON profile.id=posting.profile_id AND profile.company_id=posting.company_id WHERE posting.order_id=record.id AND posting.company_id=record.company_id),
 'operations',(SELECT jsonb_agg(to_jsonb(operation) ORDER BY operation.id) FROM manufacturing_operation_events operation WHERE operation.order_id=record.id))::text image
 FROM manufacturing_orders record LEFT JOIN (${specificationRows}) specification ON specification.id=record.specification_id AND specification.company_id=record.company_id
 LEFT JOIN (${planningAttachmentRecommendationSourceRows}) recommendation ON recommendation.id=record.recommendation_id AND recommendation.company_id=record.company_id LEFT JOIN (${manufacturingWarehouseSources}) warehouse ON warehouse.id=record.warehouse_id AND warehouse.company_id=record.company_id WHERE record.id=? AND record.company_id=?`;
 return database.prepare(query).bind(id,companyId).first<string>('image');
}
