import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {finalRootOwnerProjectionSources} from '../../assets/public/final-root-owner-projection-sources';
/** The original Files policy may read a retained legacy Work row without a
 * current physical Asset. Observe optional lineage without inventing a join
 * requirement or acquiring the full People/Finance management capability. */
export function maintenanceAttachmentReadSourceImage(database:SqlDatabase,companyId:string,id:string){
 return database.prepare(`SELECT jsonb_build_object('work',to_jsonb(work),'asset',to_jsonb(asset))::text image
 FROM maintenance_work_orders work LEFT JOIN (${finalRootOwnerProjectionSources.operational_assets}) asset ON asset.id=work.asset_id AND asset.company_id=work.company_id
 WHERE work.id=? AND work.company_id=?`).bind(id,companyId).first<string>('image');
}
