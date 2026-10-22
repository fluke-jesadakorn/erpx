import { hospitalityOwnerProjectionSources as qualityProjectionSources } from "@erpx/server-quality/public/hospitality-owner-projection-sources";
import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
type Row=Record<string,string|number|null>;
export function qualityInspectionFiles(write:SqlDatabase,actor:WorkContext,id:string,attachmentIds:string[]){return write.prepare(`SELECT id FROM attachments WHERE owner_type='quality_hold' AND owner_id=? AND purpose='quality_inspection_evidence' AND created_by=? AND revoked_at IS NULL AND id IN (${attachmentIds.map(()=>'?').join(',')}) AND NOT EXISTS(SELECT 1 FROM (${qualityProjectionSources.inventory_quality_inspection_attachments}) ia WHERE ia.attachment_id=attachments.id)`).bind(id,actor.userId,...attachmentIds).all<Row>();}
