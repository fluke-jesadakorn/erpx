import 'server-only';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {commerceQualityAttachmentReadSourceLocks,commerceQualityAttachmentReadSourceRows} from '@erpx/commerce-server/public/quality-attachment-read-sources';
import {qualityPurchaseSourceRows} from '@erpx/quality-server/adapters/purchase-sources';
import {qualityChildAggSql,qualityJoinedChildAggSql} from './quality-child-agg-sql';
import {qualitySourceArmHead} from './quality-source-arm-head';
import {evidenceHead} from './inspection-evidence-source-sql';

export function isQualityAttachmentReadTarget(target:string){return target==='quality_hold'||target==='inventory_quality_hold'||target==='inventory_quality_spec';}
export function qualityAttachmentReadSourceLocks(database:SqlDatabase){return [
 ...commerceQualityAttachmentReadSourceLocks(database),
 database.prepare('LOCK TABLE inventory_quality_holds,inventory_quality_specs,inventory_quality_spec_metrics,inventory_quality_inspections,inventory_quality_measurements,inventory_quality_inspection_attachments,inventory_quality_cancellations,inventory_quality_disposition_events,inventory_counts,inventory_count_lines,inventory_count_lot_lines IN SHARE ROW EXCLUSIVE MODE'),
];}
const specs=qualitySourceArmHead(`s.id,s.company_id,jsonb_build_object('specification',to_jsonb(s),'metrics',${qualityChildAggSql('inventory_quality_spec_metrics','m','spec_id','s.id')}) image`,"inventory_quality_specs s");
/** Full current evidence and captured specification supplement the deliberately
 * stable purchase lineage. Optional legacy origin remains optional, without
 * upgrading a manual hold into an included Sales/Supply source. */
export function qualityAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:string,id:string){
 if(!isQualityAttachmentReadTarget(target))throw new Error('QUALITY_FILE_SOURCE_TARGET_REQUIRED');
 const query=target==='inventory_quality_spec'?evidenceHead(`s.image::text image`,` FROM (${specs}) s`,' WHERE s.id=? AND s.company_id=?'):
 evidenceHead(`jsonb_build_object('hold',to_jsonb(h),'root',to_jsonb(root),'purchase',source.witness,'commerce',origin.image,'specification',spec.image,
 'inspections',${qualityChildAggSql('inventory_quality_inspections','i','hold_id','h.id')},
 'measurements',${qualityJoinedChildAggSql('inventory_quality_measurements','m','m.id','inspection_id','h.id')},
 'inspectionAttachments',${qualityJoinedChildAggSql('inventory_quality_inspection_attachments','a','a.inspection_id,a.attachment_id','inspection_id','h.id')},
 'cancellations',${qualityChildAggSql('inventory_quality_cancellations','c','hold_id','h.id')},
 'dispositions',${qualityChildAggSql('inventory_quality_disposition_events','d','hold_id','h.id')})::text image
`,` FROM inventory_quality_holds h LEFT JOIN (${qualityPurchaseSourceRows}) source ON source.id=h.id AND source.company_id=h.company_id
 LEFT JOIN inventory_quality_holds root ON root.id=source.root_id AND root.company_id=h.company_id
 LEFT JOIN (${commerceQualityAttachmentReadSourceRows}) origin ON origin.hold_id=root.id AND origin.company_id=h.company_id
 LEFT JOIN (${specs}) spec ON spec.id=h.spec_id AND spec.company_id=h.company_id WHERE h.id=? AND h.company_id=?`,'');
 return database.prepare(query).bind(id,companyId).first<string>('image');
}
