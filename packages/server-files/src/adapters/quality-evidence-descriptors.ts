import { hospitalityOwnerProjectionSources as qualityProjectionSources } from "@erpx/server-quality/public/hospitality-owner-projection-sources";
import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
type Row=Record<string,string|number|null>;
export function qualityEvidenceDescriptors(write:SqlDatabase,_actor:WorkContext,id:string){return write.prepare(`SELECT ia.inspection_id,ia.ordinal,a.id,a.original_name,a.media_type,a.bytes,a.checksum_sha256 FROM (${qualityProjectionSources.inventory_quality_inspection_attachments}) ia JOIN (${qualityProjectionSources.inventory_quality_inspections}) i ON i.id=ia.inspection_id JOIN attachments a ON a.id=ia.attachment_id AND a.revoked_at IS NULL WHERE i.hold_id=? ORDER BY i.created_at,ia.ordinal`).bind(id).all<Row>();}
