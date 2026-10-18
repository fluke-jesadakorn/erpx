import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {commerceQualityAttachmentReadSourceLocks,commerceQualityAttachmentReadSourceRows} from '../../commerce/public/quality-attachment-read-sources';
import {qualityPurchaseSourceRows} from './purchase-sources';

export function isQualityAttachmentReadTarget(target:string){return target==='quality_hold'||target==='inventory_quality_hold'||target==='inventory_quality_spec';}
export function qualityAttachmentReadSourceLocks(database:SqlDatabase){return [
 ...commerceQualityAttachmentReadSourceLocks(database),
 database.prepare('LOCK TABLE inventory_quality_holds,inventory_quality_specs,inventory_quality_spec_metrics,inventory_quality_inspections,inventory_quality_measurements,inventory_quality_inspection_attachments,inventory_quality_cancellations,inventory_quality_disposition_events,inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE ROW EXCLUSIVE MODE'),
];}
const specs=`SELECT s.id,s.company_id,jsonb_build_object('specification',to_jsonb(s),'metrics',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_quality_spec_metrics m WHERE m.spec_id=s.id)) image FROM inventory_quality_specs s`;
/** Full current evidence and captured specification supplement the deliberately
 * stable purchase lineage. Optional legacy origin remains optional, without
 * upgrading a manual hold into an included Sales/Supply source. */
export function qualityAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:string,id:string){
 if(!isQualityAttachmentReadTarget(target))throw new Error('QUALITY_FILE_SOURCE_TARGET_REQUIRED');
 const query=target==='inventory_quality_spec'?`SELECT s.image::text image FROM (${specs}) s WHERE s.id=? AND s.company_id=?`:
 `SELECT jsonb_build_object('hold',to_jsonb(h),'root',to_jsonb(root),'purchase',source.witness,'commerce',origin.image,'specification',spec.image,
 'inspections',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM inventory_quality_inspections i WHERE i.hold_id=h.id),
 'measurements',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM inventory_quality_measurements m JOIN inventory_quality_inspections i ON i.id=m.inspection_id WHERE i.hold_id=h.id),
 'inspectionAttachments',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.inspection_id,a.attachment_id) FROM inventory_quality_inspection_attachments a JOIN inventory_quality_inspections i ON i.id=a.inspection_id WHERE i.hold_id=h.id),
 'cancellations',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM inventory_quality_cancellations c WHERE c.hold_id=h.id),
 'dispositions',(SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM inventory_quality_disposition_events d WHERE d.hold_id=h.id))::text image
 FROM inventory_quality_holds h LEFT JOIN (${qualityPurchaseSourceRows}) source ON source.id=h.id AND source.company_id=h.company_id
 LEFT JOIN inventory_quality_holds root ON root.id=source.root_id AND root.company_id=h.company_id
 LEFT JOIN (${commerceQualityAttachmentReadSourceRows}) origin ON origin.hold_id=root.id AND origin.company_id=h.company_id
 LEFT JOIN (${specs}) spec ON spec.id=h.spec_id AND spec.company_id=h.company_id WHERE h.id=? AND h.company_id=?`;
 return database.prepare(query).bind(id,companyId).first<string>('image');
}
